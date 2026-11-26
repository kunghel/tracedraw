//! Layer styles of the image editing mode (Bitmaps > Edit Inside, #4369):
//! effects worked out from a layer's see-through edge each time the
//! picture is put together, so they stay changeable until the layers are
//! flattened or the editing is finished. A drop shadow and an outer glow
//! behind the layer; a colour overlay, a gradient overlay, an inner
//! glow, an inner shadow, a bevel and an outline on it.
//!
//! The pure parts (distances, blurs, the effects) are here; the Layers
//! panel shows the settings (`ui/pixel_layers.rs`) and
//! `PixelImage::composite_region` runs them.

use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};
use tracedraw_core::live::MergeMode;

/// A shadow cast by the layer (behind it) or into it (inside its edge).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Shadow {
    pub on: bool,
    pub color: [u8; 3],
    /// 0 to 1.
    pub opacity: f32,
    pub merge: MergeMode,
    /// Where the light comes from, degrees counter-clockwise from the
    /// right: the shadow falls the other way.
    pub angle: f32,
    /// How far the shadow falls, pixels.
    pub distance: f32,
    /// How far its edge is blurred, pixels.
    pub size: f32,
    /// How much of the size is solid before the blur starts, 0 to 1.
    pub spread: f32,
}

impl Shadow {
    fn drop() -> Self {
        Shadow {
            on: false,
            color: [0, 0, 0],
            opacity: 0.6,
            merge: MergeMode::Multiply,
            angle: 120.0,
            distance: 8.0,
            size: 10.0,
            spread: 0.0,
        }
    }
}

impl Default for Shadow {
    fn default() -> Self {
        Shadow::drop()
    }
}

/// Where a glow starts inside the layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum GlowSource {
    /// From the edge in.
    #[default]
    Edge,
    /// From the middle out.
    Center,
}

/// A soft glow round the layer's edge (outside or inside).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Glow {
    pub on: bool,
    pub color: [u8; 3],
    pub opacity: f32,
    pub merge: MergeMode,
    pub size: f32,
    pub spread: f32,
    /// The inner glow only.
    pub source: GlowSource,
}

impl Default for Glow {
    fn default() -> Self {
        Glow {
            on: false,
            color: [255, 240, 170],
            opacity: 0.75,
            merge: MergeMode::Screen,
            size: 12.0,
            spread: 0.0,
            source: GlowSource::Edge,
        }
    }
}

/// How the bevel stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BevelStyle {
    /// Raised inside the layer's edge.
    #[default]
    Inner,
    /// Raised round the outside of the edge.
    Outer,
    /// Raised from the picture, inside and out.
    Emboss,
    /// Sunk round the edge, as if pressed in.
    Pillow,
}

impl BevelStyle {
    pub const ALL: [BevelStyle; 4] = [
        BevelStyle::Inner,
        BevelStyle::Outer,
        BevelStyle::Emboss,
        BevelStyle::Pillow,
    ];

    pub fn key(self) -> &'static str {
        match self {
            BevelStyle::Inner => "pixel.style.bevel_inner",
            BevelStyle::Outer => "pixel.style.bevel_outer",
            BevelStyle::Emboss => "pixel.style.bevel_emboss",
            BevelStyle::Pillow => "pixel.style.bevel_pillow",
        }
    }
}

/// The layer made to stand out of the picture: lit on one side, shaded
/// on the other.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Bevel {
    pub on: bool,
    pub style: BevelStyle,
    /// How steep, 0.1 to 10 (1 as drawn).
    pub depth: f32,
    /// How wide the slope is, pixels.
    pub size: f32,
    /// How rounded the slope is, pixels of blur.
    pub soften: f32,
    /// Where the light comes from (degrees, as the shadows) and how high
    /// it stands (degrees over the picture).
    pub angle: f32,
    pub altitude: f32,
    /// Sunk in rather than raised.
    pub down: bool,
    pub highlight: [u8; 3],
    pub highlight_opacity: f32,
    pub shadow: [u8; 3],
    pub shadow_opacity: f32,
}

