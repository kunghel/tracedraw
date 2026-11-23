//! Geometry primitives. We re-export kurbo so the rest of the workspace
//! shares one set of types, and add the few helpers the editor needs.

pub use kurbo::{
    Affine, Arc, BezPath, Circle, CubicBez, Ellipse, Line, ParamCurve, ParamCurveArclen,
    ParamCurveDeriv, ParamCurveNearest, PathEl, PathSeg, Point, Rect, RoundedRect, Shape, Size,
    Vec2,
};

/// Flatten a path into lines within `tolerance`, calling `f` with each
/// move, line and close element.
pub fn flatten(path: &BezPath, tolerance: f64, f: &mut dyn FnMut(PathEl)) {
    kurbo::flatten(path.elements().iter().copied(), tolerance, f);
}

/// Build a closed rectangle path, optionally with rounded corners.
pub fn rect_path(rect: Rect, radius: f64) -> BezPath {
    if radius > 0.0 {
        RoundedRect::from_rect(rect, radius).to_path(0.01)
    } else {
        rect.to_path(0.01)
    }
}

/// Build a closed ellipse path inscribed in `rect`: four curves between
/// nodes at the right, top, left and bottom (in that order, starting at
/// the right), each node with equal handles along the tangent, and the
/// last curve ending on the start, so it is one closed curve of four
/// nodes when converted to curves (#625).
pub fn ellipse_path(rect: Rect) -> BezPath {
    // Handle length of a quarter circle as a cubic, as a share of the
    // radius: 4/3 (sqrt 2 - 1).
    const K: f64 = 0.552_284_749_830_793_4;
    let c = rect.center();
    let (rx, ry) = (rect.width() / 2.0, rect.height() / 2.0);
    let (kx, ky) = (rx * K, ry * K);
    let right = Point::new(c.x + rx, c.y);
    let top = Point::new(c.x, c.y + ry);
    let left = Point::new(c.x - rx, c.y);
    let bottom = Point::new(c.x, c.y - ry);
    let mut path = BezPath::new();
    path.move_to(right);
    let at = |x: f64, y: f64| Point::new(x, y);
    path.curve_to(at(right.x, right.y + ky), at(top.x + kx, top.y), top);
    path.curve_to(at(top.x - kx, top.y), at(left.x, left.y + ky), left);
    path.curve_to(at(left.x, left.y - ky), at(bottom.x - kx, bottom.y), bottom);
    path.curve_to(
        at(bottom.x + kx, bottom.y),
        at(right.x, right.y - ky),
        right,
    );
    path.close_path();
    path
}

/// Arc or pie of the ellipse inscribed in `rect`, angles in degrees CCW from +x.
pub fn ellipse_arc_path(rect: Rect, start_deg: f64, end_deg: f64, pie: bool) -> BezPath {
    let c = rect.center();
    let radii = kurbo::Vec2::new(rect.width() / 2.0, rect.height() / 2.0);
    let mut sweep = (end_deg - start_deg).rem_euclid(360.0);
    if sweep == 0.0 {
        sweep = 360.0;
    }
    let arc = kurbo::Arc::new(c, radii, start_deg.to_radians(), sweep.to_radians(), 0.0);
    let mut path = BezPath::new();
    let start = Point::new(
        c.x + radii.x * start_deg.to_radians().cos(),
        c.y + radii.y * start_deg.to_radians().sin(),
    );
    if pie {
        path.move_to(c);
        path.line_to(start);
    } else {
        path.move_to(start);
    }
    // A tolerance of 0.1 mm, or a millionth of the radius on a huge arc:
    // the number of pieces grows with radius over tolerance, so an arc
    // with a radius of 1e300 from a damaged file asked for more pieces
    // than memory holds (#3338).
    let tolerance = (radii.x.abs().max(radii.y.abs()) * 1e-6).max(0.1);
    arc.append_iter(tolerance).for_each(|el| path.push(el));
    if pie {
        path.close_path();
    }
    path
}

/// A ring sector of the ellipse that fills `rect`: the band between the
/// ellipse and a smaller one at `inner` times its radii (0 to 0.99), from
/// `start_deg` to `end_deg` counter-clockwise, closed across both ends; a
/// whole turn is a ring (the outline and the inner ellipse turning the
/// other way, so the middle stays empty under either fill rule). `inner`
/// 0 or less is the pie itself.
pub fn ellipse_ring_path(rect: Rect, start_deg: f64, end_deg: f64, inner: f64) -> BezPath {
    if !inner.is_finite() || inner <= 0.0 {
        return ellipse_arc_path(rect, start_deg, end_deg, true);
    }
    let k = inner.min(0.99);
    let c = rect.center();
    let radii = kurbo::Vec2::new(rect.width() / 2.0, rect.height() / 2.0);
    let inner_radii = radii * k;
    let mut sweep = (end_deg - start_deg).rem_euclid(360.0);
    let whole = sweep == 0.0;
    if whole {
        sweep = 360.0;
    }
    let tolerance = (radii.x.abs().max(radii.y.abs()) * 1e-6).max(0.1);
    let at = |r: kurbo::Vec2, deg: f64| {
        Point::new(
            c.x + r.x * deg.to_radians().cos(),
            c.y + r.y * deg.to_radians().sin(),
        )
    };
    let mut path = BezPath::new();
    path.move_to(at(radii, start_deg));
    kurbo::Arc::new(c, radii, start_deg.to_radians(), sweep.to_radians(), 0.0)
        .append_iter(tolerance)
        .for_each(|el| path.push(el));
    let back = kurbo::Arc::new(
        c,
        inner_radii,
        (start_deg + sweep).to_radians(),
        -sweep.to_radians(),
        0.0,
    );
    if whole {
        path.close_path();
        path.move_to(at(inner_radii, start_deg + sweep));
    } else {
        path.line_to(at(inner_radii, start_deg + sweep));
    }
    back.append_iter(tolerance).for_each(|el| path.push(el));
    path.close_path();
    path
}

/// Build a regular polygon or star inscribed in the ellipse that fills
/// `rect`, with its first vertex at the top.
///
/// `points` is the number of vertices; `sharpness` in `0.0..1.0` pulls every
/// other vertex toward the centre, turning a polygon into a star, and a
/// negative one puts those points past the centre: an inverted star (see
/// [`star_inner_ratio`]).
pub fn polygon_path(rect: Rect, points: u32, sharpness: f64) -> BezPath {
    twisted_polygon_path(rect, points, sharpness, 0.0)
}

/// A polygon or star whose points between the vertices are turned by
/// `twist` radians (counter-clockwise, within half a turn) about the
/// centre: a twisted star. A polygon without sharpness gains points
/// between its vertices, where the middles of its sides were.
pub fn twisted_polygon_path(rect: Rect, points: u32, sharpness: f64, twist: f64) -> BezPath {
    let c = rect.center();
    let rx = rect.width() / 2.0;
    let ry = rect.height() / 2.0;
    let mut path = BezPath::new();
    for (i, (x, y)) in polygon_unit_vertices(points, sharpness, twist).enumerate() {
        let p = Point::new(c.x + rx * x, c.y + ry * y);
        if i == 0 {
            path.move_to(p);
        } else {
            path.line_to(p);
        }
    }
    path.close_path();
    path
}

