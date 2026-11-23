//! Live outline effects that make a drawing look hand made or that
//! frame it: Roughen (the outline redrawn with many small random bumps),
//! Jitter Nodes (the nodes it has moved at random), Scribble (the fill
//! drawn as one loose zigzag line), Shape Frame (the outline replaced by a
//! rectangle, rounded rectangle or ellipse round it) and Trim Marks
//! (lines at the corners of the object, outside it). The random ones take
//! a seed, so a drawing looks the same every time it is drawn.

use crate::geometry::{BezPath, PathEl, Point, Rect, Vec2};
use kurbo::Shape as _;
use serde::{Deserialize, Serialize};

/// The largest bump, jitter, gap or line length these effects take, mm.
pub const ROUGH_MAX: f64 = 500.0;
/// The most points Roughen puts on 10 mm of outline.
pub const DETAIL_MAX: u32 = 100;

/// A number from 0 to 1 for `seed` and `i`, the same every time.
fn unit(seed: u32, i: u64) -> f64 {
    let mut z = (u64::from(seed) << 32 ^ i).wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 11) as f64 / (1u64 << 53) as f64
}

/// A number from -1 to 1 for `seed` and `i`.
fn signed(seed: u32, i: u64) -> f64 {
    unit(seed, i) * 2.0 - 1.0
}

fn clean(v: f64, max: f64) -> f64 {
    if v.is_finite() {
        v.clamp(0.0, max)
    } else {
        0.0
    }
}

/// The subpaths of `path` flattened to points, each with whether it is
/// closed (the closing point not repeated).
fn polylines(path: &BezPath, tolerance: f64) -> Vec<(Vec<Point>, bool)> {
    let mut out: Vec<(Vec<Point>, bool)> = Vec::new();
    let mut cur: Vec<Point> = Vec::new();
    kurbo::flatten(path.elements().iter().copied(), tolerance, |el| match el {
        PathEl::MoveTo(p) => {
            if cur.len() > 1 {
                out.push((std::mem::take(&mut cur), false));
            }
            cur = vec![p];
        }
        PathEl::LineTo(p) => cur.push(p),
        PathEl::ClosePath => {
            if cur.len() > 1 && cur.first() == cur.last() {
                cur.pop();
            }
            if cur.len() > 1 {
                out.push((std::mem::take(&mut cur), true));
            }
            cur.clear();
        }
        _ => {}
    });
    if cur.len() > 1 {
        out.push((cur, false));
    }
    out
}

/// Points along `line` (closed or not) at most `step` apart, the corners
/// kept.
fn resample(line: &[Point], closed: bool, step: f64) -> Vec<Point> {
    let mut out = Vec::new();
    let n = line.len();
    let segs = if closed { n } else { n - 1 };
    for i in 0..segs {
        let (a, b) = (line[i], line[(i + 1) % n]);
        let k = ((a.distance(b) / step).ceil() as usize).clamp(1, 10_000);
        for j in 0..k {
            out.push(a.lerp(b, j as f64 / k as f64));
        }
    }
    if !closed {
        out.push(line[n - 1]);
    }
    out
}

/// Smooth curves through `pts` (Catmull-Rom as cubic segments).
fn smooth_through(path: &mut BezPath, pts: &[Point], closed: bool) {
    let n = pts.len();
    if n < 2 {
        return;
    }
    let at = |i: isize| -> Point {
        if closed {
            pts[i.rem_euclid(n as isize) as usize]
        } else {
            pts[i.clamp(0, n as isize - 1) as usize]
        }
    };
    path.move_to(pts[0]);
    let segs = if closed { n } else { n - 1 };
    for i in 0..segs as isize {
        let (p0, p1, p2, p3) = (at(i - 1), at(i), at(i + 1), at(i + 2));
        let c1 = p1 + (p2 - p0) / 6.0;
        let c2 = p2 - (p3 - p1) / 6.0;
        path.curve_to(c1, c2, p2);
    }
    if closed {
        path.close_path();
    }
}

