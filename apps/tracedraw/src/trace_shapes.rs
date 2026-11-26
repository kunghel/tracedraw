//! Geometric shapes for the outline trace (the Geometric shapes box of
//! the Trace dialog, [`crate::trace::Settings::geometric`]). See
//! docs/behavior/bitmap-tracing.md, "Geometric shapes".
//!
//! A border all round one region (a ring with no third colour on it) is
//! measured against a whole circle, a whole ellipse and a whole rectangle
//! with rounded corners (square corners when the rounding is under half a
//! pixel); the simplest that lies within the tolerance all round takes the
//! border's place, so a round dot is a true circle and a badge a true
//! rounded rectangle. A ring that is none of them but mirrors itself
//! across the upright or level line through its middle is made exactly
//! symmetric before it is fitted. Both work on the shared border, so the
//! two regions either side of it still meet along one curve, with no gap.

use kurbo::{Affine, BezPath, ParamCurve, ParamCurveArclen, PathEl, Point, Vec2};

/// The least the tolerance is, in pixels of the art.
const TOLERANCE: f64 = 0.75;
/// The share of a shape's size its border may stray from the whole
/// shape (a wobble drawn by hand is straightened).
const TOLERANCE_SHARE: f64 = 0.012;
/// The root mean square of the stray, as a share of the tolerance.
const MEAN_SHARE: f64 = 0.45;
/// The share of the points that must lie within the tolerance, and how
/// many times the tolerance the rest may stray (a speck of noise on the
/// edge).
const MOST_POINTS: f64 = 0.95;
const BUMP_SHARE: f64 = 2.5;
/// How far beyond the tolerance the border's own steps may lie (pixels
/// of the art): the staircase of its grid.
const STEP_SLACK: f64 = 0.5;
/// The smallest shape (its narrower side, pixels of the art) made whole.
const SMALLEST: f64 = 4.0;
/// A rounding under this (pixels of the art) is a square corner.
const SQUARE_CORNER: f64 = 0.5;
/// An ellipse or rectangle this close to level or upright (degrees) is
/// set level or upright.
const LEVEL_DEGREES: f64 = 1.5;
/// The longest a border measured for symmetry is (points): longer ones
/// are left as they are, to keep the trace quick.
const MIRROR_MOST: usize = 6000;
/// How far a mirrored point may lie from the border (pixels of the art),
/// at most and as a root mean square, for the border to be symmetric.
const MIRROR_MOST_OFF: f64 = 1.0;
const MIRROR_MEAN_OFF: f64 = 0.35;
/// The share of a rectangle's half side, round its middle, whose points
/// say where the side runs.
const MIDDLE_SHARE: f64 = 0.3;
/// The cubic that draws a quarter circle of radius 1.
const KAPPA: f64 = 0.552_284_749_830_793_4;

/// What a whole shape is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Whole {
    Circle {
        center: Point,
        radius: f64,
    },
    Ellipse {
        center: Point,
        radii: Vec2,
        angle: f64,
    },
    /// Half its width and height, the rounding of its corners.
    Rect {
        center: Point,
        half: Vec2,
        radius: f64,
        angle: f64,
    },
}

impl Whole {
    /// The shape as a closed path, clockwise on screen (y down) when
    /// `clockwise`.
    pub(crate) fn path(&self, clockwise: bool) -> BezPath {
        let (local, place) = match *self {
            Whole::Circle { center, radius } => (
                rounded_box(radius, radius, radius),
                Affine::translate(center.to_vec2()),
            ),
            Whole::Ellipse {
                center,
                radii,
                angle,
            } => (
                rounded_box(1.0, 1.0, 1.0),
                Affine::translate(center.to_vec2())
                    * Affine::rotate(angle)
                    * Affine::scale_non_uniform(radii.x, radii.y),
            ),
            Whole::Rect {
                center,
                half,
                radius,
                angle,
            } => (
                rounded_box(half.x, half.y, radius),
                Affine::translate(center.to_vec2()) * Affine::rotate(angle),
            ),
        };
        let path = place * local;
        if clockwise {
            path
        } else {
            path.reverse_subpaths()
        }
    }

