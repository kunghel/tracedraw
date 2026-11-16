//! The faithful fit of a border of the outline trace, which checks its
//! own work. See docs/behavior/bitmap-tracing.md, "Faithful fit".
//!
//! A picture enlarged before it is traced has sharp edges already: its
//! borders say where the art's edges run, and the curves have only to
//! follow them with as few nodes as that takes. The border is smoothed
//! just enough to lose the steps of its grid; its corners are found by
//! how its turn grows with the distance it is measured over (a corner
//! turns as much over a short distance as over a long one, an arc twice
//! as much over twice the distance); a corner between straight sides is
//! put where their lines cross, so it is sharp; and each stretch between
//! two corners becomes the fewest straight lines and cubics that stay
//! within the tolerance. Then the result is measured against the border
//! it was made from, both ways, and a border whose curves stray is fitted
//! again more strictly, until it does not.

use tracedraw_core::geometry::{BezPath, CubicBez, ParamCurve, ParamCurveDeriv, Point, Vec2};

/// How a border is fitted faithfully. Lengths are in pixels of the
/// picture traced.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Faithful {
    /// Points of the border's grid per pixel of the picture.
    pub k: f64,
    /// How far a curve may stray from the smoothed border.
    pub tolerance: f64,
    /// The turn that makes a corner, radians.
    pub corner: f64,
    /// How many pixels of the picture traced one pixel of the picture it
    /// was enlarged from is.
    pub scale: f64,
}

/// The picture a border is traced from, and the colours of the two
/// regions it runs between: enough to say where between its pixels the
/// edge runs.
#[derive(Clone, Copy)]
pub(crate) struct Picture<'a> {
    pub colors: &'a [[u8; 3]],
    pub w: usize,
    pub h: usize,
    /// The colour on the border's `a` side (the right of its direction
    /// of travel on screen) and on its `b` side; none where the region
    /// has no single colour (a blend, the background taken out): the
    /// picture's own colours a little way into that side are taken.
    pub a: Option<[f32; 3]>,
    pub b: Option<[f32; 3]>,
    /// Whether the `a` side's region covers less of the picture than the
    /// `b` side's (the lesser is the shape, the greater its ground); none
    /// when it is not known.
    pub a_less: Option<bool>,
}

impl Picture<'_> {
    /// The picture's colour at a place (pixels; a pixel's middle lies
    /// half a pixel in), laid evenly between the four pixels round it.
    fn at(&self, p: Point) -> [f32; 3] {
        let (x, y) = (
            (p.x - 0.5).clamp(0.0, (self.w - 1) as f64),
            (p.y - 0.5).clamp(0.0, (self.h - 1) as f64),
        );
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(self.w - 1), (y0 + 1).min(self.h - 1));
        let (tx, ty) = ((x - x0 as f64) as f32, (y - y0 as f64) as f32);
        let c = |x: usize, y: usize| self.colors[y * self.w + x];
        std::array::from_fn(|k| {
            let top = c(x0, y0)[k] as f32 * (1.0 - tx) + c(x1, y0)[k] as f32 * tx;
            let bottom = c(x0, y1)[k] as f32 * (1.0 - tx) + c(x1, y1)[k] as f32 * tx;
            top * (1.0 - ty) + bottom * ty
        })
    }
}

/// How far (pixels) either side of the border the edge is looked for,
/// in steps of [`EDGE_STEP`], and how far into a side its colour is
/// taken from the picture.
const EDGE_REACH: f64 = 1.5;
const EDGE_STEP: f64 = 0.25;
const EDGE_INTO: f64 = 3.0;
/// How far from its own side's colour (share of the way to the other)
/// each end of the look across a border may be, and the share of a
/// border's points that must show the step for the border to move.
const EDGE_SIDE: f32 = 0.3;
const EDGE_FOUND: f64 = 0.8;
/// The least the two sides' colours differ (RGB distance) for the edge
/// between them to be placed by them.
const EDGE_CONTRAST: f32 = 40.0;

/// The points of a border (`mids`, on a grid `k` times the picture's)
/// moved across it to where the edge runs in the picture: where its
/// colour is half way from the one side's colour to the other's. The
/// labels a border is walked along were cleaned and smoothed, which
/// leaves them a pixel off here and there; the picture itself says where
/// its edge is to a fraction of a pixel. A point with no such place
/// within [`EDGE_REACH`] stays, and so do the ends of an open border.
fn refine(mids: &[Point], closed: bool, pic: &Picture, k: f64) -> Vec<Point> {
    let n = mids.len();
    if n < 6 || pic.w < 2 || pic.h < 2 || pic.colors.len() != pic.w * pic.h {
        return mids.to_vec();
    }
    let light = smooth(mids, 2, closed);
    // The way across the border at each point, towards its `b` side.
    let across: Vec<Vec2> = (0..n)
        .map(|i| {
            let (a, b) = if closed {
                (light[(i + n - 2) % n], light[(i + 2) % n])
            } else {
                (light[i.saturating_sub(2)], light[(i + 2).min(n - 1)])
            };
            let t = unit(b - a);
            Vec2::new(t.y, -t.x)
        })
        .collect();
    let place = |i: usize, off: f64| Point::new(mids[i].x / k, mids[i].y / k) + across[i] * off;
    // A side's colour: given, or the middle one of the picture's own a
    // little way into the side.
    let side = |given: Option<[f32; 3]>, way: f64| -> [f32; 3] {
        given.unwrap_or_else(|| {
            let step = (n / 64).max(1);
            let mut seen: Vec<[f32; 3]> = (0..n)
                .step_by(step)
                .map(|i| pic.at(place(i, way * EDGE_INTO)))
                .collect();
            std::array::from_fn(|c| {
                seen.sort_by(|x, y| x[c].total_cmp(&y[c]));
                seen[seen.len() / 2][c]
            })
        })
    };
    let (ca, cb) = (side(pic.a, -1.0), side(pic.b, 1.0));
    let ab: [f32; 3] = std::array::from_fn(|c| cb[c] - ca[c]);
    let len2: f32 = ab.iter().map(|v| v * v).sum();
    if len2 < EDGE_CONTRAST * EDGE_CONTRAST {
        return mids.to_vec();
    }
    let share = |p: Point| -> f32 {
        let c = pic.at(p);
        ((0..3).map(|q| (c[q] - ca[q]) * ab[q]).sum::<f32>() / len2).clamp(0.0, 1.0)
    };
    let steps = (2.0 * EDGE_REACH / EDGE_STEP).round() as usize;
    let mut found = 0usize;
    let mut off: Vec<f64> = (0..n)
        .map(|i| {
            let mut best: Option<f64> = None;
            let mut before = share(place(i, -EDGE_REACH));
            // No plain step from one side's colour to the other's here.
            if before > EDGE_SIDE || share(place(i, EDGE_REACH)) < 1.0 - EDGE_SIDE {
                return 0.0;
            }
            for j in 1..=steps {
                let o = -EDGE_REACH + j as f64 * EDGE_STEP;
                let here = share(place(i, o));
                if before < 0.5 && here >= 0.5 {
                    let cut = o - EDGE_STEP + EDGE_STEP * ((0.5 - before) / (here - before)) as f64;
                    if best.is_none_or(|b| cut.abs() < b.abs()) {
                        best = Some(cut);
                    }
                }
                before = here;
            }
            found += usize::from(best.is_some());
            best.unwrap_or(0.0)
        })
        .collect();
    // A border the picture shows only here and there stays as traced.
    if (found as f64) < EDGE_FOUND * n as f64 {
        return mids.to_vec();
    }
    // One point far from the two beside it found another edge.
    let raw = off.clone();
    for i in 0..n {
        let (a, b) = if closed {
            (raw[(i + n - 1) % n], raw[(i + 1) % n])
        } else if i == 0 || i == n - 1 {
            continue;
        } else {
            (raw[i - 1], raw[i + 1])
        };
        let mut three = [a, raw[i], b];
        three.sort_by(|x, y| x.total_cmp(y));
        off[i] = three[1];
    }
    (0..n)
        .map(|i| {
            // The ends of an open border are where other borders end.
            let ease = if closed {
                1.0
            } else {
                (i.min(n - 1 - i) as f64 / (2.0 * k).max(1.0)).min(1.0)
            };
            mids[i] + across[i] * (off[i] * ease * k)
        })
        .collect()
}

/// A border fitted, and how the check of it went.
#[derive(Debug, Clone)]
pub(crate) struct Fitted {
    pub path: BezPath,
    /// How many times it was fitted again more strictly (0: the first
    /// fit passed the check).
    pub level: u8,
    /// The furthest the path and the border lie apart, in points of the
    /// border's grid, corners made sharp aside.
    pub stray: f64,
}

/// Round shapes. A stretch is an arc of a circle or a level ellipse
/// when no point of it lies further than [`ARC_SLACK`] tolerances from
/// one that turns at least [`ARC_LEAST`] radians (a circle;
/// [`ARC_LEAST_OVAL`] an ellipse, which bends to fit more) and meets a
/// smooth join within [`ARC_JOIN`] radians; its cubics cover at most
/// [`ARC_PART`] radians each. An ellipse is no flatter than
/// [`OVAL_FLAT`] (the lesser radius over the greater), and one whose
/// radii differ by less than [`OVAL_ROUND`] of the greater is a circle.
const ARC_SLACK: f64 = 1.3;
const ARC_LEAST: f64 = 0.9;
const ARC_LEAST_OVAL: f64 = 2.0;
const ARC_JOIN: f64 = 0.2;
const ARC_PART: f64 = 1.95;
/// How far an end of the stretch may lie off its arc, times what its
/// other points may.
const ARC_END: f64 = 2.0;
const OVAL_FLAT: f64 = 0.3;
const OVAL_ROUND: f64 = 0.03;
/// The share of the distance allowed a circle that an ellipse is allowed.
const OVAL_NEAR: f64 = 0.5;

/// The distance (pixels) a corner's turn is measured over, and the longer
/// one it is compared with.
const TURN_NEAR: f64 = 1.5;
const TURN_FAR: f64 = 3.0;
/// A corner turns no more than this many times as much over the longer
/// distance (an arc turns twice as much), and no less than
/// [`TURN_LEAST`] times (a bump of the border turns back).
const TURN_MOST: f64 = 1.5;
const TURN_LEAST: f64 = 0.7;
/// The widest radius (pixels) of a rounding that is still a corner of the
/// art, rounded by the picture's making.
const ROUND_MOST: f64 = 1.75;
/// How much a corner must stand out: its turn (radians) times the
/// length of its shorter side (pixels).
const STANDS_OUT: f64 = 2.2;
/// The furthest (pixels) a corner made sharp may lie from the border.
const SHARP_MOST: f64 = 1.1;
/// The furthest (pixels) the two corners of a stroke's end made square
/// may lie from the border; the end is put square across the stroke
/// when its two sides turn [`END_SIDES`] (radians) or more from each
/// other and it is within [`END_SQUARE`] of square.
const END_MOST: f64 = 1.5;
const END_SIDES: f64 = 2.6;
const END_SQUARE: f64 = 0.8;
/// How far from level or upright (the sine of the angle) the end of a
/// stroke may run and be made exactly so.
const END_LEVEL: f64 = 0.45;
/// The longest bend (pixels) between two straight pieces that may be
/// made sharp.
const BEND_MOST: f64 = 10.0;
/// How far (pixels) a side of a corner may stray from its line to be
/// straight.
const SIDE_STRAIGHT: f64 = 0.2;
/// The furthest (pixels) a level or upright side's far end may be off
/// for the side to be made exactly level or upright, and the angle
/// (radians) within which it is in any case.
const AXIS_OFF: f64 = 0.3;
const AXIS_TURN: f64 = 0.03;
/// A straight piece is made level or upright when it is at least
/// [`LEVEL_LEAST`] pixels long, turned no more than [`LEVEL_TURN`]
/// (radians) from it, and neither end has to move further than
/// [`LEVEL_MOVE`] pixels.
const LEVEL_LEAST: f64 = 2.5;
const LEVEL_TURN: f64 = 0.09;
const LEVEL_MOVE: f64 = 0.8;
/// What a pixel of a cubic's length adds to its cost (a piece costs
/// one).
const CURVE_LENGTH: f64 = 1e-4;
/// The most a single cubic turns, radians.
const CUBIC_TURN: f64 = 1.95;
/// How far (radians) a line may point from the border's direction where
/// it joins a curve smoothly.
const JOIN_TURN: f64 = 0.1;
/// How much further than the tolerance a straight line may stray.
const LINE_SLACK: f64 = 1.6;
/// How far beyond the tolerance (pixels) the finished path may lie from
/// the border itself: the smoothing and the steps of the grid.
const CHECK_SLACK: f64 = 0.3;
/// The widest stroke (pixels of the picture before it was enlarged)
/// whose round end is made square; the least turn (radians) of the
/// border round it, the longest the turn may be for the stroke's width,
/// how far (pixels) a side of the stroke may stray from its line, and
/// how far back from the tip of the round end (of the stroke's width)
/// the square end is put, so the stroke covers as much.
const CAP_WIDE: f64 = 2.0;
const CAP_TURN: f64 = 2.6;
const CAP_LONG: f64 = 2.4;
const CAP_FLANK: f64 = 0.3;
const CAP_BACK: f64 = 0.1;
/// A straight run is at least this long (pixels) and no point of it
/// further than [`RUN_OFF`] from its line, where the border turns
/// [`RUN_TURN`] (radians) or more over the stretch [`RUN_PAST`] (pixels,
/// from and to) past each of its ends. Between two sharper turns
/// ([`RUN_TURN_SHORT`]) a run of [`RUN_SHORT`] pixels is one.
const RUN_LEAST: f64 = 14.0;
const RUN_OFF: f64 = 0.3;
const RUN_OFF_SHORT: f64 = 0.2;
const RUN_PAST_SHORT: (f64, f64) = (1.0, 4.5);
const RUN_TURN: f64 = 0.25;
const RUN_PAST: (f64, f64) = (2.0, 8.0);
const RUN_SHORT: f64 = 6.0;
const RUN_TURN_SHORT: f64 = 0.6;
/// The tolerance of each stricter fit of a stretch, as a share of the
/// first.
const STRICTER: [f64; 3] = [1.0, 0.6, 0.35];

