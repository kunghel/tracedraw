//! The Join tool (#2150): joins the end nodes of open curves. A stroke
//! drawn over end nodes joins them in pairs, in the order the stroke
//! reaches them; a click on an end node and then on another joins those
//! two. Ends lying within the pick distance of each other become one
//! node halfway between them; ends further apart are joined by a straight
//! segment. Ends of two objects make one object (the first one keeps its
//! fill and outline). Every join of one stroke is one undo step.

use crate::app::App;
use crate::i18n::tr;
use crate::tools::Tool;
use egui::{Color32, Painter, PointerButton, Response, Stroke as EStroke};
use std::collections::HashMap;
use tracedraw_core::geometry::{Affine, BezPath, PathEl, Point};
use tracedraw_core::{nodes, Command, ShapeId, ShapeKind};

/// The end nodes of the open subpaths of `path`: (element index, point).
pub(crate) fn open_ends(path: &BezPath) -> Vec<(usize, Point)> {
    let els = path.elements();
    let mut out = Vec::new();
    let mut start: Option<(usize, Point)> = None;
    let mut last: Option<(usize, Point)> = None;
    let flush = |start: &mut Option<(usize, Point)>,
                 last: &mut Option<(usize, Point)>,
                 closed: bool,
                 out: &mut Vec<(usize, Point)>| {
        if let (Some(s), Some(l)) = (start.take(), last.take()) {
            if !closed && l.0 != s.0 {
                out.push(s);
                out.push(l);
            }
        }
    };
    for (i, el) in els.iter().enumerate() {
        match *el {
            PathEl::MoveTo(p) => {
                flush(&mut start, &mut last, false, &mut out);
                start = Some((i, p));
                last = Some((i, p));
            }
            PathEl::LineTo(p) | PathEl::QuadTo(_, p) | PathEl::CurveTo(_, _, p) => {
                last = Some((i, p));
            }
            PathEl::ClosePath => flush(&mut start, &mut last, true, &mut out),
        }
    }
    flush(&mut start, &mut last, false, &mut out);
    out
}

/// One end node on the page: its object and where it is (page space).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JoinEnd {
    pub shape: ShapeId,
    pub at: Point,
}

/// Every subpath of `p` closed (an open one keeps the curve open).
fn all_closed(p: &BezPath) -> bool {
    let mut any = false;
    let mut open = false;
    for el in p.elements() {
        match el {
            PathEl::MoveTo(_) => {
                if any && open {
                    return false;
                }
                any = true;
                open = true;
            }
            PathEl::ClosePath => open = false,
            _ => {}
        }
    }
    any && !open
}

impl App {
    /// The curves the Join tool works on: the visible, unlocked curves of
    /// the page's visible, unlocked layers (not B-splines), with their
    /// transforms.
    fn join_targets(&self) -> Vec<(ShapeId, BezPath, Affine)> {
        let Ok(pg) = self.doc().page(self.page) else {
            return Vec::new();
        };
        pg.layers
            .iter()
            .filter(|l| l.visible && !l.locked)
            .flat_map(|l| l.shapes.iter())
            .filter(|s| s.visible && !s.locked && s.spline.is_none())
            .filter_map(|s| match &s.kind {
                ShapeKind::Path { path, .. } => Some((s.id, path.clone(), s.transform)),
                _ => None,
            })
            .collect()
    }

    /// The end nodes of the curves the Join tool works on, on the page.
    pub fn join_ends_on_page(&self) -> Vec<JoinEnd> {
        self.join_targets()
            .into_iter()
            .flat_map(|(id, path, t)| {
                open_ends(&path).into_iter().map(move |(_, p)| JoinEnd {
                    shape: id,
                    at: t * p,
                })
            })
            .collect()
    }

    /// The pick distance, in page millimetres.
    fn join_reach(&self) -> f64 {
        (self.settings.nodes.pick_px() + 1.0) / self.view.zoom.max(1e-6) as f64
    }

