//! Spreadsheets as tables (#1564): File > Open and File > Import read an
//! Excel workbook's first sheet (`.xlsx`) or a CSV/TSV file into live
//! tables whose cells stay editable text, to lay out and print. Opened, the
//! rows run over as many A4 pages as they need (landscape when the table
//! is wider than a portrait page), the first row repeated at the top of
//! each page; imported, the sheet comes as one table on the active layer.
//! See docs/behavior/spreadsheet-import.md.

use crate::app::App;
use tracedraw_core::{
    document::{paper, Shape, ShapeKind, Table, TextAlign},
    geometry::{Point, Rect, Size},
    Color, Command, Document, Fill, Stroke, TextSpan,
};

/// The extensions read as spreadsheets.
pub const EXTENSIONS: [&str; 3] = ["xlsx", "csv", "tsv"];

/// The page margin of an opened sheet, mm.
const MARGIN: f64 = 10.0;
/// The text size the cells start at, and the smallest a wide sheet
/// shrinks to so it fits the page across, points.
const SIZE_PT: f64 = 10.0;
const MIN_SIZE_PT: f64 = 6.0;
/// The narrowest a column starts, mm.
const MIN_COL: f64 = 8.0;

pub fn is_spreadsheet(ext: &str) -> bool {
    EXTENSIONS.contains(&ext)
}

/// The rows of the file at `path`: the first sheet of a workbook, or the
/// lines of a CSV (comma, semicolon or tab) or TSV file, each a row of
/// cell texts, trailing empty rows left out; with the workbook's bytes
/// when the file is one (kept with the table, #4143).
pub fn read_sheet(
    path: &std::path::Path,
) -> Result<(Vec<Vec<String>>, Option<tracedraw_core::SharedBytes>), String> {
    let bytes = crate::files::read(path).map_err(|e| e.to_string())?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let rows = rows_from_bytes(&ext, &bytes)?;
    Ok((rows, (ext == "xlsx").then(|| bytes.into())))
}

pub fn rows_from_bytes(ext: &str, bytes: &[u8]) -> Result<Vec<Vec<String>>, String> {
    let mut rows = if ext == "xlsx" {
        tracedraw_io::xlsx::read_rows(bytes)?
    } else {
        let (head, mut rows) = crate::export::parse_csv(&crate::export::decode_text(bytes));
        if !head.is_empty() {
            rows.insert(0, head);
        }
        rows
    };
    while rows
        .last()
        .is_some_and(|r| r.iter().all(|c| c.trim().is_empty()))
    {
        rows.pop();
    }
    if rows.is_empty() {
        return Err("the sheet is empty".into());
    }
    Ok(rows)
}

/// Whether a cell reads as a number (aligned right, as spreadsheets do):
/// digits with an optional sign, separators, decimal point, currency or
/// percent sign.
fn is_number(s: &str) -> bool {
    let t = s.trim();
    t.chars().any(|c| c.is_ascii_digit())
        && t.chars()
            .all(|c| c.is_ascii_digit() || " +-.,%$€£¥()".contains(c) || c == '\u{a0}')
}

/// A table of `rows` with its top left corner at `top_left`: one row of
/// cells per sheet row, the first row bold, numbers aligned right,
/// columns as wide as their widest text, hairline borders.
pub fn table_of(rows: &[Vec<String>], top_left: Point, size_pt: f64) -> Table {
    let nrows = rows.len().max(1) as u32;
    let ncols = rows.iter().map(|r| r.len()).max().unwrap_or(1).max(1) as u32;
    let line = (size_pt * 25.4 / 72.0 * 1.5).max(3.0);
    let rect = Rect::new(
        top_left.x,
        top_left.y - nrows as f64 * line,
        top_left.x + ncols as f64 * MIN_COL,
        top_left.y,
    );
    let mut t = Table::new(rect, nrows, ncols, Some(Stroke::hairline(Color::BLACK)));
    t.padding = 0.8;
    for (r, row) in rows.iter().enumerate() {
        for (c, txt) in row.iter().enumerate() {
            if txt.is_empty() {
                continue;
            }
            if let Some(i) = t.cell_index(r as u32, c as u32) {
                let mut span = TextSpan::new(txt.as_str(), "Arial", size_pt);
                span.bold = r == 0;
                t.cells[i].text = vec![span];
                if r > 0 && is_number(txt) {
                    t.cells[i].align = TextAlign::Right;
                }
            }
        }
    }
    crate::table::fit_columns_to_text(&mut t);
    t
}

