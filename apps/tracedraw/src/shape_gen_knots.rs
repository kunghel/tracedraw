//! Shape Generator interlace: knotwork and star strapwork whose bands
//! pass over and under each other in turn.
//!
//! - Knotwork: a grid of square tiles whose strands run diagonally and
//!   cross in the middle of every tile edge. A crossing can be replaced
//!   by a break (a wall along the edge, or across it) that turns the two
//!   strands back instead; breaks are placed at random by the seed, and
//!   mirrored left to right and top to bottom when asked. The box's border
//!   is always a wall. Each strand is a smooth closed curve.
//! - Ten-point stars, eight-point stars, six-point stars: straight
//!   straps drawn by rays from the middle of every edge of a tiling (the
//!   thin and thick rhombs of an aperiodic tiling, squares, hexagons),
//!   each at the contact angle to its edge, stopping where it meets the
//!   ray from the neighbouring edge. The straps cross straight over the
//!   edges and turn where the rays meet.
//!
//! Weaving: every crossing of two strands is found; walking along a
//! strand its crossings go over, under, over in turn, and each crossing's
//! two strands get opposite turns (the turns spread from strand to strand
//! across each crossing). A strand under another is cut there, leaving a
//! gap on each side of the strand on top. Strands are drawn as one line
//! or as a band of two lines (the band's width).
//!
//! Model space: mm, y up, the box's bottom left at the origin.

use crate::shape_gen::{Generated, Paint, ShapeGenSettings};
use std::collections::HashMap;
use tracedraw_core::geometry::{BezPath, Point, Rect, Vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnotType {
    #[default]
    Knotwork,
    Star10,
    Star8,
    Star6,
}

impl KnotType {
    pub const ALL: [KnotType; 4] = [
        KnotType::Knotwork,
        KnotType::Star10,
        KnotType::Star8,
        KnotType::Star6,
    ];
    pub fn key(self) -> &'static str {
        match self {
            KnotType::Knotwork => "shapegen.knot_celtic",
            KnotType::Star10 => "shapegen.knot_star10",
            KnotType::Star8 => "shapegen.knot_star8",
            KnotType::Star6 => "shapegen.knot_star6",
        }
    }
    /// Whether the straps follow the panel's contact angle (ten-point
    /// stars keep 74 degrees, which makes their decagons).
    pub fn uses_angle(self) -> bool {
        matches!(self, KnotType::Star8 | KnotType::Star6)
    }
}

/// A strand: its points and whether it closes on itself.
#[derive(Debug, Clone)]
pub struct Strand {
    pub pts: Vec<Point>,
    pub closed: bool,
}

pub(crate) fn interlace(s: &ShapeGenSettings, out: &mut Generated) {
    let rect = Rect::new(0.0, 0.0, s.width, s.height);
    out.frame = Some(rect);
    let strands = match s.knot_type {
        KnotType::Knotwork => knotwork(s),
        KnotType::Star10 => {
            let polys = crate::shape_gen_curves::penrose_rhombs(rect.inflate(s.knot_size, s.knot_size), s.knot_size)
                .into_iter()
                .map(|(_, q)| q.to_vec())
                .collect::<Vec<_>>();
            strap_strands(&polys, 74f64.to_radians())
        }
        KnotType::Star8 => strap_strands(&square_grid(rect, s.knot_size), s.knot_angle.to_radians()),
        KnotType::Star6 => strap_strands(&hex_grid(rect, s.knot_size), s.knot_angle.to_radians()),
    };
    weave(&strands, s, rect, out);
}

// ----- knotwork --------------------------------------------------------------

/// What happens where two strands meet in the middle of a tile edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Meet {
    Cross,
    /// A wall along the edge: each strand turns back into its own tile.
    Along,
    /// A wall across the edge: the strands pass beside each other.
    Across,
}

/// A tile corner direction: x and y signs of the strand's way.
type Dir = (i8, i8);