fn unit(v: Vec2) -> Vec2 {
    let l = v.hypot();
    if l > 1e-12 && l.is_finite() {
        v / l
    } else {
        Vec2::ZERO
    }
}

fn angle(a: Vec2, b: Vec2) -> f64 {
    a.cross(b).atan2(a.dot(b))
}

/// `pts` blurred along the border by `passes` rounds of quarter, half,
/// quarter; an open border keeps its two ends.
fn smooth(pts: &[Point], passes: usize, closed: bool) -> Vec<Point> {
    let n = pts.len();
    let mut cur = pts.to_vec();
    if n < 3 {
        return cur;
    }
    let mut prev = cur.clone();
    for _ in 0..passes {
        std::mem::swap(&mut cur, &mut prev);
        for i in 0..n {
            let (a, c) = if closed {
                (prev[(i + n - 1) % n], prev[(i + 1) % n])
            } else if i == 0 || i == n - 1 {
                cur[i] = prev[i];
                continue;
            } else {
                (prev[i - 1], prev[i + 1])
            };
            cur[i] = Point::new(
                (a.x + 2.0 * prev[i].x + c.x) / 4.0,
                (a.y + 2.0 * prev[i].y + c.y) / 4.0,
            );
        }
    }
    cur
}

/// The points of a border and the walks along it.
struct Line<'a> {
    p: &'a [Point],
    closed: bool,
    /// The whole length round a closed border.
    total: f64,
}

impl Line<'_> {
    fn n(&self) -> usize {
        self.p.len()
    }

    /// The point `step` points on from `i` (round a closed border); none
    /// past the end of an open one.
    fn next(&self, i: usize, forwards: bool) -> Option<usize> {
        let n = self.n();
        if self.closed {
            Some(if forwards {
                (i + 1) % n
            } else {
                (i + n - 1) % n
            })
        } else if forwards {
            (i + 1 < n).then_some(i + 1)
        } else {
            i.checked_sub(1)
        }
    }

    /// The first point at least `d` along the border from `i`, and how
    /// many points on it is.
    fn along(&self, i: usize, d: f64, forwards: bool) -> Option<(usize, usize)> {
        let (mut at, mut gone, mut count) = (i, 0.0, 0usize);
        while gone < d {
            let j = self.next(at, forwards)?;
            gone += (self.p[j] - self.p[at]).hypot();
            at = j;
            count += 1;
            if count >= self.n() {
                return None;
            }
        }
        Some((at, count))
    }

    /// The turn of the border at `i` measured over `d` each way.
    fn turn(&self, i: usize, d: f64) -> Option<f64> {
        let (a, _) = self.along(i, d, false)?;
        let (b, _) = self.along(i, d, true)?;
        Some(angle(self.p[i] - self.p[a], self.p[b] - self.p[i]))
    }
}

/// The line through `pts` by least squares: a point of it, its direction
/// (towards the last point) and the furthest a point lies from it.
fn line_through(pts: &[Point]) -> Option<(Point, Vec2, f64)> {
    let n = pts.len();
    if n < 2 {
        return None;
    }
    let count = n as f64;
    let (mut cx, mut cy) = (0.0, 0.0);
    for q in pts {
        cx += q.x;
        cy += q.y;
    }
    let c = Point::new(cx / count, cy / count);
    let (mut xx, mut xy, mut yy) = (0.0, 0.0, 0.0);
    for q in pts {
        let d = *q - c;
        xx += d.x * d.x;
        xy += d.x * d.y;
        yy += d.y * d.y;
    }
    let chord = pts[n - 1] - pts[0];
    let mut dir = if n >= 3 {
        let a = 0.5 * (2.0 * xy).atan2(xx - yy);
        Vec2::new(a.cos(), a.sin())
    } else {
        unit(chord)
    };
    if dir == Vec2::ZERO {
        return None;
    }
    if dir.dot(chord) < 0.0 {
        dir = -dir;
    }
    let off = pts
        .iter()
        .map(|q| (*q - c).cross(dir).abs())
        .fold(0.0, f64::max);
    Some((c, dir, off))
}

/// Where the lines `a` and `b` (a point and a direction each) cross.
fn crossing(a: (Point, Vec2), b: (Point, Vec2)) -> Option<Point> {
    let det = a.1.cross(b.1);
    if det.abs() < 0.15 {
        return None;
    }
    let s = (b.0 - a.0).cross(b.1) / det;
    let x = a.0 + a.1 * s;
    (x.x.is_finite() && x.y.is_finite()).then_some(x)
}

/// One side of a corner: the line it runs along (a point and its
/// direction of travel along the border), whether it is straight, and
/// how many points from the corner its first point is.
#[derive(Clone, Copy)]
struct Side {
    line: (Point, Vec2),
    straight: bool,
    trim: usize,
    /// How long (points of the grid) it runs straight.
    len: f64,
}

/// The side of the corner at `c` going forwards or backwards, no further
/// than `most` points from it.
fn side_of(l: &Line, c: usize, forwards: bool, most: usize, f: &Faithful) -> Option<Side> {
    let (first, trim) = l.along(c, TURN_NEAR * f.k, forwards)?;
    if trim >= most {
        return None;
    }
    // The points of the side, from the corner on.
    let mut pts = vec![l.p[first]];
    let mut at = first;
    let room = (most - trim).min((40.0 * f.k) as usize);
    let mut lens = vec![0.0];
    for _ in 0..room {
        let Some(j) = l.next(at, forwards) else { break };
        let d = (l.p[j] - l.p[at]).hypot();
        pts.push(l.p[j]);
        lens.push(lens[lens.len() - 1] + d);
        at = j;
    }
    // Its line from as much of it as is straight: the span doubles
    // while the points stay on the line.
    let upto = |d: f64| lens.iter().position(|v| *v >= d).unwrap_or(lens.len() - 1);
    let mut span = TURN_FAR * f.k;
    let mut best: Option<(Point, Vec2, f64)> = None;
    loop {
        let m = upto(span);
        if m < 2 {
            break;
        }
        let Some(found) = line_through(&pts[..=m]) else {
            break;
        };
        if found.2 > SIDE_STRAIGHT * f.k {
            break;
        }
        best = Some((found.0, found.1, lens[m]));
        if m + 1 >= pts.len() {
            break;
        }
        span *= 1.6;
    }
    let side = match best {
        Some((point, dir, len)) => {
            // Level or upright when it is nearly so.
            let near = |off: f64| off.abs() * len <= AXIS_OFF * f.k || off.abs() <= AXIS_TURN;
            let dir = if near(dir.y) && dir.x.abs() > 0.9 {
                Vec2::new(dir.x.signum(), 0.0)
            } else if near(dir.x) && dir.y.abs() > 0.9 {
                Vec2::new(0.0, dir.y.signum())
            } else {
                dir
            };
            Side {
                line: (point, dir),
                straight: true,
                trim,
                len,
            }
        }
        None => {
            // A curved side: the way it leaves the corner.
            let m = upto(TURN_NEAR * f.k).max(1).min(pts.len() - 1);
            let dir = unit(pts[m] - pts[0]);
            if dir == Vec2::ZERO {
                return None;
            }
            Side {
                line: (pts[0], dir),
                straight: false,
                trim,
                len: TURN_NEAR * f.k,
            }
        }
    };
    // Its direction is the border's direction of travel.
    Some(if forwards {
        side
    } else {
        Side {
            line: (side.line.0, -side.line.1),
            ..side
        }
    })
}

/// A fixed point of the fit: a corner, an end of an open border, or the
/// place the fit of a closed border without corners starts.
#[derive(Clone, Copy)]
struct Stop {
    at: usize,
    node: Point,
    /// The points before and after it left out of the stretches beside
    /// it (the rounding of a corner).
    back: usize,
    fwd: usize,
    /// The direction of travel arriving at it and leaving it.
    arrive: Vec2,
    leave: Vec2,
    /// A corner made sharp: its node lies off the border.
    sharp: bool,
}

/// The corners of the border, in order.
fn corners_of(l: &Line, f: &Faithful, round_most: f64) -> Vec<Stop> {
    let n = l.n();
    let mut out: Vec<Stop> = Vec::new();
    let reach = if l.closed { l.total / 8.0 } else { f64::MAX };
    let far = (TURN_FAR * f.k).min(reach);
    let near = far * TURN_NEAR / TURN_FAR;
    if n < 8 || far < 1.5 {
        return out;
    }
    let turns: Vec<f64> = (0..n).map(|i| l.turn(i, near).unwrap_or(0.0)).collect();
    let mut found: Vec<usize> = Vec::new();
    for i in 0..n {
        let t = turns[i].abs();
        if t < f.corner {
            continue;
        }
        // The sharpest point within the near distance either way.
        let mut top = true;
        for forwards in [false, true] {
            let (mut at, mut gone) = (i, 0.0);
            while gone < near && top {
                let Some(j) = l.next(at, forwards) else { break };
                gone += (l.p[j] - l.p[at]).hypot();
                at = j;
                let other = turns[j].abs();
                if other > t || (other == t && j < i) {
                    top = false;
                }
            }
        }
        if !top {
            continue;
        }
        let Some(wide) = l.turn(i, far) else { continue };
        if wide * turns[i] <= 0.0 {
            continue;
        }
        let ratio = wide.abs() / t;
        if !(TURN_LEAST..=TURN_MOST).contains(&ratio) {
            continue;
        }
        found.push(i);
    }
    let count = found.len();
    for (q, &c) in found.iter().enumerate() {
        // No further than the corners before and after it.
        let gap = |a: usize, b: usize| {
            if l.closed {
                (b + n - a) % n
            } else {
                b.saturating_sub(a)
            }
        };
        let before = if count > 1 || !l.closed {
            if q == 0 && !l.closed {
                c
            } else {
                gap(found[(q + count - 1) % count], c)
            }
        } else {
            n
        };
        let after = if count > 1 || !l.closed {
            if q + 1 == count && !l.closed {
                n - 1 - c
            } else {
                gap(c, found[(q + 1) % count])
            }
        } else {
            n
        };
        let before = if before == 0 { n } else { before };
        let after = if after == 0 { n } else { after };
        let apex = l.p[c];
        let plain = |arrive: Vec2, leave: Vec2| Stop {
            at: c,
            node: apex,
            back: 0,
            fwd: 0,
            arrive,
            leave,
            sharp: false,
        };
        // The way the border runs just before and after the corner.
        let short = |forwards: bool| -> Vec2 {
            match l.along(c, near, forwards) {
                Some((j, _)) if forwards => unit(l.p[j] - apex),
                Some((j, _)) => unit(apex - l.p[j]),
                None => Vec2::ZERO,
            }
        };
        let (sa, sb) = (
            side_of(l, c, false, before / 2 + 1, f),
            side_of(l, c, true, after / 2 + 1, f),
        );
        let stop = match (sa, sb) {
            (Some(a), Some(b)) if round_most > 0.0 => {
                let total = angle(a.line.1, b.line.1).abs();
                // A slight turn between two short sides is a ripple of
                // the border, no corner.
                if total * (TURN_NEAR * f.k + a.len.min(b.len)) < STANDS_OUT * f.k {
                    continue;
                }
                let half = (total / 2.0).cos().max(0.2);
                let most =
                    (round_most * f.k * (1.0 / half - 1.0) + 0.25 * f.k).min(SHARP_MOST * f.k);
                let first_after = l.along(c, near, true).map(|x| l.p[x.0]);
                let last_before = l.along(c, near, false).map(|x| l.p[x.0]);
                let x = crossing(a.line, b.line).filter(|x| {
                    (*x - apex).hypot() <= most
                        && first_after.is_some_and(|q| (q - *x).dot(b.line.1) > 0.0)
                        && last_before.is_some_and(|q| (*x - q).dot(a.line.1) > 0.0)
                });
                match x {
                    Some(x) => Stop {
                        at: c,
                        node: x,
                        back: a.trim,
                        fwd: b.trim,
                        arrive: a.line.1,
                        leave: b.line.1,
                        sharp: true,
                    },
                    // Rounded wider than a corner is: the border
                    // turns smoothly there.
                    None => continue,
                }
            }
            (Some(_), Some(_)) => continue,
            _ => plain(short(false), short(true)),
        };
        if stop.arrive == Vec2::ZERO || stop.leave == Vec2::ZERO {
            continue;
        }
        out.push(stop);
    }
    out
}

