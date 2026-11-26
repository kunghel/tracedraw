//! Shape Generator packings: circle packing, the sunflower spiral and
//! the square subdivision with geometric motifs.
//!
//! - Circle packing: circles of many sizes fill the box (or the picked
//!   outline) without touching. Random places are tried; each takes the
//!   largest circle that fits there (up to the largest size), kept when it
//!   is at least the smallest size. Early tries find room for big circles
//!   and later ones fill the gaps with small ones.
//! - Sunflower spiral: dots placed by a fixed turn (the golden angle,
//!   137.508 degrees, by default) and a distance growing with the square
//!   root of their number, so they spread evenly in interlocking spirals.
//! - Squares and motifs: the box is cut into squares, each split again
//!   into four at random up to the deepest level; every square takes a
//!   palette colour and a motif in another colour (a quarter disc, a half
//!   disc, a disc, a ring, a triangle, a smaller square, a leaf or bars).
//!
//! Model space: mm, y up, the box's bottom left at the origin.

use crate::shape_gen::{polygon, CellShape, Generated, Paint, Rng, ShapeGenSettings};
use std::f64::consts::{FRAC_PI_2, PI, TAU};
use tracedraw_core::geometry::{Affine, BezPath, Point, Rect, Vec2};

/// The default colours of the squares and motifs: cream, red, blue,
/// yellow and near black.
pub fn default_motif_palette() -> Vec<[u8; 3]> {
    vec![
        [242, 234, 216],
        [214, 48, 49],
        [32, 72, 160],
        [244, 190, 30],
        [24, 24, 24],
    ]
}

/// A full circle of radius `r` round `c` as four cubic quarters.
pub(crate) fn circle(c: Point, r: f64) -> BezPath {
    tracedraw_core::geometry::ellipse_path(Rect::new(c.x - r, c.y - r, c.x + r, c.y + r))
}

/// An elliptical arc round `c` with radii `rx`, `ry` from angle `a0` to
/// `a1` (radians), as cubic quarters at most; `start` adds the move.
fn arc(path: &mut BezPath, c: Point, rx: f64, ry: f64, a0: f64, a1: f64, start: bool) {
    let sweep = a1 - a0;
    let n = (sweep.abs() / FRAC_PI_2).ceil().max(1.0) as usize;
    let step = sweep / n as f64;
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    let at = |a: f64| Point::new(c.x + rx * a.cos(), c.y + ry * a.sin());
    let tan = |a: f64| Vec2::new(-rx * a.sin(), ry * a.cos());
    if start {
        path.move_to(at(a0));
    }
    for i in 0..n {
        let (s, e) = (a0 + step * i as f64, a0 + step * (i + 1) as f64);
        path.curve_to(at(s) + tan(s) * k, at(e) - tan(e) * k, at(e));
    }
}

// ----- circle packing --------------------------------------------------------

/// The flattened picked outline: its segments, for the distance to it.
fn outline_segments(b: &BezPath, tol: f64) -> Vec<(Point, Point)> {
    use tracedraw_core::geometry::PathEl;
    let mut segs = Vec::new();
    let mut first: Option<Point> = None;
    let mut last: Option<Point> = None;
    tracedraw_core::geometry::flatten(b, tol, &mut |el| match el {
        PathEl::MoveTo(p) => {
            if let (Some(f), Some(l)) = (first, last) {
                segs.push((l, f));
            }
            first = Some(p);
            last = Some(p);
        }
        PathEl::LineTo(p) => {
            if let Some(l) = last {
                segs.push((l, p));
            }
            last = Some(p);
        }
        PathEl::ClosePath => {
            if let (Some(f), Some(l)) = (first, last) {
                segs.push((l, f));
            }
            last = first;
        }
        _ => {}
    });
    if let (Some(f), Some(l)) = (first, last) {
        if (f - l).hypot() > 1e-9 {
            segs.push((l, f));
        }
    }
    segs
}

