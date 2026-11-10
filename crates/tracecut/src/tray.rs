//! Folded Tray: the flat pattern of a tray folded from composite panel
//! (two thin metal skins round a plastic core), such as a sign face with
//! returns or the body of a light box.
//!
//! The finished tray has a face `W x H` (outside sizes) and a return on
//! each side, `d` deep (outside, from the face's front). The pattern is
//! routed from the back: a V-groove along every fold line takes out the
//! core and the back skin and leaves the front skin, which then folds up
//! by hand. The pattern is drawn as seen from the back, the side that is
//! routed, so the return on the left of the finished face lies on the
//! right of the pattern.
//!
//! **Allowance**: every fold takes up a little length, the groove
//! allowance `a` (a quarter of the thickness unless set), half from each
//! of the two parts it joins. Between its fold lines the face is
//! `(W - a) x (H - a)`; a return is `d - a/2` deep, `d - a` with ears on
//! its edge, and an ear is `e - a/2` long, so across the pattern the
//! length is the finished lengths less `a` for each fold.
//!
//! **Corners**: the square between two returns is cut away. The returns
//! of one pair of opposite sides (the left and right, or the top and
//! bottom) run through to the corners; the other two stop the corner
//! relief short of them (the panel's thickness for a butt joint against
//! the through returns' insides), so the cut runs that far along the fold
//! line and the folds never bind. With overlap flaps each end of a
//! through return carries a flap past the corner, joined to it by a fold
//! groove; folded round the corner, it laps over the next return, where
//! it is riveted.
//!
//! **Ears** stand out from the free edge of the returns, fold to make a
//! flange behind the tray and carry a fixing hole each. Ears and return
//! holes are spread evenly over a return's length, `count` of them or as
//! many as keep them at most `spacing` apart, the space to each end half
//! the space between them. **Window**: a light box gets an opening in its
//! face, its edge the inset in from the face's fold lines, with rounded
//! corners.
//!
//! The cut lines are closed (the outline, holes and window, inner ones
//! first); the grooves are open straight lines, routed first.

use crate::cut::fin;
use tracedraw_core::geometry::{BezPath, Point, Rect, Vec2};

/// Which pair of opposite returns runs through to the corners.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Pair {
    #[default]
    LeftRight,
    TopBottom,
}

/// Where the tray is put on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Place {
    /// The gap in from the page's bottom-left corner.
    #[default]
    PageCorner,
    /// The gap to the right of the selection, bottoms level.
    BesideSelection,
}

/// The sides of the finished face as seen from the front, in the order
/// the per-side depths are given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

/// Settings of [`pattern`], lengths in millimetres.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct TraySpec {
    /// The finished face, outside.
    pub width: f64,
    pub height: f64,
    /// The depth of every return, unless `per_side`.
    pub depth: f64,
    pub per_side: bool,
    /// Left, right, top and bottom, seen from the front.
    pub depths: [f64; 4],
    pub thickness: f64,
    /// The allowance is a quarter of the thickness (else `allowance`).
    pub auto_allowance: bool,
    pub allowance: f64,
    pub through: Pair,
    /// How far the cut-back returns stop short of the through ones.
    pub relief: f64,
    pub flaps: bool,
    /// From the flap's fold line to its end.
    pub flap_length: f64,
    pub ears: bool,
    /// The returns that carry ears.
    pub ear_sides: EarSides,
    pub ear_width: f64,
    /// Out from the return's edge, finished.
    pub ear_length: f64,
    pub ear_by_count: bool,
    pub ear_spacing: f64,
    pub ear_count: u32,
    /// 0 for no hole.
    pub ear_hole: f64,
    /// From the ear's fold line to the hole's centre.
    pub ear_hole_offset: f64,
    pub holes: bool,
    pub hole_diameter: f64,
    /// From the face's fold line to the holes' centres.
    pub hole_offset: f64,
    pub hole_spacing: f64,
    pub window: bool,
    /// From the face's fold lines to the window's edge.
    pub window_inset: f64,
    pub window_radius: f64,
    pub place: Place,
    pub gap: f64,
}

/// The returns that carry ears.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum EarSides {
    #[default]
    All,
    LeftRight,
    TopBottom,
}

impl Default for TraySpec {
    fn default() -> Self {
        TraySpec {
            width: 600.0,
            height: 400.0,
            depth: 50.0,
            per_side: false,
            depths: [50.0; 4],
            thickness: 4.0,
            auto_allowance: true,
            allowance: 1.0,
            through: Pair::LeftRight,
            relief: 4.0,
            flaps: false,
            flap_length: 20.0,
            ears: false,
            ear_sides: EarSides::All,
            ear_width: 40.0,
            ear_length: 25.0,
            ear_by_count: false,
            ear_spacing: 400.0,
            ear_count: 2,
            ear_hole: 5.0,
            ear_hole_offset: 12.0,
            holes: false,
            hole_diameter: 6.0,
            hole_offset: 25.0,
            hole_spacing: 300.0,
            window: false,
            window_inset: 30.0,
            window_radius: 10.0,
            place: Place::PageCorner,
            gap: 10.0,
        }
    }
}

