//! Bend with Pins: pins stuck in the selected objects, each dragged to a
//! new place, bend the objects around them as if they were a sheet of
//! rubber held at the pins. One pin moves the objects, two turn them,
//! more bend them; near a pin a part keeps its shape and only turns.
//!
//! Every point goes where the pins take it by a least-squares fit of a
//! turn and a move, weighted to the pins nearest that point (the moving
//! least squares method of Schaefer, McPhail and Warren, 2006). Curves
//! are cut into short pieces first, so a long straight side bends
//! smoothly.

use kurbo::{Affine, BezPath, CubicBez, ParamCurve, PathEl, Point, Vec2};
use tracedraw_core::{Shape, ShapeKind};

/// A pin: where it was stuck, and where it is dragged to (both in page
/// space).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pin {
    pub from: Point,
    pub to: Point,
}

/// The pieces each segment is cut into before it bends.
const PIECES: usize = 8;

/// Where `v` goes. `reach` (1 to 3) sets how fast a pin's hold fades with
/// distance: higher keeps each part closer to its nearest pin.
pub fn bend_point(pins: &[Pin], v: Point, reach: f64) -> Point {
    match pins {
        [] => return v,
        [p] => return v + (p.to - p.from),
        _ => {}
    }
    let alpha = reach.clamp(0.5, 4.0);
    let mut ws = Vec::with_capacity(pins.len());
    for p in pins {
        let d2 = (p.from - v).hypot2();
        if d2 < 1e-18 {
            return p.to;
        }
        ws.push(1.0 / d2.powf(alpha));
    }
    let sw: f64 = ws.iter().sum();
    let pc = pins
        .iter()
        .zip(&ws)
        .fold(Vec2::ZERO, |a, (p, w)| a + p.from.to_vec2() * *w)
        / sw;
    let qc = pins
        .iter()
        .zip(&ws)
        .fold(Vec2::ZERO, |a, (p, w)| a + p.to.to_vec2() * *w)
        / sw;
    // The best turn: the angle of the sum of w · conj(p̂) · q̂.
    let (mut re, mut im) = (0.0, 0.0);
    for (p, w) in pins.iter().zip(&ws) {
        let a = p.from.to_vec2() - pc;
        let b = p.to.to_vec2() - qc;
        re += w * (a.x * b.x + a.y * b.y);
        im += w * (a.x * b.y - a.y * b.x);
    }
    let len = (re * re + im * im).sqrt();
    let (c, s) = if len < 1e-18 {
        (1.0, 0.0)
    } else {
        (re / len, im / len)
    };
    let d = v.to_vec2() - pc;
    let r = Vec2::new(c * d.x - s * d.y, s * d.x + c * d.y);
    (r + qc).to_point()
}

/// The point that `pins` take to `at`: where a pin stuck at `at` in the
/// bent picture was on the objects as they are. Found by Newton steps
/// with the bend's slopes measured close by, for a bend that does not
/// fold.
pub fn unbend_point(pins: &[Pin], at: Point, reach: f64) -> Point {
    let mut u = at;
    for _ in 0..40 {
        let f = bend_point(pins, u, reach);
        let miss = at - f;
        if miss.hypot() < 1e-9 {
            break;
        }
        let h = 1e-4 * (1.0 + u.to_vec2().hypot());
        let dx = (bend_point(pins, u + Vec2::new(h, 0.0), reach) - f) / h;
        let dy = (bend_point(pins, u + Vec2::new(0.0, h), reach) - f) / h;
        let det = dx.x * dy.y - dy.x * dx.y;
        if det.abs() < 1e-12 {
            break;
        }
        let step = Vec2::new(
            (dy.y * miss.x - dy.x * miss.y) / det,
            (-dx.y * miss.x + dx.x * miss.y) / det,
        );
        u += step;
    }
    u
}

