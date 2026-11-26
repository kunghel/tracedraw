//! Effects > Frame Generator: a decorative frame drawn around the
//! selection (or inside the page when nothing is selected) as vector
//! curves: ruled lines, classic corner scrolls, art deco steps and fans,
//! beads, a Greek key, scallops, interlaced waves and guilloche (many
//! fine waved lines weaving along the band, for certificates and
//! vouchers), with optional
//! ornaments in the middle of the sides. The frame is one group on top,
//! in one undo step.

use std::f64::consts::{FRAC_PI_2, PI, TAU};
use tracedraw_core::document::{Shape, ShapeKind};
use tracedraw_core::geometry::{BezPath, PathEl, Point, Rect, Shape as _, Vec2};
use tracedraw_core::style::{LineCap, LineJoin};
use tracedraw_core::{Color, Command, Fill, Stroke};

/// The frame's look, in the order of the panel's list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Lines,
    Classic,
    ArtDeco,
    Beads,
    GreekKey,
    Scallops,
    Waves,
    Guilloche,
}

impl Style {
    pub const ALL: [Style; 8] = [
        Style::Lines,
        Style::Classic,
        Style::ArtDeco,
        Style::Beads,
        Style::GreekKey,
        Style::Scallops,
        Style::Waves,
        Style::Guilloche,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Style::Lines => "docker.frame_lines",
            Style::Classic => "docker.frame_classic",
            Style::ArtDeco => "docker.frame_art_deco",
            Style::Beads => "docker.frame_beads",
            Style::GreekKey => "docker.frame_greek_key",
            Style::Scallops => "docker.frame_scallops",
            Style::Waves => "docker.frame_waves",
            Style::Guilloche => "docker.frame_guilloche",
        }
    }
}

/// How the band's outlines turn the corners.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Corner {
    Square,
    Round,
    Notch,
    Cut,
}

impl Corner {
    pub const ALL: [Corner; 4] = [Corner::Square, Corner::Round, Corner::Notch, Corner::Cut];

    pub fn key(self) -> &'static str {
        match self {
            Corner::Square => "docker.frame_corner_square",
            Corner::Round => "docker.frame_corner_round",
            Corner::Notch => "docker.frame_corner_notch",
            Corner::Cut => "docker.frame_corner_cut",
        }
    }
}

/// The Frame Generator docker's settings (sizes in millimetres).
#[derive(Debug, Clone, PartialEq)]
pub struct FrameSettings {
    pub style: Style,
    pub corner: Corner,
    /// Gap between the selection and the frame (with nothing selected,
    /// between the page edge and the frame).
    pub margin: f64,
    /// The decorated band's width.
    pub width: f64,
    /// Outline width of the lines.
    pub line: f64,
    /// Length of one repeat of beads, keys, scallops and waves.
    pub repeat: f64,
    /// Guilloche: how many waved lines weave along the band.
    pub strands: u32,
    /// An ornament in the middle of each side.
    pub side_ornaments: bool,
    pub color: Color,
}

impl Default for FrameSettings {
    fn default() -> Self {
        FrameSettings {
            style: Style::Classic,
            corner: Corner::Square,
            margin: 5.0,
            width: 8.0,
            line: 0.5,
            repeat: 8.0,
            strands: 12,
            side_ornaments: false,
            color: Color::BLACK,
        }
    }
}