impl Default for Bevel {
    fn default() -> Self {
        Bevel {
            on: false,
            style: BevelStyle::Inner,
            depth: 1.0,
            size: 6.0,
            soften: 0.0,
            angle: 120.0,
            altitude: 30.0,
            down: false,
            highlight: [255, 255, 255],
            highlight_opacity: 0.75,
            shadow: [0, 0, 0],
            shadow_opacity: 0.75,
        }
    }
}

/// Where the outline lies on the layer's edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum StrokeAt {
    #[default]
    Outside,
    Inside,
    Center,
}

impl StrokeAt {
    pub const ALL: [StrokeAt; 3] = [StrokeAt::Outside, StrokeAt::Inside, StrokeAt::Center];

    pub fn key(self) -> &'static str {
        match self {
            StrokeAt::Outside => "pixel.style.stroke_outside",
            StrokeAt::Inside => "pixel.style.stroke_inside",
            StrokeAt::Center => "pixel.style.stroke_center",
        }
    }
}

/// An outline round the layer's edge.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Outline {
    pub on: bool,
    pub color: [u8; 3],
    pub opacity: f32,
    pub merge: MergeMode,
    pub size: f32,
    pub at: StrokeAt,
}

impl Default for Outline {
    fn default() -> Self {
        Outline {
            on: false,
            color: [0, 0, 0],
            opacity: 1.0,
            merge: MergeMode::Normal,
            size: 3.0,
            at: StrokeAt::Outside,
        }
    }
}

/// The layer's pixels coloured over, keeping their see-through.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ColorOverlay {
    pub on: bool,
    pub color: [u8; 3],
    pub opacity: f32,
    pub merge: MergeMode,
}

impl Default for ColorOverlay {
    fn default() -> Self {
        ColorOverlay {
            on: false,
            color: [230, 40, 40],
            opacity: 1.0,
            merge: MergeMode::Normal,
        }
    }
}

/// The layer's pixels coloured over with a gradient.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GradientOverlay {
    pub on: bool,
    pub from: [u8; 4],
    pub to: [u8; 4],
    pub opacity: f32,
    pub merge: MergeMode,
    /// Degrees counter-clockwise from the right, the way `from` to `to`.
    pub angle: f32,
    /// Round from the middle rather than straight across.
    pub radial: bool,
    /// The gradient's length as a share of the layer's box, 0.1 to 1.5.
    pub scale: f32,
    pub reverse: bool,
}

impl Default for GradientOverlay {
    fn default() -> Self {
        GradientOverlay {
            on: false,
            from: [0, 0, 0, 255],
            to: [255, 255, 255, 255],
            opacity: 1.0,
            merge: MergeMode::Normal,
            angle: 90.0,
            radial: false,
            scale: 1.0,
            reverse: false,
        }
    }
}

/// A layer's styles. All off by default.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LayerStyle {
    /// The effects shown (off hides them all, keeping their settings).
    pub on: bool,
    /// How much of the layer's own pixels show, 0 to 1, its effects
    /// staying as they are (0 leaves only the effects).
    pub fill_opacity: f32,
    pub drop_shadow: Shadow,
    pub outer_glow: Glow,
    pub color_overlay: ColorOverlay,
    pub gradient_overlay: GradientOverlay,
    pub inner_glow: Glow,
    pub inner_shadow: Shadow,
    pub bevel: Bevel,
    pub stroke: Outline,
}

impl Default for LayerStyle {
    fn default() -> Self {
        let mut inner_shadow = Shadow::drop();
        inner_shadow.distance = 5.0;
        inner_shadow.size = 6.0;
        let inner_glow = Glow {
            merge: MergeMode::Screen,
            size: 8.0,
            ..Glow::default()
        };
        LayerStyle {
            on: true,
            fill_opacity: 1.0,
            drop_shadow: Shadow::drop(),
            outer_glow: Glow::default(),
            color_overlay: ColorOverlay::default(),
            gradient_overlay: GradientOverlay::default(),
            inner_glow,
            inner_shadow,
            bevel: Bevel::default(),
            stroke: Outline::default(),
        }
    }
}

impl LayerStyle {
    /// Something shows: an effect is on, or the fill is not whole.
    pub fn shows(&self) -> bool {
        self.on
            && (self.drop_shadow.on
                || self.outer_glow.on
                || self.color_overlay.on
                || self.gradient_overlay.on
                || self.inner_glow.on
                || self.inner_shadow.on
                || self.bevel.on
                || self.stroke.on
                || self.fill_opacity < 1.0)
    }

