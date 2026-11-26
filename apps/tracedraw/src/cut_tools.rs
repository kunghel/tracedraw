//! Cutting commands of Object > Shaping: Cut Objects Below (the selected
//! object cuts everything under it and goes), Cut into Grid (each
//! selected object becomes rows and columns of pieces: their number,
//! size, the gaps between them and the size they take together are set,
//! the pieces are boxes or keep the object's outline, guidelines can be
//! added at their edges, and the drawing shows them before OK), and
//! Remove Strays (lone points, curves with
//! no fill and no outline, and empty texts on the page go). Each is one
//! undo step. See docs/help/en/lines-shapes/shaping.md.

use crate::app::App;
use tracedraw_core::document::{Guide, GuideLine};
use tracedraw_core::geometry::{BezPath, Point, Rect, Shape as _};
use tracedraw_core::shaping::{self, FillRule, Op};
use tracedraw_core::{Command, Fill, LayerId, Shape, ShapeId, ShapeKind};

/// The settings of Cut into Grid: the window is open while `open`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridCut {
    pub open: bool,
    pub rows: u32,
    pub cols: u32,
    /// The gap between columns, mm.
    pub gap_x: f64,
    /// The gap between rows, mm.
    pub gap_y: f64,
    /// The width the columns and their gaps take together, mm, from the
    /// object's left edge; `None` is the object's own width.
    pub total_w: Option<f64>,
    /// The height the rows and their gaps take together, mm, from the
    /// object's top edge; `None` is the object's own height.
    pub total_h: Option<f64>,
    /// The pieces keep the object's outline instead of being boxes.
    pub keep_shape: bool,
    /// Guidelines are added along the pieces' edges.
    pub guides: bool,
    /// The pieces of an object are put in a group.
    pub group: bool,
    /// The drawing shows the pieces while the window is open.
    pub preview: bool,
}

impl Default for GridCut {
    fn default() -> Self {
        Self {
            open: false,
            rows: 2,
            cols: 2,
            gap_x: 0.0,
            gap_y: 0.0,
            total_w: None,
            total_h: None,
            keep_shape: false,
            guides: false,
            group: true,
            preview: true,
        }
    }
}

/// The smallest row height or column width Cut into Grid makes, mm.
const GRID_CUT_MIN_CELL: f64 = 1e-3;

/// One direction of a grid: `n` cells of `size` with `gap` between them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridAxis {
    pub n: u32,
    pub size: f64,
    pub gap: f64,
}

impl GridAxis {
    /// `n` cells with `gap` between them sharing `total`; `None` when
    /// the gaps leave no room for the cells.
    fn shared(n: u32, gap: f64, total: f64) -> Option<Self> {
        let n = n.clamp(1, GRID_CUT_MAX);
        let gap = if gap.is_finite() { gap.max(0.0) } else { 0.0 };
        let size = (total - gap * f64::from(n - 1)) / f64::from(n);
        (size.is_finite() && size >= GRID_CUT_MIN_CELL).then_some(Self { n, size, gap })
    }

    /// What the cells and their gaps take together.
    pub fn total(&self) -> f64 {
        self.size * f64::from(self.n) + self.gap * f64::from(self.n - 1)
    }

    /// How far cell `i` starts from the first cell's start.
    fn start(&self, i: u32) -> f64 {
        f64::from(i) * (self.size + self.gap)
    }
}

impl GridCut {
    /// The columns of an object `width` wide.
    pub fn columns(&self, width: f64) -> Option<GridAxis> {
        GridAxis::shared(self.cols, self.gap_x, self.total_w.unwrap_or(width))
    }

    /// The rows of an object `height` high.
    pub fn row_axis(&self, height: f64) -> Option<GridAxis> {
        GridAxis::shared(self.rows, self.gap_y, self.total_h.unwrap_or(height))
    }

    /// The boxes of the pieces of an object with bounds `b` (page mm),
    /// row by row from the top, each row from the left. The grid starts
    /// at the object's top-left corner.
    pub fn cells(&self, b: Rect) -> Vec<Rect> {
        let (Some(cx), Some(cy)) = (self.columns(b.width()), self.row_axis(b.height())) else {
            return Vec::new();
        };
        let mut out = Vec::with_capacity((cx.n * cy.n) as usize);
        for r in 0..cy.n {
            let top = b.y1 - cy.start(r);
            for c in 0..cx.n {
                let x = b.x0 + cx.start(c);
                out.push(Rect::new(x, top - cy.size, x + cx.size, top));
            }
        }
        out
    }

