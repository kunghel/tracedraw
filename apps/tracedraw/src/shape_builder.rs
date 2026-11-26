//! The Shape Builder tool (#1149): with overlapping objects selected,
//! each part of the drawing they cut each other into (a place inside the
//! same objects and outside the others) lights up under the pointer, and
//! a drag across parts merges the parts it crossed into one new object,
//! cut out of the objects they came from. Alt+drag (Option on macOS)
//! takes the parts away instead. A click on a part makes it an object of
//! its own; a click with nothing selected selects. See
//! docs/behavior/shape-builder.md.

use crate::app::{App, Drag};
use crate::tools::Tool;
use egui::{Color32, Modifiers, Painter, PointerButton, Response};
use tracedraw_core::geometry::{Affine, BezPath, ParamCurve as _, Point, Shape as _};
use tracedraw_core::shaping::{onto_sources, overlay, overlay_parts, Op};
use tracedraw_core::{Command, Shape, ShapeId, ShapeKind};

/// The tool's state: the parts the drag has crossed so far, on the page,
/// and the part under the pointer, kept while the drawing and the
/// selection stay as they are.
#[derive(Default)]
pub struct Builder {
    pub parts: Vec<BezPath>,
    /// Alt was held during the drag: the parts go instead of merging.
    pub remove: bool,
    /// The property bar's Delete mode: a drag or a click takes the parts
    /// away, and Alt merges them (#3220).
    pub delete_mode: bool,
    /// The property bar's Keep Originals: a merge makes the new object
    /// and leaves the objects it came from as they were (#3220).
    pub keep_originals: bool,
    /// The steps the tool made since it was chosen or since the last
    /// Accept, for Cancel to undo (#3220).
    pub session_steps: usize,
    /// The property bar's Use Guidelines, off by default: the guidelines
    /// shown cut the objects into parts too, and stay as they are
    /// (#3558).
    pub use_guides: bool,
    /// The property bar's Use Grid, off by default: the document grid's
    /// lines cut the objects into parts as guidelines do, and its cells
    /// are parts of their own (#4390).
    pub use_grid: bool,
    /// Alt was held during the drag (not only the Delete mode): a drag
    /// over lines deletes their segments (#4079).
    pub alt: bool,
    /// While set, Segment Delete works on these objects alone: the
    /// Shape Builder's Alt deletes segments of the selection only.
    pub segment_scope: Option<Vec<ShapeId>>,
    /// The property bar's Remove Leftovers, off by default: Accept
    /// deletes what is left of the objects and lines the merges since the
    /// tool was chosen used, so only the shapes built stay (#4123).
    pub remove_leftovers: bool,
    /// The objects and lines the merges since the tool was chosen (or
    /// the last Accept) cut, and the objects they made (#4123).
    pub session_sources: Vec<ShapeId>,
    pub session_made: Vec<ShapeId>,
    hover: std::cell::RefCell<Option<Hover>>,
}

struct Hover {
    key: (u64, Vec<ShapeId>, bool, bool),
    part: BezPath,
}

/// One object the tool works on: its area on the page and the transform
/// from its own space to the page.
pub struct Piece {
    pub id: ShapeId,
    pub area: BezPath,
    pub to_page: Affine,
}

/// Whether `p` lies inside `path`, read with the even-odd rule.
fn inside(path: &BezPath, p: Point) -> bool {
    path.bounding_box().contains(p) && path.winding(p) % 2 != 0
}

/// `path` flattened once into straight lines, as the overlay flattens
/// it. The parts are worked out from these, so two parts that share an
/// edge share its points exactly and merge with no seam; the curves are
/// laid back onto the objects at the end. Parts cut from the curves
/// themselves each flattened their stretch of a shared arc a little
/// differently, and the merge kept a zigzag of short pieces along it
/// (#4123: dozens of nodes on a circle merged with a square).
pub fn polygon(path: &BezPath) -> BezPath {
    let mut out = BezPath::new();
    for c in tracedraw_core::shaping::contours(path) {
        let mut pts = c.into_iter().map(|[x, y]| Point::new(x, y));
        let Some(first) = pts.next() else {
            continue;
        };
        out.move_to(first);
        for q in pts {
            out.line_to(q);
        }
        out.close_path();
    }
    out
}

/// The part of the drawing the `areas` cut each other into that holds
/// `p`: inside the areas that hold `p`, outside the others, the one
/// piece of that place around `p`. None outside every area or on an edge.
pub fn part_at(areas: &[BezPath], p: Point) -> Option<BezPath> {
    let (ins, outs): (Vec<&BezPath>, Vec<&BezPath>) = areas.iter().partition(|a| inside(a, p));
    let (first, rest) = ins.split_first()?;
    let mut part = (*first).clone();
    for a in rest {
        part = overlay(&part, a, Op::Intersect);
    }
    for a in outs {
        let b = part.bounding_box();
        if b.intersect(a.bounding_box()).area() <= 0.0 {
            continue;
        }
        part = overlay(&part, a, Op::Trim);
    }
    let piece = overlay_parts(&part, &BezPath::new(), Op::Weld)
        .into_iter()
        .find(|piece| inside(piece, p))?;
    // Laid back onto the objects, so a curve cut by an object that does
    // not bound the part is one piece again (#3587).
    let sources: Vec<&BezPath> = areas.iter().collect();
    Some(onto_sources(&piece, &sources))
}

/// The width open curves are thickened to when they cut parts: thin
/// enough that a part reaches the curve, wide enough to cut.
const OPEN_CUT: f64 = 0.002;

/// The piece of `area` around `p` left once the open curves `open`,
/// thickened to thin areas, are cut out of it one by one.
fn cut_by_open(area: BezPath, open: &[BezPath], p: Point) -> Option<BezPath> {
    let style = kurbo::Stroke::new(OPEN_CUT);
    let mut piece = area;
    for o in open {
        let reach = o.bounding_box().inflate(OPEN_CUT, OPEN_CUT);
        if reach.intersect(piece.bounding_box()).area() <= 0.0 {
            continue;
        }
        let cut = kurbo::stroke(o.iter(), &style, &kurbo::StrokeOpts::default(), 1e-4);
        piece = overlay_parts(&piece, &cut, Op::Trim)
            .into_iter()
            .find(|q| inside(q, p))?;
    }
    Some(piece)
}

/// The room around `p` that the open curves `open` close in on every side
/// (a cell of a grid of lines, #3814); None when `p` is not closed in.
fn closed_in(open: &[BezPath], p: Point) -> Option<BezPath> {
    let b = open
        .iter()
        .map(|o| o.bounding_box())
        .reduce(|a, b| a.union(b))?
        .inflate(1.0, 1.0);
    let piece = cut_by_open(b.to_path(1e-3), open, p)?;
    let pb = piece.bounding_box();
    let touches = pb.x0 <= b.x0 + 1e-6
        || pb.y0 <= b.y0 + 1e-6
        || pb.x1 >= b.x1 - 1e-6
        || pb.y1 >= b.y1 - 1e-6;
    (!touches).then_some(piece)
}

/// The room around `p` that the guidelines `lines` close in on every
/// side, as far as `bounds` (which must hold every crossing of them);
/// None when `p` is on a line or its room is open on a side (#4123:
/// guidelines alone make parts).
pub fn guide_room(
    lines: &[tracedraw_core::document::GuideLine],
    bounds: tracedraw_core::geometry::Rect,
    p: Point,
) -> Option<BezPath> {
    let cell = cell_at(bounds, lines, p)?;
    let b = cell.bounding_box();
    let open = b.x0 <= bounds.x0 + 1e-6
        || b.y0 <= bounds.y0 + 1e-6
        || b.x1 >= bounds.x1 - 1e-6
        || b.y1 >= bounds.y1 - 1e-6;
    (!open).then_some(cell)
}

/// [`part_at`] with `lines` cutting the areas too: the part is kept to
/// the side of every line that `p` is on. None on a line. Selected open
/// curves `open` cut the parts as well, and the rooms they close in on
/// every side are parts of their own (#3814).
pub fn part_at_cut(
    areas: &[BezPath],
    lines: &[tracedraw_core::document::GuideLine],
    open: &[BezPath],
    p: Point,
) -> Option<BezPath> {
    let part = match part_at(areas, p) {
        Some(part) if !open.is_empty() => cut_by_open(part, open, p)?,
        Some(part) => part,
        None if !open.is_empty() => closed_in(open, p)?,
        None => return None,
    };
    if lines.is_empty() {
        return Some(part);
    }
    let cell = cell_at(part.bounding_box().inflate(10.0, 10.0), lines, p)?;
    part_in_cell(&part, &cell, p)
}

/// The piece of `part` inside `cell` that holds `p`.
pub(crate) fn part_in_cell(part: &BezPath, cell: &BezPath, p: Point) -> Option<BezPath> {
    overlay_parts(part, cell, Op::Intersect)
        .into_iter()
        .find(|piece| inside(piece, p))
}

