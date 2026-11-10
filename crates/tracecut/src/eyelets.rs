//! Eyelets (grommets): marks for the holes along the edge of a banner or
//! a sign.
//!
//! **Along the box**: the box of the objects is shrunk by the inset (the
//! distance from the edge to an eyelet's centre); an eyelet goes in each
//! of its corners and more along each side. **Along each closed curve**:
//! every closed outline is offset inward by the inset
//! (`tracedraw_core::shaping::offset_polygon`); an eyelet goes on each
//! corner of the result (a turn sharper than 30 degrees) and more along
//! the stretches between corners. A loop with no corner (a circle) gets
//! its eyelets spread evenly from its start.
//!
//! Along a side or stretch of length `L`: **by spacing**, `ceil(L / s) -
//! 1` eyelets in between, so no two are more than `s` apart; **by
//! count**, `count - 2` in between (the count includes both corners).
//! They are spread evenly by length. Eyelets closer than a thousandth of
//! a millimetre are one.

use crate::cut::{fin, finite_rect, poly_bounds, polygon_path, polys, FLATTEN_TOLERANCE};
use tracedraw_core::geometry::{BezPath, Point, Rect};

/// What the eyelets go along.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Along {
    /// The box of the objects.
    #[default]
    Box,
    /// Each closed outline.
    Curve,
}

/// Settings of [`eyelets`], lengths in millimetres.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct EyeletSpec {
    pub along: Along,
    pub diameter: f64,
    /// From the edge to an eyelet's centre.
    pub inset: f64,
    /// A count per side (else at most `spacing` apart).
    pub by_count: bool,
    pub spacing: f64,
    /// Per side, both corners included.
    pub count: u32,
}

impl Default for EyeletSpec {
    fn default() -> Self {
        EyeletSpec {
            along: Along::Box,
            diameter: 10.0,
            inset: 15.0,
            by_count: false,
            spacing: 300.0,
            count: 3,
        }
    }
}

pub const DIAMETER_RANGE: (f64, f64) = (0.1, 200.0);
pub const INSET_RANGE: (f64, f64) = (0.0, 1000.0);
pub const SPACING_RANGE: (f64, f64) = (1.0, 10_000.0);
pub const COUNT_RANGE: (u32, u32) = (2, 500);

/// A turn sharper than this at a point of a curve makes it a corner
/// (degrees).
pub const CORNER_DEG: f64 = 30.0;

/// The most eyelets one call gives.
pub const MAX_EYELETS: usize = 100_000;

impl EyeletSpec {
    /// The settings with every value finite and in its range.
    pub fn clamped(&self) -> EyeletSpec {
        let d = EyeletSpec::default();
        let c = |v: f64, fallback: f64, r: (f64, f64)| fin(v, fallback).clamp(r.0, r.1);
        EyeletSpec {
            diameter: c(self.diameter, d.diameter, DIAMETER_RANGE),
            inset: c(self.inset, d.inset, INSET_RANGE),
            spacing: c(self.spacing, d.spacing, SPACING_RANGE),
            count: self.count.clamp(COUNT_RANGE.0, COUNT_RANGE.1),
            ..*self
        }
    }

    /// How many eyelets go between the two ends of a stretch `len` long.
    fn between(&self, len: f64) -> usize {
        if len <= 1e-9 {
            return 0;
        }
        if self.by_count {
            self.count.saturating_sub(2) as usize
        } else {
            let n = (len / self.spacing - 1e-9).ceil().max(1.0);
            (n as usize).saturating_sub(1).min(MAX_EYELETS)
        }
    }
}

/// Push `p` unless an eyelet is already there.
fn push(out: &mut Vec<Point>, p: Point) {
    if out.len() < MAX_EYELETS && !out.iter().any(|q| (*q - p).hypot() < 1e-3) {
        out.push(p);
    }
}

/// The eyelets' centres along the rectangle `r`: its corners first
/// (bottom left, counter-clockwise), then along the bottom, right, top
/// and left sides.
pub fn along_box(r: Rect, spec: &EyeletSpec) -> Vec<Point> {
    let spec = spec.clamped();
    if !finite_rect(r) {
        return Vec::new();
    }
    let r = r.abs();
    let i = spec.inset;
    let (cx, cy) = ((r.x0 + r.x1) / 2.0, (r.y0 + r.y1) / 2.0);
    // An inset deeper than half the box puts the eyelets on its middle.
    let (x0, x1) = if r.width() > 2.0 * i {
        (r.x0 + i, r.x1 - i)
    } else {
        (cx, cx)
    };
    let (y0, y1) = if r.height() > 2.0 * i {
        (r.y0 + i, r.y1 - i)
    } else {
        (cy, cy)
    };
    let corners = [
        Point::new(x0, y0),
        Point::new(x1, y0),
        Point::new(x1, y1),
        Point::new(x0, y1),
    ];
    let mut out = Vec::new();
    for c in corners {
        push(&mut out, c);
    }
    for k in 0..4 {
        let (a, b) = (corners[k], corners[(k + 1) % 4]);
        let n = spec.between((b - a).hypot());
        for j in 1..=n {
            push(&mut out, a.lerp(b, j as f64 / (n + 1) as f64));
        }
    }
    out
}

