//! Print layout: how pages go onto the printed sheets (imposition) and
//! the printers' marks around them. The Print dialog chooses; `print`
//! builds the sheets with [`impose`] and writes them as one PDF.
//!
//! Layouts: as in the document (one page per sheet), n-up (several pages
//! per sheet, across or down, see [`PageOrder`]), booklet (two pages per sheet side, bound
//! by saddle stitch or perfect binding, see [`Booklet`]) and step and
//! repeat (one page repeated over the sheet). On a fixed sheet the pages are scaled down to fit their cells;
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
    /// Two pages per sheet side in the order a folded booklet needs (the
    /// page count is rounded up to a multiple of four); how the sheets
    /// are bound is in [`Options::booklet`].
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

/// The order of the pages in the cells of an n-up sheet (#1658).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PageOrder {
    /// Left to right, then the next row down; the next sheet goes on
    /// from the last page of this one.
    #[default]
    Across,
    /// Top to bottom, then the next column to the right; the next sheet
    /// goes on from the last page of this one (2 x 3: 1 2 3 down the
    /// first column, 4 5 6 down the second, 7 on the next sheet).
    Down,
    /// Top to bottom, each column going on through the same column of
    /// every sheet before the next column starts, so the columns cut
    /// from the stack of sheets run in one series (2 x 3 on two sheets:
    /// 1 2 3 and 7 8 9 on the first sheet, 4 5 6 and 10 11 12 on the
    /// second).
    DownThroughSheets,
    /// Cut and stack: each cell runs down the stack of sheets, the next
    /// page on the next sheet, so the piles cut from the stacked sheets
    /// each run in one series and go on top of each other in order. With
    /// `pages` pages and `per` cells a sheet there are `sheets` =
    /// ceil(pages / per) sheets, and cell `n` of sheet `s` holds page
    /// `n * sheets + s` (100 pages 4 up: 1, 26, 51 and 76 on the first
    /// sheet, 2, 27, 52 and 77 on the second).
    CutAndStack,
}

impl PageOrder {
    /// The order a Print dialog index stands for (0 across).
    pub fn from_index(i: usize) -> Self {
        match i {
            1 => PageOrder::Down,
            2 => PageOrder::DownThroughSheets,
            3 => PageOrder::CutAndStack,
            _ => PageOrder::Across,
        }
    }

    /// The page in cell `n` (left to right, top to bottom) of sheet
    /// `sheet` of a grid of `cols` x `rows` holding `pages` pages over
    /// `sheets` sheets; `None` past the last page.
    pub fn page_at(
        self,
        n: usize,
        sheet: usize,
        cols: usize,
        rows: usize,
        sheets: usize,
        pages: usize,
    ) -> Option<usize> {
        let (cols, rows) = (cols.max(1), rows.max(1));
        let (col, row) = (n % cols, n / cols);
        let per = cols * rows;
        let i = match self {
            PageOrder::Across => sheet * per + n,
            PageOrder::Down => sheet * per + col * rows + row,
            PageOrder::DownThroughSheets => col * rows * sheets + sheet * rows + row,
            PageOrder::CutAndStack => n * sheets + sheet,
        };
        (i < pages).then_some(i)
    }
}

/// How the folded sheets of a booklet are bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Binding {
    /// Saddle stitch: every sheet folded inside the one before and
    /// stapled through the fold (8 pages: 8 1, 2 7 on the outer sheet).
    #[default]
    SaddleStitch,
    /// Perfect bound: the sheets go in signatures of
    /// [`Booklet::signature_sheets`] sheets, each signature folded on its
    /// own, the signatures stacked and glued at the spine (one sheet per
    /// signature: 4 1, 2 3 on the first sheet, 8 5, 6 7 on the second).
    PerfectBound,
}

/// The booklet's binding and the room it needs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Booklet {
    pub binding: Binding,
    /// Sheets in each signature of a perfect-bound booklet (1 to 32).
    pub signature_sheets: u32,
    /// Creep compensation of a saddle-stitched booklet, mm: how far the
    /// pages of the innermost sheet move toward the spine. The outer
    /// sheet stays, the sheets between move in proportion, so the fore
    /// edges line up once the sheets are nested and trimmed.
    pub creep: f64,
    /// The space between the two pages of a spread, mm, centred on the
    /// fold.
    pub gap: f64,
    /// Pages on each side of a sheet: 2 (one fold), 4, 8 or 16 (a large
    /// sheet folded into a signature of 8, 16 or 32 pages, #3380, #4279).
    pub up: u32,
}

impl Default for Booklet {
    fn default() -> Self {
        Booklet {
            binding: Binding::SaddleStitch,
            signature_sheets: 4,
            creep: 0.0,
            gap: 0.0,
            up: 2,
        }
    }
}

impl Booklet {
    /// Sheets per signature, clamped to 1..=32.
    pub fn sheets_per_signature(&self) -> usize {
        self.signature_sheets.clamp(1, 32) as usize
    }

    fn safe(v: f64, max: f64) -> f64 {
        if v.is_finite() {
            v.clamp(0.0, max)
        } else {
            0.0
        }
    }

    /// The gap, clamped to 0..=100 mm.
    pub fn gap_mm(&self) -> f64 {
        Self::safe(self.gap, 100.0)
    }

    /// The creep at the innermost sheet, clamped to 0..=20 mm.
    pub fn creep_mm(&self) -> f64 {
        Self::safe(self.creep, 20.0)
    }

    /// Pages on each side of a sheet: 2, 4, 8 or 16.
    pub fn pages_per_side(&self) -> usize {
        match self.up {
            4 => 4,
            8 => 8,
            16 => 16,
            _ => 2,
        }
    }

    /// The folds that make a signature of a sheet printed
    /// [`Self::pages_per_side`] pages a side ('v' across the columns, 'h'
    /// across the rows), the last one the spine.
    fn folds(&self) -> &'static [u8] {
        match self.pages_per_side() {
            16 => b"hvhv",
            8 => b"vhv",
            _ => b"hv",
        }
    }

    /// The grid of pages on a sheet side (columns, rows).
    pub fn grid(&self) -> (u32, u32) {
        match self.pages_per_side() {
            4 => (2, 2),
            8 => (4, 2),
            16 => (4, 4),
            _ => (2, 1),
        }
    }
}

/// Where the pages of one folded signature go on its sheet (#3380): the
/// page number within the signature (0 first) of each place of the front
/// and of the back (places left to right, top to bottom; the back as it
/// is seen once the sheet is turned over side to side), each with whether
/// the page stands upside down there. A sheet of `cols` x `rows` places
/// is folded in half by `folds` in turn ('v' across the columns, the left
/// half going behind; 'h' across the rows, the top half going behind),
/// the last fold the spine; read front to back, the leaves of the folded
/// sheet are the pages in order.
fn folded_places(cols: usize, rows: usize, folds: &[u8]) -> [Vec<(usize, bool)>; 2] {
    folded_layout(cols, rows, folds).0
}

/// [`folded_places`], with the side the spine is on for each place of
/// the front and of the back: -1 to its left, 1 to its right on the
/// flat sheet. The last fold, across the columns, lays the left half
/// behind the right one, so the spine is at the left of a page no fold
/// across the columns has turned over, at its right after an odd number
/// of them; the back, turned over side to side, has it the other way.
#[allow(clippy::type_complexity)]
fn folded_layout(
    cols: usize,
    rows: usize,
    folds: &[u8],
) -> ([Vec<(usize, bool)>; 2], [Vec<f64>; 2]) {
    // Each place of the folded sheet: its leaves front to back, each a
    // place of the flat sheet and whether its front faces the front.
    let (mut w, mut h) = (cols, rows);
    let mut stacks: Vec<Vec<(usize, bool)>> = (0..cols * rows).map(|i| vec![(i, true)]).collect();
    let mut turned = vec![false; cols * rows];
    let mut mirrored = vec![false; cols * rows];
    for &f in folds {
        let across = f == b'v';
        let (nw, nh) = if across { (w / 2, h) } else { (w, h / 2) };
        let stays = |x: usize, y: usize| if across { x >= nw } else { y >= nh };
        let mut next: Vec<Vec<(usize, bool)>> = vec![Vec::new(); nw * nh];
        for y in 0..h {
            for x in 0..w {
                if stays(x, y) {
                    let (nx, ny) = if across { (x - nw, y) } else { (x, y - nh) };
                    next[ny * nw + nx].extend(stacks[y * w + x].iter().copied());
                }
            }
        }
        for y in 0..h {
            for x in 0..w {
                if stays(x, y) {
                    continue;
                }
                let (nx, ny) = if across {
                    (w - 1 - x - nw, y)
                } else {
                    (x, h - 1 - y - nh)
                };
                let st = &stacks[y * w + x];
                for &(i, _) in st {
                    if across {
                        mirrored[i] = !mirrored[i];
                    } else {
                        turned[i] = !turned[i];
                    }
                }
                next[ny * nw + nx].extend(st.iter().rev().map(|&(i, front)| (i, !front)));
            }
        }
        stacks = next;
        (w, h) = (nw, nh);
    }
    let leaves = stacks.into_iter().next().unwrap_or_default();
    let mut front = vec![(0, false); cols * rows];
    let mut back = vec![(0, false); cols * rows];
    let mut spine_front = vec![-1.0; cols * rows];
    let mut spine_back = vec![1.0; cols * rows];
    for (k, (i, faces_front)) in leaves.into_iter().enumerate() {
        let (a, b) = if faces_front {
            (2 * k, 2 * k + 1)
        } else {
            (2 * k + 1, 2 * k)
        };
        let (x, y) = (i % cols, i / cols);
        front[i] = (a, turned[i]);
        // The back, turned over side to side: the column mirrored.
        back[y * cols + (cols - 1 - x)] = (b, turned[i]);
        let spine = if mirrored[i] { 1.0 } else { -1.0 };
        spine_front[i] = spine;
        spine_back[y * cols + (cols - 1 - x)] = -spine;
    }
    ([front, back], [spine_front, spine_back])
}

/// The sides of the sheets of a booklet printed 4 or 8 pages a side
/// (#3380), front then back of each sheet: the page in each place (`None`
/// blank) and whether it stands upside down. Each sheet folds into a
/// signature; saddle-stitched, the signatures nest one inside the other
/// (the first holds the first and the last pages), perfect-bound they are
/// gathered in order.
pub fn folded_sides(pages: usize, b: &Booklet) -> Vec<Vec<(Option<usize>, bool)>> {
    let up = b.pages_per_side();
    let (cols, rows) = b.grid();
    let places = folded_places(cols as usize, rows as usize, b.folds());
    let per = 2 * up;
    let sheets = pages.div_ceil(per).max(1);
    let padded = sheets * per;
    let half = per / 2;
    let mut out = Vec::new();
    for s in 0..sheets {
        let page = |i: usize| {
            let p = match b.binding {
                Binding::PerfectBound => s * per + i,
                Binding::SaddleStitch if i < half => s * half + i,
                Binding::SaddleStitch => padded - (s + 1) * half + (i - half),
            };
            (p < pages).then_some(p)
        };
        for side in &places {
            out.push(side.iter().map(|&(i, turned)| (page(i), turned)).collect());
        }
    }
    out
}

