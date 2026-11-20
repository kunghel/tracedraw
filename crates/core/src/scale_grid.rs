//! Stretch zones of a symbol: four lines set in from the sides of the
//! symbol's objects part them into nine zones. When an instance is
//! stretched, the corner zones keep their size, the side zones stretch
//! along their side only and the middle zone takes up the rest, so frames,
//! buttons and labels keep their corners whatever their size.
//!
//! The objects are reshaped in the symbol's own space so that the
//! instance's transformation then gives that result: their curves are cut
//! where they cross the lines, and every point is moved by a map that is
//! a plain move in a corner and a stretch between the lines.

use crate::document::{ScaleGrid, Shape, ShapeKind, Symbol};
use crate::geometry::{Affine, BezPath, Point};
use crate::ShapeId;
use kurbo::{CubicBez, ParamCurve, PathEl};

/// One axis of the map: `v` between `lo` and `hi` (the symbol's extent),
/// with `a` and `b` kept at the low and high ends, for a stretch of `s`.
/// The result, stretched by `s` again, keeps the ends' sizes. Too short
/// for both ends, the axis stretches as a whole.
fn axis(v: f64, lo: f64, hi: f64, a: f64, b: f64, s: f64) -> f64 {
    let w = hi - lo;
    if w <= 0.0 || a + b >= w || s * w <= a + b || (s - 1.0).abs() < 1e-12 {
        return v;
    }
    let u = v - lo;
    let phi = if u <= a {
        u
    } else if u >= w - b {
        s * w - (w - u)
    } else {
        a + (u - a) * (s * w - a - b) / (w - a - b)
    };
    lo + phi / s
}

/// The map of a symbol with extent `r`, grid `g`, stretched by `sx`, `sy`.
#[derive(Clone, Copy)]
struct Map {
    r: kurbo::Rect,
    g: ScaleGrid,
    sx: f64,
    sy: f64,
}

impl Map {
    fn point(&self, p: Point) -> Point {
        Point::new(
            axis(
                p.x,
                self.r.x0,
                self.r.x1,
                self.g.left,
                self.g.right,
                self.sx,
            ),
            axis(
                p.y,
                self.r.y0,
                self.r.y1,
                self.g.bottom,
                self.g.top,
                self.sy,
            ),
        )
    }

    /// The lines between the zones, along X and along Y.
    fn lines(&self) -> ([f64; 2], [f64; 2]) {
        (
            [self.r.x0 + self.g.left, self.r.x1 - self.g.right],
            [self.r.y0 + self.g.bottom, self.r.y1 - self.g.top],
        )
    }

    /// `c` cut where it crosses a line between the zones.
    fn cut(&self, c: CubicBez) -> Vec<CubicBez> {
        let (xs, ys) = self.lines();
        let mut ts: Vec<f64> = Vec::new();
        let coeffs = |p0: f64, p1: f64, p2: f64, p3: f64| {
            (
                p0,
                3.0 * (p1 - p0),
                3.0 * (p2 - 2.0 * p1 + p0),
                p3 - 3.0 * p2 + 3.0 * p1 - p0,
            )
        };
        let cx = coeffs(c.p0.x, c.p1.x, c.p2.x, c.p3.x);
        let cy = coeffs(c.p0.y, c.p1.y, c.p2.y, c.p3.y);
        for (k, lines) in [(cx, xs), (cy, ys)] {
            for line in lines {
                for t in kurbo::common::solve_cubic(k.0 - line, k.1, k.2, k.3) {
                    if t > 1e-9 && t < 1.0 - 1e-9 {
                        ts.push(t);
                    }
                }
            }
        }
        ts.sort_by(f64::total_cmp);
        ts.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
        let mut out = Vec::with_capacity(ts.len() + 1);
        let mut t0 = 0.0;
        for t in ts.into_iter().chain([1.0]) {
            out.push(c.subsegment(t0..t));
            t0 = t;
        }
        out
    }

    /// `path` (in the symbol's space) reshaped.
    fn path(&self, path: &BezPath) -> BezPath {
        let mut out = BezPath::new();
        let (mut start, mut last) = (Point::ZERO, Point::ZERO);
        let line =
            |a: Point, b: Point| CubicBez::new(a, a.lerp(b, 1.0 / 3.0), a.lerp(b, 2.0 / 3.0), b);
        let push = |out: &mut BezPath, c: CubicBez, straight: bool| {
            for piece in self.cut(c) {
                if straight {
                    out.line_to(self.point(piece.p3));
                } else {
                    out.curve_to(
                        self.point(piece.p1),
                        self.point(piece.p2),
                        self.point(piece.p3),
                    );
                }
            }
        };
        for el in path.elements() {
            match *el {
                PathEl::MoveTo(p) => {
                    out.move_to(self.point(p));
                    start = p;
                    last = p;
                }
                PathEl::LineTo(p) => {
                    push(&mut out, line(last, p), true);
                    last = p;
                }
                PathEl::QuadTo(a, p) => {
                    push(&mut out, kurbo::QuadBez::new(last, a, p).raise(), false);
                    last = p;
                }
                PathEl::CurveTo(a, b, p) => {
                    push(&mut out, CubicBez::new(last, a, b, p), false);
                    last = p;
                }
                PathEl::ClosePath => {
                    if (last - start).hypot() > 1e-9 {
                        push(&mut out, line(last, start), true);
                    }
                    out.close_path();
                    last = start;
                }
            }
        }
        out
    }

