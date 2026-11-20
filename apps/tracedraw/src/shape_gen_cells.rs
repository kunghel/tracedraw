//! Shape Generator cells: Voronoi cells, stippling and vector halftone,
//! the generators built on sites and on a picture's tones.
//!
//! - Voronoi: sites placed at random, stratified, on a hex or a square
//!   lattice (jitter), thinned by a density gradient (linear or radial,
//!   small cells in the middle or at the edge), relaxed (Lloyd) and kept
//!   inside a chosen outline; each cell is the exact half-plane
//!   intersection round its site, optionally inset and rounded for
//!   cutting, clipped by the outline (holes stay empty). Sites and the
//!   Delaunay edges can be drawn too. Sites typed or dragged in the panel
//!   replace the generated ones.
//! - Stippling: dots spread by a picture's darkness with weighted Lloyd
//!   relaxation on a raster of the picture, sized by tone or all alike.
//! - Halftone: dots, squares, diamonds, ellipses or lines on a turned
//!   screen, sized by the picture's darkness; or four screens, one per
//!   ink (C 15, M 75, Y 0, K 45 degrees).
//!
//! The picture is the panel's source (the selection drawn to a PNG when
//! it was picked); without one a radial ramp stands in. Model space: mm,
//! y up, the box's bottom left at the origin.

use crate::shape_gen::{lerp, polygon, Generated, Paint, Rng, ShapeGenSettings};
use tracedraw_core::geometry::{Affine, BezPath, Point, Rect, Shape as _, Vec2};

/// The most sites a Voronoi or stippling may hold.
pub const MAX_SITES: usize = 20_000;

/// How Voronoi sites are first placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PointMode {
    #[default]
    Random,
    /// One random site in each square of the spacing.
    Uniform,
    Hex,
    Square,
}

impl PointMode {
    pub const ALL: [PointMode; 4] = [
        PointMode::Random,
        PointMode::Uniform,
        PointMode::Hex,
        PointMode::Square,
    ];
    pub fn key(self) -> &'static str {
        match self {
            PointMode::Random => "shapegen.points_random",
            PointMode::Uniform => "shapegen.points_uniform",
            PointMode::Hex => "shapegen.points_hex",
            PointMode::Square => "shapegen.points_square",
        }
    }
}

/// Where Voronoi cells are smaller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Gradient {
    #[default]
    None,
    Linear,
    Radial,
}

impl Gradient {
    pub const ALL: [Gradient; 3] = [Gradient::None, Gradient::Linear, Gradient::Radial];
    pub fn key(self) -> &'static str {
        match self {
            Gradient::None => "shapegen.gradient_none",
            Gradient::Linear => "shapegen.gradient_linear",
            Gradient::Radial => "shapegen.gradient_radial",
        }
    }
}

/// The mark a halftone cell holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HtShape {
    #[default]
    Circle,
    Square,
    Diamond,
    Ellipse,
    /// Bands along the screen's rows, thick where dark.
    Line,
}

impl HtShape {
    pub const ALL: [HtShape; 5] = [
        HtShape::Circle,
        HtShape::Square,
        HtShape::Diamond,
        HtShape::Ellipse,
        HtShape::Line,
    ];
    pub fn key(self) -> &'static str {
        match self {
            HtShape::Circle => "shapegen.cell_circle",
            HtShape::Square => "shapegen.cell_square",
            HtShape::Diamond => "shapegen.cell_diamond",
            HtShape::Ellipse => "shapegen.ht_ellipse",
            HtShape::Line => "shapegen.ht_line",
        }
    }
}

// ----- the source picture ---------------------------------------------------

/// A picture as straight RGBA in 0 to 1, row 0 at the top.
#[derive(Debug, Clone)]
pub struct Tone {
    w: usize,
    h: usize,
    px: Vec<[f32; 4]>,
}

impl Tone {
    /// From a PNG kept as base64 text.
    pub fn from_base64_png(text: &str) -> Option<Tone> {
        use base64::Engine as _;
        let png = base64::engine::general_purpose::STANDARD
            .decode(text.as_bytes())
            .ok()?;
        let (w, h, rgba) = tracedraw_core::bitmap_warp::decode_rgba(&png)?;
        let (w, h) = (w as usize, h as usize);
        if w == 0 || h == 0 || rgba.len() < w * h * 4 {
            return None;
        }
        let px = rgba
            .chunks_exact(4)
            .take(w * h)
            .map(|c| {
                [
                    c[0] as f32 / 255.0,
                    c[1] as f32 / 255.0,
                    c[2] as f32 / 255.0,
                    c[3] as f32 / 255.0,
                ]
            })
            .collect();
        Some(Tone { w, h, px })
    }

    pub fn size(&self) -> (usize, usize) {
        (self.w, self.h)
    }

    /// The colour at `u` (0 left to 1 right), `v` (0 bottom to 1 top),
    /// bilinear between pixel middles.
    pub fn sample(&self, u: f64, v: f64) -> [f32; 4] {
        let x = (u.clamp(0.0, 1.0) * self.w as f64 - 0.5).clamp(0.0, (self.w - 1) as f64);
        let y = ((1.0 - v.clamp(0.0, 1.0)) * self.h as f64 - 0.5).clamp(0.0, (self.h - 1) as f64);
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(self.w - 1), (y0 + 1).min(self.h - 1));
        let (fx, fy) = ((x - x0 as f64) as f32, (y - y0 as f64) as f32);
        let at = |x: usize, y: usize| self.px[y * self.w + x];
        let (a, b, c, d) = (at(x0, y0), at(x1, y0), at(x0, y1), at(x1, y1));
        let mut out = [0.0f32; 4];
        for k in 0..4 {
            let top = a[k] + (b[k] - a[k]) * fx;
            let bot = c[k] + (d[k] - c[k]) * fx;
            out[k] = top + (bot - top) * fy;
        }
        out
    }
}