/// How far creep moves the page in each place of each side of
/// [`folded_sides`] across the sheet (mm, x right; #4280): toward the
/// spine, nothing for the outermost leaf, [`Booklet::creep_mm`] for the
/// innermost. In a signature the leaves nest round its middle;
/// saddle-stitched, the signatures nest too (the first round all the
/// others), so the creep grows from the outer leaf of the first
/// signature to the middle of the last; perfect-bound, each signature
/// is folded and trimmed on its own and takes the whole creep.
pub fn folded_creep(pages: usize, b: &Booklet) -> Vec<Vec<f64>> {
    let up = b.pages_per_side();
    let (cols, rows) = b.grid();
    let (places, spines) = folded_layout(cols as usize, rows as usize, b.folds());
    let per = 2 * up;
    let sheets = pages.div_ceil(per).max(1);
    // Leaves nested in one signature.
    let levels = per / 4;
    let (total, saddle) = match b.binding {
        Binding::SaddleStitch => (sheets * levels, true),
        Binding::PerfectBound => (levels, false),
    };
    let creep = b.creep_mm();
    let mut out = Vec::new();
    for s in 0..sheets {
        for (side, spine) in places.iter().zip(&spines) {
            out.push(
                side.iter()
                    .zip(spine)
                    .map(|(&(i, _), &dir)| {
                        let depth = i.min(per - 1 - i) / 2 + if saddle { s * levels } else { 0 };
                        if total < 2 || creep == 0.0 {
                            0.0
                        } else {
                            dir * creep * depth as f64 / (total - 1) as f64
                        }
                    })
                    .collect(),
            );
        }
    }
    out
}

/// The page count of a booklet and what it takes to print it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BookletInfo {
    /// The pages printed.
    pub pages: usize,
    /// Blank pages added at the end to reach a multiple of four.
    pub blanks: usize,
    /// Sheets of paper, each printed on both sides.
    pub sheets: usize,
    /// Signatures (1 for saddle stitch).
    pub signatures: usize,
}

/// What a booklet of `pages` pages bound as `b` says takes.
pub fn booklet_info(pages: usize, b: &Booklet) -> BookletInfo {
    if b.pages_per_side() > 2 {
        // One signature a sheet.
        let per = 2 * b.pages_per_side();
        let sheets = pages.div_ceil(per).max(1);
        return BookletInfo {
            pages,
            blanks: sheets * per - pages,
            sheets,
            signatures: sheets,
        };
    }
    let padded = pages.div_ceil(4).max(1) * 4;
    let sheets = padded / 4;
    let signatures = match b.binding {
        Binding::SaddleStitch => 1,
        Binding::PerfectBound => sheets.div_ceil(b.sheets_per_signature()),
    };
    BookletInfo {
        pages,
        blanks: padded - pages,
        sheets,
        signatures,
    }
}

/// One side of a booklet sheet: the left and right page (`None` blank),
/// and how deep its sheet sits in its fold (0 outermost) of how many.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BookletSide {
    pub left: Option<usize>,
    pub right: Option<usize>,
    pub depth: usize,
    pub nested: usize,
}

/// The sides of the sheets folded together for the pages `first ..
/// first + n` (`n` a multiple of four), outer sheet first, front then
/// back. Pages from `pages` on are blank.
fn nested_sides(first: usize, n: usize, pages: usize) -> Vec<BookletSide> {
    let page = |i: usize| (first + i < pages).then_some(first + i);
    (0..n / 2)
        .map(|side| {
            let (a, b) = (n - 1 - side, side);
            let (left, right) = if side % 2 == 0 {
                (page(a), page(b))
            } else {
                (page(b), page(a))
            };
            BookletSide {
                left,
                right,
                depth: side / 2,
                nested: n / 4,
            }
        })
        .collect()
}

/// The sides of every sheet of a booklet of `pages` pages bound as `b`,
/// in printing order.
pub fn booklet_sides(pages: usize, b: &Booklet) -> Vec<BookletSide> {
    let padded = pages.div_ceil(4).max(1) * 4;
    match b.binding {
        Binding::SaddleStitch => nested_sides(0, padded, pages),
        Binding::PerfectBound => {
            let per = 4 * b.sheets_per_signature();
            (0..padded)
                .step_by(per)
                .flat_map(|first| nested_sides(first, per.min(padded - first), pages))
                .collect()
        }
    }
}

/// The printers' marks to draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Marks {
    pub crop: bool,
    /// Short lines on the bleed's edges, beyond the corners (#2197).
    pub bleed: bool,
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
            || self.bleed
            || self.registration
            || self.color_bars
            || self.densitometer
            || self.page_info
            || self.page_numbers
    }
}

/// How the crop marks are drawn (mm): each line's length, the gap
/// between it and the page's edge (beyond the bleed), its width, and
/// whether marks also go in the gaps between the pages.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CropStyle {
    pub length: f64,
    pub offset: f64,
    pub width: f64,
    pub between: bool,
}

impl Default for CropStyle {
    fn default() -> Self {
        CropStyle {
            length: 5.0,
            offset: 1.5,
            width: 0.1,
            between: false,
        }
    }
}

impl CropStyle {
    /// Each value kept where the marks fit in the room kept for them.
    pub fn clamped(self) -> Self {
        let num = |v: f64, lo: f64, hi: f64, d: f64| {
            if v.is_finite() {
                v.clamp(lo, hi)
            } else {
                d
            }
        };
        let d = CropStyle::default();
        let offset = num(self.offset, 0.0, 6.0, d.offset);
        CropStyle {
            offset,
            length: num(self.length, 1.0, MARK_MARGIN - offset, d.length),
            width: num(self.width, 0.02, 1.0, d.width),
            between: self.between,
        }
    }
}

/// Everything the layout needs besides the document.
#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    pub layout: Layout,
    /// The sheet size; `None` fits the sheet to the pages.
    pub sheet: Option<Size>,
    /// Space between the cells side by side (the columns), in mm.
    pub gutter: f64,
    /// Space between the rows of cells, in mm (#1795); `None` takes
    /// [`Self::gutter`].
    pub gutter_y: Option<f64>,
    /// The order of the pages of an n-up layout (#1658).
    pub order: PageOrder,
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
    /// Margins of the paper kept clear (mm: left, right, top, bottom):
    /// the pages and marks go inside them (#580).
    pub paper_margins: [f64; 4],
    /// Every page moved on the sheet (mm, x right and y up) and stretched
    /// about its middle (x and y factors, 1 for none), with its clip and
    /// marks: Print Preview's artwork position and Transform (#536, #595).
    pub page_offset: (f64, f64),
    pub page_stretch: (f64, f64),
    /// Every page turned about its middle on the sheet, degrees
    /// counter-clockwise (0 for none): Print Preview's Rotation.
    pub page_rotation: f64,
    /// A further turn of the page in each place of the sheet, degrees
    /// counter-clockwise, by the place's number (0 the first, across the
    /// rows; missing ones 0): Print Preview's Rotation for the page
    /// chosen on the sheet, the same place on every sheet (#2587).
    pub cell_rotation: Vec<f64>,
    /// The pages moved between the places of the sheet, the same on
    /// every sheet: place `n` takes the page the layout puts in place
    /// `cell_order[n]` (see [`place_order`]; empty for none): Print
    /// Preview's Arrange pages.
    pub cell_order: Vec<usize>,
    /// On a fixed sheet the pages stretch to fill the room inside the
    /// margins, width and height each on its own (#591).
    pub stretch_fit: bool,
    /// Fit to page: on a fixed sheet the pages grow or shrink to fill the
    /// room inside the margins, keeping their proportions. What runs off
    /// a page stays cut at its edge, as on screen (#2678).
    pub fit: bool,
    /// The binding, creep and gap of the Booklet layout.
    pub booklet: Booklet,
    pub marks: Marks,
    /// How the crop marks are drawn.
    pub crop: CropStyle,
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
            gutter_y: None,
            order: PageOrder::Across,
            bleed: 0.0,
            scale: 1.0,
            back_side: false,
            keep_size: false,
            info_offset: (0.0, 0.0),
            paper_margins: [0.0; 4],
            page_offset: (0.0, 0.0),
            page_stretch: (1.0, 1.0),
            page_rotation: 0.0,
            cell_rotation: Vec::new(),
            cell_order: Vec::new(),
            stretch_fit: false,
            fit: false,
            booklet: Booklet::default(),
            marks: Marks::default(),
            crop: CropStyle::default(),
            title: String::new(),
            when: String::new(),
        }
    }
}

/// The start of the name of a page's clip frame on a sheet, before its
/// place number (see [`cell_of`]).
pub const CELL_NAME: &str = "cell ";

