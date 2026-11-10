//! Box Maker: the flat panels of a box with finger joints, for cutting
//! from sheet material with a laser or a CNC router.
//!
//! The box is `W` wide (x), `D` deep (y) and `H` high (z), outside
//! sizes; with inside sizes `W = width + 2t`, `D = depth + 2t` and
//! `H = height + 2t` (`+ t` with an open top), `t` the material's
//! thickness. The panels are the bottom and the top (`W x D`), the front
//! and the back (`W x H`) and the left and right sides (`D x H`); an
//! open box has no top.
//!
//! Every edge where two panels meet is a joint. Its length `L` less the
//! corners at its ends (`t` where a third panel meets it, 0 at an open
//! top) is split into `n` equal stretches, `n` odd and at least 3, as
//! many as fit the finger width (`floor((L - corners) / finger)`, one
//! less when even). One panel of the joint, the owner, keeps its
//! material to the edge on the even stretches and on the corners; the
//! other keeps it on the odd stretches and is cut back by `t` elsewhere,
//! so every finger of one panel fills a slot of the other. The front and
//! the back own their joints with the sides, and the four walls own
//! their joints with the bottom and the top, so each corner of the box
//! belongs to the front or the back.
//!
//! The panels are laid out from a corner with a gap between them: the
//! front, the back, the left and the right side in a row, the bottom and
//! the top in a row above.

use crate::cut::{fin, polygon_path};
use tracedraw_core::geometry::{BezPath, Point};

/// Whether the sizes given are the box's outside or inside.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Sizes {
    #[default]
    Outside,
    Inside,
}

/// Settings of [`panels`], lengths in millimetres.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct BoxSpec {
    pub width: f64,
    pub depth: f64,
    pub height: f64,
    pub thickness: f64,
    /// The width of a finger the joints aim for.
    pub finger: f64,
    pub sizes: Sizes,
    pub open_top: bool,
    /// Between the panels laid out.
    pub gap: f64,
}

impl Default for BoxSpec {
    fn default() -> Self {
        BoxSpec {
            width: 200.0,
            depth: 150.0,
            height: 100.0,
            thickness: 6.0,
            finger: 20.0,
            sizes: Sizes::Outside,
            open_top: false,
            gap: 5.0,
        }
    }
}

pub const SIZE_RANGE: (f64, f64) = (1.0, 10_000.0);
pub const THICKNESS_RANGE: (f64, f64) = (0.1, 100.0);
pub const FINGER_RANGE: (f64, f64) = (0.5, 1000.0);
pub const GAP_RANGE: (f64, f64) = (0.0, 1000.0);

/// The most stretches one joint is split into.
pub const MAX_FINGERS: usize = 999;

/// Why no box could be made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoxError {
    /// A side is shorter than four thicknesses.
    TooSmall,
}

/// The panels of a box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelKind {
    Front,
    Back,
    Left,
    Right,
    Bottom,
    Top,
}

/// What one side of a panel is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Edge {
    /// Straight, the material to the edge (an open top).
    Flat,
    Joint(Joint),
}

/// One side of a joint, along the side's own direction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Joint {
    pub len: f64,
    /// The corners at the start and the end of the side.
    pub c0: f64,
    pub c1: f64,
    /// Stretches between the corners (odd).
    pub n: usize,
    /// This panel keeps the even stretches and the corners.
    pub owner: bool,
}

impl Joint {
    /// The stretches along the side: from, to, and whether the material
    /// reaches the edge there.
    pub fn profile(&self) -> Vec<(f64, f64, bool)> {
        let n = self.n.max(1);
        let s = (self.len - self.c0 - self.c1) / n as f64;
        (0..n)
            .map(|k| {
                let a = if k == 0 { 0.0 } else { self.c0 + k as f64 * s };
                let b = if k + 1 == n {
                    self.len
                } else {
                    self.c0 + (k + 1) as f64 * s
                };
                (a, b, (k % 2 == 0) == self.owner)
            })
            .collect()
    }
}

impl Edge {
    /// The stretches along a side `len` long: from, to and how far the
    /// edge is cut back.
    fn steps(&self, len: f64, t: f64) -> Vec<(f64, f64, f64)> {
        match self {
            Edge::Flat => vec![(0.0, len, 0.0)],
            Edge::Joint(j) => j
                .profile()
                .into_iter()
                .map(|(a, b, out)| (a, b, if out { 0.0 } else { t }))
                .collect(),
        }
    }
}

