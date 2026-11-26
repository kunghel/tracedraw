//! Shapes cut into equal parts and curves read back as ellipses:
//!
//! - Divide into Slices (the Ellipse bar, #4209): a selected ellipse, or
//!   pie, becomes that many pie wedges of equal angle, each an ellipse
//!   object of its own that keeps the look of the original (its pie and
//!   ring settings stay editable).
//! - Divide into Triangles (the Polygon and Star bars, #4210): a selected
//!   polygon or star becomes one triangle per side, each joining the side
//!   to the centre, as curves with the look of the original.
//! - Convert to Ellipse (Object menu, #4214): a closed curve that runs
//!   round an ellipse, such as a circle drawn as many short lines, becomes
//!   an ellipse object again.
//!
//! Each command is one undo step; the pieces become the selection, the
//! first one in the place of the original and the others right above it.

use crate::app::App;
use tracedraw_core::document::{EllipseArc, Shape, ShapeKind};
use tracedraw_core::geometry::{BezPath, ParamCurve, PathEl, Point, Rect, Shape as _};
use tracedraw_core::{Command, ShapeId};

/// The most slices or triangles one object is divided into.
pub const MAX_PARTS: u32 = 360;

/// The angles of `n` equal pie wedges over `arc` (the whole ellipse with
/// `None`), each keeping the arc's ring; empty when `arc` is an open arc
/// (it has no area to divide) or `n` is below 2.
pub fn slice_arcs(arc: Option<EllipseArc>, n: u32) -> Vec<EllipseArc> {
    if !(2..=MAX_PARTS).contains(&n) {
        return Vec::new();
    }
    let (start, span, inner) = match arc {
        None => (90.0, 360.0, 0.0),
        Some(a) if a.pie => {
            let span = (a.end_deg - a.start_deg).rem_euclid(360.0);
            let span = if span < 1e-9 { 360.0 } else { span };
            (a.start_deg, span, a.inner)
        }
        Some(_) => return Vec::new(),
    };
    if !start.is_finite() || !span.is_finite() {
        return Vec::new();
    }
    let step = span / n as f64;
    (0..n)
        .map(|i| EllipseArc {
            start_deg: start + step * i as f64,
            end_deg: start + step * (i + 1) as f64,
            pie: true,
            inner,
        })
        .collect()
}

/// The triangles that divide a straight-sided closed outline `path`: one
/// per side, joining it to the outline's centre (the middle of its box).
/// Empty when the outline is not one closed run of straight lines with
/// at least three sides.
pub fn fan_triangles(path: &BezPath) -> Vec<BezPath> {
    let mut pts: Vec<Point> = Vec::new();
    let mut moves = 0;
    for el in path.elements() {
        match *el {
            PathEl::MoveTo(p) => {
                moves += 1;
                pts.push(p);
            }
            PathEl::LineTo(p) => pts.push(p),
            PathEl::ClosePath => {}
            PathEl::QuadTo(..) | PathEl::CurveTo(..) => return Vec::new(),
        }
    }
    if moves != 1 {
        return Vec::new();
    }
    // The closing point repeats the first.
    while pts.len() > 1 && pts.last().is_some_and(|p| (*p - pts[0]).hypot() < 1e-9) {
        pts.pop();
    }
    if pts.len() < 3 || pts.len() > MAX_PARTS as usize {
        return Vec::new();
    }
    let c = path.bounding_box().center();
    let n = pts.len();
    (0..n)
        .filter_map(|i| {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            // A side through the centre makes no triangle.
            let area = (a - c).cross(b - c);
            (area.abs() > 1e-9).then(|| {
                let mut t = BezPath::new();
                t.move_to(c);
                t.line_to(a);
                t.line_to(b);
                t.close_path();
                t
            })
        })
        .collect()
}