/// The point `s` along the closed polygon `pts` from its first point.
fn at_length(pts: &[Point], s: f64) -> Option<Point> {
    let n = pts.len();
    let mut left = s;
    for k in 0..n {
        let (a, b) = (pts[k], pts[(k + 1) % n]);
        let l = (b - a).hypot();
        if left <= l {
            return Some(if l > 0.0 { a.lerp(b, left / l) } else { a });
        }
        left -= l;
    }
    pts.first().copied()
}

/// The eyelets' centres along one closed polygon.
fn along_loop(pts: &[Point], spec: &EyeletSpec, out: &mut Vec<Point>) {
    let n = pts.len();
    if n < 3 {
        return;
    }
    let limit = CORNER_DEG.to_radians();
    let corners: Vec<usize> = (0..n)
        .filter(|&i| {
            let a = pts[i] - pts[(i + n - 1) % n];
            let b = pts[(i + 1) % n] - pts[i];
            let (la, lb) = (a.hypot(), b.hypot());
            la > 1e-12 && lb > 1e-12 && (a.dot(b) / (la * lb)).clamp(-1.0, 1.0).acos() > limit
        })
        .collect();
    // The loop from its first corner (or its start), closed.
    let start = corners.first().copied().unwrap_or(0);
    let mut ring: Vec<Point> = (0..n).map(|k| pts[(start + k) % n]).collect();
    let first = ring[0];
    ring.push(first);
    let mut lens = vec![0.0];
    for w in ring.windows(2) {
        let l = lens.last().copied().unwrap_or(0.0) + (w[1] - w[0]).hypot();
        lens.push(l);
    }
    let total = lens.last().copied().unwrap_or(0.0);
    if total <= 1e-9 {
        return;
    }
    let open = &ring[..n];
    if corners.is_empty() {
        let k = if spec.by_count {
            spec.count as usize
        } else {
            ((total / spec.spacing - 1e-9).ceil().max(1.0) as usize).min(MAX_EYELETS)
        }
        .max(1);
        for j in 0..k {
            if let Some(p) = at_length(open, total * j as f64 / k as f64) {
                push(out, p);
            }
        }
        return;
    }
    // Corner positions along the ring, by length from the first corner.
    let mut at: Vec<f64> = corners.iter().map(|c| lens[(c + n - start) % n]).collect();
    at.sort_by(f64::total_cmp);
    at.push(total);
    for w in at.windows(2) {
        let (a, b) = (w[0], w[1]);
        if let Some(p) = at_length(open, a) {
            push(out, p);
        }
        let m = spec.between(b - a);
        for j in 1..=m {
            if let Some(p) = at_length(open, a + (b - a) * j as f64 / (m + 1) as f64) {
                push(out, p);
            }
        }
    }
}

/// The eyelets' centres for `objects` (page-space outlines per object).
pub fn eyelets(objects: &[Vec<BezPath>], spec: &EyeletSpec) -> Vec<Point> {
    let spec = spec.clamped();
    let all: Vec<BezPath> = objects.iter().flatten().cloned().collect();
    match spec.along {
        Along::Box => crate::cut::bounds(&all)
            .map(|b| along_box(b, &spec))
            .unwrap_or_default(),
        Along::Curve => {
            let mut out = Vec::new();
            for p in &all {
                for poly in polys(p, FLATTEN_TOLERANCE) {
                    if !poly.closed || poly.pts.len() < 3 {
                        continue;
                    }
                    if spec.inset <= 0.0 {
                        along_loop(&poly.pts, &spec, &mut out);
                        continue;
                    }
                    let inner = tracedraw_core::shaping::offset_polygon(
                        &polygon_path(&poly.pts),
                        -spec.inset,
                    );
                    for l in polys(&inner, FLATTEN_TOLERANCE) {
                        if l.closed && poly_bounds(&l.pts).is_some() {
                            along_loop(&l.pts, &spec, &mut out);
                        }
                    }
                }
            }
            out
        }
    }
}

