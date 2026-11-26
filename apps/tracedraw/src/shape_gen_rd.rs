//! Shape Generator reaction-diffusion: organic spots, mazes, coral and
//! stripe patterns grown by two chemicals on a grid and traced as curves.
//!
//! The grid holds two amounts per cell, `u` (the feed, starting at 1) and
//! `v` (the catalyst, starting at 0 but for a few seeded patches). Each
//! step every cell takes
//!
//! ```text
//! u += Du * lap(u) - u * v * v + feed * (1 - u)
//! v += Dv * lap(v) + u * v * v - (kill + feed) * v
//! ```
//!
//! with Du = 1, Dv = 0.5 and a 3 x 3 Laplacian (0.2 for the four sides,
//! 0.05 for the corners, -1 for the cell), wrapping round the edges. The
//! feed and kill rates pick the pattern (the presets, or the panel's own).
//! The result is traced by marching squares at a level (a share of the
//! highest `v`) as smooth closed curves: one filled compound curve whose
//! holes stay open, or separate outlines. Cells outside the box count as
//! empty, so shapes cut by the box are closed along it.
//!
//! The last grid made is kept, so changing only the level or the look
//! does not grow the pattern again. Model space: mm, y up.

use crate::shape_gen::{smooth_through, Generated, Paint, Rng, ShapeGenSettings};
use std::sync::{Arc, Mutex};
use tracedraw_core::geometry::{BezPath, PathEl, Point, Rect};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RdPreset {
    #[default]
    Spots,
    Maze,
    Coral,
    Holes,
    Stripes,
    Custom,
}

impl RdPreset {
    pub const ALL: [RdPreset; 6] = [
        RdPreset::Spots,
        RdPreset::Maze,
        RdPreset::Coral,
        RdPreset::Holes,
        RdPreset::Stripes,
        RdPreset::Custom,
    ];
    pub fn key(self) -> &'static str {
        match self {
            RdPreset::Spots => "shapegen.rd_spots",
            RdPreset::Maze => "shapegen.rd_maze",
            RdPreset::Coral => "shapegen.rd_coral",
            RdPreset::Holes => "shapegen.rd_holes",
            RdPreset::Stripes => "shapegen.rd_stripes",
            RdPreset::Custom => "shapegen.rd_custom",
        }
    }
    /// The feed and kill rates.
    pub fn rates(self, s: &ShapeGenSettings) -> (f64, f64) {
        match self {
            RdPreset::Spots => (0.0367, 0.0649),
            RdPreset::Maze => (0.029, 0.057),
            RdPreset::Coral => (0.0545, 0.062),
            RdPreset::Holes => (0.039, 0.058),
            RdPreset::Stripes => (0.037, 0.06),
            RdPreset::Custom => (s.rd_feed, s.rd_kill),
        }
    }
}

/// What a grid was grown from.
#[derive(Debug, Clone, Copy, PartialEq)]
struct GridKey {
    nx: usize,
    ny: usize,
    feed: f64,
    kill: f64,
    steps: u32,
    seed: u32,
}

/// The grid's size for the settings: `rd_cells` across the longer side.
fn grid_size(s: &ShapeGenSettings) -> (usize, usize) {
    let n = s.rd_cells.max(8) as f64;
    let side = s.width.max(s.height).max(1e-9);
    let nx = ((s.width / side * n).round() as usize).max(4);
    let ny = ((s.height / side * n).round() as usize).max(4);
    (nx, ny)
}

/// The catalyst `v` after the steps, row by row from the bottom.
pub fn grow(s: &ShapeGenSettings) -> (usize, usize, Arc<Vec<f32>>) {
    static LAST: Mutex<Option<(GridKey, Arc<Vec<f32>>)>> = Mutex::new(None);
    let (nx, ny) = grid_size(s);
    let (feed, kill) = s.rd_preset.rates(s);
    let key = GridKey {
        nx,
        ny,
        feed,
        kill,
        steps: s.rd_steps,
        seed: s.seed,
    };
    if let Ok(last) = LAST.lock() {
        if let Some((k, v)) = last.as_ref() {
            if *k == key {
                return (nx, ny, v.clone());
            }
        }
    }
    let v = Arc::new(simulate(key));
    if let Ok(mut last) = LAST.lock() {
        *last = Some((key, v.clone()));
    }
    (nx, ny, v)
}