/// What the tone generators read: the box, its picture (or the stand-in
/// ramp) and the outline that keeps marks inside.
pub(crate) struct Source<'a> {
    pub(crate) rect: Rect,
    tone: Option<Tone>,
    boundary: Option<&'a BezPath>,
    invert: bool,
}

/// Ink amounts of a colour on white: (C, M, Y, K), the darkness, and the
/// alpha.
fn inks(rgba: [f32; 4], invert: bool) -> ([f64; 4], f64, f64) {
    let a = rgba[3].clamp(0.0, 1.0) as f64;
    let mut c = [0.0f64; 3];
    for k in 0..3 {
        // Over white.
        let v = rgba[k].clamp(0.0, 1.0) as f64 * a + (1.0 - a);
        c[k] = if invert { 1.0 - v } else { v };
    }
    let lum = 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2];
    let kk = 1.0 - c[0].max(c[1]).max(c[2]);
    let cmyk = if kk >= 1.0 - 1e-9 {
        [0.0, 0.0, 0.0, 1.0]
    } else {
        [
            (1.0 - c[0] - kk) / (1.0 - kk),
            (1.0 - c[1] - kk) / (1.0 - kk),
            (1.0 - c[2] - kk) / (1.0 - kk),
            kk,
        ]
    };
    (cmyk, (1.0 - lum).clamp(0.0, 1.0), a)
}

impl Source<'_> {
    pub(crate) fn new<'a>(s: &'a ShapeGenSettings) -> Source<'a> {
        Source {
            rect: Rect::new(0.0, 0.0, s.width, s.height),
            tone: s.tone.as_deref().and_then(Tone::from_base64_png),
            boundary: s.boundary.as_ref(),
            invert: s.invert,
        }
    }

    fn color(&self, p: Point) -> [f32; 4] {
        let r = self.rect;
        let (u, v) = ((p.x - r.x0) / r.width(), (p.y - r.y0) / r.height());
        match &self.tone {
            Some(t) => t.sample(u, v),
            None => {
                // The stand-in: dark in the middle, white at the edge.
                let d = ((u - 0.5).powi(2) + (v - 0.5).powi(2)).sqrt() / 0.5;
                let g = d.clamp(0.0, 1.0) as f32;
                [g, g, g, 1.0]
            }
        }
    }

    /// Ink amounts at `p` (darkness at index 4), or None outside: off the
    /// box, transparent in the picture or outside the outline.
    pub(crate) fn at(&self, p: Point) -> Option<[f64; 5]> {
        if !inside_rect(self.rect, p) || !inside(self.boundary, p) {
            return None;
        }
        let (cmyk, dark, a) = inks(self.color(p), self.invert);
        if a < 0.5 {
            return None;
        }
        Some([cmyk[0], cmyk[1], cmyk[2], cmyk[3], dark])
    }

    /// A raster of the box for relaxation: pixel size and the sample
    /// grid's width and height.
    pub(crate) fn raster(&self, want: usize) -> (usize, usize) {
        if let Some(t) = &self.tone {
            let (w, h) = t.size();
            if w * h >= 64 {
                return (w.min(400), h.min(400));
            }
        }
        let r = self.rect;
        let aspect = r.width() / r.height().max(1e-9);
        let h = ((want as f64 / aspect).sqrt()).round().max(8.0) as usize;
        let w = (h as f64 * aspect).round().max(8.0) as usize;
        (w.min(600), h.min(600))
    }
}

fn inside_rect(r: Rect, p: Point) -> bool {
    p.x >= r.x0 && p.x <= r.x1 && p.y >= r.y0 && p.y <= r.y1
}

/// Inside the outline by the even-odd rule (holes outside); everywhere
/// when there is none.
pub fn inside(boundary: Option<&BezPath>, p: Point) -> bool {
    match boundary {
        None => true,
        Some(b) => b.winding(p) % 2 != 0,
    }
}

// ----- sites, grid and cells ------------------------------------------------

/// Sites bucketed by square cells for neighbour searches.
struct SiteGrid {
    x0: f64,
    y0: f64,
    cell: f64,
    nx: i64,
    ny: i64,
    buckets: Vec<Vec<u32>>,
}

impl SiteGrid {
    fn new(sites: &[Point], rect: Rect) -> SiteGrid {
        let n = sites.len().max(1) as f64;
        let area = (rect.width() * rect.height()).max(1e-9);
        let mut cell = (area / n).sqrt().max(1e-6);
        // At most about four million buckets.
        while (rect.width() / cell) * (rect.height() / cell) > 4.0e6 {
            cell *= 2.0;
        }
        let nx = ((rect.width() / cell).ceil() as i64).max(1);
        let ny = ((rect.height() / cell).ceil() as i64).max(1);
        let mut g = SiteGrid {
            x0: rect.x0,
            y0: rect.y0,
            cell,
            nx,
            ny,
            buckets: vec![Vec::new(); (nx * ny) as usize],
        };
        for (k, p) in sites.iter().enumerate() {
            let (i, j) = g.idx(*p);
            g.buckets[(j * nx + i) as usize].push(k as u32);
        }
        g
    }

