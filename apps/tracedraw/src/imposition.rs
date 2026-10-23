//! Print layout: how pages go onto the printed sheets (imposition) and
//! the printers' marks around them. The Print dialog chooses; `print`
//! builds the sheets with [`impose`] and writes them as one PDF.
//!
//! Layouts: as in the document (one page per sheet), n-up (several pages
//! per sheet in reading order), booklet (two pages per sheet side in
//! saddle-stitch order) and step and repeat (one page repeated over the
//! sheet). On a fixed sheet the pages are scaled down to fit their cells;
//! on a fitted sheet they keep their size and the sheet grows. Marks sit
//! in the margin around the pages: crop marks at every cut, registration
//! targets, colour bars, a densitometer scale and a line of page
//! information.

use tracedraw_core::{
    document::{Layer, Page, ShapeKind, TextSpan},
    geometry::{Affine, BezPath, Point, Rect, Size},
    Color, Document, Fill, Shape, ShapeId, Stroke,
};

/// How pages go onto sheets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Layout {
    /// One page per sheet, as in the document.
    #[default]
    AsIs,
    /// Several pages per sheet, left to right and top to bottom.
    NUp { cols: u32, rows: u32 },
    /// Two pages per sheet side in the order a folded, stapled booklet
    /// needs (the page count is rounded up to a multiple of four).
    Booklet,
    /// Each page repeated over the sheet (cards, labels).
    Repeat { cols: u32, rows: u32 },
}

impl Layout {
    /// The grid of cells on a sheet (columns, rows).
    pub fn grid(self) -> (u32, u32) {
        match self {
            Layout::AsIs => (1, 1),
            Layout::NUp { cols, rows } | Layout::Repeat { cols, rows } => {
                (cols.clamp(1, 20), rows.clamp(1, 20))
            }
            Layout::Booklet => (2, 1),
        }
    }
}

/// The printers' marks to draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Marks {
    pub crop: bool,
    pub registration: bool,
    pub color_bars: bool,
    pub densitometer: bool,
    pub page_info: bool,
    /// Each page's number under it.
    pub page_numbers: bool,
}

impl Marks {
    pub fn any(self) -> bool {
        self.crop
            || self.registration
            || self.color_bars
            || self.densitometer
            || self.page_info
            || self.page_numbers
    }
}

/// Everything the layout needs besides the document.
#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    pub layout: Layout,
    /// The sheet size; `None` fits the sheet to the pages.
    pub sheet: Option<Size>,
    /// Space between the cells, in mm.
    pub gutter: f64,
    /// Bleed kept around every page, in mm (0 for none).
    pub bleed: f64,
    /// Scale of the pages on a fitted sheet (1 = 100 %).
    pub scale: f64,
    /// The back of a double-sided sheet: the columns run right to left,
    /// so each page lands behind its front on the other side; the pages
    /// themselves are not mirrored (#387).
    pub back_side: bool,
    /// On a fixed sheet the pages keep their size, the marks taking the
    /// room there is (#388), and the grid keeps its columns and rows: what
    /// does not fit runs off the sheet instead of the pages shrinking
    /// (#496, see [`overflows`]).
    pub keep_size: bool,
    /// Where the page information sits, moved from its place (mm).
    pub info_offset: (f64, f64),
    pub marks: Marks,
    /// The drawing's name and the moment, for the page information.
    pub title: String,
    pub when: String,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            layout: Layout::AsIs,
            sheet: None,
            gutter: 0.0,
            bleed: 0.0,
            scale: 1.0,
            back_side: false,
            keep_size: false,
            info_offset: (0.0, 0.0),
            marks: Marks::default(),
            title: String::new(),
            when: String::new(),
        }
    }
}

/// Room around the pages for the marks, in mm (and the bleed beyond it).
pub const MARK_MARGIN: f64 = 12.0;

/// One page placed on a sheet: the page (`None` for a blank cell of a
/// booklet) and the map from page coordinates to the sheet.
#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    pub page: Option<usize>,
    pub transform: Affine,
    /// The page's trimmed area on the sheet.
    pub trim: Rect,
}

