//! Effects > Vector Mosaic: a vector mosaic of
//! tiles (circles, squares or a custom curve) on a screen of rows turned by
//! the screen angle, sampled from the selected objects. The tiles can be
//! uniform (flattened on white), sized by opacity or by darkness, limited
//! to a number of colours, merged into bigger tiles where neighbours match,
//! and welded into one curve per colour. Glyph tiles pick a built-in symbol
//! by darkness (or at random) and Characters tiles a letter of a character
//! set, so the mosaic turns into a glyph mosaic or a text-art picture.

use tracedraw_core::document::{Shape, ShapeKind};
use tracedraw_core::geometry::{Affine, BezPath, Point, Rect, Shape as _};
use tracedraw_core::{Color, Command, Fill};

/// How tiles follow the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TrackMethod {
    /// Uniform (white matte): tiles of one size, transparency flattened
    /// against white.
    #[default]
    Uniform,
    /// Size modulation 1 (opacity): more opaque, bigger.
    Opacity,
    /// Size modulation 2 (luminosity): darker, bigger.
    Luminosity,
}

impl TrackMethod {
    pub const ALL: [TrackMethod; 3] = [
        TrackMethod::Uniform,
        TrackMethod::Opacity,
        TrackMethod::Luminosity,
    ];

    pub fn key(self) -> &'static str {
        match self {
            TrackMethod::Uniform => "docker.vector_mosaic_uniform",
            TrackMethod::Opacity => "docker.vector_mosaic_opacity",
            TrackMethod::Luminosity => "docker.vector_mosaic_luminosity",
        }
    }
}

/// The tile shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TileShape {
    #[default]
    Circle,
    Square,
    /// A closed curve picked from the drawing.
    Custom,
    /// Built-in symbols, the light tiles taking those with the least ink.
    Glyphs,
    /// Letters of a character set, the light tiles taking the first ones.
    Characters,
}

impl TileShape {
    pub const ALL: [TileShape; 5] = [
        TileShape::Circle,
        TileShape::Square,
        TileShape::Custom,
        TileShape::Glyphs,
        TileShape::Characters,
    ];

    pub fn key(self) -> &'static str {
        match self {
            TileShape::Circle => "docker.vector_mosaic_circle",
            TileShape::Square => "docker.vector_mosaic_square",
            TileShape::Custom => "docker.vector_mosaic_custom",
            TileShape::Glyphs => "docker.vector_mosaic_glyphs",
            TileShape::Characters => "docker.vector_mosaic_characters",
        }
    }

    /// Glyphs and Characters pick each tile's outline by its tone.
    pub fn picks_by_tone(self) -> bool {
        matches!(self, TileShape::Glyphs | TileShape::Characters)
    }
}

/// The character set Characters starts with, light to dark.
pub const DEFAULT_CHARS: &str = " .:-=+*#%@";

/// The font Characters draws with (a fixed-width face or its stand-in).
pub const CHAR_FONT: &str = "Courier New";

/// The Vector Mosaic docker's settings.
#[derive(Debug, Clone, PartialEq)]
pub struct VectorMosaicSettings {
    /// Tiles per inch along the rows and columns, 1 to 100.
    pub density: f64,
    /// Tile size factor, 0.1 to 5.
    pub scale: f64,
    /// Screen angle, degrees counter-clockwise, -90 to 90.
    pub angle: f64,
    pub keep_original: bool,
    pub limit_colors: bool,
    /// Most colours when limited, 2 to 256.
    pub colors: u32,
    pub method: TrackMethod,
    /// Merge adjacent: the most tiles of one colour combined into one.
    pub merge: u32,
    /// Weld adjacent overlap: one curve per colour.
    pub weld: bool,
    pub shape: TileShape,
    /// The custom tile, fitted to the unit square around the origin.
    pub custom: Option<BezPath>,
    /// Glyphs: pick each tile's symbol at random instead of by tone.
    pub random_glyphs: bool,
    /// The seed of the random symbols.
    pub seed: u32,
    /// Characters: the set, light to dark (a space leaves a cell empty).
    pub chars: String,
    /// Glyphs and Characters: dark tiles take the light end instead (for a
    /// light mosaic on a dark ground).
    pub invert_tone: bool,
    /// Every tile in one colour instead of the picture's.
    pub mono: bool,
    pub mono_color: [u8; 3],
    /// Leave out the tiles of the background's colour (the empty cells
    /// of a logo on a solid ground).
    pub skip_background: bool,
    /// The background's colour; `None` finds it along the source's edges.
    pub background: Option<[u8; 3]>,
    /// How far from the background a tile's colour may be and still be
    /// left out, 0 to 100 %.
    pub background_tolerance: f64,
}

impl Default for VectorMosaicSettings {
    fn default() -> Self {
        VectorMosaicSettings {
            density: 10.0,
            scale: 1.0,
            angle: 30.0,
            keep_original: false,
            limit_colors: true,
            colors: 16,
            method: TrackMethod::Opacity,
            merge: 1,
            weld: true,
            shape: TileShape::Circle,
            custom: None,
            random_glyphs: false,
            seed: 1,
            chars: DEFAULT_CHARS.to_string(),
            invert_tone: false,
            mono: false,
            mono_color: [0, 0, 0],
            skip_background: false,
            background: None,
            background_tolerance: 15.0,
        }
    }
}