    /// Where guidelines go for an object with bounds `b`: the x of the
    /// columns' edges and the y of the rows' edges.
    pub fn edges(&self, b: Rect) -> (Vec<f64>, Vec<f64>) {
        let (Some(cx), Some(cy)) = (self.columns(b.width()), self.row_axis(b.height())) else {
            return (Vec::new(), Vec::new());
        };
        let mut xs = Vec::new();
        for c in 0..cx.n {
            let x = b.x0 + cx.start(c);
            xs.extend([x, x + cx.size]);
        }
        let mut ys = Vec::new();
        for r in 0..cy.n {
            let y = b.y1 - cy.start(r);
            ys.extend([y, y - cy.size]);
        }
        let near = |a: &f64, b: &f64| (a - b).abs() < 1e-6;
        xs.dedup_by(|a, b| near(a, b));
        ys.dedup_by(|a, b| near(a, b));
        (xs, ys)
    }
}

/// The most rows or columns Cut into Grid makes.
pub const GRID_CUT_MAX: u32 = 100;

/// A piece of `src` with area `area` (page space): its fill, outline and
/// opacity.
fn cut_piece(id: ShapeId, area: BezPath, src: &Shape) -> Shape {
    let mut s = Shape::new(
        id,
        ShapeKind::Path {
            path: area.clone(),
            closed: true,
        },
    );
    s.fill = src.fill.for_result(src.transform, &area);
    s.extra_fills = src.extra_fills.clone();
    s.stroke = src.stroke.clone();
    s.opacity = src.opacity;
    s.fill_rule = FillRule::EvenOdd;
    s
}

/// Whether `s` is an object a cut can split: a rectangle, ellipse,
/// polygon or curve that is closed or filled.
fn cuttable(s: &Shape) -> bool {
    match &s.kind {
        ShapeKind::Rect { .. } | ShapeKind::Ellipse { .. } | ShapeKind::Polygon { .. } => true,
        ShapeKind::Path { closed, .. } => *closed || s.fill != Fill::None,
        _ => false,
    }
}

/// The commands that put `made` (in that order, bottom first) where the
/// object `old` is, at `index` on `layer`, and take `old` out.
fn replace_with(
    cmds: &mut Vec<Command>,
    layer: LayerId,
    index: usize,
    old: ShapeId,
    made: Vec<Shape>,
) {
    for (k, s) in made.into_iter().enumerate() {
        let id = s.id;
        cmds.push(Command::AddShape { layer, shape: s });
        cmds.push(Command::Reorder {
            shape: id,
            layer,
            index: index + k,
        });
    }
    cmds.push(Command::DeleteShapes { shapes: vec![old] });
}

impl App {
    /// The one selected object that can cut the objects below it.
    fn cutter(&self) -> Option<Shape> {
        if self.selection.len() != 1 {
            return None;
        }
        let s = self.doc().shape(self.selection[0]).ok()?.1.clone();
        (!s.locked && cuttable(&s)).then_some(s)
    }

    /// Whether Cut Objects Below can run.
    pub fn cut_below_ready(&self) -> bool {
        self.cutter().is_some()
    }

    /// Cut Objects Below: every object under the selected one on the
    /// page (on unlocked, shown layers) that it overlaps is cut along its
    /// outline into the parts inside and outside it, each keeping the
    /// object's paint and place; the selected object goes. Groups, texts
    /// and pictures are left as they are.
    pub fn cut_objects_below(&mut self) {
        let Some(cutter) = self.cutter() else {
            return;
        };
        let doc = self.doc();
        let Ok(page) = doc.page(self.page) else {
            return;
        };
        let Ok((cut_layer, cut_index)) = doc.locate(cutter.id) else {
            return;
        };
        let Some(cut_rank) = page.layers.iter().position(|l| l.id == cut_layer) else {
            return;
        };
        let knife = crate::ops::shaping_area(&cutter);
        let reach = knife.bounding_box();
        // The objects to cut, from the top down, so a cut object's extra
        // pieces do not move the places of the objects still to cut.
        let mut work: Vec<(LayerId, usize, Shape, Vec<BezPath>)> = Vec::new();
        for layer in page.layers.iter().take(cut_rank + 1).rev() {
            if layer.locked || !layer.visible {
                continue;
            }
            let end = if layer.id == cut_layer {
                cut_index
            } else {
                layer.shapes.len()
            };
            for (i, s) in layer.shapes.iter().enumerate().take(end).rev() {
                if s.locked || !s.visible || !cuttable(s) {
                    continue;
                }
                if s.bounds().intersect(reach).area() <= 0.0 {
                    continue;
                }
                let area = crate::ops::shaping_area(s);
                let mut parts = Vec::new();
                for op in [Op::Trim, Op::Intersect] {
                    let side = shaping::overlay(&area, &knife, op);
                    for p in shaping::overlay_parts(&side, &BezPath::new(), Op::Weld) {
                        if shaping::filled_area(&p, FillRule::EvenOdd) > 1e-4 {
                            parts.push(p);
                        }
                    }
                }
                if parts.len() >= 2 {
                    work.push((layer.id, i, s.clone(), parts));
                }
            }
        }
        let mut cmds = Vec::new();
        let mut made_ids = Vec::new();
        for (layer, i, s, parts) in work {
            let made: Vec<Shape> = parts
                .into_iter()
                .map(|p| cut_piece(self.engine.new_shape_id(), p, &s))
                .collect();
            made_ids.extend(made.iter().map(|m| m.id));
            replace_with(&mut cmds, layer, i, s.id, made);
        }
        if cmds.is_empty() {
            self.status = crate::i18n::tr("status.no_pieces");
            return;
        }
        cmds.push(Command::DeleteShapes {
            shapes: vec![cutter.id],
        });
        if let Err(e) = self.engine.run_batch("Cut Objects Below", &cmds) {
            self.status = e.to_string();
            return;
        }
        self.select(made_ids);
    }

