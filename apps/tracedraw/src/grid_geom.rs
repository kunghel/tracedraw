//! The document grid's geometry (#4395): the two axes the grid runs
//! along (their directions, spacing, subdivisions and gutters) worked out
//! from the document's grid settings, the zoom and the units, and where
//! its lines and crossings lie for drawing and snapping.
//!
//! A point of the page is written in the grid's own coordinates `(u, v)`:
//! `p = origin + u * a + v * b`, `a` and `b` the unit directions of the
//! first and second axis. Lines of the first family are `u` = constant
//! (they run along `b`), lines of the second `v` = constant (along `a`).
//! On a standard grid `a` is the x axis and `b` the y axis, so `u` and `v`
//! are plain distances from the origin.

use tracedraw_core::document::{GridMode, GridSettings};
use tracedraw_core::geometry::{Point, Vec2};

/// One axis of the grid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Axis {
    /// Unit direction of the axis on the page.
    pub dir: Vec2,
    /// A cell's side along the axis, in mm.
    pub spacing: f64,
    /// Subdivisions of a cell (1: none).
    pub divisions: u32,
    /// Empty room between two cells, in mm.
    pub gutter: f64,
}

impl Axis {
    /// The distance from one cell's start to the next one's.
    pub fn pitch(&self) -> f64 {
        self.spacing + self.gutter
    }

    /// The line coordinates near `v`, the cells one to each side
    /// included: each cell's edges, and its subdivisions with `minor`.
    pub fn lines_near(&self, v: f64, minor: bool) -> Vec<f64> {
        let p = self.pitch();
        if !(p.is_finite() && p > 0.0 && v.is_finite()) {
            return Vec::new();
        }
        let i0 = (v / p).floor() as i64;
        let mut out = Vec::new();
        for i in (i0 - 1)..=(i0 + 1) {
            let start = i as f64 * p;
            out.push(start);
            if self.gutter > 0.0 {
                out.push(start + self.spacing);
            }
            if minor && self.divisions > 1 {
                let d = self.spacing / f64::from(self.divisions);
                for j in 1..self.divisions {
                    out.push(start + d * f64::from(j));
                }
            }
        }
        out
    }

    /// The line coordinate nearest `v` (a cell edge, or a subdivision with
    /// `minor`).
    pub fn nearest(&self, v: f64, minor: bool) -> Option<f64> {
        self.lines_near(v, minor)
            .into_iter()
            .min_by(|a, b| (a - v).abs().total_cmp(&(b - v).abs()))
    }

    /// The coordinates between `a` and `b` of the lines drawn: each
    /// `every`-th cell's edges (its far edge too when cells have a
    /// gutter and `every` is 1), and with `minor` each cell's
    /// subdivisions; as (coordinate, is a subdivision). Empty when there
    /// would be more than `limit`.
    pub fn lines_between(
        &self,
        a: f64,
        b: f64,
        every: f64,
        minor: bool,
        limit: usize,
    ) -> Vec<(f64, bool)> {
        let (a, b) = (a.min(b), a.max(b));
        let p = self.pitch();
        if !(p.is_finite() && p > 0.0 && every.is_finite() && every >= 1.0) {
            return Vec::new();
        }
        let step = (every.round() as i64).max(1);
        let first = (a / p).floor() as i64 - 1;
        let last = (b / p).ceil() as i64 + 1;
        if (last - first) / step > limit as i64 {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut i = first.div_euclid(step) * step;
        while i <= last {
            let start = i as f64 * p;
            out.push((start, false));
            if step == 1 && self.gutter > 0.0 {
                out.push((start + self.spacing, false));
            }
            if minor && step == 1 && self.divisions > 1 {
                let d = self.spacing / f64::from(self.divisions);
                for j in 1..self.divisions {
                    out.push((start + d * f64::from(j), true));
                }
            }
            if out.len() > limit {
                return Vec::new();
            }
            i += step;
        }
        out.retain(|(v, _)| *v >= a - 1e-9 && *v <= b + 1e-9);
        out
    }
}

/// The grid laid out on the page.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridGeom {
    /// Where the first lines of both families cross.
    pub origin: Point,
    pub axes: [Axis; 2],
    /// Lines along the third direction (`a` + `b` scaled by the
    /// spacings) through the crossings: angled grids with the option on.
    pub third: bool,
}