impl FrameSettings {
    /// Every setting back to its default.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// One piece of the frame: a curve stroked with `width` (mm), or filled
/// when `width` is 0.
#[derive(Debug, Clone)]
pub struct Piece {
    pub path: BezPath,
    pub width: f64,
}

fn stroked(path: BezPath, width: f64) -> Piece {
    Piece { path, width }
}

fn filled(path: BezPath) -> Piece {
    Piece { path, width: 0.0 }
}

fn polyline(pts: &[Point], closed: bool) -> BezPath {
    let mut p = BezPath::new();
    for (i, q) in pts.iter().enumerate() {
        if i == 0 {
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

/// A circle as four cubic quarters.
fn circle(c: Point, r: f64) -> BezPath {
    let k = 0.552_284_75 * r;
    let mut p = BezPath::new();
    p.move_to((c.x + r, c.y));
    p.curve_to((c.x + r, c.y + k), (c.x + k, c.y + r), (c.x, c.y + r));
    p.curve_to((c.x - k, c.y + r), (c.x - r, c.y + k), (c.x - r, c.y));
    p.curve_to((c.x - r, c.y - k), (c.x - k, c.y - r), (c.x, c.y - r));
    p.curve_to((c.x + k, c.y - r), (c.x + r, c.y - k), (c.x + r, c.y));
    p.close_path();
    p
}

/// Points along an arc round `c` from angle `a0` to `a1` (radians).
fn arc_pts(c: Point, r: f64, a0: f64, a1: f64, out: &mut Vec<Point>) {
    let n = (((a1 - a0).abs() / (PI / 24.0)).ceil() as usize).max(2);
    for i in 0..=n {
        let a = a0 + (a1 - a0) * i as f64 / n as f64;
        out.push(Point::new(c.x + r * a.cos(), c.y + r * a.sin()));
    }
}

/// The outline of `r` with its corners turned the `corner` way, the turn
/// `size` mm across (a round corner's radius, a notch's, a cut's leg).
pub fn corner_rect(r: Rect, corner: Corner, size: f64) -> BezPath {
    let s = size.min(r.width() / 2.0).min(r.height() / 2.0).max(0.0);
    if corner == Corner::Square || s <= 1e-6 {
        return polyline(
            &[
                Point::new(r.x0, r.y0),
                Point::new(r.x1, r.y0),
                Point::new(r.x1, r.y1),
                Point::new(r.x0, r.y1),
            ],
            true,
        );
    }
    // Corners counter-clockwise from the lower right, with the unit
    // directions toward the corner before and the one after.
    let corners = [
        (
            Point::new(r.x1, r.y0),
            Vec2::new(-1.0, 0.0),
            Vec2::new(0.0, 1.0),
        ),
        (
            Point::new(r.x1, r.y1),
            Vec2::new(0.0, -1.0),
            Vec2::new(-1.0, 0.0),
        ),
        (
            Point::new(r.x0, r.y1),
            Vec2::new(1.0, 0.0),
            Vec2::new(0.0, -1.0),
        ),
        (
            Point::new(r.x0, r.y0),
            Vec2::new(0.0, 1.0),
            Vec2::new(1.0, 0.0),
        ),
    ];
    let mut pts = Vec::new();
    for (q, prev, next) in corners {
        let p0 = q + prev * s;
        let p1 = q + next * s;
        match corner {
            Corner::Round => {
                // Convex: a quarter circle round the point s in from both.
                let c = p0 + next * s;
                let a0 = (p0 - c).atan2();
                arc_pts(c, s, a0, a0 + FRAC_PI_2, &mut pts);
            }
            Corner::Notch => {
                // Concave: a quarter circle round the corner itself.
                let a0 = prev.atan2();
                arc_pts(q, s, a0, a0 - FRAC_PI_2, &mut pts);
            }
            _ => {
                pts.push(p0);
                pts.push(p1);
            }
        }
    }
    polyline(&pts, true)
}

/// The rectangle `d` mm inside `r`, with the corner turn its line takes
/// when it runs alongside the outer one `d` mm in (round corners shrink,
/// notches and cuts grow).
fn inset(r: Rect, corner: Corner, size: f64, d: f64, width: f64) -> Piece {
    let inner = r.inflate(-d, -d);
    let s = match corner {
        Corner::Square => 0.0,
        Corner::Round => (size - d).max(0.0),
        Corner::Notch => size + d,
        Corner::Cut => size + d * (2f64.sqrt() - 1.0),
    };
    stroked(corner_rect(inner, corner, s), width)
}

/// A frame of the band from `outer` to `width` mm inside it: its pieces,
/// in page coordinates (y up).
pub fn frame(outer: Rect, s: &FrameSettings) -> Vec<Piece> {
    let b = s
        .width
        .max(0.5)
        .min(outer.width() / 2.0)
        .min(outer.height() / 2.0);
    let lw = s.line.max(0.01);
    let size = b * 0.75;
    let inner = outer.inflate(-b, -b);
    let period = s.repeat.max(1.0);
    let mut out = Vec::new();
    match s.style {
        Style::Lines => {
            out.push(inset(outer, s.corner, size, 0.0, lw * 3.0));
            out.push(inset(outer, s.corner, size, b * 0.45, lw));
            out.push(inset(outer, s.corner, size, b, lw));
        }
        Style::Classic => {
            out.push(inset(outer, s.corner, size, 0.0, lw * 2.0));
            out.push(inset(outer, s.corner, size, lw * 3.0, lw * 0.6));
            out.push(inset(outer, s.corner, size, b, lw));
            for (q, ax, ay) in corner_frames(outer) {
                out.extend(scroll(q, ax, ay, b, lw));
            }
        }
        Style::ArtDeco => {
            let step = b * 0.3;
            for (i, d) in [0.0, b * 0.35, b].into_iter().enumerate() {
                let w = if i == 0 { lw * 2.0 } else { lw };
                out.push(stroked(stepped(outer.inflate(-d, -d), step), w));
            }
            for (q, ax, ay) in corner_frames(inner) {
                // A fan in the corner of the picture area.
                for k in 1..=3 {
                    let mut pts = Vec::new();
                    let r = b * 0.35 * k as f64;
                    for i in 0..=16 {
                        let t = FRAC_PI_2 * i as f64 / 16.0;
                        pts.push(q + ax * (r * t.cos()) + ay * (r * t.sin()));
                    }
                    out.push(stroked(polyline(&pts, false), lw * 0.7));
                }
                for i in 1..4 {
                    let t = FRAC_PI_2 * i as f64 / 4.0;
                    let r = b * 1.05;
                    out.push(stroked(
                        polyline(&[q, q + ax * (r * t.cos()) + ay * (r * t.sin())], false),
                        lw * 0.7,
                    ));
                }
            }
        }
        Style::Beads => {
            out.push(inset(outer, s.corner, size, 0.0, lw));
            out.push(inset(outer, s.corner, size, b, lw));
            let mid = outer.inflate(-b / 2.0, -b / 2.0);
            let r = (b * 0.32).min(period * 0.4);
            for q in along_sides(mid, period) {
                out.push(filled(circle(q, r)));
            }
        }
        Style::GreekKey => {
            out.push(inset(outer, Corner::Square, 0.0, 0.0, lw));
            out.push(inset(outer, Corner::Square, 0.0, b, lw));
            out.extend(greek_key(outer, b, period, lw));
        }
        Style::Scallops => {
            out.push(stroked(
                scalloped(outer.inflate(-b * 0.3, -b * 0.3), period, b * 0.3),
                lw * 1.5,
            ));
            out.push(inset(outer, s.corner, size, b * 0.55, lw));
            out.push(inset(outer, s.corner, size, b, lw));
        }
        Style::Waves => {
            out.push(inset(outer, Corner::Round, b, 0.0, lw));
            out.push(inset(outer, Corner::Round, b, b, lw));
            let mid = outer.inflate(-b / 2.0, -b / 2.0);
            for phase in [0.0, PI] {
                out.push(stroked(wave(mid, b / 2.0, b * 0.32, period, phase), lw));
            }
        }
        Style::Guilloche => {
            // Ruled edges, then the strands: each waves round the band's
            // middle line with its own phase, its height swelling and
            // shrinking along a slower wave, so the lines weave into a
            // braid of loops.
            out.push(inset(outer, Corner::Round, b, 0.0, lw));
            out.push(inset(outer, Corner::Round, b, b, lw));
            let mid = outer.inflate(-b / 2.0, -b / 2.0);
            let n = s.strands.clamp(2, 60);
            let fine = (lw * 0.35).max(0.05);
            for k in 0..n {
                let phase = TAU * k as f64 / n as f64;
                out.push(stroked(
                    guilloche_strand(mid, b / 2.0, b * 0.36, period, phase),
                    fine,
                ));
            }
        }
    }
    if s.side_ornaments {
        out.extend(side_ornaments(outer, b, lw));
    }
    out
}

/// The four corners of `r` with the two unit directions from each that
/// run along its sides, into the rectangle.
fn corner_frames(r: Rect) -> [(Point, Vec2, Vec2); 4] {
    [
        (
            Point::new(r.x0, r.y0),
            Vec2::new(1.0, 0.0),
            Vec2::new(0.0, 1.0),
        ),
        (
            Point::new(r.x1, r.y0),
            Vec2::new(0.0, 1.0),
            Vec2::new(-1.0, 0.0),
        ),
        (
            Point::new(r.x1, r.y1),
            Vec2::new(-1.0, 0.0),
            Vec2::new(0.0, -1.0),
        ),
        (
            Point::new(r.x0, r.y1),
            Vec2::new(0.0, -1.0),
            Vec2::new(1.0, 0.0),
        ),
    ]
}

/// A classic corner: a leaf on the diagonal and a scroll curling along
/// each side, in the corner `q` of the band whose sides run along `ax`
/// and `ay`.
fn scroll(q: Point, ax: Vec2, ay: Vec2, b: f64, lw: f64) -> Vec<Piece> {
    let at = |x: f64, y: f64| q + ax * x + ay * y;
    let mut out = Vec::new();
    // The leaf: two arcs meeting at its tips, along the diagonal.
    let (t0, t1) = ((0.25, 0.25), (0.95, 0.95));
    let mut leaf = BezPath::new();
    leaf.move_to(at(t0.0 * b, t0.1 * b));
    leaf.quad_to(at(0.85 * b, 0.35 * b), at(t1.0 * b, t1.1 * b));
    leaf.quad_to(at(0.35 * b, 0.85 * b), at(t0.0 * b, t0.1 * b));
    leaf.close_path();
    out.push(filled(leaf));
    for swap in [false, true] {
        // Along x (or y when swapped): a tail from the leaf's base that
        // curls back into a spiral ending in a dot.
        let pt = |x: f64, y: f64| if swap { at(y, x) } else { at(x, y) };
        let c = (1.55 * b, 0.5 * b);
        let mut pts = vec![pt(0.45 * b, 0.25 * b)];
        let turns = 1.6;
        let n = 48;
        for i in 0..=n {
            let t = i as f64 / n as f64;
            let a = PI - TAU * turns * t;
            let r = 0.33 * b * (1.0 - 0.75 * t);
            pts.push(pt(c.0 + r * a.cos(), c.1 + r * a.sin()));
        }
        let end = *pts.last().unwrap_or(&q);
        out.push(stroked(polyline(&pts, false), lw * 1.3));
        out.push(filled(circle(end, lw * 1.4)));
    }
    out
}

/// A rectangle with each corner stepped inward twice by `step`.
fn stepped(r: Rect, step: f64) -> BezPath {
    let s = step.min(r.width() / 4.0).min(r.height() / 4.0).max(0.0);
    let mut pts = Vec::new();
    for (q, ax, ay) in corner_frames(r) {
        // Coming in along -ay from the previous side, stepping round.
        pts.push(q + ay * (2.0 * s));
        pts.push(q + ay * (2.0 * s) + ax * s);
        pts.push(q + ay * s + ax * s);
        pts.push(q + ay * s + ax * (2.0 * s));
        pts.push(q + ax * (2.0 * s));
    }
    polyline(&pts, true)
}

/// Points along the sides of `r` about `period` apart, one in each
/// corner and the rest spread evenly.
fn along_sides(r: Rect, period: f64) -> Vec<Point> {
    let mut out = Vec::new();
    let corners = [
        Point::new(r.x0, r.y0),
        Point::new(r.x1, r.y0),
        Point::new(r.x1, r.y1),
        Point::new(r.x0, r.y1),
    ];
    for i in 0..4 {
        let (a, c) = (corners[i], corners[(i + 1) % 4]);
        let n = ((a.distance(c) / period).round() as usize).max(1);
        for k in 0..n {
            out.push(a.lerp(c, k as f64 / n as f64));
        }
    }
    out
}

/// A Greek key running round the band from `outer` to `b` mm in: a
/// hooked spiral in each repeat of every side, a square rosette in each
/// corner.
fn greek_key(outer: Rect, b: f64, period: f64, lw: f64) -> Vec<Piece> {
    let mut out = Vec::new();
    // The hook, in a unit cell (x along the side, y from the inner edge
    // outward), entering from the baseline.
    let hook = [
        (0.0, 0.15),
        (0.15, 0.15),
        (0.15, 0.85),
        (0.85, 0.85),
        (0.85, 0.3),
        (0.4, 0.3),
        (0.4, 0.65),
        (0.65, 0.65),
        (0.65, 0.48),
    ];
    for (q, ax, ay) in corner_frames(outer) {
        // This side runs from q along ax; its band lies toward ay.
        let side = if ax.x.abs() > 0.5 {
            outer.width()
        } else {
            outer.height()
        };
        let len = side - 2.0 * b;
        if len > 0.0 {
            let n = ((len / period).round() as usize).max(1);
            let cell = len / n as f64;
            // Cell (u, v) to the page: v 0 at the inner edge.
            let to = |u: f64, v: f64| q + ax * (b + u) + ay * (b * (1.0 - v));
            out.push(stroked(
                polyline(&[to(0.0, 0.15), to(len, 0.15)], false),
                lw,
            ));
            for k in 0..n {
                let pts: Vec<Point> = hook
                    .iter()
                    .map(|(x, y)| to(cell * (k as f64 + x), *y))
                    .collect();
                out.push(stroked(polyline(&pts, false), lw));
            }
        }
        // The corner rosette.
        let at = |x: f64, y: f64| q + ax * (x * b) + ay * (y * b);
        out.push(stroked(
            polyline(
                &[at(0.2, 0.2), at(0.8, 0.2), at(0.8, 0.8), at(0.2, 0.8)],
                true,
            ),
            lw,
        ));
        out.push(filled(polyline(
            &[at(0.5, 0.32), at(0.68, 0.5), at(0.5, 0.68), at(0.32, 0.5)],
            true,
        )));
    }
    out
}

/// The outline of `r` scalloped outward: half circles about `period`
/// across along each side (at most `depth` deep), corner to corner.
fn scalloped(r: Rect, period: f64, depth: f64) -> BezPath {
    let corners = [
        Point::new(r.x0, r.y0),
        Point::new(r.x1, r.y0),
        Point::new(r.x1, r.y1),
        Point::new(r.x0, r.y1),
    ];
    let mut pts = Vec::new();
    for i in 0..4 {
        let (a, c) = (corners[i], corners[(i + 1) % 4]);
        let n = ((a.distance(c) / period).round() as usize).max(1);
        let dir = (c - a) / a.distance(c).max(1e-9);
        // Outward is to the right of a counter-clockwise walk.
        let outward = Vec2::new(dir.y, -dir.x);
        let chord = a.distance(c) / n as f64;
        let h = (chord / 2.0).min(depth.max(chord * 0.15));
        for k in 0..n {
            let p0 = a + dir * (chord * k as f64);
            // A circular arc through both ends and the top, h high.
            let rad = (chord * chord / 4.0 + h * h) / (2.0 * h);
            let centre = p0 + dir * (chord / 2.0) + outward * (h - rad);
            let half = (chord / 2.0 / rad).clamp(-1.0, 1.0).asin();
            let up = outward.atan2();
            let mut seg = Vec::new();
            let (a0, a1) = (up - half, up + half);
            arc_pts(centre, rad, a0, a1, &mut seg);
            // The arc's points run counter-clockwise; walk from p0.
            if seg
                .first()
                .map(|s| s.distance(p0) > seg.last().map_or(0.0, |l| l.distance(p0)))
                .unwrap_or(false)
            {
                seg.reverse();
            }
            pts.extend(seg);
        }
    }
    polyline(&pts, true)
}

/// The point and outward normal at arc length `t` along the rounded
/// rectangle `r` (corner radius `rad`), counter-clockwise from the
/// middle of the bottom side.
pub(crate) fn round_rect_at(r: Rect, rad: f64, t: f64) -> (Point, Vec2) {
    let (w, h) = (r.width() - 2.0 * rad, r.height() - 2.0 * rad);
    let q = FRAC_PI_2 * rad;
    let per = 2.0 * (w + h) + 4.0 * q;
    let mut t = t.rem_euclid(per);
    // Bottom from its middle, then each corner and side in turn.
    let segs: [(f64, u8); 9] = [
        (w / 2.0, 0),
        (q, 1),
        (h, 2),
        (q, 3),
        (w, 4),
        (q, 5),
        (h, 6),
        (q, 7),
        (w / 2.0, 8),
    ];
    for (len, k) in segs {
        if t <= len || k == 8 {
            let f = if len > 0.0 { (t / len).min(1.0) } else { 0.0 };
            let corner = |cx: f64, cy: f64, a0: f64| {
                let a = a0 + FRAC_PI_2 * f;
                let n = Vec2::new(a.cos(), a.sin());
                (Point::new(cx, cy) + n * rad, n)
            };
            return match k {
                0 => (Point::new(r.center().x + t, r.y0), Vec2::new(0.0, -1.0)),
                1 => corner(r.x1 - rad, r.y0 + rad, -FRAC_PI_2),
                2 => (Point::new(r.x1, r.y0 + rad + t), Vec2::new(1.0, 0.0)),
                3 => corner(r.x1 - rad, r.y1 - rad, 0.0),
                4 => (Point::new(r.x1 - rad - t, r.y1), Vec2::new(0.0, 1.0)),
                5 => corner(r.x0 + rad, r.y1 - rad, FRAC_PI_2),
                6 => (Point::new(r.x0, r.y1 - rad - t), Vec2::new(-1.0, 0.0)),
                7 => corner(r.x0 + rad, r.y0 + rad, PI),
                _ => (Point::new(r.x0 + rad + t, r.y0), Vec2::new(0.0, -1.0)),
            };
        }
        t -= len;
    }
    (Point::new(r.x0, r.y0), Vec2::new(0.0, -1.0))
}

/// A wave round the rounded rectangle `r` (corner radius `rad`),
/// `amp` mm either side of it, a whole number of waves about `period`
/// long, starting at `phase`.
fn wave(r: Rect, rad: f64, amp: f64, period: f64, phase: f64) -> BezPath {
    let rad = rad.min(r.width() / 2.0).min(r.height() / 2.0);
    let per = 2.0 * (r.width() + r.height() - 4.0 * rad) + TAU * rad;
    let n = ((per / period).round()).max(1.0);
    let steps = (n as usize * 24).min(20_000);
    let pts: Vec<Point> = (0..steps)
        .map(|i| {
            let t = per * i as f64 / steps as f64;
            let (p, nrm) = round_rect_at(r, rad, t);
            p + nrm * (amp * (TAU * n * t / per + phase).sin())
        })
        .collect();
    polyline(&pts, true)
}

/// One guilloche strand round the rounded rectangle `r`: a wave about
/// `period` long whose height (up to `amp`) follows a wave four times as
/// long, both shifted by `phase`, so neighbouring strands cross and
/// weave.
fn guilloche_strand(r: Rect, rad: f64, amp: f64, period: f64, phase: f64) -> BezPath {
    let rad = rad.min(r.width() / 2.0).min(r.height() / 2.0);
    let per = 2.0 * (r.width() + r.height() - 4.0 * rad) + TAU * rad;
    let n = (per / period).round().max(4.0);
    // The slow wave: a whole number round the frame.
    let slow = (n / 4.0).round().max(1.0);
    let steps = (n as usize * 32).min(40_000);
    let pts: Vec<Point> = (0..steps)
        .map(|i| {
            let t = per * i as f64 / steps as f64;
            let u = TAU * t / per;
            let (p, nrm) = round_rect_at(r, rad, t);
            let swell = 0.55 + 0.45 * (slow * u + phase / 2.0).cos();
            p + nrm * (amp * swell * (n * u + phase).sin())
        })
        .collect();
    polyline(&pts, true)
}

/// A diamond flanked by two dots in the middle of each side of the band.
fn side_ornaments(outer: Rect, b: f64, lw: f64) -> Vec<Piece> {
    let mut out = Vec::new();
    for (q, ax, ay) in corner_frames(outer) {
        let side = if ax.x.abs() > 0.5 {
            outer.width()
        } else {
            outer.height()
        };
        let c = q + ax * (side / 2.0) + ay * (b / 2.0);
        let (l, w) = (b * 0.42, b * 0.24);
        out.push(filled(polyline(
            &[c - ax * l, c - ay * w, c + ax * l, c + ay * w],
            true,
        )));
        for s in [-1.0, 1.0] {
            out.push(filled(circle(
                c + ax * (s * (l + b * 0.22)),
                lw.max(b * 0.07),
            )));
        }
    }
    out
}

/// The rectangle the frame's outer edge follows: round `target` (the
/// selection's bounds) `margin` + band out, or `margin` inside `page`
/// when there is no target.
pub fn outer_rect(target: Option<Rect>, page: Rect, s: &FrameSettings) -> Rect {
    match target {
        Some(t) => t.inflate(s.margin + s.width, s.margin + s.width),
        None => page.inflate(-s.margin, -s.margin),
    }
}

impl crate::app::App {
    /// Apply the Frame Generator: the frame round the selection (inside
    /// the page with nothing selected), one group on top, selected, in
    /// one undo step. False with a status message when it does not fit.
    pub fn apply_frame(&mut self, s: &FrameSettings) -> bool {
        let Some(layer) = self.active_layer() else {
            return false;
        };
        let target = self
            .selected_shapes()
            .iter()
            .map(|sh| sh.page_path().bounding_box())
            .reduce(|a, b| a.union(b));
        let outer = outer_rect(target, self.page_rect(), s);
        if outer.width() < 2.0 * s.width.max(0.5) || outer.height() < 2.0 * s.width.max(0.5) {
            self.status = crate::i18n::tr("status.frame_too_small");
            return false;
        }
        let children: Vec<Shape> = frame(outer, s)
            .into_iter()
            .map(|pc| {
                let closed =
                    pc.width == 0.0 || pc.path.elements().last() == Some(&PathEl::ClosePath);
                let mut sh = Shape::new(
                    self.engine.new_shape_id(),
                    ShapeKind::Path {
                        path: pc.path,
                        closed,
                    },
                );
                if pc.width == 0.0 {
                    sh.fill = Fill::Solid(s.color);
                    sh.stroke = None;
                } else {
                    sh.fill = Fill::None;
                    let mut st = Stroke::new(s.color, pc.width);
                    st.cap = LineCap::Round;
                    st.join = LineJoin::Round;
                    sh.stroke = Some(st);
                }
                sh
            })
            .collect();
        let group_id = self.engine.new_shape_id();
        let mut group = Shape::new(group_id, ShapeKind::Group { children });
        group.name = Some(crate::i18n::tr("docker.frame_generator"));
        if let Err(e) = self.engine.run_batch(
            "Frame",
            &[Command::AddShape {
                layer,
                shape: group,
            }],
        ) {
            self.status = e.to_string();
            return false;
        }
        self.selection = vec![group_id];
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(pieces: &[Piece]) -> Rect {
        pieces
            .iter()
            .map(|p| p.path.bounding_box().inflate(p.width / 2.0, p.width / 2.0))
            .reduce(|a, b| a.union(b))
            .expect("pieces")
    }

    #[test]
    fn every_style_stays_in_its_band() {
        let outer = Rect::new(0.0, 0.0, 180.0, 120.0);
        for style in Style::ALL {
            for corner in Corner::ALL {
                for side_ornaments in [false, true] {
                    let s = FrameSettings {
                        style,
                        corner,
                        side_ornaments,
                        ..Default::default()
                    };
                    let pieces = frame(outer, &s);
                    assert!(pieces.len() >= 3, "{style:?}");
                    let b = bounds(&pieces);
                    let slack = s.line * 2.0;
                    assert!(
                        b.x0 >= -slack
                            && b.y0 >= -slack
                            && b.x1 <= 180.0 + slack
                            && b.y1 <= 120.0 + slack,
                        "{style:?} {corner:?} {b:?}"
                    );
                    // Nothing reaches into the picture area but corner
                    // ornaments (scrolls, fans).
                    let inner = outer.inflate(-s.width - slack, -s.width - slack);
                    let centre = Rect::from_center_size(
                        inner.center(),
                        (inner.width() * 0.4, inner.height() * 0.4),
                    );
                    assert!(
                        pieces.iter().all(|p| {
                            let pb = p.path.bounding_box();
                            pb.intersect(centre).area() <= 0.0 || pb.contains_rect(centre)
                        }),
                        "{style:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn corners_take_away_the_right_area() {
        let r = Rect::new(0.0, 0.0, 40.0, 20.0);
        let area = |c| corner_rect(r, c, 5.0).area().abs();
        assert!((area(Corner::Square) - 800.0).abs() < 1e-6);
        assert!((area(Corner::Round) - (800.0 - (4.0 - PI) * 25.0)).abs() < 0.5);
        assert!((area(Corner::Notch) - (800.0 - PI * 25.0)).abs() < 0.5);
        assert!((area(Corner::Cut) - (800.0 - 2.0 * 25.0)).abs() < 1e-6);
    }

    #[test]
    fn beads_sit_in_every_corner_and_scallops_bulge_out() {
        let r = Rect::new(0.0, 0.0, 40.0, 20.0);
        let pts = along_sides(r, 10.0);
        assert_eq!(pts.len(), 12);
        for c in [(0.0, 0.0), (40.0, 0.0), (40.0, 20.0), (0.0, 20.0)] {
            assert!(pts.iter().any(|p| p.distance(Point::new(c.0, c.1)) < 1e-9));
        }
        let sc = scalloped(r, 10.0, 3.0);
        let b = sc.bounding_box();
        assert!(
            (b.y0 + 3.0).abs() < 0.05 && (b.x1 - 43.0).abs() < 0.05,
            "{b:?}"
        );
        assert!(sc.area().abs() > 800.0);
    }

    #[test]
    fn a_frame_goes_round_the_selection_in_one_step() {
        let mut app = crate::app::App::headless();
        let id = app
            .new_shape(ShapeKind::Rect {
                rect: Rect::new(50.0, 60.0, 110.0, 100.0),
                radius: 0.0,
                corners: None,
            })
            .expect("rect");
        app.select(vec![id]);
        let depth = app.engine.history_labels().0.len();
        let s = FrameSettings::default();
        assert!(app.apply_frame(&s));
        assert_eq!(app.engine.history_labels().0.len(), depth + 1);
        let g = app
            .doc()
            .find_shape(app.selection[0])
            .expect("frame")
            .clone();
        let ShapeKind::Group { children } = &g.kind else {
            panic!("a group")
        };
        assert!(children.len() > 3);
        let b = g.page_path().bounding_box();
        let d = s.margin + s.width;
        assert!(
            (b.x0 - (50.0 - d)).abs() < 1.0 && (b.y1 - (100.0 + d)).abs() < 1.0,
            "{b:?}"
        );
        app.undo();
        assert!(app.doc().find_shape(g.id).is_none());
        // With nothing selected the frame lies inside the page.
        app.select(vec![]);
        assert!(app.apply_frame(&FrameSettings {
            style: Style::Waves,
            ..Default::default()
        }));
        let b = app
            .doc()
            .find_shape(app.selection[0])
            .expect("frame")
            .page_path()
            .bounding_box();
        let page = app.page_rect();
        assert!(
            b.x0 >= page.x0 + 4.0 && b.x1 <= page.x1 - 4.0,
            "{b:?} {page:?}"
        );
    }
}
