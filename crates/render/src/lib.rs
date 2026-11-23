//! CPU rasterizer. Draws a page region into an RGBA pixmap with tiny-skia:
//! correct non-convex fills, fountain fills, outline widths, caps, joins,
//! dashes, hairlines and anti-aliasing. The GPU renderer, when it exists,
//! must match this one.

#![allow(clippy::field_reassign_with_default)]
pub mod accel;
pub mod bitmap_effects;
pub mod halftone;
mod image_cache;

use tiny_skia::{
    Color as SkColor, FillRule, GradientStop, LineCap, LineJoin, LinearGradient, Paint,
    Path as SkPath, PathBuilder, Pixmap, RadialGradient, SpreadMode, Stroke as SkStroke,
    StrokeDash, Transform,
};
use tracedraw_core::geometry::Shape as _;
use tracedraw_core::{
    document::{Shape, ShapeKind},
    geometry::{Affine, BezPath, PathEl, Point, Rect},
    Color, Document, Fill, LineCap as LCap, LineJoin as LJoin, PageId, ShapeId, Stroke,
};

/// Page (mm, Y up) to pixel mapping for one render.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewTransform {
    /// Pixels per millimetre.
    pub zoom: f64,
    /// Pixel position of the page origin (bottom-left corner) inside the target.
    pub origin_x: f64,
    pub origin_y: f64,
    /// The vertical pixels per millimetre as a share of `zoom`: 1 but for
    /// an exported picture, whose rounded width and height each take the
    /// page edge to edge (#2866), a hair apart.
    pub stretch_y: f64,
}

impl ViewTransform {
    /// A page of `width` by `height` mm at `zoom` pixels per mm as a
    /// picture of whole pixels: the page's size rounded up, as the
    /// export dialog counts it, and the zoom made a hair larger each way
    /// so the page fills every one of them. At the zoom asked, the page left a
    /// row and a column partly empty above it and to its right, a light
    /// line on the edge of an exported picture (#2866). Returns the
    /// picture's width, height and the view.
    pub fn whole_page(width: f64, height: f64, zoom: f64) -> (u32, u32, ViewTransform) {
        let px = |mm: f64| {
            let v = (mm * zoom).ceil();
            // A size that is not a number asks for a picture too large
            // to make, as before (no picture rather than a wrong one).
            if v.is_nan() {
                u32::MAX
            } else {
                v.clamp(1.0, f64::from(u32::MAX)) as u32
            }
        };
        let (w, h) = (px(width), px(height));
        // Each way its own: one zoom for both took the larger of the two
        // and cut up to a pixel or so off the other side (the top of an
        // A4 page), more on a long strip.
        let ok = |z: f64| z.is_finite() && z > 0.0;
        let (zx, zy) = (f64::from(w) / width, f64::from(h) / height);
        let zoom = if ok(zx) { zx } else { zoom };
        let stretch_y = if ok(zy) && ok(zoom) { zy / zoom } else { 1.0 };
        (
            w,
            h,
            ViewTransform {
                zoom,
                origin_x: 0.0,
                origin_y: f64::from(h),
                stretch_y,
            },
        )
    }

    pub fn affine(&self) -> Affine {
        Affine::new([
            self.zoom,
            0.0,
            0.0,
            -self.zoom * self.stretch_y,
            self.origin_x,
            self.origin_y,
        ])
    }
}

/// Objects to draw displaced by a transform (live drag preview). The ids
/// are a set: every object of the page is looked up in it each frame, so
/// a drag of tens of thousands of objects stays quick (#3601).
#[derive(Debug, Clone, Default)]
pub struct Preview {
    pub shapes: std::collections::HashSet<ShapeId>,
    pub transform: Affine,
}

/// The page-space box of every object of a page, worked out once for a
/// render and shared by its bands: a band's culling asks every object's
/// box, and before this cache each of the bands (one per core) walked
/// the whole page's paths again (#3559).
type PageBounds = std::collections::HashMap<ShapeId, Rect>;

/// [`PageBounds`] of the page's drawable layers: each shape's box with
/// the transforms of the groups and clip frames holding it taken in,
/// the same value [`Renderer::reach`] would compute for it.
#[cfg(not(target_arch = "wasm32"))]
fn page_bounds_cache(doc: &Document, page: PageId) -> PageBounds {
    fn walk(s: &Shape, parent: Affine, map: &mut PageBounds) {
        let b = if parent == Affine::IDENTITY {
            s.bounds()
        } else {
            parent.transform_rect_bbox(s.bounds())
        };
        map.insert(s.id, b);
        let t = parent * s.transform;
        match &s.kind {
            ShapeKind::Group { children } => {
                for c in children {
                    walk(c, t, map);
                }
            }
            ShapeKind::ClipFrame { frame, contents } => {
                walk(frame, t, map);
                for c in contents {
                    walk(c, t, map);
                }
            }
            _ => {}
        }
    }
    let mut map = PageBounds::new();
    if let Ok(layers) = doc.layers_drawn_for_page(page) {
        for layer in layers {
            for s in &layer.shapes {
                walk(s, Affine::IDENTITY, &mut map);
            }
        }
    }
    map
}

/// Runs bitmap effects over straight (not premultiplied) RGBA pixels of
/// `width` x `height`, in place. The renderer has no bitmap effects of its
/// own: the application that has them registers this for the Bitmap
/// Effects lens; without it the lens shows what is beneath unchanged.
pub type PixelEffects = fn(&[tracedraw_core::BitmapEffect], u32, u32, &mut Vec<u8>);

static PIXEL_EFFECTS: std::sync::OnceLock<PixelEffects> = std::sync::OnceLock::new();

/// Register the bitmap effects (once; later calls are ignored).
pub fn set_pixel_effects(f: PixelEffects) {
    let _ = PIXEL_EFFECTS.set(f);
}

#[derive(Clone)]
pub struct RenderOptions {
    pub width: u32,
    pub height: u32,
    pub view: ViewTransform,
    pub preview: Option<Preview>,
    /// Draw outlines only (the Wireframe view).
    pub wireframe: bool,
    /// Show overprints (View > Simulate Overprints): a fill marked
    /// `overprint_fill` and an outline marked `overprint_outline` are
    /// multiplied over what lies beneath instead of knocking it out.
    pub simulate_overprints: bool,
    /// Rasterise lenses and non-uniform transparency ("Rasterize complex
    /// effects" in Enhanced view). When false
    /// those objects are drawn plainly, which is much faster.
    pub complex_effects: bool,
    /// Smooth edges (Convert to Bitmap's Anti-aliasing); off gives hard
    /// pixel edges.
    pub anti_alias: bool,
    /// On screen: large procedural fills (textures, conical and square
    /// gradients) may be worked out on a coarser grid, and drop shadows
    /// may be blurred on the graphics card ([`accel`], same pixels).
    /// Exports keep every pixel and draw everything on the CPU.
    pub draft: bool,
    /// Focus mode: these objects drawn as they are and everything else
    /// faded to the given visibility (0 to 1).
    pub focus: Option<(Vec<tracedraw_core::ShapeId>, f32)>,
}

impl Default for RenderOptions {
    fn default() -> Self {
        RenderOptions {
            width: 1,
            height: 1,
            view: ViewTransform {
                zoom: 1.0,
                origin_x: 0.0,
                origin_y: 0.0,
                stretch_y: 1.0,
            },
            preview: None,
            wireframe: false,
            simulate_overprints: false,
            complex_effects: true,
            anti_alias: true,
            draft: false,
            focus: None,
        }
    }
}

/// Per-renderer behaviour flags, copied into every nested renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Flags {
    wireframe: bool,
    simulate_overprints: bool,
    complex_effects: bool,
    /// Fill open subpaths (the document's "Fill open curves").
    fill_open_curves: bool,
    anti_alias: bool,
    draft: bool,
    /// Drawing the warped outline of a text: filled with the nonzero rule
    /// as the text is, so letters that overlap leave no hole (#647).
    text_outline: bool,
    /// Drawing on a layer of its own that starts see-through (a halftone
    /// screens it next): an inner shadow keeps each pixel's coverage and
    /// darkens the object's edge pixels as fully as their insides, so the
    /// edge shows no light rim once the layer is screened.
    atop: bool,
}

impl Flags {
    fn from_options(opts: &RenderOptions) -> Self {
        Flags {
            wireframe: opts.wireframe,
            simulate_overprints: opts.simulate_overprints,
            complex_effects: opts.complex_effects,
            fill_open_curves: false,
            anti_alias: opts.anti_alias,
            draft: opts.draft,
            text_outline: false,
            atop: false,
        }
    }
}

impl Default for Flags {
    fn default() -> Self {
        Flags {
            wireframe: false,
            simulate_overprints: false,
            complex_effects: true,
            fill_open_curves: false,
            anti_alias: true,
            draft: false,
            text_outline: false,
            atop: false,
        }
    }
}

/// How `kind` is filled: text, and the warped outline of a text
/// (`text_outline`), with the nonzero rule, so overlapping strokes and
/// letters stay filled (#647, #800); other curves with even-odd.
fn fill_rule(kind: &ShapeKind, text_outline: bool) -> FillRule {
    match kind {
        ShapeKind::Text { .. } => FillRule::Winding,
        _ if text_outline => FillRule::Winding,
        _ => FillRule::EvenOdd,
    }
}

/// How `shape` is filled: as [`fill_rule`] says for its kind, or with the
/// nonzero rule when the object asks for it (#1919, #1211).
fn shape_fill_rule(shape: &Shape, text_outline: bool) -> FillRule {
    match shape.fill_rule {
        tracedraw_core::shaping::FillRule::NonZero => FillRule::Winding,
        tracedraw_core::shaping::FillRule::EvenOdd => fill_rule(&shape.kind, text_outline),
    }
}

/// The subpaths of `path` that are closed: ended with a close command, or
/// ending where they start. Open curves are not filled unless the
/// document asks for it.
pub fn closed_subpaths(path: &BezPath) -> BezPath {
    let mut out = BezPath::new();
    let mut cur: Vec<PathEl> = Vec::new();
    let flush = |cur: &mut Vec<PathEl>, out: &mut BezPath| {
        if cur.is_empty() {
            return;
        }
        let start = match cur.first() {
            Some(PathEl::MoveTo(p)) => Some(*p),
            _ => None,
        };
        let end = match cur.last() {
            Some(PathEl::LineTo(p))
            | Some(PathEl::QuadTo(_, p))
            | Some(PathEl::CurveTo(_, _, p)) => Some(*p),
            _ => None,
        };
        let closed = cur.iter().any(|e| matches!(e, PathEl::ClosePath))
            || matches!((start, end), (Some(a), Some(b)) if (a - b).hypot() < 1e-6);
        if closed {
            for e in cur.drain(..) {
                out.push(e);
            }
        }
        cur.clear();
    };
    for el in path.elements() {
        if matches!(el, PathEl::MoveTo(_)) {
            flush(&mut cur, &mut out);
        }
        cur.push(*el);
    }
    flush(&mut cur, &mut out);
    out
}

/// The most pixels a page is drawn as at once: 1 G pixels (4 GB of RGBA),
/// more than any export or print asks for. A page whose size is not a
/// number (left by a division by zero somewhere, or read from a damaged
/// file) asks for an image as tall as a number holds, and asking the
/// allocator for it takes the program down, which cannot be caught; such
/// an image is refused instead (`None`).
pub const MAX_PIXELS: u64 = 1 << 30;

/// Render the objects of one page onto a transparent pixmap, `None` for
/// an image of more than [`MAX_PIXELS`].
pub fn render_page(doc: &Document, page: PageId, opts: &RenderOptions) -> Option<Pixmap> {
    render_page_until(doc, page, opts, None)
}

/// [`render_page`] that gives up once `cancel` is set (checked before
/// each object): `None` then, as soon as the object being drawn is
/// done. The canvas sets it when the view has zoomed on before the
/// picture was ready, so the picture for the new zoom starts at once
/// instead of after a whole picture nobody will see.
pub fn render_page_cancellable(
    doc: &Document,
    page: PageId,
    opts: &RenderOptions,
    cancel: &std::sync::atomic::AtomicBool,
) -> Option<Pixmap> {
    render_page_until(doc, page, opts, Some(cancel))
}

/// [`render_page_cancellable`] that hands over the picture a band of
/// rows at a time as the bands come done, `band(y0, rows)` with the rows
/// `y0..y0 + rows.height()` exactly as they are in the whole picture,
/// the band holding row `first` first and the others outward from it
/// (#2389). A zoom on a large drawing showed the last picture scaled,
/// blurry, until the whole new one was ready: the canvas now lays each
/// band over it as it comes, sharp where it is done, the middle of the
/// window first. The
/// bands are a few times as many as the cores, so the first comes in a
/// fraction of the time; each is drawn with the rows of margin shadow
/// blurs need. Drawn whole (no bands handed over) where bands cannot be
/// used: a small image, one core, a lens, blurs too wide.
#[cfg(not(target_arch = "wasm32"))]
pub fn render_page_progressive(
    doc: &Document,
    page: PageId,
    opts: &RenderOptions,
    cancel: &std::sync::atomic::AtomicBool,
    first: u32,
    band: &(dyn Fn(u32, &Pixmap) + Sync),
) -> Option<Pixmap> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let (w, h) = (opts.width.max(1), opts.height.max(1));
    if (w as u64) * (h as u64) > MAX_PIXELS {
        return None;
    }
    let cancelled = || cancel.load(Ordering::Relaxed);
    if cancelled() {
        return None;
    }
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .min(16) as u32;
    let (lens, blur_mm) = page_reach(doc, page);
    let bands = if cores < 2 || lens || (w as u64) * (h as u64) < 250_000 {
        None
    } else {
        // Bands of at least 64 rows, about three for each core.
        let count = (cores * PROGRESS_BANDS_PER_CORE).min(h / 64).max(cores);
        band_layout(h, count, blur_margin(blur_mm, opts.view.zoom))
    };
    let Some(mut bands) = bands.filter(|b| b.len() > 1) else {
        return render_page_cancellable(doc, page, opts, cancel);
    };
    let middle = |b: &(u32, u32, u32, u32)| (b.0 + b.1) / 2;
    bands.sort_by_key(|b| middle(b).abs_diff(first));
    // Every band's culling asks every object's box: work the boxes out
    // once here instead of once per band (#3559).
    let boxes = page_bounds_cache(doc, page);
    let next = AtomicUsize::new(0);
    // Assemble each band as it finishes instead of keeping a whole
    // picture of bands beside a second whole picture at the end. Only
    // the workers' current bands and their blur margins stay live.
    let out = std::sync::Mutex::new(Pixmap::new(w, h)?);
    let done = AtomicUsize::new(0);
    std::thread::scope(|sc| {
        for _ in 0..cores.min(bands.len() as u32) {
            sc.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(&(y0, y1, ry0, ry1)) = bands.get(i) else {
                    return;
                };
                if cancelled() {
                    return;
                }
                let mut o = opts.clone();
                o.height = ry1 - ry0;
                o.view.origin_y = opts.view.origin_y - ry0 as f64;
                let Some(part) = render_page_whole_in(doc, page, &o, Some(cancel), Some(&boxes))
                else {
                    return;
                };
                if cancelled() {
                    return;
                }
                let Some(mut rows) = Pixmap::new(w, y1 - y0) else {
                    return;
                };
                let row = w as usize * 4;
                let from = (y0 - ry0) as usize * row;
                let len = (y1 - y0) as usize * row;
                let Some(src) = part.data().get(from..from + len) else {
                    return;
                };
                rows.data_mut().copy_from_slice(src);
                drop(part);
                {
                    let Ok(mut out) = out.lock() else {
                        return;
                    };
                    let start = y0 as usize * row;
                    let Some(dst) = out.data_mut().get_mut(start..start + len) else {
                        return;
                    };
                    dst.copy_from_slice(rows.data());
                }
                // The callback may wait for its consumer. It must never
                // hold the assembly lock while it does so.
                band(y0, &rows);
                done.fetch_add(1, Ordering::Relaxed);
            });
        }
    });
    if cancelled() || done.load(Ordering::Relaxed) != bands.len() {
        return None;
    }
    out.into_inner().ok()
}

/// How many bands for each core a picture handed over a band at a time
/// is drawn in ([`render_page_progressive`]).
#[cfg(not(target_arch = "wasm32"))]
const PROGRESS_BANDS_PER_CORE: u32 = 3;

fn render_page_until(
    doc: &Document,
    page: PageId,
    opts: &RenderOptions,
    cancel: Option<&std::sync::atomic::AtomicBool>,
) -> Option<Pixmap> {
    if (opts.width.max(1) as u64) * (opts.height.max(1) as u64) > MAX_PIXELS {
        return None;
    }
    let cancelled = || cancel.is_some_and(|c| c.load(std::sync::atomic::Ordering::Relaxed));
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(p) = render_page_in_bands(doc, page, opts, cancel) {
        return (!cancelled()).then_some(p);
    }
    let p = render_page_whole(doc, page, opts, cancel)?;
    (!cancelled()).then_some(p)
}

/// [`render_page`] on the calling thread alone, never in bands on all
/// the cores: for a picture nobody is waiting for (the whole page drawn
/// small behind the canvas's picture, #2048), so that the other cores
/// stay free for the frames meanwhile.
pub fn render_page_on_one_thread(
    doc: &Document,
    page: PageId,
    opts: &RenderOptions,
) -> Option<Pixmap> {
    if (opts.width.max(1) as u64) * (opts.height.max(1) as u64) > MAX_PIXELS {
        return None;
    }
    render_page_whole(doc, page, opts, None)
}

/// Rows `y0..y1` of the image `opts` describes, the same as those rows of
/// the whole image drawn at once: drawn with rows of margin above and
/// below as far as shadow blurs reach (every row when the page has a
/// lens, which may show what lies anywhere beneath it), so a large page
/// can be drawn a strip at a time without the whole of it in memory
/// (#1404). Strips should start on rows that are multiples of 16, as the
/// coarse grid of procedural fills does.
pub fn render_page_rows(
    doc: &Document,
    page: PageId,
    opts: &RenderOptions,
    y0: u32,
    y1: u32,
) -> Option<Pixmap> {
    let (w, h) = (opts.width.max(1), opts.height.max(1));
    let (y0, y1) = (y0.min(h), y1.min(h));
    if y1 <= y0 {
        return None;
    }
    let (lens, blur_mm) = page_reach(doc, page);
    let margin = if lens {
        h
    } else {
        blur_margin(blur_mm, opts.view.zoom)
    };
    let (ry0, ry1) = (y0.saturating_sub(margin), y1.saturating_add(margin).min(h));
    let mut o = opts.clone();
    o.width = w;
    o.height = ry1 - ry0;
    o.view.origin_y = opts.view.origin_y - ry0 as f64;
    let part = render_page(doc, page, &o)?;
    let mut out = Pixmap::new(w, y1 - y0)?;
    let row = w as usize * 4;
    let from = (y0 - ry0) as usize * row;
    let len = (y1 - y0) as usize * row;
    out.data_mut()
        .copy_from_slice(part.data().get(from..from + len)?);
    Some(out)
}

/// The reduced level of a picture to draw it from, given `m` from its
/// pixels to the screen: each level halves its sides, and the one
/// chosen is drawn at between half and the whole of its pixels' size
/// (level 0 when the picture is drawn at half its size or more).
fn bitmap_level(m: Affine) -> u32 {
    let c = m.as_coeffs();
    let scale = c[0].hypot(c[1]).max(c[2].hypot(c[3]));
    if !scale.is_finite() || scale <= 0.0 || scale >= 0.5 {
        return 0;
    }
    ((1.0 / scale).log2().floor().max(0.0) as u32).min(image_cache::MAX_LEVEL)
}

/// A picture's pixels-to-screen map `m` (a `w` x `h` picture) made to
/// cover whole screen pixels when it is upright (not turned or
/// slanted): its edges rounded to the nearest pixel boundary, so two
/// pictures that meet on the page meet on the same pixel boundary. The
/// area to fill comes with it, `None` when the picture is narrower than
/// a pixel after rounding. `None` for a turned or slanted picture.
fn snap_upright(m: [f64; 6], w: u32, h: u32) -> Option<([f64; 6], Option<tiny_skia::Rect>)> {
    let upright = m[1].abs() < 1e-9 && m[2].abs() < 1e-9;
    if !upright || !m.iter().all(|v| v.is_finite()) {
        return None;
    }
    let (w, h) = (w.max(1) as f64, h.max(1) as f64);
    let (xa, xb) = (m[4], m[4] + m[0] * w);
    let (ya, yb) = (m[5], m[5] + m[3] * h);
    let (x0, x1) = (xa.min(xb).round(), xa.max(xb).round());
    let (y0, y1) = (ya.min(yb).round(), ya.max(yb).round());
    // Each side goes to the rounded edge it came from (a mirrored
    // picture keeps its mirroring).
    let (a, e) = if m[0] >= 0.0 {
        ((x1 - x0) / w, x0)
    } else {
        (-(x1 - x0) / w, x1)
    };
    let (d, f) = if m[3] >= 0.0 {
        ((y1 - y0) / h, y0)
    } else {
        (-(y1 - y0) / h, y1)
    };
    let area = tiny_skia::Rect::from_ltrb(x0 as f32, y0 as f32, x1 as f32, y1 as f32)
        .filter(|_| x1 > x0 && y1 > y0);
    Some(([a, 0.0, 0.0, d, e, f], area))
}

/// The Blend If ranges `shape` is drawn with (its own lightness, what
/// lies beneath), when it has the effect and it hides anything.
fn blend_if(shape: &Shape) -> Option<([f64; 4], [f64; 4])> {
    shape.effects.iter().find_map(|e| match e {
        tracedraw_core::live::Effect::BlendIf { this, below }
            if *this != tracedraw_core::live::BLEND_IF_ALL
                || *below != tracedraw_core::live::BLEND_IF_ALL =>
        {
            Some((*this, *below))
        }
        _ => None,
    })
}

/// How a Displace effect of `shape` shades it from the photo beneath:
/// the shading, its strength and the photo's middle grey, when it does
/// (#3595).
fn surface_shading(
    shape: &Shape,
) -> Option<(tracedraw_core::live::SurfaceShade, f64, f64)> {
    use tracedraw_core::live::{Effect, SurfaceShade};
    shape.effects.iter().find_map(|e| match e {
        Effect::Displace {
            map,
            shade,
            strength,
            ..
        } if *shade != SurfaceShade::None && *strength > 0.0 => {
            Some((*shade, *strength, map.as_ref().map_or(0.5, |m| m.mean)))
        }
        _ => None,
    })
}

/// The first Halftone effect of `shape`, when it has one switched on.
fn halftone_of(shape: &Shape) -> Option<tracedraw_core::live::Effect> {
    shape
        .effects
        .iter()
        .find(|e| {
            matches!(
                e,
                tracedraw_core::live::Effect::Halftone { size, .. }
                    if size.is_finite() && *size > 0.0
            )
        })
        .cloned()
}

/// The radius (mm) of the Gaussian blur `shape` is drawn with, when it
/// has one switched on (#1141).
fn gaussian_blur(shape: &Shape) -> Option<f64> {
    shape.effects.iter().find_map(|e| match e {
        tracedraw_core::live::Effect::GaussianBlur { radius }
            if radius.is_finite() && *radius > 0.0 =>
        {
            Some(*radius)
        }
        _ => None,
    })
}

/// The width (mm) over which `shape` fades out towards its edge, when it
/// has a Feather effect.
fn feather(shape: &Shape) -> Option<f64> {
    shape.effects.iter().find_map(|e| match e {
        tracedraw_core::live::Effect::Feather { radius } if radius.is_finite() && *radius > 0.0 => {
            Some(radius.min(1000.0))
        }
        _ => None,
    })
}

/// What `s` reaches past its own pixels: sets `lens` when it or an
/// object in it has a lens, and grows `blur` to its widest drop shadow
/// blur (mm).
fn reach_walk(s: &Shape, lens: &mut bool, blur: &mut f64) {
    if let Some(sh) = &s.shadow {
        *blur = blur.max(sh.blur.abs());
    }
    // Stacked outer shadows, glows and a Gaussian blur reach as far
    // (#1141).
    for e in &s.effects {
        use tracedraw_core::live::Effect;
        let r = match e {
            Effect::OuterShadow { offset, blur, .. } => blur.abs() + offset.hypot(),
            Effect::Glow { blur, .. } | Effect::GaussianBlur { radius: blur } => blur.abs(),
            // Whole dots reach half a cell past the edge.
            Effect::Halftone { size, .. } => size.abs(),
            _ => 0.0,
        };
        if r.is_finite() {
            *blur = blur.max(r);
        }
    }
    if s.effects
        .iter()
        .any(|e| matches!(e, tracedraw_core::live::Effect::Lens(_)))
    {
        *lens = true;
    }
    match &s.kind {
        ShapeKind::Group { children } => {
            for c in children {
                reach_walk(c, lens, blur);
            }
        }
        ShapeKind::ClipFrame { frame, contents } => {
            reach_walk(frame, lens, blur);
            for c in contents {
                reach_walk(c, lens, blur);
            }
        }
        _ => {}
    }
}

/// What on the page reaches past its own pixels: a lens (it samples what
/// lies around it) and the widest drop shadow blur, mm.
fn page_reach(doc: &Document, page: PageId) -> (bool, f64) {
    let (mut lens, mut blur) = (false, 0.0f64);
    if let Ok(layers) = doc.layers_drawn_for_page(page) {
        for l in layers {
            for s in &l.shapes {
                reach_walk(s, &mut lens, &mut blur);
            }
        }
    }
    (lens, blur)
}

/// A large render split into horizontal bands drawn at once on the
/// machine's cores, then put together (#345: one core took half a second
/// for 5000 objects). Each band is drawn with rows of margin above and
/// below, as far as the widest shadow blur reaches, so blurred edges meet; the
/// bands start on rows that are multiples of 16, as the coarse grid of
/// procedural fills does. `None` (draw it whole) for a small image, a
/// single core, a lens on the page, or blurs too wide for the bands.
#[cfg(not(target_arch = "wasm32"))]
fn render_page_in_bands(
    doc: &Document,
    page: PageId,
    opts: &RenderOptions,
    cancel: Option<&std::sync::atomic::AtomicBool>,
) -> Option<Pixmap> {
    let (w, h) = (opts.width.max(1), opts.height.max(1));
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .min(16) as u32;
    if cores < 2 || (w as u64) * (h as u64) < 250_000 || h < BAND_ALIGN * 4 {
        return None;
    }
    let (lens, blur_mm) = page_reach(doc, page);
    if lens {
        return None;
    }
    let bands = band_layout(h, cores, blur_margin(blur_mm, opts.view.zoom))?;
    // Every band's culling asks every object's box: work the boxes out
    // once here instead of once per band (#3559).
    let boxes = page_bounds_cache(doc, page);
    let parts: Vec<Option<Pixmap>> = std::thread::scope(|sc| {
        let handles: Vec<_> = bands
            .iter()
            .map(|&(_, _, ry0, ry1)| {
                let mut o = opts.clone();
                o.height = ry1 - ry0;
                o.view.origin_y = opts.view.origin_y - ry0 as f64;
                let boxes = &boxes;
                sc.spawn(move || render_page_whole_in(doc, page, &o, cancel, Some(boxes)))
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().ok().flatten())
            .collect()
    });
    let mut out = Pixmap::new(w, h)?;
    let row = w as usize * 4;
    for (&(y0, y1, ry0, _), part) in bands.iter().zip(parts) {
        let part = part?;
        let src = part.data();
        let dst = out.data_mut();
        for y in y0..y1 {
            let s = (y - ry0) as usize * row;
            let d = y as usize * row;
            dst[d..d + row].copy_from_slice(&src[s..s + row]);
        }
    }
    Some(out)
}

/// Bands and strips start on rows that are multiples of this, as the
/// coarse grid of procedural fills does.
const BAND_ALIGN: u32 = 16;

/// The rows of margin a strip of a page needs above and below for the
/// widest drop shadow blur `blur_mm` to meet across its edges at `zoom`:
/// a shadow is blurred with three box passes of its radius (or one of
/// 1.7 times it), so it reaches three radii past its silhouette. The
/// arithmetic saturates, so a blur that is not a number (read from a
/// damaged file) gives the whole range, not an overflow.
fn blur_margin(blur_mm: f64, zoom: f64) -> u32 {
    ((blur_mm * zoom).round() as u32)
        .saturating_mul(3)
        .saturating_add(4)
        .div_ceil(BAND_ALIGN)
        .saturating_mul(BAND_ALIGN)
}

/// The bands of an image `h` rows high drawn on `cores` cores, each as
/// `(y0, y1, ry0, ry1)`: its own rows `y0..y1` and the rows `ry0..ry1`
/// it is drawn with, `margin` rows above and below. `None` when the
/// margin is wider than two bands (the bands would draw more than the
/// whole). The arithmetic saturates, so an image as tall as a number
/// holds (a page whose size is not a number, read from a damaged file)
/// gives bands that end at its last row, not an overflow.
#[cfg(not(target_arch = "wasm32"))]
fn band_layout(h: u32, cores: u32, margin: u32) -> Option<Vec<(u32, u32, u32, u32)>> {
    let band = h
        .div_ceil(cores.max(1))
        .div_ceil(BAND_ALIGN)
        .saturating_mul(BAND_ALIGN);
    if margin > band.saturating_mul(2) {
        return None;
    }
    Some(
        (0..cores)
            .map(|i| i.saturating_mul(band))
            .take_while(|y0| *y0 < h)
            .map(|y0| {
                let y1 = y0.saturating_add(band).min(h);
                (
                    y0,
                    y1,
                    y0.saturating_sub(margin),
                    y1.saturating_add(margin).min(h),
                )
            })
            .collect(),
    )
}

fn render_page_whole(
    doc: &Document,
    page: PageId,
    opts: &RenderOptions,
    cancel: Option<&std::sync::atomic::AtomicBool>,
) -> Option<Pixmap> {
    render_page_whole_in(doc, page, opts, cancel, None)
}

/// [`render_page_whole`] with the page's object boxes already worked
/// out (`bounds`), shared by the bands of one render (#3559).
fn render_page_whole_in(
    doc: &Document,
    page: PageId,
    opts: &RenderOptions,
    cancel: Option<&std::sync::atomic::AtomicBool>,
    bounds: Option<&PageBounds>,
) -> Option<Pixmap> {
    let mut pixmap = Pixmap::new(opts.width.max(1), opts.height.max(1))?;
    let page = doc.page(page).ok()?;
    let screen = opts.view.affine();
    let mut flags = Flags::from_options(opts);
    flags.fill_open_curves = doc.metadata.fill_open_curves;
    let mut r = Renderer {
        pixmap: &mut pixmap,
        screen,
        zoom: opts.view.zoom,
        flags,
        symbols: &doc.symbols,
        preview: opts.preview.as_ref(),
        bounds,
        origin: (0, 0),
        cancel,
    };
    // Focus at 0 % visibility hides everything but the objects in focus:
    // the page and its layers are not drawn at all, as their faded
    // pixels would all be clear (#981).
    let rest_hidden = rest_hidden(opts);
    if let Some(bg) = page.background.as_ref().filter(|_| !rest_hidden) {
        let mut bg_shape = Shape::new(
            tracedraw_core::ShapeId(0),
            ShapeKind::Rect {
                rect: page.rect(),
                radius: 0.0,
                corners: None,
            },
        );
        bg_shape.fill = bg.clone();
        bg_shape.stroke = None;
        r.draw_shape(&bg_shape, Affine::IDENTITY);
    }
    let visible = visible_page_rect(opts);
    let layers = doc.layers_drawn_for_page(page.id).ok()?;
    for layer in layers {
        if !layer.visible || rest_hidden {
            continue;
        }
        for shape in &layer.shapes {
            if r.out_of_sight(shape, &visible) {
                continue;
            }
            let extra = r.child_transform(shape, Affine::IDENTITY);
            r.draw_shape(shape, extra);
        }
    }
    // Focus mode: everything fades, then the objects in focus are drawn
    // again over it as they are.
    if let Some((ids, visibility)) = &opts.focus {
        let v = visibility.clamp(0.0, 1.0);
        let faded = if rest_hidden {
            &mut [][..]
        } else {
            r.pixmap.pixels_mut()
        };
        for px in faded {
            let c = |x: u8| (x as f32 * v).round() as u8;
            if let Some(faded) = tiny_skia::PremultipliedColorU8::from_rgba(
                c(px.red()),
                c(px.green()),
                c(px.blue()),
                c(px.alpha()),
            ) {
                *px = faded;
            }
        }
        for id in ids {
            if let (Some(shape), Some(parent)) = (doc.find_shape(*id), doc.parent_transform(*id)) {
                let extra = r.child_transform(shape, parent);
                r.draw_shape(shape, extra);
            }
        }
    }
    Some(pixmap)
}

/// Whether focus mode hides everything outside the focus: a visibility
/// of 0 (or one that is not a number, read from damaged settings).
fn rest_hidden(opts: &RenderOptions) -> bool {
    opts.focus
        .as_ref()
        .is_some_and(|(_, v)| v.is_nan() || *v <= 0.0)
}

/// The part of the page the target shows, page millimetres.
fn visible_page_rect(opts: &RenderOptions) -> Rect {
    let inv = opts.view.affine().inverse();
    let a = inv * Point::new(0.0, 0.0);
    let b = inv * Point::new(opts.width as f64, opts.height as f64);
    Rect::from_points(a, b)
}

/// How far an outline may reach past the path it follows, in widths:
/// mitres, arrowheads and calligraphic nibs all stay within this (the
/// nodes of a variable width are counted on their own).
const OUTLINE_REACH: f64 = 12.0;

/// Paths drawn on this thread, for tests.
#[cfg(test)]
thread_local! {
    static PATHS_DRAWN: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The largest scratch buffer (in pixels) a lens, a transparency or a
/// blur made, on any thread (a page may be drawn in bands on several),
/// for tests (#3439, #3441, #3442), which hold [`SCRATCH_TEST`] while
/// they use it.
#[cfg(test)]
static LARGEST_SCRATCH: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
#[cfg(test)]
static SCRATCH_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Notes a scratch buffer of `w` by `h` pixels, for tests.
#[inline]
fn note_scratch(w: u32, h: u32) {
    #[cfg(test)]
    LARGEST_SCRATCH.fetch_max(w as usize * h as usize, std::sync::atomic::Ordering::Relaxed);
    #[cfg(not(test))]
    let _ = (w, h);
}

struct Renderer<'a> {
    pixmap: &'a mut Pixmap,
    screen: Affine,
    zoom: f64,
    flags: Flags,
    symbols: &'a [tracedraw_core::Symbol],
    /// Objects drawn displaced (the live drag preview), on a layer or
    /// inside a group or a clip frame.
    preview: Option<&'a Preview>,
    /// The page-space box of every object of the page, worked out once
    /// for a render in bands ([`page_bounds_cache`]): each band's
    /// culling asks every object's box, and without it all bands paid
    /// that walk again (#3559). `None` on single-pass renders.
    bounds: Option<&'a PageBounds>,
    /// Where `pixmap` lies on the target it is laid on, in pixels: a
    /// semi-transparent object's layer covers only what it reaches, and
    /// pattern tiles stay where they fall on the target (#1335).
    origin: (i32, i32),
    /// Set when the picture is no longer wanted (the view zoomed on
    /// before it was ready): every object from then on is skipped.
    cancel: Option<&'a std::sync::atomic::AtomicBool>,
}

impl Renderer<'_> {
    /// A renderer with the same settings drawing into another pixmap.
    fn sub<'b>(&self, pixmap: &'b mut Pixmap) -> Renderer<'b>
    where
        Self: 'b,
    {
        Renderer {
            pixmap,
            screen: self.screen,
            zoom: self.zoom,
            flags: self.flags,
            symbols: self.symbols,
            preview: self.preview,
            bounds: self.bounds,
            origin: self.origin,
            cancel: self.cancel,
        }
    }

    /// The transform `child` is drawn with inside `transform`: the live
    /// drag preview's first when the child is being dragged.
    /// True when nothing of `shape` can show in `visible` (page mm), so
    /// it need not be drawn: its bounds, grown by what its outline may
    /// reach, lie wholly outside. Objects whose extent is not their
    /// bounds (effects, shadows, symbols, the drag preview) are always
    /// drawn.
    fn out_of_sight(&self, shape: &Shape, visible: &Rect) -> bool {
        self.out_of_sight_in(shape, Affine::IDENTITY, visible)
    }

    /// [`Self::out_of_sight`] for `shape` inside a group or clip frame
    /// whose transform to the page is `parent`: the objects of a group
    /// and the frame of a clip frame lie in its space, so a moved clip
    /// frame (the pages placed on a printed sheet) is tested where it
    /// draws (#848).
    fn out_of_sight_in(&self, shape: &Shape, parent: Affine, visible: &Rect) -> bool {
        if !shape.visible {
            return true;
        }
        if !shape.effects.is_empty()
            || shape.shadow.is_some()
            || matches!(shape.kind, ShapeKind::SymbolInstance { .. })
            || self.preview.is_some_and(|p| p.shapes.contains(&shape.id))
        {
            return false;
        }
        // A group or a clip frame: its own outline reaches nowhere, its
        // objects' outlines do. Out of sight when every object is (a clip
        // frame draws nothing past its frame): the group's bounds count
        // no outline, so its strips used to cut a wide or variable
        // outline inside it flat (#626).
        match &shape.kind {
            ShapeKind::Group { children } => {
                let t = parent * shape.transform;
                return children.iter().all(|c| self.out_of_sight_in(c, t, visible));
            }
            ShapeKind::ClipFrame { frame, .. } => {
                return self.out_of_sight_in(frame, parent * shape.transform, visible)
            }
            _ => {}
        }
        let Some(b) = Self::reach(shape, parent, self.bounds) else {
            return false;
        };
        b.x1 < visible.x0 || b.x0 > visible.x1 || b.y1 < visible.y0 || b.y0 > visible.y1
    }

    /// The part of the page (mm) this renderer's pixmap shows, `None`
    /// when its transform cannot be undone (nothing is left out then).
    fn sight(&self) -> Option<Rect> {
        let c = self.screen.as_coeffs();
        let det = c[0] * c[3] - c[1] * c[2];
        if !det.is_finite() || det.abs() < 1e-12 {
            return None;
        }
        let px = Rect::new(
            0.0,
            0.0,
            self.pixmap.width() as f64,
            self.pixmap.height() as f64,
        );
        let r = self.screen.inverse().transform_rect_bbox(px);
        [r.x0, r.y0, r.x1, r.y1]
            .iter()
            .all(|v| v.is_finite())
            .then_some(r)
    }

    /// Where a plain object (no group or clip frame) inside `parent` can
    /// paint, page mm: its bounds grown by what its outline may reach.
    /// `None` when its bounds are not finite. Effects, shadows, symbols
    /// and the drag preview reach further; callers leave those out. The
    /// box comes from the render's shared [`PageBounds`] when there is
    /// one (#3559).
    fn reach(shape: &Shape, parent: Affine, cached: Option<&PageBounds>) -> Option<Rect> {
        let b = match cached.and_then(|c| c.get(&shape.id)) {
            Some(b) => *b,
            None if parent == Affine::IDENTITY => shape.bounds(),
            None => parent.transform_rect_bbox(shape.bounds()),
        };
        if !b.x0.is_finite() || !b.y0.is_finite() || !b.x1.is_finite() || !b.y1.is_finite() {
            return None;
        }
        let scale = {
            let c = (parent * shape.transform).as_coeffs();
            (c[0] * c[0] + c[1] * c[1])
                .sqrt()
                .max((c[2] * c[2] + c[3] * c[3]).sqrt())
                .max(1.0)
        };
        let width = shape.stroke.as_ref().map_or(0.0, |s| s.width.abs()) * scale;
        // A Variable Length node may reach any distance from the path,
        // past what a width covers: counted on its own, or the band was
        // left out of the strips of the page it reaches into (#626).
        let reach = shape.stroke.as_ref().map_or(0.0, |s| {
            s.widths
                .iter()
                .map(|n| n.left.abs().max(n.right.abs()))
                .filter(|r| r.is_finite())
                .fold(0.0, f64::max)
        }) * scale;
        let margin = (width * OUTLINE_REACH).max(reach) + 1.0;
        Some(b.inflate(margin, margin))
    }

    /// The pixels of this renderer's target that a semi-transparent
    /// object inside `parent` can paint, as x, y, width and height: its
    /// scratch layer needs no more (#1335). A layer the size of the whole
    /// target for each such object made a page of 2000 of them take
    /// seconds to draw. The whole target for an object whose reach is
    /// not its bounds (see [`Self::out_of_sight`]).
    fn layer_area(&self, shape: &Shape, parent: Affine) -> (u32, u32, u32, u32) {
        self.layer_area_grown(shape, parent, 0.0)
    }

    /// [`Self::layer_area`] grown by `grow` pixels each way before it is
    /// cut to the target: what a blur of the object spreads over, which
    /// reaches onto the target from an object just off it.
    fn layer_area_grown(&self, shape: &Shape, parent: Affine, grow: f64) -> (u32, u32, u32, u32) {
        let (w, h) = (self.pixmap.width(), self.pixmap.height());
        let whole = (0, 0, w, h);
        if !shape.effects.is_empty()
            || shape.shadow.is_some()
            || matches!(
                shape.kind,
                ShapeKind::SymbolInstance { .. }
                    | ShapeKind::Group { .. }
                    | ShapeKind::ClipFrame { .. }
            )
            || self.preview.is_some()
        {
            return whole;
        }
        let Some(page) = Self::reach(shape, parent, self.bounds) else {
            return whole;
        };
        let r = self.screen.transform_rect_bbox(page).inflate(grow, grow);
        // One pixel more each way for the edge's anti-aliasing.
        let clamp = |v: f64, max: u32| (v.max(0.0).min(max as f64)) as u32;
        let (x0, y0) = (clamp(r.x0.floor() - 1.0, w), clamp(r.y0.floor() - 1.0, h));
        let (x1, y1) = (clamp(r.x1.ceil() + 1.0, w), clamp(r.y1.ceil() + 1.0, h));
        (x0, y0, x1.saturating_sub(x0), y1.saturating_sub(y0))
    }

    /// A group with a look (see `GroupLook`): its members drawn into a
    /// layer of their own (each hiding what it covers of the ones before
    /// it with a knockout), the layer then laid down with the group's
    /// `opacity`.
    fn draw_group_on_its_own(
        &mut self,
        children: &[Shape],
        look: tracedraw_core::document::GroupLook,
        transform: Affine,
        opacity: f64,
    ) {
        let (w, h) = (self.pixmap.width(), self.pixmap.height());
        let Some(mut layer) = Pixmap::new(w, h) else {
            return;
        };
        let sight = self.sight();
        for c in children {
            if sight.is_some_and(|v| self.out_of_sight_in(c, transform, &v)) {
                continue;
            }
            let t = self.child_transform(c, transform);
            if !look.knockout {
                let mut sub = self.sub(&mut layer);
                sub.draw_shape(c, t);
                continue;
            }
            // The member as it paints, and where it lies: drawn solid, its
            // coverage hides what came before, see-through or not.
            let (Some(mut paint), Some(mut cover)) = (Pixmap::new(w, h), Pixmap::new(w, h)) else {
                continue;
            };
            {
                let mut sub = self.sub(&mut paint);
                sub.draw_shape(c, t);
            }
            {
                let mut solid = c.clone();
                solid.opacity = 1.0;
                let mut sub = self.sub(&mut cover);
                sub.draw_shape(&solid, t);
            }
            knock_out(&mut layer, &paint, &cover);
        }
        let pp = tiny_skia::PixmapPaint {
            opacity: opacity.clamp(0.0, 1.0) as f32,
            ..Default::default()
        };
        self.pixmap
            .draw_pixmap(0, 0, layer.as_ref(), &pp, Transform::identity(), None);
    }

    fn child_transform(&self, child: &Shape, transform: Affine) -> Affine {
        match self.preview {
            Some(p) if p.shapes.contains(&child.id) => p.transform * transform,
            _ => transform,
        }
    }

    /// Blend mode for a fill or outline: multiply when it overprints and
    /// overprint simulation is on, otherwise ordinary source-over.
    fn blend_for(&self, overprint: bool) -> tiny_skia::BlendMode {
        if overprint && self.flags.simulate_overprints {
            tiny_skia::BlendMode::Multiply
        } else {
            tiny_skia::BlendMode::SourceOver
        }
    }

    fn draw_shape(&mut self, shape: &Shape, parent: Affine) {
        if self
            .cancel
            .is_some_and(|c| c.load(std::sync::atomic::Ordering::Relaxed))
        {
            return;
        }
        // Bitmap effects on a vector object: its picture in its place.
        if let Some(pic) = shape.drawn_as().filter(|_| !self.flags.wireframe) {
            return self.draw_shape(&pic, parent);
        }
        if !shape.visible {
            return;
        }
        // Halftone: the object as drawn with all its other effects, on a
        // layer of its own, turned into dots.
        if self.flags.complex_effects && !self.flags.wireframe {
            if let Some(ht) = halftone_of(shape) {
                let mut plain = shape.clone();
                plain
                    .effects
                    .retain(|e| !matches!(e, tracedraw_core::live::Effect::Halftone { .. }));
                return self.draw_halftoned(&plain, parent, &ht);
            }
        }
        // Character backgrounds go under the text, in its transform.
        if matches!(shape.kind, ShapeKind::Text { .. }) && !self.flags.wireframe {
            for (rect, fill) in tracedraw_core::document::text_backgrounds(&shape.kind) {
                let mut bg = Shape::new(
                    shape.id,
                    ShapeKind::Rect {
                        rect,
                        radius: 0.0,
                        corners: None,
                    },
                );
                bg.transform = shape.transform;
                bg.fill = fill;
                bg.stroke = None;
                bg.opacity = shape.opacity;
                self.draw_shape(&bg, parent);
            }
        }
        // Live effects: geometry warps, extra objects below and above, and
        // the pixel effects (lens, non-uniform transparency).
        if !shape.effects.is_empty() {
            let ev = tracedraw_core::live::evaluate(shape);
            // What the effects draw keeps the object's id (a symmetry's
            // mirrored copies, a warped outline), while the shared box
            // cache holds the object's own box under that id: a band of
            // the picture outside that box left the copies out, and only
            // a zoom out (one band) showed them all (#3821). They are
            // measured on their own here.
            let cached = self.bounds.take();
            // An object whose effects draw objects of their own (the body
            // of an extrusion, a contour's steps) casts the shadow of all
            // of it, beneath all of it. The shadow was the object's
            // alone, drawn over what its effects put behind it (#2215).
            let whole = casts_whole(&ev);
            if whole && !self.flags.wireframe && shape.opacity > 0.0 {
                if let Some(sh) = shape.shadow.as_ref().filter(|sh| sh.opacity > 0.0) {
                    self.draw_group_shadow(sh, shape, parent);
                }
            }
            // Stacked outer shadows and outer glows (#1141), beneath all
            // the object and its effects draw, cast by its shape.
            if self.flags.complex_effects && !self.flags.wireframe && shape.opacity > 0.0 {
                self.draw_outer_shadows(shape, &ev.main, parent);
            }
            for b in &ev.below {
                let mut b = b.clone();
                b.effects.clear();
                if whole {
                    b.shadow = None;
                }
                self.draw_shape(&b, parent);
            }
            let lens = shape.effects.iter().find_map(|e| match e {
                tracedraw_core::live::Effect::Lens(l) => Some(l.clone()),
                _ => None,
            });
            let transparency = shape.effects.iter().find_map(|e| match e {
                tracedraw_core::live::Effect::Transparency {
                    mask,
                    merge,
                    target,
                } => Some((mask.clone(), *merge, *target)),
                _ => None,
            });
            let mut main = ev.main.clone();
            main.effects.clear();
            if whole {
                main.shadow = None;
            }
            let was_text = self.flags.text_outline;
            self.flags.text_outline |= matches!(shape.kind, ShapeKind::Text { .. });
            if !self.flags.complex_effects || self.flags.wireframe {
                // Complex effects off: the object is drawn as it is, with
                // no pixel work for lenses or transparency masks.
                self.draw_shape(&main, parent);
            } else if let Some(l) = lens {
                self.draw_lens(&main, parent, &l);
            } else if let Some((mask, merge, target)) = transparency {
                // The transparency's target: the fill alone (0), the
                // outline alone (1) or the whole object (2). The part it
                // leaves out is drawn as it is (live test of 0.41: the
                // target was ignored).
                // An outline set behind the fill is drawn first.
                for (part, faded) in tracedraw_core::live::transparency_parts(&main, target) {
                    if faded {
                        self.draw_with_mask(&part, parent, &mask, merge);
                    } else {
                        self.draw_shape(&part, parent);
                    }
                }
            } else if let Some((this, below)) = blend_if(shape) {
                // Blend If: the object shown by its own lightness and
                // that of what lies beneath it.
                self.draw_blend_if(&main, parent, &this, &below);
            } else if let Some((shade, strength, mean)) = surface_shading(shape) {
                // Displace's shading: the photo's shadows and highlights
                // taken onto the object (#3595).
                self.draw_surface_shaded(&main, parent, shade, strength, mean);
            } else if let Some(radius) = gaussian_blur(shape) {
                // Gaussian blur (#1141): the object drawn on its own
                // and blurred.
                self.draw_blurred(&main, parent, radius);
            } else if let Some(radius) = feather(shape) {
                self.draw_feathered(&main, parent, radius);
            } else {
                self.draw_shape(&main, parent);
            }
            self.flags.text_outline = was_text;
            // An inner shadow, over the object and cut to its shape
            // (#2473).
            // An inner glow is an inner shadow that does not move (#1141).
            // Fill opacity leaves both as they are.
            if self.flags.complex_effects && !self.flags.wireframe && shape.opacity > 0.0 {
                for e in &shape.effects {
                    let ish = match e {
                        tracedraw_core::live::Effect::InnerShadow {
                            offset,
                            color,
                            opacity,
                            blur,
                            merge,
                            spread,
                        } => InnerShadow {
                            offset: *offset,
                            color: *color,
                            opacity: *opacity,
                            blur: *blur,
                            merge: *merge,
                            spread: *spread,
                        },
                        tracedraw_core::live::Effect::Glow {
                            inner: true,
                            color,
                            opacity,
                            blur,
                            merge,
                        } => InnerShadow {
                            offset: tracedraw_core::geometry::Vec2::ZERO,
                            color: *color,
                            opacity: *opacity,
                            blur: *blur,
                            merge: *merge,
                            spread: 0.0,
                        },
                        _ => continue,
                    };
                    let mut lit = main.clone();
                    lit.opacity = shape.opacity;
                    self.draw_inner_shadow(&lit, parent, ish);
                }
            }
            for a in &ev.above {
                let mut a = a.clone();
                // A mirrored copy keeps its transparency or lens (#2121).
                a.effects.retain(|e| {
                    matches!(
                        e,
                        tracedraw_core::live::Effect::Lens(_)
                            | tracedraw_core::live::Effect::Transparency { .. }
                    )
                });
                if whole {
                    a.shadow = None;
                }
                self.draw_shape(&a, parent);
            }
            self.bounds = cached;
            return;
        }
        // Characters with their own fill or outline: one text a look.
        if let Some(parts) = shape.text_parts() {
            for p in &parts {
                self.draw_shape(p, parent);
            }
            return;
        }
        let transform = parent * shape.transform;
        // A group's or a clip frame's own shadow, beneath all it holds.
        if matches!(
            shape.kind,
            ShapeKind::Group { .. } | ShapeKind::ClipFrame { .. }
        ) && !self.flags.wireframe
            && shape.opacity > 0.0
        {
            if let Some(sh) = shape.shadow.as_ref().filter(|sh| sh.opacity > 0.0) {
                self.draw_group_shadow(sh, shape, parent);
            }
        }
        if let (ShapeKind::Group { children }, Some(look)) = (&shape.kind, shape.group_look) {
            // A group drawn on its own first, then laid down as one
            // picture (its transparency on the whole), its members
            // hiding the ones beneath them with a knockout.
            if !look.is_plain() && !self.flags.wireframe {
                if shape.opacity > 0.0 {
                    self.draw_group_on_its_own(children, look, transform, shape.opacity);
                }
                return;
            }
        }
        if let ShapeKind::Group { children } = &shape.kind {
            // A group's uniform transparency goes on each member, as if
            // set on each: members that overlap show through each other,
            // as files keep it (on the objects of a group) and as Ungroup
            // leaves it (#1386).
            if shape.opacity < 1.0 && !self.flags.wireframe {
                if shape.opacity <= 0.0 {
                    return;
                }
                let sight = self.sight();
                for c in children {
                    if sight.is_some_and(|v| self.out_of_sight_in(c, transform, &v)) {
                        continue;
                    }
                    let mut c = c.clone();
                    c.absorb_opacity(shape.opacity);
                    let t = self.child_transform(&c, transform);
                    self.draw_shape(&c, t);
                }
                return;
            }
            // Only the members that can show in this picture: zoomed in on
            // a large group (a whole design grouped, as files often keep
            // it), every member off screen was drawn all the same, at the
            // zoom's detail, so each zoom in took far longer to draw than
            // the page fitted in the window (discussion #2389).
            let sight = self.sight();
            for c in children {
                if sight.is_some_and(|v| self.out_of_sight_in(c, transform, &v)) {
                    continue;
                }
                let t = self.child_transform(c, transform);
                self.draw_shape(c, t);
            }
            return;
        }
        if matches!(
            shape.kind,
            ShapeKind::Table(_) | ShapeKind::SymbolInstance { .. }
        ) {
            for c in shape.expand(self.symbols) {
                self.draw_shape(&c, transform);
            }
            return;
        }
        if let (true, ShapeKind::ClipFrame { frame, contents }) =
            (self.flags.wireframe, &shape.kind)
        {
            // Wireframe: the frame, and of the contents what lies inside
            // it, as the other views cut them (#2146). They were drawn
            // whole, so the parts the frame hides showed all round it.
            self.draw_wireframe_contents(frame, contents, transform);
            self.draw_shape(frame, transform);
            return;
        }
        if let (ShapeKind::ClipFrame { frame, contents }, Some(om)) =
            (&shape.kind, shape.opacity_mask)
        {
            // Opacity mask: the frame drawn alone gives, by its brightness,
            // how much of the contents shows; the frame itself is not drawn.
            let (w, h) = (self.pixmap.width(), self.pixmap.height());
            if let (Some(mut frame_px), Some(mut layer)) = (Pixmap::new(w, h), Pixmap::new(w, h)) {
                {
                    let mut fill_only = (**frame).clone();
                    fill_only.stroke = None;
                    let mut sub = self.sub(&mut frame_px);
                    sub.draw_shape(&fill_only, transform);
                }
                if om.invert {
                    // Black shows: white where the frame is black, and
                    // nothing outside the frame.
                    for p in frame_px.pixels_mut() {
                        let c = p.demultiply();
                        let a = c.alpha();
                        let inv = |v: u8| 255 - v;
                        *p = tiny_skia::ColorU8::from_rgba(
                            inv(c.red()),
                            inv(c.green()),
                            inv(c.blue()),
                            a,
                        )
                        .premultiply();
                    }
                }
                let mask =
                    tiny_skia::Mask::from_pixmap(frame_px.as_ref(), tiny_skia::MaskType::Luminance);
                {
                    let mut sub = self.sub(&mut layer);
                    for c in contents {
                        let t = sub.child_transform(c, transform);
                        sub.draw_shape(c, t);
                    }
                }
                let pp = tiny_skia::PixmapPaint {
                    opacity: shape.opacity.clamp(0.0, 1.0) as f32,
                    ..Default::default()
                };
                self.pixmap.draw_pixmap(
                    0,
                    0,
                    layer.as_ref(),
                    &pp,
                    Transform::identity(),
                    Some(&mask),
                );
            }
            return;
        }
        if let ShapeKind::ClipFrame { frame, contents } = &shape.kind {
            // A shadow the frame had of its own (given before it became a
            // frame, or while editing inside it) goes once beneath it all.
            // It went with both the fill and the outline drawn below, so
            // it came twice, the second time over the contents.
            if let Some(sh) = frame.shadow.as_ref().filter(|sh| sh.opacity > 0.0) {
                if frame.visible && frame.opacity > 0.0 && shape.opacity > 0.0 {
                    let page_path = (transform * frame.transform) * frame.local_path();
                    self.draw_object_shadow(sh, frame, transform, &page_path);
                }
            }
            // Frame's fill first, contents clipped to the frame, then the
            // frame's outline; an outline behind the fill goes first of
            // all, beneath the fill and the contents. It came over both,
            // so a text frame with a wide outline behind its fill showed
            // the outline's colour where its fill was (#3110).
            let behind = frame.stroke.as_ref().is_some_and(|st| st.behind_fill);
            let outline = || {
                let mut outline_only = (**frame).clone();
                outline_only.fill = Fill::None;
                outline_only.shadow = None;
                outline_only.opacity = shape.opacity;
                outline_only
            };
            if behind {
                self.draw_shape(&outline(), transform);
            }
            let mut frame_fill_only = (**frame).clone();
            frame_fill_only.stroke = None;
            frame_fill_only.shadow = None;
            frame_fill_only.opacity = shape.opacity;
            self.draw_clip_frame_inside(shape, &frame_fill_only, contents, transform);
            if frame.stroke.is_some() && !behind {
                self.draw_shape(&outline(), transform);
            }
            return;
        }
        // Further fills and outlines: the parts, bottom first.
        if let Some(parts) = shape.layers() {
            for p in &parts {
                self.draw_shape(p, parent);
            }
            return;
        }
        let local = shape.local_path();
        if local.elements().is_empty() {
            return;
        }
        #[cfg(test)]
        PATHS_DRAWN.with(|n| n.set(n.get() + 1));
        let page_path = transform * local;
        let Some(sk) = to_sk_path(&(self.screen * page_path.clone())) else {
            return;
        };
        if self.flags.wireframe {
            // Wireframe: every object as its bare path, the same black
            // hairline whatever its outline's colour or width, its fill,
            // opacity or effects; bitmaps as grey boxes. The hairline
            // took the outline's colour, so an object with a pale outline
            // stood out from the others in a colour of its own, and one
            // with a white outline did not show on the page at all
            // (#2041).
            if let ShapeKind::Bitmap { rect, .. } = &shape.kind {
                let mut paint = Paint::default();
                paint.anti_alias = self.flags.anti_alias;
                paint.set_color_rgba8(208, 208, 208, 255);
                self.pixmap
                    .fill_path(&sk, &paint, FillRule::Winding, Transform::identity(), None);
                // Corner to corner, the mark of a picture left out
                // (#2146): a grey box alone read as an empty frame.
                let mut cross = tracedraw_core::BezPath::new();
                cross.move_to((rect.x0, rect.y0));
                cross.line_to((rect.x1, rect.y1));
                cross.move_to((rect.x0, rect.y1));
                cross.line_to((rect.x1, rect.y0));
                if let Some(cross) = to_sk_path(&(self.screen * (transform * cross))) {
                    self.hairline(&cross, tracedraw_core::Color::BLACK);
                }
            }
            self.hairline(&sk, tracedraw_core::Color::BLACK);
            return;
        }
        let opacity = shape.opacity.clamp(0.0, 1.0) as f32;
        if opacity <= 0.0 {
            return;
        }

        // Drop shadow: blurred silhouette behind the object. A picture's
        // silhouette is what its pixels cover, not its box, so a picture
        // with a transparent background casts the shadow of what it
        // shows (#2027).
        if let Some(sh) = &shape.shadow {
            if !self.flags.wireframe && sh.opacity > 0.0 {
                self.draw_object_shadow(sh, shape, parent, &page_path);
            }
        }

        // Semi-transparent objects render into a scratch layer first so fill
        // and outline do not double up where they overlap.
        // The layer covers only what the object can reach (#1335).
        if opacity < 1.0 {
            let (x, y, w, h) = self.layer_area(shape, parent);
            if w == 0 || h == 0 {
                return;
            }
            if let Some(mut layer) = Pixmap::new(w, h) {
                let mut sub = self.sub(&mut layer);
                sub.screen = Affine::translate((-(x as f64), -(y as f64))) * self.screen;
                sub.origin = (self.origin.0 + x as i32, self.origin.1 + y as i32);
                let mut opaque = shape.clone();
                opaque.opacity = 1.0;
                sub.draw_shape(&opaque, parent);
                let paint = tiny_skia::PixmapPaint {
                    opacity,
                    ..Default::default()
                };
                self.pixmap.draw_pixmap(
                    x as i32,
                    y as i32,
                    layer.as_ref(),
                    &paint,
                    Transform::identity(),
                    None,
                );
            }
            return;
        }

        if let ShapeKind::Bitmap {
            rect,
            width_px,
            height_px,
            png,
            fx: _,
        } = &shape.kind
        {
            if !self.flags.wireframe {
                // Image pixel space (y down) to local rect (y up) to screen.
                let (w0, h0) = ((*width_px).max(1) as f64, (*height_px).max(1) as f64);
                let full = self.screen
                    * transform
                    * Affine::new([
                        rect.width() / w0,
                        0.0,
                        0.0,
                        -rect.height() / h0,
                        rect.x0,
                        rect.y1,
                    ]);
                // Drawn much smaller than its pixels: from a reduced level
                // of the picture near the size drawn (#853).
                let level = bitmap_level(full);
                if let Some(img) = image_cache::decode_level(png, level) {
                    let m = if level == 0 {
                        full.as_coeffs()
                    } else {
                        let (lw, lh) = (img.width().max(1) as f64, img.height().max(1) as f64);
                        let img_to_local = Affine::new([
                            rect.width() / lw,
                            0.0,
                            0.0,
                            -rect.height() / lh,
                            rect.x0,
                            rect.y1,
                        ]);
                        (self.screen * transform * img_to_local).as_coeffs()
                    };
                    let mut paint = Paint::default();
                    paint.anti_alias = self.flags.anti_alias;
                    paint.blend_mode = self.blend_for(shape.overprint_fill);
                    // Upright: on whole pixels, so pictures laid side by
                    // side meet without a pale seam where both edges were
                    // half covered (#853).
                    let snapped = snap_upright(m, img.width(), img.height());
                    let (m, area) = match snapped {
                        Some((m, r)) => (m, Some(r)),
                        None => (m, None),
                    };
                    let t = Transform::from_row(
                        m[0] as f32,
                        m[1] as f32,
                        m[2] as f32,
                        m[3] as f32,
                        m[4] as f32,
                        m[5] as f32,
                    );
                    // A black and white (1-bit) picture blown up keeps
                    // its pixels' hard edges, black or white, as it
                    // prints; smoothed, its dots grew grey rims (#2922).
                    let quality = if level == 0
                        && png_is_black_white(png)
                        && m[0].hypot(m[1]).min(m[2].hypot(m[3])) > 1.0
                    {
                        tiny_skia::FilterQuality::Nearest
                    } else {
                        tiny_skia::FilterQuality::Bilinear
                    };
                    paint.shader = tiny_skia::Pattern::new(
                        Pixmap::as_ref(&img),
                        SpreadMode::Pad,
                        quality,
                        1.0,
                        t,
                    );
                    match area {
                        Some(None) => {}
                        Some(Some(r)) => {
                            paint.anti_alias = false;
                            self.pixmap
                                .fill_rect(r, &paint, Transform::identity(), None);
                        }
                        None => self.pixmap.fill_path(
                            &sk,
                            &paint,
                            FillRule::Winding,
                            Transform::identity(),
                            None,
                        ),
                    }
                }
            }
            if let Some(stroke) = &shape.stroke {
                let (mut paint, sk_stroke) = self.stroke_paint(stroke, transform);
                paint.blend_mode = self.blend_for(shape.overprint_outline);
                self.pixmap
                    .stroke_path(&sk, &paint, &sk_stroke, Transform::identity(), None);
            } else if self.flags.wireframe {
                let mut paint = Paint::default();
                paint.set_color_rgba8(0, 0, 0, 255);
                let s = SkStroke {
                    width: 0.0,
                    ..Default::default()
                };
                self.pixmap
                    .stroke_path(&sk, &paint, &s, Transform::identity(), None);
            }
            return;
        }

        // An outline set behind the fill: the outline first, then the
        // fill over it (half the outline's width shows outside).
        if shape.stroke.as_ref().is_some_and(|st| st.behind_fill)
            && !matches!(shape.fill, Fill::None)
        {
            let mut outline = shape.clone();
            outline.fill = Fill::None;
            if let Some(st) = outline.stroke.as_mut() {
                st.behind_fill = false;
            }
            let mut fill = shape.clone();
            fill.stroke = None;
            fill.shadow = None;
            outline.shadow = None;
            self.draw_shape(&outline, parent);
            self.draw_shape(&fill, parent);
            return;
        }

        if !self.flags.wireframe {
            let rule = shape_fill_rule(shape, self.flags.text_outline);
            let blend = self.blend_for(shape.overprint_fill);
            if let Fill::Mesh(m) = &shape.fill {
                // Mesh nodes are local: bring them to page space first.
                self.fill_mesh(&m.transformed(transform), &sk, rule, blend);
            } else if !self.flags.fill_open_curves
                // An ellipse made an arc (not a pie) is an open curve
                // too: not filled while Fill open curves is off.
                && shape.kind.is_open_curve()
                && !matches!(shape.fill, Fill::None)
            {
                // Only the closed subpaths of a curve get the fill.
                let closed = closed_subpaths(&page_path);
                if let Some(sk_fill) = to_sk_path(&(self.screen * closed.clone())) {
                    self.draw_fill_of(&shape.fill, &closed, &sk_fill, rule, blend, transform);
                }
            } else {
                self.draw_fill_of(&shape.fill, &page_path, &sk, rule, blend, transform);
            }
        }
        if let Some(stroke) = &shape.stroke {
            let (mut paint, sk_stroke) = self.stroke_paint(stroke, transform);
            paint.blend_mode = self.blend_for(shape.overprint_outline);
            if let Some(brush) = stroke.painterly.as_ref().filter(|_| !self.flags.wireframe) {
                // Painterly brush: its marks, each with its own opacity.
                let c = transform.as_coeffs();
                let scale = if stroke.scale_with_object {
                    ((c[0] * c[3] - c[1] * c[2]).abs()).sqrt().max(1e-6)
                } else {
                    1.0
                };
                let [r, g, b] = stroke.color.to_rgb8();
                // A pen's pressure along the stroke sets the marks' width
                // (#883).
                for mark in tracedraw_core::painterly::marks_along(
                    &page_path,
                    stroke.width * scale,
                    brush,
                    &stroke.widths,
                    stroke.width,
                ) {
                    let Some(mp) = to_sk_path(&(self.screen * mark.path)) else {
                        continue;
                    };
                    paint.set_color_rgba8(r, g, b, (mark.alpha * 255.0).round() as u8);
                    self.pixmap.fill_path(
                        &mp,
                        &paint,
                        FillRule::Winding,
                        Transform::identity(),
                        None,
                    );
                }
                return;
            }
            let object_scale = if stroke.scale_with_object {
                let c = transform.as_coeffs();
                ((c[0] * c[3] - c[1] * c[2]).abs()).sqrt().max(1e-6)
            } else {
                1.0
            };
            if let Some(pieces) = (!self.flags.wireframe)
                .then(|| {
                    tracedraw_core::shaping::along_slices(
                        &page_path,
                        stroke,
                        stroke.width * object_scale,
                    )
                })
                .flatten()
            {
                // A gradient along the line: each piece in its colour.
                for (piece, color, alpha) in pieces {
                    let Some(pp) = to_sk_path(&(self.screen * piece)) else {
                        continue;
                    };
                    let [r, g, b] = color.to_rgb8();
                    paint.set_color_rgba8(r, g, b, (alpha.clamp(0.0, 1.0) * 255.0).round() as u8);
                    self.pixmap.fill_path(
                        &pp,
                        &paint,
                        FillRule::Winding,
                        Transform::identity(),
                        None,
                    );
                }
                return;
            }
            if let Some(band) =
                tracedraw_core::shaping::outline_band(&page_path, stroke, object_scale)
            {
                // Calligraphic nib or variable width: fill the band instead
                // of stroking, with the outline's own fill when it has one.
                if let Some(bp) = to_sk_path(&(self.screen * band.clone())) {
                    match outline_paint(stroke, transform) {
                        Some(fill) => {
                            use tracedraw_core::geometry::Shape as _;
                            let b = bbox(&band).to_path(0.01);
                            self.draw_fill(&fill, &b, &bp, FillRule::Winding);
                        }
                        None => self.pixmap.fill_path(
                            &bp,
                            &paint,
                            FillRule::Winding,
                            Transform::identity(),
                            None,
                        ),
                    }
                }
            } else if let Some(fill) =
                outline_paint(stroke, transform).filter(|_| sk_stroke.width > 0.0)
            {
                // A filled outline: the stroke's area painted with the fill,
                // spread over the outline's bounds.
                let sk = headed_line(&page_path, stroke, self.screen).unwrap_or_else(|| sk.clone());
                if let Some(band) = sk.stroke(&sk_stroke, 1.0) {
                    use tracedraw_core::geometry::Shape as _;
                    let pad = (sk_stroke.width as f64 / self.zoom.max(1e-9)) / 2.0;
                    let b = bbox(&page_path).inflate(pad, pad);
                    self.draw_fill(&fill, &b.to_path(0.01), &band, FillRule::Winding);
                }
            } else {
                // Cut back under its arrowheads (#2981).
                let sk = headed_line(&page_path, stroke, self.screen).unwrap_or_else(|| sk.clone());
                self.pixmap
                    .stroke_path(&sk, &paint, &sk_stroke, Transform::identity(), None);
            }
            for head in tracedraw_core::style::arrowhead_paths(&page_path, stroke) {
                if let Some(hp) = to_sk_path(&(self.screen * head)) {
                    self.pixmap.fill_path(
                        &hp,
                        &paint,
                        FillRule::Winding,
                        Transform::identity(),
                        None,
                    );
                }
            }
        } else if self.flags.wireframe {
            let mut paint = Paint::default();
            paint.set_color_rgba8(0, 0, 0, 255);
            paint.anti_alias = self.flags.anti_alias;
            let s = SkStroke {
                width: 0.0,
                ..Default::default()
            };
            self.pixmap
                .stroke_path(&sk, &paint, &s, Transform::identity(), None);
        }
    }

    /// The drop shadow `sh` of `shape` drawn inside `parent`, whose
    /// page-space path is `page_path`, without the object: a picture's
    /// from its pixels, a group's or clip frame's from what it shows,
    /// any other object's from its path.
    fn draw_object_shadow(
        &mut self,
        sh: &tracedraw_core::Shadow,
        shape: &Shape,
        parent: Affine,
        page_path: &BezPath,
    ) {
        let drawn = match &shape.kind {
            ShapeKind::Group { .. } | ShapeKind::ClipFrame { .. } => {
                self.draw_group_shadow(sh, shape, parent);
                true
            }
            _ if page_path.elements().is_empty() => true,
            ShapeKind::Bitmap {
                rect,
                width_px,
                height_px,
                png,
                ..
            } => self.draw_bitmap_shadow(
                sh,
                page_path,
                parent * shape.transform,
                *rect,
                (*width_px, *height_px),
                png,
            ),
            _ => false,
        };
        if !drawn {
            let rule = shape_fill_rule(shape, self.flags.text_outline);
            self.draw_drop_shadow(sh, shape, parent, page_path, rule);
        }
    }

    /// The inside of a clip frame `clip` drawn inside `parent`: the
    /// frame's fill (`fill_only`, its outline and shadow taken off) and
    /// the contents over it, cut to the frame. Both are put together
    /// first and then cut by the frame's coverage in one step, so an edge
    /// pixel the frame half covers takes half of what the frame shows
    /// and half of what lies beneath. The fill was drawn with its own
    /// soft edge and the contents then blended over it by the same
    /// coverage again, which left a thin line of the fill's colour (or
    /// of a shadow beneath) all round a frame that did not sit on whole
    /// pixels.
    fn draw_clip_frame_inside(
        &mut self,
        clip: &Shape,
        fill_only: &Shape,
        contents: &[Shape],
        parent: Affine,
    ) {
        let (w, h) = (self.pixmap.width(), self.pixmap.height());
        let frame_path = self.screen * (parent * fill_only.page_path());
        let Some(sk) = to_sk_path(&frame_path) else {
            // No frame to cut to: its fill alone, as it is.
            self.draw_shape(fill_only, parent);
            return;
        };
        let b = sk.bounds();
        let x0 = (b.left().floor().max(0.0) as usize).min(w as usize);
        let x1 = (b.right().ceil().max(0.0) as usize).min(w as usize);
        let y0 = (b.top().floor().max(0.0) as usize).min(h as usize);
        let y1 = (b.bottom().ceil().max(0.0) as usize).min(h as usize);
        // The contents are put together on a layer covering only the
        // frame's box (with room for their shadows' blur), not the whole
        // canvas: a canvas-sized copy and mask for every clip frame made
        // a large drawing of many of them slow to draw at every zoom
        // (#2522), and the contents outside the box are then left out as
        // off screen. A lens samples what lies anywhere around it, so
        // with one inside the layer stays the whole canvas.
        let (mut lens, mut blur) = (false, 0.0f64);
        for c in contents {
            reach_walk(c, &mut lens, &mut blur);
        }
        let (lx0, ly0, lx1, ly1) = if lens {
            (0, 0, w as usize, h as usize)
        } else {
            let m = blur_margin(blur, self.zoom) as usize;
            let align = BAND_ALIGN as usize;
            (
                x0.saturating_sub(m) / align * align,
                y0.saturating_sub(m) / align * align,
                x1.saturating_add(m).min(w as usize),
                y1.saturating_add(m).min(h as usize),
            )
        };
        let (rw, rh) = (lx1.saturating_sub(lx0), ly1.saturating_sub(ly0));
        let mask = (|| {
            let mut mask = tiny_skia::Mask::new(rw as u32, rh as u32)?;
            let rule = shape_fill_rule(fill_only, false);
            mask.fill_path(
                &sk,
                rule,
                true,
                Transform::from_translate(-(lx0 as f32), -(ly0 as f32)),
            );
            Some(mask)
        })();
        let Some(mask) = mask else {
            // Nothing of the frame on the canvas: its fill alone, as it is.
            self.draw_shape(fill_only, parent);
            return;
        };
        // The fill on a layer of its own, only as large as it reaches.
        let (fx, fy, fw, fh) = self.layer_area(fill_only, parent);
        let fill_px = Pixmap::new(fw, fh).map(|mut px| {
            let mut sub = self.sub(&mut px);
            sub.screen = Affine::translate((-f64::from(fx), -f64::from(fy))) * self.screen;
            sub.origin = (self.origin.0 + fx as i32, self.origin.1 + fy as i32);
            sub.draw_shape(fill_only, parent);
            px
        });
        let (bw, w) = (x1.saturating_sub(x0), w as usize);
        // What lies beneath inside the frame's box, before the fill.
        let mut beneath = Vec::with_capacity(bw * (y1.saturating_sub(y0)) * 4);
        for y in y0..y1 {
            if let Some(row) = self.pixmap.data().get((y * w + x0) * 4..(y * w + x1) * 4) {
                beneath.extend_from_slice(row);
            }
        }
        if beneath.len() != bw * (y1.saturating_sub(y0)) * 4 {
            return;
        }
        // The fill as drawn, edge and all: where the frame does not reach
        // (the outlines of a group used as a frame) it stays so.
        if let Some(px) = &fill_px {
            self.pixmap.draw_pixmap(
                fx as i32,
                fy as i32,
                px.as_ref(),
                &tiny_skia::PixmapPaint::default(),
                Transform::identity(),
                None,
            );
        }
        // The contents are drawn over a copy of what lies beneath, so
        // their merge modes (Multiply on a photo, say) work on it as
        // outside a frame; inside the frame the copy then takes the place
        // of the page (#13). Inside the frame the copy holds the fill
        // without its soft edge (the frame's coverage gives the edge
        // once, at the end). The copy is of the layer's box only.
        let Some(mut layer) = Pixmap::new(rw as u32, rh as u32) else {
            return;
        };
        {
            let src = self.pixmap.data();
            let dst = layer.data_mut();
            for y in ly0..ly1 {
                let (s0, d0) = ((y * w + lx0) * 4, (y - ly0) * rw * 4);
                if let (Some(s), Some(d)) = (src.get(s0..s0 + rw * 4), dst.get_mut(d0..d0 + rw * 4))
                {
                    d.copy_from_slice(s);
                }
            }
        }
        // Index of canvas pixel (x, y) in the layer and its mask.
        let at = |x: usize, y: usize| (y - ly0) * rw + (x - lx0);
        let m = mask.data();
        let mut under = beneath.clone();
        {
            let fill = fill_px.as_ref().map(|p| (p.data(), p.width() as usize));
            let dst = layer.data_mut();
            for y in y0..y1 {
                for x in x0..x1 {
                    let i = at(x, y);
                    let k = m.get(i).copied().unwrap_or(0);
                    if k == 0 {
                        continue;
                    }
                    let j = ((y - y0) * bw + (x - x0)) * 4;
                    let f = fill.and_then(|(d, fw)| {
                        let (lx, ly) = (x.checked_sub(fx as usize)?, y.checked_sub(fy as usize)?);
                        if lx >= fw {
                            return None;
                        }
                        d.get((ly * fw + lx) * 4..(ly * fw + lx) * 4 + 4)
                    });
                    let mut f = match f {
                        Some(f) => [f[0] as f32, f[1] as f32, f[2] as f32, f[3] as f32],
                        None => [0.0; 4],
                    };
                    if k < 255 {
                        let s = 255.0 / k as f32;
                        f[3] = (f[3] * s).min(255.0);
                        for c in 0..3 {
                            f[c] = (f[c] * s).min(f[3]);
                        }
                    }
                    let keep = 1.0 - f[3] / 255.0;
                    for c in 0..4 {
                        let v = (f[c] + under[j + c] as f32 * keep)
                            .round()
                            .clamp(0.0, 255.0);
                        under[j + c] = v as u8;
                        if let Some(d) = dst.get_mut(i * 4 + c) {
                            *d = v as u8;
                        }
                    }
                }
            }
        }
        {
            let mut sub = self.sub(&mut layer);
            sub.screen = Affine::translate((-(lx0 as f64), -(ly0 as f64))) * self.screen;
            sub.origin = (self.origin.0 + lx0 as i32, self.origin.1 + ly0 as i32);
            let sight = sub.sight();
            for c in contents {
                if sight.is_some_and(|v| sub.out_of_sight_in(c, parent, &v)) {
                    continue;
                }
                let t = sub.child_transform(c, parent);
                sub.draw_shape(c, t);
            }
        }
        let opacity = clip.opacity.clamp(0.0, 1.0) as f32;
        let src = layer.data();
        let dst = self.pixmap.data_mut();
        for y in y0..y1 {
            for x in x0..x1 {
                let i = y * w + x;
                let li = at(x, y);
                let k = m.get(li).copied().unwrap_or(0) as f32 / 255.0;
                if k <= 0.0 {
                    continue;
                }
                let j = ((y - y0) * bw + (x - x0)) * 4;
                for c in 0..4 {
                    let (Some(s), Some(d)) = (src.get(li * 4 + c), dst.get_mut(i * 4 + c)) else {
                        continue;
                    };
                    let u = under[j + c] as f32;
                    let inside = u + (*s as f32 - u) * opacity;
                    let bg = beneath[j + c] as f32;
                    *d = (bg + (inside - bg) * k).round().clamp(0.0, 255.0) as u8;
                }
            }
        }
    }

    /// A drop shadow beneath an object whose page-space path is
    /// `page_path`: its silhouette moved (flat) or sheared (perspective),
    /// filled with the shadow colour, feathered, faded along a
    /// perspective shadow and merged with what is beneath. `rule` is the
    /// rule the object is filled by, so its holes are the shadow's.
    /// `shape` is the object, drawn inside `parent`: the shadow is of
    /// what it shows, its fill and its outline, and its layer reaches
    /// round all its outline may cover.
    fn draw_drop_shadow(
        &mut self,
        sh: &tracedraw_core::Shadow,
        shape: &Shape,
        parent: Affine,
        page_path: &BezPath,
        rule: FillRule,
    ) {
        let b = page_path.bounding_box();
        let extent = Self::reach(shape, parent, self.bounds).map_or(b, |r| r.union(b));
        let object = parent * shape.transform;
        let Some((layer_at, mut layer, screen)) = self.shadow_target_over(sh, b, extent) else {
            return;
        };
        let _screen = accel::on_screen(self.flags.draft);
        if !shadow_layer(
            &mut layer,
            sh,
            Shown {
                shape,
                path: page_path,
                object,
                rule,
            },
            screen,
            self.zoom,
            self.flags.anti_alias,
        ) {
            return;
        }
        self.merge_shadow(sh, &layer, layer_at);
    }

    /// The drop shadow of a picture: its silhouette is the picture's own
    /// alpha, placed as the picture is and moved as the shadow asks, so a
    /// transparent background casts no shadow (#2027). False when the
    /// picture cannot be decoded; the caller then shadows its box.
    fn draw_bitmap_shadow(
        &mut self,
        sh: &tracedraw_core::Shadow,
        page_path: &BezPath,
        transform: Affine,
        rect: Rect,
        (width_px, height_px): (u32, u32),
        png: &[u8],
    ) -> bool {
        let Some(img) = image_cache::decode_level(png, 0) else {
            return false;
        };
        let b = page_path.bounding_box();
        let Some((layer_at, mut layer, screen)) = self.shadow_target(sh, b) else {
            return true;
        };
        bitmap_silhouette(
            &mut layer,
            sh,
            b,
            screen,
            transform,
            rect,
            (width_px, height_px),
            &img,
        );
        let _screen = accel::on_screen(self.flags.draft);
        shade_shadow(&mut layer, sh, b, screen, self.zoom);
        self.merge_shadow(sh, &layer, layer_at);
        true
    }

    /// The layer a drop shadow measured on box `b` is painted on: only
    /// the pixels of the target it can reach (see [`shadow_pixels`]),
    /// with where it sits on the target and the page-to-layer map. A
    /// layer the size of the whole target made every frame of a shadow
    /// drag blur and shade the whole window (#935).
    fn shadow_target(
        &self,
        sh: &tracedraw_core::Shadow,
        b: Rect,
    ) -> Option<((i32, i32), Pixmap, Affine)> {
        self.shadow_target_over(sh, b, b)
    }

    /// [`Self::shadow_target`] for a silhouette that covers `extent`.
    fn shadow_target_over(
        &self,
        sh: &tracedraw_core::Shadow,
        b: Rect,
        extent: Rect,
    ) -> Option<((i32, i32), Pixmap, Affine)> {
        let (w, h) = (self.pixmap.width(), self.pixmap.height());
        let (x, y, lw, lh) = shadow_pixels_over(sh, b, extent, self.screen, self.zoom, w, h)?;
        let layer = Pixmap::new(lw, lh)?;
        let screen = Affine::translate((-f64::from(x), -f64::from(y))) * self.screen;
        Some(((x, y), layer, screen))
    }

    /// The drop shadow of a group or a clip frame drawn inside `parent`:
    /// the silhouette of what it shows (see [`group_shadow_layer`]). These
    /// returned before their own shadow was drawn, so a shadow given to a
    /// clip frame or a group never showed (#873).
    fn draw_group_shadow(&mut self, sh: &tracedraw_core::Shadow, shape: &Shape, parent: Affine) {
        let Some((b, extent)) = cast_boxes(shape, parent) else {
            return;
        };
        let Some((layer_at, mut layer, screen)) = self.shadow_target_over(sh, b, extent) else {
            return;
        };
        let _screen = accel::on_screen(self.flags.draft);
        group_shadow_layer(
            &mut layer,
            sh,
            shape,
            parent,
            b,
            screen,
            self.zoom,
            self.flags.anti_alias,
        );
        self.merge_shadow(sh, &layer, layer_at);
    }

    /// The inner shadow `ish` of `shape` drawn inside `parent`: what lies
    /// outside the object, moved by the shadow's offset, blurred by its
    /// feathering and kept where the object covers, in the shadow's
    /// colour and opacity, merged over what the object drew (#2473).
    fn draw_inner_shadow(&mut self, shape: &Shape, parent: Affine, ish: InnerShadow) {
        let opacity = if ish.opacity.is_finite() {
            ish.opacity.clamp(0.0, 1.0)
        } else {
            0.0
        };
        if opacity <= 0.0 {
            return;
        }
        let page_path = parent * shape.page_path();
        let Some(object) = to_sk_path(&(self.screen * page_path.clone())) else {
            return;
        };
        let (w, h) = (self.pixmap.width(), self.pixmap.height());
        let b = object.bounds();
        let clamp = |v: f32, max: u32| (v.max(0.0).min(max as f32)) as u32;
        let (x0, y0) = (
            clamp(b.left().floor() - 1.0, w),
            clamp(b.top().floor() - 1.0, h),
        );
        let (x1, y1) = (
            clamp(b.right().ceil() + 1.0, w),
            clamp(b.bottom().ceil() + 1.0, h),
        );
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let blur = if ish.blur.is_finite() {
            ish.blur.max(0.0)
        } else {
            0.0
        };
        // Blurs wider than the box blur's reach are drawn on a layer of
        // a fraction of the picture's pixels and blown up: the shadow
        // keeps its width in mm at every zoom. Its radius stopped at 64
        // pixels, so zoomed in close the shadow came out narrower at each
        // zoom step than the picture scaled meanwhile showed it, and the
        // drawing seemed to shake while zooming (#2895).
        let full = (blur * self.zoom).round().max(0.0);
        let k = if full.is_finite() {
            (full / f64::from(BLUR_REACH)).ceil().max(1.0)
        } else {
            1.0
        };
        let radius = if full.is_finite() {
            (full / k).round() as i32
        } else {
            0
        };
        // Room round the object for the blur to draw on what lies outside
        // (layer pixels).
        let pad = radius.max(0) as u32 * 3 + 2;
        let lw = (f64::from(x1 - x0) / k).ceil() as u32 + 2 * pad + 1;
        let lh = (f64::from(y1 - y0) / k).ceil() as u32 + 2 * pad + 1;
        let Some(mut layer) = Pixmap::new(lw, lh) else {
            return;
        };
        let to_layer = Affine::translate((f64::from(pad), f64::from(pad)))
            * Affine::scale(1.0 / k)
            * Affine::translate((-f64::from(x0), -f64::from(y0)));
        let rule = shape_fill_rule(shape, self.flags.text_outline);
        // Everything outside the object, moved by the offset, casts.
        layer.fill(tiny_skia::Color::WHITE);
        let moved = to_layer
            * self.screen
            * (Affine::translate((ish.offset.x, ish.offset.y)) * page_path.clone());
        if let Some(sk) = to_sk_path(&moved) {
            let mut clear = Paint::default();
            clear.anti_alias = self.flags.anti_alias;
            clear.blend_mode = tiny_skia::BlendMode::Clear;
            layer.fill_path(&sk, &clear, rule, Transform::identity(), None);
        }
        if radius > 0 {
            accel::box_blur(&mut layer, radius, 3);
        }
        // Kept where the object covers, at the picture's own pixels.
        let (ow, oh) = (x1 - x0, y1 - y0);
        let Some(mut mask) = tiny_skia::Mask::new(ow, oh) else {
            return;
        };
        let to_out = Affine::translate((-f64::from(x0), -f64::from(y0)));
        if let Some(sk) = to_sk_path(&(to_out * (self.screen * page_path))) {
            mask.fill_path(&sk, rule, self.flags.anti_alias, Transform::identity());
        }
        let Some(mut out) = Pixmap::new(ow, oh) else {
            return;
        };
        let [cr, cg, cb] = ish.color.to_rgb8();
        let alpha = layer_alpha_sampler(&layer, k, pad);
        // Spread chokes the feathered edge: the shadow is solid over that
        // share of it and fades over the rest.
        let keep = 1.0
            - if ish.spread.is_finite() {
                ish.spread.clamp(0.0, 0.99)
            } else {
                0.0
            };
        let data = mask.data();
        for (n, px) in out.pixels_mut().iter_mut().enumerate() {
            let m = data[n];
            if m == 0 {
                continue;
            }
            let (x, y) = ((n as u32) % ow, (n as u32) / ow);
            let cast = (alpha(x, y) / 255.0 / keep).min(1.0);
            let cover = if self.flags.atop {
                1.0
            } else {
                f64::from(m) / 255.0
            };
            let a = cast * cover * opacity;
            *px =
                tiny_skia::ColorU8::from_rgba(cr, cg, cb, (a * 255.0).round() as u8).premultiply();
        }
        if !self.flags.atop {
            composite_layer(self.pixmap, &out, (x0 as i32, y0 as i32), ish.merge);
            return;
        }
        // Each pixel keeps the coverage it had: the shadow lands on the
        // object only.
        let pw = self.pixmap.width() as usize;
        let before: Vec<u8> = (y0..y1)
            .flat_map(|y| {
                let row = &self.pixmap.pixels()[y as usize * pw..];
                (x0..x1).map(move |x| row[x as usize].alpha())
            })
            .collect();
        composite_layer(self.pixmap, &out, (x0 as i32, y0 as i32), ish.merge);
        let pixels = self.pixmap.pixels_mut();
        for (n, &was) in before.iter().enumerate() {
            let (x, y) = (x0 as usize + n % ow as usize, y0 as usize + n / ow as usize);
            let p = &mut pixels[y * pw + x];
            let now = p.alpha();
            if now == was {
                continue;
            }
            let k = if now == 0 {
                0.0
            } else {
                f32::from(was) / f32::from(now)
            };
            let c = |v: u8| ((f32::from(v) * k).round() as u8).min(was);
            *p = tiny_skia::PremultipliedColorU8::from_rgba(
                c(p.red()),
                c(p.green()),
                c(p.blue()),
                was,
            )
            .unwrap_or(*p);
        }
    }

    /// The stacked outer shadows and outer glows of `shape` (#1141),
    /// cast by `main`, what its effects leave of the object: each a drop
    /// shadow of its own; a glow does not move and spreads outside the
    /// edge only.
    fn draw_outer_shadows(&mut self, shape: &Shape, main: &Shape, parent: Affine) {
        use tracedraw_core::live::Effect;
        let mut caster = main.clone();
        caster.effects.clear();
        caster.shadow = None;
        caster.opacity = shape.opacity;
        let page_path = (parent * caster.transform) * caster.local_path();
        for e in &shape.effects {
            let sh = match e {
                Effect::OuterShadow {
                    offset,
                    color,
                    opacity,
                    blur,
                    merge,
                } => tracedraw_core::Shadow {
                    offset: *offset,
                    opacity: *opacity,
                    blur: *blur,
                    color: *color,
                    merge: *merge,
                    ..Default::default()
                },
                Effect::Glow {
                    inner: false,
                    color,
                    opacity,
                    blur,
                    merge,
                } => tracedraw_core::Shadow {
                    offset: tracedraw_core::geometry::Vec2::ZERO,
                    opacity: *opacity,
                    blur: *blur,
                    color: *color,
                    merge: *merge,
                    feather_direction: tracedraw_core::shadow::FeatherDirection::Outside,
                    ..Default::default()
                },
                _ => continue,
            };
            if sh.opacity.is_finite() && sh.opacity > 0.0 {
                self.draw_object_shadow(&sh, &caster, parent, &page_path);
            }
        }
    }

    /// `shape` drawn inside `parent` on a layer of its own, blurred by
    /// `radius` mm (three box passes), and merged over the target
    /// (#1141).
    fn draw_blurred(&mut self, shape: &Shape, parent: Affine, radius: f64) {
        let px = (radius * self.zoom).round().clamp(0.0, 64.0) as i32;
        // The layer covers the object and what its blur spreads over (3
        // passes of `px` each way, and one more), not the whole target:
        // a page-sized layer was made and blurred for each object
        // (#3442). Past that the blur of nothing is nothing, and at the
        // target's edge the layer ends there too, so each pass repeats
        // the same edge pixels as before.
        let (x, y, w, h) = self.layer_area_grown(shape, parent, f64::from(3 * px + 1));
        if w == 0 || h == 0 {
            return;
        }
        note_scratch(w, h);
        let Some(mut layer) = Pixmap::new(w, h) else {
            self.draw_shape(shape, parent);
            return;
        };
        {
            let mut sub = self.sub(&mut layer);
            sub.screen = Affine::translate((-f64::from(x), -f64::from(y))) * self.screen;
            sub.origin = (self.origin.0 + x as i32, self.origin.1 + y as i32);
            sub.draw_shape(shape, parent);
        }
        if px > 0 {
            accel::box_blur(&mut layer, px, 3);
        }
        composite_layer(
            self.pixmap,
            &layer,
            (x as i32, y as i32),
            tracedraw_core::live::MergeMode::Normal,
        );
    }

    /// Feather: the object drawn on its own and faded out towards its
    /// edge over `radius` mm: its pixels kept as much as a mask of its
    /// outline, shrunk by half the radius and blurred, covers them. An
    /// object with no fill, or too thin for the shrunk outline, is drawn
    /// as it is.
    fn draw_feathered(&mut self, shape: &Shape, parent: Affine, radius: f64) {
        use tracedraw_core::shaping::{offset_smooth, Join};
        if matches!(shape.fill, tracedraw_core::Fill::None) {
            self.draw_shape(shape, parent);
            return;
        }
        let (x, y, w, h) = self.layer_area(shape, parent);
        if w == 0 || h == 0 {
            return;
        }
        let inset = offset_smooth(
            &(parent * shape.page_path()),
            -radius / 2.0,
            0.0,
            Join::Round,
            shape.fill_rule,
        );
        let to_layer = Affine::translate((-f64::from(x), -f64::from(y))) * self.screen;
        let sk = if inset.elements().is_empty() {
            None
        } else {
            to_sk_path(&(to_layer * inset))
        };
        note_scratch(w, h);
        let (Some(sk), Some(mut layer), Some(mut mask)) = (sk, Pixmap::new(w, h), Pixmap::new(w, h))
        else {
            self.draw_shape(shape, parent);
            return;
        };
        {
            let mut sub = self.sub(&mut layer);
            sub.screen = to_layer;
            sub.origin = (self.origin.0 + x as i32, self.origin.1 + y as i32);
            sub.draw_shape(shape, parent);
        }
        let mut paint = tiny_skia::Paint::default();
        paint.set_color_rgba8(0, 0, 0, 255);
        paint.anti_alias = self.flags.anti_alias;
        mask.fill_path(&sk, &paint, FillRule::Winding, Transform::identity(), None);
        let px = (radius * self.zoom / 6.0).round().clamp(0.0, 64.0) as i32;
        if px > 0 {
            accel::box_blur(&mut mask, px, 3);
        }
        for (d, m) in layer.data_mut().chunks_exact_mut(4).zip(mask.data().chunks_exact(4)) {
            let a = u16::from(m[3]);
            for c in d.iter_mut() {
                *c = ((u16::from(*c) * a + 127) / 255) as u8;
            }
        }
        composite_layer(
            self.pixmap,
            &layer,
            (x as i32, y as i32),
            tracedraw_core::live::MergeMode::Normal,
        );
    }

    /// Blend If: the object drawn on a layer of its own, each pixel kept
    /// as much as its lightness falls in `this` and the lightness of the
    /// target beneath it (over white paper where nothing is drawn yet)
    /// falls in `below`, then laid over the target.
    fn draw_blend_if(&mut self, shape: &Shape, parent: Affine, this: &[f64; 4], below: &[f64; 4]) {
        let (x, y, w, h) = self.layer_area_grown(shape, parent, 1.0);
        if w == 0 || h == 0 {
            return;
        }
        note_scratch(w, h);
        let Some(mut layer) = Pixmap::new(w, h) else {
            self.draw_shape(shape, parent);
            return;
        };
        {
            let mut sub = self.sub(&mut layer);
            sub.screen = Affine::translate((-f64::from(x), -f64::from(y))) * self.screen;
            sub.origin = (self.origin.0 + x as i32, self.origin.1 + y as i32);
            sub.draw_shape(shape, parent);
        }
        let tw = self.pixmap.width() as usize;
        let back = self.pixmap.pixels();
        let (x0, y0) = (x as usize, y as usize);
        let lw = w as usize;
        for (i, px) in layer.pixels_mut().iter_mut().enumerate() {
            let a = px.alpha();
            if a == 0 {
                continue;
            }
            let fa = f64::from(a);
            let own = tracedraw_core::live::blend_if_lightness(
                f64::from(px.red()) / fa,
                f64::from(px.green()) / fa,
                f64::from(px.blue()) / fa,
            );
            let (bx, by) = (x0 + i % lw, y0 + i / lw);
            let under = back.get(by * tw + bx).map_or(1.0, |b| {
                let paper = 255.0 - f64::from(b.alpha());
                tracedraw_core::live::blend_if_lightness(
                    (f64::from(b.red()) + paper) / 255.0,
                    (f64::from(b.green()) + paper) / 255.0,
                    (f64::from(b.blue()) + paper) / 255.0,
                )
            });
            let k = tracedraw_core::live::blend_if_weight(this, own)
                * tracedraw_core::live::blend_if_weight(below, under);
            if k >= 1.0 {
                continue;
            }
            let scale = |c: u8| (f64::from(c) * k).round().clamp(0.0, 255.0) as u8;
            let (r, g, b, a) = (scale(px.red()), scale(px.green()), scale(px.blue()), scale(a));
            *px = tiny_skia::PremultipliedColorU8::from_rgba(r.min(a), g.min(a), b.min(a), a)
                .unwrap_or(tiny_skia::PremultipliedColorU8::TRANSPARENT);
        }
        composite_layer(
            self.pixmap,
            &layer,
            (x as i32, y as i32),
            tracedraw_core::live::MergeMode::Normal,
        );
    }

    /// Displace's shading (#3595): the object drawn on a layer of its own,
    /// each pixel's colour multiplied down where the target beneath it
    /// (over white paper) is darker than the photo's middle grey `mean`
    /// and screened up where it is lighter, by `strength`, then laid over
    /// the target.
    fn draw_surface_shaded(
        &mut self,
        shape: &Shape,
        parent: Affine,
        shade: tracedraw_core::live::SurfaceShade,
        strength: f64,
        mean: f64,
    ) {
        let (x, y, w, h) = self.layer_area_grown(shape, parent, 1.0);
        if w == 0 || h == 0 {
            return;
        }
        note_scratch(w, h);
        let Some(mut layer) = Pixmap::new(w, h) else {
            self.draw_shape(shape, parent);
            return;
        };
        {
            let mut sub = self.sub(&mut layer);
            sub.screen = Affine::translate((-f64::from(x), -f64::from(y))) * self.screen;
            sub.origin = (self.origin.0 + x as i32, self.origin.1 + y as i32);
            sub.draw_shape(shape, parent);
        }
        let tw = self.pixmap.width() as usize;
        let back = self.pixmap.pixels();
        let (x0, y0) = (x as usize, y as usize);
        let lw = w as usize;
        for (i, px) in layer.pixels_mut().iter_mut().enumerate() {
            let a = px.alpha();
            if a == 0 {
                continue;
            }
            let (bx, by) = (x0 + i % lw, y0 + i / lw);
            let Some(b) = back.get(by * tw + bx) else {
                continue;
            };
            let paper = 255.0 - f64::from(b.alpha());
            let under = tracedraw_core::live::blend_if_lightness(
                (f64::from(b.red()) + paper) / 255.0,
                (f64::from(b.green()) + paper) / 255.0,
                (f64::from(b.blue()) + paper) / 255.0,
            );
            let (mul, scr) = tracedraw_core::live::surface_shade(shade, strength, mean, under);
            if mul >= 1.0 && scr <= 0.0 {
                continue;
            }
            // Premultiplied: c' = c * mul, then screened as a + (A - a) * scr.
            let fa = f64::from(a);
            let tone = |c: u8| {
                let c = f64::from(c) * mul;
                (c + (fa - c) * scr).round().clamp(0.0, fa) as u8
            };
            let (r, g, bl) = (tone(px.red()), tone(px.green()), tone(px.blue()));
            *px = tiny_skia::PremultipliedColorU8::from_rgba(r, g, bl, a)
                .unwrap_or(tiny_skia::PremultipliedColorU8::TRANSPARENT);
        }
        composite_layer(
            self.pixmap,
            &layer,
            (x as i32, y as i32),
            tracedraw_core::live::MergeMode::Normal,
        );
    }

    /// `shape` (its Halftone taken off) drawn on a layer of its own with
    /// all it reaches (shadows, glows), and the layer turned into the
    /// halftone `ht` before it goes onto the target.
    fn draw_halftoned(&mut self, shape: &Shape, parent: Affine, ht: &tracedraw_core::live::Effect) {
        let tracedraw_core::live::Effect::Halftone {
            pattern,
            mode,
            size,
            scale,
            softness,
            angle,
            center_x,
            center_y,
            clip,
            color,
        } = ht
        else {
            return self.draw_shape(shape, parent);
        };
        let (mut lens, mut reach) = (false, 0.0);
        reach_walk(shape, &mut lens, &mut reach);
        let grow = ((reach + size.abs()) * self.zoom).ceil().clamp(0.0, 4096.0) + 2.0;
        let (x, y, w, h) = self.layer_area_grown(shape, parent, grow);
        if w == 0 || h == 0 {
            return;
        }
        note_scratch(w, h);
        let Some(mut layer) = Pixmap::new(w, h) else {
            return self.draw_shape(shape, parent);
        };
        let to_layer = Affine::translate((-f64::from(x), -f64::from(y))) * self.screen;
        {
            let mut sub = self.sub(&mut layer);
            sub.screen = to_layer;
            sub.origin = (self.origin.0 + x as i32, self.origin.1 + y as i32);
            sub.flags.atop = true;
            sub.draw_shape(shape, parent);
        }
        // The screen's centre across the object's bounds, and its angle,
        // both in the layer's pixels (y down there).
        let b = (parent * shape.page_path()).bounding_box();
        let c = to_layer
            * tracedraw_core::geometry::Point::new(
                b.x0 + center_x.clamp(0.0, 1.0) * b.width(),
                b.y0 + center_y.clamp(0.0, 1.0) * b.height(),
            );
        let rgb = color.to_rgb8();
        halftone::halftone(
            &mut layer,
            &halftone::Params {
                pattern: *pattern,
                mode: *mode,
                cell: size * self.zoom,
                scale: *scale,
                softness: *softness,
                angle: -angle.to_radians(),
                origin: (c.x, c.y),
                clip: *clip,
                color: [rgb[0], rgb[1], rgb[2]],
            },
        );
        composite_layer(
            self.pixmap,
            &layer,
            (x as i32, y as i32),
            tracedraw_core::live::MergeMode::Normal,
        );
    }

    /// Merge a finished shadow layer, its top left at `at` on the
    /// target, with what lies beneath.
    fn merge_shadow(&mut self, sh: &tracedraw_core::Shadow, layer: &Pixmap, at: (i32, i32)) {
        composite_layer(self.pixmap, layer, at, sh.merge);
    }

    /// The contents of a clip frame in the Wireframe view: their
    /// hairlines on a layer the size of the frame, cut to the frame.
    /// A frame with no area to cut to shows them whole.
    fn draw_wireframe_contents(&mut self, frame: &Shape, contents: &[Shape], parent: Affine) {
        if contents.is_empty() {
            return;
        }
        let (x, y, w, h) = self.layer_area(frame, parent);
        let to_layer = Affine::translate((-f64::from(x), -f64::from(y))) * self.screen;
        let cut = to_sk_path(&(to_layer * (parent * frame.page_path()))).and_then(|sk| {
            let mut mask = tiny_skia::Mask::new(w, h)?;
            mask.fill_path(
                &sk,
                shape_fill_rule(frame, false),
                self.flags.anti_alias,
                Transform::identity(),
            );
            Some(mask)
        });
        let (Some(mask), Some(mut layer)) = (cut, Pixmap::new(w, h)) else {
            // Nothing of the frame in sight, or no outline to cut to.
            if w > 0 && h > 0 {
                for c in contents {
                    let t = self.child_transform(c, parent);
                    self.draw_shape(c, t);
                }
            }
            return;
        };
        {
            let mut sub = self.sub(&mut layer);
            sub.screen = to_layer;
            sub.origin = (self.origin.0 + x as i32, self.origin.1 + y as i32);
            for c in contents {
                let t = sub.child_transform(c, parent);
                sub.draw_shape(c, t);
            }
        }
        layer.apply_mask(&mask);
        self.pixmap.draw_pixmap(
            x as i32,
            y as i32,
            layer.as_ref(),
            &tiny_skia::PixmapPaint::default(),
            Transform::identity(),
            None,
        );
    }

    /// A one pixel line along `sk` (the Wireframe view).
    fn hairline(&mut self, sk: &tiny_skia::Path, color: tracedraw_core::Color) {
        let mut paint = Paint::default();
        let [r, g, b] = color.to_rgb8();
        paint.set_color_rgba8(r, g, b, 255);
        paint.anti_alias = self.flags.anti_alias;
        let s = SkStroke {
            width: 0.0,
            ..Default::default()
        };
        self.pixmap
            .stroke_path(sk, &paint, &s, Transform::identity(), None);
    }

    /// Lens: transform the pixels already drawn beneath the shape's area.
    fn draw_lens(&mut self, shape: &Shape, parent: Affine, lens: &tracedraw_core::live::Lens) {
        use tracedraw_core::live::Lens;
        let transform = parent * shape.transform;
        let path = self.screen * (transform * shape.local_path());
        let Some(sk) = to_sk_path(&path) else {
            return;
        };
        let (w, h) = (self.pixmap.width(), self.pixmap.height());
        let b = path.bounding_box();
        let center = b.center();
        let (x0, y0) = (b.x0.max(0.0) as u32, b.y0.max(0.0) as u32);
        let (x1, y1) = ((b.x1.ceil() as u32).min(w), (b.y1.ceil() as u32).min(h));
        if x1 <= x0 || y1 <= y0 {
            self.draw_lens_outline(shape, parent);
            return;
        }
        // Only the pixels under the lens's box are worked on: the mask,
        // what lies beneath (made opaque over white once) and the result
        // are that size. The whole page was copied and masked for every
        // lens, and each pixel read made opaque again 4 or 5 times (#3439).
        let bw = x1 - x0;
        // The mask reaches a little past the box, so the whole outline
        // lies inside it and is filled exactly as on a page-sized mask.
        // An outline that crosses the view's edge is cut there, and that
        // cut only gives the same edge pixels when the mask starts where
        // the view does, so then the mask starts at the view's corner.
        const PAD: u32 = 32;
        let crosses = b.x0 < 0.0 || b.y0 < 0.0 || b.x1 > w as f64 || b.y1 > h as f64;
        let (mx0, my0) = if crosses {
            (0, 0)
        } else {
            (x0.saturating_sub(PAD), y0.saturating_sub(PAD))
        };
        let (mw, mh) = ((x1 + PAD).min(w) - mx0, (y1 + PAD).min(h) - my0);
        note_scratch(mw, mh);
        let Some(mut mask) = tiny_skia::Mask::new(mw, mh) else {
            return;
        };
        let rule = shape_fill_rule(shape, false);
        mask.fill_path(
            &sk,
            rule,
            true,
            Transform::from_translate(-(mx0 as f32), -(my0 as f32)),
        );
        let mdata = mask.data();
        let coverage = |x: u32, y: u32| mdata[((y - my0) * mw + (x - mx0)) as usize];
        // A pixel of the page made opaque over white (the page).
        let opaque = |p: tiny_skia::PremultipliedColorU8| -> [f32; 3] {
            let p = p.demultiply();
            if p.alpha() == 0 {
                return [1.0, 1.0, 1.0];
            }
            let a = p.alpha() as f32 / 255.0;
            [
                p.red() as f32 / 255.0 * a + (1.0 - a),
                p.green() as f32 / 255.0 * a + (1.0 - a),
                p.blue() as f32 / 255.0 * a + (1.0 - a),
            ]
        };
        let page = self.pixmap.pixels();
        let under: Vec<[f32; 3]> = (y0..y1)
            .flat_map(|y| (x0..x1).map(move |x| (x, y)))
            .map(|(x, y)| opaque(page[(y * w + x) as usize]))
            .collect();
        // What lies beneath: inside the box from the copy made before any
        // pixel changes, outside it from the page, which the lens leaves
        // as it is.
        let src_px = |x: i64, y: i64| -> [f32; 3] {
            if x < 0 || y < 0 || x >= w as i64 || y >= h as i64 {
                return [1.0, 1.0, 1.0];
            }
            let (x, y) = (x as u32, y as u32);
            if x >= x0 && x < x1 && y >= y0 && y < y1 {
                under[((y - y0) * bw + (x - x0)) as usize]
            } else {
                opaque(page[(y * w + x) as usize])
            }
        };
        // What lies beneath at a point between pixels, blended from the
        // four around it, so a magnified scene is not blocky (#2800).
        let smooth_px = |fx: f64, fy: f64| -> [f32; 3] {
            let (fx, fy) = (fx - 0.5, fy - 0.5);
            let (x0, y0) = (fx.floor(), fy.floor());
            let (tx, ty) = ((fx - x0) as f32, (fy - y0) as f32);
            let (xi, yi) = (x0 as i64, y0 as i64);
            let a = src_px(xi, yi);
            let b = src_px(xi + 1, yi);
            let c = src_px(xi, yi + 1);
            let d = src_px(xi + 1, yi + 1);
            let mut out = [0.0f32; 3];
            for i in 0..3 {
                let top = a[i] + (b[i] - a[i]) * tx;
                let bottom = c[i] + (d[i] - c[i]) * tx;
                out[i] = top + (bottom - top) * ty;
            }
            out
        };
        // The largest difference of two colours' channels.
        let differ = |p: [f32; 3], q: [f32; 3]| -> f32 {
            (p[0] - q[0])
                .abs()
                .max((p[1] - q[1]).abs())
                .max((p[2] - q[2]).abs())
        };
        // Bitmap Effects lens: the effects run over what lies beneath (with
        // a margin, so blurs and the like see past the edge), then the
        // result shows inside the lens.
        let filtered: Option<(u32, u32, u32, Vec<u8>)> = match lens {
            Lens::Effects { effects } => PIXEL_EFFECTS.get().and_then(|run| {
                let m = 24;
                let (rx0, ry0) = (x0.saturating_sub(m), y0.saturating_sub(m));
                let (rx1, ry1) = ((x1 + m).min(w), (y1 + m).min(h));
                let (rw, rh) = (rx1.checked_sub(rx0)?, ry1.checked_sub(ry0)?);
                if rw == 0 || rh == 0 {
                    return None;
                }
                let mut buf = Vec::with_capacity((rw * rh * 4) as usize);
                for y in ry0..ry1 {
                    for x in rx0..rx1 {
                        let c = src_px(x as i64, y as i64);
                        buf.extend_from_slice(&[
                            (c[0] * 255.0).round() as u8,
                            (c[1] * 255.0).round() as u8,
                            (c[2] * 255.0).round() as u8,
                            255,
                        ]);
                    }
                }
                run(effects, rw, rh, &mut buf);
                (buf.len() == (rw * rh * 4) as usize).then_some((rx0, ry0, rw, buf))
            }),
            _ => None,
        };
        // Cut lens: what it needs once, before the pixels (the cut line on
        // the screen, the pieces it crosses and their spurs' colours).
        let cut = match lens {
            Lens::Cut { .. } => {
                let m = self.screen * transform;
                CutLens::new(lens, m, shape.local_path().bounding_box(), &smooth_px)
            }
            _ => None,
        };
        let mut changed: Vec<Option<tiny_skia::PremultipliedColorU8>> = vec![None; under.len()];
        for y in y0..y1 {
            for x in x0..x1 {
                let at = ((y - y0) * bw + (x - x0)) as usize;
                let m = coverage(x, y);
                if m == 0 {
                    continue;
                }
                let c = match lens {
                    Lens::Effects { .. } => match &filtered {
                        Some((rx0, ry0, rw, buf)) => {
                            let i = (((y - ry0) * rw + (x - rx0)) * 4) as usize;
                            match buf.get(i..i + 3) {
                                Some(p) => [
                                    p[0] as f32 / 255.0,
                                    p[1] as f32 / 255.0,
                                    p[2] as f32 / 255.0,
                                ],
                                None => src_px(x as i64, y as i64),
                            }
                        }
                        None => src_px(x as i64, y as i64),
                    },
                    Lens::Cut { .. } => match &cut {
                        Some(cut) => cut.color(x, y, src_px(x as i64, y as i64)),
                        None => src_px(x as i64, y as i64),
                    },
                    Lens::FishEye { rate } => {
                        let r = (*rate / 1000.0).clamp(-0.95, 10.0);
                        let dx = x as f64 - center.x;
                        let dy = y as f64 - center.y;
                        let radius = (b.width().min(b.height()) / 2.0).max(1.0);
                        let d = (dx * dx + dy * dy).sqrt() / radius;
                        let k = if d > 0.0 { d.powf(1.0 + r) / d } else { 1.0 };
                        smooth_px(center.x + dx * k, center.y + dy * k)
                    }
                    Lens::Magnify { amount } => {
                        let a = amount.max(1.0);
                        let dx = (x as f64 - center.x) / a;
                        let dy = (y as f64 - center.y) / a;
                        smooth_px(center.x + dx, center.y + dy)
                    }
                    Lens::Wireframe { outline, fill } => {
                        // The objects beneath as a wireframe (#2800): their
                        // edges in the outline colour, their insides in the
                        // fill colour, the bare page left as it is.
                        let rgb = |c: &tracedraw_core::Color| {
                            let [r, g, bl] = c.to_rgb8();
                            [r as f32 / 255.0, g as f32 / 255.0, bl as f32 / 255.0]
                        };
                        let here = src_px(x as i64, y as i64);
                        let (xi, yi) = (x as i64, y as i64);
                        let edge = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                            .iter()
                            .any(|(ox, oy)| differ(here, src_px(xi + ox, yi + oy)) > 0.12);
                        if edge {
                            rgb(outline)
                        } else if differ(here, [1.0, 1.0, 1.0]) > 0.02 {
                            rgb(fill)
                        } else {
                            here
                        }
                    }
                    other => other.map_color(src_px(x as i64, y as i64)),
                };
                let mf = m as f32 / 255.0;
                let old = under[at];
                let out = [
                    old[0] + (c[0] - old[0]) * mf,
                    old[1] + (c[1] - old[1]) * mf,
                    old[2] + (c[2] - old[2]) * mf,
                ];
                if let Some(px) = tiny_skia::ColorU8::from_rgba(
                    (out[0].clamp(0.0, 1.0) * 255.0).round() as u8,
                    (out[1].clamp(0.0, 1.0) * 255.0).round() as u8,
                    (out[2].clamp(0.0, 1.0) * 255.0).round() as u8,
                    255,
                )
                .premultiply()
                .into()
                {
                    changed[at] = Some(px);
                }
            }
        }
        let pixels = self.pixmap.pixels_mut();
        for (i, px) in changed.into_iter().enumerate() {
            if let Some(px) = px {
                let (x, y) = (x0 + i as u32 % bw, y0 + i as u32 / bw);
                pixels[(y * w + x) as usize] = px;
            }
        }
        self.draw_lens_outline(shape, parent);
    }

    /// The lens object's own outline.
    fn draw_lens_outline(&mut self, shape: &Shape, parent: Affine) {
        if let Some(stroke) = &shape.stroke {
            let mut outline = shape.clone();
            outline.fill = Fill::None;
            outline.stroke = Some(stroke.clone());
            self.draw_shape(&outline, parent);
        }
    }

    /// Non-uniform transparency: render the shape to a layer, multiply its
    /// alpha by the luminance of the mask fill, composite with a blend mode.
    fn draw_with_mask(
        &mut self,
        shape: &Shape,
        parent: Affine,
        mask_fill: &Fill,
        merge: tracedraw_core::live::MergeMode,
    ) {
        // The drop shadow goes beneath, outside the mask, cast by the
        // object's shape. Drawn on the layer it went through the mask,
        // which past the object's box repeats its edge pixels: a shadow
        // cut by a hard line, faded above it and dark below (an object
        // with a fountain transparency).
        let shadowless;
        let shape = match shape.shadow.as_ref() {
            Some(sh) => {
                if sh.opacity > 0.0 && shape.visible && shape.opacity > 0.0 && !self.flags.wireframe
                {
                    let page_path = (parent * shape.transform) * shape.local_path();
                    self.draw_object_shadow(sh, shape, parent, &page_path);
                }
                let mut s = shape.clone();
                s.shadow = None;
                shadowless = s;
                &shadowless
            }
            None => shape,
        };
        // The layer and the mask cover only what the object can reach,
        // not the whole target: two page-sized buffers and a third for
        // the mask's values were made for each such object (#3441).
        let (ox, oy, w, h) = self.layer_area(shape, parent);
        if w == 0 || h == 0 {
            return;
        }
        note_scratch(w, h);
        let (Some(mut layer), Some(mut mask_pm)) = (Pixmap::new(w, h), Pixmap::new(w, h)) else {
            return;
        };
        let screen = Affine::translate((-(ox as f64), -(oy as f64))) * self.screen;
        let origin = (self.origin.0 + ox as i32, self.origin.1 + oy as i32);
        {
            let mut sub = self.sub(&mut layer);
            sub.screen = screen;
            sub.origin = origin;
            sub.draw_shape(shape, parent);
        }
        let transform = parent * shape.transform;
        let page_path = transform * shape.local_path();
        let bounds = page_path.bounding_box();
        let rect_path = tracedraw_core::geometry::rect_path(bounds, 0.0);
        if let Some(sk) = to_sk_path(&(screen * rect_path.clone())) {
            let mut sub = self.sub(&mut mask_pm);
            sub.screen = screen;
            sub.origin = origin;
            sub.flags.wireframe = false;
            match mask_fill {
                // A shaped mask follows the object, over its whole bounds.
                Fill::Fountain(f) if f.kind == tracedraw_core::FountainKind::Shaped => {
                    if let Some(shape_sk) = to_sk_path(&(screen * page_path.clone())) {
                        let rule = shape_fill_rule(shape, false);
                        sub.fill_shaped(f, &shape_sk, &sk, rule);
                    }
                }
                // A pattern mask set to transform with its object follows it.
                _ => sub.draw_fill_of(
                    mask_fill,
                    &rect_path,
                    &sk,
                    FillRule::Winding,
                    tiny_skia::BlendMode::SourceOver,
                    transform,
                ),
            }
        }
        let lum = |p: &tiny_skia::PremultipliedColorU8| {
            let d = p.demultiply();
            if d.alpha() == 0 {
                255
            } else {
                ((d.red() as u32 * 54 + d.green() as u32 * 183 + d.blue() as u32 * 19) / 256) as u8
            }
        };
        let mask_px = mask_pm.pixels();
        // The mask is drawn over the object's box; past it (where the
        // outer half of an outline lies) it goes on as it is at the box's
        // edge, the nearest pixel wholly inside the box. Taken as no
        // transparency there, the outline of a rounded rectangle showed
        // opaque outside the box and faded inside it, so most of it
        // seemed to stay and its corners to vanish (live test of 0.41).
        let sb = (screen * rect_path).bounding_box();
        let inner = |lo: f64, hi: f64, n: u32| -> Option<(usize, usize)> {
            let (a, b) = (lo.ceil().max(0.0), (hi.floor() - 1.0).min(n as f64 - 1.0));
            (a.is_finite() && b.is_finite() && a <= b).then_some((a as usize, b as usize))
        };
        // Each pixel's mask value is read as the layer is worked on, with
        // no third buffer of them.
        let inside = inner(sb.x0, sb.x1, w).zip(inner(sb.y0, sb.y1, h));
        let mask_at = |i: usize| -> u8 {
            match inside {
                Some(((x0, x1), (y0, y1))) => {
                    let (x, y) = (i % w as usize, i / w as usize);
                    let (cx, cy) = (x.clamp(x0, x1), y.clamp(y0, y1));
                    mask_px.get(cy * w as usize + cx).map_or(255, lum)
                }
                None => mask_px.get(i).map_or(255, lum),
            }
        };
        // A uniform transparency (a solid mask) is one value over the whole
        // box: read once, not once a pixel, and a wholly opaque one leaves
        // the layer as it is (#3761).
        let constant = match (mask_fill, inside) {
            (Fill::Solid(_), Some(((x0, _), (y0, _)))) => {
                Some(mask_px.get(y0 * w as usize + x0).map_or(255, lum))
            }
            _ => None,
        };
        for (i, px) in layer.pixels_mut().iter_mut().enumerate() {
            if constant == Some(255) {
                break;
            }
            if px.alpha() == 0 {
                continue;
            }
            let m = constant.unwrap_or_else(|| mask_at(i));
            let d = px.demultiply();
            let a = (d.alpha() as u32 * m as u32 / 255) as u8;
            if let Some(np) = tiny_skia::ColorU8::from_rgba(d.red(), d.green(), d.blue(), a)
                .premultiply()
                .into()
            {
                *px = np;
            }
        }
        composite_layer(self.pixmap, &layer, (ox as i32, oy as i32), merge);
    }

    /// Dither the pixels the screen-space path `sk` covers: each colour
    /// channel moved up or down by up to two levels, by an even pattern
    /// fixed to the page (the same wherever the view is drawn in bands).
    fn dither_area(&mut self, sk: &SkPath, rule: FillRule) {
        let (w, h) = (self.pixmap.width(), self.pixmap.height());
        let Some(mut mask) = tiny_skia::Mask::new(w, h) else {
            return;
        };
        mask.fill_path(sk, rule, self.flags.anti_alias, Transform::identity());
        let (ox, oy) = self.origin;
        let m = mask.data();
        let data = self.pixmap.data_mut();
        for y in 0..h as usize {
            for x in 0..w as usize {
                let i = y * w as usize + x;
                let cover = m[i];
                if cover == 0 {
                    continue;
                }
                let d = &mut data[i * 4..i * 4 + 4];
                let a = i32::from(d[3]);
                if a == 0 {
                    continue;
                }
                // A hash of the page pixel: an even noise from -2 to 2.
                let (px, py) = ((x as i32 + ox) as u32, (y as i32 + oy) as u32);
                let mut z = px.wrapping_mul(0x8DA6_B343) ^ py.wrapping_mul(0xD816_3841);
                z ^= z >> 13;
                z = z.wrapping_mul(0x9E37_79B1);
                z ^= z >> 16;
                let n = ((z & 0xFF) as i32 - 128) * 2 * i32::from(cover) / (128 * 255);
                for c in d.iter_mut().take(3) {
                    *c = (i32::from(*c) + n).clamp(0, a) as u8;
                }
            }
        }
    }

    /// Fill a screen-space path with any fill type.
    fn draw_fill(&mut self, fill: &Fill, page_path: &BezPath, sk: &SkPath, rule: FillRule) {
        self.draw_fill_blended(fill, page_path, sk, rule, tiny_skia::BlendMode::SourceOver);
    }

    /// `draw_fill` with a blend mode. Solid fills blend directly; the other
    /// fill types render into a scratch layer that is then composited with
    /// the mode, so every fill type can overprint.
    fn draw_fill_blended(
        &mut self,
        fill: &Fill,
        page_path: &BezPath,
        sk: &SkPath,
        rule: FillRule,
        blend: tiny_skia::BlendMode,
    ) {
        self.draw_fill_of(fill, page_path, sk, rule, blend, Affine::IDENTITY);
    }

    /// `draw_fill_blended` for the fill of an object drawn with
    /// `object` (its transform with its parents'): a pattern set to
    /// transform with its object follows it.
    fn draw_fill_of(
        &mut self,
        fill: &Fill,
        page_path: &BezPath,
        sk: &SkPath,
        rule: FillRule,
        blend: tiny_skia::BlendMode,
        object: Affine,
    ) {
        // A dithered fountain: drawn as it is, then a faint even noise
        // over what it covers, so close colours show no steps.
        if let Fill::Fountain(f) = fill {
            if f.dither {
                let mut plain = f.clone();
                plain.dither = false;
                self.draw_fill_of(&Fill::Fountain(plain), page_path, sk, rule, blend, object);
                self.dither_area(sk, rule);
                return;
            }
        }
        if blend != tiny_skia::BlendMode::SourceOver && !matches!(fill, Fill::Solid(_)) {
            let (w, h) = (self.pixmap.width(), self.pixmap.height());
            let Some(mut layer) = Pixmap::new(w, h) else {
                return;
            };
            {
                let mut sub = self.sub(&mut layer);
                sub.draw_fill_of(
                    fill,
                    page_path,
                    sk,
                    rule,
                    tiny_skia::BlendMode::SourceOver,
                    object,
                );
            }
            let pp = tiny_skia::PixmapPaint {
                blend_mode: blend,
                ..Default::default()
            };
            self.pixmap
                .draw_pixmap(0, 0, layer.as_ref(), &pp, Transform::identity(), None);
            return;
        }
        let mut paint = Paint::default();
        paint.anti_alias = self.flags.anti_alias;
        paint.blend_mode = blend;
        match fill {
            Fill::None => {}
            Fill::Solid(c) => {
                paint.set_color(sk_color(*c));
                self.pixmap
                    .fill_path(sk, &paint, rule, Transform::identity(), None);
            }
            Fill::Fountain(f) => {
                let b = bbox(page_path);
                // The edge pad, repeats, mirror and inversion are worked
                // into one run of stops the shader pads beyond.
                let stops: Vec<GradientStop> = f
                    .resolved_stops()
                    .iter()
                    .map(|s| GradientStop::new(s.pos as f32, sk_color_alpha(s.color, s.opacity)))
                    .collect();
                // Nodes placed freely with the Interactive Fill tool: the
                // fill's own frame (start, end, skew handle) places it.
                if let Some(frame) = f.frame(b) {
                    let m = (self.screen * frame).as_coeffs();
                    let t = Transform::from_row(
                        m[0] as f32,
                        m[1] as f32,
                        m[2] as f32,
                        m[3] as f32,
                        m[4] as f32,
                        m[5] as f32,
                    );
                    let origin = tiny_skia::Point::from_xy(0.0, 0.0);
                    let shader = match f.kind {
                        tracedraw_core::FountainKind::Linear => LinearGradient::new(
                            origin,
                            tiny_skia::Point::from_xy(1.0, 0.0),
                            stops,
                            SpreadMode::Pad,
                            t,
                        ),
                        tracedraw_core::FountainKind::Radial => {
                            RadialGradient::new(origin, 0.0, origin, 1.0, stops, SpreadMode::Pad, t)
                        }
                        _ => None,
                    };
                    if let Some(sh) = shader {
                        paint.shader = sh;
                        self.pixmap
                            .fill_path(sk, &paint, rule, Transform::identity(), None);
                    } else if matches!(
                        f.kind,
                        tracedraw_core::FountainKind::Conical
                            | tracedraw_core::FountainKind::Square
                    ) {
                        let f2 = f.clone();
                        // The blends once; per pixel only the parameter,
                        // its band and a bisection in the stops.
                        let base = f.blended_stops();
                        let key = format!("{f:?}");
                        self.fill_procedural(page_path, sk, rule, &key, move |u, v| {
                            let p = Point::new(b.x0 + u * b.width(), b.y0 + v * b.height());
                            let t = f2.param_at(b, p).unwrap_or(0.0);
                            let s = f2.sample_blended(&base, t);
                            (s.color, s.opacity)
                        });
                    }
                    return;
                }
                match f.kind {
                    tracedraw_core::FountainKind::Shaped => {
                        self.fill_shaped(f, sk, sk, rule);
                    }
                    tracedraw_core::FountainKind::Points => {
                        let f2 = f.clone();
                        let places = f.point_places();
                        let key = format!("{f:?}");
                        self.fill_procedural(page_path, sk, rule, &key, move |u, v| {
                            let p = Point::new(b.x0 + u * b.width(), b.y0 + v * b.height());
                            f2.points_sample(&places, b, p)
                        });
                    }
                    tracedraw_core::FountainKind::Linear => {
                        let a = f.angle.to_radians();
                        let (cx, cy) = (b.center().x, b.center().y);
                        let half = (b.width() * a.cos().abs() + b.height() * a.sin().abs()) / 2.0;
                        let p0 = self.screen * Point::new(cx - half * a.cos(), cy - half * a.sin());
                        let p1 = self.screen * Point::new(cx + half * a.cos(), cy + half * a.sin());
                        if let Some(sh) = LinearGradient::new(
                            tiny_skia::Point::from_xy(p0.x as f32, p0.y as f32),
                            tiny_skia::Point::from_xy(p1.x as f32, p1.y as f32),
                            stops,
                            SpreadMode::Pad,
                            Transform::identity(),
                        ) {
                            paint.shader = sh;
                            self.pixmap
                                .fill_path(sk, &paint, rule, Transform::identity(), None);
                        }
                    }
                    tracedraw_core::FountainKind::Radial => {
                        let c = Point::new(
                            b.center().x + f.offset.x * b.width() / 2.0,
                            b.center().y + f.offset.y * b.height() / 2.0,
                        );
                        let radius = (b.width().max(b.height()) / 2.0) * std::f64::consts::SQRT_2;
                        let sc = self.screen * c;
                        if let Some(sh) = RadialGradient::new(
                            tiny_skia::Point::from_xy(sc.x as f32, sc.y as f32),
                            0.0,
                            tiny_skia::Point::from_xy(sc.x as f32, sc.y as f32),
                            (radius * self.zoom) as f32,
                            stops,
                            SpreadMode::Pad,
                            Transform::identity(),
                        ) {
                            paint.shader = sh;
                            self.pixmap
                                .fill_path(sk, &paint, rule, Transform::identity(), None);
                        }
                    }
                    tracedraw_core::FountainKind::Conical
                    | tracedraw_core::FountainKind::Square => {
                        let f2 = f.clone();
                        let base = f.blended_stops();
                        let key = format!("{f:?}");
                        self.fill_procedural(page_path, sk, rule, &key, move |u, v| {
                            let dx = u - 0.5 - f2.offset.x / 2.0;
                            let dy = v - 0.5 - f2.offset.y / 2.0;
                            let t = match f2.kind {
                                tracedraw_core::FountainKind::Conical => {
                                    let a = dy.atan2(dx) - f2.angle.to_radians();
                                    let a =
                                        a.rem_euclid(std::f64::consts::TAU) / std::f64::consts::TAU;
                                    if a < 0.5 {
                                        a * 2.0
                                    } else {
                                        2.0 - a * 2.0
                                    }
                                }
                                _ => (dx.abs().max(dy.abs()) * 2.0).min(1.0),
                            };
                            let s = f2.sample_blended(&base, t);
                            (s.color, s.opacity)
                        });
                    }
                }
            }
            Fill::Pattern(p) => self.fill_pattern(p, sk, rule, object),
            Fill::Procedural(p) => self.fill_procedural_tile(p, sk, rule),
            Fill::Mesh(m) => self.fill_mesh(m, sk, rule, tiny_skia::BlendMode::SourceOver),
            Fill::Texture(t) => {
                let b = bbox(page_path);
                let t2 = t.clone();
                let key = format!("{t:?}");
                // The end colours once (a CMYK colour's conversion is not
                // cheap), then blended per pixel as `lerp_color` does.
                let ca = t.color_a.to_rgb8();
                let cb = t.color_b.to_rgb8();
                self.fill_procedural(page_path, sk, rule, &key, move |u, v| {
                    let x = u * b.width() / t2.scale.max(0.1);
                    let y = v * b.height() / t2.scale.max(0.1);
                    if t2.kind.is_holographic() {
                        let f = |c: [u8; 3]| c.map(|v| v as f32 / 255.0);
                        let glitter = t2.kind == tracedraw_core::TextureKind::HoloGlitter;
                        let [r, g, bl] = holo_color(x, y, t2.seed, glitter, f(ca), f(cb));
                        return (Color::Rgb { r, g, b: bl }, 1.0);
                    }
                    let n = texture_value(t2.kind, x, y, t2.seed) as f32;
                    let l = |x: u8, y: u8| {
                        (x as f32 + (y as f32 - x as f32) * n)
                            .round()
                            .clamp(0.0, 255.0) as u8
                    };
                    (
                        Color::rgb8(l(ca[0], cb[0]), l(ca[1], cb[1]), l(ca[2], cb[2])),
                        1.0,
                    )
                });
            }
        }
    }

    /// Mesh fill (nodes in page space): each cell is the Coons patch
    /// bounded by its four grid lines (cubic curves where the nodes carry
    /// handles), cut into small triangles whose colour and opacity are
    /// mixed pixel by pixel from the cell's four nodes (bilinear), or
    /// with Smooth colour from the sixteen around it (Catmull-Rom across
    /// the grid), so they flow through the nodes without creases. The
    /// picture is worked out in the screen's scale and turn without its
    /// pan and kept (`procedural_cache`), so panning, a selection change
    /// or another object dragged do not compute it again; it is drawn
    /// through the path with bilinear sampling.
    fn fill_mesh(
        &mut self,
        m: &tracedraw_core::Mesh,
        sk: &SkPath,
        rule: FillRule,
        blend: tiny_skia::BlendMode,
    ) {
        let [a, b, c, d, tx, ty] = self.screen.as_coeffs();
        let lm = m.transformed(Affine::new([a, b, c, d, 0.0, 0.0]));
        let Some(bounds) = mesh_bounds(&lm) else {
            return;
        };
        if !(bounds.width().is_finite() && bounds.height().is_finite()) {
            return;
        }
        // A mesh far larger than the canvas is worked out for the part on
        // it only, and not kept.
        let (cw, ch) = (self.pixmap.width() as f64, self.pixmap.height() as f64);
        let huge = bounds.width() * bounds.height() > MESH_RASTER_PIXELS as f64;
        let area = if huge {
            let visible = bounds.intersect(Rect::new(-tx, -ty, cw - tx, ch - ty));
            if visible.is_zero_area() {
                return;
            }
            visible
        } else {
            bounds
        };
        // On screen a large picture is worked out on a coarser grid (one
        // picture pixel for `step` screen pixels).
        let step = if self.flags.draft {
            procedural_step(area.width().ceil() as u32, area.height().ceil() as u32)
        } else {
            1
        };
        // The picture starts two of its pixels before the mesh's corner,
        // wherever that falls, so a moved mesh is the same picture (the
        // pattern's transform carries the fraction of a pixel); the
        // outermost pixels stay clear (the edge is grown by one pixel
        // only, and the ring is cleared besides), so the path shows
        // nothing outside the grid, at every step.
        let pad = 2.0 * step as f64;
        let x0 = area.x0 - pad;
        let y0 = area.y0 - pad;
        let w = ((area.width() + 2.0 * pad) / step as f64).ceil().max(1.0) as u32 + 1;
        let h = ((area.height() + 2.0 * pad) / step as f64).ceil().max(1.0) as u32 + 1;
        let key = if huge {
            None
        } else {
            use std::hash::{Hash, Hasher};
            let mut hs = std::collections::hash_map::DefaultHasher::new();
            let q = |p: Point| {
                (
                    ((p.x - x0) * 64.0).round() as i64,
                    ((p.y - y0) * 64.0).round() as i64,
                )
            };
            for n in &lm.nodes {
                q(n.pos).hash(&mut hs);
                n.color.to_rgb8().hash(&mut hs);
                n.alpha.to_bits().hash(&mut hs);
                if let Some(hd) = n.handles {
                    for p in hd {
                        q(p).hash(&mut hs);
                    }
                }
            }
            (lm.rows, lm.cols, lm.smooth).hash(&mut hs);
            Some(format!("mesh|{:x}|{w},{h},{step}", hs.finish()))
        };
        let compute = || {
            let mut pm = Pixmap::new(w, h)?;
            let to_raster = Affine::scale(1.0 / step as f64) * Affine::translate((-x0, -y0));
            rasterise_mesh(&lm, to_raster, &mut pm);
            Some(pm)
        };
        let Some(pm) = (match key {
            Some(key) => procedural_cached(&key, compute),
            None => compute().map(std::sync::Arc::new),
        }) else {
            return;
        };
        let mut paint = Paint::default();
        paint.anti_alias = self.flags.anti_alias;
        paint.blend_mode = blend;
        paint.shader = tiny_skia::Pattern::new(
            (*pm).as_ref(),
            SpreadMode::Pad,
            tiny_skia::FilterQuality::Bilinear,
            1.0,
            Transform::from_translate((tx + x0) as f32, (ty + y0) as f32)
                .pre_scale(step as f32, step as f32),
        );
        self.pixmap
            .fill_path(sk, &paint, rule, Transform::identity(), None);
    }

    /// Fill with a per-pixel function over the path's bounding box
    /// (u, v in 0..1, v up). Only the part on the canvas is worked out;
    /// large areas are worked out on a coarser grid (the functions are
    /// smooth) and on several threads, and the last results are kept, so
    /// a drawing redrawn with the object in the same place (another object
    /// dragged, the selection changed) does not compute it again.
    /// Shaped fountain: the colour follows the distance from the edge of
    /// `mask` (screen space) towards its inside, 0 on the edge and 1 at
    /// the deepest point; `area` is painted with it. The distances are
    /// worked out on a grid of at most `SHAPED_GRID` pixels across the
    /// whole mask, so zooming and panning keep the colours in place.
    fn fill_shaped(
        &mut self,
        f: &tracedraw_core::Fountain,
        mask: &SkPath,
        area: &SkPath,
        rule: FillRule,
    ) {
        let b = mask.bounds();
        let (bw, bh) = (b.width(), b.height());
        if !(bw > 0.0 && bh > 0.0 && bw.is_finite() && bh.is_finite()) {
            return;
        }
        let scale = (SHAPED_GRID / bw.max(bh)).min(1.0);
        let gw = (bw * scale).ceil() as u32 + 2;
        let gh = (bh * scale).ceil() as u32 + 2;
        // Grid pixel (1, 1) is the mask's top-left corner.
        let to_grid = Transform::from_translate(1.0, 1.0)
            .pre_scale(scale, scale)
            .pre_translate(-b.left(), -b.top());
        let key = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            for p in mask.points() {
                (((p.x - b.left()) * scale * 64.0).round() as i64).hash(&mut h);
                (((p.y - b.top()) * scale * 64.0).round() as i64).hash(&mut h);
            }
            mask.verbs().len().hash(&mut h);
            matches!(rule, FillRule::EvenOdd).hash(&mut h);
            format!("shaped|{f:?}|{:x}|{gw}x{gh}", h.finish())
        };
        let pm = match procedural_cache_get(&key) {
            Some(pm) => pm,
            None => {
                let Some(mut m) = Pixmap::new(gw, gh) else {
                    return;
                };
                let mut paint = Paint::default();
                paint.set_color_rgba8(0, 0, 0, 255);
                paint.anti_alias = false;
                m.fill_path(mask, &paint, rule, to_grid, None);
                let inside: Vec<bool> = m.pixels().iter().map(|p| p.alpha() >= 128).collect();
                let dist = edge_distance(&inside, gw as usize, gh as usize);
                let deepest = dist.iter().cloned().fold(0.0f64, f64::max).max(1e-9);
                let base = f.blended_stops();
                let Some(mut pm) = Pixmap::new(gw, gh) else {
                    return;
                };
                for (px, d) in pm.pixels_mut().iter_mut().zip(&dist) {
                    let s = f.sample_blended(&base, d / deepest);
                    let [r, g, bb] = s.color.to_rgb8();
                    let a = (s.opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
                    *px = tiny_skia::ColorU8::from_rgba(r, g, bb, a).premultiply();
                }
                let pm = std::sync::Arc::new(pm);
                procedural_cache_put(key, pm.clone());
                pm
            }
        };
        let Some(to_screen) = to_grid.invert() else {
            return;
        };
        let mut paint = Paint::default();
        paint.anti_alias = self.flags.anti_alias;
        paint.shader = tiny_skia::Pattern::new(
            (*pm).as_ref(),
            SpreadMode::Pad,
            tiny_skia::FilterQuality::Bilinear,
            1.0,
            to_screen,
        );
        self.pixmap
            .fill_path(area, &paint, rule, Transform::identity(), None);
    }

    fn fill_procedural(
        &mut self,
        page_path: &BezPath,
        sk: &SkPath,
        rule: FillRule,
        key: &str,
        f: impl Fn(f64, f64) -> (Color, f32) + Sync,
    ) {
        use tracedraw_core::geometry::Shape as _;
        let b = bbox(page_path);
        let sb = (self.screen * b.to_path(0.01)).bounding_box();
        if !(sb.width() > 0.0 && sb.height() > 0.0) {
            return;
        }
        // Zoomed in, the object may be far larger than the window; a pixel
        // of margin keeps the edges smooth.
        let vis = sb.intersect(tracedraw_core::Rect::new(
            -1.0,
            -1.0,
            self.pixmap.width() as f64 + 1.0,
            self.pixmap.height() as f64 + 1.0,
        ));
        if !(vis.width() > 0.0 && vis.height() > 0.0) {
            return;
        }
        let (x0, y0) = (vis.x0.floor(), vis.y0.floor());
        let full_w = ((vis.x1.ceil() - x0) as u32).clamp(1, 8192);
        let full_h = ((vis.y1.ceil() - y0) as u32).clamp(1, 8192);
        let step = if self.flags.draft {
            procedural_step(full_w, full_h)
        } else {
            1
        };
        let w = full_w.div_ceil(step).max(1);
        let h = full_h.div_ceil(step).max(1);
        let cache_key = format!(
            "{key}|{:.2},{:.2},{:.2},{:.2}|{x0},{y0},{w},{h},{step}",
            sb.x0, sb.y0, sb.x1, sb.y1
        );
        let pm = match procedural_cache_get(&cache_key) {
            Some(pm) => pm,
            None => {
                let Some(mut pm) = Pixmap::new(w, h) else {
                    return;
                };
                let (bw, bh) = (sb.width(), sb.height());
                let st = step as f64;
                let row = |y: u32, out: &mut [tiny_skia::PremultipliedColorU8]| {
                    for (x, px) in out.iter_mut().enumerate() {
                        let u = (x0 + (x as f64 + 0.5) * st - sb.x0) / bw;
                        let v = 1.0 - (y0 + (y as f64 + 0.5) * st - sb.y0) / bh;
                        let (c, o) = f(u, v);
                        let [r, g, bb] = c.to_rgb8();
                        let a = (o.clamp(0.0, 1.0) * 255.0).round() as u8;
                        *px = tiny_skia::ColorU8::from_rgba(r, g, bb, a).premultiply();
                    }
                };
                for_rows(pm.pixels_mut(), w as usize, &row);
                let pm = std::sync::Arc::new(pm);
                procedural_cache_put(cache_key, pm.clone());
                pm
            }
        };
        let mut paint = Paint::default();
        paint.anti_alias = self.flags.anti_alias;
        paint.shader = tiny_skia::Pattern::new(
            (*pm).as_ref(),
            SpreadMode::Pad,
            tiny_skia::FilterQuality::Bilinear,
            1.0,
            Transform::from_translate(x0 as f32, y0 as f32).pre_scale(step as f32, step as f32),
        );
        self.pixmap
            .fill_path(sk, &paint, rule, Transform::identity(), None);
    }

    /// A pattern fill: the block of tiles that repeats (one tile, or the
    /// mirrored and shifted tiles of the placement) as one pixmap, drawn
    /// once with a repeating shader whose transform carries the whole
    /// placement (tile size, offset, rotation, skew, the row or column
    /// lean) and, with Transform with object, the object's transform
    /// `object`. Tiles anchor to the page origin otherwise.
    fn fill_pattern(
        &mut self,
        p: &tracedraw_core::Pattern,
        sk: &SkPath,
        rule: FillRule,
        object: Affine,
    ) {
        let placement = p.placement().sanitized();
        let tile = p.tile_size();
        if !(tile.width.is_finite() && tile.height.is_finite())
            || tile.width <= 0.0
            || tile.height <= 0.0
        {
            return;
        }
        let to_page = if placement.transform_with_object {
            object * placement.place()
        } else {
            placement.place()
        };
        let unit_to_screen = self.screen * to_page * placement.cell(tile);
        let Some((pm, px_to_unit)) = self.pattern_cell(p, &placement, tile, unit_to_screen) else {
            return;
        };
        let m = (unit_to_screen * px_to_unit).as_coeffs();
        if !m.iter().all(|v| v.is_finite()) {
            return;
        }
        let t = Transform::from_row(
            m[0] as f32,
            m[1] as f32,
            m[2] as f32,
            m[3] as f32,
            m[4] as f32,
            m[5] as f32,
        );
        // A two-colour tile keeps its hard edges when its pixels line up
        // with the screen's; turned or skewed, it is smoothed like the
        // other kinds.
        let axis_aligned = m[1].abs() < 1e-9 && m[2].abs() < 1e-9;
        let quality = if matches!(p, tracedraw_core::Pattern::TwoColor { .. }) && axis_aligned {
            tiny_skia::FilterQuality::Nearest
        } else {
            tiny_skia::FilterQuality::Bilinear
        };
        let mut paint = Paint::default();
        paint.anti_alias = self.flags.anti_alias;
        paint.shader =
            tiny_skia::Pattern::new(pm.as_ref().as_ref(), SpreadMode::Repeat, quality, 1.0, t);
        self.pixmap
            .fill_path(sk, &paint, rule, Transform::identity(), None);
    }

    /// The pixels of the block that repeats for `p` placed by
    /// `placement` (tiles of `tile` mm), at the resolution
    /// `unit_to_screen` (the block's unit square to screen pixels) asks
    /// for, with the transform from those pixels to the unit square (Y
    /// up). A bitmap pattern without mirrored or shifted tiles is its own
    /// picture, decoded once and kept.
    fn pattern_cell(
        &self,
        p: &tracedraw_core::Pattern,
        placement: &tracedraw_core::style::PatternPlacement,
        tile: tracedraw_core::geometry::Size,
        unit_to_screen: Affine,
    ) -> Option<(std::sync::Arc<Pixmap>, Affine)> {
        use std::sync::Arc;
        // Pixels along the block's two edges on screen.
        let o = unit_to_screen * Point::new(0.0, 0.0);
        let ex = ((unit_to_screen * Point::new(1.0, 0.0)) - o).hypot();
        let ey = ((unit_to_screen * Point::new(0.0, 1.0)) - o).hypot();
        if !(ex.is_finite() && ey.is_finite()) {
            return None;
        }
        let size = |max: u32| -> (u32, u32) {
            (
                (ex.round() as u32).clamp(1, max),
                (ey.round() as u32).clamp(1, max),
            )
        };
        let px_to_unit = |tw: u32, th: u32| {
            Affine::new([
                1.0 / f64::from(tw),
                0.0,
                0.0,
                -1.0 / f64::from(th),
                0.0,
                1.0,
            ])
        };
        let cell = placement.cell(tile);
        let cell_inv = cell.inverse();
        match p {
            tracedraw_core::Pattern::TwoColor {
                tile: kind,
                front,
                back,
                ..
            } => {
                let (tw, th) = size(1024);
                let key =
                    format!("pat2|{kind:?}|{front:?}|{back:?}|{placement:?}|{tile:?}|{tw}x{th}");
                if let Some(pm) = procedural_cache_get(&key) {
                    return Some((pm, px_to_unit(tw, th)));
                }
                let mut pm = Pixmap::new(tw, th)?;
                let [fr, fg, fb] = front.to_rgb8();
                let [br, bg, bb] = back.to_rgb8();
                let fc = tiny_skia::ColorU8::from_rgba(fr, fg, fb, 255).premultiply();
                let bc = tiny_skia::ColorU8::from_rgba(br, bg, bb, 255).premultiply();
                let data = pm.pixels_mut();
                for y in 0..th {
                    for x in 0..tw {
                        let unit = Point::new(
                            (f64::from(x) + 0.5) / f64::from(tw),
                            1.0 - (f64::from(y) + 0.5) / f64::from(th),
                        );
                        let (u, v) = placement.tile_uv(cell * unit, tile);
                        if let Some(px) = data.get_mut((y * tw + x) as usize) {
                            *px = if kind.front(u, v) { fc } else { bc };
                        }
                    }
                }
                let pm = Arc::new(pm);
                procedural_cache_put(key, pm.clone());
                Some((pm, px_to_unit(tw, th)))
            }
            tracedraw_core::Pattern::Bitmap { png, .. } => {
                let img = image_cache::decode(png)?;
                let (iw, ih) = (img.width(), img.height());
                // The picture's pixels to one tile's millimetres.
                let content = Affine::new([
                    tile.width / f64::from(iw.max(1)),
                    0.0,
                    0.0,
                    -tile.height / f64::from(ih.max(1)),
                    0.0,
                    tile.height,
                ]);
                if !placement.needs_cell() {
                    // One tile is the block: the picture itself repeats.
                    return Some((img, cell_inv * content));
                }
                let (cols, rows) = placement.cell_tiles();
                // No sharper than the picture, no larger than the screen asks.
                let tw = ((ex.round() as u32).min(iw.saturating_mul(cols))).clamp(1, 4096);
                let th = ((ey.round() as u32).min(ih.saturating_mul(rows))).clamp(1, 4096);
                let key = format!(
                    "patb|{:x}|{placement:?}|{tile:?}|{tw}x{th}",
                    image_cache::fingerprint(png)
                );
                if let Some(pm) = procedural_cache_get(&key) {
                    return Some((pm, px_to_unit(tw, th)));
                }
                let mut pm = Pixmap::new(tw, th)?;
                let unit_to_px = px_to_unit(tw, th).inverse();
                let paint = tiny_skia::PixmapPaint {
                    quality: tiny_skia::FilterQuality::Bilinear,
                    ..Default::default()
                };
                let picture = Rect::new(0.0, 0.0, f64::from(iw), f64::from(ih));
                for copy in placement.tile_copies(tile) {
                    let t = unit_to_px * cell_inv * copy * content;
                    if !copy_shows(t, picture, tw, th) {
                        continue;
                    }
                    let c = t.as_coeffs();
                    pm.draw_pixmap(
                        0,
                        0,
                        img.as_ref().as_ref(),
                        &paint,
                        Transform::from_row(
                            c[0] as f32,
                            c[1] as f32,
                            c[2] as f32,
                            c[3] as f32,
                            c[4] as f32,
                            c[5] as f32,
                        ),
                        None,
                    );
                }
                let pm = Arc::new(pm);
                procedural_cache_put(key, pm.clone());
                Some((pm, px_to_unit(tw, th)))
            }
            tracedraw_core::Pattern::Vector {
                shapes, tile: own, ..
            } => {
                let (tw, th) = size(2048);
                // The tile's content stretched to the tile size placed.
                let content = Affine::scale_non_uniform(
                    tile.width / own.width.max(1e-9),
                    tile.height / own.height.max(1e-9),
                );
                let unit_to_px = px_to_unit(tw, th).inverse();
                let copies: Vec<Affine> = placement
                    .tile_copies(tile)
                    .into_iter()
                    .map(|copy| unit_to_px * cell_inv * copy * content)
                    .collect();
                let pm = self.render_vector_cell(
                    shapes,
                    Rect::new(0.0, 0.0, own.width, own.height),
                    &copies,
                    tw,
                    th,
                )?;
                Some((Arc::new(pm), px_to_unit(tw, th)))
            }
        }
    }

    /// A procedural fill: its tile (core builds the paths once per set of
    /// numbers) painted once at the current zoom, kept with the other
    /// procedural pixmaps, and repeated from the page origin, so the
    /// tiles stay in place when the object moves and match the exports.
    /// No per-pixel work: the tile is drawn as paths and the pattern
    /// shader repeats it.
    fn fill_procedural_tile(
        &mut self,
        p: &tracedraw_core::Procedural,
        sk: &SkPath,
        rule: FillRule,
    ) {
        let tile = tracedraw_core::procedural::procedural_tile(p);
        let (tw_mm, th_mm) = (tile.size.width, tile.size.height);
        if !(tw_mm.is_finite() && th_mm.is_finite()) || tw_mm <= 0.0 || th_mm <= 0.0 {
            return;
        }
        let tw = ((tw_mm * self.zoom).round() as u32).clamp(1, 2048);
        let th = ((th_mm * self.zoom).round() as u32).clamp(1, 2048);
        let key = format!("procedural|{p:?}|{tw}x{th}|{}", self.flags.anti_alias);
        let pm = match procedural_cache_get(&key) {
            Some(pm) => pm,
            None => {
                let Some(mut pm) = Pixmap::new(tw, th) else {
                    return;
                };
                if let Some(bg) = p.background {
                    pm.fill(sk_color(bg));
                }
                // Tile space (mm, Y up, origin bottom-left) to tile pixels.
                let to_px = Affine::new([
                    tw as f64 / tw_mm,
                    0.0,
                    0.0,
                    -(th as f64) / th_mm,
                    0.0,
                    th as f64,
                ]);
                let mut paint = Paint::default();
                paint.anti_alias = self.flags.anti_alias;
                paint.set_color(sk_color(p.foreground));
                for path in &tile.paths {
                    if let Some(sp) = to_sk_path(&(to_px * path)) {
                        pm.fill_path(&sp, &paint, FillRule::Winding, Transform::identity(), None);
                    }
                }
                let pm = std::sync::Arc::new(pm);
                procedural_cache_put(key, pm.clone());
                pm
            }
        };
        // The tiles start at the page origin.
        let o = self.screen * Point::new(0.0, 0.0);
        let shift = Transform::from_translate(
            o.x.rem_euclid(tw as f64) as f32,
            o.y.rem_euclid(th as f64) as f32,
        );
        let mut paint = Paint::default();
        paint.anti_alias = self.flags.anti_alias;
        paint.shader = tiny_skia::Pattern::new(
            pm.as_ref().as_ref(),
            SpreadMode::Repeat,
            tiny_skia::FilterQuality::Bilinear,
            1.0,
            shift,
        );
        self.pixmap
            .fill_path(sk, &paint, rule, Transform::identity(), None);
    }

    /// Rasterise a vector pattern's block of tiles: the tile's shapes
    /// drawn once per `copies` entry (tile mm, Y up, to block pixels, Y
    /// down) into a `tw` by `th` pixmap. The shapes go through the
    /// ordinary shape drawing, so nested groups, fills, outlines and text
    /// in the tile render exactly as on the page. The pixmap is a whole
    /// number of pixels, so the repeat is seamless; the tile's millimetre
    /// size is stretched to fit by at most half a pixel on each axis.
    fn render_vector_cell(
        &self,
        shapes: &[Shape],
        tile: Rect,
        copies: &[Affine],
        tw: u32,
        th: u32,
    ) -> Option<Pixmap> {
        let mut pm = Pixmap::new(tw, th)?;
        for screen in copies {
            if !copy_shows(*screen, tile, tw, th) {
                continue;
            }
            let c = screen.as_coeffs();
            // Pixels per millimetre of the content, for outline widths.
            let zoom = (c[0] * c[3] - c[1] * c[2]).abs().sqrt();
            if !(zoom.is_finite() && zoom > 0.0) {
                continue;
            }
            let mut sub = Renderer {
                pixmap: &mut pm,
                screen: *screen,
                zoom,
                flags: Flags {
                    wireframe: false,
                    ..self.flags
                },
                symbols: self.symbols,
                preview: None,
                bounds: None,
                origin: (0, 0),
                cancel: None,
            };
            for s in shapes {
                sub.draw_shape(s, Affine::IDENTITY);
            }
        }
        Some(pm)
    }

    fn stroke_paint(&self, stroke: &Stroke, transform: Affine) -> (Paint<'static>, SkStroke) {
        let mut paint = Paint::default();
        paint.anti_alias = self.flags.anti_alias;
        paint.set_color(sk_color(stroke.color));
        let hair = stroke.width <= Stroke::HAIRLINE + 1e-9;
        // "Scale with object" uses the object's own scale factor.
        let object_scale = if stroke.scale_with_object {
            let c = transform.as_coeffs();
            ((c[0] * c[3] - c[1] * c[2]).abs()).sqrt().max(1e-6)
        } else {
            1.0
        };
        let true_px = (stroke.width * object_scale * self.zoom) as f32;
        // An outline thinner than a pixel is drawn one pixel wide (a
        // hairline) rather than faded until it disappears, as previews of
        // such files show it.
        let width_px = if hair || true_px < 1.0 { 0.0 } else { true_px };
        // A dashed hairline keeps its dashes, a pixel per unit of the
        // pattern at least, as a thin outline's do. It was drawn solid, so
        // dashed seams of a `.cdr` drawing opened as plain lines (#2463).
        let dash = if stroke.dash.is_empty() {
            None
        } else {
            let unit = if hair {
                (Stroke::HAIRLINE * object_scale * self.zoom) as f32
            } else {
                true_px
            }
            .max(1.0);
            let arr: Vec<f32> = stroke
                .dash
                .iter()
                .map(|d| (*d as f32 * unit).max(0.01))
                .collect();
            let arr = if arr.len() % 2 == 1 {
                [arr.clone(), arr].concat()
            } else {
                arr
            };
            StrokeDash::new(arr, 0.0)
        };
        let sk = SkStroke {
            // An outline wider than the reach covers the picture all the
            // same, and its edges overflowed the rasteriser.
            width: width_px.min(MAX_REACH as f32),
            miter_limit: stroke.miter_ratio() as f32,
            line_cap: match stroke.cap {
                LCap::Butt => LineCap::Butt,
                LCap::Round => LineCap::Round,
                LCap::Square => LineCap::Square,
            },
            line_join: match stroke.join {
                LJoin::Miter => LineJoin::Miter,
                LJoin::Round => LineJoin::Round,
                LJoin::Bevel => LineJoin::Bevel,
            },
            dash,
        };
        (paint, sk)
    }
}

/// The silhouette of a picture on `layer`: its alpha as the coverage, in
/// white, placed as the picture is (`transform`, its `rect` of
/// `width_px` by `height_px` pixels) and moved as the shadow `sh`
/// measured on box `b` asks, so a transparent background casts no shadow
/// (#2027).
#[allow(clippy::too_many_arguments)]
fn bitmap_silhouette(
    layer: &mut Pixmap,
    sh: &tracedraw_core::Shadow,
    b: Rect,
    screen: Affine,
    transform: Affine,
    rect: Rect,
    (width_px, height_px): (u32, u32),
    img: &Pixmap,
) {
    let (w0, h0) = (width_px.max(1) as f64, height_px.max(1) as f64);
    let img_to_page = transform
        * Affine::new([
            rect.width() / w0,
            0.0,
            0.0,
            -rect.height() / h0,
            rect.x0,
            rect.y1,
        ]);
    let m = (screen * sh.silhouette_transform(b) * img_to_page).as_coeffs();
    let t = Transform::from_row(
        m[0] as f32,
        m[1] as f32,
        m[2] as f32,
        m[3] as f32,
        m[4] as f32,
        m[5] as f32,
    );
    let pp = tiny_skia::PixmapPaint {
        quality: tiny_skia::FilterQuality::Bilinear,
        ..Default::default()
    };
    layer.draw_pixmap(0, 0, Pixmap::as_ref(img), &pp, t, None);
    // The silhouette in white, the picture's alpha as its coverage.
    for px in layer.pixels_mut().iter_mut() {
        let a = px.alpha();
        *px = tiny_skia::ColorU8::from_rgba(255, 255, 255, a).premultiply();
    }
}

/// Paint the drop shadow `sh` of an object with page path `page_path` into
/// the empty `layer` (pixels = `screen` times page millimetres, `zoom`
/// pixels per millimetre), with its opacity and fade but not its merge
/// mode. The silhouette is what the object shows ([`paint_shown`]): its
/// fill by the rule the object is filled by, so a hole of the object is a
/// hole of its shadow (#2216), and its outline as drawn, so an object
/// with no fill casts the shadow of its outline alone and a wide or
/// variable outline casts its own shape. It was the filled path always:
/// a ring drawn with a variable outline cast a whole disc. False when
/// nothing was painted.
fn shadow_layer(
    layer: &mut Pixmap,
    sh: &tracedraw_core::Shadow,
    shown: Shown,
    screen: Affine,
    zoom: f64,
    anti_alias: bool,
) -> bool {
    let b = shown.path.bounding_box();
    if to_sk_path(shown.path).is_none() {
        return false;
    }
    paint_shown(
        layer,
        &shown,
        screen * sh.silhouette_transform(b),
        None,
        anti_alias,
    );
    shade_shadow(layer, sh, b, screen, zoom);
    true
}

/// An inner shadow's settings (see `Effect::InnerShadow`).
#[derive(Debug, Clone, Copy)]
struct InnerShadow {
    offset: tracedraw_core::geometry::Vec2,
    color: tracedraw_core::Color,
    opacity: f64,
    blur: f64,
    merge: tracedraw_core::live::MergeMode,
    /// 0..1, see `Effect::InnerShadow`.
    spread: f64,
}

/// An object as its shadow sees it: `shape`, its outline `path` on the
/// page (its parent's space), `object` its transform there and `rule`
/// the rule its fill is drawn by.
struct Shown<'a> {
    shape: &'a Shape,
    path: &'a BezPath,
    object: Affine,
    rule: FillRule,
}

/// Paint in white, through `to_screen` (page to layer), what `shown`
/// shows: its fill unless it has none, and its outline as drawn (a band
/// for a variable, calligraphic or placed outline, the stroked line
/// otherwise, dashes and arrowheads with it; a hairline one pixel wide).
/// An object with neither fill nor outline is painted filled, as before.
fn paint_shown(
    layer: &mut Pixmap,
    shown: &Shown,
    to_screen: Affine,
    clip: Option<&tiny_skia::Mask>,
    anti_alias: bool,
) {
    let Some(sk) = to_sk_path(&(to_screen * shown.path.clone())) else {
        return;
    };
    let mut paint = Paint::default();
    paint.anti_alias = anti_alias;
    paint.set_color_rgba8(255, 255, 255, 255);
    let stroke = shown.shape.stroke.as_ref();
    let filled = !matches!(shown.shape.fill, Fill::None);
    if filled || stroke.is_none() {
        layer.fill_path(&sk, &paint, shown.rule, Transform::identity(), clip);
    }
    let Some(stroke) = stroke else {
        return;
    };
    let object_scale = if stroke.scale_with_object {
        let c = shown.object.as_coeffs();
        ((c[0] * c[3] - c[1] * c[2]).abs()).sqrt().max(1e-6)
    } else {
        1.0
    };
    if let Some(band) = tracedraw_core::shaping::outline_band(shown.path, stroke, object_scale) {
        if let Some(bp) = to_sk_path(&(to_screen * band)) {
            layer.fill_path(&bp, &paint, FillRule::Winding, Transform::identity(), clip);
        }
    } else {
        let c = to_screen.as_coeffs();
        let px_per_mm = ((c[0] * c[3] - c[1] * c[2]).abs()).sqrt();
        let hair = stroke.width <= Stroke::HAIRLINE + 1e-9 || !stroke.width.is_finite();
        let true_px = if hair {
            0.0
        } else {
            (stroke.width * object_scale * px_per_mm) as f32
        };
        let dash = if stroke.dash.is_empty() {
            None
        } else {
            // A dashed hairline keeps its dashes, as on the page (#2463).
            let unit = if hair {
                (Stroke::HAIRLINE * object_scale * px_per_mm) as f32
            } else {
                true_px
            }
            .max(1.0);
            let mut arr: Vec<f32> = stroke
                .dash
                .iter()
                .map(|d| (*d as f32 * unit).max(0.01))
                .collect();
            if arr.len() % 2 == 1 {
                arr = [arr.clone(), arr].concat();
            }
            StrokeDash::new(arr, 0.0)
        };
        let line = SkStroke {
            width: true_px.clamp(1.0, MAX_REACH as f32),
            miter_limit: stroke.miter_ratio() as f32,
            line_cap: match stroke.cap {
                LCap::Butt => LineCap::Butt,
                LCap::Round => LineCap::Round,
                LCap::Square => LineCap::Square,
            },
            line_join: match stroke.join {
                LJoin::Miter => LineJoin::Miter,
                LJoin::Round => LineJoin::Round,
                LJoin::Bevel => LineJoin::Bevel,
            },
            dash,
        };
        let sk = headed_line(shown.path, stroke, to_screen).unwrap_or(sk);
        layer.stroke_path(&sk, &paint, &line, Transform::identity(), clip);
    }
    for head in tracedraw_core::style::arrowhead_paths(shown.path, stroke) {
        if let Some(hp) = to_sk_path(&(to_screen * head)) {
            layer.fill_path(&hp, &paint, FillRule::Winding, Transform::identity(), clip);
        }
    }
}

/// [`shadow_layer`] for a group or a clip frame drawn inside `parent`:
/// the silhouette is what the object shows, every object of a group and
/// a clip frame's contents cut to its frame (#873), not the outline of
/// the whole. The shadow is measured on the object's box, as the Drop
/// Shadow tool measures it.
#[allow(clippy::too_many_arguments)]
fn group_shadow_layer(
    layer: &mut Pixmap,
    sh: &tracedraw_core::Shadow,
    shape: &Shape,
    parent: Affine,
    b: Rect,
    screen: Affine,
    zoom: f64,
    anti_alias: bool,
) {
    let to_screen = screen * sh.silhouette_transform(b) * parent;
    fill_silhouette(layer, shape, to_screen, None, anti_alias);
    shade_shadow(layer, sh, b, screen, zoom);
}

/// The box the shadow of a group or a clip frame drawn inside `parent`
/// is measured on; `None` when it is not finite.
fn group_shadow_box(shape: &Shape, parent: Affine) -> Option<Rect> {
    cast_boxes(shape, parent).map(|(b, _)| b)
}

/// The effects of an object draw objects of their own: its drop shadow
/// is cast by all of them together (a picture keeps the shadow of what
/// its pixels show).
fn casts_whole(ev: &tracedraw_core::live::Evaluated) -> bool {
    !(ev.below.is_empty() && ev.above.is_empty())
        && !matches!(ev.main.kind, ShapeKind::Bitmap { .. })
}

/// The box the drop shadow of `shape`, drawn inside `parent`, is measured
/// on, and the box all it casts the shadow from lies in. The first is the
/// box of its outline as drawn, an envelope or another warp taken in (of
/// its plain outline when its effects draw it among their own objects,
/// as a body turned in space does); the second reaches round what its
/// effects add. `None` when they are not finite.
fn cast_boxes(shape: &Shape, parent: Affine) -> Option<(Rect, Rect)> {
    let plain = || (parent * shape.page_path()).bounding_box();
    let (b, extent) = if shape.has_live_effects() {
        let drawn = (!shape.effects.is_empty())
            .then(|| tracedraw_core::live::evaluate(shape).main.page_path())
            .filter(|p| !p.elements().is_empty());
        let b = drawn.map_or_else(plain, |p| (parent * p).bounding_box());
        let all = parent.transform_rect_bbox(shape.visual_bounds());
        (b, if all.is_finite() { b.union(all) } else { b })
    } else {
        let b = plain();
        (b, b)
    };
    // What its outline covers casts the shadow too.
    let extent = match shape.kind {
        ShapeKind::Group { .. } | ShapeKind::ClipFrame { .. } => extent,
        _ if shape.stroke.is_none() => extent,
        _ => Renderer::reach(shape, parent, None).map_or(extent, |r| r.union(extent)),
    };
    (b.width().is_finite() && b.height().is_finite()).then_some((b, extent))
}

/// The pixels of a `w` x `h` target that the shadow `sh` of an object
/// measured on box `b` can reach through `screen` (`zoom` pixels per
/// millimetre): the moved or sheared box, widened by the three blur
/// passes and a pixel of smoothing, as left, top, width and height.
/// Pixels beyond stay clear on a layer of any size, so the shadow drawn
/// on a layer of only these is the same as on one the size of the target
/// (a blur clamps at the target's own edges, which this keeps). `None`
/// when it covers no pixel of the target.
#[cfg(test)]
fn shadow_pixels(
    sh: &tracedraw_core::Shadow,
    b: Rect,
    screen: Affine,
    zoom: f64,
    w: u32,
    h: u32,
) -> Option<(i32, i32, u32, u32)> {
    shadow_pixels_over(sh, b, b, screen, zoom, w, h)
}

/// [`shadow_pixels`] for a silhouette that covers `extent`, which the
/// shadow measured on `b` moves or shears.
fn shadow_pixels_over(
    sh: &tracedraw_core::Shadow,
    b: Rect,
    extent: Rect,
    screen: Affine,
    zoom: f64,
    w: u32,
    h: u32,
) -> Option<(i32, i32, u32, u32)> {
    let (w, h) = (f64::from(w), f64::from(h));
    let flat = sh.silhouette_transform(b).transform_rect_bbox(extent);
    let corners = [
        Point::new(flat.x0, flat.y0),
        Point::new(flat.x1, flat.y0),
        Point::new(flat.x0, flat.y1),
        Point::new(flat.x1, flat.y1),
    ]
    .map(|p| screen * p);
    let (mut x0, mut y0) = (f64::INFINITY, f64::INFINITY);
    let (mut x1, mut y1) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for c in corners {
        (x0, x1) = (x0.min(c.x), x1.max(c.x));
        (y0, y1) = (y0.min(c.y), y1.max(c.y));
    }
    let blur = if sh.blur.is_finite() { sh.blur } else { 0.0 };
    let radius = (blur * zoom).round().max(0.0);
    // Three box passes reach three radii; Average's one pass of 1.7
    // radii less. Two more pixels for the smoothed edge and rounding.
    let reach = radius * 3.0 + 2.0;
    if !(x0.is_finite() && x1.is_finite() && y0.is_finite() && y1.is_finite() && reach.is_finite())
    {
        // Nothing to measure by: the whole target, as before.
        return Some((0, 0, w as u32, h as u32));
    }
    let left = (x0 - reach).floor().clamp(0.0, w);
    let right = (x1 + reach).ceil().clamp(0.0, w);
    let top = (y0 - reach).floor().clamp(0.0, h);
    let bottom = (y1 + reach).ceil().clamp(0.0, h);
    if right <= left || bottom <= top {
        return None;
    }
    Some((
        left as i32,
        top as i32,
        (right - left) as u32,
        (bottom - top) as u32,
    ))
}

/// Fill in white what `shape` covers, its parent's space taken to the
/// layer by `to_screen`, inside `clip` when given: the objects of a group
/// each, a clip frame's filled frame and its contents only where its
/// frame is (nested frames cut further), hidden objects not at all.
/// Each is filled by its own fill rule, holes left open (#2216). An
/// object with live effects covers what they draw: its outline as they
/// warp it and the objects they add (#2215).
fn fill_silhouette(
    layer: &mut Pixmap,
    shape: &Shape,
    to_screen: Affine,
    clip: Option<&tiny_skia::Mask>,
    anti_alias: bool,
) {
    silhouette(layer, shape, to_screen, clip, anti_alias, false)
}

/// [`fill_silhouette`]; `text` when `shape` is drawn from a text, whose
/// warped outline fills as text does.
fn silhouette(
    layer: &mut Pixmap,
    shape: &Shape,
    to_screen: Affine,
    clip: Option<&tiny_skia::Mask>,
    anti_alias: bool,
    text: bool,
) {
    if !shape.visible {
        return;
    }
    if !shape.effects.is_empty() {
        let ev = tracedraw_core::live::evaluate(shape);
        let text = text || matches!(shape.kind, ShapeKind::Text { .. });
        for part in ev.below.iter().chain([&ev.main]).chain(&ev.above) {
            let mut part = part.clone();
            part.effects.clear();
            silhouette(layer, &part, to_screen, clip, anti_alias, text);
        }
        return;
    }
    let inner = to_screen * shape.transform;
    match &shape.kind {
        ShapeKind::Group { children } => {
            for c in children {
                silhouette(layer, c, inner, clip, anti_alias, text);
            }
        }
        ShapeKind::ClipFrame { frame, contents } => {
            let Some(sk_frame) = to_sk_path(&(inner * frame.page_path())) else {
                return;
            };
            // A filled frame shows whole: its fill casts the shadow, and
            // the contents, all inside it, add nothing. Painted over the
            // fill they thickened its soft edge, a darker line along a
            // sharp shadow's edge.
            if !matches!(frame.fill, Fill::None) && frame.visible {
                let mut paint = Paint::default();
                paint.anti_alias = anti_alias;
                paint.set_color_rgba8(255, 255, 255, 255);
                layer.fill_path(
                    &sk_frame,
                    &paint,
                    shape_fill_rule(frame, false),
                    Transform::identity(),
                    clip,
                );
                return;
            }
            let mask = match clip {
                Some(m) => {
                    let mut m = m.clone();
                    m.intersect_path(
                        &sk_frame,
                        FillRule::EvenOdd,
                        anti_alias,
                        Transform::identity(),
                    );
                    Some(m)
                }
                None => tiny_skia::Mask::new(layer.width(), layer.height()).map(|mut m| {
                    m.fill_path(
                        &sk_frame,
                        FillRule::EvenOdd,
                        anti_alias,
                        Transform::identity(),
                    );
                    m
                }),
            };
            let Some(mask) = mask else {
                return;
            };
            for c in contents {
                fill_silhouette(layer, c, inner, Some(&mask), anti_alias);
            }
        }
        _ => {
            let path = shape.page_path();
            let shown = Shown {
                shape,
                path: &path,
                object: shape.transform,
                rule: shape_fill_rule(shape, text),
            };
            paint_shown(layer, &shown, to_screen, clip, anti_alias);
        }
    }
}

/// Widest radius, in pixels, a shadow's box blur takes at the picture's
/// own pixels; wider blurs run on a layer that many times smaller.
const BLUR_REACH: i32 = 64;

/// The alpha (0 to 255) of `layer`, drawn `k` times smaller than the
/// picture with `pad` layer pixels of margin, at the picture pixel `x`,
/// `y` (counted from the layer's origin less its margin): exact at
/// `k` = 1, read between the four nearest layer pixels otherwise.
fn layer_alpha_sampler(layer: &Pixmap, k: f64, pad: u32) -> impl Fn(u32, u32) -> f64 + '_ {
    let (w, h) = (layer.width() as i64, layer.height() as i64);
    let px = layer.pixels();
    let at = move |x: i64, y: i64| -> f64 {
        let (x, y) = (x.clamp(0, w - 1), y.clamp(0, h - 1));
        f64::from(px[(y * w + x) as usize].alpha())
    };
    move |x: u32, y: u32| {
        if k <= 1.0 {
            return at(i64::from(x + pad), i64::from(y + pad));
        }
        let fx = (f64::from(x) + 0.5) / k - 0.5 + f64::from(pad);
        let fy = (f64::from(y) + 0.5) / k - 0.5 + f64::from(pad);
        let (ix, iy) = (fx.floor(), fy.floor());
        let (tx, ty) = (fx - ix, fy - iy);
        let (ix, iy) = (ix as i64, iy as i64);
        let top = at(ix, iy) * (1.0 - tx) + at(ix + 1, iy) * tx;
        let bottom = at(ix, iy + 1) * (1.0 - tx) + at(ix + 1, iy + 1) * tx;
        top * (1.0 - ty) + bottom * ty
    }
}

/// A box blur of `radius` pixels and `passes` passes on `pm`, however
/// wide: past [`BLUR_REACH`] it runs on a copy that many times smaller,
/// then blown back up, so a shadow keeps its width at every zoom (#2895).
fn wide_box_blur(pm: &mut Pixmap, radius: i32, passes: usize) {
    if radius <= BLUR_REACH {
        accel::box_blur(pm, radius, passes);
        return;
    }
    let k = (radius + BLUR_REACH - 1) / BLUR_REACH;
    let (w, h) = (pm.width(), pm.height());
    let ku = k as u32;
    let (sw, sh) = (w.div_ceil(ku), h.div_ceil(ku));
    let Some(mut small) = Pixmap::new(sw, sh) else {
        accel::box_blur(pm, BLUR_REACH, passes);
        return;
    };
    {
        let src = pm.data();
        let dst = small.data_mut();
        for sy in 0..sh {
            for sx in 0..sw {
                let mut acc = [0u32; 4];
                let mut n = 0u32;
                for y in sy * ku..((sy + 1) * ku).min(h) {
                    for x in sx * ku..((sx + 1) * ku).min(w) {
                        let i = ((y * w + x) * 4) as usize;
                        for c in 0..4 {
                            acc[c] += u32::from(src[i + c]);
                        }
                        n += 1;
                    }
                }
                let o = ((sy * sw + sx) * 4) as usize;
                for c in 0..4 {
                    dst[o + c] = ((acc[c] + n / 2) / n.max(1)) as u8;
                }
            }
        }
    }
    let r = (f64::from(radius) / f64::from(k)).round() as i32;
    accel::box_blur(&mut small, r, passes);
    let kf = f64::from(k);
    let (sw, sh) = (i64::from(sw), i64::from(sh));
    let sd = small.data();
    let dst = pm.data_mut();
    for y in 0..h {
        let fy = (f64::from(y) + 0.5) / kf - 0.5;
        let iy = fy.floor();
        let ty = fy - iy;
        let (y0, y1) = (
            (iy as i64).clamp(0, sh - 1),
            (iy as i64 + 1).clamp(0, sh - 1),
        );
        for x in 0..w {
            let fx = (f64::from(x) + 0.5) / kf - 0.5;
            let ix = fx.floor();
            let tx = fx - ix;
            let (x0, x1) = (
                (ix as i64).clamp(0, sw - 1),
                (ix as i64 + 1).clamp(0, sw - 1),
            );
            let p = |xx: i64, yy: i64, c: usize| f64::from(sd[((yy * sw + xx) * 4) as usize + c]);
            let o = ((y * w + x) * 4) as usize;
            let mut v: [u8; 4] = std::array::from_fn(|c| {
                let top = p(x0, y0, c) * (1.0 - tx) + p(x1, y0, c) * tx;
                let bottom = p(x0, y1, c) * (1.0 - tx) + p(x1, y1, c) * tx;
                (top * (1.0 - ty) + bottom * ty).round().clamp(0.0, 255.0) as u8
            });
            // Premultiplied: no channel above the alpha.
            let a = v[3];
            for c in &mut v[..3] {
                *c = (*c).min(a);
            }
            dst[o..o + 4].copy_from_slice(&v);
        }
    }
}

/// Turn a layer holding a shadow's silhouette in white into the shadow:
/// feathered, faded along a perspective shadow, in its colour and
/// opacity. `b` is the box the shadow is measured on.
fn shade_shadow(
    layer: &mut Pixmap,
    sh: &tracedraw_core::Shadow,
    b: Rect,
    screen: Affine,
    zoom: f64,
) {
    use tracedraw_core::shadow::FeatherDirection;
    let blur = if sh.blur.is_finite() { sh.blur } else { 0.0 };
    let radius = (blur * zoom).round() as i32;
    let feathered = radius > 0;
    if feathered {
        match sh.feather_direction {
            // One even pass about as wide as the three passes together.
            FeatherDirection::Average => {
                accel::box_blur(layer, ((radius as f64) * 1.7).round().min(110.0) as i32, 1)
            }
            _ => wide_box_blur(layer, radius, 3),
        }
    }
    // The fade of a perspective shadow, in pixels.
    let fade = sh.fade_axis(b).map(|(a, e, f)| {
        let (a, e) = (screen * a, screen * e);
        let d = e - a;
        (a, d, d.hypot2().max(1e-9), f)
    });
    let opacity = sh.opacity.clamp(0.0, 1.0);
    let [cr, cg, cb] = sh.color.to_rgb8();
    let width = layer.width() as usize;
    for (i, px) in layer.pixels_mut().iter_mut().enumerate() {
        let cov = px.alpha();
        if cov == 0 {
            continue;
        }
        let mut a = cov as f64 / 255.0;
        if feathered {
            a = sh.feather_value(a);
        }
        if let Some((pa, d, len2, f)) = fade {
            let x = (i % width) as f64 + 0.5;
            let y = (i / width) as f64 + 0.5;
            let t = ((x - pa.x) * d.x + (y - pa.y) * d.y) / len2;
            a *= 1.0 - f * t.clamp(0.0, 1.0);
        }
        let a = (a * opacity).clamp(0.0, 1.0);
        let c = tiny_skia::ColorU8::from_rgba(cr, cg, cb, (a * 255.0).round() as u8);
        *px = c.premultiply();
    }
}

/// The drop shadow of `shape` alone as an image at `dpi`, with the page
/// rectangle it covers (Break Drop Shadow Apart). The shadow's opacity and
/// fade are in the pixels; its merge mode is not. `None` when the shape
/// has no shadow or it covers nothing.
pub fn render_shadow_image(shape: &Shape, dpi: f64) -> Option<(Rect, Pixmap)> {
    let sh = shape.shadow.as_ref()?;
    let path = shape.page_path();
    if path.elements().is_empty() || !(dpi.is_finite() && dpi > 0.0) {
        return None;
    }
    // Measured on the outline as drawn and reaching round all the
    // object's effects draw, as on the page (#2215, #2217).
    let (b, extent) = cast_boxes(shape, Affine::IDENTITY)?;
    let pad = if sh.blur.is_finite() {
        sh.blur.max(0.0) * 3.0
    } else {
        0.0
    };
    let area = sh
        .silhouette_transform(b)
        .transform_rect_bbox(extent)
        .inflate(pad, pad);
    let zoom = dpi / 25.4;
    let w = (area.width() * zoom).ceil();
    let h = (area.height() * zoom).ceil();
    if !(w >= 1.0 && h >= 1.0 && w * h <= 64.0e6) {
        return None;
    }
    let mut pm = Pixmap::new(w as u32, h as u32)?;
    // Page (y up) to the image (y down), the area's top left at 0, 0.
    let screen = Affine::new([zoom, 0.0, 0.0, -zoom, -area.x0 * zoom, area.y1 * zoom]);
    let drawn = if !shape.effects.is_empty() {
        group_shadow_layer(&mut pm, sh, shape, Affine::IDENTITY, b, screen, zoom, true);
        true
    } else if matches!(
        shape.kind,
        ShapeKind::Group { .. } | ShapeKind::ClipFrame { .. }
    ) {
        match group_shadow_box(shape, Affine::IDENTITY) {
            Some(b) => {
                group_shadow_layer(&mut pm, sh, shape, Affine::IDENTITY, b, screen, zoom, true);
                true
            }
            None => false,
        }
    } else if let Some(img) = match &shape.kind {
        ShapeKind::Bitmap { png, .. } => image_cache::decode_level(png, 0),
        _ => None,
    } {
        // A picture casts the shadow of what its pixels cover, as on the
        // page (#2027): a picture with a transparent background cast the
        // shadow of its whole box in PDF output (d#2596).
        if let ShapeKind::Bitmap {
            rect,
            width_px,
            height_px,
            ..
        } = &shape.kind
        {
            bitmap_silhouette(
                &mut pm,
                sh,
                b,
                screen,
                shape.transform,
                *rect,
                (*width_px, *height_px),
                &img,
            );
            shade_shadow(&mut pm, sh, b, screen, zoom);
        }
        true
    } else {
        let shown = Shown {
            shape,
            path: &path,
            object: shape.transform,
            rule: shape_fill_rule(shape, false),
        };
        shadow_layer(&mut pm, sh, shown, screen, zoom, true)
    };
    if !drawn {
        return None;
    }
    let exact = Rect::new(area.x0, area.y1 - h / zoom, area.x0 + w / zoom, area.y1);
    Some((exact, pm))
}

/// The inner shadow of `shape` (its first `Effect::InnerShadow`, `shape`
/// in page space) as a picture at `dpi`, for Break Shadow Apart (#2893):
/// the shadow's density in the alpha of each pixel, its colour in all of
/// them, over the box of the object; with the box it covers on the page.
/// The merge mode is left to the caller. `None` when the object has no
/// inner shadow or casts nothing.
pub fn render_inner_shadow_image(shape: &Shape, dpi: f64) -> Option<(Rect, Pixmap)> {
    let (offset, color, opacity, blur) = shape.effects.iter().find_map(|e| match e {
        tracedraw_core::live::Effect::InnerShadow {
            offset,
            color,
            opacity,
            blur,
            ..
        } => Some((*offset, *color, *opacity, *blur)),
        _ => None,
    })?;
    let opacity = if opacity.is_finite() {
        opacity.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let path = shape.page_path();
    if opacity <= 0.0 || path.elements().is_empty() || !(dpi.is_finite() && dpi > 0.0) {
        return None;
    }
    let area = path.bounding_box();
    let zoom = dpi / 25.4;
    let w = (area.width() * zoom).ceil();
    let h = (area.height() * zoom).ceil();
    if !(w >= 1.0 && h >= 1.0 && w * h <= 64.0e6) {
        return None;
    }
    let blur = if blur.is_finite() { blur.max(0.0) } else { 0.0 };
    let radius = (blur * zoom).round().min(64.0) as i32;
    // Room round the object for the blur to draw what lies outside.
    let pad = radius.max(0) as u32 * 3 + 2;
    let (lw, lh) = (w as u32 + 2 * pad, h as u32 + 2 * pad);
    let mut layer = Pixmap::new(lw, lh)?;
    // Page (y up) to the layer (y down), the object's box at `pad`.
    let screen = Affine::new([
        zoom,
        0.0,
        0.0,
        -zoom,
        f64::from(pad) - area.x0 * zoom,
        f64::from(pad) + area.y1 * zoom,
    ]);
    let rule = shape_fill_rule(shape, false);
    // Everything outside the object, moved by the offset, casts.
    layer.fill(tiny_skia::Color::WHITE);
    let moved = screen * (Affine::translate((offset.x, offset.y)) * path.clone());
    if let Some(sk) = to_sk_path(&moved) {
        let mut clear = Paint::default();
        clear.anti_alias = true;
        clear.blend_mode = tiny_skia::BlendMode::Clear;
        layer.fill_path(&sk, &clear, rule, Transform::identity(), None);
    }
    if radius > 0 {
        accel::box_blur(&mut layer, radius, 3);
    }
    // Kept where the object covers.
    let mut mask = tiny_skia::Mask::new(lw, lh)?;
    if let Some(sk) = to_sk_path(&(screen * path)) {
        mask.fill_path(&sk, rule, true, Transform::identity());
    }
    let [cr, cg, cb] = color.to_rgb8();
    let mut out = Pixmap::new(w as u32, h as u32)?;
    let (ow, pad) = (w as usize, pad as usize);
    let src = layer.pixels();
    let m = mask.data();
    for (i, px) in out.pixels_mut().iter_mut().enumerate() {
        let (x, y) = (i % ow + pad, i / ow + pad);
        let j = y * lw as usize + x;
        let a = f64::from(src[j].alpha()) / 255.0 * f64::from(m[j]) / 255.0 * opacity;
        *px = tiny_skia::ColorU8::from_rgba(cr, cg, cb, (a * 255.0).round() as u8).premultiply();
    }
    let exact = Rect::new(area.x0, area.y1 - h / zoom, area.x0 + w / zoom, area.y1);
    Some((exact, out))
}

/// The blend mode a merge mode draws with. Subtract, Texturize and
/// Lightness have no blend of their own and get the nearest;
/// [`composite_layer`] draws them exactly.
pub fn merge_blend(merge: tracedraw_core::live::MergeMode) -> tiny_skia::BlendMode {
    use tiny_skia::BlendMode as B;
    use tracedraw_core::live::MergeMode as M;
    match merge {
        M::Normal | M::Red | M::Green | M::Blue => B::SourceOver,
        M::Add => B::Plus,
        M::Subtract | M::Multiply | M::Texturize => B::Multiply,
        M::Difference => B::Difference,
        M::Divide => B::Screen,
        M::IfLighter => B::Lighten,
        M::IfDarker => B::Darken,
        M::Hue => B::Hue,
        M::Saturation => B::Saturation,
        M::Color => B::Color,
        M::Lightness => B::Luminosity,
        M::Invert => B::Exclusion,
        M::And | M::Or | M::Xor => B::Xor,
        M::Behind => B::DestinationOver,
        M::Screen => B::Screen,
        M::Overlay => B::Overlay,
        M::SoftLight => B::SoftLight,
        M::HardLight => B::HardLight,
        M::ColorDodge => B::ColorDodge,
        M::ColorBurn => B::ColorBurn,
        M::Exclusion => B::Exclusion,
    }
}

/// Draw the premultiplied `layer`, its top left at `at`, over `target`
/// with `merge`: the blend of [`merge_blend`], except Subtract (the
/// backdrop and the source added, less white: `b + s - 1`, so white
/// leaves the backdrop as it is and darker source colours darken it,
/// #2512), Texturize (the backdrop multiplied by the source's lightness)
/// and Lightness (the source's HSL lightness with the backdrop's hue and
/// saturation), which are mixed pixel by pixel here.
pub fn composite_layer(
    target: &mut Pixmap,
    layer: &Pixmap,
    at: (i32, i32),
    merge: tracedraw_core::live::MergeMode,
) {
    use tracedraw_core::live::MergeMode as M;
    let mix: fn([f32; 3], [f32; 3]) -> [f32; 3] = match merge {
        M::Subtract => |cb, cs| {
            [
                (cb[0] + cs[0] - 1.0).max(0.0),
                (cb[1] + cs[1] - 1.0).max(0.0),
                (cb[2] + cs[2] - 1.0).max(0.0),
            ]
        },
        M::Texturize => |cb, cs| {
            let l = 0.2126 * cs[0] + 0.7152 * cs[1] + 0.0722 * cs[2];
            [cb[0] * l, cb[1] * l, cb[2] * l]
        },
        M::Lightness => |cb, cs| {
            let l = (cs[0].max(cs[1]).max(cs[2]) + cs[0].min(cs[1]).min(cs[2])) / 2.0;
            set_lightness(cb, l)
        },
        _ => {
            let pp = tiny_skia::PixmapPaint {
                blend_mode: merge_blend(merge),
                ..Default::default()
            };
            target.draw_pixmap(at.0, at.1, layer.as_ref(), &pp, Transform::identity(), None);
            return;
        }
    };
    let (tw, th) = (target.width() as i32, target.height() as i32);
    let (lw, lh) = (layer.width() as i32, layer.height() as i32);
    let x0 = at.0.max(0);
    let y0 = at.1.max(0);
    let x1 = (at.0 + lw).min(tw);
    let y1 = (at.1 + lh).min(th);
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let src = layer.pixels();
    let dst = target.pixels_mut();
    for y in y0..y1 {
        let srow = ((y - at.1) * lw) as usize;
        let drow = (y * tw) as usize;
        for x in x0..x1 {
            let (Some(s), Some(d)) = (
                src.get(srow + (x - at.0) as usize),
                dst.get_mut(drow + x as usize),
            ) else {
                continue;
            };
            let sa = s.alpha();
            if sa == 0 {
                continue;
            }
            let (s, d8) = (s.demultiply(), d.demultiply());
            let (a_s, a_b) = (sa as f32 / 255.0, d8.alpha() as f32 / 255.0);
            let cs = [
                s.red() as f32 / 255.0,
                s.green() as f32 / 255.0,
                s.blue() as f32 / 255.0,
            ];
            let cb = [
                d8.red() as f32 / 255.0,
                d8.green() as f32 / 255.0,
                d8.blue() as f32 / 255.0,
            ];
            let b = mix(cb, cs);
            // The compositing formula: the source where only it is, the
            // backdrop where only it is, the mix where both are.
            let a_o = a_s + a_b - a_s * a_b;
            let mut out = [0u8; 3];
            for i in 0..3 {
                let c = cs[i] * a_s * (1.0 - a_b) + cb[i] * a_b * (1.0 - a_s) + a_s * a_b * b[i];
                out[i] = ((c / a_o.max(1e-6)).clamp(0.0, 1.0) * 255.0).round() as u8;
            }
            if let Some(np) =
                tiny_skia::ColorU8::from_rgba(out[0], out[1], out[2], (a_o * 255.0).round() as u8)
                    .premultiply()
                    .into()
            {
                *d = np;
            }
        }
    }
}

/// `c` with its HSL lightness set to `l`, its hue and saturation kept.
fn set_lightness(c: [f32; 3], l: f32) -> [f32; 3] {
    let max = c[0].max(c[1]).max(c[2]);
    let min = c[0].min(c[1]).min(c[2]);
    let lb = (max + min) / 2.0;
    let d = max - min;
    if d < 1e-6 {
        return [l, l, l];
    }
    let s = d / (1.0 - (2.0 * lb - 1.0).abs()).max(1e-6);
    let h = if max == c[0] {
        ((c[1] - c[2]) / d).rem_euclid(6.0)
    } else if max == c[1] {
        (c[2] - c[0]) / d + 2.0
    } else {
        (c[0] - c[1]) / d + 4.0
    };
    let chroma = (1.0 - (2.0 * l - 1.0).abs()) * s.min(1.0);
    let x = chroma * (1.0 - (h.rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match h as i32 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let m = l - chroma / 2.0;
    [r + m, g + m, b + m]
}

/// Three-pass box blur (approximates a Gaussian) on premultiplied RGBA.
pub fn box_blur(pm: &mut Pixmap, radius: i32) {
    box_blur_passes(pm, radius, 3);
}

/// Box blur with `passes` passes each way on premultiplied RGBA.
pub fn box_blur_passes(pm: &mut Pixmap, radius: i32, passes: usize) {
    let w = pm.width() as usize;
    let h = pm.height() as usize;
    if w == 0 || h == 0 {
        return;
    }
    let r = radius.max(1) as usize;
    let data = pm.data_mut();
    let mut tmp = vec![0u8; data.len()];
    for _ in 0..passes {
        blur_pass(data, &mut tmp, w, h, r, true);
        blur_pass(&tmp, data, w, h, r, false);
    }
}

fn blur_pass(src: &[u8], dst: &mut [u8], w: usize, h: usize, r: usize, horizontal: bool) {
    let (outer, inner) = if horizontal { (h, w) } else { (w, h) };
    let idx = |o: usize, i: usize| -> usize {
        if horizontal {
            (o * w + i) * 4
        } else {
            (i * w + o) * 4
        }
    };
    let span = (2 * r + 1) as u32;
    for o in 0..outer {
        let mut acc = [0u32; 4];
        // Prime the window with the first pixel repeated, like edge clamping.
        for i in 0..=r {
            let p = idx(o, i.min(inner - 1));
            for c in 0..4 {
                acc[c] += src[p + c] as u32;
            }
        }
        for _ in 0..r {
            let p = idx(o, 0);
            for c in 0..4 {
                acc[c] += src[p + c] as u32;
            }
        }
        for i in 0..inner {
            let d = idx(o, i);
            for c in 0..4 {
                dst[d + c] = (acc[c] / span) as u8;
            }
            let add = idx(o, (i + r + 1).min(inner - 1));
            let sub = idx(o, i.saturating_sub(r));
            for c in 0..4 {
                acc[c] = acc[c] + src[add + c] as u32 - src[sub + c] as u32;
            }
        }
    }
}

/// An outline's own paint when it has one other than a plain colour (a
/// gradient, pattern, texture or mesh), ready to draw in page space: a
/// mesh's nodes are local to the object and go through `transform`.
fn outline_paint(stroke: &Stroke, transform: Affine) -> Option<Fill> {
    match stroke.fill.as_deref()? {
        Fill::None | Fill::Solid(_) => None,
        Fill::Mesh(m) => Some(Fill::Mesh(m.transformed(transform))),
        other => Some(other.clone()),
    }
}

/// Largest side, in pixels, of the grid a shaped fountain measures its
/// distances on.
const SHAPED_GRID: f32 = 384.0;

/// For each pixel of a `w` by `h` grid, the distance in pixels from its
/// centre to the edge of the region `inside` marks (0 outside): exact
/// Euclidean distance to the nearest outside pixel, less the half pixel
/// to the edge between them.
fn edge_distance(inside: &[bool], w: usize, h: usize) -> Vec<f64> {
    const INF: f64 = 1e20;
    let mut g: Vec<f64> = inside.iter().map(|&i| if i { INF } else { 0.0 }).collect();
    if g.len() != w * h || w == 0 || h == 0 {
        return vec![0.0; inside.len()];
    }
    let n = w.max(h);
    let (mut f, mut d) = (vec![0.0; n], vec![0.0; n]);
    let (mut v, mut z) = (vec![0usize; n], vec![0.0; n + 1]);
    for x in 0..w {
        for y in 0..h {
            f[y] = g[y * w + x];
        }
        distance_1d(&f[..h], &mut d[..h], &mut v, &mut z);
        for y in 0..h {
            g[y * w + x] = d[y];
        }
    }
    for y in 0..h {
        f[..w].copy_from_slice(&g[y * w..(y + 1) * w]);
        distance_1d(&f[..w], &mut d[..w], &mut v, &mut z);
        g[y * w..(y + 1) * w].copy_from_slice(&d[..w]);
    }
    g.iter()
        .zip(inside)
        .map(|(s, &i)| if i { (s.sqrt() - 0.5).max(0.0) } else { 0.0 })
        .collect()
}

/// One pass of the squared distance transform: `d[q] = min over p of
/// (q - p)^2 + f[p]`, by the lower envelope of the parabolas.
fn distance_1d(f: &[f64], d: &mut [f64], v: &mut [usize], z: &mut [f64]) {
    let n = f.len();
    if n == 0 {
        return;
    }
    let sq = |i: usize| (i * i) as f64;
    let mut k = 0usize;
    v[0] = 0;
    z[0] = f64::NEG_INFINITY;
    z[1] = f64::INFINITY;
    for q in 1..n {
        loop {
            let p = v[k];
            let s = ((f[q] + sq(q)) - (f[p] + sq(p))) / (2.0 * (q as f64 - p as f64));
            // z[0] is minus infinity, so this stops at the first parabola.
            if k > 0 && s <= z[k] {
                k -= 1;
                continue;
            }
            k += 1;
            v[k] = q;
            z[k] = s;
            z[k + 1] = f64::INFINITY;
            break;
        }
    }
    let mut k = 0usize;
    for (q, out) in d.iter_mut().enumerate() {
        while z[k + 1] < q as f64 {
            k += 1;
        }
        let p = v[k];
        let dq = q as f64 - p as f64;
        *out = dq * dq + f[p];
    }
}

/// Cheap value noise in 0..1 for texture fills.
/// The grid of a procedural fill over `w` by `h` pixels: every pixel up
/// to about a third of a megapixel, then every second or third.
fn procedural_step(w: u32, h: u32) -> u32 {
    match w as u64 * h as u64 {
        0..=350_000 => 1,
        350_001..=1_400_000 => 2,
        _ => 3,
    }
}

/// Fill the rows of `px` (`w` pixels wide) with `row`, on several threads
/// where there are some.
fn for_rows(
    px: &mut [tiny_skia::PremultipliedColorU8],
    w: usize,
    row: &(impl Fn(u32, &mut [tiny_skia::PremultipliedColorU8]) + Sync),
) {
    if w == 0 {
        return;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
            .min(8);
        let rows = px.len() / w;
        if threads > 1 && rows >= 64 {
            let per = rows.div_ceil(threads);
            std::thread::scope(|scope| {
                for (k, chunk) in px.chunks_mut(per * w).enumerate() {
                    scope.spawn(move || {
                        for (i, line) in chunk.chunks_mut(w).enumerate() {
                            row((k * per + i) as u32, line);
                        }
                    });
                }
            });
            return;
        }
    }
    for (y, line) in px.chunks_mut(w).enumerate() {
        row(y as u32, line);
    }
}

/// The largest mesh picture kept, in pixels; past it only the part on
/// the canvas is worked out, and not kept.
const MESH_RASTER_PIXELS: u32 = 8_000_000;
/// Triangles gathered before they are painted: bounds the memory a mesh
/// takes while it is drawn (96 bytes a triangle).
const MESH_TRIANGLE_BATCH: usize = 1 << 17;
/// The most pieces a cell's side is cut into.
const MESH_MAX_SUBDIVISIONS: usize = 256;
/// Pixels of a cell's side per piece.
const MESH_CHORD: f64 = 4.0;

/// The box round a mesh's nodes and handles.
fn mesh_bounds(m: &tracedraw_core::Mesh) -> Option<Rect> {
    m.nodes
        .iter()
        .flat_map(|n| std::iter::once(n.pos).chain(n.handles.into_iter().flatten()))
        .filter(|p| p.x.is_finite() && p.y.is_finite())
        .map(|p| Rect::from_points(p, p))
        .reduce(|a, b| a.union(b))
}

/// A corner of a mesh triangle: its place in the picture and its colour
/// and opacity (0 to 1).
#[derive(Clone, Copy)]
struct MeshVertex {
    x: f64,
    y: f64,
    rgba: [f32; 4],
}

/// How many pieces the sides of a cell are cut into, from its size in
/// pixels (`MESH_CHORD` pixels a piece, at most `MESH_MAX_SUBDIVISIONS`),
/// or `None` for a cell that misses the picture of `w` x `h` pixels:
/// `cell` is the box of its four side curves, grown by a few per cent
/// of the cell's size and a chord, as the patch inside can reach a
/// little past its sides.
fn mesh_cell_subdivisions(cell: Rect, size: f64, w: usize, h: usize) -> Option<usize> {
    let grow = size * 0.05 + MESH_CHORD;
    if !(cell.x1 + grow >= -1.0
        && cell.y1 + grow >= -1.0
        && cell.x0 - grow <= w as f64 + 1.0
        && cell.y0 - grow <= h as f64 + 1.0)
    {
        return None;
    }
    Some(((size / MESH_CHORD).ceil() as usize).clamp(1, MESH_MAX_SUBDIVISIONS))
}

/// The triangles of the mesh `m` (its nodes taken through `to_raster`,
/// which maps them to the pixels of a picture `w` x `h`), handed to
/// `paint` in batches of at most `MESH_TRIANGLE_BATCH`: every cell as a
/// grid of triangles small enough for the curves to look smooth, the
/// cells and pieces off the picture left out, so a mesh far larger than
/// the picture costs only what shows.
fn mesh_triangles(
    m: &tracedraw_core::Mesh,
    to_raster: Affine,
    w: usize,
    h: usize,
    mut paint: impl FnMut(&[[MeshVertex; 3]]),
) {
    use tracedraw_core::geometry::ParamCurve;
    let cols = m.cols as usize + 1;
    let rgb: Vec<[f32; 3]> = m.nodes.iter().map(|n| n.color.to_rgb_f32()).collect();
    // Smooth colour: each channel through the 4 x 4 nodes around the
    // cell with Catmull-Rom weights (rows and columns past the edge
    // repeat the edge).
    let weights = |t: f64| {
        let (t2, t3) = (t * t, t * t * t);
        [
            (-t3 + 2.0 * t2 - t) / 2.0,
            (3.0 * t3 - 5.0 * t2 + 2.0) / 2.0,
            (-3.0 * t3 + 4.0 * t2 + t) / 2.0,
            (t3 - t2) / 2.0,
        ]
    };
    let smooth_at = |r: usize, c: usize, u: f64, v: f64| -> Option<[f32; 4]> {
        let (wu, wv) = (weights(u), weights(v));
        let mut acc = [0.0f64; 4];
        for (j, wj) in wv.iter().enumerate() {
            let rr = (r + j).saturating_sub(1).min(m.rows as usize);
            for (i, wi) in wu.iter().enumerate() {
                let cc = (c + i).saturating_sub(1).min(m.cols as usize);
                let k = rr * cols + cc;
                let (n, c3) = (m.nodes.get(k)?, rgb.get(k)?);
                let wgt = wi * wj;
                for ch in 0..3 {
                    acc[ch] += wgt * c3[ch] as f64;
                }
                acc[3] += wgt * n.alpha as f64;
            }
        }
        Some([
            acc[0].clamp(0.0, 1.0) as f32,
            acc[1].clamp(0.0, 1.0) as f32,
            acc[2].clamp(0.0, 1.0) as f32,
            acc[3].clamp(0.0, 1.0) as f32,
        ])
    };
    let bilinear = |r: usize, c: usize, u: f64, v: f64| -> Option<[f32; 4]> {
        let at = |rr: usize, cc: usize| {
            let k = rr * cols + cc;
            let n = m.nodes.get(k)?;
            let c3 = rgb.get(k)?;
            Some([c3[0], c3[1], c3[2], n.alpha])
        };
        let (a, b, d, e) = (at(r, c)?, at(r, c + 1)?, at(r + 1, c)?, at(r + 1, c + 1)?);
        let (u, v) = (u as f32, v as f32);
        let mut out = [0.0f32; 4];
        for (ch, o) in out.iter_mut().enumerate() {
            let top = a[ch] + (b[ch] - a[ch]) * u;
            let bottom = d[ch] + (e[ch] - d[ch]) * u;
            *o = top + (bottom - top) * v;
        }
        Some(out)
    };
    let mut tris: Vec<[MeshVertex; 3]> = Vec::new();
    let mut grid: Vec<MeshVertex> = Vec::new();
    let (mut tops, mut bottoms, mut lefts, mut rights) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for r in 0..m.rows {
        for c in 0..m.cols {
            // The cell's four sides, in pixels.
            let (Some(top), Some(bottom), Some(left), Some(right)) = (
                m.edge(r, c, true),
                m.edge(r + 1, c, true),
                m.edge(r, c, false),
                m.edge(r, c + 1, false),
            ) else {
                continue;
            };
            let (top, bottom, left, right) = (
                to_raster * top,
                to_raster * bottom,
                to_raster * left,
                to_raster * right,
            );
            let cell = [top, bottom, left, right]
                .iter()
                .map(|e| e.bounding_box())
                .reduce(|a, b| a.union(b))
                .unwrap_or_default();
            // Subdivisions from the cell's size in pixels: a chord every
            // few pixels keeps the curves and the colour smooth.
            let size = (top.p0 - top.p3)
                .hypot()
                .max((bottom.p0 - bottom.p3).hypot())
                .max((left.p0 - left.p3).hypot())
                .max((right.p0 - right.p3).hypot());
            let Some(k) = mesh_cell_subdivisions(cell, size, w, h) else {
                continue;
            };
            // The sides evaluated once each; the Coons patch mixes them.
            let steps = |curve: tracedraw_core::geometry::CubicBez, out: &mut Vec<Point>| {
                out.clear();
                out.extend((0..=k).map(|i| curve.eval(i as f64 / k as f64)));
            };
            steps(top, &mut tops);
            steps(bottom, &mut bottoms);
            steps(left, &mut lefts);
            steps(right, &mut rights);
            let (a, b, d, e) = (top.p0, top.p3, bottom.p0, bottom.p3);
            grid.clear();
            let mut ok = true;
            'cell: for j in 0..=k {
                let v = j as f64 / k as f64;
                for i in 0..=k {
                    let u = i as f64 / k as f64;
                    let colour = if m.smooth {
                        smooth_at(r as usize, c as usize, u, v)
                    } else {
                        bilinear(r as usize, c as usize, u, v)
                    };
                    let (Some(rgba), Some(&t), Some(&bo), Some(&l), Some(&ri)) = (
                        colour,
                        tops.get(i),
                        bottoms.get(i),
                        lefts.get(j),
                        rights.get(j),
                    ) else {
                        ok = false;
                        break 'cell;
                    };
                    let p = t.to_vec2() * (1.0 - v)
                        + bo.to_vec2() * v
                        + l.to_vec2() * (1.0 - u)
                        + ri.to_vec2() * u
                        - a.to_vec2() * ((1.0 - u) * (1.0 - v))
                        - b.to_vec2() * (u * (1.0 - v))
                        - d.to_vec2() * ((1.0 - u) * v)
                        - e.to_vec2() * (u * v);
                    grid.push(MeshVertex {
                        x: p.x,
                        y: p.y,
                        rgba,
                    });
                }
            }
            if !ok {
                continue;
            }
            for j in 0..k {
                for i in 0..k {
                    let at = |ii: usize, jj: usize| grid.get(jj * (k + 1) + ii).copied();
                    let (Some(v00), Some(v10), Some(v01), Some(v11)) =
                        (at(i, j), at(i + 1, j), at(i, j + 1), at(i + 1, j + 1))
                    else {
                        continue;
                    };
                    // A piece off the picture is left out.
                    let q = [v00, v10, v01, v11];
                    if q.iter().all(|p| p.x < -1.0)
                        || q.iter().all(|p| p.x > w as f64 + 1.0)
                        || q.iter().all(|p| p.y < -1.0)
                        || q.iter().all(|p| p.y > h as f64 + 1.0)
                    {
                        continue;
                    }
                    tris.push([v00, v10, v11]);
                    tris.push([v00, v11, v01]);
                    if tris.len() >= MESH_TRIANGLE_BATCH {
                        paint(&tris);
                        tris.clear();
                    }
                }
            }
        }
    }
    if !tris.is_empty() {
        paint(&tris);
    }
}

/// Draw the mesh `m` (its nodes taken through `to_raster`, which maps
/// them to the picture's pixels) into `pm`: its triangles painted batch
/// by batch (see [`mesh_triangles`]), the colour mixed per pixel, large
/// batches on several threads, each painting a band of rows; then the
/// picture's edge is grown by a pixel so the path it is drawn through
/// finds colour under its smoothed edge, and the outermost ring of
/// pixels is cleared so nothing spreads outside the grid.
fn rasterise_mesh(m: &tracedraw_core::Mesh, to_raster: Affine, pm: &mut Pixmap) {
    let (w, h) = (pm.width() as usize, pm.height() as usize);
    let mut buf: Vec<[u8; 4]> = vec![[0; 4]; w * h];
    mesh_triangles(m, to_raster, w, h, |tris| {
        raster_triangles(&mut buf, w, tris)
    });
    for (i, px) in pm.pixels_mut().iter_mut().enumerate() {
        let (x, y) = (i % w, i / w);
        let ring = x == 0 || y == 0 || x + 1 == w || y + 1 == h;
        let mut c = if ring { [0; 4] } else { buf[i] };
        if c[3] == 0 && !ring {
            for n in [buf[i + 1], buf[i - 1], buf[i + w], buf[i - w]] {
                if n[3] > c[3] {
                    c = n;
                }
            }
        }
        *px = tiny_skia::ColorU8::from_rgba(c[0], c[1], c[2], c[3]).premultiply();
    }
}

/// Paint the triangles into `buf` (`w` pixels wide): a large batch has
/// the rows cut into bands, one thread each, every band painting the
/// triangles that reach it; a small one is painted on this thread.
fn raster_triangles(buf: &mut [[u8; 4]], w: usize, tris: &[[MeshVertex; 3]]) {
    if w == 0 || tris.is_empty() {
        return;
    }
    let rows = buf.len() / w;
    let paint_band = |band: &mut [[u8; 4]], y0: usize| {
        let band_rows = band.len() / w;
        for t in tris {
            raster_triangle(band, w, y0, band_rows, *t);
        }
    };
    #[cfg(not(target_arch = "wasm32"))]
    {
        let threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
            .min(8);
        if threads > 1 && rows >= 64 && tris.len() >= 4096 {
            let per = rows.div_ceil(threads);
            std::thread::scope(|scope| {
                for (k, band) in buf.chunks_mut(per * w).enumerate() {
                    scope.spawn(move || paint_band(band, k * per));
                }
            });
            return;
        }
    }
    paint_band(buf, 0);
}

/// Paint a triangle into the band `buf` of `band_rows` rows (`w` pixels
/// wide, starting at row `y0` of the picture), the colour of each pixel
/// mixed from the corners by where it lies; the edges reach half a pixel
/// out so neighbouring triangles leave no gaps between them.
fn raster_triangle(buf: &mut [[u8; 4]], w: usize, y0: usize, band_rows: usize, v: [MeshVertex; 3]) {
    let area = (v[1].x - v[0].x) * (v[2].y - v[0].y) - (v[2].x - v[0].x) * (v[1].y - v[0].y);
    if !area.is_finite() || area.abs() < 1e-12 {
        return;
    }
    const REACH: f64 = 0.55;
    let min_x = v.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
    let max_x = v.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
    let min_y = v.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
    let max_y = v.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
    if !(min_x.is_finite() && max_x.is_finite() && min_y.is_finite() && max_y.is_finite()) {
        return;
    }
    // The pixels whose centres can be within reach of the triangle, in
    // this band.
    let x_start = (min_x - REACH - 0.5).ceil().max(0.0) as usize;
    let x_end = (max_x + REACH - 0.5).floor().min(w as f64 - 1.0);
    let y_start = (min_y - REACH - 0.5).ceil().max(y0 as f64) as usize;
    let y_end = (max_y + REACH - 0.5)
        .floor()
        .min((y0 + band_rows) as f64 - 1.0);
    if x_end < x_start as f64 || y_end < y_start as f64 {
        return;
    }
    let (x_end, y_end) = (x_end as usize, y_end as usize);
    // Edge functions, signed by the triangle's turn, in pixels (divided
    // by the edge's length), and their change per pixel across.
    let sign = area.signum();
    let aa = area.abs();
    let edge_len = |i: usize, j: usize| ((v[j].x - v[i].x).hypot(v[j].y - v[i].y)).max(1e-9);
    let lens = [edge_len(1, 2), edge_len(2, 0), edge_len(0, 1)];
    let at = |px: f64, py: f64| {
        [
            ((v[1].x - px) * (v[2].y - py) - (v[2].x - px) * (v[1].y - py)) * sign,
            ((v[2].x - px) * (v[0].y - py) - (v[0].x - px) * (v[2].y - py)) * sign,
            ((v[0].x - px) * (v[1].y - py) - (v[1].x - px) * (v[0].y - py)) * sign,
        ]
    };
    let dx = [
        (v[1].y - v[2].y) * sign,
        (v[2].y - v[0].y) * sign,
        (v[0].y - v[1].y) * sign,
    ];
    let limits = [-REACH * lens[0], -REACH * lens[1], -REACH * lens[2]];
    for y in y_start..=y_end {
        let py = y as f64 + 0.5;
        let mut wgt = at(x_start as f64 + 0.5, py);
        let row = &mut buf[(y - y0) * w..(y - y0 + 1) * w];
        for out in &mut row[x_start..=x_end] {
            if wgt[0] >= limits[0] && wgt[1] >= limits[1] && wgt[2] >= limits[2] {
                let (b0, b1, b2) = (
                    (wgt[0] / aa).clamp(0.0, 1.0),
                    (wgt[1] / aa).clamp(0.0, 1.0),
                    (wgt[2] / aa).clamp(0.0, 1.0),
                );
                let sum = (b0 + b1 + b2).max(1e-9);
                let (b0, b1, b2) = ((b0 / sum) as f32, (b1 / sum) as f32, (b2 / sum) as f32);
                for (ch, o) in out.iter_mut().enumerate() {
                    let x = v[0].rgba[ch] * b0 + v[1].rgba[ch] * b1 + v[2].rgba[ch] * b2;
                    *o = (x.clamp(0.0, 1.0) * 255.0).round() as u8;
                }
            }
            wgt[0] += dx[0];
            wgt[1] += dx[1];
            wgt[2] += dx[2];
        }
    }
}

/// The last procedural fills worked out, newest last: up to this many,
/// and up to this many pixels together. Shared by every thread (the
/// application draws a slow page on a thread of its own), so a picture
/// worked out once serves every later render.
const PROCEDURAL_CACHE: usize = 256;
const PROCEDURAL_CACHE_PIXELS: usize = 24_000_000;

static PROCEDURAL: std::sync::Mutex<Vec<(String, std::sync::Arc<Pixmap>)>> =
    std::sync::Mutex::new(Vec::new());

fn procedural_cache_get(key: &str) -> Option<std::sync::Arc<Pixmap>> {
    let mut c = PROCEDURAL.lock().unwrap_or_else(|e| e.into_inner());
    let i = c.iter().position(|(k, _)| k == key)?;
    let hit = c.remove(i);
    let pm = hit.1.clone();
    c.push(hit);
    Some(pm)
}

/// The keys of pictures being worked out, so that threads asking for the
/// same picture at once (the bands of one render) wait for the first
/// instead of each working it out.
static IN_FLIGHT: std::sync::Mutex<Vec<(String, std::sync::Arc<std::sync::Mutex<()>>)>> =
    std::sync::Mutex::new(Vec::new());

/// The cached picture of `key`, or the one `compute` gives, kept under
/// that key. A second thread asking for a key being worked out waits for
/// it and takes the result.
fn procedural_cached(
    key: &str,
    compute: impl FnOnce() -> Option<Pixmap>,
) -> Option<std::sync::Arc<Pixmap>> {
    if let Some(pm) = procedural_cache_get(key) {
        return Some(pm);
    }
    let gate = {
        let mut f = IN_FLIGHT.lock().unwrap_or_else(|e| e.into_inner());
        match f.iter().find(|(k, _)| k == key) {
            Some((_, g)) => g.clone(),
            None => {
                let g = std::sync::Arc::new(std::sync::Mutex::new(()));
                f.push((key.to_string(), g.clone()));
                g
            }
        }
    };
    let held = gate.lock().unwrap_or_else(|e| e.into_inner());
    let pm = match procedural_cache_get(key) {
        Some(pm) => Some(pm),
        None => {
            let pm = compute().map(std::sync::Arc::new);
            if let Some(pm) = &pm {
                procedural_cache_put(key.to_string(), pm.clone());
            }
            pm
        }
    };
    {
        let mut f = IN_FLIGHT.lock().unwrap_or_else(|e| e.into_inner());
        f.retain(|(k, _)| k != key);
    }
    drop(held);
    pm
}

/// Forget every kept procedural picture (when the last drawing closes,
/// so the memory goes back).
pub fn clear_procedural_cache() {
    PROCEDURAL.lock().unwrap_or_else(|e| e.into_inner()).clear();
}

fn procedural_cache_put(key: String, pm: std::sync::Arc<Pixmap>) {
    let mut c = PROCEDURAL.lock().unwrap_or_else(|e| e.into_inner());
    c.retain(|(k, _)| *k != key);
    c.push((key, pm));
    let mut total: usize = c
        .iter()
        .map(|(_, p)| p.width() as usize * p.height() as usize)
        .sum();
    while c.len() > PROCEDURAL_CACHE || (total > PROCEDURAL_CACHE_PIXELS && c.len() > 1) {
        let (_, old) = c.remove(0);
        total -= old.width() as usize * old.height() as usize;
    }
}

/// The keys of the procedural fills the cache holds.
#[cfg(test)]
fn procedural_cache_keys() -> Vec<String> {
    PROCEDURAL
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .map(|(k, _)| k.clone())
        .collect()
}

fn noise(x: f64, y: f64, seed: u32) -> f64 {
    fn hash(ix: i64, iy: i64, seed: u32) -> f64 {
        let mut h = (ix as u64).wrapping_mul(0x9E3779B97F4A7C15)
            ^ (iy as u64).wrapping_mul(0xC2B2AE3D27D4EB4F)
            ^ (seed as u64).wrapping_mul(0x165667B19E3779F9);
        h ^= h >> 31;
        h = h.wrapping_mul(0x9E3779B97F4A7C15);
        h ^= h >> 29;
        (h & 0xFFFF) as f64 / 65535.0
    }
    let (ix, iy) = (x.floor() as i64, y.floor() as i64);
    let (fx, fy) = (x - x.floor(), y - y.floor());
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let a = hash(ix, iy, seed);
    let b = hash(ix + 1, iy, seed);
    let c = hash(ix, iy + 1, seed);
    let d = hash(ix + 1, iy + 1, seed);
    let top = a + (b - a) * sx;
    let bot = c + (d - c) * sx;
    top + (bot - top) * sy
}

fn fbm(x: f64, y: f64, seed: u32) -> f64 {
    let mut v = 0.0;
    let mut amp = 0.5;
    let mut f = 1.0;
    for _ in 0..5 {
        v += amp * noise(x * f, y * f, seed);
        amp *= 0.5;
        f *= 2.0;
    }
    v
}

pub fn texture_value(kind: tracedraw_core::TextureKind, x: f64, y: f64, seed: u32) -> f64 {
    match kind {
        tracedraw_core::TextureKind::Clouds => fbm(x, y, seed).clamp(0.0, 1.0),
        tracedraw_core::TextureKind::Marble => ((x + 4.0 * fbm(x, y, seed)) * 2.0).sin().abs(),
        tracedraw_core::TextureKind::Noise => noise(x * 8.0, y * 8.0, seed),
        tracedraw_core::TextureKind::Wood => {
            let r = ((x - 0.5).powi(2) + (y * 0.2).powi(2)).sqrt() * 6.0 + fbm(x, y, seed) * 1.5;
            r.fract()
        }
        tracedraw_core::TextureKind::Holographic | tracedraw_core::TextureKind::HoloGlitter => {
            holo_field(x, y, seed).shade as f64
        }
    }
}

/// The numbers of the holographic texture at one point (feature units):
/// how light the foil is there (0 colour A, 1 colour B), the phase of
/// the rainbow and how strongly it shows.
struct HoloField {
    shade: f32,
    phase: f32,
    strength: f32,
}

fn smoothstep(e0: f64, e1: f64, x: f64) -> f64 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn holo_field(x: f64, y: f64, seed: u32) -> HoloField {
    // The coordinates bent by two slow noise fields, so the bands flow
    // in curls as a film of oil does.
    let wx = fbm(x * 0.7 + 5.2, y * 0.7 + 1.3, seed);
    let wy = fbm(x * 0.7 + 9.1, y * 0.7 + 4.7, seed ^ 0x51ED);
    let (px, py) = (x + 1.8 * wx, y + 1.8 * wy);
    // The film's thickness sets the colour; a slight slope across the
    // object adds the long sweep of a foil lit from one side.
    let film = fbm(px * 0.9, py * 0.9, seed ^ 0xA3C5);
    let phase = film * 1.8 + x * 0.06 + y * 0.04;
    // Broad light and dark patches, as the foil catches the light.
    let sheen = fbm(px * 0.45 + 3.3, py * 0.45 + 7.7, seed ^ 0x7719);
    let shade = smoothstep(0.32, 0.66, sheen);
    // The rainbow shows most in the half tones, less in the brightest
    // highlights and the deepest shadows.
    let strength = 0.15 + 0.85 * (1.0 - (2.0 * shade - 1.0).abs()).powf(0.6);
    HoloField {
        shade: shade as f32,
        phase: phase as f32,
        strength: strength as f32,
    }
}

/// A colour of the spectrum at `phase` (one turn per unit) as a thin
/// film shows it: three cosines a little apart, so the turn runs through
/// gold, magenta, deep blue and cyan more than through green.
fn spectrum(phase: f32) -> [f32; 3] {
    let t = std::f32::consts::TAU * phase;
    [
        0.5 + 0.5 * t.cos(),
        0.5 + 0.5 * (t - 0.9).cos(),
        0.5 + 0.5 * (t - 2.2).cos(),
    ]
}

fn grain_hash(ix: i64, iy: i64, seed: u32, k: u64) -> f32 {
    let mut h = (ix as u64).wrapping_mul(0x9E3779B97F4A7C15)
        ^ (iy as u64).wrapping_mul(0xC2B2AE3D27D4EB4F)
        ^ ((seed as u64) << 8 | k).wrapping_mul(0x165667B19E3779F9);
    h ^= h >> 31;
    h = h.wrapping_mul(0xD6E8FEB86659FD93);
    h ^= h >> 32;
    (h & 0xFFFF) as f32 / 65535.0
}

/// The colour (RGB, 0 to 1) of a holographic texture at `x`, `y`
/// (feature units, as [`texture_value`]): the foil from `dark` to
/// `light`, the spectrum laid over it, and with `glitter` fine grains
/// of spectral colour sparkling on top. The same numbers always give
/// the same colour: the look is fixed, not tied to a viewing angle.
pub fn holo_color(
    x: f64,
    y: f64,
    seed: u32,
    glitter: bool,
    dark: [f32; 3],
    light: [f32; 3],
) -> [f32; 3] {
    let f = holo_field(x, y, seed);
    let spec = spectrum(f.phase);
    // The rainbow is as bright as the foil under it: dim and deep in the
    // dark patches, pale in the light ones.
    let lum = 0.3 + 0.7 * f.shade;
    let k = 0.85 * f.strength;
    let mut c = [0.0f32; 3];
    for i in 0..3 {
        let base = dark[i] + (light[i] - dark[i]) * f.shade;
        let tint = lum * (0.1 + spec[i].powf(1.4));
        c[i] = base + (tint - base) * k;
    }
    if glitter {
        // Square grains, 80 to a feature, about a quarter of them lit.
        // Each catches the light in a hue near the film's there and is
        // brighter or darker than it, so the grain sparkles where the
        // rainbow shows and fades in the plain patches.
        let (gx, gy) = ((x * 80.0).floor() as i64, (y * 80.0).floor() as i64);
        if grain_hash(gx, gy, seed, 1) < 0.25 {
            let g = spectrum(f.phase + 0.35 * (grain_hash(gx, gy, seed, 2) - 0.5));
            let up = grain_hash(gx, gy, seed, 3);
            let a = f.strength * (0.3 + 0.6 * grain_hash(gx, gy, seed, 4));
            let lift = if up < 0.6 { 1.1 } else { 0.45 };
            for i in 0..3 {
                let target = (0.25 + 0.75 * g[i]) * lift;
                c[i] += (target - c[i]) * a;
            }
        }
    }
    c.map(|v| v.clamp(0.0, 1.0))
}

fn bbox(p: &BezPath) -> Rect {
    use tracedraw_core::geometry::Shape as _;
    p.bounding_box()
}

pub fn sk_color(c: Color) -> SkColor {
    let [r, g, b] = c.to_rgb8();
    SkColor::from_rgba8(r, g, b, 255)
}

/// [`sk_color`] at an opacity (0 clear, 1 opaque).
pub fn sk_color_alpha(c: Color, opacity: f32) -> SkColor {
    let [r, g, b] = c.to_rgb8();
    SkColor::from_rgba8(r, g, b, (opacity.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// An open line with arrowheads cut back under them (page space), on
/// screen: `None` when nothing is cut (#2981).
fn headed_line(page_path: &BezPath, stroke: &Stroke, screen: Affine) -> Option<SkPath> {
    let cut = tracedraw_core::style::arrowhead_stroke_path(page_path, stroke)?;
    to_sk_path(&(screen * cut))
}

/// How far from the target a path may reach, pixels, before it is cut
/// at this distance: the rasteriser overflowed on an object millions of
/// pixels long (a rectangle typed 1,000 km tall ended the program, and
/// a smaller one drew nothing). Far beyond any picture drawn.
const MAX_REACH: f64 = (1u32 << 20) as f64;

pub fn to_sk_path(path: &BezPath) -> Option<SkPath> {
    let b = bbox(path);
    let path = &if b.x0 < -MAX_REACH || b.y0 < -MAX_REACH || b.x1 > MAX_REACH || b.y1 > MAX_REACH {
        cut_to_reach(path)
    } else {
        path.clone()
    };
    let mut pb = PathBuilder::new();
    for el in path.elements() {
        match el {
            PathEl::MoveTo(p) => pb.move_to(p.x as f32, p.y as f32),
            PathEl::LineTo(p) => pb.line_to(p.x as f32, p.y as f32),
            PathEl::QuadTo(c, p) => pb.quad_to(c.x as f32, c.y as f32, p.x as f32, p.y as f32),
            PathEl::CurveTo(c1, c2, p) => pb.cubic_to(
                c1.x as f32,
                c1.y as f32,
                c2.x as f32,
                c2.y as f32,
                p.x as f32,
                p.y as f32,
            ),
            PathEl::ClosePath => pb.close(),
        }
    }
    pb.finish()
}

/// `path` cut to the square of [`MAX_REACH`] around the target, its curves
/// flattened: a closed outline cut as a polygon (so what it fills inside
/// the square stays the same), an open one as a line, its pieces outside
/// left out (so a stroke along it stays the same).
fn cut_to_reach(path: &BezPath) -> BezPath {
    use tracedraw_core::geometry::CubicBez;
    let m = MAX_REACH;
    let mut out = BezPath::new();
    let flush = |pts: &mut Vec<Point>, closed: bool, out: &mut BezPath| {
        pts.retain(|p| p.x.is_finite() && p.y.is_finite());
        if pts.len() >= 2 {
            if closed {
                let poly = clip_polygon(pts, m);
                if poly.len() >= 3 {
                    out.move_to(poly[0]);
                    for p in &poly[1..] {
                        out.line_to(*p);
                    }
                    out.close_path();
                }
            } else {
                let mut pen = false;
                for w in pts.windows(2) {
                    match clip_segment(w[0], w[1], m) {
                        Some((a, b, cut_start, cut_end)) => {
                            if !pen || cut_start {
                                out.move_to(a);
                            }
                            out.line_to(b);
                            pen = !cut_end;
                        }
                        None => pen = false,
                    }
                }
            }
        }
        pts.clear();
    };
    let mut pts: Vec<Point> = Vec::new();
    let mut last = Point::ZERO;
    for el in path.elements() {
        match *el {
            PathEl::MoveTo(p) => {
                flush(&mut pts, false, &mut out);
                pts.push(p);
                last = p;
            }
            PathEl::LineTo(p) => {
                pts.push(p);
                last = p;
            }
            PathEl::QuadTo(c, p) => {
                let c1 = last + (c - last) * (2.0 / 3.0);
                let c2 = p + (c - p) * (2.0 / 3.0);
                curve_points(CubicBez::new(last, c1, c2, p), m, &mut pts);
                last = p;
            }
            PathEl::CurveTo(c1, c2, p) => {
                curve_points(CubicBez::new(last, c1, c2, p), m, &mut pts);
                last = p;
            }
            PathEl::ClosePath => {
                let start = pts.first().copied();
                flush(&mut pts, true, &mut out);
                if let Some(s) = start {
                    pts.push(s);
                    last = s;
                }
            }
        }
    }
    flush(&mut pts, false, &mut out);
    out
}

/// The points of the curve `c` after its start, as lines: the pieces of
/// it far from the square from -`m` to `m` as one line each (a curve lies
/// within its control points, so a piece whose control points are all
/// outside one side of the square, and the line across it, stay outside
/// it), the pieces near it flattened. Halving the curve until a piece is
/// near or far keeps a curve billions of pixels long to a few thousand
/// lines (flattening it whole asked for more lines than memory holds).
fn curve_points(c: tracedraw_core::geometry::CubicBez, m: f64, pts: &mut Vec<Point>) {
    use tracedraw_core::geometry::ParamCurve as _;
    let mut todo = vec![(c, 0u32)];
    while let Some((c, depth)) = todo.pop() {
        let ps = [c.p0, c.p1, c.p2, c.p3];
        if ps.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
            pts.push(c.p3);
            continue;
        }
        let (x0, x1) = ps
            .iter()
            .fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.x), b.max(p.x)));
        let (y0, y1) = ps
            .iter()
            .fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.y), b.max(p.y)));
        let far = x1 < -m || x0 > m || y1 < -m || y0 > m;
        if far || depth > 1200 {
            pts.push(c.p3);
        } else if (x1 - x0).max(y1 - y0) <= 4.0 * m {
            let mut piece = BezPath::new();
            piece.move_to(c.p0);
            piece.curve_to(c.p1, c.p2, c.p3);
            tracedraw_core::geometry::flatten(&piece, 0.25, &mut |el| {
                if let PathEl::LineTo(p) = el {
                    pts.push(p);
                }
            });
        } else {
            let (a, b) = c.subdivide();
            todo.push((b, depth + 1));
            todo.push((a, depth + 1));
        }
    }
}

/// The polygon `pts` cut to the square from -`m` to `m` (Sutherland and
/// Hodgman, one side of the square at a time).
fn clip_polygon(pts: &[Point], m: f64) -> Vec<Point> {
    let mut poly = pts.to_vec();
    // Inside test and crossing point for each side.
    type Side = (fn(Point, f64) -> bool, fn(Point, Point, f64) -> Point);
    let sides: [Side; 4] = [
        (|p, m| p.x >= -m, |a, b, m| at_x(a, b, -m)),
        (|p, m| p.x <= m, |a, b, m| at_x(a, b, m)),
        (|p, m| p.y >= -m, |a, b, m| at_y(a, b, -m)),
        (|p, m| p.y <= m, |a, b, m| at_y(a, b, m)),
    ];
    for (inside, cross) in sides {
        if poly.is_empty() {
            break;
        }
        let mut next = Vec::with_capacity(poly.len() + 4);
        let mut prev = *poly.last().unwrap();
        for &p in &poly {
            match (inside(prev, m), inside(p, m)) {
                (true, true) => next.push(p),
                (true, false) => next.push(cross(prev, p, m)),
                (false, true) => {
                    next.push(cross(prev, p, m));
                    next.push(p);
                }
                (false, false) => {}
            }
            prev = p;
        }
        poly = next;
    }
    poly
}

fn at_x(a: Point, b: Point, x: f64) -> Point {
    let t = (x - a.x) / (b.x - a.x);
    Point::new(x, a.y + (b.y - a.y) * t)
}

fn at_y(a: Point, b: Point, y: f64) -> Point {
    let t = (y - a.y) / (b.y - a.y);
    Point::new(a.x + (b.x - a.x) * t, y)
}

/// The part of the segment from `a` to `b` inside the square from -`m` to
/// `m` (Liang and Barsky), and whether its start and its end were cut.
fn clip_segment(a: Point, b: Point, m: f64) -> Option<(Point, Point, bool, bool)> {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for (p, q) in [(-dx, a.x + m), (dx, m - a.x), (-dy, a.y + m), (dy, m - a.y)] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                t0 = t0.max(r);
            } else {
                t1 = t1.min(r);
            }
        }
    }
    if t0 > t1 {
        return None;
    }
    let at = |t: f64| Point::new(a.x + dx * t, a.y + dy * t);
    Some((at(t0), at(t1), t0 > 0.0, t1 < 1.0))
}

/// Rasterise a fill over `bounds` (page mm) at `dpi`, y up mapped to image
/// rows top-down. Used by exporters that have no native pattern or
/// texture support (PDF, SVG fallback).
/// `paint` laid into `layer` where `cover` lies, hiding what `layer` held
/// there: the layer keeps `1 - cover` of itself under the paint
/// (premultiplied pixels of the same size).
fn knock_out(layer: &mut Pixmap, paint: &Pixmap, cover: &Pixmap) {
    for ((d, s), k) in layer
        .pixels_mut()
        .iter_mut()
        .zip(paint.pixels())
        .zip(cover.pixels())
    {
        let ka = u32::from(k.alpha());
        if ka == 0 {
            continue;
        }
        let keep = 255 - ka;
        let mix = |dv: u8, sv: u8| (u32::from(sv) + (u32::from(dv) * keep + 127) / 255).min(255) as u8;
        let a = mix(d.alpha(), s.alpha());
        let (r, g, b) = (
            mix(d.red(), s.red()).min(a),
            mix(d.green(), s.green()).min(a),
            mix(d.blue(), s.blue()).min(a),
        );
        if let Some(c) = tiny_skia::PremultipliedColorU8::from_rgba(r, g, b, a) {
            *d = c;
        }
    }
}

pub fn render_fill_image(fill: &Fill, bounds: Rect, dpi: f64) -> Option<Pixmap> {
    let zoom = dpi / 25.4;
    let w = (bounds.width() * zoom).ceil().max(1.0) as u32;
    let h = (bounds.height() * zoom).ceil().max(1.0) as u32;
    if w > 8192 || h > 8192 {
        return None;
    }
    let mut pixmap = Pixmap::new(w, h)?;
    let view = ViewTransform {
        zoom,
        origin_x: -bounds.x0 * zoom,
        origin_y: bounds.y1 * zoom,
        stretch_y: 1.0,
    };
    let screen = view.affine();
    let page_path = tracedraw_core::geometry::rect_path(bounds, 0.0);
    let sk = to_sk_path(&(screen * page_path.clone()))?;
    let mut r = Renderer {
        pixmap: &mut pixmap,
        screen,
        zoom,
        flags: Flags::default(),
        symbols: &[],
        preview: None,
        bounds: None,
        origin: (0, 0),
        cancel: None,
    };
    r.draw_fill(fill, &page_path, &sk, FillRule::Winding);
    Some(pixmap)
}

/// [`render_fill_image`] over the bounds of `path` (page space, mm) for
/// a fill that depends on the object's outline: a shaped fountain
/// measures its distances from the edge of `path` (even-odd) and covers
/// the whole rectangle, so an exporter can clip it with the path. Other
/// fills render as [`render_fill_image`] does.
pub fn render_fill_image_in(fill: &Fill, path: &BezPath, dpi: f64) -> Option<Pixmap> {
    let bounds = bbox(path);
    let Fill::Fountain(f) = fill else {
        return render_fill_image(fill, bounds, dpi);
    };
    if f.kind != tracedraw_core::FountainKind::Shaped {
        return render_fill_image(fill, bounds, dpi);
    }
    let zoom = dpi / 25.4;
    let w = (bounds.width() * zoom).ceil().max(1.0) as u32;
    let h = (bounds.height() * zoom).ceil().max(1.0) as u32;
    if w > 8192 || h > 8192 {
        return None;
    }
    let mut pixmap = Pixmap::new(w, h)?;
    let screen = ViewTransform {
        zoom,
        origin_x: -bounds.x0 * zoom,
        origin_y: bounds.y1 * zoom,
        stretch_y: 1.0,
    }
    .affine();
    let rect = to_sk_path(&(screen * tracedraw_core::geometry::rect_path(bounds, 0.0)))?;
    let mask = to_sk_path(&(screen * path.clone()))?;
    let mut r = Renderer {
        pixmap: &mut pixmap,
        screen,
        zoom,
        flags: Flags::default(),
        symbols: &[],
        preview: None,
        bounds: None,
        origin: (0, 0),
        cancel: None,
    };
    r.fill_shaped(f, &mask, &rect, FillRule::EvenOdd);
    Some(pixmap)
}

/// Whether a tile copy drawn with `t` (tile mm to block pixels) reaches a
/// `tw` by `th` pixmap at all: its corners' box meets the pixmap.
fn copy_shows(t: Affine, tile: Rect, tw: u32, th: u32) -> bool {
    let b = (t * tile.to_path(0.01)).bounding_box();
    b.x1 > 0.0 && b.y1 > 0.0 && b.x0 < f64::from(tw) && b.y0 < f64::from(th)
}

/// The block of tiles a pattern repeats (one tile, or the mirrored and
/// shifted tiles of its placement), rasterised at `dpi`, with the
/// transform from the pixmap's unit square (Y up) to page millimetres:
/// the placement's tile size, offset, rotation, skew and lean. Exporters
/// tile the picture with that transform (the object's transform in front
/// of it when the pattern transforms with its object).
pub fn render_pattern_cell(p: &tracedraw_core::Pattern, dpi: f64) -> Option<(Pixmap, Affine)> {
    let zoom = dpi / 25.4;
    if !(zoom.is_finite() && zoom > 0.0) {
        return None;
    }
    let placement = p.placement().sanitized();
    let tile = p.tile_size();
    if !(tile.width.is_finite() && tile.height.is_finite())
        || tile.width <= 0.0
        || tile.height <= 0.0
    {
        return None;
    }
    let unit_to_page = placement.transform(tile);
    let mut scratch = Pixmap::new(1, 1)?;
    let r = Renderer {
        pixmap: &mut scratch,
        screen: Affine::scale(zoom),
        zoom,
        flags: Flags::default(),
        symbols: &[],
        preview: None,
        bounds: None,
        origin: (0, 0),
        cancel: None,
    };
    let (pm, px_to_unit) =
        r.pattern_cell(p, &placement, tile, Affine::scale(zoom) * unit_to_page)?;
    // The picture's pixels (Y down) laid over the unit square: a decoded
    // bitmap is the block itself only when it covers the square exactly.
    let unit = px_to_unit.as_coeffs();
    let covers = (unit[0] * f64::from(pm.width()) - 1.0).abs() < 1e-6
        && (unit[3] * f64::from(pm.height()) + 1.0).abs() < 1e-6
        && unit[4].abs() < 1e-9
        && (unit[5] - 1.0).abs() < 1e-9;
    if covers {
        return Some(((*pm).clone(), unit_to_page));
    }
    // A bitmap tile stretched over the block: drawn into a pixmap of its own.
    let (tw, th) = (pm.width().clamp(1, 4096), pm.height().clamp(1, 4096));
    let mut out = Pixmap::new(tw, th)?;
    let t = Affine::new([f64::from(tw), 0.0, 0.0, -f64::from(th), 0.0, f64::from(th)]) * px_to_unit;
    let c = t.as_coeffs();
    out.draw_pixmap(
        0,
        0,
        pm.as_ref().as_ref(),
        &tiny_skia::PixmapPaint {
            quality: tiny_skia::FilterQuality::Bilinear,
            ..Default::default()
        },
        Transform::from_row(
            c[0] as f32,
            c[1] as f32,
            c[2] as f32,
            c[3] as f32,
            c[4] as f32,
            c[5] as f32,
        ),
        None,
    );
    Some((out, unit_to_page))
}

/// Render a whole page to an RGBA image at `dpi`, white background (exports, thumbnails, tests).
pub fn render_page_image(doc: &Document, page: PageId, dpi: f64) -> Option<Pixmap> {
    render_page_image_with(doc, page, dpi, &RenderOptions::default())
}

/// `area` (page coordinates, mm) of `page` as an image at `dpi`, white
/// where nothing is drawn: the page with whatever lies around it on the
/// desktop, for the Navigator (#2053).
pub fn render_area_image(doc: &Document, page: PageId, area: Rect, dpi: f64) -> Option<Pixmap> {
    render_area_image_on_desk(doc, page, area, dpi, None)
}

/// [`render_area_image`] with the desktop round the page in `desk`
/// (RGB) and the page white as paper, as the drawing window shows them:
/// for Edit > Copy Visible Area in a dark theme (#4027). `None` is white
/// everywhere.
pub fn render_area_image_on_desk(
    doc: &Document,
    page: PageId,
    area: Rect,
    dpi: f64,
    desk: Option<[u8; 3]>,
) -> Option<Pixmap> {
    let page_size = doc.page(page).ok()?.size;
    let zoom = dpi / 25.4;
    if !(area.width() > 0.0 && area.height() > 0.0) || !zoom.is_finite() || zoom <= 0.0 {
        return None;
    }
    let w = (area.width() * zoom).ceil().clamp(1.0, 8192.0) as u32;
    let h = (area.height() * zoom).ceil().clamp(1.0, 8192.0) as u32;
    let view = ViewTransform {
        zoom,
        origin_x: -area.x0 * zoom,
        origin_y: h as f64 + area.y0 * zoom,
        stretch_y: 1.0,
    };
    let objects = render_page(
        doc,
        page,
        &RenderOptions {
            width: w,
            height: h,
            view,
            ..RenderOptions::default()
        },
    )?;
    let mut out = Pixmap::new(w, h)?;
    match desk {
        Some([r, g, b]) => {
            out.fill(SkColor::from_rgba8(r, g, b, 255));
            // The paper: the page's box (y up from its bottom edge).
            let x0 = (-area.x0 * zoom) as f32;
            let x1 = ((page_size.width - area.x0) * zoom) as f32;
            let y0 = (h as f64 - (page_size.height - area.y0) * zoom) as f32;
            let y1 = (h as f64 + area.y0 * zoom) as f32;
            if let Some(paper) = tiny_skia::Rect::from_ltrb(x0, y0, x1, y1) {
                let mut paint = tiny_skia::Paint::default();
                paint.set_color(SkColor::WHITE);
                out.fill_rect(paper, &paint, Transform::identity(), None);
            }
        }
        None => out.fill(SkColor::WHITE),
    }
    out.draw_pixmap(
        0,
        0,
        objects.as_ref(),
        &tiny_skia::PixmapPaint::default(),
        Transform::identity(),
        None,
    );
    Some(out)
}

/// How far around the page, in page sizes, [`whole_page_area`] reaches:
/// an object left far out on the desktop does not shrink the rest to dots.
pub const WHOLE_PAGE_REACH: f64 = 3.0;

/// Everything drawn on the page and beside it on the desktop (page
/// coordinates, mm), up to [`WHOLE_PAGE_REACH`] page sizes away: what a
/// file's thumbnail shows (#2357). The drawing's own extent, not the
/// page's, as zooming to all objects shows it (#3052: the page with a
/// margin round it); the page when nothing is drawn. `None` without the
/// page or with a page of no size.
pub fn whole_page_area(doc: &Document, page: PageId) -> Option<Rect> {
    let p = doc.page(page).ok()?;
    let (w, h) = (p.size.width, p.size.height);
    if !(w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0) {
        return None;
    }
    let sheet = Rect::new(0.0, 0.0, w, h);
    let reach = sheet.inflate(w * WHOLE_PAGE_REACH, h * WHOLE_PAGE_REACH);
    let content = doc
        .content_bounds(page)
        .ok()
        .flatten()
        .filter(|b| [b.x0, b.y0, b.x1, b.y1].iter().all(|v| v.is_finite()))
        .map(|b| b.intersect(reach))
        .filter(|b| b.width() > 0.0 || b.height() > 0.0);
    Some(content.unwrap_or(sheet))
}

/// The page with whatever lies beside it on the desktop
/// ([`whole_page_area`]) on white, its longer side `long_px` pixels: the
/// thumbnail of a file (Welcome Screen, file browser, `.cdr` preview).
pub fn render_whole_page_image(doc: &Document, page: PageId, long_px: f64) -> Option<Pixmap> {
    let area = whole_page_area(doc, page)?;
    let longest = area.width().max(area.height());
    if !(long_px.is_finite() && long_px > 0.0 && longest > 0.0) {
        return None;
    }
    render_area_image(doc, page, area, long_px * 25.4 / longest)
}

/// `render_page_image` with the behaviour flags of `opts` (`wireframe`,
/// `simulate_overprints`, `complex_effects`); its size, view and preview
/// are replaced by the page's own.
pub fn render_page_image_with(
    doc: &Document,
    page: PageId,
    dpi: f64,
    opts: &RenderOptions,
) -> Option<Pixmap> {
    let p = doc.page(page).ok()?;
    let zoom = dpi / 25.4;
    let w = (p.size.width * zoom).ceil() as u32;
    let h = (p.size.height * zoom).ceil() as u32;
    let view = ViewTransform {
        zoom,
        origin_x: 0.0,
        origin_y: h as f64,
        stretch_y: 1.0,
    };
    page_image_in(doc, page, (w, h, view), opts)
}

/// [`render_page_image`] for a picture file (#2866): the page fills
/// every pixel of it (see [`ViewTransform::whole_page`]), with no light
/// row above it or column to its right.
pub fn render_page_export_image(doc: &Document, page: PageId, dpi: f64) -> Option<Pixmap> {
    render_page_export_image_aa(doc, page, dpi, true)
}

/// [`render_page_export_image`], its edges smoothed (`anti_alias`) or
/// left hard, each pixel all in or all out of a shape.
pub fn render_page_export_image_aa(
    doc: &Document,
    page: PageId,
    dpi: f64,
    anti_alias: bool,
) -> Option<Pixmap> {
    let p = doc.page(page).ok()?;
    let fit = ViewTransform::whole_page(p.size.width, p.size.height, dpi / 25.4);
    page_image_in(
        doc,
        page,
        fit,
        &RenderOptions {
            anti_alias,
            ..RenderOptions::default()
        },
    )
}

/// `page` drawn `w` by `h` pixels through `view`, on white.
fn page_image_in(
    doc: &Document,
    page: PageId,
    (w, h, view): (u32, u32, ViewTransform),
    opts: &RenderOptions,
) -> Option<Pixmap> {
    let mut objects = render_page(
        doc,
        page,
        &RenderOptions {
            width: w,
            height: h,
            view,
            preview: None,
            wireframe: opts.wireframe,
            simulate_overprints: opts.simulate_overprints,
            complex_effects: opts.complex_effects,
            anti_alias: opts.anti_alias,
            draft: false,
            focus: None,
        },
    )?;
    let mut out = Pixmap::new(w, h)?;
    out.fill(SkColor::WHITE);
    out.draw_pixmap(
        0,
        0,
        objects.as_ref(),
        &tiny_skia::PixmapPaint::default(),
        Transform::identity(),
        None,
    );
    let _ = &mut objects;
    Some(out)
}

/// Whether `png` is a black and white (1-bit) picture: a grey PNG of one
/// bit per pixel, or one marked "TraceDraw color mode" = "BlackWhite" in a
/// `tEXt` chunk before its pixels (#2922).
fn png_is_black_white(png: &[u8]) -> bool {
    const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
    if png.get(..8) != Some(SIGNATURE) || png.get(12..16) != Some(b"IHDR".as_slice()) {
        return false;
    }
    if png.get(24) == Some(&1) && png.get(25) == Some(&0) {
        return true;
    }
    let mut at = 8usize;
    while let Some(len) = png
        .get(at..at + 4)
        .and_then(|b| b.try_into().ok())
        .map(|b: [u8; 4]| u32::from_be_bytes(b) as usize)
    {
        let (Some(ty), Some(data)) = (png.get(at + 4..at + 8), png.get(at + 8..at + 8 + len))
        else {
            return false;
        };
        if ty == b"IDAT" {
            return false;
        }
        if ty == b"tEXt" && data == b"TraceDraw color mode\0BlackWhite" {
            return true;
        }
        at += 12 + len;
    }
    false
}


/// The Cut lens worked out for one drawing of it, in screen pixels: the
/// cut line along the lens's long middle (from `origin` along `u`, `len`
/// long, `n` across it towards the shadow), the places along it where
/// an object beneath starts or ends (each with the way out of the object
/// and the object's colour there), the colour of the bare background
/// along the line, and the sizes in pixels.
struct CutLens {
    origin: Point,
    u: tracedraw_core::geometry::Vec2,
    n: tracedraw_core::geometry::Vec2,
    edges: Vec<(f64, f64, [f32; 3])>,
    bg: [f32; 3],
    spur_len: f64,
    spur_half: f64,
    curve: f64,
    gap_half: f64,
    shadow: f32,
    shadow_len: f64,
    shadow_color: [f32; 3],
}

impl CutLens {
    /// How different from the background a colour must be (largest channel
    /// difference) to count as an object.
    const INK: f32 = 0.1;

    fn new(
        lens: &tracedraw_core::live::Lens,
        m: Affine,
        b: tracedraw_core::geometry::Rect,
        sample: &dyn Fn(f64, f64) -> [f32; 3],
    ) -> Option<CutLens> {
        let tracedraw_core::live::Lens::Cut {
            spur_length,
            spur_width,
            curve,
            gap,
            shadow,
            shadow_color,
            shadow_up,
        } = lens
        else {
            return None;
        };
        let cy = (b.y0 + b.y1) / 2.0;
        let origin = m * Point::new(b.x0, cy);
        let end = m * Point::new(b.x1, cy);
        let along = end - origin;
        let len = along.hypot();
        if !(len.is_finite() && len >= 2.0) {
            return None;
        }
        let u = along / len;
        // Across: towards the lens's lower edge on the screen (its upper
        // one with "shadow up").
        let low = m * Point::new(b.x0, b.y0) - origin;
        let high = m * Point::new(b.x0, b.y1) - origin;
        let mut side = if low.y >= high.y { low } else { high };
        if *shadow_up {
            side = -side;
        }
        let mut n = tracedraw_core::geometry::Vec2::new(-u.y, u.x);
        if n.dot(side) < 0.0 {
            n = -n;
        }
        let shadow_len = side.dot(n).abs().max(1.0);
        // Pixels per millimetre, from the lens's own scale.
        let scale = m.determinant().abs().sqrt();
        let px = |mm: f64| (mm.max(0.0) * scale).min(4000.0);
        let differ = |p: [f32; 3], q: [f32; 3]| -> f32 {
            (p[0] - q[0])
                .abs()
                .max((p[1] - q[1]).abs())
                .max((p[2] - q[2]).abs())
        };
        // Samples along the line, a pixel apart.
        let count = len.ceil() as usize + 1;
        let samples: Vec<[f32; 3]> = (0..count)
            .map(|i| {
                let p = origin + u * (i as f64).min(len);
                sample(p.x, p.y)
            })
            .collect();
        // The background: the colour both ends share, or else the most
        // common colour along the line.
        let (first, last) = (samples[0], samples[count - 1]);
        let bg = if differ(first, last) < Self::INK {
            first
        } else {
            let mut counts: std::collections::HashMap<[u8; 3], (usize, [f32; 3])> =
                std::collections::HashMap::new();
            for c in &samples {
                let key = [
                    (c[0] * 15.0).round() as u8,
                    (c[1] * 15.0).round() as u8,
                    (c[2] * 15.0).round() as u8,
                ];
                counts.entry(key).or_insert((0, *c)).0 += 1;
            }
            counts
                .values()
                .max_by_key(|(k, _)| *k)
                .map(|(_, c)| *c)
                .unwrap_or([1.0, 1.0, 1.0])
        };
        let ink: Vec<bool> = samples.iter().map(|c| differ(*c, bg) > Self::INK).collect();
        let mut edges = Vec::new();
        for i in 1..count {
            if ink[i] && !ink[i - 1] {
                // An object starts: its spur points back along the line.
                let c = samples[(i + 1).min(count - 1)];
                let c = if ink[(i + 1).min(count - 1)] { c } else { samples[i] };
                edges.push((i as f64 - 0.5, -1.0, c));
            } else if !ink[i] && ink[i - 1] {
                let c = if i >= 2 && ink[i - 2] { samples[i - 2] } else { samples[i - 1] };
                edges.push((i as f64 - 0.5, 1.0, c));
            }
        }
        let rgb = {
            let [r, g, bl] = shadow_color.to_rgb8();
            [r as f32 / 255.0, g as f32 / 255.0, bl as f32 / 255.0]
        };
        Some(CutLens {
            origin,
            u,
            n,
            edges,
            bg,
            spur_len: px(*spur_length).max(0.5),
            spur_half: px(*spur_width) / 2.0,
            curve: curve.clamp(0.3, 8.0),
            gap_half: px(*gap) / 2.0,
            shadow: (*shadow as f32 / 100.0).clamp(0.0, 1.0),
            shadow_len,
            shadow_color: rgb,
        })
    }

    /// The colour of pixel (x, y), whose colour beneath is `here`.
    fn color(&self, x: u32, y: u32, here: [f32; 3]) -> [f32; 3] {
        let p = Point::new(x as f64 + 0.5, y as f64 + 0.5) - self.origin;
        let a = p.dot(self.u);
        let d = p.dot(self.n);
        let differ = |p: [f32; 3], q: [f32; 3]| -> f32 {
            (p[0] - q[0])
                .abs()
                .max((p[1] - q[1]).abs())
                .max((p[2] - q[2]).abs())
        };
        let is_ink = differ(here, self.bg) > Self::INK;
        let mix = |a: [f32; 3], b: [f32; 3], t: f32| -> [f32; 3] {
            [
                a[0] + (b[0] - a[0]) * t,
                a[1] + (b[1] - a[1]) * t,
                a[2] + (b[2] - a[2]) * t,
            ]
        };
        let mut out = here;
        let mut covered = is_ink;
        if !is_ink && self.spur_half > 0.0 {
            // The spur that covers this pixel most, if any: its half
            // thickness narrows from the object's edge to a point.
            let mut best: Option<(f32, [f32; 3])> = None;
            for &(t, way, c) in &self.edges {
                let dx = (a - t) * way;
                if !(0.0..self.spur_len).contains(&dx) {
                    continue;
                }
                let half = self.spur_half * (1.0 - dx / self.spur_len).powf(self.curve);
                let cover = (half - d.abs() + 0.5).clamp(0.0, 1.0) as f32;
                if cover > 0.0 && best.is_none_or(|(b, _)| cover > b) {
                    best = Some((cover, c));
                }
            }
            if let Some((cover, c)) = best {
                out = mix(out, c, cover);
                covered = cover > 0.5;
            }
        }
        if covered && self.gap_half > 0.0 {
            let cover = (self.gap_half - d.abs() + 0.5).clamp(0.0, 1.0) as f32;
            if cover > 0.0 {
                out = mix(out, self.bg, cover);
            }
        }
        if is_ink && self.shadow > 0.0 && d > self.gap_half {
            let t = ((d - self.gap_half) / self.shadow_len).clamp(0.0, 1.0);
            let f = self.shadow * (1.0 - t as f32).powf(1.6);
            out = mix(out, self.shadow_color, f);
        }
        out
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn one_bit_pictures_are_known_by_their_png() {
        // #2922: blown up, they keep hard edges.
        let mut one_bit = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        one_bit.extend_from_slice(&[0, 0, 0, 4, 0, 0, 0, 4, 1, 0, 0, 0, 0]);
        assert!(super::png_is_black_white(&one_bit));
        let mut gray8 = one_bit.clone();
        gray8[24] = 8;
        assert!(!super::png_is_black_white(&gray8));
        let mut marked = gray8.clone();
        marked.extend_from_slice(&[0, 0, 0, 0]); // the IHDR CRC
        let text = b"TraceDraw color mode\0BlackWhite";
        marked.extend_from_slice(&(text.len() as u32).to_be_bytes());
        marked.extend_from_slice(b"tEXt");
        marked.extend_from_slice(text);
        marked.extend_from_slice(&[0, 0, 0, 0]);
        assert!(super::png_is_black_white(&marked));
        assert!(!super::png_is_black_white(b"not a png"));
    }
    /// A text and the warped outline of one fill with the nonzero rule,
    /// so letters an envelope pushes together stay filled where they
    /// overlap (#647); other curves fill even-odd.
    #[test]
    fn warped_text_fills_like_text() {
        use super::fill_rule;
        let path = ShapeKind::Path {
            path: BezPath::new(),
            closed: true,
        };
        assert!(matches!(fill_rule(&path, false), FillRule::EvenOdd));
        assert!(matches!(fill_rule(&path, true), FillRule::Winding));
    }

    use super::*;
    use tracedraw_core::document::{Shape, ShapeKind};
    use tracedraw_core::{
        Fountain, FountainKind, Pattern, PatternTile, Stop, Texture, TextureKind,
    };

    fn px(pm: &Pixmap, x: u32, y: u32) -> (u8, u8, u8) {
        let p = pm.pixel(x, y).unwrap().demultiply();
        (p.red(), p.green(), p.blue())
    }

    /// An object with a fountain transparency casts the whole shadow
    /// of its shape, the same as without the transparency. The shadow
    /// was drawn through the transparency's mask, which past the
    /// object's box repeats its edge: cut by a hard line, faint above
    /// it and dark below (reported 14/11/2026). A fountain fill, black to
    /// white, casts the whole shadow too, as in the reference program.
    #[test]
    fn a_fountain_transparency_leaves_the_drop_shadow_whole() {
        let disc_filled = |faded: bool, fill: Fill| {
            let mut s = Shape::new(
                tracedraw_core::ShapeId(1),
                ShapeKind::Ellipse {
                    rect: Rect::new(15.0, 55.0, 45.0, 85.0),
                    arc: None,
                },
            );
            s.fill = fill;
            if faded {
                s.effects.push(tracedraw_core::live::Effect::Transparency {
                    mask: Fill::linear(Color::WHITE, Color::BLACK, -45.0),
                    merge: tracedraw_core::live::MergeMode::Normal,
                    target: 2,
                });
            }
            s.shadow = Some(tracedraw_core::Shadow {
                offset: tracedraw_core::geometry::Vec2::new(15.0, -30.0),
                opacity: 0.8,
                blur: 3.0,
                color: Color::BLACK,
                ..Default::default()
            });
            let (doc, page) = doc_with(vec![s]);
            render_page_image(&doc, page, 25.4).unwrap()
        };
        let disc = |faded: bool| disc_filled(faded, Fill::Solid(Color::rgb8(0, 160, 80)));
        let (plain, faded) = (disc(false), disc(true));
        let graded = disc_filled(false, Fill::linear(Color::BLACK, Color::WHITE, -45.0));
        // Down the shadow's middle, beside and below the object's box.
        for y in 40..95 {
            assert_eq!(px(&faded, 45, y), px(&plain, 45, y), "row {y}");
            assert_eq!(px(&graded, 45, y), px(&plain, 45, y), "graded, row {y}");
        }
        assert!(px(&faded, 45, 60).0 < 80, "shadow dark in its middle");
        // The object itself still fades.
        let lighter = (18..43)
            .flat_map(|y| (18..43).map(move |x| (x, y)))
            .filter(|&(x, y)| u32::from(px(&faded, x, y).0) > u32::from(px(&plain, x, y).0) + 40)
            .count();
        assert!(lighter > 50, "{lighter} pixels of the object faded");
    }

    /// An object reaching millions of pixels past the picture (a site
    /// plan two kilometres across seen at the closest zoom, or a size
    /// typed far too large) draws its part on the picture: its outline
    /// overflowed the rasteriser, which panicked inside a destructor and
    /// ended the program, or drew nothing.
    #[test]
    fn objects_far_larger_than_the_picture_draw() {
        let big = 2_000_000.0; // 2 km in millimetres
        let blue = Color::Rgb {
            r: 0.0,
            g: 0.0,
            b: 1.0,
        };
        let mut shapes = Vec::new();
        for (i, kind) in [
            ShapeKind::Rect {
                rect: Rect::new(0.0, -big / 2.0, big, big / 2.0),
                radius: 5.0,
                corners: None,
            },
            ShapeKind::Ellipse {
                rect: Rect::new(0.0, -big / 2.0, big, big / 2.0),
                arc: None,
            },
        ]
        .into_iter()
        .enumerate()
        {
            let mut s = Shape::new(ShapeId(i as u64 + 1), kind);
            s.fill = Fill::Solid(blue);
            s.stroke = Some(Stroke::new(Color::BLACK, 0.05));
            shapes.push(s);
        }
        // A huge outline width too.
        let mut wide = Shape::new(
            ShapeId(9),
            ShapeKind::Rect {
                rect: Rect::new(10.0, 10.0, 20.0, 20.0),
                radius: 0.0,
                corners: None,
            },
        );
        wide.fill = Fill::None;
        wide.stroke = Some(Stroke::new(Color::BLACK, 4.0e9));
        for one in shapes.into_iter().chain([wide]) {
            let (doc, page) = doc_with(vec![one]);
            // 400 pixels a millimetre (the closest zoom), the object's left
            // edge (page x 0, y 0) in the middle of the picture.
            let opts = RenderOptions {
                width: 200,
                height: 200,
                view: ViewTransform {
                    zoom: 400.0,
                    origin_x: 100.0,
                    origin_y: 100.0,
                    stretch_y: 1.0,
                },
                ..Default::default()
            };
            let pm = render_page(&doc, page, &opts).expect("drawn");
            let inside = pm.pixel(150, 50).unwrap();
            let outside = pm.pixel(50, 50).unwrap();
            assert!(inside.alpha() > 200, "inside: {inside:?}");
            if doc.pages[0].layers[0].shapes[0].id != ShapeId(9) {
                assert!(outside.alpha() < 50, "outside: {outside:?}");
            }
        }
    }

    fn doc_with(shapes: Vec<Shape>) -> (Document, PageId) {
        let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(100.0, 100.0));
        let page = doc.pages[0].id;
        let layer = doc.pages[0].layers[0].id;
        for s in shapes {
            doc.layer_mut(layer).unwrap().shapes.push(s);
        }
        (doc, page)
    }

    /// A picture in perspective draws its pixels inside the warped
    /// shape, transparent beside it (#2691): it drew nothing, the picture
    /// having become an empty outline.
    #[test]
    fn a_picture_in_perspective_draws_warped() {
        use tracedraw_core::live::Effect;
        // 100 x 50 pixels: red left half, blue right half.
        let mut pm = Pixmap::new(100, 50).unwrap();
        for y in 0..50 {
            for x in 0..100 {
                let c = if x < 50 { (255, 0, 0) } else { (0, 0, 255) };
                pm.pixels_mut()[y * 100 + x] =
                    tiny_skia::ColorU8::from_rgba(c.0, c.1, c.2, 255).premultiply();
            }
        }
        let png = pm.encode_png().unwrap();
        let rect = Rect::new(5.0, 20.0, 95.0, 70.0);
        let mut s = Shape::new(
            ShapeId(1),
            ShapeKind::Bitmap {
                rect,
                width_px: 100,
                height_px: 50,
                png: png.into(),
                fx: None,
            },
        );
        let (doc, page) = doc_with(vec![s.clone()]);
        let plain = render_page_image(&doc, page, 25.4).expect("page");
        let h = plain.height() as f64;
        // Page millimetres (y up) to a pixel at 1 px a millimetre.
        let at = |pm: &Pixmap, x: f64, y: f64| px(pm, x as u32, (h - y) as u32);
        assert_eq!(at(&plain, 8.0, 66.0), (255, 0, 0));
        assert_eq!(at(&plain, 92.0, 66.0), (0, 0, 255));
        // The top pulled in to a trapezium.
        s.effects = vec![Effect::Perspective {
            corners: [
                Point::new(5.0, 20.0),
                Point::new(95.0, 20.0),
                Point::new(70.0, 70.0),
                Point::new(30.0, 70.0),
            ],
        }];
        let (doc, page) = doc_with(vec![s]);
        let warped = render_page_image(&doc, page, 25.4).expect("page");
        assert_eq!(at(&warped, 8.0, 66.0), (255, 255, 255), "beside it");
        assert_eq!(at(&warped, 92.0, 66.0), (255, 255, 255), "beside it");
        assert_eq!(at(&warped, 40.0, 66.0), (255, 0, 0));
        assert_eq!(at(&warped, 60.0, 66.0), (0, 0, 255));
        assert_eq!(at(&warped, 8.0, 23.0), (255, 0, 0));
        assert_eq!(at(&warped, 92.0, 23.0), (0, 0, 255));
    }

    /// Two squares overlapping in one curve, both drawn the same way
    /// round, as the overlapping parts of traced lettering (#1919): with
    /// the curve's fill rule nonzero the overlap stays filled; even-odd
    /// (the default) leaves it a hole.
    #[test]
    fn a_curve_fills_its_overlaps_by_its_own_fill_rule() {
        use tracedraw_core::shaping::FillRule as Rule;
        let mut path = BezPath::new();
        for (x0, x1) in [(10.0, 60.0), (40.0, 90.0)] {
            path.move_to((x0, 10.0));
            path.line_to((x1, 10.0));
            path.line_to((x1, 90.0));
            path.line_to((x0, 90.0));
            path.close_path();
        }
        let curve = |rule: Rule| {
            let mut s = Shape::new(
                ShapeId(1),
                ShapeKind::Path {
                    path: path.clone(),
                    closed: true,
                },
            );
            s.fill = Fill::Solid(Color::BLACK);
            s.stroke = None;
            s.fill_rule = rule;
            s
        };
        for (rule, overlap) in [(Rule::NonZero, (0, 0, 0)), (Rule::EvenOdd, (255, 255, 255))] {
            let (doc, page) = doc_with(vec![curve(rule)]);
            let pm = render_page_image(&doc, page, 25.4).expect("page");
            assert_eq!(px(&pm, 50, 50), overlap, "{rule:?}");
            assert_eq!(px(&pm, 20, 50), (0, 0, 0), "{rule:?}");
        }
    }

    /// A page whose size is not a number (read from a damaged file, or
    /// left by a division by zero) asks for an image as tall as a number
    /// holds. Splitting it into bands for the machine's cores, and
    /// drawing a strip of it with its margin of rows, added past the end
    /// of the range and panicked; now the bands and the strip end at the
    /// image's last row, and drawing the page gives no picture, as a
    /// picture that large cannot be made, instead of taking the program
    /// down.
    #[test]
    fn an_image_as_tall_as_a_number_holds_does_not_overflow_its_bands() {
        use tracedraw_core::geometry::{Rect, Size};
        let s = rect(1, Rect::new(20.0, 20.0, 80.0, 80.0), Color::BLACK);
        let (mut doc, page) = doc_with(vec![s]);
        doc.pages[0].size = Size::new(100.0, f64::INFINITY);
        assert!(render_page_image(&doc, page, 25.4).is_none());
        // A strip at the end of such an image is still drawn.
        let opts = RenderOptions {
            width: 10,
            height: u32::MAX,
            view: ViewTransform {
                zoom: 1.0,
                origin_x: 0.0,
                origin_y: 100.0,
                stretch_y: 1.0,
            },
            ..RenderOptions::default()
        };
        let strip = render_page_rows(&doc, page, &opts, u32::MAX - 16, u32::MAX).expect("strip");
        assert_eq!((strip.width(), strip.height()), (10, 16));
        // The bands themselves, for every core count the renderer uses,
        // with a blur that is not a number as well.
        assert_eq!(blur_margin(f64::INFINITY, 1.0), u32::MAX);
        assert_eq!(blur_margin(f64::NAN, 1.0), 16);
        for cores in 2..=16 {
            for margin in [0, 16, u32::MAX] {
                if let Some(bands) = band_layout(u32::MAX, cores, margin) {
                    assert!(bands.iter().all(|b| b.0 < b.1 && b.2 <= b.0 && b.1 <= b.3));
                    assert_eq!(bands.last().map(|b| (b.1, b.3)), Some((u32::MAX, u32::MAX)));
                }
            }
        }
        let bands = band_layout(u32::MAX, 2, 16).expect("two bands");
        assert_eq!(bands.len(), 2);
        assert_eq!(bands[1].0, bands[0].1);
        assert!(band_layout(64, 2, u32::MAX).is_none());
    }

    /// A bevel drawn over a mid-grey square: the band on the light side is
    /// lighter than the fill and the one opposite darker, and with more
    /// levels the tone fades from the edge inwards (#1135).
    #[test]
    fn a_bevel_is_lit_on_one_side_and_fades_with_its_levels() {
        use tracedraw_core::geometry::Rect;
        use tracedraw_core::live::Effect;
        let bevelled = |style: u8, level: u8| {
            let mut s = rect(
                1,
                Rect::new(20.0, 20.0, 80.0, 80.0),
                Color::rgb8(128, 128, 128),
            );
            s.effects.push(Effect::Bevel {
                distance: 12.0,
                light_angle: 180.0,
                intensity: 100.0,
                style,
                shadow_color: Color::BLACK,
                light_color: Color::WHITE,
                level,
                emboss_color: None,
                directional: false,
            });
            let (doc, page) = doc_with(vec![s]);
            let pm = render_page_image(&doc, page, 25.4).expect("render");
            if let Ok(dir) = std::env::var("TRACEDRAW_TEST_PNG") {
                let _ = pm.save_png(format!("{dir}/bevel-{style}-{level}.png"));
            }
            pm
        };
        // Light from the left (angle 180): the left band is lit, the right shaded.
        let flat = bevelled(0, 1);
        assert!(px(&flat, 22, 50).0 > 200, "{:?}", px(&flat, 22, 50));
        assert!(px(&flat, 78, 50).0 < 60, "{:?}", px(&flat, 78, 50));
        assert_eq!(px(&flat, 50, 50), (128, 128, 128));
        let soft = bevelled(0, 4);
        let (edge, inner) = (px(&soft, 21, 50).0, px(&soft, 30, 50).0);
        assert!(edge > inner && inner > 128, "{edge} {inner}");
        // Emboss lies outside the square.
        let emboss = bevelled(1, 1);
        assert!(px(&emboss, 12, 50).0 > 200, "{:?}", px(&emboss, 12, 50));
        assert_eq!(px(&emboss, 50, 50), (128, 128, 128));
    }

    /// A white-to-black gradient on a wide rectangle, turned half a turn,
    /// mirrored or turned a quarter turn through the engine, is drawn
    /// turned, mirrored and turned with it (#775).
    #[test]
    fn a_gradient_is_drawn_turned_and_mirrored_with_its_object() {
        use tracedraw_core::{Command, Engine};
        let mut s = rect(1, Rect::new(10.0, 40.0, 90.0, 60.0), Color::WHITE);
        s.fill = Fill::Fountain(Fountain::two(
            FountainKind::Linear,
            Color::WHITE,
            Color::BLACK,
            0.0,
        ));
        let (doc, page) = doc_with(vec![s]);
        let mut eng = Engine::new(doc);
        let about =
            |a: Affine| Affine::translate((50.0, 50.0)) * a * Affine::translate((-50.0, -50.0));
        let mut turn = |eng: &mut Engine, t: Affine| {
            eng.run(&Command::TransformShapes {
                shapes: vec![tracedraw_core::ShapeId(1)],
                transform: about(t),
            })
            .expect("turned");
            let pm = render_page_image(eng.document(), page, 25.4).expect("render");
            if let Ok(dir) = std::env::var("TRACEDRAW_TEST_PNG") {
                let n = eng.history_labels().0.len();
                let _ = pm.save_png(format!("{dir}/gradient-{n}.png"));
            }
            pm
        };
        let pm = render_page_image(eng.document(), page, 25.4).expect("render");
        // Light on the left, dark on the right.
        assert!(px(&pm, 15, 50).0 > 200 && px(&pm, 85, 50).0 < 55);
        let pm = turn(&mut eng, Affine::rotate(std::f64::consts::PI));
        assert!(px(&pm, 15, 50).0 < 55 && px(&pm, 85, 50).0 > 200);
        let pm = turn(&mut eng, Affine::scale_non_uniform(-1.0, 1.0));
        assert!(px(&pm, 15, 50).0 > 200 && px(&pm, 85, 50).0 < 55);
        // A quarter turn: light at the bottom, dark at the top of the
        // upright rectangle (the page's y grows upwards, the picture's
        // downwards).
        let pm = turn(&mut eng, Affine::rotate(std::f64::consts::FRAC_PI_2));
        assert!(px(&pm, 50, 85).0 > 200 && px(&pm, 50, 15).0 < 55);
    }

    fn rect(id: u64, r: Rect, fill: Color) -> Shape {
        let mut s = Shape::new(
            tracedraw_core::ShapeId(id),
            ShapeKind::Rect {
                rect: r,
                radius: 0.0,
                corners: None,
            },
        );
        s.fill = Fill::Solid(fill);
        s.stroke = None;
        s
    }

    /// The drag preview answers membership once per object drawn: a Vec
    /// there made a drag of a large selection quadratic in the number of
    /// objects on the page. Run with `cargo test --release -p
    /// tracedraw-render -- --ignored preview_membership --nocapture`
    /// (#3559, step 5).
    #[test]
    #[ignore]
    fn preview_membership_cost_on_a_large_drawing() {
        let n = 8000u64;
        let shapes: Vec<Shape> = (0..n)
            .map(|i| {
                let (x, y) = ((i % 100) as f64 * 12.0, (i / 100) as f64 * 12.0);
                rect(i + 100, Rect::new(x, y, x + 10.0, y + 10.0), Color::rgb8(255, 0, 0))
            })
            .collect();
        let (doc, page) = doc_with(shapes);
        let opts = RenderOptions {
            width: 64,
            height: 64,
            view: ViewTransform {
                zoom: 1.0,
                origin_x: 0.0,
                origin_y: 64.0,
                stretch_y: 1.0,
            },
            preview: Some(Preview {
                shapes: (100..100 + n).map(tracedraw_core::ShapeId).collect(),
                transform: Affine::translate((0.0, 1.0)),
            }),
            ..RenderOptions::default()
        };
        let t = std::time::Instant::now();
        let pm = render_page(&doc, page, &opts).unwrap();
        let elapsed = t.elapsed();
        assert_eq!(pm.width(), 64);
        eprintln!("render with a preview of {n} objects: {elapsed:?}");
    }

    /// A render in bands asks every object's box once for all bands
    /// together, not once per band (#3559). Run with `cargo test
    /// --release -p tracedraw-render --lib -- --ignored banded_render
    /// --nocapture`.
    #[test]
    #[ignore]
    fn banded_render_asks_boxes_once() {
        let n = 8000u64;
        let shapes: Vec<Shape> = (0..n)
            .map(|i| {
                let (x, y) = ((i % 100) as f64 * 12.0, (i / 100) as f64 * 12.0);
                rect(i + 100, Rect::new(x, y, x + 10.0, y + 10.0), Color::rgb8(255, 0, 0))
            })
            .collect();
        let (doc, page) = doc_with(shapes);
        let opts = RenderOptions {
            width: 1600,
            height: 1600,
            view: ViewTransform {
                zoom: 1.5,
                origin_x: 0.0,
                origin_y: 2400.0,
                stretch_y: 1.0,
            },
            ..RenderOptions::default()
        };
        // A warm-up, then the measured run.
        let pm = render_page(&doc, page, &opts).expect("render");
        assert_eq!(pm.width(), 1600);
        let t = std::time::Instant::now();
        let pm = render_page(&doc, page, &opts).expect("render");
        let elapsed = t.elapsed();
        assert_eq!(pm.width(), 1600);
        eprintln!("banded render of {n} objects: {elapsed:?}");
    }

    /// A semi-transparent object draws into a layer only as large as
    /// what it can reach (#1335), and the page comes out as with a layer
    /// the size of the window: plain, outlined with arrowheads, with a
    /// gradient, a pattern, a texture, a picture, across the window's
    /// edge, and wholly outside it.
    #[test]
    fn a_transparent_object_takes_a_small_layer_and_draws_the_same() {
        use tracedraw_core::style::{Arrowhead, Pattern, PatternTile};
        let opts = RenderOptions {
            width: 240,
            height: 180,
            view: ViewTransform {
                zoom: 3.0,
                origin_x: 10.0,
                origin_y: 170.0,
                stretch_y: 1.0,
            },
            anti_alias: true,
            ..RenderOptions::default()
        };
        let screen = opts.view.affine();
        let under = rect(1, Rect::new(0.0, 0.0, 80.0, 60.0), Color::rgb8(30, 160, 60));
        let mut photo = Pixmap::new(6, 4).expect("pixmap");
        for (k, p) in photo.data_mut().chunks_mut(4).enumerate() {
            p.copy_from_slice(&[(k * 40) as u8, 90, (255 - k * 9) as u8, 255]);
        }
        let mut cases: Vec<Shape> = Vec::new();
        cases.push(rect(
            2,
            Rect::new(10.3, 12.7, 31.1, 29.9),
            Color::rgb8(200, 20, 40),
        ));
        let mut line = Shape::new(
            tracedraw_core::ShapeId(2),
            ShapeKind::Path {
                path: {
                    let mut p = BezPath::new();
                    p.move_to((12.0, 8.0));
                    p.curve_to((30.0, 50.0), (45.0, -5.0), (60.0, 30.0));
                    p
                },
                closed: false,
            },
        );
        let mut stroke = Stroke::new(Color::rgb8(20, 20, 200), 1.6);
        stroke.start_arrow = Arrowhead::Arrow;
        stroke.end_arrow = Arrowhead::Circle;
        line.stroke = Some(stroke);
        line.fill = Fill::None;
        cases.push(line);
        let mut graded = rect(2, Rect::new(20.0, 5.0, 50.0, 40.0), Color::BLACK);
        graded.fill = Fill::Fountain(tracedraw_core::Fountain::two(
            tracedraw_core::FountainKind::Linear,
            Color::rgb8(250, 200, 0),
            Color::rgb8(0, 40, 160),
            30.0,
        ));
        graded.stroke = Some(Stroke::new(Color::BLACK, 0.8));
        cases.push(graded);
        let mut patterned = rect(2, Rect::new(5.0, 30.0, 45.0, 55.0), Color::BLACK);
        patterned.fill = Fill::Pattern(Pattern::TwoColor {
            tile: PatternTile::DiagonalStripes,
            front: Color::rgb8(0, 0, 0),
            back: Color::rgb8(255, 255, 255),
            size_mm: 3.0,
            placement: Default::default(),
        });
        cases.push(patterned);
        let mut clouds = rect(2, Rect::new(8.0, 3.0, 38.0, 21.0), Color::BLACK);
        clouds.fill = Fill::Texture(tracedraw_core::style::Texture {
            kind: tracedraw_core::style::TextureKind::Clouds,
            color_a: Color::rgb8(0, 0, 0),
            color_b: Color::rgb8(255, 255, 255),
            scale: 10.0,
            seed: 7,
        });
        cases.push(clouds);
        let mut pic = Shape::new(
            tracedraw_core::ShapeId(2),
            ShapeKind::Bitmap {
                rect: Rect::new(33.0, 22.0, 51.0, 34.0),
                width_px: photo.width(),
                height_px: photo.height(),
                png: photo.encode_png().expect("png").into(),
                fx: None,
            },
        );
        pic.fill = Fill::None;
        pic.stroke = None;
        cases.push(pic);
        // Across the window's left edge, and wholly outside it.
        cases.push(rect(
            2,
            Rect::new(-20.0, 20.0, 15.0, 40.0),
            Color::rgb8(90, 0, 90),
        ));
        cases.push(rect(
            2,
            Rect::new(200.0, 20.0, 230.0, 40.0),
            Color::rgb8(90, 0, 90),
        ));
        for (k, mut top) in cases.into_iter().enumerate() {
            let (plain_doc, page) = doc_with(vec![under.clone()]);
            let plain = render_page(&plain_doc, page, &opts).expect("render");
            // The reference: the object drawn opaque on a layer the size
            // of the window, laid over the page at half strength.
            let mut full = Pixmap::new(opts.width, opts.height).expect("layer");
            {
                let mut r = Renderer {
                    pixmap: &mut full,
                    screen,
                    zoom: opts.view.zoom,
                    flags: Flags::from_options(&opts),
                    symbols: &[],
                    preview: None,
                    bounds: None,
                    origin: (0, 0),
                    cancel: None,
                };
                r.draw_shape(&top, Affine::IDENTITY);
            }
            let mut expected = plain.clone();
            let pp = tiny_skia::PixmapPaint {
                opacity: 0.5,
                ..Default::default()
            };
            expected.draw_pixmap(0, 0, full.as_ref(), &pp, Transform::identity(), None);
            top.opacity = 0.5;
            let mut layer_px = Pixmap::new(opts.width, opts.height).expect("pixmap");
            let area = Renderer {
                pixmap: &mut layer_px,
                screen,
                zoom: opts.view.zoom,
                flags: Flags::from_options(&opts),
                symbols: &[],
                preview: None,
                bounds: None,
                origin: (0, 0),
                cancel: None,
            }
            .layer_area(&top, Affine::IDENTITY);
            let (doc, page) = doc_with(vec![under.clone(), top]);
            let drawn = render_page(&doc, page, &opts).expect("render");
            assert!(drawn.data() == expected.data(), "case {k}, area {area:?}");
            if k < 7 {
                assert!(drawn.data() != plain.data(), "case {k} shows");
            }
            // An outline's reach counts 12 widths (mitres, arrowheads),
            // so the outlined ones take more.
            let most = match k {
                1..=3 => opts.width * opts.height,
                _ => opts.width * opts.height / 4,
            };
            assert!(area.2 * area.3 <= most, "case {k}: a small layer, {area:?}");
            if k == 7 {
                assert_eq!(area.2 * area.3, 0, "outside: no layer");
            }
        }
    }

    /// A stand-in text engine: character `i` of the text is a square from
    /// (10 i, 0) to (10 i + 8, 8) mm; characters only laid out are left
    /// out.
    fn square_letters(req: &tracedraw_core::document::text_outline::TextRequest) -> BezPath {
        let mut p = BezPath::new();
        let mut i = 0usize;
        for sp in req.spans {
            for _ in sp.text.chars() {
                if !sp.layout_only {
                    let x = 10.0 * i as f64;
                    p.extend(Rect::new(x, 0.0, x + 8.0, 8.0).to_path(0.01));
                }
                i += 1;
            }
        }
        p
    }

    #[test]
    fn characters_are_drawn_in_their_own_colours() {
        tracedraw_core::document::text_outline::set(square_letters);
        let red = Color::rgb8(255, 0, 0);
        let blue = Color::rgb8(0, 0, 255);
        let mut b = tracedraw_core::TextSpan::new("b", "Any", 12.0);
        b.fill = Some(Fill::Solid(red));
        let mut c = tracedraw_core::TextSpan::new("c", "Any", 12.0);
        c.outline = Some(tracedraw_core::Stroke::new(blue, 3.0));
        let mut t = Shape::new(
            tracedraw_core::ShapeId(1),
            ShapeKind::Text {
                spans: vec![
                    tracedraw_core::TextSpan::new("a", "Any", 12.0),
                    b,
                    c,
                    tracedraw_core::TextSpan::new("d", "Any", 12.0),
                ],
                origin: tracedraw_core::Point::ZERO,
                frame: None,
                align: Default::default(),
                para: Default::default(),
                on_path: None,
            },
        );
        t.transform = Affine::translate((10.0, 10.0));
        t.fill = Fill::Solid(Color::BLACK);
        t.stroke = None;
        let (doc, page) = doc_with(vec![t]);
        let opts = RenderOptions {
            width: 100,
            height: 100,
            view: ViewTransform {
                zoom: 1.0,
                origin_x: 0.0,
                origin_y: 100.0,
                stretch_y: 1.0,
            },
            ..RenderOptions::default()
        };
        let pm = render_page(&doc, page, &opts).unwrap();
        // Letters at x 10, 20, 30, 40 mm; their middles at y 86 on screen.
        assert_eq!(px(&pm, 14, 86), (0, 0, 0));
        assert_eq!(px(&pm, 24, 86), (255, 0, 0));
        assert_eq!(px(&pm, 34, 86), (0, 0, 0));
        // The third letter's own outline, on its edge.
        assert_eq!(px(&pm, 30, 86), (0, 0, 255));
        assert_eq!(px(&pm, 44, 86), (0, 0, 0));
        // Between the letters nothing is drawn.
        assert_eq!(pm.pixel(19, 86).unwrap().alpha(), 0);
    }

    /// A further fill paints over the fill, a further outline over the
    /// outline.
    #[test]
    fn further_fills_and_outlines_paint_over_the_first() {
        let mut r = rect(1, Rect::new(10.0, 10.0, 90.0, 90.0), Color::rgb8(255, 0, 0));
        r.extra_fills = vec![Fill::Solid(Color::rgb8(0, 0, 255))];
        r.stroke = Some(tracedraw_core::Stroke::new(Color::BLACK, 4.0));
        r.extra_strokes = vec![tracedraw_core::Stroke::new(Color::rgb8(0, 255, 0), 2.0)];
        let (doc, page) = doc_with(vec![r]);
        let opts = RenderOptions {
            width: 100,
            height: 100,
            view: ViewTransform {
                zoom: 1.0,
                origin_x: 0.0,
                origin_y: 100.0,
                stretch_y: 1.0,
            },
            ..RenderOptions::default()
        };
        let pm = render_page(&doc, page, &opts).unwrap();
        assert_eq!(px(&pm, 50, 50), (0, 0, 255));
        // On the edge the thin green outline lies over the black one.
        assert_eq!(px(&pm, 50, 89), (0, 255, 0));
        // Just outside it, the black one shows.
        assert_eq!(px(&pm, 50, 91), (0, 0, 0));
    }

    /// A further fill with a look of its own: grown and moved, it paints
    /// beside the first fill, which stays where it is.
    #[test]
    fn a_further_fill_draws_with_its_own_look() {
        let mut r = rect(1, Rect::new(10.0, 40.0, 30.0, 60.0), Color::rgb8(255, 0, 0));
        r.stroke = None;
        r.extra_fills = vec![Fill::Solid(Color::rgb8(0, 0, 255))];
        r.fill_looks = Some(Box::new(vec![tracedraw_core::document::FillLook {
            grow: 5.0,
            move_x: 50.0,
            move_y: 0.0,
            blur: 0.0,
        }]));
        let (doc, page) = doc_with(vec![r]);
        let opts = RenderOptions {
            width: 100,
            height: 100,
            view: ViewTransform {
                zoom: 1.0,
                origin_x: 0.0,
                origin_y: 100.0,
                stretch_y: 1.0,
            },
            ..RenderOptions::default()
        };
        let pm = render_page(&doc, page, &opts).unwrap();
        assert_eq!(px(&pm, 20, 50), (255, 0, 0), "the first fill in place");
        assert_eq!(px(&pm, 70, 50), (0, 0, 255), "the further fill moved");
        // Grown by 5 mm: past the moved box's edge (x 80).
        assert_eq!(px(&pm, 83, 50), (0, 0, 255));
        assert_eq!(pm.pixel(40, 50).unwrap().alpha(), 0);
    }

    /// A clip frame as an opacity mask: the contents show where the
    /// frame is light, not where it is dark, and nothing outside it; the
    /// frame is not drawn. Inverted, the other way round.
    #[test]
    fn opacity_masks_show_the_contents_by_the_frame_brightness() {
        let red = Color::rgb8(255, 0, 0);
        let contents = rect(1, Rect::new(0.0, 0.0, 100.0, 100.0), red);
        // The frame: white on the left half, black on the right, covering
        // the middle 80 mm.
        let mut frame = rect(2, Rect::new(10.0, 10.0, 90.0, 90.0), Color::WHITE);
        frame.fill = Fill::Fountain(Fountain {
            kind: FountainKind::Linear,
            stops: vec![
                Stop::new(0.0, Color::WHITE),
                Stop::new(0.499, Color::WHITE),
                Stop::new(0.501, Color::BLACK),
                Stop::new(1.0, Color::BLACK),
            ],
            ..Fountain::two(FountainKind::Linear, Color::WHITE, Color::BLACK, 0.0)
        });
        let mut clip = Shape::new(
            tracedraw_core::ShapeId(3),
            ShapeKind::ClipFrame {
                frame: Box::new(frame),
                contents: vec![contents],
            },
        );
        clip.opacity_mask = Some(tracedraw_core::document::OpacityMask { invert: false });
        let opts = RenderOptions {
            width: 100,
            height: 100,
            view: ViewTransform {
                zoom: 1.0,
                origin_x: 0.0,
                origin_y: 100.0,
                stretch_y: 1.0,
            },
            ..RenderOptions::default()
        };
        let render = |clip: &Shape| {
            let (doc, page) = doc_with(vec![clip.clone()]);
            render_page(&doc, page, &opts).unwrap()
        };
        let pm = render(&clip);
        // White part: the red shows fully; black part and outside: nothing.
        assert_eq!(pm.pixel(25, 50).unwrap().alpha(), 255);
        assert_eq!(px(&pm, 25, 50), (255, 0, 0));
        assert_eq!(pm.pixel(75, 50).unwrap().alpha(), 0);
        assert_eq!(pm.pixel(5, 50).unwrap().alpha(), 0);
        clip.opacity_mask = Some(tracedraw_core::document::OpacityMask { invert: true });
        let pm = render(&clip);
        assert_eq!(pm.pixel(25, 50).unwrap().alpha(), 0);
        assert_eq!(pm.pixel(75, 50).unwrap().alpha(), 255);
        assert_eq!(pm.pixel(5, 50).unwrap().alpha(), 0);
    }

    /// The drag preview moves an object inside a moved group by the
    /// page-space transform, and only that object.
    #[test]
    fn the_drag_preview_reaches_objects_inside_groups() {
        let red = Color::rgb8(255, 0, 0);
        let blue = Color::rgb8(0, 0, 255);
        let a = rect(1, Rect::new(0.0, 0.0, 10.0, 10.0), red);
        let b = rect(2, Rect::new(20.0, 0.0, 30.0, 10.0), blue);
        let mut g = Shape::new(
            tracedraw_core::ShapeId(3),
            ShapeKind::Group {
                children: vec![a, b],
            },
        );
        g.transform = Affine::translate((10.0, 10.0));
        let (doc, page) = doc_with(vec![g]);
        let mut opts = RenderOptions {
            width: 100,
            height: 100,
            view: ViewTransform {
                zoom: 1.0,
                origin_x: 0.0,
                origin_y: 100.0,
                stretch_y: 1.0,
            },
            ..RenderOptions::default()
        };
        // Without a preview: red at page (15, 15), screen (15, 85).
        let pm = render_page(&doc, page, &opts).unwrap();
        assert_eq!(px(&pm, 15, 85), (255, 0, 0));
        opts.preview = Some(Preview {
            shapes: [tracedraw_core::ShapeId(1)].into_iter().collect(),
            transform: Affine::translate((0.0, 40.0)),
        });
        let pm = render_page(&doc, page, &opts).unwrap();
        // The red square moved up by 40 mm; the blue one stayed.
        assert_eq!(pm.pixel(15, 85).unwrap().alpha(), 0);
        assert_eq!(px(&pm, 15, 45), (255, 0, 0));
        assert_eq!(px(&pm, 35, 85), (0, 0, 255));
    }

    #[test]
    fn open_curves_are_not_filled_unless_the_document_says_so() {
        use tracedraw_core::geometry::Point as P;
        // A "U" (open) and a triangle (closed) in one curve.
        let mut path = BezPath::new();
        path.move_to(P::new(10.0, 90.0));
        path.line_to(P::new(10.0, 10.0));
        path.line_to(P::new(40.0, 10.0));
        path.line_to(P::new(40.0, 90.0));
        path.move_to(P::new(60.0, 10.0));
        path.line_to(P::new(90.0, 10.0));
        path.line_to(P::new(75.0, 60.0));
        path.close_path();
        let closed = closed_subpaths(&path);
        assert_eq!(closed.elements().len(), 4);
        let mut s = Shape::new(
            tracedraw_core::ShapeId(1),
            ShapeKind::Path {
                path,
                closed: false,
            },
        );
        s.fill = Fill::Solid(Color::rgb8(255, 0, 0));
        s.stroke = None;
        let (mut doc, page) = doc_with(vec![s]);
        let opts = RenderOptions {
            width: 100,
            height: 100,
            view: ViewTransform {
                zoom: 1.0,
                origin_x: 0.0,
                origin_y: 100.0,
                stretch_y: 1.0,
            },
            ..RenderOptions::default()
        };
        let pm = render_page(&doc, page, &opts).unwrap();
        // Inside the U (page 25, 50 -> screen 25, 50): nothing.
        assert_eq!(pm.pixel(25, 50).unwrap().alpha(), 0);
        // Inside the triangle (page 75, 20 -> screen 75, 80): red.
        assert_eq!(px(&pm, 75, 80), (255, 0, 0));
        doc.metadata.fill_open_curves = true;
        let pm = render_page(&doc, page, &opts).unwrap();
        assert_eq!(px(&pm, 25, 50), (255, 0, 0));
    }

    /// A live test of 0.41: on a rounded rectangle with a transparency,
    /// part of the outline vanished and the target (fill, outline or
    /// all) made no difference to the outline. The mask stopped at the
    /// object's box, so the outer half of the outline stayed opaque while
    /// the inner half and the corners faded. Now the whole outline fades
    /// with the outline or the whole object as the target, and keeps its
    /// colour with the fill alone.
    #[test]
    fn a_transparency_fades_the_part_its_target_names_outline_and_all() {
        use tracedraw_core::live::{Effect, MergeMode};
        let shape = |target: u8| {
            let mut s = Shape::new(
                tracedraw_core::ShapeId(1),
                ShapeKind::Rect {
                    rect: Rect::new(20.0, 20.0, 80.0, 60.0),
                    radius: 8.0,
                    corners: None,
                },
            );
            s.fill = Fill::Solid(Color::rgb8(0, 0, 255));
            s.stroke = Some(tracedraw_core::Stroke::new(Color::BLACK, 4.0));
            s.effects.push(Effect::Transparency {
                mask: Fill::Solid(Color::Gray { v: 0.5 }),
                merge: MergeMode::Normal,
                target,
            });
            s
        };
        let opts = RenderOptions {
            width: 100,
            height: 100,
            view: ViewTransform {
                zoom: 1.0,
                origin_x: 0.0,
                origin_y: 100.0,
                stretch_y: 1.0,
            },
            ..RenderOptions::default()
        };
        let alpha = |pm: &Pixmap, (x, y): (f64, f64)| {
            pm.pixel(x as u32, (100.0 - y) as u32).unwrap().alpha()
        };
        let half = |a: u8| (100..=160).contains(&a);
        // Page points: the outline's outer half on the top side (outside
        // the box), the round corner's outline past the fill, the
        // outline's inner half (over the fill), the fill.
        let (outer, corner, inner, fill) = ((50.0, 61.5), (21.8, 58.2), (50.0, 58.5), (50.0, 40.0));
        let colour = |pm: &Pixmap, (x, y): (f64, f64)| px(pm, x as u32, (100.0 - y) as u32);
        for target in [1u8, 2] {
            let (doc, page) = doc_with(vec![shape(target)]);
            let pm = render_page(&doc, page, &opts).unwrap();
            for p in [outer, corner] {
                let a = alpha(&pm, p);
                assert!(half(a), "target {target} at {p:?}: {a}");
                assert_eq!(colour(&pm, p), (0, 0, 0));
            }
            let (f, i) = (alpha(&pm, fill), alpha(&pm, inner));
            if target == 1 {
                // The fill as it is, the outline over it half see-through.
                assert_eq!((f, colour(&pm, fill)), (255, (0, 0, 255)));
                let (r, g, b) = colour(&pm, inner);
                assert!(i == 255 && r == 0 && g == 0 && half(b), "{r} {g} {b}");
            } else {
                // The whole object half see-through.
                assert!(half(f) && half(i), "{f} {i}");
                assert_eq!(colour(&pm, inner), (0, 0, 0));
            }
        }
        // The fill alone: the outline stays opaque all round.
        let (doc, page) = doc_with(vec![shape(0)]);
        let pm = render_page(&doc, page, &opts).unwrap();
        for p in [outer, corner, inner] {
            assert_eq!(alpha(&pm, p), 255, "at {p:?}");
            assert_eq!(colour(&pm, p), (0, 0, 0));
        }
        assert!(half(alpha(&pm, fill)));
        assert_eq!(colour(&pm, fill), (0, 0, 255));
    }

    /// A live test of 0.41: a variable outline on a closed object (an
    /// ellipse here) spread over the whole object, stepped and left an
    /// open node where the path closes. A node on the closing point (the
    /// right of the ellipse) is wide there on both sides of it, with no
    /// gap, and the outline tapers to its base width half way round.
    #[test]
    fn a_variable_outline_on_an_ellipse_tapers_across_its_closing_point() {
        use tracedraw_core::style::WidthNode;
        let mut s = Shape::new(
            tracedraw_core::ShapeId(1),
            ShapeKind::Ellipse {
                rect: Rect::new(20.0, 20.0, 80.0, 80.0),
                arc: None,
            },
        );
        s.fill = Fill::None;
        let mut st = tracedraw_core::Stroke::new(Color::BLACK, 2.0);
        st.widths = vec![WidthNode::symmetric(0.0, 8.0)];
        s.stroke = Some(st);
        let (doc, page) = doc_with(vec![s]);
        let opts = RenderOptions {
            width: 100,
            height: 100,
            view: ViewTransform {
                zoom: 1.0,
                origin_x: 0.0,
                origin_y: 100.0,
                stretch_y: 1.0,
            },
            ..RenderOptions::default()
        };
        let pm = render_page(&doc, page, &opts).unwrap();
        let ink = |x: f64, y: f64| pm.pixel(x as u32, (100.0 - y) as u32).unwrap().alpha() > 200;
        // At the closing point, (80, 50): 4 mm each way, just above and
        // just below it alike.
        for y in [47.5, 49.5, 50.5, 52.5] {
            assert!(ink(83.2, y) && ink(76.8, y), "at {y}");
            assert!(!ink(84.8, y), "at {y}");
        }
        // Half way round, (20, 50): the base width, 1 mm each way.
        assert!(ink(20.2, 50.5) && !ink(17.5, 50.5) && !ink(22.5, 50.5));
    }

    #[test]
    fn calligraphic_outline_is_wide_across_the_nib_and_thin_along_it() {
        // 25.4 dpi: one pixel per millimetre. Flat 6 mm nib at 0 degrees.
        let mut nib = tracedraw_core::Stroke::hairline(Color::BLACK);
        nib.width = 6.0;
        nib.stretch = 0.1;
        nib.nib_angle = 0.0;
        let mut vertical = Shape::new(
            tracedraw_core::ShapeId(1),
            ShapeKind::Path {
                path: {
                    let mut p = tracedraw_core::geometry::BezPath::new();
                    p.move_to(tracedraw_core::geometry::Point::new(30.0, 10.0));
                    p.line_to(tracedraw_core::geometry::Point::new(30.0, 90.0));
                    p
                },
                closed: false,
            },
        );
        vertical.fill = Fill::None;
        vertical.stroke = Some(nib.clone());
        let mut horizontal = vertical.clone();
        horizontal.id = tracedraw_core::ShapeId(2);
        horizontal.kind = ShapeKind::Path {
            path: {
                let mut p = tracedraw_core::geometry::BezPath::new();
                p.move_to(tracedraw_core::geometry::Point::new(10.0, 70.0));
                p.line_to(tracedraw_core::geometry::Point::new(90.0, 70.0));
                p
            },
            closed: false,
        };
        horizontal.stroke = Some(nib);
        let (doc, page) = doc_with(vec![vertical, horizontal]);
        let pm = render_page_image(&doc, page, 25.4).unwrap();
        // The vertical stroke covers x 27..33 at y = 50 (image y = 50).
        assert_eq!(px(&pm, 28, 50), (0, 0, 0));
        assert_eq!(px(&pm, 32, 50), (0, 0, 0));
        assert_ne!(px(&pm, 25, 50), (0, 0, 0));
        // The horizontal stroke is only 0.6 mm tall: two pixels away it is gone.
        assert_ne!(px(&pm, 60, 28), (0, 0, 0));
        assert_ne!(px(&pm, 60, 32), (0, 0, 0));
    }

    #[test]
    fn invert_lens_inverts_what_is_beneath() {
        let base = rect(1, Rect::new(0.0, 0.0, 100.0, 100.0), Color::rgb8(255, 0, 0));
        let mut lens = rect(2, Rect::new(25.0, 25.0, 75.0, 75.0), Color::WHITE);
        lens.effects.push(tracedraw_core::live::Effect::Lens(
            tracedraw_core::live::Lens::Invert,
        ));
        let (doc, page) = doc_with(vec![base, lens]);
        let pm = render_page_image(&doc, page, 25.4).unwrap();
        assert_eq!(px(&pm, 50, 50), (0, 255, 255));
        assert_eq!(px(&pm, 5, 5), (255, 0, 0));
    }

    /// #3439: a lens works on its own box only: no page-sized copy or
    /// mask, and a lens partly off the page still changes what it covers
    /// and nothing past it.
    #[test]
    fn lens_works_on_its_own_box_only() {
        use tracedraw_core::live::{Effect, Lens};
        let base = rect(1, Rect::new(0.0, 0.0, 100.0, 100.0), Color::rgb8(255, 0, 0));
        let mut small = rect(2, Rect::new(40.0, 40.0, 50.0, 50.0), Color::WHITE);
        small.effects.push(Effect::Lens(Lens::Invert));
        let (doc, page) = doc_with(vec![base.clone(), small]);
        let _one_at_a_time = SCRATCH_TEST.lock().unwrap_or_else(|e| e.into_inner());
        LARGEST_SCRATCH.store(0, std::sync::atomic::Ordering::Relaxed);
        let pm = render_page_image(&doc, page, 254.0).unwrap();
        let page_px = pm.width() as usize * pm.height() as usize;
        let largest = LARGEST_SCRATCH.load(std::sync::atomic::Ordering::Relaxed);
        assert!(largest > 0 && largest * 20 < page_px, "{largest} of {page_px}");
        assert_eq!(px(&pm, 450, 550), (0, 255, 255));
        assert_eq!(px(&pm, 350, 550), (255, 0, 0));
        // Partly off the page, and magnifying, which reads past its box.
        for lens_kind in [Lens::Invert, Lens::Magnify { amount: 2.0 }, Lens::FishEye { rate: -500.0 }] {
            let mut off = rect(2, Rect::new(-30.0, 80.0, 30.0, 140.0), Color::WHITE);
            off.effects.push(Effect::Lens(lens_kind.clone()));
            let (doc, page) = doc_with(vec![base.clone(), off]);
            let pm = render_page_image(&doc, page, 25.4).unwrap();
            if matches!(lens_kind, Lens::Invert) {
                assert_eq!(px(&pm, 10, 5), (0, 255, 255));
            }
            assert_eq!(px(&pm, 40, 5), (255, 0, 0), "{lens_kind:?}");
            assert_eq!(px(&pm, 10, 30), (255, 0, 0), "{lens_kind:?}");
        }
    }

    /// The Cut lens: where its middle line crosses the objects beneath
    /// each side grows a pointed spur in the object's colour, the cut
    /// leaves a gap, and a shadow fades over the objects (only) from the
    /// cut to the lens's lower edge; the bare page stays as it is.
    #[test]
    fn cut_lens_grows_spurs_and_casts_a_shadow_below_the_cut() {
        use tracedraw_core::live::{Effect, Lens};
        let green = Color::rgb8(0, 128, 0);
        let page_white = rect(1, Rect::new(0.0, 0.0, 100.0, 100.0), Color::WHITE);
        let bar1 = rect(2, Rect::new(20.0, 10.0, 30.0, 90.0), green);
        let bar2 = rect(3, Rect::new(50.0, 10.0, 60.0, 90.0), green);
        let mut lens = rect(4, Rect::new(0.0, 40.0, 100.0, 60.0), Color::WHITE);
        lens.stroke = None;
        lens.effects.push(Effect::Lens(Lens::Cut {
            spur_length: 6.0,
            spur_width: 6.0,
            curve: 2.0,
            gap: 0.4,
            shadow: 80.0,
            shadow_color: Color::BLACK,
            shadow_up: false,
        }));
        let (doc, page) = doc_with(vec![page_white, bar1, bar2, lens]);
        // 10 pixels a millimetre.
        let pm = render_page_image(&doc, page, 254.0).unwrap();
        let at = |x: f64, y: f64| px(&pm, (x * 10.0) as u32, (y * 10.0) as u32);
        // Spurs: green just outside both sides of a bar, beside the cut.
        assert_eq!(at(19.0, 51.0), (0, 128, 0), "left spur");
        assert_eq!(at(31.0, 49.0), (0, 128, 0), "right spur");
        // A spur narrows: away from the cut it is the page again.
        assert_eq!(at(19.0, 53.5), (255, 255, 255));
        assert_eq!(at(15.0, 51.0), (255, 255, 255));
        // Between the bars, past the spurs, the page is untouched.
        assert_eq!(at(40.0, 52.0), (255, 255, 255));
        // The cut: a thin line of page colour across the bar.
        assert_eq!(at(25.0, 50.0), (255, 255, 255));
        // The shadow: darkest under the cut, fading to the lens's edge,
        // none above the cut and none outside the lens.
        let g = |y: f64| at(25.0, y).1;
        assert!(g(51.0) < g(55.0) && g(55.0) < g(59.0), "{} {} {}", g(51.0), g(55.0), g(59.0));
        assert!(g(51.0) < 60, "{}", g(51.0));
        assert_eq!(at(25.0, 45.0), (0, 128, 0));
        assert_eq!(at(25.0, 70.0), (0, 128, 0));
    }

    /// #2800: Wireframe shows the objects beneath as outlines and fills
    /// in its colours, the bare page as it is; Tinted grayscale turns
    /// black into the lens colour and keeps white.
    #[test]
    fn wireframe_and_tinted_grayscale_lenses_show_what_is_beneath() {
        let page_white = rect(1, Rect::new(0.0, 0.0, 100.0, 100.0), Color::WHITE);
        let base = rect(2, Rect::new(30.0, 30.0, 70.0, 70.0), Color::rgb8(255, 0, 0));
        let mut lens = rect(3, Rect::new(0.0, 0.0, 100.0, 100.0), Color::WHITE);
        lens.effects.push(tracedraw_core::live::Effect::Lens(
            tracedraw_core::live::Lens::Wireframe {
                outline: Color::rgb8(0, 0, 255),
                fill: Color::rgb8(255, 255, 0),
            },
        ));
        let (doc, page) = doc_with(vec![page_white.clone(), base.clone(), lens]);
        let pm = render_page_image(&doc, page, 25.4).unwrap();
        assert_eq!(px(&pm, 50, 50), (255, 255, 0), "inside: the fill colour");
        assert_eq!(px(&pm, 30, 50), (0, 0, 255), "the edge: the outline colour");
        assert_eq!(px(&pm, 10, 10), (255, 255, 255), "the page stays");
        let black = rect(2, Rect::new(30.0, 30.0, 70.0, 70.0), Color::BLACK);
        let mut lens = rect(3, Rect::new(0.0, 0.0, 100.0, 100.0), Color::WHITE);
        lens.effects.push(tracedraw_core::live::Effect::Lens(
            tracedraw_core::live::Lens::TintedGrayscale {
                color: Color::rgb8(255, 0, 0),
            },
        ));
        let (doc, page) = doc_with(vec![page_white, black, lens]);
        let pm = render_page_image(&doc, page, 25.4).unwrap();
        assert_eq!(px(&pm, 50, 50), (255, 0, 0), "black becomes the tint");
        assert_eq!(px(&pm, 10, 10), (255, 255, 255), "white stays white");
    }

    #[test]
    fn bitmap_effects_lens_runs_the_registered_effects_beneath_it() {
        // A stand-in for the application's effects: "swap" swaps red and
        // blue.
        fn run(effects: &[tracedraw_core::BitmapEffect], _w: u32, _h: u32, rgba: &mut Vec<u8>) {
            if effects.iter().any(|e| e.id == "swap") {
                for px in rgba.chunks_mut(4) {
                    px.swap(0, 2);
                }
            }
        }
        set_pixel_effects(run);
        let base = rect(1, Rect::new(0.0, 0.0, 100.0, 100.0), Color::rgb8(255, 0, 0));
        let mut lens = rect(2, Rect::new(25.0, 25.0, 75.0, 75.0), Color::WHITE);
        lens.effects.push(tracedraw_core::live::Effect::Lens(
            tracedraw_core::live::Lens::Effects {
                effects: vec![tracedraw_core::BitmapEffect {
                    id: "swap".into(),
                    params: Default::default(),
                    visible: true,
                }],
            },
        ));
        let (doc, page) = doc_with(vec![base, lens]);
        let pm = render_page_image(&doc, page, 25.4).unwrap();
        assert_eq!(px(&pm, 50, 50), (0, 0, 255));
        assert_eq!(px(&pm, 5, 5), (255, 0, 0));
    }

    #[test]
    fn simulated_overprint_multiplies_the_fill_over_what_is_beneath() {
        let yellow = rect(
            1,
            Rect::new(0.0, 0.0, 100.0, 100.0),
            Color::rgb8(255, 255, 0),
        );
        let mut grey = rect(
            2,
            Rect::new(25.0, 25.0, 75.0, 75.0),
            Color::rgb8(128, 128, 128),
        );
        grey.overprint_fill = true;
        let (doc, page) = doc_with(vec![yellow, grey]);
        // Off (the default): the grey knocks the yellow out.
        let pm = render_page_image(&doc, page, 25.4).unwrap();
        assert_eq!(px(&pm, 50, 50), (128, 128, 128));
        // On: multiply gives a darker yellow, and the surroundings stay yellow.
        let opts = RenderOptions {
            simulate_overprints: true,
            ..RenderOptions::default()
        };
        let pm = render_page_image_with(&doc, page, 25.4, &opts).unwrap();
        let (r, g, b) = px(&pm, 50, 50);
        assert!((120..=136).contains(&r) && r == g && b == 0, "{r} {g} {b}");
        assert_eq!(px(&pm, 5, 5), (255, 255, 0));
    }

    #[test]
    fn simulated_overprint_applies_to_outlines_and_leaves_plain_objects_alone() {
        let yellow = rect(
            1,
            Rect::new(0.0, 0.0, 100.0, 100.0),
            Color::rgb8(255, 255, 0),
        );
        // A thick grey outline across the page, no fill.
        let mut line = Shape::new(
            tracedraw_core::ShapeId(2),
            ShapeKind::Path {
                path: {
                    let mut p = tracedraw_core::geometry::BezPath::new();
                    p.move_to((0.0, 50.0));
                    p.line_to((100.0, 50.0));
                    p
                },
                closed: false,
            },
        );
        line.fill = Fill::None;
        line.stroke = Some(Stroke::new(Color::rgb8(128, 128, 128), 10.0));
        line.overprint_outline = true;
        // A plain grey square that does not overprint.
        let plain = rect(
            3,
            Rect::new(5.0, 5.0, 20.0, 20.0),
            Color::rgb8(128, 128, 128),
        );
        let (doc, page) = doc_with(vec![yellow, line, plain]);
        let opts = RenderOptions {
            simulate_overprints: true,
            ..RenderOptions::default()
        };
        let pm = render_page_image_with(&doc, page, 25.4, &opts).unwrap();
        let (r, g, b) = px(&pm, 50, 50);
        assert!(
            (120..=136).contains(&r) && r == g && b == 0,
            "outline {r} {g} {b}"
        );
        assert_eq!(px(&pm, 12, 87), (128, 128, 128), "plain fill knocks out");
    }

    #[test]
    fn outlines_thinner_than_a_pixel_stay_one_pixel_wide() {
        // A 0.1 mm black line at 25.4 dpi (a tenth of a pixel): drawn as a
        // full-strength one-pixel line, not faded to a tenth.
        let mut line = Shape::new(
            tracedraw_core::ShapeId(1),
            ShapeKind::Path {
                path: {
                    let mut p = tracedraw_core::geometry::BezPath::new();
                    p.move_to((0.0, 50.5));
                    p.line_to((100.0, 50.5));
                    p
                },
                closed: false,
            },
        );
        line.fill = Fill::None;
        line.stroke = Some(Stroke::new(Color::BLACK, 0.1));
        let (doc, page) = doc_with(vec![line]);
        let pm = render_page_image(&doc, page, 25.4).unwrap();
        let darkest = (0..100).map(|y| px(&pm, 50, y).0).min().unwrap_or(255);
        assert!(darkest < 64, "darkest {darkest}");
    }

    #[test]
    fn outlines_behind_the_fill_show_only_outside_it() {
        // A red square with a 10 mm white outline behind its fill: inside
        // the square stays red, the outline shows only outside.
        let mut s = rect(1, Rect::new(20.0, 20.0, 80.0, 80.0), Color::rgb8(255, 0, 0));
        let mut st = Stroke::new(Color::rgb8(0, 0, 255), 10.0);
        st.behind_fill = true;
        s.stroke = Some(st);
        let (doc, page) = doc_with(vec![s]);
        let pm = render_page_image(&doc, page, 25.4).unwrap();
        assert_eq!(px(&pm, 22, 50), (255, 0, 0), "fill over the inner half");
        assert_eq!(px(&pm, 17, 50), (0, 0, 255), "outline outside");
    }

    #[test]
    fn wireframe_draws_hairlines_whatever_the_outline() {
        // A red square with a 10 mm outline at half opacity.
        let mut s = rect(1, Rect::new(20.0, 20.0, 80.0, 80.0), Color::rgb8(255, 0, 0));
        s.stroke = Some(Stroke::new(Color::BLACK, 10.0));
        s.opacity = 0.5;
        let (doc, page) = doc_with(vec![s]);
        let opts = RenderOptions {
            wireframe: true,
            ..RenderOptions::default()
        };
        let pm = render_page_image_with(&doc, page, 25.4, &opts).unwrap();
        assert_eq!(px(&pm, 50, 50), (255, 255, 255), "no fill");
        assert_eq!(px(&pm, 24, 50), (255, 255, 255), "no outline width");
        assert_eq!(px(&pm, 16, 50), (255, 255, 255), "no outline width outside");
        let edge = (18..23).map(|x| px(&pm, x, 50).0).min().unwrap();
        assert!(edge < 200, "a dark hairline on the edge, darkest {edge}");
        // The hairline is black whatever the outline's colour (#2041): a
        // blue outline gives no blue line, and a white one still shows.
        for colour in [Color::rgb8(0, 0, 255), Color::WHITE] {
            let mut s = rect(1, Rect::new(20.0, 20.0, 80.0, 80.0), Color::WHITE);
            s.stroke = Some(Stroke::new(colour, 10.0));
            let (doc, page) = doc_with(vec![s]);
            let pm = render_page_image_with(&doc, page, 25.4, &opts).unwrap();
            let line: Vec<_> = (18..23).map(|x| px(&pm, x, 50)).collect();
            assert!(
                line.iter().all(|p| p.0 == p.1 && p.1 == p.2),
                "a grey to black line, no colour: {line:?}"
            );
            let darkest = line.iter().map(|p| p.0).min().unwrap();
            assert!(darkest < 200, "a dark hairline, darkest {darkest}");
        }
    }

    /// The Wireframe view cuts a clip frame's contents to the frame, as
    /// the other views do (#2146): a content reaching past the frame
    /// shows its line inside the frame only. It was drawn whole.
    #[test]
    fn wireframe_cuts_clip_frame_contents_to_the_frame() {
        let frame = rect(1, Rect::new(20.0, 20.0, 60.0, 60.0), Color::WHITE);
        // From inside the frame to far outside it at the right.
        let content = rect(2, Rect::new(40.0, 30.0, 90.0, 50.0), Color::rgb8(255, 0, 0));
        let clip = Shape::new(
            ShapeId(3),
            ShapeKind::ClipFrame {
                frame: Box::new(frame),
                contents: vec![content],
            },
        );
        let (doc, page) = doc_with(vec![clip]);
        let opts = RenderOptions {
            wireframe: true,
            ..RenderOptions::default()
        };
        let pm = render_page_image_with(&doc, page, 25.4, &opts).unwrap();
        let dark = |x0: u32, x1: u32, y: u32| (x0..x1).any(|x| px(&pm, x, y).0 < 200);
        // Page y 50 is row 50 of the 100 px picture; the content's top and
        // bottom sides run along rows 50 and 70 (page y 50 and 30).
        assert!(
            dark(38, 43, 60),
            "the content's left side, inside the frame"
        );
        assert!(dark(58, 63, 60), "the frame's right side");
        assert!(!dark(64, 95, 60), "nothing of the content's far side");
        let row = |y: u32| (66..88).any(|x| px(&pm, x, y).0 < 200);
        assert!(
            !(45..75).any(row),
            "no content lines to the right of the frame"
        );
        assert_eq!(px(&pm, 50, 60), (255, 255, 255), "no fill");
    }

    /// A clip frame whose outline is behind its fill draws the outline
    /// beneath the fill and the contents, as text in a PowerClip with a
    /// wide outline behind its fill is drawn in the file of #3110: the
    /// outline came over both, so the frame showed the outline's colour
    /// where its fill and contents were.
    #[test]
    fn a_clip_frame_outline_behind_its_fill_goes_beneath() {
        let mut frame = rect(1, Rect::new(20.0, 20.0, 60.0, 60.0), Color::rgb8(0, 0, 255));
        let mut st = tracedraw_core::Stroke::new(Color::BLACK, 30.0);
        st.behind_fill = true;
        frame.stroke = Some(st);
        let content = rect(2, Rect::new(24.0, 24.0, 32.0, 32.0), Color::rgb8(255, 0, 0));
        let clip = Shape::new(
            ShapeId(3),
            ShapeKind::ClipFrame {
                frame: Box::new(frame),
                contents: vec![content],
            },
        );
        let (doc, page) = doc_with(vec![clip]);
        let pm = render_page_image_with(&doc, page, 25.4, &RenderOptions::default()).unwrap();
        // Page y 40 is row 60 of the 100 px picture.
        assert_eq!(px(&pm, 40, 60), (0, 0, 255), "the fill, over the outline");
        assert_eq!(px(&pm, 28, 72), (255, 0, 0), "the contents, over the outline");
        assert_eq!(px(&pm, 12, 60), (0, 0, 0), "the outline, past the frame");
    }

    /// A picture in the Wireframe view is a grey box crossed corner to
    /// corner, the mark of a picture left out, not a plain box (#2146).
    #[test]
    fn wireframe_marks_pictures_with_a_cross() {
        let mut photo = Pixmap::new(2, 2).unwrap();
        photo.fill(tiny_skia::Color::from_rgba8(255, 0, 0, 255));
        let png = photo.encode_png().unwrap();
        let bmp = Shape::new(
            ShapeId(1),
            ShapeKind::Bitmap {
                rect: Rect::new(20.0, 20.0, 80.0, 80.0),
                width_px: 2,
                height_px: 2,
                png: png.into(),
                fx: None,
            },
        );
        let (doc, page) = doc_with(vec![bmp]);
        let opts = RenderOptions {
            wireframe: true,
            ..RenderOptions::default()
        };
        let pm = render_page_image_with(&doc, page, 25.4, &opts).unwrap();
        // Off the diagonals: the grey of the box.
        assert_eq!(px(&pm, 50, 30), (208, 208, 208));
        // On them, at a quarter and at the middle: a dark line.
        for (x, y) in [(35, 35), (65, 35), (50, 50)] {
            let darkest = (x - 1..=x + 1)
                .flat_map(|i| (y - 1..=y + 1).map(move |j| (i, j)))
                .map(|(i, j)| px(&pm, i, j).0)
                .min()
                .unwrap();
            assert!(darkest < 120, "a diagonal at {x}, {y}: {darkest}");
        }
    }

    #[test]
    fn complex_effects_off_skips_the_lens() {
        let base = rect(1, Rect::new(0.0, 0.0, 100.0, 100.0), Color::rgb8(255, 0, 0));
        let mut lens = rect(2, Rect::new(25.0, 25.0, 75.0, 75.0), Color::WHITE);
        lens.effects.push(tracedraw_core::live::Effect::Lens(
            tracedraw_core::live::Lens::Invert,
        ));
        let (doc, page) = doc_with(vec![base, lens]);
        let full = render_page_image(&doc, page, 25.4).unwrap();
        assert_eq!(px(&full, 50, 50), (0, 255, 255), "lens inverts");
        let opts = RenderOptions {
            complex_effects: false,
            ..RenderOptions::default()
        };
        let fast = render_page_image_with(&doc, page, 25.4, &opts).unwrap();
        assert_eq!(
            px(&fast, 50, 50),
            (255, 255, 255),
            "plain white fill instead"
        );
        assert_eq!(px(&fast, 5, 5), (255, 0, 0));
        assert_ne!(full.data(), fast.data());
    }

    /// #3441: a transparency mask and a uniform transparency work on a
    /// layer the size of what the object reaches, not of the page; an
    /// object partly off the page is still faded where it shows.
    #[test]
    fn transparency_layers_cover_only_the_object() {
        use tracedraw_core::live::{Effect, MergeMode};
        let page_white = rect(1, Rect::new(0.0, 0.0, 100.0, 100.0), Color::WHITE);
        let mut masked = rect(2, Rect::new(40.0, 40.0, 50.0, 50.0), Color::BLACK);
        masked.effects.push(Effect::Transparency {
            mask: Fill::Solid(Color::rgb8(128, 128, 128)),
            merge: MergeMode::Normal,
            target: 2,
        });
        let mut faded = rect(3, Rect::new(60.0, 60.0, 70.0, 70.0), Color::BLACK);
        faded.opacity = 0.5;
        let (doc, page) = doc_with(vec![page_white.clone(), masked, faded]);
        let _one_at_a_time = SCRATCH_TEST.lock().unwrap_or_else(|e| e.into_inner());
        LARGEST_SCRATCH.store(0, std::sync::atomic::Ordering::Relaxed);
        let pm = render_page_image(&doc, page, 254.0).unwrap();
        let page_px = pm.width() as usize * pm.height() as usize;
        let largest = LARGEST_SCRATCH.load(std::sync::atomic::Ordering::Relaxed);
        assert!(largest > 0 && largest * 20 < page_px, "{largest} of {page_px}");
        let grey = px(&pm, 450, 550).0;
        assert!((100..160).contains(&grey), "masked: {grey}");
        let grey = px(&pm, 650, 350).0;
        assert!((100..160).contains(&grey), "faded: {grey}");
        assert_eq!(px(&pm, 550, 450), (255, 255, 255));
        // Partly off the page, the mask still fades what shows.
        let mut off = rect(2, Rect::new(-20.0, 40.0, 20.0, 60.0), Color::BLACK);
        off.effects.push(Effect::Transparency {
            mask: Fill::linear(Color::BLACK, Color::WHITE, 0.0),
            merge: MergeMode::Normal,
            target: 2,
        });
        let (doc, page) = doc_with(vec![page_white, off]);
        let pm = render_page_image(&doc, page, 25.4).unwrap();
        let (left, right) = (px(&pm, 2, 50).0, px(&pm, 18, 50).0);
        assert!(left.abs_diff(right) > 30 && left.max(right) < 255, "{left} {right}");
        assert_eq!(px(&pm, 25, 50), (255, 255, 255));
    }

    /// #3442: a Gaussian blur works on the object and what its blur
    /// spreads over, not on the whole page; an object partly off the
    /// page still blurs where it shows.
    #[test]
    fn gaussian_blur_works_near_the_object_only() {
        use tracedraw_core::live::Effect;
        let page_white = rect(1, Rect::new(0.0, 0.0, 100.0, 100.0), Color::WHITE);
        let mut small = rect(2, Rect::new(40.0, 40.0, 50.0, 50.0), Color::BLACK);
        small.effects.push(Effect::GaussianBlur { radius: 1.0 });
        let (doc, page) = doc_with(vec![page_white.clone(), small]);
        let _one_at_a_time = SCRATCH_TEST.lock().unwrap_or_else(|e| e.into_inner());
        LARGEST_SCRATCH.store(0, std::sync::atomic::Ordering::Relaxed);
        let pm = render_page_image(&doc, page, 254.0).unwrap();
        let page_px = pm.width() as usize * pm.height() as usize;
        let largest = LARGEST_SCRATCH.load(std::sync::atomic::Ordering::Relaxed);
        assert!(largest > 0 && largest * 8 < page_px, "{largest} of {page_px}");
        assert!(px(&pm, 450, 550).0 < 10, "the middle stays black");
        let edge = px(&pm, 400, 550).0;
        assert!(edge > 0 && edge < 255, "{edge}");
        assert_eq!(px(&pm, 300, 550), (255, 255, 255));
        // Partly off the page's left edge, it blurs where it shows.
        let mut off = rect(2, Rect::new(-10.0, 40.0, 5.0, 60.0), Color::BLACK);
        off.effects.push(Effect::GaussianBlur { radius: 2.0 });
        let (doc, page) = doc_with(vec![page_white, off]);
        let pm = render_page_image(&doc, page, 25.4).unwrap();
        assert!(px(&pm, 0, 50).0 < 10, "{:?}", px(&pm, 0, 50));
        let edge = px(&pm, 6, 50).0;
        assert!(edge > 0 && edge < 255, "{edge}");
        assert_eq!(px(&pm, 30, 50), (255, 255, 255));
    }

    /// Blend If: a red bar over a page half black, half white shows only
    /// over the white half with the range of what lies beneath set to
    /// the light half, and nowhere with its own range set past its
    /// lightness; split ends fade.
    #[test]
    fn blend_if_shows_the_object_where_the_lightness_falls_in_range() {
        use tracedraw_core::live::{Effect, BLEND_IF_ALL};
        let dark = rect(1, Rect::new(0.0, 0.0, 50.0, 100.0), Color::BLACK);
        let light = rect(2, Rect::new(50.0, 0.0, 100.0, 100.0), Color::WHITE);
        let bar = |this: [f64; 4], below: [f64; 4]| {
            let mut b = rect(3, Rect::new(0.0, 40.0, 100.0, 60.0), Color::rgb8(255, 0, 0));
            b.effects.push(Effect::BlendIf { this, below });
            b
        };
        let draw = |b: Shape| {
            let (doc, page) = doc_with(vec![dark.clone(), light.clone(), b]);
            render_page_image(&doc, page, 25.4).unwrap()
        };
        let pm = draw(bar(BLEND_IF_ALL, [0.5, 0.5, 1.0, 1.0]));
        assert_eq!(px(&pm, 25, 50), (0, 0, 0));
        assert_eq!(px(&pm, 75, 50), (255, 0, 0));
        let pm = draw(bar([0.5, 0.5, 1.0, 1.0], BLEND_IF_ALL));
        assert_eq!(px(&pm, 25, 50), (0, 0, 0));
        assert_eq!(px(&pm, 75, 50), (255, 255, 255));
        // The full range draws it as it is, both halves.
        let pm = draw(bar(BLEND_IF_ALL, BLEND_IF_ALL));
        assert_eq!(px(&pm, 25, 50), (255, 0, 0));
        // A split dark end over a lightness half way shows it half.
        let grey = rect(1, Rect::new(0.0, 0.0, 100.0, 100.0), Color::rgb8(128, 128, 128));
        let (doc, page) = doc_with(vec![grey, bar(BLEND_IF_ALL, [0.0, 1.0, 1.0, 1.0])]);
        let pm = render_page_image(&doc, page, 25.4).unwrap();
        let (r, g, _) = px(&pm, 50, 50);
        assert!(r > 150 && r < 230 && g > 40 && g < 100, "{:?}", px(&pm, 50, 50));
    }

    /// Displace's shading (#3595): a white bar over a page half dark
    /// grey, half white takes the shadow of the dark half (Multiply) and
    /// stays white over the light half; Screen lightens a black bar over
    /// the light half only; None leaves it as it is.
    #[test]
    fn displace_shading_takes_the_shadows_and_highlights_beneath() {
        use tracedraw_core::live::{Effect, SurfaceShade};
        let dark = rect(1, Rect::new(0.0, 0.0, 50.0, 100.0), Color::rgb8(64, 64, 64));
        let light = rect(2, Rect::new(50.0, 0.0, 100.0, 100.0), Color::WHITE);
        let bar = |color: Color, shade: SurfaceShade| {
            let mut b = rect(3, Rect::new(0.0, 40.0, 100.0, 60.0), color);
            b.effects.push(Effect::Displace {
                map: None,
                horizontal: 0.0,
                vertical: 0.0,
                shade,
                strength: 1.0,
            });
            b
        };
        let draw = |b: Shape| {
            let (doc, page) = doc_with(vec![dark.clone(), light.clone(), b]);
            render_page_image(&doc, page, 25.4).unwrap()
        };
        // The photo's middle grey is 0.5 with no map: the dark half (0.25)
        // halves the white bar.
        let pm = draw(bar(Color::WHITE, SurfaceShade::Multiply));
        let (r, _, _) = px(&pm, 25, 50);
        assert!((120..=136).contains(&r), "{r}");
        assert_eq!(px(&pm, 75, 50), (255, 255, 255));
        let pm = draw(bar(Color::BLACK, SurfaceShade::Screen));
        assert_eq!(px(&pm, 25, 50), (0, 0, 0));
        assert_eq!(px(&pm, 75, 50), (255, 255, 255));
        let pm = draw(bar(Color::WHITE, SurfaceShade::None));
        assert_eq!(px(&pm, 25, 50), (255, 255, 255));
    }

    #[test]
    fn halftone_turns_the_object_and_its_effects_into_dots() {
        use tracedraw_core::live::Effect;
        let page_white = rect(1, Rect::new(0.0, 0.0, 100.0, 100.0), Color::WHITE);
        let mut grey = rect(
            2,
            Rect::new(20.0, 20.0, 80.0, 80.0),
            Color::rgb8(128, 128, 128),
        );
        grey.effects.push(Effect::Halftone {
            pattern: 0,
            mode: 2,
            size: 2.0,
            scale: 1.0,
            softness: 0.0,
            angle: 45.0,
            center_x: 0.5,
            center_y: 0.5,
            clip: true,
            color: Color::BLACK,
        });
        let (doc, page) = doc_with(vec![page_white.clone(), grey.clone()]);
        let pm = render_page_image(&doc, page, 254.0).unwrap();
        // Inside: black dots on the white page, about half of it inked.
        let (mut dark, mut light) = (0, 0);
        for y in (300..700).step_by(3) {
            for x in (300..700).step_by(3) {
                match px(&pm, x, y).0 {
                    0..=60 => dark += 1,
                    200..=255 => light += 1,
                    _ => {}
                }
            }
        }
        let inked = f64::from(dark) / f64::from(dark + light);
        assert!((inked - 0.5).abs() < 0.12, "{inked}");
        // Outside, nothing.
        assert_eq!(px(&pm, 100, 100), (255, 255, 255));
        // An inner shadow on it darkens the edges first: bigger dots there.
        grey.effects.insert(
            0,
            Effect::InnerShadow {
                offset: tracedraw_core::geometry::Vec2::ZERO,
                color: Color::BLACK,
                opacity: 1.0,
                blur: 6.0,
                merge: tracedraw_core::live::MergeMode::Multiply,
                spread: 0.0,
            },
        );
        let (doc, page) = doc_with(vec![page_white, grey]);
        let pm = render_page_image(&doc, page, 254.0).unwrap();
        let ink = |x0: u32, x1: u32| {
            let mut n = 0u32;
            for y in (300..700).step_by(2) {
                for x in (x0..x1).step_by(2) {
                    if px(&pm, x, y).0 < 128 {
                        n += 1;
                    }
                }
            }
            n
        };
        let (edge, middle) = (ink(205, 240), ink(480, 515));
        assert!(edge > middle, "{edge} {middle}");
    }

    #[test]
    fn complex_effects_off_skips_the_transparency_mask() {
        let mut s = rect(1, Rect::new(0.0, 0.0, 100.0, 10.0), Color::rgb8(0, 0, 0));
        s.effects.push(tracedraw_core::live::Effect::Transparency {
            mask: Fill::linear(Color::WHITE, Color::BLACK, 0.0),
            merge: tracedraw_core::live::MergeMode::Normal,
            target: 2,
        });
        let (doc, page) = doc_with(vec![s]);
        let opts = RenderOptions {
            complex_effects: false,
            ..RenderOptions::default()
        };
        let pm = render_page_image_with(&doc, page, 25.4, &opts).unwrap();
        // Without the mask the bar is solid black from end to end.
        assert_eq!(px(&pm, 3, 95), (0, 0, 0));
        assert_eq!(px(&pm, 96, 95), (0, 0, 0));
    }

    #[test]
    fn custom_arrowhead_paints_the_line_end() {
        // A right-pointing triangle, 20 wide and 20 tall, as a custom head.
        let mut tri = tracedraw_core::geometry::BezPath::new();
        tri.move_to((0.0, 0.0));
        tri.line_to((20.0, 10.0));
        tri.line_to((0.0, 20.0));
        tri.close_path();
        let head = tracedraw_core::Arrowhead::from_shape_path(&tri, "Tri");
        let mut line = Shape::new(
            tracedraw_core::ShapeId(1),
            ShapeKind::Path {
                path: {
                    let mut p = tracedraw_core::geometry::BezPath::new();
                    p.move_to((10.0, 50.0));
                    p.line_to((60.0, 50.0));
                    p
                },
                closed: false,
            },
        );
        line.fill = Fill::None;
        line.stroke = Some(Stroke {
            end_arrow: head,
            ..Stroke::new(Color::rgb8(0, 0, 255), 2.0)
        });
        let (doc, page) = doc_with(vec![line]);
        let pm = render_page_image(&doc, page, 25.4).unwrap();
        // Size = 4 widths = 8 mm: the head spans x 52..60, y 46..54. Its
        // base is 8 mm tall at x = 52, well outside the 2 mm line; its tip
        // is at x = 60.
        let blue = |x: u32, y: u32| {
            let (r, g, b) = px(&pm, x, y);
            assert!(
                b == 255 && r < 40 && g < 40,
                "({x}, {y}) should be blue: {r} {g} {b}"
            );
        };
        blue(53, 48);
        blue(53, 52);
        blue(58, 50);
        // Beside the line, before the head, there is nothing.
        assert_eq!(px(&pm, 40, 47), (255, 255, 255));
        // Beyond the tip, nothing either.
        assert_eq!(px(&pm, 63, 50), (255, 255, 255));
        // Without a head the same spots beside the line end stay white.
        let mut plain = doc.clone();
        if let Some(s) = plain.pages[0].layers[0].shapes.first_mut() {
            if let Some(st) = s.stroke.as_mut() {
                st.end_arrow = tracedraw_core::Arrowhead::None;
            }
        }
        let pm2 = render_page_image(&plain, page, 25.4).unwrap();
        assert_eq!(px(&pm2, 53, 48), (255, 255, 255));
        assert_eq!(px(&pm2, 53, 52), (255, 255, 255));
    }

    /// A symmetry's mirrored copy is as see-through as the object (#2121):
    /// the copies were drawn opaque, with fill and outline only.
    #[test]
    fn a_mirrored_copy_keeps_the_transparency() {
        let mut s = rect(1, Rect::new(10.0, 0.0, 30.0, 10.0), Color::rgb8(0, 0, 0));
        s.effects.push(tracedraw_core::live::Effect::Symmetry {
            center: tracedraw_core::geometry::Point::new(50.0, 0.0),
            angle: 90.0,
            lines: 1,
            rotate: false,
        });
        s.effects.push(tracedraw_core::live::Effect::Transparency {
            mask: Fill::Solid(Color::rgb8(128, 128, 128)),
            merge: tracedraw_core::live::MergeMode::Normal,
            target: 2,
        });
        let (doc, page) = doc_with(vec![s]);
        let pm = render_page_image(&doc, page, 25.4).unwrap();
        let original = px(&pm, 20, 95).0;
        let copy = px(&pm, 80, 95).0;
        assert!(
            (90..170).contains(&original),
            "the object is half see-through: {original}"
        );
        assert!(
            original.abs_diff(copy) <= 2,
            "the copy as the object: {original} against {copy}"
        );
    }

    /// An inner shadow darkens the inside of its object along the edges
    /// facing away from its offset, and nothing outside it (#2473).
    #[test]
    fn an_inner_shadow_darkens_the_inside_away_from_its_offset() {
        let mut s = rect(1, Rect::new(20.0, 20.0, 80.0, 80.0), Color::WHITE);
        s.effects.push(tracedraw_core::live::Effect::InnerShadow {
            offset: tracedraw_core::geometry::Vec2::new(6.0, -6.0),
            color: Color::BLACK,
            opacity: 1.0,
            blur: 0.0,
            merge: tracedraw_core::live::MergeMode::Normal,
            spread: 0.0,
        });
        let (doc, page) = doc_with(vec![s]);
        let pm = render_page_image(&doc, page, 25.4).unwrap();
        // Page y grows upwards; the picture's rows grow downwards.
        assert!(px(&pm, 23, 50).0 < 30, "left inside is dark");
        assert!(px(&pm, 50, 23).0 < 30, "top inside is dark");
        assert_eq!(px(&pm, 76, 50), (255, 255, 255), "right inside is lit");
        assert_eq!(px(&pm, 50, 76), (255, 255, 255), "bottom inside is lit");
        assert_eq!(px(&pm, 50, 50), (255, 255, 255), "middle is lit");
        assert_eq!(px(&pm, 15, 50), (255, 255, 255), "outside is untouched");
        // Feathered, the edge fades into the middle.
        let mut f = rect(1, Rect::new(20.0, 20.0, 80.0, 80.0), Color::WHITE);
        f.effects.push(tracedraw_core::live::Effect::InnerShadow {
            offset: tracedraw_core::geometry::Vec2::new(6.0, -6.0),
            color: Color::BLACK,
            opacity: 1.0,
            blur: 4.0,
            merge: tracedraw_core::live::MergeMode::Normal,
            spread: 0.0,
        });
        let (doc, page) = doc_with(vec![f]);
        let pm = render_page_image(&doc, page, 25.4).unwrap();
        let (edge, mid) = (px(&pm, 22, 50).0, px(&pm, 30, 50).0);
        assert!(edge < mid && mid < 255, "fades from {edge} to {mid}");
        assert_eq!(px(&pm, 15, 50), (255, 255, 255), "outside is untouched");
    }

    /// Soft edge: a black square fades out towards its edge over the
    /// radius, black in the middle, untouched outside.
    #[test]
    fn soft_edge_fades_the_object_towards_its_edge() {
        let mut f = rect(1, Rect::new(20.0, 20.0, 80.0, 80.0), Color::BLACK);
        f.effects
            .push(tracedraw_core::live::Effect::Feather { radius: 10.0 });
        let (doc, page) = doc_with(vec![f]);
        let pm = render_page_image(&doc, page, 25.4).unwrap();
        assert!(px(&pm, 50, 50).0 < 10, "{:?}", px(&pm, 50, 50));
        let (edge, inside) = (px(&pm, 21, 50).0, px(&pm, 26, 50).0);
        assert!(edge > 150 && inside < edge, "fades from {inside} to {edge}");
        assert_eq!(px(&pm, 15, 50), (255, 255, 255), "outside is untouched");
    }

    /// Spread makes an inner shadow's feathered edge solid further in:
    /// darker at the same depth, still fading toward the middle and
    /// still nothing outside the object.
    #[test]
    fn inner_shadow_spread_darkens_the_feathered_edge() {
        let shade = |spread: f64| {
            let mut s = rect(1, Rect::new(20.0, 20.0, 80.0, 80.0), Color::WHITE);
            s.effects.push(tracedraw_core::live::Effect::InnerShadow {
                offset: tracedraw_core::geometry::Vec2::ZERO,
                color: Color::BLACK,
                opacity: 1.0,
                blur: 8.0,
                merge: tracedraw_core::live::MergeMode::Normal,
                spread,
            });
            let (doc, page) = doc_with(vec![s]);
            render_page_image(&doc, page, 25.4).unwrap()
        };
        let (plain, spread) = (shade(0.0), shade(0.6));
        let (a, b) = (px(&plain, 24, 50).0, px(&spread, 24, 50).0);
        assert!(b + 20 < a, "spread is darker near the edge: {b} against {a}");
        assert_eq!(px(&spread, 50, 50), (255, 255, 255), "middle stays lit");
        assert_eq!(px(&spread, 15, 50), (255, 255, 255), "outside is untouched");
        // Out of range values are kept in range.
        let wild = shade(f64::NAN);
        assert_eq!(px(&wild, 24, 50), px(&plain, 24, 50));
    }

    /// The layer effects drawn by the renderer (#1141): an outer shadow
    /// and an outer glow beneath, outside the object; an inner glow along
    /// the inside edge; a Gaussian blur softening the edge; fill opacity
    /// fading the object and not its glow.
    #[test]
    fn layer_effects_shadow_glow_and_blur_the_object() {
        use tracedraw_core::geometry::Vec2;
        use tracedraw_core::live::{Effect, MergeMode};
        let with = |e: Vec<Effect>| {
            let mut s = rect(1, Rect::new(30.0, 30.0, 70.0, 70.0), Color::BLACK);
            s.effects = e;
            let (doc, page) = doc_with(vec![s]);
            render_page_image(&doc, page, 25.4).unwrap()
        };
        let plain = with(vec![]);
        assert_eq!(px(&plain, 75, 50), (255, 255, 255));
        // Outer shadow to the right: dark right of the object only.
        let shadow = with(vec![Effect::OuterShadow {
            offset: Vec2::new(8.0, 0.0),
            color: Color::BLACK,
            opacity: 1.0,
            blur: 0.0,
            merge: MergeMode::Normal,
        }]);
        assert!(px(&shadow, 75, 50).0 < 30, "{:?}", px(&shadow, 75, 50));
        assert_eq!(px(&shadow, 25, 50), (255, 255, 255));
        // Outer glow: red all round, fading out.
        let glow = with(vec![Effect::Glow {
            inner: false,
            color: Color::rgb8(255, 0, 0),
            opacity: 1.0,
            blur: 6.0,
            merge: MergeMode::Normal,
        }]);
        for (x, y) in [(72, 50), (27, 50), (50, 72), (50, 27)] {
            let (r, g, b) = px(&glow, x, y);
            assert!(r > 200 && g < 200 && b < 200, "({x},{y}): {:?}", (r, g, b));
        }
        assert_eq!(px(&glow, 50, 50), (0, 0, 0), "the object over its glow");
        assert_eq!(px(&glow, 90, 50), (255, 255, 255), "beyond its reach");
        // Inner glow on a black object: lit inside the edge, dark middle.
        let inner = with(vec![Effect::Glow {
            inner: true,
            color: Color::WHITE,
            opacity: 1.0,
            blur: 3.0,
            merge: MergeMode::Normal,
        }]);
        assert!(px(&inner, 31, 50).0 > 50, "{:?}", px(&inner, 31, 50));
        assert!(px(&inner, 50, 50).0 < 30, "{:?}", px(&inner, 50, 50));
        assert_eq!(px(&inner, 25, 50), (255, 255, 255));
        // Gaussian blur: grey across the edge, still black in the middle.
        let blurred = with(vec![Effect::GaussianBlur { radius: 3.0 }]);
        let edge = px(&blurred, 71, 50).0;
        assert!(edge > 0 && edge < 255, "{edge}");
        assert!(px(&blurred, 50, 50).0 < 10);
        // Fill opacity: the object fades, its glow does not.
        let faded = with(vec![
            Effect::Glow {
                inner: false,
                color: Color::rgb8(255, 0, 0),
                opacity: 1.0,
                blur: 6.0,
                merge: MergeMode::Normal,
            },
            Effect::FillOpacity { opacity: 0.0 },
        ]);
        assert!(px(&faded, 72, 50).0 > 200);
        // The object gone, its glow (solid up to the edge) shows.
        assert_eq!(px(&faded, 50, 50), (255, 0, 0), "the object is gone");
    }

    #[test]
    fn fountain_transparency_fades_the_object() {
        let mut s = rect(1, Rect::new(0.0, 0.0, 100.0, 10.0), Color::rgb8(0, 0, 0));
        s.effects.push(tracedraw_core::live::Effect::Transparency {
            mask: Fill::linear(Color::WHITE, Color::BLACK, 0.0),
            merge: tracedraw_core::live::MergeMode::Normal,
            target: 2,
        });
        let (doc, page) = doc_with(vec![s]);
        let pm = render_page_image(&doc, page, 25.4).unwrap();
        let left = px(&pm, 3, 95).0;
        let right = px(&pm, 96, 95).0;
        assert!(left < 40, "left should be opaque black: {left}");
        assert!(
            right > 215,
            "right should be transparent (white page): {right}"
        );
    }

    /// Each merge mode mixes a source drawn through an opaque mask with
    /// the backdrop as its formula says (source and backdrop in 0..1).
    #[test]
    fn merge_modes_mix_with_the_backdrop_as_their_formulas_say() {
        use tracedraw_core::live::MergeMode as M;
        let cb = [0.6f32, 0.4, 0.2];
        let cs = [0.5f32, 0.8, 0.25];
        let to8 = |v: f32| (v * 255.0).round() as u8;
        let backdrop = rect(
            1,
            Rect::new(0.0, 0.0, 100.0, 100.0),
            Color::rgb8(to8(cb[0]), to8(cb[1]), to8(cb[2])),
        );
        let screen = |b: f32, s: f32| b + s - b * s;
        let hard_light = |b: f32, s: f32| {
            if s <= 0.5 {
                b * 2.0 * s
            } else {
                screen(b, 2.0 * s - 1.0)
            }
        };
        let soft_light = |b: f32, s: f32| {
            if s <= 0.5 {
                b - (1.0 - 2.0 * s) * b * (1.0 - b)
            } else {
                let d = if b <= 0.25 {
                    ((16.0 * b - 12.0) * b + 4.0) * b
                } else {
                    b.sqrt()
                };
                b + (2.0 * s - 1.0) * (d - b)
            }
        };
        let dodge = |b: f32, s: f32| {
            if b <= 0.0 {
                0.0
            } else if s >= 1.0 {
                1.0
            } else {
                (b / (1.0 - s)).min(1.0)
            }
        };
        let burn = |b: f32, s: f32| {
            if b >= 1.0 {
                1.0
            } else if s <= 0.0 {
                0.0
            } else {
                1.0 - ((1.0 - b) / s).min(1.0)
            }
        };
        let lum = 0.2126 * cs[0] + 0.7152 * cs[1] + 0.0722 * cs[2];
        let per_channel =
            |f: &dyn Fn(f32, f32) -> f32| [f(cb[0], cs[0]), f(cb[1], cs[1]), f(cb[2], cs[2])];
        let cases: Vec<(M, [f32; 3])> = vec![
            (M::Screen, per_channel(&screen)),
            (M::Overlay, per_channel(&|b, s| hard_light(s, b))),
            (M::SoftLight, per_channel(&soft_light)),
            (M::HardLight, per_channel(&hard_light)),
            (M::ColorDodge, per_channel(&dodge)),
            (M::ColorBurn, per_channel(&burn)),
            (M::Exclusion, per_channel(&|b, s| b + s - 2.0 * b * s)),
            (M::Texturize, [cb[0] * lum, cb[1] * lum, cb[2] * lum]),
            // Backdrop and source added, less white (#2512).
            (M::Subtract, per_channel(&|b, s| (b + s - 1.0).max(0.0))),
            // Hue 30 degrees and saturation 0.5 of the backdrop at the
            // source's lightness (0.8 + 0.25) / 2.
            (M::Lightness, [0.7625, 0.525, 0.2875]),
            // Nothing shows over an opaque backdrop.
            (M::Behind, cb),
        ];
        for (merge, want) in cases {
            let mut top = rect(
                2,
                Rect::new(20.0, 20.0, 80.0, 80.0),
                Color::rgb8(to8(cs[0]), to8(cs[1]), to8(cs[2])),
            );
            top.effects
                .push(tracedraw_core::live::Effect::Transparency {
                    mask: Fill::Solid(Color::WHITE),
                    merge,
                    target: 2,
                });
            let (doc, page) = doc_with(vec![backdrop.clone(), top]);
            let pm = render_page_image(&doc, page, 25.4).unwrap();
            let got = px(&pm, 50, 50);
            let want = (to8(want[0]), to8(want[1]), to8(want[2]));
            for (g, w) in [got.0, got.1, got.2]
                .into_iter()
                .zip([want.0, want.1, want.2])
            {
                assert!(
                    (g as i32 - w as i32).abs() <= 3,
                    "{merge:?}: got {got:?}, want {want:?}"
                );
            }
            // Outside the source the backdrop is untouched.
            assert_eq!(
                px(&pm, 5, 5),
                (to8(cb[0]), to8(cb[1]), to8(cb[2])),
                "{merge:?}"
            );
        }
    }

    /// Subtract over coloured artwork (#2512): a white source leaves the
    /// artwork as it is and a grey one darkens it, as Multiply does; it
    /// no longer turns the artwork into its negative.
    #[test]
    fn subtract_darkens_the_backdrop_by_the_source() {
        let mut target = Pixmap::new(2, 1).unwrap();
        let mut layer = Pixmap::new(2, 1).unwrap();
        for p in target.pixels_mut() {
            *p = tiny_skia::ColorU8::from_rgba(40, 120, 220, 255).premultiply();
        }
        layer.pixels_mut()[0] = tiny_skia::ColorU8::from_rgba(255, 255, 255, 255).premultiply();
        layer.pixels_mut()[1] = tiny_skia::ColorU8::from_rgba(200, 200, 200, 255).premultiply();
        composite_layer(
            &mut target,
            &layer,
            (0, 0),
            tracedraw_core::live::MergeMode::Subtract,
        );
        assert_eq!(px(&target, 0, 0), (40, 120, 220));
        assert_eq!(px(&target, 1, 0), (0, 65, 165));
    }

    /// Behind shows the source only where the backdrop is clear.
    #[test]
    fn behind_draws_only_where_the_backdrop_is_clear() {
        let mut target = Pixmap::new(4, 1).unwrap();
        let mut layer = Pixmap::new(4, 1).unwrap();
        let red = tiny_skia::ColorU8::from_rgba(255, 0, 0, 255).premultiply();
        let blue = tiny_skia::ColorU8::from_rgba(0, 0, 255, 255).premultiply();
        let half_blue = tiny_skia::ColorU8::from_rgba(0, 0, 255, 128).premultiply();
        target.pixels_mut()[0] = red;
        target.pixels_mut()[2] = half_blue;
        for p in layer.pixels_mut() {
            *p = blue;
        }
        composite_layer(
            &mut target,
            &layer,
            (0, 0),
            tracedraw_core::live::MergeMode::Behind,
        );
        // Over the opaque red: red. Over nothing: the source. Over the half
        // blue: blue filled in to opaque.
        assert_eq!(px(&target, 0, 0), (255, 0, 0));
        assert_eq!(px(&target, 1, 0), (0, 0, 255));
        assert_eq!(target.pixel(2, 0).unwrap().alpha(), 255);
        assert_eq!(px(&target, 2, 0), (0, 0, 255));
    }

    /// Subtract, Texturize and Lightness, drawn pixel by pixel, keep the
    /// alpha compositing of the blend modes: a half-clear source over a
    /// clear backdrop is the source at half alpha, over an opaque one the
    /// mix.
    #[test]
    fn pixel_merges_composite_alpha_like_blend_modes() {
        use tracedraw_core::live::MergeMode as M;
        for merge in [M::Subtract, M::Texturize, M::Lightness] {
            let mut target = Pixmap::new(2, 1).unwrap();
            let mut layer = Pixmap::new(2, 1).unwrap();
            target.pixels_mut()[1] = tiny_skia::ColorU8::from_rgba(200, 100, 50, 255).premultiply();
            for p in layer.pixels_mut() {
                *p = tiny_skia::ColorU8::from_rgba(255, 255, 255, 128).premultiply();
            }
            composite_layer(&mut target, &layer, (0, 0), merge);
            let clear = target.pixel(0, 0).unwrap();
            assert_eq!(clear.alpha(), 128, "{merge:?}");
            assert_eq!(px(&target, 0, 0), (255, 255, 255), "{merge:?}");
            let over = target.pixel(1, 0).unwrap();
            assert_eq!(over.alpha(), 255, "{merge:?}");
            // White at half strength: Subtract and Texturize leave the
            // backdrop (white adds 1 and takes 1 away; its lightness is
            // 1), Lightness lifts it half way to white.
            let got = px(&target, 1, 0);
            let want = match merge {
                M::Subtract | M::Texturize => (200, 100, 50),
                _ => (228, 178, 153),
            };
            for (g, w) in [got.0, got.1, got.2]
                .into_iter()
                .zip([want.0, want.1, want.2])
            {
                assert!(
                    (g as i32 - w as i32).abs() <= 2,
                    "{merge:?}: {got:?} vs {want:?}"
                );
            }
        }
    }

    #[test]
    fn shaped_transparency_follows_the_object() {
        // White at the edge (opaque), black inside (clear): a black disc
        // fades towards its middle, the same all round.
        let id = tracedraw_core::ShapeId(1);
        let mut s = Shape::new(
            id,
            ShapeKind::Ellipse {
                rect: Rect::new(0.0, 0.0, 60.0, 60.0),
                arc: None,
            },
        );
        s.fill = Fill::Solid(Color::BLACK);
        s.stroke = None;
        s.effects.push(tracedraw_core::live::Effect::Transparency {
            mask: Fill::Fountain(Fountain::two(
                FountainKind::Shaped,
                Color::WHITE,
                Color::BLACK,
                0.0,
            )),
            merge: tracedraw_core::live::MergeMode::Normal,
            target: 2,
        });
        let (doc, page) = doc_with(vec![s]);
        let pm = render_page_image(&doc, page, 25.4).unwrap();
        let (centre, east, north) = (px(&pm, 30, 70).0, px(&pm, 52, 70).0, px(&pm, 30, 48).0);
        assert!(centre > 215, "the middle is clear: {centre}");
        assert!(
            east < 120 && (east as i32 - north as i32).abs() < 12,
            "{east} {north}"
        );
    }

    #[test]
    fn contour_effect_draws_outside_steps() {
        let mut s = rect(1, Rect::new(40.0, 40.0, 60.0, 60.0), Color::rgb8(255, 0, 0));
        s.effects.push(tracedraw_core::live::Effect::Contour {
            steps: 2,
            offset: 5.0,
            outside: true,
            to_center: false,
            both: false,
            fill_to: Color::rgb8(0, 0, 255),
            outline_to: None,
            fill_end_to: None,
            color_blend: 0,
            corners: None,
            accel_objects: 0.0,
            accel_colors: 0.0,
        });
        let (doc, page) = doc_with(vec![s]);
        let pm = render_page_image(&doc, page, 25.4).unwrap();
        // y is flipped in the image: page y 50 is image row 50 either way (100 tall).
        assert_eq!(px(&pm, 50, 50), (255, 0, 0));
        let (r, _, b) = px(&pm, 32, 50);
        assert!(b > r, "outer contour step should be bluish");
        assert_eq!(px(&pm, 5, 5), (255, 255, 255));
    }

    /// Smooth colour carries a bright middle node's colour further into
    /// its cells than the bilinear mix does.
    #[test]
    fn smooth_mesh_colour_flows_through_the_nodes() {
        let render = |smooth: bool| {
            let mut s = rect(1, Rect::new(0.0, 0.0, 100.0, 100.0), Color::WHITE);
            let mut m =
                tracedraw_core::Mesh::new(Rect::new(0.0, 0.0, 100.0, 100.0), 2, 2, Color::BLACK);
            m.nodes[4].color = Color::WHITE; // the middle node
            m.smooth = smooth;
            s.fill = Fill::Mesh(m);
            let (doc, page) = doc_with(vec![s]);
            render_page_image(&doc, page, 25.4).unwrap()
        };
        let (flat, smooth) = (render(false), render(true));
        let (f, _, _) = px(&flat, 30, 50);
        let (g, _, _) = px(&smooth, 30, 50);
        assert!(g > f + 8, "smooth {g} should be brighter than bilinear {f}");
        // Next to the middle node it is nearly white either way.
        assert!(px(&smooth, 50, 50).0 > 200 && px(&flat, 50, 50).0 > 200);
    }

    /// A mesh whose grid line bends (the nodes carry handles) fills the
    /// bulge, and a pan reuses the picture worked out before (#896).
    #[test]
    fn curved_mesh_cells_follow_their_handles() {
        use tracedraw_core::MeshHandle;
        let area = Rect::new(0.0, 0.0, 100.0, 100.0);
        let mut m = tracedraw_core::Mesh::new(Rect::new(0.0, 0.0, 50.0, 100.0), 1, 1, Color::BLACK);
        // The right edge bows out to x = 80 in the middle.
        m.nodes[1].set_handle(MeshHandle::Down, Point::new(90.0, 33.0));
        m.nodes[3].set_handle(MeshHandle::Up, Point::new(90.0, 67.0));
        let mut s = rect(1, area, Color::WHITE);
        s.kind = ShapeKind::Path {
            path: {
                let mut p = BezPath::new();
                p.move_to((0.0, 0.0));
                p.line_to((50.0, 0.0));
                p.curve_to((90.0, 33.0), (90.0, 67.0), (50.0, 100.0));
                p.line_to((0.0, 100.0));
                p.close_path();
                p
            },
            closed: true,
        };
        s.fill = Fill::Mesh(m.clone());
        let (doc, page) = doc_with(vec![s.clone()]);
        let pm = render_page_image(&doc, page, 25.4).unwrap();
        // Inside the bulge the mesh is black; the straight grid would
        // leave it to the path's white.
        let (r, g, b) = px(&pm, 70, 50);
        assert!(
            r < 40 && g < 40 && b < 40,
            "bulge should be black: {r} {g} {b}"
        );
        let (r, _, _) = px(&pm, 95, 50);
        assert!(r > 200, "outside the bulge is white: {r}");
        let p = m.patch_point(0, 0, 1.0, 0.5).unwrap();
        assert!(
            (p.x - 80.0).abs() < 1e-6 && (p.y - 50.0).abs() < 1e-6,
            "{p:?}"
        );
        // The same mesh moved: the picture is found in the cache.
        let mut moved = s.clone();
        moved.transform = tracedraw_core::geometry::Affine::translate((10.0, 0.0));
        let (doc2, page2) = doc_with(vec![moved]);
        // (Other tests share the cache: only this mesh's pictures count,
        // told by their size.)
        let mine = || {
            procedural_cache_keys()
                .into_iter()
                .filter(|k| k.starts_with("mesh|") && k.ends_with("|95,105,1"))
                .count()
        };
        let n = mine();
        assert!(n >= 1, "{:?}", procedural_cache_keys());
        let pm2 = render_page_image(&doc2, page2, 25.4).unwrap();
        assert_eq!(mine(), n);
        let (r, _, _) = px(&pm2, 80, 50);
        assert!(r < 40, "moved bulge should be black: {r}");
    }

    /// A mesh node's own transparency (#2789, #2154): the colour fades
    /// to see-through at the nodes whose alpha is 0 and stays solid at
    /// the opaque ones, across the patch.
    #[test]
    fn mesh_nodes_fade_by_their_own_transparency() {
        let mut s = rect(1, Rect::new(0.0, 0.0, 100.0, 100.0), Color::BLACK);
        let mut m =
            tracedraw_core::Mesh::new(Rect::new(0.0, 0.0, 100.0, 100.0), 1, 1, Color::BLACK);
        // The left column of nodes see-through, the right one solid.
        m.nodes[0].alpha = 0.0;
        m.nodes[2].alpha = 0.0;
        s.fill = Fill::Mesh(m);
        let (doc, page) = doc_with(vec![s]);
        let pm = render_page_image(&doc, page, 25.4).unwrap();
        let (left, _, _) = px(&pm, 3, 50);
        let (mid, _, _) = px(&pm, 50, 50);
        let (right, _, _) = px(&pm, 97, 50);
        assert!(left > 230, "left nearly clear: {left}");
        assert!(right < 25, "right solid: {right}");
        assert!((90..170).contains(&mid), "half way: {mid}");
    }

    /// A mesh over part of its object leaves the rest of the path clear
    /// (nothing is drawn outside the grid), also on screen where a large
    /// picture is worked out on a coarser grid (steps 2 and 3).
    #[test]
    fn nothing_shows_outside_a_mesh_grid() {
        let mut s = rect(1, Rect::new(0.0, 0.0, 100.0, 100.0), Color::WHITE);
        s.fill = Fill::Mesh(tracedraw_core::Mesh::new(
            Rect::new(0.0, 0.0, 50.0, 100.0),
            2,
            2,
            Color::BLACK,
        ));
        let (doc, page) = doc_with(vec![s]);
        let pm = render_page_image(&doc, page, 25.4).unwrap();
        assert!(px(&pm, 25, 50).0 < 40);
        for x in [55, 75, 95] {
            let (r, g, b) = px(&pm, x, 50);
            assert!(r > 250 && g > 250 && b > 250, "{x}: {r} {g} {b}");
        }
        // A page of 200 x 150 mm with the grid over 20..100 x 20..130 mm
        // of a path over the whole page: at 8 px/mm the picture is 640 x
        // 880 px (step 2), at 14 px/mm 1120 x 1540 px (step 3).
        for zoom in [8.0, 14.0] {
            let mut s = rect(1, Rect::new(0.0, 0.0, 200.0, 150.0), Color::WHITE);
            s.fill = Fill::Mesh(tracedraw_core::Mesh::new(
                Rect::new(20.0, 20.0, 100.0, 130.0),
                3,
                3,
                Color::BLACK,
            ));
            let (doc, page) = doc_with(vec![s]);
            let (w, h) = ((200.0 * zoom) as u32, (150.0 * zoom) as u32);
            let opts = RenderOptions {
                width: w,
                height: h,
                view: ViewTransform {
                    zoom,
                    origin_x: 0.0,
                    origin_y: h as f64,
                    stretch_y: 1.0,
                },
                draft: true,
                ..Default::default()
            };
            let pm = render_page(&doc, page, &opts).unwrap();
            // (No page is painted here: outside the grid the pixels stay
            // clear.)
            let at = |x_mm: f64, y_mm: f64| {
                pm.pixel((x_mm * zoom) as u32, (h as f64 - y_mm * zoom) as u32)
                    .unwrap()
                    .demultiply()
            };
            let inside = at(60.0, 75.0);
            assert!(
                inside.alpha() == 255 && inside.red() < 40,
                "zoom {zoom}: inside the grid {inside:?}"
            );
            for (x, y) in [
                (10.0, 75.0),
                (150.0, 75.0),
                (60.0, 10.0),
                (60.0, 140.0),
                (3.0, 3.0),
            ] {
                let c = at(x, y);
                assert!(
                    c.alpha() == 0,
                    "zoom {zoom}: ({x}, {y}) mm outside the grid is {c:?}"
                );
            }
        }
    }

    /// A large render is drawn in bands on several threads: the bands
    /// share one mesh picture (worked out once, one cache entry), rather
    /// than each working it out.
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn band_threads_share_one_mesh_picture() {
        let mut s = rect(1, Rect::new(0.0, 0.0, 180.0, 180.0), Color::WHITE);
        s.fill = Fill::Mesh(tracedraw_core::Mesh::new(
            Rect::new(0.0, 0.0, 180.0, 180.0),
            4,
            4,
            Color::rgb8(10, 20, 30),
        ));
        let (doc, page) = doc_with(vec![s]);
        let (w, h) = (1300u32, 1300u32);
        let opts = RenderOptions {
            width: w,
            height: h,
            view: ViewTransform {
                zoom: 7.0,
                origin_x: 20.0,
                origin_y: 1280.0,
                stretch_y: 1.0,
            },
            ..Default::default()
        };
        let before: Vec<String> = procedural_cache_keys();
        let pm = render_page(&doc, page, &opts).unwrap();
        assert_eq!(px(&pm, 650, 650), (10, 20, 30));
        // (Other tests share the cache: this mesh's pictures are told by
        // their size.)
        let new: Vec<String> = procedural_cache_keys()
            .into_iter()
            .filter(|k| {
                k.starts_with("mesh|") && k.ends_with("|1265,1265,1") && !before.contains(k)
            })
            .collect();
        assert_eq!(new.len(), 1, "{new:?}");
        let mut sorted = procedural_cache_keys();
        let n = sorted.len();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), n, "a key kept twice");
    }

    /// A 30 x 30 mesh zoomed until each cell is 670 pixels across costs
    /// only what shows on a 1920 x 1080 picture: cells and pieces off the
    /// picture are left out and the triangles come in bounded batches,
    /// where the whole mesh would be some 50 million triangles (and
    /// gigabytes) at once.
    #[test]
    fn a_zoomed_mesh_costs_only_what_shows() {
        let m = tracedraw_core::Mesh::new(Rect::new(0.0, 0.0, 200.0, 200.0), 30, 30, Color::BLACK);
        let (w, h) = (1920usize, 1080usize);
        let (mut total, mut biggest, mut batches) = (0usize, 0usize, 0usize);
        mesh_triangles(&m, Affine::scale(100.0), w, h, |t| {
            total += t.len();
            biggest = biggest.max(t.len());
            batches += 1;
        });
        // About two triangles per 16 pixels of the picture.
        assert!(total > 100_000 && total < 600_000, "{total}");
        assert!(biggest <= MESH_TRIANGLE_BATCH + 2, "{biggest}");
        assert!(batches >= 2);
        // Off the picture entirely: nothing.
        let mut none = 0;
        mesh_triangles(
            &m,
            Affine::translate((-30_000.0, 0.0)) * Affine::scale(100.0),
            w,
            h,
            |t| none += t.len(),
        );
        assert_eq!(none, 0);
        // The same mesh drawn on a page: the picture is bounded by the
        // canvas and the cells on it are painted.
        let mut s = rect(1, Rect::new(0.0, 0.0, 200.0, 200.0), Color::WHITE);
        s.fill = Fill::Mesh(m);
        let (doc, page) = doc_with(vec![s]);
        let opts = RenderOptions {
            width: w as u32,
            height: h as u32,
            view: ViewTransform {
                zoom: 100.0,
                origin_x: 0.0,
                origin_y: 1080.0,
                stretch_y: 1.0,
            },
            draft: true,
            ..Default::default()
        };
        let pm = render_page(&doc, page, &opts).unwrap();
        assert!(px(&pm, 960, 540).0 < 40);
    }

    /// Peak memory and time of the zoomed 30 x 30 and 10 x 10 meshes
    /// (`cargo test --release -p tracedraw-render -- --ignored mesh_memory --nocapture`).
    #[test]
    #[ignore]
    fn mesh_memory_at_high_zoom() {
        let peak = || {
            std::fs::read_to_string("/proc/self/status")
                .ok()
                .and_then(|s| {
                    s.lines()
                        .find(|l| l.starts_with("VmHWM"))
                        .map(|l| l.trim().to_string())
                })
                .unwrap_or_default()
        };
        for n in [10u32, 30] {
            let m =
                tracedraw_core::Mesh::new(Rect::new(0.0, 0.0, 200.0, 200.0), n, n, Color::BLACK);
            let mut s = rect(1, Rect::new(0.0, 0.0, 200.0, 200.0), Color::WHITE);
            s.fill = Fill::Mesh(m);
            let (doc, page) = doc_with(vec![s]);
            let opts = RenderOptions {
                width: 1920,
                height: 1080,
                view: ViewTransform {
                    zoom: 100.0,
                    origin_x: 0.0,
                    origin_y: 1080.0,
                    stretch_y: 1.0,
                },
                draft: true,
                ..Default::default()
            };
            let t = std::time::Instant::now();
            let _ = render_page(&doc, page, &opts).unwrap();
            eprintln!("{n}x{n}: {:?}, peak {}", t.elapsed(), peak());
        }
    }

    #[test]
    fn mesh_fill_blends_corner_colours() {
        let mut s = rect(1, Rect::new(0.0, 0.0, 100.0, 100.0), Color::WHITE);
        let mut m =
            tracedraw_core::Mesh::new(Rect::new(0.0, 0.0, 100.0, 100.0), 1, 1, Color::WHITE);
        m.nodes[0].color = Color::rgb8(255, 0, 0); // bottom-left
        m.nodes[1].color = Color::rgb8(0, 0, 255); // bottom-right
        m.nodes[2].color = Color::rgb8(255, 0, 0);
        m.nodes[3].color = Color::rgb8(0, 0, 255);
        s.fill = Fill::Mesh(m);
        let (doc, page) = doc_with(vec![s]);
        let pm = render_page_image(&doc, page, 25.4).unwrap();
        let (r, _, b) = px(&pm, 5, 50);
        assert!(r > 200 && b < 60, "left should be red: {r} {b}");
        let (r, _, b) = px(&pm, 95, 50);
        assert!(b > 200 && r < 60, "right should be blue: {r} {b}");
        let (r, _, b) = px(&pm, 50, 50);
        assert!(
            (r as i32 - b as i32).abs() < 40,
            "middle should be purple: {r} {b}"
        );
    }

    /// A fill whose nodes were placed freely runs from its start to its
    /// end node, held at the end colours beyond them; an elliptical one
    /// is centred on its start (#330).
    #[test]
    fn node_opacity_shows_through_the_gradient() {
        let b = Rect::new(0.0, 0.0, 100.0, 10.0);
        let mut f = Fountain::two(FountainKind::Linear, Color::BLACK, Color::BLACK, 0.0);
        f.stops[1].opacity = 0.0;
        for kind in [FountainKind::Linear, FountainKind::Square] {
            f.kind = kind;
            let pm = render_fill_image(&Fill::Fountain(f.clone()), b, 25.4).unwrap();
            let a = |x: u32| pm.pixel(x, 5).map(|p| p.alpha()).unwrap_or(0);
            if kind == FountainKind::Linear {
                assert!(a(2) > 240, "opaque start: {}", a(2));
                assert!(a(97) < 15, "clear end: {}", a(97));
            } else {
                // Square: opaque at the centre, clear at the corners.
                assert!(a(50) > a(1), "{} {}", a(50), a(1));
            }
        }
    }

    /// A dithered gradient differs from the plain one by at most two
    /// levels a channel, in some pixels, the same every time.
    #[test]
    fn a_dithered_gradient_adds_faint_noise() {
        let b = Rect::new(0.0, 0.0, 100.0, 10.0);
        let mut f = Fountain::two(
            FountainKind::Linear,
            Color::rgb8(100, 100, 100),
            Color::rgb8(110, 110, 110),
            0.0,
        );
        let plain = render_fill_image(&Fill::Fountain(f.clone()), b, 25.4).unwrap();
        f.dither = true;
        let dithered = render_fill_image(&Fill::Fountain(f.clone()), b, 25.4).unwrap();
        let again = render_fill_image(&Fill::Fountain(f), b, 25.4).unwrap();
        assert_eq!(dithered.data(), again.data());
        let diffs: Vec<i32> = plain
            .data()
            .iter()
            .zip(dithered.data())
            .map(|(a, b)| (i32::from(*a) - i32::from(*b)).abs())
            .collect();
        assert!(diffs.iter().all(|d| *d <= 2), "{:?}", diffs.iter().max());
        assert!(diffs.iter().filter(|d| **d > 0).count() > 100);
    }

    #[test]
    fn free_fill_nodes_place_the_gradient() {
        use tracedraw_core::style::FountainHandles;
        let b = Rect::new(0.0, 0.0, 100.0, 10.0);
        let rel = |x: f64, y: f64| Fountain::page_to_rel(b, Point::new(x, y));
        let mut f = Fountain::two(FountainKind::Linear, Color::BLACK, Color::WHITE, 0.0);
        f.set_handles(
            b,
            FountainHandles {
                start: rel(20.0, 5.0),
                end: rel(60.0, 5.0),
                skew: rel(20.0, 10.0),
            },
        );
        let pm = render_fill_image(&Fill::Fountain(f.clone()), b, 25.4).unwrap();
        let (r, _, _) = px(&pm, 10, 5);
        assert!(r < 10, "before the start: {r}");
        let (r, _, _) = px(&pm, 40, 5);
        assert!((110..150).contains(&r), "half way: {r}");
        let (r, _, _) = px(&pm, 80, 5);
        assert!(r > 245, "beyond the end: {r}");
        // Elliptical, centred at x = 70.
        f.kind = FountainKind::Radial;
        f.set_handles(
            b,
            FountainHandles {
                start: rel(70.0, 5.0),
                end: rel(90.0, 5.0),
                skew: rel(70.0, 25.0),
            },
        );
        let pm = render_fill_image(&Fill::Fountain(f.clone()), b, 25.4).unwrap();
        let (r, _, _) = px(&pm, 70, 5);
        assert!(r < 20, "the centre: {r}");
        let (r, _, _) = px(&pm, 20, 5);
        assert!(r > 245, "far out: {r}");
        // Square, procedural: centre dark, beyond the half axis light.
        f.kind = FountainKind::Square;
        let pm = render_fill_image(&Fill::Fountain(f), b, 25.4).unwrap();
        let (r, _, _) = px(&pm, 70, 5);
        assert!(r < 30, "square centre: {r}");
        let (r, _, _) = px(&pm, 5, 5);
        assert!(r > 240, "square outside: {r}");
    }

    #[test]
    fn linear_fountain_three_stops_passes_through_middle_colour() {
        let fill = Fill::Fountain(Fountain {
            kind: FountainKind::Linear,
            stops: vec![
                Stop::new(0.0, Color::rgb8(0, 0, 0)),
                Stop::new(0.5, Color::rgb8(255, 0, 0)),
                Stop::new(1.0, Color::rgb8(255, 255, 255)),
            ],
            angle: 0.0,
            offset: Point::ZERO,
            edge_pad: 0.0,
            ..Default::default()
        });
        let pm = render_fill_image(&fill, Rect::new(0.0, 0.0, 100.0, 10.0), 25.4).unwrap();
        assert_eq!(pm.width(), 100);
        let (r, g, b) = px(&pm, 50, 5);
        assert!(r > 240 && g < 20 && b < 20, "middle {r} {g} {b}");
        let (r, _, _) = px(&pm, 2, 5);
        assert!(r < 20);
        let (_, g, _) = px(&pm, 97, 5);
        assert!(g > 230);
    }

    #[test]
    fn conical_fountain_differs_around_centre() {
        let fill = Fill::Fountain(Fountain::two(
            FountainKind::Conical,
            Color::rgb8(0, 0, 255),
            Color::rgb8(255, 255, 0),
            0.0,
        ));
        let pm = render_fill_image(&fill, Rect::new(0.0, 0.0, 100.0, 100.0), 25.4).unwrap();
        let a = px(&pm, 90, 50);
        let b = px(&pm, 10, 50);
        assert_ne!(a, b);
    }

    #[test]
    fn checker_pattern_alternates() {
        let fill = Fill::Pattern(Pattern::TwoColor {
            tile: PatternTile::Checker,
            front: Color::rgb8(0, 0, 0),
            back: Color::rgb8(255, 255, 255),
            size_mm: 20.0,
            placement: Default::default(),
        });
        let pm = render_fill_image(&fill, Rect::new(0.0, 0.0, 40.0, 40.0), 25.4).unwrap();
        let a = px(&pm, 5, 5);
        let b = px(&pm, 15, 5);
        assert_ne!(a.0, b.0, "checker cells should differ");
        assert!(a.0 == 0 || a.0 == 255);
    }

    #[test]
    fn vector_pattern_tiles_the_shape() {
        // A 10 mm tile with a red square in its lower-left quarter.
        let tile_square = rect(7, Rect::new(0.0, 0.0, 5.0, 5.0), Color::rgb8(255, 0, 0));
        let mut s = rect(1, Rect::new(0.0, 0.0, 20.0, 20.0), Color::WHITE);
        s.fill = Fill::Pattern(Pattern::Vector {
            shapes: vec![tile_square],
            tile: tracedraw_core::geometry::Size::new(10.0, 10.0),
            placement: Default::default(),
        });
        let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(20.0, 20.0));
        let page = doc.pages[0].id;
        let layer = doc.pages[0].layers[0].id;
        doc.layer_mut(layer).unwrap().shapes.push(s);
        let pm = render_page_image(&doc, page, 25.4).unwrap(); // 1 px per mm
        assert_eq!((pm.width(), pm.height()), (20, 20));
        // Tiles start at the top-left corner; the red square sits in the
        // lower-left quarter of each tile (image rows 5..10 and 15..20).
        for (x, y) in [(2, 7), (12, 7), (2, 17), (12, 17)] {
            assert_eq!(px(&pm, x, y), (255, 0, 0), "({x}, {y}) must be red");
        }
        for (x, y) in [(2, 2), (7, 7), (7, 2), (12, 12), (17, 17), (17, 7)] {
            assert_eq!(
                px(&pm, x, y),
                (255, 255, 255),
                "({x}, {y}) must be page white"
            );
        }
    }

    fn checker(size_mm: f64, placement: tracedraw_core::style::PatternPlacement) -> Fill {
        Fill::Pattern(Pattern::TwoColor {
            tile: PatternTile::Checker,
            front: Color::rgb8(0, 0, 0),
            back: Color::rgb8(255, 255, 255),
            size_mm,
            placement,
        })
    }

    /// Mirror tiles horizontally: the second column of tiles shows the
    /// tile flipped, so a checker's black bottom-left cell sits at the
    /// bottom right of the second tile.
    #[test]
    fn mirrored_checker_flips_the_second_column() {
        use tracedraw_core::style::PatternPlacement;
        let bounds = Rect::new(0.0, 0.0, 20.0, 10.0);
        // 1 px per mm: the first tile covers columns 0..10, the second
        // 10..20; the tile's bottom-left cell is image rows 5..10.
        let plain =
            render_fill_image(&checker(10.0, PatternPlacement::default()), bounds, 25.4).unwrap();
        assert_eq!(px(&plain, 2, 7), (0, 0, 0));
        assert_eq!(px(&plain, 12, 7), (0, 0, 0), "the same tile again");
        assert_eq!(px(&plain, 17, 7), (255, 255, 255));
        let mirrored = render_fill_image(
            &checker(
                10.0,
                PatternPlacement {
                    mirror_horizontal: true,
                    ..PatternPlacement::default()
                },
            ),
            bounds,
            25.4,
        )
        .unwrap();
        assert_eq!(px(&mirrored, 2, 7), (0, 0, 0), "the first tile as before");
        assert_eq!(px(&mirrored, 12, 7), (255, 255, 255), "flipped");
        assert_eq!(px(&mirrored, 17, 7), (0, 0, 0), "flipped");
        // Vertically: the second row of tiles is turned upside down.
        let upside = render_fill_image(
            &checker(
                10.0,
                PatternPlacement {
                    mirror_vertical: true,
                    ..PatternPlacement::default()
                },
            ),
            Rect::new(0.0, 0.0, 10.0, 20.0),
            25.4,
        )
        .unwrap();
        assert_eq!(px(&upside, 2, 17), (0, 0, 0), "the first row as before");
        assert_eq!(px(&upside, 2, 7), (255, 255, 255), "the second row flipped");
        assert_eq!(px(&upside, 2, 2), (0, 0, 0));
    }

    /// A row offset of 50 percent moves every other row half a tile along;
    /// a column offset does the same to every other column.
    #[test]
    fn row_offset_shifts_every_other_row_by_half_a_tile() {
        use tracedraw_core::style::{PatternPlacement, TileShift};
        let bounds = Rect::new(0.0, 0.0, 20.0, 20.0);
        let shifted = render_fill_image(
            &checker(
                10.0,
                PatternPlacement {
                    shift_percent: 50.0,
                    ..PatternPlacement::default()
                },
            ),
            bounds,
            25.4,
        )
        .unwrap();
        // The bottom row (image rows 10..20) is not shifted: black at the
        // bottom left of each tile.
        assert_eq!(px(&shifted, 2, 17), (0, 0, 0));
        assert_eq!(px(&shifted, 7, 17), (255, 255, 255));
        // The row above starts 5 mm along: its tiles' black cells sit at
        // x 5..10 and 15..20 (image rows 5..10 are the tiles' lower halves).
        assert_eq!(px(&shifted, 7, 7), (0, 0, 0));
        assert_eq!(px(&shifted, 2, 7), (255, 255, 255));
        assert_eq!(px(&shifted, 17, 7), (0, 0, 0));
        let columns = render_fill_image(
            &checker(
                10.0,
                PatternPlacement {
                    shift_percent: 50.0,
                    shift: TileShift::Column,
                    ..PatternPlacement::default()
                },
            ),
            bounds,
            25.4,
        )
        .unwrap();
        // The first column (x 0..10) stays; the second is 5 mm up, so its
        // first tile's black cell covers y 5..10 (image rows 10..15).
        assert_eq!(px(&columns, 2, 17), (0, 0, 0));
        assert_eq!(px(&columns, 12, 12), (0, 0, 0));
        assert_eq!(px(&columns, 12, 17), (255, 255, 255));
    }

    /// Hexagonal tiles laid with half a tile of row offset and a quarter
    /// of row overlap cover the page with no gaps; without the overlap
    /// the corners between the rows stay empty.
    #[test]
    fn overlapping_rows_interlock_hexagons() {
        use tracedraw_core::style::PatternPlacement;
        let r = 10.0;
        let (w, h) = (3f64.sqrt() * r, 2.0 * r);
        let mut hex = tracedraw_core::geometry::BezPath::new();
        for k in 0..6 {
            let a = std::f64::consts::FRAC_PI_2 + f64::from(k) * std::f64::consts::FRAC_PI_3;
            let q = tracedraw_core::geometry::Point::new(w / 2.0 + r * a.cos(), h / 2.0 + r * a.sin());
            if k == 0 {
                hex.move_to(q);
            } else {
                hex.line_to(q);
            }
        }
        hex.close_path();
        let mut cell = tracedraw_core::Shape::new(
            tracedraw_core::ShapeId(7),
            tracedraw_core::ShapeKind::Path {
                path: hex,
                closed: true,
            },
        );
        cell.fill = Fill::Solid(Color::rgb8(255, 0, 0));
        cell.stroke = None;
        let red_share = |overlap: f64| {
            let mut s = rect(1, Rect::new(0.0, 0.0, 60.0, 60.0), Color::WHITE);
            s.fill = Fill::Pattern(Pattern::Vector {
                shapes: vec![cell.clone()],
                tile: tracedraw_core::geometry::Size::new(w, h),
                placement: PatternPlacement {
                    shift_percent: 50.0,
                    overlap_percent: overlap,
                    ..PatternPlacement::default()
                },
            });
            let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(60.0, 60.0));
            let page = doc.pages[0].id;
            let layer = doc.pages[0].layers[0].id;
            doc.layer_mut(layer).unwrap().shapes.push(s);
            let pm = render_page_image(&doc, page, 25.4 * 4.0).unwrap();
            let red = pm
                .pixels()
                .iter()
                .filter(|p| p.red() > 200 && p.green() < 60)
                .count();
            red as f64 / pm.pixels().len() as f64
        };
        let gaps = red_share(0.0);
        let hexes = red_share(25.0);
        assert!(gaps < 0.8, "{gaps}");
        assert!(hexes > 0.97, "{hexes}");
    }

    /// A red and a blue square overlapping in a group: drawn as one, the
    /// group's transparency goes on the whole and the red does not show
    /// through the blue; with a knockout, a see-through blue shows the
    /// page beneath the group, not the red.
    #[test]
    fn group_looks_draw_the_group_as_one_and_knock_out() {
        use tracedraw_core::document::GroupLook;
        let page_with = |member_opacity: f64, group_opacity: f64, look: Option<GroupLook>| {
            let mut red = rect(2, Rect::new(0.0, 0.0, 20.0, 10.0), Color::rgb8(255, 0, 0));
            let mut blue = rect(3, Rect::new(10.0, 0.0, 30.0, 10.0), Color::rgb8(0, 0, 255));
            red.opacity = member_opacity;
            blue.opacity = member_opacity;
            let mut g = tracedraw_core::Shape::new(
                tracedraw_core::ShapeId(1),
                tracedraw_core::ShapeKind::Group {
                    children: vec![red, blue],
                },
            );
            g.opacity = group_opacity;
            g.group_look = look;
            let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(30.0, 10.0));
            let page = doc.pages[0].id;
            let layer = doc.pages[0].layers[0].id;
            doc.layer_mut(layer).unwrap().shapes.push(g);
            render_page_image(&doc, page, 25.4).unwrap()
        };
        let as_one = Some(GroupLook {
            as_one: true,
            knockout: false,
        });
        let knockout = Some(GroupLook {
            as_one: false,
            knockout: true,
        });
        // A plain half-transparent group: the red shows through the blue.
        let plain = px(&page_with(1.0, 0.5, None), 15, 5);
        assert!(plain.0 > 40 && plain.2 > 100, "{plain:?}");
        // As one: the blue alone at half strength over the white page.
        let one = px(&page_with(1.0, 0.5, as_one), 15, 5);
        assert!(one.0 > 120 && one.1 > 120 && one.2 > 250, "{one:?}");
        assert!((i32::from(one.0) - i32::from(one.1)).abs() < 4, "{one:?}");
        // The left part is the red alone at half strength either way.
        assert_eq!(px(&page_with(1.0, 0.5, as_one), 5, 5), px(&plain_page(), 5, 5));
        // Knockout: half-transparent members, the blue hides the red.
        let ko = px(&page_with(0.5, 1.0, knockout), 15, 5);
        assert!((i32::from(ko.0) - i32::from(ko.1)).abs() < 4 && ko.2 > 250, "{ko:?}");
        let mixed = px(&page_with(0.5, 1.0, None), 15, 5);
        assert!(mixed.0 > mixed.1 + 30, "the red shows without: {mixed:?}");

        fn plain_page() -> Pixmap {
            let mut red = rect(2, Rect::new(0.0, 0.0, 20.0, 10.0), Color::rgb8(255, 0, 0));
            red.opacity = 0.5;
            let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(30.0, 10.0));
            let page = doc.pages[0].id;
            let layer = doc.pages[0].layers[0].id;
            doc.layer_mut(layer).unwrap().shapes.push(red);
            render_page_image(&doc, page, 25.4).unwrap()
        }
    }

    /// The offset moves the tiles, the rotation turns them and the tile
    /// size scales them, the tiles still one pattern draw each.
    #[test]
    fn placement_offsets_turns_and_sizes_the_tiles() {
        use tracedraw_core::style::PatternPlacement;
        let bounds = Rect::new(0.0, 0.0, 20.0, 20.0);
        let moved = render_fill_image(
            &checker(
                10.0,
                PatternPlacement {
                    offset_x: 5.0,
                    ..PatternPlacement::default()
                },
            ),
            bounds,
            25.4,
        )
        .unwrap();
        // The tile edge is now at x 5: black at x 5..10 in the bottom cell.
        assert_eq!(px(&moved, 7, 17), (0, 0, 0));
        assert_eq!(px(&moved, 2, 17), (255, 255, 255));
        let sized = render_fill_image(
            &checker(
                10.0,
                PatternPlacement {
                    size: Some(tracedraw_core::geometry::Size::new(20.0, 20.0)),
                    ..PatternPlacement::default()
                },
            ),
            bounds,
            25.4,
        )
        .unwrap();
        // One 20 mm tile: its bottom-left cell is the whole bottom-left
        // quarter.
        assert_eq!(px(&sized, 7, 17), (0, 0, 0));
        assert_eq!(px(&sized, 7, 7), (255, 255, 255));
        let turned = render_fill_image(
            &checker(
                10.0,
                PatternPlacement {
                    rotation: 45.0,
                    ..PatternPlacement::default()
                },
            ),
            bounds,
            25.4,
        )
        .unwrap();
        // Turned by 45 degrees counter-clockwise around the origin, the
        // tile's bottom edge runs along y = x: the page point (0.5, 3.5)
        // lies 2.8 mm along the tile and 2.1 mm up it, in the black cell,
        // and (3.5, 0.5) lies 2.1 mm below the edge, in the white cell
        // of the tile row beneath.
        assert_eq!(px(&turned, 0, 16), (0, 0, 0));
        assert_eq!(px(&turned, 3, 19), (255, 255, 255));
        // The skew leans the columns: at the top of the tile the edge has
        // moved by tan(45) * 10 mm.
        let skewed = render_fill_image(
            &checker(
                10.0,
                PatternPlacement {
                    skew: 45.0,
                    ..PatternPlacement::default()
                },
            ),
            bounds,
            25.4,
        )
        .unwrap();
        // Page (3.5, 1.5) is 2 mm into the first tile, black; (13.5,
        // 11.5) is 2 mm into the tile above, which starts 10 mm right;
        // (8.5, 11.5) is 7 mm into the tile left of that, white.
        assert_eq!(px(&skewed, 3, 18), (0, 0, 0), "the bottom is as before");
        assert_eq!(px(&skewed, 13, 8), (0, 0, 0), "a tile up, 10 mm right");
        assert_eq!(px(&skewed, 8, 8), (255, 255, 255));
    }

    /// Transform with object: moving the object through the engine moves
    /// its tiles along; without it they stay anchored to the page.
    #[test]
    fn transform_with_object_moves_the_tiles_with_the_object() {
        use tracedraw_core::style::PatternPlacement;
        use tracedraw_core::{Command, Engine};
        let make = |with_object: bool| {
            let tile_square = rect(7, Rect::new(0.0, 0.0, 5.0, 5.0), Color::rgb8(255, 0, 0));
            let mut s = rect(1, Rect::new(0.0, 0.0, 20.0, 20.0), Color::WHITE);
            s.fill = Fill::Pattern(Pattern::Vector {
                shapes: vec![tile_square],
                tile: tracedraw_core::geometry::Size::new(10.0, 10.0),
                placement: PatternPlacement {
                    transform_with_object: with_object,
                    ..PatternPlacement::default()
                },
            });
            let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(30.0, 20.0));
            let page = doc.pages[0].id;
            let layer = doc.pages[0].layers[0].id;
            doc.layer_mut(layer).unwrap().shapes.push(s);
            let mut e = Engine::new(doc);
            e.run(&Command::TransformShapes {
                shapes: vec![tracedraw_core::ShapeId(1)],
                transform: Affine::translate((5.0, 0.0)),
            })
            .unwrap();
            render_page_image(e.document(), page, 25.4).unwrap()
        };
        // Anchored: the tiles stay on the page grid, the object now covers
        // x 5..25, so red squares show at x 10..15 and 20..25 (rows 15..20).
        let anchored = make(false);
        assert_eq!(px(&anchored, 12, 17), (255, 0, 0));
        assert_eq!(px(&anchored, 7, 17), (255, 255, 255));
        // With the object: the tiles moved 5 mm right too, red at x 5..10
        // and 15..20.
        let along = make(true);
        assert_eq!(px(&along, 7, 17), (255, 0, 0));
        assert_eq!(px(&along, 12, 17), (255, 255, 255));
        assert_eq!(px(&along, 17, 17), (255, 0, 0));
    }

    /// A pattern as a transparency mask (a group's vector pattern
    /// transparency read from a `.cdr`): set to transform with its
    /// object, the mask moves with the object; it stayed on the page.
    #[test]
    fn a_pattern_mask_set_to_transform_with_its_object_follows_it() {
        use tracedraw_core::live::{Effect, MergeMode};
        use tracedraw_core::style::PatternPlacement;
        use tracedraw_core::{Command, Engine};
        let make = |with_object: bool| {
            // The mask: white (opaque) over the left half of the tile,
            // black (clear) over the right half; one tile over the object.
            let white = rect(7, Rect::new(0.0, 0.0, 10.0, 20.0), Color::WHITE);
            let black = rect(8, Rect::new(10.0, 0.0, 20.0, 20.0), Color::BLACK);
            let mut s = rect(1, Rect::new(0.0, 0.0, 20.0, 20.0), Color::rgb8(255, 0, 0));
            s.effects.push(Effect::Transparency {
                mask: Fill::Pattern(Pattern::Vector {
                    shapes: vec![white, black],
                    tile: tracedraw_core::geometry::Size::new(20.0, 20.0),
                    placement: PatternPlacement {
                        transform_with_object: with_object,
                        ..PatternPlacement::default()
                    },
                }),
                merge: MergeMode::Normal,
                target: 2,
            });
            let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(30.0, 20.0));
            let page = doc.pages[0].id;
            let layer = doc.pages[0].layers[0].id;
            doc.layer_mut(layer).unwrap().shapes.push(s);
            let mut e = Engine::new(doc);
            e.run(&Command::TransformShapes {
                shapes: vec![tracedraw_core::ShapeId(1)],
                transform: Affine::translate((5.0, 0.0)),
            })
            .unwrap();
            render_page_image(e.document(), page, 25.4).unwrap()
        };
        // The object covers x 5..25. With the object, its left half
        // (x 5..15) is opaque and its right half clear.
        let along = make(true);
        assert_eq!(px(&along, 7, 10), (255, 0, 0));
        assert_eq!(px(&along, 12, 10), (255, 0, 0));
        assert_eq!(px(&along, 17, 10), (255, 255, 255));
        assert_eq!(px(&along, 22, 10), (255, 255, 255));
        // Anchored to the page, the tiles stay where they were: opaque
        // over x 0..10 and 20..30.
        let anchored = make(false);
        assert_eq!(px(&anchored, 7, 10), (255, 0, 0));
        assert_eq!(px(&anchored, 12, 10), (255, 255, 255));
        assert_eq!(px(&anchored, 22, 10), (255, 0, 0));
    }

    /// Mirrored and shifted bitmap tiles: the picture's block is built
    /// once from the decoded picture; a two-by-two block holds the picture
    /// four times, flipped in turn.
    #[test]
    fn bitmap_pattern_mirrors_its_picture() {
        use tracedraw_core::style::PatternPlacement;
        // A 2 x 1 picture: red left, blue right.
        let mut pm = Pixmap::new(2, 1).unwrap();
        pm.pixels_mut()[0] = tiny_skia::ColorU8::from_rgba(255, 0, 0, 255).premultiply();
        pm.pixels_mut()[1] = tiny_skia::ColorU8::from_rgba(0, 0, 255, 255).premultiply();
        let png = pm.encode_png().unwrap();
        let fill = |placement| {
            Fill::Pattern(Pattern::Bitmap {
                png: png.clone().into(),
                width_px: 2,
                height_px: 1,
                size_mm: 10.0,
                placement,
            })
        };
        let bounds = Rect::new(0.0, 0.0, 20.0, 5.0);
        let plain = render_fill_image(&fill(PatternPlacement::default()), bounds, 25.4).unwrap();
        assert_eq!(px(&plain, 2, 2), (255, 0, 0));
        assert_eq!(px(&plain, 12, 2), (255, 0, 0));
        let mirrored = render_fill_image(
            &fill(PatternPlacement {
                mirror_horizontal: true,
                ..PatternPlacement::default()
            }),
            bounds,
            25.4,
        )
        .unwrap();
        assert_eq!(px(&mirrored, 2, 2), (255, 0, 0));
        assert_eq!(px(&mirrored, 12, 2), (0, 0, 255), "the second tile flipped");
        assert_eq!(px(&mirrored, 17, 2), (255, 0, 0));
    }

    /// The block exporters tile: one tile with its placement transform.
    #[test]
    fn pattern_cell_for_exporters_carries_the_placement() {
        use tracedraw_core::style::PatternPlacement;
        let p = Pattern::TwoColor {
            tile: PatternTile::Checker,
            front: Color::BLACK,
            back: Color::WHITE,
            size_mm: 10.0,
            placement: PatternPlacement {
                mirror_horizontal: true,
                offset_x: 3.0,
                ..PatternPlacement::default()
            },
        };
        let (pm, to_page) = render_pattern_cell(&p, 25.4).unwrap();
        assert_eq!((pm.width(), pm.height()), (20, 10));
        let c = to_page.as_coeffs();
        assert_eq!(c, [20.0, 0.0, 0.0, 10.0, 3.0, 0.0]);
        // The second half is the mirrored tile.
        assert_eq!(px(&pm, 2, 7), (0, 0, 0));
        assert_eq!(px(&pm, 12, 7), (255, 255, 255));
    }

    #[test]
    fn vector_pattern_tile_draws_nested_groups_and_outlines() {
        let mut child = rect(8, Rect::new(2.0, 2.0, 8.0, 8.0), Color::rgb8(0, 0, 255));
        child.stroke = Some(Stroke::new(Color::rgb8(0, 255, 0), 2.0));
        let group = Shape::new(
            tracedraw_core::ShapeId(9),
            ShapeKind::Group {
                children: vec![child],
            },
        );
        let fill = Fill::Pattern(Pattern::Vector {
            shapes: vec![group],
            tile: tracedraw_core::geometry::Size::new(10.0, 10.0),
            placement: Default::default(),
        });
        let pm = render_fill_image(&fill, Rect::new(0.0, 0.0, 10.0, 10.0), 254.0).unwrap();
        // 10 px per mm: the centre is blue, the 2 mm outline band is green.
        assert_eq!(px(&pm, 50, 50), (0, 0, 255));
        assert_eq!(px(&pm, 20, 50), (0, 255, 0));
        assert_eq!(px(&pm, 5, 5), (0, 0, 0));
        assert_eq!(
            pm.pixel(5, 5).unwrap().alpha(),
            0,
            "outside the tile content is clear"
        );
    }

    #[test]
    fn degenerate_vector_pattern_does_not_panic() {
        for tile in [
            tracedraw_core::geometry::Size::new(0.0, 10.0),
            tracedraw_core::geometry::Size::new(f64::NAN, 10.0),
            tracedraw_core::geometry::Size::new(0.0001, 0.0001),
            tracedraw_core::geometry::Size::new(1.0e9, 1.0e9),
        ] {
            let fill = Fill::Pattern(Pattern::Vector {
                shapes: vec![rect(1, Rect::new(0.0, 0.0, 1.0, 1.0), Color::BLACK)],
                tile,
                placement: Default::default(),
            });
            assert!(render_fill_image(&fill, Rect::new(0.0, 0.0, 10.0, 10.0), 96.0).is_some());
        }
    }

    #[test]
    fn texture_is_deterministic_for_seed() {
        let t = Fill::Texture(Texture {
            kind: TextureKind::Clouds,
            color_a: Color::rgb8(0, 0, 0),
            color_b: Color::rgb8(255, 255, 255),
            scale: 10.0,
            seed: 7,
        });
        let a = render_fill_image(&t, Rect::new(0.0, 0.0, 30.0, 30.0), 25.4).unwrap();
        let b = render_fill_image(&t, Rect::new(0.0, 0.0, 30.0, 30.0), 25.4).unwrap();
        assert_eq!(a.data(), b.data());
        let v: std::collections::HashSet<u8> = (0..30).map(|x| px(&a, x, 15).0).collect();
        assert!(v.len() > 3, "texture should vary");
    }

    /// A bricks fill drawn at 4 px/mm: the mortar between the rows and
    /// between the bricks shows the background colour, the bricks the
    /// foreground.
    #[test]
    fn procedural_bricks_show_mortar_in_the_background_colour() {
        use tracedraw_core::{Procedural, ProceduralPattern};
        let mut p = Procedural::new(ProceduralPattern::Bricks, Color::rgb8(0, 0, 0));
        p.params.line_width_mm = 1.0;
        p.background = Some(Color::rgb8(255, 255, 255));
        let img = render_fill_image(&Fill::Procedural(p), Rect::new(0.0, 0.0, 20.0, 20.0), 101.6)
            .unwrap();
        assert_eq!((img.width(), img.height()), (80, 80));
        // Page y = 5 mm (row 60) is the mortar between the two rows.
        assert_eq!(px(&img, 10, 60), (255, 255, 255));
        assert_eq!(px(&img, 50, 60), (255, 255, 255));
        // Page y = 7.5 mm (row 50): a brick at x = 2.5 mm, the joint at
        // x = 5 mm, a brick again at 7.5 mm.
        assert_eq!(px(&img, 10, 50), (0, 0, 0));
        assert_eq!(px(&img, 20, 50), (255, 255, 255));
        assert_eq!(px(&img, 30, 50), (0, 0, 0));
        // The lower row's joint is half a brick along, at x = 0 and 10 mm.
        assert_eq!(px(&img, 40, 70), (255, 255, 255));
        assert_eq!(px(&img, 20, 70), (0, 0, 0));
        // The second tile (y = 15 mm, row 20) repeats the first.
        assert_eq!(px(&img, 10, 20), (255, 255, 255));
        assert_eq!(px(&img, 10, 10), (0, 0, 0));
    }

    /// Dots covering half the tile paint about half of its pixels.
    #[test]
    fn procedural_dots_cover_their_share() {
        use tracedraw_core::{Procedural, ProceduralPattern};
        let mut p = Procedural::new(ProceduralPattern::Dots, Color::rgb8(0, 0, 0));
        p.params.share = 0.5;
        p.background = Some(Color::rgb8(255, 255, 255));
        let img = render_fill_image(&Fill::Procedural(p), Rect::new(0.0, 0.0, 10.0, 10.0), 254.0)
            .unwrap();
        assert_eq!((img.width(), img.height()), (100, 100));
        let dark = img
            .pixels()
            .iter()
            .filter(|c| c.demultiply().red() < 128)
            .count();
        assert!((4800..=5200).contains(&dark), "{dark} dark pixels of 10000");
        // The centre is in the dot, a corner is not.
        assert_eq!(px(&img, 50, 50), (0, 0, 0));
        assert_eq!(px(&img, 2, 2), (255, 255, 255));
    }

    /// The speckle is the same picture every time for a seed, another
    /// picture for another seed, and without a background it leaves the
    /// rest clear.
    #[test]
    fn procedural_speckle_is_deterministic_and_can_be_clear() {
        use tracedraw_core::{Procedural, ProceduralPattern};
        let mut p = Procedural::new(ProceduralPattern::Speckle, Color::rgb8(255, 0, 0));
        p.seed = 5;
        p.background = None;
        let f = Fill::Procedural(p.clone());
        let a = render_fill_image(&f, Rect::new(0.0, 0.0, 20.0, 20.0), 101.6).unwrap();
        let b = render_fill_image(&f, Rect::new(0.0, 0.0, 20.0, 20.0), 101.6).unwrap();
        assert_eq!(a.data(), b.data());
        let painted = a.pixels().iter().filter(|c| c.alpha() > 0).count();
        let clear = a.pixels().iter().filter(|c| c.alpha() == 0).count();
        assert!(
            painted > 50 && clear > painted,
            "{painted} painted, {clear} clear"
        );
        p.seed = 6;
        let c = render_fill_image(&Fill::Procedural(p), Rect::new(0.0, 0.0, 20.0, 20.0), 101.6)
            .unwrap();
        assert_ne!(a.data(), c.data());
    }

    #[test]
    fn large_textures_on_screen_are_quick_and_exports_keep_every_pixel() {
        let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(300.0, 300.0));
        let layer = doc.pages[0].layers[0].id;
        let id = doc.ids_mut().shape();
        let mut s = Shape::new(
            id,
            ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 300.0, 300.0),
                radius: 0.0,
                corners: None,
            },
        );
        s.fill = Fill::Texture(Texture {
            kind: TextureKind::Clouds,
            color_a: Color::Cmyk {
                c: 1.0,
                m: 0.2,
                y: 0.0,
                k: 0.1,
            },
            color_b: Color::rgb8(255, 255, 255),
            scale: 40.0,
            seed: 11,
        });
        s.stroke = None;
        doc.layer_mut(layer).unwrap().shapes.push(s);
        let page = doc.pages[0].id;
        let opts = |draft: bool| RenderOptions {
            width: 1600,
            height: 1000,
            view: ViewTransform {
                zoom: 6.0,
                origin_x: -100.0,
                origin_y: 1500.0,
                stretch_y: 1.0,
            },
            draft,
            ..Default::default()
        };
        // Drawn whole, on one thread, so the times compare.
        let exact = render_page_whole(&doc, page, &opts(false), None).unwrap();
        let t = std::time::Instant::now();
        let draft = render_page_whole(&doc, page, &opts(true), None).unwrap();
        let first = t.elapsed();
        // The best of three, with room for other tests loading the cores.
        let mut cached = std::time::Duration::MAX;
        let mut again = None;
        for _ in 0..3 {
            let t = std::time::Instant::now();
            again = render_page_whole(&doc, page, &opts(true), None);
            cached = cached.min(t.elapsed());
        }
        assert_eq!(
            draft.data(),
            again.unwrap().data(),
            "the kept result is the same"
        );
        assert!(cached <= first * 2, "{cached:?} {first:?}");
        // The coarser grid stays close to the exact picture.
        let mut worst = 0i32;
        for (a, b) in exact.pixels().iter().zip(draft.pixels()).step_by(97) {
            worst = worst.max((a.red() as i32 - b.red() as i32).abs());
        }
        assert!(worst < 40, "{worst}");
    }

    #[test]
    fn outlines_take_a_gradient_fill() {
        let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(100.0, 20.0));
        let layer = doc.pages[0].layers[0].id;
        let id = doc.ids_mut().shape();
        let mut path = tracedraw_core::BezPath::new();
        path.move_to((5.0, 10.0));
        path.line_to((95.0, 10.0));
        let mut s = Shape::new(
            id,
            ShapeKind::Path {
                path,
                closed: false,
            },
        );
        s.fill = Fill::None;
        let mut stroke = Stroke::new(Color::rgb8(0, 255, 0), 6.0);
        stroke.fill = Some(Box::new(Fill::Fountain(tracedraw_core::Fountain::two(
            tracedraw_core::FountainKind::Linear,
            Color::rgb8(255, 0, 0),
            Color::rgb8(0, 0, 255),
            0.0,
        ))));
        s.stroke = Some(stroke);
        doc.layer_mut(layer).unwrap().shapes.push(s);
        let img = render_page_image(&doc, doc.pages[0].id, 25.4).unwrap();
        let left = img.pixel(8, 10).unwrap();
        let right = img.pixel(92, 10).unwrap();
        assert!(left.red() > 200 && left.blue() < 60, "{left:?}");
        assert!(right.blue() > 200 && right.red() < 60, "{right:?}");
        assert!(left.green() < 30, "not the outline colour");
        // Outside the stroke nothing is painted.
        assert_eq!(img.pixel(50, 2).unwrap().green(), 255);
    }

    #[test]
    fn a_gradient_along_the_line_follows_it_round_a_turn() {
        // Right along the top, down, and back left along the bottom: the
        // gradient runs from the start (red) to the end (blue), so the
        // two left ends differ, where a gradient across would match them.
        let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(100.0, 20.0));
        let layer = doc.pages[0].layers[0].id;
        let id = doc.ids_mut().shape();
        let mut path = tracedraw_core::BezPath::new();
        path.move_to((5.0, 5.0));
        path.line_to((95.0, 5.0));
        path.line_to((95.0, 15.0));
        path.line_to((5.0, 15.0));
        let mut s = Shape::new(
            id,
            ShapeKind::Path {
                path,
                closed: false,
            },
        );
        s.fill = Fill::None;
        let mut stroke = Stroke::new(Color::rgb8(0, 255, 0), 4.0);
        stroke.fill = Some(Box::new(Fill::Fountain(tracedraw_core::Fountain::two(
            tracedraw_core::FountainKind::Linear,
            Color::rgb8(255, 0, 0),
            Color::rgb8(0, 0, 255),
            0.0,
        ))));
        stroke.fill_along = true;
        s.stroke = Some(stroke);
        doc.layer_mut(layer).unwrap().shapes.push(s);
        let img = render_page_image(&doc, doc.pages[0].id, 25.4).unwrap();
        let a = img.pixel(7, 5).unwrap();
        let b = img.pixel(7, 15).unwrap();
        let (red, blue) = if a.red() > b.red() { (a, b) } else { (b, a) };
        assert!(red.red() > 200 && red.blue() < 60, "{red:?}");
        assert!(blue.blue() > 200 && blue.red() < 60, "{blue:?}");
        assert!(red.green() < 30 && blue.green() < 30, "not the outline colour");
        // The far end of the turn is half way.
        let mid = img.pixel(95, 10).unwrap();
        assert!(mid.red() > 80 && mid.blue() > 80, "{mid:?}");
    }

    /// A 100 x 10 mm rectangle filled with `fill`, drawn at 1 px/mm.
    fn strip(fill: Fill) -> Pixmap {
        let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(100.0, 10.0));
        let page = doc.pages[0].id;
        let mut s = rect(1, Rect::new(0.0, 0.0, 100.0, 10.0), Color::WHITE);
        s.fill = fill;
        s.stroke = None;
        doc.layer_mut(doc.pages[0].layers[0].id)
            .unwrap()
            .shapes
            .push(s);
        render_page_image(&doc, page, 25.4).unwrap()
    }

    #[test]
    fn linear_fountains_repeat_mirror_and_invert() {
        let bw = || Fountain::two(FountainKind::Linear, Color::BLACK, Color::WHITE, 0.0);
        let red_at = |img: &Pixmap, x: u32| img.pixel(x, 5).unwrap().red();
        // Sawtooth twice: light just before the middle, dark just after.
        let mut f = bw();
        f.repeat = tracedraw_core::FountainRepeat::Repeat;
        f.repeats = 2;
        let img = strip(Fill::Fountain(f));
        assert!(red_at(&img, 48) > 220, "{}", red_at(&img, 48));
        assert!(red_at(&img, 51) < 35, "{}", red_at(&img, 51));
        assert!((red_at(&img, 25) as i32 - 128).abs() < 20);
        // Triangular twice: white in the middle, black at both ends.
        let mut m = bw();
        m.repeat = tracedraw_core::FountainRepeat::Mirror;
        m.repeats = 2;
        let img = strip(Fill::Fountain(m));
        assert!(red_at(&img, 50) > 240 && red_at(&img, 1) < 15 && red_at(&img, 98) < 15);
        // Bilinear gives the same run.
        let mut b = bw();
        b.bilinear = true;
        let bimg = strip(Fill::Fountain(b));
        for x in [1, 25, 50, 75, 98] {
            assert!((red_at(&img, x) as i32 - red_at(&bimg, x) as i32).abs() <= 3);
        }
        // Inverted: white on the left.
        let mut i = bw();
        i.invert = true;
        let img = strip(Fill::Fountain(i));
        assert!(red_at(&img, 1) > 240 && red_at(&img, 98) < 15);
        // Conical and square fountains take the options too.
        let mut sq = Fountain::two(FountainKind::Square, Color::BLACK, Color::WHITE, 0.0);
        sq.invert = true;
        let img = strip(Fill::Fountain(sq));
        assert!(red_at(&img, 50) > 200, "{}", red_at(&img, 50));
        assert!(red_at(&img, 1) < 30, "{}", red_at(&img, 1));
    }

    /// A stepped fill is drawn as exactly `steps` colours, by the gradient
    /// shader (linear) and pixel by pixel (square); a hue path and the
    /// smooth transition show in the pixels too (#1208).
    #[test]
    fn stepped_and_hue_fountains_draw_their_colours() {
        let bw = || Fountain::two(FountainKind::Linear, Color::BLACK, Color::WHITE, 0.0);
        let row = |img: &Pixmap| -> Vec<u8> {
            let mut v: Vec<u8> = (1..99).map(|x| img.pixel(x, 5).unwrap().red()).collect();
            v.sort_unstable();
            v.dedup();
            v
        };
        for steps in [2u32, 4, 7] {
            let mut f = bw();
            f.steps = steps;
            let greys = row(&strip(Fill::Fountain(f)));
            assert_eq!(greys.len(), steps as usize, "{steps}: {greys:?}");
            assert_eq!((greys[0], greys[greys.len() - 1]), (0, 255));
            let mut sq = Fountain::two(FountainKind::Square, Color::BLACK, Color::WHITE, 0.0);
            sq.steps = steps;
            let greys = row(&strip(Fill::Fountain(sq)));
            assert_eq!(greys.len(), steps as usize, "square {steps}: {greys:?}");
        }
        // Four bands: the left quarter black, the next 85.
        let mut f = bw();
        f.steps = 4;
        let img = strip(Fill::Fountain(f));
        assert_eq!(img.pixel(10, 5).unwrap().red(), 0);
        assert_eq!(img.pixel(40, 5).unwrap().red(), 85);
        assert_eq!(img.pixel(90, 5).unwrap().red(), 255);
        // Red to blue clockwise: green in the middle; straight: purple.
        let (red, blue) = (Color::rgb8(255, 0, 0), Color::rgb8(0, 0, 255));
        let mut h = Fountain::two(FountainKind::Linear, red, blue, 0.0);
        let mid = strip(Fill::Fountain(h.clone())).pixel(50, 5).unwrap();
        assert!(
            mid.green() < 10 && mid.red() > 100 && mid.blue() > 100,
            "{mid:?}"
        );
        h.blend_direction = tracedraw_core::ColorBlend::Clockwise;
        let mid = strip(Fill::Fountain(h.clone())).pixel(50, 5).unwrap();
        assert!(
            mid.green() > 240 && mid.red() < 20 && mid.blue() < 20,
            "{mid:?}"
        );
        // Conical fills take the hue path too (drawn pixel by pixel):
        // the colour half way round is green as well.
        h.kind = FountainKind::Conical;
        let img = strip(Fill::Fountain(h));
        let mid = img.pixel(50, 9).unwrap();
        assert!(mid.green() > 200, "{mid:?}");
        // Smooth transition: darker early on, lighter late, grey in the middle.
        let mut s = bw();
        s.smooth = true;
        let img = strip(Fill::Fountain(s));
        let plain = strip(Fill::Fountain(bw()));
        assert!(img.pixel(15, 5).unwrap().red() < plain.pixel(15, 5).unwrap().red() - 10);
        assert!(img.pixel(85, 5).unwrap().red() > plain.pixel(85, 5).unwrap().red() + 10);
        assert!((img.pixel(50, 5).unwrap().red() as i32 - 128).abs() <= 2);
        // Acceleration +100: light almost from the start.
        let mut a = bw();
        a.acceleration = 100;
        let img = strip(Fill::Fountain(a));
        assert!(
            img.pixel(10, 5).unwrap().red() > 150,
            "{}",
            img.pixel(10, 5).unwrap().red()
        );
    }

    #[test]
    fn shaped_fountains_follow_the_outline() {
        // Black on the edge, white deepest inside.
        let f = Fountain::two(FountainKind::Shaped, Color::BLACK, Color::WHITE, 0.0);
        let img = strip(Fill::Fountain(f.clone()));
        let at = |img: &Pixmap, x: u32, y: u32| img.pixel(x, y).unwrap().red() as i32;
        assert!(at(&img, 50, 0) < 40, "edge {}", at(&img, 50, 0));
        assert!(at(&img, 50, 5) > 200, "middle {}", at(&img, 50, 5));
        // Along a long strip the colour depends on the distance to the
        // nearest edge, not on the distance to the centre.
        assert!((at(&img, 20, 5) - at(&img, 80, 5)).abs() <= 6);
        assert!((at(&img, 20, 5) - at(&img, 50, 5)).abs() <= 6);
        assert!(at(&img, 2, 5) < at(&img, 20, 5));
        // A circle: rings of equal colour.
        let (mut doc, page) = doc_with(vec![]);
        let id = doc.ids_mut().shape();
        let mut c = Shape::new(
            id,
            ShapeKind::Ellipse {
                rect: Rect::new(0.0, 0.0, 60.0, 60.0),
                arc: None,
            },
        );
        c.fill = Fill::Fountain(f);
        c.stroke = None;
        doc.layer_mut(doc.pages[0].layers[0].id)
            .unwrap()
            .shapes
            .push(c);
        let img = render_page_image(&doc, page, 25.4).unwrap();
        // Page y is up: the circle's centre is at (30, page height - 30).
        let cy = img.height() - 30;
        let (e, w, n) = (at(&img, 45, cy), at(&img, 15, cy), at(&img, 30, cy - 15));
        assert!((e - w).abs() <= 8 && (e - n).abs() <= 8, "{e} {w} {n}");
        assert!(at(&img, 30, cy) > 230, "{}", at(&img, 30, cy));
        assert!(at(&img, 2, cy) < at(&img, 15, cy));
    }

    #[test]
    fn edge_distance_measures_to_the_nearest_outside_pixel() {
        // A 7 x 5 grid with a 5 x 3 block inside.
        let (w, h) = (7usize, 5usize);
        let inside: Vec<bool> = (0..w * h)
            .map(|i| (1..6).contains(&(i % w)) && (1..4).contains(&(i / w)))
            .collect();
        let d = edge_distance(&inside, w, h);
        assert_eq!(d[0], 0.0);
        assert!((d[w + 1] - 0.5).abs() < 1e-9);
        assert!((d[2 * w + 3] - 1.5).abs() < 1e-9);
        assert!(edge_distance(&[], 0, 0).is_empty());
    }

    #[test]
    fn outlines_take_pattern_texture_mesh_and_shaped_paints() {
        let line = |paint: Fill| {
            let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(100.0, 20.0));
            let layer = doc.pages[0].layers[0].id;
            let id = doc.ids_mut().shape();
            let mut path = tracedraw_core::BezPath::new();
            path.move_to((5.0, 10.0));
            path.line_to((95.0, 10.0));
            let mut s = Shape::new(
                id,
                ShapeKind::Path {
                    path,
                    closed: false,
                },
            );
            s.fill = Fill::None;
            let mut stroke = Stroke::new(Color::rgb8(0, 255, 0), 6.0);
            stroke.fill = Some(Box::new(paint));
            s.stroke = Some(stroke);
            doc.layer_mut(layer).unwrap().shapes.push(s);
            render_page_image(&doc, doc.pages[0].id, 25.4).unwrap()
        };
        let red = Color::rgb8(255, 0, 0);
        let blue = Color::rgb8(0, 0, 255);
        // A two-colour pattern: both colours along the outline, never the
        // outline colour, nothing beside it.
        let img = line(Fill::Pattern(Pattern::TwoColor {
            tile: tracedraw_core::PatternTile::Stripes,
            front: red,
            back: blue,
            size_mm: 4.0,
            placement: Default::default(),
        }));
        let row: Vec<_> = (6..94).map(|x| img.pixel(x, 10).unwrap()).collect();
        assert!(row.iter().all(|p| p.green() < 30));
        assert_eq!(img.pixel(50, 2).unwrap().green(), 255);
        let col: Vec<_> = (7..13).map(|y| img.pixel(50, y).unwrap()).collect();
        assert!(col.iter().any(|p| p.red() > 200) && col.iter().any(|p| p.blue() > 200));
        // A texture.
        let img = line(Fill::Texture(Texture {
            kind: tracedraw_core::TextureKind::Noise,
            color_a: red,
            color_b: blue,
            scale: 2.0,
            seed: 3,
        }));
        let p = img.pixel(50, 10).unwrap();
        assert!(p.green() < 30 && p.red() + p.blue() > 150, "{p:?}");
        // A mesh: its nodes are local to the object, red on the left and
        // blue on the right.
        let mut m = tracedraw_core::Mesh::new(Rect::new(5.0, 7.0, 95.0, 13.0), 1, 1, red);
        for (i, n) in m.nodes.iter_mut().enumerate() {
            if i % 2 == 1 {
                n.color = blue;
            }
        }
        let img = line(Fill::Mesh(m));
        let (l, r) = (img.pixel(8, 10).unwrap(), img.pixel(92, 10).unwrap());
        assert!(l.red() > 200 && l.blue() < 60, "{l:?}");
        assert!(r.blue() > 200 && r.red() < 60, "{r:?}");
        // A shaped gradient follows the outline's own area: its edge
        // dark, its middle light.
        let img = line(Fill::Fountain(Fountain::two(
            FountainKind::Shaped,
            Color::BLACK,
            Color::WHITE,
            0.0,
        )));
        let (edge, mid) = (img.pixel(50, 7).unwrap(), img.pixel(50, 10).unwrap());
        assert!(mid.red() > edge.red() + 60, "{edge:?} {mid:?}");
    }

    #[test]
    fn procedural_fills_work_out_only_the_visible_part() {
        let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(100.0, 100.0));
        let layer = doc.pages[0].layers[0].id;
        let id = doc.ids_mut().shape();
        let mut s = Shape::new(
            id,
            ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 2.0, 2.0),
                radius: 0.0,
                corners: None,
            },
        );
        s.fill = Fill::Texture(Texture {
            kind: TextureKind::Clouds,
            color_a: Color::rgb8(0, 0, 0),
            color_b: Color::rgb8(255, 255, 255),
            scale: 0.5,
            seed: 3,
        });
        s.stroke = None;
        doc.layer_mut(layer).unwrap().shapes.push(s);
        let page = doc.pages[0].id;
        let view = |w: u32, ox: f64, zoom: f64| RenderOptions {
            width: w,
            height: 64,
            view: ViewTransform {
                zoom,
                origin_x: ox,
                origin_y: 64.0,
                stretch_y: 1.0,
            },
            ..Default::default()
        };
        // The whole square, then its right half alone: the same pixels.
        let whole = render_page(&doc, page, &view(64, 0.0, 32.0)).unwrap();
        let half = render_page(&doc, page, &view(32, -32.0, 32.0)).unwrap();
        for y in [5u32, 30, 60] {
            for x in [2u32, 15, 29] {
                assert_eq!(
                    whole.pixel(x + 32, y).unwrap(),
                    half.pixel(x, y).unwrap(),
                    "{x},{y}"
                );
            }
        }
        // Zoomed so far that the square is 200 000 pixels wide: only the
        // window is worked out, at once.
        let t = std::time::Instant::now();
        let deep = render_page(&doc, page, &view(64, 0.0, 100_000.0)).unwrap();
        assert!(t.elapsed().as_secs_f64() < 2.0);
        assert!(deep.pixel(10, 10).unwrap().alpha() == 255);
    }

    #[test]
    fn star_fill_covers_center_and_not_corner() {
        let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(100.0, 100.0));
        let layer = doc.pages[0].layers[0].id;
        let id = doc.ids_mut().shape();
        let mut s = Shape::new(
            id,
            ShapeKind::Polygon {
                rect: Rect::new(10.0, 10.0, 90.0, 90.0),
                points: 5,
                sharpness: 0.6,
                complex: None,
                twist: 0.0,
                bend: 0.0,
            },
        );
        s.fill = Fill::Solid(Color::rgb8(255, 0, 0));
        s.stroke = None;
        doc.layer_mut(layer).unwrap().shapes.push(s);
        let page = doc.pages[0].id;
        let img = render_page_image(&doc, page, 25.4).unwrap(); // 1 px per mm
        let px = |x: u32, y: u32| img.pixel(x, y).unwrap();
        // Centre of the star is red; the page corner is white.
        assert_eq!(px(50, 50).red(), 255);
        assert_eq!(px(50, 50).green(), 0);
        assert_eq!(px(2, 2).green(), 255);
    }

    #[test]
    fn without_anti_aliasing_edges_are_hard() {
        let mut ellipse = Shape::new(
            tracedraw_core::ShapeId(3),
            ShapeKind::Ellipse {
                rect: Rect::new(10.0, 10.0, 90.0, 70.0),
                arc: None,
            },
        );
        ellipse.fill = Fill::Solid(Color::rgb8(0, 0, 0));
        ellipse.stroke = None;
        let (doc, page) = doc_with(vec![ellipse]);
        let render = |anti_alias: bool| {
            let opts = RenderOptions {
                width: 100,
                height: 100,
                view: ViewTransform {
                    zoom: 1.0,
                    origin_x: 0.0,
                    origin_y: 100.0,
                    stretch_y: 1.0,
                },
                anti_alias,
                ..RenderOptions::default()
            };
            render_page(&doc, page, &opts).unwrap()
        };
        let partial = |pm: &Pixmap| {
            pm.pixels()
                .iter()
                .filter(|p| p.alpha() != 0 && p.alpha() != 255)
                .count()
        };
        assert!(partial(&render(true)) > 20);
        assert_eq!(partial(&render(false)), 0);
    }

    #[test]
    fn half_transparent_red_over_white_is_pink() {
        let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(20.0, 20.0));
        let layer = doc.pages[0].layers[0].id;
        let id = doc.ids_mut().shape();
        let mut s = Shape::new(
            id,
            ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 20.0, 20.0),
                radius: 0.0,
                corners: None,
            },
        );
        s.fill = Fill::Solid(Color::rgb8(255, 0, 0));
        s.stroke = None;
        s.opacity = 0.5;
        doc.layer_mut(layer).unwrap().shapes.push(s);
        let page = doc.pages[0].id;
        let img = render_page_image(&doc, page, 25.4).unwrap();
        let p = img.pixel(10, 10).unwrap();
        assert_eq!(p.red(), 255);
        assert!((120..=136).contains(&p.green()), "green was {}", p.green());
    }

    /// Two overlapping squares in a group on a 20 mm page: the left one
    /// red, the right one blue, overlapping in the middle.
    fn group_of_two_squares(opacity: f64) -> (Document, tracedraw_core::PageId) {
        let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(20.0, 20.0));
        let layer = doc.pages[0].layers[0].id;
        let square = |doc: &mut Document, x0: f64, x1: f64, color: Color| {
            let id = doc.ids_mut().shape();
            let mut s = Shape::new(
                id,
                ShapeKind::Rect {
                    rect: Rect::new(x0, 0.0, x1, 20.0),
                    radius: 0.0,
                    corners: None,
                },
            );
            s.fill = Fill::Solid(color);
            s.stroke = None;
            s
        };
        let red = square(&mut doc, 0.0, 14.0, Color::rgb8(255, 0, 0));
        let blue = square(&mut doc, 6.0, 20.0, Color::rgb8(0, 0, 255));
        let gid = doc.ids_mut().shape();
        let mut group = Shape::new(
            gid,
            ShapeKind::Group {
                children: vec![red, blue],
            },
        );
        group.stroke = None;
        group.opacity = opacity;
        doc.layer_mut(layer).unwrap().shapes.push(group);
        let page = doc.pages[0].id;
        (doc, page)
    }

    /// A group's uniform transparency is drawn on each member, exactly as
    /// the same transparency set on each member of an opaque group: the
    /// screenshot of #1386, "first grouped, then transparency" looking as
    /// "first transparency, then grouped".
    #[test]
    fn a_group_with_uniform_transparency_is_drawn_see_through() {
        let (doc, page) = group_of_two_squares(0.5);
        let img = render_page_image(&doc, page, 25.4).unwrap();
        // Red alone: half red over white is pink.
        let red = img.pixel(3, 10).unwrap();
        assert_eq!(red.red(), 255);
        assert!(
            (120..=136).contains(&red.green()),
            "green was {}",
            red.green()
        );
        // Where they overlap the red one shows through the blue one: the
        // overlap is darker than the blue one alone (less green).
        let overlap = img.pixel(10, 10).unwrap();
        let blue = img.pixel(17, 10).unwrap();
        assert!(overlap.green() + 30 < blue.green(), "{overlap:?} {blue:?}");
        // The same as each member set half transparent.
        let (mut each, _) = group_of_two_squares(1.0);
        let layer = each.pages[0].layers[0].id;
        if let Some(ShapeKind::Group { children }) = each
            .layer_mut(layer)
            .ok()
            .and_then(|l| l.shapes.first_mut())
            .map(|g| &mut g.kind)
        {
            for c in children {
                c.opacity = 0.5;
            }
        }
        let want = render_page_image(&each, page, 25.4).unwrap();
        assert_eq!(img.data(), want.data());
    }

    /// Nested groups multiply their transparencies, and a member's own.
    #[test]
    fn nested_group_transparencies_multiply() {
        let (mut doc, page) = group_of_two_squares(0.5);
        let layer = doc.pages[0].layers[0].id;
        let gid = doc.ids_mut().shape();
        let l = doc.layer_mut(layer).unwrap();
        let outer = l.shapes.remove(0);
        let mut wrap = Shape::new(
            gid,
            ShapeKind::Group {
                children: vec![outer],
            },
        );
        wrap.stroke = None;
        wrap.opacity = 0.5;
        l.shapes.push(wrap);
        let img = render_page_image(&doc, page, 25.4).unwrap();
        // Red alone at a quarter: 255 - 255 / 4 green and blue.
        let red = img.pixel(3, 10).unwrap();
        assert_eq!(red.red(), 255);
        assert!(
            (184..=200).contains(&red.green()),
            "green was {}",
            red.green()
        );
    }

    #[test]
    fn a_fully_see_through_group_draws_nothing() {
        let (doc, page) = group_of_two_squares(0.0);
        let img = render_page_image(&doc, page, 25.4).unwrap();
        for x in [3, 10, 17] {
            let p = img.pixel(x, 10).unwrap();
            assert_eq!((p.red(), p.green(), p.blue()), (255, 255, 255), "x {x}");
        }
    }

    #[test]
    fn an_opaque_group_is_drawn_as_before() {
        let (doc, page) = group_of_two_squares(1.0);
        let img = render_page_image(&doc, page, 25.4).unwrap();
        let red = img.pixel(3, 10).unwrap();
        assert_eq!((red.red(), red.green(), red.blue()), (255, 0, 0));
        let overlap = img.pixel(10, 10).unwrap();
        assert_eq!(
            (overlap.red(), overlap.green(), overlap.blue()),
            (0, 0, 255)
        );
    }

    #[test]
    fn shadow_darkens_pixels_beside_the_object() {
        let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(40.0, 40.0));
        let layer = doc.pages[0].layers[0].id;
        let id = doc.ids_mut().shape();
        let mut s = Shape::new(
            id,
            ShapeKind::Rect {
                rect: Rect::new(10.0, 10.0, 25.0, 25.0),
                radius: 0.0,
                corners: None,
            },
        );
        s.fill = Fill::Solid(Color::rgb8(0, 0, 255));
        s.stroke = None;
        s.shadow = Some(tracedraw_core::Shadow {
            offset: tracedraw_core::geometry::Vec2::new(5.0, -5.0),
            opacity: 0.8,
            blur: 0.0,
            color: Color::BLACK,
            ..Default::default()
        });
        doc.layer_mut(layer).unwrap().shapes.push(s);
        let page = doc.pages[0].id;
        let img = render_page_image(&doc, page, 25.4).unwrap();
        // Just right of the rect and below: shadow only (y down in the image).
        let p = img.pixel(28, 32).unwrap();
        assert!(p.red() < 100, "red was {}", p.red());
        let far = img.pixel(2, 2).unwrap();
        assert_eq!(far.red(), 255);
    }

    /// A drop shadow is painted on a layer of only the pixels it can
    /// reach, which a drag redraws every frame (#935): the picture is
    /// the same, pixel for pixel, as with a layer the size of the whole
    /// window, for flat and perspective shadows, every feather
    /// direction, merge modes over a filled object, shadows running off
    /// the window's edges, and a group's shadow.
    #[test]
    fn shadows_on_their_own_pixels_match_a_layer_of_the_whole_window() {
        use tracedraw_core::live::MergeMode;
        use tracedraw_core::shadow::{FeatherDirection, Perspective, ShadowEdge};
        use tracedraw_core::Shadow;
        let opts = RenderOptions {
            width: 240,
            height: 180,
            view: ViewTransform {
                zoom: 3.0,
                origin_x: -15.0,
                origin_y: 200.0,
                stretch_y: 1.0,
            },
            ..Default::default()
        };
        let screen = opts.view.affine();
        let rect = |doc: &mut Document, r: Rect, fill: Fill| {
            let mut s = Shape::new(
                doc.ids_mut().shape(),
                ShapeKind::Rect {
                    rect: r,
                    radius: 0.0,
                    corners: None,
                },
            );
            s.fill = fill;
            s.stroke = None;
            s
        };
        let flat = |dx: f64, dy: f64, blur: f64, dir: FeatherDirection, merge: MergeMode| Shadow {
            offset: tracedraw_core::geometry::Vec2::new(dx, dy),
            opacity: 0.7,
            blur,
            color: Color::rgb8(20, 40, 160),
            merge,
            feather_direction: dir,
            ..Default::default()
        };
        let mut floor = flat(0.0, 0.0, 2.0, FeatherDirection::Middle, MergeMode::Multiply);
        floor.perspective = Some(Perspective {
            fade: 0.6,
            ..Perspective::new(ShadowEdge::Bottom)
        });
        let body = Rect::new(30.0, 30.0, 50.0, 60.0);
        let cases = [
            (
                body,
                flat(
                    6.0,
                    -4.0,
                    1.5,
                    FeatherDirection::Middle,
                    MergeMode::Multiply,
                ),
            ),
            (
                body,
                flat(3.0, 3.0, 2.5, FeatherDirection::Outside, MergeMode::Normal),
            ),
            (
                body,
                flat(
                    -4.0,
                    -2.0,
                    3.0,
                    FeatherDirection::Average,
                    MergeMode::Difference,
                ),
            ),
            (
                Rect::new(40.0, 40.0, 60.0, 50.0),
                flat(2.0, 2.0, 1.0, FeatherDirection::Inside, MergeMode::Add),
            ),
            (body, floor),
            // Off the window's left and top edges.
            (
                Rect::new(0.0, 55.0, 15.0, 70.0),
                flat(
                    -3.0,
                    3.0,
                    4.0,
                    FeatherDirection::Middle,
                    MergeMode::Multiply,
                ),
            ),
            // Off the right and bottom edges, unfeathered.
            (
                Rect::new(75.0, 5.0, 90.0, 20.0),
                flat(4.0, -4.0, 0.0, FeatherDirection::Middle, MergeMode::Normal),
            ),
        ];
        for (k, (r, sh)) in cases.into_iter().enumerate() {
            for group in [false, true] {
                let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(100.0, 100.0));
                let layer = doc.pages[0].layers[0].id;
                // A filled object beneath, so the merge mode tells.
                let under = rect(
                    &mut doc,
                    Rect::new(20.0, 20.0, 90.0, 140.0),
                    Fill::Solid(Color::rgb8(230, 180, 40)),
                );
                // The shadowed object draws nothing itself, so only its
                // shadow lies over what is beneath.
                let mut top = rect(&mut doc, r, Fill::None);
                if group {
                    let second = rect(
                        &mut doc,
                        Rect::new(r.x0 + 2.0, r.y1 + 3.0, r.x1, r.y1 + 9.0),
                        Fill::None,
                    );
                    top = Shape::new(
                        doc.ids_mut().shape(),
                        ShapeKind::Group {
                            children: vec![top, second],
                        },
                    );
                    top.fill = Fill::None;
                    top.stroke = None;
                }
                let mut without = doc.clone();
                without.layer_mut(layer).unwrap().shapes = vec![under.clone(), top.clone()];
                top.shadow = Some(sh);
                let shadowed = top.clone();
                doc.layer_mut(layer).unwrap().shapes = vec![under, top];
                let page = doc.pages[0].id;
                let drawn = render_page(&doc, page, &opts).expect("render");
                let plain = render_page(&without, page, &opts).expect("render");
                // The reference: a layer the size of the window.
                let mut expected = plain.clone();
                let mut full = Pixmap::new(opts.width, opts.height).expect("layer");
                let zoom = opts.view.zoom;
                if group {
                    let b = group_shadow_box(&shadowed, Affine::IDENTITY).expect("box");
                    group_shadow_layer(
                        &mut full,
                        &sh,
                        &shadowed,
                        Affine::IDENTITY,
                        b,
                        screen,
                        zoom,
                        true,
                    );
                } else {
                    let path = shadowed.page_path();
                    assert!(shadow_layer(
                        &mut full,
                        &sh,
                        Shown {
                            shape: &shadowed,
                            path: &path,
                            object: shadowed.transform,
                            rule: shape_fill_rule(&shadowed, false),
                        },
                        screen,
                        zoom,
                        true
                    ));
                }
                let pp = tiny_skia::PixmapPaint {
                    blend_mode: merge_blend(sh.merge),
                    ..Default::default()
                };
                expected.draw_pixmap(0, 0, full.as_ref(), &pp, Transform::identity(), None);
                assert!(drawn.data() == expected.data(), "case {k}, group {group}");
                // And the shadow shows at all.
                assert!(drawn.data() != plain.data(), "case {k}, group {group}");
            }
        }
        // A shadow wholly outside the window paints nothing and takes no layer.
        let sh = flat(500.0, 0.0, 1.0, FeatherDirection::Middle, MergeMode::Normal);
        assert_eq!(shadow_pixels(&sh, body, screen, 3.0, 240, 180), None);
        // A small one takes a small layer.
        let sh = flat(2.0, -2.0, 1.0, FeatherDirection::Middle, MergeMode::Normal);
        let small = Rect::new(30.0, 30.0, 40.0, 40.0);
        let (_, _, w, h) = shadow_pixels(&sh, small, screen, 3.0, 240, 180).expect("pixels");
        assert!(w < 60 && h < 60, "{w} x {h}");
    }

    /// A clip frame and a group with a shadow of their own draw it: the
    /// silhouette of what they show, a clip frame's contents cut to its
    /// frame (an empty part of the frame or contents outside it cast
    /// none), each object of a group, moved with the object (#873).
    #[test]
    fn clip_frames_and_groups_cast_their_shadows() {
        let shadow = tracedraw_core::Shadow {
            offset: tracedraw_core::geometry::Vec2::new(15.0, 0.0),
            opacity: 1.0,
            blur: 0.0,
            color: Color::BLACK,
            merge: tracedraw_core::live::MergeMode::Normal,
            ..Default::default()
        };
        // Page 80 x 40 mm at 1 px a mm; image y = 40 - page y.
        let dark = |img: &Pixmap, x: u32, page_y: u32| {
            let p = img.pixel(x, 40 - page_y).expect("pixel");
            p.red() < 60 && p.green() < 60 && p.blue() < 60
        };
        let white = |img: &Pixmap, x: u32, page_y: u32| {
            let p = img.pixel(x, 40 - page_y).expect("pixel");
            p.red() > 240 && p.green() > 240 && p.blue() > 240
        };
        for moved in [0.0, 30.0] {
            let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(80.0, 40.0));
            let layer = doc.pages[0].layers[0].id;
            let mut frame = rect(1, Rect::new(10.0, 10.0, 30.0, 30.0), Color::BLACK);
            frame.fill = Fill::None;
            let inside = rect(2, Rect::new(5.0, 5.0, 20.0, 20.0), Color::rgb8(0, 0, 255));
            let mut clip = Shape::new(
                tracedraw_core::ShapeId(3),
                ShapeKind::ClipFrame {
                    frame: Box::new(frame),
                    contents: vec![inside],
                },
            );
            clip.shadow = Some(shadow);
            clip.transform = Affine::translate((moved, 0.0));
            doc.layer_mut(layer)
                .expect("layer")
                .shapes
                .push(clip.clone());
            let img = render_page_image(&doc, doc.pages[0].id, 25.4).expect("image");
            let x = |v: f64| (v + moved) as u32;
            // The shown part of the contents (10..20 square) casts a
            // shadow 15 mm to its right.
            assert!(dark(&img, x(30.0), 15), "{moved}");
            assert!(dark(&img, x(26.0), 12), "{moved}");
            // The empty part of the frame casts none, nor do the contents
            // outside it.
            assert!(white(&img, x(40.0), 25), "{moved}");
            assert!(white(&img, x(22.0), 7), "{moved}");
            // The contents still show.
            let c = img.pixel(x(15.0), 40 - 15).expect("pixel");
            assert!(c.blue() > 200 && c.red() < 60, "{c:?}");
            // Break Drop Shadow Apart: the same silhouette.
            let (r, pm) = render_shadow_image(&clip, 25.4).expect("shadow image");
            let at = |page_x: f64, page_y: f64| {
                pm.pixel((page_x - r.x0) as u32, (r.y1 - page_y) as u32)
                    .map_or(0, |p| p.alpha())
            };
            assert!(at(moved + 30.0, 15.0) > 200, "{r:?}");
            assert!(at(moved + 40.0, 25.0) < 20, "{r:?}");
        }
        // A group: each object's shadow, nothing between them.
        let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(80.0, 40.0));
        let layer = doc.pages[0].layers[0].id;
        let a = rect(1, Rect::new(5.0, 10.0, 10.0, 20.0), Color::rgb8(0, 0, 255));
        let b = rect(2, Rect::new(30.0, 10.0, 35.0, 20.0), Color::rgb8(0, 0, 255));
        let mut group = Shape::new(
            tracedraw_core::ShapeId(3),
            ShapeKind::Group {
                children: vec![a, b],
            },
        );
        group.shadow = Some(shadow);
        doc.layer_mut(layer).expect("layer").shapes.push(group);
        let img = render_page_image(&doc, doc.pages[0].id, 25.4).expect("image");
        assert!(dark(&img, 22, 15));
        assert!(dark(&img, 47, 15));
        assert!(white(&img, 40, 15));
        assert!(white(&img, 15, 15));
    }

    /// A clip frame with a drop shadow, its frame off whole pixels, draws
    /// as the plain object it shows with the same shadow: no thin line of
    /// the frame's fill or of the shadow along its edge, and nothing past
    /// the shadow. A shadow the frame had of its own goes once, beneath
    /// the contents, not a second time over them (live test of 0.41).
    #[test]
    fn a_clip_frame_with_a_shadow_draws_no_line_at_its_edge() {
        use tracedraw_core::live::MergeMode;
        let soft = tracedraw_core::Shadow::default();
        let sharp = tracedraw_core::Shadow {
            offset: tracedraw_core::geometry::Vec2::new(3.0, -3.0),
            opacity: 1.0,
            blur: 0.0,
            merge: MergeMode::Normal,
            ..soft
        };
        let blue = Color::rgb8(0, 0, 255);
        let box_ = Rect::new(20.06, 20.06, 40.06, 40.06);
        let render = |s: Shape| {
            let (doc, page) = doc_with(vec![s]);
            render_page_image(&doc, page, 25.4 * 8.0).expect("image")
        };
        let worst = |a: &Pixmap, b: &Pixmap| {
            let mut worst = (0i32, 0u32, 0u32);
            for y in 0..a.height() {
                for x in 0..a.width() {
                    let (p, q) = (px(a, x, y), px(b, x, y));
                    let d = [(p.0, q.0), (p.1, q.1), (p.2, q.2)]
                        .iter()
                        .map(|(u, v)| (*u as i32 - *v as i32).abs())
                        .max()
                        .unwrap_or(0);
                    if d > worst.0 {
                        worst = (d, x, y);
                    }
                }
            }
            worst
        };
        for fill in [
            Fill::Solid(Color::rgb8(255, 0, 0)),
            Fill::Solid(Color::WHITE),
            Fill::None,
        ] {
            for (k, sh) in [soft, sharp].into_iter().enumerate() {
                for on_frame in [false, true] {
                    let mut frame = rect(1, box_, Color::WHITE);
                    frame.fill = fill.clone();
                    let inside = rect(2, Rect::new(10.0, 10.0, 50.0, 50.0), blue);
                    if on_frame {
                        frame.shadow = Some(sh);
                    }
                    let mut clip = Shape::new(
                        ShapeId(3),
                        ShapeKind::ClipFrame {
                            frame: Box::new(frame),
                            contents: vec![inside],
                        },
                    );
                    if !on_frame {
                        clip.shadow = Some(sh);
                    }
                    let mut plain = rect(4, box_, blue);
                    plain.shadow = Some(sh);
                    let (d, x, y) = worst(&render(clip), &render(plain));
                    assert!(
                        d <= 3,
                        "fill {fill:?}, shadow {k}, on frame {on_frame}: off by {d} at {x}, {y}"
                    );
                }
            }
        }
    }

    /// A native ellipse or star casts the same shadow, flat or in
    /// perspective, as the same outline converted to curves: its own
    /// outline, not its box (#1323).
    #[test]
    fn native_shapes_cast_the_shadow_of_their_outline() {
        use tracedraw_core::shadow::{Perspective, ShadowEdge};
        let flat = tracedraw_core::Shadow {
            offset: tracedraw_core::geometry::Vec2::new(6.0, -6.0),
            opacity: 1.0,
            blur: 0.0,
            color: Color::BLACK,
            ..Default::default()
        };
        let persp = tracedraw_core::Shadow {
            perspective: Some(Perspective::new(ShadowEdge::Bottom)),
            ..flat
        };
        let kinds = [
            ShapeKind::Ellipse {
                rect: Rect::new(8.0, 12.0, 28.0, 32.0),
                arc: None,
            },
            ShapeKind::Polygon {
                rect: Rect::new(8.0, 12.0, 28.0, 32.0),
                points: 5,
                sharpness: 0.5,
                complex: None,
                twist: 0.0,
                bend: 0.0,
            },
        ];
        for kind in kinds {
            for sh in [flat, persp] {
                let scene = |curves: bool| {
                    let mut doc =
                        Document::new("t", tracedraw_core::geometry::Size::new(40.0, 40.0));
                    let layer = doc.pages[0].layers[0].id;
                    let mut s = Shape::new(doc.ids_mut().shape(), kind.clone());
                    s.transform = Affine::rotate_about(0.3, Point::new(18.0, 22.0));
                    if curves {
                        s.kind = ShapeKind::Path {
                            path: s.local_path(),
                            closed: true,
                        };
                    }
                    s.fill = Fill::Solid(Color::rgb8(0, 0, 255));
                    s.stroke = None;
                    s.shadow = Some(sh);
                    doc.layer_mut(layer).unwrap().shapes.push(s);
                    let page = doc.pages[0].id;
                    render_page_image(&doc, page, 25.4).unwrap()
                };
                let (native, curves) = (scene(false), scene(true));
                let differ = native
                    .pixels()
                    .iter()
                    .zip(curves.pixels())
                    .filter(|(a, b)| {
                        (i32::from(a.red()) - i32::from(b.red())).abs() > 8
                            || (i32::from(a.blue()) - i32::from(b.blue())).abs() > 8
                    })
                    .count();
                assert!(
                    differ < 4,
                    "{kind:?} {:?}: {differ} pixels differ",
                    sh.perspective
                );
            }
        }
    }

    /// The shadow is of what the object shows: an ellipse with no fill and
    /// a wide variable outline casts the ring its outline draws, its hole
    /// clear and its outer edge whole past the ellipse's own box, the same
    /// as after Convert to Curves. It cast the whole filled ellipse; only
    /// the shadow of its outline converted to an object was right.
    #[test]
    fn an_unfilled_object_casts_the_shadow_of_its_outline() {
        use tracedraw_core::style::WidthNode;
        let sh = tracedraw_core::Shadow {
            offset: tracedraw_core::geometry::Vec2::new(100.0, 0.0),
            opacity: 1.0,
            blur: 0.0,
            color: Color::BLACK,
            merge: tracedraw_core::live::MergeMode::Normal,
            ..Default::default()
        };
        let scene = |curves: bool, fill: Fill| {
            let mut s = Shape::new(
                tracedraw_core::ShapeId(1),
                ShapeKind::Ellipse {
                    rect: Rect::new(30.0, 30.0, 70.0, 70.0),
                    arc: None,
                },
            );
            if curves {
                s.kind = ShapeKind::Path {
                    path: s.local_path(),
                    closed: true,
                };
            }
            s.fill = fill;
            let mut st = tracedraw_core::Stroke::new(Color::BLACK, 1.0);
            // 8 mm each way at the bottom, tapering to 0.5 mm at the top.
            st.widths = vec![WidthNode::symmetric(0.75, 16.0)];
            s.stroke = Some(st);
            s.shadow = Some(sh);
            let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(200.0, 100.0));
            let layer = doc.pages[0].layers[0].id;
            doc.layer_mut(layer).unwrap().shapes.push(s);
            let page = doc.pages[0].id;
            render_page_image(&doc, page, 25.4).unwrap()
        };
        // One pixel a millimetre, y down from the page's top.
        let dark = |pm: &Pixmap, x: f64, y: f64| {
            let px = pm.pixel(x as u32, (100.0 - y) as u32).unwrap();
            px.red() < 128
        };
        for curves in [false, true] {
            let pm = scene(curves, Fill::None);
            // The shadow's middle (the hole) is clear, its ring dark.
            assert!(!dark(&pm, 150.5, 50.5), "curves {curves}: hole");
            assert!(dark(&pm, 150.5, 25.5), "curves {curves}: wide bottom");
            // Past the ellipse's box (30 to 70, moved 100): the outline's
            // reach at the bottom, 4 mm out, is not cut off.
            assert!(dark(&pm, 150.5, 27.5), "curves {curves}: outer edge");
            // A filled one stays whole.
            let pm = scene(curves, Fill::Solid(Color::WHITE));
            assert!(dark(&pm, 150.5, 50.5), "curves {curves}: filled");
        }
        let (a, b) = (scene(false, Fill::None), scene(true, Fill::None));
        let differ = a
            .pixels()
            .iter()
            .zip(b.pixels())
            .filter(|(p, q)| (i32::from(p.red()) - i32::from(q.red())).abs() > 8)
            .count();
        assert!(differ < 8, "{differ} pixels differ");
    }

    /// #2505: a square with no fill and a thick black outline, given a
    /// soft drop shadow (opacity 63 %, feathering 1.5 mm, offset 2 mm
    /// right and 2 mm down, Multiply): the shadow follows the outline and
    /// the inside stays clear. A filled square beside it shadows its
    /// whole inside.
    #[test]
    fn an_unfilled_square_shadows_only_its_outline() {
        let sh = tracedraw_core::Shadow {
            offset: tracedraw_core::geometry::Vec2::new(2.0, -2.0),
            opacity: 0.63,
            blur: 1.5,
            color: Color::BLACK,
            merge: tracedraw_core::live::MergeMode::Multiply,
            ..Default::default()
        };
        let square = |id: u64, x: f64, fill: Fill| {
            let mut s = Shape::new(
                tracedraw_core::ShapeId(id),
                ShapeKind::Rect {
                    rect: Rect::new(x, 20.0, x + 40.0, 60.0),
                    radius: 0.0,
                    corners: None,
                },
            );
            s.fill = fill;
            s.stroke = Some(tracedraw_core::Stroke::new(Color::BLACK, 2.0));
            s.shadow = Some(sh);
            s
        };
        let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(140.0, 80.0));
        let layer = doc.pages[0].layers[0].id;
        let shapes = &mut doc.layer_mut(layer).unwrap().shapes;
        shapes.push(square(1, 10.0, Fill::None));
        shapes.push(square(2, 80.0, Fill::Solid(Color::rgb8(190, 225, 245))));
        let page = doc.pages[0].id;
        // Two pixels a millimetre, y down from the page's top.
        let pm = render_page_image(&doc, page, 50.8).unwrap();
        let red = |x: f64, y: f64| {
            pm.pixel((x * 2.0) as u32, ((80.0 - y) * 2.0) as u32)
                .unwrap()
                .red()
        };
        // Inside the unfilled square, where its shadow's inside falls:
        // the white page.
        assert!(
            red(32.0, 38.0) > 245,
            "shadow over the inside: {}",
            red(32.0, 38.0)
        );
        // Its shadow beside the outline, right of the right side and
        // below the bottom.
        assert!(
            red(51.8, 30.0) < 200,
            "no shadow by the outline: {}",
            red(51.8, 30.0)
        );
        assert!(
            red(35.0, 18.2) < 200,
            "no shadow below: {}",
            red(35.0, 18.2)
        );
        // The filled square's shadow is under all of it, seen past its
        // right side.
        assert!(red(121.8, 30.0) < 200, "the filled square's shadow");
    }

    /// A blue square at 20 to 40 mm with a body running 10 mm up and to
    /// the right and a flat, sharp black shadow.
    fn extruded_square(offset: (f64, f64), turn: [f64; 3]) -> Shape {
        let mut s = Shape::new(
            tracedraw_core::ShapeId(1),
            ShapeKind::Rect {
                rect: Rect::new(20.0, 20.0, 40.0, 40.0),
                radius: 0.0,
                corners: None,
            },
        );
        s.fill = Fill::Solid(Color::rgb8(0, 0, 255));
        s.stroke = None;
        s.effects = vec![tracedraw_core::live::Effect::Extrude {
            depth: tracedraw_core::geometry::Vec2::new(10.0, 10.0),
            vanishing: None,
            amount: 0.0,
            shade_from: None,
            shade_to: None,
            light_angle: 135.0,
            light_intensity: 50.0,
            bevel: 0.0,
            kind: None,
            on_object: false,
            solid: None,
            space: tracedraw_core::extrude3d::Solid {
                rotation: turn,
                ..Default::default()
            },
        }];
        s.shadow = Some(tracedraw_core::Shadow {
            offset: tracedraw_core::geometry::Vec2::new(offset.0, offset.1),
            opacity: 1.0,
            blur: 0.0,
            color: Color::BLACK,
            merge: tracedraw_core::live::MergeMode::Normal,
            ..Default::default()
        });
        s
    }

    /// d#2596: the picture of a drop shadow for PDF output follows a
    /// picture's pixels, as on the page (#2027). A picture with a
    /// transparent background cast the shadow of its whole box, a grey
    /// slab under a cut-out drawing.
    #[test]
    fn the_shadow_picture_of_a_picture_follows_its_pixels() {
        // 10 by 10 pixels, opaque only in the left half.
        let mut src = Pixmap::new(10, 10).unwrap();
        let mut paint = Paint::default();
        paint.set_color_rgba8(255, 0, 0, 255);
        src.fill_rect(
            tiny_skia::Rect::from_xywh(0.0, 0.0, 5.0, 10.0).unwrap(),
            &paint,
            Transform::identity(),
            None,
        );
        let mut s = Shape::new(
            tracedraw_core::ShapeId(1),
            ShapeKind::Bitmap {
                rect: Rect::new(0.0, 0.0, 20.0, 20.0),
                width_px: 10,
                height_px: 10,
                png: src.encode_png().unwrap().into(),
                fx: None,
            },
        );
        s.fill = Fill::None;
        s.stroke = None;
        s.shadow = Some(tracedraw_core::Shadow {
            offset: tracedraw_core::geometry::Vec2::new(0.0, 0.0),
            opacity: 1.0,
            blur: 0.0,
            color: Color::BLACK,
            merge: tracedraw_core::live::MergeMode::Normal,
            ..Default::default()
        });
        let (area, pm) = render_shadow_image(&s, 25.4).expect("picture");
        let at = |x: f64, y: f64| {
            pm.pixel((x - area.x0) as u32, (area.y1 - y) as u32)
                .map_or(0, |p| p.alpha())
        };
        assert_eq!(at(5.0, 10.0), 255, "under the opaque half");
        assert_eq!(at(15.0, 10.0), 0, "none under the transparent half");
    }

    /// #2215: an extruded object casts the shadow of the object and its
    /// body together, beneath both. The shadow was the object's alone and
    /// was drawn over the body behind it, a dark patch on the body's
    /// faces; a body turned in space cast none.
    #[test]
    fn an_extruded_object_casts_the_shadow_of_its_body_beneath_it() {
        let black = (0, 0, 0);
        // The shadow 4 mm to the right: the square's own would cover the
        // body's right face up to x = 44.
        let s = extruded_square((4.0, 0.0), [0.0; 3]);
        let (doc, page) = doc_with(vec![s.clone()]);
        let pm = render_page_image(&doc, page, 25.4).unwrap();
        // Image rows run down: page y = 100 - row.
        let (r, g, b) = px(&pm, 42, 68);
        assert!(b > 60 && r < 40 && g < 40, "the body's face: {r} {g} {b}");
        assert_eq!(px(&pm, 30, 70), (0, 0, 255), "the object");
        // Beyond the body (it ends at x = 50) lies the body's shadow.
        assert_eq!(px(&pm, 52, 60), black, "the body's shadow");
        assert_eq!(px(&pm, 56, 60), (255, 255, 255));
        // The picture of the shadow has the body in it.
        let (area, alone) = render_shadow_image(&s, 25.4).expect("picture");
        assert!(
            (area.x1 - 54.0).abs() < 1.5 && (area.y1 - 50.0).abs() < 1.5,
            "{area:?}"
        );
        assert!(
            (area.x0 - 24.0).abs() < 1.5 && (area.y0 - 20.0).abs() < 1.5,
            "{area:?}"
        );
        let at = |x: f64, y: f64| {
            alone
                .pixel((x - area.x0) as u32, (area.y1 - y) as u32)
                .map_or(0, |p| p.alpha())
        };
        assert_eq!(at(52.5, 45.5), 255, "the body's far corner");
        assert_eq!(at(26.5, 45.5), 0, "above the object, left of the body");
        // In a group that has the shadow the body casts it too.
        let mut child = s.clone();
        let shadow = child.shadow.take();
        let mut group = Shape::new(
            tracedraw_core::ShapeId(2),
            ShapeKind::Group {
                children: vec![child],
            },
        );
        group.shadow = shadow;
        let (doc, page) = doc_with(vec![group]);
        let pm = render_page_image(&doc, page, 25.4).unwrap();
        assert_eq!(px(&pm, 52, 60), black, "a group's shadow");
        // Turned in space the body casts a shadow at all.
        let turned = extruded_square((40.0, 0.0), [0.0, 30.0, 0.0]);
        let (doc, page) = doc_with(vec![turned]);
        let pm = render_page_image(&doc, page, 25.4).unwrap();
        assert_eq!(px(&pm, 70, 70), black, "a turned body's shadow");
    }

    /// The picture of the shadow of an enveloped object (Break Drop
    /// Shadow Apart, PDF and `.cdr` output) is of the outline the envelope
    /// draws, where the page shows the shadow; it was of the outline
    /// without the envelope (#2217).
    #[test]
    fn the_shadow_picture_of_an_enveloped_object_is_of_what_is_drawn() {
        let mut s = extruded_square((40.0, 0.0), [0.0; 3]);
        // The envelope stretches the square to twice its height.
        s.effects = vec![tracedraw_core::live::Effect::envelope_around(
            Rect::new(20.0, 20.0, 40.0, 60.0),
            false,
        )];
        let (area, pm) = render_shadow_image(&s, 25.4).expect("picture");
        assert!(
            (area.x0 - 60.0).abs() < 1.5 && (area.x1 - 80.0).abs() < 1.5,
            "{area:?}"
        );
        assert!(
            (area.y0 - 20.0).abs() < 1.5 && (area.y1 - 60.0).abs() < 1.5,
            "{area:?}"
        );
        assert_eq!(pm.pixel(10, 3).map(|p| p.alpha()), Some(255));
        // The page shows it there: beside the drawn top, page y = 55.
        let (doc, page) = doc_with(vec![s]);
        let pm = render_page_image(&doc, page, 25.4).unwrap();
        assert_eq!(px(&pm, 70, 45), (0, 0, 0));
        assert_eq!(px(&pm, 30, 45), (0, 0, 255));
    }

    /// A ring as a user makes one, with a flat black shadow 30 mm to the
    /// right: two circles combined, or one curve whose inner circle runs
    /// the same way round as the outer one (`reversed` false) or the
    /// other way, filled by `rule`.
    fn shadowed_ring(
        combined: bool,
        reversed: bool,
        rule: tracedraw_core::shaping::FillRule,
    ) -> Shape {
        let circle = |r: f64| tracedraw_core::geometry::Circle::new((30.0, 50.0), r).to_path(0.01);
        let mut ring = if combined {
            let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(100.0, 100.0));
            let layer = doc.pages[0].layers[0].id;
            let mut ids = Vec::new();
            for r in [20.0, 10.0] {
                let mut s = Shape::new(
                    doc.ids_mut().shape(),
                    ShapeKind::Ellipse {
                        rect: Rect::new(30.0 - r, 50.0 - r, 30.0 + r, 50.0 + r),
                        arc: None,
                    },
                );
                s.fill = Fill::Solid(Color::rgb8(0, 0, 255));
                ids.push(s.id);
                tracedraw_core::Command::AddShape { layer, shape: s }
                    .apply(&mut doc)
                    .unwrap();
            }
            tracedraw_core::Command::Combine { shapes: ids }
                .apply(&mut doc)
                .unwrap();
            doc.layer(layer).unwrap().shapes[0].clone()
        } else {
            let mut path = circle(20.0);
            let inner = circle(10.0);
            path.extend(if reversed {
                inner.reverse_subpaths()
            } else {
                inner
            });
            Shape::new(ShapeId(1), ShapeKind::Path { path, closed: true })
        };
        ring.fill = Fill::Solid(Color::rgb8(0, 0, 255));
        ring.stroke = None;
        ring.fill_rule = rule;
        ring.shadow = Some(tracedraw_core::Shadow {
            offset: tracedraw_core::geometry::Vec2::new(30.0, 0.0),
            opacity: 1.0,
            blur: 0.0,
            color: Color::BLACK,
            ..Default::default()
        });
        ring
    }

    /// The drop shadow of an object with a hole has the hole too (#2216:
    /// it was filled by the nonzero rule whatever the object's own, so
    /// a ring of two circles combined cast a solid disc). It is open
    /// wherever the object is: on the page, alone as a picture (Break
    /// Drop Shadow Apart, PDF and `.cdr` output) and as part of a
    /// group's shadow. A curve set to the nonzero rule whose inner
    /// circle runs the same way round has no hole, and neither has its
    /// shadow.
    #[test]
    fn a_drop_shadow_keeps_the_holes_of_its_object() {
        use tracedraw_core::shaping::FillRule as Rule;
        let white = (255, 255, 255);
        let cases = [
            ("combined", true, false, Rule::EvenOdd),
            ("same way, even-odd", false, false, Rule::EvenOdd),
            ("reversed, even-odd", false, true, Rule::EvenOdd),
            ("reversed, nonzero", false, true, Rule::NonZero),
            ("same way, nonzero", false, false, Rule::NonZero),
        ];
        let mut failed = Vec::new();
        for (name, combined, reversed, rule) in cases {
            let ring = shadowed_ring(combined, reversed, rule);
            let (doc, page) = doc_with(vec![ring.clone()]);
            let pm = render_page_image(&doc, page, 25.4).expect("page");
            // The object itself says whether it has a hole.
            let hole = px(&pm, 30, 50) == white;
            assert_eq!(hole, !(rule == Rule::NonZero && !reversed), "{name}");
            assert_eq!(px(&pm, 15, 50), (0, 0, 255), "{name}: the ring");
            assert_eq!(px(&pm, 75, 50), (0, 0, 0), "{name}: the shadow's ring");
            // On the page: the shadow's middle, 30 mm right of the hole.
            if (px(&pm, 60, 50) == white) != hole {
                failed.push(format!("{name}: on the page"));
            }
            // Alone as a picture.
            let (area, alone) = render_shadow_image(&ring, 25.4).expect("picture");
            let at = |x: f64| (x - area.x0).floor() as u32;
            let y = (area.y1 - 50.0).floor() as u32;
            assert_eq!(alone.pixel(at(75.0), y).unwrap().alpha(), 255, "{name}");
            if (alone.pixel(at(60.0), y).unwrap().alpha() == 0) != hole {
                failed.push(format!("{name}: as a picture"));
            }
            // In a group that has the shadow.
            let mut child = ring.clone();
            let shadow = child.shadow.take();
            let mut group = Shape::new(
                ShapeId(9),
                ShapeKind::Group {
                    children: vec![child],
                },
            );
            group.shadow = shadow;
            let (doc, page) = doc_with(vec![group]);
            let pm = render_page_image(&doc, page, 25.4).expect("page");
            assert_eq!(px(&pm, 75, 50), (0, 0, 0), "{name}: the group's shadow");
            if (px(&pm, 60, 50) == white) != hole {
                failed.push(format!("{name}: in a group"));
            }
        }
        assert!(failed.is_empty(), "shadow and object differ: {failed:#?}");
    }

    /// A 40 x 40 mm page with a red square under a blue square that has
    /// `shadow`; rendered at 1 px per mm.
    fn shadow_scene(shadow: tracedraw_core::Shadow) -> Pixmap {
        let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(40.0, 40.0));
        let layer = doc.pages[0].layers[0].id;
        let mut under = rect(
            doc.ids_mut().shape().0,
            Rect::new(0.0, 0.0, 40.0, 40.0),
            Color::rgb8(255, 0, 0),
        );
        under.stroke = None;
        let mut s = rect(
            doc.ids_mut().shape().0,
            Rect::new(10.0, 20.0, 20.0, 30.0),
            Color::rgb8(0, 0, 255),
        );
        s.stroke = None;
        s.shadow = Some(shadow);
        let l = doc.layer_mut(layer).unwrap();
        l.shapes.push(under);
        l.shapes.push(s);
        let page = doc.pages[0].id;
        render_page_image(&doc, page, 25.4).unwrap()
    }

    #[test]
    fn shadow_merge_mode_multiplies_with_what_is_beneath() {
        let base = tracedraw_core::Shadow {
            offset: tracedraw_core::geometry::Vec2::new(5.0, -5.0),
            opacity: 1.0,
            blur: 0.0,
            color: Color::rgb8(0, 255, 0),
            ..Default::default()
        };
        // Normal: green over red is green.
        let normal = shadow_scene(tracedraw_core::Shadow {
            merge: tracedraw_core::live::MergeMode::Normal,
            ..base
        });
        // Multiply (the default): green times red is black.
        let multiply = shadow_scene(base);
        // (22, 22) in the image is inside the shadow, outside the object.
        let n = normal.pixel(22, 22).unwrap();
        let m = multiply.pixel(22, 22).unwrap();
        assert!(n.green() > 200 && n.red() < 50, "{n:?}");
        assert!(m.green() < 50 && m.red() < 50, "{m:?}");
    }

    #[test]
    fn perspective_shadow_falls_on_the_floor_and_fades() {
        use tracedraw_core::shadow::{Perspective, ShadowEdge};
        let mut sh = tracedraw_core::Shadow {
            opacity: 1.0,
            blur: 0.0,
            color: Color::BLACK,
            merge: tracedraw_core::live::MergeMode::Normal,
            perspective: Some(Perspective::new(ShadowEdge::Bottom)),
            ..Default::default()
        };
        // The square's base at y 20 stays; its top lands at (38, 22).
        let b = Rect::new(10.0, 20.0, 20.0, 30.0);
        sh.set_end_point(b, tracedraw_core::geometry::Point::new(38.0, 22.0));
        if let Some(p) = sh.perspective.as_mut() {
            p.fade = 0.8;
        }
        let img = shadow_scene(sh);
        // Image y = 40 - page y. Near the base, right of the square: dark.
        let near = img.pixel(22, 19).unwrap();
        // Far along the shadow: lighter (faded), still darker than the red.
        let far = img.pixel(33, 18).unwrap();
        assert!(near.red() < 120, "{near:?}");
        assert!(far.red() > near.red(), "{far:?} {near:?}");
        // Above the square's top where a flat shadow would not be: red.
        let above = img.pixel(15, 5).unwrap();
        assert_eq!((above.red(), above.green()), (255, 0));
    }

    #[test]
    fn shadow_image_holds_the_shadow_alone() {
        let mut s = rect(1, Rect::new(10.0, 10.0, 20.0, 20.0), Color::rgb8(0, 0, 255));
        assert!(render_shadow_image(&s, 100.0).is_none());
        s.shadow = Some(tracedraw_core::Shadow {
            offset: tracedraw_core::geometry::Vec2::new(5.0, -5.0),
            blur: 1.0,
            ..Default::default()
        });
        let (r, pm) = render_shadow_image(&s, 100.0).expect("image");
        assert!(
            r.x0 < 15.0 && r.x1 > 25.0 && r.y0 < 5.0 && r.y1 > 15.0,
            "{r:?}"
        );
        let zoom = 100.0 / 25.4;
        assert!((pm.width() as f64 - r.width() * zoom).abs() < 1.0);
        // The middle of the shadow is half opaque black.
        let cx = ((20.0 - r.x0) * zoom) as u32;
        let cy = ((r.y1 - 10.0) * zoom) as u32;
        let c = pm.pixel(cx, cy).unwrap().demultiply();
        assert!((c.alpha() as i32 - 128).abs() < 10, "{c:?}");
        assert!(render_shadow_image(&s, f64::NAN).is_none());
    }

    #[test]
    fn inside_outlines_stay_inside_the_fill() {
        let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(40.0, 40.0));
        let layer = doc.pages[0].layers[0].id;
        let mut s = rect(
            doc.ids_mut().shape().0,
            Rect::new(10.0, 10.0, 30.0, 30.0),
            Color::rgb8(0, 0, 255),
        );
        let mut st = Stroke::new(Color::BLACK, 4.0);
        st.position = tracedraw_core::OutlinePosition::Inside;
        s.stroke = Some(st.clone());
        doc.layer_mut(layer).unwrap().shapes.push(s.clone());
        let page = doc.pages[0].id;
        let img = render_page_image(&doc, page, 25.4).unwrap();
        // 1 mm outside the edge: white; 1 mm and 3 mm inside: black.
        assert_eq!(img.pixel(8, 20).unwrap().red(), 255);
        assert!(img.pixel(11, 20).unwrap().red() < 60);
        assert!(img.pixel(13, 20).unwrap().red() < 60);
        // 5 mm inside: the blue fill.
        assert!(img.pixel(15, 20).unwrap().blue() > 200);
        // Outside: the same width beyond the edge, the fill untouched.
        st.position = tracedraw_core::OutlinePosition::Outside;
        if let Some(sh) = doc.find_shape_mut(s.id) {
            sh.stroke = Some(st);
        }
        let img = render_page_image(&doc, page, 25.4).unwrap();
        assert!(img.pixel(7, 20).unwrap().red() < 60);
        assert!(img.pixel(11, 20).unwrap().blue() > 200);
        assert_eq!(img.pixel(5, 20).unwrap().red(), 255);
    }

    /// A dashed hairline is drawn dashed (#2463): along a hairline with
    /// the pattern 4 on, 4 off, some pixels are dark and some stay white.
    /// It was drawn solid.
    #[test]
    fn a_dashed_hairline_keeps_its_gaps() {
        let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(50.0, 50.0));
        let layer = doc.pages[0].layers[0].id;
        let id = doc.ids_mut().shape();
        let mut s = Shape::new(
            id,
            ShapeKind::Path {
                path: {
                    let mut p = BezPath::new();
                    p.move_to((5.0, 25.0));
                    p.line_to((45.0, 25.0));
                    p
                },
                closed: false,
            },
        );
        let mut st = Stroke::new(Color::BLACK, Stroke::HAIRLINE);
        st.dash = vec![4.0, 4.0];
        s.stroke = Some(st);
        doc.layer_mut(layer).unwrap().shapes.push(s);
        let page = doc.pages[0].id;
        // 4 pixels a millimetre: the hairline is one pixel wide and a unit
        // of the pattern one pixel.
        let img = render_page_image(&doc, page, 101.6).unwrap();
        let row: Vec<u8> = (30..170)
            .map(|x| {
                (99..102)
                    .map(|y| img.pixel(x, y).unwrap().red())
                    .min()
                    .unwrap()
            })
            .collect();
        let dark = row.iter().filter(|r| **r < 128).count();
        let white = row.iter().filter(|r| **r > 230).count();
        assert!(
            dark > 30 && white > 30,
            "{dark} dark, {white} white: {row:?}"
        );
    }

    /// #3404: round dots, dashes of no length that round caps draw: a
    /// row of separate dots two widths apart, centre to centre; with flat
    /// caps they would not show, so taking the pattern gives round caps.
    #[test]
    fn round_dots_draw_as_separate_dots() {
        let dots = |cap: Option<tracedraw_core::LineCap>| {
            let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(50.0, 50.0));
            let layer = doc.pages[0].layers[0].id;
            let id = doc.ids_mut().shape();
            let mut s = Shape::new(
                id,
                ShapeKind::Path {
                    path: {
                        let mut p = BezPath::new();
                        p.move_to((5.0, 25.0));
                        p.line_to((45.0, 25.0));
                        p
                    },
                    closed: false,
                },
            );
            let mut st = Stroke::new(Color::BLACK, 1.0);
            assert!(st.set_dash(&Stroke::ROUND_DOTS), "round caps come with the dots");
            assert_eq!(st.cap, tracedraw_core::LineCap::Round);
            if let Some(c) = cap {
                st.cap = c;
            }
            s.stroke = Some(st);
            doc.layer_mut(layer).unwrap().shapes.push(s);
            let page = doc.pages[0].id;
            // 4 pixels a millimetre: dots 4 pixels wide, 8 apart.
            let img = render_page_image(&doc, page, 101.6).unwrap();
            let row: Vec<bool> = (30..170)
                .map(|x| img.pixel(x, 100).unwrap().red() < 128)
                .collect();
            let runs = row.windows(2).filter(|w| !w[0] && w[1]).count();
            let longest = row
                .split(|d| !*d)
                .map(|r| r.len())
                .max()
                .unwrap_or(0);
            (runs, longest)
        };
        let (runs, longest) = dots(None);
        assert!((15..=19).contains(&runs), "{runs} dots");
        assert!((2..=6).contains(&longest), "a dot {longest} pixels long");
        let (runs, _) = dots(Some(tracedraw_core::LineCap::Butt));
        assert_eq!(runs, 0, "flat caps draw no dots");
    }

    #[test]
    fn dashed_hairline_does_not_panic() {
        let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(50.0, 50.0));
        let layer = doc.pages[0].layers[0].id;
        let id = doc.ids_mut().shape();
        let mut s = Shape::new(
            id,
            ShapeKind::Rect {
                rect: Rect::new(5.0, 5.0, 45.0, 45.0),
                radius: 0.0,
                corners: None,
            },
        );
        let mut st = Stroke::new(Color::BLACK, 1.0);
        st.dash = vec![3.0, 1.0, 1.0];
        s.stroke = Some(st);
        doc.layer_mut(layer).unwrap().shapes.push(s);
        let page = doc.pages[0].id;
        assert!(render_page_image(&doc, page, 96.0).is_some());
    }
}

#[cfg(test)]
mod culling_tests {
    use super::*;
    use tracedraw_core::document::{Shape, ShapeKind};

    /// A symmetry's mirrored copies show in every band of a picture drawn
    /// in bands (#3821): they keep the object's id, and the bands' shared
    /// box cache gave them the object's own box, so a band away from the
    /// object left them out until a zoom out drew the page in one band.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn mirrored_copies_show_in_bands_away_from_the_object() {
        let mut s = Shape::new(
            tracedraw_core::ShapeId(6),
            ShapeKind::Group {
                children: vec![rect_shape(7, Rect::new(10.0, 70.0, 30.0, 90.0))],
            },
        );
        s.effects = vec![tracedraw_core::live::Effect::Symmetry {
            center: tracedraw_core::geometry::Point::new(50.0, 50.0),
            angle: 0.0,
            lines: 1,
            rotate: false,
        }];
        let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(100.0, 100.0));
        let page = doc.pages[0].id;
        doc.pages[0].layers[0].shapes = vec![s];
        // The bottom half of the page only: the mirror of the square,
        // below the line, and not the square itself.
        let opts = RenderOptions {
            width: 100,
            height: 50,
            view: ViewTransform {
                zoom: 1.0,
                origin_x: 0.0,
                origin_y: 50.0,
                stretch_y: 1.0,
            },
            ..RenderOptions::default()
        };
        let boxes = page_bounds_cache(&doc, page);
        let whole = render_page_whole_in(&doc, page, &opts, None, None).expect("drawn");
        let band = render_page_whole_in(&doc, page, &opts, None, Some(&boxes)).expect("drawn");
        let ink = |pm: &Pixmap| pm.data().chunks(4).filter(|p| p[3] > 0).count();
        assert!(ink(&whole) > 100, "the mirror shows in the bottom half");
        assert_eq!(ink(&band), ink(&whole), "the band draws it too");
    }

    fn rect_shape(id: u64, r: Rect) -> Shape {
        let mut s = Shape::new(
            tracedraw_core::ShapeId(id),
            ShapeKind::Rect {
                rect: r,
                radius: 0.0,
                corners: None,
            },
        );
        s.fill = tracedraw_core::Fill::Solid(tracedraw_core::Color::rgb8(10, 20, 30));
        s
    }

    /// Zoomed in on a large group, only its members that show are drawn
    /// (discussion #2389): every member was drawn at the zoom's detail,
    /// so a zoom in on a grouped design drew far slower than the page
    /// fitted in the window. The picture is the same as with every
    /// member drawn, and a semi-transparent group leaves them out too.
    #[test]
    fn a_zoom_into_a_group_draws_only_the_members_that_show() {
        let members: Vec<Shape> = (0..400u64)
            .map(|i| {
                let (x, y) = ((i % 20) as f64 * 5.0, (i / 20) as f64 * 5.0);
                let mut r = rect_shape(10 + i, Rect::new(x, y, x + 4.0, y + 4.0));
                r.stroke = Some(tracedraw_core::Stroke::new(
                    tracedraw_core::Color::BLACK,
                    0.2,
                ));
                r
            })
            .collect();
        let group = |opacity: f64| {
            let mut g = Shape::new(
                tracedraw_core::ShapeId(1),
                ShapeKind::Group {
                    children: members.clone(),
                },
            );
            g.opacity = opacity;
            g
        };
        // 40 px per mm on a 200 x 200 window: page x 0..5, y 95..100,
        // about four members of the 400.
        let opts = RenderOptions {
            width: 200,
            height: 200,
            view: ViewTransform {
                zoom: 40.0,
                origin_x: 0.0,
                origin_y: 100.0 * 40.0,
                stretch_y: 1.0,
            },
            ..RenderOptions::default()
        };
        let doc_of = |shapes: Vec<Shape>| {
            let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(100.0, 100.0));
            let page = doc.pages[0].id;
            doc.pages[0].layers[0].shapes = shapes;
            (doc, page)
        };
        let drawn = |doc: &Document, page| {
            PATHS_DRAWN.with(|n| n.set(0));
            let pm = render_page_whole(doc, page, &opts, None).expect("render");
            (pm, PATHS_DRAWN.with(|n| n.get()))
        };
        let (doc, page) = doc_of(vec![group(1.0)]);
        let (grouped, n) = drawn(&doc, page);
        assert!(n > 0 && n <= 16, "{n} members drawn of 400");
        // The same members loose on the layer: the same picture.
        let (loose_doc, page) = doc_of(members.clone());
        let (loose, _) = drawn(&loose_doc, page);
        assert!(grouped.data() == loose.data(), "the picture changed");
        let (doc, page) = doc_of(vec![group(0.5)]);
        let (_, n) = drawn(&doc, page);
        assert!(
            n > 0 && n <= 16,
            "{n} members drawn of 400, half transparent"
        );
        // The whole group in sight: every member drawn.
        let whole = RenderOptions {
            view: ViewTransform {
                zoom: 2.0,
                origin_x: 0.0,
                origin_y: 200.0,
                stretch_y: 1.0,
            },
            ..opts.clone()
        };
        PATHS_DRAWN.with(|n| n.set(0));
        let (doc, page) = doc_of(vec![group(1.0)]);
        render_page_whole(&doc, page, &whole, None).expect("render");
        assert_eq!(PATHS_DRAWN.with(|n| n.get()), 400);
    }

    /// A clip frame's contents are put together on a layer the size of
    /// the frame's box, not of the whole canvas (#2522): the contents
    /// outside the box are left out as off screen, and the picture is
    /// the one the contents inside the frame give.
    #[test]
    fn a_clip_frame_puts_its_contents_together_on_its_own_box() {
        let contents: Vec<Shape> = (0..400u64)
            .map(|i| {
                let (x, y) = ((i % 20) as f64 * 5.0, (i / 20) as f64 * 5.0);
                let mut r = rect_shape(10 + i, Rect::new(x, y, x + 4.0, y + 4.0));
                r.fill = tracedraw_core::Fill::Solid(tracedraw_core::Color::rgb8(
                    (i * 7 % 256) as u8,
                    90,
                    (i * 13 % 256) as u8,
                ));
                r.stroke = Some(tracedraw_core::Stroke::new(
                    tracedraw_core::Color::BLACK,
                    0.3,
                ));
                r
            })
            .collect();
        let clip = |contents: Vec<Shape>| {
            // A round frame over page x 2..13, y 2..13, filled pale.
            let mut frame = Shape::new(
                tracedraw_core::ShapeId(2),
                ShapeKind::Ellipse {
                    rect: Rect::new(2.0, 2.0, 13.0, 13.0),
                    arc: None,
                },
            );
            frame.fill = tracedraw_core::Fill::Solid(tracedraw_core::Color::rgb8(250, 240, 200));
            frame.stroke = None;
            Shape::new(
                tracedraw_core::ShapeId(1),
                ShapeKind::ClipFrame {
                    frame: Box::new(frame),
                    contents,
                },
            )
        };
        let mut under = rect_shape(3, Rect::new(0.0, 0.0, 40.0, 40.0));
        under.fill = tracedraw_core::Fill::Solid(tracedraw_core::Color::rgb8(30, 160, 60));
        // The whole page on a 200 x 200 window, 2 px per mm.
        let opts = RenderOptions {
            width: 200,
            height: 200,
            view: ViewTransform {
                zoom: 2.0,
                origin_x: 0.0,
                origin_y: 200.0,
                stretch_y: 1.0,
            },
            ..RenderOptions::default()
        };
        let render = |contents: Vec<Shape>| {
            let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(100.0, 100.0));
            let page = doc.pages[0].id;
            doc.pages[0].layers[0].shapes = vec![under.clone(), clip(contents)];
            PATHS_DRAWN.with(|n| n.set(0));
            let pm = render_page_whole(&doc, page, &opts, None).expect("render");
            (pm, PATHS_DRAWN.with(|n| n.get()))
        };
        let (all, n) = render(contents.clone());
        // The frame's box (with a few pixels round it) takes in the
        // members at the page's corner: far fewer than the 400 (each a
        // fill and an outline), which were all drawn on a canvas-sized
        // layer.
        assert!(
            n <= 100,
            "{n} paths drawn for a frame over 9 of 400 members"
        );
        // Only the members over the frame: the same picture.
        let near: Vec<Shape> = contents
            .iter()
            .filter(|s| {
                let b = s.bounds();
                b.x0 < 15.0 && b.y0 < 15.0
            })
            .cloned()
            .collect();
        let (few, _) = render(near);
        assert!(all.data() == few.data(), "the picture changed");
        // Inside the frame the members show; outside, what lies beneath.
        let px = |pm: &Pixmap, x: f64, y: f64| {
            let p = pm
                .pixel((x * 2.0) as u32, (200.0 - y * 2.0) as u32)
                .expect("pixel");
            (p.red(), p.green(), p.blue())
        };
        assert_eq!(px(&all, 30.0, 30.0), (30, 160, 60));
        assert_eq!(
            px(&all, 2.5, 12.5),
            (30, 160, 60),
            "outside the round frame"
        );
        assert_ne!(
            px(&all, 7.0, 7.0),
            (30, 160, 60),
            "a member inside the frame"
        );
    }

    /// A render told to give up (the view zoomed on, #2522) gives no
    /// picture; one never told gives the picture of `render_page`.
    #[test]
    fn a_cancelled_render_gives_no_picture() {
        let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(100.0, 100.0));
        let page = doc.pages[0].id;
        doc.pages[0].layers[0].shapes = (0..50u64)
            .map(|i| rect_shape(i + 1, Rect::new(i as f64, 10.0, i as f64 + 1.0, 90.0)))
            .collect();
        let opts = RenderOptions {
            width: 600,
            height: 600,
            view: ViewTransform {
                zoom: 6.0,
                origin_x: 0.0,
                origin_y: 600.0,
                stretch_y: 1.0,
            },
            ..RenderOptions::default()
        };
        let go = std::sync::atomic::AtomicBool::new(false);
        let drawn = render_page_cancellable(&doc, page, &opts, &go).expect("drawn");
        let plain = render_page(&doc, page, &opts).expect("drawn");
        assert!(drawn.data() == plain.data());
        let stop = std::sync::atomic::AtomicBool::new(true);
        assert!(render_page_cancellable(&doc, page, &opts, &stop).is_none());
    }

    /// Objects wholly outside the part of the page on screen are not
    /// drawn (a page of thousands of objects pans at the cost of the
    /// visible ones); an outline, a shadow or an effect that may reach
    /// in keeps an object drawn.
    #[test]
    fn objects_out_of_sight_are_skipped() {
        let opts = RenderOptions {
            width: 400,
            height: 300,
            // 4 px per mm, the view's bottom-left at page (100, 100):
            // page x 100..200, y 100..175 on screen.
            view: ViewTransform {
                zoom: 4.0,
                origin_x: -400.0,
                origin_y: 300.0 + 400.0,
                stretch_y: 1.0,
            },
            ..RenderOptions::default()
        };
        let visible = visible_page_rect(&opts);
        assert!((visible.x0 - 100.0).abs() < 1e-9 && (visible.x1 - 200.0).abs() < 1e-9);
        assert!((visible.y0 - 100.0).abs() < 1e-9 && (visible.y1 - 175.0).abs() < 1e-9);
        let mut pixmap = Pixmap::new(400, 300).unwrap();
        let r = Renderer {
            pixmap: &mut pixmap,
            screen: opts.view.affine(),
            zoom: 4.0,
            flags: Flags::from_options(&opts),
            symbols: &[],
            preview: None,
            bounds: None,
            origin: (0, 0),
            cancel: None,
        };
        let inside = rect_shape(1, Rect::new(120.0, 120.0, 130.0, 130.0));
        let far = rect_shape(2, Rect::new(0.0, 0.0, 10.0, 10.0));
        let touching = rect_shape(3, Rect::new(90.0, 90.0, 100.5, 100.5));
        assert!(!r.out_of_sight(&inside, &visible));
        assert!(r.out_of_sight(&far, &visible));
        assert!(!r.out_of_sight(&touching, &visible));
        // Two millimetres short of the view, but a 1 mm outline reaches it.
        let mut near = rect_shape(4, Rect::new(80.0, 120.0, 97.0, 130.0));
        assert!(r.out_of_sight(&near, &visible));
        near.stroke = Some(tracedraw_core::Stroke::new(
            tracedraw_core::Color::BLACK,
            1.0,
        ));
        assert!(!r.out_of_sight(&near, &visible));
        // Scaled up five times: the outline reaches five times as far.
        let mut scaled = rect_shape(5, Rect::new(10.0, 25.0, 15.0, 27.0));
        scaled.transform = Affine::scale(5.0);
        scaled.stroke = Some(tracedraw_core::Stroke::new(
            tracedraw_core::Color::BLACK,
            0.5,
        ));
        assert!(!r.out_of_sight(&scaled, &visible), "{:?}", scaled.bounds());
        // A shadow may fall anywhere: drawn.
        let mut shadowed = rect_shape(6, Rect::new(0.0, 0.0, 10.0, 10.0));
        shadowed.shadow = Some(tracedraw_core::document::Shadow::default());
        assert!(!r.out_of_sight(&shadowed, &visible));
        // A thin outline with a Variable Length node reaching 8 mm: the
        // band reaches the view from 5 mm away, so it is drawn (#626).
        let mut path = BezPath::new();
        path.move_to((120.0, 95.0));
        path.line_to((150.0, 95.0));
        let mut thin = Shape::new(
            tracedraw_core::ShapeId(7),
            ShapeKind::Path {
                path,
                closed: false,
            },
        );
        let mut st = tracedraw_core::Stroke::new(tracedraw_core::Color::BLACK, 0.2);
        thin.stroke = Some(st.clone());
        assert!(r.out_of_sight(&thin, &visible));
        st.widths = vec![tracedraw_core::WidthNode::symmetric(0.5, 16.0)];
        thin.stroke = Some(st);
        assert!(!r.out_of_sight(&thin, &visible));
        // Hidden objects never draw.
        let mut hidden = inside.clone();
        hidden.visible = false;
        assert!(r.out_of_sight(&hidden, &visible));
        // Through the page render: the far object leaves no pixel.
        let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(300.0, 300.0));
        let page = doc.pages[0].id;
        let layer = doc.pages[0].layers[0].id;
        doc.layer_mut(layer).unwrap().shapes.push(far);
        let px = render_page(&doc, page, &opts).unwrap();
        assert!(px.pixels().iter().all(|p| p.alpha() == 0));
        doc.layer_mut(layer).unwrap().shapes.push(inside);
        let px = render_page(&doc, page, &opts).unwrap();
        assert!(px.pixels().iter().any(|p| p.alpha() > 0));
    }

    /// Focus at 0 % visibility draws the objects in focus and nothing
    /// else: no page colour, no other object (#981). Above 0 the rest
    /// still shows, faded.
    #[test]
    fn focus_at_no_visibility_draws_only_the_focus() {
        let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(100.0, 100.0));
        let page = doc.pages[0].id;
        let layer = doc.pages[0].layers[0].id;
        doc.pages[0].background = Some(tracedraw_core::Fill::Solid(tracedraw_core::Color::rgb8(
            200, 0, 0,
        )));
        let shapes = &mut doc.layer_mut(layer).unwrap().shapes;
        shapes.push(rect_shape(1, Rect::new(0.0, 0.0, 40.0, 100.0)));
        shapes.push(rect_shape(2, Rect::new(60.0, 0.0, 100.0, 100.0)));
        let opts = |v: f32| RenderOptions {
            width: 100,
            height: 100,
            view: ViewTransform {
                zoom: 1.0,
                origin_x: 0.0,
                origin_y: 100.0,
                stretch_y: 1.0,
            },
            focus: Some((vec![tracedraw_core::ShapeId(2)], v)),
            ..RenderOptions::default()
        };
        assert!(rest_hidden(&opts(0.0)));
        assert!(rest_hidden(&opts(f32::NAN)));
        assert!(!rest_hidden(&opts(0.2)));
        assert!(!rest_hidden(&RenderOptions::default()));
        let alpha = |pm: &Pixmap, x: u32| pm.pixel(x, 50).unwrap().alpha();
        let hidden = render_page(&doc, page, &opts(0.0)).unwrap();
        assert_eq!(
            alpha(&hidden, 20),
            0,
            "the object out of focus is not drawn"
        );
        assert_eq!(alpha(&hidden, 50), 0, "the page colour is not drawn");
        assert_eq!(alpha(&hidden, 80), 255, "the object in focus is drawn");
        let faded = render_page(&doc, page, &opts(0.5)).unwrap();
        assert!((120..=135).contains(&alpha(&faded, 20)));
        assert!((120..=135).contains(&alpha(&faded, 50)));
        assert_eq!(alpha(&faded, 80), 255);
    }
}

#[cfg(test)]
mod band_tests {
    use super::*;
    use tracedraw_core::document::{Shape, ShapeKind};
    use tracedraw_core::geometry::BezPath;

    /// A thin outline whose Variable Length node reaches far past its
    /// width is drawn whole when the page is drawn in strips: the part
    /// in the strips above and below the path is not left out (#626).
    #[test]
    fn a_variable_outline_is_drawn_whole_across_the_strips() {
        let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(200.0, 120.0));
        let page = doc.pages[0].id;
        let layer = doc.pages[0].layers[0].id;
        let mut path = BezPath::new();
        path.move_to((20.0, 45.0));
        path.line_to((180.0, 45.0));
        let mut s = Shape::new(
            tracedraw_core::ShapeId(1),
            ShapeKind::Path {
                path,
                closed: false,
            },
        );
        let mut st = tracedraw_core::Stroke::new(tracedraw_core::Color::BLACK, 0.2);
        // 40 mm across in the middle: 20 mm above and below the path.
        st.widths = vec![tracedraw_core::WidthNode::symmetric(0.5, 40.0)];
        s.stroke = Some(st);
        if let Ok(l) = doc.layer_mut(layer) {
            l.shapes.push(s);
        }
        // 5 px per mm: 1000 x 600 px, enough to be drawn in strips.
        let opts = RenderOptions {
            width: 1000,
            height: 600,
            view: ViewTransform {
                zoom: 5.0,
                origin_x: 0.0,
                origin_y: 600.0,
                stretch_y: 1.0,
            },
            ..RenderOptions::default()
        };
        let px = render_page(&doc, page, &opts).expect("render");
        let dark = |x: f64, y: f64| {
            let (col, row) = ((x * 5.0) as u32, (600.0 - y * 5.0) as u32);
            px.pixel(col, row).is_some_and(|p| p.alpha() > 200)
        };
        // At the middle, every row from 19 mm below the path to 19 mm
        // above it is inside the band, whichever strip it fell in.
        for k in 0..=38 {
            let y = 26.0 + k as f64;
            assert!(dark(100.0, y), "{y} mm");
        }
        assert!(!dark(100.0, 66.0) && !dark(100.0, 24.0));
    }

    /// Clip frames and groups moved by their own transform (the pages a
    /// printed sheet repeats, each a clip frame placed on the sheet) are
    /// drawn in every strip they reach: only the strip holding their
    /// untransformed frame drew them, so Print Preview showed one row of
    /// a repeated page and white above it (#848).
    #[test]
    fn moved_clip_frames_and_groups_are_drawn_in_every_strip() {
        let rect = |id: u64, r: Rect| {
            let mut s = Shape::new(
                tracedraw_core::ShapeId(id),
                ShapeKind::Rect {
                    rect: r,
                    radius: 0.0,
                    corners: None,
                },
            );
            s.fill = tracedraw_core::Fill::Solid(tracedraw_core::Color::BLACK);
            s.stroke = None;
            s
        };
        let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(100.0, 200.0));
        let page = doc.pages[0].id;
        let layer = doc.pages[0].layers[0].id;
        let mut shapes = Vec::new();
        // A 40 x 20 mm card repeated four times up the sheet, left.
        for row in 0..4u64 {
            let mut frame = rect(100 + row, Rect::new(0.0, 0.0, 40.0, 20.0));
            frame.fill = tracedraw_core::Fill::None;
            let mut clip = Shape::new(
                tracedraw_core::ShapeId(10 + row),
                ShapeKind::ClipFrame {
                    frame: Box::new(frame),
                    contents: vec![rect(200 + row, Rect::new(0.0, 0.0, 40.0, 20.0))],
                },
            );
            clip.transform = Affine::translate((5.0, 10.0 + 45.0 * row as f64));
            shapes.push(clip);
        }
        // A group moved up the sheet, right.
        let mut group = Shape::new(
            tracedraw_core::ShapeId(50),
            ShapeKind::Group {
                children: vec![rect(51, Rect::new(0.0, 0.0, 40.0, 20.0))],
            },
        );
        group.transform = Affine::translate((55.0, 160.0));
        shapes.push(group);
        if let Ok(l) = doc.layer_mut(layer) {
            l.shapes.extend(shapes);
        }
        // 5 px per mm: 500 x 1000 px, enough to be drawn in strips.
        let opts = RenderOptions {
            width: 500,
            height: 1000,
            view: ViewTransform {
                zoom: 5.0,
                origin_x: 0.0,
                origin_y: 1000.0,
                stretch_y: 1.0,
            },
            ..RenderOptions::default()
        };
        let visible = Rect::new(0.0, 100.0, 100.0, 200.0);
        let r_px = &mut Pixmap::new(1, 1).expect("pixmap");
        let r = Renderer {
            pixmap: r_px,
            screen: opts.view.affine(),
            zoom: opts.view.zoom,
            flags: Flags::from_options(&opts),
            symbols: &doc.symbols,
            preview: None,
            bounds: None,
            origin: (0, 0),
            cancel: None,
        };
        let drawn = &doc.pages[0].layers[0].shapes;
        assert!(r.out_of_sight(&drawn[0], &visible), "the bottom card");
        assert!(!r.out_of_sight(&drawn[3], &visible), "the top card");
        assert!(!r.out_of_sight(&drawn[4], &visible), "the group");
        let px = render_page(&doc, page, &opts).expect("render");
        let dark = |x: f64, y: f64| {
            let (col, row) = ((x * 5.0) as u32, (1000.0 - y * 5.0) as u32);
            px.pixel(col, row).is_some_and(|p| p.alpha() > 200)
        };
        for row in 0..4 {
            let y = 20.0 + 45.0 * row as f64;
            assert!(dark(25.0, y), "card {row} at {y} mm");
            assert!(!dark(25.0, y + 20.0), "gap above card {row}");
        }
        assert!(dark(75.0, 170.0), "the group");
    }

    /// The same inside a group, nested twice, and an ordinary 16 mm
    /// outline inside a group: the strips no longer cut the band flat at
    /// the group's bounds, which count no outline (#626).
    #[test]
    fn outlines_inside_groups_are_drawn_whole_across_the_strips() {
        let line = |id: u64, y: f64, stroke: tracedraw_core::Stroke| {
            let mut path = BezPath::new();
            path.move_to((20.0, y));
            path.line_to((180.0, y));
            let mut s = Shape::new(
                tracedraw_core::ShapeId(id),
                ShapeKind::Path {
                    path,
                    closed: false,
                },
            );
            s.stroke = Some(stroke);
            s
        };
        let mut st = tracedraw_core::Stroke::new(tracedraw_core::Color::BLACK, 0.2);
        st.widths = vec![tracedraw_core::WidthNode::symmetric(0.5, 40.0)];
        // Each crosses into the strip below its group's bounds.
        let variable = line(1, 40.0, st);
        let wide = line(
            2,
            95.0,
            tracedraw_core::Stroke::new(tracedraw_core::Color::BLACK, 16.0),
        );
        let group = |id: u64, children: Vec<Shape>| {
            let mut g = Shape::new(tracedraw_core::ShapeId(id), ShapeKind::Group { children });
            g.stroke = None;
            g
        };
        let nested = group(11, vec![group(10, vec![variable])]);
        let plain = group(12, vec![wide]);
        let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(200.0, 120.0));
        let page = doc.pages[0].id;
        let layer = doc.pages[0].layers[0].id;
        if let Ok(l) = doc.layer_mut(layer) {
            l.shapes.push(nested);
            l.shapes.push(plain);
        }
        let opts = RenderOptions {
            width: 1000,
            height: 600,
            view: ViewTransform {
                zoom: 5.0,
                origin_x: 0.0,
                origin_y: 600.0,
                stretch_y: 1.0,
            },
            ..RenderOptions::default()
        };
        let px = render_page(&doc, page, &opts).expect("render");
        let dark = |x: f64, y: f64| {
            let (col, row) = ((x * 5.0) as u32, (600.0 - y * 5.0) as u32);
            px.pixel(col, row).is_some_and(|p| p.alpha() > 200)
        };
        for k in 0..=38 {
            let y = 21.0 + k as f64;
            assert!(dark(100.0, y), "variable band at {y} mm");
        }
        assert!(!dark(100.0, 61.0) && !dark(100.0, 19.0));
        // The 16 mm outline: 8 mm either side of its path.
        for k in 0..=14 {
            let y = 88.0 + k as f64;
            assert!(dark(100.0, y), "wide outline at {y} mm");
        }
        assert!(!dark(100.0, 104.0) && !dark(100.0, 86.0));
    }

    /// A heavy page: `n` filled and stroked curves spread over A4.
    fn heavy(n: usize) -> (Document, PageId) {
        let mut doc = Document::default();
        let page = doc.pages[0].id;
        let layer = doc.layers_for_page(page).unwrap()[0].id;
        let mut s = 7u64;
        let mut r = || {
            s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((s >> 33) as f64) / (1u64 << 31) as f64
        };
        for i in 0..n {
            let (x, y) = (r() * 210.0, r() * 297.0);
            let mut p = BezPath::new();
            p.move_to((x, y));
            for _ in 0..6 {
                p.curve_to(
                    (x + r() * 20.0 - 10.0, y + r() * 20.0 - 10.0),
                    (x + r() * 20.0 - 10.0, y + r() * 20.0 - 10.0),
                    (x + r() * 20.0 - 10.0, y + r() * 20.0 - 10.0),
                );
            }
            p.close_path();
            let mut sh = Shape::new(
                tracedraw_core::ShapeId(1000 + i as u64),
                ShapeKind::Path {
                    path: p,
                    closed: true,
                },
            );
            sh.fill =
                tracedraw_core::Fill::Solid(tracedraw_core::Color::rgb8((i % 255) as u8, 80, 160));
            let _ = tracedraw_core::Command::AddShape { layer, shape: sh }.apply(&mut doc);
        }
        (doc, page)
    }

    /// Inside a clip frame an object with the Multiply merge mode
    /// multiplies what lies beneath the frame, as outside it (a design on
    /// a photo of a shirt keeps the cloth's shading).
    #[test]
    fn merge_modes_inside_a_clip_frame_reach_the_backdrop() {
        use tracedraw_core::live::{Effect, MergeMode};
        let mut doc = Document::default();
        let page = doc.pages[0].id;
        let layer = doc.layers_for_page(page).unwrap()[0].id;
        let rect = |id: u64, r: tracedraw_core::geometry::Rect, c: Color| {
            let mut s = Shape::new(
                tracedraw_core::ShapeId(id),
                ShapeKind::Rect {
                    rect: r,
                    radius: 0.0,
                    corners: None,
                },
            );
            s.fill = tracedraw_core::Fill::Solid(c);
            s.stroke = None;
            s
        };
        let area = tracedraw_core::geometry::Rect::new(0.0, 0.0, 40.0, 40.0);
        let back = rect(1, area, Color::rgb8(128, 128, 128));
        let mut red = rect(2, area, Color::rgb8(255, 0, 0));
        red.effects.push(Effect::Transparency {
            mask: tracedraw_core::Fill::Solid(Color::Gray { v: 1.0 }),
            merge: MergeMode::Multiply,
            target: 2,
        });
        let mut frame = rect(3, area, Color::WHITE);
        frame.fill = tracedraw_core::Fill::None;
        let clip = Shape::new(
            tracedraw_core::ShapeId(4),
            ShapeKind::ClipFrame {
                frame: Box::new(frame),
                contents: vec![red],
            },
        );
        for s in [back, clip] {
            let _ = tracedraw_core::Command::AddShape { layer, shape: s }.apply(&mut doc);
        }
        let opts = RenderOptions {
            width: 40,
            height: 40,
            view: ViewTransform {
                zoom: 1.0,
                origin_x: 0.0,
                origin_y: 40.0,
                stretch_y: 1.0,
            },
            ..RenderOptions::default()
        };
        let px = render_page(&doc, page, &opts).unwrap();
        let c = px.pixel(20, 20).unwrap().demultiply();
        assert!(
            (c.red() as i32 - 128).abs() <= 3 && c.green() <= 3,
            "red times grey: {c:?}"
        );
    }

    /// A picture shape of `w` x `h` pixels from `pm` over `rect`.
    fn picture(id: u64, pm: &Pixmap, rect: Rect) -> Shape {
        let mut s = Shape::new(
            tracedraw_core::ShapeId(id),
            ShapeKind::Bitmap {
                rect,
                width_px: pm.width(),
                height_px: pm.height(),
                png: pm.encode_png().unwrap().into(),
                fx: None,
            },
        );
        s.fill = Fill::None;
        s.stroke = None;
        s
    }

    /// #853: pictures laid side by side meet without a pale seam, at any
    /// zoom (their shared edge falls between pixels here).
    #[test]
    fn pictures_side_by_side_meet_without_a_seam() {
        let mut pm = Pixmap::new(16, 16).unwrap();
        pm.fill(tiny_skia::Color::from_rgba8(200, 0, 0, 255));
        let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(100.0, 100.0));
        let page = doc.pages[0].id;
        for (i, x) in [10.0, 30.3].into_iter().enumerate() {
            doc.pages[0].layers[0].shapes.push(picture(
                i as u64 + 1,
                &pm,
                Rect::new(x, 10.0, x + 20.3, 30.3),
            ));
        }
        for zoom in [1.0, 1.37, 2.9, 0.41] {
            let opts = RenderOptions {
                width: 300,
                height: 300,
                view: ViewTransform {
                    zoom,
                    origin_x: 0.3,
                    origin_y: 299.6,
                    stretch_y: 1.0,
                },
                anti_alias: true,
                ..RenderOptions::default()
            };
            let out = render_page_whole(&doc, page, &opts, None).unwrap();
            let y = (299.6 - 20.0 * zoom) as u32;
            let x0 = (12.0 * zoom) as u32;
            let x1 = (48.0 * zoom) as u32;
            for x in x0..=x1 {
                let p = out.pixel(x, y).unwrap();
                assert_eq!(
                    (p.red(), p.green(), p.alpha()),
                    (200, 0, 255),
                    "zoom {zoom}, x {x}"
                );
            }
        }
    }

    /// #853: a picture drawn far smaller than its pixels shows their
    /// average, not a few scattered pixels: a one-pixel checkerboard at
    /// about a tenth of its size is an even grey.
    #[test]
    fn a_picture_drawn_small_shows_the_average_of_its_pixels() {
        let mut pm = Pixmap::new(400, 400).unwrap();
        for (i, px) in pm.data_mut().chunks_mut(4).enumerate() {
            let (x, y) = (i % 400, i / 400);
            let v = if (x + y) % 2 == 0 { 255 } else { 0 };
            px.copy_from_slice(&[v, v, v, 255]);
        }
        let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(100.0, 100.0));
        let page = doc.pages[0].id;
        doc.pages[0].layers[0]
            .shapes
            .push(picture(1, &pm, Rect::new(0.0, 0.0, 100.0, 100.0)));
        // 400 pixels over 37: about a tenth of their size.
        let opts = RenderOptions {
            width: 37,
            height: 37,
            view: ViewTransform {
                zoom: 0.37,
                origin_x: 0.0,
                origin_y: 37.0,
                stretch_y: 1.0,
            },
            anti_alias: true,
            ..RenderOptions::default()
        };
        let out = render_page_whole(&doc, page, &opts, None).unwrap();
        for y in 2..35 {
            for x in 2..35 {
                let r = out.pixel(x, y).unwrap().red();
                assert!((110..=145).contains(&r), "({x}, {y}): {r}");
            }
        }
        assert_eq!(bitmap_level(Affine::scale(0.1)), 3);
        assert_eq!(bitmap_level(Affine::scale(0.5)), 0);
        assert_eq!(bitmap_level(Affine::scale(0.49)), 1);
        assert_eq!(bitmap_level(Affine::scale(2.0)), 0);
    }

    /// One picture cache serves every band: a picture across all bands of
    /// a render is decoded once, and the next render decodes nothing.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn bands_share_one_decode_of_a_picture() {
        let mut pm = Pixmap::new(37, 23).unwrap();
        pm.fill(tiny_skia::Color::from_rgba8(12, 200, 99, 255));
        let png = pm.encode_png().unwrap();
        let mut s = Shape::new(
            tracedraw_core::ShapeId(1),
            ShapeKind::Bitmap {
                rect: Rect::new(0.0, 0.0, 100.0, 100.0),
                width_px: 37,
                height_px: 23,
                png: png.clone().into(),
                fx: None,
            },
        );
        s.fill = Fill::None;
        s.stroke = None;
        let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(100.0, 100.0));
        let page = doc.pages[0].id;
        doc.pages[0].layers[0].shapes.push(s);
        let opts = RenderOptions {
            width: 800,
            height: 800,
            view: ViewTransform {
                zoom: 8.0,
                origin_x: 0.0,
                origin_y: 800.0,
                stretch_y: 1.0,
            },
            ..RenderOptions::default()
        };
        let Some(first) = render_page_in_bands(&doc, page, &opts, None) else {
            // One core: no bands.
            return;
        };
        let green = |y| first.pixel(400, y).map(|p| p.green());
        assert_eq!(green(30), Some(200));
        assert_eq!(green(770), Some(200));
        render_page_in_bands(&doc, page, &opts, None).unwrap();
        assert_eq!(image_cache::decodes_of(&png), 1);
    }

    /// The shadow of an object with a fountain transparency is the same
    /// drawn in bands as drawn whole. Each band faded it by its own cut
    /// of the transparency, whole in a band below the object and faint
    /// in the band holding it: a hard line across the shadow, which
    /// jumped from frame to frame while a shadow was dragged (the bands
    /// of a picture handed over at once and of one handed over a band at
    /// a time do not meet on the same rows), so every such shadow on
    /// screen flickered until the mouse was let go.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn a_faded_object_casts_the_same_shadow_in_bands() {
        let mut s = Shape::new(
            tracedraw_core::ShapeId(1),
            ShapeKind::Ellipse {
                rect: Rect::new(15.0, 55.0, 45.0, 85.0),
                arc: None,
            },
        );
        s.fill = Fill::Solid(Color::rgb8(240, 130, 30));
        s.effects.push(tracedraw_core::live::Effect::Transparency {
            mask: Fill::linear(Color::WHITE, Color::BLACK, -45.0),
            merge: tracedraw_core::live::MergeMode::Normal,
            target: 2,
        });
        s.shadow = Some(tracedraw_core::Shadow {
            offset: tracedraw_core::geometry::Vec2::new(15.0, -30.0),
            opacity: 0.8,
            blur: 2.0,
            color: Color::BLACK,
            ..Default::default()
        });
        let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(100.0, 100.0));
        let page = doc.pages[0].id;
        let layer = doc.pages[0].layers[0].id;
        doc.layer_mut(layer).unwrap().shapes.push(s);
        let opts = RenderOptions {
            width: 800,
            height: 800,
            view: ViewTransform {
                zoom: 8.0,
                origin_x: 0.0,
                origin_y: 800.0,
                stretch_y: 1.0,
            },
            anti_alias: true,
            ..RenderOptions::default()
        };
        let whole = render_page_whole(&doc, page, &opts, None).expect("drawn");
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let banded =
            render_page_progressive(&doc, page, &opts, &cancel, 400, &|_, _| ()).expect("drawn");
        // The shadow, beside and below the object (rows 120..360).
        let from = 370 * 800 * 4;
        let worst = banded.data()[from..]
            .iter()
            .zip(&whole.data()[from..])
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap_or(0);
        assert!(worst <= 2, "bands differ by {worst}");
    }

    /// A picture handed over a band at a time (#2389): the bands cover
    /// every row once, the middle row's comes among the first, the
    /// picture handed back is the bands put together, and it is the
    /// picture drawn at once (but for the few edge pixels of curves cut
    /// by a band's edge, as with [`render_page`]'s own bands). A render
    /// told to give up hands over no picture.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn a_picture_comes_a_band_at_a_time_from_the_middle() {
        let (doc, page) = heavy(60);
        let opts = RenderOptions {
            width: 900,
            height: 700,
            view: ViewTransform {
                zoom: 3.0,
                origin_x: 20.0,
                origin_y: 650.0,
                stretch_y: 1.0,
            },
            anti_alias: true,
            ..RenderOptions::default()
        };
        let whole = render_page_whole(&doc, page, &opts, None).expect("drawn");
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let got: std::sync::Mutex<Vec<(u32, Pixmap)>> = std::sync::Mutex::new(Vec::new());
        let band = |y0: u32, rows: &Pixmap| got.lock().unwrap().push((y0, rows.clone()));
        let picture =
            render_page_progressive(&doc, page, &opts, &cancel, 350, &band).expect("drawn");
        let (mut worst, mut off) = (0u8, 0usize);
        for (x, y) in picture.data().iter().zip(whole.data()) {
            let d = x.abs_diff(*y);
            worst = worst.max(d);
            if d > 2 {
                off += 1;
            }
        }
        assert!(worst <= 128, "bands differ by {worst}");
        assert!(off * 2000 < whole.data().len(), "{off} values differ");
        let got = got.into_inner().unwrap();
        let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
        if cores < 2 {
            assert!(got.is_empty(), "one core: drawn whole");
            return;
        }
        let row = 900 * 4;
        // Every run hands over every row once, each band as the picture
        // has it.
        let check = |got: &[(u32, Pixmap)], picture: &Pixmap| {
            assert!(
                got.len() > cores.min(16),
                "more bands than cores: {}",
                got.len()
            );
            let mut covered = vec![0u8; 700];
            for (y0, rows) in got {
                assert_eq!(rows.width(), 900);
                let (a, b) = (*y0 as usize, *y0 as usize + rows.height() as usize);
                for c in &mut covered[a..b] {
                    *c += 1;
                }
                assert!(
                    rows.data() == &picture.data()[a * row..b * row],
                    "rows {a}..{b}"
                );
            }
            assert!(covered.iter().all(|c| *c == 1), "every row once");
        };
        check(&got, &picture);
        // The band holding the middle row is the first one taken, so it
        // is handed over among the first few, unless the test machine
        // holds its thread back while the others finish theirs. A busy
        // machine may do that now and then, so the middle is looked for
        // over a few runs, each run checked whole.
        let middle_first = |got: &[(u32, Pixmap)]| {
            got[..cores.min(got.len())]
                .iter()
                .any(|(y0, r)| (*y0..*y0 + r.height()).contains(&350))
        };
        let mut seen = middle_first(&got);
        for _ in 0..8 {
            if seen {
                break;
            }
            let again: std::sync::Mutex<Vec<(u32, Pixmap)>> = std::sync::Mutex::new(Vec::new());
            let band = |y0: u32, rows: &Pixmap| again.lock().unwrap().push((y0, rows.clone()));
            let picture =
                render_page_progressive(&doc, page, &opts, &cancel, 350, &band).expect("drawn");
            let again = again.into_inner().unwrap();
            check(&again, &picture);
            seen = middle_first(&again);
        }
        assert!(seen, "the middle comes among the first");
        cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(render_page_progressive(&doc, page, &opts, &cancel, 0, &|_, _| ()).is_none());
    }

    /// A page drawn on one thread is the page drawn whole (large enough
    /// here for [`render_page`] to draw it in bands), and an image too
    /// large is refused as [`render_page`] refuses it.
    #[test]
    fn one_thread_draws_the_whole_page() {
        let (doc, page) = heavy(60);
        let opts = RenderOptions {
            width: 900,
            height: 700,
            view: ViewTransform {
                zoom: 3.0,
                origin_x: 20.0,
                origin_y: 650.0,
                stretch_y: 1.0,
            },
            anti_alias: true,
            ..RenderOptions::default()
        };
        let one = render_page_on_one_thread(&doc, page, &opts).expect("drawn");
        let whole = render_page_whole(&doc, page, &opts, None).expect("drawn");
        assert_eq!((one.width(), one.height()), (900, 700));
        assert!(one.data() == whole.data());
        assert!(one.data().iter().any(|v| *v != 0), "something is drawn");
        let huge = RenderOptions {
            width: 1 << 20,
            height: 1 << 20,
            ..opts
        };
        assert!(render_page_on_one_thread(&doc, page, &huge).is_none());
    }

    /// A page drawn in bands on several threads matches the page drawn
    /// whole, drop shadows included (their blur reaches across bands).
    #[test]
    fn bands_match_a_whole_render() {
        let (mut doc, page) = heavy(300);
        let layer = doc.layers_for_page(page).unwrap()[0].id;
        for (i, s) in doc.layer_mut(layer).unwrap().shapes.iter_mut().enumerate() {
            if i % 7 == 0 {
                s.shadow = Some(tracedraw_core::document::Shadow {
                    blur: 3.0,
                    ..Default::default()
                });
            }
        }
        let opts = RenderOptions {
            width: 900,
            height: 700,
            view: ViewTransform {
                zoom: 3.0,
                origin_x: 20.0,
                origin_y: 650.0,
                stretch_y: 1.0,
            },
            anti_alias: true,
            ..RenderOptions::default()
        };
        let (a, b) = match render_page_in_bands(&doc, page, &opts, None) {
            Some(a) => (a, render_page_whole(&doc, page, &opts, None).unwrap()),
            // One core: nothing to compare.
            None => return,
        };
        // A curve cut by a band's edge is flattened a little differently,
        // which moves the coverage of a few edge pixels slightly; nothing
        // else may differ.
        let (mut worst, mut off) = (0u8, 0usize);
        for (x, y) in a.data().iter().zip(b.data()) {
            let d = x.abs_diff(*y);
            worst = worst.max(d);
            if d > 2 {
                off += 1;
            }
        }
        assert!(worst <= 48, "bands differ by {worst}");
        assert!(off * 2000 < a.data().len(), "{off} values differ");
    }

    /// Render times of a page of 200 photo tiles (6000 x 4000 pixels in
    /// all) at screen zooms from 2 % to 100 %, the second render of each
    /// with the pictures decoded (#853).
    #[test]
    #[ignore]
    fn picture_render_times() {
        let (tw, th) = (300u32, 400u32);
        let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(210.0, 297.0));
        let page = doc.pages[0].id;
        for j in 0..10u32 {
            for i in 0..20u32 {
                let mut pm = Pixmap::new(tw, th).unwrap();
                for (k, px) in pm.data_mut().chunks_mut(4).enumerate() {
                    let v = ((k as u32 * 7 + i * 13 + j * 29) % 251) as u8;
                    px.copy_from_slice(&[v, v / 2, 255 - v, 255]);
                }
                let (w_mm, h_mm) = (33.33, 44.44);
                let x = -150.0 + i as f64 * w_mm;
                let y = 100.0 - (j + 1) as f64 * h_mm;
                doc.pages[0].layers[0].shapes.push(picture(
                    (j * 20 + i + 1) as u64,
                    &pm,
                    Rect::new(x, y, x + w_mm, y + h_mm),
                ));
            }
        }
        for pct in [2.0, 10.0, 21.0, 100.0] {
            // 96 pixels per inch on screen.
            let zoom = pct / 100.0 * 96.0 / 25.4;
            let opts = RenderOptions {
                width: 1360,
                height: 768,
                view: ViewTransform {
                    zoom,
                    origin_x: 680.0 + 100.0 * zoom,
                    origin_y: 384.0 + 100.0 * zoom,
                    stretch_y: 1.0,
                },
                anti_alias: true,
                draft: true,
                ..RenderOptions::default()
            };
            let t = std::time::Instant::now();
            let _ = render_page(&doc, page, &opts);
            let first = t.elapsed();
            let t = std::time::Instant::now();
            let _ = render_page(&doc, page, &opts);
            eprintln!("{pct} %: first {first:?}, again {:?}", t.elapsed());
        }
    }

    /// Render times of a page of 2000 half transparent objects, as a
    /// transparency slider dragged over them redraws it (#1335):
    /// `cargo test --release -p tracedraw-render -- --ignored render_times_of_transparent_objects --nocapture`.
    #[test]
    #[ignore]
    fn render_times_of_transparent_objects() {
        let (mut doc, page) = heavy(2000);
        for l in doc.pages.iter_mut().flat_map(|p| p.layers.iter_mut()) {
            for s in &mut l.shapes {
                s.opacity = 0.5;
            }
        }
        for (w, h) in [(1600u32, 1000u32), (3200, 2000)] {
            let zoom = h as f64 / 297.0;
            let opts = RenderOptions {
                width: w,
                height: h,
                view: ViewTransform {
                    zoom,
                    origin_x: 0.0,
                    origin_y: h as f64,
                    stretch_y: 1.0,
                },
                anti_alias: true,
                ..RenderOptions::default()
            };
            let t = std::time::Instant::now();
            let _ = render_page(&doc, page, &opts);
            eprintln!("{w} x {h}: {:?}", t.elapsed());
        }
    }

    #[test]
    #[ignore]
    fn render_times() {
        let one = std::env::var_os("BENCH_ONE").is_some();
        let ns: &[usize] = if one { &[1000] } else { &[1000, 5000, 20000] };
        for &n in ns {
            let (doc, page) = heavy(n);
            let t = std::time::Instant::now();
            let c = doc.clone();
            eprintln!("n={n} clone: {:?}", t.elapsed());
            drop(c);
            let views: &[(u32, u32, f64)] = if one {
                &[(1920, 1080, 3.0)]
            } else {
                &[(1920, 1080, 3.0), (3840, 2160, 6.0), (1920, 1080, 20.0)]
            };
            for &(w, h, zoom) in views {
                let opts = RenderOptions {
                    width: w,
                    height: h,
                    view: ViewTransform {
                        zoom,
                        origin_x: 100.0,
                        origin_y: h as f64 - 50.0,
                        stretch_y: 1.0,
                    },
                    anti_alias: true,
                    draft: true,
                    ..RenderOptions::default()
                };
                let t = std::time::Instant::now();
                let _ = render_page(&doc, page, &opts);
                eprintln!("n={n} {w}x{h} zoom={zoom}: {:?}", t.elapsed());
            }
        }
    }
}

/// Shadows seen zoomed in close (#2895): the object reaches past the
/// picture, which is drawn in bands and anew at every pan and zoom step.
/// Each pixel comes out the same whatever band or view draws it, and a
/// shadow keeps its width in mm, so nothing moves on screen while the
/// picture is replaced.
#[cfg(all(test, not(target_arch = "wasm32")))]
mod inner_shadow_view_tests {
    use super::*;
    use tracedraw_core::{Color, Fill};

    fn mesh_with_inner_shadow() -> (Document, PageId) {
        let r = Rect::new(10.0, 10.0, 90.0, 90.0);
        let mut s = Shape::new(
            tracedraw_core::ShapeId(1),
            ShapeKind::Rect {
                rect: r,
                radius: 0.0,
                corners: None,
            },
        );
        let mut m = tracedraw_core::Mesh::new(r, 2, 2, Color::WHITE);
        let colors = [
            Color::rgb8(255, 0, 0),
            Color::rgb8(0, 255, 0),
            Color::rgb8(0, 0, 255),
            Color::rgb8(255, 255, 0),
            Color::rgb8(0, 255, 255),
            Color::rgb8(255, 0, 255),
            Color::rgb8(255, 128, 0),
            Color::rgb8(0, 128, 255),
            Color::rgb8(128, 0, 255),
        ];
        for (n, c) in m.nodes.iter_mut().zip(colors) {
            n.color = c;
        }
        s.fill = Fill::Mesh(m);
        s.stroke = None;
        s.effects.push(tracedraw_core::live::Effect::InnerShadow {
            offset: tracedraw_core::geometry::Vec2::new(2.0, -2.0),
            color: Color::BLACK,
            opacity: 0.8,
            blur: 2.0,
            merge: tracedraw_core::live::MergeMode::Normal,
            spread: 0.0,
        });
        let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(100.0, 100.0));
        let layer = doc.pages[0].layers[0].id;
        doc.layer_mut(layer).expect("layer").shapes.push(s);
        let page = doc.pages[0].id;
        (doc, page)
    }

    fn opts(zoom: f64, dx: f64) -> RenderOptions {
        RenderOptions {
            width: 1200,
            height: 900,
            view: ViewTransform {
                zoom,
                origin_x: -8.0 * zoom - dx,
                origin_y: 900.0 + 8.0 * zoom,
                stretch_y: 1.0,
            },
            draft: true,
            ..Default::default()
        }
    }

    fn diff(p: tiny_skia::PremultipliedColorU8, q: tiny_skia::PremultipliedColorU8) -> u8 {
        p.red()
            .abs_diff(q.red())
            .max(p.green().abs_diff(q.green()))
            .max(p.blue().abs_diff(q.blue()))
            .max(p.alpha().abs_diff(q.alpha()))
    }

    #[test]
    fn bands_draw_an_inner_shadow_as_one_picture_does() {
        let (doc, page) = mesh_with_inner_shadow();
        for zoom in [4.0, 20.0, 60.0] {
            let a = render_page(&doc, page, &opts(zoom, 0.0)).expect("bands");
            let b = render_page_on_one_thread(&doc, page, &opts(zoom, 0.0)).expect("whole");
            let worst = a
                .pixels()
                .iter()
                .zip(b.pixels())
                .map(|(p, q)| diff(*p, *q))
                .max()
                .unwrap_or(0);
            assert!(worst <= 1, "zoom {zoom}: {worst}");
        }
    }

    /// How far into a white square, mm, an inner shadow or a drop
    /// shadow beside it reaches at `zoom` before it fades to 90% light.
    fn shadow_reach_mm(inner: bool, zoom: f64) -> f64 {
        let r = Rect::new(10.0, 10.0, 90.0, 90.0);
        let mut s = Shape::new(
            tracedraw_core::ShapeId(1),
            ShapeKind::Rect {
                rect: r,
                radius: 0.0,
                corners: None,
            },
        );
        s.fill = Fill::Solid(Color::WHITE);
        s.stroke = None;
        if inner {
            s.effects.push(tracedraw_core::live::Effect::InnerShadow {
                offset: tracedraw_core::geometry::Vec2::new(0.0, 0.0),
                color: Color::BLACK,
                opacity: 1.0,
                blur: 2.0,
                merge: tracedraw_core::live::MergeMode::Normal,
                spread: 0.0,
            });
        } else {
            s.shadow = Some(tracedraw_core::Shadow {
                offset: tracedraw_core::geometry::Vec2::new(0.0, 0.0),
                opacity: 1.0,
                blur: 2.0,
                color: Color::BLACK,
                ..Default::default()
            });
        }
        let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(100.0, 100.0));
        let layer = doc.pages[0].layers[0].id;
        doc.layer_mut(layer).expect("layer").shapes.push(s);
        let page = doc.pages[0].id;
        // The square's left edge at x = 600, a row through its middle.
        let o = RenderOptions {
            width: 1200,
            height: 50,
            view: ViewTransform {
                zoom,
                origin_x: 600.0 - 10.0 * zoom,
                origin_y: 25.0 + 50.0 * zoom,
                stretch_y: 1.0,
            },
            draft: true,
            ..Default::default()
        };
        let pm = render_page(&doc, page, &o).expect("render");
        let light = |x: u32| {
            let p = pm.pixel(x, 25).expect("pixel");
            // Over the transparent desktop the shadow shows in its alpha.
            if p.alpha() < 255 {
                255 - p.alpha()
            } else {
                p.red()
            }
        };
        let x = if inner {
            (600..1200).find(|&x| light(x) > 230)
        } else {
            (0..600).rev().find(|&x| light(x) > 230)
        }
        .expect("fades");
        (f64::from(x) - 600.0).abs() / zoom
    }

    /// A shadow keeps its width in mm zoomed in close (#2895): its blur
    /// stopped at 64 pixels, so past that zoom each step drew it
    /// narrower than the scaled picture shown meanwhile, and the drawing
    /// seemed to shake while zooming.
    #[test]
    fn a_shadow_keeps_its_width_zoomed_in_close() {
        for inner in [true, false] {
            let at = |z: f64| shadow_reach_mm(inner, z);
            let base = at(20.0);
            for zoom in [40.0, 60.0, 75.0] {
                let w = at(zoom);
                assert!(
                    (w - base).abs() < 0.2,
                    "inner {inner}: {w} mm at zoom {zoom}, {base} mm at 20"
                );
            }
        }
    }

    #[test]
    fn a_pan_moves_an_inner_shadow_and_nothing_else() {
        let (doc, page) = mesh_with_inner_shadow();
        for zoom in [4.0, 20.0, 60.0] {
            let a = render_page(&doc, page, &opts(zoom, 0.0)).expect("before");
            let b = render_page(&doc, page, &opts(zoom, 37.0)).expect("panned");
            let mut worst = 0;
            for y in 0..900 {
                for x in 0..1100 {
                    let p = a.pixel(x + 37, y).expect("pixel");
                    let q = b.pixel(x, y).expect("pixel");
                    worst = worst.max(diff(p, q));
                }
            }
            assert!(worst <= 1, "zoom {zoom}: {worst}");
        }
    }
}

/// A picture of a page whose size is not whole pixels at the resolution
/// asked (#2866): what covers the page covers every pixel, with no light
/// row above it or column to its right.
#[cfg(test)]
mod whole_page_tests {
    use super::*;
    use tracedraw_core::{Color, Fill};

    #[test]
    fn an_exported_page_has_no_light_edge() {
        for (w, h, dpi) in [(30.1, 20.3, 96.0), (95.37, 41.82, 300.0), (10.0, 10.0, 72.0)] {
            let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(w, h));
            let mut s = Shape::new(
                tracedraw_core::ShapeId(1),
                ShapeKind::Rect {
                    rect: Rect::new(0.0, 0.0, w, h),
                    radius: 0.0,
                    corners: None,
                },
            );
            s.fill = Fill::Solid(Color::BLACK);
            s.stroke = None;
            let layer = doc.pages[0].layers[0].id;
            doc.layer_mut(layer).expect("layer").shapes.push(s);
            let page = doc.pages[0].id;
            let pm = render_page_export_image(&doc, page, dpi).expect("render");
            let zoom = dpi / 25.4;
            assert_eq!(pm.width(), (w * zoom).ceil() as u32);
            assert_eq!(pm.height(), (h * zoom).ceil() as u32);
            let lightest = pm.pixels().iter().map(|p| p.red()).max().unwrap_or(0);
            assert!(lightest < 8, "{w} x {h} mm at {dpi} dpi: a pixel at {lightest}");
        }
    }

    /// The page's edges are the picture's edges on all four sides: one
    /// zoom for both ways, made large enough for the width, cut a pixel
    /// off the top of an A4 page and many off a long strip (#2866).
    #[test]
    fn an_exported_page_keeps_its_top_and_right_edges() {
        for (w, h, dpi) in [
            (210.0, 297.0, 72.0),
            (210.0, 297.0, 300.0),
            (10.0, 300.0, 300.0),
            (300.0, 10.0, 300.0),
        ] {
            let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(w, h));
            let zoom = dpi / 25.4;
            // A band one pixel deep along the top and the right edge.
            let px = 1.0 / zoom;
            let layer = doc.pages[0].layers[0].id;
            for (k, rect) in [Rect::new(0.0, h - px, w, h), Rect::new(w - px, 0.0, w, h)]
                .into_iter()
                .enumerate()
            {
                let mut s = Shape::new(
                    tracedraw_core::ShapeId(k as u64 + 1),
                    ShapeKind::Rect {
                        rect,
                        radius: 0.0,
                        corners: None,
                    },
                );
                s.fill = Fill::Solid(Color::BLACK);
                s.stroke = None;
                doc.layer_mut(layer).expect("layer").shapes.push(s);
            }
            let page = doc.pages[0].id;
            let pm = render_page_export_image(&doc, page, dpi).expect("render");
            let (pw, ph) = (pm.width() as usize, pm.height() as usize);
            let px = |x: usize, y: usize| pm.pixels()[y * pw + x].red();
            let top = (0..pw).map(|x| px(x, 0)).max().unwrap_or(0);
            let right = (0..ph).map(|y| px(pw - 1, y)).max().unwrap_or(0);
            assert!(top < 64, "{w} x {h} mm at {dpi} dpi: top row at {top}");
            assert!(
                right < 64,
                "{w} x {h} mm at {dpi} dpi: right column at {right}"
            );
        }
    }
}

#[cfg(test)]
mod holo_tests {
    use super::*;

    const DARK: [f32; 3] = [0.11, 0.12, 0.17];
    const LIGHT: [f32; 3] = [0.93, 0.93, 0.95];

    fn hue_and_sat(c: [f32; 3]) -> (f32, f32) {
        let (mx, mn) = (c[0].max(c[1]).max(c[2]), c[0].min(c[1]).min(c[2]));
        let d = mx - mn;
        if d < 1e-6 {
            return (0.0, 0.0);
        }
        let h = if mx == c[0] {
            ((c[1] - c[2]) / d).rem_euclid(6.0)
        } else if mx == c[1] {
            (c[2] - c[0]) / d + 2.0
        } else {
            (c[0] - c[1]) / d + 4.0
        };
        (h * 60.0, d / mx.max(1e-6))
    }

    /// The holographic look is fixed: the same point and seed always give
    /// the same colour, and another seed another picture.
    #[test]
    fn holographic_colour_is_fixed() {
        for glitter in [false, true] {
            let a = holo_color(1.3, 2.7, 5, glitter, DARK, LIGHT);
            assert_eq!(a, holo_color(1.3, 2.7, 5, glitter, DARK, LIGHT));
        }
        let differs = (0..50).any(|i| {
            let (x, y) = (i as f64 * 0.37, i as f64 * 0.21);
            holo_color(x, y, 1, false, DARK, LIGHT) != holo_color(x, y, 2, false, DARK, LIGHT)
        });
        assert!(differs);
    }

    /// Across an object the foil runs from dark to light and shows the
    /// spectrum: coloured pixels of hues far apart, not two colours mixed.
    #[test]
    fn holographic_texture_shows_the_spectrum() {
        let mut hues = [false; 6];
        let (mut darkest, mut lightest) = (1.0f32, 0.0f32);
        for i in 0..80 {
            for j in 0..80 {
                let c = holo_color(i as f64 * 0.05, j as f64 * 0.05, 1, false, DARK, LIGHT);
                let l = (c[0] + c[1] + c[2]) / 3.0;
                darkest = darkest.min(l);
                lightest = lightest.max(l);
                let (h, s) = hue_and_sat(c);
                if s > 0.35 && l > 0.15 {
                    hues[((h / 60.0) as usize).min(5)] = true;
                }
            }
        }
        assert!(darkest < 0.3 && lightest > 0.8, "{darkest} {lightest}");
        assert!(hues.iter().filter(|h| **h).count() >= 3, "{hues:?}");
    }

    /// The glitter changes single grains, not the whole picture.
    #[test]
    fn holo_glitter_sparkles_in_grains() {
        let (mut changed, mut n) = (0, 0);
        for i in 0..200 {
            let (x, y) = (i as f64 * 0.0131, i as f64 * 0.0077);
            n += 1;
            if holo_color(x, y, 1, true, DARK, LIGHT) != holo_color(x, y, 1, false, DARK, LIGHT) {
                changed += 1;
            }
        }
        assert!(changed > n / 20 && changed < n / 2, "{changed} of {n}");
    }

    /// The renderer paints a holographic texture fill in colour.
    #[test]
    fn holographic_texture_fill_renders_in_colour() {
        let fill = Fill::Texture(tracedraw_core::Texture {
            kind: tracedraw_core::TextureKind::Holographic,
            color_a: Color::rgb8(28, 30, 44),
            color_b: Color::rgb8(236, 238, 242),
            scale: 20.0,
            seed: 1,
        });
        let pm = render_fill_image(&fill, Rect::new(0.0, 0.0, 60.0, 60.0), 50.0).unwrap();
        let coloured = pm
            .pixels()
            .iter()
            .filter(|p| {
                let (r, g, b) = (p.red() as i32, p.green() as i32, p.blue() as i32);
                r.max(g).max(b) - r.min(g).min(b) > 60
            })
            .count();
        assert!(coloured > pm.pixels().len() / 50, "{coloured}");
    }
}