/// The room around `p` inside `bounds` that `lines` leave it, as a
/// closed outline: `bounds` cut by each line, the side `p` is on kept.
/// None when `p` is on a line or nothing is left.
pub(crate) fn cell_at(
    bounds: tracedraw_core::geometry::Rect,
    lines: &[tracedraw_core::document::GuideLine],
    p: Point,
) -> Option<BezPath> {
    let mut poly = vec![
        Point::new(bounds.x0, bounds.y0),
        Point::new(bounds.x1, bounds.y0),
        Point::new(bounds.x1, bounds.y1),
        Point::new(bounds.x0, bounds.y1),
    ];
    for line in lines {
        let side = line.distance(p);
        if side.abs() < 1e-9 {
            return None;
        }
        // Signed so that the side of `p` is positive.
        let d = |q: Point| line.distance(q) * side.signum();
        let mut out = Vec::with_capacity(poly.len() + 1);
        for i in 0..poly.len() {
            let a = poly[i];
            let b = poly[(i + 1) % poly.len()];
            let (da, db) = (d(a), d(b));
            if da >= 0.0 {
                out.push(a);
            }
            if (da >= 0.0) != (db >= 0.0) {
                out.push(a.lerp(b, da / (da - db)));
            }
        }
        if out.len() < 3 {
            return None;
        }
        poly = out;
    }
    let mut path = BezPath::new();
    path.move_to(poly[0]);
    for q in &poly[1..] {
        path.line_to(*q);
    }
    path.close_path();
    Some(path)
}

/// The parts crossed merged into one area, its curves those of the
/// `areas` the parts were cut from: each part has nodes where the others
/// cut it, and the merge kept them where the cut was gone, a curve in
/// many short pieces (#3587).
pub fn merged(parts: &[BezPath], areas: &[BezPath]) -> BezPath {
    let mut it = parts.iter();
    let Some(first) = it.next() else {
        return BezPath::new();
    };
    let all = it.fold(overlay(first, &BezPath::new(), Op::Weld), |acc, p| {
        overlay(&acc, p, Op::Weld)
    });
    if all.elements().is_empty() || areas.is_empty() {
        return all;
    }
    let sources: Vec<&BezPath> = areas.iter().collect();
    onto_sources(&all, &sources)
}

/// Whether `s` is an open curve that cuts parts (#3814), not locked.
fn open_curve(s: &Shape) -> bool {
    !s.locked && matches!(s.kind, ShapeKind::Path { closed: false, .. })
}

/// Whether the tool can cut `s`: a closed curve, rectangle, ellipse or
/// polygon, not locked.
fn usable(s: &Shape) -> bool {
    !s.locked
        && match &s.kind {
            ShapeKind::Path { closed, .. } => *closed,
            ShapeKind::Rect { .. } | ShapeKind::Polygon { .. } => true,
            ShapeKind::Ellipse { arc, .. } => arc.is_none(),
            _ => false,
        }
}

impl App {
    /// The selected objects, a selected group standing for the objects in
    /// it (and in the groups inside it): the tool cuts and merges them
    /// where they are, in their group (#4223).
    fn builder_selected(&self) -> Vec<Shape> {
        fn walk(s: Shape, out: &mut Vec<Shape>) {
            match s.kind {
                ShapeKind::Group { children } if !s.locked => {
                    for c in children {
                        walk(c, out);
                    }
                }
                _ => out.push(s),
            }
        }
        let mut out = Vec::new();
        for s in self.selected_shapes() {
            walk(s, &mut out);
        }
        out
    }

    /// The selected objects the Shape Builder works on, bottom first.
    pub fn builder_pieces(&self) -> Vec<Piece> {
        let mut shapes: Vec<Shape> = self.builder_selected().into_iter().filter(usable).collect();
        shapes.sort_by_key(|s| self.z_key(s.id));
        shapes
            .iter()
            .map(|s| {
                let parent = self
                    .doc()
                    .parent_transform(s.id)
                    .unwrap_or(Affine::IDENTITY);
                Piece {
                    id: s.id,
                    area: parent * crate::ops::shaping_area(s),
                    to_page: parent * s.transform,
                }
            })
            .collect()
    }

    /// The selected open curves, on the page: they cut the objects into
    /// parts, and the rooms they close in are parts too (#3814: a grid
    /// of lines made no parts).
    pub fn builder_open_lines(&self) -> Vec<(ShapeId, BezPath)> {
        self.builder_selected()
            .into_iter()
            .filter(open_curve)
            .map(|s| {
                let parent = self
                    .doc()
                    .parent_transform(s.id)
                    .unwrap_or(Affine::IDENTITY);
                (s.id, parent * s.page_path())
            })
            .collect()
    }

    /// Whether the selection gives the tool anything to cut.
    fn builder_has_work(&self) -> bool {
        !self.builder_pieces().is_empty()
            || !self.builder_open_lines().is_empty()
            || !self.builder_lines().is_empty()
    }

    /// How far the rooms of guidelines alone are looked for: far round
    /// the page, so a room between guidelines off the page counts too.
    fn builder_guide_bounds(&self) -> tracedraw_core::geometry::Rect {
        let r = self.page_rect();
        let m = 10.0 * r.width().max(r.height()).max(1.0);
        r.inflate(m, m)
    }

    /// The guidelines that cut the objects with Use Guidelines on: those
    /// the page shows (#3558).
    pub fn builder_lines(&self) -> Vec<tracedraw_core::document::GuideLine> {
        let mut lines = Vec::new();
        if self.builder.use_guides {
            lines = self.shown_guide_lines();
        }
        if self.builder.use_grid {
            lines.extend(self.builder_grid_lines());
        }
        lines
    }

    /// The document grid's lines over the page and the selected objects,
    /// through the grid's origin at its spacing (#4390). None when the
    /// grid is so fine that more than 400 lines a side would cross them.
    pub fn builder_grid_lines(&self) -> Vec<tracedraw_core::document::GuideLine> {
        use tracedraw_core::document::GuideLine;
        let g = self.doc().metadata.grid;
        let o = self.grid_origin();
        let mut r = self.page_rect();
        for q in self.builder_pieces() {
            let b = q.area.bounding_box();
            if [b.x0, b.y0, b.x1, b.y1].iter().all(|v| v.is_finite()) {
                r = r.union(b);
            }
        }
        for (_, l) in self.builder_open_lines() {
            let b = l.bounding_box();
            if [b.x0, b.y0, b.x1, b.y1].iter().all(|v| v.is_finite()) {
                r = r.union(b);
            }
        }
        let steps = |lo: f64, hi: f64, step: f64, from: f64| -> Option<Vec<f64>> {
            if !(step.is_finite() && step > 1e-6) {
                return None;
            }
            let a = ((lo - from) / step).floor() as i64;
            let b = ((hi - from) / step).ceil() as i64;
            (b - a <= 400).then(|| (a..=b).map(|k| from + k as f64 * step).collect())
        };
        let (Some(xs), Some(ys)) = (
            steps(r.x0, r.x1, g.spacing_x, o.x),
            steps(r.y0, r.y1, g.spacing_y, o.y),
        ) else {
            return Vec::new();
        };
        xs.into_iter()
            .map(|x| GuideLine::Vertical { x })
            .chain(ys.into_iter().map(|y| GuideLine::Horizontal { y }))
            .collect()
    }

    /// The guidelines the page shows, hidden ones left out: what cuts
    /// the objects for the Shape Builder and Smart Fill with Use
    /// Guidelines on (#3558, #3863).
    pub fn shown_guide_lines(&self) -> Vec<tracedraw_core::document::GuideLine> {
        let mut lines = Vec::new();
        if self.edited_guides_shown() {
            lines.extend(self.page_guides());
        }
        if self.other_guides_shown() {
            lines.extend(self.other_guides_on_page());
        }
        lines
            .into_iter()
            .filter(|g| !g.hidden)
            .map(|g| g.line)
            .collect()
    }

    /// The part under `p`, worked out again only when `p` leaves the one
    /// found last or the drawing or the selection changed.
    pub fn builder_part_at(&self, p: Point) -> Option<BezPath> {
        let key = (
            self.engine.revision(),
            self.selection.clone(),
            self.builder.use_guides,
            self.builder.use_grid,
        );
        if let Some(h) = self.builder.hover.borrow().as_ref() {
            if h.key == key && inside(&h.part, p) {
                return Some(h.part.clone());
            }
        }
        let areas: Vec<BezPath> = self
            .builder_pieces()
            .into_iter()
            .map(|q| polygon(&q.area))
            .collect();
        let open: Vec<BezPath> = self.builder_open_lines().into_iter().map(|(_, o)| o).collect();
        let lines = self.builder_lines();
        // Nothing selected to cut, Use Guidelines on: the rooms the
        // guidelines close in are the parts (#4123).
        let part = if areas.is_empty() && open.is_empty() {
            guide_room(&lines, self.builder_guide_bounds(), p)?
        } else {
            match part_at_cut(&areas, &lines, &open, p) {
                Some(part) => part,
                // Off the selection, the rooms of the guidelines are
                // parts as well, at once, with no click to drop the
                // selection first (#4223).
                None => self.builder_free_room(&areas, &lines, p)?,
            }
        };
        *self.builder.hover.borrow_mut() = Some(Hover {
            key,
            part: part.clone(),
        });
        Some(part)
    }

