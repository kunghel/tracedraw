//! Small thin letters of an enlarged picture drawn again by their
//! strokes. See docs/behavior/bitmap-tracing.md, "Small letters by
//! their strokes".
//!
//! A letter a few pixels tall in the picture before it was enlarged has
//! strokes about two pixels wide there. The enlarging gives it an
//! outline, but not a true one: its stems lean, its bowls bulge, its
//! strokes thicken and thin. Such a letter was drawn with a pen of one
//! width, and that says more about it than its outline does: the line
//! down the middle of each stroke is found (the letter thinned to one
//! pixel), fitted with straight lines, arcs and corners, and drawn
//! again with the one width the letter has, its ends cut square. The
//! letter drawn again is compared with the letter of the picture, and
//! where the two do not agree the letter is left as the picture has it.

use tracedraw_core::geometry::{BezPath, ParamCurve, Point, Vec2};

use crate::trace_fit::Faithful;

/// The longest side of a letter drawn again, pixels of the picture
/// before it was enlarged.
const BOX_MOST: f64 = 20.0;
/// The widths of stroke drawn again, the same pixels.
const WIDE_MOST: f64 = 2.7;
const WIDE_LEAST: f64 = 0.8;
/// A letter is of one width when at least [`EVEN_SHARE`] of its middle
/// line lies within [`EVEN`] of the middle width (and a point of the
/// grid).
const EVEN_SHARE: f64 = 0.8;
const EVEN: f64 = 0.3;
/// A branch of the middle line shorter than this many widths is the
/// thinning's (it forks into the corners of a square end).
const SPUR: f64 = 0.9;
/// Two branches at a meeting go straight through it when they turn
/// less than this, radians.
const THROUGH: f64 = 0.5;
/// How far the middle line may stray from the thinned one, and how far
/// the letter drawn again from the picture's, pixels of the picture
/// before it was enlarged; and the share of the letter that may lie
/// further.
const FIT_TOLERANCE: f64 = 0.2;
/// The longest piece between two lines taken for a cut corner, widths.
const CHAMFER: f64 = 0.9;
/// A stroke has a width of its own when it differs from the letter's by
/// more than [`OWN_LEAST`] of it, and by [`OWN_MOST`] at most.
const OWN_LEAST: f64 = 0.12;
const OWN_MOST: f64 = 0.3;
/// How much of the tip of a thinned line is left out, widths.
const TIP: f64 = 0.6;
const STRAY_MOST: f64 = 0.6;
const STRAY_SHARE: f64 = 0.015;
/// The same distance is no more than this share of the stroke's width.
const STRAY_WIDE: f64 = 0.3;
/// A turn of the middle line sharper than this is a corner, drawn with
/// a point, up to [`MITER_MOST`].
const MITER_LEAST: f64 = 0.45;
const MITER_MOST: f64 = 2.4;
/// A straight stretch of a middle line: at least [`RUN_LEAST`] widths
/// long (and [`RUN_POINTS`] points of the grid), no point of it further
/// than [`RUN_OFF`] points from its line, its two halves within
/// [`RUN_TURN`] radians of each other. One within [`RUN_AXIS`] radians
/// of level or upright is level or upright.
const RUN_LEAST: f64 = 1.5;
const RUN_POINTS: f64 = 12.0;
/// How much of that length is the run's own, what the line goes on
/// past a free end aside.
const RUN_OWN: f64 = 0.7;
const RUN_OFF: f64 = 0.6;
const RUN_TURN: f64 = 0.1;
const RUN_AXIS: f64 = 0.07;
/// A free end is carried on along the bend its stroke has there when
/// the bend is at least this much, radians over one width, and
/// [`BEND_MOST`] at most.
const BEND_LEAST: f64 = 0.08;
const BEND_MOST: f64 = 0.8;
/// Letters side by side are a word: of one colour, their heights
/// within [`WORD_TALL`] of each other, on one line (their tops and
/// bottoms within the same share of the height) and no further apart
/// than [`WORD_GAP`] heights. A word of [`WORD_LEAST`] letters or more
/// has one width of stroke, the middle one of its letters, taken by a
/// letter whose own is within [`WORD_WIDE`] of it; and a level line or
/// a square end within [`WORD_LINE`] pixels of the picture before it
/// was enlarged of the word's top or bottom lies on it.
const WORD_TALL: f64 = 0.15;
const WORD_GAP: f64 = 3.0;
const WORD_LEAST: usize = 3;
const WORD_WIDE: f64 = 0.25;
const WORD_LINE: f64 = 0.75;
/// Two colours this near are shades of one, a letter in both one letter.
const SHADE_NEAR: i64 = 48;

fn unit(v: Vec2) -> Vec2 {
    let l = v.hypot();
    if l > 1e-12 && l.is_finite() {
        v / l
    } else {
        Vec2::ZERO
    }
}

/// A stretch of the middle line: its points, the meeting each end is at
/// (none for a free end) and whether it is a loop.
struct Chain {
    pts: Vec<Point>,
    ends: [Option<usize>; 2],
    closed: bool,
    alive: bool,
}

impl Chain {
    fn length(&self) -> f64 {
        self.pts.windows(2).map(|p| (p[1] - p[0]).hypot()).sum()
    }
}

/// The index of the point of `pts` at least `far` along it from its
/// first point (its last when it is shorter).
fn index_at(pts: &[Point], far: f64) -> usize {
    let mut d = 0.0;
    for i in 1..pts.len() {
        d += (pts[i] - pts[i - 1]).hypot();
        if d >= far {
            return i;
        }
    }
    pts.len().saturating_sub(1)
}

/// `pts` again with its points `step` apart.
fn resample(pts: &[Point], step: f64, closed: bool) -> Vec<Point> {
    let mut src = pts.to_vec();
    if closed {
        if let Some(first) = src.first().copied() {
            src.push(first);
        }
    }
    let total: f64 = src.windows(2).map(|p| (p[1] - p[0]).hypot()).sum();
    if src.len() < 2 || total < step {
        return pts.to_vec();
    }
    let n = (total / step).round().max(1.0) as usize;
    let each = total / n as f64;
    let mut out = Vec::with_capacity(n + 1);
    out.push(src[0]);
    let (mut seg, mut before) = (0usize, 0.0);
    for i in 1..n {
        let want = each * i as f64;
        loop {
            let len = (src[seg + 1] - src[seg]).hypot();
            if before + len >= want || seg + 2 >= src.len() {
                let t = if len > 1e-12 {
                    ((want - before) / len).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                out.push(src[seg].lerp(src[seg + 1], t));
                break;
            }
            before += len;
            seg += 1;
        }
    }
    if !closed {
        out.push(src[src.len() - 1]);
    }
    out
}

/// How far each set point of `mask` (`w` x `h`) lies from the nearest
/// unset one (a point outside the box is unset).
fn distances(mask: &[bool], w: usize, h: usize) -> Vec<f32> {
    const DIAGONAL: f32 = std::f32::consts::SQRT_2;
    let mut d: Vec<f32> = mask
        .iter()
        .map(|m| if *m { f32::MAX } else { 0.0 })
        .collect();
    let at = |d: &[f32], x: i64, y: i64| -> f32 {
        if x < 0 || y < 0 || x >= w as i64 || y >= h as i64 {
            0.0
        } else {
            d[y as usize * w + x as usize]
        }
    };
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            let i = y as usize * w + x as usize;
            if d[i] == 0.0 {
                continue;
            }
            let best = (at(&d, x - 1, y) + 1.0)
                .min(at(&d, x, y - 1) + 1.0)
                .min(at(&d, x - 1, y - 1) + DIAGONAL)
                .min(at(&d, x + 1, y - 1) + DIAGONAL);
            d[i] = d[i].min(best);
        }
    }
    for y in (0..h as i64).rev() {
        for x in (0..w as i64).rev() {
            let i = y as usize * w + x as usize;
            if d[i] == 0.0 {
                continue;
            }
            let best = (at(&d, x + 1, y) + 1.0)
                .min(at(&d, x, y + 1) + 1.0)
                .min(at(&d, x + 1, y + 1) + DIAGONAL)
                .min(at(&d, x - 1, y + 1) + DIAGONAL);
            d[i] = d[i].min(best);
        }
    }
    d
}