    /// How many effects are on (the Layers panel's row shows it).
    pub fn count(&self) -> usize {
        [
            self.drop_shadow.on,
            self.outer_glow.on,
            self.color_overlay.on,
            self.gradient_overlay.on,
            self.inner_glow.on,
            self.inner_shadow.on,
            self.bevel.on,
            self.stroke.on,
        ]
        .iter()
        .filter(|b| **b)
        .count()
    }

    /// How far round the layer's pixels the effects reach, pixels: the
    /// room a part of the picture needs round it to be styled right.
    pub fn reach(&self) -> u32 {
        let mut r: f32 = 0.0;
        if self.drop_shadow.on {
            r = r.max(self.drop_shadow.distance.abs() + self.drop_shadow.size);
        }
        if self.inner_shadow.on {
            r = r.max(self.inner_shadow.distance.abs() + self.inner_shadow.size);
        }
        if self.outer_glow.on {
            r = r.max(self.outer_glow.size);
        }
        if self.inner_glow.on {
            r = r.max(self.inner_glow.size);
        }
        if self.bevel.on {
            r = r.max(self.bevel.size + self.bevel.soften);
        }
        if self.stroke.on {
            r = r.max(self.stroke.size);
        }
        (r.max(0.0).ceil() as u32).saturating_add(3).min(2000)
    }
}

// ---------------------------------------------------------------------
// Distances and blurs on planes of f32 (0 to 1)
// ---------------------------------------------------------------------

/// One row of the squared distance transform (Felzenszwalb and
/// Huttenlocher): `f` in, the squared distances out.
fn dt_1d(f: &[f64], out: &mut [f64], v: &mut [usize], z: &mut [f64]) {
    let n = f.len();
    if n == 0 {
        return;
    }
    let mut k = 0usize;
    v[0] = 0;
    z[0] = f64::NEG_INFINITY;
    z[1] = f64::INFINITY;
    let sect = |q: usize, p: usize| {
        let (qf, pf) = (q as f64, p as f64);
        ((f[q] + qf * qf) - (f[p] + pf * pf)) / (2.0 * (qf - pf))
    };
    for q in 1..n {
        let mut s = sect(q, v[k]);
        while s <= z[k] && k > 0 {
            k -= 1;
            s = sect(q, v[k]);
        }
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = f64::INFINITY;
    }
    k = 0;
    for (q, o) in out.iter_mut().enumerate().take(n) {
        while z[k + 1] < q as f64 {
            k += 1;
        }
        let d = q as f64 - v[k] as f64;
        *o = d * d + f[v[k]];
    }
}

/// The distance (pixels) from each pixel to the nearest one where `inside`
/// holds; 0 on those. Far when none holds.
pub fn distance_to(w: usize, h: usize, inside: impl Fn(usize) -> bool) -> Vec<f32> {
    const FAR: f64 = 1e10;
    let n = w * h;
    let mut g: Vec<f64> = (0..n).map(|i| if inside(i) { 0.0 } else { FAR }).collect();
    let longest = w.max(h).max(1);
    let mut f = vec![0.0f64; longest];
    let mut out = vec![0.0f64; longest];
    let mut v = vec![0usize; longest];
    let mut z = vec![0.0f64; longest + 1];
    // Down the columns, then along the rows.
    for x in 0..w {
        for y in 0..h {
            f[y] = g[y * w + x];
        }
        dt_1d(&f[..h], &mut out[..h], &mut v, &mut z);
        for y in 0..h {
            g[y * w + x] = out[y];
        }
    }
    let mut res = vec![0.0f32; n];
    for y in 0..h {
        f[..w].copy_from_slice(&g[y * w..y * w + w]);
        dt_1d(&f[..w], &mut out[..w], &mut v, &mut z);
        for x in 0..w {
            res[y * w + x] = out[x].sqrt() as f32;
        }
    }
    res
}