/// [`twisted_polygon_path`] with every side bent the same way (#2451):
/// each side from one point to the next becomes a curve whose control
/// points stand off the side's thirds by `bend` times the side's length,
/// away from the inside of the outline (towards it when negative). Every
/// side gets the same curve, so a polygon or star keeps its mirror
/// symmetry. The curves are made in the unit circle and then stretched
/// to `rect`, as the points are.
pub fn bent_polygon_path(
    rect: Rect,
    points: u32,
    sharpness: f64,
    twist: f64,
    bend: f64,
) -> BezPath {
    let bend = if bend.is_finite() {
        bend.clamp(-MAX_POLYGON_BEND, MAX_POLYGON_BEND)
    } else {
        0.0
    };
    if bend == 0.0 {
        return twisted_polygon_path(rect, points, sharpness, twist);
    }
    let c = rect.center();
    let (rx, ry) = (rect.width() / 2.0, rect.height() / 2.0);
    let page = |x: f64, y: f64| Point::new(c.x + rx * x, c.y + ry * y);
    let unit: Vec<(f64, f64)> = polygon_unit_vertices(points, sharpness, twist).collect();
    let mut path = BezPath::new();
    let Some(&(x0, y0)) = unit.first() else {
        return path;
    };
    path.move_to(page(x0, y0));
    // Which side is out: the way the points go round.
    let area: f64 = (0..unit.len())
        .map(|i| {
            let (ax, ay) = unit[i];
            let (bx, by) = unit[(i + 1) % unit.len()];
            ax * by - bx * ay
        })
        .sum();
    let out = if area < 0.0 { -bend } else { bend };
    for i in 0..unit.len() {
        let (ax, ay) = unit[i];
        let (bx, by) = unit[(i + 1) % unit.len()];
        let (dx, dy) = (bx - ax, by - ay);
        let (nx, ny) = (dy * out, -dx * out);
        path.curve_to(
            page(ax + dx / 3.0 + nx, ay + dy / 3.0 + ny),
            page(bx - dx / 3.0 + nx, by - dy / 3.0 + ny),
            page(bx, by),
        );
    }
    path.close_path();
    path
}

/// The points a polygon or star's outline goes through, in the unit
/// circle (y down, centre at 0), in order: its vertices and, on a star,
/// the points between them.
pub fn polygon_unit_points(points: u32, sharpness: f64, twist: f64) -> Vec<(f64, f64)> {
    polygon_unit_vertices(points, sharpness, twist).collect()
}

/// The most a polygon's sides bend either way, as a part of their length.
pub const MAX_POLYGON_BEND: f64 = 2.0;

/// Whether `sharpness` makes a polygon a star: any finite value but 0
/// (a negative one an inverted star).
pub fn is_star_sharpness(sharpness: f64) -> bool {
    sharpness.is_finite() && sharpness != 0.0
}

/// The distance of a star's inner points from its centre, as a part of
/// the vertices' distance, for a star `sharpness` (not 0). From 0 to 1
/// the points come in from the rim (`1 - sharpness`) to the centre; from
/// 0 to -1 they lie past the centre, on the far side (#1331): `-0.5`
/// puts them half way out on the opposite side, `-1` at the centre again.
/// The ratio is then negative: `-(1 - |sharpness|)`.
pub fn star_inner_ratio(sharpness: f64) -> f64 {
    if !sharpness.is_finite() {
        return 1.0;
    }
    let s = sharpness.clamp(-1.0, 1.0);
    if s < 0.0 {
        -(1.0 + s)
    } else {
        1.0 - s
    }
}

/// The star sharpness whose inner points sit at `ratio` of the vertices'
/// distance from the centre, negative past it: the inverse of
/// [`star_inner_ratio`], kept a star (its size between 0.001 and 0.999)
/// on either side.
pub fn star_sharpness_for_ratio(ratio: f64) -> f64 {
    let r = if ratio.is_finite() { ratio } else { 0.0 };
    if r < 0.0 {
        -(1.0 + r).clamp(0.001, 0.999)
    } else {
        (1.0 - r).clamp(0.001, 0.999)
    }
}

/// The most points a polygon or star has (the tools' limit too).
pub const POLYGON_MAX_POINTS: u32 = 500;

/// The fewest points a complex star has.
pub const COMPLEX_STAR_MIN_POINTS: u32 = 5;

/// The largest sharpness a complex star of `points` points takes: its
/// sides join vertices at most (points - 1) / 2 apart.
pub fn complex_star_max_sharpness(points: u32) -> u32 {
    let n = points.max(COMPLEX_STAR_MIN_POINTS);
    ((n - 1) / 2).saturating_sub(1).max(1)
}

/// A complex star: `points` vertices on the ellipse in `rect` (the first
/// at the top, then clockwise), each joined to the one `sharpness + 1`
/// further on, so the sides cross. When that step shares a factor with
/// the count the star is several closed subpaths (two triangles for six
/// points). Filled even-odd, the middle stays empty.
pub fn complex_star_path(rect: Rect, points: u32, sharpness: u32) -> BezPath {
    let n = points.clamp(COMPLEX_STAR_MIN_POINTS, POLYGON_MAX_POINTS) as usize;
    let step = sharpness.clamp(1, complex_star_max_sharpness(n as u32)) as usize + 1;
    let verts: Vec<Point> = {
        let c = rect.center();
        let (rx, ry) = (rect.width() / 2.0, rect.height() / 2.0);
        polygon_unit_vertices(n as u32, 0.0, 0.0)
            .map(|(x, y)| Point::new(c.x + rx * x, c.y + ry * y))
            .collect()
    };
    let g = gcd(n, step);
    let mut path = BezPath::new();
    for start in 0..g {
        let mut i = start;
        path.move_to(verts[i]);
        for _ in 1..n / g {
            i = (i + step) % n;
            path.line_to(verts[i]);
        }
        path.close_path();
    }
    path
}

fn gcd(a: usize, b: usize) -> usize {
    if b == 0 {
        a.max(1)
    } else {
        gcd(b, a % b)
    }
}