/// One tile: its centre (mm, page space), size (mm) and colour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tile {
    pub centre: Point,
    pub size: f64,
    pub color: [u8; 3],
    /// Row and column on the screen.
    pub cell: (i64, i64),
    /// Darkness of the flattened colour, 0 (white) to 1 (black).
    pub tone: f64,
}

/// The most tiles one mosaic makes.
pub const MAX_TILES: usize = 60_000;

/// A shape's outline fitted to the unit square centred on the origin (a
/// custom tile).
pub fn unit_tile(path: &BezPath) -> Option<BezPath> {
    let b = path.bounding_box();
    let side = b.width().max(b.height());
    if side.is_nan() || side <= 1e-9 || path.elements().is_empty() {
        return None;
    }
    let c = b.center();
    Some(Affine::scale(1.0 / side) * Affine::translate((-c.x, -c.y)) * path.clone())
}

/// The tiles for an image of the selection: `img` covers `bounds`
/// (page mm, y up) with `px_per_mm` pixels per millimetre.
pub fn tiles(
    img: &image::RgbaImage,
    bounds: Rect,
    px_per_mm: f64,
    s: &VectorMosaicSettings,
) -> Result<Vec<Tile>, usize> {
    let density = if s.density.is_finite() {
        s.density.clamp(1.0, 100.0)
    } else {
        10.0
    };
    let pitch = 25.4 / density;
    let scale = if s.scale.is_finite() {
        s.scale.clamp(0.1, 5.0)
    } else {
        1.0
    };
    let a = (if s.angle.is_finite() {
        s.angle.clamp(-90.0, 90.0)
    } else {
        0.0
    })
    .to_radians();
    let (sin, cos) = a.sin_cos();
    let centre = bounds.center();
    let half = (bounds.width().hypot(bounds.height()) / 2.0 / pitch).ceil() as i64 + 1;
    let estimate = ((bounds.width() / pitch + 2.0) * (bounds.height() / pitch + 2.0)) as usize;
    if estimate > MAX_TILES {
        return Err(estimate);
    }
    let (w, h) = (img.width() as f64, img.height() as f64);
    // Average a 3 x 3 grid of samples over the tile's cell.
    let sample = |p: Point| -> ([f64; 3], f64) {
        let mut acc = [0f64; 4];
        for dj in [-1.0, 0.0, 1.0] {
            for di in [-1.0, 0.0, 1.0] {
                let q = Point::new(p.x + di * pitch / 3.0, p.y + dj * pitch / 3.0);
                let x = ((q.x - bounds.x0) * px_per_mm).floor();
                let y = ((bounds.y1 - q.y) * px_per_mm).floor();
                if x < 0.0 || y < 0.0 || x >= w || y >= h {
                    continue;
                }
                let px = img.get_pixel(x as u32, y as u32);
                let al = px[3] as f64 / 255.0;
                for c in 0..3 {
                    acc[c] += px[c] as f64 * al;
                }
                acc[3] += al;
            }
        }
        let alpha = acc[3] / 9.0;
        if acc[3] <= 1e-9 {
            return ([255.0; 3], 0.0);
        }
        ([acc[0] / acc[3], acc[1] / acc[3], acc[2] / acc[3]], alpha)
    };
    // Remove background: the picked colour, or the commonest along the
    // source's edges, and the furthest a tile may be from it to go.
    let background = s
        .skip_background
        .then(|| s.background.or_else(|| edge_colour(img)))
        .flatten()
        .map(|c| c.map(f64::from));
    let reach = if s.background_tolerance.is_finite() {
        s.background_tolerance.clamp(0.0, 100.0)
    } else {
        15.0
    } / 100.0
        * (3.0f64 * 255.0 * 255.0).sqrt();
    let mut out = Vec::new();
    for j in -half..=half {
        for i in -half..=half {
            let (u, v) = (i as f64 * pitch, j as f64 * pitch);
            let p = Point::new(centre.x + u * cos - v * sin, centre.y + u * sin + v * cos);
            if p.x < bounds.x0 || p.x > bounds.x1 || p.y < bounds.y0 || p.y > bounds.y1 {
                continue;
            }
            let (c, alpha) = sample(p);
            // Flattened against white.
            let flat: [f64; 3] = std::array::from_fn(|k| c[k] * alpha + 255.0 * (1.0 - alpha));
            let factor = match s.method {
                TrackMethod::Uniform => 1.0,
                TrackMethod::Opacity => alpha.sqrt(),
                TrackMethod::Luminosity => {
                    let l = 0.299 * flat[0] + 0.587 * flat[1] + 0.114 * flat[2];
                    (1.0 - l / 255.0).max(0.0).sqrt()
                }
            };
            if factor < 0.05 {
                continue;
            }
            let color = match s.method {
                TrackMethod::Opacity => c,
                _ => flat,
            };
            if let Some(bg) = background {
                let d = (0..3).map(|k| (flat[k] - bg[k]).powi(2)).sum::<f64>().sqrt();
                if d <= reach {
                    continue;
                }
            }
            let l = 0.299 * flat[0] + 0.587 * flat[1] + 0.114 * flat[2];
            out.push(Tile {
                centre: p,
                size: pitch * scale * factor,
                color: color.map(|v| v.round().clamp(0.0, 255.0) as u8),
                cell: (i, j),
                tone: (1.0 - l / 255.0).clamp(0.0, 1.0),
            });
            if out.len() > MAX_TILES {
                return Err(out.len());
            }
        }
    }
    if s.mono {
        for t in &mut out {
            t.color = s.mono_color;
        }
    } else if s.limit_colors {
        limit_colors(&mut out, s.colors.clamp(2, 256));
    }
    // Picked tiles keep their own symbol, so they are never merged.
    if s.merge >= 4 && !s.shape.picks_by_tone() {
        out = merge(out, s.merge);
    }
    Ok(out)
}