/// A sheet: its size and the pages on it.
#[derive(Debug, Clone, PartialEq)]
pub struct SheetPlan {
    pub size: Size,
    pub cells: Vec<Cell>,
}

/// The order of the pages of a booklet, two per sheet side (left, right):
/// for eight pages 8 1, 2 7, 6 3, 4 5. Pages past the end are blank.
pub fn booklet_order(pages: usize) -> Vec<(Option<usize>, Option<usize>)> {
    let n = pages.div_ceil(4).max(1) * 4;
    let page = |i: usize| (i < pages).then_some(i);
    (0..n / 2)
        .map(|side| {
            let (a, b) = (n - 1 - side, side);
            if side % 2 == 0 {
                (page(a), page(b))
            } else {
                (page(b), page(a))
            }
        })
        .collect()
}

/// Plan the sheets for pages of `sizes`.
pub fn plan(sizes: &[Size], o: &Options) -> Vec<SheetPlan> {
    if sizes.is_empty() {
        return Vec::new();
    }
    let (cols, rows) = o.layout.grid();
    // Which page goes in each cell, sheet by sheet.
    let sheets: Vec<Vec<Option<usize>>> = match o.layout {
        Layout::AsIs => (0..sizes.len()).map(|i| vec![Some(i)]).collect(),
        Layout::NUp { .. } => {
            let per = (cols * rows) as usize;
            (0..sizes.len())
                .collect::<Vec<_>>()
                .chunks(per)
                .map(|c| {
                    let mut v: Vec<Option<usize>> = c.iter().map(|&i| Some(i)).collect();
                    v.resize(per, None);
                    v
                })
                .collect()
        }
        Layout::Booklet => booklet_order(sizes.len())
            .into_iter()
            .map(|(a, b)| vec![a, b])
            .collect(),
        Layout::Repeat { .. } => (0..sizes.len())
            .map(|i| vec![Some(i); (cols * rows) as usize])
            .collect(),
    };
    let margin = if o.marks.any() { MARK_MARGIN } else { 0.0 } + o.bleed.max(0.0);
    // Booklet pages share the spine: no gutter or bleed between them.
    let (gutter, inner_bleed) = match o.layout {
        Layout::Booklet => (0.0, 0.0),
        _ => (o.gutter.max(0.0), o.bleed.max(0.0)),
    };
    sheets
        .iter()
        .map(|cells| {
            // The cell size: the largest page on this sheet.
            let cell = cells
                .iter()
                .flatten()
                .filter_map(|&i| sizes.get(i))
                .fold(Size::new(0.0, 0.0), |a, s| {
                    Size::new(a.width.max(s.width), a.height.max(s.height))
                });
            let cell = if cell.width > 0.0 && cell.height > 0.0 {
                cell
            } else {
                sizes[0]
            };
            // Pages with their bleed, and the gaps between them.
            let step_x = cell.width + 2.0 * inner_bleed + gutter;
            let step_y = cell.height + 2.0 * inner_bleed + gutter;
            let grid_w = cols as f64 * step_x - gutter;
            let grid_h = rows as f64 * step_y - gutter;
            let (size, scale) = match o.sheet {
                Some(sheet) => {
                    // Kept size: the pages as they are; else the grid
                    // shrinks to fit inside the marks.
                    let avail_w = (sheet.width - 2.0 * margin).max(1.0);
                    let avail_h = (sheet.height - 2.0 * margin).max(1.0);
                    let s = if o.keep_size {
                        1.0
                    } else {
                        (avail_w / grid_w).min(avail_h / grid_h).min(1.0)
                    };
                    (sheet, s)
                }
                None => {
                    let s = if o.scale.is_finite() && o.scale > 0.0 {
                        o.scale
                    } else {
                        1.0
                    };
                    (
                        Size::new(grid_w * s + 2.0 * margin, grid_h * s + 2.0 * margin),
                        s,
                    )
                }
            };
            // The grid centred on the sheet.
            let x0 = (size.width - grid_w * scale) / 2.0;
            let y_top = (size.height + grid_h * scale) / 2.0;
            let cells = cells
                .iter()
                .enumerate()
                .map(|(n, page)| {
                    let col = n as u32 % cols;
                    let col = if o.back_side { cols - 1 - col } else { col } as f64;
                    let row = (n as u32 / cols) as f64;
                    let ps = page.and_then(|i| sizes.get(i)).copied().unwrap_or(cell);
                    // The page centred in its cell (y up: row 0 at the top).
                    let cx = x0 + (col * step_x + inner_bleed + cell.width / 2.0) * scale;
                    let cy = y_top - (row * step_y + inner_bleed + cell.height / 2.0) * scale;
                    let trim = Rect::new(
                        cx - ps.width * scale / 2.0,
                        cy - ps.height * scale / 2.0,
                        cx + ps.width * scale / 2.0,
                        cy + ps.height * scale / 2.0,
                    );
                    let transform = Affine::translate((trim.x0, trim.y0)) * Affine::scale(scale);
                    Cell {
                        page: *page,
                        transform,
                        trim,
                    }
                })
                .collect();
            SheetPlan { size, cells }
        })
        .collect()
}