/// How many 4-connected patches of unset points `mask` has.
fn grounds(mask: &[bool], w: usize, h: usize) -> usize {
    let mut seen = vec![false; w * h];
    let (mut count, mut stack) = (0usize, Vec::new());
    for start in 0..w * h {
        if mask[start] || seen[start] {
            continue;
        }
        count += 1;
        seen[start] = true;
        stack.push(start);
        while let Some(i) = stack.pop() {
            let (x, y) = (i % w, i / w);
            let mut visit = |j: usize| {
                if !mask[j] && !seen[j] {
                    seen[j] = true;
                    stack.push(j);
                }
            };
            if x > 0 {
                visit(i - 1);
            }
            if x + 1 < w {
                visit(i + 1);
            }
            if y > 0 {
                visit(i - w);
            }
            if y + 1 < h {
                visit(i + w);
            }
        }
    }
    count
}

/// The eight points round `i`, clockwise from the one above; none
/// outside the box.
fn ring(i: usize, w: usize, h: usize) -> [Option<usize>; 8] {
    const ROUND: [(i64, i64); 8] = [
        (0, -1),
        (1, -1),
        (1, 0),
        (1, 1),
        (0, 1),
        (-1, 1),
        (-1, 0),
        (-1, -1),
    ];
    let (x, y) = ((i % w) as i64, (i / w) as i64);
    ROUND.map(|(dx, dy)| {
        let (u, v) = (x + dx, y + dy);
        (u >= 0 && v >= 0 && u < w as i64 && v < h as i64).then(|| v as usize * w + u as usize)
    })
}

/// The middle line `thin` (`w` x `h`, one point wide) as chains between
/// its meetings and its free ends, and how many meetings it has.
fn chains_of(thin: &[bool], w: usize, h: usize) -> (Vec<Chain>, Vec<Point>) {
    let centre = |i: usize| Point::new((i % w) as f64 + 0.5, (i / w) as f64 + 0.5);
    let set = |j: Option<usize>| j.is_some_and(|j| thin[j]);
    // 1 a point of a chain, 2 a free end, 3 a meeting.
    let mut kind = vec![0u8; w * h];
    for i in 0..w * h {
        if !thin[i] {
            continue;
        }
        let r = ring(i, w, h);
        let turns = (0..8)
            .filter(|k| !set(r[*k]) && set(r[(k + 1) % 8]))
            .count();
        let around = r.iter().filter(|j| set(**j)).count();
        kind[i] = if around == 0 {
            0
        } else if turns <= 1 {
            2
        } else if turns >= 3 {
            3
        } else {
            1
        };
    }
    // The points of a meeting that touch are one meeting.
    let mut node = vec![usize::MAX; w * h];
    let mut nodes: Vec<Point> = Vec::new();
    let mut stack = Vec::new();
    for start in 0..w * h {
        if kind[start] != 3 || node[start] != usize::MAX {
            continue;
        }
        let id = nodes.len();
        let (mut sum, mut count) = (Vec2::ZERO, 0.0);
        node[start] = id;
        stack.push(start);
        while let Some(i) = stack.pop() {
            sum += centre(i).to_vec2();
            count += 1.0;
            for j in ring(i, w, h).into_iter().flatten() {
                if kind[j] == 3 && node[j] == usize::MAX {
                    node[j] = id;
                    stack.push(j);
                }
            }
        }
        nodes.push((sum / count).to_point());
    }
    let mut seen = vec![false; w * h];
    let mut chains: Vec<Chain> = Vec::new();
    // Walk from `first`, a point of a chain beside where it starts.
    let walk = |first: usize, from: Option<usize>, start: Point, seen: &mut Vec<bool>| {
        let mut pts = vec![start, centre(first)];
        seen[first] = true;
        let (mut at, mut cells) = (first, 1usize);
        let beside = |i: usize, n: usize| {
            ring(i, w, h)
                .into_iter()
                .flatten()
                .any(|j| kind[j] == 3 && node[j] == n)
        };
        let end = loop {
            let r = ring(at, w, h);
            // Where it ends: a meeting or a free end beside it.
            let mut found = None;
            for j in r.into_iter().flatten() {
                if kind[j] == 3 && (Some(node[j]) != from || cells > 2) {
                    found = Some((nodes[node[j]], Some(node[j])));
                    break;
                }
            }
            if found.is_none() {
                for j in r.into_iter().flatten() {
                    if kind[j] == 2 && !seen[j] {
                        seen[j] = true;
                        found = Some((centre(j), None));
                        break;
                    }
                }
            }
            if let Some((p, n)) = found {
                pts.push(p);
                break n;
            }
            // Straight neighbours before diagonal ones.
            let mut next = None;
            for k in [0usize, 2, 4, 6, 1, 3, 5, 7] {
                let Some(j) = r[k] else { continue };
                if kind[j] != 1 || seen[j] {
                    continue;
                }
                if cells <= 2 && from.is_some_and(|n| beside(j, n)) {
                    continue;
                }
                next = Some(j);
                break;
            }
            let Some(j) = next else { break None };
            seen[j] = true;
            pts.push(centre(j));
            at = j;
            cells += 1;
        };
        Chain {
            pts,
            ends: [from, end],
            closed: false,
            alive: true,
        }
    };
    for i in 0..w * h {
        if kind[i] != 3 {
            continue;
        }
        for j in ring(i, w, h).into_iter().flatten() {
            if kind[j] == 1 && !seen[j] {
                chains.push(walk(j, Some(node[i]), nodes[node[i]], &mut seen));
            }
        }
    }
    for i in 0..w * h {
        if kind[i] != 2 || seen[i] {
            continue;
        }
        seen[i] = true;
        let first = ring(i, w, h)
            .into_iter()
            .flatten()
            .find(|j| kind[*j] == 1 && !seen[*j]);
        if let Some(j) = first {
            chains.push(walk(j, None, centre(i), &mut seen));
        }
    }
    // What is left has no meeting and no end: loops.
    for i in 0..w * h {
        if kind[i] != 1 || seen[i] {
            continue;
        }
        let mut c = walk(i, None, centre(i), &mut seen);
        c.pts.remove(0);
        let n = c.pts.len();
        if n >= 8 && (c.pts[0] - c.pts[n - 1]).hypot() < 1.5 {
            c.closed = true;
        }
        chains.push(c);
    }
    (chains, nodes)
}