pub const SIZE_RANGE: (f64, f64) = (10.0, 10_000.0);
pub const DEPTH_RANGE: (f64, f64) = (1.0, 1000.0);
pub const THICKNESS_RANGE: (f64, f64) = (0.5, 50.0);
pub const ALLOWANCE_RANGE: (f64, f64) = (0.0, 20.0);
pub const RELIEF_RANGE: (f64, f64) = (0.0, 100.0);
pub const FLAP_RANGE: (f64, f64) = (1.0, 1000.0);
pub const EAR_RANGE: (f64, f64) = (1.0, 1000.0);
pub const HOLE_RANGE: (f64, f64) = (0.0, 200.0);
pub const OFFSET_RANGE: (f64, f64) = (0.0, 1000.0);
pub const SPACING_RANGE: (f64, f64) = (10.0, 10_000.0);
pub const COUNT_RANGE: (u32, u32) = (1, 100);
pub const INSET_RANGE: (f64, f64) = (0.0, 5000.0);
pub const RADIUS_RANGE: (f64, f64) = (0.0, 5000.0);
pub const GAP_RANGE: (f64, f64) = (0.0, 10_000.0);

/// The least room kept between ears, beside flaps and round holes.
const CLEAR: f64 = 1.0;

/// Why no tray could be made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayError {
    /// A return or an ear is no deeper than the allowance takes.
    TooShallow,
    /// A side of the face leaves no return between the corner reliefs.
    TooSmall,
    /// The window's inset leaves no opening.
    NoRoomForWindow,
}

impl TraySpec {
    /// The settings with every value finite and in its range.
    pub fn clamped(&self) -> TraySpec {
        let d = TraySpec::default();
        let c = |v: f64, fallback: f64, r: (f64, f64)| fin(v, fallback).clamp(r.0, r.1);
        let mut depths = self.depths;
        for v in &mut depths {
            *v = c(*v, d.depth, DEPTH_RANGE);
        }
        TraySpec {
            width: c(self.width, d.width, SIZE_RANGE),
            height: c(self.height, d.height, SIZE_RANGE),
            depth: c(self.depth, d.depth, DEPTH_RANGE),
            depths,
            thickness: c(self.thickness, d.thickness, THICKNESS_RANGE),
            allowance: c(self.allowance, d.allowance, ALLOWANCE_RANGE),
            relief: c(self.relief, d.relief, RELIEF_RANGE),
            flap_length: c(self.flap_length, d.flap_length, FLAP_RANGE),
            ear_width: c(self.ear_width, d.ear_width, EAR_RANGE),
            ear_length: c(self.ear_length, d.ear_length, EAR_RANGE),
            ear_spacing: c(self.ear_spacing, d.ear_spacing, SPACING_RANGE),
            ear_count: self.ear_count.clamp(COUNT_RANGE.0, COUNT_RANGE.1),
            ear_hole: c(self.ear_hole, d.ear_hole, HOLE_RANGE),
            ear_hole_offset: c(self.ear_hole_offset, d.ear_hole_offset, OFFSET_RANGE),
            hole_diameter: c(self.hole_diameter, d.hole_diameter, HOLE_RANGE),
            hole_offset: c(self.hole_offset, d.hole_offset, OFFSET_RANGE),
            hole_spacing: c(self.hole_spacing, d.hole_spacing, SPACING_RANGE),
            window_inset: c(self.window_inset, d.window_inset, INSET_RANGE),
            window_radius: c(self.window_radius, d.window_radius, RADIUS_RANGE),
            gap: c(self.gap, d.gap, GAP_RANGE),
            ..self.clone()
        }
    }

    /// The groove allowance one fold takes up.
    pub fn allowance(&self) -> f64 {
        let s = self.clamped();
        if s.auto_allowance {
            s.thickness / 4.0
        } else {
            s.allowance
        }
    }

    /// The finished depth of the return on `side`.
    pub fn depth_of(&self, side: Side) -> f64 {
        let s = self.clamped();
        if !s.per_side {
            return s.depth;
        }
        match side {
            Side::Left => s.depths[0],
            Side::Right => s.depths[1],
            Side::Top => s.depths[2],
            Side::Bottom => s.depths[3],
        }
    }

    /// Whether the return on `side` carries ears.
    pub fn ears_on(&self, side: Side) -> bool {
        self.ears
            && match self.ear_sides {
                EarSides::All => true,
                EarSides::LeftRight => matches!(side, Side::Left | Side::Right),
                EarSides::TopBottom => matches!(side, Side::Top | Side::Bottom),
            }
    }

