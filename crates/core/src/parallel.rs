//! Parallel curves: a line that runs alongside a path at a set distance,
//! for drawing several lines at once (Parallel Lines on the curve tools'
//! property bar). Unlike an offset of an area, an open path stays open:
//! each segment is moved sideways, curves by an approximation that keeps
//! the distance, and the pieces are joined at the corners, out to a point
//! where the corner opens and straight across where it closes.

use crate::geometry::{BezPath, Point, Vec2};
use kurbo::{CubicBez, ParamCurve, ParamCurveDeriv, PathEl};

/// How close to the requested distance a moved curve keeps, mm.
const TOLERANCE: f64 = 0.01;

/// A corner is carried out to a point when the point lies at most this
/// many distances from the corner, else it is cut straight across.
const MITER_LIMIT: f64 = 4.0;

/// One piece of a subpath: a straight segment or a cubic.
#[derive(Clone, Copy)]
enum Piece {
    Line(Point, Point),
    Cubic(CubicBez),
}

impl Piece {
    fn start_tangent(&self) -> Vec2 {
        match *self {
            Piece::Line(a, b) => b - a,
            Piece::Cubic(c) => {
                let d = c.deriv().eval(0.0).to_vec2();
                if d.hypot() > 1e-12 {
                    d
                } else {
                    c.p3 - c.p0
                }
            }
        }
    }

    fn end_tangent(&self) -> Vec2 {
        match *self {
            Piece::Line(a, b) => b - a,
            Piece::Cubic(c) => {
                let d = c.deriv().eval(1.0).to_vec2();
                if d.hypot() > 1e-12 {
                    d
                } else {
                    c.p3 - c.p0
                }
            }
        }
    }
}

/// A cubic whose control points all lie on the line between its ends.
fn is_straight(c: &CubicBez) -> bool {
    let chord = c.p3 - c.p0;
    let len = chord.hypot();
    if len < 1e-12 {
        return (c.p1 - c.p0).hypot() < 1e-9 && (c.p2 - c.p0).hypot() < 1e-9;
    }
    let off = |p: Point| (chord.cross(p - c.p0) / len).abs();
    off(c.p1) < 1e-9 && off(c.p2) < 1e-9
}

/// The left-hand normal (unit) of direction `d`.
fn normal(d: Vec2) -> Vec2 {
    let n = d.hypot();
    if n < 1e-12 {
        Vec2::ZERO
    } else {
        Vec2::new(-d.y / n, d.x / n)
    }
}

/// The pieces of each subpath of `path`, with whether it is closed.
fn subpaths(path: &BezPath) -> Vec<(Vec<Piece>, bool)> {
    let mut out: Vec<(Vec<Piece>, bool)> = Vec::new();
    let mut cur: Vec<Piece> = Vec::new();
    let (mut start, mut last) = (Point::ZERO, Point::ZERO);
    let flush = |cur: &mut Vec<Piece>, closed: bool, out: &mut Vec<(Vec<Piece>, bool)>| {
        if !cur.is_empty() {
            out.push((std::mem::take(cur), closed));
        }
    };
    for el in path.elements() {
        match *el {
            PathEl::MoveTo(p) => {
                flush(&mut cur, false, &mut out);
                start = p;
                last = p;
            }
            PathEl::LineTo(p) => {
                if (p - last).hypot() > 1e-12 {
                    cur.push(Piece::Line(last, p));
                }
                last = p;
            }
            PathEl::QuadTo(a, p) => {
                let c = kurbo::QuadBez::new(last, a, p).raise();
                cur.push(if is_straight(&c) {
                    Piece::Line(last, p)
                } else {
                    Piece::Cubic(c)
                });
                last = p;
            }
            PathEl::CurveTo(a, b, p) => {
                let c = CubicBez::new(last, a, b, p);
                if (p - last).hypot() > 1e-12 || !is_straight(&c) {
                    cur.push(if is_straight(&c) {
                        Piece::Line(last, p)
                    } else {
                        Piece::Cubic(c)
                    });
                }
                last = p;
            }
            PathEl::ClosePath => {
                if (last - start).hypot() > 1e-12 {
                    cur.push(Piece::Line(last, start));
                }
                flush(&mut cur, true, &mut out);
                last = start;
            }
        }
    }
    flush(&mut cur, false, &mut out);
    out
}