fn simulate(k: GridKey) -> Vec<f32> {
    let (nx, ny) = (k.nx, k.ny);
    let n = nx * ny;
    let mut u = vec![1.0f32; n];
    let mut v = vec![0.0f32; n];
    // Seeded patches: small squares where the catalyst starts.
    let mut rng = Rng::new(k.seed, 0x9e5c);
    let patches = (n / 120).max(3);
    for _ in 0..patches {
        let cx = rng.below(nx as u32) as usize;
        let cy = rng.below(ny as u32) as usize;
        let r = 1 + rng.below(2) as usize;
        for dy in 0..=2 * r {
            for dx in 0..=2 * r {
                let x = (cx + dx + nx - r) % nx;
                let y = (cy + dy + ny - r) % ny;
                u[y * nx + x] = 0.5;
                v[y * nx + x] = 0.25 + 0.05 * rng.unit() as f32;
            }
        }
    }
    let (feed, kill) = (k.feed as f32, k.kill as f32);
    let (du, dv) = (1.0f32, 0.5f32);
    let mut u2 = u.clone();
    let mut v2 = v.clone();
    for _ in 0..k.steps {
        for y in 0..ny {
            let ym = if y == 0 { ny - 1 } else { y - 1 } * nx;
            let yp = if y + 1 == ny { 0 } else { y + 1 } * nx;
            let yc = y * nx;
            for x in 0..nx {
                let xm = if x == 0 { nx - 1 } else { x - 1 };
                let xp = if x + 1 == nx { 0 } else { x + 1 };
                let i = yc + x;
                let lap = |f: &[f32]| {
                    0.2 * (f[yc + xm] + f[yc + xp] + f[ym + x] + f[yp + x])
                        + 0.05 * (f[ym + xm] + f[ym + xp] + f[yp + xm] + f[yp + xp])
                        - f[i]
                };
                let (a, b) = (u[i], v[i]);
                let r = a * b * b;
                u2[i] = (a + du * lap(&u) - r + feed * (1.0 - a)).clamp(0.0, 1.0);
                let nv = (b + dv * lap(&v) + r - (kill + feed) * b).clamp(0.0, 1.0);
                // Tiny amounts are dropped: they change nothing and are
                // slow to compute with.
                v2[i] = if nv < 1e-7 { 0.0 } else { nv };
            }
        }
        std::mem::swap(&mut u, &mut u2);
        std::mem::swap(&mut v, &mut v2);
    }
    v
}

/// Closed rings round the places where `f` is above zero, by marching
/// squares over an `nx` x `ny` grid of samples with a ring of empty
/// samples round it; points in grid units, sample (0, 0) at (0.5, 0.5).
pub(crate) fn rings(nx: usize, ny: usize, f: impl Fn(usize, usize) -> f64) -> Vec<Vec<Point>> {
    // Padded: sample (x, y) of the padded grid is (x - 1, y - 1) inside.
    let (w, h) = (nx + 2, ny + 2);
    let val = |x: usize, y: usize| -> f64 {
        if x == 0 || y == 0 || x > nx || y > ny {
            -1.0
        } else {
            f(x - 1, y - 1)
        }
    };
    let field: Vec<f64> = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .map(|(x, y)| val(x, y))
        .collect();
    let at = |x: usize, y: usize| field[y * w + x];
    // A crossing is named by its grid edge: (x, y, 0) from (x, y) to
    // (x + 1, y), (x, y, 1) from (x, y) to (x, y + 1).
    type Edge = (usize, usize, u8);
    let mut next: std::collections::HashMap<Edge, Edge> = std::collections::HashMap::new();
    for y in 0..h - 1 {
        for x in 0..w - 1 {
            let (a, b, c, d) = (at(x, y), at(x + 1, y), at(x + 1, y + 1), at(x, y + 1));
            let idx = (a > 0.0) as u8
                | ((b > 0.0) as u8) << 1
                | ((c > 0.0) as u8) << 2
                | ((d > 0.0) as u8) << 3;
            let (bottom, right, top, left) = ((x, y, 0), (x + 1, y, 1), (x, y + 1, 0), (x, y, 1));
            let centre = (a + b + c + d) / 4.0 > 0.0;
            // Each segment runs with the filled side on its left, so the
            // rings come out joined head to tail.
            let segs: &[(Edge, Edge)] = match idx {
                1 => &[(bottom, left)],
                2 => &[(right, bottom)],
                3 => &[(right, left)],
                4 => &[(top, right)],
                5 => {
                    if centre {
                        &[(top, left), (bottom, right)]
                    } else {
                        &[(bottom, left), (top, right)]
                    }
                }
                6 => &[(top, bottom)],
                7 => &[(top, left)],
                8 => &[(left, top)],
                9 => &[(bottom, top)],
                10 => {
                    if centre {
                        &[(left, bottom), (right, top)]
                    } else {
                        &[(left, top), (right, bottom)]
                    }
                }
                11 => &[(right, top)],
                12 => &[(left, right)],
                13 => &[(bottom, right)],
                14 => &[(left, bottom)],
                _ => &[],
            };
            for (s, e) in segs {
                next.insert(*s, *e);
            }
        }
    }
    let point = |e: Edge| {
        let (x, y, dir) = e;
        let (x1, y1) = if dir == 0 { (x + 1, y) } else { (x, y + 1) };
        let (v0, v1) = (at(x, y), at(x1, y1));
        let t = if (v1 - v0).abs() < 1e-12 {
            0.5
        } else {
            (-v0 / (v1 - v0)).clamp(0.0, 1.0)
        };
        // Back to unpadded grid units, samples at cell middles.
        let px = x as f64 + (x1 as f64 - x as f64) * t - 0.5;
        let py = y as f64 + (y1 as f64 - y as f64) * t - 0.5;
        Point::new(px, py)
    };
    let mut out = Vec::new();
    let mut keys: Vec<Edge> = next.keys().copied().collect();
    keys.sort_unstable();
    let mut seen: std::collections::HashSet<Edge> = std::collections::HashSet::new();
    for start in keys {
        if seen.contains(&start) {
            continue;
        }
        let mut ring = Vec::new();
        let mut e = start;
        while seen.insert(e) {
            ring.push(point(e));
            match next.get(&e) {
                Some(n) => e = *n,
                None => break,
            }
        }
        if ring.len() >= 3 {
            out.push(ring);
        }
    }
    out
}