    /// `shape` (placed in the symbol's space by `outer`) reshaped, its
    /// members too. Pictures stay as they are.
    fn shape(&self, shape: &mut Shape, outer: Affine) {
        let t = outer * shape.transform;
        match &mut shape.kind {
            ShapeKind::Path { path, .. } => {
                if t.determinant().abs() > 1e-12 {
                    *path = t.inverse() * self.path(&(t * path.clone()));
                }
            }
            ShapeKind::Group { children } => {
                for c in children {
                    self.shape(c, t);
                }
            }
            ShapeKind::ClipFrame { frame, contents } => {
                self.shape(frame, t);
                for c in contents {
                    self.shape(c, t);
                }
            }
            _ => {}
        }
    }
}

/// The objects of symbol definition `def`, with stretch zones `grid`,
/// reshaped for an instance placed by `instance` (its transformation):
/// drawn through that transformation, the corners keep their size. None
/// when the instance is not stretched or cannot be.
pub fn stretched(
    def: &Shape,
    grid: ScaleGrid,
    instance: Affine,
    symbols: &[Symbol],
) -> Option<Shape> {
    let [a, b, c, d, _, _] = instance.as_coeffs();
    let (sx, sy) = (a.hypot(b), c.hypot(d));
    if !(sx > 1e-9 && sy > 1e-9 && sx.is_finite() && sy.is_finite()) {
        return None;
    }
    if (sx - 1.0).abs() < 1e-9 && (sy - 1.0).abs() < 1e-9 {
        return None;
    }
    let r = def.bounds();
    if !(r.width() > 0.0 && r.height() > 0.0 && r.is_finite()) {
        return None;
    }
    let mut n = 0u64;
    let mut next = || {
        n += 1;
        ShapeId(u64::MAX - n)
    };
    let mut out = def
        .converted_to_curves(symbols, &mut next)
        .unwrap_or_else(|| def.clone());
    let map = Map {
        r,
        g: grid.sanitized(),
        sx,
        sy,
    };
    map.shape(&mut out, Affine::IDENTITY);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Rect;
    use kurbo::Shape as _;

    fn frame() -> Shape {
        // A 20 by 10 frame with a notch 2 in from the left at the top.
        let mut p = BezPath::new();
        p.move_to((0.0, 0.0));
        p.line_to((20.0, 0.0));
        p.line_to((20.0, 10.0));
        p.line_to((2.0, 10.0));
        p.line_to((2.0, 8.0));
        p.line_to((0.0, 8.0));
        p.close_path();
        Shape::new(
            ShapeId(1),
            ShapeKind::Path {
                path: p,
                closed: true,
            },
        )
    }

    #[test]
    fn corners_keep_their_size_when_stretched() {
        let def = frame();
        let grid = ScaleGrid {
            left: 3.0,
            right: 3.0,
            top: 3.0,
            bottom: 3.0,
        };
        let t = Affine::scale_non_uniform(3.0, 2.0);
        assert!(stretched(&def, grid, Affine::translate((5.0, 5.0)), &[]).is_none());
        let s = stretched(&def, grid, t, &[]).expect("stretched");
        let ShapeKind::Path { path, .. } = &s.kind else {
            panic!();
        };
        let page = t * path.clone();
        // The box is the plain stretch's: 60 by 20.
        let b = page.bounding_box();
        assert!(
            (b.x1 - 60.0).abs() < 1e-9 && (b.y1 - 20.0).abs() < 1e-9,
            "{b:?}"
        );
        // The notch keeps its size at the top left corner: 2 wide, 2 deep.
        let pts: Vec<Point> = page
            .elements()
            .iter()
            .filter_map(|e| match e {
                PathEl::MoveTo(p) | PathEl::LineTo(p) => Some(*p),
                _ => None,
            })
            .collect();
        let near = |q: Point| pts.iter().any(|p| (*p - q).hypot() < 1e-9);
        assert!(near(Point::new(2.0, 20.0)), "{pts:?}");
        assert!(near(Point::new(2.0, 18.0)), "{pts:?}");
        assert!(near(Point::new(0.0, 18.0)), "{pts:?}");
    }

    #[test]
    fn instances_of_a_symbol_with_zones_expand_stretched() {
        let mut def = frame();
        def.scale_grid = Some(Box::new(ScaleGrid {
            left: 3.0,
            right: 3.0,
            top: 3.0,
            bottom: 3.0,
        }));
        let symbols = vec![Symbol {
            name: "S".into(),
            shape: def,
        }];
        let mut inst = Shape::new(ShapeId(9), ShapeKind::SymbolInstance { index: 0 });
        inst.transform = Affine::scale_non_uniform(4.0, 1.0);
        let parts = inst.expand(&symbols);
        let page = inst.transform * parts[0].local_path();
        assert!(page
            .elements()
            .iter()
            .any(|e| matches!(e, PathEl::LineTo(p) if (p.x - 2.0).abs() < 1e-9 && (p.y - 8.0).abs() < 1e-9)));
        // An instance at its own size draws the symbol as it is.
        inst.transform = Affine::translate((1.0, 2.0));
        assert_eq!(inst.expand(&symbols)[0], symbols[0].shape);
    }

    #[test]
    fn a_curve_across_a_line_is_cut_there() {
        let def = Shape::new(
            ShapeId(1),
            ShapeKind::Ellipse {
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                arc: None,
            },
        );
        let grid = ScaleGrid {
            left: 4.0,
            right: 4.0,
            top: 4.0,
            bottom: 4.0,
        };
        let t = Affine::scale_non_uniform(2.0, 1.0);
        let s = stretched(&def, grid, t, &[]).expect("stretched");
        let page = t * s.local_path();
        let b = page.bounding_box();
        assert!((b.x1 - 20.0).abs() < 1e-6 && (b.x0).abs() < 1e-6, "{b:?}");
        // The leftmost point of the circle stays where it was, 5 up.
        let left = kurbo::Point::new(0.0, 5.0);
        assert!(page
            .segments()
            .any(|seg| (seg.eval(0.0) - left).hypot() < 1e-6));
    }
}