/// Whether a page of the plan runs off its sheet (pages kept at their
/// size in a grid larger than the sheet, #496).
pub fn overflows(plan: &[SheetPlan]) -> bool {
    const EPS: f64 = 1e-6;
    plan.iter().any(|sheet| {
        sheet.cells.iter().filter(|c| c.page.is_some()).any(|c| {
            c.trim.x0 < -EPS
                || c.trim.y0 < -EPS
                || c.trim.x1 > sheet.size.width + EPS
                || c.trim.y1 > sheet.size.height + EPS
        })
    })
}

/// A mark object on the sheet.
fn mark_shape(kind: ShapeKind, fill: Fill, stroke: Option<Stroke>) -> Shape {
    let mut s = Shape::new(ShapeId(0), kind);
    s.fill = fill;
    s.stroke = stroke;
    s
}

fn rect_shape(r: Rect, color: Color) -> Shape {
    mark_shape(
        ShapeKind::Rect {
            rect: r,
            radius: 0.0,
            corners: None,
        },
        Fill::Solid(color),
        None,
    )
}

/// Crop marks: at every cut line, short lines in the margin outside the
/// pages (so they never fall on a neighbouring page).
pub fn crop_marks(cells: &[Cell], bleed: f64) -> BezPath {
    let mut p = BezPath::new();
    let trims: Vec<Rect> = cells
        .iter()
        .filter(|c| c.page.is_some())
        .map(|c| c.trim)
        .collect();
    let Some(all) = trims.iter().copied().reduce(|a, b| a.union(b)) else {
        return p;
    };
    let gap = bleed.max(0.0) + 1.5;
    let len = 5.0;
    let mut xs: Vec<f64> = trims.iter().flat_map(|r| [r.x0, r.x1]).collect();
    let mut ys: Vec<f64> = trims.iter().flat_map(|r| [r.y0, r.y1]).collect();
    for v in [&mut xs, &mut ys] {
        v.sort_by(|a, b| a.total_cmp(b));
        v.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    }
    for x in xs {
        p.move_to((x, all.y1 + gap));
        p.line_to((x, all.y1 + gap + len));
        p.move_to((x, all.y0 - gap));
        p.line_to((x, all.y0 - gap - len));
    }
    for y in ys {
        p.move_to((all.x0 - gap, y));
        p.line_to((all.x0 - gap - len, y));
        p.move_to((all.x1 + gap, y));
        p.line_to((all.x1 + gap + len, y));
    }
    p
}

/// A registration target: a circle with a cross through it.
fn target(p: &mut BezPath, c: Point, r: f64) {
    p.extend(tracedraw_core::geometry::ellipse_path(Rect::new(
        c.x - r,
        c.y - r,
        c.x + r,
        c.y + r,
    )));
    p.move_to((c.x - r * 1.6, c.y));
    p.line_to((c.x + r * 1.6, c.y));
    p.move_to((c.x, c.y - r * 1.6));
    p.line_to((c.x, c.y + r * 1.6));
}

