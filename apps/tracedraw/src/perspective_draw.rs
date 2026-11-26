//! Draw in Perspective (View > Draw in Perspective): a one-, two- or
//! three-point perspective grid over the page. While it is on, every
//! object a drawing tool makes is laid onto the active plane of the grid:
//! the box it was drawn in is taken onto the plane and the object gets a
//! Perspective effect that maps it there exactly (a plane in perspective
//! is a projective image of a flat rectangle), so it stays editable with
//! the Shape tool. The vanishing points and the grid's near corner are
//! dragged on the page.
//!
//! The 4 Corners grid is a plane set by clicking its four corners on the
//! page, over a photo's floor, wall or road; Ctrl+drag on the middle of
//! one of its sides pulls up a plane standing on that side, square to
//! it in the photo's perspective. With the Pick tool, an object let go
//! inside the active plane goes onto it, and an object already on it
//! slides along it, staying the object it was (a text stays text).

use crate::ui::btn_icons::IconUi;
use crate::app::App;
use crate::tools::Tool;
use crate::ui::tips::RichTip;
use egui::{Color32, Painter, Pos2, Stroke as EStroke};
use std::collections::HashSet;
use tracedraw_core::geometry::{Affine, Point, Rect};
use tracedraw_core::live::{homography, map_homography, Effect};
use tracedraw_core::{Command, ShapeId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    One,
    Two,
    Three,
    /// A plane set by its four corners, with planes standing on its sides.
    Corners,
}

impl Kind {
    pub const ALL: [Kind; 4] = [Kind::One, Kind::Two, Kind::Three, Kind::Corners];
    pub fn key(self) -> &'static str {
        match self {
            Kind::One => "perspective.one_point",
            Kind::Two => "perspective.two_point",
            Kind::Three => "perspective.three_point",
            Kind::Corners => "perspective.four_corners",
        }
    }
    /// The planes the kind offers (with 4 Corners, those it can have).
    pub fn planes(self) -> &'static [Plane] {
        match self {
            Kind::One => &[Plane::Ground, Plane::Left, Plane::Right, Plane::Top],
            Kind::Two | Kind::Three => &[Plane::Ground, Plane::Left, Plane::Right],
            Kind::Corners => &[
                Plane::Ground,
                Plane::Front,
                Plane::Right,
                Plane::Back,
                Plane::Left,
            ],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plane {
    Ground,
    Left,
    Right,
    Top,
    /// 4 Corners: the plane standing on the first side (first to second
    /// corner).
    Front,
    /// 4 Corners: the plane standing on the third side.
    Back,
}

impl Plane {
    pub fn key(self) -> &'static str {
        match self {
            Plane::Ground => "perspective.ground",
            Plane::Left => "perspective.left",
            Plane::Right => "perspective.right",
            Plane::Top => "perspective.top",
            Plane::Front => "perspective.front",
            Plane::Back => "perspective.back",
        }
    }

    /// 4 Corners: the side of the base plane a standing plane is on.
    pub fn side(self) -> Option<usize> {
        match self {
            Plane::Front => Some(0),
            Plane::Right => Some(1),
            Plane::Back => Some(2),
            Plane::Left => Some(3),
            Plane::Ground | Plane::Top => None,
        }
    }

    /// 4 Corners: the standing plane on side `i`.
    pub fn on_side(i: usize) -> Plane {
        match i % 4 {
            0 => Plane::Front,
            1 => Plane::Right,
            2 => Plane::Back,
            _ => Plane::Left,
        }
    }
}

/// What can be dragged on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handle {
    /// A vanishing point (0, 1, 2).
    Vanish(usize),
    /// The near bottom corner of the grid.
    Corner,
    /// The near top corner (sets the walls' height).
    Top,
    /// 4 Corners: a corner of the base plane.
    QuadCorner(usize),
    /// 4 Corners: the middle of a side (drags the side; with Ctrl, pulls
    /// up a plane standing on it).
    Side(usize),
    /// 4 Corners: the middle of the top side of the plane standing on a
    /// side (its height).
    WallTop(usize),
}

#[derive(Debug, Clone)]
pub struct Session {
    pub kind: Kind,
    pub plane: Plane,
    /// Vanishing points: [left or only one, right, vertical].
    pub vanish: [Point; 3],
    /// The near bottom corner (two- and three-point) or near bottom-left
    /// corner (one-point) of the grid.
    pub corner: Point,
    /// The near top corner's height above `corner`, mm.
    pub height: f64,
    /// One-point: the near face's width, mm.
    pub width: f64,
    /// How far towards the vanishing points the grid reaches (0 to 1).
    pub depth: f64,
    pub grid: bool,
    pub drag: Option<Handle>,
    /// Objects already on the page (not laid onto the grid).
    pub known: HashSet<ShapeId>,
    /// 4 Corners: the base plane's corners, in the order of the unit
    /// square's (0,0), (1,0), (1,1), (0,1).
    pub corners: [Point; 4],
    /// 4 Corners: the height of the plane standing on each side, page mm
    /// from the side's middle towards the vertical vanishing point (less
    /// than nothing: away from it); none where no plane stands.
    pub walls: [Option<f64>; 4],
    /// 4 Corners: the corners clicked so far while the plane is being
    /// set; none once it is.
    pub placing: Option<Vec<Point>>,
    /// The page's middle: where the camera is taken to point (the
    /// standing planes' vertical vanishing point is worked out from it).
    pub centre: Point,
    /// Where the pointer was at the last step of a side's drag.
    pub last: Option<Point>,
}

impl Session {
    /// A grid sized to a page `w` x `h` mm.
    pub fn new(kind: Kind, w: f64, h: f64) -> Self {
        let horizon = h * 0.55;
        Session {
            kind,
            plane: Plane::Ground,
            vanish: [
                Point::new(
                    if kind == Kind::One {
                        w * 0.5
                    } else {
                        -w * 0.35
                    },
                    horizon,
                ),
                Point::new(w * 1.35, horizon),
                Point::new(w * 0.5, h * 2.2),
            ],
            corner: if kind == Kind::One {
                Point::new(w * 0.15, h * 0.08)
            } else {
                Point::new(w * 0.5, h * 0.08)
            },
            height: h * 0.75,
            width: w * 0.7,
            depth: if kind == Kind::One { 0.7 } else { 0.45 },
            grid: true,
            drag: None,
            known: HashSet::new(),
            corners: [
                Point::new(w * 0.15, h * 0.08),
                Point::new(w * 0.85, h * 0.08),
                Point::new(w * 0.6, h * 0.45),
                Point::new(w * 0.4, h * 0.45),
            ],
            walls: [None; 4],
            placing: (kind == Kind::Corners).then(Vec::new),
            centre: Point::new(w * 0.5, h * 0.5),
            last: None,
        }
    }