    /// How far `p` lies from the shape's outline.
    fn off(&self, p: Point) -> f64 {
        match *self {
            Whole::Circle { center, radius } => ((p - center).hypot() - radius).abs(),
            Whole::Ellipse {
                center,
                radii,
                angle,
            } => {
                let q = Affine::rotate(-angle) * (p - center).to_point();
                let rho = ((q.x / radii.x).powi(2) + (q.y / radii.y).powi(2)).sqrt();
                if rho < 1e-9 {
                    return radii.x.min(radii.y);
                }
                q.to_vec2().hypot() * (1.0 - 1.0 / rho).abs()
            }
            Whole::Rect {
                center,
                half,
                radius,
                angle,
            } => {
                let q = Affine::rotate(-angle) * (p - center).to_point();
                box_distance(q, half, radius).abs()
            }
        }
    }

    /// How many nodes the shape takes, to prefer the simpler.
    fn rank(&self) -> u8 {
        match self {
            Whole::Circle { .. } => 0,
            Whole::Ellipse { .. } => 1,
            Whole::Rect { radius, .. } if *radius <= 0.0 => 2,
            Whole::Rect { .. } => 3,
        }
    }
}

/// A box of half width `hx` and half height `hy` round the origin with
/// corners rounded by `r`, clockwise on screen from the top of its right
/// side.
fn rounded_box(hx: f64, hy: f64, r: f64) -> BezPath {
    let r = r.clamp(0.0, hx.min(hy));
    let k = r * KAPPA;
    let mut p = BezPath::new();
    p.move_to((hx, -hy + r));
    p.line_to((hx, hy - r));
    if r > 0.0 {
        p.curve_to((hx, hy - r + k), (hx - r + k, hy), (hx - r, hy));
    }
    p.line_to((-hx + r, hy));
    if r > 0.0 {
        p.curve_to((-hx + r - k, hy), (-hx, hy - r + k), (-hx, hy - r));
    }
    p.line_to((-hx, -hy + r));
    if r > 0.0 {
        p.curve_to((-hx, -hy + r - k), (-hx + r - k, -hy), (-hx + r, -hy));
    }
    p.line_to((hx - r, -hy));
    if r > 0.0 {
        p.curve_to((hx - r + k, -hy), (hx, -hy + r - k), (hx, -hy + r));
    }
    p.close_path();
    // Lines of no length (where the rounding takes a whole side) out.
    let mut out = BezPath::new();
    let mut at = Point::ORIGIN;
    for el in p.elements() {
        match *el {
            PathEl::LineTo(q) if (q - at).hypot() < 1e-9 => {}
            PathEl::MoveTo(q) | PathEl::LineTo(q) | PathEl::CurveTo(_, _, q) => {
                out.push(*el);
                at = q;
            }
            _ => out.push(*el),
        }
    }
    out
}

/// The signed distance from `q` to a box of half size `half` round the
/// origin with corners rounded by `r` (negative inside).
fn box_distance(q: Point, half: Vec2, r: f64) -> f64 {
    let dx = q.x.abs() - half.x + r;
    let dy = q.y.abs() - half.y + r;
    let outside = Vec2::new(dx.max(0.0), dy.max(0.0)).hypot();
    outside + dx.max(dy).min(0.0) - r
}

/// Points along `path` about `step` apart, its first subpath only.
pub(crate) fn sampled(path: &BezPath, step: f64) -> Vec<Point> {
    let mut out = Vec::new();
    let step = step.max(1e-3);
    for (i, seg) in path.segments().enumerate() {
        if i > 0 && out.len() > 200_000 {
            break;
        }
        let len = seg.arclen(0.01);
        let n = ((len / step).ceil() as usize).clamp(1, 4096);
        for k in 0..n {
            out.push(seg.eval(k as f64 / n as f64));
        }
    }
    out
}

