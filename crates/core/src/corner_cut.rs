//! Fillet, scallop and chamfer the corners of curves (the Corners
//! panel). A corner is a node where two segments meet
//! at an angle; smooth and symmetrical nodes are not corners. Corners are
//! cut in path order, and one whose segments are too short for the cut,
//! after the cuts already made on them, is left as it is.

use crate::geometry::{
    BezPath, CubicBez, Line, ParamCurve, ParamCurveArclen, PathEl, PathSeg, Point, Vec2,
};

/// How a corner is cut.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CornerCut {
    /// A circular arc of this radius, tangent to both segments.
    Fillet(f64),
    /// A circular notch of this radius centred on the corner.
    Scallop(f64),
    /// A straight cut starting `a` back along the segment that comes into
    /// the corner and ending `b` along the one that leaves it (the order
    /// the path is drawn in).
    Chamfer(f64, f64),
}

/// One subpath: its segments with the element index of each segment's
/// end node, the index of its move-to, whether it is closed and whether
/// its last segment is the closing line (drawn by the close itself).
struct Sub {
    segs: Vec<(PathSeg, usize)>,
    start_index: usize,
    closed: bool,
    closing_line: bool,
}

fn subpaths(path: &BezPath) -> Vec<Sub> {
    let path = crate::nodes::normalize(path);
    let mut out: Vec<Sub> = Vec::new();
    let mut cur: Option<Sub> = None;
    let (mut start, mut last) = (Point::ZERO, Point::ZERO);
    for (i, el) in path.elements().iter().enumerate() {
        match *el {
            PathEl::MoveTo(p) => {
                if let Some(s) = cur.take() {
                    out.push(s);
                }
                cur = Some(Sub {
                    segs: Vec::new(),
                    start_index: i,
                    closed: false,
                    closing_line: false,
                });
                start = p;
                last = p;
            }
            PathEl::LineTo(p) => {
                if let Some(s) = cur.as_mut() {
                    if (p - last).hypot() > 1e-12 {
                        s.segs.push((PathSeg::Line(Line::new(last, p)), i));
                    }
                }
                last = p;
            }
            PathEl::CurveTo(c1, c2, p) => {
                if let Some(s) = cur.as_mut() {
                    let degenerate = [c1, c2, p].iter().all(|q| (*q - last).hypot() < 1e-12);
                    if !degenerate {
                        s.segs
                            .push((PathSeg::Cubic(CubicBez::new(last, c1, c2, p)), i));
                    }
                }
                last = p;
            }
            PathEl::QuadTo(..) => {}
            PathEl::ClosePath => {
                if let Some(s) = cur.as_mut() {
                    if (last - start).hypot() > 1e-12 {
                        let index = s.start_index;
                        s.segs.push((PathSeg::Line(Line::new(last, start)), index));
                        s.closing_line = true;
                    }
                    s.closed = true;
                }
                last = start;
            }
        }
    }
    if let Some(s) = cur {
        out.push(s);
    }
    out
}

fn unit(v: Vec2) -> Option<Vec2> {
    let l = v.hypot();
    (l > 1e-12 && l.is_finite()).then(|| v / l)
}

/// Direction of travel at the start of a segment.
fn start_dir(seg: &PathSeg) -> Option<Vec2> {
    match seg {
        PathSeg::Line(l) => unit(l.p1 - l.p0),
        PathSeg::Quad(q) => unit(q.p1 - q.p0).or_else(|| unit(q.p2 - q.p0)),
        PathSeg::Cubic(c) => unit(c.p1 - c.p0)
            .or_else(|| unit(c.p2 - c.p0))
            .or_else(|| unit(c.p3 - c.p0)),
    }
}

/// Direction of travel at the end of a segment.
fn end_dir(seg: &PathSeg) -> Option<Vec2> {
    match seg {
        PathSeg::Line(l) => unit(l.p1 - l.p0),
        PathSeg::Quad(q) => unit(q.p2 - q.p1).or_else(|| unit(q.p2 - q.p0)),
        PathSeg::Cubic(c) => unit(c.p3 - c.p2)
            .or_else(|| unit(c.p3 - c.p1))
            .or_else(|| unit(c.p3 - c.p0)),
    }
}