    /// Whether the return on `side` runs through to the corners.
    pub fn runs_through(&self, side: Side) -> bool {
        match self.through {
            Pair::LeftRight => matches!(side, Side::Left | Side::Right),
            Pair::TopBottom => matches!(side, Side::Top | Side::Bottom),
        }
    }
}

/// The flat pattern, as seen from the back, its box's corner at the
/// origin.
#[derive(Debug, Clone, PartialEq)]
pub struct Pattern {
    /// The outer cut line, counter-clockwise, closed back to its start.
    pub outline: Vec<Point>,
    /// Round holes: centre and diameter (ears first, then the returns).
    pub holes: Vec<(Point, f64)>,
    /// The face's opening and its corner radius.
    pub window: Option<(Rect, f64)>,
    /// The fold lines, routed as V-grooves: the face's four sides first
    /// (bottom, right, top, left), then ears and flaps.
    pub grooves: Vec<(Point, Point)>,
    /// The face between its fold lines.
    pub face: Rect,
    /// The whole pattern.
    pub bounds: Rect,
}

impl Pattern {
    /// The cut lines as closed paths: holes and the window first (the
    /// inner cuts), then the outline.
    pub fn cut_paths(&self) -> Vec<BezPath> {
        let mut out: Vec<BezPath> = self
            .holes
            .iter()
            .map(|(c, d)| {
                let r = d / 2.0;
                tracedraw_core::geometry::ellipse_path(Rect::new(
                    c.x - r,
                    c.y - r,
                    c.x + r,
                    c.y + r,
                ))
            })
            .collect();
        if let Some((r, radius)) = self.window {
            out.push(tracedraw_core::geometry::rect_path(r, radius));
        }
        out.push(crate::cut::polygon_path(&self.outline));
        out
    }

    /// The grooves as open straight paths.
    pub fn groove_paths(&self) -> Vec<BezPath> {
        self.grooves
            .iter()
            .map(|(a, b)| crate::cut::line_path(*a, *b))
            .collect()
    }

    /// The pattern moved by `by`.
    pub fn translated(&self, by: Vec2) -> Pattern {
        let r = |r: Rect| r + by;
        Pattern {
            outline: self.outline.iter().map(|p| *p + by).collect(),
            holes: self.holes.iter().map(|(c, d)| (*c + by, *d)).collect(),
            window: self.window.map(|(w, rad)| (r(w), rad)),
            grooves: self
                .grooves
                .iter()
                .map(|(a, b)| (*a + by, *b + by))
                .collect(),
            face: r(self.face),
            bounds: r(self.bounds),
        }
    }
}

/// One return in the working frame (seen from the front, the face's
/// bottom-left fold corner at the origin): `along` its face edge from
/// the corner it starts at, counter-clockwise, and `out` away from the
/// face.
struct Ret {
    origin: Point,
    u: Vec2,
    n: Vec2,
    len: f64,
    /// Flat depth.
    depth: f64,
    through: bool,
    ears: bool,
}

impl Ret {
    fn at(&self, along: f64, out: f64) -> Point {
        self.origin + self.u * along + self.n * out
    }

    /// The stretch of the face edge the return covers.
    fn span(&self, relief: f64) -> (f64, f64) {
        if self.through {
            (0.0, self.len)
        } else {
            (relief, self.len - relief)
        }
    }
}

/// `n` centres spread evenly over `a..b`, the space to each end half the
/// space between them.
fn spread(a: f64, b: f64, n: usize) -> Vec<f64> {
    let step = (b - a) / n.max(1) as f64;
    (0..n).map(|i| a + (i as f64 + 0.5) * step).collect()
}

/// How many go along `len`: `count`, or as many as keep them at most
/// `spacing` apart.
fn how_many(len: f64, by_count: bool, count: u32, spacing: f64) -> usize {
    if by_count {
        count as usize
    } else {
        ((len / spacing - 1e-9).ceil().max(1.0) as usize).min(COUNT_RANGE.1 as usize)
    }
}

/// Push `p` unless it repeats the last point.
fn push(pts: &mut Vec<Point>, p: Point) {
    if pts.last().is_none_or(|q| (*q - p).hypot() > 1e-9) {
        pts.push(p);
    }
}

/// Drop repeated points and points in the middle of a straight run.
fn tidy(pts: Vec<Point>) -> Vec<Point> {
    let mut v = pts;
    while v.len() > 1
        && v.first()
            .zip(v.last())
            .is_some_and(|(a, b)| (*a - *b).hypot() <= 1e-9)
    {
        v.pop();
    }
    let mut changed = true;
    while changed && v.len() > 3 {
        changed = false;
        let n = v.len();
        for i in 0..n {
            let a = v[(i + n - 1) % n];
            let b = v[i];
            let c = v[(i + 1) % n];
            let (ab, bc) = (b - a, c - b);
            if ab.cross(bc).abs() < 1e-9 && ab.dot(bc) >= 0.0 {
                v.remove(i);
                changed = true;
                break;
            }
        }
    }
    v
}