/// Join the chains at the meetings: the branches the thinning made are
/// dropped, two chains going straight through a meeting are one, and a
/// branch ends on the chain it meets. Which meetings a chain goes
/// through.
fn join(chains: &mut [Chain], nodes: &[Point], wide: f64) {
    for c in chains.iter_mut() {
        let len = c.length();
        let spur = matches!(c.ends, [Some(_), None] | [None, Some(_)]) && len < SPUR * wide;
        let knot = c.ends[0].is_some() && c.ends[0] == c.ends[1] && len < 2.0 * SPUR * wide;
        if spur || knot {
            c.alive = false;
        }
    }
    let mut through = vec![false; nodes.len()];
    for (n, at) in nodes.iter().enumerate() {
        loop {
            let mut halves: Vec<(usize, usize)> = Vec::new();
            for (e, c) in chains.iter().enumerate() {
                if !c.alive || c.closed {
                    continue;
                }
                for side in 0..2 {
                    if c.ends[side] == Some(n) {
                        halves.push((e, side));
                    }
                }
            }
            if halves.len() < 2 {
                if let [(e, side)] = halves[..] {
                    if !through[n] {
                        chains[e].ends[side] = None;
                    }
                }
                break;
            }
            // The way each leaves the meeting.
            let way = |e: usize, side: usize| -> Vec2 {
                let c = &chains[e];
                let mut pts = c.pts.clone();
                if side == 1 {
                    pts.reverse();
                }
                unit(pts[index_at(&pts, wide)] - *at)
            };
            let pair = if halves.len() == 2 {
                Some((0, 1, false))
            } else {
                let mut best: Option<(usize, usize, f64)> = None;
                for i in 0..halves.len() {
                    for j in i + 1..halves.len() {
                        let (a, b) = (way(halves[i].0, halves[i].1), way(halves[j].0, halves[j].1));
                        let turn = (-a).cross(b).atan2((-a).dot(b)).abs();
                        if turn < THROUGH && best.is_none_or(|x| turn < x.2) {
                            best = Some((i, j, turn));
                        }
                    }
                }
                best.map(|(i, j, _)| (i, j, true))
            };
            let Some((i, j, flat)) = pair else { break };
            let ((e1, s1), (e2, s2)) = (halves[i], halves[j]);
            if e1 == e2 {
                let c = &mut chains[e1];
                c.pts.pop();
                c.closed = true;
                c.ends = [None, None];
                break;
            }
            let mut a = chains[e1].pts.clone();
            if s1 == 0 {
                a.reverse();
            }
            let mut b = chains[e2].pts.clone();
            if s2 == 1 {
                b.reverse();
            }
            let ends = [chains[e1].ends[1 - s1], chains[e2].ends[1 - s2]];
            let middle = a.len() - 1;
            a.extend_from_slice(&b[1..]);
            if flat {
                // The thinning pulls the line towards the branch: it is
                // straight across the meeting.
                through[n] = true;
                let mut back: Vec<Point> = a[..=middle].to_vec();
                back.reverse();
                let lo = middle - index_at(&back, wide);
                let hi = middle + index_at(&a[middle..], wide);
                if hi > lo + 1 {
                    let (p, q) = (a[lo], a[hi]);
                    for k in lo + 1..hi {
                        a[k] = p.lerp(q, (k - lo) as f64 / (hi - lo) as f64);
                    }
                }
            }
            chains[e1].pts = a;
            chains[e1].ends = ends;
            chains[e2].alive = false;
            if !flat {
                break;
            }
        }
    }
    // A branch ends straight on the chain it meets.
    for c in chains.iter_mut() {
        if !c.alive || c.closed {
            continue;
        }
        // A bar between two such chains (the bar of an H) is pulled
        // at both ends: where its middle half is straight, it is that
        // line from one to the other.
        if let [Some(n0), Some(n1)] = c.ends {
            let len = c.length();
            if through[n0] && through[n1] && len > wide {
                let (i, m, j) = (
                    index_at(&c.pts, 0.25 * len),
                    index_at(&c.pts, 0.5 * len),
                    index_at(&c.pts, 0.75 * len),
                );
                let d = unit(c.pts[j] - c.pts[i]);
                let straight = c.pts[i..=j]
                    .iter()
                    .all(|p| (*p - c.pts[i]).cross(d).abs() <= 0.25 * wide);
                if d != Vec2::ZERO && straight {
                    let mid = c.pts[m];
                    let (t0, t1) = ((nodes[n0] - mid).dot(d), (nodes[n1] - mid).dot(d));
                    if t0 < 0.0 && t1 > 0.0 {
                        c.pts = vec![mid + d * t0, mid, mid + d * t1];
                        continue;
                    }
                }
            }
        }
        for side in 0..2 {
            let Some(n) = c.ends[side] else { continue };
            if !through[n] {
                continue;
            }
            if side == 0 {
                c.pts.reverse();
            }
            let len = c.length();
            if len > 0.75 * wide + 2.0 {
                let mut back = c.pts.clone();
                back.reverse();
                let near = index_at(&back, 0.75 * wide);
                let far = index_at(&back, (1.75 * wide).min(len));
                let d = unit(back[near] - back[far]);
                let t = (nodes[n] - back[near]).dot(d);
                if d != Vec2::ZERO && t > 0.0 {
                    let keep = c.pts.len() - near;
                    c.pts.truncate(keep);
                    c.pts.push(back[near] + d * t);
                }
            }
            if side == 0 {
                c.pts.reverse();
            }
        }
    }
}

/// How far the letter goes on past each free end of a chain: the
/// thinning stops short of the end by about half the stroke.
fn past_ends(c: &Chain, inside: &[f32], w: usize, h: usize) -> [f64; 2] {
    let at = |p: Point| -> f64 {
        if p.x >= 0.0 && p.y >= 0.0 && (p.x as usize) < w && (p.y as usize) < h {
            (inside[p.y as usize * w + p.x as usize] as f64 - 0.5).max(0.0)
        } else {
            0.0
        }
    };
    let n = c.pts.len();
    [
        if c.ends[0].is_none() && !c.closed && n > 0 {
            at(c.pts[0])
        } else {
            0.0
        },
        if c.ends[1].is_none() && !c.closed && n > 0 {
            at(c.pts[n - 1])
        } else {
            0.0
        },
    ]
}

/// What the word a letter is of says of it: the width of its strokes,
/// where its top and bottom are (in the letter's own box) and how near
/// one a line must be to lie on it.
#[derive(Clone, Copy)]
struct Word {
    wide: f64,
    top: f64,
    bottom: f64,
    near: f64,
}

/// The straight stretches of `soft` (points a unit apart): pairs of
/// the indexes of their first and last points. `past` is how far the
/// line goes on before the first point and after the last.
fn straight_runs(soft: &[Point], past: [f64; 2], least: f64) -> Vec<(usize, usize)> {
    let m = soft.len();
    let straight = |a: usize, b: usize| -> bool {
        let d = unit(soft[b] - soft[a]);
        if d == Vec2::ZERO {
            return false;
        }
        if soft[a + 1..b]
            .iter()
            .any(|p| (*p - soft[a]).cross(d).abs() > RUN_OFF)
        {
            return false;
        }
        if b - a >= 8 {
            let mid = (a + b) / 2;
            let (u, v) = (soft[mid] - soft[a], soft[b] - soft[mid]);
            if u.cross(v).atan2(u.dot(v)).abs() > RUN_TURN {
                return false;
            }
        }
        true
    };
    let mut runs = Vec::new();
    let mut a = 0;
    while a + 4 < m {
        let mut b = a + 1;
        while b + 1 < m && straight(a, b + 1) {
            b += 1;
        }
        let own = (soft[b] - soft[a]).hypot();
        let mut long = own;
        if a == 0 {
            long += past[0];
        }
        if b == m - 1 {
            long += past[1];
        }
        if long < least || own < RUN_OWN * least || b - a < 4 {
            a += 1;
            continue;
        }
        // Its ends inside the line lean into the bends there.
        let lo = if a == 0 { a } else { a + 2 };
        let hi = if b == m - 1 { b } else { b - 2 };
        if hi > lo + 1 {
            runs.push((lo, hi));
        }
        a = b;
    }
    runs
}

/// `path`, of straight lines only, with the lines that are nearly level
/// or upright made so. `lines` is where a level middle line lies along
/// the top and the bottom of the word, and how near one counts.
fn upright(path: &BezPath, lines: Option<(f64, f64, f64)>) -> BezPath {
    use tracedraw_core::geometry::PathEl;
    let mut pts: Vec<Point> = Vec::new();
    let mut closed = false;
    for el in path.elements() {
        match el {
            PathEl::MoveTo(p) | PathEl::LineTo(p) => pts.push(*p),
            PathEl::ClosePath => closed = true,
            _ => return path.clone(),
        }
    }
    let n = pts.len();
    let sides = if closed { n } else { n.saturating_sub(1) };
    for i in 0..sides {
        let j = (i + 1) % n;
        let d = unit(pts[j] - pts[i]);
        if d == Vec2::ZERO {
            continue;
        }
        if d.y.abs() < RUN_AXIS.sin() {
            let mut y = (pts[i].y + pts[j].y) / 2.0;
            // A level line beside the top or the bottom of its word
            // lies on it.
            if let Some((top, bottom, near)) = lines {
                if (y - top).abs() <= near {
                    y = top;
                } else if (y - bottom).abs() <= near {
                    y = bottom;
                }
            }
            (pts[i].y, pts[j].y) = (y, y);
        } else if d.x.abs() < RUN_AXIS.sin() {
            let x = (pts[i].x + pts[j].x) / 2.0;
            (pts[i].x, pts[j].x) = (x, x);
        }
    }
    let mut out = BezPath::new();
    for (i, p) in pts.iter().enumerate() {
        if i == 0 {
            out.move_to(*p);
        } else {
            out.line_to(*p);
        }
    }
    if closed {
        out.close_path();
    }
    out
}