/// `path`, drawn with `to_page`, bent by `pins`; the result is in the
/// path's own space again. Every segment becomes [`PIECES`] curves.
pub fn bend_path(path: &BezPath, to_page: Affine, pins: &[Pin], reach: f64) -> BezPath {
    let back = to_page.inverse();
    let map = |p: Point| back * bend_point(pins, to_page * p, reach);
    let mut out = BezPath::new();
    let mut start = Point::ZERO;
    let mut last = Point::ZERO;
    let piece = |out: &mut BezPath, c: CubicBez| {
        for i in 0..PIECES {
            let t0 = i as f64 / PIECES as f64;
            let t1 = (i + 1) as f64 / PIECES as f64;
            let s = c.subsegment(t0..t1);
            out.curve_to(map(s.p1), map(s.p2), map(s.p3));
        }
    };
    let line = |a: Point, b: Point| CubicBez::new(a, a.lerp(b, 1.0 / 3.0), a.lerp(b, 2.0 / 3.0), b);
    for el in path.elements() {
        match *el {
            PathEl::MoveTo(p) => {
                out.move_to(map(p));
                start = p;
                last = p;
            }
            PathEl::LineTo(p) => {
                piece(&mut out, line(last, p));
                last = p;
            }
            PathEl::QuadTo(a, p) => {
                let q = kurbo::QuadBez::new(last, a, p).raise();
                piece(&mut out, q);
                last = p;
            }
            PathEl::CurveTo(a, b, p) => {
                piece(&mut out, CubicBez::new(last, a, b, p));
                last = p;
            }
            PathEl::ClosePath => {
                if (last - start).hypot() > 1e-9 {
                    piece(&mut out, line(last, start));
                }
                out.close_path();
                last = start;
            }
        }
    }
    out
}

/// `shape` (already curves) bent by `pins`, its members too; `outer` is
/// what places it on the page (the groups it is in). Bitmaps and other
/// objects that are not curves stay as they are. Whether anything bent.
pub fn bend_shape(shape: &mut Shape, outer: Affine, pins: &[Pin], reach: f64) -> bool {
    let t = outer * shape.transform;
    match &mut shape.kind {
        ShapeKind::Path { path, .. } => {
            *path = bend_path(path, t, pins, reach);
            true
        }
        ShapeKind::Group { children } => {
            let mut any = false;
            for c in children {
                any |= bend_shape(c, t, pins, reach);
            }
            any
        }
        ShapeKind::ClipFrame { frame, contents } => {
            let mut any = bend_shape(frame, t, pins, reach);
            for c in contents {
                any |= bend_shape(c, t, pins, reach);
            }
            any
        }
        _ => false,
    }
}

/// The outlines of `shape` on the page, its members' too, for a preview.
pub fn page_outlines(shape: &Shape, outer: Affine, out: &mut Vec<BezPath>) {
    let t = outer * shape.transform;
    match &shape.kind {
        ShapeKind::Path { path, .. } => out.push(t * path.clone()),
        ShapeKind::Group { children } => {
            for c in children {
                page_outlines(c, t, out);
            }
        }
        ShapeKind::ClipFrame { frame, contents } => {
            page_outlines(frame, t, out);
            for c in contents {
                page_outlines(c, t, out);
            }
        }
        ShapeKind::Bitmap { .. } => {}
        _ => out.push(t * shape.local_path()),
    }
}

