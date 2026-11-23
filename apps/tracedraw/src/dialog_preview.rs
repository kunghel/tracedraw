//! What a dialog is about to do, drawn on the page while the dialog is
//! open: where Arrange in Grid puts the objects, the places Fill With
//! Copies fills and the curves Outline Offset makes. The lines are plain
//! screen lines, a light one under a blue one so they show on any colour.

use crate::app::App;
use crate::ui::dialogs::Dialog;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use tracedraw_core::geometry::{BezPath, PathEl, Point, Rect};

/// The most boxes a preview draws (a larger result shows its first).
const MAX_BOXES: usize = 4000;

/// The offset curves kept while the dialog's values and the drawing stay
/// the same: what they were made from, hashed, and the curves.
pub type OffsetCache = std::cell::RefCell<Option<(u64, Arc<Vec<BezPath>>)>>;

fn strokes() -> [egui::Stroke; 2] {
    [
        egui::Stroke::new(3.0, egui::Color32::from_white_alpha(170)),
        egui::Stroke::new(1.0, egui::Color32::from_rgb(0, 110, 230)),
    ]
}

fn paint_boxes(app: &App, painter: &egui::Painter, boxes: &[Rect]) {
    for stroke in strokes() {
        for r in boxes.iter().take(MAX_BOXES) {
            let c = [
                Point::new(r.x0, r.y0),
                Point::new(r.x1, r.y0),
                Point::new(r.x1, r.y1),
                Point::new(r.x0, r.y1),
            ];
            for k in 0..4 {
                painter.line_segment(
                    [app.view.to_screen(c[k]), app.view.to_screen(c[(k + 1) % 4])],
                    stroke,
                );
            }
        }
    }
}

/// `path` as screen polylines, each with whether it is closed.
fn polylines(app: &App, path: &BezPath) -> Vec<(Vec<egui::Pos2>, bool)> {
    let tolerance = 0.3 / f64::from(app.view.zoom).max(1e-6);
    let mut out: Vec<(Vec<egui::Pos2>, bool)> = Vec::new();
    tracedraw_core::geometry::flatten(path, tolerance, &mut |el| match el {
        PathEl::MoveTo(p) => out.push((vec![app.view.to_screen(p)], false)),
        PathEl::LineTo(p) => {
            if let Some(last) = out.last_mut() {
                last.0.push(app.view.to_screen(p));
            }
        }
        PathEl::ClosePath => {
            if let Some(last) = out.last_mut() {
                last.1 = true;
            }
        }
        _ => {}
    });
    out
}

fn paint_paths(app: &App, painter: &egui::Painter, paths: &[BezPath]) {
    let lines: Vec<(Vec<egui::Pos2>, bool)> =
        paths.iter().flat_map(|p| polylines(app, p)).collect();
    for stroke in strokes() {
        for (pts, closed) in &lines {
            if pts.len() < 2 {
                continue;
            }
            painter.add(if *closed {
                egui::epaint::PathShape::closed_line(pts.clone(), stroke)
            } else {
                egui::epaint::PathShape::line(pts.clone(), stroke)
            });
        }
    }
}

/// The boxes Arrange in Grid moves the objects' boxes to.
pub fn arrange_boxes(app: &App, g: &crate::ops::GridArrange) -> Vec<Rect> {
    app.arrange_grid_plan(g)
        .into_iter()
        .map(|(_, b, d)| b + d)
        .collect()
}

/// The boxes of the copies Fill With Copies makes (gaps in mm).
pub fn copies_boxes(app: &App, gap_x: f64, gap_y: f64) -> Vec<Rect> {
    match app.fill_with_copies_plan(gap_x, gap_y) {
        Some((_, b, places)) => places.into_iter().map(|d| b + d).collect(),
        None => Vec::new(),
    }
}