fn seg_dist(p: Point, a: Point, b: Point) -> f64 {
    let ab = b - a;
    let len2 = ab.hypot2();
    let t = if len2 > 1e-18 {
        ((p - a).dot(ab) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p - (a + ab * t)).hypot()
}

/// The circles of a packing: middles and radii.
pub fn pack_circles(s: &ShapeGenSettings) -> Vec<(Point, f64)> {
    let (w, h) = (s.width, s.height);
    let rmin = s.pack_min / 2.0;
    let rmax = (s.pack_max / 2.0).max(rmin);
    let gap = s.pack_gap;
    let boundary = s.boundary.as_ref();
    let segs = boundary
        .map(|b| outline_segments(b, (w.max(h) / 800.0).max(0.01)))
        .unwrap_or_default();
    // Buckets one largest circle and gap wide: a circle farther than one
    // bucket can never limit a new circle below the largest size.
    let cell = (2.0 * rmax + gap).max(w.max(h) / 400.0).max(1e-6);
    let nx = ((w / cell).ceil() as usize).max(1);
    let ny = ((h / cell).ceil() as usize).max(1);
    let mut buckets: Vec<Vec<u32>> = vec![Vec::new(); nx * ny];
    let mut circles: Vec<(Point, f64)> = Vec::new();
    let target = s.pack_count as usize;
    let tries = target.saturating_mul(40).clamp(1000, 400_000);
    let mut rng = Rng::new(s.seed, 0xc1c1e);
    for _ in 0..tries {
        if circles.len() >= target {
            break;
        }
        let p = Point::new(rng.unit() * w, rng.unit() * h);
        if !crate::shape_gen_cells::inside(boundary, p) {
            continue;
        }
        let mut free = rmax;
        if boundary.is_none() {
            free = free.min(p.x).min(w - p.x).min(p.y).min(h - p.y);
        } else {
            for (a, b) in &segs {
                free = free.min(seg_dist(p, *a, *b));
                if free < rmin {
                    break;
                }
            }
        }
        if free < rmin {
            continue;
        }
        let bx = ((p.x / cell) as usize).min(nx - 1);
        let by = ((p.y / cell) as usize).min(ny - 1);
        'near: for yy in by.saturating_sub(1)..=(by + 1).min(ny - 1) {
            for xx in bx.saturating_sub(1)..=(bx + 1).min(nx - 1) {
                for &i in &buckets[yy * nx + xx] {
                    let (c, r) = circles[i as usize];
                    free = free.min((p - c).hypot() - r - gap);
                    if free < rmin {
                        break 'near;
                    }
                }
            }
        }
        if free < rmin {
            continue;
        }
        buckets[by * nx + bx].push(circles.len() as u32);
        circles.push((p, free));
    }
    circles
}

pub(crate) fn circle_packing(s: &ShapeGenSettings, out: &mut Generated) {
    out.frame = Some(Rect::new(0.0, 0.0, s.width, s.height));
    let paint = if s.pack_filled {
        Paint::Solid
    } else {
        Paint::Line
    };
    for (c, r) in pack_circles(s) {
        if out.full() {
            out.cut = true;
            return;
        }
        out.push(circle(c, r), true, paint);
    }
}

// ----- sunflower spiral ------------------------------------------------------

/// The dots' middles, turns (radians) and diameters.
pub fn phyllotaxis_dots(s: &ShapeGenSettings) -> Vec<(Point, f64, f64)> {
    let n = s.phy_count.max(1) as usize;
    let c = Point::new(s.width / 2.0, s.height / 2.0);
    let reach = s.width.min(s.height) / 2.0;
    let turn = s.phy_angle.to_radians();
    let grow = s.phy_grow / 100.0;
    // Dots sit about sqrt(pi) * spacing apart; the outer ones (their size
    // times `grow`) must still fit in the box.
    let k = (PI.sqrt() * s.phy_size / 100.0).max(1e-6);
    let edge = grow.max(1.0);
    // reach = spacing * sqrt(n) + spacing * k * edge / 2.
    let spacing = reach / ((n as f64).sqrt() + k * edge / 2.0);
    (0..n)
        .map(|i| {
            let r = spacing * (i as f64 + 0.5).sqrt();
            let a = i as f64 * turn;
            let t = (r / (spacing * (n as f64).sqrt())).clamp(0.0, 1.0);
            let d = spacing * k * (1.0 + (grow - 1.0) * t);
            (Point::new(c.x + r * a.cos(), c.y + r * a.sin()), a, d)
        })
        .collect()
}