/// Direction of travel at parameter `t`.
fn dir_at(seg: &PathSeg, t: f64) -> Option<Vec2> {
    let d = match seg {
        PathSeg::Line(l) => l.p1 - l.p0,
        PathSeg::Quad(q) => {
            let mt = 1.0 - t;
            ((q.p1 - q.p0) * mt + (q.p2 - q.p1) * t) * 2.0
        }
        PathSeg::Cubic(c) => {
            let mt = 1.0 - t;
            ((c.p1 - c.p0) * (mt * mt) + (c.p2 - c.p1) * (2.0 * mt * t) + (c.p3 - c.p2) * (t * t))
                * 3.0
        }
    };
    unit(d).or_else(|| {
        if t < 0.5 {
            start_dir(seg)
        } else {
            end_dir(seg)
        }
    })
}

const ACCURACY: f64 = 1e-6;

/// Parameter of the point `s` along the segment (arc length).
fn param_at(seg: &PathSeg, len: f64, s: f64) -> f64 {
    if s <= 0.0 {
        return 0.0;
    }
    if s >= len {
        return 1.0;
    }
    match seg {
        PathSeg::Line(_) => s / len,
        _ => seg.inv_arclen(s, ACCURACY),
    }
}

/// The elements of a circular arc around `center` from `from` turning by
/// `sweep` radians, ending exactly on `to`.
fn arc_to(out: &mut Vec<PathEl>, center: Point, from: Point, to: Point, sweep: f64) {
    let r = (from - center).hypot();
    if r < 1e-12 || sweep.abs() < 1e-9 {
        out.push(PathEl::LineTo(to));
        return;
    }
    let start = (from - center).atan2();
    let arc = crate::geometry::Arc {
        center,
        radii: Vec2::new(r, r),
        start_angle: start,
        sweep_angle: sweep,
        x_rotation: 0.0,
    };
    // One cubic per quarter turn or so, as a drawn corner has.
    let mut els: Vec<PathEl> = arc.append_iter((r * 1e-3).max(1e-9)).collect();
    // The arc ends where the cut meets the next segment.
    if let Some(last) = els.last_mut() {
        match last {
            PathEl::CurveTo(_, c2, p) => {
                let d = to - *p;
                *c2 += d;
                *p = to;
            }
            PathEl::LineTo(p) | PathEl::QuadTo(_, p) => *p = to,
            _ => {}
        }
    }
    if els.is_empty() {
        els.push(PathEl::LineTo(to));
    }
    out.extend(els);
}

/// Signed angle (radians) that turns `a` onto `b`, in (-pi, pi].
fn turn(a: Vec2, b: Vec2) -> f64 {
    a.cross(b).atan2(a.dot(b))
}

/// A corner of a curve, for the Shape tool's corner widgets: the node
/// (element index, as the Shape tool counts nodes), where it is, the
/// unit direction from it into the corner (halfway between its two
/// sides) and the angle between the sides (radians).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Corner {
    pub node: usize,
    pub at: Point,
    pub inside: Vec2,
    pub theta: f64,
}

/// The corners of `path` that [`cut_corners`] would cut, in path order.
/// Cheap: no lengths are measured.
pub fn corners(path: &BezPath) -> Vec<Corner> {
    let mut out = Vec::new();
    for sub in subpaths(path) {
        let n = sub.segs.len();
        if n == 0 {
            continue;
        }
        let count = if sub.closed { n } else { n - 1 };
        for k in 0..count {
            let (seg_in, index) = &sub.segs[k];
            let (seg_out, _) = &sub.segs[(k + 1) % n];
            let (Some(u_in), Some(u_out)) = (end_dir(seg_in), start_dir(seg_out)) else {
                continue;
            };
            let theta = (-u_in).dot(u_out).clamp(-1.0, 1.0).acos();
            if !(1e-3..=std::f64::consts::PI - 1e-3).contains(&theta) {
                continue;
            }
            let Some(inside) = unit(u_out - u_in) else {
                continue;
            };
            let at_start = sub.closed && k == n - 1;
            out.push(Corner {
                node: if at_start { sub.start_index } else { *index },
                at: seg_in.eval(1.0),
                inside,
                theta,
            });
        }
    }
    out
}