    fn idx(&self, p: Point) -> (i64, i64) {
        let i = ((p.x - self.x0) / self.cell).floor() as i64;
        let j = ((p.y - self.y0) / self.cell).floor() as i64;
        (i.clamp(0, self.nx - 1), j.clamp(0, self.ny - 1))
    }

    /// The sites in the cells `r` steps (in the chessboard sense) from
    /// cell `(ci, cj)`.
    fn ring(&self, ci: i64, cj: i64, r: i64, f: &mut impl FnMut(usize)) {
        let mut visit = |i: i64, j: i64| {
            if i >= 0 && j >= 0 && i < self.nx && j < self.ny {
                for k in &self.buckets[(j * self.nx + i) as usize] {
                    f(*k as usize);
                }
            }
        };
        if r == 0 {
            visit(ci, cj);
            return;
        }
        for i in (ci - r)..=(ci + r) {
            visit(i, cj - r);
            visit(i, cj + r);
        }
        for j in (cj - r + 1)..=(cj + r - 1) {
            visit(ci - r, j);
            visit(ci + r, j);
        }
    }

    fn max_ring(&self) -> i64 {
        self.nx.max(self.ny)
    }

    fn nearest(&self, sites: &[Point], p: Point) -> Option<usize> {
        let (ci, cj) = self.idx(p);
        let mut best: Option<(usize, f64)> = None;
        for r in 0..=self.max_ring() {
            self.ring(ci, cj, r, &mut |k| {
                let d = (sites[k] - p).hypot2();
                if best.is_none_or(|(_, bd)| d < bd) {
                    best = Some((k, d));
                }
            });
            if let Some((_, d)) = best {
                if d.sqrt() <= r as f64 * self.cell {
                    break;
                }
            }
        }
        best.map(|(k, _)| k)
    }
}

/// A cell corner and the neighbour whose bisector the edge from it to the
/// next corner lies on (-1 for the box).
type Corner = (Point, i64);

/// Keep the part of `poly` where `(p - m) . n <= 0`; new edges along the
/// cut get `tag`.
fn clip(poly: &[Corner], m: Point, n: Vec2, tag: i64) -> Vec<Corner> {
    let mut out = Vec::with_capacity(poly.len() + 2);
    let d = |p: Point| (p - m).dot(n);
    for k in 0..poly.len() {
        let (a, ta) = poly[k];
        let (b, _) = poly[(k + 1) % poly.len()];
        let (da, db) = (d(a), d(b));
        let cut = |da: f64, db: f64| a.lerp(b, da / (da - db));
        match (da <= 0.0, db <= 0.0) {
            (true, true) => out.push((a, ta)),
            (true, false) => {
                out.push((a, ta));
                out.push((cut(da, db), tag));
            }
            (false, true) => out.push((cut(da, db), ta)),
            (false, false) => {}
        }
    }
    out
}

/// The Voronoi cell of site `i` inside `rect`.
fn cell_of(i: usize, sites: &[Point], grid: &SiteGrid, rect: Rect) -> Vec<Corner> {
    let s = sites[i];
    let mut poly: Vec<Corner> = vec![
        (Point::new(rect.x0, rect.y0), -1),
        (Point::new(rect.x1, rect.y0), -1),
        (Point::new(rect.x1, rect.y1), -1),
        (Point::new(rect.x0, rect.y1), -1),
    ];
    let (ci, cj) = grid.idx(s);
    let mut cand: Vec<usize> = Vec::new();
    for r in 0..=grid.max_ring() {
        cand.clear();
        grid.ring(ci, cj, r, &mut |k| {
            if k != i {
                cand.push(k)
            }
        });
        cand.sort_by(|a, b| {
            (sites[*a] - s)
                .hypot2()
                .total_cmp(&(sites[*b] - s).hypot2())
        });
        for &k in &cand {
            let q = sites[k];
            let n = q - s;
            if n.hypot2() < 1e-18 {
                // The same place: the later site gets nothing.
                if k < i {
                    return Vec::new();
                }
                continue;
            }
            poly = clip(&poly, s.midpoint(q), n, k as i64);
            if poly.len() < 3 {
                return Vec::new();
            }
        }
        let reach = poly
            .iter()
            .map(|(p, _)| (*p - s).hypot())
            .fold(0.0, f64::max);
        if r as f64 * grid.cell >= 2.0 * reach {
            break;
        }
    }
    poly
}

fn area_centroid(pts: &[Point]) -> (f64, Point) {
    let mut a = 0.0;
    let (mut cx, mut cy) = (0.0, 0.0);
    for k in 0..pts.len() {
        let (p, q) = (pts[k], pts[(k + 1) % pts.len()]);
        let c = p.x * q.y - q.x * p.y;
        a += c;
        cx += (p.x + q.x) * c;
        cy += (p.y + q.y) * c;
    }
    a /= 2.0;
    if a.abs() < 1e-12 {
        let n = pts.len().max(1) as f64;
        let m = pts.iter().fold(Vec2::ZERO, |s, p| s + p.to_vec2()) / n;
        return (0.0, m.to_point());
    }
    (a, Point::new(cx / (6.0 * a), cy / (6.0 * a)))
}

