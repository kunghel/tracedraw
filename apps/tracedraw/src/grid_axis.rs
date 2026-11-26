//! The document grid's axis controls on the canvas (Grid panel, Show Axis
//! Controls, #4395): a round handle at the grid's origin and an arrow
//! along each axis. With the Pick tool the origin drags the grid along;
//! an arrow's end sets that axis's spacing (and with the arrow turned,
//! its angle: the grid becomes a custom one). Shift keeps the arrow's
//! direction. The canvas shows the grid live while dragging; the release
//! changes the document in one step.

use crate::app::{App, Drag};
use tracedraw_core::document::{GridKind, GridMode, GridSettings};
use tracedraw_core::geometry::{Point, Vec2};
use tracedraw_core::Command;

/// One of the axis controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxisPart {
    Origin,
    First,
    Second,
}

/// How near a handle the press must be, in screen points.
const REACH_PX: f64 = 8.0;
/// The shortest an arrow is drawn, in screen points: as many whole cells
/// as that takes.
const ARROW_MIN_PX: f64 = 56.0;

impl App {
    /// The axis controls shown: the grid shown and its controls on.
    pub fn grid_axis_shown(&self) -> bool {
        self.grid_axis_controls && self.show_grid && self.drawing_shown()
    }

    /// The axis controls and where they are (page space), each arrow's
    /// end with the number of cells it spans.
    pub fn grid_axis_handles(&self) -> Vec<(AxisPart, Point, f64)> {
        if !self.grid_axis_shown() {
            return Vec::new();
        }
        let geom = self.grid_geom();
        let zoom = f64::from(self.view.zoom.max(1e-6));
        let mut out = vec![(AxisPart::Origin, geom.origin, 0.0)];
        for (part, axis) in [(AxisPart::First, geom.axes[0]), (AxisPart::Second, geom.axes[1])] {
            let pitch = axis.pitch().max(1e-6);
            let cells = (ARROW_MIN_PX / (pitch * zoom)).ceil().clamp(1.0, 1e6);
            out.push((part, geom.origin + axis.dir * (pitch * cells), cells));
        }
        out
    }

    /// A press at `p` on one of the axis controls starts dragging it.
    pub fn grid_axis_press(&mut self, p: Point) -> bool {
        let reach = REACH_PX / f64::from(self.view.zoom.max(1e-6));
        // The arrows' ends before the origin: at a large zoom out they
        // may lie close together.
        let mut handles = self.grid_axis_handles();
        handles.reverse();
        let Some((part, _, cells)) = handles.into_iter().find(|(_, h, _)| (*h - p).hypot() <= reach)
        else {
            return false;
        };
        let start = self.doc().metadata.grid;
        let start = match start.mode {
            // The cells shown now become the fixed ones once dragged.
            GridMode::Automatic if part != AxisPart::Origin => {
                let geom = self.grid_geom();
                GridSettings {
                    mode: GridMode::Basic,
                    spacing_x: geom.axes[0].spacing,
                    spacing_y: geom.axes[1].spacing,
                    divisions_x: geom.axes[0].divisions,
                    divisions_y: geom.axes[1].divisions,
                    ..start
                }
            }
            _ => start,
        };
        self.drag = Drag::GridAxis {
            part,
            from: p,
            start,
            cells,
        };
        self.grid_live = Some(start);
        true
    }

    /// The axis control being dragged moved to `p`; `keep_angle` (Shift)
    /// keeps an arrow's direction.
    pub fn grid_axis_drag_to(&mut self, p: Point, keep_angle: bool) {
        let Drag::GridAxis {
            part,
            from,
            start,
            cells,
        } = self.drag
        else {
            return;
        };
        let origin = self.grid_origin();
        self.grid_live = Some(dragged(&start, part, from, p, cells, origin, keep_angle));
    }

    /// The release: the grid as dragged goes into the document in one
    /// step.
    pub fn finish_grid_axis(&mut self) {
        let Some(g) = self.grid_live.take() else {
            return;
        };
        let mut meta = self.doc().metadata.clone();
        if meta.grid != g {
            meta.grid = g;
            self.run(Command::SetMetadata { metadata: meta });
        }
    }
}