/// One panel: its kind, its size and its outline (counter-clockwise,
/// from its own bottom-left corner).
#[derive(Debug, Clone, PartialEq)]
pub struct Panel {
    pub kind: PanelKind,
    pub width: f64,
    pub height: f64,
    /// The bottom, right, top and left sides.
    pub edges: [Edge; 4],
    pub outline: Vec<Point>,
}

impl BoxSpec {
    /// The settings with every value finite and in its range.
    pub fn clamped(&self) -> BoxSpec {
        let d = BoxSpec::default();
        let c = |v: f64, fallback: f64, r: (f64, f64)| fin(v, fallback).clamp(r.0, r.1);
        BoxSpec {
            width: c(self.width, d.width, SIZE_RANGE),
            depth: c(self.depth, d.depth, SIZE_RANGE),
            height: c(self.height, d.height, SIZE_RANGE),
            thickness: c(self.thickness, d.thickness, THICKNESS_RANGE),
            finger: c(self.finger, d.finger, FINGER_RANGE),
            gap: c(self.gap, d.gap, GAP_RANGE),
            ..*self
        }
    }

    /// The outside width, depth and height.
    pub fn outside(&self) -> (f64, f64, f64) {
        let s = self.clamped();
        let t = s.thickness;
        match s.sizes {
            Sizes::Outside => (s.width, s.depth, s.height),
            Sizes::Inside => (
                s.width + 2.0 * t,
                s.depth + 2.0 * t,
                s.height + if s.open_top { t } else { 2.0 * t },
            ),
        }
    }
}

/// The number of stretches a joint of `inner` between its corners gets.
fn stretches(inner: f64, finger: f64) -> usize {
    let m = (inner / finger).floor();
    let m = if m.is_finite() && m > 0.0 {
        m.min(MAX_FINGERS as f64) as usize
    } else {
        0
    };
    let m = if m % 2 == 0 { m.saturating_sub(1) } else { m };
    m.max(3)
}

/// The point `along` a panel side and `depth` in from it, in the panel's
/// own space: sides 0 to 3 are the bottom (left to right), the right
/// (upwards), the top (right to left) and the left (downwards).
pub fn side_point(w: f64, h: f64, side: usize, along: f64, depth: f64) -> Point {
    match side % 4 {
        0 => Point::new(along, depth),
        1 => Point::new(w - depth, along),
        2 => Point::new(w - along, h - depth),
        _ => Point::new(depth, h - along),
    }
}

/// The outline of a `w` x `h` panel with these sides.
fn outline(w: f64, h: f64, t: f64, edges: &[Edge; 4]) -> Vec<Point> {
    let lens = [w, h, w, h];
    let steps: Vec<Vec<(f64, f64, f64)>> =
        edges.iter().zip(lens).map(|(e, l)| e.steps(l, t)).collect();
    let first = |k: usize| steps[k].first().map_or(0.0, |s| s.2);
    let last = |k: usize| steps[k].last().map_or(0.0, |s| s.2);
    let mut pts: Vec<Point> = Vec::new();
    for k in 0..4 {
        let (prev, next) = ((k + 3) % 4, (k + 1) % 4);
        let side = &steps[k];
        let mut add = |along: f64, depth: f64| {
            let p = side_point(w, h, k, along, depth);
            if pts.last().is_none_or(|q| (*q - p).hypot() > 1e-9) {
                pts.push(p);
            }
        };
        add(last(prev), first(k));
        for pair in side.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if (a.2 - b.2).abs() > 1e-12 {
                add(a.1, a.2);
                add(a.1, b.2);
            }
        }
        add(lens[k] - first(next), last(k));
    }
    if pts.len() > 1
        && pts
            .first()
            .zip(pts.last())
            .is_some_and(|(a, b)| (*a - *b).hypot() <= 1e-9)
    {
        pts.pop();
    }
    // Points on a straight line between their neighbours add nothing.
    let n = pts.len();
    let keep: Vec<Point> = (0..n)
        .filter(|&i| {
            let (a, p, b) = (pts[(i + n - 1) % n], pts[i], pts[(i + 1) % n]);
            ((p - a).cross(b - p)).abs() > 1e-9
        })
        .map(|i| pts[i])
        .collect();
    keep
}