/// Move each site to the middle of its cell, `iters` times.
fn lloyd_exact(sites: &mut [Point], rect: Rect, iters: u32) {
    for _ in 0..iters {
        let grid = SiteGrid::new(sites, rect);
        let next: Vec<Point> = (0..sites.len())
            .map(|i| {
                let poly: Vec<Point> = cell_of(i, sites, &grid, rect)
                    .into_iter()
                    .map(|(p, _)| p)
                    .collect();
                if poly.len() < 3 {
                    return sites[i];
                }
                let (a, c) = area_centroid(&poly);
                if a.abs() > 1e-12 {
                    c
                } else {
                    sites[i]
                }
            })
            .collect();
        sites.copy_from_slice(&next);
    }
}

/// Weighted Lloyd on a raster of `w` x `h` samples over `rect`: each site
/// moves to the weighted middle of the samples nearest to it.
fn lloyd_raster(sites: &mut [Point], rect: Rect, w: usize, h: usize, weight: &[f32], iters: u32) {
    if sites.is_empty() || weight.len() < w * h {
        return;
    }
    let (dx, dy) = (rect.width() / w as f64, rect.height() / h as f64);
    for _ in 0..iters {
        let grid = SiteGrid::new(sites, rect);
        let mut acc = vec![(0.0f64, 0.0f64, 0.0f64); sites.len()];
        for y in 0..h {
            for x in 0..w {
                let wt = weight[y * w + x] as f64;
                if wt <= 0.0 {
                    continue;
                }
                let p = Point::new(
                    rect.x0 + (x as f64 + 0.5) * dx,
                    rect.y0 + (y as f64 + 0.5) * dy,
                );
                if let Some(k) = grid.nearest(sites, p) {
                    let a = &mut acc[k];
                    a.0 += wt * p.x;
                    a.1 += wt * p.y;
                    a.2 += wt;
                }
            }
        }
        for (s, a) in sites.iter_mut().zip(&acc) {
            if a.2 > 0.0 {
                *s = Point::new(a.0 / a.2, a.1 / a.2);
            }
        }
    }
}

/// How dense the sites are at `p`: 1 where cells are smallest, down to
/// 1 - strength.
fn density(s: &ShapeGenSettings, rect: Rect, p: Point) -> f64 {
    let k = s.vor_grad_strength / 100.0;
    let t = match s.vor_gradient {
        Gradient::None => return 1.0,
        Gradient::Linear => {
            let a = s.vor_grad_angle.to_radians();
            let dir = Vec2::new(a.cos(), a.sin());
            let c = rect.center();
            let half = (rect.width() * dir.x.abs() + rect.height() * dir.y.abs()) / 2.0;
            ((p - c).dot(dir) / half.max(1e-9) + 1.0) / 2.0
        }
        Gradient::Radial => {
            let c = rect.center();
            let half = (rect.width().hypot(rect.height()) / 2.0).max(1e-9);
            (p - c).hypot() / half
        }
    }
    .clamp(0.0, 1.0);
    let t = if s.vor_grad_edge { 1.0 - t } else { t };
    (1.0 - k * t).max(0.03)
}

fn raster_weights(rect: Rect, w: usize, h: usize, f: impl Fn(Point) -> f64) -> Vec<f32> {
    let (dx, dy) = (rect.width() / w as f64, rect.height() / h as f64);
    let mut out = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let p = Point::new(
                rect.x0 + (x as f64 + 0.5) * dx,
                rect.y0 + (y as f64 + 0.5) * dy,
            );
            out.push(f(p).max(0.0) as f32);
        }
    }
    out
}

/// Relax the sites `iters` times: by exact cells when every place counts
/// the same, else by a raster weighted by the gradient and the outline.
fn relax(s: &ShapeGenSettings, sites: &mut [Point], rect: Rect, iters: u32) {
    if iters == 0 || sites.is_empty() {
        return;
    }
    if s.vor_gradient == Gradient::None && s.boundary.is_none() {
        lloyd_exact(sites, rect, iters);
        return;
    }
    let total = (sites.len() * 24).clamp(20_000, 250_000);
    let aspect = rect.width() / rect.height().max(1e-9);
    let h = ((total as f64 / aspect).sqrt().round() as usize).max(8);
    let w = ((h as f64 * aspect).round() as usize).max(8);
    let b = s.boundary.as_ref();
    let weight = raster_weights(rect, w, h, |p| {
        // Relaxed sites settle at a density that follows the square
        // root of the weight: squared, they keep the gradient's density.
        if inside(b, p) {
            density(s, rect, p).powi(2)
        } else {
            0.0
        }
    });
    lloyd_raster(sites, rect, w, h, &weight, iters);
}