/// A cubic through `pts` leaving along `t1` and arriving back along `t2`
/// (unit vectors pointing into the curve from its two ends), and the
/// furthest a point lies from it.
fn cubic_through(pts: &[Point], t1: Vec2, t2: Vec2) -> (CubicBez, f64) {
    let n = pts.len();
    let (p0, p3) = (pts[0], pts[n - 1]);
    let dist = (p3 - p0).hypot();
    let third = CubicBez::new(p0, p0 + t1 * (dist / 3.0), p3 + t2 * (dist / 3.0), p3);
    if n <= 2 {
        return (third, 0.0);
    }
    let mut u = vec![0.0; n];
    for i in 1..n {
        u[i] = u[i - 1] + (pts[i] - pts[i - 1]).hypot();
    }
    let total = u[n - 1];
    if total < 1e-12 {
        return (third, 0.0);
    }
    for v in u.iter_mut() {
        *v /= total;
    }
    let generate = |u: &[f64]| -> CubicBez {
        let (mut c00, mut c01, mut c11, mut x0, mut x1) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for (p, &t) in pts.iter().zip(u) {
            let mt = 1.0 - t;
            let b0 = mt * mt * mt;
            let b1 = 3.0 * t * mt * mt;
            let b2 = 3.0 * t * t * mt;
            let b3 = t * t * t;
            let a1 = t1 * b1;
            let a2 = t2 * b2;
            c00 += a1.dot(a1);
            c01 += a1.dot(a2);
            c11 += a2.dot(a2);
            let tmp = p.to_vec2() - (p0.to_vec2() * (b0 + b1) + p3.to_vec2() * (b2 + b3));
            x0 += a1.dot(tmp);
            x1 += a2.dot(tmp);
        }
        let det = c00 * c11 - c01 * c01;
        let (mut l1, mut l2) = if det.abs() > 1e-12 {
            ((x0 * c11 - x1 * c01) / det, (c00 * x1 - c01 * x0) / det)
        } else {
            (0.0, 0.0)
        };
        let eps = 1e-3 * dist;
        if !(l1.is_finite() && l2.is_finite()) || l1 < eps || l2 < eps || l1.max(l2) > 2.0 * total {
            l1 = dist / 3.0;
            l2 = dist / 3.0;
        }
        CubicBez::new(p0, p0 + t1 * l1, p3 + t2 * l2, p3)
    };
    let worst = |c: &CubicBez, u: &[f64]| -> f64 {
        (1..n - 1)
            .map(|i| (c.eval(u[i]) - pts[i]).hypot2())
            .fold(0.0, f64::max)
            .sqrt()
    };
    let mut curve = generate(&u);
    let mut err = worst(&curve, &u);
    for _ in 0..4 {
        if err < 1e-3 {
            break;
        }
        let d1 = curve.deriv();
        let d2 = d1.deriv();
        for i in 1..n - 1 {
            let t = u[i];
            let diff = curve.eval(t) - pts[i];
            let q1 = d1.eval(t).to_vec2();
            let q2 = d2.eval(t).to_vec2();
            let den = q1.dot(q1) + diff.dot(q2);
            if den.abs() > 1e-12 {
                u[i] = (t - diff.dot(q1) / den).clamp(0.0, 1.0);
            }
        }
        let next = generate(&u);
        let e = worst(&next, &u);
        if e < err {
            (curve, err) = (next, e);
        } else {
            break;
        }
    }
    (curve, err)
}

/// A piece of the fitted path, ending at `to`.
#[derive(Clone, Copy)]
enum Piece {
    Line(Point),
    Cubic(Point, Point, Point),
}

impl Piece {
    fn to(&self) -> Point {
        match *self {
            Piece::Line(p) | Piece::Cubic(_, _, p) => p,
        }
    }
}

/// The places a piece of the stretch `d` may end, about a pixel apart
/// (indices into `d`, its two ends among them), and how far along the
/// stretch each point lies.
fn candidates(d: &[Point], f: &Faithful) -> (Vec<usize>, Vec<f64>) {
    let m = d.len();
    let mut s = vec![0.0; m];
    for i in 1..m {
        s[i] = s[i - 1] + (d[i] - d[i - 1]).hypot();
    }
    let mut cand: Vec<usize> = vec![0];
    let gap = f.k.max(1.0);
    for i in 1..m.saturating_sub(1) {
        if s[i] - s[cand[cand.len() - 1]] >= gap && s[m - 1] - s[i] >= gap {
            cand.push(i);
        }
    }
    if m > 1 {
        cand.push(m - 1);
    }
    (cand, s)
}

/// The furthest of `q` candidates a piece from candidate `a` may reach,
/// by `ok`: doubling steps, then halving.
fn furthest(a: usize, q: usize, ok: &dyn Fn(usize, usize) -> bool) -> usize {
    if a + 1 >= q || !ok(a, a + 1) {
        return a;
    }
    let (mut good, mut step) = (a + 1, 1usize);
    let mut bad = q;
    while good + step < q {
        if ok(a, good + step) {
            good += step;
            step *= 2;
        } else {
            bad = good + step;
            break;
        }
    }
    if bad == q && good < q - 1 {
        if ok(a, q - 1) {
            return q - 1;
        }
        bad = q - 1;
    }
    while bad - good > 1 {
        let mid = (good + bad) / 2;
        if ok(a, mid) {
            good = mid;
        } else {
            bad = mid;
        }
    }
    good
}

/// How far the points of `d` between `a` and `b` lie from the chord
/// joining the two.
fn off_chord(d: &[Point], a: usize, b: usize) -> f64 {
    let chord = d[b] - d[a];
    let len = chord.hypot();
    if len < 1e-9 {
        return d[a..=b]
            .iter()
            .map(|q| (*q - d[a]).hypot())
            .fold(0.0, f64::max);
    }
    d[a + 1..b]
        .iter()
        .map(|q| ((*q - d[a]).cross(chord) / len).abs())
        .fold(0.0, f64::max)
}

/// The fewest lines and cubics through `d` (its first and last points
/// are its fixed ends), leaving along `leave` and arriving along
/// `arrive` (directions of travel), no point of it further than the
/// tolerance from them.
fn fit_gap(
    d: &[Point],
    leave: Vec2,
    arrive: Vec2,
    smooth_ends: (bool, bool),
    tol: f64,
    f: &Faithful,
    out: &mut Vec<Piece>,
) {
    let m = d.len();
    if m < 2 {
        return;
    }
    let line_tol = tol * LINE_SLACK;
    let off_chord = |a: usize, b: usize| off_chord(d, a, b);
    if m == 2 || (off_chord(0, m - 1) <= line_tol && !(smooth_ends.0 && smooth_ends.1)) {
        let chord = unit(d[m - 1] - d[0]);
        let joins = |smooth: bool, t: Vec2| !smooth || angle(chord, t).abs() <= JOIN_TURN;
        if m == 2 || (joins(smooth_ends.0, leave) && joins(smooth_ends.1, arrive)) {
            out.push(Piece::Line(d[m - 1]));
            return;
        }
    }
    let (cand, s) = candidates(d, f);
    let q = cand.len();
    // The border's direction of travel at each.
    let reach = 1.5 * f.k;
    let tan: Vec<Vec2> = (0..q)
        .map(|c| {
            if c == 0 {
                return leave;
            }
            if c == q - 1 {
                return arrive;
            }
            let i = cand[c];
            let (mut a, mut b) = (i, i);
            while a > 0 && s[i] - s[a] < reach {
                a -= 1;
            }
            while b < m - 1 && s[b] - s[i] < reach {
                b += 1;
            }
            unit(d[b] - d[a])
        })
        .collect();
    // How much the border turns from the first candidate to each.
    let mut bend = vec![0.0; q];
    for c in 1..q {
        bend[c] = bend[c - 1] + angle(tan[c - 1], tan[c]).abs();
    }
    let line_ok = |a: usize, b: usize| off_chord(cand[a], cand[b]) <= line_tol;
    let cubic_ok = |a: usize, b: usize| {
        bend[b] - bend[a] <= CUBIC_TURN
            && cubic_through(&d[cand[a]..=cand[b]], tan[a], -tan[b]).1 <= tol
    };
    let reach_line: Vec<usize> = (0..q).map(|a| furthest(a, q, &line_ok)).collect();
    let reach_cubic: Vec<usize> = (0..q).map(|a| furthest(a, q, &cubic_ok)).collect();
    // Whether a line joins the border's direction at a candidate: at a
    // fixed end that is a corner any line does.
    let joins = |a: usize, b: usize, at: usize| -> bool {
        if (at == 0 && !smooth_ends.0) || (at == q - 1 && !smooth_ends.1) {
            return true;
        }
        angle(unit(d[cand[b]] - d[cand[a]]), tan[at]).abs() <= JOIN_TURN
    };
    // The fewest pieces to each candidate, by the kind of the last: a
    // cubic, a line joining the border's direction there, a line that
    // does not (a corner too slight to be found as one).
    const KINDS: usize = 3;
    const NONE: f64 = f64::MAX;
    let mut best = vec![[NONE; KINDS]; q];
    let mut from = vec![[(0usize, 0usize); KINDS]; q];
    best[0] = [0.0, NONE, NONE];
    // A line costs a little less than a cubic, so a straight stretch is
    // a line; a slight corner costs a little more than a smooth join.
    for a in 0..q {
        for kind in 0..KINDS {
            let base = best[a][kind];
            if base == NONE {
                continue;
            }
            // A cubic from here: not after a line that does not join.
            if kind != 2 {
                for b in a + 1..=reach_cubic[a] {
                    // Of two ways with as many pieces, the one with the
                    // shorter curves: what is straight is a line to its
                    // end, not the flat start of a curve.
                    let long = (s[cand[b]] - s[cand[a]]) / f.k;
                    let cost = base + 1.0 + (CURVE_LENGTH * long).min(0.4);
                    if cost < best[b][0] {
                        best[b][0] = cost;
                        from[b][0] = (a, kind);
                    }
                }
            }
            for b in a + 1..=reach_line[a] {
                // After a cubic the line must go on the border's way;
                // after a line it may turn (a slight corner).
                let start_joins = joins(a, b, a);
                if kind == 0 && a > 0 && !start_joins {
                    continue;
                }
                let turn = if kind != 0 && !start_joins { 0.25 } else { 0.0 };
                let to = if joins(a, b, b) { 1 } else { 2 };
                let cost = base + 0.98 + turn;
                if cost < best[b][to] {
                    best[b][to] = cost;
                    from[b][to] = (a, kind);
                }
            }
        }
    }
    let last = q - 1;
    let Some(mut kind) = (0..KINDS)
        .filter(|k| best[last][*k] != NONE)
        .min_by(|x, y| best[last][*x].total_cmp(&best[last][*y]))
    else {
        // Nothing reaches the end (it cannot be: a line always reaches
        // the next candidate): the points themselves.
        for p in &d[1..] {
            out.push(Piece::Line(*p));
        }
        return;
    };
    let mut cuts: Vec<(usize, usize, bool)> = Vec::new();
    let mut b = last;
    while b > 0 {
        let (a, before) = from[b][kind];
        cuts.push((a, b, kind != 0));
        b = a;
        kind = before;
    }
    cuts.reverse();
    for (i, &(a, b, line)) in cuts.iter().enumerate() {
        if line {
            out.push(Piece::Line(d[cand[b]]));
            continue;
        }
        // A cubic beside a line leaves along the line.
        let beside = |j: Option<&(usize, usize, bool)>, own: Vec2| -> Vec2 {
            match j {
                Some(&(x, y, true)) => {
                    let t = unit(d[cand[y]] - d[cand[x]]);
                    if t != Vec2::ZERO && angle(t, own).abs() <= 2.0 * JOIN_TURN {
                        t
                    } else {
                        own
                    }
                }
                _ => own,
            }
        };
        let t1 = beside(i.checked_sub(1).and_then(|j| cuts.get(j)), tan[a]);
        let t2 = beside(cuts.get(i + 1), tan[b]);
        let pts = &d[cand[a]..=cand[b]];
        let (mut c, err) = cubic_through(pts, t1, -t2);
        if err > tol * 1.25 {
            c = cubic_through(pts, tan[a], -tan[b]).0;
        }
        out.push(Piece::Cubic(c.p1, c.p2, c.p3));
    }
}

/// The straight pieces through `pts` (from the first to the last) that
/// stay within `eps` of them.
fn plain(pts: &[Point], eps: f64, out: &mut Vec<Piece>) {
    let n = pts.len();
    if n < 2 {
        return;
    }
    let mut keep = vec![false; n];
    keep[n - 1] = true;
    let mut stack = vec![(0usize, n - 1)];
    while let Some((a, b)) = stack.pop() {
        let chord = pts[b] - pts[a];
        let len = chord.hypot();
        let mut worst = (eps, None);
        for j in a + 1..b {
            let d = pts[j] - pts[a];
            let off = if len > 1e-9 {
                let t = (d.dot(chord) / (len * len)).clamp(0.0, 1.0);
                (d - chord * t).hypot()
            } else {
                d.hypot()
            };
            if off > worst.0 {
                worst = (off, Some(j));
            }
        }
        if let Some(j) = worst.1 {
            keep[j] = true;
            stack.push((a, j));
            stack.push((j, b));
        }
    }
    for i in 1..n {
        if keep[i] {
            out.push(Piece::Line(pts[i]));
        }
    }
}

fn path_of(start: Point, pieces: &[Piece], closed: bool) -> BezPath {
    let mut path = BezPath::new();
    path.move_to(start);
    for p in pieces {
        match *p {
            Piece::Line(to) => path.line_to(to),
            Piece::Cubic(c1, c2, to) => path.curve_to(c1, c2, to),
        }
    }
    if closed {
        path.close_path();
    }
    path
}

