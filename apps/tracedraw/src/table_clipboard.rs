//! Tables and spreadsheets through the clipboard (#4146): a copied table,
//! or the chosen cells of one, goes to the system clipboard as rows of
//! text separated by tabs (what spreadsheet apps paste as cells) and as
//! an HTML table carrying the cells' fonts and fills; cells copied in a
//! spreadsheet app paste into the table being edited from its active cell,
//! or onto the page as a new table. See docs/behavior/table.md,
//! "Copy and paste with spreadsheets".

use crate::app::App;
use tracedraw_core::{
    document::{ShapeKind, Table},
    geometry::Point,
    Command, Fill, TextSpan,
};

/// Rows of cell texts as tab-separated text: a cell holding a tab, a line
/// break or a quote is quoted, its quotes doubled, as spreadsheet apps
/// write it. Each row ends with a line break.
pub fn tsv_of(rows: &[Vec<String>]) -> String {
    let mut out = String::new();
    for row in rows {
        let cells: Vec<String> = row
            .iter()
            .map(|c| {
                if c.contains(['\t', '\n', '\r', '"']) {
                    format!("\"{}\"", c.replace('"', "\"\""))
                } else {
                    c.clone()
                }
            })
            .collect();
        out.push_str(&cells.join("\t"));
        out.push('\n');
    }
    out
}

/// Tab-separated text as rows of cell texts: quoted cells may hold tabs
/// and line breaks; a last empty line is left out.
pub fn parse_tsv(text: &str) -> Vec<Vec<String>> {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut chars = text.chars().peekable();
    let mut quoted = false;
    let mut at_start = true;
    while let Some(ch) = chars.next() {
        if quoted {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    cell.push('"');
                } else {
                    quoted = false;
                }
            } else {
                cell.push(ch);
            }
            continue;
        }
        match ch {
            '"' if at_start => {
                quoted = true;
                at_start = false;
            }
            '\t' => {
                row.push(std::mem::take(&mut cell));
                at_start = true;
            }
            '\n' => {
                row.push(std::mem::take(&mut cell));
                rows.push(std::mem::take(&mut row));
                at_start = true;
            }
            c => {
                cell.push(c);
                at_start = false;
            }
        }
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        rows.push(row);
    }
    rows
}

/// Whether pasted text reads as cells copied in a spreadsheet: two rows
/// or more, each with a tab (two cells or more).
pub fn looks_tabular(text: &str) -> bool {
    let rows = parse_tsv(text);
    rows.len() >= 2 && rows.iter().all(|r| r.len() >= 2)
}

/// Whether text pasted into a table cell goes over several cells: it
/// holds a tab, or several lines ending with a line break (one column of
/// a spreadsheet).
pub fn spreads_over_cells(text: &str) -> bool {
    let t = text.replace("\r\n", "\n");
    t.contains('\t') || (t.ends_with('\n') && t.trim_end_matches('\n').contains('\n'))
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\n', "<br>")
}

fn solid_hex(fill: &Fill) -> Option<String> {
    match fill {
        Fill::Solid(c) => Some(c.to_hex()),
        _ => None,
    }
}