    /// The end node nearest `p` within the pick distance.
    pub fn join_end_at(&self, p: Point) -> Option<JoinEnd> {
        let reach = self.join_reach();
        self.join_ends_on_page()
            .into_iter()
            .map(|e| (e, (e.at - p).hypot()))
            .filter(|(_, d)| *d <= reach)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(e, _)| e)
    }

    /// The end nodes a stroke (page points) passes over, in the order it
    /// reaches them.
    pub fn join_ends_along(&self, points: &[Point]) -> Vec<JoinEnd> {
        let reach = self.join_reach();
        let dist = |q: Point| -> (usize, f64) {
            let mut best = (usize::MAX, f64::INFINITY);
            for (k, w) in points.windows(2).enumerate() {
                let (a, b) = (w[0], w[1]);
                let ab = b - a;
                let len2 = ab.hypot2();
                let t = if len2 > 0.0 {
                    ((q - a).dot(ab) / len2).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let d = (a + ab * t - q).hypot();
                if d < best.1 {
                    best = (k, d);
                }
            }
            if points.len() == 1 {
                best = (0, (points[0] - q).hypot());
            }
            best
        };
        let mut hit: Vec<(usize, JoinEnd)> = self
            .join_ends_on_page()
            .into_iter()
            .filter_map(|e| {
                let (k, d) = dist(e.at);
                (d <= reach).then_some((k, e))
            })
            .collect();
        hit.sort_by_key(|(k, _)| *k);
        hit.into_iter().map(|(_, e)| e).collect()
    }

    /// Join the ends in pairs (first with second, third with fourth...),
    /// as one undo step. The number of joins made.
    pub fn join_end_pairs(&mut self, ends: &[JoinEnd]) -> usize {
        let fuse = self.join_reach();
        let mut work: HashMap<ShapeId, (BezPath, Affine)> = self
            .join_targets()
            .into_iter()
            .map(|(id, p, t)| (id, (p, t)))
            .collect();
        // Objects merged into another: where their ends went.
        let mut merged: HashMap<ShapeId, ShapeId> = HashMap::new();
        let mut deleted = Vec::new();
        let mut changed = Vec::new();
        let mut joins = 0;
        let owner = |merged: &HashMap<ShapeId, ShapeId>, mut id: ShapeId| {
            while let Some(n) = merged.get(&id) {
                id = *n;
            }
            id
        };
        for pair in ends.chunks_exact(2) {
            let (ea, eb) = (pair[0], pair[1]);
            let (a, b) = (owner(&merged, ea.shape), owner(&merged, eb.shape));
            if !work.contains_key(&a) || !work.contains_key(&b) {
                continue;
            }
            // Ends of two objects: the second joins the first's curve.
            if a != b {
                let Some((pb, tb)) = work.get(&b).cloned() else {
                    continue;
                };
                let Some((pa, ta)) = work.get_mut(&a) else {
                    continue;
                };
                if ta.determinant().abs() < 1e-12 {
                    continue;
                }
                pa.extend(ta.inverse() * tb * pb);
                work.remove(&b);
                merged.insert(b, a);
                deleted.push(b);
            }
            let Some((path, t)) = work.get(&a).cloned() else {
                continue;
            };
            let find = |at: Point| {
                open_ends(&path)
                    .into_iter()
                    .map(|(i, p)| (i, (t * p - at).hypot()))
                    .filter(|(_, d)| *d < 1e-6)
                    .min_by(|x, y| x.1.total_cmp(&y.1))
                    .map(|(i, _)| i)
            };
            let (Some(ia), Some(ib)) = (find(ea.at), find(eb.at)) else {
                continue;
            };
            let joined = if (ea.at - eb.at).hypot() <= fuse {
                nodes::join_ends(&path, ia, ib)
            } else {
                nodes::connect_ends(&path, ia, ib)
            };
            if let Some(j) = joined {
                work.insert(a, (j, t));
                if !changed.contains(&a) {
                    changed.push(a);
                }
                joins += 1;
            }
        }
        if joins == 0 {
            return 0;
        }
        let mut cmds: Vec<Command> = changed
            .iter()
            .filter_map(|id| {
                let (path, _) = work.get(id)?;
                Some(Command::SetShapeKind {
                    shape: *id,
                    kind: ShapeKind::Path {
                        closed: all_closed(path),
                        path: path.clone(),
                    },
                })
            })
            .collect();
        if !deleted.is_empty() {
            cmds.push(Command::DeleteShapes { shapes: deleted });
        }
        match self.engine.run_batch("Join Nodes", &cmds) {
            Ok(()) => {
                self.selection = changed;
                self.node_selection.clear();
                joins
            }
            Err(e) => {
                self.status = e.to_string();
                0
            }
        }
    }

    /// A stroke of the Join tool ended: join the ends it went over.
    pub fn join_along(&mut self, points: &[Point]) {
        self.join_pending = None;
        let ends = self.join_ends_along(points);
        if ends.len() < 2 {
            self.status = tr("status.join_none");
            return;
        }
        self.join_end_pairs(&ends);
    }

    /// A click of the Join tool: an end node is held, and a click on a
    /// second end node joins the two; a click away lets go.
    pub fn join_click(&mut self, p: Point) {
        let Some(end) = self.join_end_at(p) else {
            self.join_pending = None;
            return;
        };
        match self.join_pending.take() {
            Some(first) if (first.at - end.at).hypot() > 1e-9 || first.shape != end.shape => {
                self.join_end_pairs(&[first, end]);
            }
            Some(_) => {}
            None => self.join_pending = Some(end),
        }
    }

    /// Pointer input of the Join tool.
    pub fn join_tool_input(&mut self, response: &Response, p: Point) {
        if response.clicked_by(PointerButton::Primary) {
            self.join_click(p);
        }
        if response.drag_started_by(PointerButton::Primary) {
            self.drag = crate::app::Drag::Freehand { points: vec![p] };
        }
        if response.dragged_by(PointerButton::Primary) {
            let step = 1.0 / self.view.zoom.max(1e-6) as f64;
            if let crate::app::Drag::Freehand { points } = &mut self.drag {
                if points.last().is_none_or(|l| (*l - p).hypot() > step) {
                    points.push(p);
                }
            }
        }
    }
}