    /// With Use Guidelines on, the room of the guidelines around `p` when
    /// it lies clear of every selected object `areas`.
    fn builder_free_room(
        &self,
        areas: &[BezPath],
        lines: &[tracedraw_core::document::GuideLine],
        p: Point,
    ) -> Option<BezPath> {
        if lines.is_empty() {
            return None;
        }
        let room = guide_room(lines, self.builder_guide_bounds(), p)?;
        let b = room.bounding_box();
        let clear = areas.iter().all(|a| {
            a.bounding_box().intersect(b).area() <= 0.0
                || overlay_parts(&room, a, Op::Intersect)
                    .iter()
                    .all(|q| tracedraw_core::shaping::filled_area(q, Default::default()) < 1e-6)
        });
        clear.then_some(room)
    }

    /// `p` is in a room of the guidelines off the selected objects: the
    /// selection is dropped so the room is made as one of guidelines
    /// alone (#4223).
    fn builder_off_selection(&self, p: Point) -> bool {
        if self.selection.is_empty() || self.builder_lines().is_empty() {
            return false;
        }
        let areas: Vec<BezPath> = self
            .builder_pieces()
            .into_iter()
            .map(|q| polygon(&q.area))
            .collect();
        let open: Vec<BezPath> = self.builder_open_lines().into_iter().map(|(_, o)| o).collect();
        if areas.is_empty() && open.is_empty() {
            return false;
        }
        let lines = self.builder_lines();
        part_at_cut(&areas, &lines, &open, p).is_none()
            && self.builder_free_room(&areas, &lines, p).is_some()
    }

    /// Add the part under `p` to the drag's parts, once.
    fn builder_take(&mut self, p: Point) {
        if self.builder.parts.iter().any(|q| inside(q, p)) {
            return;
        }
        if let Some(part) = self.builder_part_at(p) {
            self.builder.parts.push(part);
        }
    }

    /// An outline of the selection (a line or a closed object) passes
    /// within a few pixels of `p`.
    fn builder_outline_near(&self, p: Point) -> bool {
        use tracedraw_core::geometry::ParamCurveNearest as _;
        let tol = 5.0 / self.view.zoom.max(1e-6) as f64;
        let paths: Vec<BezPath> = self
            .builder_open_lines()
            .into_iter()
            .map(|(_, l)| l)
            .chain(self.builder_pieces().into_iter().map(|q| q.area))
            .collect();
        paths.iter().any(|path| {
            path.bounding_box().inflate(tol, tol).contains(p)
                && path
                    .segments()
                    .any(|seg| seg.nearest(p, 1e-6).distance_sq.sqrt() <= tol)
        })
    }

    /// Alt with the Shape Builder: `f` (a Segment Delete action) on the
    /// selected objects only, a step Cancel takes back (#4079).
    fn builder_segment_delete(&mut self, f: impl FnOnce(&mut App) -> bool) -> bool {
        self.builder.segment_scope = Some(self.selection.clone());
        let done = f(self);
        self.builder.segment_scope = None;
        if done {
            self.builder.session_steps += 1;
        }
        done
    }

    /// The Shape Builder's drag ended (`points`, the pointer's way on the
    /// page). With Alt and no closed object selected (lines alone), the
    /// segments of the lines the drag crossed go, as far as their
    /// crossings, as Segment Delete takes them (#4079); otherwise the
    /// parts crossed merge, or with Alt or Delete go.
    pub fn builder_drag_end(&mut self, points: &[Point]) {
        if self.builder.alt && self.builder_pieces().is_empty() {
            self.builder.parts.clear();
            let pts = points.to_vec();
            self.builder_segment_delete(|app| app.delete_virtual_segments_along(&pts));
            return;
        }
        let remove = self.builder.remove;
        self.builder_finish(remove);
    }

    pub fn shape_builder_input(&mut self, response: &Response, p: Point, mods: Modifiers) {
        let pieces = self.builder_has_work();
        // Alt and a click on an outline of the selection: its segment
        // between the nearest crossings goes, as with Segment Delete
        // (#4079).
        if response.clicked_by(PointerButton::Primary)
            && mods.alt
            && pieces
            && self.builder_outline_near(p)
        {
            self.builder_segment_delete(|app| app.delete_virtual_segment_at(p));
            return;
        }
        if response.clicked_by(PointerButton::Primary) {
            if !pieces || self.builder_part_at(p).is_none() {
                self.click_select(p, mods);
                if !self.builder_has_work() {
                    self.status = crate::i18n::tr("status.shape_builder_select");
                }
                return;
            }
            if self.builder_off_selection(p) {
                self.selection.clear();
            }
            self.builder.parts.clear();
            self.builder_take(p);
            let remove = mods.alt != self.builder.delete_mode;
            self.builder_finish(remove);
            return;
        }
        if response.drag_started_by(PointerButton::Primary) {
            if self.builder_off_selection(p) {
                self.selection.clear();
            }
            self.builder.parts.clear();
            self.builder.alt = mods.alt;
            self.builder.remove = mods.alt != self.builder.delete_mode;
            self.drag = Drag::Freehand { points: vec![p] };
            self.builder_take(p);
        }
        if response.dragged_by(PointerButton::Primary) {
            self.builder.alt = mods.alt;
            self.builder.remove = mods.alt != self.builder.delete_mode;
            let step = 1.0 / self.view.zoom.max(1e-6) as f64;
            let mut add = Vec::new();
            if let Drag::Freehand { points } = &mut self.drag {
                if let Some(last) = points.last().copied() {
                    // The parts between two pointer positions far apart
                    // are crossed too.
                    let d = (p - last).hypot();
                    if d > step {
                        let n = (d / (4.0 * step)).ceil().max(1.0) as usize;
                        for i in 1..=n {
                            add.push(last.lerp(p, i as f64 / n as f64));
                        }
                        points.push(p);
                    }
                }
            }
            for q in add {
                self.builder_take(q);
            }
        }
    }

    /// The drag ended: merge the parts crossed into a new object cut out
    /// of the objects they came from, or with `remove` (Alt) only cut
    /// them out. One undo step. Objects left with nothing are deleted.
    pub fn builder_finish(&mut self, remove: bool) {
        let parts = std::mem::take(&mut self.builder.parts);
        if parts.is_empty() {
            return;
        }
        let pieces = self.builder_pieces();
        let lines = self.builder_open_lines();
        let areas: Vec<BezPath> = pieces.iter().map(|q| q.area.clone()).collect();
        // The parts are straight-line pieces of the objects' flattenings
        // (see [`polygon`]): merged and cut out as such, and only then
        // laid back onto the objects' curves (#4123).
        let union = merged(&parts, &[]);
        let sources: Vec<&BezPath> = areas.iter().collect();
        let area = if sources.is_empty() {
            union.clone()
        } else {
            onto_sources(&union, &sources)
        };
        if area.elements().is_empty() {
            return;
        }
        // The new object looks like the front object over the first part.
        let probe = parts[0]
            .segments()
            .next()
            .map(|_| interior_point(&parts[0]))
            .unwrap_or_default();
        let look = pieces
            .iter()
            .rev()
            .find(|q| inside(&q.area, probe))
            .map(|q| q.id);
        let mut cmds = Vec::new();
        let mut gone = Vec::new();
        let mut kept = Vec::new();
        // What is left of the objects and lines a merge cut (#4123).
        let mut used = Vec::new();
        // Keep Originals: a merge leaves the objects as they were (#3220).
        let keep = self.builder.keep_originals && !remove;
        for q in &pieces {
            if keep {
                kept.push(q.id);
                continue;
            }
            if q.area.bounding_box().intersect(area.bounding_box()).area() <= 0.0 {
                kept.push(q.id);
                continue;
            }
            let rest = overlay(&polygon(&q.area), &union, Op::Trim);
            if tracedraw_core::shaping::filled_area(&rest, Default::default()) < 1e-6 {
                gone.push(q.id);
                continue;
            }
            let rest = onto_sources(&rest, &sources);
            if rest.elements().is_empty() {
                gone.push(q.id);
                continue;
            }
            if q.to_page.determinant().abs() < 1e-12 {
                kept.push(q.id);
                continue;
            }
            cmds.push(Command::SetShapeKind {
                shape: q.id,
                kind: ShapeKind::Path {
                    path: q.to_page.inverse() * rest,
                    closed: true,
                },
            });
            kept.push(q.id);
            if !remove {
                used.push(q.id);
            }
        }
        let mut made = None;
        if !remove {
            let Some(layer) = self.active_layer() else {
                return;
            };
            let id = self.engine.new_shape_id();
            let mut s = Shape::new(
                id,
                ShapeKind::Path {
                    path: area.clone(),
                    closed: true,
                },
            );
            if let Some(src) = look.and_then(|id| self.doc().find_shape(id).cloned()) {
                let t = pieces
                    .iter()
                    .find(|q| q.id == src.id)
                    .map_or(src.transform, |q| q.to_page);
                s.fill = src.fill.for_result(t, &area);
                s.stroke = src.stroke.clone();
                s.opacity = src.opacity;
            } else if let Some(src) = lines
                .first()
                .and_then(|(id, _)| self.doc().find_shape(*id).cloned())
            {
                // Made only of lines: drawn with the first line's outline.
                s.fill = tracedraw_core::Fill::None;
                s.stroke = src.stroke.clone();
            } else {
                // Made of guidelines alone: the default fill and outline
                // of a new curve (#4123).
                self.doc().defaults.apply_to(&mut s);
            }
            cmds.push(Command::AddShape { layer, shape: s });
            made = Some(id);
        }
        // The lines that divided the parts merged lose the stretches
        // inside the new object, which is one object with no line across
        // it; the rest of each line stays (#4079).
        let mut lines_left: Vec<ShapeId> = Vec::new();
        for (id, line) in &lines {
            let trimmed = if remove || keep {
                None
            } else {
                line_outside(line, &area)
            };
            let Some(trimmed) = trimmed else {
                lines_left.push(*id);
                continue;
            };
            let to_page = self
                .doc()
                .find_shape(*id)
                .map(|s| self.doc().parent_transform(*id).unwrap_or(Affine::IDENTITY) * s.transform);
            match to_page {
                _ if trimmed.elements().is_empty() => gone.push(*id),
                Some(t) if t.determinant().abs() > 1e-12 => {
                    cmds.push(Command::SetShapeKind {
                        shape: *id,
                        kind: ShapeKind::Path {
                            path: t.inverse() * trimmed,
                            closed: false,
                        },
                    });
                    lines_left.push(*id);
                    used.push(*id);
                }
                _ => lines_left.push(*id),
            }
        }
        if !gone.is_empty() {
            cmds.push(Command::DeleteShapes {
                shapes: gone.clone(),
            });
        }
        let label = if remove {
            "Shape Builder Delete"
        } else {
            "Shape Builder"
        };
        if let Err(e) = self.engine.run_batch(label, &cmds) {
            self.status = e.to_string();
            return;
        }
        self.builder.session_steps += 1;
        for id in used {
            if !self.builder.session_sources.contains(&id) {
                self.builder.session_sources.push(id);
            }
        }
        self.builder.session_made.extend(made);
        // The objects stay selected, with the new one, to go on building;
        // the lines too (what is left of them).
        kept.extend(lines_left);
        // An object of a selected group stays selected as that group.
        let sel = self.selection.clone();
        let mut chosen: Vec<ShapeId> = Vec::new();
        for id in kept {
            let id = if sel.contains(&id) {
                id
            } else {
                self.doc()
                    .ancestors(id)
                    .into_iter()
                    .find(|a| sel.contains(a))
                    .unwrap_or(id)
            };
            if !chosen.contains(&id) && self.doc().find_shape(id).is_some() {
                chosen.push(id);
            }
        }
        chosen.extend(made);
        self.select(chosen);
    }
}