/// The rows `r0..=r1` and columns `c0..=c1` of `t` as an HTML table: each
/// cell's text with its first span's font, size, weight, slant and
/// colour, its own fill (else the table's) as background, merged cells
/// spanning.
pub fn html_of(t: &Table, (r0, r1, c0, c1): (u32, u32, u32, u32)) -> String {
    let mut out = String::from(
        "<html><body><table style=\"border-collapse:collapse\" border=\"1\" cellspacing=\"0\">",
    );
    for r in r0..=r1 {
        out.push_str("<tr>");
        for c in c0..=c1 {
            let Some(cell) = t.cell_index(r, c).and_then(|i| t.cells.get(i)) else {
                // Covered by a merged cell: nothing of its own.
                continue;
            };
            let rows = cell.row_span.min(r1 + 1 - r);
            let cols = cell.col_span.min(c1 + 1 - c);
            let mut style = String::new();
            if let Some(span) = cell.text.first() {
                style.push_str(&format!(
                    "font-family:'{}';font-size:{}pt;",
                    span.font_family.replace('\'', ""),
                    (span.size_pt * 10.0).round() / 10.0
                ));
                if span.bold {
                    style.push_str("font-weight:bold;");
                }
                if span.italic {
                    style.push_str("font-style:italic;");
                }
                if let Some(hex) = span.fill.as_ref().and_then(solid_hex) {
                    style.push_str(&format!("color:{hex};"));
                }
            }
            if let Some(hex) = cell.fill.as_ref().or(Some(&t.cell_fill)).and_then(solid_hex) {
                style.push_str(&format!("background-color:{hex};"));
            }
            let align = match cell.align {
                tracedraw_core::TextAlign::Center => "center",
                tracedraw_core::TextAlign::Right => "right",
                _ => "left",
            };
            style.push_str(&format!("text-align:{align};"));
            out.push_str("<td");
            if rows > 1 {
                out.push_str(&format!(" rowspan=\"{rows}\""));
            }
            if cols > 1 {
                out.push_str(&format!(" colspan=\"{cols}\""));
            }
            let text: String = cell.text.iter().map(|s| s.text.as_str()).collect();
            out.push_str(&format!(" style=\"{style}\">{}</td>", html_escape(&text)));
        }
        out.push_str("</tr>");
    }
    out.push_str("</table></body></html>");
    out
}

/// The texts of rows `r0..=r1`, columns `c0..=c1` of `t`.
fn block_rows(t: &Table, (r0, r1, c0, c1): (u32, u32, u32, u32)) -> Vec<Vec<String>> {
    let all = t.text_rows();
    (r0..=r1)
        .map(|r| {
            (c0..=c1)
                .map(|c| {
                    all.get(r as usize)
                        .and_then(|row| row.get(c as usize))
                        .cloned()
                        .unwrap_or_default()
                })
                .collect()
        })
        .collect()
}

/// Put `tsv` (the plain text) and `html` on the system clipboard; whether
/// they went there.
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
fn write_html(html: &str, tsv: &str) -> bool {
    crate::clipboard::with_system_clipboard(|cb| {
        cb.set_html(html.to_string(), Some(tsv.to_string())).is_ok()
    })
    .unwrap_or(false)
        || crate::clipboard::write_text(tsv)
}

/// The browser and Android builds hand over the text alone.
#[cfg(any(target_arch = "wasm32", target_os = "android"))]
fn write_html(_html: &str, tsv: &str) -> bool {
    crate::clipboard::write_text(tsv)
}

impl App {
    /// Copy the cells `cells` of table `id` (every cell when empty) to
    /// the system clipboard as tab-separated text and an HTML table; the
    /// text written, or `None` when nothing went (#4146).
    pub fn copy_table_cells_to_system(&mut self, id: tracedraw_core::ShapeId, cells: &[(u32, u32)]) -> Option<String> {
        let t = match &self.doc().find_shape(id)?.kind {
            ShapeKind::Table(t) => t.clone(),
            _ => return None,
        };
        let all;
        let cells = if cells.is_empty() {
            all = t.all_cells();
            &all
        } else {
            cells
        };
        let block = t.span_of(cells)?;
        let tsv = tsv_of(&block_rows(&t, block));
        let html = html_of(&t, block);
        write_html(&html, &tsv).then_some(tsv)
    }

    /// The selection is one table and nothing else: its id.
    pub fn lone_selected_table(&self) -> Option<tracedraw_core::ShapeId> {
        let [id] = self.selection.as_slice() else {
            return None;
        };
        matches!(self.doc().find_shape(*id)?.kind, ShapeKind::Table(_)).then_some(*id)
    }