/// Make the short bends between two straight pieces sharp where a sharp
/// shape fits them: a run of cubics no longer than [`BEND_MOST`] pixels
/// between two lines becomes the corner where the two lines cross, or,
/// where they run opposite ways (the end of a stroke), a straight end
/// with two corners, when no corner put lies further than
/// [`SHARP_MOST`] pixels from the bend. The rounding the picture's
/// making leaves on a corner is that small; a bend rounded wider is the
/// art's and stays. Gives the path's first point and its pieces.
fn sharpen_bends(
    start: Point,
    mut pieces: Vec<Piece>,
    closed: bool,
    f: &Faithful,
) -> (Point, Vec<Piece>) {
    // The lines long enough to be the sides of a bend.
    let long_lines = |start: Point, pieces: &[Piece]| -> Vec<bool> {
        let mut at = start;
        pieces
            .iter()
            .map(|p| {
                let from = at;
                at = p.to();
                matches!(p, Piece::Line(_)) && (at - from).hypot() >= LEVEL_LEAST * f.k
            })
            .collect()
    };
    let mut start = start;
    if closed {
        // From the start of a line on, so no bend runs over the end.
        let Some(first) = long_lines(start, &pieces).iter().position(|l| *l) else {
            return (start, pieces);
        };
        if first > 0 {
            start = pieces[first - 1].to();
            pieces.rotate_left(first);
        }
    }
    let long = long_lines(start, &pieces);
    let is_line = |q: usize| long[q];
    let m = pieces.len();
    let node = |i: usize| if i == 0 { start } else { pieces[i - 1].to() };
    // (first cubic, last cubic, first corner, second corner).
    let mut made: Vec<(usize, usize, Point, Option<Point>)> = Vec::new();
    let mut i = 1;
    while i < m {
        if is_line(i) || !is_line(i - 1) {
            i += 1;
            continue;
        }
        let mut j = i;
        while j + 1 < m && !is_line(j + 1) {
            j += 1;
        }
        // The line after the run: the first piece again round a closed
        // border.
        let after = if j + 1 < m {
            Some((node(j + 1), pieces[j + 1].to()))
        } else if closed {
            Some((node(0), pieces[0].to()))
        } else {
            None
        };
        let run = (i, j);
        i = j + 1;
        let Some((b0, b1)) = after else { continue };
        let (a0, a1) = (node(run.0 - 1), node(run.0));
        let (da, db) = (unit(a1 - a0), unit(b1 - b0));
        if da == Vec2::ZERO || db == Vec2::ZERO {
            continue;
        }
        // The bend's points.
        let mut pts: Vec<Point> = vec![a1];
        let mut len = 0.0;
        for q in run.0..=run.1 {
            let c = match pieces[q] {
                Piece::Cubic(c1, c2, to) => CubicBez::new(node(q), c1, c2, to),
                Piece::Line(to) => {
                    let from = node(q);
                    CubicBez::new(from, from.lerp(to, 1.0 / 3.0), from.lerp(to, 2.0 / 3.0), to)
                }
            };
            for t in 1..=12 {
                let p = c.eval(t as f64 / 12.0);
                len += (p - pts[pts.len() - 1]).hypot();
                pts.push(p);
            }
        }
        if len > BEND_MOST * f.k {
            continue;
        }
        let near = |x: Point| {
            pts.iter()
                .map(|q| (*q - x).hypot())
                .fold(f64::MAX, f64::min)
        };
        let most = SHARP_MOST * f.k;
        let turn = angle(da, db).abs();
        // Where the first line may end and the second start: ahead of
        // the one's start and before the other's end.
        let fits = |x: Point, y: Point| {
            (x - a0).dot(da) > 0.5 * f.k
                && (b1 - y).dot(db) > 0.5 * f.k
                && near(x) <= most
                && near(y) <= most
        };
        if (0.5..=2.5).contains(&turn) {
            if let Some(x) = crossing((a1, da), (b0, db)).filter(|x| fits(*x, *x)) {
                made.push((run.0, run.1, x, None));
                continue;
            }
        }
        if turn >= 1.9 {
            // The end of a stroke: its straight end runs as the middle
            // of the bend does, through the bend's furthest point.
            let out = unit(da - db);
            let (lo, hi) = (pts.len() * 3 / 10, pts.len() * 7 / 10);
            let Some((_, dir, _)) = line_through(&pts[lo..=hi.max(lo + 1).min(pts.len() - 1)])
            else {
                continue;
            };
            let Some(tip) = pts
                .iter()
                .copied()
                .max_by(|x, y| x.to_vec2().dot(out).total_cmp(&y.to_vec2().dot(out)))
            else {
                continue;
            };
            // Level or upright when it is nearly so (the foot of a
            // slanting stroke stands on its line), or else square across
            // a stroke whose two sides run alongside.
            let across = Vec2::new(-out.y, out.x);
            let across = if across.dot(dir) < 0.0 {
                -across
            } else {
                across
            };
            let dir = if turn >= END_SIDES && angle(across, dir).abs() <= END_SQUARE {
                across
            } else {
                dir
            };
            let dir = if dir.y.abs() <= END_LEVEL {
                Vec2::new(dir.x.signum(), 0.0)
            } else if dir.x.abs() <= END_LEVEL {
                Vec2::new(0.0, dir.y.signum())
            } else {
                dir
            };
            let most = END_MOST * f.k;
            let fits = |x: Point, y: Point| {
                (x - a0).dot(da) > 0.5 * f.k
                    && (b1 - y).dot(db) > 0.5 * f.k
                    && near(x) <= most
                    && near(y) <= most
            };
            let (Some(x), Some(y)) = (
                crossing((a1, da), (tip, dir)),
                crossing((tip, dir), (b0, db)),
            ) else {
                continue;
            };
            if fits(x, y) && (y - x).dot(dir) > 0.5 * f.k {
                made.push((run.0, run.1, x, Some(y)));
            }
        }
    }
    if made.is_empty() {
        return (start, pieces);
    }
    let mut out: Vec<Piece> = Vec::with_capacity(m);
    let mut next = made.iter().peekable();
    let mut q = 0;
    while q < m {
        match next.peek() {
            Some(&&(first, last, x, y)) if q + 1 == first => {
                // The line before the bend ends at the corner.
                out.push(Piece::Line(x));
                if let Some(y) = y {
                    out.push(Piece::Line(y));
                }
                q = last + 1;
                next.next();
            }
            _ => {
                out.push(pieces[q]);
                q += 1;
            }
        }
    }
    // Round a closed border the last corner is where the first line
    // starts.
    if closed {
        if let Some(last) = out.last() {
            start = last.to();
        }
    }
    (start, out)
}

/// Make the straight pieces that are nearly level or upright exactly so:
/// both ends of such a line move to the line's mean height (or place
/// across), no further than [`LEVEL_MOVE`] pixels each, and the curves
/// joining there move with them. The two ends of an open border stay
/// (other borders end there too). Gives the path's first point, which
/// may have moved.
fn level_lines(start: Point, pieces: &mut [Piece], closed: bool, f: &Faithful) -> Point {
    let m = pieces.len();
    if m == 0 {
        return start;
    }
    // The nodes: the start, then the end of each piece; round a closed
    // border the last is the first again.
    let node = |i: usize, pieces: &[Piece]| if i == 0 { start } else { pieces[i - 1].to() };
    let slot = |i: usize| if closed && i == m { 0 } else { i };
    let count = if closed { m } else { m + 1 };
    let fixed = |i: usize| !closed && (i == 0 || i == m);
    // What each node is asked to be at, across and down.
    let mut want: Vec<(Option<f64>, Option<f64>)> = vec![(None, None); count];
    for i in 0..m {
        if !matches!(pieces[i], Piece::Line(_)) {
            continue;
        }
        let (a, b) = (node(i, pieces), node(i + 1, pieces));
        let d = b - a;
        let (ia, ib) = (slot(i), slot(i + 1));
        let near = |off: f64, len: f64| {
            len >= LEVEL_LEAST * f.k
                && off.abs() <= 2.0 * LEVEL_MOVE * f.k
                && off.abs() <= LEVEL_TURN * len
        };
        if near(d.y, d.x.abs()) {
            let y = match (fixed(ia), fixed(ib)) {
                (true, _) => a.y,
                (_, true) => b.y,
                _ => (a.y + b.y) / 2.0,
            };
            for j in [ia, ib] {
                want[j].1 = Some(match want[j].1 {
                    Some(v) => (v + y) / 2.0,
                    None => y,
                });
            }
        } else if near(d.x, d.y.abs()) {
            let x = match (fixed(ia), fixed(ib)) {
                (true, _) => a.x,
                (_, true) => b.x,
                _ => (a.x + b.x) / 2.0,
            };
            for j in [ia, ib] {
                want[j].0 = Some(match want[j].0 {
                    Some(v) => (v + x) / 2.0,
                    None => x,
                });
            }
        }
    }
    let moved: Vec<Vec2> = (0..count)
        .map(|j| {
            if fixed(j) {
                return Vec2::ZERO;
            }
            let at = node(j, pieces);
            Vec2::new(
                want[j].0.map_or(0.0, |x| x - at.x),
                want[j].1.map_or(0.0, |y| y - at.y),
            )
        })
        .collect();
    for i in 0..m {
        let (from, to) = (moved[slot(i)], moved[slot(i + 1)]);
        pieces[i] = match pieces[i] {
            Piece::Line(p) => Piece::Line(p + to),
            Piece::Cubic(c1, c2, p) => Piece::Cubic(c1 + from, c2 + to, p + to),
        };
    }
    start + moved[0]
}

/// A straight run of a stretch: the first and last of its points and
/// the direction it runs in.
#[derive(Clone, Copy)]
struct Run {
    a: usize,
    b: usize,
    from: Point,
    to: Point,
    dir: Vec2,
}

/// The straight runs of the stretch `d` (the side of a stem, the bar of
/// a letter): at least [`RUN_LEAST`] pixels within [`RUN_OFF`] of one
/// line, the border turning away past each end. A wide arc, which is as
/// straight over a short way, turns no faster past the run than along
/// it and is no run. `corner` says which ends of the stretch are corners
/// (a turn by themselves).
fn straight_runs(d: &[Point], corner: (bool, bool), f: &Faithful) -> Vec<Run> {
    let m = d.len();
    let mut runs = Vec::new();
    if m < 3 {
        return runs;
    }
    let mut s = vec![0.0; m];
    for i in 1..m {
        s[i] = s[i - 1] + (d[i] - d[i - 1]).hypot();
    }
    let off = RUN_OFF * f.k;
    let ok = |a: usize, b: usize| line_through(&d[a..=b]).is_some_and(|l| l.2 <= off);
    // The turn of the border past the end `e` of a run along `dir`
    // (`forwards` past its last point), none when the stretch ends in a
    // corner before it.
    let turn_past = |e: usize, dir: Vec2, forwards: bool, past: (f64, f64)| -> Option<f64> {
        let (near, far) = (past.0 * f.k, past.1 * f.k);
        let gone = |i: usize| (s[i] - s[e]).abs();
        let step = |i: usize| -> Option<usize> {
            if forwards {
                (i + 1 < m).then_some(i + 1)
            } else {
                i.checked_sub(1)
            }
        };
        let mut p = e;
        while gone(p) < near {
            p = step(p)?;
        }
        let mut q = p;
        while gone(q) < far {
            match step(q) {
                Some(j) => q = j,
                None => break,
            }
        }
        if q == p {
            return None;
        }
        let way = if forwards { d[q] - d[p] } else { d[p] - d[q] };
        Some(angle(dir, way).abs())
    };
    let mut a = 0;
    while a + 2 < m {
        // The furthest the points from `a` stay on one line.
        let b = furthest_from(a, m, &ok);
        if s[b] - s[a] < RUN_SHORT * f.k {
            a += 1;
            continue;
        }
        // The line of its middle, and the points about the middle that
        // lie on it: the run's ends do not lean into the bends.
        let (lo, hi) = (a + (b - a) / 5, b - (b - a) / 5);
        let Some((c, dir, _)) = line_through(&d[lo..=hi]) else {
            a += 1;
            continue;
        };
        let on = |i: usize| (d[i] - c).cross(dir).abs() <= 0.8 * off;
        let (mut x, mut y) = (lo, hi);
        while x > 0 && on(x - 1) {
            x -= 1;
        }
        while y + 1 < m && on(y + 1) {
            y += 1;
        }
        // A run that ends beside a corner ends at it.
        let reach = RUN_PAST.0 * f.k;
        if corner.0 && s[x] <= reach {
            x = 0;
        }
        if corner.1 && s[m - 1] - s[y] <= reach {
            y = m - 1;
        }
        let long = (s[y] - s[x]) / f.k;
        let turns = |least: f64, past: (f64, f64)| {
            let at = |e: usize, end: usize, is_corner: bool, forwards: bool| {
                (e == end && is_corner)
                    || turn_past(e, dir, forwards, past).is_some_and(|t| t >= least)
            };
            at(x, 0, corner.0, false) && at(y, m - 1, corner.1, true)
        };
        let tight = (x..=y).all(|i| (d[i] - c).cross(dir).abs() <= RUN_OFF_SHORT * f.k);
        if (long >= RUN_LEAST && turns(RUN_TURN, RUN_PAST))
            || (long >= RUN_SHORT && tight && turns(RUN_TURN_SHORT, RUN_PAST_SHORT))
        {
            let foot = |q: Point| c + dir * (q - c).dot(dir);
            let from = if x == 0 { d[0] } else { foot(d[x]) };
            let to = if y == m - 1 { d[m - 1] } else { foot(d[y]) };
            let dir = unit(to - from);
            if dir != Vec2::ZERO {
                runs.push(Run {
                    a: x,
                    b: y,
                    from,
                    to,
                    dir,
                });
            }
            a = y.max(a + 1);
        } else {
            a = (a + ((b - a) / 4).max(1)).min(m);
        }
    }
    runs
}