/// The ellipse box a closed curve `path` runs round, when it does: its
/// points (nodes and points along every segment) lie within 1 % of the
/// radii from the ellipse that fills the curve's box, and the curve
/// goes all the way round (#4214). Only one closed run, with at least
/// four nodes.
pub fn ellipse_of(path: &BezPath) -> Option<Rect> {
    let moves = path
        .elements()
        .iter()
        .filter(|e| matches!(e, PathEl::MoveTo(_)))
        .count();
    let nodes = path
        .elements()
        .iter()
        .filter(|e| !matches!(e, PathEl::ClosePath))
        .count();
    if moves != 1 || nodes < 4 {
        return None;
    }
    let r = path.bounding_box();
    let (rx, ry) = (r.width() / 2.0, r.height() / 2.0);
    if !(rx > 1e-6 && ry > 1e-6) || !rx.is_finite() || !ry.is_finite() {
        return None;
    }
    let c = r.center();
    let mut turned = 0.0;
    let mut last: Option<f64> = None;
    let mut off = 0.0_f64;
    let mut seen = 0usize;
    // Every segment's start and points along it.
    let mut points: Vec<Point> = Vec::new();
    for seg in path.segments() {
        for k in 0..8 {
            points.push(seg.eval(k as f64 / 8.0));
        }
    }
    for p in points {
        let (dx, dy) = ((p.x - c.x) / rx, (p.y - c.y) / ry);
        off = off.max(((dx * dx + dy * dy).sqrt() - 1.0).abs());
        let a = dy.atan2(dx);
        if let Some(l) = last {
            let mut d = a - l;
            while d > std::f64::consts::PI {
                d -= std::f64::consts::TAU;
            }
            while d < -std::f64::consts::PI {
                d += std::f64::consts::TAU;
            }
            turned += d;
        }
        last = Some(a);
        seen += 1;
    }
    // Closed: back to the start.
    let (first, end) = (path.elements().first(), path.elements().last());
    let closed = matches!(end, Some(PathEl::ClosePath))
        || match (first, path.segments().last()) {
            (Some(PathEl::MoveTo(a)), Some(s)) => (s.eval(1.0) - *a).hypot() < 1e-6 * rx.max(ry),
            _ => false,
        };
    if !closed {
        return None;
    }
    // The closing line goes back to the first point too.
    if let (Some(PathEl::MoveTo(a)), Some(l)) = (first, last) {
        let a0 = ((a.y - c.y) / ry).atan2((a.x - c.x) / rx);
        let mut d = a0 - l;
        while d > std::f64::consts::PI {
            d -= std::f64::consts::TAU;
        }
        while d < -std::f64::consts::PI {
            d += std::f64::consts::TAU;
        }
        turned += d;
    }
    (seen >= 4 && off <= 0.01 && (turned.abs() - std::f64::consts::TAU).abs() < 0.1).then_some(r)
}

/// What a selected object becomes: the kinds of its pieces, first the one
/// that takes its place.
type Split = fn(&Shape, u32) -> Vec<ShapeKind>;

