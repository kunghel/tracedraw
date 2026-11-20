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
    hover: std::cell::RefCell<Option<Hover>>,
}

struct Hover {
    key: (u64, Vec<ShapeId>),
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
    /// The selected objects the Shape Builder works on, bottom first.
    pub fn builder_pieces(&self) -> Vec<Piece> {
        let mut shapes: Vec<Shape> = self.selected_shapes().into_iter().filter(usable).collect();
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

    /// The part under `p`, worked out again only when `p` leaves the one
    /// found last or the drawing or the selection changed.
    pub fn builder_part_at(&self, p: Point) -> Option<BezPath> {
        let key = (self.engine.revision(), self.selection.clone());
        if let Some(h) = self.builder.hover.borrow().as_ref() {
            if h.key == key && inside(&h.part, p) {
                return Some(h.part.clone());
            }
        }
        let areas: Vec<BezPath> = self.builder_pieces().into_iter().map(|q| q.area).collect();
        let part = part_at(&areas, p)?;
        *self.builder.hover.borrow_mut() = Some(Hover {
            key,
            part: part.clone(),
        });
        Some(part)
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

    pub fn shape_builder_input(&mut self, response: &Response, p: Point, mods: Modifiers) {
        let pieces = !self.builder_pieces().is_empty();
        if response.clicked_by(PointerButton::Primary) {
            if !pieces || self.builder_part_at(p).is_none() {
                self.click_select(p, mods);
                if self.builder_pieces().is_empty() {
                    self.status = crate::i18n::tr("status.shape_builder_select");
                }
                return;
            }
            self.builder.parts.clear();
            self.builder_take(p);
            let remove = mods.alt != self.builder.delete_mode;
            self.builder_finish(remove);
            return;
        }
        if response.drag_started_by(PointerButton::Primary) {
            self.builder.parts.clear();
            self.builder.remove = mods.alt != self.builder.delete_mode;
            self.drag = Drag::Freehand { points: vec![p] };
            self.builder_take(p);
        }
        if response.dragged_by(PointerButton::Primary) {
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
        let areas: Vec<BezPath> = pieces.iter().map(|q| q.area.clone()).collect();
        let area = merged(&parts, &areas);
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
            let rest = overlay(&q.area, &area, Op::Trim);
            if tracedraw_core::shaping::filled_area(&rest, Default::default()) < 1e-6 {
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
            }
            cmds.push(Command::AddShape { layer, shape: s });
            made = Some(id);
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
        // The objects stay selected, with the new one, to go on building.
        kept.extend(made);
        self.select(kept);
    }
}

impl App {
    /// The Shape Builder's Accept (#3220): the shapes built stay, and the
    /// Pick tool comes back.
    pub fn builder_accept(&mut self) {
        self.builder.session_steps = 0;
        self.builder.parts.clear();
        self.set_tool(Tool::Pick);
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
                .is_some_and(|l| l.starts_with("Shape Builder"));
            if !ours || self.engine.undo().is_err() {
                break;
            }
            self.builder.session_steps -= 1;
        }
        self.builder.session_steps = 0;
        let ids: Vec<ShapeId> = self
            .selection
            .iter()
            .copied()
            .filter(|id| self.doc().find_shape(*id).is_some())
            .collect();
        self.select(ids);
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
    let color = crate::theme::Tokens::SELECTION;
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
}

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
}