/// The commonest colour along the image's edges (flattened against
/// white): the ground of a logo, for Remove background. Colours are
/// counted in steps of 16 per channel and the winning step's pixels
/// averaged. `None` for an empty image.
pub fn edge_colour(img: &image::RgbaImage) -> Option<[u8; 3]> {
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return None;
    }
    let mut bins: std::collections::HashMap<[u8; 3], (u32, [u64; 3])> = Default::default();
    let mut add = |x: u32, y: u32| {
        let p = img.get_pixel(x, y);
        let a = p[3] as f64 / 255.0;
        let flat: [u8; 3] =
            std::array::from_fn(|k| (p[k] as f64 * a + 255.0 * (1.0 - a)).round() as u8);
        let e = bins.entry(flat.map(|v| v >> 4)).or_default();
        e.0 += 1;
        for k in 0..3 {
            e.1[k] += flat[k] as u64;
        }
    };
    for x in 0..w {
        add(x, 0);
        add(x, h - 1);
    }
    for y in 1..h.saturating_sub(1) {
        add(0, y);
        add(w - 1, y);
    }
    // The commonest step; ties go to the smaller key so the pick is the
    // same on every run.
    let (_, (n, sum)) = bins
        .into_iter()
        .max_by(|a, b| a.1 .0.cmp(&b.1 .0).then(b.0.cmp(&a.0)))?;
    Some(sum.map(|v| (v / n as u64) as u8))
}

/// Reduce the tiles' colours to at most `n` with a median-cut palette.
fn limit_colors(tiles: &mut [Tile], n: u32) {
    if tiles.is_empty() {
        return;
    }
    let img = image::RgbaImage::from_fn(tiles.len() as u32, 1, |x, _| {
        let c = tiles[x as usize].color;
        image::Rgba([c[0], c[1], c[2], 255])
    });
    let settings = crate::bitmap_modes::PalettedSettings {
        palette: crate::bitmap_modes::PaletteType::Optimized,
        colors: n,
        ..Default::default()
    };
    let palette = crate::bitmap_modes::build_palette(&img, &settings);
    if palette.is_empty() {
        return;
    }
    for t in tiles.iter_mut() {
        let d = |p: &[u8; 3]| -> i32 {
            (0..3)
                .map(|k| (p[k] as i32 - t.color[k] as i32).pow(2))
                .sum()
        };
        if let Some(best) = palette.iter().min_by_key(|p| d(p)) {
            t.color = *best;
        }
    }
}

/// Merge adjacent: blocks of k x k tiles of one colour and size (k up to
/// the square root of `most`) become one tile k times as big.
fn merge(tiles: Vec<Tile>, most: u32) -> Vec<Tile> {
    use std::collections::HashMap;
    let k_max = ((most as f64).sqrt().floor() as i64).clamp(1, 6);
    let index: HashMap<(i64, i64), usize> =
        tiles.iter().enumerate().map(|(i, t)| (t.cell, i)).collect();
    let mut used = vec![false; tiles.len()];
    let mut out = Vec::new();
    for k in (2..=k_max).rev() {
        for i in 0..tiles.len() {
            if used[i] {
                continue;
            }
            let (ci, cj) = tiles[i].cell;
            let mut block = Vec::new();
            'cells: for dj in 0..k {
                for di in 0..k {
                    match index.get(&(ci + di, cj + dj)) {
                        Some(&t)
                            if !used[t]
                                && tiles[t].color == tiles[i].color
                                && (tiles[t].size - tiles[i].size).abs() < 1e-9 =>
                        {
                            block.push(t)
                        }
                        _ => break 'cells,
                    }
                }
            }
            if block.len() == (k * k) as usize {
                let n = block.len() as f64;
                let (sx, sy) = block.iter().fold((0.0, 0.0), |acc, &t| {
                    (acc.0 + tiles[t].centre.x, acc.1 + tiles[t].centre.y)
                });
                for &t in &block {
                    used[t] = true;
                }
                out.push(Tile {
                    centre: Point::new(sx / n, sy / n),
                    size: tiles[i].size * k as f64,
                    color: tiles[i].color,
                    cell: tiles[i].cell,
                    tone: tiles[i].tone,
                });
            }
        }
    }
    out.extend(
        tiles
            .iter()
            .enumerate()
            .filter(|(i, _)| !used[*i])
            .map(|(_, t)| *t),
    );
    out
}