/// Round spacing for Automatic mode at `zoom` (screen points per mm) in
/// a unit of `unit_mm` millimetres: 1, 2 or 5 times a power of ten units,
/// the smallest that leaves cells at least [`AUTO_MIN_PX`] apart, and the
/// subdivisions that make whole fifths, quarters or fifths of it.
pub fn auto_spacing(zoom: f64, unit_mm: f64) -> (f64, u32) {
    let unit_mm = if unit_mm.is_finite() && unit_mm > 0.0 {
        unit_mm
    } else {
        1.0
    };
    if !(zoom.is_finite() && zoom > 0.0) {
        return (10.0, 1);
    }
    for k in -6..12 {
        for (m, d) in [(1.0, 5), (2.0, 4), (5.0, 5)] {
            let step = m * 10f64.powi(k) * unit_mm;
            if step * zoom >= AUTO_MIN_PX {
                return (step, d);
            }
        }
    }
    (10.0, 1)
}

/// Fewest screen points between two cell edges in Automatic mode.
pub const AUTO_MIN_PX: f64 = 64.0;

fn dir_of(deg: f64) -> Vec2 {
    let r = deg.to_radians();
    Vec2::new(r.cos(), r.sin())
}

impl GridGeom {
    /// The grid of `g` with its origin at `origin` (the ruler origin; the
    /// grid's offset is added here), at `zoom` screen points per mm, in
    /// units of `unit_mm` mm (Automatic mode).
    pub fn of(g: &GridSettings, origin: Point, zoom: f64, unit_mm: f64) -> GridGeom {
        let origin = Point::new(origin.x + g.offset_x, origin.y + g.offset_y);
        let pos = |v: f64, fallback: f64| {
            if v.is_finite() && v > 1e-6 {
                v
            } else {
                fallback
            }
        };
        let gut = |v: f64| if v.is_finite() && v > 0.0 { v } else { 0.0 };
        let (sx, sy) = (pos(g.spacing_x, 10.0), pos(g.spacing_y, 10.0));
        let (a0, a1) = g.axis_angles();
        let (axes, third) = match g.mode {
            GridMode::Automatic => {
                let (s, d) = auto_spacing(zoom, unit_mm);
                let ax = |dir| Axis {
                    dir,
                    spacing: s,
                    divisions: d,
                    gutter: 0.0,
                };
                ([ax(dir_of(0.0)), ax(dir_of(90.0))], false)
            }
            GridMode::Basic => {
                let d = g.divisions_x.clamp(1, 100);
                (
                    [
                        Axis {
                            dir: dir_of(0.0),
                            spacing: sx,
                            divisions: d,
                            gutter: 0.0,
                        },
                        Axis {
                            dir: dir_of(90.0),
                            spacing: sy,
                            divisions: d,
                            gutter: 0.0,
                        },
                    ],
                    false,
                )
            }
            GridMode::Advanced => {
                let first = Axis {
                    dir: dir_of(a0),
                    spacing: sx,
                    divisions: g.divisions_x.clamp(1, 100),
                    gutter: gut(g.gutter_x),
                };
                let second = if g.uniform {
                    Axis {
                        dir: dir_of(a1),
                        ..first
                    }
                } else {
                    Axis {
                        dir: dir_of(a1),
                        spacing: sy,
                        divisions: g.divisions_y.clamp(1, 100),
                        gutter: gut(g.gutter_y),
                    }
                };
                // Two axes nearly along one line make no grid: the second
                // stands square to the first.
                let second = if first.dir.cross(second.dir).abs() < 0.05 {
                    Axis {
                        dir: Vec2::new(-first.dir.y, first.dir.x),
                        ..second
                    }
                } else {
                    second
                };
                let angled = !is_rectilinear(first.dir, second.dir);
                (
                    [first, second],
                    angled && g.third_axis && first.gutter == 0.0 && second.gutter == 0.0,
                )
            }
        };
        GridGeom {
            origin,
            axes,
            third,
        }
    }