    /// The planes this grid has: with 4 Corners, the base plane and the
    /// planes standing on its sides.
    pub fn planes(&self) -> Vec<Plane> {
        match self.kind {
            Kind::Corners => std::iter::once(Plane::Ground)
                .chain((0..4).filter(|i| self.walls[*i].is_some()).map(Plane::on_side))
                .collect(),
            k => k.planes().to_vec(),
        }
    }

    /// 4 Corners: the vanishing points of the base plane's two pairs of
    /// sides (homogeneous: at infinity for sides that do not meet).
    fn side_vanishing(&self) -> [H; 2] {
        let c = self.corners.map(hp);
        [
            cross(cross(c[0], c[1]), cross(c[3], c[2])),
            cross(cross(c[1], c[2]), cross(c[0], c[3])),
        ]
    }

    /// 4 Corners: the vanishing point of lines standing square to the
    /// base plane. With both of its vanishing points on the page's plane
    /// and the page's middle between them, it is the point making the
    /// middle the meeting point of the heights of the triangle of the
    /// three (a camera pointed at the middle); otherwise the standing
    /// lines are parallel, square to the horizon.
    pub fn vertical_vanishing(&self) -> H {
        let [v1, v2] = self.side_vanishing();
        let horizon = cross(v1, v2);
        let square = || {
            let n = (horizon[0], horizon[1]);
            if n.0.hypot(n.1) < 1e-12 * horizon[2].abs().max(1.0) {
                [0.0, 1.0, 0.0]
            } else {
                [n.0, n.1, 0.0]
            }
        };
        let (Some(a), Some(b)) = (point_of(v1), point_of(v2)) else {
            return square();
        };
        let p = self.centre;
        if (a - p).dot(b - p) >= 0.0 {
            return square();
        }
        // (V3 - P).(V1 - V2) = 0 and (V3 - V1).(V2 - P) = 0.
        let d = a - b;
        let e = b - p;
        let det = d.x * e.y - d.y * e.x;
        if det.abs() < 1e-12 {
            return square();
        }
        let r1 = d.x * p.x + d.y * p.y;
        let r2 = e.x * a.x + e.y * a.y;
        let x = (r1 * e.y - d.y * r2) / det;
        let y = (d.x * r2 - r1 * e.x) / det;
        if !x.is_finite() || !y.is_finite() {
            return square();
        }
        [x, y, 1.0]
    }

    /// 4 Corners: the base plane's quad, turned so that it is not seen
    /// from beneath (what goes onto it reads the right way round).
    fn base_quad(&self) -> [Point; 4] {
        let c = self.corners;
        if signed_area(&c) < 0.0 {
            [c[1], c[0], c[3], c[2]]
        } else {
            c
        }
    }

    /// 4 Corners: the quad of the plane standing on side `i`, `h` mm
    /// high at the side's middle.
    fn wall_quad(&self, i: usize, h: f64) -> Option<[Point; 4]> {
        let (a, b) = (self.corners[i % 4], self.corners[(i + 1) % 4]);
        let v3 = self.vertical_vanishing();
        let along = self.side_vanishing()[i % 2];
        let m = a.midpoint(b);
        let m_top = m + toward_unit(m, v3)? * h;
        let top = cross(hp(m_top), along);
        let a_top = point_of(cross(top, cross(hp(a), v3)))?;
        let b_top = point_of(cross(top, cross(hp(b), v3)))?;
        let q = [a, b, b_top, a_top];
        Some(if signed_area(&q) < 0.0 {
            [b, a, a_top, b_top]
        } else {
            q
        })
    }

    /// 4 Corners: the middle of the top side of the plane standing on
    /// side `i`.
    fn wall_top(&self, i: usize) -> Option<Point> {
        let h = self.walls.get(i).copied().flatten()?;
        let (a, b) = (self.corners[i % 4], self.corners[(i + 1) % 4]);
        let m = a.midpoint(b);
        Some(m + toward_unit(m, self.vertical_vanishing())? * h)
    }

    fn top_corner(&self) -> Point {
        let c = self.corner;
        match self.kind {
            Kind::Three => {
                // The vertical edge leans to the vertical vanishing point.
                let v = self.vanish[2];
                let d = v - c;
                let t = if d.y.abs() > 1e-9 {
                    self.height / d.y
                } else {
                    0.0
                };
                c + d * t.clamp(-1.0, 0.95)
            }
            _ => Point::new(c.x, c.y + self.height),
        }
    }

    /// The quad (page mm) the unit square of `plane` maps to, corners in
    /// the order (0,0), (1,0), (1,1), (0,1).
    pub fn quad(&self, plane: Plane) -> Option<[Point; 4]> {
        if self.kind == Kind::Corners {
            return match plane.side() {
                None => Some(self.base_quad()),
                Some(i) => self.wall_quad(i, self.walls[i]?),
            };
        }
        let f = self.depth.clamp(0.05, 0.98);
        let toward = |p: Point, v: Point| p + (v - p) * f;
        let c = self.corner;
        let top = self.top_corner();
        match self.kind {
            Kind::One => {
                let v = self.vanish[0];
                let (l, r) = (c.x, c.x + self.width);
                let (b, t) = (c.y, top.y);
                let (lb, rb, lt, rt) = (
                    Point::new(l, b),
                    Point::new(r, b),
                    Point::new(l, t),
                    Point::new(r, t),
                );
                Some(match plane {
                    Plane::Ground => [lb, rb, toward(rb, v), toward(lb, v)],
                    Plane::Left => [lb, toward(lb, v), toward(lt, v), lt],
                    Plane::Right => [toward(rb, v), rb, rt, toward(rt, v)],
                    Plane::Top => [toward(lt, v), toward(rt, v), rt, lt],
                    Plane::Front | Plane::Back => return None,
                })
            }
            Kind::Corners => None,
            Kind::Two | Kind::Three => {
                let (v1, v2) = (self.vanish[0], self.vanish[1]);
                let left_b = toward(c, v1);
                let right_b = toward(c, v2);
                // Far top corners: towards the side point from the top
                // corner, up the edge that leans to the vertical point (or
                // stands upright).
                let up = |p: Point| -> Point {
                    if self.kind == Kind::Three {
                        self.vanish[2]
                    } else {
                        Point::new(p.x, p.y + 1.0)
                    }
                };
                let left_t = intersect(top, v1, left_b, up(left_b))?;
                let right_t = intersect(top, v2, right_b, up(right_b))?;
                Some(match plane {
                    Plane::Ground => {
                        let far = intersect(left_b, v2, right_b, v1)?;
                        [c, right_b, far, left_b]
                    }
                    Plane::Left => [left_b, c, top, left_t],
                    Plane::Right | Plane::Top => [c, right_b, right_t, top],
                    Plane::Front | Plane::Back => return None,
                })
            }
        }
    }