/// The last point from `a` (of `m`) that `ok` holds to: doubling steps,
/// then halving.
fn furthest_from(a: usize, m: usize, ok: &dyn Fn(usize, usize) -> bool) -> usize {
    let mut good = a + 1;
    let mut step = 2usize;
    let mut bad = m;
    while good + step < m {
        if ok(a, good + step) {
            good += step;
            step *= 2;
        } else {
            bad = good + step;
            break;
        }
    }
    if bad == m {
        if ok(a, m - 1) {
            return m - 1;
        }
        bad = m - 1;
    }
    while bad - good > 1 {
        let mid = (good + bad) / 2;
        if ok(a, mid) {
            good = mid;
        } else {
            bad = mid;
        }
    }
    good
}

/// An ellipse whose axes are level and upright (a circle when its two
/// radii are the same): the shape a round part of the art is drawn
/// with, where a free curve would copy the small unevenness of the
/// picture instead.
#[derive(Debug, Clone, Copy)]
struct Oval {
    c: Point,
    rx: f64,
    ry: f64,
}

/// The solution of `m x = v` by elimination, none when `m` is singular.
fn solve<const N: usize>(mut m: [[f64; N]; N], mut v: [f64; N]) -> Option<[f64; N]> {
    for i in 0..N {
        let p = (i..N).max_by(|a, b| m[*a][i].abs().total_cmp(&m[*b][i].abs()))?;
        if m[p][i].abs() < 1e-12 || !m[p][i].is_finite() {
            return None;
        }
        m.swap(i, p);
        v.swap(i, p);
        for r in i + 1..N {
            let g = m[r][i] / m[i][i];
            for c in i..N {
                m[r][c] -= g * m[i][c];
            }
            v[r] -= g * v[i];
        }
    }
    let mut x = [0.0; N];
    for i in (0..N).rev() {
        let mut s = v[i];
        for c in i + 1..N {
            s -= m[i][c] * x[c];
        }
        x[i] = s / m[i][i];
    }
    x.iter().all(|a| a.is_finite()).then_some(x)
}

impl Oval {
    /// The circle (`round`) or level ellipse nearest `pts` by least
    /// squares of its equation, none when they do not say one.
    fn through(pts: &[Point], round: bool) -> Option<Oval> {
        let n = pts.len();
        if n < 6 {
            return None;
        }
        let mut g = Vec2::ZERO;
        for p in pts {
            g += p.to_vec2();
        }
        g /= n as f64;
        let s = pts
            .iter()
            .map(|p| (p.to_vec2() - g).hypot())
            .fold(0.0, f64::max);
        if s < 1e-9 || !s.is_finite() {
            return None;
        }
        let local = |p: &Point| ((p.x - g.x) / s, (p.y - g.y) / s);
        let (cx, cy, rx, ry) = if round {
            // x2 + y2 = a x + b y + c
            let (mut m, mut v) = ([[0.0; 3]; 3], [0.0; 3]);
            for p in pts {
                let (x, y) = local(p);
                let (row, rhs) = ([x, y, 1.0], x * x + y * y);
                for i in 0..3 {
                    for j in 0..3 {
                        m[i][j] += row[i] * row[j];
                    }
                    v[i] += row[i] * rhs;
                }
            }
            let [a, b, c] = solve(m, v)?;
            let (cx, cy) = (a / 2.0, b / 2.0);
            let r2 = c + cx * cx + cy * cy;
            if r2 <= 0.0 {
                return None;
            }
            (cx, cy, r2.sqrt(), r2.sqrt())
        } else {
            // x2 + p y2 = a x + b y + c
            let (mut m, mut v) = ([[0.0; 4]; 4], [0.0; 4]);
            for q in pts {
                let (x, y) = local(q);
                let (row, rhs) = ([-y * y, x, y, 1.0], x * x);
                for i in 0..4 {
                    for j in 0..4 {
                        m[i][j] += row[i] * row[j];
                    }
                    v[i] += row[i] * rhs;
                }
            }
            let [p, a, b, c] = solve(m, v)?;
            if !(OVAL_FLAT * OVAL_FLAT..=1.0 / (OVAL_FLAT * OVAL_FLAT)).contains(&p) {
                return None;
            }
            let (cx, cy) = (a / 2.0, b / (2.0 * p));
            let r2 = c + cx * cx + p * cy * cy;
            if r2 <= 0.0 {
                return None;
            }
            (cx, cy, r2.sqrt(), (r2 / p).sqrt())
        };
        let o = Oval {
            c: Point::new(g.x + cx * s, g.y + cy * s),
            rx: rx * s,
            ry: ry * s,
        };
        (o.rx.is_finite() && o.ry.is_finite() && o.rx > 1e-6 && o.ry > 1e-6).then_some(o)
    }

    /// How far `q` lies from the oval, measured along the ray from its
    /// centre (never less than the true distance).
    fn off(&self, q: Point) -> f64 {
        let v = q - self.c;
        let rho = (v.x / self.rx).hypot(v.y / self.ry);
        if rho < 1e-9 {
            return self.rx.min(self.ry);
        }
        (1.0 - 1.0 / rho).abs() * v.hypot()
    }

    /// The furthest any of `pts` lies from the oval.
    fn worst(&self, pts: &[Point]) -> f64 {
        pts.iter().map(|q| self.off(*q)).fold(0.0, f64::max)
    }

    /// The angle of `q` round the oval.
    fn angle_of(&self, q: Point) -> f64 {
        ((q.y - self.c.y) / self.ry).atan2((q.x - self.c.x) / self.rx)
    }

    fn at(&self, a: f64) -> Point {
        Point::new(self.c.x + self.rx * a.cos(), self.c.y + self.ry * a.sin())
    }

    /// The way the oval runs at angle `a`, for a growing angle.
    fn way(&self, a: f64) -> Vec2 {
        Vec2::new(-self.rx * a.sin(), self.ry * a.cos())
    }

    /// The arc from angle `a0` over `sweep` as cubics, none of them over
    /// more than `most` radians.
    fn arc(&self, a0: f64, sweep: f64, most: f64, out: &mut Vec<Piece>) {
        let parts = ((sweep.abs() / most).ceil() as usize).max(1);
        let step = sweep / parts as f64;
        let h = 4.0 / 3.0 * (step / 4.0).tan();
        for i in 0..parts {
            let (a, b) = (a0 + step * i as f64, a0 + step * (i + 1) as f64);
            out.push(Piece::Cubic(
                self.at(a) + self.way(a) * h,
                self.at(b) - self.way(b) * h,
                self.at(b),
            ));
        }
    }
}

/// A closed border with no corner that is a circle or a level ellipse:
/// that shape exactly, four curves with their nodes at its top, bottom
/// and sides. None when any point of the border lies further than
/// `most` from it.
fn whole_oval(light: &[Point], most: f64) -> Option<(Point, Vec<Piece>)> {
    let mut o = Oval::through(light, false)?;
    if (o.rx - o.ry).abs() <= OVAL_ROUND * o.rx.max(o.ry) {
        if let Some(c) = Oval::through(light, true).filter(|c| c.worst(light) <= most) {
            o = c;
        }
    }
    if o.worst(light) > most {
        return None;
    }
    // The border must go once round the centre.
    let n = light.len();
    let mut round = 0.0;
    for i in 0..n {
        let step = angle(light[i] - o.c, light[(i + 1) % n] - o.c);
        round += step;
    }
    if (round.abs() - std::f64::consts::TAU).abs() > 0.5 {
        return None;
    }
    let mut pieces = Vec::with_capacity(4);
    o.arc(
        0.0,
        std::f64::consts::TAU.copysign(round),
        std::f64::consts::FRAC_PI_2 + 1e-9,
        &mut pieces,
    );
    Some((o.at(0.0), pieces))
}

/// The curved stretch `d` as an arc of a circle, or of a level ellipse,
/// when it is one: no point of it further than `most` from the arc. Its
/// ends stay where they are, and where an end joins smoothly the arc
/// leaves along `leave` or arrives along `arrive`. None when the
/// stretch is not such an arc (too straight, an S, a spiral, a join
/// that would show).
fn fit_arc(
    d: &[Point],
    leave: Vec2,
    arrive: Vec2,
    smooth_ends: (bool, bool),
    most: f64,
    out: &mut Vec<Piece>,
) -> bool {
    let m = d.len();
    if m < 8 {
        return false;
    }
    let tried = |round: bool| -> Option<(Oval, f64, f64)> {
        // The two ends may be corners made sharp, off the round: the
        // shape is that of the points between them.
        let inner = &d[1..m - 1];
        let o = Oval::through(inner, round)?;
        // An ellipse bends to fit what is not one (a U): it must lie
        // nearer.
        if o.worst(inner) > if round { most } else { most * OVAL_NEAR } {
            return None;
        }
        // Once round at most, always the same way.
        let (mut sweep, mut back) = (0.0f64, 0.0f64);
        let mut last = o.angle_of(d[0]);
        let a0 = last;
        for q in &d[1..] {
            let a = o.angle_of(*q);
            let mut step = a - last;
            if step > std::f64::consts::PI {
                step -= std::f64::consts::TAU;
            } else if step < -std::f64::consts::PI {
                step += std::f64::consts::TAU;
            }
            sweep += step;
            last = a;
            back = back.max(-step * sweep.signum());
        }
        let least = if round { ARC_LEAST } else { ARC_LEAST_OVAL };
        if back > 0.08 || sweep.abs() < least || sweep.abs() > std::f64::consts::TAU - 0.3 {
            return None;
        }
        Some((o, a0, sweep))
    };
    let Some((o, a0, sweep)) = tried(true).or_else(|| tried(false)) else {
        return false;
    };
    let dir = sweep.signum();
    let (t0, t1) = (unit(o.way(a0) * dir), unit(o.way(a0 + sweep) * dir));
    if smooth_ends.0 && angle(t0, leave).abs() > ARC_JOIN {
        return false;
    }
    if smooth_ends.1 && angle(t1, arrive).abs() > ARC_JOIN {
        return false;
    }
    let (e0, e1) = (d[0] - o.at(a0), d[m - 1] - o.at(a0 + sweep));
    if e0.hypot() > ARC_END * most || e1.hypot() > ARC_END * most {
        return false;
    }
    let first = out.len();
    o.arc(a0, sweep, ARC_PART, out);
    // The ends are the stretch's own: the arc is bent to them at its
    // two ends only, and is the round's own between.
    let parts = out.len() - first;
    let mut from = d[0];
    for (i, piece) in out[first..].iter_mut().enumerate() {
        if let Piece::Cubic(c1, c2, to) = piece {
            if i == 0 {
                *c1 += e0;
                if smooth_ends.0 {
                    *c1 = from + unit(leave) * (*c1 - from).hypot();
                }
            }
            if i + 1 == parts {
                *c2 += e1;
                *to = d[m - 1];
                if smooth_ends.1 {
                    *c2 = *to - unit(arrive) * (*to - *c2).hypot();
                }
            }
            from = *to;
        }
    }
    true
}

/// The stretch `d` between two straight lines, leaving along `leave`
/// and arriving along `arrive`, as an arc of a circle or of a level
/// ellipse when it is one within `most`.
pub(crate) fn arc_between(d: &[Point], leave: Vec2, arrive: Vec2, most: f64) -> Option<BezPath> {
    let mut pieces = Vec::new();
    fit_arc(d, leave, arrive, (true, true), most, &mut pieces)
        .then(|| path_of(d[0], &pieces, false))
}

/// The round `g` between the straight runs `before` and `after` as the
/// arc of a circle that touches both lines, when it is one: no point of
/// the round further than `most` from the arc or the two lines. Gives
/// where the arc leaves the first line and its pieces; it ends on the
/// second line.
fn touching_arc(g: &[Point], before: &Run, after: &Run, most: f64) -> Option<(Point, Vec<Piece>)> {
    let m = g.len();
    if m < 6 {
        return None;
    }
    let o = Oval::through(&g[1..m - 1], true)?;
    let (d0, d1) = (before.dir, after.dir);
    let (n0, n1) = (Vec2::new(-d0.y, d0.x), Vec2::new(-d1.y, d1.x));
    let (s0, s1) = ((o.c - before.to).dot(n0), (o.c - after.from).dot(n1));
    // The round turns one way: its centre is on the same side of both.
    if s0 * s1 <= 0.0 || (s0.abs() - o.rx).abs() > most || (s1.abs() - o.rx).abs() > most {
        return None;
    }
    let side = s0.signum();
    let r = (s0.abs() + s1.abs()) / 2.0;
    let c = match crossing(
        (before.to + n0 * (r * side), d0),
        (after.from + n1 * (r * side), d1),
    ) {
        Some(c) if d0.cross(d1).abs() > 0.05 => c,
        _ => o.c - n0 * (s0 - r * side),
    };
    let (from, to) = (c - n0 * (r * side), c - n1 * (r * side));
    // Each line still has some of its length.
    let kept = |a: Point, b: Point, d: Vec2, whole: f64| (b - a).dot(d) >= 0.3 * whole;
    if !kept(before.from, from, d0, (before.to - before.from).hypot())
        || !kept(to, after.to, d1, (after.to - after.from).hypot())
    {
        return None;
    }
    let near = |p: &Point| {
        ((*p - c).hypot() - r).abs() <= most
            || (*p - from).cross(d0).abs() <= most
            || (*p - to).cross(d1).abs() <= most
    };
    if !g.iter().all(near) {
        return None;
    }
    let mut sweep = d0.cross(d1).atan2(d0.dot(d1));
    if sweep * side < 0.0 {
        sweep += side * std::f64::consts::TAU;
    }
    if !(0.3..=5.0).contains(&sweep.abs()) {
        return None;
    }
    let round = Oval { c, rx: r, ry: r };
    let mut pieces = Vec::new();
    round.arc(round.angle_of(from), sweep, ARC_PART, &mut pieces);
    if let Some(Piece::Cubic(_, _, end)) = pieces.last_mut() {
        *end = to;
    }
    Some((from, pieces))
}