    /// The axes run horizontally and vertically (either way round).
    pub fn rectilinear(&self) -> bool {
        is_rectilinear(self.axes[0].dir, self.axes[1].dir)
    }

    /// A grid drawn and snapped as before #4395: horizontal and vertical
    /// lines, no gutters, no subdivisions, no third direction.
    pub fn plain(&self) -> bool {
        self.rectilinear()
            && !self.third
            && self
                .axes
                .iter()
                .all(|a| a.gutter == 0.0 && a.divisions <= 1)
            && self.axes[0].dir.x > 0.5
            && self.axes[1].dir.y > 0.5
    }

    /// The sine of the angle between the axes (1 for a square grid).
    pub fn skew_sine(&self) -> f64 {
        self.axes[0].dir.cross(self.axes[1].dir).abs().max(1e-6)
    }

    /// `p` in the grid's coordinates.
    pub fn to_uv(&self, p: Point) -> (f64, f64) {
        let d = p - self.origin;
        let (a, b) = (self.axes[0].dir, self.axes[1].dir);
        let det = a.cross(b);
        if det.abs() < 1e-12 {
            return (d.x, d.y);
        }
        (d.cross(b) / det, a.cross(d) / det)
    }

    /// The page point at grid coordinates `(u, v)`.
    pub fn from_uv(&self, u: f64, v: f64) -> Point {
        self.origin + self.axes[0].dir * u + self.axes[1].dir * v
    }

    /// The crossing of grid lines (subdivisions included) nearest `p`,
    /// when within `tol` mm of it.
    pub fn snap_node(&self, p: Point, tol: f64) -> Option<Point> {
        let (u, v) = self.to_uv(p);
        let nu = self.axes[0].nearest(u, true)?;
        let nv = self.axes[1].nearest(v, true)?;
        let q = self.from_uv(nu, nv);
        ((q - p).hypot() <= tol).then_some(q)
    }

    /// `p` moved square onto the grid line nearest it (subdivisions and
    /// the third direction included), when that lies within `tol` mm.
    pub fn snap_line(&self, p: Point, tol: f64) -> Option<Point> {
        let (u, v) = self.to_uv(p);
        let sin = self.skew_sine();
        let mut best: Option<(f64, Point)> = None;
        let mut take = |d: f64, q: Point| {
            if d <= tol && best.is_none_or(|b| d < b.0) {
                best = Some((d, q));
            }
        };
        if let Some(nu) = self.axes[0].nearest(u, true) {
            take((u - nu).abs() * sin, self.from_uv(nu, v));
        }
        if let Some(nv) = self.axes[1].nearest(v, true) {
            take((v - nv).abs() * sin, self.from_uv(u, nv));
        }
        if self.third {
            // Lines u/s0 - v/s1 = k through the cell corners.
            let (s0, s1) = (self.axes[0].spacing, self.axes[1].spacing);
            let k = (u / s0 - v / s1).round();
            let o = self.from_uv(k * s0, 0.0);
            let dir = self.axes[0].dir * s0 + self.axes[1].dir * s1;
            let len = dir.hypot();
            if len > 1e-9 {
                let dir = dir * (1.0 / len);
                let t = (p - o).dot(dir);
                let q = o + dir * t;
                take((q - p).hypot(), q);
            }
        }
        best.map(|b| b.1)
    }

    /// Rectilinear grids: the grid line nearest the coordinate `v` (an x
    /// with `horizontal`, else a y), when within `tol`; none on an angled
    /// grid, whose lines are not found one axis at a time.
    pub fn snap_axis_value(&self, v: f64, horizontal: bool, tol: f64) -> Option<f64> {
        let (axis, o) = self.page_axis(horizontal)?;
        let n = axis.nearest(v - o, true)? + o;
        ((n - v).abs() <= tol).then_some(n)
    }

    /// Rectilinear grids: `v` (an x with `horizontal`, else a y) lies on a
    /// grid line.
    pub fn on_line(&self, v: f64, horizontal: bool) -> bool {
        self.snap_axis_value(v, horizontal, 1e-9).is_some()
    }

