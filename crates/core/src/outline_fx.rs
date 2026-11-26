//! Live outline effects that rebuild the outline rather than bend it:
//! Round Corners (every corner of a curve, a group or a text filleted
//! with one radius, optionally only the sharper ones, or only the
//! convex or concave ones) and Slice (the outline cut into parallel
//! strips with a gap between them, each strip slid along the cut, turned
//! a little, and its corners rounded). Both work on the outline as it is
//! drawn and leave the object itself as it is, so a text stays text.

use crate::corner_cut::{corners, cut_corners, CornerCut};
use crate::geometry::{Affine, BezPath, PathEl, Point, Rect, Shape as _, Vec2};
use crate::shaping::{overlay_with, FillRule, Op};
use serde::{Deserialize, Serialize};

/// Which corners Round Corners takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CornerSide {
    /// Every corner.
    #[default]
    Both,
    /// Only corners that point out of the shape.
    Convex,
    /// Only corners that point into the shape.
    Concave,
}

/// The largest radius and gap taken, millimetres, and the most slices.
pub const MAX_RADIUS: f64 = 1000.0;
pub const MAX_SLICES: u32 = 200;

/// `path` with its corners filleted with `radius` mm: only the corners
/// whose sides meet at less than `max_angle` degrees (180 or more: all)
/// and only those on `side`. A corner whose sides are too short for the
/// radius is left as it is, as the Corners panel does.
pub fn round_corners(path: &BezPath, radius: f64, max_angle: f64, side: CornerSide) -> BezPath {
    if !(radius.is_finite() && radius > 0.0) {
        return path.clone();
    }
    let radius = radius.min(MAX_RADIUS);
    let all = max_angle >= 180.0 || !max_angle.is_finite();
    if all && side == CornerSide::Both {
        return cut_corners(path, CornerCut::Fillet(radius), None).0;
    }
    let size = path.bounding_box();
    let eps = (size.width().max(size.height()) * 1e-4).max(1e-6);
    let nodes: Vec<usize> = corners(path)
        .into_iter()
        .filter(|c| all || c.theta.to_degrees() < max_angle)
        .filter(|c| match side {
            CornerSide::Both => true,
            side => {
                let inside = path.winding(c.at + c.inside * eps) % 2 != 0;
                inside == (side == CornerSide::Convex)
            }
        })
        .map(|c| c.node)
        .collect();
    if nodes.is_empty() {
        return path.clone();
    }
    cut_corners(path, CornerCut::Fillet(radius), Some(&nodes)).0
}

/// How Slice cuts an outline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SliceParams {
    /// Number of strips across the outline's box (at least 1).
    pub count: u32,
    /// Direction of the cuts, degrees counterclockwise (0: level cuts,
    /// level strips).
    pub angle: f64,
    /// Width of each cut, millimetres.
    pub gap: f64,
    /// How far each strip slides along the cut, millimetres.
    pub shift: f64,
    /// Strips slide one way and the other in turn (otherwise each by its
    /// own pseudo-random share of `shift`, from `seed`).
    pub alternate: bool,
    /// Each strip turns by its own pseudo-random share of this, degrees
    /// either way, about its middle.
    pub turn: f64,
    pub seed: u32,
    /// The strips' corners rounded with this radius, millimetres.
    pub radius: f64,
}

/// A pseudo-random share in -1..1 for strip `k` of seed `seed`
/// (splitmix64, so the same strip of the same seed always gets it).
fn share(seed: u32, k: u32, salt: u64) -> f64 {
    let mut z = (u64::from(seed) << 32 | u64::from(k)) ^ salt.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
}