    /// The selected objects Cut into Grid works on: unlocked ones placed
    /// straight on a layer.
    fn grid_cut_targets(&self) -> Vec<(Shape, LayerId, usize)> {
        let doc = self.doc();
        self.selection
            .iter()
            .filter_map(|id| {
                let s = doc.shape(*id).ok()?.1;
                let (l, i) = doc.locate(*id).ok()?;
                (!s.locked).then(|| (s.clone(), l, i))
            })
            .collect()
    }

    /// Whether Cut into Grid can run.
    pub fn grid_cut_ready(&self) -> bool {
        !self.grid_cut_targets().is_empty()
    }

    /// The bounds of the first object Cut into Grid works on: the sizes
    /// its window shows are this object's.
    pub fn grid_cut_bounds(&self) -> Option<Rect> {
        self.grid_cut_targets().first().map(|t| t.0.bounds())
    }

    /// Cut into Grid with the settings in `self.grid_cut`: each selected
    /// object becomes `rows` by `cols` pieces from its top-left corner,
    /// in its paint and place, with the gaps between them. The pieces
    /// are boxes, or the parts of the object inside the boxes with Keep
    /// the object's shape; they are grouped with Group the pieces, and
    /// Add guidelines puts a guideline on each edge of the rows and
    /// columns. One undo step.
    pub fn cut_into_grid(&mut self) {
        let spec = self.grid_cut;
        let mut targets = self.grid_cut_targets();
        // From the top down on each layer, so places stay right.
        targets.sort_by_key(|t| std::cmp::Reverse(t.2));
        let mut cmds = Vec::new();
        let mut made_ids = Vec::new();
        let (mut gx, mut gy): (Vec<f64>, Vec<f64>) = (Vec::new(), Vec::new());
        for (s, layer, index) in targets {
            let b = s.bounds();
            let boxes = spec.cells(b);
            if boxes.is_empty() {
                continue;
            }
            let outline = (spec.keep_shape && cuttable(&s)).then(|| crate::ops::shaping_area(&s));
            let mut cells = Vec::new();
            for r in boxes {
                let rect = r.to_path(0.1);
                let area = match &outline {
                    Some(o) => {
                        let part = shaping::overlay(o, &rect, Op::Intersect);
                        if shaping::filled_area(&part, FillRule::EvenOdd) <= 1e-6 {
                            continue;
                        }
                        part
                    }
                    None => rect,
                };
                cells.push(cut_piece(self.engine.new_shape_id(), area, &s));
            }
            if cells.is_empty() {
                continue;
            }
            if spec.guides {
                let (xs, ys) = spec.edges(b);
                gx.extend(xs);
                gy.extend(ys);
            }
            let made = if spec.group && cells.len() > 1 {
                vec![Shape::new(
                    self.engine.new_shape_id(),
                    ShapeKind::Group { children: cells },
                )]
            } else {
                cells
            };
            made_ids.extend(made.iter().map(|m| m.id));
            replace_with(&mut cmds, layer, index, s.id, made);
        }
        if cmds.is_empty() {
            self.status = crate::i18n::tr("status.no_pieces");
            return;
        }
        // Guidelines on the edges, none twice and none where there is
        // one already; they go where guidelines dragged from the rulers
        // go (the page's own, or those every page shows).
        let page = self.page;
        let have = self.page_guides();
        let near = |a: f64, b: f64| (a - b).abs() < 1e-6;
        let mut seen_x: Vec<f64> = have
            .iter()
            .filter_map(|g| match g.line {
                GuideLine::Vertical { x } => Some(x),
                _ => None,
            })
            .collect();
        let mut seen_y: Vec<f64> = have
            .iter()
            .filter_map(|g| match g.line {
                GuideLine::Horizontal { y } => Some(y),
                _ => None,
            })
            .collect();
        let mut added: Vec<Guide> = Vec::new();
        for x in gx {
            if !seen_x.iter().any(|v| near(*v, x)) {
                seen_x.push(x);
                added.push(Guide::from(GuideLine::Vertical { x }));
            }
        }
        for y in gy {
            if !seen_y.iter().any(|v| near(*v, y)) {
                seen_y.push(y);
                added.push(Guide::from(GuideLine::Horizontal { y }));
            }
        }
        if !added.is_empty() {
            if self.edit_master_guides {
                let mut guides = have;
                guides.extend(added);
                cmds.push(Command::SetMasterGuides { guides });
            } else {
                cmds.extend(
                    added
                        .into_iter()
                        .map(|guide| Command::AddGuide { page, guide }),
                );
            }
        }
        if let Err(e) = self.engine.run_batch("Cut into Grid", &cmds) {
            self.status = e.to_string();
            return;
        }
        self.select(made_ids);
    }