    /// Rectilinear grids: the grid line nearest `v` along x (`horizontal`)
    /// or y, cell edges only (Align to Grid); `v` itself otherwise.
    pub fn nearest_edge(&self, v: f64, horizontal: bool) -> f64 {
        match self.page_axis(horizontal) {
            Some((axis, o)) => axis.nearest(v - o, false).map_or(v, |n| n + o),
            None => v,
        }
    }

    /// The axis that runs along x (`horizontal`) or y on a rectilinear
    /// grid, its lines' coordinates counted from the origin's x or y: its
    /// direction may point the other way, so it is mirrored to count in
    /// page terms.
    fn page_axis(&self, horizontal: bool) -> Option<(Axis, f64)> {
        if !self.rectilinear() {
            return None;
        }
        let axis = self.axes.iter().copied().find(|a| {
            if horizontal {
                a.dir.x.abs() > 0.5
            } else {
                a.dir.y.abs() > 0.5
            }
        })?;
        let o = if horizontal {
            self.origin.x
        } else {
            self.origin.y
        };
        let flip = if horizontal { axis.dir.x } else { axis.dir.y } < 0.0;
        if flip {
            // Lines at -(i * pitch + offsets): mirrored cells.
            let mirrored = Axis {
                dir: -axis.dir,
                ..axis
            };
            return Some((MirroredAxis(mirrored).as_axis(), o));
        }
        Some((axis, o))
    }
}

/// An axis pointing against the page axis: its lines lie at the negated
/// coordinates. With gutters that is a different set, which `as_axis`
/// keeps by shifting the cells back by the spacing.
struct MirroredAxis(Axis);

impl MirroredAxis {
    fn as_axis(&self) -> Axis {
        // Lines at -(i p) and -(i p + s): the same as i p and i p + g,
        // i.e. cells of the same pitch whose edges are g then s apart.
        // Without a gutter the set is symmetric and nothing changes.
        let a = self.0;
        if a.gutter == 0.0 {
            return a;
        }
        Axis {
            spacing: a.gutter,
            gutter: a.spacing,
            divisions: 1,
            ..a
        }
    }
}