pub(crate) fn phyllotaxis(s: &ShapeGenSettings, out: &mut Generated) {
    out.frame = Some(Rect::new(0.0, 0.0, s.width, s.height));
    let paint = if s.phy_filled {
        Paint::Solid
    } else {
        Paint::Line
    };
    for (p, a, d) in phyllotaxis_dots(s) {
        if out.full() {
            out.cut = true;
            return;
        }
        let half = d / 2.0;
        if half <= 1e-6 {
            continue;
        }
        let path = match s.phy_shape {
            CellShape::Circle => circle(p, half),
            CellShape::Square | CellShape::Diamond => {
                let extra = if s.phy_shape == CellShape::Diamond {
                    PI / 4.0
                } else {
                    0.0
                };
                let mut q = tracedraw_core::geometry::rect_path(
                    Rect::new(p.x - half, p.y - half, p.x + half, p.y + half),
                    0.0,
                );
                q.apply_affine(Affine::rotate_about(a + extra, p));
                q
            }
        };
        out.push(path, true, paint);
    }
}

// ----- squares and motifs ----------------------------------------------------

/// The motif a square takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Motif {
    Quarter,
    Half,
    Disc,
    Ring,
    Triangle,
    Square,
    Leaf,
    Bars,
}

const MOTIFS: [Motif; 8] = [
    Motif::Quarter,
    Motif::Half,
    Motif::Disc,
    Motif::Ring,
    Motif::Triangle,
    Motif::Square,
    Motif::Leaf,
    Motif::Bars,
];

/// The leaves of the subdivision: each square and its level.
pub fn quadtree_cells(s: &ShapeGenSettings) -> Vec<(Rect, u32)> {
    let (w, h) = (s.width, s.height);
    let side = w.min(h).max(1e-9);
    let cols = ((w / side).round() as usize).max(1);
    let rows = ((h / side).round() as usize).max(1);
    let (cw, ch) = (w / cols as f64, h / rows as f64);
    let mut rng = Rng::new(s.seed, 0x9ad7);
    let mut out = Vec::new();
    let mut stack: Vec<(Rect, u32)> = Vec::new();
    for j in (0..rows).rev() {
        for i in (0..cols).rev() {
            let x0 = i as f64 * cw;
            let y0 = j as f64 * ch;
            stack.push((Rect::new(x0, y0, x0 + cw, y0 + ch), 0));
        }
    }
    let chance = s.qt_split / 100.0;
    while let Some((r, level)) = stack.pop() {
        // The first cut always happens when there are levels to go, so a
        // square box does not stay one plain square.
        let split = level < s.qt_depth.saturating_sub(1)
            && (level == 0 && s.qt_depth > 1 || rng.unit() < chance)
            && out.len() + stack.len() < crate::shape_gen::MAX_ITEMS / 2;
        if split {
            let m = r.center();
            for q in [
                Rect::new(m.x, m.y, r.x1, r.y1),
                Rect::new(r.x0, m.y, m.x, r.y1),
                Rect::new(m.x, r.y0, r.x1, m.y),
                Rect::new(r.x0, r.y0, m.x, m.y),
            ] {
                stack.push((q, level + 1));
            }
        } else {
            out.push((r, level));
        }
    }
    out
}

fn tone_of(i: usize, n: usize) -> Paint {
    if n <= 1 {
        Paint::Tone(0)
    } else {
        Paint::Tone(((i as f64 / (n - 1) as f64) * 255.0).round() as u8)
    }
}