    /// The strays on the current page's unlocked, shown layers: curves of
    /// one node or none, curves with no fill, outline or effect, and
    /// artistic texts with nothing but spaces in them; inside groups too.
    pub fn strays(&self) -> Vec<ShapeId> {
        fn walk(shapes: &[Shape], out: &mut Vec<ShapeId>) {
            for s in shapes {
                if s.locked {
                    continue;
                }
                match &s.kind {
                    ShapeKind::Group { children } => walk(children, out),
                    ShapeKind::Path { path, .. } => {
                        let lone = tracedraw_core::nodes::nodes(path).len() <= 1;
                        let unpainted = s.fill == Fill::None
                            && s.stroke.is_none()
                            && s.extra_fills.is_empty()
                            && s.effects.is_empty()
                            && s.name.is_none();
                        if lone || unpainted {
                            out.push(s.id);
                        }
                    }
                    ShapeKind::Text {
                        spans,
                        frame: None,
                        on_path: None,
                        ..
                    } if spans.iter().all(|t| t.text.trim().is_empty()) => out.push(s.id),
                    _ => {}
                }
            }
        }
        let mut out = Vec::new();
        if let Ok(page) = self.doc().page(self.page) {
            for layer in page.layers.iter().filter(|l| !l.locked && l.visible) {
                walk(&layer.shapes, &mut out);
            }
        }
        out
    }

    /// Remove Strays: the objects [`Self::strays`] finds go, in one undo
    /// step; the status bar says how many.
    pub fn remove_strays(&mut self) {
        let gone = self.strays();
        let n = gone.len().to_string();
        if !gone.is_empty() {
            let cmds = vec![Command::DeleteShapes {
                shapes: gone.clone(),
            }];
            if let Err(e) = self.engine.run_batch("Remove Strays", &cmds) {
                self.status = e.to_string();
                return;
            }
            self.selection.retain(|id| !gone.contains(id));
        }
        self.status = crate::i18n::trf("status.strays_removed", &[("n", &n)]);
    }
}

/// A small picture of rows (`across`: three bars lying one under the
/// other) or of columns (three bars standing side by side), in `rect`.
fn axis_picture(ui: &egui::Ui, rect: egui::Rect, across: bool) {
    let color = crate::theme::Tokens::icon();
    let gap = 2.0;
    for i in 0..3 {
        let f = i as f32;
        let bar = if across {
            let h = (rect.height() - 2.0 * gap) / 3.0;
            egui::Rect::from_min_size(
                egui::pos2(rect.left(), rect.top() + f * (h + gap)),
                egui::vec2(rect.width(), h),
            )
        } else {
            let w = (rect.width() - 2.0 * gap) / 3.0;
            egui::Rect::from_min_size(
                egui::pos2(rect.left() + f * (w + gap), rect.top()),
                egui::vec2(w, rect.height()),
            )
        };
        ui.painter().rect_filled(bar, 0.5, color);
    }
}