    /// The homography from the unit square to the plane, and back.
    pub fn maps(&self, plane: Plane) -> Option<([f64; 8], [f64; 8])> {
        let q = self.quad(plane)?;
        let unit = [
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(1.0, 1.0),
            Point::new(0.0, 1.0),
        ];
        Some((homography(&unit, &q)?, homography(&q, &unit)?))
    }

    /// The page positions of the handles.
    pub fn handles(&self) -> Vec<(Handle, Point)> {
        if self.kind == Kind::Corners {
            if self.placing.is_some() {
                return Vec::new();
            }
            let c = self.corners;
            let mut h: Vec<(Handle, Point)> = (0..4)
                .filter_map(|i| self.wall_top(i).map(|p| (Handle::WallTop(i), p)))
                .collect();
            h.extend((0..4).map(|i| (Handle::Side(i), c[i].midpoint(c[(i + 1) % 4]))));
            h.extend((0..4).map(|i| (Handle::QuadCorner(i), c[i])));
            return h;
        }
        let mut h = vec![(Handle::Vanish(0), self.vanish[0])];
        if self.kind != Kind::One {
            h.push((Handle::Vanish(1), self.vanish[1]));
        }
        if self.kind == Kind::Three {
            h.push((Handle::Vanish(2), self.vanish[2]));
        }
        h.push((Handle::Corner, self.corner));
        let top = self.top_corner();
        h.push((
            Handle::Top,
            if self.kind == Kind::One {
                Point::new(self.corner.x + self.width, top.y)
            } else {
                top
            },
        ));
        h
    }

    /// Move a handle to `p`.
    pub fn drag_to(&mut self, handle: Handle, p: Point) {
        match handle {
            Handle::Vanish(i) => {
                if let Some(v) = self.vanish.get_mut(i) {
                    *v = p;
                }
                // The side points share the horizon.
                if self.kind != Kind::One && i < 2 {
                    let y = p.y;
                    self.vanish[0].y = y;
                    self.vanish[1].y = y;
                }
            }
            Handle::Corner => self.corner = p,
            Handle::QuadCorner(i) => {
                if let Some(c) = self.corners.get_mut(i) {
                    *c = p;
                }
            }
            Handle::Side(i) => {
                // The side moves with the pointer, both its corners.
                let d = self.last.map_or(tracedraw_core::geometry::Vec2::ZERO, |l| p - l);
                self.corners[i % 4] += d;
                self.corners[(i + 1) % 4] += d;
                self.last = Some(p);
            }
            Handle::WallTop(i) => {
                let (a, b) = (self.corners[i % 4], self.corners[(i + 1) % 4]);
                let m = a.midpoint(b);
                if let Some(u) = toward_unit(m, self.vertical_vanishing()) {
                    self.walls[i % 4] = Some((p - m).dot(u));
                }
            }
            Handle::Top => {
                self.height = (p.y - self.corner.y).max(1.0);
                if self.kind == Kind::One {
                    self.width = (p.x - self.corner.x).max(1.0);
                }
            }
        }
    }
}

/// A point or a line in homogeneous coordinates.
type H = [f64; 3];

fn hp(p: Point) -> H {
    [p.x, p.y, 1.0]
}

fn cross(a: H, b: H) -> H {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// The page point of a homogeneous point; none at (or near) infinity.
fn point_of(h: H) -> Option<Point> {
    let scale = h[0].abs().max(h[1].abs());
    if h[2].abs() <= 1e-9 * scale || h[2] == 0.0 {
        return None;
    }
    let p = Point::new(h[0] / h[2], h[1] / h[2]);
    (p.x.is_finite() && p.y.is_finite()).then_some(p)
}

/// The unit direction from `p` towards the homogeneous point `v` (along
/// it, when it is at infinity).
fn toward_unit(p: Point, v: H) -> Option<tracedraw_core::geometry::Vec2> {
    let d = match point_of(v) {
        Some(q) => q - p,
        None => tracedraw_core::geometry::Vec2::new(v[0], v[1]),
    };
    let n = d.hypot();
    (n > 1e-12 && n.is_finite()).then(|| d / n)
}

/// Twice the signed area of a quad (more than nothing: anticlockwise,
/// with the page's y up).
fn signed_area(q: &[Point; 4]) -> f64 {
    (0..4)
        .map(|i| {
            let (a, b) = (q[i], q[(i + 1) % 4]);
            a.x * b.y - b.x * a.y
        })
        .sum()
}

/// Where the line a→b crosses the line c→d.
fn intersect(a: Point, b: Point, c: Point, d: Point) -> Option<Point> {
    let r = b - a;
    let s = d - c;
    let den = r.x * s.y - r.y * s.x;
    if den.abs() < 1e-12 {
        return None;
    }
    let t = ((c.x - a.x) * s.y - (c.y - a.y) * s.x) / den;
    Some(a + r * t)
}

/// The perspective corners (local coordinates) that lay an object with
/// local box `b` and page transform `t` onto a plane with maps
/// `(to_page, to_plane)`.
pub fn laid_corners(
    b: Rect,
    t: Affine,
    to_page: &[f64; 8],
    to_plane: &[f64; 8],
) -> Option<[Point; 4]> {
    let local = [
        Point::new(b.x0, b.y0),
        Point::new(b.x1, b.y0),
        Point::new(b.x1, b.y1),
        Point::new(b.x0, b.y1),
    ];
    let plane: Vec<Point> = local
        .iter()
        .map(|p| map_homography(to_plane, t * *p))
        .collect();
    if plane.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
        return None;
    }
    // The box's sides on the plane: each side's mean, so a wall whose
    // lines lean against the page's keeps the drawn orientation.
    let (bl, br, tr, tl) = (plane[0], plane[1], plane[2], plane[3]);
    let r = Rect::new(
        (bl.x + tl.x) / 2.0,
        (bl.y + br.y) / 2.0,
        (br.x + tr.x) / 2.0,
        (tl.y + tr.y) / 2.0,
    );
    // Turned over: the box lies past the vanishing line of this plane.
    if r.x1 - r.x0 < 1e-9 || r.y1 - r.y0 < 1e-9 {
        return None;
    }
    plane_rect_corners(r, t, to_page)
}