/// `path` (read with `fill`) cut into strips as `p` says, in the box `b`
/// of the whole outline (several parts cut with one box line up). The
/// result fills with the even-odd rule.
pub fn slice(path: &BezPath, b: Rect, p: &SliceParams, fill: FillRule) -> BezPath {
    let count = p.count.clamp(1, MAX_SLICES);
    let gap = if p.gap.is_finite() {
        p.gap.clamp(0.0, MAX_RADIUS)
    } else {
        0.0
    };
    let finite = |v: f64| if v.is_finite() { v } else { 0.0 };
    let (shift, turn, radius) = (finite(p.shift), finite(p.turn), finite(p.radius));
    if count == 1 && gap == 0.0 && shift == 0.0 && turn == 0.0 && radius <= 0.0 {
        return path.clone();
    }
    let c = b.center();
    let level = Affine::rotate_about(-finite(p.angle).to_radians(), c);
    let back = level.inverse();
    let flat = level * path.clone();
    let fb = flat.bounding_box();
    if !(fb.height() > 1e-9 && fb.width() > 1e-9) {
        return path.clone();
    }
    // The strips are laid across the box of the whole outline turned
    // level, so parts cut with the same box share their cuts.
    let lb = (level * b.to_path(0.1)).bounding_box().union(fb);
    let step = lb.height() / f64::from(count);
    let half = (gap / 2.0).min(step / 2.0);
    let (x0, x1) = (lb.x0 - lb.width() - 1.0, lb.x1 + lb.width() + 1.0);
    let mut out = BezPath::new();
    for k in 0..count {
        let y0 = lb.y0 + step * f64::from(k) + if k > 0 { half } else { 0.0 };
        let y1 = lb.y0 + step * f64::from(k + 1) - if k + 1 < count { half } else { 0.0 };
        if y1 - y0 <= 1e-9 {
            continue;
        }
        let band = Rect::new(x0, y0, x1, y1).to_path(0.1);
        let mut piece = overlay_with(&flat, &band, Op::Intersect, fill);
        if piece.elements().is_empty() {
            continue;
        }
        if radius > 0.0 {
            piece = round_corners(&piece, radius, 180.0, CornerSide::Both);
        }
        let dx = if p.alternate {
            if k % 2 == 0 {
                shift
            } else {
                -shift
            }
        } else {
            shift * share(p.seed, k, 1)
        };
        let mid = Point::new(lb.center().x, (y0 + y1) / 2.0);
        let t = Affine::translate(Vec2::new(dx, 0.0))
            * Affine::rotate_about((turn * share(p.seed, k, 2)).to_radians(), mid);
        out.extend((back * t * piece).elements().iter().copied());
    }
    out
}