/// Roughen: the outline of `path` redrawn through points `10 / detail`
/// mm apart, each moved up to `size` mm at random (across the outline
/// and along it), as smooth curves or as corners.
pub fn roughen(path: &BezPath, size: f64, detail: u32, smooth: bool, seed: u32) -> BezPath {
    let size = clean(size, ROUGH_MAX);
    let detail = detail.clamp(1, DETAIL_MAX);
    if size <= 0.0 {
        return path.clone();
    }
    let step = 10.0 / f64::from(detail);
    let mut out = BezPath::new();
    let mut k: u64 = 0;
    for (line, closed) in polylines(path, (step / 8.0).max(1e-3)) {
        let mut pts = resample(&line, closed, step);
        if pts.len() > 200_000 {
            pts.truncate(200_000);
        }
        let n = pts.len();
        for (i, p) in pts.iter_mut().enumerate() {
            // The ends of an open line stay where they were.
            if !closed && (i == 0 || i + 1 == n) {
                continue;
            }
            *p += Vec2::new(signed(seed, 2 * k), signed(seed, 2 * k + 1)) * size;
            k += 1;
        }
        if smooth {
            smooth_through(&mut out, &pts, closed);
        } else {
            out.move_to(pts[0]);
            for p in &pts[1..] {
                out.line_to(*p);
            }
            if closed {
                out.close_path();
            }
        }
    }
    out
}

/// Jitter Nodes: every node of `path` moved at random up to `across` mm
/// across and `down` mm down, its handles moved with it; with `handles`,
/// each handle also moves on its own as far again.
pub fn jitter_nodes(path: &BezPath, across: f64, down: f64, handles: bool, seed: u32) -> BezPath {
    let (across, down) = (clean(across, ROUGH_MAX), clean(down, ROUGH_MAX));
    let mut k: u64 = 0;
    let mut shift = || {
        let v = Vec2::new(signed(seed, 2 * k) * across, signed(seed, 2 * k + 1) * down);
        k += 1;
        v
    };
    let mut out = BezPath::new();
    let mut start = Vec2::ZERO;
    let mut last = Vec2::ZERO;
    let mut start_pt = Point::ZERO;
    for el in path.elements() {
        match *el {
            PathEl::MoveTo(p) => {
                last = shift();
                start = last;
                start_pt = p;
                out.move_to(p + last);
            }
            PathEl::LineTo(p) => {
                let d = if p == start_pt { start } else { shift() };
                out.line_to(p + d);
                last = d;
            }
            PathEl::QuadTo(c, p) => {
                let d = if p == start_pt { start } else { shift() };
                let own = if handles { shift() } else { Vec2::ZERO };
                out.quad_to(c + (last + d) / 2.0 + own, p + d);
                last = d;
            }
            PathEl::CurveTo(c1, c2, p) => {
                let d = if p == start_pt { start } else { shift() };
                let (o1, o2) = if handles {
                    (shift(), shift())
                } else {
                    (Vec2::ZERO, Vec2::ZERO)
                };
                out.curve_to(c1 + last + o1, c2 + d + o2, p + d);
                last = d;
            }
            PathEl::ClosePath => out.close_path(),
        }
    }
    out
}

/// The insides of `area` (even-odd) along the horizontal line at `y`:
/// the x of each pair of crossings.
fn spans_at(edges: &[(Point, Point)], y: f64) -> Vec<(f64, f64)> {
    let mut xs: Vec<f64> = edges
        .iter()
        .filter(|(a, b)| (a.y <= y) != (b.y <= y))
        .map(|(a, b)| a.x + (y - a.y) * (b.x - a.x) / (b.y - a.y))
        .collect();
    xs.sort_by(f64::total_cmp);
    xs.chunks_exact(2).map(|c| (c[0], c[1])).collect()
}