/// The flat pattern of the tray.
pub fn pattern(spec: &TraySpec) -> Result<Pattern, TrayError> {
    let s = spec.clamped();
    let a = spec.allowance();
    let half = a / 2.0;
    let c = s.relief;
    let (fw, fh) = (s.width - a, s.height - a);
    // The face's sides counter-clockwise from its bottom-left corner.
    let sides = [Side::Bottom, Side::Right, Side::Top, Side::Left];
    let corners = [
        Point::new(0.0, 0.0),
        Point::new(fw, 0.0),
        Point::new(fw, fh),
        Point::new(0.0, fh),
    ];
    let dirs = [
        (Vec2::new(1.0, 0.0), Vec2::new(0.0, -1.0)),
        (Vec2::new(0.0, 1.0), Vec2::new(1.0, 0.0)),
        (Vec2::new(-1.0, 0.0), Vec2::new(0.0, 1.0)),
        (Vec2::new(0.0, -1.0), Vec2::new(-1.0, 0.0)),
    ];
    let mut rets = Vec::with_capacity(4);
    for k in 0..4 {
        let side = sides[k];
        let ears = s.ears_on(side);
        let depth = s.depth_of(side) - half - if ears { half } else { 0.0 };
        let len = if k % 2 == 0 { fw } else { fh };
        rets.push(Ret {
            origin: corners[k],
            u: dirs[k].0,
            n: dirs[k].1,
            len,
            depth,
            through: s.runs_through(side),
            ears,
        });
    }
    let ear_len = s.ear_length - half;
    if rets.iter().any(|r| r.depth <= CLEAR) || (s.ears && ear_len <= CLEAR) {
        return Err(TrayError::TooShallow);
    }
    if rets.iter().any(|r| {
        let (a0, a1) = r.span(c);
        a1 - a0 <= CLEAR
    }) {
        return Err(TrayError::TooSmall);
    }

    let mut outline: Vec<Point> = Vec::new();
    let mut grooves: Vec<(Point, Point)> = Vec::new();
    let mut holes: Vec<(Point, f64)> = Vec::new();
    // The face's fold lines first.
    for r in &rets {
        grooves.push((r.at(0.0, 0.0), r.at(r.len, 0.0)));
    }
    for (k, r) in rets.iter().enumerate() {
        let d = r.depth;
        // The flaps keep clear of the face's corner and the return's edge,
        // and stay within the next return's depth.
        let clear = c.max(CLEAR).min(d / 4.0);
        let flap = |next: &Ret| s.flap_length.min(next.depth);
        let prev = &rets[(k + 3) % 4];
        let next = &rets[(k + 1) % 4];
        let (a0, a1) = r.span(c);
        push(&mut outline, r.at(0.0, 0.0));
        if r.through {
            if s.flaps {
                let l = flap(prev);
                for (al, o) in [(0.0, clear), (-l, clear), (-l, d - clear), (0.0, d - clear)] {
                    push(&mut outline, r.at(al, o));
                }
                grooves.push((r.at(0.0, clear), r.at(0.0, d - clear)));
            }
        } else {
            push(&mut outline, r.at(a0, 0.0));
        }
        push(&mut outline, r.at(a0, d));
        if r.ears {
            let n = how_many(a1 - a0, s.ear_by_count, s.ear_count, s.ear_spacing);
            let w = s.ear_width.min((a1 - a0) / n as f64 - 2.0 * CLEAR);
            if w > CLEAR {
                for m in spread(a0, a1, n) {
                    let (e0, e1) = (m - w / 2.0, m + w / 2.0);
                    for (al, o) in [(e0, d), (e0, d + ear_len), (e1, d + ear_len), (e1, d)] {
                        push(&mut outline, r.at(al, o));
                    }
                    grooves.push((r.at(e0, d), r.at(e1, d)));
                    let hd = s.ear_hole;
                    if hd > 0.0 && w >= hd + 2.0 * CLEAR && ear_len >= hd + 2.0 * CLEAR {
                        let off = s
                            .ear_hole_offset
                            .clamp(hd / 2.0 + CLEAR, ear_len - hd / 2.0 - CLEAR);
                        holes.push((r.at(m, d + off), hd));
                    }
                }
            }
        }
        push(&mut outline, r.at(a1, d));
        if r.through {
            if s.flaps {
                let l = flap(next);
                let e = r.len;
                for (al, o) in [
                    (e, d - clear),
                    (e + l, d - clear),
                    (e + l, clear),
                    (e, clear),
                ] {
                    push(&mut outline, r.at(al, o));
                }
                grooves.push((r.at(e, clear), r.at(e, d - clear)));
            }
        } else {
            push(&mut outline, r.at(a1, 0.0));
        }
    }
    // Holes in the returns, the same on each.
    if s.holes && s.hole_diameter > 0.0 {
        let hd = s.hole_diameter;
        for r in &rets {
            let (a0, a1) = r.span(c);
            if r.depth < hd + 2.0 * CLEAR {
                continue;
            }
            let off = s
                .hole_offset
                .clamp(hd / 2.0 + CLEAR, r.depth - hd / 2.0 - CLEAR);
            let n = how_many(a1 - a0, false, 1, s.hole_spacing);
            for m in spread(a0, a1, n) {
                holes.push((r.at(m, off), hd));
            }
        }
    }
    let window = if s.window {
        let i = s.window_inset;
        let w = Rect::new(i, i, fw - i, fh - i);
        if w.width() <= CLEAR || w.height() <= CLEAR {
            return Err(TrayError::NoRoomForWindow);
        }
        Some((w, s.window_radius.min(w.width().min(w.height()) / 2.0)))
    } else {
        None
    };
    let outline = tidy(outline);

    // Seen from the back: mirror left to right about the face's middle,
    // then put the box's corner at the origin.
    let flip = |p: Point| Point::new(fw - p.x, p.y);
    let mut outline: Vec<Point> = outline.into_iter().map(flip).collect();
    outline.reverse();
    let flip_rect = |r: Rect| Rect::new(fw - r.x1, r.y0, fw - r.x0, r.y1);
    let mut grooves: Vec<(Point, Point)> = grooves
        .into_iter()
        .map(|(p, q)| (flip(p), flip(q)))
        .collect();
    // The face's sides in their order seen from the back: bottom, right
    // (the front's left), top, left.
    grooves.swap(1, 3);
    let holes: Vec<(Point, f64)> = holes.into_iter().map(|(p, d)| (flip(p), d)).collect();
    let window = window.map(|(r, rad)| (flip_rect(r), rad));
    let face = Rect::new(0.0, 0.0, fw, fh);
    let bounds = outline
        .iter()
        .fold(None::<Rect>, |b, p| {
            let r = Rect::from_points(*p, *p);
            Some(b.map_or(r, |b| b.union(r)))
        })
        .unwrap_or(face);
    let raw = Pattern {
        outline,
        holes,
        window,
        grooves,
        face,
        bounds,
    };
    Ok(raw.translated(Vec2::new(-bounds.x0, -bounds.y0)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cut::{inside, signed_area};

    const EPS: f64 = 1e-9;

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    /// Whether the segments `p1 p2` and `q1 q2` meet (touching counts).
    fn meet(p1: Point, p2: Point, q1: Point, q2: Point) -> bool {
        let o = |a: Point, b: Point, c: Point| (b - a).cross(c - a);
        let on = |a: Point, b: Point, c: Point| {
            c.x >= a.x.min(b.x) - EPS
                && c.x <= a.x.max(b.x) + EPS
                && c.y >= a.y.min(b.y) - EPS
                && c.y <= a.y.max(b.y) + EPS
        };
        let (d1, d2, d3, d4) = (o(q1, q2, p1), o(q1, q2, p2), o(p1, p2, q1), o(p1, p2, q2));
        if ((d1 > EPS && d2 < -EPS) || (d1 < -EPS && d2 > EPS))
            && ((d3 > EPS && d4 < -EPS) || (d3 < -EPS && d4 > EPS))
        {
            return true;
        }
        (d1.abs() <= EPS && on(q1, q2, p1))
            || (d2.abs() <= EPS && on(q1, q2, p2))
            || (d3.abs() <= EPS && on(p1, p2, q1))
            || (d4.abs() <= EPS && on(p1, p2, q2))
    }

    /// No two sides of the closed outline meet but neighbours at their
    /// shared corner.
    fn simple(pts: &[Point]) -> bool {
        let n = pts.len();
        for i in 0..n {
            for j in i + 1..n {
                if j == i + 1 || (i == 0 && j == n - 1) {
                    continue;
                }
                if meet(pts[i], pts[(i + 1) % n], pts[j], pts[(j + 1) % n]) {
                    return false;
                }
            }
        }
        true
    }

    fn all_specs() -> Vec<TraySpec> {
        let mut out = Vec::new();
        for through in [Pair::LeftRight, Pair::TopBottom] {
            for flaps in [false, true] {
                for ears in [false, true] {
                    for relief in [0.0, 4.0] {
                        out.push(TraySpec {
                            through,
                            flaps,
                            ears,
                            relief,
                            holes: true,
                            window: true,
                            per_side: true,
                            depths: [40.0, 60.0, 50.0, 70.0],
                            ear_sides: EarSides::All,
                            ..Default::default()
                        });
                    }
                }
            }
        }
        out
    }

    #[test]
    fn the_flat_size_is_the_face_and_returns_less_the_allowance() {
        let spec = TraySpec::default();
        assert!(near(spec.allowance(), 1.0), "a quarter of 4 mm");
        let p = pattern(&spec).expect("tray");
        // 600 + 2 x 50 less two folds of 1 mm; 400 + 2 x 50 likewise.
        assert!(near(p.bounds.width(), 698.0), "{:?}", p.bounds);
        assert!(near(p.bounds.height(), 498.0), "{:?}", p.bounds);
        assert!(near(p.face.width(), 599.0) && near(p.face.height(), 399.0));
        // Returns on each side, d - a/2 deep.
        assert!(
            near(p.face.x0, 49.5) && near(p.face.y0, 49.5),
            "{:?}",
            p.face
        );
        // A set allowance.
        let set = TraySpec {
            auto_allowance: false,
            allowance: 3.0,
            ..Default::default()
        };
        let q = pattern(&set).expect("tray");
        assert!(near(q.bounds.width(), 600.0 + 100.0 - 6.0));
        assert!(near(q.face.width(), 597.0));
        // No allowance: the flat size is the sum of the finished ones.
        let none = TraySpec {
            auto_allowance: false,
            allowance: 0.0,
            ..Default::default()
        };
        let z = pattern(&none).expect("tray");
        assert!(near(z.bounds.width(), 700.0) && near(z.bounds.height(), 500.0));
    }

    #[test]
    fn ears_add_their_length_and_one_more_fold() {
        let spec = TraySpec {
            ears: true,
            ear_length: 25.0,
            ..Default::default()
        };
        let p = pattern(&spec).expect("tray");
        // Per axis: face + 2 returns + 2 ears, less four folds of 1 mm.
        assert!(
            near(p.bounds.width(), 600.0 + 100.0 + 50.0 - 4.0),
            "{:?}",
            p.bounds
        );
        assert!(near(p.bounds.height(), 400.0 + 100.0 + 50.0 - 4.0));
        // The face sits behind an ear (24.5) and a return (49).
        assert!(near(p.face.x0, 24.5 + 49.0), "{:?}", p.face);
        // Only on the left and right.
        let lr = TraySpec {
            ear_sides: EarSides::LeftRight,
            ..spec.clone()
        };
        let q = pattern(&lr).expect("tray");
        assert!(near(q.bounds.width(), 746.0) && near(q.bounds.height(), 498.0));
    }

    #[test]
    fn per_side_depths_are_mirrored_as_seen_from_the_back() {
        let spec = TraySpec {
            per_side: true,
            depths: [30.0, 60.0, 40.0, 80.0],
            ..Default::default()
        };
        let p = pattern(&spec).expect("tray");
        // The front's right return (60) lies on the left of the pattern,
        // its left one (30) on the right; top and bottom stay.
        assert!(near(p.face.x0, 59.5), "{:?}", p.face);
        assert!(near(p.bounds.x1 - p.face.x1, 29.5));
        assert!(near(p.face.y0, 79.5) && near(p.bounds.y1 - p.face.y1, 39.5));
        assert!(near(p.bounds.width(), 600.0 + 90.0 - 2.0));
        assert!(near(p.bounds.height(), 400.0 + 120.0 - 2.0));
        // One depth for all unless per side.
        let one = TraySpec {
            per_side: false,
            ..spec
        };
        assert!(near(one.depth_of(Side::Left), 50.0));
    }

    #[test]
    fn the_face_fold_lines_are_the_face_edges() {
        for spec in all_specs() {
            let p = pattern(&spec).expect("tray");
            let f = p.face;
            let edges = [
                (Point::new(f.x0, f.y0), Point::new(f.x1, f.y0)),
                (Point::new(f.x1, f.y0), Point::new(f.x1, f.y1)),
                (Point::new(f.x1, f.y1), Point::new(f.x0, f.y1)),
                (Point::new(f.x0, f.y1), Point::new(f.x0, f.y0)),
            ];
            for (k, (a, b)) in edges.iter().enumerate() {
                let (g0, g1) = p.grooves[k];
                let same = |x: Point, y: Point| (x - y).hypot() < 1e-9;
                assert!(
                    (same(g0, *a) && same(g1, *b)) || (same(g0, *b) && same(g1, *a)),
                    "side {k}: {:?} vs {:?}",
                    p.grooves[k],
                    (a, b)
                );
            }
            // The face lies inside the outline, its middle too.
            for q in [f.center(), Point::new(f.x0 + 0.5, f.y0 + 0.5)] {
                assert!(inside(q, &p.outline));
            }
        }
    }

    #[test]
    fn the_outline_is_closed_counter_clockwise_and_never_crosses_itself() {
        // The check itself sees a crossing and a touch.
        let p = |x: f64, y: f64| Point::new(x, y);
        assert!(!simple(&[
            p(0.0, 0.0),
            p(10.0, 10.0),
            p(10.0, 0.0),
            p(0.0, 10.0)
        ]));
        assert!(!simple(&[
            p(0.0, 0.0),
            p(10.0, 0.0),
            p(10.0, 10.0),
            p(5.0, 0.0),
            p(0.0, 10.0)
        ]));
        assert!(simple(&[
            p(0.0, 0.0),
            p(10.0, 0.0),
            p(10.0, 10.0),
            p(0.0, 10.0)
        ]));
        for spec in all_specs() {
            let p = pattern(&spec).expect("tray");
            assert!(p.outline.len() >= 8);
            assert!(signed_area(&p.outline) > 0.0);
            assert!(simple(&p.outline), "{spec:?}");
            // The path is closed; the grooves are open lines.
            let cut = p.cut_paths();
            let last = cut.last().expect("outline");
            assert!(matches!(
                last.elements().last(),
                Some(tracedraw_core::geometry::PathEl::ClosePath)
            ));
            for g in p.groove_paths() {
                assert_eq!(g.elements().len(), 2);
                assert!(!g
                    .elements()
                    .iter()
                    .any(|e| matches!(e, tracedraw_core::geometry::PathEl::ClosePath)));
            }
            // Every hole and the window lie inside the outline.
            for (c, d) in &p.holes {
                for q in [
                    *c + Vec2::new(d / 2.0, 0.0),
                    *c - Vec2::new(d / 2.0, 0.0),
                    *c + Vec2::new(0.0, d / 2.0),
                    *c - Vec2::new(0.0, d / 2.0),
                ] {
                    assert!(inside(q, &p.outline), "{c:?} {spec:?}");
                }
            }
            let (w, _) = p.window.expect("window");
            assert!(w.x0 > p.face.x0 && w.x1 < p.face.x1);
        }
    }

    #[test]
    fn corners_are_cut_away_with_the_relief_on_the_cut_back_returns() {
        let spec = TraySpec::default();
        let p = pattern(&spec).expect("tray");
        let f = p.face;
        let has = |x: f64, y: f64| inside(Point::new(x, y), &p.outline);
        // The corner square is gone.
        assert!(!has(f.x0 - 10.0, f.y0 - 10.0));
        assert!(!has(f.x1 + 10.0, f.y1 + 10.0));
        // Left and right run through: material right up to the corner.
        assert!(has(f.x0 - 10.0, f.y0 + 0.1));
        assert!(has(f.x1 + 10.0, f.y1 - 0.1));
        // Top and bottom stop the relief (4 mm) short of the corner.
        assert!(!has(f.x0 + 3.9, f.y0 - 10.0));
        assert!(has(f.x0 + 4.1, f.y0 - 10.0));
        assert!(!has(f.x1 - 3.9, f.y1 + 10.0));
        assert!(has(f.x1 - 4.1, f.y1 + 10.0));
        // The other pair the other way round.
        let tb = TraySpec {
            through: Pair::TopBottom,
            ..spec
        };
        let q = pattern(&tb).expect("tray");
        let f = q.face;
        let has = |x: f64, y: f64| inside(Point::new(x, y), &q.outline);
        assert!(has(f.x0 + 0.1, f.y0 - 10.0));
        assert!(!has(f.x0 - 10.0, f.y0 + 3.9));
        assert!(has(f.x0 - 10.0, f.y0 + 4.1));
    }

    #[test]
    fn flaps_reach_past_the_corner_on_the_through_returns() {
        let spec = TraySpec {
            flaps: true,
            flap_length: 20.0,
            ..Default::default()
        };
        let p = pattern(&spec).expect("tray");
        let f = p.face;
        let has = |x: f64, y: f64| inside(Point::new(x, y), &p.outline);
        // The left return's flap below the bottom-left corner, clear of
        // the return's edges by the relief.
        assert!(has(f.x0 - 25.0, f.y0 - 19.9));
        assert!(!has(f.x0 - 25.0, f.y0 - 20.1));
        assert!(!has(f.x0 - 2.0, f.y0 - 5.0));
        assert!(!has(f.x0 - 47.0, f.y0 - 5.0));
        // Four flaps with a fold groove each: 4 face sides + 4.
        assert_eq!(p.grooves.len(), 8);
        let (a, b) = p.grooves[4];
        assert!(near((a - b).hypot(), 49.5 - 8.0));
        // Flaps never reach past the next return's depth.
        let long = TraySpec {
            flap_length: 500.0,
            ..spec
        };
        let q = pattern(&long).expect("tray");
        assert!(near(q.bounds.width(), 698.0) && near(q.bounds.height(), 498.0));
    }

    #[test]
    fn ears_and_holes_are_spread_symmetrically() {
        let spec = TraySpec {
            ears: true,
            ear_by_count: true,
            ear_count: 3,
            ear_width: 40.0,
            ear_hole: 5.0,
            ear_hole_offset: 12.0,
            holes: true,
            hole_spacing: 250.0,
            ..Default::default()
        };
        let p = pattern(&spec).expect("tray");
        let f = p.face;
        let ear_holes: Vec<Point> = p
            .holes
            .iter()
            .filter(|(_, d)| near(*d, 5.0))
            .map(|(c, _)| *c)
            .collect();
        assert_eq!(ear_holes.len(), 12);
        // Bottom ears: below the return, mirrored about the middle.
        let mut bottom: Vec<f64> = ear_holes
            .iter()
            .filter(|c| c.y < f.y0 - 49.0)
            .map(|c| c.x)
            .collect();
        bottom.sort_by(f64::total_cmp);
        assert_eq!(bottom.len(), 3);
        let mid = f.center().x;
        assert!(near(bottom[1], mid));
        assert!(near(mid - bottom[0], bottom[2] - mid));
        // The bottom return is cut back by the relief at both ends.
        assert!(near(bottom[0], f.x0 + 4.0 + (599.0 - 8.0) / 6.0));
        // The hole is the offset out from the ear's fold line.
        let fold = f.y0 - 49.0;
        for c in ear_holes.iter().filter(|c| c.y < f.y0 - 49.0) {
            assert!(near(fold - c.y, 12.0), "{c:?}");
        }
        // Return holes: at most 250 apart, 25 from the face's fold.
        let ret: Vec<Point> = p
            .holes
            .iter()
            .filter(|(_, d)| near(*d, 6.0))
            .map(|(c, _)| *c)
            .collect();
        let mut bottom: Vec<f64> = ret
            .iter()
            .filter(|c| near(c.y, f.y0 - 25.0))
            .map(|c| c.x)
            .collect();
        bottom.sort_by(f64::total_cmp);
        assert_eq!(bottom.len(), 3);
        assert!(near(mid - bottom[0], bottom[2] - mid));
        let left: Vec<&Point> = ret.iter().filter(|c| near(c.x, f.x0 - 25.0)).collect();
        assert_eq!(left.len(), 2, "399 mm at most 250 apart");
        // By spacing, ears are counted the same way.
        let spaced = TraySpec {
            ear_by_count: false,
            ear_spacing: 200.0,
            holes: false,
            ..spec
        };
        let q = pattern(&spaced).expect("tray");
        // 591 / 200 -> 3 on the bottom and top, 399 / 200 -> 2 on the
        // sides.
        assert_eq!(q.holes.len(), 3 + 3 + 2 + 2);
        assert_eq!(q.grooves.len(), 4 + 10);
    }

    #[test]
    fn the_window_is_inset_from_the_fold_lines() {
        let spec = TraySpec {
            window: true,
            window_inset: 30.0,
            window_radius: 10.0,
            ..Default::default()
        };
        let p = pattern(&spec).expect("tray");
        let (w, r) = p.window.expect("window");
        let f = p.face;
        assert!(near(w.x0 - f.x0, 30.0) && near(f.x1 - w.x1, 30.0));
        assert!(near(w.y0 - f.y0, 30.0) && near(f.y1 - w.y1, 30.0));
        assert!(near(r, 10.0));
        // The radius is never more than half the opening.
        let round = TraySpec {
            window_radius: 1000.0,
            ..spec.clone()
        };
        let (w, r) = pattern(&round).expect("tray").window.expect("window");
        assert!(near(r, w.height() / 2.0));
        let shut = TraySpec {
            window_inset: 300.0,
            ..spec
        };
        assert_eq!(pattern(&shut), Err(TrayError::NoRoomForWindow));
    }

    #[test]
    fn shallow_small_and_damaged_settings() {
        // A return no deeper than the allowance takes.
        let shallow = TraySpec {
            depth: 1.0,
            thickness: 8.0,
            ..Default::default()
        };
        assert_eq!(pattern(&shallow), Err(TrayError::TooShallow));
        let small = TraySpec {
            width: 10.0,
            relief: 10.0,
            through: Pair::LeftRight,
            ..Default::default()
        };
        assert_eq!(pattern(&small), Err(TrayError::TooSmall));
        let odd = TraySpec {
            width: f64::NAN,
            depth: f64::INFINITY,
            thickness: -2.0,
            relief: f64::NAN,
            ear_count: 0,
            depths: [f64::NAN; 4],
            ..Default::default()
        };
        let p = pattern(&odd).expect("defaults stand in");
        assert!(simple(&p.outline));
        assert!(p.bounds.width().is_finite());
    }

    #[test]
    fn the_pattern_starts_at_the_origin_and_moves_whole() {
        let p = pattern(&TraySpec::default()).expect("tray");
        assert!(near(p.bounds.x0, 0.0) && near(p.bounds.y0, 0.0));
        let m = p.translated(Vec2::new(10.0, 20.0));
        assert!(near(m.bounds.x0, 10.0) && near(m.face.y0, p.face.y0 + 20.0));
        assert_eq!(m.cut_paths().len(), 1);
        assert_eq!(m.groove_paths().len(), 4);
    }
}