/// The elements (after the first point) of `piece` moved `d` to its left.
fn moved(piece: &Piece, d: f64) -> Vec<PathEl> {
    match *piece {
        Piece::Line(a, b) => {
            let n = normal(b - a) * d;
            vec![PathEl::MoveTo(a + n), PathEl::LineTo(b + n)]
        }
        Piece::Cubic(c) => {
            let mut out = BezPath::new();
            kurbo::offset::offset_cubic(c, d, TOLERANCE, &mut out);
            out.elements().to_vec()
        }
    }
}

/// The point where the line through `a` along `ta` meets the line
/// through `b` along `tb`, if they are not parallel.
fn meet(a: Point, ta: Vec2, b: Point, tb: Vec2) -> Option<Point> {
    let den = ta.cross(tb);
    if den.abs() < 1e-12 {
        return None;
    }
    let t = (b - a).cross(tb) / den;
    Some(a + ta * t)
}

/// `path` moved `d` mm to the left of its direction (to the right when
/// negative): every subpath a line alongside it at that distance, open
/// ones open and closed ones closed.
pub fn parallel_path(path: &BezPath, d: f64) -> BezPath {
    let mut out = BezPath::new();
    if !d.is_finite() {
        return out;
    }
    for (pieces, closed) in subpaths(path) {
        let mut moved_pieces: Vec<Vec<PathEl>> = pieces.iter().map(|p| moved(p, d)).collect();
        let n = pieces.len();
        // Where two straight pieces close in on each other at a corner,
        // both end where they cross instead of overlapping.
        for i in 0..n {
            if i == 0 && !closed {
                continue;
            }
            let prev = if i == 0 { n - 1 } else { i - 1 };
            if prev == i {
                continue;
            }
            let (Piece::Line(..), Piece::Line(..)) = (pieces[prev], pieces[i]) else {
                continue;
            };
            let a = end_point(&moved_pieces[prev]);
            let b = start_point(&moved_pieces[i]);
            let (ta, tb) = (pieces[prev].end_tangent(), pieces[i].start_tangent());
            if let Some(x) = meet(a, ta, b, tb) {
                let a0 = start_point(&moved_pieces[prev]);
                let b1 = end_point(&moved_pieces[i]);
                // Behind the end of the one and past the start of the
                // other, inside both.
                if ta.dot(x - a) < 0.0
                    && tb.dot(b - x) < 0.0
                    && ta.dot(x - a0) > 0.0
                    && tb.dot(b1 - x) > 0.0
                {
                    if let Some(PathEl::LineTo(e)) = moved_pieces[prev].last_mut() {
                        *e = x;
                    }
                    if let Some(PathEl::MoveTo(s0)) = moved_pieces[i].first_mut() {
                        *s0 = x;
                    }
                }
            }
        }
        // The corner before piece i: where the moved piece before it ends
        // and this one starts.
        let corner = |i: usize| -> Vec<Point> {
            let prev = if i == 0 { n - 1 } else { i - 1 };
            let a = end_point(&moved_pieces[prev]);
            let b = start_point(&moved_pieces[i]);
            if (a - b).hypot() < 1e-9 {
                return Vec::new();
            }
            let ta = pieces[prev].end_tangent();
            let tb = pieces[i].start_tangent();
            match meet(a, ta, b, tb) {
                // Ahead of the end and before the start: the corner opens.
                Some(x)
                    if ta.dot(x - a) > 0.0
                        && tb.dot(b - x) > 0.0
                        && (x - a).hypot() <= MITER_LIMIT * d.abs() =>
                {
                    vec![x, b]
                }
                _ => vec![b],
            }
        };
        let first = start_point(&moved_pieces[0]);
        let begin = if closed {
            match corner(0).as_slice() {
                [x, _] => *x,
                _ => first,
            }
        } else {
            first
        };
        out.move_to(begin);
        if closed && begin != first {
            out.line_to(first);
        }
        for (i, els) in moved_pieces.iter().enumerate() {
            if i > 0 {
                for p in corner(i) {
                    out.line_to(p);
                }
            }
            for el in els.iter().skip(1) {
                out.push(*el);
            }
        }
        if closed {
            if let [x, _] = corner(0).as_slice() {
                out.line_to(*x);
            }
            out.close_path();
        }
    }
    out
}