fn is_rectilinear(a: Vec2, b: Vec2) -> bool {
    let axis = |d: Vec2| d.x.abs() < 1e-9 || d.y.abs() < 1e-9;
    axis(a) && axis(b) && a.dot(b).abs() < 1e-9
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::document::GridKind;

    fn geom(g: &GridSettings) -> GridGeom {
        GridGeom::of(g, Point::ZERO, 4.0, 1.0)
    }

    #[test]
    fn the_default_grid_is_plain_and_snaps_as_before() {
        let g = GridSettings::default();
        let geo = geom(&g);
        assert!(geo.plain());
        assert_eq!(geo.snap_axis_value(19.0, true, 1.5), Some(20.0));
        assert_eq!(geo.snap_axis_value(14.0, true, 1.5), None);
        assert_eq!(geo.snap_axis_value(-9.5, false, 1.0), Some(-10.0));
        assert!(geo.on_line(30.0, true) && !geo.on_line(31.0, true));
    }

    #[test]
    fn subdivisions_and_gutters_add_lines() {
        let g = GridSettings {
            mode: GridMode::Advanced,
            spacing_x: 20.0,
            divisions_x: 4,
            gutter_x: 5.0,
            ..Default::default()
        };
        let geo = geom(&g);
        assert!(!geo.plain() && geo.rectilinear());
        // Cells 20 wide, 5 apart: edges at 0, 20, 25, 45; quarters at 5..
        assert_eq!(geo.snap_axis_value(21.0, true, 1.5), Some(20.0));
        assert_eq!(geo.snap_axis_value(26.0, true, 1.5), Some(25.0));
        assert_eq!(geo.snap_axis_value(9.0, true, 1.5), Some(10.0));
        // The gutter holds no subdivision.
        assert_eq!(geo.snap_axis_value(22.5, true, 0.4), None);
        let lines = geo.axes[0].lines_between(0.0, 45.0, 1.0, true, 1000);
        let majors: Vec<f64> = lines.iter().filter(|l| !l.1).map(|l| l.0).collect();
        assert_eq!(majors, vec![0.0, 20.0, 25.0, 45.0]);
        assert_eq!(lines.iter().filter(|l| l.1).count(), 3 + 3);
        // Align to Grid takes cell edges only.
        assert_eq!(geo.nearest_edge(11.0, true), 20.0);
        assert_eq!(geo.nearest_edge(9.0, true), 0.0);
    }

    #[test]
    fn basic_mode_keeps_both_spacings_and_shares_its_subdivisions() {
        let g = GridSettings {
            spacing_x: 10.0,
            spacing_y: 6.0,
            divisions_x: 2,
            divisions_y: 7,
            kind: GridKind::Isometric,
            gutter_x: 3.0,
            ..Default::default()
        };
        let geo = geom(&g);
        assert!(geo.rectilinear());
        assert_eq!(geo.axes[0].divisions, 2);
        assert_eq!(geo.axes[1].divisions, 2);
        assert_eq!(geo.axes[1].spacing, 6.0);
        assert_eq!(geo.axes[0].gutter, 0.0, "gutters are Advanced only");
    }

    #[test]
    fn an_isometric_grid_snaps_to_its_crossings_and_lines() {
        let g = GridSettings {
            mode: GridMode::Advanced,
            kind: GridKind::Isometric,
            spacing_x: 10.0,
            spacing_y: 10.0,
            third_axis: true,
            ..Default::default()
        };
        let geo = geom(&g);
        assert!(!geo.rectilinear() && geo.third);
        // A crossing one cell along each axis lies straight above the
        // origin, 10 mm up (30 and 150 degree axes).
        let up = geo.from_uv(10.0, 10.0);
        assert!((up.x).abs() < 1e-9 && (up.y - 10.0).abs() < 1e-9);
        let near = Point::new(0.3, 9.6);
        let q = geo.snap_node(near, 1.0).expect("a crossing");
        assert!((q - up).hypot() < 1e-9);
        // The vertical third line through the origin takes a point beside
        // it, away from any crossing.
        let p = Point::new(0.2, 4.0);
        let q = geo.snap_line(p, 0.5).expect("a line");
        assert!(q.x.abs() < 1e-9, "{q:?}");
        // No axis by axis snapping on an angled grid.
        assert_eq!(geo.snap_axis_value(0.1, true, 1.0), None);
        let (u, v) = geo.to_uv(up);
        assert!((u - 10.0).abs() < 1e-9 && (v - 10.0).abs() < 1e-9);
    }

    #[test]
    fn automatic_spacing_follows_the_zoom_in_round_units() {
        // 1 screen point per mm: 100 mm cells (at least 64 points).
        assert_eq!(auto_spacing(1.0, 1.0), (100.0, 5));
        // 4 points per mm: 20 mm cells, quarters.
        assert_eq!(auto_spacing(4.0, 1.0), (20.0, 4));
        // Inches at 10 points per mm: half an inch (127 points).
        let (s, _) = auto_spacing(10.0, 25.4);
        assert!((s - 12.7).abs() < 1e-9);
        let g = GridSettings {
            mode: GridMode::Automatic,
            ..Default::default()
        };
        let near = GridGeom::of(&g, Point::ZERO, 40.0, 1.0);
        let far = GridGeom::of(&g, Point::ZERO, 0.5, 1.0);
        assert!(near.axes[0].spacing < far.axes[0].spacing);
    }

    #[test]
    fn the_offset_moves_the_grid() {
        let g = GridSettings {
            offset_x: 3.0,
            offset_y: -2.0,
            ..Default::default()
        };
        let geo = GridGeom::of(&g, Point::new(10.0, 100.0), 4.0, 1.0);
        assert_eq!(geo.origin, Point::new(13.0, 98.0));
        assert_eq!(geo.snap_axis_value(22.0, true, 1.5), Some(23.0));
    }

    #[test]
    fn nearly_parallel_axes_are_set_square() {
        let g = GridSettings {
            mode: GridMode::Advanced,
            kind: GridKind::Custom,
            angle_x: 10.0,
            angle_y: 11.0,
            ..Default::default()
        };
        let geo = geom(&g);
        assert!(geo.skew_sine() > 0.99);
    }
}