    /// Paste `rows` into the table being edited from its active cell on,
    /// adding the rows and columns they need, as one undo step (#4146).
    /// False when no cell is active.
    pub fn table_paste_rows(&mut self, rows: &[Vec<String>]) -> bool {
        let Some(e) = self.table_edit.clone() else {
            return false;
        };
        let Some((r0, c0)) = e.cells.first().copied() else {
            return false;
        };
        let Some(ShapeKind::Table(t)) = self.doc().find_shape(e.shape).map(|s| s.kind.clone())
        else {
            return false;
        };
        let mut t = t;
        let need_rows = r0 + rows.len() as u32;
        let need_cols = c0 + rows.iter().map(|r| r.len()).max().unwrap_or(0) as u32;
        if need_rows > t.rows() {
            let last = t.rows() - 1;
            t.insert_rows(t.rows(), need_rows - t.rows(), last);
        }
        if need_cols > t.cols() {
            let last = t.cols() - 1;
            t.insert_cols(t.cols(), need_cols - t.cols(), last);
        }
        let style = t
            .cells
            .iter()
            .find(|c| c.row == r0 && c.col == c0)
            .and_then(|c| c.text.first().cloned())
            .unwrap_or_else(|| {
                TextSpan::new("", self.text_font.clone(), (self.text_size_pt / 2.0).max(8.0))
            });
        for (dr, row) in rows.iter().enumerate() {
            for (dc, text) in row.iter().enumerate() {
                let (r, c) = (r0 + dr as u32, c0 + dc as u32);
                if let Some(cell) = t.cells.iter_mut().find(|x| x.row == r && x.col == c) {
                    let keep = cell.text.first().cloned().unwrap_or_else(|| style.clone());
                    cell.text = if text.is_empty() {
                        Vec::new()
                    } else {
                        vec![TextSpan {
                            text: text.clone(),
                            ..keep
                        }]
                    };
                }
            }
        }
        self.set_table_as(e.shape, t, "Paste Cells");
        true
    }