/// The perspective corners (local coordinates, for page transform `t`)
/// of the rectangle `r` of a plane with map `to_page`; a rectangle
/// reaching past the plane's vanishing line is pulled towards the grid
/// until it fits.
fn plane_rect_corners(mut r: Rect, t: Affine, to_page: &[f64; 8]) -> Option<[Point; 4]> {
    // Every corner must stay on the near side of the vanishing line (the
    // projective map's denominator keeps its sign); a box drawn past it is
    // pulled towards the grid until it fits.
    let den = |p: Point| to_page[6] * p.x + to_page[7] * p.y + 1.0;
    let sign = den(Point::new(0.5, 0.5)).signum();
    let fits = |r: &Rect| {
        [
            Point::new(r.x0, r.y0),
            Point::new(r.x1, r.y0),
            Point::new(r.x1, r.y1),
            Point::new(r.x0, r.y1),
        ]
        .iter()
        .all(|p| den(*p) * sign > 0.05)
    };
    let mut tries = 0;
    while !fits(&r) {
        tries += 1;
        if tries > 40 {
            return None;
        }
        let k = 0.85;
        r = Rect::new(
            0.5 + (r.x0 - 0.5) * k,
            0.5 + (r.y0 - 0.5) * k,
            0.5 + (r.x1 - 0.5) * k,
            0.5 + (r.y1 - 0.5) * k,
        );
    }
    let det = t.determinant();
    if !det.is_finite() || det.abs() < 1e-12 {
        return None;
    }
    let inv = t.inverse();
    let snapped = [
        Point::new(r.x0, r.y0),
        Point::new(r.x1, r.y0),
        Point::new(r.x1, r.y1),
        Point::new(r.x0, r.y1),
    ];
    let mut out = [Point::ZERO; 4];
    for (o, p) in out.iter_mut().zip(snapped.iter()) {
        *o = inv * map_homography(to_page, *p);
    }
    Some(out)
}

/// The corners (local, for page transform `t`, already moved by `total`)
/// of an object whose perspective `corners` lay it on the plane of
/// `(to_page, to_plane)` before the move, slid along the plane so that
/// its middle follows the pointer and its size on the plane stays; none
/// when it was not on that plane.
fn slide_on_plane(
    corners: &[Point; 4],
    t: Affine,
    total: tracedraw_core::geometry::Vec2,
    to_page: &[f64; 8],
    to_plane: &[f64; 8],
) -> Option<[Point; 4]> {
    let before: Vec<Point> = corners
        .iter()
        .map(|c| map_homography(to_plane, t * *c - total))
        .collect();
    if before.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
        return None;
    }
    let r = Rect::new(before[0].x, before[0].y, before[2].x, before[2].y);
    let size = r.width().abs().max(r.height().abs()).max(1e-9);
    let tol = size * 1e-4;
    let on = (before[0].y - before[1].y).abs() < tol
        && (before[2].y - before[3].y).abs() < tol
        && (before[0].x - before[3].x).abs() < tol
        && (before[1].x - before[2].x).abs() < tol
        && r.width() > 1e-9
        && r.height() > 1e-9;
    if !on {
        return None;
    }
    let mid = map_homography(to_page, r.center()) + total;
    let to = map_homography(to_plane, mid);
    if !to.x.is_finite() || !to.y.is_finite() {
        return None;
    }
    let moved = r + (to - r.center());
    plane_rect_corners(moved, t, to_page)
}

/// The corners (local, for page transform `t`) that lay an object with
/// local box `b`, let go on a plane, where its middle is, as large there
/// as it was on the page: its width and height on the page divided by
/// how long the plane's unit sides are on the page at that spot.
fn dropped_corners(
    b: Rect,
    t: Affine,
    to_page: &[f64; 8],
    to_plane: &[f64; 8],
) -> Option<[Point; 4]> {
    let mid = t * b.center();
    let u = map_homography(to_plane, mid);
    if !u.x.is_finite() || !u.y.is_finite() {
        return None;
    }
    let e = 1e-3;
    let at = map_homography(to_page, u);
    let along_x = (map_homography(to_page, Point::new(u.x + e, u.y)) - at).hypot() / e;
    let along_y = (map_homography(to_page, Point::new(u.x, u.y + e)) - at).hypot() / e;
    if !(along_x > 1e-9 && along_y > 1e-9 && along_x.is_finite() && along_y.is_finite()) {
        return None;
    }
    let [a, bb, c, d, _, _] = t.as_coeffs();
    let w = b.width() * a.hypot(bb) / along_x;
    let h = b.height() * c.hypot(d) / along_y;
    if !(w > 1e-9 && h > 1e-9) {
        return None;
    }
    let r = Rect::new(u.x - w / 2.0, u.y - h / 2.0, u.x + w / 2.0, u.y + h / 2.0);
    plane_rect_corners(r, t, to_page)
}

/// Tools that draw new objects (the ones laid onto the grid).
fn draws(tool: Tool) -> bool {
    !matches!(
        tool,
        Tool::Pick
            | Tool::FreeformPick
            | Tool::FreeTransform
            | Tool::Shape
            | Tool::Zoom
            | Tool::Pan
            | Tool::Crop
            | Tool::Knife
            | Tool::Eraser
            | Tool::SegmentDelete
            | Tool::ColorEyedropper
            | Tool::AttributesEyedropper
            | Tool::InteractiveFill
            | Tool::MeshFill
            | Tool::Transparency
            | Tool::OutlinePen
            | Tool::OutlineColor
    )
}