/// A stretch with no straight run: an arc of a circle or an ellipse
/// when it is one ([`fit_arc`]), else the fewest lines and cubics
/// ([`fit_gap`]).
fn fit_bend(
    d: &[Point],
    leave: Vec2,
    arrive: Vec2,
    smooth_ends: (bool, bool),
    tol: f64,
    f: &Faithful,
    out: &mut Vec<Piece>,
) {
    let straight = d.len() < 2 || off_chord(d, 0, d.len() - 1) <= tol * LINE_SLACK;
    if !straight && fit_arc(d, leave, arrive, smooth_ends, tol * ARC_SLACK, out) {
        return;
    }
    fit_gap(d, leave, arrive, smooth_ends, tol, f, out);
}

/// The stretch `d` as its straight `runs`, each a line, and between
/// them the fewest lines and cubics ([`fit_gap`]), joining each run
/// along it.
#[allow(clippy::too_many_arguments)]
fn fit_stretch(
    d: &[Point],
    runs: &[Run],
    leave: Vec2,
    arrive: Vec2,
    smooth_ends: (bool, bool),
    tol: f64,
    f: &Faithful,
    out: &mut Vec<Piece>,
) {
    if runs.is_empty() {
        fit_bend(d, leave, arrive, smooth_ends, tol, f, out);
        return;
    }
    // One arc all the way is a truer account of the stretch than short
    // lines and the curves between them.
    let bent = d.len() >= 2 && off_chord(d, 0, d.len() - 1) > tol * LINE_SLACK;
    if bent && fit_arc(d, leave, arrive, smooth_ends, tol * ARC_SLACK, out) {
        return;
    }
    let mut pts = d.to_vec();
    for r in runs {
        pts[r.a] = r.from;
        pts[r.b] = r.to;
    }
    let (mut at, mut way, mut smooth) = (0usize, leave, smooth_ends.0);
    for (i, r) in runs.iter().enumerate() {
        // A round between two runs: the arc that touches both.
        let round = if i > 0 && r.a > at {
            touching_arc(&pts[at..=r.a], &runs[i - 1], r, tol * ARC_SLACK)
        } else {
            None
        };
        if let Some((from, arc)) = round {
            if let Some(Piece::Line(to)) = out.last_mut() {
                *to = from;
            }
            out.extend(arc);
        } else if r.a > at {
            fit_bend(&pts[at..=r.a], way, r.dir, (smooth, true), tol, f, out);
        }
        out.push(Piece::Line(r.to));
        at = r.b;
        way = r.dir;
        smooth = true;
    }
    if at + 1 < pts.len() {
        fit_bend(
            &pts[at..],
            way,
            arrive,
            (smooth, smooth_ends.1),
            tol,
            f,
            out,
        );
    }
}

/// The round end of a thin stroke made square. A stroke no wider than
/// [`CAP_WIDE`] pixels of the picture before it was enlarged has no
/// shape of its end in that picture: the rounding is the enlarging's.
/// Where the border turns back on itself round such a stroke (its two
/// sides straight and running alongside, the turn no longer than a
/// round end is), the points of the turn are put on the square end:
/// level or upright when it is nearly so, else square across the
/// stroke. `shape_right` says whether the shape is on the right of the
/// border's way (a turn round the ground, the inside of a U, is left).
fn square_ends(mids: Vec<Point>, closed: bool, shape_right: bool, f: &Faithful) -> Vec<Point> {
    let n = mids.len();
    let wide = CAP_WIDE * f.scale * f.k;
    if !closed || n < 24 || wide <= 0.0 {
        return mids;
    }
    let mut s = vec![0.0; n + 1];
    for i in 0..n {
        s[i + 1] = s[i] + (mids[(i + 1) % n] - mids[i]).hypot();
    }
    let total = s[n];
    let reach = (0.75 * wide).max(2.0 * f.k);
    if total < 4.0 * reach + wide {
        return mids;
    }
    // The line of the border over `reach` before each point and after
    // it, where it is straight.
    let flank = |i: usize, forwards: bool| -> Option<Vec2> {
        let mut pts = vec![mids[i]];
        let (mut at, mut gone) = (i, 0.0);
        while gone < reach && pts.len() < n {
            let j = if forwards {
                (at + 1) % n
            } else {
                (at + n - 1) % n
            };
            gone += (mids[j] - mids[at]).hypot();
            pts.push(mids[j]);
            at = j;
        }
        if !forwards {
            pts.reverse();
        }
        line_through(&pts)
            .filter(|l| l.2 <= CAP_FLANK * f.k)
            .map(|l| l.1)
    };
    let before: Vec<Option<Vec2>> = (0..n).map(|i| flank(i, false)).collect();
    let after: Vec<Option<Vec2>> = (0..n).map(|i| flank(i, true)).collect();
    // (length of the turn, first point, points on, the two corners).
    let mut found: Vec<(f64, usize, usize, Point, Point)> = Vec::new();
    for i in 0..n {
        let Some(t_in) = before[i] else { continue };
        let mut j = i;
        let mut steps = 0;
        loop {
            j = (j + 1) % n;
            steps += 1;
            let long = if j > i {
                s[j] - s[i]
            } else {
                total - s[i] + s[j]
            };
            if long > CAP_LONG * wide || steps + 2 >= n {
                break;
            }
            let Some(t_out) = after[j] else { continue };
            let turn = angle(t_in, t_out);
            if turn.abs() < CAP_TURN || (turn > 0.0) != shape_right {
                continue;
            }
            let across_w = (mids[j] - mids[i]).hypot();
            if across_w > wide || across_w < f.k || long > CAP_LONG * across_w || long < across_w {
                continue;
            }
            // The way the stroke runs, to its end.
            let out = unit(t_in - t_out);
            if out == Vec2::ZERO {
                break;
            }
            let mut tip = f64::MIN;
            for q in 0..=steps {
                tip = tip.max(mids[(i + q) % n].to_vec2().dot(out));
            }
            let end = mids[i] + out * (tip - mids[i].to_vec2().dot(out) - CAP_BACK * across_w);
            let mut dir = Vec2::new(-out.y, out.x);
            if dir.dot(mids[j] - mids[i]) < 0.0 {
                dir = -dir;
            }
            if dir.y.abs() <= END_LEVEL {
                dir = Vec2::new(dir.x.signum(), 0.0);
            } else if dir.x.abs() <= END_LEVEL {
                dir = Vec2::new(0.0, dir.y.signum());
            }
            let (Some(x), Some(y)) = (
                crossing((mids[i], t_in), (end, dir)),
                crossing((end, dir), (mids[j], t_out)),
            ) else {
                break;
            };
            // The corners lie ahead of the turn's two ends and no
            // further than a round end leaves them.
            let most = 0.25 * across_w + 0.5 * f.k;
            let near = |c: Point| {
                (0..=steps)
                    .map(|q| (mids[(i + q) % n] - c).hypot())
                    .fold(f64::MAX, f64::min)
            };
            if (x - mids[i]).dot(t_in) >= 0.0
                && (mids[j] - y).dot(t_out) >= 0.0
                && (y - x).dot(dir) > 0.5 * f.k
                && near(x) <= most
                && near(y) <= most
            {
                found.push((long, i, steps, x, y));
            }
            break;
        }
    }
    if found.is_empty() {
        return mids;
    }
    // The shortest turns first, none over another.
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut taken = vec![false; n];
    let mut put: Vec<Option<Point>> = vec![None; n];
    for (_, i, steps, x, y) in found {
        if (0..=steps).any(|q| taken[(i + q) % n]) {
            continue;
        }
        let (a, b) = (mids[i], mids[(i + steps) % n]);
        let legs = [(a - x).hypot(), (y - x).hypot(), (b - y).hypot()];
        let whole: f64 = legs.iter().sum();
        for q in 0..=steps {
            let k = (i + q) % n;
            taken[k] = true;
            // Evenly along the square end, from the one side to the other.
            let at = whole * q as f64 / steps as f64;
            put[k] = Some(if at <= legs[0] {
                a.lerp(x, if legs[0] > 0.0 { at / legs[0] } else { 1.0 })
            } else if at <= legs[0] + legs[1] {
                x.lerp(y, (at - legs[0]) / legs[1].max(1e-9))
            } else {
                y.lerp(b, ((at - legs[0] - legs[1]) / legs[2].max(1e-9)).min(1.0))
            });
        }
    }
    mids.iter()
        .zip(&put)
        .map(|(p, q)| q.unwrap_or(*p))
        .collect()
}

/// The fit of the border `mids`: its corners, then each stretch between
/// two of them fitted and checked against its own part of the border,
/// again with less tolerance while its curves stray, and as the border's
/// own points when nothing else passes. With the path, how many times
/// the worst of its stretches was fitted again and the furthest path and
/// border lie apart.
fn attempt(mids: &[Point], closed: bool, f: &Faithful) -> Option<(BezPath, u8, f64)> {
    let n = mids.len();
    let sigma = (0.5 * f.k).max(0.7);
    let passes = ((2.0 * sigma * sigma).round() as usize).max(1);
    let light = smooth(mids, passes, closed);
    let mut total = 0.0;
    for i in 1..n {
        total += (light[i] - light[i - 1]).hypot();
    }
    if closed {
        total += (light[0] - light[n - 1]).hypot();
    }
    let l = Line {
        p: &light,
        closed,
        total,
    };
    let tol = f.tolerance * f.k;
    let allowed = (f.tolerance * LINE_SLACK + CHECK_SLACK) * f.k;
    let mut stops = corners_of(&l, f, ROUND_MOST);
    let mut smooth_start = false;
    if closed && stops.is_empty() {
        // No corner: a circle or an ellipse when it is one.
        if let Some((start, pieces)) = whole_oval(&light, tol * ARC_SLACK) {
            let path = path_of(start, &pieces, true);
            let (off, d) = stray(mids, &path, &[], allowed, f);
            if off <= 1.0 {
                return Some((path, 0, d));
            }
        }
        // Else the fit starts where the border turns most.
        let far = (TURN_FAR * f.k).min(total / 8.0);
        let start = (0..n)
            .max_by(|a, b| {
                let t = |i: usize| l.turn(i, far).unwrap_or(0.0).abs();
                t(*a).total_cmp(&t(*b))
            })
            .unwrap_or(0);
        let (a, _) = l.along(start, 1.5 * f.k, false)?;
        let (b, _) = l.along(start, 1.5 * f.k, true)?;
        let t = unit(light[b] - light[a]);
        if t == Vec2::ZERO {
            return None;
        }
        stops.push(Stop {
            at: start,
            node: light[start],
            back: 0,
            fwd: 0,
            arrive: t,
            leave: t,
            sharp: false,
        });
        smooth_start = true;
    }
    if !closed {
        let reach = |i: usize, forwards: bool| l.along(i, 1.5 * f.k, forwards).map(|x| x.0);
        let first = reach(0, true).map_or(Vec2::ZERO, |j| unit(light[j] - light[0]));
        let last = reach(n - 1, false).map_or(Vec2::ZERO, |j| unit(light[n - 1] - light[j]));
        let chord = unit(mids[n - 1] - mids[0]);
        let end = |at: usize, t: Vec2| Stop {
            at,
            node: mids[at],
            back: 0,
            fwd: 0,
            arrive: if t == Vec2::ZERO { chord } else { t },
            leave: if t == Vec2::ZERO { chord } else { t },
            sharp: false,
        };
        // A corner on top of an end is the end.
        stops.retain(|s| s.at > s.back + 1 && s.at + s.fwd + 1 < n - 1);
        stops.insert(0, end(0, first));
        stops.push(end(n - 1, last));
    }
    let count = stops.len();
    let mut pieces: Vec<Piece> = Vec::new();
    let stretches = if closed { count } else { count - 1 };
    let (mut data, mut border, mut local) = (Vec::new(), Vec::new(), Vec::new());
    let (mut level, mut far) = (0u8, 0.0f64);
    for q in 0..stretches {
        let (a, b) = (stops[q], stops[(q + 1) % count]);
        let len = if closed {
            let d = (b.at + n - a.at) % n;
            if d == 0 {
                n
            } else {
                d
            }
        } else {
            b.at - a.at
        };
        data.clear();
        data.push(a.node);
        if len > a.fwd + b.back {
            let (lo, hi) = (a.fwd.max(1), len - b.back.max(1));
            for j in lo..=hi {
                data.push(light[(a.at + j) % n]);
            }
        }
        data.push(b.node);
        border.clear();
        border.extend((0..=len).map(|j| mids[(a.at + j) % n]));
        let sharp: Vec<(Point, Point)> = [a, b]
            .iter()
            .filter(|s| s.sharp)
            .map(|s| (mids[s.at], s.node))
            .collect();
        let ends = (smooth_start, smooth_start);
        let runs = straight_runs(&data, (!smooth_start, !smooth_start), f);
        let mut passed = false;
        for (round, share) in STRICTER.iter().enumerate() {
            local.clear();
            fit_stretch(
                &data,
                &runs,
                a.leave,
                b.arrive,
                ends,
                tol * share,
                f,
                &mut local,
            );
            let drawn = path_of(a.node, &local, false);
            let (off, d) = stray(&border, &drawn, &sharp, allowed, f);
            if off <= 1.0 {
                passed = true;
                level = level.max(round as u8);
                far = far.max(d);
                break;
            }
        }
        if !passed {
            // The border's own points, as straight pieces.
            local.clear();
            plain(&data, 0.35, &mut local);
            level = STRICTER.len() as u8;
        }
        pieces.extend(local.iter().copied());
    }
    if pieces.is_empty() {
        return None;
    }
    let (start, mut pieces) = sharpen_bends(stops[0].node, pieces, closed, f);
    let start = level_lines(start, &mut pieces, closed, f);
    Some((path_of(start, &pieces, closed), level, far))
}