    /// Paste `rows` (cells copied in a spreadsheet app) onto the page as a
    /// new table at the page's centre, selected (#4146).
    pub fn paste_rows_as_table(&mut self, rows: &[Vec<String>]) {
        let Some(layer) = self.active_layer() else {
            return;
        };
        let mut t = crate::spreadsheet_import::table_of(rows, Point::ZERO, 10.0);
        let c = self.page_rect().center();
        let (w, h) = (t.rect.width(), t.rect.height());
        let dx = c.x - w / 2.0 - t.rect.x0;
        let dy = c.y + h / 2.0 - t.rect.y1;
        t.rect = tracedraw_core::geometry::Rect::new(
            t.rect.x0 + dx,
            t.rect.y0 + dy,
            t.rect.x1 + dx,
            t.rect.y1 + dy,
        );
        let id = self.engine.new_shape_id();
        let mut shape = tracedraw_core::Shape::new(id, ShapeKind::Table(t));
        shape.fill = Fill::None;
        shape.stroke = None;
        self.run(Command::AddShape { layer, shape });
        self.bring_pasted_into_edited_frame(&[id]);
        self.select(vec![id]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_separated_text_goes_both_ways() {
        let rows = vec![
            vec!["Name".to_string(), "Note".into()],
            vec!["Pen".into(), "two\nlines".into()],
            vec!["Ink \"blue\"".into(), "a\tb".into()],
        ];
        let tsv = tsv_of(&rows);
        assert_eq!(
            tsv,
            "Name\tNote\nPen\t\"two\nlines\"\n\"Ink \"\"blue\"\"\"\t\"a\tb\"\n"
        );
        assert_eq!(parse_tsv(&tsv), rows);
        assert_eq!(
            parse_tsv("a\tb\r\nc\td\r\n"),
            vec![vec!["a", "b"], vec!["c", "d"]]
        );
        assert!(looks_tabular("a\tb\r\nc\td\r\n"));
        assert!(!looks_tabular("just a line\twith a tab"));
        assert!(!looks_tabular("two\nlines"));
        assert!(spreads_over_cells("a\tb"));
        assert!(spreads_over_cells("1\n2\n3\n"));
        assert!(!spreads_over_cells("two\nlines"));
    }

    #[test]
    fn a_table_copies_as_tab_separated_text_and_html() {
        let rect = tracedraw_core::geometry::Rect::new(0.0, 0.0, 40.0, 20.0);
        let mut t = Table::new(rect, 2, 2, None);
        let mut head = TextSpan::new("Item <1>", "Arial", 10.0);
        head.bold = true;
        let i = t.cell_index(0, 0).unwrap();
        t.cells[i].text = vec![head];
        t.cells[i].fill = Some(Fill::Solid(tracedraw_core::Color::rgb8(255, 0, 0)));
        let i = t.cell_index(1, 1).unwrap();
        t.cells[i].text = vec![TextSpan::new("12", "Arial", 10.0)];
        let block = t.span_of(&t.all_cells()).unwrap();
        assert_eq!(
            tsv_of(&block_rows(&t, block)),
            "Item <1>\t\n\t12\n"
        );
        let html = html_of(&t, block);
        assert!(html.contains("<td style=\"font-family:'Arial';font-size:10pt;font-weight:bold;background-color:#ff0000;text-align:left;\">Item &lt;1&gt;</td>"), "{html}");
        assert_eq!(html.matches("<tr>").count(), 2);
        assert_eq!(html.matches("<td").count(), 4);
    }

    /// #4336: cells copied in a spreadsheet come with a picture of them
    /// too; they paste as a table, not as the picture.
    #[test]
    fn spreadsheet_cells_with_their_picture_paste_as_a_table() {
        let mut app = App::headless();
        app.paste_from(crate::clipboard::ClipboardContent {
            text: Some("Item\tQty\r\nPen\t3\r\n".into()),
            image: Some(image::RgbaImage::new(40, 20)),
            ..Default::default()
        });
        assert!(app.lone_selected_table().is_some(), "a table");
        let pictures = app
            .doc()
            .all_layers()
            .flat_map(|l| l.shapes.iter())
            .filter(|s| matches!(s.kind, ShapeKind::Bitmap { .. }))
            .count();
        assert_eq!(pictures, 0);
        // A picture with plain text still pastes as the picture.
        let mut app = App::headless();
        app.paste_from(crate::clipboard::ClipboardContent {
            text: Some("just words".into()),
            image: Some(image::RgbaImage::new(40, 20)),
            ..Default::default()
        });
        assert!(app.lone_selected_table().is_none());
    }

    #[test]
    fn spreadsheet_cells_paste_as_a_table_and_into_cells() {
        let mut app = App::headless();
        let before = app.doc().all_layers().map(|l| l.shapes.len()).sum::<usize>();
        app.paste_as_text_or_table("Item\tQty\r\nPen\t3\r\nInk\t12\r\n");
        let id = app.lone_selected_table().expect("a table");
        let t = match &app.doc().find_shape(id).unwrap().kind {
            ShapeKind::Table(t) => t.clone(),
            _ => unreachable!(),
        };
        assert_eq!((t.rows(), t.cols()), (3, 2));
        assert_eq!(t.text_rows()[2], vec!["Ink".to_string(), "12".into()]);
        let after = app.doc().all_layers().map(|l| l.shapes.len()).sum::<usize>();
        assert_eq!(after, before + 1);
        // Into the table being edited, from its active cell: it grows.
        app.table_edit = Some(crate::table::TableEdit::new(id, vec![(2, 1)], 0));
        assert!(app.table_paste_rows(&parse_tsv("5\t6\n7\t8\n")));
        let t = match &app.doc().find_shape(id).unwrap().kind {
            ShapeKind::Table(t) => t.clone(),
            _ => unreachable!(),
        };
        assert_eq!((t.rows(), t.cols()), (4, 3));
        assert_eq!(
            t.text_rows(),
            vec![
                vec!["Item".to_string(), "Qty".into(), "".into()],
                vec!["Pen".into(), "3".into(), "".into()],
                vec!["Ink".into(), "5".into(), "6".into()],
                vec!["".into(), "7".into(), "8".into()],
            ]
        );
        assert_eq!(app.engine.undo_label(), Some("Paste Cells"));
    }
}