impl App {
    /// The objects on the current page, at the top level.
    fn page_object_ids(&self) -> HashSet<ShapeId> {
        self.doc()
            .page(self.page)
            .map(|p| {
                p.layers
                    .iter()
                    .flat_map(|l| l.shapes.iter().map(|s| s.id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// View > Draw in Perspective on or off.
    pub fn toggle_perspective_drawing(&mut self) {
        if self.perspective.is_some() {
            self.perspective = None;
            return;
        }
        let size = self.page_size();
        let mut s = Session::new(Kind::One, size.width, size.height);
        s.known = self.page_object_ids();
        self.perspective = Some(s);
    }

    /// Change the grid's kind, keeping its plane when the kind has it.
    pub fn set_perspective_kind(&mut self, kind: Kind) {
        let size = self.page_size();
        if let Some(s) = self.perspective.as_mut() {
            let plane = s.plane;
            let known = std::mem::take(&mut s.known);
            *s = Session::new(kind, size.width, size.height);
            s.known = known;
            if kind.planes().contains(&plane) {
                s.plane = plane;
            }
        }
    }

    /// Lay the selected objects onto the active plane, one undo step each.
    pub fn lay_selection_onto_plane(&mut self) -> usize {
        let ids = self.selection.clone();
        ids.into_iter()
            .filter(|id| self.lay_onto_plane_with(*id, false))
            .count()
    }

    /// Lay `id` onto the active plane (one undo step with its drawing).
    pub fn lay_onto_plane(&mut self, id: ShapeId) -> bool {
        self.lay_onto_plane_with(id, true)
    }

    /// Lay `id` onto the active plane; with `amend`, as part of the step
    /// just recorded.
    fn lay_onto_plane_with(&mut self, id: ShapeId, amend: bool) -> bool {
        let Some(session) = self.perspective.as_ref() else {
            return false;
        };
        let Some((to_page, to_plane)) = session.maps(session.plane) else {
            return false;
        };
        let doc = self.doc();
        let Some(shape) = doc.find_shape(id).cloned() else {
            return false;
        };
        let parent = doc.parent_transform(id).unwrap_or(Affine::IDENTITY);
        use tracedraw_core::geometry::Shape as _;
        let b = shape.local_path().bounding_box();
        let Some(corners) = laid_corners(b, parent * shape.transform, &to_page, &to_plane) else {
            return false;
        };
        let mut effects = shape.effects.clone();
        effects.retain(|e| !matches!(e, Effect::Perspective { .. }));
        effects.push(Effect::Perspective { corners });
        let cmd = Command::SetEffects { shape: id, effects };
        if !amend || self.engine.amend(&cmd).is_err() {
            self.run(cmd);
        }
        true
    }

    /// After each frame's input: objects a drawing tool just made go onto
    /// the plane; anything else new (pasted, duplicated) is left as it is.
    pub fn perspective_tick(&mut self) {
        if self.perspective.is_none() || !matches!(self.drag, crate::app::Drag::None) {
            return;
        }
        let now = self.page_object_ids();
        let new: Vec<ShapeId> = match self.perspective.as_ref() {
            Some(s) => now
                .iter()
                .filter(|id| !s.known.contains(id))
                .copied()
                .collect(),
            None => return,
        };
        if new.is_empty() {
            return;
        }
        if draws(self.tool) && self.text_edit.is_none() {
            for id in &new {
                self.lay_onto_plane(*id);
            }
        }
        if let Some(s) = self.perspective.as_mut() {
            s.known.extend(new);
        }
    }

    /// A press on a handle drags it; true when the pointer was used here.
    /// While a 4 Corners plane is being set, each click is a corner.
    pub fn perspective_pointer(&mut self, response: &egui::Response) -> bool {
        let Some(pos) = response.interact_pointer_pos().or(response.hover_pos()) else {
            return false;
        };
        let view = self.view;
        let Some(s) = self.perspective.as_mut() else {
            return false;
        };
        if let Some(placed) = s.placing.as_mut() {
            if response.clicked_by(egui::PointerButton::Primary) {
                placed.push(view.to_page(pos));
                if placed.len() >= 4 {
                    let c = [placed[0], placed[1], placed[2], placed[3]];
                    s.corners = c;
                    s.walls = [None; 4];
                    s.placing = None;
                    s.plane = Plane::Ground;
                }
            }
            return true;
        }
        if response.drag_started_by(egui::PointerButton::Primary) {
            let ctrl = response.ctx.input(|i| i.modifiers.command || i.modifiers.ctrl);
            s.drag = s
                .handles()
                .into_iter()
                .find(|(_, p)| (view.to_screen(*p) - pos).length() <= 8.0)
                .map(|(h, _)| match h {
                    // Ctrl on a side's middle: a plane standing on it,
                    // pulled up from nothing (or the one there).
                    Handle::Side(i) if ctrl => {
                        if s.walls[i].is_none() {
                            s.walls[i] = Some(0.0);
                        }
                        s.plane = Plane::on_side(i);
                        Handle::WallTop(i)
                    }
                    h => h,
                });
            s.last = Some(view.to_page(pos));
        }
        let Some(h) = s.drag else {
            return false;
        };
        if response.dragged_by(egui::PointerButton::Primary) {
            s.drag_to(h, view.to_page(pos));
        }
        if response.drag_stopped() {
            s.drag = None;
            s.last = None;
            // A standing plane let go with no height is taken away.
            for i in 0..4 {
                if s.walls[i].is_some_and(|h| h.abs() < 0.5) {
                    s.walls[i] = None;
                    if s.plane == Plane::on_side(i) {
                        s.plane = Plane::Ground;
                    }
                }
            }
        }
        true
    }

    /// After a move with the Pick tool while drawing in perspective: an
    /// object already on the active plane slides along it, keeping its
    /// size on the plane; any other object let go with its middle inside
    /// the plane's grid goes onto the plane. Both join the move's undo
    /// step, and the objects stay what they were (a text stays text).
    pub fn perspective_after_move(&mut self, total: tracedraw_core::geometry::Vec2) {
        if !matches!(self.tool, Tool::Pick | Tool::FreeformPick) || total.hypot() < 1e-9 {
            return;
        }
        let Some(session) = self.perspective.as_ref() else {
            return;
        };
        if session.placing.is_some() {
            return;
        }
        let Some((to_page, to_plane)) = session.maps(session.plane) else {
            return;
        };
        let ids = self.selection.clone();
        for id in ids {
            let doc = self.doc();
            let Some(shape) = doc.find_shape(id).cloned() else {
                continue;
            };
            let t = doc.parent_transform(id).unwrap_or(Affine::IDENTITY) * shape.transform;
            let on = shape.effects.iter().find_map(|e| match e {
                Effect::Perspective { corners } => Some(*corners),
                _ => None,
            });
            if let Some(slid) = on.and_then(|c| slide_on_plane(&c, t, total, &to_page, &to_plane)) {
                let mut effects = shape.effects.clone();
                for e in effects.iter_mut() {
                    if let Effect::Perspective { corners } = e {
                        *corners = slid;
                    }
                }
                let cmd = Command::SetEffects { shape: id, effects };
                if self.engine.amend(&cmd).is_err() {
                    self.run(cmd);
                }
                continue;
            }
            use tracedraw_core::geometry::Shape as _;
            let b = shape.local_path().bounding_box();
            let mid = map_homography(&to_plane, t * b.center());
            let inside = (-1e-6..=1.0 + 1e-6).contains(&mid.x)
                && (-1e-6..=1.0 + 1e-6).contains(&mid.y);
            if !inside {
                continue;
            }
            let Some(corners) = dropped_corners(b, t, &to_page, &to_plane) else {
                continue;
            };
            let mut effects = shape.effects.clone();
            effects.retain(|e| !matches!(e, Effect::Perspective { .. }));
            effects.push(Effect::Perspective { corners });
            let cmd = Command::SetEffects { shape: id, effects };
            if self.engine.amend(&cmd).is_err() {
                self.run(cmd);
            }
        }
    }
}

/// The grid: the active plane's lines strong, the others faint; the
/// horizon, the vanishing points and the corner handles.
pub fn draw_grid(app: &App, painter: &Painter, rect: egui::Rect) {
    let Some(s) = app.perspective.as_ref() else {
        return;
    };
    let view = &app.view;
    let blue = Color32::from_rgb(0x1F, 0x8F, 0xE0);
    if let Some(placed) = s.placing.as_ref() {
        // Setting a 4 Corners plane: the corners clicked so far.
        let pts: Vec<Pos2> = placed.iter().map(|p| view.to_screen(*p)).collect();
        for w in pts.windows(2) {
            painter.line_segment([w[0], w[1]], EStroke::new(1.5, blue));
        }
        for p in &pts {
            painter.circle_filled(*p, 4.0, Color32::WHITE);
            painter.circle_stroke(*p, 4.0, EStroke::new(1.5, blue));
        }
        return;
    }
    // Horizon.
    let horizon = if s.kind == Kind::Corners {
        let [a, b] = s.side_vanishing();
        point_of(a).zip(point_of(b))
    } else {
        let hy = s.vanish[0].y;
        Some((Point::new(0.0, hy), Point::new(1.0, hy)))
    };
    if let Some((a, b)) = horizon {
        let (pa, pb) = (view.to_screen(a), view.to_screen(b));
        let d = pb - pa;
        if d.length() > 1e-3 {
            let far = (rect.width() + rect.height()) * 4.0 / d.length();
            painter.line_segment(
                [pa - d * far, pa + d * far],
                EStroke::new(1.0, blue.gamma_multiply(0.7)),
            );
        }
    }
    if s.grid {
        for plane in s.planes() {
            let Some((to_page, _)) = s.maps(plane) else {
                continue;
            };
            let active = plane == s.plane;
            let stroke = EStroke::new(
                if active { 1.0 } else { 0.7 },
                blue.gamma_multiply(if active { 0.75 } else { 0.25 }),
            );
            let n = 8;
            for i in 0..=n {
                let t = i as f64 / n as f64;
                for (a, b) in [
                    (Point::new(t, 0.0), Point::new(t, 1.0)),
                    (Point::new(0.0, t), Point::new(1.0, t)),
                ] {
                    let pa = view.to_screen(map_homography(&to_page, a));
                    let pb = view.to_screen(map_homography(&to_page, b));
                    painter.line_segment([pa, pb], stroke);
                }
            }
        }
    }
    for (h, p) in s.handles() {
        let at = view.to_screen(p);
        match h {
            Handle::Side(_) | Handle::WallTop(_) => {
                painter.circle_filled(at, 4.0, Color32::WHITE);
                painter.circle_stroke(at, 4.0, EStroke::new(1.5, blue));
            }
            Handle::Vanish(_) => {
                painter.circle_filled(at, 7.0, Color32::WHITE);
                painter.circle_stroke(at, 7.0, EStroke::new(2.0, blue));
                painter.circle_filled(at, 2.5, blue);
            }
            _ => {
                let r = egui::Rect::from_center_size(at, egui::vec2(9.0, 9.0));
                painter.rect_filled(r, 0.0, Color32::WHITE);
                painter.rect_stroke(r, 0.0, EStroke::new(1.5, blue), egui::StrokeKind::Middle);
            }
        }
    }
}

/// The bar at the top of the drawing window while drawing in
/// perspective: kind, plane, grid, Finish.
pub fn show_bar(app: &mut App, ctx: &egui::Context) {
    if app.perspective.is_none() {
        return;
    }
    use crate::i18n::tr;
    let at = app.canvas_rect.min + egui::vec2(14.0, 14.0);
    egui::Area::new(egui::Id::new("perspective_bar"))
        .fixed_pos(at)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.ibutton("perspective.finish").clicked() {
                        app.perspective = None;
                        return;
                    }
                    ui.separator();
                    let Some(kind) = app.perspective.as_ref().map(|s| s.kind) else {
                        return;
                    };
                    for k in Kind::ALL {
                        let mut label = ui.selectable_label(kind == k, tr(k.key()));
                        if k == Kind::Corners {
                            label = label.rich_tip(tr("perspective.four_corners_tip"));
                        }
                        if label.clicked() {
                            if kind != k {
                                app.set_perspective_kind(k);
                            } else if k == Kind::Corners {
                                // Again: set the corners anew.
                                if let Some(s) = app.perspective.as_mut() {
                                    s.placing = Some(Vec::new());
                                }
                            }
                        }
                    }
                    ui.separator();
                    if let Some(s) = app.perspective.as_mut() {
                        if s.placing.is_some() {
                            ui.label(tr("perspective.click_corners"));
                            return;
                        }
                        for p in s.planes() {
                            if ui.selectable_label(s.plane == p, tr(p.key())).clicked() {
                                s.plane = p;
                            }
                        }
                        ui.separator();
                        ui.checkbox(&mut s.grid, tr("perspective.grid"));
                        if s.kind != Kind::Corners {
                            ui.separator();
                            ui.label(tr("perspective.depth"));
                            let mut d = (s.depth * 100.0).round();
                            if ui
                                .add(
                                    crate::ui::field::NumField::new(&mut d)
                                        .range(5.0..=98.0)
                                        .suffix(" %")
                                        .width(50.0),
                                )
                                .changed()
                            {
                                s.depth = d / 100.0;
                            }
                        }
                    }
                    let some = !app.selection.is_empty();
                    if ui
                        .add_enabled(some, crate::ui::btn_icons::Ib::new("perspective.put_on_plane"))
                        .rich_tip(tr("perspective.put_on_plane_tip"))
                        .clicked()
                    {
                        app.lay_selection_onto_plane();
                    }
                });
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_plane_maps_the_unit_square_onto_its_quad_and_back() {
        for kind in Kind::ALL {
            let mut s = Session::new(kind, 210.0, 297.0);
            // 4 Corners: a plane standing on every side.
            s.walls = [Some(20.0), Some(-15.0), Some(10.0), Some(25.0)];
            for plane in &s.planes() {
                let q = s.quad(*plane).expect("a quad");
                let (to_page, to_plane) = s.maps(*plane).expect("maps");
                let unit = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
                for ((u, w), corner) in unit.iter().zip(q.iter()) {
                    let p = map_homography(&to_page, Point::new(*u, *w));
                    assert!((p - *corner).hypot() < 1e-6, "{kind:?} {plane:?}");
                    let back = map_homography(&to_plane, p);
                    assert!((back - Point::new(*u, *w)).hypot() < 1e-6);
                }
            }
        }
    }

    #[test]
    fn grid_lines_meet_at_the_vanishing_points() {
        // One-point ground: the depth lines head for the vanishing point.
        let s = Session::new(Kind::One, 200.0, 200.0);
        let q = s.quad(Plane::Ground).expect("a quad");
        let meet = intersect(q[0], q[3], q[1], q[2]).expect("they meet");
        assert!((meet - s.vanish[0]).hypot() < 1e-6, "{meet:?}");
        // Two-point ground: each pair of sides meets at its own point.
        let s = Session::new(Kind::Two, 200.0, 200.0);
        let q = s.quad(Plane::Ground).expect("a quad");
        let a = intersect(q[0], q[3], q[1], q[2]).expect("meet");
        let b = intersect(q[0], q[1], q[3], q[2]).expect("meet");
        assert!((a - s.vanish[0]).hypot() < 1e-6 && (b - s.vanish[1]).hypot() < 1e-6);
        // Three-point walls: the vertical edges lean to the third point.
        let s = Session::new(Kind::Three, 200.0, 200.0);
        let q = s.quad(Plane::Left).expect("a quad");
        let v = intersect(q[0], q[3], q[1], q[2]).expect("meet");
        assert!((v - s.vanish[2]).hypot() < 1e-3, "{v:?}");
    }

    #[test]
    fn boxes_drawn_across_a_wall_keep_their_corners_apart_and_their_side_up() {
        let s = Session::new(Kind::One, 297.0, 210.0);
        for plane in Kind::One.planes() {
            let (tp, tl) = s.maps(*plane).expect("maps");
            // Partly off the wall, reaching towards the vanishing point.
            let b = Rect::new(101.0, 55.7, 129.0, 83.6);
            let laid = laid_corners(b, Affine::IDENTITY, &tp, &tl);
            // Past the right wall's and the ceiling's vanishing lines (the
            // box is left of the far end of the right wall, below the
            // horizon): left flat.
            if matches!(plane, Plane::Right | Plane::Top) {
                assert!(laid.is_none(), "{laid:?}");
                continue;
            }
            let c = laid.expect("laid");
            for i in 0..4 {
                for j in i + 1..4 {
                    assert!((c[i] - c[j]).hypot() > 0.5, "{plane:?} {c:?}");
                }
            }
            // The quad is not turned over: same winding as the drawn box.
            let area: f64 = (0..4)
                .map(|i| {
                    let (a, b) = (c[i], c[(i + 1) % 4]);
                    a.x * b.y - b.x * a.y
                })
                .sum();
            assert!(area > 0.0, "{plane:?} {c:?}");
        }
    }

    #[test]
    fn a_rectangle_drawn_on_the_grid_lands_on_the_plane_in_one_undo_step() {
        let mut app = App::headless();
        app.toggle_perspective_drawing();
        app.tool = Tool::Rectangle;
        let layer = app.active_layer().expect("a layer");
        let id = app.engine.new_shape_id();
        let shape = tracedraw_core::Shape::new(
            id,
            tracedraw_core::ShapeKind::Rect {
                rect: Rect::new(60.0, 30.0, 120.0, 60.0),
                radius: 0.0,
                corners: None,
            },
        );
        app.run(Command::AddShape { layer, shape });
        app.perspective_tick();
        let s = app.doc().find_shape(id).cloned().expect("the rectangle");
        assert!(s
            .effects
            .iter()
            .any(|e| matches!(e, Effect::Perspective { .. })));
        // Its corners lie on the ground plane: their plane coordinates form
        // a rectangle (two by two the same u and w).
        let session = app.perspective.as_ref().expect("a session");
        let (_, to_plane) = session.maps(Plane::Ground).expect("maps");
        let corners = match s.effects.iter().find_map(|e| match e {
            Effect::Perspective { corners } => Some(*corners),
            _ => None,
        }) {
            Some(c) => c,
            None => unreachable!(),
        };
        let p: Vec<Point> = corners
            .iter()
            .map(|c| map_homography(&to_plane, s.transform * *c))
            .collect();
        assert!((p[0].y - p[1].y).abs() < 1e-6 && (p[2].y - p[3].y).abs() < 1e-6);
        assert!((p[0].x - p[3].x).abs() < 1e-6 && (p[1].x - p[2].x).abs() < 1e-6);
        // Foreshortened: the far (upper) edge is shorter than the near one.
        let page: Vec<Point> = corners.iter().map(|c| s.transform * *c).collect();
        assert!((page[2] - page[3]).hypot() < (page[1] - page[0]).hypot());
        // Drawing and laying it down undo together.
        app.undo();
        assert!(app.doc().find_shape(id).is_none());
        // With the Pick tool, new objects (pasted, duplicated) stay flat.
        app.tool = Tool::Pick;
        let id2 = app.engine.new_shape_id();
        let shape = tracedraw_core::Shape::new(
            id2,
            tracedraw_core::ShapeKind::Rect {
                rect: Rect::new(10.0, 10.0, 20.0, 20.0),
                radius: 0.0,
                corners: None,
            },
        );
        app.run(Command::AddShape { layer, shape });
        app.perspective_tick();
        assert!(app
            .doc()
            .find_shape(id2)
            .is_some_and(|s| s.effects.is_empty()));
        // Put on Plane lays an existing object down as a step of its own.
        app.select(vec![id2]);
        assert_eq!(app.lay_selection_onto_plane(), 1);
        assert!(app
            .doc()
            .find_shape(id2)
            .is_some_and(|s| !s.effects.is_empty()));
        app.undo();
        assert!(app
            .doc()
            .find_shape(id2)
            .is_some_and(|s| s.effects.is_empty()));
        app.toggle_perspective_drawing();
        assert!(app.perspective.is_none());
    }

    /// A 4 Corners plane over a photo taken by a camera turned and tilted:
    /// the vertical vanishing point found from the plane's corners is the
    /// camera's own, a plane pulled up on a side has its upright sides
    /// heading there and its top running to the side's vanishing point.
    #[test]
    fn four_corners_find_the_upright_direction_of_the_photo() {
        let (f, centre) = (120.0, Point::new(105.0, 148.5));
        let (yaw, pitch) = (30f64.to_radians(), (-20f64).to_radians());
        let rot = |v: [f64; 3]| -> [f64; 3] {
            // Turn about the upright axis, then tilt.
            let (x, y, z) = (
                v[0] * yaw.cos() - v[2] * yaw.sin(),
                v[1],
                v[0] * yaw.sin() + v[2] * yaw.cos(),
            );
            [x, y * pitch.cos() - z * pitch.sin(), y * pitch.sin() + z * pitch.cos()]
        };
        let project = |w: [f64; 3]| {
            let c = rot([w[0], w[1] - 2.0, w[2]]);
            Point::new(centre.x + f * c[0] / c[2], centre.y + f * c[1] / c[2])
        };
        let mut s = Session::new(Kind::Corners, 210.0, 297.0);
        assert!(s.placing.is_some());
        s.placing = None;
        s.corners = [
            project([-1.0, 0.0, 4.0]),
            project([1.0, 0.0, 4.0]),
            project([1.0, 0.0, 6.0]),
            project([-1.0, 0.0, 6.0]),
        ];
        let up = rot([0.0, 1.0, 0.0]);
        let want = Point::new(centre.x + f * up[0] / up[2], centre.y + f * up[1] / up[2]);
        let got = point_of(s.vertical_vanishing()).expect("a point");
        assert!((got - want).hypot() < 1e-6 * want.to_vec2().hypot().max(1.0), "{got:?} {want:?}");
        // A plane pulled up on the right side, and laid on.
        s.walls[1] = Some(-30.0);
        assert_eq!(s.planes(), vec![Plane::Ground, Plane::Right]);
        let q = s.quad(Plane::Right).expect("a wall");
        let v3 = intersect(q[0], q[3], q[1], q[2]).expect("they meet");
        assert!((v3 - want).hypot() < 1e-3 * want.to_vec2().hypot().max(1.0), "{v3:?}");
        // The real wall's corners: the top is the same height all along.
        let top_a = project([1.0, 1.0, 4.0]);
        let top_b = project([1.0, 1.0, 6.0]);
        let along = intersect(top_a, top_b, q[0], q[1]).expect("meet");
        let top_along = intersect(q[3], q[2], q[0], q[1]).expect("meet");
        assert!((along - top_along).hypot() < 1e-3 * along.to_vec2().hypot().max(1.0));
        assert!(signed_area(&q) > 0.0 && signed_area(&s.quad(Plane::Ground).expect("q")) > 0.0);
        // Dragging the top handle sets the height; let go at nothing, the
        // plane goes.
        let top = s.wall_top(1).expect("top");
        s.drag_to(Handle::WallTop(1), top);
        assert!((s.walls[1].expect("h") + 30.0).abs() < 1e-6);
        // A side's middle drags both its corners.
        let c0 = s.corners;
        s.last = Some(Point::new(0.0, 0.0));
        s.drag_to(Handle::Side(0), Point::new(3.0, 4.0));
        assert!((s.corners[0] - c0[0] - tracedraw_core::geometry::Vec2::new(3.0, 4.0)).hypot() < 1e-9);
        assert!((s.corners[1] - c0[1] - tracedraw_core::geometry::Vec2::new(3.0, 4.0)).hypot() < 1e-9);
        assert_eq!(s.corners[2], c0[2]);
        // A square-on road (sides parallel across): upright lines stay
        // upright.
        let mut road = Session::new(Kind::Corners, 200.0, 200.0);
        road.corners = [
            Point::new(20.0, 10.0),
            Point::new(180.0, 10.0),
            Point::new(120.0, 90.0),
            Point::new(80.0, 90.0),
        ];
        road.walls[3] = Some(40.0);
        let q = road.quad(Plane::Left).expect("a wall");
        assert!((q[0].x - q[3].x).abs() < 1e-9 && (q[1].x - q[2].x).abs() < 1e-9, "{q:?}");
    }

    /// With the Pick tool on a 4 Corners plane: an object let go inside
    /// the plane goes onto it, one already on it slides along it keeping
    /// its size on the plane, each in the move's undo step.
    #[test]
    fn objects_moved_onto_the_plane_lie_on_it_and_slide_along_it() {
        let mut app = App::headless();
        app.toggle_perspective_drawing();
        app.set_perspective_kind(Kind::Corners);
        if let Some(s) = app.perspective.as_mut() {
            s.placing = None;
        }
        app.tool = Tool::Pick;
        let layer = app.active_layer().expect("a layer");
        let id = app.engine.new_shape_id();
        let shape = tracedraw_core::Shape::new(
            id,
            tracedraw_core::ShapeKind::Rect {
                rect: Rect::new(10.0, 250.0, 40.0, 270.0),
                radius: 0.0,
                corners: None,
            },
        );
        app.run(Command::AddShape { layer, shape });
        app.select(vec![id]);
        let corners = |app: &App| {
            app.doc().find_shape(id).and_then(|s| {
                s.effects.iter().find_map(|e| match e {
                    Effect::Perspective { corners } => Some(corners.map(|c| s.transform * c)),
                    _ => None,
                })
            })
        };
        let session = app.perspective.clone().expect("a session");
        let (_, to_plane) = session.maps(Plane::Ground).expect("maps");
        let on_plane = |c: [Point; 4]| -> Rect {
            let p = c.map(|c| map_homography(&to_plane, c));
            assert!((p[0].y - p[1].y).abs() < 1e-6 && (p[0].x - p[3].x).abs() < 1e-6, "{p:?}");
            Rect::new(p[0].x, p[0].y, p[2].x, p[2].y)
        };
        let mv = |app: &mut App, d: tracedraw_core::geometry::Vec2| {
            app.run(Command::TransformShapes {
                shapes: vec![id],
                transform: Affine::translate(d),
            });
            app.perspective_after_move(d);
        };
        // Let go above the plane: it stays flat.
        mv(&mut app, tracedraw_core::geometry::Vec2::new(5.0, 0.0));
        assert!(corners(&app).is_none());
        // Into the plane's middle: onto it.
        let mid = map_homography(&session.maps(Plane::Ground).expect("m").0, Point::new(0.5, 0.5));
        let d = mid - Point::new(30.0, 260.0);
        mv(&mut app, d);
        let r = on_plane(corners(&app).expect("on the plane"));
        // Undo takes the move and the laying back together.
        app.undo();
        assert!(corners(&app).is_none());
        app.redo();
        // Slid along the plane: the same size on it, its middle where the
        // pointer took it.
        mv(&mut app, tracedraw_core::geometry::Vec2::new(10.0, 15.0));
        let c = corners(&app).expect("still on it");
        let r2 = on_plane(c);
        assert!((r2.width() - r.width()).abs() < 1e-6 && (r2.height() - r.height()).abs() < 1e-6);
        assert!((r2.center() - r.center()).hypot() > 0.01);
        let at = map_homography(&session.maps(Plane::Ground).expect("m").0, r.center())
            + tracedraw_core::geometry::Vec2::new(10.0, 15.0);
        let got = map_homography(&session.maps(Plane::Ground).expect("m").0, r2.center());
        assert!((got - at).hypot() < 1e-6, "{got:?} {at:?}");
        // The object is still a rectangle.
        assert!(matches!(
            app.doc().find_shape(id).map(|s| &s.kind),
            Some(tracedraw_core::ShapeKind::Rect { .. })
        ));
    }

}