/// Twice the signed area of the closed polygon `pts` (positive clockwise
/// on screen, y down) and its centroid.
fn area_centroid(pts: &[Point]) -> (f64, Point) {
    let n = pts.len();
    let (mut a, mut cx, mut cy) = (0.0, 0.0, 0.0);
    for i in 0..n {
        let (p, q) = (pts[i], pts[(i + 1) % n]);
        let c = p.x * q.y - q.x * p.y;
        a += c;
        cx += (p.x + q.x) * c;
        cy += (p.y + q.y) * c;
    }
    if a.abs() < 1e-12 {
        return (0.0, pts.first().copied().unwrap_or(Point::ORIGIN));
    }
    (a, Point::new(cx / (3.0 * a), cy / (3.0 * a)))
}

/// The second moments of the area inside the closed polygon `pts` about
/// `c`, per unit of area: (xx, yy, xy).
fn moments(pts: &[Point], c: Point, twice_area: f64) -> (f64, f64, f64) {
    let n = pts.len();
    let (mut xx, mut yy, mut xy) = (0.0, 0.0, 0.0);
    for i in 0..n {
        let p = pts[i] - c;
        let q = pts[(i + 1) % n] - c;
        let cr = p.x * q.y - q.x * p.y;
        xx += (p.x * p.x + p.x * q.x + q.x * q.x) * cr;
        yy += (p.y * p.y + p.y * q.y + q.y * q.y) * cr;
        xy += (p.x * q.y + 2.0 * p.x * p.y + 2.0 * q.x * q.y + q.x * p.y) * cr;
    }
    let a = twice_area / 2.0;
    (xx / 12.0 / a, yy / 12.0 / a, xy / 24.0 / a)
}

/// `angle` set level or upright when within [`LEVEL_DEGREES`] of it.
fn level(angle: f64) -> f64 {
    let quarter = std::f64::consts::FRAC_PI_2;
    let nearest = (angle / quarter).round() * quarter;
    if (angle - nearest).abs() <= LEVEL_DEGREES.to_radians() {
        nearest
    } else {
        angle
    }
}

/// The stray of `pts` from `shape`: the most, the root mean square and
/// the stray most points stay within ([`MOST_POINTS`] of them).
fn stray(shape: &Whole, pts: &[Point]) -> (f64, f64, f64) {
    let mut d: Vec<f64> = pts.iter().map(|p| shape.off(*p)).collect();
    let mean = (d.iter().map(|v| v * v).sum::<f64>() / d.len().max(1) as f64).sqrt();
    d.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let most = d.last().copied().unwrap_or(0.0);
    let k = ((d.len() as f64 * MOST_POINTS) as usize).min(d.len().saturating_sub(1));
    (most, mean, d.get(k).copied().unwrap_or(0.0))
}

/// Whether a stray of (most, mean, most points) is within `tolerance`:
/// most points within it, a speck of the picture's noise on the edge
/// within [`BUMP_SHARE`] of it.
fn within(s: (f64, f64, f64), tolerance: f64) -> bool {
    s.2 <= tolerance && s.0 <= BUMP_SHARE * tolerance && s.1 <= MEAN_SHARE * tolerance
}