/// The knotwork's closed strands, smooth.
pub fn knotwork(s: &ShapeGenSettings) -> Vec<Strand> {
    let (w, h) = (s.width, s.height);
    let cols = ((w / s.knot_size).round() as i64).max(1);
    let rows = ((h / s.knot_size).round() as i64).max(1);
    let (tw, th) = (w / cols as f64, h / rows as f64);
    let chance = s.knot_breaks / 100.0;
    // Nodes: vertical edge midpoints (0, i, j) at (i * tw, (j + 0.5) * th),
    // horizontal ones (1, i, j) at ((i + 0.5) * tw, j * th).
    let meet = |kind: u8, i: i64, j: i64| -> Meet {
        let border = if kind == 0 {
            i == 0 || i == cols
        } else {
            j == 0 || j == rows
        };
        if border {
            return Meet::Along;
        }
        // Mirrored: the same choice for the node's images.
        let (mut ci, mut cj) = (i, j);
        if s.knot_symmetric {
            let mi = if kind == 0 { cols - i } else { cols - 1 - i };
            let mj = if kind == 0 { rows - 1 - j } else { rows - j };
            ci = ci.min(mi);
            cj = cj.min(mj);
        }
        let hv = crate::shape_gen::hash_unit(s.seed, ci, cj, kind as i64);
        if hv >= chance {
            Meet::Cross
        } else if crate::shape_gen::hash_unit(s.seed, ci, cj, kind as i64 + 7) < 0.5 {
            Meet::Along
        } else {
            Meet::Across
        }
    };
    let node_pt = |kind: u8, i: i64, j: i64| -> Point {
        if kind == 0 {
            Point::new(i as f64 * tw, (j as f64 + 0.5) * th)
        } else {
            Point::new((i as f64 + 0.5) * tw, j as f64 * th)
        }
    };
    // A half side: from a node in a direction, along a tile's diamond
    // side to the next node. Named by (kind, i, j, dir).
    type Half = (u8, i64, i64, Dir);
    let other_end = |h: Half| -> Half {
        let (kind, i, j, (dx, dy)) = h;
        // From a vertical node, a diagonal reaches a horizontal node and
        // back.
        if kind == 0 {
            let ni = if dx > 0 { i } else { i - 1 };
            let nj = if dy > 0 { j + 1 } else { j };
            (1, ni, nj, (-dx, -dy))
        } else {
            let ni = if dx > 0 { i + 1 } else { i };
            let nj = if dy > 0 { j } else { j - 1 };
            (0, ni, nj, (-dx, -dy))
        }
    };
    // Whether the half side lies inside the grid.
    let valid = |h: Half| -> bool {
        let (kind, i, j, (dx, dy)) = h;
        if kind == 0 {
            let ti = if dx > 0 { i } else { i - 1 };
            (0..cols).contains(&ti) && (0..rows).contains(&j)
        } else {
            let tj = if dy > 0 { j } else { j - 1 };
            (0..cols).contains(&i) && (0..rows).contains(&tj)
        }
    };
    // Where a strand arriving at a node along `d` (pointing from the node
    // back the way it came) leaves.
    let pair = |kind: u8, m: Meet, d: Dir| -> Dir {
        let (dx, dy) = d;
        match (m, kind) {
            (Meet::Cross, _) => (-dx, -dy),
            // Vertical edge: along keeps the side (x), across keeps y.
            (Meet::Along, 0) | (Meet::Across, 1) => (dx, -dy),
            _ => (-dx, dy),
        }
    };
    let mut used: std::collections::HashSet<Half> = std::collections::HashSet::new();
    let mut strands = Vec::new();
    let pull = 0.18 * tw.min(th);
    for kind in 0..2u8 {
        let (imax, jmax) = if kind == 0 { (cols, rows - 1) } else { (cols - 1, rows) };
        for i in 0..=imax {
            for j in 0..=jmax {
                for d in [(1i8, 1i8), (1, -1), (-1, 1), (-1, -1)] {
                    let start: Half = (kind, i, j, d);
                    if !valid(start) || used.contains(&start) {
                        continue;
                    }
                    let mut pts = Vec::new();
                    let mut h = start;
                    let mut guard = 0usize;
                    loop {
                        guard += 1;
                        if guard > 1_000_000 || !used.insert(h) {
                            break;
                        }
                        let e = other_end(h);
                        used.insert(e);
                        let a = node_pt(h.0, h.1, h.2);
                        let b = node_pt(e.0, e.1, e.2);
                        pts.push(a.midpoint(b));
                        // At the far node: pass through or turn.
                        let m = meet(e.0, e.1, e.2);
                        let out_dir = pair(e.0, m, e.3);
                        if m == Meet::Cross {
                            pts.push(b);
                        } else {
                            let v = Vec2::new(
                                (e.3 .0 + out_dir.0) as f64,
                                (e.3 .1 + out_dir.1) as f64,
                            );
                            let len = v.hypot().max(1e-9);
                            pts.push(b + v / len * pull);
                        }
                        h = (e.0, e.1, e.2, out_dir);
                        if h == start {
                            break;
                        }
                    }
                    if pts.len() >= 3 {
                        strands.push(Strand {
                            pts: catmull(&pts, 8),
                            closed: true,
                        });
                    }
                }
            }
        }
    }
    strands
}