fn start_point(els: &[PathEl]) -> Point {
    match els.first() {
        Some(PathEl::MoveTo(p)) => *p,
        _ => Point::ZERO,
    }
}

fn end_point(els: &[PathEl]) -> Point {
    match els.last() {
        Some(PathEl::LineTo(p)) | Some(PathEl::MoveTo(p)) => *p,
        Some(PathEl::QuadTo(_, p)) | Some(PathEl::CurveTo(_, _, p)) => *p,
        _ => Point::ZERO,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::{ParamCurveNearest, Shape};

    fn pts(path: &BezPath) -> Vec<Point> {
        path.elements()
            .iter()
            .filter_map(|e| match e {
                PathEl::MoveTo(p) | PathEl::LineTo(p) => Some(*p),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_corner_line_gets_a_line_alongside_with_a_pointed_corner() {
        let mut p = BezPath::new();
        p.move_to((0.0, 0.0));
        p.line_to((10.0, 0.0));
        p.line_to((10.0, 10.0));
        // Left of a line going right then up: above it, then left of the
        // upward part (inside the corner).
        let left = parallel_path(&p, 1.0);
        let l = pts(&left);
        // The two lines meet at (9, 1), with no overlap.
        assert_eq!(l.len(), 3, "{l:?}");
        assert!((l[1] - Point::new(9.0, 1.0)).hypot() < 1e-9, "{l:?}");
        assert!((l[0] - Point::new(0.0, 1.0)).hypot() < 1e-9);
        assert!((l.last().unwrap().x - 9.0).abs() < 1e-9);
        // Right: below and outside, carried out to a point at (11, -1).
        let right = parallel_path(&p, -1.0);
        let r = pts(&right);
        assert!(
            r.iter()
                .any(|q| (*q - Point::new(11.0, -1.0)).hypot() < 1e-9),
            "{r:?}"
        );
        assert!((r[0] - Point::new(0.0, -1.0)).hypot() < 1e-9);
        assert!((*r.last().unwrap() - Point::new(11.0, 10.0)).hypot() < 1e-9);
    }

    #[test]
    fn a_curve_stays_at_the_distance() {
        let mut p = BezPath::new();
        p.move_to((0.0, 0.0));
        p.curve_to((10.0, 20.0), (30.0, 20.0), (40.0, 0.0));
        let q = parallel_path(&p, 3.0);
        let src = CubicBez::new((0.0, 0.0), (10.0, 20.0), (30.0, 20.0), (40.0, 0.0));
        for seg in q.segments() {
            for k in 0..=10 {
                let pt = seg.eval(f64::from(k) / 10.0);
                let dist = src.nearest(pt, 1e-6).distance_sq.sqrt();
                assert!((dist - 3.0).abs() < 0.05, "{dist}");
            }
        }
    }

    #[test]
    fn a_closed_square_gets_a_closed_square() {
        let mut p = BezPath::new();
        p.move_to((0.0, 0.0));
        p.line_to((10.0, 0.0));
        p.line_to((10.0, 10.0));
        p.line_to((0.0, 10.0));
        p.close_path();
        // The square turns counterclockwise (y up), so its left is inside.
        let inside = parallel_path(&p, 1.0);
        let b = inside.bounding_box();
        assert!(
            (b.x0 - 1.0).abs() < 1e-9 && (b.x1 - 9.0).abs() < 1e-9,
            "{b:?}"
        );
        assert!(matches!(inside.elements().last(), Some(PathEl::ClosePath)));
        let outside = parallel_path(&p, -1.0);
        let b = outside.bounding_box();
        assert!(
            (b.x0 + 1.0).abs() < 1e-9 && (b.y1 - 11.0).abs() < 1e-9,
            "{b:?}"
        );
    }
}