/// The whole circle, ellipse or rounded rectangle the closed border
/// `border` (its points, `unit` per pixel of the art) and its fitted
/// curve `fitted` draw, when one lies within the tolerance of both; the
/// simplest one that does.
pub(crate) fn whole(border: &[Point], fitted: &BezPath, unit: f64) -> Option<Whole> {
    if border.len() < 8 || !unit.is_finite() || unit <= 0.0 {
        return None;
    }
    let subpaths = fitted
        .elements()
        .iter()
        .filter(|e| matches!(e, PathEl::MoveTo(_)))
        .count();
    if subpaths != 1 {
        return None;
    }
    let pts = sampled(fitted, 0.5 * unit);
    if pts.len() < 12 {
        return None;
    }
    let (twice, center) = area_centroid(&pts);
    if twice.abs() < 1e-9 {
        return None;
    }
    let (xx, yy, xy) = moments(&pts, center, twice);
    // The covariance's two directions.
    let mid = (xx + yy) / 2.0;
    let spread = (((xx - yy) / 2.0).powi(2) + xy * xy).sqrt();
    let (big, small) = (mid + spread, (mid - spread).max(0.0));
    if small <= 0.0 {
        return None;
    }
    let axis = 0.5 * (2.0 * xy).atan2(xx - yy);
    let (a, b) = (2.0 * big.sqrt(), 2.0 * small.sqrt());
    if b < SMALLEST * unit {
        return None;
    }
    let size = 2.0 * a;
    let tolerance = (TOLERANCE * unit).max(TOLERANCE_SHARE * size);
    let mut found: Vec<Whole> = Vec::new();
    // A circle: the mean distance from the middle.
    let radius = pts.iter().map(|p| (*p - center).hypot()).sum::<f64>() / pts.len() as f64;
    found.push(Whole::Circle { center, radius });
    found.push(Whole::Ellipse {
        center,
        radii: Vec2::new(a, b),
        angle: level(axis),
    });
    if let Some(r) = rect_of(&pts, unit) {
        found.push(r);
    }
    let border_tolerance = tolerance + STEP_SLACK * unit;
    found
        .into_iter()
        .filter(|w| {
            within(stray(w, &pts), tolerance)
                && within(stray(w, border), border_tolerance)
                && same_area(w, twice.abs() / 2.0, tolerance)
        })
        .min_by(|x, y| {
            x.rank().cmp(&y.rank()).then(
                stray(x, &pts)
                    .1
                    .partial_cmp(&stray(y, &pts).1)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
        })
}

/// Whether `shape` covers `area` give or take a band of `tolerance`
/// along its outline: the outline of a thin band bent round (an open
/// ring) lies near a circle all along, but covers next to nothing of it.
fn same_area(shape: &Whole, area: f64, tolerance: f64) -> bool {
    let outline = shape.path(true);
    let own = kurbo::Shape::area(&outline).abs();
    let around = kurbo::Shape::perimeter(&outline, 0.1);
    (own - area).abs() <= tolerance * around
}

/// The rounded rectangle `pts` (along a closed curve) fit best: turned
/// the way most of its sides run, as wide and high as they reach, its
/// corners rounded by the radius that strays least.
fn rect_of(pts: &[Point], unit: f64) -> Option<Whole> {
    let n = pts.len();
    // The way the sides run, a quarter turn the same: the mean of four
    // times each piece's angle, weighted by its length.
    let (mut s, mut c) = (0.0, 0.0);
    for i in 0..n {
        let d = pts[(i + 1) % n] - pts[i];
        let len = d.hypot();
        if len < 1e-9 {
            continue;
        }
        let t = 4.0 * d.y.atan2(d.x);
        s += len * t.sin();
        c += len * t.cos();
    }
    if s.hypot(c) < 1e-9 {
        return None;
    }
    let angle = level(s.atan2(c) / 4.0);
    let undo = Affine::rotate(-angle);
    let local: Vec<Point> = pts.iter().map(|p| undo * *p).collect();
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in &local {
        x0 = x0.min(p.x);
        y0 = y0.min(p.y);
        x1 = x1.max(p.x);
        y1 = y1.max(p.y);
    }
    // Each side where its middle third runs (the mean of its points
    // there), so a wobble at its furthest point does not widen it.
    let (bx, by) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let (rx, ry) = ((x1 - x0) / 2.0, (y1 - y0) / 2.0);
    let side = |pick: &dyn Fn(&Point) -> Option<f64>, fallback: f64| {
        let (mut sum, mut n) = (0.0, 0usize);
        for v in local.iter().filter_map(pick) {
            sum += v;
            n += 1;
        }
        if n == 0 {
            fallback
        } else {
            sum / n as f64
        }
    };
    let middle = MIDDLE_SHARE;
    let right = side(
        &|p| (p.x > bx && (p.y - by).abs() < middle * ry).then_some(p.x),
        x1,
    );
    let left = side(
        &|p| (p.x < bx && (p.y - by).abs() < middle * ry).then_some(p.x),
        x0,
    );
    let bottom = side(
        &|p| (p.y > by && (p.x - bx).abs() < middle * rx).then_some(p.y),
        y1,
    );
    let top = side(
        &|p| (p.y < by && (p.x - bx).abs() < middle * rx).then_some(p.y),
        y0,
    );
    let half = Vec2::new((right - left) / 2.0, (bottom - top) / 2.0);
    let mid = Point::new((left + right) / 2.0, (top + bottom) / 2.0);
    let most_r = half.x.min(half.y);
    if most_r < SMALLEST * unit / 2.0 {
        return None;
    }
    let mean = |r: f64| {
        let sum: f64 = local
            .iter()
            .map(|p| box_distance(*p - mid.to_vec2(), half, r).powi(2))
            .sum();
        sum / local.len() as f64
    };
    // Coarse steps, then a finer look round the best.
    let steps = 48;
    let mut best = (0.0, mean(0.0));
    for k in 1..=steps {
        let r = most_r * k as f64 / steps as f64;
        let m = mean(r);
        if m < best.1 {
            best = (r, m);
        }
    }
    let width = most_r / steps as f64;
    for k in -8..=8 {
        let r = (best.0 + width * k as f64 / 8.0).clamp(0.0, most_r);
        let m = mean(r);
        if m < best.1 {
            best = (r, m);
        }
    }
    let radius = if best.0 < SQUARE_CORNER * unit {
        0.0
    } else {
        best.0
    };
    Some(Whole::Rect {
        center: Affine::rotate(angle) * mid,
        half,
        radius,
        angle,
    })
}

/// The whole shape for the closed border `border`, its fitted curve
/// `fitted` and `unit` points per pixel of the art, as a path running
/// the same way round as `fitted`; `None` when it draws no whole shape.
pub(crate) fn snapped(border: &[Point], fitted: &BezPath, unit: f64) -> Option<BezPath> {
    let shape = whole(border, fitted, unit)?;
    let pts = sampled(fitted, 0.5 * unit);
    let (twice, _) = area_centroid(&pts);
    Some(shape.path(twice > 0.0))
}

/// The nearest point to `p` on the closed polygon `pts`, looking only at
/// the sides listed in the cells of `grid` round it.
struct Sides<'a> {
    pts: &'a [Point],
    origin: Point,
    cell: f64,
    across: usize,
    down: usize,
    cells: Vec<Vec<u32>>,
}