fn box_line(src: &[f32], out: &mut [f32], r: usize) {
    let n = src.len();
    if n == 0 {
        return;
    }
    let at = |i: isize| src[i.clamp(0, n as isize - 1) as usize];
    let r = r as isize;
    let mut acc: f32 = (-r..=r).map(at).sum();
    let k = (2 * r + 1) as f32;
    for (i, o) in out.iter_mut().enumerate().take(n) {
        *o = acc / k;
        acc += at(i as isize + r + 1) - at(i as isize - r);
    }
}

/// A blur of about `size` pixels' reach (three box passes each way),
/// the plane's edges carried on.
pub fn blur(plane: &[f32], w: usize, h: usize, size: f32) -> Vec<f32> {
    let r = (size / 3.0).round() as usize;
    if r == 0 || w == 0 || h == 0 {
        return plane.to_vec();
    }
    let mut a = plane.to_vec();
    let mut line = vec![0.0f32; w.max(h)];
    let mut out = vec![0.0f32; w.max(h)];
    for _ in 0..3 {
        for y in 0..h {
            box_line(&a[y * w..y * w + w], &mut out[..w], r);
            a[y * w..y * w + w].copy_from_slice(&out[..w]);
        }
        for x in 0..w {
            for y in 0..h {
                line[y] = a[y * w + x];
            }
            box_line(&line[..h], &mut out[..h], r);
            for y in 0..h {
                a[y * w + x] = out[y];
            }
        }
    }
    a
}

/// The plane grown by `by` pixels (round), soft at its new edge.
pub fn spread(alpha: &[f32], w: usize, h: usize, by: f32) -> Vec<f32> {
    if by <= 0.0 {
        return alpha.to_vec();
    }
    let d = distance_to(w, h, |i| alpha[i] >= 0.5);
    alpha
        .iter()
        .zip(d)
        .map(|(a, d)| a.max((by + 1.0 - d).clamp(0.0, 1.0)))
        .collect()
}

/// The plane moved by (dx, dy) pixels (whole pixels), clear where it
/// comes in.
fn shifted(p: &[f32], w: usize, h: usize, dx: i64, dy: i64, fill: f32) -> Vec<f32> {
    let mut out = vec![fill; p.len()];
    for y in 0..h as i64 {
        let sy = y - dy;
        if sy < 0 || sy >= h as i64 {
            continue;
        }
        for x in 0..w as i64 {
            let sx = x - dx;
            if sx < 0 || sx >= w as i64 {
                continue;
            }
            out[(y as usize) * w + x as usize] = p[(sy as usize) * w + sx as usize];
        }
    }
    out
}

/// The way a shadow falls for light from `angle` degrees, `d` pixels.
fn fall(angle: f32, d: f32) -> (i64, i64) {
    let (s, c) = angle.to_radians().sin_cos();
    ((-c * d).round() as i64, (s * d).round() as i64)
}

/// A shadow's or glow's soft shape from `alpha`: spread solid, then the
/// rest of the size blurred.
fn soft(alpha: &[f32], w: usize, h: usize, size: f32, spread_share: f32) -> Vec<f32> {
    let s = spread_share.clamp(0.0, 1.0);
    let grown = spread(alpha, w, h, size * s);
    blur(&grown, w, h, size * (1.0 - s))
}

// ---------------------------------------------------------------------
// Putting the effects together
// ---------------------------------------------------------------------

/// A colour with a coverage plane as a picture.
fn colored(w: usize, h: usize, rgb: [u8; 3], cover: &[f32], opacity: f32) -> RgbaImage {
    let mut img = RgbaImage::new(w as u32, h as u32);
    for (q, c) in img.pixels_mut().zip(cover) {
        let a = (c * opacity).clamp(0.0, 1.0);
        *q = Rgba([rgb[0], rgb[1], rgb[2], (a * 255.0).round() as u8]);
    }
    img
}

fn to_pixmap(img: &RgbaImage) -> Option<tiny_skia::Pixmap> {
    crate::pixel_edit::rgba_to_pixmap(img, 1.0)
}

fn from_pixmap(pm: &tiny_skia::Pixmap) -> RgbaImage {
    crate::pixel_edit::pixmap_to_rgba(pm, pm.width(), pm.height())
        .unwrap_or_else(|| RgbaImage::new(pm.width(), pm.height()))
}