/// One direction's fields in the Cut into Grid window: the number of
/// cells, their size, the gap and the total, each with its tip. Changing
/// the number, the gap or the total shares the total out anew; changing
/// the size keeps the number and makes the total follow.
#[allow(clippy::too_many_arguments)]
fn grid_axis_fields(
    ui: &mut egui::Ui,
    units: crate::app::Units,
    title: &str,
    across: bool,
    size_key: &str,
    tips: [&str; 4],
    n: &mut u32,
    gap: &mut f64,
    total: &mut Option<f64>,
    extent: f64,
) {
    use crate::i18n::tr;
    use crate::ui::field::NumField;
    use crate::ui::tips::{pair, RichTip};
    let unit = format!(" {}", units.short());
    let max = units.world_from_mm(100_000.0);
    let whole = total.unwrap_or(extent);
    let cells = f64::from(*n);
    let size_mm = ((whole - *gap * (cells - 1.0)) / cells).max(GRID_CUT_MIN_CELL);
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
            axis_picture(ui, r.shrink(1.5), across);
            ui.label(egui::RichText::new(tr(title)).font(crate::theme::bold(12.5)));
        });
        egui::Grid::new(("grid_cut_axis", title))
            .num_columns(2)
            .spacing([8.0, 6.0])
            .show(ui, |ui| {
                ui.label(tr("grid_cut.number"));
                ui.add(NumField::new(n).range(1..=GRID_CUT_MAX))
                    .rich_tip(pair("grid_cut.number", tips[0]));
                ui.end_row();

                ui.label(tr(size_key));
                let mut v = units.world_from_mm(size_mm);
                let resp = ui
                    .add(
                        NumField::new(&mut v)
                            .range(units.world_from_mm(GRID_CUT_MIN_CELL)..=max)
                            .max_decimals(3)
                            .speed(0.1)
                            .suffix(unit.clone()),
                    )
                    .rich_tip(pair(size_key, tips[1]));
                if resp.changed() && v.is_finite() {
                    let size = units.world_to_mm(v).max(GRID_CUT_MIN_CELL);
                    *total = Some(size * cells + *gap * (cells - 1.0));
                }
                ui.end_row();

                ui.label(tr("grid_cut.gap"));
                let mut v = units.world_from_mm(*gap);
                let resp = ui
                    .add(
                        NumField::new(&mut v)
                            .range(0.0..=max)
                            .max_decimals(3)
                            .speed(0.1)
                            .suffix(unit.clone()),
                    )
                    .rich_tip(pair("grid_cut.gap", tips[2]));
                if resp.changed() && v.is_finite() {
                    *gap = units.world_to_mm(v).max(0.0);
                }
                ui.end_row();

                ui.label(tr("grid_cut.total"));
                let mut v = units.world_from_mm(total.unwrap_or(extent));
                let resp = ui
                    .add(
                        NumField::new(&mut v)
                            .range(units.world_from_mm(GRID_CUT_MIN_CELL)..=max)
                            .max_decimals(3)
                            .speed(0.1)
                            .suffix(unit.clone()),
                    )
                    .rich_tip(pair("grid_cut.total", tips[3]));
                if resp.changed() && v.is_finite() {
                    *total = Some(units.world_to_mm(v).max(GRID_CUT_MIN_CELL));
                }
                ui.end_row();
            });
    });
    // The gaps never take the whole total: the cells keep some room.
    let whole = total.unwrap_or(extent);
    let cells = f64::from((*n).clamp(1, GRID_CUT_MAX));
    if cells > 1.0 {
        let most = ((whole - GRID_CUT_MIN_CELL * cells) / (cells - 1.0)).max(0.0);
        *gap = gap.min(most);
    }
}