pub(crate) fn quadtree(s: &ShapeGenSettings, out: &mut Generated) {
    out.palette = s.qt_colors.clone();
    out.frame = Some(Rect::new(0.0, 0.0, s.width, s.height));
    let n = s.qt_colors.len().max(1);
    let mut rng = Rng::new(s.seed, 0xba05);
    for (cell, _) in quadtree_cells(s) {
        if out.full() {
            out.cut = true;
            return;
        }
        let r = cell.inflate(-s.qt_gap / 2.0, -s.qt_gap / 2.0);
        if r.width() <= 1e-6 || r.height() <= 1e-6 {
            continue;
        }
        let back = rng.below(n as u32) as usize;
        let mut front = rng.below(n as u32) as usize;
        if n > 1 && front == back {
            front = (front + 1 + rng.below(n as u32 - 1) as usize) % n;
        }
        out.push(polygon(&corners(r)), true, tone_of(back, n));
        let motif = MOTIFS[rng.below(MOTIFS.len() as u32) as usize];
        let turn = rng.below(4) as f64 * FRAC_PI_2;
        let path = motif_path(motif, r, turn);
        out.push(path, true, tone_of(front, n));
    }
}

fn corners(r: Rect) -> [Point; 4] {
    [
        Point::new(r.x0, r.y0),
        Point::new(r.x1, r.y0),
        Point::new(r.x1, r.y1),
        Point::new(r.x0, r.y1),
    ]
}