/// The largest fillet radius every chosen corner of `path` takes (every
/// corner without `only`), the sides two chosen corners share split
/// between them; `None` when no corner is chosen.
pub fn max_fillet(path: &BezPath, only: Option<&[usize]>) -> Option<f64> {
    let mut best: Option<f64> = None;
    for sub in subpaths(path) {
        let n = sub.segs.len();
        if n == 0 {
            continue;
        }
        let count = if sub.closed { n } else { n - 1 };
        let lens: Vec<f64> = sub.segs.iter().map(|(s, _)| s.arclen(ACCURACY)).collect();
        // Corner k sits after segment k: its angle when it is a chosen
        // corner.
        let angle: Vec<Option<f64>> = (0..n)
            .map(|k| {
                if k >= count {
                    return None;
                }
                let (seg_in, index) = &sub.segs[k];
                let at_start = sub.closed && k == n - 1;
                if let Some(only) = only {
                    if !(only.contains(index) || (at_start && only.contains(&sub.start_index))) {
                        return None;
                    }
                }
                let (seg_out, _) = &sub.segs[(k + 1) % n];
                let (u_in, u_out) = (end_dir(seg_in)?, start_dir(seg_out)?);
                let theta = (-u_in).dot(u_out).clamp(-1.0, 1.0).acos();
                (1e-3..=std::f64::consts::PI - 1e-3)
                    .contains(&theta)
                    .then_some(theta)
            })
            .collect();
        for k in 0..count {
            let Some(theta) = angle[k] else {
                continue;
            };
            let out_k = (k + 1) % n;
            // The corner before segment k and the one after segment out_k.
            let before = if k > 0 {
                angle[k - 1].is_some()
            } else {
                sub.closed && angle[n - 1].is_some()
            };
            let after = angle[out_k].is_some() && (out_k < count);
            let room_in = if before { lens[k] / 2.0 } else { lens[k] };
            let room_out = if after {
                lens[out_k] / 2.0
            } else {
                lens[out_k]
            };
            let r = room_in.min(room_out) * (theta / 2.0).tan();
            best = Some(best.map_or(r, |b: f64| b.min(r)));
        }
    }
    best
}