fn ellipse_slices(s: &Shape, n: u32) -> Vec<ShapeKind> {
    match &s.kind {
        ShapeKind::Ellipse { rect, arc } => slice_arcs(*arc, n)
            .into_iter()
            .map(|a| ShapeKind::Ellipse {
                rect: *rect,
                arc: Some(a),
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn polygon_triangles(s: &Shape, _n: u32) -> Vec<ShapeKind> {
    match &s.kind {
        ShapeKind::Polygon { .. } => fan_triangles(&s.local_path())
            .into_iter()
            .map(|path| ShapeKind::Path { path, closed: true })
            .collect(),
        _ => Vec::new(),
    }
}

fn curve_ellipse(s: &Shape, _n: u32) -> Vec<ShapeKind> {
    match &s.kind {
        ShapeKind::Path { path, closed: true } => ellipse_of(path)
            .map(|rect| vec![ShapeKind::Ellipse { rect, arc: None }])
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

impl App {
    /// Whether a selected object is an ellipse or pie Divide into Slices
    /// can divide.
    pub fn can_divide_slices(&self) -> bool {
        self.selected_shapes()
            .iter()
            .any(|s| !ellipse_slices(s, 2).is_empty())
    }

    /// Whether a selected object is a polygon or star with straight sides.
    pub fn can_divide_triangles(&self) -> bool {
        self.selected_shapes()
            .iter()
            .any(|s| !polygon_triangles(s, 0).is_empty())
    }

    /// Whether a selected curve runs round an ellipse (Convert to Ellipse).
    pub fn can_convert_to_ellipse(&self) -> bool {
        self.selected_shapes()
            .iter()
            .any(|s| !curve_ellipse(s, 0).is_empty())
    }

    /// Divide into Slices: every selected ellipse or pie into `n` equal
    /// wedges (#4209). The number of objects divided.
    pub fn divide_into_slices(&mut self, n: u32) -> usize {
        self.split_selected(ellipse_slices, n, "Divide into Slices")
    }

    /// Divide into Triangles: every selected polygon or star into one
    /// triangle per side, to its centre (#4210).
    pub fn divide_into_triangles(&mut self) -> usize {
        self.split_selected(polygon_triangles, 0, "Divide into Triangles")
    }

    /// Convert to Ellipse: every selected closed curve that runs round an
    /// ellipse becomes an ellipse object (#4214).
    pub fn convert_to_ellipse(&mut self) -> usize {
        self.split_selected(curve_ellipse, 0, "Convert to Ellipse")
    }

    /// The selected objects on a layer (not locked) that `split` turns
    /// into pieces become them: the first piece takes the object's place
    /// and id, the others go right above it with its look. One undo step;
    /// the pieces become the selection.
    fn split_selected(&mut self, split: Split, n: u32, label: &'static str) -> usize {
        let targets: Vec<(Shape, Vec<ShapeKind>)> = self
            .selected_shapes()
            .into_iter()
            .filter(|s| !s.locked && self.doc().locate(s.id).is_ok())
            .filter_map(|s| {
                let parts = split(&s, n);
                (!parts.is_empty()).then_some((s, parts))
            })
            .collect();
        if targets.is_empty() {
            return 0;
        }
        let mut cmds = Vec::new();
        let mut picked: Vec<ShapeId> = Vec::new();
        for (s, parts) in &targets {
            let Ok((layer, _)) = self.doc().locate(s.id) else {
                continue;
            };
            let mut parts = parts.iter();
            if let Some(first) = parts.next() {
                cmds.push(Command::SetShapeKind {
                    shape: s.id,
                    kind: first.clone(),
                });
                picked.push(s.id);
            }
            let mut added = Vec::new();
            for kind in parts {
                let id = self.engine.new_shape_id();
                let mut piece = s.clone();
                piece.id = id;
                piece.kind = kind.clone();
                // A name stays with the first piece only.
                piece.name = None;
                cmds.push(Command::AddShape {
                    layer,
                    shape: piece,
                });
                added.push(id);
            }
            if !added.is_empty() {
                cmds.push(Command::PlaceBeside {
                    shapes: added.clone(),
                    reference: s.id,
                    in_front: true,
                });
            }
            picked.extend(added);
        }
        if let Err(e) = self.engine.run_batch(label, &cmds) {
            self.status = e.to_string();
            return 0;
        }
        self.select(picked);
        targets.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::Fill;

    fn red() -> Fill {
        Fill::Solid(tracedraw_core::Color::rgb8(255, 0, 0))
    }

    /// A whole ellipse divides into equal wedges from the top, a pie into
    /// equal wedges of its own angle keeping its ring; an open arc and
    /// fewer than two slices give nothing (#4209).
    #[test]
    fn slices_share_the_angle_out() {
        let s = slice_arcs(None, 4);
        assert_eq!(s.len(), 4);
        assert!(s.iter().all(|a| a.pie && (a.end_deg - a.start_deg - 90.0).abs() < 1e-9));
        assert!((s[0].start_deg - 90.0).abs() < 1e-9 && (s[3].end_deg - 450.0).abs() < 1e-9);
        let pie = EllipseArc {
            start_deg: 0.0,
            end_deg: 90.0,
            pie: true,
            inner: 0.5,
        };
        let s = slice_arcs(Some(pie), 3);
        assert_eq!(s.len(), 3);
        assert!(s.iter().all(|a| (a.end_deg - a.start_deg - 30.0).abs() < 1e-9 && a.inner == 0.5));
        assert!(slice_arcs(Some(EllipseArc { pie: false, ..pie }), 3).is_empty());
        assert!(slice_arcs(None, 1).is_empty());
        assert!(slice_arcs(None, MAX_PARTS + 1).is_empty());
    }

    /// Divide into Slices on the page: the ellipse becomes 6 pie objects
    /// with its fill, the first keeping its id, all selected; one undo
    /// brings the ellipse back alone.
    #[test]
    fn an_ellipse_divides_into_pie_objects() {
        let mut app = App::headless();
        let before = app.selectable_ids().len();
        let id = app
            .new_shape_styled(
                ShapeKind::Ellipse {
                    rect: Rect::new(0.0, 0.0, 40.0, 40.0),
                    arc: None,
                },
                |s| s.fill = red(),
            )
            .expect("ellipse");
        app.select(vec![id]);
        assert!(app.can_divide_slices());
        assert_eq!(app.divide_into_slices(6), 1);
        assert_eq!(app.selection.len(), 6);
        assert_eq!(app.selection[0], id);
        for sid in &app.selection {
            let s = app.doc().find_shape(*sid).expect("slice");
            assert_eq!(s.fill, red());
            assert!(matches!(
                s.kind,
                ShapeKind::Ellipse {
                    arc: Some(EllipseArc { pie: true, .. }),
                    ..
                }
            ));
        }
        assert_eq!(app.selectable_ids().len(), before + 6);
        app.undo();
        assert_eq!(app.selectable_ids().len(), before + 1);
        assert!(matches!(
            app.doc().find_shape(id).expect("back").kind,
            ShapeKind::Ellipse { arc: None, .. }
        ));
    }

    /// A hexagon divides into six triangles meeting at its centre, which
    /// together cover its area; a star into one per side (#4210).
    #[test]
    fn a_polygon_divides_into_triangles() {
        let mut app = App::headless();
        let id = app
            .new_shape(ShapeKind::Polygon {
                rect: Rect::new(0.0, 0.0, 60.0, 60.0),
                points: 6,
                sharpness: 0.0,
                complex: None,
                twist: 0.0,
                bend: 0.0,
            })
            .expect("hexagon");
        let area = app.doc().find_shape(id).expect("hex").local_path().area().abs();
        app.select(vec![id]);
        assert!(app.can_divide_triangles());
        assert_eq!(app.divide_into_triangles(), 1);
        assert_eq!(app.selection.len(), 6);
        let total: f64 = app
            .selection
            .iter()
            .map(|t| app.doc().find_shape(*t).expect("t").local_path().area().abs())
            .sum();
        assert!((total - area).abs() < 1e-6 * area, "{total} vs {area}");
        let star = app
            .new_shape(ShapeKind::Polygon {
                rect: Rect::new(100.0, 0.0, 160.0, 60.0),
                points: 5,
                sharpness: 0.5,
                complex: None,
                twist: 0.0,
                bend: 0.0,
            })
            .expect("star");
        app.select(vec![star]);
        assert_eq!(app.divide_into_triangles(), 1);
        assert_eq!(app.selection.len(), 10);
    }

    /// A circle drawn as 64 short lines (as plans bring them) becomes an
    /// ellipse object of the same box; a square, an open run of lines and
    /// a half circle do not (#4214).
    #[test]
    fn a_segmented_circle_converts_back_to_an_ellipse() {
        let circle = |n: usize, open: bool, sweep: f64| {
            let mut p = BezPath::new();
            for i in 0..n {
                let a = sweep * i as f64 / n as f64;
                let q = Point::new(50.0 + 20.0 * a.cos(), 30.0 + 10.0 * a.sin());
                if i == 0 {
                    p.move_to(q);
                } else {
                    p.line_to(q);
                }
            }
            if !open {
                p.close_path();
            }
            p
        };
        let r = ellipse_of(&circle(64, false, std::f64::consts::TAU)).expect("ellipse");
        assert!((r.width() - 40.0).abs() < 0.1 && (r.height() - 20.0).abs() < 0.1, "{r:?}");
        assert!(ellipse_of(&circle(64, true, std::f64::consts::TAU)).is_none(), "open");
        assert!(ellipse_of(&circle(64, false, std::f64::consts::PI)).is_none(), "half");
        let mut sq = BezPath::new();
        sq.move_to((0.0, 0.0));
        sq.line_to((10.0, 0.0));
        sq.line_to((10.0, 10.0));
        sq.line_to((0.0, 10.0));
        sq.close_path();
        assert!(ellipse_of(&sq).is_none(), "a square is no ellipse");

        let mut app = App::headless();
        let id = app
            .new_shape(ShapeKind::Path {
                path: circle(64, false, std::f64::consts::TAU),
                closed: true,
            })
            .expect("curve");
        app.select(vec![id]);
        assert!(app.can_convert_to_ellipse());
        assert_eq!(app.convert_to_ellipse(), 1);
        assert!(matches!(
            app.doc().find_shape(id).expect("e").kind,
            ShapeKind::Ellipse { arc: None, .. }
        ));
        assert_eq!(app.selection, vec![id]);
    }
}
