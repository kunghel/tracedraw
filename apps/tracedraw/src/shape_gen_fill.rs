//! Shape Generator fills: hatching, contour lines, mazes and
//! space-filling curves, inside the picked outline (holes left empty) or
//! the Width x Height box.
//!
//! - Hatch: parallel or crossed lines at an angle and spacing, cut exactly
//!   at the outline; by tone, layers of lines at turned angles, each drawn
//!   only where the picture is darker than its threshold.
//! - Contour: rings offset inward from the outline at a spacing, or the
//!   picture's tone lines (marching squares at evenly spaced levels).
//! - Maze: a perfect maze on square cells inside the outline (a random
//!   depth-first walk, one in each separate region), drawn as walls with
//!   an entrance and an exit, or as its passages.
//! - Space-filling curve: Hilbert, Moore (closed) or Peano over the box,
//!   cut where it leaves the outline, with sharp or rounded corners.
//!
//! Model space: mm, y up, the box's bottom left at the origin.

use crate::shape_gen::{smooth_through, Generated, Paint, Rng, ShapeGenSettings};
use crate::shape_gen_cells::{inside, Source};
use tracedraw_core::geometry::{BezPath, PathEl, Point, Rect, Vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HatchMode {
    #[default]
    Parallel,
    Cross,
    Tone,
}

impl HatchMode {
    pub const ALL: [HatchMode; 3] = [HatchMode::Parallel, HatchMode::Cross, HatchMode::Tone];
    pub fn key(self) -> &'static str {
        match self {
            HatchMode::Parallel => "shapegen.hatch_parallel",
            HatchMode::Cross => "shapegen.hatch_crossed",
            HatchMode::Tone => "shapegen.hatch_tone",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContourMode {
    #[default]
    Offset,
    Tone,
}

impl ContourMode {
    pub const ALL: [ContourMode; 2] = [ContourMode::Offset, ContourMode::Tone];
    pub fn key(self) -> &'static str {
        match self {
            ContourMode::Offset => "shapegen.contour_offset",
            ContourMode::Tone => "shapegen.contour_tone",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MazeStyle {
    #[default]
    Walls,
    Paths,
}

impl MazeStyle {
    pub const ALL: [MazeStyle; 2] = [MazeStyle::Walls, MazeStyle::Paths];
    pub fn key(self) -> &'static str {
        match self {
            MazeStyle::Walls => "shapegen.maze_walls",
            MazeStyle::Paths => "shapegen.maze_paths",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpaceCurve {
    #[default]
    Hilbert,
    Moore,
    Peano,
}

impl SpaceCurve {
    pub const ALL: [SpaceCurve; 3] = [SpaceCurve::Hilbert, SpaceCurve::Moore, SpaceCurve::Peano];
    pub fn key(self) -> &'static str {
        match self {
            SpaceCurve::Hilbert => "shapegen.curve_hilbert",
            SpaceCurve::Moore => "shapegen.curve_moore",
            SpaceCurve::Peano => "shapegen.curve_peano",
        }
    }
    /// The highest order the panel offers.
    pub fn max_order(self) -> u32 {
        match self {
            SpaceCurve::Hilbert | SpaceCurve::Moore => 8,
            SpaceCurve::Peano => 5,
        }
    }
}

// ----- the outline ----------------------------------------------------------

/// The outline as flat closed rings: the picked outline, or the box.
fn rings(s: &ShapeGenSettings) -> Vec<Vec<Point>> {
    let Some(b) = s.boundary.as_ref() else {
        return vec![vec![
            Point::new(0.0, 0.0),
            Point::new(s.width, 0.0),
            Point::new(s.width, s.height),
            Point::new(0.0, s.height),
        ]];
    };
    let tol = (s.width.max(s.height) / 4000.0).max(0.002);
    let mut out: Vec<Vec<Point>> = Vec::new();
    let mut cur: Vec<Point> = Vec::new();
    tracedraw_core::geometry::flatten(b, tol, &mut |el| match el {
        PathEl::MoveTo(p) => {
            if cur.len() >= 3 {
                out.push(std::mem::take(&mut cur));
            }
            cur.clear();
            cur.push(p);
        }
        PathEl::LineTo(p) => cur.push(p),
        PathEl::ClosePath => {
            if cur.len() >= 3 {
                out.push(std::mem::take(&mut cur));
            }
            cur.clear();
        }
        _ => {}
    });
    if cur.len() >= 3 {
        out.push(cur);
    }
    out
}

fn line(a: Point, b: Point) -> BezPath {
    let mut p = BezPath::new();
    p.move_to(a);
    p.line_to(b);
    p
}

fn polyline(pts: &[Point]) -> BezPath {
    let mut p = BezPath::new();
    for (k, q) in pts.iter().enumerate() {
        if k == 0 {
            p.move_to(*q);
        } else {
            p.line_to(*q);
        }
    }
    p
}

// ----- hatching -------------------------------------------------------------

/// The parts of the lines at `angle` (degrees), `gap` apart, inside the
/// rings by the even-odd rule, as segments.
fn hatch_segments(rings: &[Vec<Point>], angle: f64, gap: f64, mid: Point) -> Vec<(Point, Point)> {
    let a = angle.to_radians();
    let (sn, cs) = a.sin_cos();
    // Into the frame where the lines are level, and back.
    let to = |p: Point| {
        let v = p - mid;
        Point::new(v.x * cs + v.y * sn, -v.x * sn + v.y * cs)
    };
    let back = |p: Point| mid + Vec2::new(p.x * cs - p.y * sn, p.x * sn + p.y * cs);
    let turned: Vec<Vec<Point>> = rings
        .iter()
        .map(|r| r.iter().map(|p| to(*p)).collect())
        .collect();
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for r in &turned {
        for p in r {
            lo = lo.min(p.y);
            hi = hi.max(p.y);
        }
    }
    let mut out = Vec::new();
    if !lo.is_finite() || gap <= 0.0 {
        return out;
    }
    // Lines on multiples of the gap through the middle, so turning the
    // angle keeps them steady.
    let first = (lo / gap).floor() as i64;
    let last = (hi / gap).ceil() as i64;
    let mut xs: Vec<f64> = Vec::new();
    for k in first..=last {
        let y = k as f64 * gap;
        xs.clear();
        for r in &turned {
            for i in 0..r.len() {
                let (p, q) = (r[i], r[(i + 1) % r.len()]);
                // Half-open, so a corner on the line counts once.
                if (p.y <= y) != (q.y <= y) {
                    xs.push(p.x + (y - p.y) / (q.y - p.y) * (q.x - p.x));
                }
            }
        }
        xs.sort_by(|a, b| a.total_cmp(b));
        for pair in xs.chunks_exact(2) {
            if pair[1] - pair[0] > 1e-9 {
                out.push((back(Point::new(pair[0], y)), back(Point::new(pair[1], y))));
            }
        }
    }
    out
}

pub(crate) fn hatch(s: &ShapeGenSettings, out: &mut Generated) {
    let rect = Rect::new(0.0, 0.0, s.width, s.height);
    out.frame = Some(rect);
    let rs = rings(s);
    let mid = rect.center();
    match s.hatch_mode {
        HatchMode::Parallel | HatchMode::Cross => {
            let mut angles = vec![s.hatch_angle];
            if s.hatch_mode == HatchMode::Cross {
                angles.push(s.hatch_angle + s.hatch_cross);
            }
            for a in angles {
                for (p, q) in hatch_segments(&rs, a, s.hatch_spacing, mid) {
                    if out.full() {
                        out.cut = true;
                        return;
                    }
                    out.push(line(p, q), false, Paint::Line);
                }
            }
        }
        HatchMode::Tone => {
            let src = Source::new(s);
            let layers = s.hatch_layers.max(1);
            let step = (s.hatch_spacing / 3.0).max(0.02);
            for k in 0..layers {
                // Each layer turned on from the last; darker places take
                // more layers.
                let a = s.hatch_angle + k as f64 * 180.0 / layers as f64;
                let threshold = k as f64 / layers as f64;
                for (p, q) in hatch_segments(&rs, a, s.hatch_spacing, mid) {
                    let len = (q - p).hypot();
                    let n = (len / step).ceil().max(1.0) as usize;
                    let mut run: Option<Point> = None;
                    let mut last = p;
                    for i in 0..=n {
                        let t = i as f64 / n as f64;
                        let x = p.lerp(q, t);
                        let on = src.at(x).is_some_and(|v| v[4] > threshold.max(0.02));
                        match (on, run) {
                            (true, None) => run = Some(x),
                            (false, Some(r)) => {
                                if (last - r).hypot() > 1e-6 {
                                    if out.full() {
                                        out.cut = true;
                                        return;
                                    }
                                    out.push(line(r, last), false, Paint::Line);
                                }
                                run = None;
                            }
                            _ => {}
                        }
                        last = x;
                    }
                    if let Some(r) = run {
                        if (q - r).hypot() > 1e-6 {
                            if out.full() {
                                out.cut = true;
                                return;
                            }
                            out.push(line(r, q), false, Paint::Line);
                        }
                    }
                }
            }
        }
    }
}

// ----- contour lines --------------------------------------------------------

pub(crate) fn contour(s: &ShapeGenSettings, out: &mut Generated) {
    let rect = Rect::new(0.0, 0.0, s.width, s.height);
    out.frame = Some(rect);
    match s.contour_mode {
        ContourMode::Offset => contour_offset(s, rect, out),
        ContourMode::Tone => contour_tone(s, rect, out),
    }
}

/// The outline and rings offset inward from it at the spacing, until
/// nothing is left.
fn contour_offset(s: &ShapeGenSettings, rect: Rect, out: &mut Generated) {
    let base = match &s.boundary {
        Some(b) => b.clone(),
        None => tracedraw_core::geometry::rect_path(rect, 0.0),
    };
    out.push(base.clone(), true, Paint::Line);
    for i in 1..=s.contour_rings {
        if out.full() {
            out.cut = true;
            return;
        }
        let ring = tracedraw_core::shaping::offset(&base, -(i as f64) * s.contour_spacing);
        if ring.elements().is_empty() {
            break;
        }
        out.push(ring, true, Paint::Line);
    }
}

/// Tone lines by marching squares on the picture's darkness: `levels`
/// evenly spaced thresholds, each traced as joined, smoothed curves.
fn contour_tone(s: &ShapeGenSettings, rect: Rect, out: &mut Generated) {
    let src = Source::new(s);
    let (w, h) = src.raster(40_000);
    let (dx, dy) = (rect.width() / w as f64, rect.height() / h as f64);
    let at = |x: usize, y: usize| Point::new((x as f64 + 0.5) * dx, (y as f64 + 0.5) * dy);
    // Outside the outline counts as no ink, so the lines close inside it.
    let field: Vec<f64> = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .map(|(x, y)| src.at(at(x, y)).map_or(0.0, |v| v[4]))
        .collect();
    let f = |x: usize, y: usize| field[y * w + x];
    let n = s.contour_levels.max(1);
    for lv in 0..n {
        let level = (lv + 1) as f64 / (n + 1) as f64;
        // Each crossing is named by its grid edge: (x, y, 0) the edge from
        // (x, y) to (x + 1, y), (x, y, 1) the edge from (x, y) to (x, y + 1).
        type Edge = (usize, usize, u8);
        let mut segs: Vec<(Edge, Edge)> = Vec::new();
        for y in 0..h.saturating_sub(1) {
            for x in 0..w.saturating_sub(1) {
                let (a, b, c, d) = (f(x, y), f(x + 1, y), f(x + 1, y + 1), f(x, y + 1));
                let idx = (a > level) as u8
                    | ((b > level) as u8) << 1
                    | ((c > level) as u8) << 2
                    | ((d > level) as u8) << 3;
                let (bottom, right, top, left) =
                    ((x, y, 0), (x + 1, y, 1), (x, y + 1, 0), (x, y, 1));
                let centre = (a + b + c + d) / 4.0 > level;
                match idx {
                    0 | 15 => {}
                    1 | 14 => segs.push((left, bottom)),
                    2 | 13 => segs.push((bottom, right)),
                    3 | 12 => segs.push((left, right)),
                    4 | 11 => segs.push((right, top)),
                    6 | 9 => segs.push((bottom, top)),
                    7 | 8 => segs.push((left, top)),
                    5 => {
                        if centre {
                            segs.push((left, top));
                            segs.push((bottom, right));
                        } else {
                            segs.push((left, bottom));
                            segs.push((right, top));
                        }
                    }
                    10 => {
                        if centre {
                            segs.push((left, bottom));
                            segs.push((right, top));
                        } else {
                            segs.push((left, top));
                            segs.push((bottom, right));
                        }
                    }
                    _ => {}
                }
            }
        }
        let point = |e: Edge| {
            let (x, y, dir) = e;
            let (p0, v0) = (at(x, y), f(x, y));
            let (p1, v1) = if dir == 0 {
                (at(x + 1, y), f(x + 1, y))
            } else {
                (at(x, y + 1), f(x, y + 1))
            };
            let t = if (v1 - v0).abs() < 1e-12 {
                0.5
            } else {
                ((level - v0) / (v1 - v0)).clamp(0.0, 1.0)
            };
            p0.lerp(p1, t)
        };
        // Join the segments that share an edge into chains.
        let mut by_edge: std::collections::HashMap<Edge, Vec<usize>> =
            std::collections::HashMap::new();
        for (i, (a, b)) in segs.iter().enumerate() {
            by_edge.entry(*a).or_default().push(i);
            by_edge.entry(*b).or_default().push(i);
        }
        let mut used = vec![false; segs.len()];
        for start in 0..segs.len() {
            if used[start] {
                continue;
            }
            used[start] = true;
            let mut chain: std::collections::VecDeque<Edge> =
                std::collections::VecDeque::from([segs[start].0, segs[start].1]);
            // Grow from both ends.
            for forward in [true, false] {
                loop {
                    let end = if forward {
                        *chain.back().expect("chain")
                    } else {
                        *chain.front().expect("chain")
                    };
                    let next = by_edge
                        .get(&end)
                        .and_then(|v| v.iter().copied().find(|i| !used[*i]));
                    let Some(i) = next else {
                        break;
                    };
                    used[i] = true;
                    let (a, b) = segs[i];
                    let other = if a == end { b } else { a };
                    if forward {
                        chain.push_back(other);
                    } else {
                        chain.push_front(other);
                    }
                }
            }
            let closed = chain.len() > 3 && chain.front() == chain.back();
            if closed {
                chain.pop_back();
            }
            if chain.len() < 2 {
                continue;
            }
            let pts: Vec<Point> = chain.iter().map(|e| point(*e)).collect();
            if out.full() {
                out.cut = true;
                return;
            }
            out.push(smooth_through(&pts, closed), closed, Paint::Line);
        }
    }
}

// ----- maze -----------------------------------------------------------------

pub(crate) fn maze(s: &ShapeGenSettings, out: &mut Generated) {
    let rect = Rect::new(0.0, 0.0, s.width, s.height);
    out.frame = Some(rect);
    let c = s.maze_cell;
    let (nx, ny) = (
        (s.width / c).floor().max(1.0) as usize,
        (s.height / c).floor().max(1.0) as usize,
    );
    let o = Point::new(
        (s.width - nx as f64 * c) / 2.0,
        (s.height - ny as f64 * c) / 2.0,
    );
    let centre =
        |i: usize, j: usize| Point::new(o.x + (i as f64 + 0.5) * c, o.y + (j as f64 + 0.5) * c);
    let b = s.boundary.as_ref();
    let on: Vec<bool> = (0..nx * ny)
        .map(|k| inside(b, centre(k % nx, k / nx)))
        .collect();
    let id = |i: usize, j: usize| j * nx + i;
    // Passages: right[k] joins cell k to its right neighbour, up[k] to the
    // one above.
    let mut right = vec![false; nx * ny];
    let mut up = vec![false; nx * ny];
    let mut seen = vec![false; nx * ny];
    let mut rng = Rng::new(s.seed, 0x3a2e);
    // Passage lines for the Paths style: a new one at each branch.
    let mut paths: Vec<Vec<Point>> = Vec::new();
    for start in 0..nx * ny {
        if !on[start] || seen[start] {
            continue;
        }
        seen[start] = true;
        let mut stack = vec![start];
        let mut cur_path: Vec<Point> = vec![centre(start % nx, start / nx)];
        while let Some(&k) = stack.last() {
            let (i, j) = (k % nx, k / nx);
            let mut next: [(usize, u8); 4] = [(0, 0); 4];
            let mut n = 0;
            if i + 1 < nx && on[id(i + 1, j)] && !seen[id(i + 1, j)] {
                next[n] = (id(i + 1, j), 0);
                n += 1;
            }
            if i > 0 && on[id(i - 1, j)] && !seen[id(i - 1, j)] {
                next[n] = (id(i - 1, j), 1);
                n += 1;
            }
            if j + 1 < ny && on[id(i, j + 1)] && !seen[id(i, j + 1)] {
                next[n] = (id(i, j + 1), 2);
                n += 1;
            }
            if j > 0 && on[id(i, j - 1)] && !seen[id(i, j - 1)] {
                next[n] = (id(i, j - 1), 3);
                n += 1;
            }
            if n == 0 {
                stack.pop();
                if cur_path.len() >= 2 {
                    paths.push(std::mem::take(&mut cur_path));
                }
                cur_path.clear();
                if let Some(&back) = stack.last() {
                    cur_path.push(centre(back % nx, back / nx));
                }
                continue;
            }
            let (m, dir) = next[rng.below(n as u32) as usize];
            match dir {
                0 => right[k] = true,
                1 => right[m] = true,
                2 => up[k] = true,
                _ => up[m] = true,
            }
            seen[m] = true;
            stack.push(m);
            cur_path.push(centre(m % nx, m / nx));
        }
        if cur_path.len() >= 2 {
            paths.push(cur_path);
        }
    }
    if s.maze_style == MazeStyle::Paths {
        for p in paths {
            if out.full() {
                out.cut = true;
                return;
            }
            out.push(polyline(&straighten(&p)), false, Paint::Line);
        }
        return;
    }
    // Entrance in the lowest row that has cells, exit in the highest.
    let first = on.iter().position(|v| *v);
    let last = on.iter().rposition(|v| *v);
    let open = |k: usize, bottom: bool| {
        s.maze_openings
            && if bottom {
                Some(k) == first
            } else {
                Some(k) == last
            }
    };
    // Walls as unit edges on the grid lines, merged into runs.
    let mut level: Vec<Vec<bool>> = vec![vec![false; nx]; ny + 1];
    let mut upright: Vec<Vec<bool>> = vec![vec![false; ny]; nx + 1];
    for j in 0..ny {
        for i in 0..nx {
            let k = id(i, j);
            if !on[k] {
                continue;
            }
            // Below.
            let below = j > 0 && on[id(i, j - 1)];
            if !(below && up[id(i, j - 1)]) && !(!below && open(k, true)) {
                level[j][i] = true;
            }
            // Above.
            let above = j + 1 < ny && on[id(i, j + 1)];
            if !above && !open(k, false) {
                level[j + 1][i] = true;
            } else if above && !up[k] {
                level[j + 1][i] = true;
            }
            // Left.
            let left = i > 0 && on[id(i - 1, j)];
            if !(left && right[id(i - 1, j)]) {
                upright[i][j] = true;
            }
            // Right.
            let rt = i + 1 < nx && on[id(i + 1, j)];
            if !rt || !right[k] {
                upright[i + 1][j] = true;
            }
        }
    }
    for (j, row) in level.iter().enumerate() {
        let y = o.y + j as f64 * c;
        let mut i = 0;
        while i < nx {
            if !row[i] {
                i += 1;
                continue;
            }
            let start = i;
            while i < nx && row[i] {
                i += 1;
            }
            if out.full() {
                out.cut = true;
                return;
            }
            out.push(
                line(
                    Point::new(o.x + start as f64 * c, y),
                    Point::new(o.x + i as f64 * c, y),
                ),
                false,
                Paint::Line,
            );
        }
    }
    for (i, col) in upright.iter().enumerate() {
        let x = o.x + i as f64 * c;
        let mut j = 0;
        while j < ny {
            if !col[j] {
                j += 1;
                continue;
            }
            let start = j;
            while j < ny && col[j] {
                j += 1;
            }
            if out.full() {
                out.cut = true;
                return;
            }
            out.push(
                line(
                    Point::new(x, o.y + start as f64 * c),
                    Point::new(x, o.y + j as f64 * c),
                ),
                false,
                Paint::Line,
            );
        }
    }
}

/// The points where a polyline turns (straight runs kept as one line).
fn straighten(pts: &[Point]) -> Vec<Point> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let mut out = vec![pts[0]];
    for k in 1..pts.len() - 1 {
        let (a, b, c) = (pts[k - 1], pts[k], pts[k + 1]);
        let cross = (b - a).cross(c - b);
        if cross.abs() > 1e-9 {
            out.push(b);
        }
    }
    out.push(pts[pts.len() - 1]);
    out
}

// ----- space-filling curves -------------------------------------------------

/// The L-system of a curve: axiom and rules (90-degree turns).
fn curve_rules(c: SpaceCurve) -> (&'static str, &'static [(char, &'static str)]) {
    match c {
        SpaceCurve::Hilbert => ("A", &[('A', "+BF-AFA-FB+"), ('B', "-AF+BFB+FA-")]),
        SpaceCurve::Moore => ("AFA+F+AFA", &[('A', "-BF+AFA+FB-"), ('B', "+AF-BFB-FA+")]),
        SpaceCurve::Peano => (
            "X",
            &[
                ('X', "XFYFX+F+YFXFY-F-XFYFX"),
                ('Y', "YFXFY-F-XFYFX+F+YFXFY"),
            ],
        ),
    }
}

/// The curve's corners on a unit grid.
fn curve_points(c: SpaceCurve, order: u32) -> Vec<Point> {
    let (axiom, rules) = curve_rules(c);
    let mut text: Vec<u8> = axiom.bytes().collect();
    // Moore's axiom already draws the first order.
    let steps = match c {
        SpaceCurve::Moore => order.max(1) - 1,
        _ => order.max(1),
    };
    for _ in 0..steps {
        let mut next = Vec::with_capacity(text.len() * 4);
        for ch in &text {
            match rules.iter().find(|(r, _)| *r as u8 == *ch) {
                Some((_, rep)) => next.extend_from_slice(rep.as_bytes()),
                None => next.push(*ch),
            }
        }
        text = next;
    }
    // Turtle on integers: start facing right.
    let (mut x, mut y, mut dir) = (0i64, 0i64, 0u8);
    let mut pts = vec![Point::new(0.0, 0.0)];
    for ch in text {
        match ch {
            b'F' => {
                match dir {
                    0 => x += 1,
                    1 => y += 1,
                    2 => x -= 1,
                    _ => y -= 1,
                }
                pts.push(Point::new(x as f64, y as f64));
            }
            b'+' => dir = (dir + 1) % 4,
            b'-' => dir = (dir + 3) % 4,
            _ => {}
        }
    }
    pts
}

/// A polyline with its corners rounded: straight to each segment's middle,
/// a quadratic round each corner.
fn rounded_polyline(pts: &[Point], closed: bool) -> BezPath {
    let n = pts.len();
    if n < 3 {
        return polyline(pts);
    }
    let mut p = BezPath::new();
    if closed {
        p.move_to(pts[n - 1].midpoint(pts[0]));
        for k in 0..n {
            p.quad_to(pts[k], pts[k].midpoint(pts[(k + 1) % n]));
        }
        p.close_path();
    } else {
        p.move_to(pts[0]);
        p.line_to(pts[0].midpoint(pts[1]));
        for k in 1..n - 1 {
            p.quad_to(pts[k], pts[k].midpoint(pts[k + 1]));
        }
        p.line_to(pts[n - 1]);
    }
    p
}

pub(crate) fn space_curve(s: &ShapeGenSettings, out: &mut Generated) {
    let rect = Rect::new(0.0, 0.0, s.width, s.height);
    out.frame = Some(rect);
    let order = s.curve_order.clamp(1, s.curve_type.max_order());
    let raw = curve_points(s.curve_type, order);
    let (mut lo, mut hi) = (
        Point::new(f64::INFINITY, f64::INFINITY),
        Point::new(f64::NEG_INFINITY, f64::NEG_INFINITY),
    );
    for p in &raw {
        lo = Point::new(lo.x.min(p.x), lo.y.min(p.y));
        hi = Point::new(hi.x.max(p.x), hi.y.max(p.y));
    }
    // One grid step per corner, half a step of margin all round.
    let (cols, rows) = (hi.x - lo.x + 1.0, hi.y - lo.y + 1.0);
    let (sx, sy) = (s.width / cols, s.height / rows);
    let pts: Vec<Point> = raw
        .iter()
        .map(|p| Point::new((p.x - lo.x + 0.5) * sx, (p.y - lo.y + 0.5) * sy))
        .collect();
    let b = s.boundary.as_ref();
    let closed_curve = s.curve_type == SpaceCurve::Moore;
    if b.is_none() {
        let path = if s.curve_round {
            rounded_polyline(&pts, closed_curve)
        } else {
            let mut p = polyline(&pts);
            if closed_curve {
                p.close_path();
            }
            p
        };
        out.push(path, closed_curve, Paint::Line);
        return;
    }
    // Runs of corners inside the outline.
    let mut run: Vec<Point> = Vec::new();
    let flush = |run: &mut Vec<Point>, out: &mut Generated| {
        if run.len() >= 2 {
            let path = if s.curve_round {
                rounded_polyline(run, false)
            } else {
                polyline(&straighten(run))
            };
            out.push(path, false, Paint::Line);
        }
        run.clear();
    };
    for p in pts {
        if out.full() {
            out.cut = true;
            return;
        }
        if inside(b, p) {
            run.push(p);
        } else {
            flush(&mut run, out);
        }
    }
    flush(&mut run, out);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape_gen::{generate, GenKind};
    use tracedraw_core::geometry::Shape as _;

    fn ring_with_hole() -> BezPath {
        let mut b = tracedraw_core::geometry::rect_path(Rect::new(0.0, 0.0, 100.0, 100.0), 0.0);
        b.extend(tracedraw_core::geometry::rect_path(
            Rect::new(30.0, 30.0, 70.0, 70.0),
            0.0,
        ));
        b
    }

    fn total_length(g: &Generated) -> f64 {
        g.items.iter().map(|i| i.path.perimeter(0.01)).sum()
    }

    #[test]
    fn hatch_lines_fill_the_outline_and_skip_its_hole() {
        let s = ShapeGenSettings {
            kind: GenKind::Hatch,
            hatch_spacing: 2.0,
            hatch_angle: 0.0,
            boundary: Some(ring_with_hole()),
            ..ShapeGenSettings::default()
        };
        let g = generate(&s);
        // Area / spacing: (10000 - 1600) / 2.
        let len = total_length(&g);
        assert!((len - 4200.0).abs() < 120.0, "{len}");
        for it in &g.items {
            let m = it.path.bounding_box().center();
            assert!(
                !(m.x > 31.0 && m.x < 69.0 && m.y > 31.0 && m.y < 69.0),
                "{m:?}"
            );
        }
        // Crossed: about twice as much.
        let g2 = generate(&ShapeGenSettings {
            hatch_mode: HatchMode::Cross,
            ..s.clone()
        });
        assert!(total_length(&g2) > len * 1.9);
    }

    #[test]
    fn tone_hatching_puts_more_lines_where_it_is_dark() {
        let s = ShapeGenSettings {
            kind: GenKind::Hatch,
            hatch_mode: HatchMode::Tone,
            hatch_spacing: 2.0,
            ..ShapeGenSettings::default()
        };
        let g = generate(&s);
        assert!(!g.items.is_empty());
        // The stand-in ramp is dark in the middle.
        let near = |c: Point, r: f64| {
            g.items
                .iter()
                .filter(|i| (i.path.bounding_box().center() - c).hypot() < r)
                .map(|i| i.path.perimeter(0.01))
                .sum::<f64>()
        };
        let centre = near(Point::new(50.0, 50.0), 15.0);
        let corner = near(Point::new(90.0, 90.0), 15.0);
        assert!(centre > corner * 2.0, "{centre} {corner}");
    }

    #[test]
    fn offset_contours_shrink_until_nothing_is_left() {
        let s = ShapeGenSettings {
            kind: GenKind::Contour,
            contour_spacing: 5.0,
            contour_rings: 500,
            ..ShapeGenSettings::default()
        };
        let g = generate(&s);
        // A 100 mm square: the outline and rings every 5 mm to the middle.
        assert!(
            g.items.len() >= 10 && g.items.len() <= 11,
            "{}",
            g.items.len()
        );
        let a0 = g.items[0].path.area().abs();
        let a1 = g.items[1].path.area().abs();
        assert!((a1 - 90.0 * 90.0).abs() < 1.0, "{a0} {a1}");
    }

    #[test]
    fn tone_contours_close_round_the_dark_middle() {
        let s = ShapeGenSettings {
            kind: GenKind::Contour,
            contour_mode: ContourMode::Tone,
            contour_levels: 4,
            ..ShapeGenSettings::default()
        };
        let g = generate(&s);
        assert!(g.items.len() >= 4, "{}", g.items.len());
        // Rings round the middle.
        assert!(g.items.iter().filter(|i| i.closed).count() >= 3);
        for it in g.items.iter().filter(|i| i.closed) {
            assert!(it.path.winding(Point::new(50.0, 50.0)) != 0);
        }
    }

    #[test]
    fn a_maze_reaches_every_cell_once() {
        let s = ShapeGenSettings {
            kind: GenKind::Maze,
            maze_cell: 10.0,
            maze_style: MazeStyle::Paths,
            ..ShapeGenSettings::default()
        };
        let g = generate(&s);
        // A spanning tree of 100 cells has 99 passages of 10 mm.
        let len = total_length(&g);
        assert!((len - 990.0).abs() < 1e-6, "{len}");
        // Walls: the full grid (2 x 11 x 100 mm) less the 99 passages and
        // the two openings.
        let w = generate(&ShapeGenSettings {
            maze_style: MazeStyle::Walls,
            ..s.clone()
        });
        let wl = total_length(&w);
        assert!((wl - (2200.0 - 990.0 - 20.0)).abs() < 1e-6, "{wl}");
        // Another seed, another maze.
        let other = generate(&ShapeGenSettings {
            seed: 7,
            ..s.clone()
        });
        assert_ne!(
            g.items
                .iter()
                .map(|i| i.path.elements().len())
                .collect::<Vec<_>>(),
            other
                .items
                .iter()
                .map(|i| i.path.elements().len())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_maze_stays_out_of_the_hole() {
        let s = ShapeGenSettings {
            kind: GenKind::Maze,
            maze_cell: 5.0,
            boundary: Some(ring_with_hole()),
            ..ShapeGenSettings::default()
        };
        let g = generate(&s);
        assert!(!g.items.is_empty());
        for it in &g.items {
            let m = it.path.bounding_box().center();
            assert!(
                !(m.x > 31.0 && m.x < 69.0 && m.y > 31.0 && m.y < 69.0),
                "{m:?}"
            );
        }
    }

    #[test]
    fn space_filling_curves_visit_every_cell() {
        assert_eq!(curve_points(SpaceCurve::Hilbert, 3).len(), 64);
        assert_eq!(curve_points(SpaceCurve::Moore, 3).len(), 64);
        assert_eq!(curve_points(SpaceCurve::Peano, 2).len(), 81);
        for c in SpaceCurve::ALL {
            let pts = curve_points(c, 3);
            let mut seen = std::collections::HashSet::new();
            for p in &pts {
                assert!(
                    seen.insert((p.x as i64, p.y as i64)),
                    "{c:?} twice at {p:?}"
                );
            }
            for w in pts.windows(2) {
                assert!(((w[1] - w[0]).hypot() - 1.0).abs() < 1e-9, "{c:?}");
            }
        }
        // Moore closes: its ends are neighbours.
        let m = curve_points(SpaceCurve::Moore, 4);
        assert!(((m[0] - m[m.len() - 1]).hypot() - 1.0).abs() < 1e-9);
        let g = generate(&ShapeGenSettings {
            kind: GenKind::SpaceCurve,
            curve_order: 4,
            boundary: Some(ring_with_hole()),
            ..ShapeGenSettings::default()
        });
        assert!(g.items.len() > 1);
        let b = g.bounds().expect("bounds");
        assert!(b.x0 >= 0.0 && b.x1 <= 100.0 && b.y0 >= 0.0 && b.y1 <= 100.0);
    }
}