/// The Cut into Grid window: for the rows and for the columns their
/// number, size, gap and total; Keep the object's shape, Add guidelines,
/// Group the pieces and Preview; what the cut will make; Object's Size,
/// OK and Cancel. The sizes shown are those of the first selected
/// object.
pub fn grid_cut_window(app: &mut App, ctx: &egui::Context) {
    if !app.grid_cut.open {
        return;
    }
    use crate::i18n::{tr, trf};
    use crate::ui::tips::{pair, RichTip};
    let Some(bounds) = app.grid_cut_bounds() else {
        // The selection went while the window was open.
        app.grid_cut.open = false;
        return;
    };
    let units = app.units;
    let mut close = false;
    let mut done = false;
    let mut g = app.grid_cut;
    let before = crate::ui::chrome::take_close_request(ctx);
    crate::ui::chrome::window(tr("grid_cut.title"))
        .id_salt("grid_cut")
        .beside_drawing()
        .show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                grid_axis_fields(
                    ui,
                    units,
                    "grid_cut.rows",
                    true,
                    "grid_cut.height",
                    [
                        "grid_cut.rows_tip",
                        "grid_cut.height_tip",
                        "grid_cut.gap_rows_tip",
                        "grid_cut.total_h_tip",
                    ],
                    &mut g.rows,
                    &mut g.gap_y,
                    &mut g.total_h,
                    bounds.height(),
                );
                ui.add_space(14.0);
                grid_axis_fields(
                    ui,
                    units,
                    "grid_cut.cols",
                    false,
                    "grid_cut.width",
                    [
                        "grid_cut.cols_tip",
                        "grid_cut.width_tip",
                        "grid_cut.gap_cols_tip",
                        "grid_cut.total_w_tip",
                    ],
                    &mut g.cols,
                    &mut g.gap_x,
                    &mut g.total_w,
                    bounds.width(),
                );
            });
            ui.add_space(4.0);
            for (on, key) in [
                (&mut g.keep_shape, "grid_cut.keep_shape"),
                (&mut g.guides, "grid_cut.guides"),
                (&mut g.group, "grid_cut.group"),
                (&mut g.preview, "grid_cut.preview"),
            ] {
                ui.checkbox(on, tr(key))
                    .rich_tip(pair(key, &format!("{key}_tip")));
            }
            ui.add_space(2.0);
            // What OK makes of the first object.
            let fmt = |mm: f64| {
                let v = units.world_from_mm(mm);
                let t = format!("{v:.3}");
                let t = t.trim_end_matches('0').trim_end_matches('.').to_owned();
                format!("{t} {}", units.short())
            };
            match (g.columns(bounds.width()), g.row_axis(bounds.height())) {
                (Some(cx), Some(cy)) => {
                    ui.weak(trf(
                        "grid_cut.info",
                        &[
                            ("n", &(cx.n * cy.n).to_string()),
                            ("w", &fmt(cx.size)),
                            ("h", &fmt(cy.size)),
                        ],
                    ));
                }
                _ => {
                    ui.weak(tr("grid_cut.no_room"));
                }
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let own = g.total_w.is_none() && g.total_h.is_none();
                if ui
                    .add_enabled(!own, crate::ui::btn_icons::Ib::new("grid_cut.object_size"))
                    .rich_tip_always(pair("grid_cut.object_size", "grid_cut.object_size_tip"))
                    .clicked()
                {
                    g.total_w = None;
                    g.total_h = None;
                }
                done = crate::ui::dialogs::ok_cancel(ui, &mut close);
            });
        });
    if before || crate::ui::chrome::take_close_request(ctx) {
        close = true;
    }
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        close = true;
    }
    app.grid_cut = g;
    if done {
        app.grid_cut.open = false;
        app.cut_into_grid();
    } else if close {
        app.grid_cut.open = false;
    }
}

/// The most pieces the preview draws box by box; a finer grid shows the
/// rows' and columns' edges as lines right across the object.
const PREVIEW_BOXES: usize = 2500;

/// The lines (page mm) that show what Cut into Grid will make of an
/// object with bounds `b`: the outline of every piece, or for a very
/// fine grid the edges of its rows and columns.
pub fn grid_preview_lines(spec: &GridCut, b: Rect) -> Vec<(Point, Point)> {
    let cells = spec.cells(b);
    let mut out = Vec::new();
    if cells.len() <= PREVIEW_BOXES {
        for r in cells {
            let c = [
                Point::new(r.x0, r.y0),
                Point::new(r.x1, r.y0),
                Point::new(r.x1, r.y1),
                Point::new(r.x0, r.y1),
            ];
            for k in 0..4 {
                out.push((c[k], c[(k + 1) % 4]));
            }
        }
        return out;
    }
    let (xs, ys) = spec.edges(b);
    let (Some(x0), Some(x1)) = (xs.first().copied(), xs.last().copied()) else {
        return out;
    };
    let (Some(y0), Some(y1)) = (ys.first().copied(), ys.last().copied()) else {
        return out;
    };
    out.extend(xs.iter().map(|x| (Point::new(*x, y0), Point::new(*x, y1))));
    out.extend(ys.iter().map(|y| (Point::new(x0, *y), Point::new(x1, *y))));
    out
}