/// The middle line through `pts` (a unit apart): its straight stretches
/// as lines, level or upright when they nearly are, and what lies
/// between them fitted like a border.
fn middle_line(
    pts: &[Point],
    closed: bool,
    fit: &Faithful,
    wide: f64,
    past: [f64; 2],
    word: Option<&Word>,
) -> BezPath {
    use tracedraw_core::geometry::PathEl;
    let fitted = crate::trace_fit::fit(pts, closed, fit, None).path;
    // A middle line of straight lines only is those lines.
    if !fitted
        .elements()
        .iter()
        .any(|el| matches!(el, PathEl::CurveTo(..) | PathEl::QuadTo(..)))
    {
        let lines = word.map(|l| (l.top + wide / 2.0, l.bottom - wide / 2.0, l.near));
        return upright(&fitted, lines);
    }
    let whole = || fitted.clone();
    let n = pts.len();
    if n < 8 {
        return whole();
    }
    let least = (RUN_LEAST * wide).max(RUN_POINTS);
    let soften = |src: &[Point], wraps: bool| -> Vec<Point> {
        let m = src.len() as i64;
        (0..m)
            .map(|i| {
                let (mut sum, mut count) = (Vec2::ZERO, 0.0);
                let reach = if wraps { 2 } else { 2.min(i).min(m - 1 - i) };
                for k in -reach..=reach {
                    sum += src[(i + k).rem_euclid(m) as usize].to_vec2();
                    count += 1.0;
                }
                (sum / count).to_point()
            })
            .collect()
    };
    // A loop is opened where a straight stretch starts.
    let mut open: Vec<Point> = pts.to_vec();
    if closed {
        let mut twice = soften(pts, true);
        twice.extend_from_within(..);
        let Some(first) = straight_runs(&twice, [0.0, 0.0], least)
            .into_iter()
            .find(|r| r.1 - r.0 < n)
        else {
            return whole();
        };
        open.rotate_left(first.0 % n);
        open.push(open[0]);
    }
    let soft = soften(&open, false);
    let runs = straight_runs(&soft, if closed { [0.0, 0.0] } else { past }, least);
    if runs.is_empty() {
        return whole();
    }
    // Each run as the line through its points.
    let lines: Vec<(Point, Point)> = runs
        .iter()
        .map(|(a, b)| {
            let count = (b - a + 1) as f64;
            let c = (open[*a..=*b]
                .iter()
                .fold(Vec2::ZERO, |s, p| s + p.to_vec2())
                / count)
                .to_point();
            let (mut xx, mut xy, mut yy) = (0.0, 0.0, 0.0);
            for p in &open[*a..=*b] {
                let v = *p - c;
                xx += v.x * v.x;
                xy += v.x * v.y;
                yy += v.y * v.y;
            }
            let turn = 0.5 * (2.0 * xy).atan2(xx - yy);
            let mut d = Vec2::new(turn.cos(), turn.sin());
            if d.dot(open[*b] - open[*a]) < 0.0 {
                d = -d;
            }
            if d.y.abs() < RUN_AXIS.sin() {
                d = Vec2::new(d.x.signum(), 0.0);
            } else if d.x.abs() < RUN_AXIS.sin() {
                d = Vec2::new(0.0, d.y.signum());
            }
            (c + d * (open[*a] - c).dot(d), c + d * (open[*b] - c).dot(d))
        })
        .collect();
    // Two lines going opposite ways with a round between them (a U):
    // the half circle that joins them, when the round is one.
    let mut lines = lines;
    let mut rounds: Vec<Option<[PathEl; 2]>> = vec![None; lines.len()];
    let near = 2.0 * fit.tolerance * fit.k;
    for i in 1..lines.len() {
        let (d0, d1) = (
            unit(lines[i - 1].1 - lines[i - 1].0),
            unit(lines[i].1 - lines[i].0),
        );
        if d0.dot(d1) > -0.995 {
            continue;
        }
        let across = lines[i].0 - lines[i - 1].1;
        let across = across - d0 * across.dot(d0);
        let r = across.hypot() / 2.0;
        if r < 0.5 * wide {
            continue;
        }
        let gap = &open[runs[i - 1].1..=runs[i].0];
        let far = gap
            .iter()
            .map(|p| (*p - lines[i - 1].1).dot(d0))
            .fold(f64::MIN, f64::max);
        let c = lines[i - 1].1 + d0 * (far - r) + across / 2.0;
        let (from, to) = (c - across / 2.0, c + across / 2.0);
        let fits = gap.iter().all(|p| {
            let v = *p - c;
            if v.dot(d0) >= 0.0 {
                (v.hypot() - r).abs() <= near
            } else {
                (v.cross(d0).abs() - r).abs() <= near
            }
        });
        if !fits || (from - lines[i - 1].0).dot(d0) <= 0.0 || (lines[i].1 - to).dot(d1) <= 0.0 {
            continue;
        }
        const QUARTER: f64 = 0.5522847498;
        let (side, top) = (unit(across), c + d0 * r);
        rounds[i] = Some([
            PathEl::CurveTo(from + d0 * (QUARTER * r), top - side * (QUARTER * r), top),
            PathEl::CurveTo(top + side * (QUARTER * r), to + d0 * (QUARTER * r), to),
        ]);
        lines[i - 1].1 = from;
        lines[i].0 = to;
    }
    let mut els: Vec<PathEl> = Vec::new();
    // What lies between two lines, ending at `to`.
    let between = |els: &mut Vec<PathEl>,
                   from: usize,
                   until: usize,
                   to: Option<Point>,
                   ways: Option<(Vec2, Vec2)>| {
        if until <= from {
            return;
        }
        let gap = &open[from..=until];
        let long: f64 = gap.windows(2).map(|p| (p[1] - p[0]).hypot()).sum();
        let before = els.len();
        if long <= CHAMFER * wide || gap.len() < 4 {
            els.push(PathEl::LineTo(gap[gap.len() - 1]));
        } else {
            // Between two lines: the arc that joins them, when it is
            // one.
            let arc = ways.and_then(|(leave, arrive)| {
                crate::trace_fit::arc_between(gap, leave, arrive, 2.0 * fit.tolerance * fit.k)
            });
            let fitted = arc.unwrap_or_else(|| crate::trace_fit::fit(gap, false, fit, None).path);
            for el in fitted.elements() {
                match el {
                    PathEl::MoveTo(p) if els.is_empty() => els.push(PathEl::MoveTo(*p)),
                    PathEl::LineTo(_) | PathEl::CurveTo(..) => els.push(*el),
                    _ => {}
                }
            }
        }
        if let (Some(to), true) = (to, els.len() > before) {
            let last = els.len() - 1;
            els[last] = match els[last] {
                PathEl::CurveTo(a, b, c) => PathEl::CurveTo(a, b + (to - c), to),
                _ => PathEl::LineTo(to),
            };
        }
    };
    let mut at = 0;
    for (i, (a, b)) in runs.iter().enumerate() {
        if *a == 0 {
            els.push(PathEl::MoveTo(lines[i].0));
        } else {
            if els.is_empty() {
                els.push(PathEl::MoveTo(open[0]));
            }
            let ways = (i > 0).then(|| {
                (
                    unit(lines[i - 1].1 - lines[i - 1].0),
                    unit(lines[i].1 - lines[i].0),
                )
            });
            match rounds[i] {
                Some(round) => els.extend(round),
                None => between(&mut els, at, *a, Some(lines[i].0), ways),
            }
        }
        els.push(PathEl::LineTo(lines[i].1));
        at = *b;
    }
    let last = lines.len() - 1;
    let (ends_at, ways) = if closed {
        (
            Some(lines[0].0),
            Some((
                unit(lines[last].1 - lines[last].0),
                unit(lines[0].1 - lines[0].0),
            )),
        )
    } else {
        (None, None)
    };
    between(&mut els, at, open.len() - 1, ends_at, ways);
    if closed {
        els.push(PathEl::ClosePath);
    }
    BezPath::from_vec(els)
}

/// A fitted middle line ready to be drawn: its points, the corners
/// that take a point (where, the way in, the way out) and which of its
/// ends are cut square.
struct Stroke {
    line: Vec<Point>,
    corners: Vec<(Point, Vec2, Vec2)>,
    /// The square ends: where, and the way the stroke was going.
    cuts: Vec<(Point, Vec2)>,
    /// Half its width.
    half: f64,
}