/// With the Join tool, the end nodes of the curves it can join, the one
/// held by a click filled in.
pub fn draw_join_ends(app: &App, painter: &Painter) {
    if app.tool != Tool::Join {
        return;
    }
    let blue = crate::theme::Tokens::SELECTION;
    for e in app.join_ends_on_page() {
        let c = app.view.to_screen(e.at);
        let held = app.join_pending == Some(e);
        let r = egui::Rect::from_center_size(c, egui::vec2(7.0, 7.0));
        painter.rect_filled(r, 0.0, if held { blue } else { Color32::WHITE });
        painter.rect_stroke(r, 0.0, EStroke::new(1.2, blue), egui::StrokeKind::Middle);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(app: &mut App, a: (f64, f64), b: (f64, f64)) -> ShapeId {
        let mut p = BezPath::new();
        p.move_to(a);
        p.line_to(b);
        app.new_shape(ShapeKind::Path {
            path: p,
            closed: false,
        })
        .expect("a line")
    }

    fn path_of(app: &App, id: ShapeId) -> (BezPath, bool) {
        match &app.doc().find_shape(id).expect("shape").kind {
            ShapeKind::Path { path, closed } => (path.clone(), *closed),
            k => panic!("{k:?}"),
        }
    }

    #[test]
    fn open_ends_lists_the_ends_of_open_subpaths_only() {
        let mut p = BezPath::new();
        p.move_to((0.0, 0.0));
        p.line_to((10.0, 0.0));
        p.move_to((0.0, 5.0));
        p.line_to((5.0, 5.0));
        p.line_to((5.0, 9.0));
        p.close_path();
        p.move_to((20.0, 0.0));
        p.curve_to((21.0, 1.0), (22.0, 1.0), (23.0, 0.0));
        let ends = open_ends(&p);
        let pts: Vec<Point> = ends.iter().map(|e| e.1).collect();
        assert_eq!(
            pts,
            vec![
                Point::new(0.0, 0.0),
                Point::new(10.0, 0.0),
                Point::new(20.0, 0.0),
                Point::new(23.0, 0.0)
            ]
        );
    }

    /// #2150: a stroke over the near ends of two lines makes one curve
    /// (the second line gone, one undo step); a click on each end of
    /// that curve closes it with a segment.
    #[test]
    fn a_stroke_joins_two_curves_and_two_clicks_close_one() {
        let mut app = App::headless();
        app.tool = Tool::Join;
        let a = line(&mut app, (0.0, 0.0), (40.0, 0.0));
        let b = line(&mut app, (40.2, 0.0), (40.0, 30.0));
        let depth = app.engine.history_labels().0.len();
        // A stroke across both near ends (0.2 mm apart: fused).
        app.join_along(&[Point::new(38.0, -3.0), Point::new(42.0, 3.0)]);
        assert!(app.doc().find_shape(b).is_none(), "the second line joined");
        let (p, closed) = path_of(&app, a);
        assert!(!closed);
        assert_eq!(open_ends(&p).len(), 2, "one open subpath");
        assert_eq!(p.elements().len(), 3, "the two ends became one node");
        assert_eq!(app.engine.history_labels().0.len(), depth + 1);
        // Click one end, then the other: a segment closes the triangle.
        app.join_click(Point::new(0.0, 0.0));
        assert!(app.join_pending.is_some());
        app.join_click(Point::new(40.0, 30.0));
        assert!(app.join_pending.is_none());
        let (p, closed) = path_of(&app, a);
        assert!(closed, "{p:?}");
        assert!(open_ends(&p).is_empty());
        // Undo puts both lines back.
        app.undo();
        app.undo();
        assert!(app.doc().find_shape(b).is_some());
    }

    /// Ends further apart than the pick distance get a straight segment
    /// between them; a stroke reaching one end only joins nothing.
    #[test]
    fn far_ends_get_a_segment_and_a_lone_end_joins_nothing() {
        let mut app = App::headless();
        app.tool = Tool::Join;
        let a = line(&mut app, (0.0, 0.0), (20.0, 0.0));
        let b = line(&mut app, (30.0, 0.0), (50.0, 0.0));
        app.join_along(&[Point::new(20.0, 0.0)]);
        assert!(app.doc().find_shape(b).is_some());
        app.join_along(&[
            Point::new(19.0, 1.0),
            Point::new(25.0, 1.0),
            Point::new(31.0, 1.0),
        ]);
        assert!(app.doc().find_shape(b).is_none());
        let (p, _) = path_of(&app, a);
        let ends: Vec<Point> = open_ends(&p).into_iter().map(|e| e.1).collect();
        assert_eq!(ends, vec![Point::new(0.0, 0.0), Point::new(50.0, 0.0)]);
        // Four elements: move, line, the joining segment, line.
        assert_eq!(p.elements().len(), 4);
    }
}