/// While the Cut into Grid window is open with Preview on, the pieces it
/// will make drawn over the selected objects: plain line segments, a
/// light line under a blue one so they show on any colour.
pub fn paint_grid_preview(app: &App, painter: &egui::Painter) {
    if !app.grid_cut.open || !app.grid_cut.preview {
        return;
    }
    let under = egui::Stroke::new(3.0, egui::Color32::from_white_alpha(170));
    let over = egui::Stroke::new(1.0, egui::Color32::from_rgb(0, 110, 230));
    for (s, _, _) in app.grid_cut_targets() {
        let lines = grid_preview_lines(&app.grid_cut, s.bounds());
        for stroke in [under, over] {
            for (a, b) in &lines {
                painter.line_segment([app.view.to_screen(*a), app.view.to_screen(*b)], stroke);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::Color;

    fn rect(app: &mut App, r: Rect) -> ShapeId {
        let id = app
            .new_shape(ShapeKind::Rect {
                rect: r,
                radius: 0.0,
                corners: None,
            })
            .expect("shape");
        app.engine
            .run(&Command::SetFill {
                shapes: vec![id],
                fill: Fill::Solid(Color::rgb8(200, 0, 0)),
            })
            .expect("fill");
        id
    }

    fn top_level(app: &App) -> Vec<Shape> {
        app.doc()
            .all_layers()
            .flat_map(|l| l.shapes.clone())
            .collect()
    }

    /// A square over the middle of a wide box cuts it into three parts
    /// (left, right and the middle under it) in the box's paint and
    /// place, and goes; a box it does not touch stays. One undo step
    /// brings it all back.
    #[test]
    fn a_square_cuts_the_objects_below_it() {
        let mut app = App::headless();
        let far = rect(&mut app, Rect::new(100.0, 0.0, 110.0, 10.0));
        let wide = rect(&mut app, Rect::new(0.0, 0.0, 30.0, 10.0));
        let knife = rect(&mut app, Rect::new(10.0, -5.0, 20.0, 15.0));
        app.select(vec![knife]);
        assert!(app.cut_below_ready());
        let depth = app.engine.history_labels().0.len();
        app.cut_objects_below();
        assert_eq!(app.engine.history_labels().0.len(), depth + 1);
        let shapes = top_level(&app);
        assert!(shapes.iter().all(|s| s.id != knife && s.id != wide));
        assert_eq!(shapes[0].id, far, "the untouched box stays at the bottom");
        assert_eq!(shapes.len(), 4);
        let mut areas: Vec<f64> = shapes[1..]
            .iter()
            .map(|s| shaping::filled_area(&s.page_path(), FillRule::EvenOdd).round())
            .collect();
        areas.sort_by(f64::total_cmp);
        assert_eq!(areas, vec![100.0, 100.0, 100.0]);
        assert!(shapes[1..]
            .iter()
            .all(|s| s.fill == Fill::Solid(Color::rgb8(200, 0, 0))));
        assert_eq!(app.selection.len(), 3);
        app.undo();
        assert_eq!(top_level(&app).len(), 3);
    }

    /// Cut into Grid makes rows by columns of boxes with the gap between
    /// them, filling the object's bounds, in its place.
    #[test]
    fn cut_into_grid_makes_boxes_with_gaps() {
        let mut app = App::headless();
        let under = rect(&mut app, Rect::new(50.0, 0.0, 60.0, 10.0));
        let id = rect(&mut app, Rect::new(0.0, 0.0, 22.0, 10.0));
        let over = rect(&mut app, Rect::new(70.0, 0.0, 80.0, 10.0));
        app.select(vec![id]);
        app.grid_cut = GridCut {
            rows: 1,
            cols: 3,
            gap_x: 2.0,
            ..GridCut::default()
        };
        app.cut_into_grid();
        let shapes = top_level(&app);
        assert_eq!(shapes.len(), 3);
        assert_eq!((shapes[0].id, shapes[2].id), (under, over));
        let ShapeKind::Group { children } = &shapes[1].kind else {
            panic!("a group of boxes");
        };
        assert_eq!(children.len(), 3);
        let b: Vec<f64> = children.iter().map(|c| c.bounds().x0.round()).collect();
        assert_eq!(b, vec![0.0, 8.0, 16.0]);
        assert!(children
            .iter()
            .all(|c| (c.bounds().width() - 6.0).abs() < 1e-6));
    }

    /// Rows and columns have their own gap, and a total smaller than the
    /// object lays the grid from the object's top-left corner: the rows
    /// go down from the top edge.
    #[test]
    fn grid_cells_follow_each_gap_and_the_totals() {
        let spec = GridCut {
            rows: 2,
            cols: 2,
            gap_x: 2.0,
            gap_y: 4.0,
            total_w: Some(12.0),
            total_h: Some(14.0),
            ..GridCut::default()
        };
        let cells = spec.cells(Rect::new(10.0, 0.0, 40.0, 20.0));
        assert_eq!(cells.len(), 4);
        // Columns 5 wide with 2 between, rows 5 high with 4 between.
        assert_eq!(cells[0], Rect::new(10.0, 15.0, 15.0, 20.0));
        assert_eq!(cells[1], Rect::new(17.0, 15.0, 22.0, 20.0));
        assert_eq!(cells[2], Rect::new(10.0, 6.0, 15.0, 11.0));
        let cols = spec.columns(30.0).expect("columns");
        assert!((cols.total() - 12.0).abs() < 1e-9);
        // Gaps wider than the total leave no room: nothing is made.
        let none = GridCut {
            cols: 3,
            gap_x: 20.0,
            ..GridCut::default()
        };
        assert!(none.cells(Rect::new(0.0, 0.0, 30.0, 10.0)).is_empty());
    }

    /// Keep the object's shape cuts the object itself: the four pieces
    /// of an ellipse cover the ellipse's area, not its box's. Without
    /// Group the pieces they stand on the layer on their own, where the
    /// object was.
    #[test]
    fn keep_shape_cuts_the_outline_and_ungrouped_pieces_take_its_place() {
        let mut app = App::headless();
        let under = rect(&mut app, Rect::new(50.0, 0.0, 60.0, 10.0));
        let id = app
            .new_shape(ShapeKind::Ellipse {
                rect: Rect::new(0.0, 0.0, 20.0, 20.0),
                arc: None,
            })
            .expect("ellipse");
        let over = rect(&mut app, Rect::new(70.0, 0.0, 80.0, 10.0));
        app.select(vec![id]);
        app.grid_cut = GridCut {
            keep_shape: true,
            group: false,
            ..GridCut::default()
        };
        let depth = app.engine.history_labels().0.len();
        app.cut_into_grid();
        assert_eq!(app.engine.history_labels().0.len(), depth + 1);
        let shapes = top_level(&app);
        assert_eq!(shapes.len(), 6);
        assert_eq!((shapes[0].id, shapes[5].id), (under, over));
        let area: f64 = shapes[1..5]
            .iter()
            .map(|s| shaping::filled_area(&s.page_path(), FillRule::EvenOdd))
            .sum();
        let disc = std::f64::consts::PI * 100.0;
        assert!((area - disc).abs() < disc * 0.01, "{area} vs {disc}");
        assert_eq!(app.selection.len(), 4);
    }

    /// Add guidelines puts one guideline on each edge of the rows and
    /// columns (the two sides of a gap each get one), none twice, in the
    /// same undo step as the cut.
    #[test]
    fn add_guidelines_marks_the_edges_of_rows_and_columns() {
        let mut app = App::headless();
        let id = rect(&mut app, Rect::new(0.0, 0.0, 22.0, 10.0));
        app.select(vec![id]);
        app.grid_cut = GridCut {
            rows: 2,
            cols: 2,
            gap_x: 2.0,
            guides: true,
            ..GridCut::default()
        };
        app.cut_into_grid();
        let guides = app.page_guides();
        let mut xs: Vec<f64> = guides
            .iter()
            .filter_map(|g| match g.line {
                GuideLine::Vertical { x } => Some(x),
                _ => None,
            })
            .collect();
        xs.sort_by(f64::total_cmp);
        assert_eq!(xs, vec![0.0, 10.0, 12.0, 22.0]);
        let mut ys: Vec<f64> = guides
            .iter()
            .filter_map(|g| match g.line {
                GuideLine::Horizontal { y } => Some(y),
                _ => None,
            })
            .collect();
        ys.sort_by(f64::total_cmp);
        assert_eq!(ys, vec![0.0, 5.0, 10.0]);
        app.undo();
        assert!(app.page_guides().is_empty());
        assert_eq!(top_level(&app).len(), 1);
    }

    /// The preview draws the outline of every piece, and for a very fine
    /// grid only the edges of the rows and columns.
    #[test]
    fn the_preview_draws_each_piece_or_the_edges_of_a_fine_grid() {
        let b = Rect::new(0.0, 0.0, 100.0, 100.0);
        let coarse = GridCut {
            rows: 2,
            cols: 3,
            ..GridCut::default()
        };
        assert_eq!(grid_preview_lines(&coarse, b).len(), 6 * 4);
        let fine = GridCut {
            rows: 100,
            cols: 100,
            ..GridCut::default()
        };
        // 101 lines each way instead of 40,000 box sides.
        assert_eq!(grid_preview_lines(&fine, b).len(), 202);
    }

    /// Remove Strays takes lone points, unpainted curves and empty texts,
    /// and leaves painted objects.
    #[test]
    fn remove_strays_takes_points_unpainted_curves_and_empty_text() {
        let mut app = App::headless();
        let keep = rect(&mut app, Rect::new(0.0, 0.0, 10.0, 10.0));
        let mut dot = BezPath::new();
        dot.move_to((5.0, 5.0));
        let point = app
            .new_shape(ShapeKind::Path {
                path: dot,
                closed: false,
            })
            .expect("point");
        let mut line = BezPath::new();
        line.move_to((0.0, 0.0));
        line.line_to((10.0, 10.0));
        let bare = app
            .new_shape(ShapeKind::Path {
                path: line,
                closed: false,
            })
            .expect("line");
        app.engine
            .run(&Command::SetStroke {
                shapes: vec![bare],
                stroke: None,
            })
            .expect("no outline");
        app.engine
            .run(&Command::SetFill {
                shapes: vec![bare],
                fill: Fill::None,
            })
            .expect("no fill");
        let found = app.strays();
        assert!(found.contains(&point) && found.contains(&bare), "{found:?}");
        assert!(!found.contains(&keep));
        app.remove_strays();
        let left: Vec<ShapeId> = top_level(&app).iter().map(|s| s.id).collect();
        assert_eq!(left, vec![keep]);
    }
}
