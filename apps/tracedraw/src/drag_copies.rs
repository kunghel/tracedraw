//! Copies left while drawing, and the arc of the 2-Point Line tool.
//!
//! While a shape is dragged out with a box tool (Rectangle, Ellipse,
//! Polygon, Star, Graph Paper, Action Lines, Balloon) or an arc with the
//! 2-Point Line tool, holding C leaves a copy of the shape as it is at
//! each move of the pointer; the shape let go is the last one. All the
//! copies and the last shape are one undo step and end up selected, so a
//! fan of lines or a tunnel of rectangles is one drag. C is the same key
//! on every keyboard layout and picks no tool while a shape is dragged.
//!
//! The arc (2-Point Line tool, Arc on its property bar): a quarter of an
//! ellipse from where the drag starts to where it is let go, leaving the
//! start up or down and arriving across; Flip arc bows it the other way.

use crate::app::{App, Drag};
use crate::tools::Tool;
use tracedraw_core::geometry::{BezPath, Point};
use tracedraw_core::{ShapeId, ShapeKind};

/// The key held to leave copies.
pub const TRAIL_KEY: egui::Key = egui::Key::C;

/// How far apart, in screen pixels, two copies are at least.
const TRAIL_SPACING_PX: f64 = 3.0;

/// The copies left during one drag.
#[derive(Debug, Clone, Default)]
pub struct DragCopies {
    /// The undo mark from before the first copy.
    pub mark: Option<u64>,
    /// The copies made so far.
    pub ids: Vec<ShapeId>,
    /// Where the pointer was at the last copy (page space).
    pub last: Option<Point>,
}

/// A quarter ellipse from `a` to `b`: it leaves `a` along the vertical
/// and reaches `b` along the horizontal (`flip`: the other way round), as
/// the arc of a circle when the box is square.
pub fn arc_path(a: Point, b: Point, flip: bool) -> BezPath {
    // The corner of the box the arc bows away from.
    let corner = if flip {
        Point::new(b.x, a.y)
    } else {
        Point::new(a.x, b.y)
    };
    // The handle length of a cubic quarter circle.
    const K: f64 = 0.552_284_749_830_793_4;
    let mut p = BezPath::new();
    p.move_to(a);
    p.curve_to(a + (corner - a) * K, b + (corner - b) * K, b);
    p
}

impl App {
    /// The 2-Point Line tool draws arcs (Arc on its property bar).
    pub fn draws_arcs(&self) -> bool {
        self.tool == Tool::TwoPointLine && self.line_arc
    }

    /// The shape tools whose drag can leave copies.
    fn leaves_copies(&self) -> bool {
        self.tool.is_box_tool() || self.draws_arcs()
    }

    /// The copy key is held over the drawing (no text typed, nothing else
    /// held with it).
    pub fn trail_key_held(&self, ctx: &egui::Context) -> bool {
        self.text_edit.is_none() && ctx.input(|i| i.key_down(TRAIL_KEY) && i.modifiers.is_none())
    }

    /// The copy key is pressed while a shape that leaves copies is being
    /// dragged: it picks no tool then.
    pub fn trail_key_busy(&self) -> bool {
        matches!(self.drag, Drag::Box { .. }) && self.leaves_copies()
    }

    /// The arc of a drag from `start` to `current`.
    pub fn arc_of_drag(&self, start: Point, current: Point) -> BezPath {
        arc_path(start, current, self.line_arc_flip)
    }

    /// Draw the arc from `a` to `b` with the default outline of a curve;
    /// selected.
    pub fn create_arc(&mut self, a: Point, b: Point) -> Option<ShapeId> {
        if (b - a).hypot() < 0.05 {
            return None;
        }
        let path = self.arc_of_drag(a, b);
        let id = self.new_shape(ShapeKind::Path {
            path,
            closed: false,
        })?;
        self.select(vec![id]);
        Some(id)
    }

    /// The shape of the box drag as it is now, made as the drag's end
    /// would make it; its id.
    fn make_dragged_shape(&mut self) -> Option<ShapeId> {
        let Drag::Box {
            start,
            current,
            from_center,
        } = self.drag
        else {
            return None;
        };
        if self.draws_arcs() {
            return self.create_arc(start, current);
        }
        let r = App::box_rect(start, current, from_center);
        let before = self.selection.clone();
        self.create_box_shape(Point::new(r.x0, r.y0), Point::new(r.x1, r.y1));
        self.selection
            .first()
            .copied()
            .filter(|id| !before.contains(id))
    }