/// Vertices of a polygon or star on the unit circle, first at the top
/// (y up), then clockwise.
fn polygon_unit_vertices(
    points: u32,
    sharpness: f64,
    twist: f64,
) -> impl Iterator<Item = (f64, f64)> {
    // The tools and bars go to 500 points; a file or a macro giving
    // millions (or 4 billion) made drawing it take minutes or ask for more
    // memory than there is, ending the program.
    let points = points.clamp(3, POLYGON_MAX_POINTS) as usize;
    let twist = if twist.is_finite() {
        twist.clamp(-std::f64::consts::PI, std::f64::consts::PI)
    } else {
        0.0
    };
    let star = is_star_sharpness(sharpness) || twist != 0.0;
    // A plain polygon turned keeps its sides' middles as the inner points.
    let inner = if is_star_sharpness(sharpness) {
        star_inner_ratio(sharpness)
    } else {
        (std::f64::consts::PI / points as f64).cos()
    };
    let n = if star { points * 2 } else { points };
    (0..n).map(move |i| {
        let mut t = std::f64::consts::FRAC_PI_2 - i as f64 * std::f64::consts::TAU / n as f64;
        let k = if star && i % 2 == 1 {
            t += twist;
            inner
        } else {
            1.0
        };
        (k * t.cos(), k * t.sin())
    })
}