/// The marks of one sheet, as objects in sheet coordinates.
pub fn marks(
    sheet: &SheetPlan,
    o: &Options,
    sheet_no: usize,
    sheets: usize,
    pages: &[String],
) -> Vec<Shape> {
    let mut out = Vec::new();
    let m = o.marks;
    if !m.any() {
        return out;
    }
    let hair = |w: f64| Some(Stroke::new(Color::Registration, w));
    let content = sheet
        .cells
        .iter()
        .map(|c| c.trim)
        .reduce(|a, b| a.union(b))
        .unwrap_or(Rect::new(0.0, 0.0, sheet.size.width, sheet.size.height));
    let bleed = o.bleed.max(0.0);
    let band_top = content.y1 + bleed + MARK_MARGIN / 2.0;
    let band_bottom = content.y0 - bleed - MARK_MARGIN / 2.0;
    if m.crop {
        out.push(mark_shape(
            ShapeKind::Path {
                path: crop_marks(&sheet.cells, bleed),
                closed: false,
            },
            Fill::None,
            hair(0.1),
        ));
        // A fold mark at a booklet's spine.
        if o.layout == Layout::Booklet {
            if let (Some(a), Some(_)) = (sheet.cells.first(), sheet.cells.get(1)) {
                let x = a.trim.x1;
                let mut p = BezPath::new();
                for (y0, y1) in [
                    (content.y1 + bleed + 1.5, content.y1 + bleed + 6.5),
                    (content.y0 - bleed - 1.5, content.y0 - bleed - 6.5),
                ] {
                    p.move_to((x, y0));
                    p.line_to((x, y1));
                }
                let mut fold = mark_shape(
                    ShapeKind::Path {
                        path: p,
                        closed: false,
                    },
                    Fill::None,
                    hair(0.1),
                );
                if let Some(s) = &mut fold.stroke {
                    s.dash = vec![1.0, 1.0];
                }
                out.push(fold);
            }
        }
    }
    if m.registration {
        let mut p = BezPath::new();
        let mid_x = (content.x0 + content.x1) / 2.0;
        let mid_y = (content.y0 + content.y1) / 2.0;
        let side = bleed + MARK_MARGIN / 2.0;
        for c in [
            Point::new(mid_x, content.y1 + side),
            Point::new(mid_x, content.y0 - side),
            Point::new(content.x0 - side, mid_y),
            Point::new(content.x1 + side, mid_y),
        ] {
            target(&mut p, c, 2.0);
        }
        out.push(mark_shape(
            ShapeKind::Path {
                path: p,
                closed: false,
            },
            Fill::None,
            hair(0.15),
        ));
    }
    let patch = 4.0;
    if m.color_bars {
        // Solids, then the two-ink overprints, to the left of the top
        // registration target.
        let colors = [
            Color::cmyk_pct(100.0, 0.0, 0.0, 0.0),
            Color::cmyk_pct(0.0, 100.0, 0.0, 0.0),
            Color::cmyk_pct(0.0, 0.0, 100.0, 0.0),
            Color::cmyk_pct(0.0, 0.0, 0.0, 100.0),
            Color::cmyk_pct(0.0, 100.0, 100.0, 0.0),
            Color::cmyk_pct(100.0, 0.0, 100.0, 0.0),
            Color::cmyk_pct(100.0, 100.0, 0.0, 0.0),
            Color::cmyk_pct(50.0, 0.0, 0.0, 0.0),
            Color::cmyk_pct(0.0, 50.0, 0.0, 0.0),
            Color::cmyk_pct(0.0, 0.0, 50.0, 0.0),
            Color::cmyk_pct(0.0, 0.0, 0.0, 50.0),
        ];
        let right = (content.x0 + content.x1) / 2.0 - 8.0;
        for (i, c) in colors.iter().enumerate() {
            let x = right - (colors.len() - i) as f64 * patch;
            out.push(rect_shape(
                Rect::new(x, band_top - patch / 2.0, x + patch, band_top + patch / 2.0),
                *c,
            ));
        }
    }
    if m.densitometer {
        // Black from 0 to 100 % in steps of ten, right of the top target.
        let left = (content.x0 + content.x1) / 2.0 + 8.0;
        for i in 0..=10 {
            let x = left + i as f64 * patch;
            let mut s = rect_shape(
                Rect::new(x, band_top - patch / 2.0, x + patch, band_top + patch / 2.0),
                Color::cmyk_pct(0.0, 0.0, 0.0, i as f32 * 10.0),
            );
            s.stroke = Some(Stroke::new(Color::cmyk_pct(0.0, 0.0, 0.0, 40.0), 0.05));
            out.push(s);
        }
    }
    if m.page_numbers {
        for c in &sheet.cells {
            let Some(i) = c.page else { continue };
            out.push(mark_shape(
                ShapeKind::Text {
                    spans: vec![TextSpan::new((i + 1).to_string(), "DejaVu Sans", 6.0)],
                    origin: Point::new(c.trim.center().x, c.trim.y0 - bleed - 3.5),
                    frame: None,
                    align: tracedraw_core::document::TextAlign::Center,
                    para: Default::default(),
                    on_path: None,
                },
                Fill::Solid(Color::Registration),
                None,
            ));
        }
    }
    if m.page_info {
        let on_sheet: Vec<String> = sheet
            .cells
            .iter()
            .filter_map(|c| c.page)
            .filter_map(|i| pages.get(i).cloned())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        let text = crate::i18n::trf(
            "print.page_info",
            &[
                ("title", &o.title),
                ("pages", &on_sheet.join(", ")),
                ("sheet", &(sheet_no + 1).to_string()),
                ("sheets", &sheets.to_string()),
                ("when", &o.when),
            ],
        );
        out.push(mark_shape(
            ShapeKind::Text {
                spans: vec![TextSpan::new(text, "DejaVu Sans", 6.0)],
                origin: Point::new(
                    content.x0 + o.info_offset.0,
                    band_bottom - 1.0 + o.info_offset.1,
                ),
                frame: None,
                align: Default::default(),
                para: Default::default(),
                on_path: None,
            },
            Fill::Solid(Color::Registration),
            None,
        ));
    }
    out
}