impl<'a> Sides<'a> {
    fn new(pts: &'a [Point], cell: f64) -> Sides<'a> {
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for p in pts {
            x0 = x0.min(p.x);
            y0 = y0.min(p.y);
            x1 = x1.max(p.x);
            y1 = y1.max(p.y);
        }
        let across = (((x1 - x0) / cell) as usize + 1).min(4096);
        let down = (((y1 - y0) / cell) as usize + 1).min(4096);
        let mut s = Sides {
            pts,
            origin: Point::new(x0, y0),
            cell,
            across,
            down,
            cells: vec![Vec::new(); across * down],
        };
        let n = pts.len();
        for i in 0..n {
            let (p, q) = (pts[i], pts[(i + 1) % n]);
            let (cx0, cy0) = s.cell_of(Point::new(p.x.min(q.x), p.y.min(q.y)));
            let (cx1, cy1) = s.cell_of(Point::new(p.x.max(q.x), p.y.max(q.y)));
            for cy in cy0..=cy1 {
                for cx in cx0..=cx1 {
                    s.cells[cy * across + cx].push(i as u32);
                }
            }
        }
        s
    }

    fn cell_of(&self, p: Point) -> (usize, usize) {
        let cx = ((p.x - self.origin.x) / self.cell).floor();
        let cy = ((p.y - self.origin.y) / self.cell).floor();
        (
            (cx.max(0.0) as usize).min(self.across - 1),
            (cy.max(0.0) as usize).min(self.down - 1),
        )
    }

    /// The nearest point within `reach` cells, if any.
    fn nearest(&self, p: Point, reach: usize) -> Option<Point> {
        let n = self.pts.len();
        let (cx, cy) = self.cell_of(p);
        let mut best: Option<(f64, Point)> = None;
        for y in cy.saturating_sub(reach)..=(cy + reach).min(self.down - 1) {
            for x in cx.saturating_sub(reach)..=(cx + reach).min(self.across - 1) {
                for i in &self.cells[y * self.across + x] {
                    let i = *i as usize;
                    let (a, b) = (self.pts[i], self.pts[(i + 1) % n]);
                    let ab = b - a;
                    let l2 = ab.hypot2();
                    let t = if l2 < 1e-12 {
                        0.0
                    } else {
                        ((p - a).dot(ab) / l2).clamp(0.0, 1.0)
                    };
                    let q = a + ab * t;
                    let d = (p - q).hypot2();
                    if best.is_none_or(|(bd, _)| d < bd) {
                        best = Some((d, q));
                    }
                }
            }
        }
        best.map(|(_, q)| q)
    }
}

/// The closed border `pts` (`unit` points per pixel of the art) made
/// exactly symmetric across the upright line through its middle, the
/// level one, or both, where it already nearly is; `None` when it is
/// neither.
pub(crate) fn mirrored(pts: &[Point], unit: f64) -> Option<Vec<Point>> {
    let n = pts.len();
    if !(12..=MIRROR_MOST).contains(&n) || !unit.is_finite() || unit <= 0.0 {
        return None;
    }
    let (twice, c) = area_centroid(pts);
    if twice.abs() < 1e-9 {
        return None;
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in pts {
        x0 = x0.min(p.x);
        y0 = y0.min(p.y);
        x1 = x1.max(p.x);
        y1 = y1.max(p.y);
    }
    if (x1 - x0).min(y1 - y0) < 2.0 * SMALLEST * unit {
        return None;
    }
    let mut cur = pts.to_vec();
    let mut changed = false;
    for upright in [true, false] {
        let flip = |p: Point| {
            if upright {
                Point::new(2.0 * c.x - p.x, p.y)
            } else {
                Point::new(p.x, 2.0 * c.y - p.y)
            }
        };
        let sides = Sides::new(&cur, 2.0 * unit);
        let reach = 1;
        let mut near = Vec::with_capacity(n);
        let (mut most, mut sum) = (0.0f64, 0.0);
        let mut ok = true;
        for p in &cur {
            let m = flip(*p);
            let Some(q) = sides.nearest(m, reach) else {
                ok = false;
                break;
            };
            let d = (m - q).hypot();
            most = most.max(d);
            sum += d * d;
            near.push(q);
        }
        if !ok || most > MIRROR_MOST_OFF * unit || (sum / n as f64).sqrt() > MIRROR_MEAN_OFF * unit
        {
            continue;
        }
        cur = cur
            .iter()
            .zip(&near)
            .map(|(p, q)| p.midpoint(flip(*q)))
            .collect();
        changed = true;
    }
    changed.then_some(cur)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The points of `path` a fine grid step apart, as the border would
    /// give them.
    fn border_of(path: &BezPath) -> Vec<Point> {
        sampled(path, 0.5)
    }

    fn wobbled(path: &BezPath, amount: f64) -> BezPath {
        // A path of short lines through the points, each pushed a little
        // out and in, as a border drawn by hand.
        let pts = sampled(path, 1.0);
        let mut out = BezPath::new();
        for (i, p) in pts.iter().enumerate() {
            let q = *p + Vec2::new((i as f64 * 0.7).sin(), (i as f64 * 1.3).cos()) * amount;
            if i == 0 {
                out.move_to(q);
            } else {
                out.line_to(q);
            }
        }
        out.close_path();
        out
    }

    #[test]
    fn a_rough_circle_becomes_a_true_circle() {
        let c = Whole::Circle {
            center: Point::new(50.0, 40.0),
            radius: 20.0,
        };
        let rough = wobbled(&c.path(true), 0.3);
        let w = whole(&border_of(&rough), &rough, 1.0).expect("a circle");
        match w {
            Whole::Circle { center, radius } => {
                assert!(
                    (center - Point::new(50.0, 40.0)).hypot() < 0.3,
                    "{center:?}"
                );
                assert!((radius - 20.0).abs() < 0.3, "{radius}");
            }
            other => panic!("not a circle: {other:?}"),
        }
        // Four cubics.
        let path = w.path(true);
        let cubics = path
            .elements()
            .iter()
            .filter(|e| matches!(e, PathEl::CurveTo(..)))
            .count();
        assert_eq!(cubics, 4);
    }

    #[test]
    fn a_turned_ellipse_becomes_a_true_ellipse() {
        let e = Whole::Ellipse {
            center: Point::new(60.0, 60.0),
            radii: Vec2::new(30.0, 14.0),
            angle: 0.5,
        };
        let rough = wobbled(&e.path(true), 0.25);
        match whole(&border_of(&rough), &rough, 1.0) {
            Some(Whole::Ellipse { radii, angle, .. }) => {
                assert!((radii.x - 30.0).abs() < 0.5, "{radii:?}");
                assert!((radii.y - 14.0).abs() < 0.5, "{radii:?}");
                assert!((angle - 0.5).abs() < 0.02, "{angle}");
            }
            other => panic!("not an ellipse: {other:?}"),
        }
    }

    #[test]
    fn a_rounded_badge_becomes_a_true_rounded_rectangle() {
        let r = Whole::Rect {
            center: Point::new(80.0, 50.0),
            half: Vec2::new(60.0, 25.0),
            radius: 8.0,
            angle: 0.0,
        };
        let rough = wobbled(&r.path(true), 0.3);
        match whole(&border_of(&rough), &rough, 1.0) {
            Some(Whole::Rect {
                half,
                radius,
                angle,
                center,
            }) => {
                assert!(
                    (half.x - 60.0).abs() < 0.6 && (half.y - 25.0).abs() < 0.6,
                    "{half:?}"
                );
                assert!((radius - 8.0).abs() < 1.0, "{radius}");
                assert_eq!(angle, 0.0, "set level");
                assert!((center - Point::new(80.0, 50.0)).hypot() < 0.5);
            }
            other => panic!("not a rounded rectangle: {other:?}"),
        }
    }

    #[test]
    fn a_turned_square_keeps_square_corners() {
        let r = Whole::Rect {
            center: Point::new(50.0, 50.0),
            half: Vec2::new(20.0, 20.0),
            radius: 0.0,
            angle: 0.3,
        };
        let rough = wobbled(&r.path(true), 0.2);
        match whole(&border_of(&rough), &rough, 1.0) {
            Some(Whole::Rect { radius, angle, .. }) => {
                assert_eq!(radius, 0.0);
                assert!((angle.rem_euclid(std::f64::consts::FRAC_PI_2) - 0.3).abs() < 0.02);
            }
            other => panic!("not a square: {other:?}"),
        }
    }

    #[test]
    fn a_star_or_a_letter_is_no_whole_shape() {
        let mut star = BezPath::new();
        for k in 0..10 {
            let r = if k % 2 == 0 { 30.0 } else { 12.0 };
            let a = k as f64 * std::f64::consts::PI / 5.0;
            let p = Point::new(50.0 + r * a.sin(), 50.0 - r * a.cos());
            if k == 0 {
                star.move_to(p);
            } else {
                star.line_to(p);
            }
        }
        star.close_path();
        assert_eq!(whole(&border_of(&star), &star, 1.0), None);
        // A circle with a bite out of it is not a circle.
        let mut bitten = Whole::Circle {
            center: Point::new(50.0, 50.0),
            radius: 20.0,
        }
        .path(true);
        bitten = wobbled(&bitten, 0.0);
        let pts: Vec<Point> = sampled(&bitten, 1.0)
            .into_iter()
            .map(|p| if p.x > 64.0 { Point::new(64.0, p.y) } else { p })
            .collect();
        let mut cut = BezPath::new();
        cut.move_to(pts[0]);
        for p in &pts[1..] {
            cut.line_to(*p);
        }
        cut.close_path();
        assert_eq!(whole(&border_of(&cut), &cut, 1.0), None);
    }

    #[test]
    fn tiny_shapes_are_left_alone() {
        let c = Whole::Circle {
            center: Point::new(5.0, 5.0),
            radius: 1.5,
        };
        let p = c.path(true);
        assert_eq!(whole(&border_of(&p), &p, 1.0), None);
    }

    #[test]
    fn the_snapped_shape_runs_the_same_way_round() {
        let c = Whole::Circle {
            center: Point::new(50.0, 40.0),
            radius: 20.0,
        };
        for clockwise in [true, false] {
            let rough = wobbled(&c.path(clockwise), 0.2);
            let snapped = snapped(&border_of(&rough), &rough, 1.0).expect("snapped");
            let (a, _) = area_centroid(&sampled(&snapped, 1.0));
            let (b, _) = area_centroid(&sampled(&rough, 1.0));
            assert!(a * b > 0.0);
        }
    }

    #[test]
    fn the_outline_of_an_open_ring_is_no_circle() {
        // A band a pixel wide bent round most of a circle: its outline
        // runs out along the band and back inside it.
        let mut band = BezPath::new();
        let (c, r) = (Point::new(100.0, 100.0), 60.0);
        let steps = 300;
        for k in 0..=steps {
            let a = 0.2 + (std::f64::consts::TAU - 0.4) * k as f64 / steps as f64;
            let p = c + Vec2::new(a.cos(), a.sin()) * (r + 0.5);
            if k == 0 {
                band.move_to(p);
            } else {
                band.line_to(p);
            }
        }
        for k in (0..=steps).rev() {
            let a = 0.2 + (std::f64::consts::TAU - 0.4) * k as f64 / steps as f64;
            band.line_to(c + Vec2::new(a.cos(), a.sin()) * (r - 0.5));
        }
        band.close_path();
        assert_eq!(whole(&border_of(&band), &band, 1.0), None);
    }

    #[test]
    fn a_nearly_symmetric_border_is_made_symmetric() {
        // A heart-like shape, mirrored across x = 50, one side pushed.
        let mut pts = Vec::new();
        for k in 0..400 {
            let t = k as f64 / 400.0 * std::f64::consts::TAU;
            let x = 16.0 * t.sin().powi(3);
            let y =
                -(13.0 * t.cos() - 5.0 * (2.0 * t).cos() - 2.0 * (3.0 * t).cos() - (4.0 * t).cos());
            let push = if x > 0.0 { 0.4 } else { 0.0 };
            pts.push(Point::new(50.0 + 2.0 * x + push, 50.0 + 2.0 * y));
        }
        let out = mirrored(&pts, 1.0).expect("symmetric");
        let (_, c) = area_centroid(&out);
        let sides = Sides::new(&out, 2.0);
        let mut most = 0.0f64;
        for p in &out {
            let m = Point::new(2.0 * c.x - p.x, p.y);
            let q = sides.nearest(m, 2).expect("near");
            most = most.max((m - q).hypot());
        }
        assert!(most < 0.25, "{most}");
        // Not symmetric at all: left alone.
        let lopsided: Vec<Point> = (0..400)
            .map(|k| {
                let t = k as f64 / 400.0 * std::f64::consts::TAU;
                let r = 20.0 + 6.0 * (t + 0.4).cos() + 4.0 * (2.0 * t + 1.0).sin();
                Point::new(50.0 + r * t.cos(), 50.0 + r * t.sin())
            })
            .collect();
        assert_eq!(mirrored(&lopsided, 1.0), None);
    }
}