/// A closed Catmull-Rom curve through `pts`, `per` points per span.
fn catmull(pts: &[Point], per: usize) -> Vec<Point> {
    let n = pts.len();
    let mut out = Vec::with_capacity(n * per);
    for i in 0..n {
        let p0 = pts[(i + n - 1) % n];
        let p1 = pts[i];
        let p2 = pts[(i + 1) % n];
        let p3 = pts[(i + 2) % n];
        // Samples between the control points, so two strands crossing at
        // a control point cross inside a segment of each.
        for k in 0..per {
            let t = (k as f64 + 0.5) / per as f64;
            let (t2, t3) = (t * t, t * t * t);
            let f = |a: f64, b: f64, c: f64, d: f64| {
                0.5 * (2.0 * b
                    + (-a + c) * t
                    + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2
                    + (-a + 3.0 * b - 3.0 * c + d) * t3)
            };
            out.push(Point::new(
                f(p0.x, p1.x, p2.x, p3.x),
                f(p0.y, p1.y, p2.y, p3.y),
            ));
        }
    }
    out
}

// ----- star straps -----------------------------------------------------------

/// Squares of side `t` covering the box and one more row round it.
fn square_grid(rect: Rect, t: f64) -> Vec<Vec<Point>> {
    let mut out = Vec::new();
    let c = rect.center();
    let ni = (rect.width() / 2.0 / t).ceil() as i64 + 1;
    let nj = (rect.height() / 2.0 / t).ceil() as i64 + 1;
    for j in -nj..nj {
        for i in -ni..ni {
            let (x, y) = (c.x + i as f64 * t, c.y + j as f64 * t);
            out.push(vec![
                Point::new(x, y),
                Point::new(x + t, y),
                Point::new(x + t, y + t),
                Point::new(x, y + t),
            ]);
        }
    }
    out
}

/// Hexagons of edge `t` covering the box and one more ring round it.
fn hex_grid(rect: Rect, t: f64) -> Vec<Vec<Point>> {
    let mut out = Vec::new();
    let c = rect.center();
    let dx = 3f64.sqrt() * t;
    let dy = 1.5 * t;
    let ni = (rect.width() / 2.0 / dx).ceil() as i64 + 2;
    let nj = (rect.height() / 2.0 / dy).ceil() as i64 + 2;
    for j in -nj..=nj {
        for i in -ni..=ni {
            let shift = if j.rem_euclid(2) == 0 { 0.0 } else { dx / 2.0 };
            let m = Point::new(c.x + i as f64 * dx + shift, c.y + j as f64 * dy);
            out.push(
                (0..6)
                    .map(|k| {
                        let a = std::f64::consts::FRAC_PI_6 + k as f64 * std::f64::consts::FRAC_PI_3;
                        Point::new(m.x + t * a.cos(), m.y + t * a.sin())
                    })
                    .collect(),
            );
        }
    }
    out
}

fn ray_meet(p: Point, u: Vec2, q: Point, v: Vec2) -> Option<(Point, f64, f64)> {
    let den = u.cross(v);
    if den.abs() < 1e-12 {
        return None;
    }
    let d = q - p;
    let t = d.cross(v) / den;
    let s = d.cross(u) / den;
    if t <= 1e-9 || s <= 1e-9 {
        return None;
    }
    Some((p + u * t, t, s))
}