/// The fitted `path` of a chain, carried `past` its two ends along its
/// own way there, with its free ends (`square`) cut square.
fn stroke_of(
    path: &BezPath,
    closed: bool,
    square: [bool; 2],
    past: [f64; 2],
    wide: f64,
    held: &dyn Fn(Point) -> bool,
    word: Option<&Word>,
) -> Stroke {
    use tracedraw_core::geometry::{Line, PathSeg};
    let mut line: Vec<Point> = Vec::new();
    let mut corners = Vec::new();
    let mut ways: Vec<(Point, Vec2, Vec2)> = Vec::new();
    // How each piece bends at its two ends, radians a point (to the
    // left of its way above zero).
    let mut bends: Vec<(f64, f64)> = Vec::new();
    // A short piece between two lines that turn is the thinning's way
    // round a corner: the two lines meet where they cross.
    let mut segs: Vec<PathSeg> = path.segments().collect();
    // A loop is gone over twice, from two places, so the piece across
    // its start is seen too.
    for pass in 0..if closed { 2 } else { 1 } {
        if pass == 1 && segs.len() > 3 {
            segs.rotate_left(2);
        }
        let mut i = 1;
        while i + 1 < segs.len() {
            if let (PathSeg::Line(a), PathSeg::Line(b)) = (segs[i - 1], segs[i + 1]) {
                let (da, db) = (unit(a.p1 - a.p0), unit(b.p1 - b.p0));
                let cross = da.cross(db);
                let short = (segs[i].eval(1.0) - segs[i].eval(0.0)).hypot() <= CHAMFER * wide;
                if short && cross.abs() > MITER_LEAST.sin() {
                    let t = (b.p0 - a.p0).cross(db) / cross;
                    let at = a.p0 + da * t;
                    if (at - segs[i].eval(0.5)).hypot() <= CHAMFER * wide {
                        segs[i - 1] = PathSeg::Line(Line::new(a.p0, at));
                        segs[i + 1] = PathSeg::Line(Line::new(at, b.p1));
                        segs.remove(i);
                        continue;
                    }
                }
            }
            i += 1;
        }
    }
    for seg in segs {
        let (from, to) = (seg.eval(0.0), seg.eval(1.0));
        if (to - from).hypot() < 1e-9 {
            continue;
        }
        let steps = match seg {
            tracedraw_core::geometry::PathSeg::Line(_) => 1,
            _ => 16,
        };
        if line.is_empty() {
            line.push(from);
        }
        for i in 1..=steps {
            line.push(seg.eval(i as f64 / steps as f64));
        }
        ways.push((from, unit(seg.eval(0.03) - from), unit(to - seg.eval(0.97))));
        bends.push(match seg {
            PathSeg::Line(_) => (0.0, 0.0),
            _ => {
                let bend = |t0: f64, t1: f64| -> f64 {
                    let a = unit(seg.eval(t0 + 0.03) - seg.eval(t0));
                    let b = unit(seg.eval(t1) - seg.eval(t1 - 0.03));
                    let long: f64 = (0..8)
                        .map(|k| {
                            let (u, v) = (
                                t0 + (t1 - t0) * k as f64 / 8.0,
                                t0 + (t1 - t0) * (k + 1) as f64 / 8.0,
                            );
                            (seg.eval(v) - seg.eval(u)).hypot()
                        })
                        .sum();
                    if long > 1e-9 {
                        a.cross(b).atan2(a.dot(b)) / long
                    } else {
                        0.0
                    }
                };
                (bend(0.0, 0.4), bend(0.6, 1.0))
            }
        });
    }
    let n = ways.len();
    for i in 0..n {
        let next = if i + 1 < n {
            i + 1
        } else if closed {
            0
        } else {
            break;
        };
        let (a, b) = (ways[i].2, ways[next].1);
        let turn = a.cross(b).atan2(a.dot(b)).abs();
        if (MITER_LEAST..=MITER_MOST).contains(&turn) {
            corners.push((ways[next].0, a, b));
        }
    }
    let mut cuts = Vec::new();
    if !closed && n > 0 && line.len() >= 2 {
        let (first, last) = (-ways[0].1, ways[n - 1].2);
        // The line carried on from `p` along `d`, bending as it did,
        // as far as the letter goes: its points and the way at its end.
        let carry = |p: Point, d: Vec2, bend: f64, most: f64| -> (Vec<Point>, Vec2) {
            // A tighter bend is the hook of a tip, not the stroke's.
            let bend = if (BEND_LEAST..=BEND_MOST).contains(&(bend.abs() * wide)) {
                bend
            } else {
                0.0
            };
            let side = Vec2::new(-d.y, d.x);
            let at = |t: f64| -> (Point, Vec2) {
                if bend == 0.0 {
                    (p + d * t, d)
                } else {
                    let a = bend * t;
                    (
                        p + d * (a.sin() / bend) + side * ((1.0 - a.cos()) / bend),
                        d * a.cos() + side * a.sin(),
                    )
                }
            };
            let mut t = 0.0;
            while t < most && held(at(t + 0.25).0) {
                t += 0.25;
            }
            let steps = if bend == 0.0 {
                1
            } else {
                (t.ceil() as usize).max(1)
            };
            let mut pts: Vec<Point> = (1..=steps)
                .map(|k| at(t * k as f64 / steps as f64).0)
                .collect();
            // An upright end beside the top or the bottom of its word
            // ends on it.
            if let (Some(l), true, Some(end)) =
                (word, bend == 0.0 && d.x.abs() < 1e-6, pts.last_mut())
            {
                let line = if d.y < 0.0 { l.top } else { l.bottom };
                if (end.y - line).abs() <= l.near && (line - p.y) * d.y > 0.0 {
                    end.y = line;
                }
            }
            (pts, at(t).1)
        };
        if square[0] && first != Vec2::ZERO {
            let (pts, way) = carry(line[0], first, -bends[0].0, 2.0 * past[0] + 4.0);
            for q in pts {
                line.insert(0, q);
            }
            cuts.push((line[0], way));
        }
        if square[1] && last != Vec2::ZERO {
            let from = line[line.len() - 1];
            let (pts, way) = carry(from, last, bends[n - 1].1, 2.0 * past[1] + 4.0);
            line.extend(pts);
            cuts.push((line[line.len() - 1], way));
        }
    }
    Stroke {
        line,
        corners,
        cuts,
        half: wide / 2.0,
    }
}

impl Stroke {
    /// Whether `q` is within `half` of the line: inside the stroke.
    fn holds(&self, q: Point) -> bool {
        let half = self.half;
        let n = self.line.len();
        // Past a square end, beside it, is outside.
        for (end, way) in &self.cuts {
            let p = q - *end;
            if p.dot(*way) > 0.0 && p.hypot2() <= 2.25 * half * half {
                return false;
            }
        }
        for i in 1..n {
            let (a, b) = (self.line[i - 1], self.line[i]);
            let ab = b - a;
            let len2 = ab.hypot2();
            if len2 < 1e-18 {
                continue;
            }
            let t = (q - a).dot(ab) / len2;
            let near = a + ab * t.clamp(0.0, 1.0);
            if (q - near).hypot2() <= half * half {
                return true;
            }
        }
        // The point of a corner: between its two outer sides.
        for (v, a, b) in &self.corners {
            let s = if a.cross(*b) > 0.0 { 1.0 } else { -1.0 };
            let (n1, n2) = (Vec2::new(a.y, -a.x) * s, Vec2::new(b.y, -b.x) * s);
            let p = q - *v;
            if p.dot(n1) <= half
                && p.dot(n2) <= half
                && p.dot(n1) >= 0.0
                && p.dot(n2) >= 0.0
                && p.dot(*a) >= -1e-9
                && p.dot(*b) <= 1e-9
            {
                return true;
            }
        }
        false
    }
}

/// The width of the strokes of the letter `mask`, none when it is not
/// a letter of strokes of one width.
fn width_of(mask: &[bool], w: usize, h: usize, unit_px: f64) -> Option<f64> {
    let inside = distances(mask, w, h);
    let mut thin = mask.to_vec();
    if !crate::trace::thin_unless(&mut thin, w, h, None) {
        return None;
    }
    let mut on: Vec<f32> = (0..w * h).filter(|i| thin[*i]).map(|i| inside[i]).collect();
    if on.len() < 4 {
        return None;
    }
    on.sort_by(|a, b| a.total_cmp(b));
    let wide = 2.0 * on[on.len() / 2] as f64 - 1.0;
    (WIDE_LEAST * unit_px..=WIDE_MOST * unit_px)
        .contains(&wide)
        .then_some(wide)
}