impl App {
    /// The Shape Builder's Accept (#3220): the shapes built stay, and the
    /// Pick tool comes back.
    pub fn builder_accept(&mut self) {
        // Remove Leftovers: what is left of the objects and lines the
        // merges used goes, one undo step, and the shapes built stay
        // (#4123).
        if self.builder.remove_leftovers {
            let made = std::mem::take(&mut self.builder.session_made);
            let left: Vec<ShapeId> = std::mem::take(&mut self.builder.session_sources)
                .into_iter()
                .filter(|id| !made.contains(id) && self.doc().find_shape(*id).is_some())
                .collect();
            if !left.is_empty() {
                if let Err(e) = self
                    .engine
                    .run_batch("Shape Builder Leftovers", &[Command::DeleteShapes { shapes: left }])
                {
                    self.status = e.to_string();
                }
                let ids: Vec<ShapeId> = made
                    .into_iter()
                    .filter(|id| self.doc().find_shape(*id).is_some())
                    .collect();
                self.select(ids);
            }
        }
        self.builder_forget_session();
        self.builder.parts.clear();
        self.set_tool(Tool::Pick);
    }

    /// A new session of the tool: no steps to cancel, nothing used or
    /// made yet.
    pub fn builder_forget_session(&mut self) {
        self.builder.session_steps = 0;
        self.builder.session_sources.clear();
        self.builder.session_made.clear();
    }

    /// The Shape Builder's Cancel (#3220): every step the tool made since
    /// it was chosen or since the last Accept is undone, so the drawing
    /// is as it was; the tool stays. Steps of other tools in between stop
    /// it (they are not the tool's to undo).
    pub fn builder_cancel(&mut self) {
        self.builder.parts.clear();
        while self.builder.session_steps > 0 {
            let ours = self
                .engine
                .undo_label()
                .is_some_and(|l| l.starts_with("Shape Builder") || l == "Delete Segment");
            if !ours || self.engine.undo().is_err() {
                break;
            }
            self.builder.session_steps -= 1;
        }
        self.builder_forget_session();
        let ids: Vec<ShapeId> = self
            .selection
            .iter()
            .copied()
            .filter(|id| self.doc().find_shape(*id).is_some())
            .collect();
        self.select(ids);
    }
}