/// Straps from the polygons' edge middles at `theta` to their edges,
/// chained into strands: straight through each edge middle and turning
/// where two rays meet.
pub fn strap_strands(polys: &[Vec<Point>], theta: f64) -> Vec<Strand> {
    let key = |p: Point| ((p.x * 1e4).round() as i64, (p.y * 1e4).round() as i64);
    // Each segment: its two ends; ends grouped by place.
    let mut segs: Vec<(Point, Point)> = Vec::new();
    for poly in polys {
        let n = poly.len();
        if n < 3 {
            continue;
        }
        let mut area = 0.0;
        for i in 0..n {
            area += poly[i].to_vec2().cross(poly[(i + 1) % n].to_vec2());
        }
        let pts: Vec<Point> = if area < 0.0 {
            poly.iter().rev().copied().collect()
        } else {
            poly.clone()
        };
        // Two rays from every edge's middle, one toward each end.
        let mut rays: Vec<(Point, Vec2)> = Vec::with_capacity(2 * n);
        for k in 0..n {
            let (a, b) = (pts[k], pts[(k + 1) % n]);
            let e = (b - a).normalize();
            let nrm = Vec2::new(-e.y, e.x);
            let m = a.midpoint(b);
            rays.push((m, e * theta.cos() + nrm * theta.sin()));
            rays.push((m, -e * theta.cos() + nrm * theta.sin()));
        }
        let size = pts
            .iter()
            .flat_map(|a| pts.iter().map(move |b| (*a - *b).hypot()))
            .fold(0.0, f64::max)
            .max(1e-9);
        let inside = |p: Point| {
            (0..n).all(|k| {
                let (a, b) = (pts[k], pts[(k + 1) % n]);
                (b - a).cross(p - a) >= -1e-7 * size
            })
        };
        // Every pair of rays that meet inside, nearest meetings first;
        // two rays on one line pointing at each other meet half way.
        let mut meets: Vec<(f64, usize, usize, Point)> = Vec::new();
        for i in 0..rays.len() {
            for j in i + 1..rays.len() {
                let ((p, u), (q, v)) = (rays[i], rays[j]);
                if (p - q).hypot() < 1e-9 * size {
                    continue;
                }
                let d = q - p;
                if u.cross(v).abs() < 1e-9 {
                    let along = d.dot(u);
                    if u.dot(v) < 0.0 && along > 0.0 && d.cross(u).abs() < 1e-7 * size {
                        meets.push((along / 2.0, i, j, p.midpoint(q)));
                    }
                    continue;
                }
                if let Some((x, t, s)) = ray_meet(p, u, q, v) {
                    if inside(x) {
                        meets.push((t.max(s), i, j, x));
                    }
                }
            }
        }
        meets.sort_by(|x, y| x.0.total_cmp(&y.0));
        let mut used = vec![false; rays.len()];
        for (_, i, j, x) in meets {
            if used[i] || used[j] {
                continue;
            }
            used[i] = true;
            used[j] = true;
            segs.push((rays[i].0, x));
            segs.push((rays[j].0, x));
        }
    }
    // Ends by place: (segment, which end).
    let mut ends: HashMap<(i64, i64), Vec<(usize, u8)>> = HashMap::new();
    for (i, (a, b)) in segs.iter().enumerate() {
        ends.entry(key(*a)).or_default().push((i, 0));
        ends.entry(key(*b)).or_default().push((i, 1));
    }
    let end_pt = |(i, e): (usize, u8)| if e == 0 { segs[i].0 } else { segs[i].1 };
    let far = |(i, e): (usize, u8)| if e == 0 { segs[i].1 } else { segs[i].0 };
    // Pair the ends at each place so the strands cross there rather than
    // touch: in turning order round the place, each end goes on to the
    // one half way round (the straight way on, for two or four ends). An
    // odd end out is left open.
    let mut partner: HashMap<(usize, u8), (usize, u8)> = HashMap::new();
    for list in ends.values() {
        let mut left: Vec<(usize, u8)> = list.clone();
        left.sort_unstable();
        left.dedup();
        if left.len() % 2 == 1 {
            // Drop the end that bends most from all the others.
            let worst = (0..left.len())
                .max_by(|&a, &b| {
                    let o = end_pt(left[a]);
                    let straight = |i: usize| {
                        let d = (far(left[i]) - o).normalize();
                        (0..left.len())
                            .filter(|&j| j != i)
                            .map(|j| d.dot((far(left[j]) - o).normalize()))
                            .fold(f64::MAX, f64::min)
                    };
                    straight(a).total_cmp(&straight(b))
                })
                .unwrap_or(0);
            left.remove(worst);
        }
        if left.len() < 2 {
            continue;
        }
        let o = end_pt(left[0]);
        left.sort_by(|a, b| {
            let (da, db) = (far(*a) - o, far(*b) - o);
            da.y.atan2(da.x).total_cmp(&db.y.atan2(db.x))
        });
        let half = left.len() / 2;
        for i in 0..half {
            let (ea, eb) = (left[i], left[i + half]);
            partner.insert(ea, eb);
            partner.insert(eb, ea);
        }
    }
    // Walk the chains.
    let mut used = vec![false; segs.len()];
    let mut strands = Vec::new();
    let walk = |start: (usize, u8), used: &mut Vec<bool>, pts: &mut Vec<Point>| -> bool {
        // From `start`'s far end onward; true when it came back round.
        let mut cur = start;
        loop {
            used[cur.0] = true;
            let exit = (cur.0, 1 - cur.1);
            pts.push(end_pt(exit));
            match partner.get(&exit) {
                Some(&next) if next == start => return true,
                Some(&next) if !used[next.0] => cur = next,
                _ => return false,
            }
        }
    };
    for i in 0..segs.len() {
        if used[i] {
            continue;
        }
        // Go back to an open end first, if there is one.
        let mut start = (i, 0u8);
        let mut steps = 0;
        while let Some(&p) = partner.get(&start) {
            let back = (p.0, 1 - p.1);
            steps += 1;
            if back.0 == i || steps > segs.len() {
                break;
            }
            start = back;
        }
        let mut pts = vec![end_pt(start)];
        let closed = walk(start, &mut used, &mut pts);
        if closed {
            pts.pop();
        }
        if pts.len() >= 2 {
            strands.push(Strand { pts, closed });
        }
    }
    strands
}