pub(crate) fn reaction_diffusion(s: &ShapeGenSettings, out: &mut Generated) {
    let (w, h) = (s.width, s.height);
    out.frame = Some(Rect::new(0.0, 0.0, w, h));
    let (nx, ny, v) = grow(s);
    let top = v.iter().copied().fold(0.0f32, f32::max) as f64;
    if top <= 1e-6 {
        return;
    }
    let level = top * s.rd_level / 100.0;
    let invert = s.rd_invert;
    let rings = rings(nx, ny, |x, y| {
        let d = v[y * nx + x] as f64 - level;
        if invert {
            -d
        } else {
            d
        }
    });
    let (sx, sy) = (w / nx as f64, h / ny as f64);
    let to_mm = |p: &Point| {
        Point::new(
            ((p.x + 0.5) * sx).clamp(0.0, w),
            ((p.y + 0.5) * sy).clamp(0.0, h),
        )
    };
    let mut all = BezPath::new();
    for ring in rings {
        let pts: Vec<Point> = ring.iter().map(to_mm).collect();
        let mut path = smooth_through(&pts, true);
        // Control points held in the box keep the curve in it.
        let clamp = |p: Point| Point::new(p.x.clamp(0.0, w), p.y.clamp(0.0, h));
        for el in path.elements_mut() {
            *el = match *el {
                PathEl::MoveTo(p) => PathEl::MoveTo(clamp(p)),
                PathEl::LineTo(p) => PathEl::LineTo(clamp(p)),
                PathEl::QuadTo(a, p) => PathEl::QuadTo(clamp(a), clamp(p)),
                PathEl::CurveTo(a, b, p) => PathEl::CurveTo(clamp(a), clamp(b), clamp(p)),
                PathEl::ClosePath => PathEl::ClosePath,
            };
        }
        if s.rd_filled {
            all.extend(path);
        } else {
            if out.full() {
                out.cut = true;
                return;
            }
            out.push(path, true, Paint::Line);
        }
    }
    if s.rd_filled && !all.elements().is_empty() {
        out.push(all, true, Paint::Solid);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape_gen::{generate, GenKind};

    fn small(preset: RdPreset) -> ShapeGenSettings {
        ShapeGenSettings {
            kind: GenKind::ReactionDiffusion,
            rd_preset: preset,
            rd_cells: 64,
            rd_steps: 1500,
            ..ShapeGenSettings::default()
        }
    }

    #[test]
    fn marching_squares_rings_a_square_and_its_hole() {
        // A 6 x 6 block with a 2 x 2 hole: two rings, joined head to tail.
        let rings = rings(10, 10, |x, y| {
            let block = (2..8).contains(&x) && (2..8).contains(&y);
            let hole = (4..6).contains(&x) && (4..6).contains(&y);
            if block && !hole {
                1.0
            } else {
                -1.0
            }
        });
        assert_eq!(rings.len(), 2);
        let area = |r: &Vec<Point>| {
            let mut a = 0.0;
            for i in 0..r.len() {
                let (p, q) = (r[i], r[(i + 1) % r.len()]);
                a += p.x * q.y - q.x * p.y;
            }
            a / 2.0
        };
        let mut areas: Vec<f64> = rings.iter().map(area).collect();
        areas.sort_by(f64::total_cmp);
        // The outline turns one way and the hole the other.
        assert!(areas[0] < 0.0 && areas[1] > 0.0, "{areas:?}");
        assert!((areas[1] - 36.0).abs() < 4.0, "{areas:?}");
    }

    #[test]
    fn every_pattern_grows_shapes_inside_the_box() {
        for preset in [RdPreset::Spots, RdPreset::Maze, RdPreset::Coral] {
            let s = small(preset);
            let g = generate(&s);
            assert_eq!(g.items.len(), 1, "{preset:?}");
            let n = g.items[0]
                .path
                .elements()
                .iter()
                .filter(|e| matches!(e, tracedraw_core::geometry::PathEl::MoveTo(_)))
                .count();
            assert!(n >= 3, "{preset:?}: {n} shapes");
            let b = g.bounds().expect("bounds");
            assert!(b.x0 >= -1e-6 && b.y0 >= -1e-6, "{b:?}");
            assert!(b.x1 <= s.width + 1e-6 && b.y1 <= s.height + 1e-6, "{b:?}");
        }
        // Outlines: one curve per ring; the same seed grows the same.
        let s = ShapeGenSettings {
            rd_filled: false,
            ..small(RdPreset::Maze)
        };
        let a = generate(&s);
        let b = generate(&s);
        assert!(a.items.len() > 1);
        assert_eq!(a.items.len(), b.items.len());
    }
}