/// Build the printed sheets of `doc` as a new document: each sheet one
/// page, the document's pages placed and clipped to their bleed, the marks
/// on top. Hidden and non-printing layers are left out; master layers go
/// with every page they show on.
pub fn impose(doc: &Document, o: &Options) -> Document {
    let sizes: Vec<Size> = doc.pages.iter().map(|p| p.size).collect();
    let names: Vec<String> = (1..=doc.pages.len()).map(|n| n.to_string()).collect();
    let plans = plan(&sizes, o);
    let mut out = Document::new(
        doc.title.clone(),
        plans
            .first()
            .map(|p| p.size)
            .unwrap_or(Size::new(210.0, 297.0)),
    );
    out.metadata = doc.metadata.clone();
    out.metadata.bleed = 0.0;
    out.symbols = doc.symbols.clone();
    out.pages.clear();
    out.master.clear();
    let bleed = o.bleed.max(0.0);
    let total = plans.len();
    for (n, sheet) in plans.iter().enumerate() {
        let mut layer = Layer::new(out.ids_mut().layer(), "Sheet");
        for cell in &sheet.cells {
            let Some(pi) = cell.page else {
                continue;
            };
            let Some(page) = doc.pages.get(pi) else {
                continue;
            };
            let mut contents = Vec::new();
            if let Some(bg) = &page.background {
                contents.push(mark_shape(
                    ShapeKind::Rect {
                        rect: Rect::new(
                            -bleed,
                            -bleed,
                            page.size.width + bleed,
                            page.size.height + bleed,
                        ),
                        radius: 0.0,
                        corners: None,
                    },
                    bg.clone(),
                    None,
                ));
            }
            for l in doc.layers_for_page(page.id).unwrap_or_default() {
                if l.visible && l.printable {
                    contents.extend(l.shapes.iter().filter(|s| s.visible).cloned());
                }
            }
            // The page and its bleed, nothing beyond.
            let frame = mark_shape(
                ShapeKind::Rect {
                    rect: Rect::new(
                        -bleed,
                        -bleed,
                        page.size.width + bleed,
                        page.size.height + bleed,
                    ),
                    radius: 0.0,
                    corners: None,
                },
                Fill::None,
                None,
            );
            let mut clip = mark_shape(
                ShapeKind::ClipFrame {
                    frame: Box::new(frame),
                    contents,
                },
                Fill::None,
                None,
            );
            clip.transform = cell.transform;
            layer.shapes.push(clip);
        }
        layer.shapes.extend(marks(sheet, o, n, total, &names));
        let page = Page {
            id: out.ids_mut().page(),
            name: format!("{}", n + 1),
            size: sheet.size,
            layers: vec![layer],
            guides: Vec::new(),
            background: None,
        };
        out.pages.push(page);
    }
    // Every object needs its own id in the new document.
    let mut ids = out.ids().clone();
    for page in out.pages.iter_mut() {
        for layer in page.layers.iter_mut() {
            for s in layer.shapes.iter_mut() {
                renumber(s, &mut ids);
            }
        }
    }
    out.set_ids(ids);
    out
}