/// The place on the sheet of a page's clip frame [`impose`] made.
pub fn cell_of(s: &Shape) -> Option<usize> {
    s.name.as_deref()?.strip_prefix(CELL_NAME)?.parse().ok()
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

/// The order of the pages of a saddle-stitched booklet, two per sheet
/// side (left, right): for eight pages 8 1, 2 7, 6 3, 4 5. Pages past the
/// end are blank.
#[cfg(test)]
pub fn booklet_order(pages: usize) -> Vec<(Option<usize>, Option<usize>)> {
    booklet_sides(pages, &Booklet::default())
        .into_iter()
        .map(|s| (s.left, s.right))
        .collect()
}

/// Where each of `n` places takes its page from: `order` for the places
/// it names when it is an arrangement of them (each place once), the
/// rest (or all, for any other list) their own.
pub fn place_order(order: &[usize], n: usize) -> Vec<usize> {
    let mut out: Vec<usize> = (0..n).collect();
    let k = order.len();
    let mut seen = vec![false; k];
    let whole = k <= n
        && order
            .iter()
            .all(|&i| i < k && !std::mem::replace(&mut seen[i], true));
    if whole {
        out[..k].copy_from_slice(order);
    }
    out
}

/// Swap the pages in places `a` and `b` of `order` (see
/// [`Options::cell_order`]); an order back to the layout's own is
/// emptied.
pub fn swap_places(order: &mut Vec<usize>, a: usize, b: usize) {
    let n = order.len().max(a + 1).max(b + 1);
    let mut o = place_order(order, n);
    o.swap(a, b);
    while o.last().is_some_and(|&i| i + 1 == o.len()) {
        o.pop();
    }
    *order = o;
}

/// Plan the sheets for pages of `sizes`.
pub fn plan(sizes: &[Size], o: &Options) -> Vec<SheetPlan> {
    if sizes.is_empty() {
        return Vec::new();
    }
    let (cols, rows) = match o.layout {
        Layout::Booklet => o.booklet.grid(),
        _ => o.layout.grid(),
    };
    // A booklet printed 4 or 8 pages a side (#3380): its sides, with the
    // pages that stand upside down.
    let folded = match o.layout {
        Layout::Booklet if o.booklet.pages_per_side() > 2 => folded_sides(sizes.len(), &o.booklet),
        _ => Vec::new(),
    };
    // A booklet's sides, and how far creep moves each side's pages
    // toward the spine.
    let sides = match o.layout {
        Layout::Booklet if folded.is_empty() => booklet_sides(sizes.len(), &o.booklet),
        _ => Vec::new(),
    };
    // Folded signatures: each place moves on its own (#4280).
    let folded_shift: Vec<Vec<f64>> = if folded.is_empty() {
        Vec::new()
    } else {
        folded_creep(sizes.len(), &o.booklet)
    };
    let creep: Vec<f64> = sides
        .iter()
        .map(|s| {
            if o.booklet.binding != Binding::SaddleStitch || s.nested < 2 {
                0.0
            } else {
                o.booklet.creep_mm() * s.depth as f64 / (s.nested - 1) as f64
            }
        })
        .collect();
    // Which page goes in each cell, sheet by sheet.
    let sheets: Vec<Vec<Option<usize>>> = match o.layout {
        Layout::AsIs => (0..sizes.len()).map(|i| vec![Some(i)]).collect(),
        Layout::NUp { .. } => {
            let (c, r) = (cols as usize, rows as usize);
            let per = c * r;
            let n = sizes.len();
            let count = n.div_ceil(per);
            (0..count)
                .map(|sheet| {
                    (0..per)
                        .map(|cell| o.order.page_at(cell, sheet, c, r, count, n))
                        .collect()
                })
                .collect()
        }
        Layout::Booklet if !folded.is_empty() => folded
            .iter()
            .map(|side| side.iter().map(|(p, _)| *p).collect())
            .collect(),
        Layout::Booklet => sides.iter().map(|s| vec![s.left, s.right]).collect(),
        Layout::Repeat { .. } => (0..sizes.len())
            .map(|i| vec![Some(i); (cols * rows) as usize])
            .collect(),
    };
    // The pages moved between places (Arrange pages).
    let sheets: Vec<Vec<Option<usize>>> = if o.cell_order.is_empty() {
        sheets
    } else {
        sheets
            .into_iter()
            .map(|cells| {
                place_order(&o.cell_order, cells.len())
                    .into_iter()
                    .map(|i| cells[i])
                    .collect()
            })
            .collect()
    };
    // The pages of a folded signature that stand upside down, moved with
    // their pages.
    let folded_shift: Vec<Vec<f64>> = folded_shift
        .into_iter()
        .map(|shift| {
            if o.cell_order.is_empty() {
                shift
            } else {
                place_order(&o.cell_order, shift.len())
                    .into_iter()
                    .map(|i| shift.get(i).copied().unwrap_or(0.0))
                    .collect()
            }
        })
        .collect();
    let upside_down: Vec<Vec<bool>> = folded
        .iter()
        .map(|side| {
            let flags: Vec<bool> = side.iter().map(|f| f.1).collect();
            if o.cell_order.is_empty() {
                flags
            } else {
                place_order(&o.cell_order, flags.len())
                    .into_iter()
                    .map(|i| flags[i])
                    .collect()
            }
        })
        .collect();
    let margin = if o.marks.any() { MARK_MARGIN } else { 0.0 } + o.bleed.max(0.0);
    let pm = o
        .paper_margins
        .map(|m| if m.is_finite() { m.max(0.0) } else { 0.0 });
    let [ml, mr, mt, mb] = pm;
    // Booklet pages share the spine: no bleed between them, only the
    // booklet's gap.
    let gap = |v: f64| if v.is_finite() { v.max(0.0) } else { 0.0 };
    let (gutter, gutter_y, inner_bleed) = match o.layout {
        Layout::Booklet => (o.booklet.gap_mm(), o.booklet.gap_mm(), 0.0),
        _ => (
            gap(o.gutter),
            gap(o.gutter_y.unwrap_or(o.gutter)),
            o.bleed.max(0.0),
        ),
    };
    // The turn of the page in each place of the sheet (radians,
    // counter-clockwise), and the box round a page so turned.
    let finite = |v: f64| if v.is_finite() { v } else { 0.0 };
    let turn_at = |n: usize| {
        let own = o.cell_rotation.get(n).map_or(0.0, |&t| finite(t));
        (finite(o.page_rotation) + own).to_radians()
    };
    let turned = |s: Size, turn: f64| {
        let (sin, cos) = turn.sin_cos();
        let (sin, cos) = (snap(sin.abs()), snap(cos.abs()));
        Size::new(
            s.width * cos + s.height * sin,
            s.width * sin + s.height * cos,
        )
    };
    sheets
        .iter()
        .enumerate()
        .map(|(side, cells)| {
            let creep = creep.get(side).copied().unwrap_or(0.0);
            // The cell size: the largest page on this sheet, as turned in
            // its place, so a turned page keeps the gaps and its room
            // (#2660).
            let cell = cells
                .iter()
                .enumerate()
                .filter_map(|(n, p)| p.and_then(|i| sizes.get(i)).map(|s| turned(*s, turn_at(n))))
                .fold(Size::new(0.0, 0.0), |a, s| {
                    Size::new(a.width.max(s.width), a.height.max(s.height))
                });
            let cell = if cell.width > 0.0 && cell.height > 0.0 {
                cell
            } else {
                turned(sizes[0], turn_at(0))
            };
            // Pages with their bleed, and the gaps between them.
            let step_x = cell.width + 2.0 * inner_bleed + gutter;
            let step_y = cell.height + 2.0 * inner_bleed + gutter_y;
            let grid_w = cols as f64 * step_x - gutter;
            let grid_h = rows as f64 * step_y - gutter_y;
            let (size, (sx, sy)) = match o.sheet {
                Some(sheet) => {
                    // Kept size: the pages as they are; stretched: they
                    // fill the room each way; else the grid shrinks to fit
                    // inside the marks.
                    let avail_w = (sheet.width - ml - mr - 2.0 * margin).max(1.0);
                    let avail_h = (sheet.height - mt - mb - 2.0 * margin).max(1.0);
                    let s = if o.keep_size {
                        (1.0, 1.0)
                    } else if o.stretch_fit {
                        (avail_w / grid_w, avail_h / grid_h)
                    } else if o.fit {
                        let s = (avail_w / grid_w).min(avail_h / grid_h);
                        (s, s)
                    } else {
                        // Shrunk only for the pages and the marks: the
                        // bleed runs out past the pages' edges (to the
                        // paper's edge if need be), so a wider bleed never
                        // makes the pages smaller (#3919).
                        let marks = if o.marks.any() { MARK_MARGIN } else { 0.0 };
                        let room_w = (sheet.width - ml - mr - 2.0 * marks).max(1.0);
                        let room_h = (sheet.height - mt - mb - 2.0 * marks).max(1.0);
                        let pages_w = grid_w - 2.0 * cols as f64 * inner_bleed;
                        let pages_h = grid_h - 2.0 * rows as f64 * inner_bleed;
                        let s = (room_w / pages_w.max(1e-9))
                            .min(room_h / pages_h.max(1e-9))
                            .min(1.0);
                        (s, s)
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
                        Size::new(
                            grid_w * s + 2.0 * margin + ml + mr,
                            grid_h * s + 2.0 * margin + mt + mb,
                        ),
                        (s, s),
                    )
                }
            };
            // The grid centred in the paper's margins (the sheet when
            // there are none).
            let x0 = ml + (size.width - ml - mr - grid_w * sx) / 2.0;
            let y_top = size.height - mt - (size.height - mt - mb - grid_h * sy) / 2.0;
            // Moved and stretched about its middle as Print Preview asks.
            let (ox, oy) = o.page_offset;
            let (kx, ky) = o.page_stretch;
            let ok = |v: f64, d: f64| if v.is_finite() { v } else { d };
            let (ox, oy) = (ok(ox, 0.0), ok(oy, 0.0));
            let (kx, ky) = (ok(kx, 1.0).max(0.01), ok(ky, 1.0).max(0.01));
            let cells = cells
                .iter()
                .enumerate()
                .map(|(n, page)| {
                    // Upside down in a folded signature (#3380).
                    let flip = upside_down
                        .get(side)
                        .and_then(|f| f.get(n))
                        .is_some_and(|f| *f);
                    let turn = turn_at(n) + if flip { std::f64::consts::PI } else { 0.0 };
                    let col = n as u32 % cols;
                    let col = if o.back_side { cols - 1 - col } else { col } as f64;
                    let row = (n as u32 / cols) as f64;
                    let ps = page.and_then(|i| sizes.get(i)).copied().unwrap_or(cell);
                    // Creep: the left page moves right, the right page
                    // left; in a folded signature each place toward its
                    // own spine (#4280).
                    let toward_spine = match folded_shift.get(side).and_then(|f| f.get(n)) {
                        Some(&shift) if o.back_side => -shift,
                        Some(&shift) => shift,
                        None if col == 0.0 => creep,
                        None => -creep,
                    };
                    // The page centred in its cell (y up: row 0 at the top).
                    let cx = x0
                        + (col * step_x + inner_bleed + cell.width / 2.0 + toward_spine) * sx
                        + ox;
                    let cy = y_top - (row * step_y + inner_bleed + cell.height / 2.0) * sy + oy;
                    let (w, h) = (ps.width * sx * kx, ps.height * sy * ky);
                    // The page turned about its middle; its trim is the
                    // box round the turned page.
                    let Size {
                        width: bw,
                        height: bh,
                    } = turned(Size::new(w, h), turn);
                    let trim =
                        Rect::new(cx - bw / 2.0, cy - bh / 2.0, cx + bw / 2.0, cy + bh / 2.0);
                    let transform = if turn == 0.0 {
                        Affine::translate((cx - w / 2.0, cy - h / 2.0))
                            * Affine::scale_non_uniform(sx * kx, sy * ky)
                    } else {
                        Affine::translate((cx, cy))
                            * Affine::rotate(turn)
                            * Affine::translate((-w / 2.0, -h / 2.0))
                            * Affine::scale_non_uniform(sx * kx, sy * ky)
                    };
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

/// A sine or cosine a hair off 0 or 1 taken as it, so a quarter turn
/// swaps a page's width and height exactly.
fn snap(v: f64) -> f64 {
    if v < 1e-12 {
        0.0
    } else if 1.0 - v < 1e-12 {
        1.0
    } else {
        v
    }
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
    crop_marks_styled(cells, bleed, CropStyle::default())
}

/// Crop marks drawn as `style` says: outside the pages, and with
/// `between` also at each page's corners in the gaps between the pages,
/// wherever a mark stays clear of every other page and its bleed.
pub fn crop_marks_styled(cells: &[Cell], bleed: f64, style: CropStyle) -> BezPath {
    let style = style.clamped();
    let mut p = BezPath::new();
    let trims: Vec<Rect> = cells
        .iter()
        .filter(|c| c.page.is_some())
        .map(|c| c.trim)
        .collect();
    let Some(all) = trims.iter().copied().reduce(|a, b| a.union(b)) else {
        return p;
    };
    let gap = bleed.max(0.0) + style.offset;
    let len = style.length;
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
    if style.between {
        let clear = |a: Point, b: Point, own: usize| {
            let seg = Rect::from_points(a, b).inflate(1e-6, 1e-6);
            trims.iter().enumerate().all(|(i, t)| {
                let t = t.inflate(bleed.max(0.0), bleed.max(0.0));
                i == own || seg.intersect(t).area() <= 0.0 && !t.contains(a) && !t.contains(b)
            })
        };
        let mut seg = |a: Point, b: Point, own: usize| {
            // Not on the sheet's outer marks, nor inside the pages.
            let outside = |q: Point| {
                q.x < all.x0 - 1e-6
                    || q.x > all.x1 + 1e-6
                    || q.y < all.y0 - 1e-6
                    || q.y > all.y1 + 1e-6
            };
            if !outside(a) && !outside(b) && clear(a, b, own) {
                p.move_to(a);
                p.line_to(b);
            }
        };
        for (i, r) in trims.iter().enumerate() {
            for (x, sx) in [(r.x0, -1.0), (r.x1, 1.0)] {
                for (y, sy) in [(r.y0, -1.0), (r.y1, 1.0)] {
                    // The vertical line beyond the corner, then the
                    // horizontal one.
                    let y0 = y + sy * gap;
                    seg(Point::new(x, y0), Point::new(x, y0 + sy * len), i);
                    let x0 = x + sx * gap;
                    seg(Point::new(x0, y), Point::new(x0 + sx * len, y), i);
                }
            }
        }
    }
    p
}

/// Bleed marks (#2197): where the bleed around `content` ends, short
/// lines in the margin beyond it, from the bleed's edge out, starting
/// as far out as the crop marks so the two never touch the bleed.
pub fn bleed_marks(content: Rect, bleed: f64) -> BezPath {
    let mut p = BezPath::new();
    let b = content.inflate(bleed, bleed);
    let (gap, len) = (1.5, 3.5);
    for x in [b.x0, b.x1] {
        p.move_to((x, b.y1 + gap));
        p.line_to((x, b.y1 + gap + len));
        p.move_to((x, b.y0 - gap));
        p.line_to((x, b.y0 - gap - len));
    }
    for y in [b.y0, b.y1] {
        p.move_to((b.x0 - gap, y));
        p.line_to((b.x0 - gap - len, y));
        p.move_to((b.x1 + gap, y));
        p.line_to((b.x1 + gap + len, y));
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
                path: crop_marks_styled(&sheet.cells, bleed, o.crop),
                closed: false,
            },
            Fill::None,
            hair(o.crop.clamped().width),
        ));
        // A fold mark at a booklet's spine.
        if o.layout == Layout::Booklet {
            // The middle pair of the top row: the spine of a booklet
            // folded once, the first fold of a large sheet (#3380).
            let mid = (o.booklet.grid().0 / 2) as usize;
            if let (Some(a), Some(b)) = (sheet.cells.get(mid - 1), sheet.cells.get(mid)) {
                // The fold: half way between the pages (creep moves them
                // toward it by the same amount).
                let x = (a.trim.x1 + b.trim.x0) / 2.0;
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
    if m.bleed && bleed > 0.0 {
        out.push(mark_shape(
            ShapeKind::Path {
                path: bleed_marks(content, bleed),
                closed: false,
            },
            Fill::None,
            hair(0.1),
        ));
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

/// The marks of every page printed on its own (the layout as in the
/// document, the sheet fitted to the page), in each page's own
/// coordinates: they lie outside the page, in its bleed and the
/// [`MARK_MARGIN`] around it. A PDF export draws them there (#789).
/// `names` are the pages' names for the page information.
pub fn page_marks(sizes: &[Size], names: &[String], o: &Options) -> Vec<Vec<Shape>> {
    let o = Options {
        layout: Layout::AsIs,
        sheet: None,
        scale: 1.0,
        page_offset: (0.0, 0.0),
        page_stretch: (1.0, 1.0),
        page_rotation: 0.0,
        cell_rotation: Vec::new(),
        paper_margins: [0.0; 4],
        ..o.clone()
    };
    let plans = plan(sizes, &o);
    let total = plans.len();
    plans
        .iter()
        .enumerate()
        .map(|(n, sheet)| {
            let Some(cell) = sheet.cells.first() else {
                return Vec::new();
            };
            let to_page = Affine::translate((-cell.trim.x0, -cell.trim.y0));
            let mut shapes = marks(sheet, &o, n, total, names);
            for s in &mut shapes {
                s.transform = to_page * s.transform;
            }
            shapes
        })
        .collect()
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
    // The spot inks: separations make a plate for each, and the print
    // preview a box (#779).
    out.inks = doc.inks.clone();
    out.pages.clear();
    out.master.clear();
    let bleed = o.bleed.max(0.0);
    let total = plans.len();
    for (n, sheet) in plans.iter().enumerate() {
        let mut layer = Layer::new(out.ids_mut().layer(), "Sheet");
        for (ci, cell) in sheet.cells.iter().enumerate() {
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
            for l in doc.layers_drawn_for_page(page.id).unwrap_or_default() {
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
            // Its place on the sheet, for Print Preview to choose it.
            clip.name = Some(format!("{CELL_NAME}{ci}"));
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

/// How a sheet larger than the paper is printed in tiles (a poster): the
/// paper, its margins (left, right, top, bottom, mm) the printer cannot
/// print on, how much each tile overlaps its neighbours, and whether
/// trim marks and each tile's name ("A1": row letter, column number) go
/// in the margins.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tiling {
    pub paper: Size,
    pub margins: [f64; 4],
    pub overlap: f64,
    pub marks: bool,
}

impl Tiling {
    /// The part of the paper printed on (mm, y up).
    pub fn area(&self) -> Rect {
        let [l, r, t, b] = self.margins.map(|v| v.max(0.0));
        let w = (self.paper.width - l - r).max(1.0);
        let h = (self.paper.height - t - b).max(1.0);
        Rect::new(l, b, l + w, b + h)
    }

    /// The overlap, kept below half the printed part so that every tile
    /// moves the poster on.
    fn overlap_for(&self, area: Rect) -> f64 {
        self.overlap
            .max(0.0)
            .min(area.width().min(area.height()) / 2.0)
    }

    /// How many tiles across and down a sheet of `size` takes.
    pub fn count(&self, size: Size) -> (usize, usize) {
        let a = self.area();
        let o = self.overlap_for(a);
        let n = |len: f64, step: f64| {
            if len <= step + o + 1e-6 {
                1
            } else {
                ((len - o) / step - 1e-9).ceil().max(1.0) as usize
            }
        };
        (n(size.width, a.width() - o), n(size.height, a.height() - o))
    }
}

/// The name of a tile: its row as letters (A, B, ... Z, AA) and its
/// column as a number from 1.
pub fn tile_name(row: usize, col: usize) -> String {
    let mut letters = String::new();
    let mut n = row + 1;
    while n > 0 {
        let d = (n - 1) % 26;
        letters.insert(0, char::from(b'A' + d as u8));
        n = (n - 1) / 26;
    }
    format!("{letters}{}", col + 1)
}

/// Every sheet of `doc` split into tiles of the paper, left to right
/// and top to bottom: each tile shows its part of the sheet inside the
/// paper's margins, overlapping its neighbours, the tiles' grid centred
/// on the sheet. With marks, short lines in the margins show where to
/// trim the overlap (its middle) and each tile carries its name. A
/// sheet that fits the paper's printed part is one tile.
pub fn tile_sheets(doc: &Document, t: &Tiling) -> Document {
    let area = t.area();
    let o = t.overlap_for(area);
    let (step_x, step_y) = (area.width() - o, area.height() - o);
    let mut out = doc.clone();
    out.pages.clear();
    let mut ids = out.ids().clone();
    for (n, sheet) in doc.pages.iter().enumerate() {
        let (nx, ny) = t.count(sheet.size);
        // The grid's extent over the sheet, centred on it.
        let gw = nx as f64 * step_x + o;
        let gh = ny as f64 * step_y + o;
        let left = (sheet.size.width - gw) / 2.0;
        let top = sheet.size.height + (gh - sheet.size.height) / 2.0;
        let contents: Vec<Shape> = sheet
            .layers
            .iter()
            .filter(|l| l.visible)
            .flat_map(|l| l.shapes.iter().cloned())
            .collect();
        for row in 0..ny {
            for col in 0..nx {
                let x0 = left + col as f64 * step_x;
                let y1 = top - row as f64 * step_y;
                let part = Rect::new(x0, y1 - area.height(), x0 + area.width(), y1);
                let mut layer = Layer::new(ids.layer(), "Tile");
                let frame = mark_shape(
                    ShapeKind::Rect {
                        rect: part,
                        radius: 0.0,
                        corners: None,
                    },
                    Fill::None,
                    None,
                );
                let mut clip = mark_shape(
                    ShapeKind::ClipFrame {
                        frame: Box::new(frame),
                        contents: contents.clone(),
                    },
                    Fill::None,
                    None,
                );
                clip.transform = Affine::translate((area.x0 - part.x0, area.y0 - part.y0));
                clip.name = Some(TILE_NAME.to_string());
                layer.shapes.push(clip);
                if t.marks {
                    let near = [col > 0, col + 1 < nx, row + 1 < ny, row > 0];
                    layer
                        .shapes
                        .extend(tile_marks(t, area, o, near, &tile_name(row, col)));
                }
                for s in layer.shapes.iter_mut() {
                    renumber(s, &mut ids);
                }
                out.pages.push(Page {
                    id: ids.page(),
                    name: format!("{} {}", n + 1, tile_name(row, col)),
                    size: t.paper,
                    layers: vec![layer],
                    guides: Vec::new(),
                    background: None,
                });
            }
        }
    }
    out.set_ids(ids);
    out
}

/// The name of a tile's clip frame on its paper.
pub const TILE_NAME: &str = "tile";

/// A tile's marks: where to trim each overlap shared with a neighbour
/// (`near`: left, right, below, above), as short lines in the margin
/// (inside the printed edge when there is no margin), and its name.
fn tile_marks(t: &Tiling, a: Rect, o: f64, near: [bool; 4], name: &str) -> Vec<Shape> {
    let [ml, mr, mt, mb] = t.margins.map(|v| v.max(0.0));
    let tick = |room: f64| {
        if room >= 2.0 {
            (room - 1.0).min(5.0)
        } else {
            -3.0
        }
    };
    let mut p = BezPath::new();
    let mut vline = |x: f64| {
        let (up, down) = (tick(mt), tick(mb));
        p.move_to((x, a.y1 + up.min(0.0)));
        p.line_to((x, a.y1 + up.max(0.0)));
        p.move_to((x, a.y0 - down.min(0.0)));
        p.line_to((x, a.y0 - down.max(0.0)));
    };
    if near[0] {
        vline(a.x0 + o / 2.0);
    }
    if near[1] {
        vline(a.x1 - o / 2.0);
    }
    let mut hline = |y: f64, p: &mut BezPath| {
        let (l, r) = (tick(ml), tick(mr));
        p.move_to((a.x0 - l.min(0.0), y));
        p.line_to((a.x0 - l.max(0.0), y));
        p.move_to((a.x1 + r.min(0.0), y));
        p.line_to((a.x1 + r.max(0.0), y));
    };
    if near[2] {
        hline(a.y0 + o / 2.0, &mut p);
    }
    if near[3] {
        hline(a.y1 - o / 2.0, &mut p);
    }
    let mut out = Vec::new();
    if !p.elements().is_empty() {
        out.push(mark_shape(
            ShapeKind::Path {
                path: p,
                closed: false,
            },
            Fill::None,
            Some(Stroke::new(Color::Registration, 0.2)),
        ));
    }
    // The name in the bottom margin, else just inside the corner.
    let y = if mb >= 3.0 { a.y0 - 2.5 } else { a.y0 + 1.0 };
    out.push(mark_shape(
        ShapeKind::Text {
            spans: vec![TextSpan::new(name.to_string(), "DejaVu Sans", 6.0)],
            origin: Point::new(a.x0 + 1.0, y),
            frame: None,
            align: tracedraw_core::document::TextAlign::Left,
            para: Default::default(),
            on_path: None,
        },
        Fill::Solid(Color::Registration),
        None,
    ));
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

    /// The paper's margins are kept clear: the page sits in the middle of
    /// what is left, and shrinks to fit inside it when it would not; a
    /// fitted sheet grows by them (#580).
    #[test]
    fn pages_sit_inside_the_paper_margins() {
        let a4 = Size::new(210.0, 297.0);
        let o = Options {
            sheet: Some(a4),
            paper_margins: [20.0, 0.0, 10.0, 30.0],
            ..Default::default()
        };
        let one = plan(&[Size::new(100.0, 100.0)], &o);
        let t = one[0].cells[0].trim;
        assert!(
            (t.center().x - (20.0 + (210.0 - 20.0) / 2.0)).abs() < 1e-9,
            "{t:?}"
        );
        // y up: 30 at the bottom, 10 at the top.
        assert!(
            (t.center().y - (30.0 + (297.0 - 10.0 - 30.0) / 2.0)).abs() < 1e-9,
            "{t:?}"
        );
        // A page as large as the paper shrinks to the room inside.
        let full = plan(&[a4], &o);
        let t = full[0].cells[0].trim;
        assert!(
            t.x0 >= 20.0 - 1e-9 && t.y0 >= 30.0 - 1e-9 && t.y1 <= 297.0 - 10.0 + 1e-9,
            "{t:?}"
        );
        let fitted = plan(
            &[Size::new(100.0, 100.0)],
            &Options {
                paper_margins: [5.0, 5.0, 5.0, 5.0],
                ..Default::default()
            },
        );
        assert_eq!(fitted[0].size, Size::new(110.0, 110.0));
    }

    /// A page chosen on the sheet turns on its own (#2587): on a 2 x 2
    /// sheet the top two pages upside down, the bottom two upright; the
    /// same places turn on the next sheet, and each page's clip frame on
    /// the imposed sheet names its place.
    #[test]
    fn a_place_on_the_sheet_turns_on_its_own() {
        let page = Size::new(100.0, 140.0);
        let o = Options {
            layout: Layout::NUp { cols: 2, rows: 2 },
            sheet: Some(Size::new(297.0, 420.0)),
            cell_rotation: vec![180.0, 180.0],
            ..Default::default()
        };
        let sheets = plan(&[page; 6], &o);
        assert_eq!(sheets.len(), 2);
        for sheet in &sheets {
            for (n, c) in sheet
                .cells
                .iter()
                .enumerate()
                .filter(|(_, c)| c.page.is_some())
            {
                // The page's origin: its top right corner when turned.
                let at = c.transform * Point::new(0.0, 0.0);
                let near = |a: f64, b: f64| (a - b).abs() < 1e-6;
                if n < 2 {
                    assert!(
                        near(at.x, c.trim.x1) && near(at.y, c.trim.y1),
                        "{n}: {at:?}"
                    );
                } else {
                    assert!(
                        near(at.x, c.trim.x0) && near(at.y, c.trim.y0),
                        "{n}: {at:?}"
                    );
                }
            }
        }
        let mut doc = Document::new("D", page);
        for _ in 0..3 {
            let mut p = doc.pages[0].clone();
            p.id = doc.ids_mut().page();
            doc.pages.push(p);
        }
        let sheet = impose(&doc, &o);
        let places: Vec<Option<usize>> = sheet.pages[0].layers[0]
            .shapes
            .iter()
            .filter(|s| matches!(s.kind, ShapeKind::ClipFrame { .. }))
            .map(cell_of)
            .collect();
        assert_eq!(places, vec![Some(0), Some(1), Some(2), Some(3)]);
    }

    /// #2660: cards repeated 2 x 2 with 5 mm between them and turned a
    /// quarter keep 5 mm between them each way, side by side and one
    /// above the other, and none overlaps the next. The grid was laid out
    /// for the cards upright, so the turned ones stood far apart across
    /// and ran into each other down the sheet, printing cut.
    #[test]
    fn turned_pages_keep_the_gaps_between_them() {
        let card = Size::new(90.0, 50.0);
        for turn in [90.0, 270.0, -90.0] {
            for keep_size in [true, false] {
                let o = Options {
                    layout: Layout::Repeat { cols: 2, rows: 2 },
                    sheet: Some(Size::new(210.0, 297.0)),
                    gutter: 5.0,
                    gutter_y: Some(5.0),
                    keep_size,
                    page_rotation: turn,
                    ..Default::default()
                };
                let sheet = &plan(&[card], &o)[0];
                let t: Vec<Rect> = sheet.cells.iter().map(|c| c.trim).collect();
                let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
                let k = t[0].height() / card.width;
                assert!(near(t[0].width(), card.height * k), "{turn} {:?}", t[0]);
                assert!(near(t[1].x0 - t[0].x1, 5.0 * k), "{turn} across {t:?}");
                assert!(near(t[0].y0 - t[2].y1, 5.0 * k), "{turn} down {t:?}");
                assert!(!overflows(&[sheet.clone()]), "{turn}");
            }
        }
    }

    /// A turn of 90 degrees swaps the trim's width and height about the
    /// same middle, and the page's corner lands where the turn puts it.
    #[test]
    fn pages_turn_about_their_middle() {
        let page = Size::new(100.0, 50.0);
        let base = Options {
            sheet: Some(Size::new(210.0, 297.0)),
            keep_size: true,
            ..Default::default()
        };
        let before = plan(&[page], &base)[0].cells[0].clone();
        let turned = plan(
            &[page],
            &Options {
                page_rotation: 90.0,
                ..base.clone()
            },
        )[0]
        .cells[0]
            .clone();
        let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
        let (b, t) = (before.trim, turned.trim);
        assert!(near(t.center().x, b.center().x) && near(t.center().y, b.center().y));
        assert!(near(t.width(), b.height()) && near(t.height(), b.width()), "{t:?}");
        // The page's lower right corner goes to the upper right.
        let c = turned.transform * Point::new(page.width, 0.0);
        assert!(near(c.x, t.x1) && near(c.y, t.y1), "{c:?} {t:?}");
        let bad = plan(
            &[page],
            &Options {
                page_rotation: f64::NAN,
                ..base.clone()
            },
        )[0]
        .cells[0]
            .clone();
        assert_eq!(bad.trim, before.trim);
    }

    /// Stretched to fill, the page takes the whole room inside the
    /// margins, each way on its own (#591); moved and stretched in Print
    /// Preview, the trim moves by the offset and grows about its middle
    /// (#595), and the page's transform follows it.
    #[test]
    fn pages_stretch_to_fill_and_follow_the_preview_transform() {
        let a4 = Size::new(210.0, 297.0);
        let page = Size::new(100.0, 50.0);
        let o = Options {
            sheet: Some(a4),
            paper_margins: [10.0, 10.0, 20.0, 20.0],
            stretch_fit: true,
            ..Default::default()
        };
        let t = plan(&[page], &o)[0].cells[0].trim;
        let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
        assert!(near(t.x0, 10.0) && near(t.x1, 200.0), "{t:?}");
        assert!(near(t.y0, 20.0) && near(t.y1, 277.0), "{t:?}");
        let base = Options {
            sheet: Some(a4),
            ..Default::default()
        };
        let before = plan(&[page], &base)[0].cells[0].clone();
        let moved = plan(
            &[page],
            &Options {
                page_offset: (5.0, -3.0),
                page_stretch: (2.0, 1.0),
                ..base.clone()
            },
        )[0]
        .cells[0]
            .clone();
        let (b, m) = (before.trim, moved.trim);
        assert!(near(m.center().x, b.center().x + 5.0), "{m:?}");
        assert!(near(m.center().y, b.center().y - 3.0), "{m:?}");
        assert!(near(m.width(), 2.0 * b.width()) && near(m.height(), b.height()));
        // The page's corners land on the trim's.
        let far = moved.transform * Point::new(page.width, page.height);
        assert!(near(far.x, m.x1) && near(far.y, m.y1), "{far:?} {m:?}");
    }

    /// Fit to page: the page grows or shrinks to fill the room inside
    /// the margins, keeping its proportions; without it a small page
    /// keeps its size (#2678).
    #[test]
    fn fit_to_page_scales_the_page_to_the_paper() {
        let a4 = Size::new(210.0, 297.0);
        let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
        let o = |page: Size, fit| {
            plan(
                &[page],
                &Options {
                    sheet: Some(a4),
                    paper_margins: [10.0, 10.0, 20.0, 20.0],
                    fit,
                    ..Default::default()
                },
            )[0]
            .cells[0]
                .trim
        };
        let small = Size::new(100.0, 50.0);
        let t = o(small, true);
        assert!(near(t.width(), 190.0) && near(t.height(), 95.0), "{t:?}");
        assert!(near(t.center().x, 105.0), "centred: {t:?}");
        let t = o(small, false);
        assert!(near(t.width(), 100.0), "kept: {t:?}");
        let big = Size::new(420.0, 594.0);
        let t = o(big, true);
        assert!(
            near(t.height(), 257.0) && t.width() <= 190.0 + 1e-9,
            "{t:?}"
        );
    }

    const A5: Size = Size {
        width: 148.0,
        height: 210.0,
    };

    /// A large sheet printed 4 or 8 pages a side folds into a signature
    /// of 8 or 16 pages: the places are the printers' layouts, the pages
    /// of the top row upside down (#3380).
    #[test]
    fn large_sheets_fold_into_signatures_of_8_and_16_pages() {
        let side = |up: u32, n: usize, binding: Binding| {
            let b = Booklet {
                up,
                binding,
                ..Default::default()
            };
            folded_sides(n, &b)
                .into_iter()
                .map(|side| {
                    side.into_iter()
                        .map(|(p, turned)| (p.map_or(0, |i| i + 1), turned))
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        };
        let pages = |v: &[(usize, bool)]| v.iter().map(|p| p.0).collect::<Vec<_>>();
        let turned = |v: &[(usize, bool)]| v.iter().map(|p| p.1).collect::<Vec<_>>();
        // 8 pages, 4 a side: 5 4 (upside down) over 8 1; behind, 3 6
        // over 2 7.
        let s = side(4, 8, Binding::SaddleStitch);
        assert_eq!(s.len(), 2);
        assert_eq!(pages(&s[0]), vec![5, 4, 8, 1]);
        assert_eq!(pages(&s[1]), vec![3, 6, 2, 7]);
        assert_eq!(turned(&s[0]), vec![true, true, false, false]);
        assert_eq!(turned(&s[1]), vec![true, true, false, false]);
        // 16 pages, 8 a side.
        let s = side(8, 16, Binding::SaddleStitch);
        assert_eq!(pages(&s[0]), vec![5, 12, 9, 8, 4, 13, 16, 1]);
        assert_eq!(pages(&s[1]), vec![7, 10, 11, 6, 2, 15, 14, 3]);
        assert_eq!(
            turned(&s[0]),
            vec![true, true, true, true, false, false, false, false]
        );
        // Two signatures of 8: saddle-stitched the first holds 1 to 4 and
        // 13 to 16 (it goes round the second), perfect-bound 1 to 8.
        let s = side(4, 16, Binding::SaddleStitch);
        let mut first: Vec<usize> = pages(&s[0]).into_iter().chain(pages(&s[1])).collect();
        first.sort();
        assert_eq!(first, vec![1, 2, 3, 4, 13, 14, 15, 16]);
        let s = side(4, 16, Binding::PerfectBound);
        let mut first: Vec<usize> = pages(&s[0]).into_iter().chain(pages(&s[1])).collect();
        first.sort();
        assert_eq!(first, vec![1, 2, 3, 4, 5, 6, 7, 8]);
        // 10 pages: two sheets, the last six places blank.
        let info = booklet_info(
            10,
            &Booklet {
                up: 4,
                ..Default::default()
            },
        );
        assert_eq!((info.sheets, info.blanks), (2, 6));
        // On the sheet, the pages of the top row are turned half round.
        let o = Options {
            layout: Layout::Booklet,
            booklet: Booklet {
                up: 4,
                ..Default::default()
            },
            ..Default::default()
        };
        let plan = plan(&[Size::new(100.0, 150.0); 8], &o);
        assert_eq!(plan.len(), 2);
        let c = &plan[0].cells;
        assert_eq!(c.len(), 4);
        let upside_down = |cell: &Cell| cell.transform.as_coeffs()[0] < 0.0;
        assert!(upside_down(&c[0]) && upside_down(&c[1]));
        assert!(!upside_down(&c[2]) && !upside_down(&c[3]));
        assert!((plan[0].size.width - 200.0).abs() < 1e-9);
        assert!((plan[0].size.height - 300.0).abs() < 1e-9);
        assert!(c[0].trim.y0 >= c[2].trim.y1 - 1e-9, "the top row is above");
    }

    /// Perfect binding folds each signature on its own: with one sheet
    /// per signature the first sheet holds pages 1 to 4 and the second 5
    /// to 8; with two sheets per signature the sixteen pages go 1 to 8,
    /// then 9 to 16 (#788, #947).
    #[test]
    fn perfect_binding_folds_each_signature_on_its_own() {
        let pairs = |n, sheets| {
            let b = Booklet {
                binding: Binding::PerfectBound,
                signature_sheets: sheets,
                ..Default::default()
            };
            booklet_sides(n, &b)
                .into_iter()
                .map(|s| (s.left.map(|i| i + 1), s.right.map(|i| i + 1)))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            pairs(8, 1),
            vec![
                (Some(4), Some(1)),
                (Some(2), Some(3)),
                (Some(8), Some(5)),
                (Some(6), Some(7))
            ]
        );
        let two = pairs(16, 2);
        assert_eq!(two.len(), 8);
        assert_eq!(two[0], (Some(8), Some(1)));
        assert_eq!(two[3], (Some(4), Some(5)));
        assert_eq!(two[4], (Some(16), Some(9)));
        // Ten pages in signatures of two sheets: twelve pages, the last
        // signature of one sheet, two blanks at the back.
        let ten = pairs(10, 2);
        assert_eq!(ten.len(), 6);
        assert_eq!(ten[4], (None, Some(9)));
        assert_eq!(ten[5], (Some(10), None));
        let info = booklet_info(
            10,
            &Booklet {
                binding: Binding::PerfectBound,
                signature_sheets: 2,
                ..Default::default()
            },
        );
        assert_eq!(
            info,
            BookletInfo {
                pages: 10,
                blanks: 2,
                sheets: 3,
                signatures: 2
            }
        );
        let saddle = booklet_info(6, &Booklet::default());
        assert_eq!((saddle.blanks, saddle.sheets, saddle.signatures), (2, 2, 1));
    }

    /// The gap opens between the two pages of a spread, centred on the
    /// fold; creep moves the pages of the inner sheets toward the spine,
    /// the innermost by the creep, the outer one not at all (#788).
    #[test]
    fn booklet_gap_and_creep_move_the_pages() {
        let page = Size::new(100.0, 150.0);
        let o = Options {
            layout: Layout::Booklet,
            booklet: Booklet {
                gap: 10.0,
                creep: 2.0,
                ..Default::default()
            },
            ..Default::default()
        };
        // 12 pages: three sheets, six sides.
        let p = plan(&[page; 12], &o);
        assert_eq!(p.len(), 6);
        assert!((p[0].size.width - 210.0).abs() < 1e-9, "{:?}", p[0].size);
        let spine = p[0].size.width / 2.0;
        let gap_of = |s: &SheetPlan| s.cells[1].trim.x0 - s.cells[0].trim.x1;
        assert!((gap_of(&p[0]) - 10.0).abs() < 1e-9, "outer sheet: no creep");
        // The middle sheet moves by 1 mm a page, the inner by 2 mm.
        assert!((gap_of(&p[2]) - 8.0).abs() < 1e-9, "{}", gap_of(&p[2]));
        assert!((gap_of(&p[5]) - 6.0).abs() < 1e-9, "{}", gap_of(&p[5]));
        for s in &p {
            let mid = (s.cells[0].trim.x1 + s.cells[1].trim.x0) / 2.0;
            assert!((mid - spine).abs() < 1e-9, "pages centred on the fold");
            // The page drawn where its trim is.
            let c = &s.cells[0];
            let at = c.transform * Point::new(0.0, 0.0);
            assert!((at.x - c.trim.x0).abs() < 1e-9);
        }
        // Perfect binding takes no creep.
        let o = Options {
            booklet: Booklet {
                binding: Binding::PerfectBound,
                ..o.booklet
            },
            ..o
        };
        let p = plan(&[page; 12], &o);
        assert!(p.iter().all(|s| (gap_of(s) - 10.0).abs() < 1e-9));
        // Nonsense values stay harmless.
        let o = Options {
            booklet: Booklet {
                gap: f64::NAN,
                creep: -3.0,
                signature_sheets: 0,
                binding: Binding::PerfectBound,
                up: 3,
            },
            ..o
        };
        let p = plan(&[page; 5], &o);
        assert_eq!(p.len(), 4);
        assert!(p.iter().all(|s| gap_of(s).abs() < 1e-9));
    }

    /// A sheet 16 pages a side folds into a signature of 32 (#4279).
    #[test]
    fn sixteen_a_side_folds_into_a_signature_of_32() {
        let b = Booklet {
            up: 16,
            ..Default::default()
        };
        assert_eq!(b.grid(), (4, 4));
        let sides = folded_sides(32, &b);
        assert_eq!(sides.len(), 2);
        let mut all: Vec<usize> = sides.iter().flatten().filter_map(|p| p.0).collect();
        all.sort_unstable();
        assert_eq!(all, (0..32).collect::<Vec<_>>());
        // The first and last pages meet at the spine on the front, the
        // first on the right.
        let front = &sides[0];
        let at = |p: usize| front.iter().position(|c| c.0 == Some(p));
        let (first, last) = (at(0).unwrap_or(99), at(31).unwrap_or(99));
        assert_eq!(first / 4, last / 4, "same row");
        assert_eq!(first, last + 1, "{front:?}");
        // 40 pages take two sheets.
        let info = booklet_info(40, &b);
        assert_eq!((info.sheets, info.blanks), (2, 24));
        // Every leaf's two pages sit back to back: the place behind a
        // page holds its other side.
        for (f, k) in sides[0].iter().zip(0..) {
            let behind = sides[1][(k / 4) * 4 + 3 - k % 4].0;
            let (a, b) = (f.0.unwrap_or(0), behind.unwrap_or(0));
            assert_eq!(a.min(b) % 2, 0);
            assert_eq!(a.abs_diff(b), 1, "{a} behind {b}");
        }
    }

    /// Creep in folded signatures (#4280): the outer leaf stays, the
    /// innermost moves the whole creep toward the spine, both sides of a
    /// leaf by the same amount; the plan moves the pages by it.
    #[test]
    fn folded_signatures_take_creep() {
        for up in [4u32, 8, 16] {
            let b = Booklet {
                up,
                creep: 3.0,
                ..Default::default()
            };
            let per = 2 * up as usize;
            let sides = folded_sides(per, &b);
            let creep = folded_creep(per, &b);
            assert_eq!(creep.len(), sides.len());
            let shift_of = |p: usize| {
                sides
                    .iter()
                    .zip(&creep)
                    .find_map(|(s, c)| s.iter().position(|x| x.0 == Some(p)).map(|i| c[i]))
                    .unwrap_or(f64::NAN)
            };
            assert!(shift_of(0).abs() < 1e-9, "up {up}: first page stays");
            assert!(shift_of(per - 1).abs() < 1e-9);
            let mid = per / 2;
            assert!((shift_of(mid - 1).abs() - 3.0).abs() < 1e-9, "up {up}");
            assert!((shift_of(mid).abs() - 3.0).abs() < 1e-9, "up {up}");
            // Both pages of a leaf move the same way on the paper's
            // two sides: toward the spine, mirrored on the back.
            for p in (0..per).step_by(2) {
                assert!((shift_of(p) + shift_of(p + 1)).abs() < 1e-9, "up {up} page {p}");
            }
            // Toward the spine: two pages side by side that meet at the
            // spine (page p beside page per - 1 - p) move toward each
            // other.
            let (cols, _) = b.grid();
            let cols = cols as usize;
            for (side, c) in sides.iter().zip(&creep) {
                for k in 0..side.len() - 1 {
                    if k % cols == cols - 1 {
                        continue;
                    }
                    if let (Some(a), Some(z)) = (side[k].0, side[k + 1].0) {
                        if a + z == per - 1 {
                            assert!(c[k] >= 0.0 && c[k + 1] <= 0.0, "up {up}: {side:?} {c:?}");
                        }
                    }
                }
            }
            // The plan moves the pages.
            let o = Options {
                layout: Layout::Booklet,
                booklet: b,
                ..Default::default()
            };
            let none = Options {
                booklet: Booklet { creep: 0.0, ..b },
                ..o.clone()
            };
            let page = Size::new(100.0, 150.0);
            let with = plan(&vec![page; per], &o);
            let without = plan(&vec![page; per], &none);
            for ((a, b), shifts) in with.iter().zip(&without).zip(&creep) {
                for ((ca, cb), s) in a.cells.iter().zip(&b.cells).zip(shifts) {
                    assert!((ca.trim.x0 - cb.trim.x0 - s).abs() < 1e-9);
                }
            }
        }
        // Saddle-stitched signatures nest: the outer leaf of the second
        // signature moves, perfect-bound it does not.
        let b = Booklet {
            up: 4,
            creep: 3.0,
            ..Default::default()
        };
        let c = folded_creep(16, &b);
        assert!(c[2].iter().all(|v| v.abs() > 0.5), "{c:?}");
        let c = folded_creep(16, &Booklet { binding: Binding::PerfectBound, ..b });
        assert!(c[2].iter().any(|v| v.abs() < 1e-9), "{c:?}");
    }

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

    /// A page that just fits its paper with crop marks keeps its size
    /// when a bleed is added: the bleed runs past the page's edges
    /// (#3919).
    #[test]
    fn a_bleed_never_shrinks_the_pages() {
        let page = Size::new(180.0, 267.0);
        let at = |bleed: f64| {
            let o = Options {
                sheet: Some(Size::new(210.0, 297.0)),
                bleed,
                marks: Marks {
                    crop: true,
                    ..Default::default()
                },
                ..Default::default()
            };
            plan(&[page], &o)[0].cells[0].trim
        };
        let (none, five) = (at(0.0), at(5.0));
        assert!(
            (none.width() - five.width()).abs() < 1e-9,
            "{none:?} {five:?}"
        );
        assert!(
            (none.height() - five.height()).abs() < 1e-9,
            "{none:?} {five:?}"
        );
        assert!((none.center().x - five.center().x).abs() < 1e-9);
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
    fn arranged_places_take_their_pages_on_every_sheet() {
        assert_eq!(place_order(&[], 3), vec![0, 1, 2]);
        assert_eq!(place_order(&[1, 0], 4), vec![1, 0, 2, 3]);
        // Not an arrangement, or longer than the places: as laid out.
        assert_eq!(place_order(&[1, 1], 3), vec![0, 1, 2]);
        assert_eq!(place_order(&[0, 2], 3), vec![0, 1, 2]);
        assert_eq!(place_order(&[1, 0, 2], 2), vec![0, 1]);
        let mut order = Vec::new();
        swap_places(&mut order, 0, 3);
        assert_eq!(order, vec![3, 1, 2, 0]);
        swap_places(&mut order, 1, 2);
        assert_eq!(order, vec![3, 2, 1, 0]);
        swap_places(&mut order, 1, 2);
        swap_places(&mut order, 3, 0);
        assert!(order.is_empty(), "back to the layout's own");
        // Two up on a sheet of five pages of different sizes: the pages
        // trade places, each with its own size, on every sheet.
        let a4 = Size::new(210.0, 297.0);
        let sizes = [A5, a4, A5, a4, A5];
        let o = Options {
            layout: Layout::NUp { cols: 2, rows: 1 },
            ..Default::default()
        };
        let pages = |o: &Options| -> Vec<Vec<Option<usize>>> {
            plan(&sizes, o)
                .iter()
                .map(|s| s.cells.iter().map(|c| c.page).collect())
                .collect()
        };
        assert_eq!(
            pages(&o),
            vec![
                vec![Some(0), Some(1)],
                vec![Some(2), Some(3)],
                vec![Some(4), None]
            ]
        );
        let swapped = Options {
            cell_order: vec![1, 0],
            ..o.clone()
        };
        assert_eq!(
            pages(&swapped),
            vec![
                vec![Some(1), Some(0)],
                vec![Some(3), Some(2)],
                vec![None, Some(4)]
            ]
        );
        let plan = plan(&sizes, &swapped);
        let first = &plan[0].cells;
        assert!(first[0].trim.x1 <= first[1].trim.x0 + 1e-9);
        assert!((first[0].trim.width() - a4.width).abs() < 1e-9);
        assert!((first[1].trim.width() - A5.width).abs() < 1e-9);
    }

    #[test]
    fn crop_marks_take_their_style_and_go_between_pages() {
        let seg = |p: &BezPath| {
            p.elements()
                .iter()
                .filter(|e| matches!(e, tracedraw_core::geometry::PathEl::MoveTo(_)))
                .count()
        };
        let o = |gutter: f64| Options {
            layout: Layout::Repeat { cols: 3, rows: 2 },
            gutter,
            marks: Marks {
                crop: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let card = Size::new(85.0, 55.0);
        let wide = plan(&[card], &o(20.0));
        let style = CropStyle {
            length: 8.0,
            offset: 3.0,
            width: 0.3,
            between: true,
        };
        let marks = crop_marks_styled(&wide[0].cells, 0.0, style);
        // The outer marks (6 + 4 cut lines, two each) and, in the gaps,
        // two lines at each corner facing another page: 12 up or down,
        // 16 left or right.
        assert_eq!(seg(&marks), 20 + 28);
        let all = wide[0]
            .cells
            .iter()
            .map(|c| c.trim)
            .reduce(|a, b| a.union(b))
            .unwrap();
        let first = marks.elements()[..2].to_vec();
        let (
            tracedraw_core::geometry::PathEl::MoveTo(a),
            tracedraw_core::geometry::PathEl::LineTo(b),
        ) = (first[0], first[1])
        else {
            panic!("{first:?}");
        };
        assert!((a.y - (all.y1 + 3.0)).abs() < 1e-6 && (b.y - a.y - 8.0).abs() < 1e-6);
        // None of them on a page.
        for t in wide[0].cells.iter().map(|c| c.trim.inflate(-1e-3, -1e-3)) {
            for e in marks.elements() {
                if let tracedraw_core::geometry::PathEl::LineTo(q)
                | tracedraw_core::geometry::PathEl::MoveTo(q) = e
                {
                    assert!(!t.contains(*q), "{q:?} on {t:?}");
                }
            }
        }
        // A gap too narrow for them keeps the outer marks alone.
        let narrow = plan(&[card], &o(2.0));
        assert_eq!(seg(&crop_marks_styled(&narrow[0].cells, 0.0, style)), 20);
        // The sheet's marks take the width; a style out of range is
        // brought back where the marks fit.
        let opts = Options {
            crop: style,
            ..o(20.0)
        };
        let shapes = super::marks(&wide[0], &opts, 0, 1, &["1".into()]);
        assert!(shapes.iter().any(|s| s
            .stroke
            .as_ref()
            .is_some_and(|k| (k.width - 0.3).abs() < 1e-9)));
        let c = CropStyle {
            length: 50.0,
            offset: -1.0,
            width: f64::NAN,
            between: false,
        }
        .clamped();
        assert_eq!((c.length, c.offset, c.width), (MARK_MARGIN, 0.0, 0.1));
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
                bleed: false,
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

    /// The marks of each page on its own, in the page's coordinates: crop
    /// marks start beyond the bleed and stay inside the marks margin, the
    /// page information names the page (#789).
    #[test]
    fn page_marks_sit_around_each_page_in_its_own_coordinates() {
        let sizes = [Size::new(100.0, 50.0), Size::new(60.0, 60.0)];
        let names = vec!["Front".to_string(), "Back".to_string()];
        let o = Options {
            bleed: 3.0,
            // Ignored: each page is on its own sheet at its own size.
            layout: Layout::NUp { cols: 2, rows: 2 },
            page_offset: (40.0, 40.0),
            marks: Marks {
                crop: true,
                registration: true,
                color_bars: true,
                page_info: true,
                ..Default::default()
            },
            title: "Card".into(),
            when: "today".into(),
            ..Default::default()
        };
        let all = page_marks(&sizes, &names, &o);
        assert_eq!(all.len(), 2);
        for (marks, size) in all.iter().zip(sizes) {
            // Crop marks, registration, 11 colour patches, the text.
            assert_eq!(marks.len(), 1 + 1 + 11 + 1);
            let page = Rect::from_origin_size((0.0, 0.0), size);
            let room = page.inflate(3.0 + MARK_MARGIN, 3.0 + MARK_MARGIN);
            let bleed = page.inflate(3.0, 3.0);
            let crop = marks[0]
                .transform
                .transform_rect_bbox(match &marks[0].kind {
                    ShapeKind::Path { path, .. } => kurbo::Shape::bounding_box(path),
                    _ => panic!("crop marks are a path"),
                });
            assert!(
                crop.x0 >= room.x0 - 1e-9 && crop.x1 <= room.x1 + 1e-9,
                "{crop:?}"
            );
            assert!(
                crop.y0 >= room.y0 - 1e-9 && crop.y1 <= room.y1 + 1e-9,
                "{crop:?}"
            );
            assert!(crop.x0 < bleed.x0 && crop.y1 > bleed.y1);
        }
        let ShapeKind::Text { spans, origin, .. } = &all[1][13].kind else {
            panic!("the page information is text");
        };
        assert!(spans[0].text.contains("Card") && spans[0].text.contains("Back"));
        let at = all[1][13].transform * *origin;
        assert!(at.y < -3.0 && at.y > -3.0 - MARK_MARGIN, "{at:?}");
        // Bleed marks: on the bleed's edges, outside it and inside the
        // marks margin (#2197).
        let o = Options {
            marks: Marks {
                bleed: true,
                ..Default::default()
            },
            ..o.clone()
        };
        let marks = page_marks(&sizes, &names, &o);
        assert_eq!(marks[0].len(), 1);
        let ShapeKind::Path { path, .. } = &marks[0][0].kind else {
            panic!("bleed marks are a path");
        };
        let b = marks[0][0]
            .transform
            .transform_rect_bbox(kurbo::Shape::bounding_box(path));
        let bleed = Rect::new(-3.0, -3.0, 103.0, 53.0);
        assert!(b.x0 < bleed.x0 && b.x1 > bleed.x1, "{b:?}");
        assert!(b.x0 >= bleed.x0 - MARK_MARGIN && b.x1 <= bleed.x1 + MARK_MARGIN);
        let xs: Vec<f64> = path
            .elements()
            .iter()
            .filter_map(|e| match e {
                kurbo::PathEl::MoveTo(p) => Some(marks[0][0].transform * *p),
                _ => None,
            })
            .filter(|p| p.y > 53.0)
            .map(|p| p.x)
            .collect();
        assert_eq!(xs, vec![-3.0, 103.0]);
        // Without a bleed there is no edge to mark.
        let o = Options { bleed: 0.0, ..o };
        assert!(page_marks(&sizes, &names, &o).iter().all(|m| m.is_empty()));
        // No marks asked for: none.
        let none = page_marks(&sizes, &names, &Options::default());
        assert!(none.iter().all(|m| m.is_empty()));
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

    /// #1795: the gap between the columns and the gap between the rows
    /// are set apart; without a row gap the column gap serves both, as
    /// before.
    #[test]
    fn the_column_and_row_gaps_are_set_apart() {
        let card = Size::new(85.0, 55.0);
        let o = Options {
            layout: Layout::Repeat { cols: 2, rows: 2 },
            gutter: 4.0,
            gutter_y: Some(10.0),
            ..Default::default()
        };
        let s = &plan(&[card], &o)[0];
        assert!((s.size.width - (2.0 * 85.0 + 4.0)).abs() < 1e-9, "{s:?}");
        assert!((s.size.height - (2.0 * 55.0 + 10.0)).abs() < 1e-9, "{s:?}");
        let (a, b, c) = (s.cells[0].trim, s.cells[1].trim, s.cells[2].trim);
        assert!((b.x0 - a.x1 - 4.0).abs() < 1e-9, "column gap {a:?} {b:?}");
        assert!((a.y0 - c.y1 - 10.0).abs() < 1e-9, "row gap {a:?} {c:?}");
        let same = &plan(
            &[card],
            &Options {
                gutter_y: None,
                ..o.clone()
            },
        )[0];
        assert!((same.size.height - (2.0 * 55.0 + 4.0)).abs() < 1e-9);
        // n-up on a fixed sheet keeps the two gaps too.
        let n = &plan(
            &[card; 4],
            &Options {
                layout: Layout::NUp { cols: 2, rows: 2 },
                sheet: Some(Size::new(297.0, 210.0)),
                ..o
            },
        )[0];
        let (a, b, c) = (n.cells[0].trim, n.cells[1].trim, n.cells[2].trim);
        assert!((b.x0 - a.x1 - 4.0).abs() < 1e-9);
        assert!((a.y0 - c.y1 - 10.0).abs() < 1e-9);
    }

    /// Cut and stack: each place on the sheet runs down the stack, so the
    /// piles cut from it each hold one run of records.
    #[test]
    fn n_up_cut_and_stack_numbers_down_the_stack() {
        let plan_of = |order: PageOrder, n: usize, cols: u32, rows: u32| {
            let o = Options {
                layout: Layout::NUp { cols, rows },
                order,
                ..Default::default()
            };
            plan(&vec![A5; n], &o)
                .iter()
                .map(|s| s.cells.iter().map(|c| c.page.map(|p| p + 1)).collect())
                .collect::<Vec<Vec<Option<usize>>>>()
        };
        // 100 records 4 up: 25 sheets.
        let sheets = plan_of(PageOrder::CutAndStack, 100, 2, 2);
        assert_eq!(sheets.len(), 25);
        assert_eq!(sheets[0], [Some(1), Some(26), Some(51), Some(76)]);
        assert_eq!(sheets[1], [Some(2), Some(27), Some(52), Some(77)]);
        assert_eq!(sheets[24], [Some(25), Some(50), Some(75), Some(100)]);
        // Each place, read down the stack, is one run.
        for p in 0..4 {
            let pile: Vec<usize> = sheets.iter().filter_map(|s| s[p]).collect();
            assert_eq!(pile, ((p * 25 + 1)..=(p * 25 + 25)).collect::<Vec<_>>());
        }
        // Every record once.
        let mut all: Vec<usize> = sheets.iter().flatten().flatten().copied().collect();
        all.sort_unstable();
        assert_eq!(all, (1..=100).collect::<Vec<_>>());
        // An uneven count: 10 records 4 up take 3 sheets, the last place
        // holding only record 10.
        assert_eq!(
            plan_of(PageOrder::CutAndStack, 10, 2, 2),
            vec![
                vec![Some(1), Some(4), Some(7), Some(10)],
                vec![Some(2), Some(5), Some(8), None],
                vec![Some(3), Some(6), Some(9), None],
            ]
        );
        // One row of three, 7 records: 3 sheets.
        assert_eq!(
            plan_of(PageOrder::CutAndStack, 7, 3, 1),
            vec![
                vec![Some(1), Some(4), Some(7)],
                vec![Some(2), Some(5), None],
                vec![Some(3), Some(6), None],
            ]
        );
        // Off, the order is as before.
        assert_eq!(
            plan_of(PageOrder::Across, 10, 2, 2)[0],
            [Some(1), Some(2), Some(3), Some(4)]
        );
        assert_eq!(PageOrder::from_index(3), PageOrder::CutAndStack);
        assert_eq!(PageOrder::from_index(0), PageOrder::Across);
    }

    /// #1658: several pages per sheet run down the columns: on the same
    /// sheet first (1 2 3 down the first column, 4 5 6 down the second),
    /// or each column through the stack of sheets before the next one
    /// starts.
    #[test]
    fn n_up_pages_can_run_down_the_columns() {
        let pages = |order: PageOrder, n: usize| -> Vec<Vec<Option<usize>>> {
            let o = Options {
                layout: Layout::NUp { cols: 2, rows: 3 },
                order,
                ..Default::default()
            };
            plan(&vec![A5; n], &o)
                .iter()
                .map(|s| s.cells.iter().map(|c| c.page.map(|p| p + 1)).collect())
                .collect()
        };
        // Cells are listed left to right, top to bottom.
        assert_eq!(
            pages(PageOrder::Across, 7),
            vec![
                vec![Some(1), Some(2), Some(3), Some(4), Some(5), Some(6)],
                vec![Some(7), None, None, None, None, None],
            ]
        );
        assert_eq!(
            pages(PageOrder::Down, 8),
            vec![
                vec![Some(1), Some(4), Some(2), Some(5), Some(3), Some(6)],
                vec![Some(7), None, Some(8), None, None, None],
            ]
        );
        assert_eq!(
            pages(PageOrder::DownThroughSheets, 12),
            vec![
                vec![Some(1), Some(7), Some(2), Some(8), Some(3), Some(9)],
                vec![Some(4), Some(10), Some(5), Some(11), Some(6), Some(12)],
            ]
        );
        // Ten pages over two sheets: the second column of the second
        // sheet ends early.
        assert_eq!(
            pages(PageOrder::DownThroughSheets, 10),
            vec![
                vec![Some(1), Some(7), Some(2), Some(8), Some(3), Some(9)],
                vec![Some(4), Some(10), Some(5), None, Some(6), None],
            ]
        );
        // The page in the top left cell is placed top left.
        let o = Options {
            layout: Layout::NUp { cols: 2, rows: 3 },
            order: PageOrder::Down,
            ..Default::default()
        };
        let s = &plan(&[A5; 6], &o)[0];
        let at = |p: usize| s.cells.iter().find(|c| c.page == Some(p)).map(|c| c.trim);
        let (p1, p2, p4) = (at(0).unwrap(), at(1).unwrap(), at(3).unwrap());
        assert!(
            (p1.x0 - p2.x0).abs() < 1e-9 && p2.y1 <= p1.y0 + 1e-9,
            "2 under 1"
        );
        assert!(
            (p1.y0 - p4.y0).abs() < 1e-9 && p4.x0 >= p1.x1 - 1e-9,
            "4 beside 1"
        );
    }

    /// A sheet larger than the paper prints in tiles: 400 x 300 mm on A4
    /// with 5 mm margins and 10 mm overlap takes 3 x 2 tiles, each the
    /// paper's size, overlapping by 10 mm, the grid centred on the sheet,
    /// named A1 to B3, with trim marks where a neighbour is.
    #[test]
    fn a_large_sheet_prints_in_overlapping_tiles() {
        let mut doc = Document::new("poster", Size::new(400.0, 300.0));
        let mut ids = doc.ids().clone();
        let mut r = Shape::new(
            ids.shape(),
            ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 400.0, 300.0),
                radius: 0.0,
                corners: None,
            },
        );
        r.fill = Fill::Solid(Color::BLACK);
        doc.set_ids(ids);
        doc.pages[0].layers[0].shapes.push(r);
        let t = Tiling {
            paper: Size::new(210.0, 297.0),
            margins: [5.0; 4],
            overlap: 10.0,
            marks: true,
        };
        assert_eq!(t.count(Size::new(400.0, 300.0)), (3, 2));
        assert_eq!(t.count(Size::new(200.0, 287.0)), (1, 1), "fits: one tile");
        let tiles = tile_sheets(&doc, &t);
        assert_eq!(tiles.pages.len(), 6);
        let names: Vec<&str> = tiles.pages.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["1 A1", "1 A2", "1 A3", "1 B1", "1 B2", "1 B3"]);
        // Each tile's part of the sheet, through its clip frame.
        let part = |i: usize| {
            let s = &tiles.pages[i].layers[0].shapes[0];
            assert_eq!(s.name.as_deref(), Some(TILE_NAME));
            let ShapeKind::ClipFrame { frame, contents } = &s.kind else {
                panic!("a clip frame");
            };
            assert_eq!(contents.len(), 1);
            let ShapeKind::Rect { rect, .. } = frame.kind else {
                panic!("a frame");
            };
            // Shown inside the margins.
            let on_paper = s.transform.transform_rect_bbox(rect);
            assert!((on_paper.x0 - 5.0).abs() < 1e-9 && (on_paper.y0 - 5.0).abs() < 1e-9);
            rect
        };
        let (a1, a2, a3, b1) = (part(0), part(1), part(2), part(3));
        assert!((a1.width() - 200.0).abs() < 1e-9 && (a1.height() - 287.0).abs() < 1e-9);
        assert!((a1.x1 - a2.x0 - 10.0).abs() < 1e-9, "overlap across");
        assert!((b1.y1 - a1.y0 - 10.0).abs() < 1e-9, "overlap down");
        // Centred: as much of the grid past the left edge as the right.
        assert!(((0.0 - a1.x0) - (a3.x1 - 400.0)).abs() < 1e-9);
        assert!(tiles.pages.iter().all(|p| p.size == t.paper));
        // A1 has neighbours right and below: its trim marks and its name.
        let marks = &tiles.pages[0].layers[0].shapes;
        assert!(marks
            .iter()
            .any(|s| matches!(s.kind, ShapeKind::Path { .. })));
        assert!(marks.iter().any(|s| matches!(
            &s.kind,
            ShapeKind::Text { spans, .. } if spans[0].text == "A1"
        )));
        assert_eq!(tile_name(25, 0), "Z1");
        assert_eq!(tile_name(26, 9), "AA10");
    }
}
