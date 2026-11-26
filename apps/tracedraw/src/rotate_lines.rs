//! Object > Transformations > Rotate by Two Lines (#2870): the selection
//! turns so that a line given on it (its base line) takes the angle of a
//! second line given anywhere (the target line). Four clicks: the base
//! line's two ends, then the target line's two, each snapping as a
//! drawing click does; the selection turns about the base line's first
//! end, in one undo step. A right click cancels.

use crate::app::App;
use crate::i18n::tr;
use tracedraw_core::{
    geometry::{Affine, Point},
    Command,
};

/// The turn, in radians, that gives the line `base` (from its first end
/// to its second) the direction of `target`; `None` when either line
/// has no length.
pub fn turn_between(base: (Point, Point), target: (Point, Point)) -> Option<f64> {
    let (b, t) = (base.1 - base.0, target.1 - target.0);
    if b.hypot() < 1e-9 || t.hypot() < 1e-9 {
        return None;
    }
    let turn = t.y.atan2(t.x) - b.y.atan2(b.x);
    let tau = std::f64::consts::TAU;
    // The shorter way round: -180 to 180 degrees.
    let mut turn = turn.rem_euclid(tau);
    if turn > std::f64::consts::PI {
        turn -= tau;
    }
    Some(turn)
}

impl App {
    /// Start Rotate by Two Lines on the selection: the clicks that follow
    /// give the two lines.
    pub fn start_rotate_by_lines(&mut self) {
        if self.selection.is_empty() {
            return;
        }
        self.pending_rotate_lines = Some(Vec::new());
        self.status = tr("status.rotate_lines_base");
    }

    /// Where the next click of Rotate by Two Lines lands for the pointer
    /// at `p` (snapped already): a line's second end goes in steps of the
    /// constrain angle from its first while the constrain key is held,
    /// as when drawing a line (#4373).
    pub fn rotate_lines_point(&self, p: Point, constrain: bool) -> Point {
        match self.pending_rotate_lines.as_deref() {
            Some(pts) if constrain && pts.len() % 2 == 1 => pts
                .last()
                .map(|from| crate::interaction::constrain_from(*from, p, self.settings.constrain_angle))
                .unwrap_or(p),
            _ => p,
        }
    }

    /// One click of Rotate by Two Lines at `p` (page space, snapped
    /// already): the fourth turns the selection.
    pub fn rotate_lines_click(&mut self, p: Point) {
        let Some(pts) = self.pending_rotate_lines.as_mut() else {
            return;
        };
        pts.push(p);
        let n = pts.len();
        if n < 4 {
            self.status = tr(if n < 2 {
                "status.rotate_lines_base"
            } else {
                "status.rotate_lines_target"
            });
            return;
        }
        let pts = self.pending_rotate_lines.take().unwrap_or_default();
        self.status.clear();
        self.rotate_selection_by_lines((pts[0], pts[1]), (pts[2], pts[3]));
    }

    /// Turn the selection about `base`'s first end so that `base` takes
    /// the direction of `target` (#2870).
    pub fn rotate_selection_by_lines(&mut self, base: (Point, Point), target: (Point, Point)) {
        let Some(turn) = turn_between(base, target) else {
            self.status = tr("status.rotate_lines_no_length");
            return;
        };
        if turn.abs() < 1e-12 || self.selection.is_empty() {
            return;
        }
        let shapes = self.selection.clone();
        let transform = Affine::rotate_about(turn, base.0);
        if let Err(e) = self
            .engine
            .run_with_label(&Command::TransformShapes { shapes, transform }, "Rotate")
        {
            self.status = e.to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::Rect;
    use tracedraw_core::ShapeKind;

    /// A rectangle's bottom edge as the base line and a line at 30
    /// degrees as the target: the rectangle turns 30 degrees about the
    /// base line's first end, one undo step; the shorter way round.
    #[test]
    fn the_base_line_takes_the_target_lines_angle() {
        let mut app = App::headless();
        let id = app
            .new_shape(ShapeKind::Rect {
                rect: Rect::new(10.0, 10.0, 50.0, 30.0),
                radius: 0.0,
                corners: None,
            })
            .expect("rect");
        app.select(vec![id]);
        app.start_rotate_by_lines();
        assert!(app.pending_rotate_lines.is_some());
        let depth = app.engine.history_labels().0.len();
        let a = 30f64.to_radians();
        for p in [
            Point::new(10.0, 10.0),
            Point::new(50.0, 10.0),
            Point::new(100.0, 100.0),
            Point::new(100.0 + 20.0 * a.cos(), 100.0 + 20.0 * a.sin()),
        ] {
            app.rotate_lines_click(p);
        }
        assert!(app.pending_rotate_lines.is_none());
        assert_eq!(app.engine.history_labels().0.len(), depth + 1, "one step");
        let s = app.doc().find_shape(id).expect("rect");
        let path = s.page_path();
        let corner = |q: Point| {
            path.elements()
                .iter()
                .filter_map(|e| e.end_point())
                .any(|c| (c - q).hypot() < 1e-6)
        };
        // The first end stays; the second goes 40 mm along 30 degrees.
        assert!(corner(Point::new(10.0, 10.0)));
        assert!(corner(Point::new(
            10.0 + 40.0 * a.cos(),
            10.0 + 40.0 * a.sin()
        )));
        // #4373: with the constrain key a line's second end keeps to the
        // constrain angle's steps from its first; the first end does not.
        app.select(vec![id]);
        app.start_rotate_by_lines();
        let free = Point::new(3.0, 50.0);
        assert_eq!(app.rotate_lines_point(free, true), free);
        app.rotate_lines_click(Point::ZERO);
        let q = app.rotate_lines_point(Point::new(20.0, 1.0), true);
        assert!(q.y.abs() < 1e-9 && q.x > 19.0, "{q:?}");
        assert_eq!(app.rotate_lines_point(Point::new(20.0, 1.0), false), Point::new(20.0, 1.0));
        app.pending_rotate_lines = None;
        // The shorter way: 350 degrees is -10.
        let t = turn_between(
            (Point::ZERO, Point::new(1.0, 0.0)),
            (
                Point::ZERO,
                Point::new(10f64.to_radians().cos(), -(10f64.to_radians().sin())),
            ),
        )
        .expect("a turn");
        assert!((t + 10f64.to_radians()).abs() < 1e-9);
        // A line of no length turns nothing.
        assert!(turn_between(
            (Point::ZERO, Point::ZERO),
            (Point::ZERO, Point::new(1.0, 0.0))
        )
        .is_none());
    }
}