/// One letter: `mask` (`w` x `h`, with room round it) drawn again by
/// its strokes, or none when it is not such a letter or the two do not
/// agree.
fn letter(
    mask: &[bool],
    w: usize,
    h: usize,
    unit_px: f64,
    f: &Faithful,
    word: Option<&Word>,
) -> Option<Vec<bool>> {
    let area = mask.iter().filter(|m| **m).count();
    let inside = distances(mask, w, h);
    let mut thin = mask.to_vec();
    if !crate::trace::thin_unless(&mut thin, w, h, None) {
        return None;
    }
    let mut on: Vec<f32> = (0..w * h).filter(|i| thin[*i]).map(|i| inside[i]).collect();
    if on.len() < 4 {
        return None;
    }
    on.sort_by(|a, b| a.total_cmp(b));
    let middle = on[on.len() / 2] as f64;
    let wide = 2.0 * middle - 1.0;
    if !(WIDE_LEAST * unit_px..=WIDE_MOST * unit_px).contains(&wide) {
        return None;
    }
    let even = on
        .iter()
        .filter(|d| (**d as f64 - middle).abs() <= EVEN * middle + 1.0)
        .count();
    if (even as f64) < EVEN_SHARE * on.len() as f64 || (on.len() as f64) < 1.5 * wide {
        return None;
    }
    // The width of its word, when its own is near it.
    let word = word.filter(|l| (l.wide - wide).abs() <= WORD_WIDE * wide);
    let wide = word.map_or(wide, |l| l.wide);
    let (mut chains, nodes) = chains_of(&thin, w, h);
    join(&mut chains, &nodes, wide);
    let fit = Faithful {
        k: f.k,
        tolerance: FIT_TOLERANCE * unit_px / f.k,
        corner: f.corner,
        scale: f.scale,
    };
    // Whether `q` is a point of the letter.
    let held = |q: Point| {
        q.x >= 0.0
            && q.y >= 0.0
            && (q.x as usize) < w
            && (q.y as usize) < h
            && mask[q.y as usize * w + q.x as usize]
    };
    let mut strokes = Vec::new();
    for c in chains.iter().filter(|c| c.alive) {
        let mut past = past_ends(c, &inside, w, h);
        // A stroke long enough to say so has its own width (the bar
        // of a T thinner than its stem).
        let mut on: Vec<f32> = c
            .pts
            .iter()
            .filter(|p| p.x >= 0.0 && p.y >= 0.0 && (p.x as usize) < w && (p.y as usize) < h)
            .map(|p| inside[p.y as usize * w + p.x as usize])
            .collect();
        on.sort_by(|a, b| a.total_cmp(b));
        let mut own = wide;
        if word.is_none() && c.length() >= 2.0 * wide && !on.is_empty() {
            let mine = 2.0 * on[on.len() / 2] as f64 - 1.0;
            if (mine - wide).abs() > OWN_LEAST * wide {
                own = mine.clamp((1.0 - OWN_MOST) * wide, (1.0 + OWN_MOST) * wide);
            }
        }
        // The tip of a thinned line veers to a corner of its end: it
        // is left out, and the line carried on as far.
        let mut pts = c.pts.clone();
        if !c.closed && c.length() >= 3.0 * wide {
            if c.ends[1].is_none() {
                let mut back = pts.clone();
                back.reverse();
                let cut = index_at(&back, TIP * wide);
                let keep = pts.len() - cut;
                past[1] += (pts[pts.len() - 1] - pts[keep - 1]).hypot();
                pts.truncate(keep);
            }
            if c.ends[0].is_none() {
                let cut = index_at(&pts, TIP * wide);
                past[0] += (pts[0] - pts[cut]).hypot();
                pts.drain(..cut);
            }
        }
        let pts = resample(&pts, 1.0, c.closed);
        if pts.len() < 2 {
            continue;
        }
        let path = middle_line(&pts, c.closed, &fit, own, past, word);
        let s = stroke_of(
            &path,
            c.closed,
            [c.ends[0].is_none(), c.ends[1].is_none()],
            past,
            own,
            &held,
            word,
        );
        if s.line.len() >= 2 {
            strokes.push(s);
        }
    }
    if strokes.is_empty() {
        return None;
    }
    let mut drawn = vec![false; w * h];
    for (i, d) in drawn.iter_mut().enumerate() {
        let q = Point::new((i % w) as f64 + 0.5, (i / w) as f64 + 0.5);
        *d = strokes.iter().any(|s| s.holds(q));
    }
    // The check: the letter drawn again against the picture's.
    let outside = distances(&mask.iter().map(|m| !*m).collect::<Vec<_>>(), w, h);
    let most = (STRAY_MOST * unit_px).min(STRAY_WIDE * wide) as f32;
    let mut off = 0usize;
    for i in 0..w * h {
        if drawn[i] != mask[i] {
            let d = if mask[i] { inside[i] } else { outside[i] };
            if d > most {
                off += 1;
            }
        }
        // Nothing drawn on the rim of the box.
        let (x, y) = (i % w, i / w);
        if drawn[i] && (x == 0 || y == 0 || x + 1 == w || y + 1 == h) {
            return None;
        }
    }
    if off as f64 > STRAY_SHARE * area as f64 || grounds(&drawn, w, h) != grounds(mask, w, h) {
        return None;
    }
    Some(drawn)
}