/// The Voronoi sites of the settings: the panel's own points, or placed
/// by the point mode, thinned by the gradient, kept inside the outline
/// and relaxed.
pub fn voronoi_sites(s: &ShapeGenSettings) -> Vec<Point> {
    let rect = Rect::new(0.0, 0.0, s.width, s.height);
    if !s.vor_points.is_empty() {
        return s
            .vor_points
            .iter()
            .map(|p| Point::new(p[0] * s.width, p[1] * s.height))
            .collect();
    }
    let sp = s.vor_spacing;
    let jit = s.vor_jitter / 100.0;
    let mut rng = Rng::new(s.seed, 0x0005_17e5);
    let mut pts: Vec<Point> = Vec::new();
    let (w, h) = (s.width, s.height);
    match s.vor_mode {
        PointMode::Random => {
            let n = ((w * h) / (sp * sp)).round().clamp(1.0, MAX_SITES as f64) as usize;
            for _ in 0..n {
                pts.push(Point::new(rng.unit() * w, rng.unit() * h));
            }
        }
        PointMode::Uniform => {
            let (nx, ny) = ((w / sp).ceil() as usize, (h / sp).ceil() as usize);
            for j in 0..ny {
                for i in 0..nx {
                    let p = Point::new((i as f64 + rng.unit()) * sp, (j as f64 + rng.unit()) * sp);
                    if p.x <= w && p.y <= h {
                        pts.push(p);
                    }
                }
            }
        }
        PointMode::Hex | PointMode::Square => {
            let hex = s.vor_mode == PointMode::Hex;
            let dy = if hex { sp * 3f64.sqrt() / 2.0 } else { sp };
            let (nx, ny) = ((w / sp).ceil() as i64 + 1, (h / dy).ceil() as i64 + 1);
            for j in 0..ny {
                let shift = if hex && j % 2 == 1 { sp / 2.0 } else { 0.0 };
                for i in 0..nx {
                    let p = Point::new(
                        sp / 2.0 + i as f64 * sp + shift + (rng.unit() - 0.5) * jit * sp,
                        dy / 2.0 + j as f64 * dy + (rng.unit() - 0.5) * jit * sp,
                    );
                    if inside_rect(rect, p) {
                        pts.push(p);
                    }
                }
            }
        }
    }
    if s.vor_gradient != Gradient::None {
        pts.retain(|p| rng.unit() < density(s, rect, *p));
    }
    let b = s.boundary.as_ref();
    pts.retain(|p| inside(b, *p));
    pts.truncate(MAX_SITES);
    relax(s, &mut pts, rect, s.vor_relax);
    pts
}

/// The current sites relaxed once more, as the panel's own points (0 to
/// 1 across the box).
pub fn voronoi_relax_once(s: &ShapeGenSettings) -> Vec<[f64; 2]> {
    let s = s.sanitized();
    let rect = Rect::new(0.0, 0.0, s.width, s.height);
    let mut pts = voronoi_sites(&s);
    relax(&s, &mut pts, rect, 1);
    pts.iter()
        .map(|p| {
            [
                (p.x / s.width).clamp(0.0, 1.0),
                (p.y / s.height).clamp(0.0, 1.0),
            ]
        })
        .collect()
}

/// A convex polygon shrunk by `d`: every edge moved inward.
fn inset_convex(pts: &[Point], d: f64) -> Vec<Point> {
    let (a, _) = area_centroid(pts);
    let sign = if a >= 0.0 { 1.0 } else { -1.0 };
    let mut poly: Vec<Corner> = pts.iter().map(|p| (*p, -1)).collect();
    for k in 0..pts.len() {
        let (p, q) = (pts[k], pts[(k + 1) % pts.len()]);
        let e = q - p;
        let len = e.hypot();
        if len < 1e-12 {
            continue;
        }
        // Outward normal; keep the side at least `d` inside the edge.
        let out = Vec2::new(e.y, -e.x) / len * sign;
        poly = clip(&poly, p - out * d, out, -1);
        if poly.len() < 3 {
            return Vec::new();
        }
    }
    poly.into_iter().map(|(p, _)| p).collect()
}

/// A closed polygon with its corners rounded by `r` (less where the
/// edges are short).
fn rounded(pts: &[Point], r: f64) -> BezPath {
    let n = pts.len();
    if n < 3 || r <= 0.0 {
        return polygon(pts);
    }
    let mut p = BezPath::new();
    for k in 0..n {
        let (a, v, b) = (pts[(k + n - 1) % n], pts[k], pts[(k + 1) % n]);
        let (la, lb) = ((a - v).hypot(), (b - v).hypot());
        if la < 1e-12 || lb < 1e-12 {
            continue;
        }
        let (ua, ub) = ((a - v) / la, (b - v) / lb);
        let theta = ua.dot(ub).clamp(-1.0, 1.0).acos();
        let half = (theta / 2.0).tan().max(1e-9);
        let t = (r / half).min(la / 2.0).min(lb / 2.0);
        let rr = t * half;
        let (p1, p2) = (v + ua * t, v + ub * t);
        let turn = std::f64::consts::PI - theta;
        let hl = 4.0 / 3.0 * (turn / 4.0).tan() * rr;
        if k == 0 {
            p.move_to(p1);
        } else {
            p.line_to(p1);
        }
        p.curve_to(p1 - ua * hl, p2 - ub * hl, p2);
    }
    p.close_path();
    p
}

pub(crate) fn voronoi(s: &ShapeGenSettings, out: &mut Generated) {
    let rect = Rect::new(0.0, 0.0, s.width, s.height);
    out.frame = Some(rect);
    let sites = voronoi_sites(s);
    out.sites = sites.clone();
    if sites.is_empty() {
        return;
    }
    let grid = SiteGrid::new(&sites, rect);
    let clip_to = s.boundary.as_ref().map(|b| {
        if s.vor_inset > 0.0 {
            tracedraw_core::shaping::offset(b, -s.vor_inset)
        } else {
            b.clone()
        }
    });
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    for i in 0..sites.len() {
        if out.full() {
            out.cut = true;
            break;
        }
        let poly = cell_of(i, &sites, &grid, rect);
        if poly.len() < 3 {
            continue;
        }
        if s.vor_delaunay {
            for (_, t) in &poly {
                if *t >= 0 {
                    let j = *t as usize;
                    pairs.push((i.min(j), i.max(j)));
                }
            }
        }
        if !s.vor_cells {
            continue;
        }
        let mut pts: Vec<Point> = poly.into_iter().map(|(p, _)| p).collect();
        if s.vor_inset > 0.0 {
            pts = inset_convex(&pts, s.vor_inset / 2.0);
            if pts.len() < 3 {
                continue;
            }
        }
        let mut path = rounded(&pts, s.vor_round);
        if let Some(c) = &clip_to {
            path =
                tracedraw_core::shaping::overlay(&path, c, tracedraw_core::shaping::Op::Intersect);
            if path.elements().is_empty() {
                continue;
            }
        }
        out.push(path, true, Paint::Line);
    }
    if s.vor_delaunay {
        pairs.sort_unstable();
        pairs.dedup();
        for (i, j) in pairs {
            let mut p = BezPath::new();
            p.move_to(sites[i]);
            p.line_to(sites[j]);
            out.push(p, false, Paint::Line);
        }
    }
    if s.vor_show_sites {
        let r = (s.vor_spacing * 0.06).max(0.05);
        let b = s.boundary.as_ref();
        for p in &sites {
            if !inside(b, *p) {
                continue;
            }
            out.push(
                tracedraw_core::geometry::ellipse_path(Rect::from_center_size(
                    *p,
                    (2.0 * r, 2.0 * r),
                )),
                true,
                Paint::Solid,
            );
        }
    }
}