/// Points along `path` no further apart than a point of the grid.
fn sampled(path: &BezPath) -> Vec<Point> {
    use tracedraw_core::geometry::PathEl;
    let mut out = Vec::new();
    let (mut at, mut start) = (Point::ZERO, Point::ZERO);
    let line = |a: Point, b: Point, out: &mut Vec<Point>| {
        let steps = ((b - a).hypot() / 0.75).ceil().max(1.0) as usize;
        for s in 1..=steps {
            out.push(a + (b - a) * (s as f64 / steps as f64));
        }
    };
    for el in path.elements() {
        match *el {
            PathEl::MoveTo(p) => {
                out.push(p);
                (at, start) = (p, p);
            }
            PathEl::LineTo(p) => {
                line(at, p, &mut out);
                at = p;
            }
            PathEl::QuadTo(c, p) => {
                line(at, c, &mut out);
                line(c, p, &mut out);
                at = p;
            }
            PathEl::CurveTo(c1, c2, p) => {
                let c = CubicBez::new(at, c1, c2, p);
                let len = (c1 - at).hypot() + (c2 - c1).hypot() + (p - c2).hypot();
                let steps = (len / 0.75).ceil().max(1.0) as usize;
                for s in 1..=steps {
                    out.push(c.eval(s as f64 / steps as f64));
                }
                at = p;
            }
            PathEl::ClosePath => {
                if at != start {
                    line(at, start, &mut out);
                }
                at = start;
            }
        }
    }
    out
}

/// The points of a line filed in squares, to find the ones near a place.
struct Near<'a> {
    p: &'a [Point],
    cell: f64,
    cells: std::collections::HashMap<(i64, i64), Vec<u32>>,
}