/// A motif drawn in the square `r`, a quarter turn `turn` round its
/// middle (squashed to fit when the square is a little oblong).
fn motif_path(m: Motif, r: Rect, turn: f64) -> BezPath {
    // Drawn in a unit square (-1..1) and mapped onto the cell.
    let c = Point::ZERO;
    let mut p = BezPath::new();
    match m {
        Motif::Quarter => {
            // A quarter disc from a corner, the square's full size.
            let o = Point::new(-1.0, -1.0);
            p.move_to(o);
            p.line_to(Point::new(1.0, -1.0));
            arc(&mut p, o, 2.0, 2.0, 0.0, FRAC_PI_2, false);
            p.close_path();
        }
        Motif::Half => {
            let o = Point::new(0.0, -1.0);
            p.move_to(Point::new(-1.0, -1.0));
            p.line_to(Point::new(1.0, -1.0));
            arc(&mut p, o, 1.0, 1.0, 0.0, PI, false);
            p.close_path();
        }
        Motif::Disc => {
            p = circle(c, 0.8);
        }
        Motif::Ring => {
            p = circle(c, 0.85);
            let mut hole = BezPath::new();
            arc(&mut hole, c, 0.45, 0.45, TAU, 0.0, true);
            hole.close_path();
            p.extend(hole);
        }
        Motif::Triangle => {
            p = polygon(&[
                Point::new(-1.0, -1.0),
                Point::new(1.0, -1.0),
                Point::new(-1.0, 1.0),
            ]);
        }
        Motif::Square => {
            p = polygon(&[
                Point::new(-0.5, -0.5),
                Point::new(0.5, -0.5),
                Point::new(0.5, 0.5),
                Point::new(-0.5, 0.5),
            ]);
        }
        Motif::Leaf => {
            // Two quarter arcs between opposite corners.
            p.move_to(Point::new(-1.0, -1.0));
            arc(&mut p, Point::new(-1.0, 1.0), 2.0, 2.0, -FRAC_PI_2, 0.0, false);
            arc(&mut p, Point::new(1.0, -1.0), 2.0, 2.0, FRAC_PI_2, PI, false);
            p.close_path();
        }
        Motif::Bars => {
            for k in 0..3 {
                let y0 = -1.0 + k as f64 * 0.8;
                let mut bar = polygon(&[
                    Point::new(-1.0, y0),
                    Point::new(1.0, y0),
                    Point::new(1.0, y0 + 0.4),
                    Point::new(-1.0, y0 + 0.4),
                ]);
                if k == 0 {
                    p = bar;
                } else {
                    p.extend(std::mem::take(&mut bar));
                }
            }
        }
    }
    let m = Affine::translate(r.center().to_vec2())
        * Affine::scale_non_uniform(r.width() / 2.0, r.height() / 2.0)
        * Affine::rotate(turn);
    p.apply_affine(m);
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape_gen::{generate, GenKind};

    fn settings(kind: GenKind) -> ShapeGenSettings {
        ShapeGenSettings {
            kind,
            ..ShapeGenSettings::default()
        }
    }

    #[test]
    fn packed_circles_never_overlap_and_stay_in_the_box() {
        let s = settings(GenKind::CirclePacking).sanitized();
        let cs = pack_circles(&s);
        assert!(cs.len() > 100, "{}", cs.len());
        for (i, (a, ra)) in cs.iter().enumerate() {
            assert!(*ra >= s.pack_min / 2.0 - 1e-9 && *ra <= s.pack_max / 2.0 + 1e-9);
            assert!(a.x - ra >= -1e-9 && a.x + ra <= s.width + 1e-9);
            assert!(a.y - ra >= -1e-9 && a.y + ra <= s.height + 1e-9);
            for (b, rb) in &cs[i + 1..] {
                assert!((*a - *b).hypot() >= ra + rb + s.pack_gap - 1e-9);
            }
        }
        // Sizes vary: big circles first, small ones in the gaps.
        let big = cs.iter().map(|c| c.1).fold(0.0, f64::max);
        let small = cs.iter().map(|c| c.1).fold(f64::MAX, f64::min);
        assert!(big > small * 3.0, "{big} {small}");
        let g = generate(&s);
        assert_eq!(g.items.len(), cs.len());
    }

    #[test]
    fn a_packing_fills_only_the_picked_outline() {
        // A ring: the circles stay out of its hole.
        let mut b = circle(Point::new(50.0, 50.0), 45.0);
        let mut hole = BezPath::new();
        arc(&mut hole, Point::new(50.0, 50.0), 20.0, 20.0, TAU, 0.0, true);
        hole.close_path();
        b.extend(std::mem::take(&mut hole));
        let s = ShapeGenSettings {
            boundary: Some(b),
            ..settings(GenKind::CirclePacking)
        }
        .sanitized();
        let cs = pack_circles(&s);
        assert!(cs.len() > 50);
        let mid = Point::new(50.0, 50.0);
        for (c, r) in cs {
            let d = (c - mid).hypot();
            assert!(d - r >= 20.0 - 0.1 && d + r <= 45.0 + 0.1, "{c:?} {r}");
        }
    }

    #[test]
    fn sunflower_dots_turn_by_the_golden_angle_and_fit() {
        let s = settings(GenKind::Phyllotaxis).sanitized();
        let dots = phyllotaxis_dots(&s);
        assert_eq!(dots.len(), s.phy_count as usize);
        let golden = 137.508f64.to_radians();
        assert!(((dots[1].1 - dots[0].1) - golden).abs() < 1e-3);
        let g = generate(&s);
        let b = g.bounds().expect("bounds");
        assert!(b.x0 >= -1e-6 && b.y0 >= -1e-6, "{b:?}");
        assert!(b.x1 <= s.width + 1e-6 && b.y1 <= s.height + 1e-6, "{b:?}");
        // Bigger towards the edge when asked.
        let s = ShapeGenSettings {
            phy_grow: 200.0,
            ..s
        };
        let dots = phyllotaxis_dots(&s);
        assert!(dots.last().expect("dot").2 > dots[0].2 * 1.8);
    }

    #[test]
    fn squares_cover_the_box_once_and_each_takes_a_motif() {
        let s = settings(GenKind::Quadtree).sanitized();
        let cells = quadtree_cells(&s);
        let area: f64 = cells.iter().map(|(r, _)| r.area()).sum();
        assert!((area - s.width * s.height).abs() < 1e-6);
        assert!(cells.len() > 4);
        assert!(cells.iter().all(|(_, l)| *l < s.qt_depth));
        let g = generate(&s);
        assert_eq!(g.items.len(), cells.len() * 2);
        assert_eq!(g.palette, s.qt_colors);
        // The motif's colour differs from its square's.
        for pair in g.items.chunks(2) {
            assert_ne!(pair[0].paint, pair[1].paint);
        }
        // No splitting: one square per box side.
        let one = ShapeGenSettings {
            qt_depth: 1,
            ..s.clone()
        };
        assert_eq!(quadtree_cells(&one).len(), 1);
    }
}