fn renumber(s: &mut Shape, ids: &mut tracedraw_core::id::IdSource) {
    s.id = ids.shape();
    match &mut s.kind {
        ShapeKind::Group { children } => children.iter_mut().for_each(|c| renumber(c, ids)),
        ShapeKind::ClipFrame { frame, contents } => {
            renumber(frame, ids);
            contents.iter_mut().for_each(|c| renumber(c, ids));
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A5: Size = Size {
        width: 148.0,
        height: 210.0,
    };

    #[test]
    fn booklets_pair_the_pages_for_folding() {
        let pairs = |n| {
            booklet_order(n)
                .into_iter()
                .map(|(a, b)| (a.map(|i| i + 1), b.map(|i| i + 1)))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            pairs(8),
            vec![
                (Some(8), Some(1)),
                (Some(2), Some(7)),
                (Some(6), Some(3)),
                (Some(4), Some(5))
            ]
        );
        // Six pages fill eight with two blanks at the back.
        assert_eq!(
            pairs(6),
            vec![
                (None, Some(1)),
                (Some(2), None),
                (Some(6), Some(3)),
                (Some(4), Some(5))
            ]
        );
        assert_eq!(pairs(1), vec![(None, Some(1)), (None, None)]);
    }

    #[test]
    fn n_up_fills_sheets_in_reading_order_and_scales_to_fit() {
        let o = Options {
            layout: Layout::NUp { cols: 2, rows: 2 },
            sheet: Some(Size::new(297.0, 420.0)),
            gutter: 0.0,
            ..Default::default()
        };
        let sheets = plan(&[A5; 5], &o);
        assert_eq!(sheets.len(), 2);
        let pages: Vec<Option<usize>> = sheets[0].cells.iter().map(|c| c.page).collect();
        assert_eq!(pages, vec![Some(0), Some(1), Some(2), Some(3)]);
        assert_eq!(
            sheets[1].cells.iter().filter(|c| c.page.is_some()).count(),
            1
        );
        // Four A5 pages fit an A3 sheet at full size (296 x 420): page 1
        // top left, page 4 bottom right.
        let c0 = &sheets[0].cells[0];
        let c3 = &sheets[0].cells[3];
        assert!((c0.trim.width() - 148.0).abs() < 1e-6);
        assert!(c0.trim.x0 < c3.trim.x0 && c0.trim.y0 > c3.trim.y0);
        // On A4 they shrink by the square root of a half (A-sizes halve in
        // area), to fit the sheet's height.
        let o = Options {
            sheet: Some(Size::new(210.0, 297.0)),
            ..o
        };
        let s = plan(&[A5; 4], &o);
        let k = 297.0 / 420.0;
        let t = s[0].cells[0].trim;
        assert!((t.width() - 148.0 * k).abs() < 1e-6, "{t:?}");
        assert!((t.y1 - 297.0).abs() < 1e-6, "{t:?}");
    }

    #[test]
    fn fitted_sheets_grow_with_marks_bleed_and_gutters() {
        let o = Options {
            layout: Layout::Repeat { cols: 3, rows: 2 },
            gutter: 2.0,
            bleed: 3.0,
            marks: Marks {
                crop: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let card = Size::new(85.0, 55.0);
        let sheets = plan(&[card], &o);
        assert_eq!(sheets.len(), 1);
        assert_eq!(sheets[0].cells.len(), 6);
        assert!(sheets[0].cells.iter().all(|c| c.page == Some(0)));
        // 3 cards of 85 + 2*3 bleed, 2 gutters, 2 margins of 12 + 3.
        let w = 3.0 * 91.0 + 2.0 * 2.0 + 2.0 * 15.0;
        assert!(
            (sheets[0].size.width - w).abs() < 1e-6,
            "{:?}",
            sheets[0].size
        );
        // Crop marks at every cut: 6 vertical and 4 horizontal cut lines,
        // each with two marks.
        let marks = crop_marks(&sheets[0].cells, 3.0);
        let segments = marks
            .elements()
            .iter()
            .filter(|e| matches!(e, tracedraw_core::geometry::PathEl::MoveTo(_)))
            .count();
        assert_eq!(segments, (6 + 4) * 2);
    }

    #[test]
    fn imposing_places_and_clips_every_page() {
        let mut doc = Document::new("t", A5);
        let id = doc.ids_mut().shape();
        let s = Shape::new(
            id,
            ShapeKind::Rect {
                rect: Rect::new(10.0, 10.0, 20.0, 20.0),
                radius: 0.0,
                corners: None,
            },
        );
        doc.pages[0].layers[0].shapes.push(s);
        let o = Options {
            layout: Layout::NUp { cols: 2, rows: 1 },
            marks: Marks {
                crop: true,
                registration: true,
                color_bars: true,
                densitometer: true,
                page_info: true,
                page_numbers: true,
            },
            title: "t".into(),
            when: "now".into(),
            ..Default::default()
        };
        let out = impose(&doc, &o);
        assert_eq!(out.pages.len(), 1);
        let shapes = &out.pages[0].layers[0].shapes;
        // The page in a clip frame, then the marks: crop marks,
        // registration, 11 colour patches, 11 density steps, the page
        // number, the text.
        assert!(matches!(shapes[0].kind, ShapeKind::ClipFrame { .. }));
        assert_eq!(shapes.len(), 1 + 1 + 1 + 11 + 11 + 1 + 1);
        // Ids are all different.
        let mut seen = std::collections::HashSet::new();
        fn walk(s: &Shape, seen: &mut std::collections::HashSet<ShapeId>) -> bool {
            let fresh = seen.insert(s.id);
            match &s.kind {
                ShapeKind::ClipFrame { frame, contents } => {
                    fresh && walk(frame, seen) && contents.iter().all(|c| walk(c, seen))
                }
                _ => fresh,
            }
        }
        assert!(shapes.iter().all(|s| walk(s, &mut seen)));
        // A sheet the PDF writer accepts.
        let pdf = tracedraw_io::pdf::document_to_pdf(&out);
        assert!(pdf.starts_with(b"%PDF"));
    }

    /// The back of a double-sided sheet: the columns run right to left so
    /// each card prints behind its front, and nothing is mirrored (#387).
    #[test]
    fn the_back_side_runs_the_columns_right_to_left() {
        let card = Size::new(85.0, 55.0);
        let sizes = vec![card; 3];
        let o = Options {
            layout: Layout::NUp { cols: 3, rows: 1 },
            sheet: Some(Size::new(297.0, 210.0)),
            gutter: 5.0,
            ..Default::default()
        };
        let front = plan(&sizes, &o);
        let back = plan(
            &sizes,
            &Options {
                back_side: true,
                ..o.clone()
            },
        );
        let x_of = |p: &SheetPlan, page: usize| {
            p.cells
                .iter()
                .find(|c| c.page == Some(page))
                .map(|c| c.trim.center().x)
                .expect("placed")
        };
        let w = front[0].size.width;
        for page in 0..3 {
            // Mirror places on the sheet: what is left on the front is
            // right on the back.
            assert!((x_of(&front[0], page) + x_of(&back[0], page) - w).abs() < 1e-9);
        }
        assert!(x_of(&back[0], 0) > x_of(&back[0], 2));
        // The pages themselves are not mirrored.
        assert!(back[0]
            .cells
            .iter()
            .all(|c| c.transform.as_coeffs()[0] > 0.0));
    }

    /// Keep size: on a fixed sheet the pages are not shrunk for the marks'
    /// room; the page information moves by its offset (#388).
    #[test]
    fn kept_size_does_not_shrink_pages_for_marks() {
        let page = Size::new(200.0, 280.0);
        let base = Options {
            sheet: Some(Size::new(210.0, 297.0)),
            marks: Marks {
                crop: true,
                page_info: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let shrunk = plan(&[page], &base);
        let kept = plan(
            &[page],
            &Options {
                keep_size: true,
                ..base.clone()
            },
        );
        let w = |p: &[SheetPlan]| p[0].cells[0].trim.width();
        assert!(w(&shrunk) < 199.0, "{}", w(&shrunk));
        assert!((w(&kept) - 200.0).abs() < 1e-9);
        assert!(!overflows(&kept));
        // The page information where the drag put it.
        let origin = |o: &Options| {
            let sheets = plan(&[page], o);
            marks(&sheets[0], o, 0, 1, &["1".to_string()])
                .into_iter()
                .find_map(|s| match s.kind {
                    ShapeKind::Text { origin, .. } => Some(origin),
                    _ => None,
                })
                .expect("page info")
        };
        let a = origin(&base);
        let b = origin(&Options {
            info_offset: (15.0, 40.0),
            ..base.clone()
        });
        assert!(((b.x - a.x) - 15.0).abs() < 1e-9 && ((b.y - a.y) - 40.0).abs() < 1e-9);
    }

    /// Keep size with more columns and rows than fit: the grid stays as
    /// set and the pages keep their size, running off the sheet, which
    /// `overflows` tells; without it the pages shrink to fit (#496).
    #[test]
    fn kept_size_keeps_the_grid_and_the_page_size() {
        let page = Size::new(90.0, 50.0);
        let base = Options {
            layout: Layout::NUp { cols: 3, rows: 7 },
            sheet: Some(Size::new(210.0, 297.0)),
            ..Default::default()
        };
        let pages = vec![page; 21];
        let shrunk = plan(&pages, &base);
        assert_eq!(shrunk.len(), 1);
        assert_eq!(shrunk[0].cells.len(), 21);
        assert!(shrunk[0].cells[0].trim.width() < 89.0);
        assert!(!overflows(&shrunk));
        let kept = plan(
            &pages,
            &Options {
                keep_size: true,
                ..base.clone()
            },
        );
        assert_eq!(kept.len(), 1, "the grid as set: one sheet");
        assert_eq!(kept[0].cells.len(), 21);
        assert!(
            kept[0]
                .cells
                .iter()
                .all(|c| (c.trim.width() - 90.0).abs() < 1e-9
                    && (c.trim.height() - 50.0).abs() < 1e-9)
        );
        assert!(overflows(&kept));
        // A grid that fits keeps its pages on the sheet.
        let fits = plan(
            &vec![page; 4],
            &Options {
                layout: Layout::NUp { cols: 2, rows: 2 },
                keep_size: true,
                ..base
            },
        );
        assert!(!overflows(&fits));
    }
}