    /// While the copy key is held during a box or arc drag, leave a copy
    /// of the shape where the pointer has moved to.
    pub fn leave_drag_copy(&mut self, ctx: &egui::Context) {
        if !self.leaves_copies() || !self.trail_key_held(ctx) {
            return;
        }
        let Drag::Box { current, .. } = self.drag else {
            return;
        };
        let zoom = f64::from(self.view.zoom.max(1e-6));
        let far = self
            .drag_copies
            .as_ref()
            .and_then(|c| c.last)
            .is_none_or(|last| (current - last).hypot() * zoom >= TRAIL_SPACING_PX);
        if !far {
            return;
        }
        let mark = match self.drag_copies.as_ref() {
            Some(c) => c.mark,
            None => self.engine.begin_group(),
        };
        let mut copies = self.drag_copies.take().unwrap_or(DragCopies {
            mark,
            ..Default::default()
        });
        if let Some(id) = self.make_dragged_shape() {
            copies.ids.push(id);
        }
        copies.last = Some(current);
        self.drag_copies = Some(copies);
    }

    /// The drag that left copies is over (the last shape made, or the drag
    /// let go or cancelled): the copies and the last shape become one
    /// undo step, all selected.
    pub fn end_drag_copies(&mut self) {
        let Some(copies) = self.drag_copies.take() else {
            return;
        };
        self.engine.end_group(copies.mark, "Draw with Copies");
        let mut ids: Vec<ShapeId> = copies
            .ids
            .into_iter()
            .filter(|id| self.doc().find_shape(*id).is_some())
            .collect();
        for id in self.selection.clone() {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        self.select(ids);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::ParamCurve as _;
    use tracedraw_core::geometry::PathEl;

    /// The arc runs from the press to the release, bowing away from the
    /// corner under the start, or the other corner when flipped.
    #[test]
    fn the_arc_is_a_quarter_ellipse_between_the_points() {
        let (a, b) = (Point::new(0.0, 0.0), Point::new(10.0, 10.0));
        let p = arc_path(a, b, false);
        let els = p.elements();
        assert_eq!(els.len(), 2);
        let PathEl::CurveTo(c1, c2, end) = els[1] else {
            panic!("a curve");
        };
        assert_eq!(end, b);
        // Leaves the start upright, arrives across.
        assert!(c1.x.abs() < 1e-9 && c1.y > 0.0);
        assert!((c2.y - 10.0).abs() < 1e-9 && c2.x < 10.0);
        // A quarter circle: its middle is the radius from the corner.
        let mid = p.segments().next().expect("segment").eval(0.5);
        let r = (mid - Point::new(10.0, 0.0)).hypot();
        assert!((r - 10.0).abs() < 0.05, "{r}");
        let q = arc_path(a, b, true);
        let mid = q.segments().next().expect("segment").eval(0.5);
        let r = (mid - Point::new(0.0, 10.0)).hypot();
        assert!((r - 10.0).abs() < 0.05, "{r}");
    }

    fn count(app: &App) -> usize {
        app.doc().all_layers().map(|l| l.shapes.len()).sum()
    }

    /// C held while an arc and a rectangle are dragged leaves copies at
    /// each move; the copies and the shape let go are one undo step and
    /// all selected; without C one shape is drawn.
    #[test]
    fn holding_c_leaves_copies_as_one_step() {
        let mut app = App::headless();
        let ctx = crate::theme::ui_context();
        let held = egui::RawInput {
            events: vec![egui::Event::Key {
                key: TRAIL_KEY,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };
        let mut out = ctx.run_ui(held, |_| {});
        out.textures_delta.clear();
        assert!(app.trail_key_held(&ctx));
        for (tool, arc) in [(Tool::TwoPointLine, true), (Tool::Rectangle, false)] {
            app.tool = tool;
            app.line_arc = arc;
            let before = count(&app);
            app.drag = Drag::Box {
                start: Point::new(0.0, 0.0),
                current: Point::new(0.0, 0.0),
                from_center: false,
            };
            for k in 1..=5 {
                if let Drag::Box { current, .. } = &mut app.drag {
                    *current = Point::new(20.0 + f64::from(k) * 5.0, 30.0);
                }
                app.leave_drag_copy(&ctx);
                // The same place again leaves nothing.
                app.leave_drag_copy(&ctx);
            }
            app.end_drag();
            let after = count(&app);
            assert_eq!(after - before, 6, "{tool:?}");
            assert_eq!(app.selection.len(), 6, "{tool:?}");
            if arc {
                let s = app.doc().find_shape(app.selection[0]).expect("arc");
                assert!(matches!(&s.kind, ShapeKind::Path { closed: false, path }
                    if path.elements().iter().any(|e| matches!(e, PathEl::CurveTo(..)))));
            }
            app.undo();
            assert_eq!(count(&app), before, "{tool:?}");
            assert!(app.drag_copies.is_none());
        }
        // Without the key: one arc.
        let ctx = egui::Context::default();
        app.tool = Tool::TwoPointLine;
        app.line_arc = true;
        let before = count(&app);
        app.drag = Drag::Box {
            start: Point::new(0.0, 0.0),
            current: Point::new(30.0, 30.0),
            from_center: false,
        };
        app.leave_drag_copy(&ctx);
        app.end_drag();
        assert_eq!(count(&app) - before, 1);
    }
}