/// `path` with its corners cut. With `only`, just the corners at those/// `path` with its corners cut. With `only`, just the corners at those
/// nodes (element indices, as the Shape tool counts them); otherwise every
/// corner. Returns the new path and the number of corners cut.
pub fn cut_corners(path: &BezPath, cut: CornerCut, only: Option<&[usize]>) -> (BezPath, usize) {
    let (t_in_of, t_out_of) = match cut {
        CornerCut::Fillet(r) | CornerCut::Scallop(r) => (r, r),
        CornerCut::Chamfer(a, b) => (a, b),
    };
    let mut out = BezPath::new();
    let mut count = 0;
    if !(t_in_of.is_finite() && t_out_of.is_finite()) || t_in_of <= 0.0 || t_out_of <= 0.0 {
        return (path.clone(), 0);
    }
    for sub in subpaths(path) {
        let n = sub.segs.len();
        if n == 0 {
            continue;
        }
        let lens: Vec<f64> = sub.segs.iter().map(|(s, _)| s.arclen(ACCURACY)).collect();
        let mut start_trim = vec![0.0; n];
        let mut end_trim = vec![0.0; n];
        // The piece that replaces the corner after segment k, if cut.
        let mut pieces: Vec<Option<(Point, Point, Vec<PathEl>)>> = vec![None; n];
        let corners = if sub.closed { n } else { n - 1 };
        for k in 0..corners {
            let (seg_in, index) = &sub.segs[k];
            let out_k = (k + 1) % n;
            let (seg_out, _) = &sub.segs[out_k];
            let at_start = sub.closed && k == n - 1;
            if let Some(only) = only {
                let chosen = only.contains(index) || (at_start && only.contains(&sub.start_index));
                if !chosen {
                    continue;
                }
            }
            let (Some(u_in), Some(u_out)) = (end_dir(seg_in), start_dir(seg_out)) else {
                continue;
            };
            // Interior angle between the two segments seen from the corner.
            let theta = (-u_in).dot(u_out).clamp(-1.0, 1.0).acos();
            if !(1e-3..=std::f64::consts::PI - 1e-3).contains(&theta) {
                continue;
            }
            let (t_in, t_out) = match cut {
                CornerCut::Fillet(r) => {
                    let t = r / (theta / 2.0).tan();
                    (t, t)
                }
                _ => (t_in_of, t_out_of),
            };
            let eps = 1e-9;
            if start_trim[k] + t_in > lens[k] + eps || end_trim[out_k] + t_out > lens[out_k] + eps {
                continue;
            }
            if k == out_k && start_trim[k] + t_in + t_out > lens[k] + eps {
                continue;
            }
            let ti = param_at(seg_in, lens[k], lens[k] - t_in);
            let to = param_at(seg_out, lens[out_k], t_out);
            let p_in = seg_in.eval(ti);
            let p_out = seg_out.eval(to);
            let corner = seg_in.eval(1.0);
            let mut els = Vec::new();
            match cut {
                CornerCut::Chamfer(..) => els.push(PathEl::LineTo(p_out)),
                CornerCut::Fillet(_) => {
                    let (Some(d_in), Some(d_out)) = (dir_at(seg_in, ti), dir_at(seg_out, to))
                    else {
                        continue;
                    };
                    let sweep = turn(d_in, d_out);
                    // The circle tangent to the incoming side at the cut,
                    // on the side the path turns to.
                    let normal = if sweep >= 0.0 {
                        Vec2::new(-d_in.y, d_in.x)
                    } else {
                        Vec2::new(d_in.y, -d_in.x)
                    };
                    let half = sweep.abs() / 2.0;
                    let chord = (p_out - p_in).hypot();
                    let r = if half.sin() > 1e-9 {
                        chord / (2.0 * half.sin())
                    } else {
                        chord
                    };
                    arc_to(&mut els, p_in + normal * r, p_in, p_out, sweep);
                }
                CornerCut::Scallop(_) => {
                    let sweep = turn(p_in - corner, p_out - corner);
                    arc_to(&mut els, corner, p_in, p_out, sweep);
                }
            }
            end_trim[k] = t_in;
            start_trim[out_k] = t_out;
            pieces[k] = Some((p_in, p_out, els));
            count += 1;
        }
        // Rebuild: each segment between its trims, then the piece that
        // replaces the corner after it.
        let first_t = param_at(&sub.segs[0].0, lens[0], start_trim[0]);
        out.move_to(sub.segs[0].0.eval(first_t));
        for k in 0..n {
            let (seg, _) = &sub.segs[k];
            let t0 = param_at(seg, lens[k], start_trim[k]);
            let t1 = param_at(seg, lens[k], lens[k] - end_trim[k]);
            // The close draws the closing line when nothing follows it.
            let closes = sub.closing_line && k == n - 1 && pieces[k].is_none();
            if t1 - t0 > 1e-9 && !closes {
                match seg.subsegment(t0..t1) {
                    PathSeg::Line(l) => out.line_to(l.p1),
                    PathSeg::Quad(q) => out.quad_to(q.p1, q.p2),
                    PathSeg::Cubic(c) => out.curve_to(c.p1, c.p2, c.p3),
                }
            }
            if let Some((_, _, els)) = &pieces[k] {
                for el in els {
                    out.push(*el);
                }
            }
        }
        if sub.closed {
            out.close_path();
        }
    }
    (out, count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Rect, Shape as _};

    /// A square's four corners point inwards at right angles; an open
    /// zigzag has corners at its middle nodes only; the largest fillet
    /// fits the shared sides.
    #[test]
    fn corners_and_the_largest_fillet_they_take() {
        let sq = Rect::new(0.0, 0.0, 10.0, 10.0).to_path(0.1);
        let c = corners(&sq);
        assert_eq!(c.len(), 4, "{c:?}");
        for k in &c {
            assert!((k.theta - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
            // Into the square, towards its centre.
            let to_centre = Point::new(5.0, 5.0) - k.at;
            assert!(k.inside.dot(to_centre) > 0.0, "{k:?}");
        }
        // Every corner: half of each 10 mm side, at 90 degrees.
        assert!((max_fillet(&sq, None).unwrap() - 5.0).abs() < 1e-6);
        // One corner alone takes the whole side.
        let one = [c[1].node];
        assert!((max_fillet(&sq, Some(&one)).unwrap() - 10.0).abs() < 1e-6);
        let mut zig = BezPath::new();
        zig.move_to((0.0, 0.0));
        zig.line_to((10.0, 0.0));
        zig.line_to((10.0, 10.0));
        zig.line_to((20.0, 10.0));
        let c = corners(&zig);
        assert_eq!(c.iter().map(|k| k.node).collect::<Vec<_>>(), [1, 2]);
        assert_eq!(max_fillet(&zig, Some(&[])), None);
        // The fillet at that radius cuts every corner.
        let r = max_fillet(&sq, None).unwrap();
        assert_eq!(cut_corners(&sq, CornerCut::Fillet(r * 0.999), None).1, 4);
    }

    fn square() -> BezPath {
        Rect::new(0.0, 0.0, 10.0, 10.0).to_path(0.01)
    }

    #[test]
    fn fillets_scallops_and_chamfers_cut_the_expected_area() {
        let pi = std::f64::consts::PI;
        for (cut, area) in [
            (CornerCut::Fillet(2.0), 100.0 - (4.0 - pi) * 4.0),
            (CornerCut::Scallop(2.0), 100.0 - pi * 4.0),
            (CornerCut::Chamfer(2.0, 2.0), 92.0),
            (CornerCut::Chamfer(1.0, 3.0), 94.0),
        ] {
            let (p, n) = cut_corners(&square(), cut, None);
            assert_eq!(n, 4, "{cut:?}");
            assert!(
                (p.area().abs() - area).abs() < 0.01,
                "{cut:?}: {}",
                p.area()
            );
            let b = p.bounding_box();
            assert!((b.width() - 10.0).abs() < 1e-6, "{cut:?}: {b:?}");
            // Each corner becomes two nodes; the last cut ends on the
            // start node, which counts once.
            assert_eq!(crate::nodes::nodes(&p).len(), 8, "{cut:?}: {p:?}");
        }
        // Fillets meet the sides smoothly: each arc leaves the side before
        // it along that side and arrives along the side after it.
        let (p, _) = cut_corners(&square(), CornerCut::Fillet(2.0), None);
        let els = p.elements();
        let mut checked = 0;
        for w in els.windows(3) {
            if let (PathEl::LineTo(a), PathEl::CurveTo(c1, c2, b), PathEl::LineTo(next)) =
                (w[0], w[1], w[2])
            {
                let prev = match els.iter().position(|e| *e == w[0]) {
                    Some(i) if i > 0 => match els[i - 1] {
                        PathEl::MoveTo(q) | PathEl::LineTo(q) | PathEl::CurveTo(_, _, q) => q,
                        _ => continue,
                    },
                    _ => continue,
                };
                assert!((a - prev).cross(c1 - a).abs() < 1e-9, "{p:?}");
                assert!((next - b).cross(b - c2).abs() < 1e-9, "{p:?}");
                checked += 1;
            }
        }
        assert!(checked >= 3, "{p:?}");
    }

    #[test]
    fn corners_whose_sides_are_too_short_are_skipped() {
        // A 10 x 4 rectangle with 3 mm fillets: every second corner fits.
        let p = Rect::new(0.0, 0.0, 10.0, 4.0).to_path(0.01);
        let (_, n) = cut_corners(&p, CornerCut::Fillet(3.0), None);
        assert_eq!(n, 2);
        let (_, n) = cut_corners(&p, CornerCut::Fillet(50.0), None);
        assert_eq!(n, 0);
    }

    #[test]
    fn smooth_nodes_are_not_corners() {
        let circle = crate::geometry::Circle::new((0.0, 0.0), 5.0).to_path(0.01);
        let (p, n) = cut_corners(&circle, CornerCut::Fillet(1.0), None);
        assert_eq!(n, 0);
        assert!((p.area() - circle.area()).abs() < 1e-6);
    }

    #[test]
    fn only_the_chosen_nodes_are_cut() {
        let p = square();
        // Element 1 ends at the second corner.
        let (q, n) = cut_corners(&p, CornerCut::Chamfer(2.0, 2.0), Some(&[1]));
        assert_eq!(n, 1);
        assert!((q.area().abs() - 98.0).abs() < 1e-9);
        // The start node of a closed path is chosen by its move-to.
        let (_, n) = cut_corners(&p, CornerCut::Chamfer(2.0, 2.0), Some(&[0]));
        assert_eq!(n, 1);
        let (_, n) = cut_corners(&p, CornerCut::Chamfer(2.0, 2.0), Some(&[]));
        assert_eq!(n, 0);
    }

    #[test]
    fn open_curves_keep_their_ends_and_curved_sides_are_filleted() {
        let mut p = BezPath::new();
        p.move_to((0.0, 0.0));
        p.line_to((10.0, 0.0));
        p.curve_to((12.0, 3.0), (12.0, 7.0), (10.0, 10.0));
        let (q, n) = cut_corners(&p, CornerCut::Fillet(1.0), None);
        assert_eq!(n, 1);
        let els = q.elements();
        assert_eq!(els.first(), Some(&PathEl::MoveTo(Point::new(0.0, 0.0))));
        match els.last() {
            Some(PathEl::CurveTo(_, _, end)) => {
                assert!((*end - Point::new(10.0, 10.0)).hypot() < 1e-9)
            }
            other => panic!("{other:?}"),
        }
        // The fillet leaves the straight side along its direction.
        let nodes = crate::nodes::nodes(&q);
        assert!(nodes.len() > 3);
    }

    #[test]
    fn bad_values_change_nothing() {
        for cut in [
            CornerCut::Fillet(0.0),
            CornerCut::Fillet(f64::NAN),
            CornerCut::Scallop(-1.0),
            CornerCut::Chamfer(1.0, f64::INFINITY),
        ] {
            let (p, n) = cut_corners(&square(), cut, None);
            assert_eq!(n, 0);
            assert_eq!(p, square());
        }
        let (p, n) = cut_corners(&BezPath::new(), CornerCut::Fillet(1.0), None);
        assert_eq!(n, 0);
        assert!(p.elements().is_empty());
    }
}