/// Scribble: the area `area` fills (even-odd) drawn as one zigzag line
/// at `angle` degrees, its strokes `spacing` mm apart, each running past
/// or short of the edge by up to `loose` mm at random. Where the area
/// splits, a new line starts. Open subpaths, to be stroked.
pub fn scribble(area: &BezPath, angle: f64, spacing: f64, loose: f64, seed: u32) -> BezPath {
    let angle = if angle.is_finite() { angle } else { 45.0 };
    let loose = clean(loose, ROUGH_MAX);
    let b = area.bounding_box();
    let spacing = if spacing.is_finite() { spacing } else { 2.0 };
    let spacing = spacing.max(b.width().max(b.height()) / 20_000.0).max(0.05);
    let c = b.center();
    // Turned so the strokes run along x.
    let turn = kurbo::Affine::rotate_about(-angle.to_radians(), c);
    let back = turn.inverse();
    let mut edges = Vec::new();
    for (line, closed) in polylines(&(turn * area.clone()), (spacing / 20.0).max(1e-3)) {
        let n = line.len();
        let segs = if closed { n } else { n - 1 };
        for i in 0..segs {
            edges.push((line[i], line[(i + 1) % n]));
        }
        if !closed {
            edges.push((line[n - 1], line[0]));
        }
    }
    let tb = (turn * area.clone()).bounding_box();
    let mut out = BezPath::new();
    let mut open = false;
    let mut k: u64 = 0;
    let mut y = tb.y0 + spacing / 2.0;
    let mut right = true;
    while y < tb.y1 {
        let spans = spans_at(&edges, y);
        if spans.len() != 1 {
            open = false;
        }
        for (x0, x1) in &spans {
            let mut wob = || {
                k += 1;
                signed(seed, k) * loose
            };
            let (a, z) = (
                Point::new(x0 - wob(), y + wob() * 0.3),
                Point::new(x1 + wob(), y + wob() * 0.3),
            );
            let (from, to) = if right { (a, z) } else { (z, a) };
            if open {
                out.line_to(back * from);
            } else {
                out.move_to(back * from);
            }
            out.line_to(back * to);
            open = spans.len() == 1;
        }
        right = !right;
        y += spacing;
    }
    out
}

/// The extra settings of the scribble: how round its turns are,
/// how far its strokes run past the edge and how much its spacing and
/// width vary.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ScribbleMore {
    /// 0 to 1: 0 turns sharp, 1 each turn a wide round loop.
    pub curvy: f64,
    /// Millimetres each stroke runs past the edge (below zero: stops
    /// short of it), before the looseness.
    pub overlap: f64,
    /// 0 to 1: how much each gap between strokes varies from `spacing`.
    pub spacing_var: f64,
    /// 0 to 1: how much each stroke's width varies.
    pub width_var: f64,
}

impl ScribbleMore {
    /// No extra: the plain scribble.
    pub fn is_plain(&self) -> bool {
        let z = |v: f64| !v.is_finite() || v == 0.0;
        z(self.curvy) && z(self.overlap) && z(self.spacing_var) && z(self.width_var)
    }
}