impl<'a> Near<'a> {
    fn new(p: &'a [Point], cell: f64) -> Near<'a> {
        let mut cells: std::collections::HashMap<(i64, i64), Vec<u32>> =
            std::collections::HashMap::new();
        for (i, q) in p.iter().enumerate() {
            cells
                .entry(((q.x / cell).floor() as i64, (q.y / cell).floor() as i64))
                .or_default()
                .push(i as u32);
        }
        Near { p, cell, cells }
    }

    /// How far `q` lies from the line through the points (the pieces
    /// joining each to the next), `cell` when further than that.
    fn distance(&self, q: Point) -> f64 {
        let (cx, cy) = (
            (q.x / self.cell).floor() as i64,
            (q.y / self.cell).floor() as i64,
        );
        let mut best = self.cell;
        let piece = |a: Point, b: Point| -> f64 {
            let ab = b - a;
            let len2 = ab.hypot2();
            let t = if len2 > 1e-12 {
                ((q - a).dot(ab) / len2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            (q - (a + ab * t)).hypot()
        };
        for dy in -1..=1 {
            for dx in -1..=1 {
                let Some(list) = self.cells.get(&(cx + dx, cy + dy)) else {
                    continue;
                };
                for &i in list {
                    let i = i as usize;
                    if i > 0 {
                        best = best.min(piece(self.p[i - 1], self.p[i]));
                    }
                    if i + 1 < self.p.len() {
                        best = best.min(piece(self.p[i], self.p[i + 1]));
                    }
                    if self.p.len() == 1 {
                        best = best.min((q - self.p[i]).hypot());
                    }
                }
            }
        }
        best
    }
}

/// The furthest `path` and the border `mids` lie apart, either way, as a
/// share of what is allowed there: `allowed` everywhere, and round a
/// corner made sharp as far as its node lies from the border besides.
/// Also the distance itself where that share is greatest.
fn stray(
    border: &[Point],
    path: &BezPath,
    sharp: &[(Point, Point)],
    allowed: f64,
    f: &Faithful,
) -> (f64, f64) {
    let drawn = sampled(path);
    let cell = 3.0 * f.k + 2.0 + allowed;
    let (near_border, near_drawn) = (Near::new(border, cell), Near::new(&drawn, cell));
    let room = |q: Point| -> f64 {
        let mut most = allowed;
        for (apex, node) in sharp {
            let reach = (*node - *apex).hypot();
            if (q - *apex).hypot() <= reach + TURN_FAR * f.k {
                most = most.max(allowed + reach);
            }
        }
        most
    };
    let mut worst = (0.0, 0.0);
    for q in &drawn {
        let d = near_border.distance(*q);
        let share = d / room(*q);
        if share > worst.0 {
            worst = (share, d);
        }
    }
    for q in border {
        let d = near_drawn.distance(*q);
        let share = d / room(*q);
        if share > worst.0 {
            worst = (share, d);
        }
    }
    worst
}

/// The border through `corners` (the pixel corners of the grid it
/// passes, a unit step apart; a closed one without its first point
/// repeated) as lines and curves that were checked against it: fitted,
/// measured, and fitted again more strictly while they stray. The last
/// resort is the border itself, lightly smoothed, as straight pieces. An
/// open border keeps its two ends.
pub(crate) fn fit(
    corners: &[Point],
    closed: bool,
    f: &Faithful,
    picture: Option<&Picture>,
) -> Fitted {
    let mut path = BezPath::new();
    let n = corners.len();
    let done = |path: BezPath, level: u8, stray: f64| Fitted { path, level, stray };
    if n == 0 {
        return done(path, 0, 0.0);
    }
    let mut mids: Vec<Point> = Vec::with_capacity(n + 2);
    if closed {
        for i in 0..n {
            mids.push(corners[i].midpoint(corners[(i + 1) % n]));
        }
    } else {
        mids.push(corners[0]);
        for e in corners.windows(2) {
            mids.push(e[0].midpoint(e[1]));
        }
        mids.push(corners[n - 1]);
    }
    let n = mids.len();
    if closed && n < 4 {
        return done(path, 0, 0.0);
    }
    if !closed && n < 6 {
        path.move_to(mids[0]);
        path.line_to(mids[n - 1]);
        return done(path, 0, 0.0);
    }
    let mids = match picture {
        Some(pic) => refine(&mids, closed, pic, f.k),
        None => mids,
    };
    // The `a` side is on the right of the border's way.
    let mids = match picture.and_then(|pic| pic.a_less) {
        Some(a_less) => square_ends(mids, closed, a_less, f),
        None => mids,
    };
    if let Some((found, level, far)) = attempt(&mids, closed, f) {
        return done(found, level, far);
    }
    // The border itself: its points, lightly smoothed, joined by the
    // straight pieces that stay within a third of a point of them.
    let mut light = smooth(&mids, 1, closed);
    if closed {
        light.push(light[0]);
    } else {
        light[0] = mids[0];
        light[n - 1] = mids[n - 1];
    }
    let mut pieces = Vec::new();
    let half = light.len() / 2;
    plain(&light[..=half], 0.35, &mut pieces);
    plain(&light[half..], 0.35, &mut pieces);
    done(
        path_of(light[0], &pieces, closed),
        STRICTER.len() as u8,
        0.35,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::{PathEl, Shape};

    /// The settings an enlarged picture is fitted with at full detail.
    fn faithful() -> Faithful {
        Faithful {
            k: 2.0,
            tolerance: 0.36,
            corner: 1.0,
            scale: 4.0,
        }
    }

    /// The border of the shape `inside` on a grid of `n` by `n` squares:
    /// the corners of the squares it passes, a unit step apart, the
    /// first not repeated.
    fn border_of(n: i32, inside: impl Fn(i32, i32) -> bool) -> Vec<Point> {
        let mut next = std::collections::HashMap::new();
        for y in 0..n {
            for x in 0..n {
                if !inside(x, y) {
                    continue;
                }
                if !inside(x, y - 1) {
                    next.insert((x, y), (x + 1, y));
                }
                if !inside(x + 1, y) {
                    next.insert((x + 1, y), (x + 1, y + 1));
                }
                if !inside(x, y + 1) {
                    next.insert((x + 1, y + 1), (x, y + 1));
                }
                if !inside(x - 1, y) {
                    next.insert((x, y + 1), (x, y));
                }
            }
        }
        let start = *next.keys().min().expect("a shape");
        let (mut at, mut out) = (start, Vec::new());
        loop {
            out.push(Point::new(at.0 as f64, at.1 as f64));
            at = next[&at];
            if at == start {
                return out;
            }
        }
    }

    fn lines_and_curves(path: &BezPath) -> (usize, usize) {
        let count = |f: fn(&PathEl) -> bool| path.elements().iter().filter(|e| f(e)).count();
        (
            count(|e| matches!(e, PathEl::LineTo(..))),
            count(|e| matches!(e, PathEl::CurveTo(..))),
        )
    }

    /// A rectangle is four lines between four sharp corners, each where
    /// the picture has it and no node more.
    #[test]
    fn a_rectangle_is_four_lines_with_sharp_corners() {
        let border = border_of(120, |x, y| (20..100).contains(&x) && (30..70).contains(&y));
        let got = fit(&border, true, &faithful(), None);
        assert_eq!(got.level, 0, "the first fit passes the check");
        let (lines, curves) = lines_and_curves(&got.path);
        assert_eq!(curves, 0, "{:?}", got.path);
        assert!((3..=4).contains(&lines), "{:?}", got.path);
        let b = got.path.bounding_box();
        for (got, want) in [(b.x0, 20.0), (b.y0, 30.0), (b.x1, 100.0), (b.y1, 70.0)] {
            assert!((got - want).abs() < 0.3, "{b:?}");
        }
        for seg in got.path.segments() {
            let (a, b) = (seg.start(), seg.end());
            assert!(
                (a.x - b.x).abs() < 1e-6 || (a.y - b.y).abs() < 1e-6,
                "a side neither level nor upright: {a:?} {b:?}"
            );
        }
    }

    /// A disc is a few smooth curves with no corner and no straight
    /// piece, nowhere further from its border than the tolerance.
    #[test]
    fn a_disc_is_a_few_curves_within_the_tolerance() {
        let border = border_of(140, |x, y| {
            let (dx, dy) = (x as f64 + 0.5 - 70.0, y as f64 + 0.5 - 70.0);
            dx * dx + dy * dy < 50.0 * 50.0
        });
        let f = faithful();
        let got = fit(&border, true, &f, None);
        let (lines, curves) = lines_and_curves(&got.path);
        assert_eq!(lines, 0, "{:?}", got.path);
        assert!((3..=8).contains(&curves), "{curves} curves");
        for seg in got.path.segments() {
            for t in [0.0, 0.25, 0.5, 0.75] {
                let p = seg.eval(t);
                let r = ((p.x - 70.0).powi(2) + (p.y - 70.0).powi(2)).sqrt();
                assert!((r - 50.0).abs() < f.tolerance * f.k + 0.75, "{p:?} at {r}");
            }
        }
    }

    /// The stem of a letter with a round end keeps its straight sides
    /// straight and upright, and its end round.
    #[test]
    fn a_stem_with_a_round_end_keeps_straight_sides() {
        let border = border_of(120, |x, y| {
            let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
            let stem = (50.0..70.0).contains(&px) && (20.0..90.0).contains(&py);
            let end = (px - 60.0).powi(2) + (py - 90.0).powi(2) < 100.0;
            stem || end
        });
        let got = fit(&border, true, &faithful(), None);
        let upright: Vec<f64> = got
            .path
            .segments()
            .filter(|s| (s.start().x - s.end().x).abs() < 1e-6)
            .map(|s| (s.start().y - s.end().y).abs())
            .collect();
        assert!(
            upright.iter().filter(|len| **len > 55.0).count() == 2,
            "two long upright sides: {upright:?} {:?}",
            got.path
        );
        let (_, curves) = lines_and_curves(&got.path);
        assert!((1..=4).contains(&curves), "{:?}", got.path);
        assert!(got.path.elements().len() <= 10, "{:?}", got.path);
    }

    /// The edge is put where the picture has it between its pixels: a
    /// border the labels drew half a pixel out moves onto the edge, and
    /// one between two colours too alike to tell stays.
    #[test]
    fn the_edge_is_placed_by_the_picture_between_its_pixels() {
        let (w, h) = (60usize, 60usize);
        // Ink over 10.5 to 49.5 by 15.5 to 34.5: its edge pixels are half
        // ink, and the labels took them whole.
        let picture = |ink: f64| -> Vec<[u8; 3]> {
            let cover = |v: usize, from: f64, to: f64| {
                ((v as f64 + 1.0).min(to) - (v as f64).max(from)).clamp(0.0, 1.0)
            };
            (0..w * h)
                .map(|i| {
                    let share = cover(i % w, 10.5, 49.5) * cover(i / w, 15.5, 34.5);
                    [(255.0 + (ink - 255.0) * share).round() as u8; 3]
                })
                .collect()
        };
        let f = faithful();
        let border = border_of(120, |x, y| (20..100).contains(&x) && (30..70).contains(&y));
        let fitted = |colors: &[[u8; 3]], ink: f32| {
            let pic = Picture {
                colors,
                w,
                h,
                a: Some([ink; 3]),
                b: Some([255.0; 3]),
                a_less: None,
            };
            fit(&border, true, &f, Some(&pic)).path.bounding_box()
        };
        let b = fitted(&picture(0.0), 0.0);
        for (got, want) in [(b.x0, 21.0), (b.y0, 31.0), (b.x1, 99.0), (b.y1, 69.0)] {
            assert!((got - want).abs() < 0.3, "the edge half a pixel in: {b:?}");
        }
        let b = fitted(&picture(235.0), 235.0);
        for (got, want) in [(b.x0, 20.0), (b.y0, 30.0), (b.x1, 100.0), (b.y1, 70.0)] {
            assert!((got - want).abs() < 0.3, "left as traced: {b:?}");
        }
    }

    /// An open border keeps its two ends, and nothing too short to fit
    /// makes a fit fail.
    #[test]
    fn open_borders_keep_their_ends_and_short_ones_are_safe() {
        let f = faithful();
        let border: Vec<Point> = (0..=40)
            .map(|i| Point::new(i as f64, 0.0))
            .chain((1..=30).map(|i| Point::new(40.0, i as f64)))
            .collect();
        let got = fit(&border, false, &f, None);
        let segs: Vec<_> = got.path.segments().collect();
        assert_eq!(segs.first().map(|s| s.start()), Some(Point::new(0.0, 0.0)));
        assert_eq!(segs.last().map(|s| s.end()), Some(Point::new(40.0, 30.0)));
        assert_eq!(lines_and_curves(&got.path), (2, 0), "{:?}", got.path);
        for n in 0..8 {
            for closed in [false, true] {
                let _ = fit(&border[..n], closed, &f, None);
            }
        }
        let same = vec![Point::new(3.0, 3.0); 40];
        let _ = fit(&same, true, &f, None);
        let _ = fit(&same, false, &f, None);
        let pic = Picture {
            colors: &[],
            w: 0,
            h: 0,
            a: None,
            b: None,
            a_less: None,
        };
        let _ = fit(&border, false, &f, Some(&pic));
    }
    /// The middles of the steps of a border, as `fit` takes them.
    fn middles(border: &[Point]) -> Vec<Point> {
        let n = border.len();
        (0..n)
            .map(|i| border[i].midpoint(border[(i + 1) % n]))
            .collect()
    }

    /// A bar with two round ends, `wide` across, upright.
    fn bar(wide: f64) -> Vec<Point> {
        border_of(160, |x, y| {
            let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
            let r = wide / 2.0;
            let stem = (px - 80.0).abs() < r && (40.0..120.0).contains(&py);
            let end = |cy: f64| (px - 80.0).powi(2) + (py - cy).powi(2) < r * r;
            stem || end(40.0) || end(120.0)
        })
    }

    /// The side of a stem is a straight run and a wide arc has none: the
    /// arc is as straight over a short way, but turns no faster past the
    /// run than along it.
    #[test]
    fn a_stem_is_a_straight_run_and_a_wide_arc_is_not() {
        let f = faithful();
        let side: Vec<Point> = (0..=30)
            .map(|i| {
                let a = std::f64::consts::PI * (1.0 - i as f64 / 30.0);
                Point::new(20.0 + 20.0 * a.cos(), 20.0 - 20.0 * a.sin())
            })
            .chain((1..=120).map(|i| Point::new(40.0, 20.0 + i as f64 * 0.5)))
            .chain((1..=30).map(|i| {
                let a = std::f64::consts::PI * i as f64 / 30.0;
                Point::new(20.0 + 20.0 * a.cos(), 80.0 + 20.0 * a.sin())
            }))
            .collect();
        let runs = straight_runs(&side, (false, false), &f);
        assert_eq!(runs.len(), 1, "the one side of the stem");
        let r = runs[0];
        assert!((r.from.x - 40.0).abs() < 0.3 && (r.to.x - 40.0).abs() < 0.3);
        assert!((r.to.y - r.from.y) > 50.0, "{:?} {:?}", r.from, r.to);
        let arc: Vec<Point> = (0..=600)
            .map(|i| {
                let a = i as f64 / 600.0 * std::f64::consts::PI;
                Point::new(300.0 * a.cos(), 300.0 * a.sin())
            })
            .collect();
        assert!(straight_runs(&arc, (false, false), &f).is_empty());
        for n in 0..4 {
            assert!(straight_runs(&side[..n], (true, true), &f).is_empty());
        }
    }

    /// The round end of a thin stroke is put square; the same turn round
    /// the ground (the inside of a U) stays round, and so does the end
    /// of a stroke wide enough to show its shape.
    #[test]
    fn the_round_end_of_a_thin_stroke_is_made_square() {
        let f = faithful();
        // 12 points of the grid across: 6 pixels, 1.5 of the picture
        // before it was enlarged.
        let thin = middles(&bar(12.0));
        let lowest = |pts: &[Point]| pts.iter().map(|p| p.y).fold(f64::MIN, f64::max);
        let at_the_end = |pts: &[Point]| {
            let end = lowest(pts);
            let xs: Vec<f64> = pts
                .iter()
                .filter(|p| p.y > end - 0.05)
                .map(|p| p.x)
                .collect();
            xs.iter().copied().fold(f64::MIN, f64::max)
                - xs.iter().copied().fold(f64::MAX, f64::min)
        };
        let squared = square_ends(thin.clone(), true, true, &f);
        assert_eq!(squared.len(), thin.len());
        assert!(at_the_end(&thin) < 6.0, "round as traced");
        assert!(
            at_the_end(&squared) > 8.0,
            "the end runs level across the stroke: {}",
            at_the_end(&squared)
        );
        assert!(lowest(&squared) < lowest(&thin), "no longer than it was");
        assert!(lowest(&squared) > lowest(&thin) - 2.0);
        // The shape on the other side: the turn is round the ground.
        assert_eq!(square_ends(thin.clone(), true, false, &f), thin);
        // Too wide to be the enlarging's rounding.
        let wide = middles(&bar(40.0));
        assert_eq!(square_ends(wide.clone(), true, true, &f), wide);
        // Open borders and short ones stay.
        assert_eq!(square_ends(thin.clone(), false, true, &f), thin);
        let short = thin[..10].to_vec();
        assert_eq!(square_ends(short.clone(), true, true, &f), short);
    }

    /// A disc is a circle: four curves with their nodes at its top,
    /// bottom and sides, every point of them on one circle.
    #[test]
    fn a_disc_is_a_circle_of_four_curves() {
        let border = border_of(140, |x, y| {
            let (dx, dy) = (x as f64 + 0.5 - 70.0, y as f64 + 0.5 - 70.0);
            dx * dx + dy * dy < 50.0 * 50.0
        });
        let got = fit(&border, true, &faithful(), None);
        assert_eq!(lines_and_curves(&got.path), (0, 4), "{:?}", got.path);
        let mut radii = Vec::new();
        for seg in got.path.segments() {
            let p = seg.eval(0.0);
            assert!(
                (p.x - 70.0).abs() < 0.6 || (p.y - 70.0).abs() < 0.6,
                "a node off the axes: {p:?}"
            );
            for t in [0.0, 0.3, 0.5, 0.8] {
                let q = seg.eval(t);
                radii.push((q.x - 70.0).hypot(q.y - 70.0));
            }
        }
        let (least, most) = radii
            .iter()
            .fold((f64::MAX, 0.0f64), |(a, b), r| (a.min(*r), b.max(*r)));
        assert!(most - least < 0.05, "radii from {least} to {most}");
        assert!((least - 50.0).abs() < 0.6, "radius {least}");
    }

    /// A level ellipse is an ellipse, four curves; the same shape
    /// turned is not taken for one.
    #[test]
    fn an_ellipse_is_four_curves_and_a_turned_one_is_not_forced() {
        let level = border_of(160, |x, y| {
            let (dx, dy) = (x as f64 + 0.5 - 80.0, y as f64 + 0.5 - 80.0);
            (dx / 60.0).powi(2) + (dy / 35.0).powi(2) < 1.0
        });
        let f = faithful();
        let got = fit(&level, true, &f, None);
        assert_eq!(lines_and_curves(&got.path), (0, 4), "{:?}", got.path);
        for seg in got.path.segments() {
            for t in [0.0, 0.25, 0.5, 0.75] {
                let p = seg.eval(t);
                let rho = ((p.x - 80.0) / 60.0).hypot((p.y - 80.0) / 35.0);
                assert!((rho - 1.0).abs() < 0.02, "{p:?} at {rho}");
            }
        }
        let (s, c) = (0.6f64.sin(), 0.6f64.cos());
        let turned = border_of(160, |x, y| {
            let (dx, dy) = (x as f64 + 0.5 - 80.0, y as f64 + 0.5 - 80.0);
            let (u, v) = (dx * c + dy * s, dy * c - dx * s);
            (u / 60.0).powi(2) + (v / 30.0).powi(2) < 1.0
        });
        let got = fit(&turned, true, &f, None);
        for seg in got.path.segments() {
            for t in [0.0, 0.25, 0.5, 0.75] {
                let p = seg.eval(t);
                let (dx, dy) = (p.x - 80.0, p.y - 80.0);
                let (u, v) = (dx * c + dy * s, dy * c - dx * s);
                let rho = (u / 60.0).hypot(v / 30.0);
                assert!((rho - 1.0).abs() < 0.05, "{p:?} at {rho}");
            }
        }
    }

    /// The round side of a D is an arc of one circle between its two
    /// corners; the round end of a U is not bent into an ellipse over
    /// its straight sides.
    #[test]
    fn a_half_disc_is_an_arc_and_a_u_keeps_its_sides() {
        let d = border_of(140, |x, y| {
            let (dx, dy) = (x as f64 + 0.5 - 40.0, y as f64 + 0.5 - 70.0);
            dx >= 0.0 && dx * dx + dy * dy < 50.0 * 50.0
        });
        let got = fit(&d, true, &faithful(), None);
        let (lines, curves) = lines_and_curves(&got.path);
        assert_eq!(lines, 1, "{:?}", got.path);
        assert!((2..=3).contains(&curves), "{curves} curves");
        for seg in got.path.segments() {
            if matches!(seg, tracedraw_core::geometry::PathSeg::Line(_)) {
                continue;
            }
            for t in [0.1, 0.3, 0.5, 0.7, 0.9] {
                let p = seg.eval(t);
                let r = (p.x - 40.0).hypot(p.y - 70.0);
                // Nearer a corner made sharp, as far off as it is.
                let most = if (p.x - 40.0).abs() < 25.0 { 1.6 } else { 0.8 };
                assert!((r - 50.0).abs() < most, "{p:?} at {r}");
            }
        }
        let u = border_of(160, |x, y| {
            let (dx, dy) = (x as f64 + 0.5 - 80.0, y as f64 + 0.5 - 100.0);
            (dx.abs() < 30.0 && (20.0..=100.0).contains(&(y as f64))) || dx * dx + dy * dy < 900.0
        });
        let got = fit(&u, true, &faithful(), None);
        let upright = got
            .path
            .segments()
            .filter(|s| {
                let (a, b) = (s.eval(0.0), s.eval(1.0));
                matches!(s, tracedraw_core::geometry::PathSeg::Line(_))
                    && (a.x - b.x).abs() < 0.3
                    && (a.y - b.y).abs() > 60.0
            })
            .count();
        assert_eq!(upright, 2, "{:?}", got.path);
    }

    /// The small round between the two sides of a U is the half circle
    /// that touches both, its ends on the sides.
    #[test]
    fn a_round_between_two_sides_is_the_arc_that_touches_both() {
        let u = border_of(120, |x, y| {
            let (dx, dy) = (x as f64 + 0.5 - 60.0, y as f64 + 0.5 - 80.0);
            (dx.abs() < 12.0 && (20.0..=80.0).contains(&(y as f64))) || dx * dx + dy * dy < 144.0
        });
        let got = fit(&u, true, &faithful(), None);
        let mut upright = 0;
        for seg in got.path.segments() {
            let (a, b) = (seg.eval(0.0), seg.eval(1.0));
            if matches!(seg, tracedraw_core::geometry::PathSeg::Line(_)) {
                if (a.x - b.x).abs() < 1e-6 && (a.y - b.y).abs() > 50.0 {
                    upright += 1;
                }
                continue;
            }
            for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
                let p = seg.eval(t);
                let r = (p.x - 60.0).hypot(p.y - 80.0);
                assert!((r - 12.0).abs() < 0.7, "{p:?} at {r}");
            }
        }
        assert_eq!(upright, 2, "{:?}", got.path);
        let (_, curves) = lines_and_curves(&got.path);
        assert_eq!(curves, 2, "{:?}", got.path);
    }
}