fn put(target: &mut tiny_skia::Pixmap, img: &RgbaImage, merge: MergeMode) {
    if let Some(layer) = to_pixmap(img) {
        tracedraw_render::composite_layer(target, &layer, (0, 0), merge);
    }
}

/// The layer's pixels with `style` run on them: a picture of the same
/// size. The parts of the effects that would fall outside it are lost,
/// so pass a part with [`LayerStyle::reach`] pixels of room round what
/// is to be seen.
pub fn render(src: &RgbaImage, style: &LayerStyle) -> RgbaImage {
    if !style.shows() {
        return src.clone();
    }
    let (wu, hu) = src.dimensions();
    let (w, h) = (wu as usize, hu as usize);
    if w == 0 || h == 0 {
        return src.clone();
    }
    let Some(mut out) = tiny_skia::Pixmap::new(wu, hu) else {
        return src.clone();
    };
    let alpha: Vec<f32> = src.pixels().map(|q| q[3] as f32 / 255.0).collect();
    // Behind the layer.
    let ds = style.drop_shadow;
    if ds.on && ds.opacity > 0.0 {
        let (dx, dy) = fall(ds.angle, ds.distance);
        let s = soft(&shifted(&alpha, w, h, dx, dy, 0.0), w, h, ds.size, ds.spread);
        put(&mut out, &colored(w, h, ds.color, &s, ds.opacity), ds.merge);
    }
    let og = style.outer_glow;
    if og.on && og.opacity > 0.0 {
        let s = soft(&alpha, w, h, og.size, og.spread);
        put(&mut out, &colored(w, h, og.color, &s, og.opacity), og.merge);
    }
    // The layer's own pixels, coloured over by the overlays.
    let mut content = src.clone();
    let co = style.color_overlay;
    let go = style.gradient_overlay;
    if co.on || go.on {
        if let Some(mut pm) = to_pixmap(src) {
            if go.on {
                let g = gradient_picture(src, &go);
                put(&mut pm, &g, go.merge);
            }
            if co.on {
                let mut c = RgbaImage::from_pixel(wu, hu, Rgba([co.color[0], co.color[1], co.color[2], 0]));
                for (q, a) in c.pixels_mut().zip(&alpha) {
                    q[3] = (a * co.opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
                }
                put(&mut pm, &c, co.merge);
            }
            content = from_pixmap(&pm);
            // The overlays keep the layer's see-through as it was.
            for (q, s) in content.pixels_mut().zip(src.pixels()) {
                q[3] = s[3];
            }
        }
    }
    let fill = style.fill_opacity.clamp(0.0, 1.0);
    if fill < 1.0 {
        for q in content.pixels_mut() {
            q[3] = (q[3] as f32 * fill).round() as u8;
        }
    }
    put(&mut out, &content, MergeMode::Normal);
    // On the layer, inside its edge.
    let ig = style.inner_glow;
    if ig.on && ig.opacity > 0.0 {
        let outside: Vec<f32> = alpha.iter().map(|a| 1.0 - a).collect();
        let edge = soft(&outside, w, h, ig.size, ig.spread);
        let cover: Vec<f32> = match ig.source {
            GlowSource::Edge => edge.iter().zip(&alpha).map(|(e, a)| e * a).collect(),
            GlowSource::Center => edge.iter().zip(&alpha).map(|(e, a)| (1.0 - e) * a).collect(),
        };
        put(&mut out, &colored(w, h, ig.color, &cover, ig.opacity), ig.merge);
    }
    let is = style.inner_shadow;
    if is.on && is.opacity > 0.0 {
        let (dx, dy) = fall(is.angle, is.distance);
        let outside: Vec<f32> = alpha.iter().map(|a| 1.0 - a).collect();
        let s = soft(&shifted(&outside, w, h, dx, dy, 1.0), w, h, is.size, is.spread);
        let cover: Vec<f32> = s.iter().zip(&alpha).map(|(s, a)| s * a).collect();
        put(&mut out, &colored(w, h, is.color, &cover, is.opacity), is.merge);
    }
    if style.bevel.on {
        let (light, dark) = bevel_shading(&alpha, w, h, &style.bevel);
        let b = style.bevel;
        put(&mut out, &colored(w, h, b.shadow, &dark, b.shadow_opacity), MergeMode::Multiply);
        put(&mut out, &colored(w, h, b.highlight, &light, b.highlight_opacity), MergeMode::Screen);
    }
    let st = style.stroke;
    if st.on && st.size > 0.0 && st.opacity > 0.0 {
        let cover = stroke_cover(&alpha, w, h, st.size, st.at);
        put(&mut out, &colored(w, h, st.color, &cover, st.opacity), st.merge);
    }
    from_pixmap(&out)
}

/// How much an outline `size` pixels wide at `at` covers each pixel of a
/// layer of see-through `alpha` (0 to 1).
pub fn stroke_cover(alpha: &[f32], w: usize, h: usize, size: f32, at: StrokeAt) -> Vec<f32> {
    let d_out = distance_to(w, h, |i| alpha[i] >= 0.5);
    let d_in = distance_to(w, h, |i| alpha[i] < 0.5);
    let band = |d: f32, wide: f32| (wide + 1.0 - d).clamp(0.0, 1.0);
    (0..w * h)
        .map(|i| {
            let a = alpha[i];
            match at {
                // Outside the edge only: as much as the layer leaves
                // see-through.
                StrokeAt::Outside => {
                    if a >= 0.5 {
                        1.0 - a
                    } else {
                        band(d_out[i], size)
                    }
                }
                StrokeAt::Inside => {
                    if a >= 0.5 {
                        band(d_in[i], size) * a
                    } else {
                        0.0
                    }
                }
                StrokeAt::Center => {
                    let half = size / 2.0;
                    if a >= 0.5 {
                        band(d_in[i], half)
                    } else {
                        band(d_out[i], half)
                    }
                }
            }
        })
        .collect()
}

/// The light and the shade of a bevel on a layer of see-through `alpha`.
fn bevel_shading(alpha: &[f32], w: usize, h: usize, b: &Bevel) -> (Vec<f32>, Vec<f32>) {
    let size = b.size.max(1.0);
    // The height: how far inside the edge each pixel is, up to the size
    // (outside the edge for an outer bevel), rounded by the softening.
    let d_in = distance_to(w, h, |i| alpha[i] < 0.5);
    let d_out = distance_to(w, h, |i| alpha[i] >= 0.5);
    let height: Vec<f32> = (0..w * h)
        .map(|i| {
            let inside = (d_in[i] / size).min(1.0);
            let outside = (d_out[i] / size).min(1.0);
            match b.style {
                BevelStyle::Inner => inside,
                BevelStyle::Outer => {
                    if alpha[i] >= 0.5 {
                        1.0
                    } else {
                        1.0 - outside
                    }
                }
                BevelStyle::Emboss => 0.5 + 0.5 * inside - 0.5 * outside,
                BevelStyle::Pillow => {
                    if alpha[i] >= 0.5 {
                        inside
                    } else {
                        outside
                    }
                }
            }
        })
        .collect();
    let height = blur(&height, w, h, b.soften.max(1.0));
    // Where the bevel shows.
    let shows = |i: usize| -> f32 {
        match b.style {
            BevelStyle::Inner => alpha[i],
            BevelStyle::Outer => {
                if d_out[i] <= size {
                    1.0 - alpha[i]
                } else {
                    0.0
                }
            }
            BevelStyle::Emboss | BevelStyle::Pillow => {
                if d_out[i] <= size {
                    1.0
                } else {
                    0.0
                }
            }
        }
    };
    let (ls, lc) = b.angle.to_radians().sin_cos();
    let alt = b.altitude.clamp(0.0, 90.0).to_radians();
    // The light, y down on the picture.
    let light = [lc * alt.cos(), -ls * alt.cos(), alt.sin()];
    let flat = light[2];
    let k = b.depth.clamp(0.1, 10.0) * size * if b.down { -1.0 } else { 1.0 };
    let mut lit = vec![0.0f32; w * h];
    let mut dark = vec![0.0f32; w * h];
    let at = |x: i64, y: i64| height[(y.clamp(0, h as i64 - 1) as usize) * w + x.clamp(0, w as i64 - 1) as usize];
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let m = shows(i);
            if m <= 0.0 {
                continue;
            }
            let (xi, yi) = (x as i64, y as i64);
            let gx = (at(xi + 1, yi) - at(xi - 1, yi)) * 0.5 * k;
            let gy = (at(xi, yi + 1) - at(xi, yi - 1)) * 0.5 * k;
            // The slope's normal: up, tipped against the rise.
            let n = [-gx, -gy, 1.0];
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            let dot = (n[0] * light[0] + n[1] * light[1] + n[2] * light[2]) / len;
            let v = dot - flat;
            if v > 0.0 {
                lit[i] = (v / (1.0 - flat).max(0.05)).min(1.0) * m;
            } else {
                dark[i] = (-v / flat.max(0.05)).min(1.0) * m;
            }
        }
    }
    (lit, dark)
}