/// The ellipse rectangle (the `rect` of a polygon shape) whose polygon or
/// star exactly fills `bounds`. Drawing tools drag the visible box of the
/// polygon; the model keeps the ellipse its outer vertices lie on.
pub fn polygon_rect_for_bounds(bounds: Rect, points: u32, sharpness: f64) -> Rect {
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for (x, y) in polygon_unit_vertices(points, sharpness, 0.0) {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    let rx = bounds.width() / (x1 - x0).max(1e-9);
    let ry = bounds.height() / (y1 - y0).max(1e-9);
    let cx = bounds.x0 - x0 * rx;
    let cy = bounds.y0 - y0 * ry;
    Rect::new(cx - rx, cy - ry, cx + rx, cy + ry)
}

/// Straight segments through the points.
pub fn polyline_path(points: &[Point], closed: bool) -> BezPath {
    let mut path = BezPath::new();
    for (i, p) in points.iter().enumerate() {
        if i == 0 {
            path.move_to(*p);
        } else {
            path.line_to(*p);
        }
    }
    if closed && points.len() > 2 {
        path.close_path();
    }
    path
}

/// Smooth cubic curve through the points (Catmull-Rom converted to Bezier),
/// the way a freehand stroke is turned into a curve. A closed curve wraps
/// around: the last point joins the first with a curve too, and the
/// tangents run on through the join, so it has no corner there. A closing
/// point equal to the first is dropped.
pub fn smooth_path(points: &[Point], closed: bool) -> BezPath {
    let mut pts = points;
    if closed && pts.len() > 3 {
        if let (Some(first), Some(last)) = (pts.first(), pts.last()) {
            if (*first - *last).hypot() < 1e-9 {
                pts = &pts[..pts.len() - 1];
            }
        }
    }
    let n = pts.len();
    if n < 3 {
        return polyline_path(pts, closed);
    }
    let mut path = BezPath::new();
    path.move_to(pts[0]);
    let get = |i: isize| -> Point {
        let idx = if closed {
            i.rem_euclid(n as isize) as usize
        } else {
            i.clamp(0, n as isize - 1) as usize
        };
        pts[idx]
    };
    let segments = if closed { n } else { n - 1 };
    // The tangent at a point runs from the point before to the one after
    // and the handles along it are a third of the segment long, so a long
    // segment next to a short one neither overshoots nor flattens.
    let handle = |from: Point, along: Vec2, len: f64| -> Point {
        let l = along.hypot();
        if l < 1e-12 {
            from
        } else {
            from + along * (len / (3.0 * l))
        }
    };
    for i in 0..segments {
        let p0 = get(i as isize - 1);
        let p1 = get(i as isize);
        let p2 = get(i as isize + 1);
        let p3 = get(i as isize + 2);
        let len = (p2 - p1).hypot();
        let c1 = handle(p1, p2 - p0, len);
        let c2 = handle(p2, p1 - p3, len);
        path.curve_to(c1, c2, p2);
    }
    if closed {
        path.close_path();
    }
    path
}

/// Drop points closer than `tolerance` to the line between their
/// neighbours (Ramer-Douglas-Peucker), used to thin freehand input.
pub fn simplify(points: &[Point], tolerance: f64) -> Vec<Point> {
    if points.len() < 3 {
        return points.to_vec();
    }
    fn rdp(pts: &[Point], tol: f64, out: &mut Vec<Point>) {
        let (a, b) = (pts[0], pts[pts.len() - 1]);
        let ab = b - a;
        let len2 = ab.hypot2();
        let mut max_d = 0.0;
        let mut idx = 0;
        for (i, p) in pts.iter().enumerate().skip(1).take(pts.len() - 2) {
            let d = if len2 < 1e-12 {
                (*p - a).hypot()
            } else {
                let t = ((*p - a).dot(ab) / len2).clamp(0.0, 1.0);
                (*p - (a + ab * t)).hypot()
            };
            if d > max_d {
                max_d = d;
                idx = i;
            }
        }
        if max_d > tol && idx > 0 {
            rdp(&pts[..=idx], tol, out);
            out.pop();
            rdp(&pts[idx..], tol, out);
        } else {
            out.push(a);
            out.push(b);
        }
    }
    let mut out = Vec::new();
    rdp(points, tolerance, &mut out);
    out
}

/// Axis-aligned bounds of a path after an affine transform: the box of
/// each mapped segment, as the box of the mapped path computes it, but
/// without copying the path. An object's box is asked for on every
/// pointer move (snapping, picking) and on every render, and the copy
/// was its cost on large drawings.
pub fn transformed_bounds(path: &BezPath, transform: Affine) -> Rect {
    let mut bbox: Option<Rect> = None;
    for seg in path.segments() {
        let b = kurbo::ParamCurveExtrema::bounding_box(&(transform * seg));
        bbox = Some(match bbox {
            Some(bb) => bb.union(b),
            None => b,
        });
    }
    bbox.unwrap_or_default()
}

/// [`bspline_path`] with the control points `fixed` says (one flag a
/// point, missing ones floating) made fixed: the curve passes through a
/// fixed point, with a corner there between the lines of the control
/// polygon, as a point taken three times by the cubic spline does. The
/// ends of an open spline are fixed already (#1758).
pub fn bspline_path_fixed(points: &[Point], fixed: &[bool], closed: bool) -> BezPath {
    if !fixed.contains(&true) {
        return bspline_path(points, closed);
    }
    let n = points.len();
    let mut all = Vec::with_capacity(n + 2 * fixed.len());
    for (i, p) in points.iter().enumerate() {
        let end = !closed && (i == 0 || i + 1 == n);
        let times = if fixed.get(i).copied().unwrap_or(false) && !end {
            3
        } else {
            1
        };
        all.extend(std::iter::repeat_n(*p, times));
    }
    bspline_path(&all, closed)
}

/// A smooth curve through every point of `points` (the Curvature Pen):
/// cubic segments whose handles at a point lie along the chord from the
/// point before to the point after, each as long as a circular arc
/// leaving its own segment's chord at that angle needs (a third of the
/// chord over cos² of half the angle, #3948), so the curve passes each
/// point without a kink and three points on a circle make that circle,
/// however wide. At a
/// point flagged in `fixed` (and at the ends of an open curve) the curve
/// has a corner: the run of points up to it ends there, its last handle
/// pointing so the end bends as a parabola would. Two points of a run
/// make a straight segment. Closed: the curve runs on from the last
/// point to the first (smooth there unless that point is fixed).
pub fn curvature_path(points: &[Point], fixed: &[bool], closed: bool) -> BezPath {
    let mut path = BezPath::new();
    // Points on top of the one before are left out: they give no way on.
    let mut pts: Vec<Point> = Vec::with_capacity(points.len());
    let mut fix: Vec<bool> = Vec::with_capacity(points.len());
    for (i, p) in points.iter().enumerate() {
        let f = fixed.get(i).copied().unwrap_or(false);
        match pts.last() {
            Some(q) if (*q - *p).hypot() < 1e-9 => {
                if let Some(l) = fix.last_mut() {
                    *l |= f;
                }
            }
            _ => {
                pts.push(*p);
                fix.push(f);
            }
        }
    }
    if closed && pts.len() > 2 && (pts[0] - pts[pts.len() - 1]).hypot() < 1e-9 {
        pts.pop();
        let f = fix.pop().unwrap_or(false);
        fix[0] |= f;
    }
    let n = pts.len();
    if n == 0 {
        return path;
    }
    path.move_to(pts[0]);
    if n == 1 {
        return path;
    }
    let closed = closed && n > 2;
    let segs = if closed { n } else { n - 1 };
    let at = |i: usize| pts[i % n];
    // A corner where fixed, and at the two ends of an open curve.
    let corner = |i: usize| fix[i % n] || (!closed && (i == 0 || i == n - 1));
    // The direction (per unit of chord) at a smooth point.
    let smooth_dir = |i: usize| -> Vec2 {
        let prev = at((i + n - 1) % n);
        let next = at(i + 1);
        let cur = at(i);
        let (d0, d1) = ((cur - prev).hypot(), (next - cur).hypot());
        (next - prev) / (d0 + d1).max(1e-12)
    };
    // The handle leaving point `i` along segment `i` and the one arriving
    // at its end.
    let handles = |i: usize| -> (Point, Point) {
        let (a, b) = (at(i), at(i + 1));
        let d = (b - a).hypot();
        let (ca, cb) = (corner(i), corner(i + 1));
        let chord = (b - a) / d.max(1e-12);
        let out_dir = if !ca { Some(smooth_dir(i)) } else { None };
        let in_dir = if !cb { Some(smooth_dir(i + 1)) } else { None };
        // At a corner the handle points so the segment bends as a
        // parabola through the smooth side: mirrored about the chord.
        let reflect = |m: Vec2| chord * (2.0 * m.dot(chord)) - m;
        let out = match (out_dir, in_dir) {
            (Some(m), _) => m,
            (None, Some(m)) => reflect(m),
            (None, None) => chord,
        };
        let inn = match (in_dir, out_dir) {
            (Some(m), _) => m,
            (None, Some(m)) => reflect(m),
            (None, None) => chord,
        };
        if ca && cb {
            return (a, b);
        }
        // Each handle along its direction, as long as a circular arc
        // leaving the chord at that angle needs: a third of the chord
        // over cos² of half the angle. A nearly straight piece keeps a
        // third of its chord, and a wide arc of three points comes out
        // as round as a short one (#3948; the handles shrank with the
        // turn instead, which flattened wide arcs). At most two thirds
        // of the chord, at a right angle.
        let handle = |m: Vec2| -> Vec2 {
            let len = m.hypot();
            if len < 1e-12 {
                return chord * (d / 3.0);
            }
            let unit = m / len;
            let cos = unit.dot(chord).clamp(0.0, 1.0);
            unit * (d / 3.0 * 2.0 / (1.0 + cos))
        };
        (a + handle(out), b - handle(inn))
    };
    for i in 0..segs {
        let (c1, c2) = handles(i);
        let b = at(i + 1);
        if c1 == at(i) && c2 == b {
            path.line_to(b);
        } else {
            path.curve_to(c1, c2, b);
        }
    }
    if closed {
        path.close_path();
    }
    path
}

/// A cubic B-spline over the control points `points` as Bezier
/// segments. Open: the uniform spline with each end point taken three
/// times, so it starts at the first point and ends at the last, its ends
/// tangent to the control polygon, and is made the same way all along:
/// a new end point placed where the last one is leaves the curve as it
/// was, so the curve drawn so far does not jump when one more point is
/// added (#2184). Two points make a line. Closed: the periodic spline,
/// smooth all round (quadratic with three points). Each knot span
/// becomes one segment, its control points the blossoms of the span's
/// ends.
pub fn bspline_path(points: &[Point], closed: bool) -> BezPath {
    let mut path = BezPath::new();
    let n = points.len();
    if n == 0 {
        return path;
    }
    if n == 1 {
        path.move_to(points[0]);
        return path;
    }
    if !closed && n > 2 {
        return open_bspline_path(points);
    }
    let deg = if closed { 3.min(n) } else { 3.min(n - 1) };
    if deg <= 1 {
        path.move_to(points[0]);
        for p in &points[1..] {
            path.line_to(*p);
        }
        if closed {
            path.close_path();
        }
        return path;
    }
    // Control points and knots.
    let (ctrl, knots): (Vec<Point>, Vec<f64>) = if closed {
        let mut c = points.to_vec();
        c.extend_from_slice(&points[..deg]);
        let k = (0..c.len() + deg + 1).map(|i| i as f64).collect();
        (c, k)
    } else {
        let mut k = vec![0.0; deg + 1];
        k.extend((1..n - deg).map(|i| i as f64));
        k.extend(std::iter::repeat_n((n - deg) as f64, deg + 1));
        (points.to_vec(), k)
    };
    let spans: Vec<usize> = if closed {
        (deg..deg + n).collect()
    } else {
        (deg..n).collect()
    };
    // The blossom f(us) on span `k` (de Boor with one parameter a level).
    let blossom = |k: usize, us: &[f64]| -> Point {
        let mut d: Vec<Point> = (0..=deg).map(|j| ctrl[j + k - deg]).collect();
        for r in 1..=deg {
            for j in (r..=deg).rev() {
                let lo = knots[j + k - deg];
                let hi = knots[j + 1 + k - r];
                let a = if hi > lo {
                    (us[r - 1] - lo) / (hi - lo)
                } else {
                    0.0
                };
                d[j] = d[j - 1].lerp(d[j], a);
            }
        }
        d[deg]
    };
    let mut first = true;
    for k in spans {
        let (a, b) = (knots[k], knots[k + 1]);
        if b <= a {
            continue;
        }
        if deg == 3 {
            let p0 = blossom(k, &[a, a, a]);
            if first {
                path.move_to(p0);
                first = false;
            }
            path.curve_to(
                blossom(k, &[a, a, b]),
                blossom(k, &[a, b, b]),
                blossom(k, &[b, b, b]),
            );
        } else {
            let p0 = blossom(k, &[a, a]);
            if first {
                path.move_to(p0);
                first = false;
            }
            path.quad_to(blossom(k, &[a, b]), blossom(k, &[b, b]));
        }
    }
    if closed {
        path.close_path();
    }
    path
}

/// The open uniform cubic B-spline over `points` (three or more) with
/// both end points taken three times: one Bezier segment for each four
/// control points in a row, segments of no length left out.
fn open_bspline_path(points: &[Point]) -> BezPath {
    let n = points.len();
    let get = |i: usize| points[i.saturating_sub(2).min(n - 1)];
    let mut path = BezPath::new();
    path.move_to(points[0]);
    for i in 0..n + 1 {
        let (a, b, c, d) = (get(i), get(i + 1), get(i + 2), get(i + 3));
        let p1 = b.lerp(c, 1.0 / 3.0);
        let p2 = b.lerp(c, 2.0 / 3.0);
        let p3 = ((b.to_vec2() + c.to_vec2() * 4.0 + d.to_vec2()) / 6.0).to_point();
        let p0 = ((a.to_vec2() + b.to_vec2() * 4.0 + c.to_vec2()) / 6.0).to_point();
        if p0 == p3 && p1 == p0 && p2 == p0 {
            continue;
        }
        path.curve_to(p1, p2, p3);
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #2451: a polygon's sides bent alike bow out (in when negative) by
    /// the same curve each, as far at every side's middle; unbent it is
    /// the plain polygon, and the bend is kept in a native file only when
    /// it is not 0.
    #[test]
    fn bent_polygon_sides_all_take_the_same_curve() {
        let r = Rect::new(-10.0, -10.0, 10.0, 10.0);
        assert_eq!(
            bent_polygon_path(r, 6, 0.0, 0.0, 0.0),
            twisted_polygon_path(r, 6, 0.0, 0.0)
        );
        let mid = |bend: f64| -> Vec<f64> {
            bent_polygon_path(r, 6, 0.0, 0.0, bend)
                .segments()
                .map(|s| kurbo::ParamCurve::eval(&s, 0.5).to_vec2().hypot())
                .collect()
        };
        let flat = mid(0.0);
        let out = mid(0.2);
        let inward = mid(-0.2);
        assert_eq!(out.len(), 6);
        for k in 0..6 {
            assert!((out[k] - out[0]).abs() < 1e-9 && (inward[k] - inward[0]).abs() < 1e-9);
        }
        // A side 10 long, its middle 0.75 * 0.2 * 10 = 1.5 further out.
        assert!((out[0] - flat[0] - 1.5).abs() < 1e-6, "{out:?} {flat:?}");
        assert!(
            (flat[0] - inward[0] - 1.5).abs() < 1e-6,
            "{inward:?} {flat:?}"
        );
        // A star's sides bend alike too: its outline stays mirror symmetric.
        let star: Vec<f64> = bent_polygon_path(r, 5, 0.5, 0.0, 0.3)
            .segments()
            .map(|s| kurbo::ParamCurve::eval(&s, 0.5).to_vec2().hypot())
            .collect();
        for k in 0..10 {
            assert!((star[k] - star[0]).abs() < 1e-9, "{star:?}");
        }
        let kind = crate::document::ShapeKind::Polygon {
            rect: r,
            points: 6,
            sharpness: 0.0,
            complex: None,
            twist: 0.0,
            bend: 0.0,
        };
        assert!(!serde_json::to_string(&kind).unwrap().contains("bend"));
        let bent = crate::document::ShapeKind::Polygon {
            rect: r,
            points: 6,
            sharpness: 0.0,
            complex: None,
            twist: 0.0,
            bend: 0.25,
        };
        let json = serde_json::to_string(&bent).unwrap();
        assert!(json.contains("\"bend\":0.25"), "{json}");
        let back: crate::document::ShapeKind = serde_json::from_str(&json).unwrap();
        assert_eq!(back, bent);
    }

    /// The corner points of a closed polygon path, in order.
    fn corners(p: &BezPath) -> Vec<Point> {
        p.elements()
            .iter()
            .filter_map(|e| match e {
                PathEl::MoveTo(p) | PathEl::LineTo(p) => Some(*p),
                _ => None,
            })
            .collect()
    }

    /// An ellipse is one closed curve of four nodes at the right, top,
    /// left and bottom, each with equal handles, whatever its size (#625).
    #[test]
    fn an_ellipse_is_closed_with_four_nodes_on_its_axes() {
        for rect in [
            Rect::new(10.0, 20.0, 70.0, 50.0),
            Rect::new(0.0, 0.0, 400.0, 900.0),
            Rect::new(-3.0, -3.0, 3.0, 3.0),
        ] {
            let p = ellipse_path(rect);
            assert_eq!(p.elements().last(), Some(&PathEl::ClosePath), "{p:?}");
            let nodes = crate::nodes::nodes(&p);
            assert_eq!(nodes.len(), 4, "{p:?}");
            let c = rect.center();
            let (rx, ry) = (rect.width() / 2.0, rect.height() / 2.0);
            let at: Vec<Point> = nodes.iter().map(|n| n.pos).collect();
            assert_eq!(
                at,
                vec![
                    Point::new(c.x + rx, c.y),
                    Point::new(c.x, c.y + ry),
                    Point::new(c.x - rx, c.y),
                    Point::new(c.x, c.y - ry),
                ]
            );
            for n in &nodes {
                let (i, o) = (n.ctrl_in.expect("in"), n.ctrl_out.expect("out"));
                assert!(((i - n.pos) + (o - n.pos)).hypot() < 1e-9, "{n:?}");
            }
            // On the ellipse within a hair everywhere.
            for seg in p.segments() {
                for k in 0..=10 {
                    let q = seg.eval(k as f64 / 10.0);
                    let r = ((q.x - c.x) / rx).powi(2) + ((q.y - c.y) / ry).powi(2);
                    assert!((r.sqrt() - 1.0).abs() < 3e-4, "{q:?}");
                }
            }
        }
    }

    /// The Curvature Pen's curve passes through every point, smooth at
    /// each (the handles on either side in line), with a corner at a
    /// fixed point; two points make a line; closed, it runs on round.
    #[test]
    fn a_curvature_curve_runs_through_its_points() {
        let pts = [
            Point::new(0.0, 0.0),
            Point::new(10.0, 10.0),
            Point::new(20.0, 0.0),
            Point::new(30.0, 10.0),
        ];
        let p = curvature_path(&pts, &[], false);
        let els = p.elements();
        assert_eq!(els.len(), 4);
        let ends: Vec<Point> = els.iter().filter_map(|e| e.end_point()).collect();
        assert_eq!(ends, pts.to_vec());
        // Smooth at the inner points: the arriving and leaving handles
        // lie on one line through the point.
        for k in 1..3 {
            let (PathEl::CurveTo(_, c2, at), PathEl::CurveTo(c1, _, _)) = (els[k], els[k + 1])
            else {
                panic!("curves");
            };
            let cross = (at - c2).cross(c1 - at);
            assert!(cross.abs() < 1e-9, "kink at {k}: {cross}");
            assert!((c1 - at).dot(at - c2) > 0.0);
        }
        // A fixed point: a corner there.
        let mut five = pts.to_vec();
        five.push(Point::new(40.0, 0.0));
        let q = curvature_path(&five, &[false, false, true], false);
        let (PathEl::CurveTo(_, c2, at), PathEl::CurveTo(c1, _, _)) =
            (q.elements()[2], q.elements()[3])
        else {
            panic!("curves");
        };
        let (a, b) = (at - c2, c1 - at);
        assert!(
            a.cross(b).abs() > 1.0 || a.dot(b) < 0.0,
            "a corner at the fixed point"
        );
        // Two points: a line.
        let two = curvature_path(&pts[..2], &[], false);
        assert!(matches!(two.elements()[1], PathEl::LineTo(_)));
        // Closed: one more segment back to the first point, then closed.
        let c = curvature_path(&pts, &[], true);
        assert_eq!(c.elements().len(), 6);
        assert!(matches!(c.elements()[5], PathEl::ClosePath));
        assert_eq!(c.elements()[4].end_point(), Some(pts[0]));
    }

    /// Three points on a wide arc make a round arc, as round as three on
    /// a short one: a half circle of radius 100 through its ends and
    /// top stays within 0.1 mm of the circle (#3948; it came out about
    /// 3 mm flat with handles a third of the chord). Five points on a
    /// three-quarter circle stay round too.
    #[test]
    fn a_wide_curvature_arc_stays_round() {
        let off = |p: &BezPath| -> f64 {
            let mut worst: f64 = 0.0;
            for seg in p.segments() {
                for k in 0..=20 {
                    let q = seg.eval(k as f64 / 20.0);
                    worst = worst.max((q.to_vec2().hypot() - 100.0).abs());
                }
            }
            worst
        };
        let half = [
            Point::new(-100.0, 0.0),
            Point::new(0.0, 100.0),
            Point::new(100.0, 0.0),
        ];
        let p = curvature_path(&half, &[], false);
        assert!(off(&p) < 0.1, "half circle {}", off(&p));
        let arc: Vec<Point> = (0..5)
            .map(|k| {
                let a = std::f64::consts::PI * (1.5 * k as f64 / 4.0);
                Point::new(100.0 * a.cos(), 100.0 * a.sin())
            })
            .collect();
        let p = curvature_path(&arc, &[], false);
        assert!(off(&p) < 0.1, "three quarters {}", off(&p));
        // A short, nearly straight run is as before: handles a third of
        // the chord.
        let flat = [
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.1),
            Point::new(20.0, 0.0),
        ];
        let PathEl::CurveTo(c1, _, _) = curvature_path(&flat, &[], false).elements()[1] else {
            panic!("a curve");
        };
        assert!(((c1 - flat[0]).hypot() - 10.0 / 3.0).abs() < 0.01);
    }

    /// A fixed control point is on the curve, a floating one is not; an
    /// open spline's ends are on it either way (#1758).
    #[test]
    fn a_fixed_bspline_point_is_on_the_curve() {
        let pts = [
            Point::new(0.0, 0.0),
            Point::new(10.0, 20.0),
            Point::new(30.0, 20.0),
            Point::new(40.0, 0.0),
            Point::new(60.0, 10.0),
        ];
        let dist = |path: &BezPath, q: Point| {
            path.segments()
                .flat_map(|s| (0..=200).map(move |k| s.eval(k as f64 / 200.0)))
                .map(|p| (p - q).hypot())
                .fold(f64::INFINITY, f64::min)
        };
        let floating = bspline_path(&pts, false);
        assert!(dist(&floating, pts[2]) > 1.0);
        let fixed = bspline_path_fixed(&pts, &[false, false, true], false);
        assert!(dist(&fixed, pts[2]) < 1e-6, "through the fixed point");
        assert!(dist(&fixed, pts[1]) > 1.0, "not the floating one");
        assert!((fixed.segments().next().expect("seg").start() - pts[0]).hypot() < 1e-9);
        // No flag set: the plain spline.
        assert_eq!(bspline_path_fixed(&pts, &[false; 5], false), floating);
        // Closed too.
        let closed = bspline_path_fixed(&pts, &[false, true], true);
        assert!(dist(&closed, pts[1]) < 1e-6);
        assert!(dist(&bspline_path(&pts, true), pts[1]) > 1.0);
    }

    #[test]
    fn bsplines_start_and_end_on_their_ends_and_stay_inside() {
        let pts = [
            Point::new(0.0, 0.0),
            Point::new(10.0, 20.0),
            Point::new(30.0, 20.0),
            Point::new(40.0, 0.0),
            Point::new(60.0, 10.0),
        ];
        let p = bspline_path(&pts, false);
        let segs: Vec<_> = p.segments().collect();
        assert_eq!(segs.len(), 6, "one cubic a span, ends taken three times");
        assert!((segs[0].start() - pts[0]).hypot() < 1e-9);
        assert!((segs[5].end() - pts[4]).hypot() < 1e-9);
        // Smooth where the segments meet, and inside the control points' box.
        let b = p.bounding_box();
        assert!(b.x0 >= -1e-9 && b.x1 <= 60.0 + 1e-9 && b.y0 >= -1e-9 && b.y1 <= 20.0 + 1e-9);
        for w in segs.windows(2) {
            if let (PathSeg::Cubic(c0), PathSeg::Cubic(c1)) = (w[0], w[1]) {
                let t0 = c0.p3 - c0.p2;
                let t1 = c1.p1 - c1.p0;
                assert!(
                    (t0.x * t1.y - t0.y * t1.x).abs() < 1e-6,
                    "tangent continuity"
                );
            } else {
                panic!("cubic segments");
            }
        }
        // Tangent to the control polygon at the ends.
        if let (PathSeg::Cubic(c0), PathSeg::Cubic(c5)) = (segs[0], segs[5]) {
            let (t, e) = (c0.p3 - c0.p0, pts[1] - pts[0]);
            assert!((t.x * e.y - t.y * e.x).abs() < 1e-6);
            let (t, e) = (c5.p3 - c5.p0, pts[4] - pts[3]);
            assert!((t.x * e.y - t.y * e.x).abs() < 1e-6);
        }
        // Closed: one segment a point, ending where it began.
        let p = bspline_path(&pts, true);
        let segs: Vec<_> = p.segments().collect();
        assert_eq!(segs.len(), 5);
        assert!((segs[4].end() - segs[0].start()).hypot() < 1e-9);
        // Two points: a line; three: cubics from end to end.
        assert_eq!(bspline_path(&pts[..2], false).segments().count(), 1);
        let three = bspline_path(&pts[..3], false);
        assert!(three.segments().all(|s| matches!(s, PathSeg::Cubic(_))));
        assert!((three.segments().last().expect("seg").end() - pts[2]).hypot() < 1e-9);
    }

    /// The curve drawn so far stays as it is when one more point is
    /// added where the last one is, as when the pointer leaves a point
    /// just placed with the B-spline tool, and a new point moved off
    /// changes only the last stretches, by little for a little move
    /// (#2184).
    #[test]
    fn a_new_bspline_end_point_does_not_move_the_curve_before_it() {
        let pts = [
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(20.0, 10.0),
            Point::new(30.0, 0.0),
            Point::new(40.0, 10.0),
        ];
        // (Two points make a line, which three in a row follow.)
        for k in 3..=pts.len() {
            let before = bspline_path(&pts[..k], false);
            let mut more = pts[..k].to_vec();
            more.push(pts[k - 1]);
            assert_eq!(bspline_path(&more, false), before, "{k} points");
        }
        // The pointer a little way off: every segment but the last three
        // is the same, and those move by no more than the pointer did.
        let before: Vec<PathSeg> = bspline_path(&pts, false).segments().collect();
        let mut more = pts.to_vec();
        more.push(pts[4] + Vec2::new(0.5, 0.0));
        let after: Vec<PathSeg> = bspline_path(&more, false).segments().collect();
        let same = before.len() - 2;
        assert_eq!(after[..same], before[..same]);
        for k in 0..=20 {
            let t = k as f64 / 20.0;
            let d = (after[same].eval(t) - before[same].eval(t)).hypot();
            assert!(d <= 0.5, "{d}");
        }
    }

    #[test]
    fn twisting_turns_the_points_between_the_vertices() {
        let r = Rect::new(-10.0, -10.0, 10.0, 10.0);
        let star = corners(&polygon_path(r, 5, 0.5));
        let turn = 0.3;
        let twisted = corners(&twisted_polygon_path(r, 5, 0.5, turn));
        assert_eq!((star.len(), twisted.len()), (10, 10));
        for (i, (a, b)) in star.iter().zip(&twisted).enumerate() {
            if i % 2 == 0 {
                // The vertices stay.
                assert!((*a - *b).hypot() < 1e-9, "{i}");
            } else {
                // The points between them keep their distance and turn.
                assert!((a.to_vec2().hypot() - b.to_vec2().hypot()).abs() < 1e-9);
                let da = b.y.atan2(b.x) - a.y.atan2(a.x);
                let da = (da + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU)
                    - std::f64::consts::PI;
                assert!((da - turn).abs() < 1e-9, "{i}: {da}");
            }
        }
        // A plain polygon turned gains points where its sides' middles were.
        let plain = corners(&twisted_polygon_path(r, 6, 0.0, 0.2));
        assert_eq!(plain.len(), 12);
        let mid = (std::f64::consts::PI / 6.0).cos() * 10.0;
        assert!((plain[1].to_vec2().hypot() - mid).abs() < 1e-9);
        // Untwisted is the polygon as before; nonsense counts as untwisted.
        assert_eq!(corners(&twisted_polygon_path(r, 6, 0.0, 0.0)).len(), 6);
        assert_eq!(corners(&twisted_polygon_path(r, 5, 0.5, f64::NAN)), star);
    }

    #[test]
    fn rect_path_has_bounds() {
        let r = Rect::new(10.0, 20.0, 110.0, 70.0);
        let p = rect_path(r, 0.0);
        let b = p.bounding_box();
        assert!((b.x0 - 10.0).abs() < 1e-9 && (b.y1 - 70.0).abs() < 1e-9);
    }

    #[test]
    fn simplify_drops_collinear_points() {
        let pts: Vec<Point> = (0..10).map(|i| Point::new(i as f64, 0.0)).collect();
        assert_eq!(simplify(&pts, 0.1).len(), 2);
        let smooth = smooth_path(&pts, false);
        assert_eq!(smooth.elements().len(), 10);
    }

    #[test]
    fn closed_smooth_curves_wrap_around_without_a_corner() {
        let pts = [
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
            Point::new(0.0, 10.0),
        ];
        let p = smooth_path(&pts, true);
        // Four curves (the last one back to the start) and the close.
        let curves: Vec<(Point, Point, Point)> = p
            .elements()
            .iter()
            .filter_map(|e| match e {
                PathEl::CurveTo(a, b, c) => Some((*a, *b, *c)),
                _ => None,
            })
            .collect();
        assert_eq!(curves.len(), 4);
        assert_eq!(curves[3].2, pts[0]);
        // The tangent into the start equals the tangent out of it.
        let into = pts[0] - curves[3].1;
        let out = curves[0].0 - pts[0];
        assert!((into.x - out.x).abs() < 1e-9 && (into.y - out.y).abs() < 1e-9);
        // A closing point equal to the first changes nothing.
        let mut again = pts.to_vec();
        again.push(pts[0]);
        assert_eq!(smooth_path(&again, true), p);
    }

    #[test]
    fn star_has_twice_the_points() {
        let p = polygon_path(Rect::new(0.0, 0.0, 10.0, 10.0), 5, 0.5);
        let n = p
            .elements()
            .iter()
            .filter(|e| matches!(e, PathEl::LineTo(_)))
            .count();
        assert_eq!(n, 9);
    }

    /// #1331: a negative sharpness puts the inner points past the centre.
    #[test]
    fn negative_sharpness_inverts_the_star() {
        let r = Rect::new(-10.0, -10.0, 10.0, 10.0);
        let pts = |s: f64| -> Vec<Point> {
            polygon_path(r, 5, s)
                .elements()
                .iter()
                .filter_map(|e| e.end_point())
                .collect()
        };
        let (plain, inverted) = (pts(0.7), pts(-0.7));
        assert_eq!(inverted.len(), 10);
        for i in 0..10 {
            if i % 2 == 0 {
                // The vertices stay.
                assert!((plain[i] - inverted[i]).hypot() < 1e-9);
            } else {
                // Each inner point mirrored through the centre.
                assert!((plain[i].to_vec2() + inverted[i].to_vec2()).hypot() < 1e-9);
            }
        }
        assert!((star_inner_ratio(-0.7) + 0.3).abs() < 1e-12);
        assert!((star_inner_ratio(0.7) - 0.3).abs() < 1e-12);
        assert_eq!(star_inner_ratio(-1.0), 0.0);
        for s in [0.001, 0.3, 0.999, -0.001, -0.3, -0.999] {
            assert!((star_sharpness_for_ratio(star_inner_ratio(s)) - s).abs() < 1e-12);
        }
        // Kept a star on both sides, never 0.
        assert_eq!(star_sharpness_for_ratio(2.0), 0.001);
        assert_eq!(star_sharpness_for_ratio(-2.0), -0.001);
        assert_eq!(star_sharpness_for_ratio(0.0), 0.999);
        assert!(is_star_sharpness(-0.5) && !is_star_sharpness(0.0));
        assert!(!is_star_sharpness(f64::NAN));
    }

    #[test]
    fn polygons_point_up() {
        // First vertex at the top centre (y up).
        let p = polygon_path(Rect::new(0.0, 0.0, 10.0, 10.0), 5, 0.0);
        match p.elements()[0] {
            PathEl::MoveTo(q) => assert!((q.x - 5.0).abs() < 1e-9 && (q.y - 10.0).abs() < 1e-9),
            ref e => panic!("{e:?}"),
        }
    }

    #[test]
    fn polygons_of_billions_of_points_draw_the_most_points() {
        // A file or a macro asked for 4 billion points: drawing the
        // polygon ran out of memory and ended the program.
        let r = Rect::new(0.0, 0.0, 10.0, 10.0);
        for (sharpness, twist, per_point) in [(0.0, 0.0, 1), (0.5, 0.0, 2), (0.0, 0.3, 2)] {
            let p = twisted_polygon_path(r, u32::MAX, sharpness, twist);
            let n = POLYGON_MAX_POINTS as usize * per_point;
            assert_eq!(p.elements().len(), n + 1, "{sharpness} {twist}");
        }
        assert_eq!(polygon_path(r, 7, 0.0).elements().len(), 8);
    }

    #[test]
    fn complex_stars_cross_their_sides() {
        let r = Rect::new(0.0, 0.0, 10.0, 10.0);
        let count =
            |p: &BezPath, f: fn(&PathEl) -> bool| p.elements().iter().filter(|e| f(e)).count();
        // A pentagram: one subpath of five vertices, every second one.
        let p = complex_star_path(r, 5, 1);
        assert_eq!(count(&p, |e| matches!(e, PathEl::MoveTo(_))), 1);
        assert_eq!(count(&p, |e| matches!(e, PathEl::LineTo(_))), 4);
        let pts: Vec<Point> = p
            .elements()
            .iter()
            .filter_map(|e| match e {
                PathEl::MoveTo(q) | PathEl::LineTo(q) => Some(*q),
                _ => None,
            })
            .collect();
        let verts = polygon_path(r, 5, 0.0);
        let PathEl::LineTo(v2) = verts.elements()[2] else {
            panic!("{verts:?}");
        };
        assert!((pts[1] - v2).hypot() < 1e-9, "{pts:?}");
        // Filled even-odd, the middle pentagon is a hole.
        assert!(p.winding(r.center()).abs() == 2);
        // Six points: two triangles.
        let p = complex_star_path(r, 6, 1);
        assert_eq!(count(&p, |e| matches!(e, PathEl::MoveTo(_))), 2);
        // Sharpness follows the point count.
        assert_eq!(complex_star_max_sharpness(5), 1);
        assert_eq!(complex_star_max_sharpness(9), 3);
        assert_eq!(complex_star_max_sharpness(12), 4);
        assert_eq!(complex_star_max_sharpness(0), 1);
        // Out of range values are brought in.
        assert_eq!(complex_star_path(r, 9, 50), complex_star_path(r, 9, 3));
        assert_eq!(complex_star_path(r, 1, 1), complex_star_path(r, 5, 1));
    }

    #[test]
    fn polygon_fills_the_dragged_box() {
        let drag = Rect::new(20.0, 10.0, 70.0, 40.0);
        for (n, sharp) in [(3, 0.0), (5, 0.0), (6, 0.0), (5, 0.5), (3, 0.2), (12, 0.3)] {
            let r = polygon_rect_for_bounds(drag, n, sharp);
            let b = polygon_path(r, n, sharp).bounding_box();
            assert!(
                (b.x0 - drag.x0).abs() < 1e-9
                    && (b.y0 - drag.y0).abs() < 1e-9
                    && (b.x1 - drag.x1).abs() < 1e-9
                    && (b.y1 - drag.y1).abs() < 1e-9,
                "{n} {sharp}: {b:?}"
            );
        }
    }

    /// A ring sector is the band between the outline and the inner
    /// ellipse: its area is the pie's less the inner pie's, a whole turn
    /// leaves the middle empty under both fill rules, and no inner radius
    /// is the pie.
    #[test]
    fn a_ring_sector_is_the_band_between_two_ellipses() {
        let r = Rect::new(-10.0, -10.0, 10.0, 10.0);
        let quarter = ellipse_ring_path(r, 0.0, 90.0, 0.5);
        let want = std::f64::consts::PI * (100.0 - 25.0) / 4.0;
        assert!((quarter.area().abs() - want).abs() < 0.2, "{}", quarter.area());
        for (x, y, inside) in [(7.0, 3.0, true), (2.0, 2.0, false), (-7.0, 3.0, false)] {
            let w = quarter.winding(Point::new(x, y));
            assert_eq!(w != 0, inside, "({x}, {y}) winding {w}");
        }
        let ring = ellipse_ring_path(r, 30.0, 30.0, 0.5);
        assert_eq!(ring.winding(Point::new(0.0, 0.0)), 0, "middle is empty");
        assert_ne!(ring.winding(Point::new(0.0, 7.5)), 0, "band is filled");
        let want = std::f64::consts::PI * (100.0 - 25.0);
        assert!((ring.area().abs() - want).abs() < 0.5, "{}", ring.area());
        assert_eq!(
            ellipse_ring_path(r, 0.0, 90.0, 0.0),
            ellipse_arc_path(r, 0.0, 90.0, true)
        );
        assert_eq!(
            ellipse_ring_path(r, 0.0, 90.0, f64::NAN),
            ellipse_arc_path(r, 0.0, 90.0, true)
        );
    }

    /// An arc with a huge radius, as a damaged file gives, is a handful of
    /// curves: the pieces once grew with the radius and an arc of 1e300
    /// asked for more than memory holds. An arc of drawing size keeps its
    /// pieces as they were.
    #[test]
    fn an_arc_of_huge_radius_is_a_handful_of_curves() {
        for r in [1e300, 1e60, f64::INFINITY, f64::NAN] {
            let p = ellipse_arc_path(Rect::new(-r, -r, r, r), 0.0, 270.0, false);
            assert!(p.elements().len() < 64, "{r}: {}", p.elements().len());
        }
        let small = ellipse_arc_path(Rect::new(-50.0, -50.0, 50.0, 50.0), 0.0, 270.0, false);
        let pieces = |r: f64| {
            let arc = kurbo::Arc::new(
                Point::ZERO,
                kurbo::Vec2::new(r, r),
                0.0,
                1.5 * std::f64::consts::PI,
                0.0,
            );
            arc.append_iter(0.1).count()
        };
        assert_eq!(small.elements().len(), 1 + pieces(50.0));
    }
}