/// The scribble with its extra settings, as pieces each with a width
/// factor (1 when the width does not vary): each piece is one stroke and
/// the turn into it, starting where the one before ends, so the pieces
/// drawn one after the other make one line. A plain `more` gives one
/// piece, the same line as [`scribble`].
pub fn scribble_pieces(
    area: &BezPath,
    angle: f64,
    spacing: f64,
    loose: f64,
    seed: u32,
    more: ScribbleMore,
) -> Vec<(BezPath, f64)> {
    if more.is_plain() {
        return vec![(scribble(area, angle, spacing, loose, seed), 1.0)];
    }
    let angle = if angle.is_finite() { angle } else { 45.0 };
    let loose = clean(loose, ROUGH_MAX);
    let curvy = clean(more.curvy, 1.0);
    let svar = clean(more.spacing_var, 1.0);
    let wvar = clean(more.width_var, 1.0);
    let overlap = if more.overlap.is_finite() {
        more.overlap.clamp(-ROUGH_MAX, ROUGH_MAX)
    } else {
        0.0
    };
    let b = area.bounding_box();
    let spacing = if spacing.is_finite() { spacing } else { 2.0 };
    let spacing = spacing.max(b.width().max(b.height()) / 20_000.0).max(0.05);
    let c = b.center();
    let turn = kurbo::Affine::rotate_about(-angle.to_radians(), c);
    let back = turn.inverse();
    let mut edges = Vec::new();
    for (line, closed) in polylines(&(turn * area.clone()), (spacing / 20.0).max(1e-3)) {
        let n = line.len();
        let segs = if closed { n } else { n - 1 };
        for i in 0..segs {
            edges.push((line[i], line[(i + 1) % n]));
        }
        if !closed {
            edges.push((line[n - 1], line[0]));
        }
    }
    let tb = (turn * area.clone()).bounding_box();
    // Numbers apart from the ones the looseness takes.
    const SPACE: u64 = 1 << 40;
    const WIDTH: u64 = 2 << 40;
    let mut out: Vec<(BezPath, f64)> = Vec::new();
    // Where the line is and which way its last stroke ran, when the next
    // stroke goes on from it.
    let mut last: Option<(Point, f64)> = None;
    let mut k: u64 = 0;
    let mut row: u64 = 0;
    let mut y = tb.y0 + spacing / 2.0;
    let mut right = true;
    // A turn's handles reach out past the stroke ends, up to one and a
    // half gaps at full curviness.
    let reach = curvy * spacing * 1.5;
    while y < tb.y1 {
        let spans = spans_at(&edges, y);
        if spans.len() != 1 {
            last = None;
        }
        for (x0, x1) in &spans {
            let mut wob = || {
                k += 1;
                signed(seed, k) * loose
            };
            let (a, z) = (
                Point::new(x0 - overlap - wob(), y + wob() * 0.3),
                Point::new(x1 + overlap + wob(), y + wob() * 0.3),
            );
            if z.x <= a.x {
                last = None;
                continue;
            }
            let (from, to) = if right { (a, z) } else { (z, a) };
            let dir = if right { 1.0 } else { -1.0 };
            let mut piece = BezPath::new();
            match last {
                Some((p, d)) => {
                    piece.move_to(back * p);
                    if reach > 0.0 {
                        // Out past the end it came from, round, and back
                        // in past the start of this one.
                        let c1 = Point::new(p.x + d * reach, p.y);
                        let c2 = Point::new(from.x - dir * reach, from.y);
                        piece.curve_to(back * c1, back * c2, back * from);
                    } else {
                        piece.line_to(back * from);
                    }
                }
                None => piece.move_to(back * from),
            }
            piece.line_to(back * to);
            let w = if wvar > 0.0 {
                (1.0 + wvar * signed(seed, WIDTH + out.len() as u64)).clamp(0.15, 2.0)
            } else {
                1.0
            };
            out.push((piece, w));
            last = if spans.len() == 1 {
                Some((to, dir))
            } else {
                None
            };
        }
        right = !right;
        row += 1;
        let step = spacing * (1.0 + svar * 0.8 * signed(seed, SPACE + row));
        y += step.max(spacing * 0.1);
    }
    if wvar == 0.0 {
        // One width: the pieces joined into one line.
        let mut one = BezPath::new();
        for (piece, _) in &out {
            for (i, el) in piece.elements().iter().enumerate() {
                match el {
                    PathEl::MoveTo(q)
                        if i == 0
                            && one.elements().last().and_then(|e| e.end_point()) == Some(*q) => {}
                    e => one.push(*e),
                }
            }
        }
        return vec![(one, 1.0)];
    }
    out
}

/// What Shape Frame draws round the object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameForm {
    #[default]
    Rectangle,
    Rounded,
    Ellipse,
}

impl FrameForm {
    pub const ALL: [FrameForm; 3] = [FrameForm::Rectangle, FrameForm::Rounded, FrameForm::Ellipse];
}

/// Shape Frame: `form` round `b` grown by `pad` mm on every side (shrunk
/// below zero, never past nothing), with corners of `radius` mm when
/// rounded.
pub fn shape_frame(b: Rect, form: FrameForm, pad: f64, radius: f64) -> BezPath {
    let pad = if pad.is_finite() {
        pad.clamp(-ROUGH_MAX, ROUGH_MAX)
    } else {
        0.0
    };
    let pad = pad.max(-b.width().min(b.height()) / 2.0);
    let r = b.inflate(pad, pad);
    match form {
        FrameForm::Rectangle => r.to_path(0.01),
        FrameForm::Rounded => {
            let rad = clean(radius, ROUGH_MAX).min(r.width().min(r.height()) / 2.0);
            kurbo::RoundedRect::from_rect(r, rad).to_path(0.01)
        }
        FrameForm::Ellipse => kurbo::Ellipse::from_rect(r).to_path(0.01),
    }
}