// ----- weaving ---------------------------------------------------------------

/// A strand's points with the closing point repeated, and the distance
/// along it at each point.
fn path_of(st: &Strand) -> (Vec<Point>, Vec<f64>) {
    let mut pts = st.pts.clone();
    if st.closed {
        if let Some(f) = pts.first().copied() {
            pts.push(f);
        }
    }
    let mut acc = Vec::with_capacity(pts.len());
    let mut d = 0.0;
    for k in 0..pts.len() {
        if k > 0 {
            d += (pts[k] - pts[k - 1]).hypot();
        }
        acc.push(d);
    }
    (pts, acc)
}

/// A crossing: the two strands, the distance along each, and the sine of
/// the angle between them.
#[derive(Debug, Clone, Copy)]
pub struct Crossing {
    pub a: usize,
    pub sa: f64,
    pub b: usize,
    pub sb: f64,
    pub sin: f64,
}

/// Every crossing between the strands (and of a strand with itself).
pub fn crossings(strands: &[Strand]) -> Vec<Crossing> {
    let paths: Vec<(Vec<Point>, Vec<f64>)> = strands.iter().map(path_of).collect();
    // Segments in square buckets.
    let mut total = 0.0;
    let mut count = 0usize;
    for (pts, _) in &paths {
        for k in 1..pts.len() {
            total += (pts[k] - pts[k - 1]).hypot();
            count += 1;
        }
    }
    let cell = (total / count.max(1) as f64 * 2.0).max(1e-6);
    let mut grid: HashMap<(i64, i64), Vec<(usize, usize)>> = HashMap::new();
    for (si, (pts, _)) in paths.iter().enumerate() {
        for k in 1..pts.len() {
            let (p, q) = (pts[k - 1], pts[k]);
            let (x0, x1) = ((p.x.min(q.x) / cell).floor() as i64, (p.x.max(q.x) / cell).floor() as i64);
            let (y0, y1) = ((p.y.min(q.y) / cell).floor() as i64, (p.y.max(q.y) / cell).floor() as i64);
            for gx in x0..=x1 {
                for gy in y0..=y1 {
                    grid.entry((gx, gy)).or_default().push((si, k));
                }
            }
        }
    }
    let mut seen: std::collections::HashSet<((usize, usize), (usize, usize))> =
        std::collections::HashSet::new();
    let mut out = Vec::new();
    for list in grid.values() {
        for x in 0..list.len() {
            for y in x + 1..list.len() {
                let (a, b) = if list[x] < list[y] {
                    (list[x], list[y])
                } else {
                    (list[y], list[x])
                };
                if a.0 == b.0 {
                    let n = paths[a.0].0.len() - 1;
                    let gap = b.1.abs_diff(a.1);
                    if gap <= 1 || (strands[a.0].closed && gap + 1 >= n) {
                        continue;
                    }
                }
                if !seen.insert((a, b)) {
                    continue;
                }
                let (p0, p1) = (paths[a.0].0[a.1 - 1], paths[a.0].0[a.1]);
                let (q0, q1) = (paths[b.0].0[b.1 - 1], paths[b.0].0[b.1]);
                let (u, v) = (p1 - p0, q1 - q0);
                let den = u.cross(v);
                if den.abs() < 1e-14 {
                    continue;
                }
                let d = q0 - p0;
                let t = d.cross(v) / den;
                let s = d.cross(u) / den;
                // Half open, so a crossing at a shared point counts once.
                if !(0.0..1.0).contains(&t) || !(0.0..1.0).contains(&s) {
                    continue;
                }
                let sin = (den / (u.hypot() * v.hypot()).max(1e-18)).abs();
                let sa = paths[a.0].1[a.1 - 1] + u.hypot() * t;
                let sb = paths[b.0].1[b.1 - 1] + v.hypot() * s;
                out.push(Crossing {
                    a: a.0,
                    sa,
                    b: b.0,
                    sb,
                    sin,
                });
            }
        }
    }
    out.sort_by(|x, y| {
        (x.a, x.b)
            .cmp(&(y.a, y.b))
            .then(x.sa.total_cmp(&y.sa))
            .then(x.sb.total_cmp(&y.sb))
    });
    out
}