// ----- stippling ------------------------------------------------------------

pub(crate) fn stipple(s: &ShapeGenSettings, out: &mut Generated) {
    let src = Source::new(s);
    let rect = src.rect;
    out.frame = Some(rect);
    let dark = |p: Point| src.at(p).map_or(0.0, |v| v[4]);
    let (w, h) = src.raster(40_000);
    let weight = raster_weights(rect, w, h, dark);
    if weight.iter().all(|v| *v <= 0.0) {
        return;
    }
    // First places: darker places take more, by rejection.
    let mut rng = Rng::new(s.seed, 0x5717);
    let n = s.st_dots as usize;
    let mut pts = Vec::with_capacity(n);
    let mut tries = 0usize;
    while pts.len() < n && tries < n * 200 {
        tries += 1;
        let p = Point::new(rng.unit() * rect.width(), rng.unit() * rect.height());
        if rng.unit() < dark(p) {
            pts.push(p);
        }
    }
    lloyd_raster(&mut pts, rect, w, h, &weight, s.st_iter);
    for p in pts {
        if out.full() {
            out.cut = true;
            return;
        }
        let d = if s.st_by_tone {
            lerp(s.st_min, s.st_max, dark(p))
        } else {
            s.st_max
        };
        if d <= 0.0 {
            continue;
        }
        out.push(
            tracedraw_core::geometry::ellipse_path(Rect::from_center_size(p, (d, d))),
            true,
            Paint::Solid,
        );
    }
}

// ----- halftone -------------------------------------------------------------

pub(crate) fn halftone(s: &ShapeGenSettings, out: &mut Generated) {
    let src = Source::new(s);
    let rect = src.rect;
    out.frame = Some(rect);
    // (screen angle, channel): 0 to 3 the inks, 4 the darkness.
    let screens: Vec<(f64, usize)> = if s.ht_cmyk {
        vec![(15.0, 0), (75.0, 1), (0.0, 2), (45.0, 3)]
    } else {
        vec![(s.ht_angle, 4)]
    };
    let c = s.ht_cell;
    let mid = rect.center();
    let reach = rect.width().hypot(rect.height()) / 2.0 + c;
    let n = (reach / c).ceil() as i64;
    let (lo, hi) = (s.ht_min / 100.0, s.ht_max / 100.0);
    for (angle, ch) in screens {
        let a = angle.to_radians();
        let (sn, cs) = a.sin_cos();
        let to = |u: f64, v: f64| Point::new(mid.x + u * cs - v * sn, mid.y + u * sn + v * cs);
        let paint = if ch == 4 {
            Paint::Solid
        } else {
            Paint::Ink(ch as u8)
        };
        if s.ht_shape == HtShape::Line {
            let step = c / 3.0;
            let m = (reach / step).ceil() as i64;
            for j in -n..=n {
                let v = j as f64 * c;
                let mut top: Vec<Point> = Vec::new();
                let mut bot: Vec<Point> = Vec::new();
                let flush = |top: &mut Vec<Point>, bot: &mut Vec<Point>, out: &mut Generated| {
                    if top.len() >= 2 {
                        let mut pts = std::mem::take(top);
                        pts.extend(bot.drain(..).rev());
                        out.push(polygon(&pts), true, paint);
                    }
                    top.clear();
                    bot.clear();
                };
                for i in -m..=m {
                    if out.full() {
                        out.cut = true;
                        return;
                    }
                    let u = i as f64 * step;
                    let th = src
                        .at(to(u, v))
                        .map(|k| c * lerp(lo, hi, k[ch]))
                        .filter(|th| *th > c * 0.01);
                    match th {
                        Some(th) => {
                            top.push(to(u, v + th / 2.0));
                            bot.push(to(u, v - th / 2.0));
                        }
                        None => flush(&mut top, &mut bot, out),
                    }
                }
                flush(&mut top, &mut bot, out);
            }
            continue;
        }
        for j in -n..=n {
            for i in -n..=n {
                if out.full() {
                    out.cut = true;
                    return;
                }
                let p = to(i as f64 * c, j as f64 * c);
                let Some(k) = src.at(p) else {
                    continue;
                };
                let cov = k[ch];
                if cov <= 0.002 {
                    continue;
                }
                // Area follows the ink: size by the square root.
                let d = c * lerp(lo, hi, cov.sqrt());
                if d < c * 0.02 {
                    continue;
                }
                let path = match s.ht_shape {
                    HtShape::Circle => {
                        tracedraw_core::geometry::ellipse_path(Rect::from_center_size(p, (d, d)))
                    }
                    HtShape::Ellipse => {
                        let mut e = tracedraw_core::geometry::ellipse_path(Rect::from_center_size(
                            p,
                            (d * 1.35, d * 0.75),
                        ));
                        e.apply_affine(Affine::rotate_about(a, p));
                        e
                    }
                    HtShape::Square | HtShape::Diamond => {
                        let turn = if s.ht_shape == HtShape::Diamond {
                            a + std::f64::consts::FRAC_PI_4
                        } else {
                            a
                        };
                        let h = d / 2.0;
                        let pts: Vec<Point> = [(-h, -h), (h, -h), (h, h), (-h, h)]
                            .iter()
                            .map(|(x, y)| {
                                Affine::rotate_about(turn, p) * Point::new(p.x + x, p.y + y)
                            })
                            .collect();
                        polygon(&pts)
                    }
                    HtShape::Line => continue,
                };
                out.push(path, true, paint);
            }
        }
    }
}