/// The built-in symbols of Glyphs, each fitted to the unit square centred
/// on the origin, ordered from the least ink to the most.
pub fn glyph_library() -> &'static [BezPath] {
    static LIB: std::sync::OnceLock<Vec<BezPath>> = std::sync::OnceLock::new();
    LIB.get_or_init(build_library)
}

fn polygon(points: &[(f64, f64)]) -> BezPath {
    let mut p = BezPath::new();
    for (i, (x, y)) in points.iter().enumerate() {
        if i == 0 {
            p.move_to((*x, *y));
        } else {
            p.line_to((*x, *y));
        }
    }
    p.close_path();
    p
}

/// A star of `n` points between radii `outer` and `inner`, the first
/// point straight up.
fn star(n: usize, outer: f64, inner: f64) -> BezPath {
    let pts: Vec<(f64, f64)> = (0..n * 2)
        .map(|k| {
            let r = if k % 2 == 0 { outer } else { inner };
            let a = std::f64::consts::FRAC_PI_2 + k as f64 * std::f64::consts::PI / n as f64;
            (r * a.cos(), r * a.sin())
        })
        .collect();
    polygon(&pts)
}

fn weld_all(parts: &[BezPath]) -> BezPath {
    use tracedraw_core::shaping::{overlay, Op};
    let mut it = parts.iter();
    let Some(first) = it.next() else {
        return BezPath::new();
    };
    it.fold(first.clone(), |acc, p| overlay(&acc, p, Op::Weld))
}

fn circle(c: (f64, f64), r: f64) -> BezPath {
    tracedraw_core::geometry::Circle::new(c, r).to_path(0.001)
}

/// The ink a unit symbol covers, counting holes as empty.
fn ink(p: &BezPath) -> f64 {
    tracedraw_core::geometry::Shape::area(p).abs()
}

fn build_library() -> Vec<BezPath> {
    let bar = |w: f64, h: f64, deg: f64| {
        Affine::rotate(deg.to_radians())
            * Rect::new(-w / 2.0, -h / 2.0, w / 2.0, h / 2.0).to_path(0.001)
    };
    // A ring: the inner circle runs the other way, so it stays a hole under
    // either fill rule.
    let mut ring = circle((0.0, 0.0), 0.45);
    ring.extend(circle((0.0, 0.0), 0.3).reverse_subpaths());
    let plus = weld_all(&[bar(0.9, 0.24, 0.0), bar(0.9, 0.24, 90.0)]);
    let cross = Affine::rotate(std::f64::consts::FRAC_PI_4) * plus.clone();
    let asterisk = weld_all(&[
        bar(0.9, 0.16, 90.0),
        bar(0.9, 0.16, 30.0),
        bar(0.9, 0.16, -30.0),
    ]);
    let flower = weld_all(&[
        circle((0.0, 0.22), 0.22),
        circle((0.22, 0.0), 0.22),
        circle((0.0, -0.22), 0.22),
        circle((-0.22, 0.0), 0.22),
        circle((0.0, 0.0), 0.15),
    ]);
    let arrow = polygon(&[
        (-0.45, 0.1),
        (0.05, 0.1),
        (0.05, 0.35),
        (0.45, 0.0),
        (0.05, -0.35),
        (0.05, -0.1),
        (-0.45, -0.1),
    ]);
    let bolt = polygon(&[
        (0.1, 0.5),
        (-0.3, -0.05),
        (-0.02, -0.05),
        (-0.12, -0.5),
        (0.3, 0.08),
        (0.02, 0.08),
    ]);
    let triangle = polygon(&[(0.0, 0.45), (0.45, -0.38), (-0.45, -0.38)]);
    let diamond = polygon(&[(0.0, 0.5), (0.5, 0.0), (0.0, -0.5), (-0.5, 0.0)]);
    let mut all = vec![
        circle((0.0, 0.0), 0.12),
        ring,
        cross,
        plus,
        asterisk,
        arrow,
        bolt,
        star(5, 0.5, 0.2),
        triangle,
        flower,
        diamond,
        circle((0.0, 0.0), 0.45),
        Rect::new(-0.45, -0.45, 0.45, 0.45).to_path(0.001),
    ];
    all.sort_by(|a, b| ink(a).total_cmp(&ink(b)));
    all
}

/// The letters of a character set as unit tiles: a fixed-width letter
/// about as wide as the unit square (an em of 1.4), in its middle. A space
/// (or a letter the font draws nothing for) gives an empty path.
pub fn char_tiles(chars: &str) -> Vec<BezPath> {
    use tracedraw_core::ShapeId;
    let em_pt = 72.0;
    let em = em_pt * 25.4 / 72.0;
    chars
        .chars()
        .filter(|c| !c.is_control())
        .take(256)
        .map(|c| {
            if c.is_whitespace() {
                return BezPath::new();
            }
            let sh = Shape::new(
                ShapeId(0),
                ShapeKind::Text {
                    spans: vec![tracedraw_core::TextSpan::new(
                        &c.to_string(),
                        CHAR_FONT,
                        em_pt,
                    )],
                    origin: Point::ZERO,
                    frame: None,
                    align: Default::default(),
                    para: Default::default(),
                    on_path: None,
                },
            );
            let path = sh.local_path();
            let b = path.bounding_box();
            if path.elements().is_empty() || b.area() <= 1e-9 || b.area().is_nan() {
                return BezPath::new();
            }
            // A fixed-width cell is 0.6 em wide; the line's middle sits at
            // about 0.35 em above the baseline.
            Affine::scale(1.4 / em) * Affine::translate((-0.3 * em, -0.35 * em)) * path
        })
        .collect()
}