/// The panels of the box `spec` describes.
pub fn panels(spec: &BoxSpec) -> Result<Vec<Panel>, BoxError> {
    let s = spec.clamped();
    let t = s.thickness;
    let (w, d, h) = s.outside();
    if w < 4.0 * t || d < 4.0 * t || h < 4.0 * t {
        return Err(BoxError::TooSmall);
    }
    let top_c = if s.open_top { 0.0 } else { t };
    let nx = stretches(w - 2.0 * t, s.finger);
    let ny = stretches(d - 2.0 * t, s.finger);
    let nz = stretches(h - t - top_c, s.finger);
    let joint = |len: f64, c0: f64, c1: f64, n: usize, owner: bool| {
        Edge::Joint(Joint {
            len,
            c0,
            c1,
            n,
            owner,
        })
    };
    // The front and the back (x by z): they own every joint.
    let front = [
        joint(w, t, t, nx, true),
        joint(h, t, top_c, nz, true),
        if s.open_top {
            Edge::Flat
        } else {
            joint(w, t, t, nx, true)
        },
        joint(h, top_c, t, nz, true),
    ];
    // The sides (y by z): they own their joints with the bottom and top.
    let side = [
        joint(d, t, t, ny, true),
        joint(h, t, top_c, nz, false),
        if s.open_top {
            Edge::Flat
        } else {
            joint(d, t, t, ny, true)
        },
        joint(h, top_c, t, nz, false),
    ];
    // The bottom and the top (x by y) own nothing.
    let base = [
        joint(w, t, t, nx, false),
        joint(d, t, t, ny, false),
        joint(w, t, t, nx, false),
        joint(d, t, t, ny, false),
    ];
    let mut kinds = vec![
        (PanelKind::Front, w, h, front),
        (PanelKind::Back, w, h, front),
        (PanelKind::Left, d, h, side),
        (PanelKind::Right, d, h, side),
        (PanelKind::Bottom, w, d, base),
    ];
    if !s.open_top {
        kinds.push((PanelKind::Top, w, d, base));
    }
    Ok(kinds
        .into_iter()
        .map(|(kind, pw, ph, edges)| Panel {
            kind,
            width: pw,
            height: ph,
            outline: outline(pw, ph, t, &edges),
            edges,
        })
        .collect())
}