/// Ink colours of the four screens.
pub fn ink_color(k: u8) -> tracedraw_core::Color {
    match k {
        0 => tracedraw_core::Color::cmyk_pct(100.0, 0.0, 0.0, 0.0),
        1 => tracedraw_core::Color::cmyk_pct(0.0, 100.0, 0.0, 0.0),
        2 => tracedraw_core::Color::cmyk_pct(0.0, 0.0, 100.0, 0.0),
        _ => tracedraw_core::Color::cmyk_pct(0.0, 0.0, 0.0, 100.0),
    }
}

/// The screen's name key, for the group of each ink.
pub fn ink_key(k: u8) -> &'static str {
    match k {
        0 => "shapegen.ink_cyan",
        1 => "shapegen.ink_magenta",
        2 => "shapegen.ink_yellow",
        _ => "shapegen.ink_black",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape_gen::{generate, GenKind};

    fn vor() -> ShapeGenSettings {
        ShapeGenSettings {
            kind: GenKind::Voronoi,
            vor_spacing: 10.0,
            vor_relax: 0,
            ..ShapeGenSettings::default()
        }
    }

    fn area(p: &BezPath) -> f64 {
        p.area().abs()
    }

    #[test]
    fn voronoi_cells_tile_the_box_and_hold_their_sites() {
        for mode in PointMode::ALL {
            let s = ShapeGenSettings {
                vor_mode: mode,
                ..vor()
            };
            let g = generate(&s);
            assert!(g.items.len() > 50, "{mode:?} {}", g.items.len());
            let total: f64 = g.items.iter().map(|i| area(&i.path)).sum();
            assert!((total - 100.0 * 100.0).abs() < 1.0, "{mode:?} {total}");
            for (it, site) in g.items.iter().zip(&g.sites) {
                assert!(it.path.winding(*site) != 0, "{mode:?}");
            }
        }
    }

    #[test]
    fn relaxing_evens_the_cells() {
        let spread = |iters: u32| {
            let g = generate(&ShapeGenSettings {
                vor_relax: iters,
                ..vor()
            });
            let a: Vec<f64> = g.items.iter().map(|i| area(&i.path)).collect();
            let m = a.iter().sum::<f64>() / a.len() as f64;
            (a.iter().map(|x| (x - m).powi(2)).sum::<f64>() / a.len() as f64).sqrt() / m
        };
        let (raw, relaxed) = (spread(0), spread(10));
        assert!(relaxed < raw * 0.5, "{raw} {relaxed}");
        // Relax once moves the panel's points the same way.
        let pts = voronoi_relax_once(&vor());
        assert_eq!(pts.len(), voronoi_sites(&vor()).len());
    }

    #[test]
    fn a_radial_gradient_makes_small_cells_in_the_middle() {
        let s = ShapeGenSettings {
            vor_gradient: Gradient::Radial,
            vor_grad_strength: 90.0,
            vor_relax: 5,
            ..vor()
        };
        let g = generate(&s);
        let mid = Point::new(50.0, 50.0);
        let near: Vec<f64> = g
            .items
            .iter()
            .zip(&g.sites)
            .filter(|(_, p)| (**p - mid).hypot() < 20.0)
            .map(|(i, _)| area(&i.path))
            .collect();
        let far: Vec<f64> = g
            .items
            .iter()
            .zip(&g.sites)
            .filter(|(_, p)| (**p - mid).hypot() > 45.0)
            .map(|(i, _)| area(&i.path))
            .collect();
        let avg = |v: &[f64]| v.iter().sum::<f64>() / v.len().max(1) as f64;
        assert!(!near.is_empty() && !far.is_empty());
        assert!(avg(&near) * 1.6 < avg(&far), "{} {}", avg(&near), avg(&far));
    }

    #[test]
    fn cells_fill_the_outline_and_leave_its_hole_empty() {
        // A ring: outer square with a square hole.
        let mut b = tracedraw_core::geometry::rect_path(Rect::new(0.0, 0.0, 100.0, 100.0), 0.0);
        b.extend(tracedraw_core::geometry::rect_path(
            Rect::new(30.0, 30.0, 70.0, 70.0),
            0.0,
        ));
        let s = ShapeGenSettings {
            boundary: Some(b),
            ..vor()
        };
        let g = generate(&s);
        assert!(!g.items.is_empty());
        let total: f64 = g.items.iter().map(|i| area(&i.path)).sum();
        assert!((total - (10_000.0 - 1_600.0)).abs() < 30.0, "{total}");
        assert!(g
            .items
            .iter()
            .all(|i| i.path.winding(Point::new(50.0, 50.0)) == 0));
    }

    #[test]
    fn inset_and_rounding_shrink_the_cells() {
        let plain: f64 = generate(&vor()).items.iter().map(|i| area(&i.path)).sum();
        let g = generate(&ShapeGenSettings {
            vor_inset: 1.0,
            vor_round: 1.0,
            ..vor()
        });
        let cut: f64 = g.items.iter().map(|i| area(&i.path)).sum();
        assert!(cut < plain * 0.95, "{plain} {cut}");
    }

    #[test]
    fn delaunay_and_sites_are_drawn_on_request() {
        let s = ShapeGenSettings {
            vor_cells: false,
            vor_delaunay: true,
            vor_show_sites: true,
            ..vor()
        };
        let g = generate(&s);
        let n = g.sites.len();
        let lines = g.items.iter().filter(|i| i.paint == Paint::Line).count();
        let dots = g.items.iter().filter(|i| i.paint == Paint::Solid).count();
        assert_eq!(dots, n);
        // A planar triangulation: fewer than 3n edges, more than n.
        assert!(lines > n && lines < 3 * n, "{lines} {n}");
    }

    #[test]
    fn the_panels_points_replace_the_generated_ones_in_order() {
        let s = ShapeGenSettings {
            vor_points: vec![[0.25, 0.25], [0.75, 0.25], [0.5, 0.8]],
            ..vor()
        };
        let g = generate(&s);
        assert_eq!(g.sites.len(), 3);
        assert_eq!(g.items.len(), 3);
        assert!((g.sites[1].x - 75.0).abs() < 1e-9);
    }

    #[test]
    fn halftone_dots_grow_where_it_is_dark() {
        let s = ShapeGenSettings {
            kind: GenKind::Halftone,
            ht_cell: 4.0,
            ..ShapeGenSettings::default()
        };
        let g = generate(&s);
        assert!(g.items.len() > 100);
        // The stand-in ramp is dark in the middle.
        let size_near = |q: Point| {
            g.items
                .iter()
                .map(|i| i.path.bounding_box())
                .min_by(|a, b| {
                    (a.center() - q)
                        .hypot()
                        .total_cmp(&(b.center() - q).hypot())
                })
                .map(|b| b.width())
                .unwrap_or(0.0)
        };
        assert!(size_near(Point::new(50.0, 50.0)) > size_near(Point::new(90.0, 50.0)) * 1.5);
        for shape in HtShape::ALL {
            let g = generate(&ShapeGenSettings {
                ht_shape: shape,
                ..s.clone()
            });
            assert!(!g.items.is_empty(), "{shape:?}");
        }
    }

    #[test]
    fn cmyk_halftone_makes_four_screens() {
        let g = generate(&ShapeGenSettings {
            kind: GenKind::Halftone,
            ht_cell: 5.0,
            ht_cmyk: true,
            ..ShapeGenSettings::default()
        });
        // Grey has no colour ink: only black dots.
        assert!(g.items.iter().all(|i| matches!(i.paint, Paint::Ink(3))));
        // A colour picture uses every ink.
        let png = tracedraw_core::bitmap_warp::encode_rgba(
            2,
            2,
            &[
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 40, 40, 40, 255,
            ],
        )
        .expect("png");
        use base64::Engine as _;
        let g = generate(&ShapeGenSettings {
            kind: GenKind::Halftone,
            ht_cell: 5.0,
            ht_cmyk: true,
            tone: Some(base64::engine::general_purpose::STANDARD.encode(png)),
            ..ShapeGenSettings::default()
        });
        for k in 0..4u8 {
            assert!(g.items.iter().any(|i| i.paint == Paint::Ink(k)), "ink {k}");
        }
    }

    #[test]
    fn stipples_crowd_where_it_is_dark() {
        let s = ShapeGenSettings {
            kind: GenKind::Stipple,
            st_dots: 600,
            st_iter: 8,
            ..ShapeGenSettings::default()
        };
        let g = generate(&s);
        assert!(g.items.len() > 500, "{}", g.items.len());
        let mid = Point::new(50.0, 50.0);
        let near = g
            .items
            .iter()
            .filter(|i| (i.path.bounding_box().center() - mid).hypot() < 20.0)
            .count();
        let ring = g
            .items
            .iter()
            .filter(|i| {
                let d = (i.path.bounding_box().center() - mid).hypot();
                d > 30.0 && d < 36.0
            })
            .count();
        // The inner disc (area 1257) holds far more than the ring (1244).
        assert!(near as f64 > ring as f64 * 1.8, "{near} {ring}");
    }

    #[test]
    fn tone_marks_stay_inside_the_outline() {
        let b = tracedraw_core::geometry::ellipse_path(Rect::new(20.0, 20.0, 80.0, 80.0));
        for kind in [GenKind::Halftone, GenKind::Stipple] {
            let g = generate(&ShapeGenSettings {
                kind,
                boundary: Some(b.clone()),
                invert: true,
                st_dots: 300,
                ..ShapeGenSettings::default()
            });
            assert!(!g.items.is_empty(), "{kind:?}");
            for i in &g.items {
                let c = i.path.bounding_box().center();
                assert!(
                    (c - Point::new(50.0, 50.0)).hypot() < 31.0,
                    "{kind:?} {c:?}"
                );
            }
        }
    }
}