/// The curves Outline Offset makes, from the cache while nothing they
/// depend on changed.
fn offset_curves(
    app: &App,
    mm: f64,
    outside: bool,
    join: tracedraw_core::shaping::Join,
    steps: u32,
) -> Arc<Vec<BezPath>> {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    app.engine.revision().hash(&mut h);
    app.selection.hash(&mut h);
    mm.to_bits().hash(&mut h);
    (outside, join as u8, steps).hash(&mut h);
    let key = h.finish();
    if let Ok(cache) = app.offset_preview.try_borrow() {
        if let Some((k, paths)) = cache.as_ref() {
            if *k == key {
                return paths.clone();
            }
        }
    }
    let paths: Arc<Vec<BezPath>> = Arc::new(
        app.outline_offset_paths(mm, outside, join, steps)
            .into_iter()
            .flat_map(|(_, p)| p)
            .collect(),
    );
    if let Ok(mut cache) = app.offset_preview.try_borrow_mut() {
        *cache = Some((key, paths.clone()));
    }
    paths
}

/// Draw the open dialog's preview over the page, when it has one and
/// Preview is on.
pub fn paint(app: &App, painter: &egui::Painter) {
    if !app.dialog_preview {
        return;
    }
    let u = app.units;
    match &app.dialog {
        Dialog::ArrangeGrid {
            columns,
            gap_x,
            gap_y,
            by_position,
            compact,
            align_x,
            align_y,
        } => {
            let g = crate::ops::GridArrange {
                columns: *columns,
                gap_x: u.to_mm(*gap_x),
                gap_y: u.to_mm(*gap_y),
                by_position: *by_position,
                compact: *compact,
                align_x: *align_x,
                align_y: *align_y,
            };
            paint_boxes(app, painter, &arrange_boxes(app, &g));
        }
        Dialog::FillWithCopies { gap_x, gap_y } => {
            paint_boxes(
                app,
                painter,
                &copies_boxes(app, u.to_mm(*gap_x), u.to_mm(*gap_y)),
            );
        }
        Dialog::OutlineOffset {
            distance,
            outside,
            join,
            steps,
        } => {
            let paths = offset_curves(app, u.to_mm(*distance), *outside, *join, *steps);
            paint_paths(app, painter, &paths);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::GridArrange;
    use tracedraw_core::geometry::Shape as _;
    use tracedraw_core::shaping::Join;
    use tracedraw_core::ShapeKind;

    fn rect(app: &mut App, r: Rect) -> tracedraw_core::ShapeId {
        app.new_shape(ShapeKind::Rect {
            rect: r,
            radius: 0.0,
            corners: None,
        })
        .expect("rect")
    }

    fn bounds(app: &App, id: tracedraw_core::ShapeId) -> Rect {
        app.doc().find_shape(id).expect("shape").bounds()
    }

    /// The preview's boxes are where Arrange in Grid then puts the
    /// objects, with the cells fitted to their columns and rows and the
    /// objects at the cells' top left.
    #[test]
    fn arrange_preview_is_where_the_objects_go() {
        let mut app = App::headless();
        let a = rect(&mut app, Rect::new(0.0, 90.0, 10.0, 100.0));
        let b = rect(&mut app, Rect::new(40.0, 60.0, 44.0, 64.0));
        let c = rect(&mut app, Rect::new(70.0, 20.0, 76.0, 40.0));
        let d = rect(&mut app, Rect::new(20.0, 0.0, 22.0, 2.0));
        app.select(vec![a, b, c, d]);
        let g = GridArrange {
            columns: 2,
            gap_x: 1.0,
            gap_y: 2.0,
            compact: true,
            align_x: 0,
            align_y: 0,
            ..GridArrange::default()
        };
        let boxes = arrange_boxes(&app, &g);
        assert_eq!(boxes.len(), 4);
        app.arrange_in_grid_with(&g);
        for (id, want) in [a, b, c, d].into_iter().zip(&boxes) {
            let got = bounds(&app, id);
            assert!(
                (got.x0 - want.x0).abs() < 1e-6 && (got.y1 - want.y1).abs() < 1e-6,
                "{got:?} {want:?}"
            );
        }
        // Columns 10 and 4 wide, rows 10 and 20 tall, from (0, 100).
        assert_eq!(bounds(&app, a), Rect::new(0.0, 90.0, 10.0, 100.0));
        assert_eq!(bounds(&app, b), Rect::new(11.0, 96.0, 15.0, 100.0));
        assert_eq!(bounds(&app, c), Rect::new(0.0, 68.0, 6.0, 88.0));
        assert_eq!(bounds(&app, d), Rect::new(11.0, 86.0, 13.0, 88.0));
    }

    /// As they lie now: the cells are filled top row first and left to
    /// right, whatever order the objects were selected in.
    #[test]
    fn arrange_by_position_reads_the_objects_as_they_lie() {
        let mut app = App::headless();
        let top_left = rect(&mut app, Rect::new(0.0, 50.0, 10.0, 60.0));
        let top_right = rect(&mut app, Rect::new(30.0, 52.0, 40.0, 62.0));
        let low_left = rect(&mut app, Rect::new(2.0, 0.0, 12.0, 10.0));
        let low_right = rect(&mut app, Rect::new(33.0, 3.0, 43.0, 13.0));
        app.select(vec![low_right, top_right, low_left, top_left]);
        app.arrange_in_grid_with(&GridArrange {
            columns: 2,
            by_position: true,
            ..GridArrange::default()
        });
        let x = |id| bounds(&app, id).x0;
        let y = |id| bounds(&app, id).y1;
        assert!(x(top_left) < x(top_right) && x(low_left) < x(low_right));
        assert!(y(top_left) > y(low_left) && (y(top_left) - y(top_right)).abs() < 1e-6);
        assert!((x(top_left) - x(low_left)).abs() < 1e-6);
    }

    /// Fill With Copies previews as many boxes as it then makes copies.
    #[test]
    fn copies_preview_counts_what_fill_makes() {
        let mut app = App::headless();
        let frame = rect(&mut app, Rect::new(0.0, 0.0, 50.0, 30.0));
        let item = rect(&mut app, Rect::new(60.0, 0.0, 68.0, 8.0));
        app.select(vec![frame, item]);
        let boxes = copies_boxes(&app, 2.0, 2.0);
        assert_eq!(boxes.len(), 15);
        let all = Rect::new(0.0, 0.0, 50.0, 30.0);
        assert!(boxes.iter().all(|b| all.union(*b) == all));
        assert_eq!(app.fill_with_copies(2.0, 2.0), 15);
    }

    /// Outline Offset makes as many outlines as asked, each the distance
    /// beyond the one before, in one undo step; round corners stay
    /// inside the sharp ones' box; the preview shows the same curves.
    #[test]
    fn outline_offset_makes_several_outlines_with_the_corners_chosen() {
        let mut app = App::headless();
        let id = rect(&mut app, Rect::new(0.0, 0.0, 10.0, 10.0));
        app.select(vec![id]);
        let shown = offset_curves(&app, 2.0, true, Join::Round, 3);
        assert_eq!(shown.len(), 3);
        let depth = app.engine.history_labels().0.len();
        app.outline_offset_with(2.0, true, Join::Round, 3);
        assert_eq!(app.engine.history_labels().0.len(), depth + 1);
        assert_eq!(app.selection.len(), 3);
        let mut widths: Vec<f64> = app
            .selection
            .iter()
            .map(|s| bounds(&app, *s).width())
            .collect();
        widths.sort_by(f64::total_cmp);
        for (w, want) in widths.iter().zip([14.0, 18.0, 22.0]) {
            assert!((w - want).abs() < 0.05, "{widths:?}");
        }
        // A round corner covers less than a sharp one.
        app.undo();
        app.select(vec![id]);
        let area = |j: Join| {
            let p = &app.outline_offset_paths(2.0, true, j, 1);
            tracedraw_core::shaping::filled_area(
                &p[0].1[0],
                tracedraw_core::shaping::FillRule::EvenOdd,
            )
        };
        assert!(area(Join::Round) < area(Join::Bevel).max(area(Join::Miter)));
        assert!(area(Join::Miter) > area(Join::Bevel));
        // Inside, the outlines stop where nothing is left.
        assert_eq!(app.outline_offset_paths(2.0, false, Join::Miter, 5).len(), 1);
        assert_eq!(app.outline_offset_paths(2.0, false, Join::Miter, 5)[0].1.len(), 2);
        let _ = shown[0].bounding_box();
    }
}