/// For each crossing, whether strand `a` goes over: along every strand
/// the crossings alternate, and at each crossing one strand is on top.
pub fn over_under(strands: &[Strand], xs: &[Crossing]) -> Vec<bool> {
    // Each strand's crossings in order along it: (distance, crossing, is a).
    let mut along: Vec<Vec<(f64, usize, bool)>> = vec![Vec::new(); strands.len()];
    for (ci, c) in xs.iter().enumerate() {
        along[c.a].push((c.sa, ci, true));
        along[c.b].push((c.sb, ci, false));
    }
    let mut index = vec![[0usize; 2]; xs.len()];
    for list in &mut along {
        list.sort_by(|x, y| x.0.total_cmp(&y.0));
        for (k, (_, ci, is_a)) in list.iter().enumerate() {
            index[*ci][if *is_a { 0 } else { 1 }] = k;
        }
    }
    // Each strand's turn: over at its even crossings (0) or odd ones (1).
    let mut phase: Vec<Option<usize>> = vec![None; strands.len()];
    for start in 0..strands.len() {
        if phase[start].is_some() {
            continue;
        }
        phase[start] = Some(0);
        let mut queue = std::collections::VecDeque::from([start]);
        while let Some(si) = queue.pop_front() {
            let Some(p) = phase[si] else { continue };
            for &(_, ci, is_a) in &along[si] {
                let c = xs[ci];
                let (mine, other, other_k) = if is_a {
                    (index[ci][0], c.b, index[ci][1])
                } else {
                    (index[ci][1], c.a, index[ci][0])
                };
                if other == si {
                    continue;
                }
                if phase[other].is_none() {
                    phase[other] = Some((1 + mine + p + other_k) % 2);
                    queue.push_back(other);
                }
            }
        }
    }
    xs.iter()
        .enumerate()
        .map(|(ci, c)| {
            let p = phase[c.a].unwrap_or(0);
            (index[ci][0] + p).is_multiple_of(2)
        })
        .collect()
}

/// The point at distance `d` along `pts` (with distances `acc`).
fn point_at(pts: &[Point], acc: &[f64], d: f64) -> Point {
    let k = acc.partition_point(|v| *v < d).clamp(1, pts.len() - 1);
    let span = (acc[k] - acc[k - 1]).max(1e-12);
    pts[k - 1].lerp(pts[k], ((d - acc[k - 1]) / span).clamp(0.0, 1.0))
}

/// The part of `pts` from distance `d0` to `d1` (`d0 < d1`, inside it).
fn piece(pts: &[Point], acc: &[f64], d0: f64, d1: f64) -> Vec<Point> {
    let mut out = vec![point_at(pts, acc, d0)];
    for k in 0..pts.len() {
        if acc[k] > d0 && acc[k] < d1 {
            out.push(pts[k]);
        }
    }
    out.push(point_at(pts, acc, d1));
    out
}

/// `pts` moved `off` mm to their left, the corners mitred.
fn offset(pts: &[Point], closed: bool, off: f64) -> Vec<Point> {
    let n = pts.len();
    (0..n)
        .map(|k| {
            let prev = if k > 0 {
                Some(pts[k - 1])
            } else if closed {
                Some(pts[n - 1])
            } else {
                None
            };
            let next = if k + 1 < n {
                Some(pts[k + 1])
            } else if closed {
                Some(pts[0])
            } else {
                None
            };
            let p = pts[k];
            let dir_in = prev.map(|q| (p - q).normalize());
            let dir_out = next.map(|q| (q - p).normalize());
            let nrm = |d: Vec2| Vec2::new(-d.y, d.x);
            match (dir_in, dir_out) {
                (Some(a), Some(b)) => {
                    let (na, nb) = (nrm(a), nrm(b));
                    let m = na + nb;
                    let len = m.hypot();
                    if len < 1e-9 {
                        return p + na * off;
                    }
                    let m = m / len;
                    p + m * (off / m.dot(na).max(0.25))
                }
                (Some(a), None) | (None, Some(a)) => p + nrm(a) * off,
                (None, None) => p,
            }
        })
        .collect()
}