/// The panels laid out from `corner` (their bottom-left), `gap` apart:
/// the walls in a row, the bottom and the top in a row above them.
pub fn layout(panels: &[Panel], corner: Point, gap: f64) -> Vec<BezPath> {
    let gap = fin(gap, 0.0).clamp(GAP_RANGE.0, GAP_RANGE.1);
    let corner = if crate::cut::finite(corner) {
        corner
    } else {
        Point::ZERO
    };
    let walls = |k: &PanelKind| !matches!(k, PanelKind::Bottom | PanelKind::Top);
    let row_h = panels
        .iter()
        .filter(|p| walls(&p.kind))
        .map(|p| p.height)
        .fold(0.0, f64::max);
    let mut out = Vec::new();
    for (row, y) in [(true, corner.y), (false, corner.y + row_h + gap)] {
        let mut x = corner.x;
        for p in panels.iter().filter(|p| walls(&p.kind) == row) {
            let moved: Vec<Point> = p
                .outline
                .iter()
                .map(|q| Point::new(q.x + x, q.y + y))
                .collect();
            out.push(polygon_path(&moved));
            x += p.width + gap;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cut::{inside, poly_bounds};

    fn find(ps: &[Panel], k: PanelKind) -> &Panel {
        ps.iter().find(|p| p.kind == k).expect("panel")
    }

    /// Whether `p` has material `along` its `side`, half a thickness in.
    fn material(p: &Panel, side: usize, along: f64, t: f64) -> bool {
        inside(
            side_point(p.width, p.height, side, along, t / 2.0),
            &p.outline,
        )
    }

    fn joint(p: &Panel, side: usize) -> Joint {
        match p.edges[side] {
            Edge::Joint(j) => j,
            Edge::Flat => panic!("{:?} side {side} is flat", p.kind),
        }
    }

    /// Along the joint between `a`'s side `sa` and `b`'s side `sb`
    /// (`reversed` when they run opposite ways), between its corners,
    /// exactly one of them has material; on the corners at most one.
    fn mates(a: &Panel, sa: usize, b: &Panel, sb: usize, reversed: bool, t: f64) {
        let ja = joint(a, sa);
        let jb = joint(b, sb);
        assert!((ja.len - jb.len).abs() < 1e-9);
        assert!(ja.owner != jb.owner);
        assert_eq!(ja.n % 2, 1);
        assert_eq!(ja.n, jb.n);
        let steps = 400;
        let mut fingers_a = 0;
        let mut last_a = None;
        for i in 0..steps {
            let at = ja.len * (i as f64 + 0.5) / steps as f64;
            let bt = if reversed { ja.len - at } else { at };
            let ma = material(a, sa, at, t);
            let mb = material(b, sb, bt, t);
            if at > ja.c0 && at < ja.len - ja.c1 {
                assert!(ma != mb, "{:?}/{:?} at {at}", a.kind, b.kind);
            } else {
                assert!(!(ma && mb), "{:?}/{:?} corner at {at}", a.kind, b.kind);
            }
            if last_a != Some(ma) && ma {
                fingers_a += 1;
            }
            last_a = Some(ma);
        }
        assert!(fingers_a >= 2, "{:?}", a.kind);
    }

    #[test]
    fn a_closed_box_has_six_panels_an_open_one_five() {
        let ps = panels(&BoxSpec::default()).expect("box");
        assert_eq!(ps.len(), 6);
        let open = panels(&BoxSpec {
            open_top: true,
            ..Default::default()
        })
        .expect("box");
        assert_eq!(open.len(), 5);
        assert!(!open.iter().any(|p| p.kind == PanelKind::Top));
    }

    #[test]
    fn panels_have_the_outside_sizes() {
        let spec = BoxSpec::default();
        let ps = panels(&spec).expect("box");
        for (k, w, h) in [
            (PanelKind::Front, 200.0, 100.0),
            (PanelKind::Back, 200.0, 100.0),
            (PanelKind::Left, 150.0, 100.0),
            (PanelKind::Right, 150.0, 100.0),
            (PanelKind::Bottom, 200.0, 150.0),
            (PanelKind::Top, 200.0, 150.0),
        ] {
            let b = poly_bounds(&find(&ps, k).outline).expect("box");
            assert!(
                (b.width() - w).abs() < 1e-9 && (b.height() - h).abs() < 1e-9,
                "{k:?} {b:?}"
            );
            assert!(b.x0.abs() < 1e-9 && b.y0.abs() < 1e-9);
        }
    }

    #[test]
    fn inside_sizes_add_the_material() {
        let t = 6.0;
        let spec = BoxSpec {
            sizes: Sizes::Inside,
            ..Default::default()
        };
        assert_eq!(spec.outside(), (212.0, 162.0, 112.0));
        let open = BoxSpec {
            open_top: true,
            ..spec
        };
        assert_eq!(open.outside(), (212.0, 162.0, 106.0));
        // The bottom fits between the walls: cut back by the thickness
        // where it meets them, it spans the inside width and depth.
        let ps = panels(&spec).expect("box");
        let bottom = find(&ps, PanelKind::Bottom);
        let has = |x: f64, y: f64| inside(Point::new(x, y), &bottom.outline);
        assert!(has(t + 0.01, t + 0.01) && has(206.0 - 0.01, 156.0 - 0.01));
        assert!(!has(t - 0.01, t + 0.01) && !has(206.0 + 0.01, 156.0 - 0.01));
        assert!(!material(bottom, 0, 3.0, t), "the corner belongs to a wall");
        // The front stands on its full height: open, its top is straight.
        let front = find(&panels(&open).expect("box"), PanelKind::Front).clone();
        assert_eq!(front.edges[2], Edge::Flat);
        assert!((poly_bounds(&front.outline).expect("box").height() - 106.0).abs() < 1e-9);
    }

    #[test]
    fn every_finger_fills_a_slot_of_its_mate() {
        let t = 6.0;
        for open_top in [false, true] {
            let spec = BoxSpec {
                open_top,
                width: 230.0,
                depth: 140.0,
                height: 95.0,
                ..Default::default()
            };
            let ps = panels(&spec).expect("box");
            let front = find(&ps, PanelKind::Front);
            let back = find(&ps, PanelKind::Back);
            let left = find(&ps, PanelKind::Left);
            let right = find(&ps, PanelKind::Right);
            let bottom = find(&ps, PanelKind::Bottom);
            // The walls on the bottom: the front and back along x, the
            // sides along y.
            mates(front, 0, bottom, 0, false, t);
            mates(back, 0, bottom, 2, true, t);
            mates(left, 0, bottom, 3, true, t);
            mates(right, 0, bottom, 1, false, t);
            // The upright corners: the front's right side and a side's
            // left one run opposite ways.
            mates(front, 1, right, 3, true, t);
            mates(front, 3, left, 1, true, t);
            mates(back, 1, left, 3, true, t);
            if !open_top {
                let top = find(&ps, PanelKind::Top);
                mates(front, 2, top, 0, true, t);
                mates(left, 2, top, 1, true, t);
            }
        }
    }

    #[test]
    fn each_corner_of_the_box_belongs_to_one_panel() {
        let t = 6.0;
        let ps = panels(&BoxSpec::default()).expect("box");
        let has = |k: PanelKind, x: f64, y: f64| inside(Point::new(x, y), &find(&ps, k).outline);
        // The bottom-left corner cube, seen from each panel.
        assert!(has(PanelKind::Front, t / 2.0, t / 2.0));
        assert!(!has(PanelKind::Left, t / 2.0, t / 2.0));
        assert!(!has(PanelKind::Left, 150.0 - t / 2.0, t / 2.0));
        assert!(!has(PanelKind::Bottom, t / 2.0, t / 2.0));
    }

    #[test]
    fn fingers_follow_the_finger_width() {
        assert_eq!(stretches(188.0, 20.0), 9);
        assert_eq!(stretches(200.0, 20.0), 9);
        assert_eq!(stretches(30.0, 20.0), 3);
        assert_eq!(stretches(f64::NAN, 20.0), 3);
        assert_eq!(stretches(1e12, 0.5), MAX_FINGERS);
        let ps = panels(&BoxSpec::default()).expect("box");
        assert_eq!(joint(find(&ps, PanelKind::Front), 0).n, 9);
    }

    #[test]
    fn too_small_and_damaged_settings() {
        assert_eq!(
            panels(&BoxSpec {
                height: 20.0,
                ..Default::default()
            }),
            Err(BoxError::TooSmall)
        );
        let odd = BoxSpec {
            width: f64::NAN,
            thickness: -3.0,
            finger: f64::INFINITY,
            ..Default::default()
        };
        let ps = panels(&odd).expect("defaults stand in");
        assert_eq!(ps.len(), 6);
    }

    #[test]
    fn the_layout_puts_the_panels_side_by_side() {
        use tracedraw_core::geometry::Shape as _;
        let ps = panels(&BoxSpec::default()).expect("box");
        let laid = layout(&ps, Point::new(10.0, 20.0), 5.0);
        assert_eq!(laid.len(), 6);
        let boxes: Vec<_> = laid.iter().map(|p| p.bounding_box()).collect();
        // Front, back, left, right in a row, 5 apart.
        assert!((boxes[0].x0 - 10.0).abs() < 1e-9 && (boxes[0].y0 - 20.0).abs() < 1e-9);
        assert!((boxes[1].x0 - 215.0).abs() < 1e-9);
        assert!((boxes[2].x0 - 420.0).abs() < 1e-9);
        assert!((boxes[3].x0 - 575.0).abs() < 1e-9);
        // The bottom and the top above them.
        assert!((boxes[4].y0 - 125.0).abs() < 1e-9 && (boxes[4].x0 - 10.0).abs() < 1e-9);
        assert!((boxes[5].x0 - 215.0).abs() < 1e-9);
        for (i, a) in boxes.iter().enumerate() {
            for b in boxes.iter().skip(i + 1) {
                assert!(a.intersect(*b).area() <= 0.0, "{a:?} {b:?}");
            }
        }
    }
}
