//! Places where outlines cross: an outline crossing itself (a figure
//! eight, a loop at a sharp node) cuts badly and fails offsets, and two
//! objects crossing each other usually want a weld before cutting.
//!
//! Outlines are flattened to lines within `tol / 4`; neighbouring
//! segments of the same subpath, which meet at their shared node, do not
//! count. Lines that run along each other (an overlap, see
//! [`crate::cleanup`]) do not count either.
//!
//! [`resolve_crossings`] and [`weld_crossing`] fix what was found: the
//! closed outlines are split where they cross and rebuilt as the outlines
//! of the area they fill, which cross nowhere. Where two rebuilt outlines
//! would still touch at a former crossing (the two loops of a figure
//! eight), a dot as wide as the tolerance is cut out there so they come
//! apart. Open lines are kept as they are. [`separate_crossing`] moves
//! objects that cross each other apart instead.

use crate::subpaths;
use tracedraw_core::geometry::{BezPath, Point, Rect, Vec2};
use tracedraw_core::shaping::{self, FillRule, Op};

/// One crossing: where, and which objects (the same index twice for an
/// outline crossing itself).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Crossing {
    pub point: Point,
    pub a: usize,
    pub b: usize,
}

struct Seg {
    p0: Point,
    p1: Point,
    obj: usize,
    sub: usize,
    idx: usize,
    sub_len: usize,
    closed: bool,
    min_x: f64,
    max_x: f64,
}

/// The crossings of the objects' outlines: each object's own, and with
/// `between` also those between different objects. Crossings closer than
/// `tol` to one already found for the same objects are one.
pub fn find_crossings(objects: &[Vec<BezPath>], between: bool, tol: f64) -> Vec<Crossing> {
    let tol = tol.max(1e-6);
    let mut segs: Vec<Seg> = Vec::new();
    let mut sub_id = 0usize;
    for (obj, paths) in objects.iter().enumerate() {
        for path in paths {
            for sub in subpaths(path) {
                let mut pts: Vec<Point> = Vec::new();
                tracedraw_core::geometry::flatten(&sub.path, tol * 0.25, &mut |el| {
                    if let Some(p) = el.end_point() {
                        if pts.last().is_none_or(|q: &Point| (*q - p).hypot() > 1e-9) {
                            pts.push(p);
                        }
                    }
                });
                if sub.closed {
                    if let (Some(a), Some(b)) = (pts.first().copied(), pts.last().copied()) {
                        if (a - b).hypot() > 1e-9 {
                            pts.push(a);
                        }
                    }
                }
                if !pts.iter().all(|p| p.x.is_finite() && p.y.is_finite()) {
                    continue;
                }
                let n = pts.len().saturating_sub(1);
                for (k, w) in pts.windows(2).enumerate() {
                    segs.push(Seg {
                        p0: w[0],
                        p1: w[1],
                        obj,
                        sub: sub_id,
                        idx: k,
                        sub_len: n,
                        closed: sub.closed,
                        min_x: w[0].x.min(w[1].x),
                        max_x: w[0].x.max(w[1].x),
                    });
                }
                sub_id += 1;
            }
        }
    }
    segs.sort_by(|a, b| a.min_x.total_cmp(&b.min_x));
    let mut found: Vec<Crossing> = Vec::new();
    for i in 0..segs.len() {
        let s = &segs[i];
        for t in &segs[i + 1..] {
            if t.min_x > s.max_x + 1e-9 {
                break;
            }
            if s.obj != t.obj && !between {
                continue;
            }
            if neighbours(s, t) {
                continue;
            }
            let Some(p) = intersect(s.p0, s.p1, t.p0, t.p1) else {
                continue;
            };
            let (a, b) = if s.obj <= t.obj {
                (s.obj, t.obj)
            } else {
                (t.obj, s.obj)
            };
            if found
                .iter()
                .any(|c| c.a == a && c.b == b && (c.point - p).hypot() < tol)
            {
                continue;
            }
            found.push(Crossing { point: p, a, b });
        }
    }
    found
}

