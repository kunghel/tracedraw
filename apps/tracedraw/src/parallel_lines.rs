//! Parallel Lines (#3626): with the option on in the curve tools'
//! property bar, each new curve drawn with the Freehand, Bezier, Pen,
//! Polyline, 2-Point Line or 3-Point Curve tool gets lines alongside it,
//! at the set distance one from the next, on its left or on both sides.
//! The lines are curves of their own with the curve's outline and fill,
//! selected with it; one undo step after the curve's own.

use crate::app::App;
use crate::tools::Tool;
use tracedraw_core::geometry::Affine;
use tracedraw_core::{Command, ShapeId, ShapeKind};

impl App {
    /// The tools that draw parallel lines with the option on.
    pub(crate) fn draws_parallel_lines(&self) -> bool {
        matches!(
            self.tool,
            Tool::Freehand
                | Tool::Bezier
                | Tool::Pen
                | Tool::Polyline
                | Tool::TwoPointLine
                | Tool::ThreePointCurve
        )
    }

    /// Curve `id`, just drawn as a curve of its own: the lines alongside
    /// it when Parallel Lines is on, all selected.
    pub(crate) fn add_parallel_lines(&mut self, id: ShapeId) {
        if !self.draws_parallel_lines() || self.symmetry_edit.is_some() {
            return;
        }
        let distances = self.settings.curves.parallel.distances();
        if distances.is_empty() {
            return;
        }
        let Some(src) = self.doc().find_shape(id).cloned() else {
            return;
        };
        let ShapeKind::Path { closed, .. } = src.kind else {
            return;
        };
        let Some(layer) = self.active_layer() else {
            return;
        };
        let page = src.page_path();
        let mut cmds = Vec::new();
        let mut ids = vec![id];
        for d in distances {
            let path = tracedraw_core::parallel::parallel_path(&page, d);
            if path.elements().len() < 2 {
                continue;
            }
            let nid = self.engine.new_shape_id();
            let mut shape = src.clone();
            shape.id = nid;
            shape.uid = None;
            shape.transform = Affine::IDENTITY;
            shape.kind = ShapeKind::Path { path, closed };
            cmds.push(Command::AddShape { layer, shape });
            ids.push(nid);
        }
        if cmds.is_empty() {
            return;
        }
        if let Err(e) = self.engine.run_batch("Parallel Lines", &cmds) {
            self.status = e.to_string();
            return;
        }
        self.select(ids);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::CurveInProgress;
    use kurbo::Shape as _;
    use tracedraw_core::geometry::Point;

    fn draw_line(app: &mut App) {
        app.curve = Some(CurveInProgress {
            nodes: vec![
                (Point::new(10.0, 10.0), None),
                (Point::new(50.0, 10.0), None),
            ],
            ..Default::default()
        });
        app.finish_curve();
    }

    #[test]
    fn a_curve_drawn_with_parallel_lines_on_gets_lines_alongside() {
        let mut app = App::headless();
        app.set_tool(Tool::Polyline);
        // Off at first: one line.
        draw_line(&mut app);
        assert_eq!(app.selection.len(), 1);
        app.select(Vec::new());
        app.settings.curves.parallel.on = true;
        app.settings.curves.parallel.count = 2;
        app.settings.curves.parallel.gap = 3.0;
        app.settings.curves.parallel.both_sides = true;
        draw_line(&mut app);
        assert_eq!(app.selection.len(), 5);
        let mut ys: Vec<f64> = app
            .selected_shapes()
            .iter()
            .map(|s| s.page_path().bounding_box().y0)
            .collect();
        ys.sort_by(f64::total_cmp);
        let want = [4.0, 7.0, 10.0, 13.0, 16.0];
        for (y, w) in ys.iter().zip(want) {
            assert!((y - w).abs() < 1e-6, "{ys:?}");
        }
        // One undo takes the lines away and leaves the curve.
        app.undo();
        let left = app.doc().pages[0]
            .layers
            .iter()
            .map(|l| l.shapes.len())
            .sum::<usize>();
        assert_eq!(left, 2);
    }
}