/// `line` (an open curve on the page) without the stretches that run
/// through the inside of `area`, both sides of them inside it: the
/// dividing lines of the cells merged into `area` (#4079). Stretches
/// along its edge or outside it stay, exactly as they were drawn. `None`
/// when nothing of the line lies inside; an empty path when all of it
/// does.
pub fn line_outside(line: &BezPath, area: &BezPath) -> Option<BezPath> {
    use tracedraw_core::geometry::{ParamCurveDeriv as _, PathSeg, Vec2};
    let ab = area.bounding_box();
    let eps = 0.01;
    let lb = line.bounding_box().inflate(eps, eps);
    if lb.x0 > ab.x1 || lb.x1 < ab.x0 || lb.y0 > ab.y1 || lb.y1 < ab.y0 {
        return None;
    }
    let deriv = |seg: &PathSeg, t: f64| -> Vec2 {
        match seg {
            PathSeg::Line(l) => l.p1 - l.p0,
            PathSeg::Quad(q) => q.deriv().eval(t).to_vec2(),
            PathSeg::Cubic(c) => c.deriv().eval(t).to_vec2(),
        }
    };
    // Inside on both sides of the line at `t`.
    let within = |seg: &PathSeg, t: f64| -> bool {
        let q = seg.eval(t);
        if !ab.inflate(eps, eps).contains(q) {
            return false;
        }
        let d = deriv(seg, t);
        let len = d.hypot();
        if len < 1e-12 {
            return false;
        }
        let n = Vec2::new(-d.y, d.x) * (eps / len);
        inside(area, q + n) && inside(area, q - n)
    };
    const N: usize = 48;
    let mut out = BezPath::new();
    let mut changed = false;
    let mut last: Option<Point> = None;
    for seg in line.segments() {
        let cls: Vec<bool> = (0..N)
            .map(|i| within(&seg, (i as f64 + 0.5) / N as f64))
            .collect();
        if !cls.contains(&true) {
            if last.is_none_or(|l| (l - seg.start()).hypot() > 1e-9) {
                out.move_to(seg.start());
            }
            push_seg(&mut out, &seg);
            last = Some(seg.end());
            continue;
        }
        changed = true;
        // The places the class changes, found more closely by halving.
        let mut cuts = vec![0.0];
        for i in 1..N {
            if cls[i] != cls[i - 1] {
                let (mut lo, mut hi) = ((i as f64 - 0.5) / N as f64, (i as f64 + 0.5) / N as f64);
                for _ in 0..30 {
                    let mid = 0.5 * (lo + hi);
                    if within(&seg, mid) == cls[i - 1] {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                cuts.push(0.5 * (lo + hi));
            }
        }
        cuts.push(1.0);
        let mut class = cls[0];
        for w in cuts.windows(2) {
            let (a, b) = (w[0], w[1]);
            if !class && b - a > 1e-9 {
                let piece = seg.subsegment(a..b);
                if last.is_none_or(|l| (l - piece.start()).hypot() > 1e-9) {
                    out.move_to(piece.start());
                }
                push_seg(&mut out, &piece);
                last = Some(piece.end());
            } else if class {
                last = None;
            }
            class = !class;
        }
    }
    changed.then_some(out)
}

fn push_seg(path: &mut BezPath, seg: &tracedraw_core::geometry::PathSeg) {
    use tracedraw_core::geometry::PathSeg;
    match *seg {
        PathSeg::Line(l) => path.line_to(l.p1),
        PathSeg::Quad(q) => path.quad_to(q.p1, q.p2),
        PathSeg::Cubic(c) => path.curve_to(c.p1, c.p2, c.p3),
    }
}

/// A point well inside `path` (a part): the middle of its box when that
/// is inside, else a point just inside one of its edges.
fn interior_point(path: &BezPath) -> Point {
    let b = path.bounding_box();
    let c = b.center();
    if inside(path, c) {
        return c;
    }
    let step = (b.width().min(b.height()) * 0.01).max(1e-4);
    for seg in path.segments() {
        let a = seg.eval(0.5);
        let d = seg.eval(0.51) - seg.eval(0.49);
        let n = tracedraw_core::geometry::Vec2::new(-d.y, d.x);
        let len = n.hypot();
        if len < 1e-12 {
            continue;
        }
        for s in [1.0, -1.0] {
            let q = a + n * (s * step / len);
            if inside(path, q) {
                return q;
            }
        }
    }
    c
}

/// With the Shape Builder, the parts the drag crossed and the one under
/// the pointer, hatched over the drawing.
pub fn draw(app: &App, painter: &Painter) {
    if app.tool != Tool::ShapeBuilder {
        return;
    }
    // Taking parts away (Alt, or the Delete mode) shows them in red, a
    // merge in the selection colour (#4223).
    let removing = if matches!(app.drag, Drag::Freehand { .. }) && !app.builder.parts.is_empty() {
        app.builder.remove
    } else {
        painter.ctx().input(|i| i.modifiers.alt) != app.builder.delete_mode
    };
    let color = if removing {
        REMOVE_RED
    } else {
        crate::theme::Tokens::SELECTION
    };
    // The objects the tool works on are edged in grey, apart from the
    // usual selection colour, so the ones taking part show (#3220).
    for q in app.builder_pieces() {
        for seg in q.area.segments() {
            let pts: Vec<egui::Pos2> = (0..=8)
                .map(|i| app.view.to_screen(seg.eval(i as f64 / 8.0)))
                .collect();
            painter.add(egui::Shape::line(
                pts.clone(),
                egui::Stroke::new(3.0, Color32::from_white_alpha(200)),
            ));
            painter.add(egui::Shape::line(pts, egui::Stroke::new(1.5, Color32::from_gray(96))));
        }
    }
    let mut shown: Vec<BezPath> = app.builder.parts.clone();
    if let Some(at) = painter.ctx().pointer_hover_pos() {
        if painter.clip_rect().contains(at) {
            if let Some(part) = app.builder_part_at(app.view.to_page(at)) {
                if !shown.iter().any(|q| q == &part) {
                    shown.push(part);
                }
            }
        }
    }
    for part in &shown {
        hatch(app, painter, part, color);
    }
    // The stretches of lines Alt takes away, in red: those a drag has
    // crossed, or the one an Alt click under the pointer would (#4223).
    let scope = Some(&app.selection);
    let doomed: Vec<BezPath> = match &app.drag {
        Drag::Freehand { points } if app.builder.alt && app.builder_pieces().is_empty() => {
            app.virtual_segments_along(points, scope)
        }
        Drag::None if painter.ctx().input(|i| i.modifiers.alt) && !app.selection.is_empty() => painter
            .ctx()
            .pointer_hover_pos()
            .filter(|at| painter.clip_rect().contains(*at))
            .map(|at| app.view.to_page(at))
            .filter(|p| app.builder_outline_near(*p))
            .and_then(|p| app.virtual_segment_at(p, scope))
            .into_iter()
            .collect(),
        _ => Vec::new(),
    };
    for path in &doomed {
        for seg in path.segments() {
            let pts: Vec<egui::Pos2> = (0..=8)
                .map(|i| app.view.to_screen(seg.eval(i as f64 / 8.0)))
                .collect();
            painter.add(egui::Shape::line(
                pts.clone(),
                egui::Stroke::new(4.0, Color32::from_white_alpha(200)),
            ));
            painter.add(egui::Shape::line(pts, egui::Stroke::new(2.5, REMOVE_RED)));
        }
    }
}

/// The colour of parts about to be taken away.
pub const REMOVE_RED: Color32 = Color32::from_rgb(0xE0, 0x24, 0x24);

/// `part` filled with thin diagonal lines in `color`, edged with it.
fn hatch(app: &App, painter: &Painter, part: &BezPath, color: Color32) {
    let b = part.bounding_box();
    let zoom = app.view.zoom.max(1e-6) as f64;
    let gap = 6.0 / zoom;
    let stroke = egui::Stroke::new(1.0, color.gamma_multiply(0.7));
    // Diagonals x - y = k across the box, each cut to the part.
    let mut k = (b.x0 - b.y1) - gap;
    let end = b.x1 - b.y0;
    let mut lines = 0;
    while k <= end && lines < 2000 {
        let a = Point::new(b.x0.max(b.y0 + k), b.y0.max(b.x0 - k));
        let z = Point::new(b.x1.min(b.y1 + k), b.y1.min(b.x1 - k));
        if (z - a).hypot() > 1e-9 {
            for (s, e) in clip_line(part, a, z) {
                painter.line_segment([app.view.to_screen(s), app.view.to_screen(e)], stroke);
            }
        }
        k += gap;
        lines += 1;
    }
    for seg in part.segments() {
        let pts: Vec<egui::Pos2> = (0..=8)
            .map(|i| app.view.to_screen(seg.eval(i as f64 / 8.0)))
            .collect();
        painter.add(egui::Shape::line(pts, egui::Stroke::new(1.5, color)));
    }
}

/// The pieces of the line from `a` to `z` inside `part`.
fn clip_line(part: &BezPath, a: Point, z: Point) -> Vec<(Point, Point)> {
    use tracedraw_core::geometry::Line;
    let line = Line::new(a, z);
    let mut ts: Vec<f64> = vec![0.0, 1.0];
    for seg in part.segments() {
        for hit in seg.intersect_line(line) {
            ts.push(hit.line_t);
        }
    }
    ts.sort_by(|x, y| x.total_cmp(y));
    ts.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
    ts.windows(2)
        .filter(|w| inside(part, line.eval((w[0] + w[1]) / 2.0)))
        .map(|w| (line.eval(w[0]), line.eval(w[1])))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::ParamCurveArclen as _;
    use tracedraw_core::geometry::Rect;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> BezPath {
        Rect::new(x0, y0, x1, y1).to_path(0.1)
    }

    fn area(p: &BezPath) -> f64 {
        tracedraw_core::shaping::filled_area(p, Default::default())
    }

    /// Two squares overlapping by half: three parts, each found under a
    /// point of it, none on the outside.
    #[test]
    fn overlapping_squares_cut_each_other_into_three_parts() {
        let areas = vec![rect(0.0, 0.0, 20.0, 20.0), rect(10.0, 0.0, 30.0, 20.0)];
        let left = part_at(&areas, Point::new(5.0, 10.0)).unwrap();
        let mid = part_at(&areas, Point::new(15.0, 10.0)).unwrap();
        let right = part_at(&areas, Point::new(25.0, 10.0)).unwrap();
        for p in [&left, &mid, &right] {
            assert!((area(p) - 200.0).abs() < 1e-6, "{}", area(p));
        }
        assert!(part_at(&areas, Point::new(40.0, 10.0)).is_none());
        let both = merged(&[left, mid], &areas);
        assert!((area(&both) - 400.0).abs() < 1e-6);
    }

    /// Three circles overlapping (#3587): two parts merged come out with
    /// one curve for each stretch of a circle, as the circles draw it,
    /// not split again where the third circle had cut the parts; the
    /// merge covers the same area as the two parts.
    #[test]
    fn merged_parts_keep_the_curves_of_their_objects() {
        use tracedraw_core::geometry::{Circle, PathEl, Shape as _};
        let circle = |x: f64, y: f64| Circle::new((x, y), 10.0).to_path(0.1);
        let areas = vec![circle(0.0, 0.0), circle(12.0, 0.0), circle(6.0, 9.0)];
        let a = part_at(&areas, Point::new(-5.0, -3.0)).unwrap();
        let b = part_at(&areas, Point::new(6.0, -3.0)).unwrap();
        let both = merged(&[a.clone(), b.clone()], &areas);
        let curves = both
            .elements()
            .iter()
            .filter(|e| matches!(e, PathEl::CurveTo(..)))
            .count();
        // The left circle's outside run (two of its quarters and a piece
        // up to its own node at the left of the top circle), the right
        // circle's run inside, the top circle's run back: six, not eight.
        assert_eq!(curves, 6, "{:?}", both.elements());
        assert!(
            (area(&both) - area(&a) - area(&b)).abs() < 1e-2,
            "{} {} {}",
            area(&both),
            area(&a),
            area(&b)
        );
        // Every node lies on one of the circles.
        for el in both.elements() {
            if let PathEl::MoveTo(p) | PathEl::CurveTo(_, _, p) = el {
                let on = [(0.0, 0.0), (12.0, 0.0), (6.0, 9.0)].iter().any(|c| {
                    ((p.to_vec2() - Point::new(c.0, c.1).to_vec2()).hypot() - 10.0).abs() < 1e-3
                });
                assert!(on, "{p:?}");
            }
        }
    }

    /// The same place inside the same objects in two pieces apart: only
    /// the piece under the point is the part (a cross of two bars makes
    /// four arms of one bar alone, two of each).
    #[test]
    fn a_part_is_the_one_piece_under_the_point() {
        let areas = vec![rect(0.0, 10.0, 30.0, 20.0), rect(10.0, 0.0, 20.0, 30.0)];
        let arm = part_at(&areas, Point::new(5.0, 15.0)).unwrap();
        assert!((area(&arm) - 100.0).abs() < 1e-6, "{}", area(&arm));
        assert!(!inside(&arm, Point::new(25.0, 15.0)));
    }

    fn square(app: &mut App, x0: f64, y0: f64, x1: f64, y1: f64) -> ShapeId {
        let id = app.engine.new_shape_id();
        let layer = app.active_layer().unwrap();
        let mut s = Shape::new(
            id,
            ShapeKind::Rect {
                rect: Rect::new(x0, y0, x1, y1),
                radius: 0.0,
                corners: None,
            },
        );
        s.fill = tracedraw_core::Fill::Solid(tracedraw_core::Color::BLACK);
        app.run(Command::AddShape { layer, shape: s });
        id
    }

    fn page_area(app: &App, id: ShapeId) -> f64 {
        let s = app.doc().find_shape(id).unwrap();
        area(&s.page_path())
    }

    /// A drag across the left part and the middle of two squares merges
    /// them into a new object; the left square is used up and goes, the
    /// right one keeps its own part; one undo brings both back.
    #[test]
    fn a_drag_merges_the_parts_it_crossed_into_a_new_object() {
        let mut app = App::headless();
        let a = square(&mut app, 0.0, 0.0, 20.0, 20.0);
        let b = square(&mut app, 10.0, 0.0, 30.0, 20.0);
        app.select(vec![a, b]);
        app.tool = Tool::ShapeBuilder;
        app.builder_take(Point::new(5.0, 10.0));
        app.builder_take(Point::new(15.0, 10.0));
        app.builder_take(Point::new(16.0, 11.0));
        assert_eq!(app.builder.parts.len(), 2, "each part once");
        app.builder_finish(false);
        assert!(app.doc().find_shape(a).is_none(), "used up");
        assert!((page_area(&app, b) - 200.0).abs() < 1e-6);
        let made = *app.selection.iter().find(|id| **id != b).unwrap();
        assert!((page_area(&app, made) - 400.0).abs() < 1e-6);
        assert!(matches!(
            app.doc().find_shape(made).unwrap().fill,
            tracedraw_core::Fill::Solid(_)
        ));
        app.engine.undo().unwrap();
        assert!(app.doc().find_shape(a).is_some());
        assert!(app.doc().find_shape(made).is_none());
    }

    /// #4223: a group selected works as the objects in it; the part left
    /// of a grouped square stays in the group, which stays selected.
    #[test]
    fn a_selected_group_is_cut_and_merged_as_its_objects() {
        let mut app = App::headless();
        let a = square(&mut app, 0.0, 0.0, 20.0, 20.0);
        let b = square(&mut app, 10.0, 0.0, 30.0, 20.0);
        app.select(vec![a, b]);
        app.group_selection();
        let g = app.selection[0];
        assert!(matches!(app.doc().find_shape(g).map(|s| &s.kind), Some(ShapeKind::Group { .. })));
        app.tool = Tool::ShapeBuilder;
        assert_eq!(app.builder_pieces().len(), 2);
        app.builder_take(Point::new(15.0, 10.0));
        app.builder_finish(false);
        assert!(app.selection.contains(&g), "{:?}", app.selection);
        assert!((page_area(&app, a) - 200.0).abs() < 1e-6);
        assert!((page_area(&app, b) - 200.0).abs() < 1e-6);
        let made = *app.selection.iter().find(|id| **id != g).unwrap();
        assert!((page_area(&app, made) - 200.0).abs() < 1e-6);
    }

    /// #3814: a grid of 6 straight lines each way (open curves) makes
    /// 25 parts, its cells; a drag across two cells makes one object of
    /// them; outside the grid there is no part; the lines stay and stay
    /// selected, the one between the two cells without its stretch
    /// across the new object (#4079). A line across a square cuts it in
    /// two.
    #[test]
    fn a_grid_of_open_lines_makes_its_cells_into_parts() {
        let mut app = App::headless();
        let layer = app.active_layer().unwrap();
        let mut ids = Vec::new();
        for i in 0..6 {
            let v = 10.0 * i as f64;
            for (a, b) in [((v, -2.0), (v, 52.0)), ((-2.0, v), (52.0, v))] {
                let mut path = BezPath::new();
                path.move_to(a);
                path.line_to(b);
                let id = app.engine.new_shape_id();
                let shape = Shape::new(id, ShapeKind::Path { path, closed: false });
                app.run(Command::AddShape { layer, shape });
                ids.push(id);
            }
        }
        app.select(ids.clone());
        app.tool = Tool::ShapeBuilder;
        let cell = app.builder_part_at(Point::new(15.0, 25.0)).expect("a cell");
        assert!((area(&cell) - 100.0).abs() < 0.1, "{}", area(&cell));
        assert!(app.builder_part_at(Point::new(55.0, 25.0)).is_none(), "outside");
        assert!(app.builder_part_at(Point::new(51.0, 25.0)).is_none(), "open beyond the grid");
        app.builder_take(Point::new(15.0, 25.0));
        app.builder_take(Point::new(25.0, 25.0));
        assert_eq!(app.builder.parts.len(), 2);
        app.builder_finish(false);
        let made = *app.selection.iter().find(|id| !ids.contains(id)).expect("made");
        assert!((page_area(&app, made) - 200.0).abs() < 0.2, "{}", page_area(&app, made));
        assert!(ids.iter().all(|id| app.doc().find_shape(*id).is_some()));
        assert!(ids.iter().all(|id| app.selection.contains(id)));
        // The line x = 20 (the fifth added) lost its stretch from y = 20
        // to 30, inside the new object; the others are whole.
        let len = |app: &App, id: ShapeId| -> f64 {
            app.doc()
                .find_shape(id)
                .expect("line")
                .page_path()
                .segments()
                .map(|s| s.arclen(1e-6))
                .sum()
        };
        assert!((len(&app, ids[4]) - 44.0).abs() < 0.01, "{}", len(&app, ids[4]));
        assert!((len(&app, ids[0]) - 54.0).abs() < 0.01);
        assert!((len(&app, ids[5]) - 54.0).abs() < 0.01, "{}", len(&app, ids[5]));
        // A line across a square cuts it in two.
        let sq = square(&mut app, 100.0, 0.0, 120.0, 20.0);
        let mut path = BezPath::new();
        path.move_to((95.0, 10.0));
        path.line_to((125.0, 10.0));
        let id = app.engine.new_shape_id();
        let shape = Shape::new(id, ShapeKind::Path { path, closed: false });
        app.run(Command::AddShape { layer, shape });
        app.select(vec![sq, id]);
        let half = app.builder_part_at(Point::new(110.0, 5.0)).expect("half");
        assert!((area(&half) - 200.0).abs() < 0.1, "{}", area(&half));
    }

    /// #4079: with lines alone selected, Alt and a click on a line takes
    /// its segment between the two nearest crossings away; an Alt drag
    /// takes away every segment it crosses; lines not selected are left.
    #[test]
    fn alt_deletes_the_segments_of_lines() {
        let mut app = App::headless();
        let layer = app.active_layer().unwrap();
        let line = |app: &mut App, a: (f64, f64), b: (f64, f64)| {
            let mut path = BezPath::new();
            path.move_to(a);
            path.line_to(b);
            let id = app.engine.new_shape_id();
            let shape = Shape::new(id, ShapeKind::Path { path, closed: false });
            app.run(Command::AddShape { layer, shape });
            id
        };
        let v = line(&mut app, (10.0, 0.0), (10.0, 40.0));
        let h1 = line(&mut app, (0.0, 10.0), (40.0, 10.0));
        let h2 = line(&mut app, (0.0, 30.0), (40.0, 30.0));
        let other = line(&mut app, (20.0, 0.0), (20.0, 40.0));
        app.select(vec![v, h1, h2]);
        app.tool = Tool::ShapeBuilder;
        let len = |app: &App, id: ShapeId| -> f64 {
            app.doc().find_shape(id).map_or(0.0, |s| {
                s.page_path().segments().map(|s| s.arclen(1e-6)).sum()
            })
        };
        app.builder.alt = true;
        assert!(app.builder_outline_near(Point::new(10.0, 20.0)));
        app.builder_segment_delete(|app| app.delete_virtual_segment_at(Point::new(10.0, 20.0)));
        assert!((len(&app, v) - 20.0).abs() < 0.01, "{}", len(&app, v));
        // The line at x = 20 is not selected: it does not cut h1.
        assert!((len(&app, h1) - 40.0).abs() < 0.01);
        app.builder_drag_end(&[Point::new(30.0, 5.0), Point::new(30.0, 15.0)]);
        assert!((len(&app, h1) - 10.0).abs() < 0.01, "{}", len(&app, h1));
        assert!((len(&app, other) - 40.0).abs() < 0.01, "not selected, left");
        assert_eq!(app.builder.session_steps, 2);
        app.builder_cancel();
        assert!((len(&app, v) - 40.0).abs() < 0.01);
        assert!((len(&app, h1) - 40.0).abs() < 0.01);
    }

    /// Alt takes the parts away: the overlap of two squares cut out of
    /// both, nothing new made.
    #[test]
    fn alt_takes_the_parts_away() {
        let mut app = App::headless();
        let a = square(&mut app, 0.0, 0.0, 20.0, 20.0);
        let b = square(&mut app, 10.0, 0.0, 30.0, 20.0);
        app.select(vec![a, b]);
        app.builder_take(Point::new(15.0, 10.0));
        let before = app.doc().pages[0]
            .layers
            .iter()
            .map(|l| l.shapes.len())
            .sum::<usize>();
        app.builder_finish(true);
        let after = app.doc().pages[0]
            .layers
            .iter()
            .map(|l| l.shapes.len())
            .sum::<usize>();
        assert_eq!(before, after);
        assert!((page_area(&app, a) - 200.0).abs() < 1e-6);
        assert!((page_area(&app, b) - 200.0).abs() < 1e-6);
    }

    /// The bar's options (#3220): Keep Originals leaves the squares as
    /// they were while the merge makes the new object; Cancel undoes every
    /// step since the tool was chosen; Accept keeps them and goes back to
    /// the Pick tool.
    #[test]
    fn keep_originals_cancel_and_accept() {
        let mut app = App::headless();
        let a = square(&mut app, 0.0, 0.0, 20.0, 20.0);
        let b = square(&mut app, 10.0, 0.0, 30.0, 20.0);
        app.select(vec![a, b]);
        app.set_tool(Tool::ShapeBuilder);
        let count = |app: &App| {
            app.doc().pages[0]
                .layers
                .iter()
                .map(|l| l.shapes.len())
                .sum::<usize>()
        };
        let before = count(&app);
        app.builder.keep_originals = true;
        app.builder_take(Point::new(15.0, 10.0));
        app.builder_finish(false);
        assert_eq!(count(&app), before + 1, "the new object, the squares kept");
        assert!((page_area(&app, a) - 400.0).abs() < 1e-6);
        assert!((page_area(&app, b) - 400.0).abs() < 1e-6);
        app.builder.keep_originals = false;
        app.builder_take(Point::new(5.0, 10.0));
        app.builder_finish(true);
        assert_eq!(app.builder.session_steps, 2);
        app.builder_cancel();
        assert_eq!(count(&app), before, "both steps undone");
        assert!((page_area(&app, a) - 400.0).abs() < 1e-6);
        assert_eq!(app.tool, Tool::ShapeBuilder, "the tool stays");
        assert!(app.selection.iter().all(|id| app.doc().find_shape(*id).is_some()));
        // Accept keeps what was built.
        app.select(vec![a, b]);
        app.builder_take(Point::new(15.0, 10.0));
        app.builder_finish(false);
        app.builder_accept();
        assert_eq!(app.tool, Tool::Pick);
        assert_eq!(app.builder.session_steps, 0);
        assert!(app.doc().find_shape(a).is_some());
        // Steps of another tool in between stop Cancel.
        app.set_tool(Tool::ShapeBuilder);
        app.select(vec![a, b]);
        app.builder_take(Point::new(25.0, 10.0));
        app.builder_finish(true);
        let c = square(&mut app, 50.0, 50.0, 60.0, 60.0);
        app.builder_cancel();
        assert!(app.doc().find_shape(c).is_some(), "not the tool's to undo");
    }

    /// Lines cut the parts too: a square crossed by a vertical and a
    /// sloping line, each part on one side of both (#3558).
    #[test]
    fn lines_cut_a_square_into_parts() {
        use tracedraw_core::document::GuideLine;
        let areas = vec![rect(0.0, 0.0, 20.0, 20.0)];
        let lines = [
            GuideLine::Vertical { x: 5.0 },
            GuideLine::Angled {
                x: 0.0,
                y: 0.0,
                angle: 45.0,
            },
        ];
        let left = part_at_cut(&areas, &lines[..1], &[], Point::new(2.0, 10.0)).unwrap();
        assert!((area(&left) - 100.0).abs() < 1e-6, "{}", area(&left));
        let below = part_at_cut(&areas, &lines[1..], &[], Point::new(15.0, 5.0)).unwrap();
        assert!((area(&below) - 200.0).abs() < 1e-6, "{}", area(&below));
        let corner = part_at_cut(&areas, &lines, &[], Point::new(15.0, 5.0)).unwrap();
        assert!((area(&corner) - 187.5).abs() < 1e-6, "{}", area(&corner));
        assert!(part_at_cut(&areas, &lines, &[], Point::new(5.0, 10.0)).is_none(), "on a line");
    }

    /// #4390: with Use Grid on, the grid's lines cut the selected object
    /// into cells, and with nothing selected a grid cell is a part.
    #[test]
    fn use_grid_cuts_by_the_grid_lines() {
        let mut app = App::headless();
        let a = square(&mut app, 0.0, 0.0, 20.0, 20.0);
        app.select(vec![a]);
        app.set_tool(Tool::ShapeBuilder);
        let g = app.doc().metadata.grid;
        let o = app.grid_origin();
        let p = Point::new(5.0, 5.0);
        let whole = app.builder_part_at(p).expect("the square");
        assert!((area(&whole) - 400.0).abs() < 1e-6, "{}", area(&whole));
        app.builder.use_grid = true;
        assert!(!app.builder_grid_lines().is_empty());
        let cell = app.builder_part_at(p).expect("a cell of the square");
        let b = cell.bounding_box();
        assert!(b.width() <= g.spacing_x + 1e-6 && b.height() <= g.spacing_y + 1e-6, "{b:?}");
        // A whole grid cell off the selection, on the page.
        app.selection.clear();
        let q = Point::new(o.x + 5.5 * g.spacing_x, o.y - 5.5 * g.spacing_y);
        let room = app.builder_part_at(q).expect("a grid cell");
        assert!((area(&room) - g.spacing_x * g.spacing_y).abs() < 1e-6, "{}", area(&room));
    }

    /// With Use Guidelines on and an object still selected, a room of
    /// the guidelines clear of it is a part at once: no click to drop the
    /// selection first; taking it makes an object of the room (#4223).
    #[test]
    fn guideline_rooms_off_the_selection_are_ready_at_once() {
        use tracedraw_core::document::GuideLine;
        let mut app = App::headless();
        let a = square(&mut app, 0.0, 0.0, 10.0, 10.0);
        let page = app.page;
        for line in [
            GuideLine::Vertical { x: 20.0 },
            GuideLine::Vertical { x: 30.0 },
            GuideLine::Horizontal { y: 20.0 },
            GuideLine::Horizontal { y: 30.0 },
        ] {
            app.run(Command::AddGuide { page, guide: line.into() });
        }
        app.select(vec![a]);
        app.set_tool(Tool::ShapeBuilder);
        let p = Point::new(25.0, 25.0);
        assert!(app.builder_part_at(p).is_none(), "Use Guidelines off");
        app.builder.use_guides = true;
        let room = app.builder_part_at(p).expect("the room is ready");
        assert!((area(&room) - 100.0).abs() < 1e-6, "{}", area(&room));
        assert!(app.builder_off_selection(p));
        assert!(!app.builder_off_selection(Point::new(5.0, 5.0)), "on the object");
        app.selection.clear();
        app.builder_take(p);
        app.builder_finish(false);
        let made = app.selection[0];
        assert_ne!(made, a);
        assert!((page_area(&app, made) - 100.0).abs() < 1e-6);
        assert!((page_area(&app, a) - 100.0).abs() < 1e-6, "the object is untouched");
    }

    /// An Alt drag over lines shows, before it ends, the stretches it
    /// takes away: the same ones that go when it ends (#4223).
    #[test]
    fn alt_drag_shows_the_stretches_it_takes_away() {
        let mut app = App::headless();
        let layer = app.active_layer().unwrap();
        let mut ids = Vec::new();
        for (a, b) in [((0.0, 10.0), (30.0, 10.0)), ((10.0, 0.0), (10.0, 30.0)), ((20.0, 0.0), (20.0, 30.0))] {
            let mut path = BezPath::new();
            path.move_to(a);
            path.line_to(b);
            let id = app.engine.new_shape_id();
            app.run(Command::AddShape {
                layer,
                shape: Shape::new(id, ShapeKind::Path { path, closed: false }),
            });
            ids.push(id);
        }
        app.select(ids.clone());
        app.tool = Tool::ShapeBuilder;
        // Across the horizontal line between the two upright ones.
        let stroke = [Point::new(15.0, 5.0), Point::new(15.0, 15.0)];
        let shown = app.virtual_segments_along(&stroke, Some(&app.selection.clone()));
        assert_eq!(shown.len(), 1, "{shown:?}");
        let b = shown[0].bounding_box();
        assert!((b.x0 - 10.0).abs() < 1e-6 && (b.x1 - 20.0).abs() < 1e-6, "{b:?}");
        // The segment an Alt click would take.
        let one = app.virtual_segment_at(Point::new(25.0, 10.0), Some(&app.selection.clone())).expect("a segment");
        let b = one.bounding_box();
        assert!((b.x0 - 20.0).abs() < 1e-6 && (b.x1 - 30.0).abs() < 1e-6, "{b:?}");
        app.builder.alt = true;
        app.builder_drag_end(&stroke);
        let h = app.doc().find_shape(ids[0]).expect("the line stays in two");
        let hb = h.page_path().bounding_box();
        assert!((hb.x0 - 0.0).abs() < 1e-6 && (hb.x1 - 30.0).abs() < 1e-6);
        assert!(
            app.doc().find_shape(ids[0]).map(|s| s.page_path().segments().count()).unwrap_or(0) == 2,
            "the middle stretch went"
        );
    }

    /// With Use Guidelines on, the page's guidelines cut the objects: a
    /// click splits a square at a guideline, which stays; off, the square
    /// is one part (#3558).
    #[test]
    fn use_guidelines_cuts_the_objects_at_the_guidelines() {
        use tracedraw_core::document::GuideLine;
        let mut app = App::headless();
        let a = square(&mut app, 0.0, 0.0, 20.0, 20.0);
        let page = app.page;
        app.run(Command::AddGuide {
            page,
            guide: GuideLine::Vertical { x: 5.0 }.into(),
        });
        let guides = app.page_guides();
        app.select(vec![a]);
        app.set_tool(Tool::ShapeBuilder);
        let off = app.builder_part_at(Point::new(2.0, 10.0)).unwrap();
        assert!((area(&off) - 400.0).abs() < 1e-6);
        app.builder.use_guides = true;
        let on = app.builder_part_at(Point::new(2.0, 10.0)).unwrap();
        assert!((area(&on) - 100.0).abs() < 1e-6, "{}", area(&on));
        app.builder_take(Point::new(2.0, 10.0));
        app.builder_finish(false);
        assert!((page_area(&app, a) - 300.0).abs() < 1e-6, "{}", page_area(&app, a));
        assert_eq!(app.page_guides(), guides, "the guideline stays");
    }

    /// Smart Fill with Use Guidelines on: a guideline closes the area
    /// off, so a click left of it fills only that part of a square; off,
    /// the whole square (#3863).
    #[test]
    fn smart_fill_use_guidelines_stops_the_area_at_a_guideline() {
        use tracedraw_core::document::GuideLine;
        let mut app = App::headless();
        let a = square(&mut app, 0.0, 0.0, 20.0, 20.0);
        let page = app.page;
        app.run(Command::AddGuide {
            page,
            guide: GuideLine::Vertical { x: 5.0 }.into(),
        });
        app.area_fill_at(Point::new(2.0, 10.0));
        let whole = app.selection[0];
        assert_ne!(whole, a);
        assert!((page_area(&app, whole) - 400.0).abs() < 1e-3, "{}", page_area(&app, whole));
        app.area_fill_guides = true;
        app.area_fill_at(Point::new(2.0, 10.0));
        let part = app.selection[0];
        assert_ne!(part, whole);
        assert!((page_area(&app, part) - 100.0).abs() < 1e-3, "{}", page_area(&app, part));
    }

    fn circle(app: &mut App, r: Rect) -> ShapeId {
        let id = app.engine.new_shape_id();
        let layer = app.active_layer().unwrap();
        let mut s = Shape::new(id, ShapeKind::Ellipse { rect: r, arc: None });
        s.fill = tracedraw_core::Fill::Solid(tracedraw_core::Color::BLACK);
        app.run(Command::AddShape { layer, shape: s });
        id
    }

    fn page_elements(app: &App, id: ShapeId) -> usize {
        app.doc().find_shape(id).unwrap().page_path().elements().len()
    }

    /// #4123: a square and a circle over it, merged whole, make one
    /// object with a node at each crossing and the circle's arcs whole,
    /// not dozens of short pieces along the arc the parts shared; the
    /// circle's part outside the square, merged alone, leaves the rest
    /// of the circle clean too.
    #[test]
    fn a_square_merged_with_a_circle_keeps_its_arcs_whole() {
        let mut app = App::headless();
        let a = square(&mut app, 0.0, 0.0, 20.0, 20.0);
        let b = circle(&mut app, Rect::new(10.0, 0.0, 30.0, 20.0));
        app.select(vec![a, b]);
        app.set_tool(Tool::ShapeBuilder);
        for x in [3.0, 15.0, 28.0] {
            app.builder_take(Point::new(x, 10.0));
        }
        assert_eq!(app.builder.parts.len(), 3);
        app.builder_finish(false);
        assert!(app.doc().find_shape(a).is_none() && app.doc().find_shape(b).is_none());
        let made = app.selection[0];
        assert!(page_elements(&app, made) <= 8, "{}", page_elements(&app, made));
        // The square and the half of the circle outside it.
        let whole = 400.0 + std::f64::consts::PI * 50.0;
        assert!((page_area(&app, made) - whole).abs() < 0.1, "{}", page_area(&app, made));

        let mut app = App::headless();
        let a = square(&mut app, 0.0, 0.0, 20.0, 20.0);
        let b = circle(&mut app, Rect::new(10.0, 0.0, 30.0, 20.0));
        let disc = page_area(&app, b);
        app.select(vec![a, b]);
        app.set_tool(Tool::ShapeBuilder);
        app.builder_take(Point::new(28.0, 10.0));
        app.builder_finish(false);
        assert!(page_elements(&app, b) <= 6, "{}", page_elements(&app, b));
        assert!(page_elements(&app, a) <= 6, "{}", page_elements(&app, a));
        let made = *app.selection.iter().find(|id| **id != a && **id != b).unwrap();
        assert!(page_elements(&app, made) <= 6, "{}", page_elements(&app, made));
        let parts = page_area(&app, b) + page_area(&app, made);
        assert!((parts - disc).abs() < 1e-3, "{parts} {disc}");
    }

    /// #4123: with Remove Leftovers on, Accept deletes what is left of
    /// the objects the merges used, one undo step, and the shapes built
    /// stay selected; an object no merge touched stays; off, the leftovers
    /// stay as before.
    #[test]
    fn remove_leftovers_deletes_what_the_merges_left_on_accept() {
        for on in [false, true] {
            let mut app = App::headless();
            let a = square(&mut app, 0.0, 0.0, 20.0, 20.0);
            let b = square(&mut app, 10.0, 0.0, 30.0, 20.0);
            let far = square(&mut app, 50.0, 0.0, 60.0, 10.0);
            app.select(vec![a, b, far]);
            app.set_tool(Tool::ShapeBuilder);
            app.builder.remove_leftovers = on;
            app.builder_take(Point::new(5.0, 10.0));
            app.builder_take(Point::new(15.0, 10.0));
            app.builder_finish(false);
            let made = *app
                .selection
                .iter()
                .find(|id| ![a, b, far].contains(id))
                .unwrap();
            assert!(app.doc().find_shape(b).is_some(), "the rest of b, before Accept");
            app.builder_accept();
            assert_eq!(app.tool, Tool::Pick);
            assert!(app.doc().find_shape(made).is_some());
            assert!(app.doc().find_shape(far).is_some(), "untouched");
            assert_eq!(app.doc().find_shape(b).is_none(), on, "on: {on}");
            if on {
                assert_eq!(app.selection, vec![made]);
                assert_eq!(app.engine.undo_label(), Some("Shape Builder Leftovers"));
                app.engine.undo().unwrap();
                assert!(app.doc().find_shape(b).is_some());
            }
            assert!(app.builder.session_sources.is_empty() && app.builder.session_made.is_empty());
        }
    }

    /// #4123: with Use Guidelines on and nothing selected, the rooms the
    /// guidelines close in on every side are parts: a click in one makes
    /// a shape of it with the default fill and outline; outside every
    /// room, or with Use Guidelines off, there is none.
    #[test]
    fn guidelines_alone_make_parts() {
        use tracedraw_core::document::GuideLine;
        let mut app = App::headless();
        let page = app.page;
        for g in [
            GuideLine::Vertical { x: 10.0 },
            GuideLine::Vertical { x: 40.0 },
            GuideLine::Horizontal { y: 20.0 },
            GuideLine::Horizontal { y: 30.0 },
        ] {
            app.run(Command::AddGuide { page, guide: g.into() });
        }
        app.select(Vec::new());
        app.set_tool(Tool::ShapeBuilder);
        assert!(app.builder_part_at(Point::new(20.0, 25.0)).is_none(), "off");
        app.builder.use_guides = true;
        let room = app.builder_part_at(Point::new(20.0, 25.0)).expect("the room");
        assert!((area(&room) - 300.0).abs() < 1e-6, "{}", area(&room));
        assert!(app.builder_part_at(Point::new(5.0, 25.0)).is_none(), "open on a side");
        app.builder_take(Point::new(20.0, 25.0));
        app.builder_finish(false);
        let made = app.selection[0];
        assert!((page_area(&app, made) - 300.0).abs() < 1e-6);
        let mut want = Shape::new(made, ShapeKind::Path { path: BezPath::new(), closed: true });
        app.doc().defaults.apply_to(&mut want);
        let got = app.doc().find_shape(made).unwrap();
        assert_eq!(got.fill, want.fill);
        assert_eq!(got.stroke, want.stroke);
    }
}