/// Segments that follow each other in one subpath (the last and first of
/// a closed one too).
fn neighbours(s: &Seg, t: &Seg) -> bool {
    if s.sub != t.sub {
        return false;
    }
    let (i, j) = if s.idx < t.idx {
        (s.idx, t.idx)
    } else {
        (t.idx, s.idx)
    };
    j == i + 1 || (s.closed && i == 0 && j + 1 == s.sub_len)
}

/// Where two segments meet, ends included; `None` when they are parallel.
fn intersect(a0: Point, a1: Point, b0: Point, b1: Point) -> Option<Point> {
    let r = a1 - a0;
    let s = b1 - b0;
    let den = r.cross(s);
    let scale = r.hypot() * s.hypot();
    if scale < 1e-18 || den.abs() <= 1e-12 * scale {
        return None;
    }
    let q = b0 - a0;
    let t = q.cross(s) / den;
    let u = q.cross(r) / den;
    const E: f64 = 1e-9;
    if (-E..=1.0 + E).contains(&t) && (-E..=1.0 + E).contains(&u) {
        Some(a0 + r * t)
    } else {
        None
    }
}

/// The closed subpaths of `path`, and its open ones.
fn split_open(path: &BezPath) -> (BezPath, BezPath) {
    let (mut closed, mut open) = (BezPath::new(), BezPath::new());
    for sub in subpaths(path) {
        let to = if sub.closed { &mut closed } else { &mut open };
        for el in sub.path.elements() {
            to.push(*el);
        }
    }
    (closed, open)
}

/// The area `closed` fills under `fill`, as outlines that cross nowhere.
fn filled_area(closed: &BezPath, fill: FillRule) -> BezPath {
    shaping::overlay_with(closed, &BezPath::new(), Op::Weld, fill)
}

/// `area` with a dot `gap` wide cut out wherever its outlines still
/// touch, then `open` added back.
fn finish(area: BezPath, open: &BezPath, gap: f64) -> BezPath {
    let gap = if gap.is_finite() { gap.max(1e-3) } else { 0.05 };
    let touches = find_crossings(&[vec![area.clone()]], false, gap);
    let mut out = if touches.is_empty() {
        area
    } else {
        let dots = touches
            .iter()
            .map(|c| dot(c.point, gap / 2.0))
            .reduce(|a, b| shaping::overlay(&a, &b, Op::Weld))
            .unwrap_or_default();
        shaping::overlay(&area, &dots, Op::Trim)
    };
    for el in open.elements() {
        out.push(*el);
    }
    out
}

/// A twelve-sided dot of radius `r` round `c`, in straight lines: a
/// circle this small would be flattened by the overlay to too few points
/// to cut anything.
fn dot(c: Point, r: f64) -> BezPath {
    let mut p = BezPath::new();
    for k in 0..12 {
        let a = k as f64 * std::f64::consts::PI / 6.0;
        let q = Point::new(c.x + r * a.cos(), c.y + r * a.sin());
        if k == 0 {
            p.move_to(q);
        } else {
            p.line_to(q);
        }
    }
    p.close_path();
    p
}

/// One object's outline with its own crossings fixed: its closed
/// subpaths, read with the object's `fill` rule, rebuilt as the outlines
/// of the area they fill (outer outlines and holes that cross neither
/// themselves nor each other), pulled `gap` apart where they would
/// still touch; its open subpaths kept as they are. A path without closed
/// subpaths comes back unchanged.
pub fn resolve_crossings(path: &BezPath, fill: FillRule, gap: f64) -> BezPath {
    let (closed, open) = split_open(path);
    if closed.elements().is_empty() {
        return path.clone();
    }
    finish(filled_area(&closed, fill), &open, gap)
}

/// Objects that cross each other welded into one outline: the areas
/// their closed outlines fill (each read with its own fill rule) joined
/// and rebuilt like [`resolve_crossings`], their open lines kept. All
/// paths in one space.
pub fn weld_crossing(paths: &[(BezPath, FillRule)], gap: f64) -> BezPath {
    let mut area: Option<BezPath> = None;
    let mut open_all = BezPath::new();
    for (path, fill) in paths {
        let (closed, open) = split_open(path);
        for el in open.elements() {
            open_all.push(*el);
        }
        if closed.elements().is_empty() {
            continue;
        }
        let a = filled_area(&closed, *fill);
        area = Some(match area {
            None => a,
            Some(u) => shaping::overlay(&u, &a, Op::Weld),
        });
    }
    finish(area.unwrap_or_default(), &open_all, gap)
}