/// The grid `start` with the control `part` dragged from `from` to `p`
/// (page space), an arrow spanning `cells` cells, the ruler origin at
/// `ruler_origin`.
pub fn dragged(
    start: &GridSettings,
    part: AxisPart,
    from: Point,
    p: Point,
    cells: f64,
    ruler_origin: Point,
    keep_angle: bool,
) -> GridSettings {
    let mut g = *start;
    if part == AxisPart::Origin {
        let d = p - from;
        g.offset_x = start.offset_x + d.x;
        g.offset_y = start.offset_y + d.y;
        return g;
    }
    let k = if part == AxisPart::First { 0 } else { 1 };
    let origin = Point::new(
        ruler_origin.x + start.offset_x,
        ruler_origin.y + start.offset_y,
    );
    let (a0, a1) = start.axis_angles();
    let old = if k == 0 { a0 } else { a1 };
    let w = p - origin;
    let dir = Vec2::new(old.to_radians().cos(), old.to_radians().sin());
    let (len, angle) = if keep_angle {
        (w.dot(dir), old)
    } else {
        (w.hypot(), snap_angle(w.y.atan2(w.x).to_degrees()))
    };
    let gutter = if start.mode == GridMode::Advanced {
        if k == 0 || start.uniform {
            start.gutter_x
        } else {
            start.gutter_y
        }
    } else {
        0.0
    };
    let spacing = (len / cells.max(1.0) - gutter).max(0.01);
    // The arrow turned: the grid's axes become its own angles.
    if (angle - old).abs() > 1e-6 && !keep_angle {
        if g.mode != GridMode::Advanced {
            g.divisions_y = g.divisions_x;
            g.mode = GridMode::Advanced;
        }
        g.kind = GridKind::Custom;
        g.angle_x = if k == 0 { angle } else { a0 };
        g.angle_y = if k == 1 { angle } else { a1 };
    }
    let k = if g.mode == GridMode::Advanced && g.uniform { 0 } else { k };
    crate::ui::grid_docker::set_spacing(&mut g, k, spacing);
    g
}

/// An angle in degrees, whole degrees, onto a multiple of 15 within 3.
fn snap_angle(a: f64) -> f64 {
    let fifteen = (a / 15.0).round() * 15.0;
    if (a - fifteen).abs() <= 3.0 {
        fifteen
    } else {
        a.round()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_origin_drags_the_grid_along() {
        let g = GridSettings::default();
        let out = dragged(
            &g,
            AxisPart::Origin,
            Point::new(10.0, 10.0),
            Point::new(13.0, 8.0),
            1.0,
            Point::ZERO,
            false,
        );
        assert_eq!((out.offset_x, out.offset_y), (3.0, -2.0));
        assert_eq!(out.spacing_x, g.spacing_x);
    }

    #[test]
    fn an_arrow_sets_its_spacing_and_turned_its_angle() {
        let g = GridSettings::default();
        // Pulled out along x to 30 mm over two cells: 15 mm cells, still
        // a Basic grid.
        let out = dragged(
            &g,
            AxisPart::First,
            Point::new(20.0, 0.0),
            Point::new(30.0, 0.4),
            2.0,
            Point::ZERO,
            false,
        );
        assert_eq!(out.mode, GridMode::Basic);
        assert!((out.spacing_x - 15.0).abs() < 0.01, "{}", out.spacing_x);
        assert_eq!(out.spacing_y, 10.0);
        // Turned to 30 degrees: a custom Advanced grid.
        let r = 20.0;
        let at = Point::new(r * 30f64.to_radians().cos(), r * 30f64.to_radians().sin());
        let out = dragged(&g, AxisPart::First, Point::new(20.0, 0.0), at, 2.0, Point::ZERO, false);
        assert_eq!(out.mode, GridMode::Advanced);
        assert_eq!(out.kind, GridKind::Custom);
        assert!((out.angle_x - 30.0).abs() < 1e-9);
        assert_eq!(out.angle_y, 90.0);
        // Shift keeps the direction: only the length along it counts.
        let out = dragged(&g, AxisPart::Second, Point::new(0.0, 10.0), Point::new(7.0, 25.0), 1.0, Point::ZERO, true);
        assert_eq!(out.mode, GridMode::Basic);
        assert!((out.spacing_y - 25.0).abs() < 1e-9);
        assert_eq!(snap_angle(46.0), 45.0);
        assert_eq!(snap_angle(52.4), 52.0);
    }

    #[test]
    fn the_controls_show_with_the_grid_and_a_drag_is_one_step() {
        let mut app = App::headless();
        assert!(app.grid_axis_handles().is_empty());
        app.show_grid = true;
        app.grid_axis_controls = true;
        let handles = app.grid_axis_handles();
        assert_eq!(handles.len(), 3);
        let origin = handles[0].1;
        assert!(app.grid_axis_press(origin));
        app.grid_axis_drag_to(origin + Vec2::new(4.0, 0.0), false);
        assert_eq!(app.grid_settings().offset_x, 4.0);
        assert_eq!(app.doc().metadata.grid.offset_x, 0.0, "live while dragging");
        app.drag = Drag::None;
        app.finish_grid_axis();
        assert_eq!(app.doc().metadata.grid.offset_x, 4.0);
        app.undo();
        assert_eq!(app.doc().metadata.grid.offset_x, 0.0);
    }
}