/// Cut a polyline to the box: the runs inside it.
fn clip_runs(pts: &[Point], rect: Rect) -> Vec<Vec<Point>> {
    let inside = |p: Point| {
        p.x >= rect.x0 - 1e-9 && p.x <= rect.x1 + 1e-9 && p.y >= rect.y0 - 1e-9 && p.y <= rect.y1 + 1e-9
    };
    let mut runs: Vec<Vec<Point>> = Vec::new();
    let mut cur: Vec<Point> = Vec::new();
    for k in 0..pts.len() {
        let p = pts[k];
        if k == 0 {
            if inside(p) {
                cur.push(p);
            }
            continue;
        }
        let q = pts[k - 1];
        match crate::shape_gen_curves::clip_segment(rect, q, p) {
            Some((a, b)) => {
                if cur.is_empty() {
                    cur.push(a);
                } else if (a - *cur.last().unwrap_or(&a)).hypot() > 1e-7 {
                    runs.push(std::mem::take(&mut cur));
                    cur.push(a);
                }
                cur.push(b);
                if !inside(p) {
                    runs.push(std::mem::take(&mut cur));
                }
            }
            None => {
                if !cur.is_empty() {
                    runs.push(std::mem::take(&mut cur));
                }
            }
        }
    }
    if !cur.is_empty() {
        runs.push(cur);
    }
    runs.retain(|r| r.len() >= 2);
    runs
}

fn polyline(pts: &[Point], closed: bool) -> BezPath {
    let mut p = BezPath::new();
    for (k, q) in pts.iter().enumerate() {
        if k == 0 {
            p.move_to(*q);
        } else {
            p.line_to(*q);
        }
    }
    if closed {
        p.close_path();
    }
    p
}