/// The sets of objects that cross one another, from the crossings
/// [`find_crossings`] found between `n` objects: each set in index order,
/// two objects at least, sets ordered by their first object.
pub fn crossing_groups(found: &[Crossing], n: usize) -> Vec<Vec<usize>> {
    let mut parent: Vec<usize> = (0..n).collect();
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while let Some(&p) = parent.get(i) {
            if p == i {
                break;
            }
            let up = parent.get(p).copied().unwrap_or(p);
            if let Some(slot) = parent.get_mut(i) {
                *slot = up;
            }
            i = p;
        }
        i
    }
    for c in found {
        if c.a == c.b || c.a >= n || c.b >= n {
            continue;
        }
        let (ra, rb) = (root(&mut parent, c.a), root(&mut parent, c.b));
        if ra != rb {
            let (lo, hi) = if ra < rb { (ra, rb) } else { (rb, ra) };
            if let Some(slot) = parent.get_mut(hi) {
                *slot = lo;
            }
        }
    }
    let mut groups: Vec<Vec<usize>> = vec![Vec::new(); n];
    for i in 0..n {
        let r = root(&mut parent, i);
        if let Some(g) = groups.get_mut(r) {
            g.push(i);
        }
    }
    groups.retain(|g| g.len() >= 2);
    groups
}

/// How many directions [`separate_crossing`] tries for each object.
pub const SEPARATE_DIRECTIONS: usize = 32;

/// An object's outlines flattened: each subpath's points and whether it
/// is closed, and the box around them.
struct Flat {
    rings: Vec<(Vec<Point>, bool)>,
    bbox: Option<Rect>,
}

impl Flat {
    fn new(paths: &[BezPath], tol: f64) -> Flat {
        let mut rings = Vec::new();
        for path in paths {
            for sub in subpaths(path) {
                let mut pts: Vec<Point> = Vec::new();
                tracedraw_core::geometry::flatten(&sub.path, tol * 0.25, &mut |el| {
                    if let Some(p) = el.end_point() {
                        if pts.last().is_none_or(|q: &Point| (*q - p).hypot() > 1e-9) {
                            pts.push(p);
                        }
                    }
                });
                if pts.is_empty() || !pts.iter().all(|p| p.x.is_finite() && p.y.is_finite()) {
                    continue;
                }
                if sub.closed {
                    if let (Some(a), Some(b)) = (pts.first().copied(), pts.last().copied()) {
                        if (a - b).hypot() > 1e-9 {
                            pts.push(a);
                        }
                    }
                }
                rings.push((pts, sub.closed));
            }
        }
        let bbox = rings
            .iter()
            .flat_map(|(r, _)| r.iter())
            .map(|p| Rect::from_points(*p, *p))
            .reduce(|a, b| a.union(b));
        Flat { rings, bbox }
    }

    /// Whether `p` lies inside the area of the closed rings (even-odd).
    fn contains(&self, p: Point) -> bool {
        let mut inside = false;
        for (r, closed) in &self.rings {
            if !*closed {
                continue;
            }
            for w in r.windows(2) {
                let (a, b) = (w[0], w[1]);
                if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x)
                {
                    inside = !inside;
                }
            }
        }
        inside
    }
}