/// The outlines a tone picks from for these settings.
pub fn tone_set(s: &VectorMosaicSettings) -> Vec<BezPath> {
    match s.shape {
        TileShape::Glyphs => glyph_library().to_vec(),
        TileShape::Characters => {
            let chars = if s.chars.chars().any(|c| !c.is_control()) {
                s.chars.as_str()
            } else {
                DEFAULT_CHARS
            };
            char_tiles(chars)
        }
        _ => Vec::new(),
    }
}

/// The outline a tile takes from the set: by its tone (light first, or
/// dark first when inverted) or, for random glyphs, from the seed.
fn pick<'a>(t: &Tile, s: &VectorMosaicSettings, set: &'a [BezPath]) -> Option<&'a BezPath> {
    if set.is_empty() {
        return None;
    }
    let n = set.len();
    let k = if s.random_glyphs && s.shape == TileShape::Glyphs {
        let mut h = (t.cell.0 as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ (t.cell.1 as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
            ^ (s.seed as u64).wrapping_mul(0x1656_67B1_9E37_79F9);
        h ^= h >> 31;
        h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        h ^= h >> 29;
        (h % n as u64) as usize
    } else {
        let tone = if s.invert_tone { 1.0 - t.tone } else { t.tone };
        ((tone * n as f64).floor() as usize).min(n - 1)
    };
    let p = &set[k];
    (!p.elements().is_empty()).then_some(p)
}

/// A tile's outline in page space (empty for a blank character).
fn tile_path(t: &Tile, s: &VectorMosaicSettings, set: &[BezPath]) -> BezPath {
    let half = t.size / 2.0;
    if s.shape.picks_by_tone() {
        let Some(unit) = pick(t, s, set) else {
            return BezPath::new();
        };
        return Affine::translate(t.centre.to_vec2())
            * Affine::rotate(s.angle.to_radians())
            * Affine::scale(t.size)
            * unit.clone();
    }
    match (s.shape, &s.custom) {
        (TileShape::Square, _) => {
            let r = Rect::new(
                t.centre.x - half,
                t.centre.y - half,
                t.centre.x + half,
                t.centre.y + half,
            );
            Affine::rotate_about(s.angle.to_radians(), t.centre) * r.to_path(0.01)
        }
        (TileShape::Custom, Some(unit)) => {
            Affine::translate(t.centre.to_vec2())
                * Affine::rotate(s.angle.to_radians())
                * Affine::scale(t.size)
                * unit.clone()
        }
        _ => tracedraw_core::geometry::Circle::new(t.centre, half).to_path(0.01),
    }
}

/// The mosaic's objects: one per tile, or one curve per colour when
/// welding.
pub fn mosaic_shapes(
    tiles: &[Tile],
    s: &VectorMosaicSettings,
    mut next_id: impl FnMut() -> tracedraw_core::ShapeId,
) -> Vec<Shape> {
    let make = |id, kind: ShapeKind, color: [u8; 3]| {
        let mut sh = Shape::new(id, kind);
        sh.fill = Fill::Solid(Color::rgb8(color[0], color[1], color[2]));
        sh.stroke = None;
        sh
    };
    let set = tone_set(s);
    if s.weld {
        let mut by_color: Vec<([u8; 3], BezPath)> = Vec::new();
        for t in tiles {
            let p = tile_path(t, s, &set);
            if p.elements().is_empty() {
                continue;
            }
            match by_color.iter_mut().find(|(c, _)| *c == t.color) {
                Some((_, path)) => path.extend(p),
                None => by_color.push((t.color, p)),
            }
        }
        return by_color
            .into_iter()
            .map(|(c, path)| make(next_id(), ShapeKind::Path { path, closed: true }, c))
            .collect();
    }
    tiles
        .iter()
        .filter_map(|t| {
            let half = t.size / 2.0;
            let r = Rect::new(
                t.centre.x - half,
                t.centre.y - half,
                t.centre.x + half,
                t.centre.y + half,
            );
            match s.shape {
                TileShape::Circle => Some(make(
                    next_id(),
                    ShapeKind::Ellipse { rect: r, arc: None },
                    t.color,
                )),
                _ => {
                    let path = tile_path(t, s, &set);
                    (!path.elements().is_empty())
                        .then(|| make(next_id(), ShapeKind::Path { path, closed: true }, t.color))
                }
            }
        })
        .collect()
}

impl crate::app::App {
    /// Apply the Vector Mosaic to the selection: the mosaic, grouped, on
    /// top of it (the selection removed unless Keep original), in one
    /// undo step. False with a status message when nothing came out.
    pub fn apply_vector_mosaic(&mut self, s: &VectorMosaicSettings) -> bool {
        let Some(bounds) = self.selection_bounds() else {
            return false;
        };
        let Some(layer) = self.active_layer() else {
            return false;
        };
        let dpi = (s.density.clamp(1.0, 100.0) * 6.0).clamp(36.0, 600.0);
        let Some((png, _, _, rendered)) = self.render_selection_png_with(dpi, true, true) else {
            return false;
        };
        let Some(img) = crate::bitmap_fx::decode(&png) else {
            return false;
        };
        let px_per_mm = img.width() as f64 / rendered.width().max(1e-9);
        let tiles = match tiles(&img, bounds, px_per_mm, s) {
            Ok(t) => t,
            Err(n) => {
                self.status =
                    crate::i18n::trf("status.vector_mosaic_too_many", &[("n", &n.to_string())]);
                return false;
            }
        };
        if tiles.is_empty() {
            self.status = crate::i18n::tr("status.vector_mosaic_empty");
            return false;
        }
        let engine = &mut self.engine;
        let children = mosaic_shapes(&tiles, s, || engine.new_shape_id());
        if children.is_empty() {
            self.status = crate::i18n::tr("status.vector_mosaic_empty");
            return false;
        }
        let group_id = self.engine.new_shape_id();
        let group = Shape::new(group_id, ShapeKind::Group { children });
        let mut cmds = Vec::new();
        if !s.keep_original {
            cmds.push(Command::DeleteShapes {
                shapes: self.selection.clone(),
            });
        }
        cmds.push(Command::AddShape {
            layer,
            shape: group,
        });
        if let Err(e) = self.engine.run_batch("Vector Mosaic", &cmds) {
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
    use image::{ImageBuffer, Rgba};

    /// A 40 x 20 mm source at 4 px/mm: red left half, transparent right.
    fn source() -> (image::RgbaImage, Rect) {
        let img = ImageBuffer::from_fn(160, 80, |x, _| {
            if x < 80 {
                Rgba([220, 20, 20, 255])
            } else {
                Rgba([0, 0, 0, 0])
            }
        });
        (img, Rect::new(0.0, 0.0, 40.0, 20.0))
    }

    /// Uniform tiles of every colour, unturned and apart: the plain
    /// settings the tests vary.
    fn uniform() -> VectorMosaicSettings {
        VectorMosaicSettings {
            angle: 0.0,
            keep_original: true,
            limit_colors: false,
            colors: 8,
            method: TrackMethod::Uniform,
            weld: false,
            ..VectorMosaicSettings::default()
        }
    }

    /// A 40 x 20 mm red ground at 4 px/mm with a black square in the
    /// middle (10 to 30 mm across, 5 to 15 mm up).
    fn logo_on_red() -> (image::RgbaImage, Rect) {
        let img = ImageBuffer::from_fn(160, 80, |x, y| {
            if (40..120).contains(&x) && (20..60).contains(&y) {
                Rgba([10, 10, 10, 255])
            } else {
                Rgba([210, 25, 30, 255])
            }
        });
        (img, Rect::new(0.0, 0.0, 40.0, 20.0))
    }

    #[test]
    fn remove_background_finds_the_ground_and_leaves_only_the_logo() {
        let (img, b) = logo_on_red();
        let ground = edge_colour(&img).expect("a ground");
        assert_eq!(ground, [210, 25, 30]);
        let all = tiles(&img, b, 4.0, &uniform()).expect("tiles");
        let s = VectorMosaicSettings {
            skip_background: true,
            ..uniform()
        };
        let kept = tiles(&img, b, 4.0, &s).expect("tiles");
        assert!(!kept.is_empty() && kept.len() < all.len());
        // Tiles over the square's edge mix both colours and may stay; plain
        // ground tiles go.
        for t in &kept {
            assert!(
                t.color[0] < 180,
                "a red ground tile was kept: {:?} at {:?}",
                t.color,
                t.centre
            );
        }
        // Every dark tile is still there.
        let dark = all.iter().filter(|t| t.color[0] < 30).count();
        assert!(kept.iter().filter(|t| t.color[0] < 30).count() == dark);
    }

    #[test]
    fn remove_background_takes_a_picked_colour_and_a_tolerance() {
        let (img, b) = logo_on_red();
        // Picking the logo's black leaves the red ground instead.
        let s = VectorMosaicSettings {
            skip_background: true,
            background: Some([0, 0, 0]),
            ..uniform()
        };
        let kept = tiles(&img, b, 4.0, &s).expect("tiles");
        assert!(kept.iter().all(|t| t.color[0] > 120));
        // No tolerance and a colour no tile has: nothing goes.
        let s = VectorMosaicSettings {
            skip_background: true,
            background: Some([0, 200, 0]),
            background_tolerance: 0.0,
            ..uniform()
        };
        let all = tiles(&img, b, 4.0, &uniform()).expect("tiles");
        assert_eq!(tiles(&img, b, 4.0, &s).expect("tiles").len(), all.len());
    }

    #[test]
    fn the_defaults_match_the_panel() {
        let s = VectorMosaicSettings::default();
        assert_eq!((s.density, s.scale, s.angle), (10.0, 1.0, 30.0));
        assert!(!s.keep_original);
        assert!(s.limit_colors && s.colors == 16);
        assert_eq!(s.method, TrackMethod::Opacity);
        assert_eq!(s.merge, 1);
        assert!(s.weld);
        assert_eq!(s.shape, TileShape::Circle);
        assert!(!s.skip_background && s.background.is_none());
        assert_eq!(s.background_tolerance, 15.0);
    }

    #[test]
    fn uniform_tiles_cover_the_bounds_flattened_on_white() {
        let (img, b) = source();
        let s = uniform();
        let t = tiles(&img, b, 4.0, &s).expect("tiles");
        // 2.54 mm pitch over 40 x 20 mm: about 16 x 8 tiles.
        assert!((100..=180).contains(&t.len()), "{}", t.len());
        assert!(t.iter().all(|t| (t.size - 2.54).abs() < 1e-9));
        assert!(t.iter().any(|t| t.color == [255, 255, 255]));
        assert!(t.iter().any(|t| t.color[0] > 200 && t.color[1] < 40));
    }

    #[test]
    fn opacity_and_luminosity_size_the_tiles() {
        let (img, b) = source();
        let s = VectorMosaicSettings {
            method: TrackMethod::Opacity,
            ..uniform()
        };
        let t = tiles(&img, b, 4.0, &s).expect("tiles");
        // Transparent cells give no tile.
        assert!(
            t.iter().all(|t| t.centre.x < 21.5),
            "{:?}",
            t.iter().map(|t| t.centre.x).fold(0.0, f64::max)
        );
        let gray = ImageBuffer::from_fn(80, 80, |x, _| {
            let v = (x * 3) as u8;
            Rgba([v, v, v, 255])
        });
        let s = VectorMosaicSettings {
            method: TrackMethod::Luminosity,
            ..uniform()
        };
        let t = tiles(&gray, Rect::new(0.0, 0.0, 20.0, 20.0), 4.0, &s).expect("tiles");
        let left = t
            .iter()
            .filter(|t| t.centre.x < 5.0)
            .map(|t| t.size)
            .fold(0.0, f64::max);
        let right = t
            .iter()
            .filter(|t| t.centre.x > 15.0)
            .map(|t| t.size)
            .fold(0.0, f64::max);
        assert!(left > right, "{left} {right}");
    }

    #[test]
    fn limits_merges_screens_and_welds() {
        let (img, b) = source();
        let limited = tiles(
            &img,
            b,
            4.0,
            &VectorMosaicSettings {
                limit_colors: true,
                colors: 2,
                ..uniform()
            },
        )
        .expect("tiles");
        let mut colors: Vec<[u8; 3]> = limited.iter().map(|t| t.color).collect();
        colors.sort();
        colors.dedup();
        assert!(colors.len() <= 2);
        let plain = tiles(&img, b, 4.0, &uniform()).expect("tiles");
        let merged = tiles(
            &img,
            b,
            4.0,
            &VectorMosaicSettings {
                merge: 4,
                ..uniform()
            },
        )
        .expect("tiles");
        assert!(merged.len() < plain.len());
        assert!(merged.iter().any(|t| (t.size - 5.08).abs() < 1e-9));
        // A turned screen still stays inside the bounds.
        let turned = tiles(
            &img,
            b,
            4.0,
            &VectorMosaicSettings {
                angle: 30.0,
                ..uniform()
            },
        )
        .expect("tiles");
        assert!(turned.iter().all(|t| b.contains(t.centre)));
        // Welding makes one curve per colour.
        let mut n = 0;
        let shapes = mosaic_shapes(
            &plain,
            &VectorMosaicSettings {
                weld: true,
                ..uniform()
            },
            || {
                n += 1;
                tracedraw_core::ShapeId(n)
            },
        );
        let distinct: std::collections::HashSet<[u8; 3]> = plain.iter().map(|t| t.color).collect();
        assert_eq!(shapes.len(), distinct.len());
        // Too dense for the area: refused, not a freeze.
        assert!(tiles(
            &img,
            Rect::new(0.0, 0.0, 2000.0, 2000.0),
            0.1,
            &VectorMosaicSettings {
                density: 100.0,
                ..uniform()
            }
        )
        .is_err());
        assert!(unit_tile(&BezPath::new()).is_none());
    }

    #[test]
    fn the_glyph_library_runs_from_light_to_dark() {
        let lib = glyph_library();
        assert!(lib.len() >= 12);
        for w in lib.windows(2) {
            assert!(ink(&w[0]) <= ink(&w[1]) + 1e-9);
        }
        // Every symbol fits the unit square around the origin.
        for g in lib {
            let b = g.bounding_box();
            assert!(
                b.x0 >= -0.51 && b.x1 <= 0.51 && b.y0 >= -0.51 && b.y1 <= 0.51,
                "{b:?}"
            );
        }
        // The ring keeps its hole.
        assert!(lib.iter().any(|g| {
            let a = ink(g);
            (a - std::f64::consts::PI * (0.45f64.powi(2) - 0.3f64.powi(2))).abs() < 0.01
        }));
    }

    /// A grey ramp, light at the left, dark at the right.
    fn ramp() -> (image::RgbaImage, Rect) {
        let img = ImageBuffer::from_fn(160, 40, |x, _| {
            let v = 255 - (x * 255 / 159) as u8;
            Rgba([v, v, v, 255])
        });
        (img, Rect::new(0.0, 0.0, 40.0, 10.0))
    }

    #[test]
    fn glyph_tiles_pick_more_ink_where_it_is_darker() {
        let (img, b) = ramp();
        let s = VectorMosaicSettings {
            shape: TileShape::Glyphs,
            ..uniform()
        };
        let t = tiles(&img, b, 4.0, &s).expect("tiles");
        let set = tone_set(&s);
        let ink_at = |pred: &dyn Fn(f64) -> bool| -> f64 {
            let picked: Vec<f64> = t
                .iter()
                .filter(|t| pred(t.centre.x))
                .filter_map(|t| pick(t, &s, &set))
                .map(ink)
                .collect();
            picked.iter().sum::<f64>() / picked.len().max(1) as f64
        };
        let light = ink_at(&|x| x < 8.0);
        let dark = ink_at(&|x| x > 32.0);
        assert!(dark > light * 2.0, "{light} {dark}");
        // Inverted, the light side takes the heavy symbols.
        let inv = VectorMosaicSettings {
            invert_tone: true,
            ..s.clone()
        };
        let light_inv: f64 = t
            .iter()
            .filter(|t| t.centre.x < 8.0)
            .filter_map(|t| pick(t, &inv, &set))
            .map(ink)
            .sum();
        assert!(light_inv > 0.0);
        // Random symbols follow the seed.
        let rnd = |seed| {
            let s = VectorMosaicSettings {
                random_glyphs: true,
                seed,
                ..s.clone()
            };
            t.iter()
                .map(|t| pick(t, &s, &set).map(ink).unwrap_or(0.0))
                .collect::<Vec<f64>>()
        };
        assert_eq!(rnd(3), rnd(3));
        assert_ne!(rnd(3), rnd(4));
        // Glyph tiles are never merged, and one colour paints them all.
        let mono = VectorMosaicSettings {
            merge: 9,
            mono: true,
            mono_color: [10, 20, 30],
            limit_colors: true,
            ..s.clone()
        };
        let m = tiles(&img, b, 4.0, &mono).expect("tiles");
        assert_eq!(m.len(), t.len());
        assert!(m.iter().all(|t| t.color == [10, 20, 30]));
    }

    #[test]
    fn character_tiles_draw_letters_and_leave_blanks_empty() {
        tracedraw_text::install();
        let set = char_tiles(" .@");
        assert_eq!(set.len(), 3);
        assert!(set[0].elements().is_empty());
        assert!(!set[2].elements().is_empty());
        // The dot is small and low, the at sign fills much more of its cell.
        assert!(
            ink(&set[1]) < ink(&set[2]) / 3.0,
            "{} {}",
            ink(&set[1]),
            ink(&set[2])
        );
        let b = set[2].bounding_box();
        assert!(b.width() < 1.1 && b.height() < 1.5, "{b:?}");
        assert!(
            b.center().x.abs() < 0.2 && b.center().y.abs() < 0.3,
            "{b:?}"
        );

        let (img, bounds) = ramp();
        let s = VectorMosaicSettings {
            shape: TileShape::Characters,
            chars: " .@".into(),
            ..uniform()
        };
        let t = tiles(&img, bounds, 4.0, &s).expect("tiles");
        let mut n = 0;
        let shapes = mosaic_shapes(&t, &s, || {
            n += 1;
            tracedraw_core::ShapeId(n)
        });
        // The light third is blank: fewer objects than tiles.
        assert!(!shapes.is_empty() && shapes.len() < t.len());
        // An empty set falls back to the default characters.
        let none = VectorMosaicSettings {
            chars: String::new(),
            ..s
        };
        assert_eq!(tone_set(&none).len(), DEFAULT_CHARS.chars().count());
    }

    #[test]
    fn applying_groups_the_mosaic_in_one_step() {
        let mut app = crate::app::App::headless();
        let id = app
            .new_shape(ShapeKind::Ellipse {
                rect: Rect::new(0.0, 0.0, 20.0, 20.0),
                arc: None,
            })
            .expect("ellipse");
        app.run(Command::SetFill {
            shapes: vec![id],
            fill: Fill::Solid(Color::rgb8(30, 90, 200)),
        });
        app.select(vec![id]);
        let depth = app.engine.history_labels().0.len();
        assert!(app.apply_vector_mosaic(&VectorMosaicSettings {
            keep_original: false,
            method: TrackMethod::Opacity,
            ..uniform()
        }));
        assert_eq!(app.engine.history_labels().0.len(), depth + 1);
        assert!(app.doc().find_shape(id).is_none());
        let g = app.selected_shapes().into_iter().next().expect("group");
        match g.kind {
            ShapeKind::Group { children } => assert!(children.len() > 20, "{}", children.len()),
            _ => panic!("not a group"),
        }
        app.undo();
        assert!(app.doc().find_shape(id).is_some());
    }
}