fn weave(strands: &[Strand], s: &ShapeGenSettings, rect: Rect, out: &mut Generated) {
    let xs = crossings(strands);
    let over = over_under(strands, &xs);
    let half = if s.knot_ribbon { s.knot_width / 2.0 } else { 0.0 };
    // Where each strand is cut: (distance along, half the gap).
    let mut cuts: Vec<Vec<(f64, f64)>> = vec![Vec::new(); strands.len()];
    for (c, a_over) in xs.iter().zip(&over) {
        let (under, d) = if *a_over { (c.b, c.sb) } else { (c.a, c.sa) };
        let g = half / c.sin.max(0.3) + s.knot_gap;
        cuts[under].push((d, g));
    }
    for (si, st) in strands.iter().enumerate() {
        let (pts, acc) = path_of(st);
        if pts.len() < 2 {
            continue;
        }
        let total = *acc.last().unwrap_or(&0.0);
        if total <= 1e-9 {
            continue;
        }
        // The kept stretches, as distance ranges.
        let mut gaps: Vec<(f64, f64)> = cuts[si].iter().map(|(d, g)| (d - g, d + g)).collect();
        gaps.sort_by(|x, y| x.0.total_cmp(&y.0));
        let lines: Vec<(Vec<Point>, Vec<f64>)> = if s.knot_ribbon {
            [half, -half]
                .iter()
                .map(|o| {
                    let mut e = offset(&st.pts, st.closed, *o);
                    if st.closed {
                        if let Some(f) = e.first().copied() {
                            e.push(f);
                        }
                    }
                    (e, acc.clone())
                })
                .collect()
        } else {
            vec![(pts.clone(), acc.clone())]
        };
        let mut keep: Vec<(f64, f64)> = Vec::new();
        if gaps.is_empty() {
            keep.push((0.0, total));
        } else if st.closed {
            // Round the loop from the end of one gap to the next one's start.
            for k in 0..gaps.len() {
                let from = gaps[k].1;
                let to = if k + 1 < gaps.len() {
                    gaps[k + 1].0
                } else {
                    gaps[0].0 + total
                };
                if to > from {
                    keep.push((from, to));
                }
            }
        } else {
            let mut at = 0.0;
            for (g0, g1) in &gaps {
                if *g0 > at {
                    keep.push((at, g0.min(total)));
                }
                at = at.max(*g1);
            }
            if at < total {
                keep.push((at, total));
            }
        }
        let whole = gaps.is_empty() && st.closed;
        for (line, la) in &lines {
            for (d0, d1) in &keep {
                let pts_run: Vec<Point> = if whole {
                    line[..line.len() - 1].to_vec()
                } else if *d1 <= total {
                    piece(line, la, d0.max(0.0), *d1)
                } else {
                    // Across the loop's start.
                    let mut a = piece(line, la, *d0, total);
                    let b = piece(line, la, 0.0, d1 - total);
                    a.extend(b.into_iter().skip(1));
                    a
                };
                if whole && pts_run.iter().all(|p| rect.contains(*p)) {
                    if out.full() {
                        out.cut = true;
                        return;
                    }
                    out.push(polyline(&pts_run, true), true, Paint::Line);
                    continue;
                }
                let mut run = pts_run;
                if whole {
                    if let Some(f) = run.first().copied() {
                        run.push(f);
                    }
                }
                for r in clip_runs(&run, rect) {
                    if out.full() {
                        out.cut = true;
                        return;
                    }
                    out.push(polyline(&r, false), false, Paint::Line);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape_gen::{generate, GenKind};

    fn knot(t: KnotType) -> ShapeGenSettings {
        ShapeGenSettings {
            kind: GenKind::Interlace,
            knot_type: t,
            ..ShapeGenSettings::default()
        }
        .sanitized()
    }

    /// Along every strand the crossings go over and under in turn.
    fn alternates(strands: &[Strand]) {
        let xs = crossings(strands);
        assert!(!xs.is_empty());
        let over = over_under(strands, &xs);
        let mut bad = 0;
        let mut total = 0;
        for si in 0..strands.len() {
            let mut list: Vec<(f64, bool)> = Vec::new();
            for (c, o) in xs.iter().zip(&over) {
                // A strand crossing itself passes the crossing twice.
                if c.a == si {
                    list.push((c.sa, *o));
                }
                if c.b == si {
                    list.push((c.sb, !*o));
                }
            }
            list.sort_by(|a, b| a.0.total_cmp(&b.0));
            for w in list.windows(2) {
                total += 1;
                if w[0].1 == w[1].1 {
                    bad += 1;
                }
            }
        }
        assert!(total > 10);
        assert_eq!(bad, 0, "{bad} of {total} neighbouring crossings repeat");
    }

    #[test]
    fn a_plain_plait_crosses_in_every_edge_middle() {
        let s = ShapeGenSettings {
            knot_breaks: 0.0,
            width: 40.0,
            height: 40.0,
            knot_size: 10.0,
            ..knot(KnotType::Knotwork)
        };
        let strands = knotwork(&s);
        let xs = crossings(&strands);
        // 4 x 4 tiles: 3 inner vertical edges per row and column of 4,
        // the same horizontally.
        assert_eq!(xs.len(), 2 * 3 * 4, "{}", xs.len());
        assert!(strands.iter().all(|s| s.closed));
        alternates(&strands);
    }

    #[test]
    fn knotwork_with_breaks_still_alternates_and_stays_inside() {
        for seed in 1..6 {
            let s = ShapeGenSettings {
                seed,
                ..knot(KnotType::Knotwork)
            };
            alternates(&knotwork(&s));
            let g = generate(&s);
            let b = g.bounds().expect("bounds");
            assert!(b.x0 >= -1e-6 && b.y0 >= -1e-6, "{b:?}");
            assert!(b.x1 <= s.width + 1e-6 && b.y1 <= s.height + 1e-6, "{b:?}");
        }
    }

    #[test]
    fn star_straps_weave_and_fit_the_box() {
        for t in [KnotType::Star10, KnotType::Star8, KnotType::Star6] {
            let s = knot(t);
            let g = generate(&s);
            assert!(g.items.len() > 50, "{t:?}: {}", g.items.len());
            let b = g.bounds().expect("bounds");
            assert!(b.x0 >= -1e-6 && b.y0 >= -1e-6, "{t:?} {b:?}");
            assert!(b.x1 <= s.width + 1e-6 && b.y1 <= s.height + 1e-6, "{t:?} {b:?}");
        }
        let s = knot(KnotType::Star8);
        let strands = strap_strands(&square_grid(Rect::new(0.0, 0.0, 60.0, 60.0), 15.0), s.knot_angle.to_radians());
        // Strands through the inner squares close on themselves.
        assert!(strands.iter().any(|s| s.closed));
        alternates(&strands);
    }

    #[test]
    fn lines_or_bands() {
        let s = knot(KnotType::Knotwork);
        let bands = generate(&s).items.len();
        let lines = generate(&ShapeGenSettings {
            knot_ribbon: false,
            ..s
        })
        .items
        .len();
        assert_eq!(bands, lines * 2);
    }
}