fn table_shape(id: tracedraw_core::ShapeId, t: Table, name: &str) -> Shape {
    let mut s = Shape::new(id, ShapeKind::Table(t));
    s.fill = Fill::None;
    s.stroke = None;
    s.name = Some(name.to_string());
    s
}

/// The page an opened sheet takes, the text size its cells get and how
/// many rows (after the first, repeated) fit on each page.
fn layout(rows: &[Vec<String>]) -> (Size, f64, usize) {
    let width = |pt: f64| table_of(rows, Point::ZERO, pt).rect.width();
    let portrait = paper::A4;
    let landscape = Size::new(portrait.height, portrait.width);
    let mut size_pt = SIZE_PT;
    let w = width(size_pt);
    let page = if w <= portrait.width - 2.0 * MARGIN {
        portrait
    } else {
        let room = landscape.width - 2.0 * MARGIN;
        if w > room {
            size_pt = (SIZE_PT * room / w).max(MIN_SIZE_PT).floor();
        }
        landscape
    };
    let line = table_of(&rows[..1], Point::ZERO, size_pt).rect.height();
    let per_page = (((page.height - 2.0 * MARGIN) / line).floor() as usize)
        .saturating_sub(1)
        .max(1);
    (page, size_pt, per_page)
}

#[cfg(test)]
pub fn document(title: &str, rows: &[Vec<String>]) -> Document {
    document_from(title, rows, None)
}

/// A drawing of the sheet `rows` (File > Open): A4 pages, the first row
/// at the top of each; the workbook `source` kept with the table when the
/// sheet fits one page (a sheet split over pages keeps none: each page's
/// table holds only part of it, #4143).
pub fn document_from(
    title: &str,
    rows: &[Vec<String>],
    source: Option<tracedraw_core::SharedBytes>,
) -> Document {
    let (page, size_pt, per_page) = layout(rows);
    let mut doc = App::localized_document(title, page);
    let mut ids = doc.ids().clone();
    let (head, body) = rows.split_first().expect("rows");
    let chunks: Vec<&[Vec<String>]> = if body.is_empty() {
        vec![&[]]
    } else {
        body.chunks(per_page).collect()
    };
    for (i, chunk) in chunks.iter().enumerate() {
        if i > 0 {
            let p = tracedraw_core::document::Page {
                id: ids.page(),
                name: crate::i18n::trf("doc.page_n", &[("n", &(i + 1).to_string())]),
                size: page,
                layers: vec![tracedraw_core::document::Layer::new(
                    ids.layer(),
                    doc.pages[0].layers[0].name.clone(),
                )],
                guides: Vec::new(),
                background: None,
            };
            doc.pages.push(p);
        }
        let mut part = vec![head.clone()];
        part.extend(chunk.iter().cloned());
        let mut t = table_of(&part, Point::new(MARGIN, page.height - MARGIN), size_pt);
        if chunks.len() == 1 {
            t.source_xlsx = source.clone();
        }
        let shape = table_shape(ids.shape(), t, title);
        doc.pages[i].layers[0].shapes.push(shape);
    }
    doc.set_ids(ids);
    doc
}