/// A circle of `diameter` round each centre.
pub fn marks(centres: &[Point], diameter: f64) -> Vec<BezPath> {
    let r = fin(diameter, EyeletSpec::default().diameter).clamp(DIAMETER_RANGE.0, DIAMETER_RANGE.1)
        / 2.0;
    centres
        .iter()
        .map(|c| {
            tracedraw_core::geometry::ellipse_path(Rect::new(c.x - r, c.y - r, c.x + r, c.y + r))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::Shape as _;

    fn sorted(mut v: Vec<Point>) -> Vec<(i64, i64)> {
        let mut out: Vec<(i64, i64)> = v
            .drain(..)
            .map(|p| ((p.x * 1000.0).round() as i64, (p.y * 1000.0).round() as i64))
            .collect();
        out.sort_unstable();
        out
    }

    fn has(v: &[Point], x: f64, y: f64) -> bool {
        v.iter()
            .any(|p| (p.x - x).abs() < 0.01 && (p.y - y).abs() < 0.01)
    }

    #[test]
    fn a_banner_gets_its_corners_and_eyelets_at_most_the_spacing_apart() {
        let r = Rect::new(0.0, 0.0, 400.0, 200.0);
        let spec = EyeletSpec::default();
        let e = along_box(r, &spec);
        // 370 x 170 between the corners: two gaps along, one up.
        assert_eq!(e.len(), 6, "{e:?}");
        for (x, y) in [(15.0, 15.0), (385.0, 15.0), (385.0, 185.0), (15.0, 185.0)] {
            assert!(has(&e, x, y), "corner {x} {y}");
        }
        assert!(has(&e, 200.0, 15.0) && has(&e, 200.0, 185.0));
        // Closer spacing: 370 / 100 -> 4 gaps, 170 / 100 -> 2 gaps.
        let close = along_box(
            r,
            &EyeletSpec {
                spacing: 100.0,
                ..spec
            },
        );
        assert_eq!(close.len(), 4 + 2 * 3 + 2);
        assert!(has(&close, 15.0, 100.0) && has(&close, 107.5, 15.0));
    }

    #[test]
    fn a_count_per_side_includes_the_corners() {
        let r = Rect::new(0.0, 0.0, 100.0, 100.0);
        let spec = EyeletSpec {
            by_count: true,
            count: 4,
            inset: 10.0,
            ..Default::default()
        };
        let e = along_box(r, &spec);
        assert_eq!(e.len(), 4 * 3);
        // Even steps of 80 / 3 along the bottom.
        assert!(has(&e, 10.0 + 80.0 / 3.0, 10.0) && has(&e, 10.0 + 160.0 / 3.0, 10.0));
        let two = along_box(r, &EyeletSpec { count: 2, ..spec });
        assert_eq!(two.len(), 4);
    }

    #[test]
    fn a_closed_rectangle_curve_gives_what_its_box_gives() {
        let r = Rect::new(0.0, 0.0, 400.0, 200.0);
        let path = crate::cut::rect_path(r);
        for spacing in [300.0, 100.0, 37.0] {
            let spec = EyeletSpec {
                spacing,
                ..Default::default()
            };
            let by_box = eyelets(&[vec![path.clone()]], &spec);
            let by_curve = eyelets(
                &[vec![path.clone()]],
                &EyeletSpec {
                    along: Along::Curve,
                    ..spec
                },
            );
            assert_eq!(sorted(by_box), sorted(by_curve), "{spacing}");
        }
    }

    #[test]
    fn a_circle_gets_its_eyelets_spread_evenly() {
        let c = Point::new(0.0, 0.0);
        let circle = tracedraw_core::geometry::ellipse_path(Rect::new(-50.0, -50.0, 50.0, 50.0));
        let spec = EyeletSpec {
            along: Along::Curve,
            by_count: true,
            count: 6,
            inset: 10.0,
            ..Default::default()
        };
        let e = eyelets(&[vec![circle]], &spec);
        assert_eq!(e.len(), 6);
        for p in &e {
            assert!(((*p - c).hypot() - 40.0).abs() < 0.1, "{p:?}");
        }
        // Neighbours a sixth of the way round.
        let mut gaps: Vec<f64> = Vec::new();
        for p in &e {
            let near = e
                .iter()
                .filter(|q| (**q - *p).hypot() > 1e-6)
                .map(|q| (*q - *p).hypot())
                .fold(f64::INFINITY, f64::min);
            gaps.push(near);
        }
        assert!(gaps.iter().all(|g| (g - 40.0).abs() < 0.5), "{gaps:?}");
    }

    #[test]
    fn marks_are_circles_of_the_diameter() {
        let m = marks(&[Point::new(10.0, 10.0), Point::new(50.0, 10.0)], 8.0);
        assert_eq!(m.len(), 2);
        let b = m[0].bounding_box();
        assert!((b.width() - 8.0).abs() < 1e-9 && (b.center().x - 10.0).abs() < 1e-9);
    }

    #[test]
    fn small_and_damaged_input_never_fails() {
        // An inset deeper than the box puts the eyelets on its middle.
        let e = along_box(Rect::new(0.0, 0.0, 20.0, 100.0), &EyeletSpec::default());
        assert!(e.iter().all(|p| (p.x - 10.0).abs() < 1e-9));
        assert_eq!(e.len(), 2);
        assert!(along_box(Rect::new(f64::NAN, 0.0, 1.0, 1.0), &EyeletSpec::default()).is_empty());
        let spec = EyeletSpec {
            spacing: f64::NAN,
            diameter: -1.0,
            count: 0,
            ..Default::default()
        };
        assert!(!along_box(Rect::new(0.0, 0.0, 100.0, 100.0), &spec).is_empty());
        assert!(eyelets(&[], &spec).is_empty());
        // A curve smaller than the inset has no room for eyelets.
        let small = crate::cut::rect_path(Rect::new(0.0, 0.0, 10.0, 10.0));
        let curve = EyeletSpec {
            along: Along::Curve,
            ..Default::default()
        };
        assert!(eyelets(&[vec![small]], &curve).is_empty());
    }
}