/// Whether nothing of `path` is left (the outline was cut away entirely).
pub fn is_empty(path: &BezPath) -> bool {
    !path
        .elements()
        .iter()
        .any(|e| !matches!(e, PathEl::MoveTo(_)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x: f64, y: f64, s: f64) -> BezPath {
        Rect::new(x, y, x + s, y + s).to_path(0.1)
    }

    fn subpaths(p: &BezPath) -> usize {
        p.elements()
            .iter()
            .filter(|e| matches!(e, PathEl::MoveTo(_)))
            .count()
    }

    /// Every corner of a square rounded: the area drops by the four
    /// corners' (4 - pi) r^2; a zero radius changes nothing.
    #[test]
    fn rounds_every_corner() {
        let sq = square(0.0, 0.0, 10.0);
        let r = round_corners(&sq, 2.0, 180.0, CornerSide::Both);
        let lost = 100.0 - r.area().abs();
        assert!(
            (lost - (4.0 - std::f64::consts::PI) * 4.0).abs() < 0.05,
            "{lost}"
        );
        assert_eq!(round_corners(&sq, 0.0, 180.0, CornerSide::Both), sq);
    }

    /// An L shape has five convex corners and one concave one: convex
    /// only leaves the inner corner sharp, concave only rounds it alone.
    #[test]
    fn convex_and_concave_corners_apart() {
        let mut l = BezPath::new();
        l.move_to((0.0, 0.0));
        l.line_to((10.0, 0.0));
        l.line_to((10.0, 4.0));
        l.line_to((4.0, 4.0));
        l.line_to((4.0, 10.0));
        l.line_to((0.0, 10.0));
        l.close_path();
        let area = l.area().abs();
        let convex = round_corners(&l, 1.0, 180.0, CornerSide::Convex);
        let concave = round_corners(&l, 1.0, 180.0, CornerSide::Concave);
        // A fillet of radius 1 takes 1 - pi/4 from a right angle.
        let k = 1.0 - std::f64::consts::FRAC_PI_4;
        assert!((area - convex.area().abs() - 5.0 * k).abs() < 0.05);
        assert!((concave.area().abs() - area - k).abs() < 0.05);
        // The inner corner is still a node of the convex-only outline.
        assert!(convex.elements().iter().any(
            |e| matches!(e, PathEl::LineTo(p) | PathEl::MoveTo(p) if (*p - Point::new(4.0, 4.0)).hypot() < 1e-6)
        ));
    }

    /// Only corners sharper than the limit: a triangle's 30 degree tip is
    /// rounded under a 45 degree limit, its two 75 degree corners not.
    #[test]
    fn angle_limit_takes_sharp_corners_only() {
        let h = 10.0 / (15.0f64.to_radians().tan());
        let mut t = BezPath::new();
        t.move_to((0.0, 0.0));
        t.line_to((10.0, h));
        t.line_to((20.0, 0.0));
        t.close_path();
        let tip = round_corners(&t, 1.0, 45.0, CornerSide::Both);
        let all = round_corners(&t, 1.0, 180.0, CornerSide::Both);
        let (a, a_tip, a_all) = (t.area().abs(), tip.area().abs(), all.area().abs());
        assert!(a_tip < a && a_all < a_tip, "{a} {a_tip} {a_all}");
    }

    /// Four strips of a square with a 1 mm gap: four subpaths, the area
    /// less the three cuts, and nothing slid.
    #[test]
    fn slices_into_strips_with_gaps() {
        let sq = square(0.0, 0.0, 12.0);
        let p = SliceParams {
            count: 4,
            angle: 0.0,
            gap: 1.0,
            shift: 0.0,
            alternate: false,
            turn: 0.0,
            seed: 0,
            radius: 0.0,
        };
        let s = slice(&sq, sq.bounding_box(), &p, FillRule::EvenOdd);
        assert_eq!(subpaths(&s), 4);
        assert!((s.area().abs() - (144.0 - 3.0 * 12.0)).abs() < 1e-6);
        let b = s.bounding_box();
        assert!((b.x0).abs() < 1e-9 && (b.x1 - 12.0).abs() < 1e-9);
        // Slid one way and the other: the box widens by the shift each way.
        let slid = slice(
            &sq,
            sq.bounding_box(),
            &SliceParams {
                shift: 2.0,
                alternate: true,
                ..p
            },
            FillRule::EvenOdd,
        );
        let b = slid.bounding_box();
        assert!((b.x0 + 2.0).abs() < 1e-9 && (b.x1 - 14.0).abs() < 1e-9);
        assert!((slid.area().abs() - s.area().abs()).abs() < 1e-6);
        // The same seed gives the same strips.
        let r = SliceParams {
            alternate: false,
            turn: 10.0,
            seed: 7,
            ..p
        };
        assert_eq!(
            slice(&sq, sq.bounding_box(), &r, FillRule::EvenOdd),
            slice(&sq, sq.bounding_box(), &r, FillRule::EvenOdd)
        );
    }

    /// Cuts at 90 degrees run up the page: the strips stand side by side.
    #[test]
    fn angled_cuts_and_rounded_strips() {
        let sq = square(0.0, 0.0, 12.0);
        let p = SliceParams {
            count: 3,
            angle: 90.0,
            gap: 1.5,
            shift: 0.0,
            alternate: false,
            turn: 0.0,
            seed: 0,
            radius: 0.0,
        };
        let s = slice(&sq, sq.bounding_box(), &p, FillRule::EvenOdd);
        assert_eq!(subpaths(&s), 3);
        let b = s.bounding_box();
        assert!((b.y0).abs() < 1e-6 && (b.y1 - 12.0).abs() < 1e-6);
        let round = slice(
            &sq,
            sq.bounding_box(),
            &SliceParams { radius: 1.0, ..p },
            FillRule::EvenOdd,
        );
        assert!(round.area().abs() < s.area().abs() - 1.0);
    }

    /// Nonzero outlines (overlapping letters) are cut as they fill.
    #[test]
    fn nonzero_overlaps_are_filled() {
        let mut two = square(0.0, 0.0, 10.0);
        two.extend(square(5.0, 0.0, 10.0).elements().iter().copied());
        let p = SliceParams {
            count: 2,
            angle: 0.0,
            gap: 0.0,
            shift: 0.0,
            alternate: false,
            turn: 0.0,
            seed: 0,
            radius: 0.0,
        };
        let s = slice(&two, two.bounding_box(), &p, FillRule::NonZero);
        assert!((s.area().abs() - 150.0).abs() < 1e-6);
        assert!(!is_empty(&s));
    }
}