/// Trim Marks: two lines at each corner of `b`, `length` mm long, starting
/// `gap` mm away from it, along its sides.
pub fn trim_marks(b: Rect, length: f64, gap: f64) -> BezPath {
    let (length, gap) = (clean(length, ROUGH_MAX), clean(gap, ROUGH_MAX));
    let mut out = BezPath::new();
    if length <= 0.0 {
        return out;
    }
    for (x, sx) in [(b.x0, -1.0), (b.x1, 1.0)] {
        for (y, sy) in [(b.y0, -1.0), (b.y1, 1.0)] {
            out.move_to((x + sx * gap, y));
            out.line_to((x + sx * (gap + length), y));
            out.move_to((x, y + sy * gap));
            out.line_to((x, y + sy * (gap + length)));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> BezPath {
        Rect::new(0.0, 0.0, 20.0, 20.0).to_path(0.01)
    }

    /// Roughen keeps the outline near where it was (within the bump
    /// size), puts many more points on it, and is the same every time
    /// for one seed and different for another.
    #[test]
    fn roughen_bumps_the_outline_within_its_size() {
        let r = roughen(&square(), 1.0, 5, false, 7);
        let b = r.bounding_box();
        assert!(
            b.x0 >= -1.0 && b.x1 <= 21.0 && b.y0 >= -1.0 && b.y1 <= 21.0,
            "{b:?}"
        );
        assert!(r.elements().len() > 30, "{}", r.elements().len());
        assert_eq!(r, roughen(&square(), 1.0, 5, false, 7));
        assert_ne!(r, roughen(&square(), 1.0, 5, false, 8));
        assert_eq!(roughen(&square(), 0.0, 5, true, 7), square());
        let smooth = roughen(&square(), 1.0, 5, true, 7);
        assert!(smooth
            .elements()
            .iter()
            .any(|e| matches!(e, PathEl::CurveTo(..))));
    }

    /// Jitter Nodes moves each node at most as far as it is told, the
    /// closing node with the first.
    #[test]
    fn jitter_moves_nodes_within_reach() {
        let j = jitter_nodes(&square(), 2.0, 0.0, false, 3);
        assert_eq!(j.elements().len(), square().elements().len());
        for (a, b) in square().elements().iter().zip(j.elements()) {
            if let (Some(p), Some(q)) = (a.end_point(), b.end_point()) {
                assert!((p.x - q.x).abs() <= 2.0 + 1e-9 && (p.y - q.y).abs() < 1e-9);
            }
        }
        assert_eq!(jitter_nodes(&square(), 0.0, 0.0, true, 3), square());
    }

    /// Scribble fills a square with zigzag strokes the spacing apart, one
    /// line from top to bottom, inside the square give or take the
    /// looseness.
    #[test]
    fn scribble_zigzags_through_the_area() {
        let s = scribble(&square(), 0.0, 2.0, 0.0, 1);
        let moves = s
            .elements()
            .iter()
            .filter(|e| matches!(e, PathEl::MoveTo(_)))
            .count();
        assert_eq!(moves, 1);
        let lines = s
            .elements()
            .iter()
            .filter(|e| matches!(e, PathEl::LineTo(_)))
            .count();
        assert_eq!(lines, 19, "10 strokes and 9 turns");
        let b = s.bounding_box();
        assert!(b.x0 >= -1e-6 && b.x1 <= 20.0 + 1e-6, "{b:?}");
        // A ring: each stroke across the hole splits, so new lines start.
        let mut ring = Rect::new(0.0, 0.0, 30.0, 30.0).to_path(0.01);
        ring.extend(Rect::new(10.0, 10.0, 20.0, 20.0).to_path(0.01));
        let r = scribble(&ring, 0.0, 2.0, 0.0, 1);
        assert!(
            r.elements()
                .iter()
                .filter(|e| matches!(e, PathEl::MoveTo(_)))
                .count()
                > 2
        );
    }

    /// The scribble's extras: round turns are curves, the overlap runs
    /// the strokes past the edge, a varying width gives pieces of their
    /// own widths and a varying spacing moves the strokes; none of them
    /// gives the plain scribble.
    #[test]
    fn scribble_extras_shape_the_line() {
        let plain = scribble(&square(), 0.0, 2.0, 0.0, 1);
        let none = scribble_pieces(&square(), 0.0, 2.0, 0.0, 1, ScribbleMore::default());
        assert_eq!(none, vec![(plain.clone(), 1.0)]);
        let more = |m: ScribbleMore| scribble_pieces(&square(), 0.0, 2.0, 0.0, 1, m);
        let round = more(ScribbleMore {
            curvy: 0.5,
            ..Default::default()
        });
        assert_eq!(round.len(), 1);
        let curves = round[0]
            .0
            .elements()
            .iter()
            .filter(|e| matches!(e, PathEl::CurveTo(..)))
            .count();
        assert_eq!(curves, 9, "each of the 9 turns round");
        assert!(
            round[0].0.bounding_box().x1 > 20.0,
            "the loops bulge past the edge"
        );
        let past = more(ScribbleMore {
            overlap: 1.0,
            ..Default::default()
        });
        let b = past[0].0.bounding_box();
        assert!(
            (b.x0 + 1.0).abs() < 1e-6 && (b.x1 - 21.0).abs() < 1e-6,
            "{b:?}"
        );
        let short = more(ScribbleMore {
            overlap: -1.0,
            ..Default::default()
        });
        let b = short[0].0.bounding_box();
        assert!(
            (b.x0 - 1.0).abs() < 1e-6 && (b.x1 - 19.0).abs() < 1e-6,
            "{b:?}"
        );
        let widths = more(ScribbleMore {
            width_var: 0.8,
            ..Default::default()
        });
        assert_eq!(widths.len(), 10, "a piece a stroke");
        assert!(widths.iter().all(|(_, w)| (0.15..=2.0).contains(w)));
        assert!(widths.iter().any(|(_, w)| (w - 1.0).abs() > 0.05));
        // The pieces run on: each starts where the one before ends.
        for pair in widths.windows(2) {
            let end = pair[0].0.elements().last().and_then(|e| e.end_point());
            let start = match pair[1].0.elements()[0] {
                PathEl::MoveTo(p) => Some(p),
                _ => None,
            };
            assert_eq!(end, start);
        }
        let spaced = more(ScribbleMore {
            spacing_var: 1.0,
            ..Default::default()
        });
        assert_ne!(spaced[0].0, plain);
        let sb = spaced[0].0.bounding_box();
        assert!(sb.y0 >= -1e-6 && sb.y1 <= 20.0 + 1e-6, "{sb:?}");
    }

    /// Shape Frame grows the box by the padding; Trim Marks draws eight
    /// lines outside the corners.
    #[test]
    fn frame_and_trim_marks() {
        let b = Rect::new(0.0, 0.0, 10.0, 20.0);
        let f = shape_frame(b, FrameForm::Ellipse, 2.0, 0.0).bounding_box();
        assert!(
            (f.x0 + 2.0).abs() < 1e-6 && (f.y1 - 22.0).abs() < 1e-6,
            "{f:?}"
        );
        let r = shape_frame(b, FrameForm::Rectangle, -100.0, 0.0).bounding_box();
        assert!(r.width() >= 0.0);
        let m = trim_marks(b, 5.0, 1.0);
        let lines = m
            .elements()
            .iter()
            .filter(|e| matches!(e, PathEl::LineTo(_)))
            .count();
        assert_eq!(lines, 8);
        let mb = m.bounding_box();
        assert!((mb.x0 + 6.0).abs() < 1e-9 && (mb.y1 - 26.0).abs() < 1e-9);
    }
}