impl App {
    /// File > Import of a spreadsheet: the sheet as one table at the
    /// page's top left margin, selected, in one undo step.
    pub fn import_spreadsheet(&mut self, path: &std::path::Path) {
        let Some(layer) = self.active_layer() else {
            return;
        };
        match read_sheet(path) {
            Ok((rows, source)) => {
                let page = self.page_rect();
                let mut t = table_of(
                    &rows,
                    Point::new(page.x0 + MARGIN, page.y1 - MARGIN),
                    SIZE_PT,
                );
                t.source_xlsx = source;
                let name = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                let id = self.engine.new_shape_id();
                self.run(Command::AddShape {
                    layer,
                    shape: table_shape(id, t, &name),
                });
                self.select(vec![id]);
            }
            Err(e) => self.status = crate::i18n::trf("status.import_failed", &[("e", &e)]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet(n: usize, cols: usize) -> Vec<Vec<String>> {
        let mut rows = vec![(0..cols).map(|c| format!("Field {c}")).collect::<Vec<_>>()];
        for r in 0..n {
            rows.push((0..cols).map(|c| format!("{}", r * 10 + c)).collect());
        }
        rows
    }

    fn tables(doc: &Document) -> Vec<Table> {
        doc.pages
            .iter()
            .flat_map(|p| &p.layers[0].shapes)
            .filter_map(|s| match &s.kind {
                ShapeKind::Table(t) => Some(t.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn csv_and_tsv_rows_come_with_their_first_row() {
        let rows = rows_from_bytes("csv", b"Name;Qty\nPen;3\nInk;12\n\n").unwrap();
        assert_eq!(
            rows,
            vec![vec!["Name", "Qty"], vec!["Pen", "3"], vec!["Ink", "12"]]
        );
        let rows = rows_from_bytes("tsv", b"a\tb\n1\t2\n").unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows_from_bytes("csv", b"\n\n").is_err());
        assert!(rows_from_bytes("xlsx", b"not a zip").is_err());
    }

    #[test]
    fn a_table_has_a_bold_first_row_and_numbers_to_the_right() {
        let rows = vec![
            vec!["Item".to_string(), "Price".into()],
            vec!["Paper".into(), "1,250.00".into()],
        ];
        let t = table_of(&rows, Point::new(10.0, 200.0), 10.0);
        assert_eq!((t.rows(), t.cols()), (2, 2));
        let cell = |r, c| &t.cells[t.cell_index(r, c).unwrap()];
        assert!(cell(0, 0).text[0].bold && !cell(1, 0).text[0].bold);
        assert_eq!(cell(1, 1).align, TextAlign::Right);
        assert_eq!(cell(1, 0).align, TextAlign::Left);
        assert!((t.rect.x0 - 10.0).abs() < 1e-9 && (t.rect.y1 - 200.0).abs() < 1e-9);
        assert!(is_number("-12%") && is_number("€ 3,50") && !is_number("A1") && !is_number("-"));
    }

    #[test]
    fn a_long_sheet_runs_over_pages_with_its_first_row_on_each() {
        let rows = sheet(150, 3);
        let doc = document("Sales", &rows);
        assert!(doc.pages.len() >= 2, "{} pages", doc.pages.len());
        assert_eq!(doc.pages[0].size, paper::A4, "narrow: portrait");
        let ts = tables(&doc);
        assert_eq!(ts.len(), doc.pages.len());
        let body: u32 = ts.iter().map(|t| t.rows() - 1).sum();
        assert_eq!(body, 150, "every row once");
        for t in &ts {
            let head = &t.cells[t.cell_index(0, 0).unwrap()].text[0];
            assert_eq!(head.text, "Field 0");
            assert!(t.rect.y0 >= MARGIN - 1e-6, "fits the page: {:?}", t.rect);
        }
        let ids: std::collections::HashSet<_> = doc
            .pages
            .iter()
            .flat_map(|p| p.layers[0].shapes.iter().map(|s| s.id))
            .collect();
        assert_eq!(ids.len(), ts.len(), "own ids");
    }

    #[test]
    fn a_wide_sheet_turns_the_page_and_shrinks_to_fit() {
        let mut rows = sheet(5, 30);
        rows[0] = (0..30)
            .map(|c| format!("A long column title {c}"))
            .collect();
        let doc = document("Wide", &rows);
        let page = doc.pages[0].size;
        assert!(page.width > page.height, "landscape");
        let t = &tables(&doc)[0];
        let size = t.cells[t.cell_index(0, 0).unwrap()].text[0].size_pt;
        assert!((MIN_SIZE_PT..SIZE_PT).contains(&size), "{size}");
    }

    #[test]
    fn opening_and_importing_a_sheet() {
        let dir = std::env::temp_dir().join(format!("td-sheet-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("prices.csv");
        std::fs::write(&path, "Item,Price\nPaper,2\nInk,3\n").unwrap();
        let mut app = App::headless();
        app.open_path(path.clone());
        assert!(app.file.is_none(), "saving does not overwrite the sheet");
        assert_eq!(tables(app.doc()).len(), 1);
        let mut other = App::headless();
        other.import_path_as_in_file(&path);
        assert_eq!(other.selection.len(), 1);
        let t = tables(other.doc());
        assert_eq!((t[0].rows(), t[0].cols()), (3, 2));
        std::fs::remove_dir_all(&dir).ok();
    }
}