/// The distance from `p` to the segment `ab`.
fn to_segment(p: Point, a: Point, b: Point) -> f64 {
    let ab = b - a;
    let len2 = ab.hypot2();
    let t = if len2 > 0.0 {
        ((p - a).dot(ab) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p - (a + ab * t)).hypot()
}

fn boxes_meet(a: Rect, b: Rect) -> bool {
    a.x0 <= b.x1 && b.x0 <= a.x1 && a.y0 <= b.y1 && b.y0 <= a.y1
}

/// Whether `a` moved by `d` keeps `gap` from `b`: no outlines cross or
/// come closer than `gap`, and neither lies inside the other's area.
fn apart(a: &Flat, d: Vec2, b: &Flat, gap: f64) -> bool {
    let (Some(ab), Some(bb)) = (a.bbox, b.bbox) else {
        return true;
    };
    let ab = ab + d;
    if !boxes_meet(ab.inflate(gap, gap), bb) {
        return true;
    }
    let near_b = bb.inflate(gap, gap);
    for (ra, _) in &a.rings {
        for wa in ra.windows(2) {
            let (p0, p1) = (wa[0] + d, wa[1] + d);
            let sa = Rect::from_points(p0, p1);
            if !boxes_meet(sa, near_b) {
                continue;
            }
            let sa = sa.inflate(gap, gap);
            for (rb, _) in &b.rings {
                for wb in rb.windows(2) {
                    let (q0, q1) = (wb[0], wb[1]);
                    if !boxes_meet(sa, Rect::from_points(q0, q1)) {
                        continue;
                    }
                    if intersect(p0, p1, q0, q1).is_some() {
                        return false;
                    }
                    let dist = to_segment(p0, q0, q1)
                        .min(to_segment(p1, q0, q1))
                        .min(to_segment(q0, p0, p1))
                        .min(to_segment(q1, p0, p1));
                    if dist < gap {
                        return false;
                    }
                }
            }
        }
    }
    // No edges meet: each ring lies wholly inside or outside the other.
    let a_in_b = a
        .rings
        .iter()
        .any(|(r, _)| r.first().is_some_and(|p| b.contains(*p + d)));
    let b_in_a = b
        .rings
        .iter()
        .any(|(r, _)| r.first().is_some_and(|p| a.contains(*p - d)));
    !(a_in_b || b_in_a)
}

/// Objects that cross each other moved apart ("Move them apart"): in each
/// set of objects that cross one another ([`crossing_groups`] of the
/// crossings `found` between the `objects`), the objects that may not
/// move (`movable` false) and, when all may, the first (lowest) stay; the
/// others, in order, each move the shortest way, of
/// [`SEPARATE_DIRECTIONS`] evenly spread, that leaves `spacing` between
/// its outlines and those of every object staying or moved before it
/// (outlines flattened within `tol / 4`; an object inside another's area
/// counts as touching it). Objects in no set, and those crossing only
/// themselves, stay where they are and are kept clear of too. Returns how
/// far each object moves (zero for those that stay).
pub fn separate_crossing(
    objects: &[Vec<BezPath>],
    movable: &[bool],
    found: &[Crossing],
    spacing: f64,
    tol: f64,
) -> Vec<Vec2> {
    let n = objects.len();
    let mut moves = vec![Vec2::ZERO; n];
    let gap = if spacing.is_finite() {
        spacing.max(0.0)
    } else {
        0.0
    };
    let tol = if tol.is_finite() { tol.max(1e-6) } else { 0.05 };
    let flats: Vec<Flat> = objects.iter().map(|o| Flat::new(o, tol)).collect();
    let can_move = |i: usize| movable.get(i).copied().unwrap_or(false);
    let groups = crossing_groups(found, n);
    let mut to_move: Vec<usize> = Vec::new();
    for g in &groups {
        let anchored = g.iter().any(|i| !can_move(*i));
        for (k, i) in g.iter().enumerate() {
            if can_move(*i) && (anchored || k > 0) {
                to_move.push(*i);
            }
        }
    }
    to_move.sort_unstable();
    // Every object stays put except those still to move.
    let mut placed: Vec<usize> = (0..n).filter(|i| !to_move.contains(i)).collect();
    let all = flats
        .iter()
        .filter_map(|f| f.bbox)
        .reduce(|a, b| a.union(b));
    let Some(all) = all else {
        return moves;
    };
    let reach = all.width().hypot(all.height());
    for i in to_move {
        let Some(me) = flats.get(i) else {
            continue;
        };
        let Some(own) = me.bbox else {
            continue;
        };
        let clear = |d: Vec2| {
            placed.iter().all(|j| {
                let other = moves.get(*j).copied().unwrap_or(Vec2::ZERO);
                flats.get(*j).is_none_or(|f| apart(me, d - other, f, gap))
            })
        };
        let mut found_move: Option<Vec2> = None;
        if !clear(Vec2::ZERO) {
            let size = own.width().hypot(own.height());
            let step = (size / 16.0).max(gap * 0.5).max(tol);
            let limit = reach + size + 2.0 * gap + step;
            let mut best: Option<f64> = None;
            for k in 0..SEPARATE_DIRECTIONS {
                let a = k as f64 / SEPARATE_DIRECTIONS as f64 * std::f64::consts::TAU;
                let u = Vec2::new(a.cos(), a.sin());
                let mut t = step;
                while t <= limit && best.is_none_or(|b| t - step < b) {
                    if clear(u * t) {
                        // Between the last place blocked and this one.
                        let (mut lo, mut hi) = (t - step, t);
                        for _ in 0..16 {
                            let mid = (lo + hi) / 2.0;
                            if clear(u * mid) {
                                hi = mid;
                            } else {
                                lo = mid;
                            }
                        }
                        if best.is_none_or(|b| hi < b) {
                            best = Some(hi);
                            found_move = Some(u * hi);
                        }
                        break;
                    }
                    t += step;
                }
            }
        }
        if let (Some(v), Some(m)) = (found_move, moves.get_mut(i)) {
            *m = v;
        }
        placed.push(i);
    }
    moves
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::{Rect, Shape as _};

    #[test]
    fn a_figure_eight_crosses_itself_once() {
        let mut p = BezPath::new();
        p.move_to((0.0, 0.0));
        p.line_to((10.0, 10.0));
        p.line_to((10.0, 0.0));
        p.line_to((0.0, 10.0));
        p.close_path();
        let c = find_crossings(&[vec![p]], false, 0.01);
        assert_eq!(c.len(), 1);
        assert!((c[0].point - Point::new(5.0, 5.0)).hypot() < 1e-9);
        assert_eq!((c[0].a, c[0].b), (0, 0));
    }

    #[test]
    fn simple_shapes_do_not_cross_themselves() {
        let r = Rect::new(0.0, 0.0, 10.0, 10.0).to_path(0.01);
        let e = tracedraw_core::geometry::Circle::new((30.0, 5.0), 5.0).to_path(0.01);
        assert!(find_crossings(&[vec![r], vec![e]], false, 0.01).is_empty());
    }

    #[test]
    fn overlapping_objects_cross_only_when_asked() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0).to_path(0.01);
        let b = Rect::new(5.0, 5.0, 15.0, 15.0).to_path(0.01);
        let objects = [vec![a], vec![b]];
        assert!(find_crossings(&objects, false, 0.01).is_empty());
        let c = find_crossings(&objects, true, 0.01);
        assert_eq!(c.len(), 2);
        assert!(c.iter().all(|c| c.a == 0 && c.b == 1));
    }

    #[test]
    fn an_open_line_through_a_node_twice_touches_itself() {
        let mut p = BezPath::new();
        p.move_to((0.0, 0.0));
        p.line_to((10.0, 0.0));
        p.line_to((10.0, 10.0));
        p.line_to((5.0, -5.0));
        let c = find_crossings(&[vec![p]], false, 0.01);
        assert_eq!(c.len(), 1);
    }

    fn area(p: &BezPath) -> f64 {
        crate::measure::measure_object(std::slice::from_ref(p)).area
    }

    fn closed_subpaths(p: &BezPath) -> usize {
        subpaths(p).iter().filter(|s| s.closed).count()
    }

    #[test]
    fn a_fixed_figure_eight_is_two_loops_that_cross_nowhere() {
        let mut p = BezPath::new();
        p.move_to((0.0, 0.0));
        p.line_to((10.0, 10.0));
        p.line_to((10.0, 0.0));
        p.line_to((0.0, 10.0));
        p.close_path();
        for fill in [FillRule::EvenOdd, FillRule::NonZero] {
            let f = resolve_crossings(&p, fill, 0.05);
            assert!(
                find_crossings(&[vec![f.clone()]], false, 0.05).is_empty(),
                "{f:?}"
            );
            assert_eq!(closed_subpaths(&f), 2, "{f:?}");
            // Two triangles of 25 mm2, less the dot cut where they met.
            let a = area(&f);
            assert!(a < 50.0 && a > 49.99, "{fill:?}: {a}");
        }
    }

    #[test]
    fn a_fixed_star_follows_the_fill_rule_and_keeps_open_lines() {
        // A five-pointed star drawn as one closed line crossing itself
        // five times, and an open line beside it.
        let mut p = BezPath::new();
        for k in 0..5 {
            let a = std::f64::consts::FRAC_PI_2 + k as f64 * 4.0 * std::f64::consts::PI / 5.0;
            let q = Point::new(50.0 * a.cos(), 50.0 * a.sin());
            if k == 0 {
                p.move_to(q);
            } else {
                p.line_to(q);
            }
        }
        p.close_path();
        let before = find_crossings(&[vec![p.clone()]], false, 0.05);
        assert_eq!(before.len(), 5);
        p.move_to((80.0, 0.0));
        p.line_to((90.0, 10.0));
        let nonzero = resolve_crossings(&p, FillRule::NonZero, 0.05);
        let even_odd = resolve_crossings(&p, FillRule::EvenOdd, 0.05);
        for f in [&nonzero, &even_odd] {
            assert!(find_crossings(&[vec![f.clone()]], false, 0.05).is_empty());
            let open: Vec<_> = subpaths(f).into_iter().filter(|s| !s.closed).collect();
            assert_eq!(open.len(), 1, "the open line is kept");
        }
        // Nonzero fills the middle: one outline of ten corners. Even-odd
        // leaves it out: five points apart.
        assert_eq!(closed_subpaths(&nonzero), 1);
        assert_eq!(closed_subpaths(&even_odd), 5);
        assert!(area(&nonzero) > area(&even_odd) + 100.0);
        // Paths without closed outlines come back as they are.
        let mut line = BezPath::new();
        line.move_to((0.0, 0.0));
        line.line_to((10.0, 0.0));
        assert_eq!(resolve_crossings(&line, FillRule::EvenOdd, f64::NAN), line);
    }

    #[test]
    fn crossing_objects_weld_into_one_outline() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0).to_path(0.01);
        let b = Rect::new(5.0, 5.0, 15.0, 15.0).to_path(0.01);
        let c = Rect::new(40.0, 0.0, 50.0, 10.0).to_path(0.01);
        let d = Rect::new(45.0, 5.0, 55.0, 15.0).to_path(0.01);
        let e = Rect::new(100.0, 0.0, 110.0, 10.0).to_path(0.01);
        let objects = [vec![a.clone()], vec![c], vec![b.clone()], vec![e], vec![d]];
        let found = find_crossings(&objects, true, 0.05);
        assert_eq!(
            crossing_groups(&found, objects.len()),
            vec![vec![0, 2], vec![1, 4]]
        );
        assert!(crossing_groups(&found, 0).is_empty());
        let w = weld_crossing(&[(a, FillRule::EvenOdd), (b, FillRule::NonZero)], 0.05);
        assert_eq!(closed_subpaths(&w), 1);
        assert!((area(&w) - 175.0).abs() < 1e-6, "{}", area(&w));
        assert!(find_crossings(&[vec![w]], false, 0.05).is_empty());
        assert!(weld_crossing(&[], 0.05).elements().is_empty());
    }

    /// The closest two objects' outlines come, flattened finely: 0 when
    /// they cross or one lies inside the other.
    fn gap_between(a: &[BezPath], b: &[BezPath]) -> f64 {
        let (fa, fb) = (Flat::new(a, 0.01), Flat::new(b, 0.01));
        if !apart(&fa, Vec2::ZERO, &fb, 0.0) {
            return 0.0;
        }
        let mut best = f64::INFINITY;
        for (ra, _) in &fa.rings {
            for wa in ra.windows(2) {
                for (rb, _) in &fb.rings {
                    for wb in rb.windows(2) {
                        best = best
                            .min(to_segment(wa[0], wb[0], wb[1]))
                            .min(to_segment(wa[1], wb[0], wb[1]))
                            .min(to_segment(wb[0], wa[0], wa[1]))
                            .min(to_segment(wb[1], wa[0], wa[1]));
                    }
                }
            }
        }
        best
    }

    fn moved(o: &[BezPath], d: Vec2) -> Vec<BezPath> {
        o.iter()
            .map(|p| tracedraw_core::geometry::Affine::translate(d) * p.clone())
            .collect()
    }

    /// Move them apart: of two squares overlapping by 3 mm, the upper
    /// one moves the shortest way, 3 mm plus the spacing to the right;
    /// a third crossing the second moves too, clear of both and of an
    /// object nearby that crossed nothing; afterwards nothing crosses
    /// and every pair keeps the spacing. A locked object stays and the
    /// other moves instead.
    #[test]
    fn crossing_objects_move_apart_the_shortest_way() {
        let a = vec![Rect::new(0.0, 0.0, 10.0, 10.0).to_path(0.01)];
        let b = vec![Rect::new(7.0, 0.0, 17.0, 10.0).to_path(0.01)];
        let objects = vec![a.clone(), b.clone()];
        let found = find_crossings(&objects, true, 0.01);
        let m = separate_crossing(&objects, &[true, true], &found, 2.0, 0.01);
        assert_eq!(m[0], Vec2::ZERO, "the lowest stays");
        assert!(
            (m[1].x - 5.0).abs() < 0.01 && m[1].y.abs() < 1e-9,
            "{:?}",
            m[1]
        );
        // Locked, the upper one stays and the lower one moves left.
        let m = separate_crossing(&objects, &[true, false], &found, 2.0, 0.01);
        assert_eq!(m[1], Vec2::ZERO);
        assert!(
            (m[0].x + 5.0).abs() < 0.01 && m[0].y.abs() < 1e-9,
            "{:?}",
            m[0]
        );
        // Neither may move: nothing moves.
        let m = separate_crossing(&objects, &[false, false], &found, 2.0, 0.01);
        assert!(m.iter().all(|v| *v == Vec2::ZERO));

        // A circle crossing the second square, a star crossing the
        // circle, a square inside nothing just above them.
        let c = vec![tracedraw_core::geometry::Circle::new((18.0, 5.0), 6.0).to_path(0.01)];
        let mut star = BezPath::new();
        for k in 0..10 {
            let ang = k as f64 * std::f64::consts::PI / 5.0;
            let r = if k % 2 == 0 { 8.0 } else { 3.0 };
            let p = Point::new(24.0 + r * ang.cos(), 6.0 + r * ang.sin());
            if k == 0 {
                star.move_to(p);
            } else {
                star.line_to(p);
            }
        }
        star.close_path();
        let lone = vec![Rect::new(0.0, 14.0, 30.0, 20.0).to_path(0.01)];
        let objects = vec![a, b, c, vec![star], lone];
        let found = find_crossings(&objects, true, 0.01);
        assert!(!found.is_empty());
        let m = separate_crossing(&objects, &[true; 5], &found, 1.5, 0.01);
        assert_eq!(m[0], Vec2::ZERO);
        assert_eq!(m[4], Vec2::ZERO, "crossed nothing: stays");
        let after: Vec<Vec<BezPath>> = objects.iter().zip(&m).map(|(o, d)| moved(o, *d)).collect();
        assert!(find_crossings(&after, true, 0.01).is_empty());
        for i in 0..after.len() {
            for j in i + 1..after.len() {
                let g = gap_between(&after[i], &after[j]);
                assert!(g >= 1.5 - 0.01, "{i} and {j}: {g} mm apart");
            }
        }
        // Bad input moves nothing.
        assert!(separate_crossing(&[], &[], &[], 2.0, 0.01).is_empty());
        let m = separate_crossing(&objects, &[], &found, f64::NAN, f64::NAN);
        assert!(m.iter().all(|v| *v == Vec2::ZERO));
    }
}