/// A point of `path` at parameter `t` of its segment `i`, for tests.
#[cfg(test)]
fn segment_point(path: &BezPath, i: usize, t: f64) -> Point {
    path.segments().nth(i).unwrap().eval(t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::Shape as _;

    fn near(a: Point, b: Point) -> bool {
        (a - b).hypot() < 1e-6
    }

    #[test]
    fn one_pin_moves_two_pins_turn() {
        let v = Point::new(3.0, 4.0);
        assert_eq!(bend_point(&[], v, 2.0), v);
        let one = [Pin {
            from: Point::new(0.0, 0.0),
            to: Point::new(10.0, 0.0),
        }];
        assert!(near(bend_point(&one, v, 2.0), Point::new(13.0, 4.0)));
        // Two pins turned a quarter about their middle: every point turns
        // with them.
        let two = [
            Pin {
                from: Point::new(-1.0, 0.0),
                to: Point::new(0.0, -1.0),
            },
            Pin {
                from: Point::new(1.0, 0.0),
                to: Point::new(0.0, 1.0),
            },
        ];
        let got = bend_point(&two, Point::new(0.0, 2.0), 2.0);
        assert!(near(got, Point::new(-2.0, 0.0)), "{got:?}");
        // A pin's own point goes exactly where the pin goes.
        assert!(near(
            bend_point(&two, Point::new(1.0, 0.0), 2.0),
            Point::new(0.0, 1.0)
        ));
        // And back.
        let u = unbend_point(&two, Point::new(-2.0, 0.0), 2.0);
        assert!(near(u, Point::new(0.0, 2.0)), "{u:?}");
    }

    #[test]
    fn pins_left_in_place_change_nothing() {
        let pins = [
            Pin {
                from: Point::new(0.0, 0.0),
                to: Point::new(0.0, 0.0),
            },
            Pin {
                from: Point::new(50.0, 10.0),
                to: Point::new(50.0, 10.0),
            },
            Pin {
                from: Point::new(20.0, 40.0),
                to: Point::new(20.0, 40.0),
            },
        ];
        for v in [Point::new(7.0, 3.0), Point::new(-20.0, 60.0)] {
            assert!(near(bend_point(&pins, v, 2.0), v));
        }
    }

    /// A long bar held at its left end, its right end dragged up: the
    /// left stays, the right rises, the middle bends between, and the
    /// straight sides become curves.
    #[test]
    fn a_bar_bends_between_its_pins() {
        let bar = kurbo::Rect::new(0.0, 0.0, 100.0, 10.0).to_path(0.1);
        let pins = [
            Pin {
                from: Point::new(5.0, 5.0),
                to: Point::new(5.0, 5.0),
            },
            Pin {
                from: Point::new(95.0, 5.0),
                to: Point::new(85.0, 45.0),
            },
        ];
        let page = Affine::translate((20.0, 30.0));
        let pins_page: Vec<Pin> = pins
            .iter()
            .map(|p| Pin {
                from: page * p.from,
                to: page * p.to,
            })
            .collect();
        let bent = bend_path(&bar, page, &pins_page, 2.0);
        assert_eq!(bent.segments().count(), 4 * PIECES);
        // The bottom side: its start held, its end raised, its middle in
        // between.
        let start = segment_point(&bent, 0, 0.0);
        let end = segment_point(&bent, PIECES - 1, 1.0);
        let mid = segment_point(&bent, PIECES / 2, 0.0);
        assert!((start.y - 0.0).abs() < 3.0, "{start:?}");
        assert!(end.y > 30.0, "{end:?}");
        assert!(mid.y > start.y + 2.0 && mid.y < end.y - 2.0, "{mid:?}");
        assert!(bent
            .elements()
            .iter()
            .any(|e| matches!(e, PathEl::ClosePath)));
    }

    #[test]
    fn a_group_bends_with_its_members() {
        let mut id = tracedraw_core::id::IdSource::default();
        let path = kurbo::Rect::new(0.0, 0.0, 10.0, 10.0).to_path(0.1);
        let child = Shape::new(id.shape(), ShapeKind::Path { path, closed: true });
        let mut group = Shape::new(
            id.shape(),
            ShapeKind::Group {
                children: vec![child],
            },
        );
        let pins = [Pin {
            from: Point::new(0.0, 0.0),
            to: Point::new(5.0, 0.0),
        }];
        assert!(bend_shape(&mut group, Affine::IDENTITY, &pins, 2.0));
        let b = group.bounds();
        assert!(
            (b.x0 - 5.0).abs() < 1e-6 && (b.x1 - 15.0).abs() < 1e-6,
            "{b:?}"
        );
    }
}