/// The gradient overlay over the box of the layer's pixels.
fn gradient_picture(src: &RgbaImage, g: &GradientOverlay) -> RgbaImage {
    let (w, h) = src.dimensions();
    // The box of what shows.
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0u32, 0u32);
    for (x, y, q) in src.enumerate_pixels() {
        if q[3] > 0 {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x + 1);
            y1 = y1.max(y + 1);
        }
    }
    if x1 <= x0 || y1 <= y0 {
        return RgbaImage::new(w, h);
    }
    let (cx, cy) = ((x0 + x1) as f32 / 2.0, (y0 + y1) as f32 / 2.0);
    let (bw, bh) = ((x1 - x0) as f32, (y1 - y0) as f32);
    let (s, c) = g.angle.to_radians().sin_cos();
    let (dx, dy) = (c, -s);
    // Half the box's extent along the direction.
    let half = (bw * dx.abs() + bh * dy.abs()) / 2.0 * g.scale.clamp(0.1, 1.5);
    let radius = (bw.hypot(bh) / 2.0 * g.scale.clamp(0.1, 1.5)).max(1.0);
    let (a, b) = if g.reverse { (g.to, g.from) } else { (g.from, g.to) };
    let mut out = RgbaImage::new(w, h);
    let op = g.opacity.clamp(0.0, 1.0);
    for (x, y, q) in out.enumerate_pixels_mut() {
        let sa = src.get_pixel(x, y)[3];
        if sa == 0 {
            continue;
        }
        let (px, py) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
        let t = if g.radial {
            (px.hypot(py) / radius).clamp(0.0, 1.0)
        } else {
            ((px * dx + py * dy) / (2.0 * half.max(1.0)) + 0.5).clamp(0.0, 1.0)
        };
        let ch = |i: usize| (a[i] as f32 + (b[i] as f32 - a[i] as f32) * t).round() as u8;
        let alpha = (ch(3) as f32 * op * sa as f32 / 255.0).round() as u8;
        *q = Rgba([ch(0), ch(1), ch(2), alpha]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(side: u32, from: u32, to: u32) -> RgbaImage {
        RgbaImage::from_fn(side, side, |x, y| {
            if (from..to).contains(&x) && (from..to).contains(&y) {
                Rgba([200, 30, 30, 255])
            } else {
                Rgba([0, 0, 0, 0])
            }
        })
    }

    #[test]
    fn distances_are_round_and_exact() {
        let (w, h) = (21, 21);
        let d = distance_to(w, h, |i| i == 10 * w + 10);
        assert_eq!(d[10 * w + 10], 0.0);
        assert!((d[10 * w + 13] - 3.0).abs() < 1e-4);
        assert!((d[13 * w + 14] - 5.0).abs() < 1e-4);
        assert!((d[0] - (200.0f32).sqrt()).abs() < 1e-3);
    }

    #[test]
    fn nothing_on_leaves_the_pixels_as_they_are() {
        let src = square(40, 10, 30);
        let style = LayerStyle::default();
        assert!(!style.shows());
        assert_eq!(render(&src, &style), src);
    }

    #[test]
    fn a_drop_shadow_falls_away_from_the_light_behind_the_layer() {
        let src = square(60, 20, 40);
        let mut style = LayerStyle::default();
        style.drop_shadow.on = true;
        style.drop_shadow.merge = MergeMode::Normal;
        style.drop_shadow.angle = 135.0; // light from the top left
        style.drop_shadow.distance = 8.0;
        style.drop_shadow.size = 0.0;
        let out = render(&src, &style);
        // Down and right of the square: shadow; up and left: nothing.
        assert!(out.get_pixel(45, 45)[3] > 100);
        assert_eq!(out.get_pixel(15, 15)[3], 0);
        // The layer itself stays on top.
        assert_eq!(out.get_pixel(30, 30), &Rgba([200, 30, 30, 255]));
        assert!(style.reach() >= 8);
    }

    #[test]
    fn an_outside_outline_rings_the_layer_only_outside() {
        let src = square(40, 10, 30);
        let mut style = LayerStyle::default();
        style.stroke.on = true;
        style.stroke.size = 3.0;
        style.stroke.color = [0, 0, 255];
        let out = render(&src, &style);
        assert_eq!(out.get_pixel(8, 20), &Rgba([0, 0, 255, 255]));
        assert_eq!(out.get_pixel(20, 20), &Rgba([200, 30, 30, 255]));
        assert_eq!(out.get_pixel(3, 20)[3], 0);
        // Inside: within the edge.
        style.stroke.at = StrokeAt::Inside;
        let out = render(&src, &style);
        assert_eq!(out.get_pixel(11, 20), &Rgba([0, 0, 255, 255]));
        assert_eq!(out.get_pixel(8, 20)[3], 0);
        assert_eq!(out.get_pixel(20, 20), &Rgba([200, 30, 30, 255]));
    }

    #[test]
    fn overlays_colour_the_layer_keeping_its_see_through() {
        let src = square(30, 5, 25);
        let mut style = LayerStyle::default();
        style.color_overlay.on = true;
        style.color_overlay.color = [0, 255, 0];
        let out = render(&src, &style);
        assert_eq!(out.get_pixel(15, 15), &Rgba([0, 255, 0, 255]));
        assert_eq!(out.get_pixel(2, 2)[3], 0);
        let mut style = LayerStyle::default();
        style.gradient_overlay.on = true;
        style.gradient_overlay.angle = 0.0; // black on the left, white on the right
        let out = render(&src, &style);
        assert!(out.get_pixel(6, 15)[0] < 40);
        assert!(out.get_pixel(23, 15)[0] > 215);
    }

    #[test]
    fn glows_bevels_and_inner_shadows_change_the_pixels_where_they_belong() {
        let src = square(60, 20, 40);
        let mut style = LayerStyle::default();
        style.outer_glow.on = true;
        let out = render(&src, &style);
        assert!(out.get_pixel(17, 30)[3] > 0, "outer glow outside the edge");
        let mut style = LayerStyle::default();
        style.inner_glow.on = true;
        style.inner_glow.merge = MergeMode::Normal;
        let out = render(&src, &style);
        assert_ne!(out.get_pixel(21, 30), src.get_pixel(21, 30));
        assert_eq!(out.get_pixel(10, 10)[3], 0, "nothing outside");
        let mut style = LayerStyle::default();
        style.bevel.on = true;
        let out = render(&src, &style);
        // Light from the top left: the top left slope lighter than the
        // bottom right one.
        let lum = |p: &Rgba<u8>| p[0] as u32 + p[1] as u32 + p[2] as u32;
        assert!(lum(out.get_pixel(22, 30)) > lum(out.get_pixel(37, 30)));
        let mut style = LayerStyle::default();
        style.inner_shadow.on = true;
        style.inner_shadow.merge = MergeMode::Normal;
        style.inner_shadow.size = 0.0;
        let out = render(&src, &style);
        // The shadow inside the top left edge (it falls down and right).
        assert!(lum(out.get_pixel(22, 22)) < lum(src.get_pixel(22, 22)));
        assert_eq!(out.get_pixel(37, 37), src.get_pixel(37, 37));
    }

    #[test]
    fn fill_opacity_hides_the_pixels_but_keeps_the_effects() {
        let src = square(40, 10, 30);
        let mut style = LayerStyle::default();
        style.fill_opacity = 0.0;
        style.stroke.on = true;
        style.stroke.color = [0, 0, 0];
        let out = render(&src, &style);
        assert_eq!(out.get_pixel(20, 20)[3], 0);
        assert_eq!(out.get_pixel(8, 20)[3], 255);
    }
}