/// Draw the small thin letters of `labels` (`w` x `h`, `unit_px` points
/// of it to a pixel of the picture before it was enlarged) again by
/// their strokes. Gives the points whose borders are drawn already, to
/// be fitted as they are and not placed by the picture again.
pub(crate) fn redraw(
    labels: &mut [usize],
    w: usize,
    h: usize,
    unit_px: f64,
    f: &Faithful,
    palette: &[[u8; 3]],
    cancel: Option<&crate::trace::Cancel>,
) -> Option<Vec<bool>> {
    let mut redrawn = vec![false; w * h];
    if w * h == 0 || !unit_px.is_finite() || unit_px < 2.0 {
        return Some(redrawn);
    }
    let most = (BOX_MOST * unit_px) as usize;
    let least = (unit_px * unit_px * 2.0) as usize;
    let pad = unit_px.ceil() as usize + 3;
    let mut seen = vec![false; w * h];
    let (mut stack, mut cells) = (Vec::new(), Vec::new());
    let mut changes: Vec<(usize, usize)> = Vec::new();
    /// A shape that may be a letter: its points, its box with room
    /// round it, its box in the picture, its label and the width of its
    /// strokes.
    struct Found {
        cells: Vec<usize>,
        mask: Vec<bool>,
        at: (usize, usize, usize, usize),
        span: (usize, usize, usize, usize),
        label: usize,
        wide: Option<f64>,
    }
    let mut found: Vec<Found> = Vec::new();
    for start in 0..w * h {
        if seen[start] || labels[start] == usize::MAX {
            continue;
        }
        if start % (w * 64) == 0 && crate::trace::cancelled(cancel) {
            return None;
        }
        let label = labels[start];
        // A letter in two shades of its colour is one letter.
        let alike = |l: usize| -> bool {
            l == label
                || match (palette.get(l), palette.get(label)) {
                    (Some(a), Some(b)) => {
                        let d = |i: usize| (a[i] as i64 - b[i] as i64).pow(2);
                        d(0) + d(1) + d(2) <= SHADE_NEAR * SHADE_NEAR
                    }
                    _ => false,
                }
        };
        cells.clear();
        seen[start] = true;
        stack.push(start);
        let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0usize, 0usize);
        let mut small = true;
        while let Some(i) = stack.pop() {
            let (x, y) = (i % w, i / w);
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
            if small {
                if x1 - x0 > most || y1 - y0 > most {
                    small = false;
                    cells.clear();
                } else {
                    cells.push(i);
                }
            }
            let mut visit = |j: usize| {
                if !seen[j] && alike(labels[j]) {
                    seen[j] = true;
                    stack.push(j);
                }
            };
            if x > 0 {
                visit(i - 1);
            }
            if x + 1 < w {
                visit(i + 1);
            }
            if y > 0 {
                visit(i - w);
            }
            if y + 1 < h {
                visit(i + w);
            }
        }
        if !small || cells.len() < least || x0 < pad || y0 < pad || x1 + pad >= w || y1 + pad >= h {
            continue;
        }
        let (bx, by) = (x0 - pad, y0 - pad);
        let (lw, lh) = (x1 - x0 + 1 + 2 * pad, y1 - y0 + 1 + 2 * pad);
        let mut mask = vec![false; lw * lh];
        for i in &cells {
            mask[(i / w - by) * lw + (i % w - bx)] = true;
        }
        let wide = width_of(&mask, lw, lh, unit_px);
        found.push(Found {
            cells: cells.clone(),
            mask,
            at: (bx, by, lw, lh),
            span: (x0, y0, x1, y1),
            label,
            wide,
        });
    }
    // The words the letters are of.
    let count = found.len();
    let mut word: Vec<usize> = (0..count).collect();
    fn root(word: &mut [usize], mut i: usize) -> usize {
        while word[i] != i {
            word[i] = word[word[i]];
            i = word[i];
        }
        i
    }
    let shade = |a: usize, b: usize| -> bool {
        a == b
            || match (palette.get(a), palette.get(b)) {
                (Some(a), Some(b)) => {
                    let d = |i: usize| (a[i] as i64 - b[i] as i64).pow(2);
                    d(0) + d(1) + d(2) <= SHADE_NEAR * SHADE_NEAR
                }
                _ => false,
            }
    };
    for i in 0..count {
        for j in i + 1..count {
            let (a, b) = (&found[i], &found[j]);
            if a.wide.is_none() || b.wide.is_none() || !shade(a.label, b.label) {
                continue;
            }
            let (ha, hb) = (
                (a.span.3 - a.span.1 + 1) as f64,
                (b.span.3 - b.span.1 + 1) as f64,
            );
            let tall = ha.max(hb);
            let off = |p: usize, q: usize| (p as f64 - q as f64).abs() <= WORD_TALL * tall;
            let gap = if a.span.0 > b.span.2 {
                (a.span.0 - b.span.2) as f64
            } else if b.span.0 > a.span.2 {
                (b.span.0 - a.span.2) as f64
            } else {
                0.0
            };
            if off(a.span.1, b.span.1) && off(a.span.3, b.span.3) && gap <= WORD_GAP * tall {
                let (ra, rb) = (root(&mut word, i), root(&mut word, j));
                word[ra] = rb;
            }
        }
    }
    let middle = |mut v: Vec<f64>| -> f64 {
        v.sort_by(|a, b| a.total_cmp(b));
        v[v.len() / 2]
    };
    for at in 0..count {
        if at % 16 == 0 && crate::trace::cancelled(cancel) {
            return None;
        }
        let mine = root(&mut word, at);
        let fellows: Vec<usize> = (0..count).filter(|i| root(&mut word, *i) == mine).collect();
        let Found {
            cells,
            mask,
            at: (bx, by, lw, lh),
            label,
            ..
        } = &found[at];
        let (bx, by, lw, lh, label) = (*bx, *by, *lw, *lh, *label);
        let said = (fellows.len() >= WORD_LEAST).then(|| Word {
            wide: middle(fellows.iter().filter_map(|i| found[*i].wide).collect()),
            top: middle(fellows.iter().map(|i| found[*i].span.1 as f64).collect()) - by as f64,
            bottom: middle(
                fellows
                    .iter()
                    .map(|i| found[*i].span.3 as f64 + 1.0)
                    .collect(),
            ) - by as f64,
            near: WORD_LINE * unit_px,
        });
        // What the word says is kept only where the letter still
        // agrees with the picture.
        let drawn = said
            .as_ref()
            .and_then(|l| letter(mask, lw, lh, unit_px, f, Some(l)))
            .or_else(|| letter(mask, lw, lh, unit_px, f, None));
        let Some(drawn) = drawn else {
            continue;
        };
        // The shade most of it has.
        let mut shades: Vec<(usize, usize)> = Vec::new();
        for i in cells.iter() {
            match shades.iter_mut().find(|s| s.0 == labels[*i]) {
                Some(s) => s.1 += 1,
                None => shades.push((labels[*i], 1)),
            }
        }
        let label = shades.iter().max_by_key(|s| s.1).map_or(label, |s| s.0);
        // What the letter no longer covers takes the nearest label
        // round it.
        let mut near: Vec<usize> = (0..lw * lh)
            .map(|i| {
                if mask[i] {
                    usize::MAX - 1
                } else {
                    labels[(by + i / lw) * w + bx + i % lw]
                }
            })
            .collect();
        let mut queue: std::collections::VecDeque<usize> =
            (0..lw * lh).filter(|i| !mask[*i]).collect();
        while let Some(i) = queue.pop_front() {
            let (x, y) = (i % lw, i / lw);
            let l = near[i];
            let mut visit = |j: usize| {
                if near[j] == usize::MAX - 1 {
                    near[j] = l;
                    queue.push_back(j);
                }
            };
            if x > 0 {
                visit(i - 1);
            }
            if x + 1 < lw {
                visit(i + 1);
            }
            if y > 0 {
                visit(i - lw);
            }
            if y + 1 < lh {
                visit(i + lw);
            }
        }
        for i in 0..lw * lh {
            let at = (by + i / lw) * w + bx + i % lw;
            redrawn[at] = true;
            if drawn[i] && labels[at] != label {
                changes.push((at, label));
            } else if !drawn[i] && mask[i] && near[i] != usize::MAX - 1 {
                changes.push((at, near[i]));
            }
        }
    }
    for (at, l) in changes {
        labels[at] = l;
    }
    Some(redrawn)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIDE: usize = 160;

    fn faithful() -> Faithful {
        Faithful {
            k: 2.0,
            tolerance: 0.85,
            corner: 1.0,
            scale: 4.0,
        }
    }

    /// An H of strokes 16 wide as an enlarging leaves it: its sides
    /// wavering by a point, the corners of its ends worn round.
    fn soft_h(x: usize, y: usize) -> bool {
        let (x, y) = (x as f64 + 0.5, y as f64 + 0.5);
        let waver = (1.2 * (y / 5.0).sin()).round();
        let stem = |x0: f64| {
            let inside = x >= x0 + waver && x < x0 + 16.0 + waver && (40.0..104.0).contains(&y);
            // The ends: half discs.
            let end = |cy: f64| (x - x0 - 8.0).hypot((y - cy) * 1.6) < 8.0;
            inside && ((46.0..98.0).contains(&y) || end(46.0) || end(98.0))
        };
        let bar = (48.0..96.0).contains(&x) && (64.0..80.0).contains(&y);
        stem(40.0) || stem(88.0) || bar
    }

    fn row(labels: &[usize], y: usize) -> Vec<usize> {
        (0..SIDE).filter(|x| labels[y * SIDE + x] == 1).collect()
    }

    /// The letter drawn again has stems with straight upright sides, all
    /// of one width, and ends cut square at the height the letter had.
    #[test]
    fn a_soft_letter_comes_back_with_straight_strokes_and_square_ends() {
        let mut labels: Vec<usize> = (0..SIDE * SIDE)
            .map(|i| soft_h(i % SIDE, i / SIDE) as usize)
            .collect();
        let palette = [[255, 255, 255], [0, 0, 0]];
        let redrawn =
            redraw(&mut labels, SIDE, SIDE, 8.0, &faithful(), &palette, None).expect("not stopped");
        assert!(redrawn.iter().any(|r| *r), "the letter was not drawn again");
        // The left stem above the bar: the same run of points on
        // every row.
        let first = row(&labels, 44);
        let left: Vec<usize> = first.iter().copied().filter(|x| *x < 70).collect();
        assert!((14..=18).contains(&left.len()), "{} wide", left.len());
        for y in 41..62 {
            let here: Vec<usize> = row(&labels, y).into_iter().filter(|x| *x < 70).collect();
            assert_eq!(here, left, "row {y}");
        }
        // Its end is square: the top row is as wide as the stem, and
        // within a point and a half of where the letter's top was.
        let top = (0..SIDE)
            .find(|y| !row(&labels, *y).is_empty())
            .expect("a letter");
        assert!((39..=42).contains(&top), "top at {top}");
        let across: Vec<usize> = row(&labels, top).into_iter().filter(|x| *x < 70).collect();
        assert_eq!(across, left, "the end is not square");
        // The bar joins the stems: one piece.
        assert!((56..88).all(|x| labels[72 * SIDE + x] == 1));
    }

    /// A shape that is not a thin stroke (a disc, a wide block) is left
    /// as it is.
    #[test]
    fn a_shape_that_is_no_stroke_is_left_alone() {
        let shape = |x: usize, y: usize| {
            let (dx, dy) = (x as f64 - 50.0, y as f64 - 50.0);
            dx * dx + dy * dy < 30.0 * 30.0 || ((90..150).contains(&x) && (90..130).contains(&y))
        };
        let before: Vec<usize> = (0..SIDE * SIDE)
            .map(|i| shape(i % SIDE, i / SIDE) as usize)
            .collect();
        let mut labels = before.clone();
        let palette = [[255, 255, 255], [0, 0, 0]];
        let redrawn =
            redraw(&mut labels, SIDE, SIDE, 8.0, &faithful(), &palette, None).expect("not stopped");
        assert_eq!(labels, before);
        assert!(redrawn.iter().all(|r| !*r));
    }

    /// A letter in two shades of its colour is drawn again as one
    /// letter, in the shade most of it has; a part in another colour
    /// altogether is not taken for part of it.
    #[test]
    fn a_letter_in_two_shades_is_drawn_as_one() {
        let make = |other: usize| -> Vec<usize> {
            (0..SIDE * SIDE)
                .map(|i| {
                    let (x, y) = (i % SIDE, i / SIDE);
                    if !soft_h(x, y) {
                        0
                    } else if x >= 88 && y > 84 {
                        other
                    } else {
                        1
                    }
                })
                .collect()
        };
        let palette = [[255, 255, 255], [20, 20, 20], [40, 30, 30], [220, 40, 40]];
        let mut labels = make(2);
        redraw(&mut labels, SIDE, SIDE, 8.0, &faithful(), &palette, None).expect("not stopped");
        assert!(labels.iter().all(|l| *l != 2), "the second shade is left");
        assert!(labels[95 * SIDE + 96] == 1, "the foot of the stem is gone");
        let mut labels = make(3);
        redraw(&mut labels, SIDE, SIDE, 8.0, &faithful(), &palette, None).expect("not stopped");
        assert!(labels[95 * SIDE + 96] == 3, "another colour was taken over");
    }

    /// The middle line of a T is its bar and its stem, the stem ending
    /// on the bar; the branches the thinning grows into the corners of
    /// the square ends are dropped.
    #[test]
    fn the_middle_line_of_a_t_is_two_strokes() {
        let (w, h) = (100usize, 100usize);
        let mask: Vec<bool> = (0..w * h)
            .map(|i| {
                let (x, y) = (i % w, i / w);
                ((20..80).contains(&x) && (20..32).contains(&y))
                    || ((44..56).contains(&x) && (20..84).contains(&y))
            })
            .collect();
        let mut thin = mask.clone();
        assert!(crate::trace::thin_unless(&mut thin, w, h, None));
        let (mut chains, nodes) = chains_of(&thin, w, h);
        join(&mut chains, &nodes, 12.0);
        let alive: Vec<&Chain> = chains.iter().filter(|c| c.alive).collect();
        assert_eq!(alive.len(), 2, "{} strokes", alive.len());
        let bar = alive
            .iter()
            .find(|c| c.ends == [None, None])
            .expect("a bar with two free ends");
        assert!(
            bar.pts.iter().all(|p| (p.y - 26.0).abs() < 2.5),
            "{:?}",
            bar.pts
        );
        let stem = alive
            .iter()
            .find(|c| c.ends != [None, None])
            .expect("a stem ending on the bar");
        assert!(
            stem.pts.iter().all(|p| (p.x - 50.0).abs() < 2.5),
            "{:?}",
            stem.pts
        );
    }

    /// The middle line of a U is its two stems, exactly upright, and
    /// the half circle that joins them.
    #[test]
    fn the_middle_line_of_a_u_is_two_stems_and_a_half_circle() {
        use tracedraw_core::geometry::PathSeg;
        let mut pts: Vec<Point> = Vec::new();
        let waver = |i: usize| 0.3 * ((i * 7 % 5) as f64 - 2.0) / 2.0;
        for i in 0..40 {
            pts.push(Point::new(20.0 + waver(i), 10.0 + i as f64));
        }
        for i in 0..63 {
            let a = std::f64::consts::PI * (1.0 - i as f64 / 62.0);
            pts.push(Point::new(40.0 + 20.0 * a.cos(), 50.0 + 20.0 * a.sin()));
        }
        for i in 1..=40 {
            pts.push(Point::new(60.0 + waver(i), 50.0 - i as f64));
        }
        let f = faithful();
        let fit = Faithful {
            tolerance: FIT_TOLERANCE * 8.0 / f.k,
            ..f
        };
        let path = middle_line(&pts, false, &fit, 12.0, [0.0, 0.0], None);
        let segs: Vec<PathSeg> = path.segments().collect();
        assert_eq!(segs.len(), 4, "{path:?}");
        for i in [0, 3] {
            let (a, b) = (segs[i].eval(0.0), segs[i].eval(1.0));
            assert!(matches!(segs[i], PathSeg::Line(_)), "{path:?}");
            assert!((a.x - b.x).abs() < 1e-9, "a stem leans: {a:?} {b:?}");
        }
        for seg in &segs[1..3] {
            for t in [0.0, 0.3, 0.6, 1.0] {
                let p = seg.eval(t);
                let r = (p.x - 40.0).hypot(p.y - 50.0);
                assert!((r - 20.0).abs() < 0.6, "{p:?} at {r}");
            }
        }
    }

    /// The free ends of a C go on round its ring: the letter drawn
    /// again stays in the ring the letter is.
    #[test]
    fn a_free_end_follows_the_bend_of_its_stroke() {
        let ring = |x: usize, y: usize| {
            let (dx, dy) = (x as f64 + 0.5 - 80.0, y as f64 + 0.5 - 80.0);
            let r = dx.hypot(dy);
            (23.0..37.0).contains(&r) && dy.atan2(dx).abs() > 0.8
        };
        let mut labels: Vec<usize> = (0..SIDE * SIDE)
            .map(|i| ring(i % SIDE, i / SIDE) as usize)
            .collect();
        let palette = [[255, 255, 255], [0, 0, 0]];
        let redrawn =
            redraw(&mut labels, SIDE, SIDE, 8.0, &faithful(), &palette, None).expect("not stopped");
        assert!(redrawn.iter().any(|r| *r), "the letter was not drawn again");
        for (i, l) in labels.iter().enumerate() {
            if *l == 1 {
                let (dx, dy) = (
                    (i % SIDE) as f64 + 0.5 - 80.0,
                    (i / SIDE) as f64 + 0.5 - 80.0,
                );
                let r = dx.hypot(dy);
                assert!((21.5..38.5).contains(&r), "a point at {r} from the middle");
            }
        }
        // And reaches the ends the ring has.
        let at = |a: f64| {
            let (x, y) = (80.0 + 30.0 * a.cos(), 80.0 + 30.0 * a.sin());
            labels[y as usize * SIDE + x as usize]
        };
        assert_eq!(at(0.9), 1, "the end is short");
        assert_eq!(at(-0.9), 1, "the end is short");
    }

    /// The letters of a word have one width of stroke and stand on
    /// one line: a stem a little wider and a little shorter than its
    /// fellows comes back as wide and as tall as they are.
    #[test]
    fn the_letters_of_a_word_share_a_width_and_a_line() {
        let stems = |x: usize, y: usize| {
            ((30..42).contains(&x) && (40..104).contains(&y))
                || ((70..84).contains(&x) && (42..104).contains(&y))
                || ((110..122).contains(&x) && (40..104).contains(&y))
        };
        let mut labels: Vec<usize> = (0..SIDE * SIDE)
            .map(|i| stems(i % SIDE, i / SIDE) as usize)
            .collect();
        let palette = [[255, 255, 255], [0, 0, 0]];
        redraw(&mut labels, SIDE, SIDE, 8.0, &faithful(), &palette, None).expect("not stopped");
        let wide = |from: usize| {
            (from..from + 40)
                .filter(|x| labels[70 * SIDE + x] == 1)
                .count()
        };
        let top = |x: usize| (0..SIDE).find(|y| labels[y * SIDE + x] == 1);
        assert_eq!(wide(20), wide(60), "the middle stem is wider");
        assert_eq!(wide(100), wide(60));
        assert_eq!(top(36), top(77), "the middle stem is shorter");
        assert_eq!(top(116), top(77));
    }
}
