//! Bitmap to vector tracing: outline mode (colour regions become filled
//! curves) and centreline mode (strokes become open curves). See
//! docs/behavior/bitmap-tracing.md for the algorithm and defaults.

use image::{Rgba, RgbaImage};
use tracedraw_core::{
    geometry::{BezPath, Point, Rect},
    Color,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Preset {
    // Centreline
    Technical,
    LineDrawing,
    // Outline
    LineArt,
    Logo,
    DetailedLogo,
    Clipart,
    LowQualityImage,
    HighQualityImage,
    /// Outline, with the detail chosen for each picture: the one that
    /// draws it most faithfully in the fewest nodes (see [`optimize`]).
    Optimized,
}

impl Preset {
    /// Every preset, centreline ones first.
    pub const ALL: [Preset; 9] = [
        Preset::Technical,
        Preset::LineDrawing,
        Preset::LineArt,
        Preset::Logo,
        Preset::DetailedLogo,
        Preset::Clipart,
        Preset::LowQualityImage,
        Preset::HighQualityImage,
        Preset::Optimized,
    ];

    pub fn is_centerline(self) -> bool {
        matches!(self, Preset::Technical | Preset::LineDrawing)
    }
    pub fn key(self) -> &'static str {
        match self {
            Preset::Technical => "menu.bitmaps.trace_technical",
            Preset::LineDrawing => "menu.bitmaps.trace_line_drawing",
            Preset::LineArt => "menu.bitmaps.trace_line_art",
            Preset::Logo => "menu.bitmaps.trace_logo",
            Preset::DetailedLogo => "menu.bitmaps.trace_detailed_logo",
            Preset::Clipart => "menu.bitmaps.trace_clipart",
            Preset::LowQualityImage => "menu.bitmaps.trace_low_quality",
            Preset::HighQualityImage => "menu.bitmaps.trace_high_quality",
            Preset::Optimized => "menu.bitmaps.trace_optimized",
        }
    }
    /// The presets of one trace type, in the order the dialog lists them.
    pub fn of_type(centerline: bool) -> &'static [Preset] {
        if centerline {
            &Preset::ALL[..2]
        } else {
            &Preset::ALL[2..]
        }
    }
    pub fn settings(self) -> Settings {
        let (detail, smoothing, colors, bw) = match self {
            Preset::Technical => (60.0, 30.0, 2, true),
            Preset::LineDrawing => (50.0, 50.0, 2, true),
            Preset::LineArt => (40.0, 25.0, 2, true),
            Preset::Logo => (75.0, 20.0, 16, false),
            Preset::DetailedLogo => (90.0, 15.0, 32, false),
            Preset::Clipart => (60.0, 25.0, 32, false),
            Preset::LowQualityImage => (35.0, 40.0, 48, false),
            Preset::HighQualityImage => (80.0, 15.0, 96, false),
            // The most colours the picture may keep: shades of one
            // colour join, so a flat picture keeps only its own.
            Preset::Optimized => (70.0, 25.0, 32, false),
        };
        Settings {
            detail,
            smoothing,
            corner_smoothness: 0.0,
            colors,
            mode: if bw {
                ColorMode::BlackWhite
            } else {
                ColorMode::Rgb
            },
            remove: Remove::Background,
            background: None,
            merge_adjacent: true,
            remove_overlap: false,
            group_by_color: false,
            centerline: self.is_centerline(),
            optimize: self == Preset::Optimized,
            delete_original: false,
            merged: Vec::new(),
            recolored: Vec::new(),
            pixel_scale: 1.0,
            clean_sketch: false,
            geometric: false,
        }
    }
}

/// The colours a trace works in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorMode {
    #[default]
    Rgb,
    /// The picture's greys, as many as Colors asks for.
    Grayscale,
    /// Two colours, by brightness.
    BlackWhite,
}

impl ColorMode {
    pub const ALL: [ColorMode; 3] = [ColorMode::Rgb, ColorMode::Grayscale, ColorMode::BlackWhite];
    pub fn key(self) -> &'static str {
        match self {
            ColorMode::Rgb => "dialog.trace_mode_rgb",
            ColorMode::Grayscale => "dialog.trace_mode_grayscale",
            ColorMode::BlackWhite => "dialog.black_and_white",
        }
    }
}

/// What a trace leaves out (the Remove list of the dialog).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Remove {
    /// The background: the regions of the background colour that touch
    /// the picture's edge. Patches of the same colour inside the picture
    /// stay.
    #[default]
    Background,
    /// Every region of the background colour, inside the picture too.
    EntireColor,
    /// Nothing: the background is traced like any colour.
    None,
}

impl Remove {
    pub const ALL: [Remove; 3] = [Remove::Background, Remove::EntireColor, Remove::None];
    pub fn key(self) -> &'static str {
        match self {
            Remove::Background => "dialog.trace_remove_background_color",
            Remove::EntireColor => "dialog.trace_remove_entire_color",
            Remove::None => "dialog.trace_remove_none",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub detail: f64,
    pub smoothing: f64,
    pub corner_smoothness: f64,
    pub colors: u32,
    pub mode: ColorMode,
    pub remove: Remove,
    /// The background colour `remove` takes out; `None` finds it (the
    /// commonest colour on the picture's edge).
    pub background: Option<[u8; 3]>,
    /// One curve per colour (every region of a colour combined); off,
    /// each separate patch of a colour is a curve of its own.
    pub merge_adjacent: bool,
    /// Each region exactly its own pixels; off, a region reaches a little
    /// under the regions drawn over it, so no page shows between them.
    pub remove_overlap: bool,
    /// The result holds a group per colour.
    pub group_by_color: bool,
    pub centerline: bool,
    /// The detail is chosen for each picture traced (the Optimized
    /// preset, [`optimize`]); `detail` is where the search starts.
    pub optimize: bool,
    pub delete_original: bool,
    /// Colours of the trace joined into one, the first of each group
    /// (the Colors tab's Merge).
    pub merged: Vec<Vec<[u8; 3]>>,
    /// Colours of the trace drawn as another colour (the Colors tab's
    /// colour buttons): (traced, shown).
    pub recolored: Vec<([u8; 3], [u8; 3])>,
    /// How many pixels of the picture make one pixel of the art: 1 for a
    /// picture as it was drawn, 4 for one enlarged four times before it
    /// is traced. Every measure the trace takes in pixels (the smallest
    /// patch kept, how far a curve may stray, how wide a corner the blur
    /// rounded) is in pixels of the art.
    pub pixel_scale: f64,
    /// A pencil sketch cleaned up first (hatching and doubled outlines
    /// made solid, construction lines and specks gone, edges smoothed)
    /// and traced black on white: see [`crate::trace_sketch`].
    pub clean_sketch: bool,
    /// Geometric shapes: the thin bands a compressed picture leaves along
    /// an edge cleaned away, every border fitted with straight lines, arcs
    /// and sharp corners, rings
    /// that nearly mirror themselves made symmetric, and whole circles,
    /// ellipses and rounded rectangles drawn as such (see
    /// [`crate::trace_shapes`]). Off, the trace is as before.
    pub geometric: bool,
}

impl Settings {
    /// Whether the background is removed at all.
    pub fn removes_background(&self) -> bool {
        self.remove != Remove::None
    }
    /// The colour `c` is shown as (the Colors tab's edits).
    pub fn shown_color(&self, c: [u8; 3]) -> [u8; 3] {
        self.recolored
            .iter()
            .find(|(from, _)| *from == c)
            .map(|(_, to)| *to)
            .unwrap_or(c)
    }

    /// Only what changes the traced curves and the colours they are
    /// traced as: the Colors tab's recolouring (see [`recolor`]),
    /// grouping and deleting the original are left at rest, and so are
    /// the values the trace type or mode does not use, so settings that
    /// trace alike compare (and hash) alike.
    pub fn traced_part(&self) -> Settings {
        let mut t = self.clone();
        t.recolored.clear();
        t.group_by_color = false;
        t.delete_original = false;
        if t.remove == Remove::None {
            t.background = None;
        }
        if t.mode == ColorMode::BlackWhite {
            t.colors = 2;
        }
        if t.centerline {
            // A centreline trace reads the detail and the smoothing only.
            t.corner_smoothness = 0.0;
            t.colors = 2;
            t.mode = ColorMode::BlackWhite;
            t.remove = Remove::None;
            t.background = None;
            t.merge_adjacent = false;
            t.remove_overlap = false;
            t.merged.clear();
            t.optimize = false;
            t.geometric = false;
        }
        t
    }
}

/// `traced` (made with [`Settings::traced_part`]) in the colours the
/// Colors tab gave them: the same as tracing with `s` itself.
pub fn recolor(traced: &mut [Traced], s: &Settings) {
    if s.recolored.is_empty() {
        return;
    }
    for t in traced {
        let c = t.color.to_rgb8();
        let to = s.shown_color(c);
        if to != c {
            t.color = Color::rgb8(to[0], to[1], to[2]);
            // A colour chosen by hand is drawn flat.
            t.blend = None;
        }
    }
}

/// One traced region or stroke.
#[derive(Debug, Clone, PartialEq)]
pub struct Traced {
    pub path: BezPath,
    pub color: Color,
    pub closed: bool,
    /// The blend the region is filled with, when it is a gradient of the
    /// picture (`color` is then its mean colour).
    pub blend: Option<crate::trace_blend::Blend>,
}

/// What the dialog's status line says of a trace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Stats {
    pub curves: usize,
    pub nodes: usize,
    pub colors: usize,
}

/// The counts of `traced`: curves (one per object), their nodes and the
/// distinct colours.
pub fn stats(traced: &[Traced]) -> Stats {
    let mut colors: Vec<[u8; 3]> = Vec::new();
    let mut nodes = 0;
    for t in traced {
        nodes += tracedraw_core::nodes::nodes(&t.path).len();
        let c = t.color.to_rgb8();
        if !colors.contains(&c) {
            colors.push(c);
        }
    }
    Stats {
        curves: traced.len(),
        nodes,
        colors: colors.len(),
    }
}

/// The distinct colours of `traced`, each with its number of curves, in
/// the order the regions stack (darkest first).
pub fn color_counts(traced: &[Traced]) -> Vec<([u8; 3], usize)> {
    let mut out: Vec<([u8; 3], usize)> = Vec::new();
    for t in traced {
        let c = t.color.to_rgb8();
        match out.iter_mut().find(|(k, _)| *k == c) {
            Some((_, n)) => *n += 1,
            None => out.push((c, 1)),
        }
    }
    out
}

/// `f(start, part)` for each run of `chunk` items of `out` (`start`:
/// the index of the part's first item), the parts on the computer's
/// cores; in order on one thread in the browser. Each part is its own,
/// so the result is the same however the parts are shared out.
pub(crate) fn par_chunks<T: Send>(out: &mut [T], chunk: usize, f: impl Fn(usize, &mut [T]) + Sync) {
    let chunk = chunk.max(1);
    #[cfg(not(target_arch = "wasm32"))]
    {
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
        if threads > 1 && out.len() > chunk {
            let workers = threads.min(out.len().div_ceil(chunk));
            let parts = std::sync::Mutex::new(out.chunks_mut(chunk).enumerate());
            std::thread::scope(|scope| {
                for _ in 0..workers {
                    scope.spawn(|| loop {
                        let next = parts.lock().unwrap_or_else(|e| e.into_inner()).next();
                        let Some((i, part)) = next else {
                            return;
                        };
                        f(i * chunk, part);
                    });
                }
            });
            return;
        }
    }
    for (i, part) in out.chunks_mut(chunk).enumerate() {
        f(i * chunk, part);
    }
}

/// A flag a trace running on another thread looks at between its steps:
/// set, the trace stops and gives nothing back.
pub type Cancel = std::sync::atomic::AtomicBool;

/// The flood fills look at the flag once per this many pixels.
const CANCEL_EVERY: usize = 4096;

pub(crate) fn cancelled(c: Option<&Cancel>) -> bool {
    c.is_some_and(|c| c.load(std::sync::atomic::Ordering::Relaxed))
}

/// Trace `img` into shapes whose coordinates are in pixel space (y down).
/// The caller maps them into the bitmap's rectangle.
pub fn trace(img: &RgbaImage, s: &Settings) -> Vec<Traced> {
    trace_unless(img, s, None).unwrap_or_default()
}

/// [`trace`], giving up (`None`) once `cancel` is set.
pub fn trace_unless(img: &RgbaImage, s: &Settings, cancel: Option<&Cancel>) -> Option<Vec<Traced>> {
    if s.centerline {
        return centerline(img, s, cancel);
    }
    if s.clean_sketch {
        let clean = crate::trace_sketch::clean_sketch(img);
        return trace_unless(&clean, &crate::trace_sketch::sketch_settings(s), cancel);
    }
    let mut traced = if s.optimize {
        let best = optimize(img, s, cancel)?;
        outline(img, &best, cancel)?
    } else {
        outline(img, s, cancel)?
    };
    // Small print redrawn with the letters of the font that draws it.
    crate::trace_progress::report(cancel, crate::trace_progress::Step::Letters, 0.0);
    crate::trace_text::redraw_text(img, s, &mut traced, cancel)?;
    Some(traced)
}

/// The trace before the small print is redrawn, for the tests of
/// `trace_text.rs`.
#[cfg(test)]
pub(crate) fn outline_for_tests(img: &RgbaImage, s: &Settings) -> Vec<Traced> {
    outline(img, s, None).unwrap_or_default()
}

// ----- the best settings for a picture -----------------------------------------

/// The details [`optimize`] tries, coarse to fine. Above 85 the cleaning
/// of edges and specks stops and the curves multiply.
#[cfg(not(target_arch = "wasm32"))]
const OPTIMIZE_DETAILS: [f64; 6] = [50.0, 60.0, 70.0, 80.0, 90.0, 95.0];
/// In the browser the details are traced one after the other, in the
/// frame: the three in the middle, where the search lands on nearly every
/// picture.
#[cfg(target_arch = "wasm32")]
const OPTIMIZE_DETAILS: [f64; 3] = [60.0, 70.0, 80.0];

/// The side of the samples [`optimize`] traces, pixels, and how many.
const SAMPLE_SIDE: u32 = 200;
const SAMPLES: usize = 4;

/// What a node costs against a pixel drawn wrong, in [`optimize`]'s
/// score: the share of pixels drawn wrong (percent) plus this much per
/// node for each pixel of the samples' sides.
const NODE_COST: f64 = 0.3;

/// The settings `s` with the detail that draws `img` best: each
/// detail of [`OPTIMIZE_DETAILS`] traces samples of the picture at its
/// own size (the [`SAMPLES`] squares of its art with the most edges, or
/// the whole art when small, side by side), the trace is drawn back and
/// compared with them ([`wrong_share`]), and the detail with the lowest
/// score wins: the share drawn wrong plus [`NODE_COST`] per node per
/// pixel of the samples' sides, so a finer detail is chosen while it
/// draws the picture better for the nodes it adds, and no further. The
/// result traces as it is (`optimize` off). `None` once `cancel` is set.
pub fn optimize(img: &RgbaImage, s: &Settings, cancel: Option<&Cancel>) -> Option<Settings> {
    let mut base = s.clone();
    base.optimize = false;
    if s.centerline {
        return Some(base);
    }
    let samples = samples_of(img);
    if samples.is_empty() {
        return Some(base);
    }
    let sides: f64 = samples
        .iter()
        .map(|p| (p.width() + p.height()) as f64)
        .sum();
    // The samples side by side, a transparent pixel apart, traced at once.
    let sheet = sheet_of(&samples);
    // The samples hold the art only: its background, transparent around
    // them, is no colour of theirs to take out (the colour along a
    // sample's edge is the art's).
    let mut sample_settings = base.clone();
    sample_settings.remove = Remove::None;
    sample_settings.background = None;
    // The progress window counts the details tried; the samples' own
    // steps are not the trace's.
    let _hush = crate::trace_progress::hush(cancel);
    let tried = std::sync::atomic::AtomicUsize::new(0);
    let to_try = OPTIMIZE_DETAILS.len() + 1;
    let score = |detail: f64| -> Option<f64> {
        let mut t = sample_settings.clone();
        t.detail = detail;
        let (w, h) = sheet.dimensions();
        let traced = outline(&sheet, &t, cancel)?;
        let drawn = render_traced(&traced, w, h, 1.0);
        let wrong = wrong_share(&sheet, &drawn, cancel)?;
        let n = tried.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        crate::trace_progress::report(
            cancel,
            crate::trace_progress::Step::Detail,
            n as f64 / to_try as f64,
        );
        Some(wrong + NODE_COST * stats(&traced).nodes as f64 / sides.max(1.0))
    };
    // Each detail on a thread of its own where there are threads.
    // The detail the settings have is tried too, and kept unless another
    // scores better: the search never leaves a trace worse than the one
    // it started from (#2930).
    let mut details: Vec<f64> = OPTIMIZE_DETAILS.to_vec();
    let own = s.detail.clamp(0.0, 100.0);
    if !details.iter().any(|d| (*d - own).abs() < 0.5) {
        details.push(own);
    }
    #[cfg(not(target_arch = "wasm32"))]
    let scores: Vec<Option<f64>> = std::thread::scope(|scope| {
        let runs: Vec<_> = details
            .iter()
            .map(|d| scope.spawn(move || score(*d)))
            .collect();
        runs.into_iter().map(|r| r.join().ok().flatten()).collect()
    });
    #[cfg(target_arch = "wasm32")]
    let scores: Vec<Option<f64>> = details.iter().map(|d| score(*d)).collect();
    if cancelled(cancel) {
        return None;
    }
    let scored: Vec<(f64, f64)> = details
        .iter()
        .zip(&scores)
        .filter_map(|(d, s)| s.map(|s| (*d, s)))
        .collect();
    let own_score = scored
        .iter()
        .find(|(d, _)| (*d - own).abs() < 0.5)
        .map(|(_, s)| *s);
    let best = scored.iter().copied().min_by(|a, b| a.1.total_cmp(&b.1));
    if let Some((detail, score)) = best {
        // Only for a real gain: scores a hair apart draw alike.
        if own_score.is_none_or(|o| score < o - 0.05) {
            base.detail = detail;
        }
    }
    Some(base)
}

/// `samples` in a row, a transparent pixel between each two.
fn sheet_of(samples: &[RgbaImage]) -> RgbaImage {
    let w: u32 = samples.iter().map(|p| p.width() + 1).sum();
    let h = samples.iter().map(|p| p.height()).max().unwrap_or(0);
    let mut sheet = RgbaImage::new(w.max(1), h.max(1));
    let mut x = 0;
    for p in samples {
        image::imageops::replace(&mut sheet, p, x as i64, 0);
        x += p.width() + 1;
    }
    sheet
}

/// The parts of `img` [`optimize`] traces: its opaque pixels' box when it
/// is no more than two samples across, else the [`SAMPLES`] squares of
/// [`SAMPLE_SIDE`] in that box with the most edges (neighbouring pixels
/// more than 60 apart), the art's finest parts, where the detail shows.
fn samples_of(img: &RgbaImage) -> Vec<RgbaImage> {
    let (w, h) = img.dimensions();
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for (x, y, p) in img.enumerate_pixels() {
        if p[3] >= 128 {
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
        }
    }
    if x0 > x1 || x1 - x0 < 8 || y1 - y0 < 8 {
        return Vec::new();
    }
    let (bw, bh) = (x1 - x0 + 1, y1 - y0 + 1);
    if bw <= 2 * SAMPLE_SIDE && bh <= 2 * SAMPLE_SIDE {
        return vec![image::imageops::crop_imm(img, x0, y0, bw, bh).to_image()];
    }
    let raw = img.as_raw();
    let px = |x: u32, y: u32| {
        let i = (y * w + x) as usize * 4;
        [
            raw[i] as i32,
            raw[i + 1] as i32,
            raw[i + 2] as i32,
            raw[i + 3] as i32,
        ]
    };
    let differs = |a: [i32; 4], b: [i32; 4]| {
        (a[3] >= 128) != (b[3] >= 128) || (0..3).map(|k| (a[k] - b[k]).pow(2)).sum::<i32>() > 3600
    };
    // Squares on a grid over the box, scored by their edges.
    let side = SAMPLE_SIDE.min(bw).min(bh);
    let mut scored: Vec<(u64, u32, u32)> = Vec::new();
    let mut ty = y0;
    while ty + side <= y1 + 1 {
        let mut tx = x0;
        while tx + side <= x1 + 1 {
            let mut edges = 0u64;
            for y in (ty..ty + side - 1).step_by(2) {
                for x in tx..tx + side - 1 {
                    let p = px(x, y);
                    if differs(p, px(x + 1, y)) || differs(p, px(x, y + 1)) {
                        edges += 1;
                    }
                }
            }
            scored.push((edges, tx, ty));
            tx += side;
        }
        ty += side;
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    scored
        .iter()
        .take(SAMPLES)
        .map(|(_, x, y)| image::imageops::crop_imm(img, *x, *y, side, side).to_image())
        .collect()
}

/// How far (RGB distance, both over white) a pixel's colour must be from
/// every pixel of the other picture within a pixel of it to be drawn
/// wrong.
const WRONG_DISTANCE: f64 = 60.0;

/// The share (percent) of the pixels of `a` and `b` (the same size; the
/// pixels opaque in either) drawn wrong: whose colour in one picture is
/// further than [`WRONG_DISTANCE`] from every pixel of the other within a
/// pixel. An edge drawn a pixel off is not wrong; a lost dot or line, a
/// region in another colour, a gap or a blob are.
pub fn wrong_share(a: &RgbaImage, b: &RgbaImage, cancel: Option<&Cancel>) -> Option<f64> {
    let (w, h) = a.dimensions();
    if b.dimensions() != (w, h) || w == 0 || h == 0 {
        return Some(100.0);
    }
    let over = |p: &Rgba<u8>| -> [f32; 3] {
        let k = p[3] as f32 / 255.0;
        std::array::from_fn(|c| p[c] as f32 * k + 255.0 * (1.0 - k))
    };
    let pa: Vec<[f32; 3]> = a.pixels().map(over).collect();
    let pb: Vec<[f32; 3]> = b.pixels().map(over).collect();
    let (w, h) = (w as usize, h as usize);
    let limit = (WRONG_DISTANCE * WRONG_DISTANCE) as f32;
    let far = |img: &[[f32; 3]], x: usize, y: usize, c: [f32; 3]| {
        (y.saturating_sub(1)..=(y + 1).min(h - 1)).all(|v| {
            (x.saturating_sub(1)..=(x + 1).min(w - 1)).all(|u| {
                let q = img[v * w + u];
                (0..3).map(|k| (q[k] - c[k]).powi(2)).sum::<f32>() > limit
            })
        })
    };
    let (ra, rb) = (a.as_raw(), b.as_raw());
    let (mut seen, mut wrong) = (0usize, 0usize);
    for y in 0..h {
        if y % 32 == 0 && cancelled(cancel) {
            return None;
        }
        for x in 0..w {
            let i = y * w + x;
            if ra[i * 4 + 3] < 8 && rb[i * 4 + 3] < 8 {
                continue;
            }
            seen += 1;
            if far(&pb, x, y, pa[i]) || far(&pa, x, y, pb[i]) {
                wrong += 1;
            }
        }
    }
    Some(if seen == 0 {
        0.0
    } else {
        wrong as f64 * 100.0 / seen as f64
    })
}

/// The palette index nearest `c` (none for an empty palette).
fn palette_index(palette: &[[u8; 3]], c: [u8; 3]) -> Option<usize> {
    (!palette.is_empty()).then(|| nearest(palette, c))
}

/// `labels` with the colours of each group of `merged` relabelled to the
/// group's first colour; a colour matches the palette entry nearest it
/// when within `reach` (RGB distance, squared).
fn merge_colors(labels: &mut [usize], palette: &[[u8; 3]], merged: &[Vec<[u8; 3]>], reach: u32) {
    let mut alias: Vec<usize> = (0..palette.len()).collect();
    for group in merged {
        let Some(first) = group.first() else {
            continue;
        };
        let Some(to) = palette_index(palette, *first) else {
            continue;
        };
        if distance2(palette[to], *first) > reach {
            continue;
        }
        for c in &group[1..] {
            if let Some(from) = palette_index(palette, *c) {
                if distance2(palette[from], *c) <= reach && from != to {
                    alias[from] = to;
                }
            }
        }
    }
    if alias.iter().enumerate().all(|(i, a)| i == *a) {
        return;
    }
    for l in labels.iter_mut() {
        if let Some(a) = alias.get(*l) {
            *l = *a;
        }
    }
}

/// Set the pixels of label `bg` that reach the picture's edge (through
/// pixels of that label, 4-connected) to transparent, so only the patches
/// of that colour inside the picture are left to trace.
fn clear_border_region(labels: &mut [usize], w: usize, h: usize, bg: usize) {
    if labels.len() != w * h {
        return;
    }
    let mut stack: Vec<usize> = Vec::new();
    let push = |i: usize, labels: &mut [usize], stack: &mut Vec<usize>| {
        if labels[i] == bg {
            labels[i] = usize::MAX;
            stack.push(i);
        }
    };
    for x in 0..w {
        push(x, labels, &mut stack);
        push((h - 1) * w + x, labels, &mut stack);
    }
    for y in 0..h {
        push(y * w, labels, &mut stack);
        push(y * w + w - 1, labels, &mut stack);
    }
    while let Some(i) = stack.pop() {
        let (x, y) = (i % w, i / w);
        if x > 0 {
            push(i - 1, labels, &mut stack);
        }
        if x + 1 < w {
            push(i + 1, labels, &mut stack);
        }
        if y > 0 {
            push(i - w, labels, &mut stack);
        }
        if y + 1 < h {
            push(i + w, labels, &mut stack);
        }
    }
}

/// The 8-connected patches of `mask` (`w` x `h`), each as the box of its
/// pixels (x0, y0, x1, y1, inclusive) and its own mask over that box.
fn patches(mask: &[bool], w: usize, h: usize) -> Vec<((usize, usize, usize, usize), Vec<bool>)> {
    let mut out = Vec::new();
    if mask.len() != w * h {
        return out;
    }
    let mut seen = vec![false; w * h];
    let mut stack: Vec<usize> = Vec::new();
    let mut pixels: Vec<usize> = Vec::new();
    for start in 0..w * h {
        if seen[start] || !mask[start] {
            continue;
        }
        seen[start] = true;
        stack.push(start);
        pixels.clear();
        let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
        while let Some(i) = stack.pop() {
            pixels.push(i);
            let (x, y) = (i % w, i / w);
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
            for dy in -1i64..=1 {
                for dx in -1i64..=1 {
                    let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                    if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                        continue;
                    }
                    let j = ny as usize * w + nx as usize;
                    if mask[j] && !seen[j] {
                        seen[j] = true;
                        stack.push(j);
                    }
                }
            }
        }
        let (pw, ph) = (x1 - x0 + 1, y1 - y0 + 1);
        let mut own = vec![false; pw * ph];
        for &i in &pixels {
            let (x, y) = (i % w, i / w);
            own[(y - y0) * pw + (x - x0)] = true;
        }
        out.push(((x0, y0, x1, y1), own));
    }
    out
}

// ----- outline mode -----------------------------------------------------------

fn outline(img: &RgbaImage, s: &Settings, cancel: Option<&Cancel>) -> Option<Vec<Traced>> {
    let (w, h) = (img.width() as usize, img.height() as usize);
    if w == 0 || h == 0 {
        return Some(Vec::new());
    }
    // Pixels of the picture for each pixel of the art, and an area of
    // the art in pixels of the picture.
    let enlarged = if s.pixel_scale.is_finite() {
        s.pixel_scale.clamp(1.0, 8.0)
    } else {
        1.0
    };
    let area = |a: usize| {
        if a <= 1 {
            a
        } else {
            (a as f64 * enlarged * enlarged).round() as usize
        }
    };
    // 1. Take out the noise of photos and compressed images below the
    // highest detail, so it makes no specks, holes and seams of its own
    // inside flat colours (#611); smooth a little more when detail is
    // low, then quantise.
    use crate::trace_progress::{report, Step};
    report(cancel, Step::Colors, 0.0);
    let mut quiet = img.clone();
    for _ in 0..denoise_passes(s.detail) {
        quiet = denoise(&quiet, cancel)?;
    }
    let mut src = if s.detail < 50.0 {
        image::imageops::blur(&quiet, (50.0 - s.detail as f32) / 25.0)
    } else {
        quiet
    };
    if cancelled(cancel) {
        return None;
    }
    if s.mode == ColorMode::Grayscale {
        for p in src.pixels_mut() {
            let l = luma(*p).round().clamp(0.0, 255.0) as u8;
            *p = Rgba([l, l, l, p[3]]);
        }
    }
    let black_white = s.mode == ColorMode::BlackWhite;
    let palette: Vec<[u8; 3]> = if black_white {
        vec![[0, 0, 0], [255, 255, 255]]
    } else {
        let n = s.colors.clamp(2, 256) as usize;
        art_palette(&src, n, cancel)?
    };
    let mut labels = vec![0usize; w * h];
    // Bands of rows on the computer's cores. Up to 256 colours to
    // compare each pixel with: look at the flag every few rows.
    let src_px: &[Rgba<u8>] = &src.pixels().copied().collect::<Vec<_>>();
    par_chunks(&mut labels, w * 32, |start, part| {
        if cancelled(cancel) {
            return;
        }
        for (k, at) in part.iter_mut().enumerate() {
            let p = src_px[start + k];
            *at = if p[3] < 128 {
                usize::MAX
            } else if black_white {
                if luma(p) < 128.0 {
                    0
                } else {
                    1
                }
            } else {
                nearest(&palette, [p[0], p[1], p[2]])
            };
        }
    });
    if cancelled(cancel) {
        return None;
    }
    // Thin bands of an in-between colour along the edge of two others
    // (the anti-aliasing of a black and yellow edge, the fringes of a
    // blurred or compressed one) go to the colours on either side, each
    // pixel to the one its own colour is nearer, so they make no slivers
    // of their own and the edge runs where the art's did (#611, #1250).
    // Before the specks, which would hand them out by their neighbours
    // and leave steps and bumps along the edge. Like the specks, kept at
    // the highest detail.
    let colors: Vec<[u8; 3]> = src.pixels().map(|p| [p[0], p[1], p[2]]).collect();
    // The pixels on the edge of two colours, each with the two colours
    // it blends (`trace_fine::Edges`), take the nearer of the two: no
    // third colour that looks like the blend, and a thin faint line is
    // as wide as it is in the picture (#2927, #2929). From the picture
    // itself, which the noise filter has not thinned.
    let raw_colors: Vec<[u8; 3]> = img.pixels().map(|p| [p[0], p[1], p[2]]).collect();
    let alpha: Vec<u8> = img.pixels().map(|p| p[3]).collect();
    report(cancel, Step::Colors, 0.3);
    let edges = crate::trace_fine::Edges::find(&raw_colors, &alpha, w, h, cancel)?;
    report(cancel, Step::Colors, 0.6);
    // One flat colour split in two by its noise, when the art has fewer
    // colours than asked for, is one colour again (#611).
    let palette = merge_split_colors(&mut labels, w, h, palette, &colors);
    // Shades no eye tells apart (the three blacks and seven whites the
    // colour reduction makes of a black and white drawing's noise) are
    // one colour, in any picture: each would be a curve of its own.
    // Rounds until nothing joins: shades joined change which others are
    // side by side.
    // Black and white stays #000000 and #FFFFFF: its two colours are
    // the mode's, not the picture's.
    let mut palette = palette;
    if !black_white {
        for _ in 0..4 {
            let before = labels.clone();
            palette = merge_near_colors(&mut labels, w, h, palette, &colors, near_reach(s.detail));
            if labels == before || cancelled(cancel) {
                break;
            }
        }
        snap_to_commonest(&labels, &colors, &mut palette);
    }
    // Colours no pixel has any more (joined into others) leave the
    // palette, so the background colour specified, picked with the
    // eyedropper or merged on the Colors tab is matched among the colours
    // of the trace only: a joined shade left behind stood nearer the
    // colour picked than the colour its pixels took, and nothing was
    // removed.
    let palette = compact_palette(&mut labels, palette);
    // At every detail: the in-between colours of an edge are no detail
    // of the art (#2929).
    settle_edges(&mut labels, w, h, &palette, &colors, cancel);
    dissolve_fringes_of(
        &mut labels,
        w,
        h,
        &palette,
        &colors,
        FRINGE_WIDTH * enlarged,
        // Geometric shapes: the thin bands of other colours a compressed
        // picture leaves along an edge go too, so the edge is one border.
        if s.geometric {
            LOOSE_FRINGE_WIDTH * enlarged
        } else {
            0.0
        },
        cancel,
    )?;
    // An end takes the label the cleaning left it when that is its
    // colour; else (the core of a thin line the cleaning took for noise)
    // the colour still in the picture nearest its own.
    {
        let mut used = vec![false; palette.len()];
        for l in &labels {
            if let Some(u) = used.get_mut(*l) {
                *u = true;
            }
        }
        let cleaned = labels.clone();
        crate::trace_fine::settle_on_ends(
            &mut labels,
            &edges,
            &raw_colors,
            &palette,
            |end, other| {
                let c = raw_colors[end];
                if black_white {
                    return (0.299 * c[0] as f32 + 0.587 * c[1] as f32 + 0.114 * c[2] as f32
                        >= 128.0) as usize;
                }
                let kept = cleaned[end];
                let true_end = palette
                    .get(kept)
                    .is_some_and(|p| distance2(*p, c) <= END_TRUE * END_TRUE);
                // Part of a stroke or an area of its colour: two or more of
                // the pixels around it are of it too. A pixel or two alone
                // (the gap in a small letter, lighter than the letter and
                // darker than the paper) may be a mix like any other.
                let (x, y) = (end % w, end / w);
                let alike = (y.saturating_sub(1)..=(y + 1).min(h - 1))
                    .flat_map(|v| {
                        (x.saturating_sub(1)..=(x + 1).min(w - 1)).map(move |u| v * w + u)
                    })
                    .filter(|j| *j != end && distance2(raw_colors[*j], c) <= END_ALIKE * END_ALIKE)
                    .count();
                if true_end && alike >= 2 {
                    return kept;
                }
                // The ringing of an enlarged edge (enlarging overshoots:
                // a darker rim along a colour's edge with a lighter one):
                // the colour it is the edge of lies between the colour
                // across the edge and its own, so it is that colour's.
                if enlarged > 1.0 && alike >= 2 && cleaned[other] != kept {
                    if let (Some(k), Some(a)) = (palette.get(kept), palette.get(cleaned[other])) {
                        if between(*k, *a, c) {
                            return kept;
                        }
                    }
                }
                // The core of a stroke too thin to reach its colour lies
                // part of the way from the colour around it (the other end)
                // to the stroke's: the nearest colour it lies on the way to,
                // at least [`THIN_SHARE`] of it; a grey hairline on white,
                // in a picture of black and white, is black (#2927).
                let around = palette.get(cleaned[other]).copied();
                let on_the_way = |p: [u8; 3]| {
                    let Some(a) = around else {
                        return false;
                    };
                    let Some((d, slack)) = off_segment(c, a, p) else {
                        return false;
                    };
                    let t = (0..3)
                        .map(|k| (c[k] as f64 - a[k] as f64) * (p[k] as f64 - a[k] as f64))
                        .sum::<f64>()
                        / (distance2(a, p) as f64).max(1.0);
                    d <= 2.0 * slack && t >= THIN_SHARE
                };
                let nearest = |way: bool| {
                    palette
                        .iter()
                        .enumerate()
                        .filter(|(i, p)| {
                            used[*i]
                                && (!way
                                    || (*i != cleaned[other]
                                        && !(true_end && *i == kept)
                                        && on_the_way(**p)))
                        })
                        .min_by_key(|(_, p)| distance2(**p, c))
                        .map(|(i, _)| i)
                };
                nearest(true)
                    .or_else(|| true_end.then_some(kept))
                    .or_else(|| nearest(false))
                    .unwrap_or(kept)
            },
        );
    }
    // Specks: patches of one colour smaller than the detail allows take
    // the colour around them, so stray dots and the fringes of soft edges
    // do not become curves of their own (#611).
    despeckle_of(
        &mut labels,
        w,
        h,
        area(speck_area(s.detail)),
        area(dot_area(s.detail)),
        (enlarged * enlarged).round() as usize,
        &palette,
        cancel,
    );
    if cancelled(cancel) {
        return None;
    }
    // The pale inside of a small letter open to the paper is paper.
    open_gaps(&mut labels, w, h, &palette, area(GAP_OPEN_AREA), cancel)?;
    // A small piece of a shade inside a small letter joins the shade
    // beside it.
    if !black_white && enlarged > 1.0 {
        join_shades(
            &mut labels,
            w,
            h,
            &palette,
            &raw_colors,
            area(SHADE_AREA),
            cancel,
        )?;
    }
    // Geometric shapes: the fringes the ends gave back go again, so a
    // shape's edge with the colour beside it is one border.
    if s.geometric {
        dissolve_fringes_of(
            &mut labels,
            w,
            h,
            &palette,
            &colors,
            0.0,
            LOOSE_FRINGE_WIDTH * enlarged,
            cancel,
        )?;
    }
    // The one-pixel bumps and notches left along the edges go, so the
    // curves fitted to them are smooth (#1250).
    if speck_area(s.detail) > 1 {
        smooth_labels(&mut labels, w, h, cancel)?;
    }
    let art = !black_white && flat_art(img);
    if art {
        full_thin_colors(&mut labels, w, h, &palette);
    }
    // The bands of a gradient are one region again, drawn as a blend
    // (#2929).
    let (palette, blends) = if art {
        crate::trace_blend::join_blends(&mut labels, w, h, palette, &raw_colors, &edges, cancel)?
    } else {
        let none = vec![None; palette.len()];
        (palette, none)
    };
    // 2. Background: the most common colour touching the border.
    let mut border_counts = vec![0usize; palette.len()];
    let mut border_clear = 0usize;
    for x in 0..w {
        for y in [0, h - 1] {
            let l = labels[y * w + x];
            if l != usize::MAX {
                border_counts[l] += 1;
            } else {
                border_clear += 1;
            }
        }
    }
    for y in 0..h {
        for x in [0, w - 1] {
            let l = labels[y * w + x];
            if l != usize::MAX {
                border_counts[l] += 1;
            } else {
                border_clear += 1;
            }
        }
    }
    if cancelled(cancel) {
        return None;
    }
    // Colours the Colors tab joined are one colour from here on; the
    // match must be close, so a palette that changed with the settings
    // does not join colours never meant to be.
    if !s.merged.is_empty() {
        merge_colors(&mut labels, &palette, &s.merged, MERGE_REACH * MERGE_REACH);
        if cancelled(cancel) {
            return None;
        }
    }
    let background = match s.remove {
        Remove::None => None,
        Remove::Background | Remove::EntireColor => match s.background {
            // The colour chosen: the palette colour nearest it.
            Some(c) => palette_index(&palette, c),
            // None when no colour touches the edge (a picture on a
            // transparent background has its background removed already):
            // the commonest of none was the last colour of the palette,
            // and Remove entire color took it out of the art (#1250).
            // The same when the edge is mostly transparent: the art
            // that runs off the edge (the stems of a floral pattern) is
            // the only colour there, and was removed as the background
            // (#1930).
            // And only paper: a dark or strongly coloured surround is part
            // of the design (the dark square behind a gold emblem), kept
            // unless its colour is specified (#2928).
            None => border_counts
                .iter()
                .enumerate()
                .filter(|(i, c)| {
                    **c > border_clear && palette.get(*i).is_some_and(|p| is_paper(*p))
                })
                .max_by_key(|(_, c)| **c)
                .map(|(i, _)| i),
        },
    };
    // Removing the background (not the entire colour) takes out the
    // patches of that colour reaching the picture's edge; patches of it
    // inside the picture (the white of an eye) are traced like any colour.
    // The background's soft shades are background too: from the
    // picture's edge on through every pixel whose colour runs on from
    // its neighbour's with no edge between, near the background colour
    // (a vignette, a faint gradient), so the whole of it goes, not the
    // part the colour reduction happened to give one colour (#2928).
    if let (Remove::Background, Some(bg)) = (s.remove, background) {
        spread_background(&mut labels, w, h, bg, &palette, &raw_colors, &edges);
    }
    // The curves of a labelling: made once, and again for each round of
    // the check below.
    let build = |labels: &[usize], mut background: Option<usize>| -> Option<Vec<Traced>> {
        // The background colour where it is left inside the picture (the
        // insides of letters, an O's counter), when it was found along the
        // edge: a hole it fills goes with the background (#4328).
        let mut paper = background.filter(|_| s.remove == Remove::Background);
        let hard = crate::trace_fine::hard_edges(labels, &edges, w, h);
        // The labels on a finer grid, where the edges run inside the pixels
        // blending two colours (`trace_fine`); everything from here on is in
        // that grid, and the curves are brought back to the picture's at the
        // end.
        // An enlarged picture has sharp edges already: twice as fine places
        // them within its pixels, and no finer grid is worth its time.
        let fine = crate::trace_fine::scale_for(w, h);
        let fine = if enlarged > 1.0 {
            fine.min(ENLARGED_FINE)
        } else {
            fine
        };
        // How soft the picture is: a soft one is sharpened before its edges
        // are placed, and its corners are taken to be rounded further.
        let soft = if hard {
            0.0
        } else {
            crate::trace_fine::soft_share(crate::trace_fine::softness(labels, &edges, w, h))
        };
        let mut labels = crate::trace_fine::finer_labels(
            labels,
            &raw_colors,
            &alpha,
            &edges,
            &palette,
            w,
            h,
            fine,
            soft,
            cancel,
        )?;
        let (w, h) = (w * fine, h * fine);
        if let (Remove::Background, Some(bg)) = (s.remove, background) {
            clear_border_region(&mut labels, w, h, bg);
            background = None;
            if !labels.contains(&usize::MAX) {
                paper = None;
            }
        }
        // The small thin letters of an enlarged picture are drawn again
        // by their strokes (`trace_strokes`).
        let redrawn = if enlarged > 1.0 {
            let corner =
                crate::trace_fine::Fit::of(s, fine as f64 * enlarged * ENLARGED_SHARP, hard, soft)
                    .corner;
            Some(crate::trace_strokes::redraw(
                &mut labels,
                w,
                h,
                enlarged * fine as f64,
                &crate::trace_fit::Faithful {
                    k: fine as f64,
                    tolerance: FAITHFUL_TOLERANCE,
                    corner,
                    scale: enlarged,
                },
                &palette,
                cancel,
            )?)
        } else {
            None
        };
        // 3. The box of each colour's pixels, in one pass.
        let mut boxes = vec![(w, h, 0usize, 0usize); palette.len()];
        for (i, l) in labels.iter().enumerate() {
            if let Some(b) = boxes.get_mut(*l) {
                let (x, y) = (i % w, i / w);
                *b = (b.0.min(x), b.1.min(y), b.2.max(x), b.3.max(y));
            }
        }
        let present: Vec<bool> = boxes.iter().map(|b| b.0 <= b.2).collect();
        // The background taken out is no region.
        let regions: std::borrow::Cow<[usize]> = match background {
            Some(bg) => labels
                .iter()
                .map(|l| if *l == bg { usize::MAX } else { *l })
                .collect::<Vec<usize>>()
                .into(),
            None => labels.as_slice().into(),
        };
        // The order the colours stack in: a colour inside another's holes
        // (the eyes in a head) above it, so the one around is drawn whole
        // under it, as one ring; else darkest first (#1250).
        let luma: Vec<u32> = palette
            .iter()
            .map(|c| (c[0] as u32 * 299 + c[1] as u32 * 587 + c[2] as u32 * 114) / 1000)
            .collect();
        // The holes of each colour with no page showing in them: their
        // pixels (in the colour's box) and the colours in them.
        let mut holes: Vec<Vec<(Vec<u32>, Vec<(usize, usize)>)>> = vec![Vec::new(); palette.len()];
        let order = if s.remove_overlap {
            let mut order: Vec<usize> = (0..palette.len()).collect();
            order.sort_by_key(|i| luma[*i]);
            order
        } else {
            // How many pixels of each colour lie in the holes of each other.
            let mut weight: Vec<Vec<usize>> = vec![vec![0; palette.len()]; palette.len()];
            let mut seen = vec![0usize; palette.len()];
            // A hole no bigger than a speck (half a pixel of the art) is
            // one: the region covers it whatever is drawn in it.
            let speck = 2 * fine * fine;
            for ci in 0..palette.len() {
                if !present[ci] || Some(ci) == background {
                    continue;
                }
                if cancelled(cancel) {
                    return None;
                }
                let (x0, y0, x1, y1) = boxes[ci];
                let (cw, ch) = (x1 - x0 + 1, y1 - y0 + 1);
                let label_at = |i: usize| regions[(y0 + i / cw) * w + x0 + i % cw];
                let own: Vec<bool> = (0..cw * ch).map(|i| label_at(i) == ci).collect();
                for_each_hole(&own, cw, ch, |hole| {
                    seen.fill(0);
                    for &i in hole {
                        match seen.get_mut(label_at(i)) {
                            Some(v) => *v += 1,
                            // The page shows in this hole: it stays one.
                            None => return,
                        }
                    }
                    let colours: Vec<(usize, usize)> = if hole.len() < speck {
                        Vec::new()
                    } else {
                        (0..seen.len())
                            .filter(|l| seen[*l] > 0)
                            .map(|l| (l, seen[l]))
                            .collect()
                    };
                    for &(l, n) in &colours {
                        weight[ci][l] += n;
                    }
                    holes[ci].push((hole.iter().map(|i| *i as u32).collect(), colours));
                });
            }
            // Two colours each in the other's holes: the one holding more
            // of the other is the one around. A few pixels of the ground
            // left between two colours of the art (blots of a compressed
            // picture, the blend at a thin gap) made the ground "inside"
            // a ring it holds; the ring went under it, each was cut to the
            // edge they share and the page showed between them as a light
            // line (#3727). Black eyes in a head inside a black outline
            // still go by darkness when they weigh alike.
            let n = palette.len();
            let mut inside: Vec<Vec<bool>> = vec![vec![false; n]; n];
            for a in 0..n {
                for b in 0..n {
                    let (ab, ba) = (weight[a][b], weight[b][a]);
                    inside[a][b] = ab > 0 && ab * MUTUAL_RATIO > ba;
                }
            }
            stacking_order(&inside, &luma)
        };
        let fit = crate::trace_fine::Fit::of(
            s,
            fine as f64
                * if enlarged > 1.0 {
                    enlarged * ENLARGED_SHARP
                } else {
                    1.0
                },
            hard,
            soft,
        );
        let mut fit = fit;
        if enlarged > 1.0 {
            fit.tolerance *= ENLARGED_TOLERANCE;
            fit.reach *= ENLARGED_REACH;
            fit.cap_near = ENLARGED_CAP_NEAR;
        }
        let fit = fit;
        let mut rank = vec![0usize; palette.len()];
        for (k, ci) in order.iter().enumerate() {
            rank[*ci] = k;
        }
        // The borders between the regions, each fitted once: every region's
        // rings are put together from them, so where two regions meet their
        // outlines are the same curve, with no gap between them and no second
        // curve beside the first (#611, #1250).
        // An enlarged picture's borders are fitted faithfully, each checked
        // against the border it was made from (`trace_fit`).
        let faithful = (enlarged > 1.0 || s.geometric).then(|| crate::trace_fit::Faithful {
            k: fine as f64,
            tolerance: FAITHFUL_TOLERANCE * fit.tolerance / ENLARGED_TOLERANCE,
            corner: fit.corner,
            scale: enlarged as f64,
        });
        // How much of the picture each region covers: of the two sides
        // of a border the lesser is the shape, the greater its ground.
        let mut covers = vec![0usize; palette.len()];
        for l in regions.iter() {
            if let Some(c) = covers.get_mut(*l) {
                *c += 1;
            }
        }
        // The colour of each region for placing its edges: none for a
        // blend, whose colour changes along the edge.
        let flat: Vec<Option<[f32; 3]>> = palette
            .iter()
            .enumerate()
            .map(|(i, c)| {
                blends
                    .get(i)
                    .is_none_or(|b| b.is_none())
                    .then(|| c.map(|v| v as f32))
            })
            .collect();
        let source = faithful.as_ref().map(|f| FaithfulSource {
            fit: f,
            colors: &raw_colors,
            w: w / fine,
            h: h / fine,
            flat: &flat,
            covers: &covers,
            redrawn: redrawn.as_deref(),
        });
        report(cancel, Step::Curves, 0.0);
        let geometric = s.geometric.then_some(fine as f64 * enlarged);
        let shared = SharedBorders::new(&regions, w, h, &fit, source.as_ref(), geometric, cancel)?;
        report(cancel, Step::Curves, 0.5);
        // A hole that the background colour fills (more than a few specks
        // of it, and most of the hole) is a counter, the inside of an O:
        // it stays a hole in the shape around, and the background colour
        // in it goes with the rest of the background, so a letter comes
        // out as one shape with a real hole, ready to cut, with nothing
        // stacked on it (#4328). Specks of the background colour (the
        // noise of a compressed picture inside a stroke) are no counter.
        let blots_max = BLOTS_AREA * fine * fine;
        let counter = |colours: &[(usize, usize)], len: usize| {
            colours
                .iter()
                .any(|(l, n)| Some(*l) == paper && *n > blots_max && 2 * *n >= len)
        };
        let mut counters = vec![false; if paper.is_some() { w * h } else { 0 }];
        if let Some(p) = paper {
            for ci in 0..palette.len() {
                let (x0, y0, x1, _) = boxes[ci];
                let cw = x1.wrapping_sub(x0).wrapping_add(1);
                for (pixels, colours) in &holes[ci] {
                    if counter(colours, pixels.len()) {
                        for &i in pixels {
                            let g = (y0 + i as usize / cw) * w + x0 + i as usize % cw;
                            if labels[g] == p {
                                counters[g] = true;
                            }
                        }
                    }
                }
            }
        }
        let mut out = Vec::new();
        let colours = order.len().max(1);
        for (k, ci) in order.into_iter().enumerate() {
            report(cancel, Step::Curves, 0.5 + 0.5 * k as f64 / colours as f64);
            if Some(ci) == background || !present[ci] {
                continue;
            }
            if cancelled(cancel) {
                return None;
            }
            let (x0, y0, x1, y1) = boxes[ci];
            let (cw, ch) = (x1 - x0 + 1, y1 - y0 + 1);
            let label_at = |i: usize| regions[(y0 + i / cw) * w + x0 + i % cw];
            let own: Vec<bool> = (0..cw * ch).map(|i| label_at(i) == ci).collect();
            // Stacked: the holes of the region holding only regions drawn
            // above it are part of it, so its curve is the outline alone and
            // the edges inside are drawn once, by the regions on top, which
            // leave no page showing between them and it. Remove overlap keeps
            // every region to its own pixels (no holes were looked for).
            let mut mask = own;
            // The background colour in counters is taken out with the
            // background.
            if Some(ci) == paper {
                for (i, m) in mask.iter_mut().enumerate() {
                    if counters[(y0 + i / cw) * w + x0 + i % cw] {
                        *m = false;
                    }
                }
                if !mask.contains(&true) {
                    continue;
                }
            }
            for (pixels, colours) in &holes[ci] {
                // A counter stays a hole (#4328).
                if counter(colours, pixels.len()) {
                    continue;
                }
                // Filled when what is drawn below the region in it is no
                // more than a few blots: they go under it, hidden, and the
                // regions drawn over it meet it with no page between. Thin
                // lines in a large hole are no blots, however small their
                // share of it: filled over, they were lost (#4327).
                let below: usize = colours
                    .iter()
                    .filter(|(l, _)| rank[*l] <= rank[ci])
                    .map(|(_, n)| *n)
                    .sum();
                if below * BLOTS_SHARE <= pixels.len() && below <= blots_max {
                    for &i in pixels {
                        mask[i as usize] = true;
                    }
                }
            }
            // One curve for the colour (every region combined), or one per
            // separate patch of it.
            let pieces: Vec<((usize, usize), usize, usize, Vec<bool>)> = if s.merge_adjacent {
                vec![((x0, y0), cw, ch, mask)]
            } else {
                patches(&mask, cw, ch)
                    .into_iter()
                    .map(|((px0, py0, px1, py1), own)| {
                        ((x0 + px0, y0 + py0), px1 - px0 + 1, py1 - py0 + 1, own)
                    })
                    .collect()
            };
            let c = s.shown_color(palette[ci]);
            for ((ox, oy), pw, ph, piece) in pieces {
                if cancelled(cancel) {
                    return None;
                }
                let path = shared.rings(&piece, (ox, oy), pw, ph, cancel)?;
                if path.elements().is_empty() {
                    continue;
                }
                let path = tracedraw_core::geometry::Affine::scale(1.0 / fine as f64) * path;
                out.push(Traced {
                    path,
                    color: Color::rgb8(c[0], c[1], c[2]),
                    closed: true,
                    blend: blends
                        .get(ci)
                        .copied()
                        .flatten()
                        .filter(|_| c == palette[ci]),
                });
            }
        }
        Some(out)
    };
    let mut out = build(&labels, background)?;
    // The check: the curves are drawn and compared with the picture, and
    // where a patch of them shows another colour than the picture does
    // (the gap of a small letter the cleaning closed), its pixels take
    // the colours the picture has there and the curves are made again.
    if enlarged > 1.0 {
        let removed = background.filter(|_| s.remove != Remove::None);
        let least = (enlarged * enlarged * CHECK_AREA).round() as usize;
        for round in 0..CHECK_ROUNDS {
            report(cancel, Step::Check, round as f64 / CHECK_ROUNDS as f64);
            let (wrong, drawn) = differs(img, &out, &palette, removed, cancel)?;
            let changes = recheck(img, &wrong, &drawn, &mut labels, &palette, &blends, least);
            if changes.is_empty() {
                break;
            }
            // The change is checked the same way: a patch that still
            // differs from the picture goes back to what it was.
            let again = build(&labels, background)?;
            let (after, _) = differs(img, &again, &palette, removed, cancel)?;
            let (mut kept, mut undone) = (0usize, 0usize);
            for c in &changes {
                let still = c
                    .pixels
                    .iter()
                    .filter(|i| after.get(**i) == Some(&true))
                    .count();
                if still * 2 > c.pixels.len() {
                    for (i, l) in c.pixels.iter().zip(&c.was) {
                        if let Some(at) = labels.get_mut(*i) {
                            *at = *l;
                        }
                    }
                    undone += 1;
                } else {
                    kept += 1;
                }
            }
            if kept == 0 {
                break;
            }
            out = if undone == 0 {
                again
            } else {
                build(&labels, background)?
            };
        }
    }
    Some(out)
}

/// How many times more of one colour must lie in the other's holes than
/// of the other in its for the first to be taken as the one around.
const MUTUAL_RATIO: usize = 4;
/// A hole of a region is filled when the colours drawn below the region
/// cover at most this share of it (one part in this many).
const BLOTS_SHARE: usize = 20;
/// And when they hold no more than this many pixels of the picture: more
/// is a drawing of its own (the fine lines of a quill in a logo), which a
/// filled hole hid (#4327).
const BLOTS_AREA: usize = 40;

/// How many times at most a trace is drawn, compared with its picture
/// and made again where it differs.
const CHECK_ROUNDS: usize = 2;
/// The smallest patch (pixels of the art) drawn in another colour than
/// the picture's that is made again.
const CHECK_AREA: f64 = 0.75;
/// How many times nearer the picture's colours the colour a patch is
/// given must be than the colours it had.
const CHECK_NEARER: f64 = 0.6;

/// A patch of pixels the check gave another label, and the labels they
/// had.
struct Relabelled {
    pixels: Vec<usize>,
    was: Vec<usize>,
}

/// Draw `traced` and compare it with the picture it was traced from:
/// which pixels differ ([`WRONG_DISTANCE`], an edge a pixel off aside),
/// and the colours drawn. `removed` is the background taken out, shown
/// where nothing is drawn. `None` once `cancel` is set.
fn differs(
    img: &RgbaImage,
    traced: &[Traced],
    palette: &[[u8; 3]],
    removed: Option<usize>,
    cancel: Option<&Cancel>,
) -> Option<(Vec<bool>, Vec<[f32; 3]>)> {
    let (w, h) = (img.width() as usize, img.height() as usize);
    if w == 0 || h == 0 {
        return Some((Vec::new(), Vec::new()));
    }
    let drawn = render_traced(traced, w as u32, h as u32, 1.0);
    if cancelled(cancel) {
        return None;
    }
    let under: [f32; 3] = match removed.and_then(|b| palette.get(b)) {
        Some(c) => c.map(|v| v as f32),
        None => [255.0; 3],
    };
    let over = |p: &Rgba<u8>| -> [f32; 3] {
        let k = p[3] as f32 / 255.0;
        std::array::from_fn(|c| p[c] as f32 * k + under[c] * (1.0 - k))
    };
    let pa: Vec<[f32; 3]> = img.pixels().map(over).collect();
    let pb: Vec<[f32; 3]> = drawn.pixels().map(over).collect();
    if pa.len() != w * h || pb.len() != w * h {
        return Some((Vec::new(), Vec::new()));
    }
    let limit = (WRONG_DISTANCE * WRONG_DISTANCE) as f32;
    let far = |x: usize, y: usize, c: [f32; 3]| {
        (y.saturating_sub(1)..=(y + 1).min(h - 1)).all(|v| {
            (x.saturating_sub(1)..=(x + 1).min(w - 1)).all(|u| {
                let q = pb[v * w + u];
                (0..3).map(|k| (q[k] - c[k]).powi(2)).sum::<f32>() > limit
            })
        })
    };
    let mut wrong = vec![false; w * h];
    for y in 0..h {
        if y % 32 == 0 && cancelled(cancel) {
            return None;
        }
        for x in 0..w {
            let i = y * w + x;
            wrong[i] = img.as_raw()[i * 4 + 3] >= 128 && far(x, y, pa[i]);
        }
    }
    Some((wrong, pb))
}

/// The second half of the check: each patch of `min_area` pixels or more
/// of `wrong` (see [`differs`]; `pb` the colours drawn) takes the label
/// of the one colour the picture is nearest there, when that is much
/// nearer ([`CHECK_NEARER`]) and no shade of what is drawn. Gives the
/// patches changed.
fn recheck(
    img: &RgbaImage,
    wrong: &[bool],
    pb: &[[f32; 3]],
    labels: &mut [usize],
    palette: &[[u8; 3]],
    blends: &[Option<crate::trace_blend::Blend>],
    min_area: usize,
) -> Vec<Relabelled> {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let mut changes = Vec::new();
    if labels.len() != w * h || wrong.len() != w * h || pb.len() != w * h || w < 3 || h < 3 {
        return changes;
    }
    let limit = (WRONG_DISTANCE * WRONG_DISTANCE) as f32;
    let mut used = vec![false; palette.len()];
    for l in labels.iter() {
        if let Some(u) = used.get_mut(*l) {
            *u = true;
        }
    }
    let mut seen = vec![false; w * h];
    let (mut stack, mut patch) = (Vec::new(), Vec::new());
    for start in 0..w * h {
        if !wrong[start] || seen[start] {
            continue;
        }
        seen[start] = true;
        stack.push(start);
        patch.clear();
        while let Some(i) = stack.pop() {
            patch.push(i);
            let (x, y) = (i % w, i / w);
            for v in y.saturating_sub(1)..=(y + 1).min(h - 1) {
                for u in x.saturating_sub(1)..=(x + 1).min(w - 1) {
                    let j = v * w + u;
                    if wrong[j] && !seen[j] {
                        seen[j] = true;
                        stack.push(j);
                    }
                }
            }
        }
        if patch.len() < min_area.max(2) {
            continue;
        }
        // How far a pixel's colour is from a label's: from the nearest
        // colour of its blend when it is drawn as one.
        let off = |i: usize, k: usize| -> f64 {
            let p = img.as_raw();
            let c = [p[i * 4], p[i * 4 + 1], p[i * 4 + 2]];
            let Some(q) = palette.get(k).copied() else {
                return f64::MAX;
            };
            let flat = distance2(q, c) as f64;
            match blends.get(k).copied().flatten() {
                Some(b) => off_segment(c, b.start, b.end)
                    .map_or(flat, |(d, _)| d * d)
                    .min(flat),
                None => flat,
            }
            .sqrt()
        };
        // The whole patch takes the one colour its pixels are nearest
        // in all, when that is much nearer than the colours they have.
        let now: f64 = patch.iter().map(|i| off(*i, labels[*i])).sum();
        let best = (0..palette.len())
            .filter(|k| used[*k])
            .map(|k| (patch.iter().map(|i| off(*i, k)).sum::<f64>(), k))
            .min_by(|x, y| x.0.total_cmp(&y.0));
        if let Some((sum, k)) = best {
            // A shade of what is drawn there already is no wrong shape:
            // as a patch of its own it would be a seam inside the shape.
            let mean: [f32; 3] = std::array::from_fn(|c| {
                patch.iter().map(|i| pb[*i][c]).sum::<f32>() / patch.len() as f32
            });
            let shade = palette.get(k).is_some_and(|q| {
                (0..3).map(|c| (q[c] as f32 - mean[c]).powi(2)).sum::<f32>() <= limit
            });
            if !shade && sum <= CHECK_NEARER * now && patch.iter().any(|i| labels[*i] != k) {
                let was = patch.iter().map(|i| labels[*i]).collect();
                for &i in &patch {
                    labels[i] = k;
                }
                changes.push(Relabelled {
                    pixels: patch.clone(),
                    was,
                });
            }
        }
    }
    changes
}

/// How near (RGB distance) the colour of an edge's end must be to the
/// colour of its label for the label to stand.
const END_TRUE: u32 = 48;
/// How near (RGB distance) a pixel beside an edge's end must be to it to
/// be of the same stroke or area.
const END_ALIKE: u32 = 24;

/// Whether a colour along the picture's edge is paper: light and nearly
/// grey, the surround a drawing is scanned or exported on.
fn is_paper(c: [u8; 3]) -> bool {
    let luma = (c[0] as u32 * 299 + c[1] as u32 * 587 + c[2] as u32 * 114) / 1000;
    let (lo, hi) = (c[0].min(c[1]).min(c[2]), c[0].max(c[1]).max(c[2]));
    luma >= PAPER_LIGHT && (hi - lo) as u32 <= PAPER_TINT
}

/// The darkest paper, and the most colour it has (the widest gap between
/// two of its channels).
const PAPER_LIGHT: u32 = 180;
const PAPER_TINT: u32 = 48;

/// How far (RGB distance) the background's soft shades may lie from its
/// colour, and one such pixel from the next.
const BACKGROUND_NEAR: u32 = 56;
const BACKGROUND_STEP: u32 = 10;

/// Give the label `bg` to the pixels the background runs on into from
/// the picture's edge: those of its own label there, and from them every
/// pixel within [`BACKGROUND_NEAR`] of the background colour whose colour
/// is within [`BACKGROUND_STEP`] of its neighbour's and that lies on no
/// edge (`edges`).
fn spread_background(
    labels: &mut [usize],
    w: usize,
    h: usize,
    bg: usize,
    palette: &[[u8; 3]],
    colors: &[[u8; 3]],
    edges: &crate::trace_fine::Edges,
) {
    let Some(&paper) = palette.get(bg) else {
        return;
    };
    if labels.len() != w * h || colors.len() != w * h {
        return;
    }
    let mut seen = vec![false; w * h];
    let mut stack: Vec<usize> = Vec::new();
    let border = (0..w)
        .flat_map(|x| [x, (h - 1) * w + x])
        .chain((0..h).flat_map(|y| [y * w, y * w + w - 1]));
    for i in border {
        if labels[i] == bg && !seen[i] {
            seen[i] = true;
            stack.push(i);
        }
    }
    let near = |j: usize, i: usize| {
        [
            (!j.is_multiple_of(w)).then(|| j - 1),
            (j % w + 1 < w).then(|| j + 1),
            (j >= w).then(|| j - w),
            (j + w < w * h).then(|| j + w),
        ]
        .into_iter()
        .flatten()
        .filter(move |n| *n != i)
    };
    // What each pixel was before the background took it.
    let was = labels.to_vec();
    let mut reached: Vec<usize> = Vec::new();
    while let Some(i) = stack.pop() {
        reached.push(i);
        for j in near(i, usize::MAX) {
            if seen[j] || labels[j] == usize::MAX {
                continue;
            }
            let soft = edges.at(j).is_none()
                && distance2(colors[j], paper) <= BACKGROUND_NEAR * BACKGROUND_NEAR
                && distance2(colors[j], colors[i]) <= BACKGROUND_STEP * BACKGROUND_STEP;
            if labels[j] == bg || soft {
                seen[j] = true;
                labels[j] = bg;
                stack.push(j);
            }
        }
    }
    // A shade of the background all of whose flat pixels went leaves its
    // edge pixels behind, a pale ring round the art: they go with it.
    let mut flat = vec![false; palette.len()];
    for (i, l) in labels.iter().enumerate() {
        if edges.at(i).is_none() {
            if let Some(f) = flat.get_mut(*l) {
                *f = true;
            }
        }
    }
    let gone = |l: usize| {
        l != bg
            && flat.get(l) == Some(&false)
            && palette
                .get(l)
                .is_some_and(|c| distance2(*c, paper) <= BACKGROUND_NEAR * BACKGROUND_NEAR)
    };
    // So do the edge pixels of a shade beside the pixels of it that
    // went, when flat pixels of the shade stay elsewhere (inside the
    // letters): in a picture traced on its own pixels they were a pale
    // line round every letter.
    let rim = |j: usize, i: usize| {
        edges.at(j).is_some()
            && was[j] != bg
            && was[j] == was[i]
            && palette
                .get(was[j])
                .is_some_and(|c| distance2(*c, paper) <= BACKGROUND_NEAR * BACKGROUND_NEAR)
    };
    while let Some(i) = reached.pop() {
        for j in near(i, usize::MAX) {
            if !seen[j] && (gone(labels[j]) || rim(j, i)) {
                seen[j] = true;
                labels[j] = bg;
                reached.push(j);
            }
        }
    }
}

/// Whether `img` is art drawn in flat colours: at least [`FLAT_SHARE`]
/// of its opaque pixels lie inside one (their eight neighbours within
/// [`FLAT_NOISE`] of them).
fn flat_art(img: &RgbaImage) -> bool {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let raw = img.as_raw();
    let at = |i: usize| [raw[i * 4], raw[i * 4 + 1], raw[i * 4 + 2]];
    let opaque = |i: usize| raw[i * 4 + 3] >= 128;
    let (mut all, mut flat) = (0usize, 0usize);
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if !opaque(i) {
                continue;
            }
            all += 1;
            if x == 0 || y == 0 || x + 1 == w || y + 1 == h {
                continue;
            }
            let c = at(i);
            flat += (y - 1..=y + 1).all(|v| {
                (x - 1..=x + 1).all(|u| {
                    let j = v * w + u;
                    opaque(j) && distance2(at(j), c) <= FLAT_NOISE * FLAT_NOISE
                })
            }) as usize;
        }
    }
    all > 0 && flat as f64 >= FLAT_SHARE * all as f64
}

/// How far along the way from the colour around it to a full colour a
/// thin colour must lie to be that colour drawn thin.
const THIN_SHARE: f64 = 0.3;

/// The colours of thin strokes take the full colour they are a faint
/// copy of (#2927): a colour with no flat pixel anywhere (every stroke
/// of it thinner than three pixels, so the picture's blur never lets it
/// reach its colour) that lies between the colour most around it and a
/// colour with flat areas, at least [`THIN_SHARE`] of the way, is that
/// colour: the grey small print of a black and white logo is black.
fn full_thin_colors(labels: &mut [usize], w: usize, h: usize, palette: &[[u8; 3]]) {
    let n = palette.len();
    if labels.len() != w * h || n < 3 {
        return;
    }
    let flat = flat_pixels(labels, w, h);
    let mut has_flat = vec![false; n];
    for (l, f) in labels.iter().zip(&flat) {
        if let Some(v) = has_flat.get_mut(*l) {
            *v |= *f;
        }
    }
    let shared = shared_edges(labels, w, h, n);
    let mut alias: Vec<usize> = (0..n).collect();
    for g in 0..n {
        if has_flat[g] {
            continue;
        }
        let Some((around, _)) = (0..n)
            .filter(|a| *a != g && has_flat[*a])
            .map(|a| (a, shared[g * n + a]))
            .filter(|(_, e)| *e > 0)
            .max_by_key(|(a, e)| (*e, usize::MAX - *a))
        else {
            continue;
        };
        let (pg, pa) = (palette[g], palette[around]);
        let mut best: Option<(f64, usize)> = None;
        for b in (0..n).filter(|b| *b != g && *b != around && has_flat[*b]) {
            let pb = palette[b];
            let Some((d, slack)) = off_segment(pg, pa, pb) else {
                continue;
            };
            let len2 = distance2(pa, pb) as f64;
            let t = (0..3)
                .map(|k| (pg[k] as f64 - pa[k] as f64) * (pb[k] as f64 - pa[k] as f64))
                .sum::<f64>()
                / len2.max(1.0);
            if d <= 2.0 * slack && t >= THIN_SHARE && best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, b));
            }
        }
        if let Some((_, b)) = best {
            alias[g] = b;
        }
    }
    for l in labels.iter_mut() {
        if let Some(a) = alias.get(*l) {
            *l = *a;
        }
    }
}

/// How near (RGB distance) a colour of the Colors tab must come to a
/// palette colour to stand for it.
const MERGE_REACH: u32 = 12;

/// The boundary rings of `mask` (`w` x `h`, at `offset` in the picture)
/// as one path: each ring fitted, or simplified to straight lines without
/// smoothing.
#[cfg(test)]
fn rings_path(
    mask: &[bool],
    w: usize,
    h: usize,
    offset: tracedraw_core::geometry::Vec2,
    _tolerance: f64,
    s: &Settings,
    cancel: Option<&Cancel>,
) -> Option<BezPath> {
    let labels: Vec<usize> = mask
        .iter()
        .map(|m| if *m { 0 } else { usize::MAX })
        .collect();
    let fitted = SharedBorders::new(
        &labels,
        w,
        h,
        &crate::trace_fine::Fit::of(s, 1.0, true, 0.0),
        None,
        None,
        cancel,
    )?;
    let path = fitted.rings(mask, (0, 0), w, h, cancel)?;
    Some(tracedraw_core::geometry::Affine::translate(offset) * path)
}

// ----- shared borders -------------------------------------------------------------

/// The unit steps along the pixel edges, from a pixel corner: east,
/// south, west, north (picture space, y down).
const STEPS: [(i64, i64); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];

/// The pixels on either side of the step `d` from the corner `(x, y)`:
/// the `a` side first (the side a region's rings keep it on, as in
/// clockwise rings on screen), then the `b` side.
fn step_sides(x: i64, y: i64, d: u8) -> ((i64, i64), (i64, i64)) {
    match d {
        0 => ((x, y), (x, y - 1)),
        1 => ((x - 1, y), (x, y)),
        2 => ((x - 1, y - 1), (x - 1, y)),
        _ => ((x, y - 1), (x - 1, y - 1)),
    }
}

/// A stretch of the border between two regions of a labelling, along the
/// pixel edges: from one junction to the next (a pixel corner where
/// three regions meet, or two meet diagonally), or round a loop with no
/// junction (an island inside one other region). The region with the
/// smaller label is on its `a` side.
struct Border {
    /// The pixel corner it starts at and the one it ends at.
    start: (i64, i64),
    end: (i64, i64),
    /// The direction of each unit step (an index into [`STEPS`]).
    steps: Vec<u8>,
    /// Round a loop: it ends where it starts, with no junction on it.
    closed: bool,
    /// The pixel on each side of its first step (`None` outside the
    /// picture).
    a: Option<usize>,
    b: Option<usize>,
}

impl Border {
    /// The pixel corners it passes, from its start to its end.
    fn corners(&self) -> Vec<Point> {
        let mut out = Vec::with_capacity(self.steps.len() + 1);
        let (mut x, mut y) = self.start;
        out.push(Point::new(x as f64, y as f64));
        for d in &self.steps {
            let (dx, dy) = STEPS[*d as usize & 3];
            x += dx;
            y += dy;
            out.push(Point::new(x as f64, y as f64));
        }
        out
    }
}

/// The borders between the regions of `labels` (`w` x `h`; `usize::MAX`
/// is no region: transparent, or the background taken out). Every edge
/// between two pixels of different labels, and between a region and the
/// picture's edge, is on exactly one border. `None` once `cancel` is set.
fn borders(labels: &[usize], w: usize, h: usize, cancel: Option<&Cancel>) -> Option<Vec<Border>> {
    let mut out = Vec::new();
    if labels.len() != w * h || w == 0 || h == 0 {
        return Some(out);
    }
    // The labels with a frame of no region round them, so the four pixels
    // round every pixel corner can be read without looking at the edges.
    let pw = w + 2;
    let mut framed = vec![usize::MAX; pw * (h + 2)];
    for y in 0..h {
        framed[(y + 1) * pw + 1..(y + 1) * pw + 1 + w].copy_from_slice(&labels[y * w..(y + 1) * w]);
    }
    // For each pixel corner, the steps from it along an edge between two
    // labels, walked one way only, with the smaller label on the step's
    // `a` side (bits 0 to 3, see [`step_sides`]), and whether it is a
    // junction (bit 4): three labels or more round it, or two diagonally.
    const JUNCTION: u8 = 1 << 4;
    let cw = w + 1;
    let mut info = vec![0u8; cw * (h + 1)];
    for y in 0..=h {
        if y % 64 == 0 && cancelled(cancel) {
            return None;
        }
        let (r0, r1) = (y * pw, (y + 1) * pw);
        for x in 0..=w {
            let (tl, tr, bl, br) = (
                framed[r0 + x],
                framed[r0 + x + 1],
                framed[r1 + x],
                framed[r1 + x + 1],
            );
            if tl == tr && tl == bl && tl == br {
                continue;
            }
            let outs = (br < tr) as u8
                | ((bl < br) as u8) << 1
                | ((tl < bl) as u8) << 2
                | ((tr < tl) as u8) << 3;
            let distinct = 1
                + (tr != tl) as usize
                + (bl != tl && bl != tr) as usize
                + (br != tl && br != tr && br != bl) as usize;
            let junction = distinct >= 3 || (distinct == 2 && tl == br && tr == bl);
            info[y * cw + x] = outs | if junction { JUNCTION } else { 0 };
        }
    }
    let pixel = |(x, y): (i64, i64)| -> Option<usize> {
        (x >= 0 && y >= 0 && x < w as i64 && y < h as i64).then(|| y as usize * w + x as usize)
    };
    let corner = |x: i64, y: i64| y as usize * cw + x as usize;
    // The steps already walked, per corner.
    let mut used = vec![0u8; cw * (h + 1)];
    let limit = 4 * cw * (h + 1);
    let walk = |x0: i64, y0: i64, d0: u8, used: &mut [u8]| -> Border {
        let (a, b) = step_sides(x0, y0, d0);
        let mut steps = vec![d0];
        used[corner(x0, y0)] |= 1 << d0;
        let (mut x, mut y) = (x0 + STEPS[d0 as usize].0, y0 + STEPS[d0 as usize].1);
        let mut closed = false;
        while steps.len() <= limit {
            let here = info[corner(x, y)];
            if (x, y) == (x0, y0) {
                closed = here & JUNCTION == 0;
                break;
            }
            if here & JUNCTION != 0 {
                break;
            }
            let free = here & 0xF & !used[corner(x, y)];
            if free == 0 {
                break;
            }
            let d = free.trailing_zeros() as u8;
            used[corner(x, y)] |= 1 << d;
            steps.push(d);
            x += STEPS[d as usize].0;
            y += STEPS[d as usize].1;
        }
        Border {
            start: (x0, y0),
            end: (x, y),
            steps,
            closed,
            a: pixel(a),
            b: pixel(b),
        }
    };
    // The borders from junction to junction, then the loops.
    for pass in 0..2 {
        for y in 0..=h {
            if y % 64 == 0 && cancelled(cancel) {
                return None;
            }
            for x in 0..=w {
                let i = y * cw + x;
                if info[i] == 0 || (pass == 0 && info[i] & JUNCTION == 0) {
                    continue;
                }
                loop {
                    let free = info[i] & 0xF & !used[i];
                    if free == 0 {
                        break;
                    }
                    let d = free.trailing_zeros() as u8;
                    out.push(walk(x as i64, y as i64, d, &mut used));
                }
            }
        }
    }
    Some(out)
}

/// The side of the squares [`SharedBorders`] files its borders in, pixels.
const BORDER_TILE: i64 = 64;

/// The borders of a labelling, each fitted once ([`fit_border`]), and the
/// rings of any region made of whole regions of it put together from
/// them: where two regions meet, both use the same curve, one forwards,
/// the other backwards, so their outlines coincide exactly, with no gap
/// and no second curve beside the first (#611, #1250).
struct SharedBorders {
    w: usize,
    borders: Vec<Border>,
    curves: Vec<BezPath>,
    /// The borders starting in each square of [`BORDER_TILE`].
    tiles: Vec<Vec<u32>>,
    tiles_across: usize,
}

impl SharedBorders {
    fn new(
        labels: &[usize],
        w: usize,
        h: usize,
        fit: &crate::trace_fine::Fit,
        faithful: Option<&FaithfulSource>,
        geometric: Option<f64>,
        cancel: Option<&Cancel>,
    ) -> Option<SharedBorders> {
        let borders = borders(labels, w, h, cancel)?;
        let mut curves = Vec::with_capacity(borders.len());
        for (k, b) in borders.iter().enumerate() {
            if k % 256 == 0 && cancelled(cancel) {
                return None;
            }
            curves.push(fit_border(b, fit, faithful, geometric, labels));
        }
        let tiles_across = w / BORDER_TILE as usize + 1;
        let mut tiles = vec![Vec::new(); tiles_across * (h / BORDER_TILE as usize + 1)];
        for (k, b) in borders.iter().enumerate() {
            let t = (b.start.1 / BORDER_TILE) as usize * tiles_across
                + (b.start.0 / BORDER_TILE) as usize;
            if let Some(v) = tiles.get_mut(t) {
                v.push(k as u32);
            }
        }
        Some(SharedBorders {
            w,
            borders,
            curves,
            tiles,
            tiles_across,
        })
    }

    /// The rings of `mask` (`mw` x `mh` at `(ox, oy)` in the picture),
    /// which must be made of whole regions of the labelling, as one path
    /// of the fitted borders. `None` once `cancel` is set.
    fn rings(
        &self,
        mask: &[bool],
        (ox, oy): (usize, usize),
        mw: usize,
        mh: usize,
        cancel: Option<&Cancel>,
    ) -> Option<BezPath> {
        let w = self.w;
        let inside = |p: Option<usize>| {
            p.is_some_and(|p| {
                let (x, y) = (p % w, p / w);
                x >= ox
                    && y >= oy
                    && x < ox + mw
                    && y < oy + mh
                    && mask.get((y - oy) * mw + (x - ox)).copied().unwrap_or(false)
            })
        };
        // The borders between the mask and the rest, each turned so the
        // mask is on its `a` side: (border, forwards).
        let mut open: Vec<(usize, bool)> = Vec::new();
        let mut path = BezPath::new();
        let (x0, y0) = (ox as i64, oy as i64);
        let (x1, y1) = ((ox + mw) as i64, (oy + mh) as i64);
        for ty in (y0 / BORDER_TILE)..=(y1 / BORDER_TILE) {
            for tx in (x0 / BORDER_TILE)..=(x1 / BORDER_TILE) {
                let Some(list) = self
                    .tiles
                    .get(ty as usize * self.tiles_across + tx as usize)
                else {
                    continue;
                };
                for &k in list {
                    let b = &self.borders[k as usize];
                    let (sx, sy) = b.start;
                    if sx < x0 || sy < y0 || sx > x1 || sy > y1 {
                        continue;
                    }
                    let (ia, ib) = (inside(b.a), inside(b.b));
                    if ia == ib {
                        continue;
                    }
                    if b.closed {
                        append_curve(&mut path, &self.curves[k as usize], ia, true);
                    } else {
                        open.push((k as usize, ia));
                    }
                }
            }
        }
        if cancelled(cancel) {
            return None;
        }
        // Chained into rings at the junctions, turning right where the
        // mask touches itself at a corner, so touching rings stay apart.
        let ends = |(k, fwd): (usize, bool)| -> ((i64, i64), (i64, i64), u8, u8) {
            let b = &self.borders[k];
            let (first, last) = (
                b.steps.first().copied().unwrap_or(0),
                b.steps.last().copied().unwrap_or(0),
            );
            if fwd {
                (b.start, b.end, first, last)
            } else {
                (b.end, b.start, (last + 2) % 4, (first + 2) % 4)
            }
        };
        let mut from: std::collections::HashMap<(i64, i64), Vec<usize>> =
            std::collections::HashMap::with_capacity(open.len());
        for (i, o) in open.iter().enumerate() {
            from.entry(ends(*o).0).or_default().push(i);
        }
        let mut taken = vec![false; open.len()];
        for first in 0..open.len() {
            if taken[first] {
                continue;
            }
            taken[first] = true;
            let (ring_start, mut at, _, mut dir) = ends(open[first]);
            path.move_to(Point::new(ring_start.0 as f64, ring_start.1 as f64));
            append_curve(&mut path, &self.curves[open[first].0], open[first].1, false);
            let mut guard = open.len();
            while at != ring_start && guard > 0 {
                guard -= 1;
                let Some(next) = from.get(&at).and_then(|c| {
                    let free = || c.iter().copied().filter(|i| !taken[*i]);
                    let turning = |t: u8| free().find(|i| ends(open[*i]).2 == t);
                    turning((dir + 1) % 4)
                        .or_else(|| turning(dir))
                        .or_else(|| free().next())
                }) else {
                    break;
                };
                taken[next] = true;
                append_curve(&mut path, &self.curves[open[next].0], open[next].1, false);
                let (_, e, _, l) = ends(open[next]);
                at = e;
                dir = l;
            }
            path.close_path();
        }
        Some(path)
    }
}

/// Append `curve` (a border's fitted curve: a move then lines and
/// curves, closed for a loop) to `path`, forwards or backwards. A loop is
/// appended whole (`closed`); an open border continues the ring `path`
/// is drawing, from where it ends.
fn append_curve(path: &mut BezPath, curve: &BezPath, forwards: bool, closed: bool) {
    use tracedraw_core::geometry::PathEl;
    let els = curve.elements();
    let Some(PathEl::MoveTo(p0)) = els.first().copied() else {
        return;
    };
    if forwards {
        let body = if closed { els } else { &els[1..] };
        path.extend(body.iter().copied());
        return;
    }
    // Each piece with the point it starts from, then the pieces the other
    // way round.
    let mut pieces: Vec<(Point, PathEl)> = Vec::with_capacity(els.len());
    let mut at = p0;
    for el in &els[1..] {
        match *el {
            PathEl::LineTo(p) | PathEl::QuadTo(_, p) | PathEl::CurveTo(_, _, p) => {
                pieces.push((at, *el));
                at = p;
            }
            PathEl::ClosePath => {
                if at != p0 {
                    pieces.push((at, PathEl::LineTo(p0)));
                    at = p0;
                }
            }
            PathEl::MoveTo(_) => {}
        }
    }
    if closed {
        path.move_to(at);
    }
    for (start, el) in pieces.into_iter().rev() {
        match el {
            PathEl::LineTo(_) => path.line_to(start),
            PathEl::QuadTo(c, _) => path.quad_to(c, start),
            PathEl::CurveTo(c1, c2, _) => path.curve_to(c2, c1, start),
            _ => {}
        }
    }
    if closed {
        path.close_path();
    }
}

/// What the faithful fit of a picture's borders works from: how it
/// fits, the picture's colours (`w` x `h`) and each label's colour, none
/// for a label drawn as a blend.
struct FaithfulSource<'a> {
    fit: &'a crate::trace_fit::Faithful,
    colors: &'a [[u8; 3]],
    w: usize,
    h: usize,
    flat: &'a [Option<[f32; 3]>],
    /// How many points of the grid each label covers.
    covers: &'a [usize],
    /// The points of the grid whose borders were drawn again
    /// ([`crate::trace_strokes`]): they are not placed by the picture.
    redrawn: Option<&'a [bool]>,
}

/// A border's curve, from its start to its end (closed round a loop):
/// fitted with its corners kept ([`crate::trace_fine::fit`]), or
/// simplified to straight lines along the pixel edges without smoothing.
fn fit_border(
    b: &Border,
    fit: &crate::trace_fine::Fit,
    faithful: Option<&FaithfulSource>,
    geometric: Option<f64>,
    labels: &[usize],
) -> BezPath {
    let mut pts = b.corners();
    if b.closed {
        pts.pop();
    }
    // Geometric shapes (`trace_shapes`): a ring all round one region is
    // made symmetric where it nearly is, and a whole circle, ellipse or
    // rounded rectangle where it draws one. `geometric` is the border's
    // points per pixel of the art.
    let Some(unit) = geometric.filter(|_| b.closed) else {
        return fit_border_points(b, &pts, fit, faithful, labels);
    };
    let curve = match crate::trace_shapes::mirrored(&pts, unit) {
        Some(even) => fit_border_points(b, &even, fit, faithful, labels),
        None => fit_border_points(b, &pts, fit, faithful, labels),
    };
    crate::trace_shapes::snapped(&pts, &curve, unit).unwrap_or(curve)
}

/// [`fit_border`] of the border `b` walked along `pts` (its corners, the
/// last one left out of a closed border).
fn fit_border_points(
    b: &Border,
    pts: &[Point],
    fit: &crate::trace_fine::Fit,
    faithful: Option<&FaithfulSource>,
    labels: &[usize],
) -> BezPath {
    if let Some(f) = faithful {
        let side = |pixel: Option<usize>| {
            pixel
                .and_then(|i| labels.get(i))
                .and_then(|l| f.flat.get(*l).copied().flatten())
        };
        let picture = crate::trace_fit::Picture {
            colors: f.colors,
            w: f.w,
            h: f.h,
            a: side(b.a),
            b: side(b.b),
            a_less: {
                let covers = |pixel: Option<usize>| {
                    pixel
                        .and_then(|i| labels.get(i))
                        .and_then(|l| f.covers.get(*l).copied())
                };
                // The background taken out covers the most.
                match (covers(b.a), covers(b.b)) {
                    (Some(x), Some(y)) => Some(x < y),
                    (None, Some(_)) => Some(false),
                    (Some(_), None) => Some(true),
                    (None, None) => None,
                }
            },
        };
        let drawn = f.redrawn.is_some_and(|r| {
            [b.a, b.b]
                .iter()
                .flatten()
                .any(|i| r.get(*i) == Some(&true))
        });
        let picture = (!drawn).then_some(&picture);
        return crate::trace_fit::fit(pts, b.closed, f.fit, picture).path;
    }
    crate::trace_fine::fit(pts, b.closed, fit)
}

/// Call `f` with the pixels of each hole of `mask` (`w` x `h`): each
/// 4-connected patch of unset pixels not reaching the box's edge.
fn for_each_hole(mask: &[bool], w: usize, h: usize, mut f: impl FnMut(&[usize])) {
    if mask.len() != w * h || w < 3 || h < 3 {
        return;
    }
    let mut seen = vec![false; w * h];
    let mut stack: Vec<usize> = Vec::new();
    let mut patch: Vec<usize> = Vec::new();
    for start in 0..w * h {
        if seen[start] || mask[start] {
            continue;
        }
        seen[start] = true;
        stack.push(start);
        patch.clear();
        let mut edge = false;
        while let Some(i) = stack.pop() {
            patch.push(i);
            let (x, y) = (i % w, i / w);
            edge |= x == 0 || y == 0 || x + 1 == w || y + 1 == h;
            let near = [
                (x > 0).then(|| i - 1),
                (x + 1 < w).then(|| i + 1),
                (y > 0).then(|| i - w),
                (y + 1 < h).then(|| i + w),
            ];
            for j in near.into_iter().flatten() {
                if !mask[j] && !seen[j] {
                    seen[j] = true;
                    stack.push(j);
                }
            }
        }
        if !edge {
            f(&patch);
        }
    }
}

/// The order colours stack in, bottom first: a colour found in another's
/// holes (`inside[a][b]`: `b` in a hole of `a` with no page showing in
/// it) goes above that one, so the one around is drawn whole under it;
/// among the colours free to go next the darkest (`luma`) goes first,
/// and where colours sit in each other's holes (black eyes in a yellow
/// head inside a black outline) the darkest of them.
fn stacking_order(inside: &[Vec<bool>], luma: &[u32]) -> Vec<usize> {
    let n = luma.len();
    let mut below = vec![0usize; n];
    for (a, row) in inside.iter().enumerate() {
        for (b, v) in row.iter().enumerate() {
            if *v && a != b && b < n {
                below[b] += 1;
            }
        }
    }
    let mut placed = vec![false; n];
    let mut order = Vec::with_capacity(n);
    while order.len() < n {
        let free = (0..n)
            .filter(|i| !placed[*i] && below[*i] == 0)
            .min_by_key(|i| (luma[*i], *i));
        let Some(next) = free.or_else(|| {
            (0..n)
                .filter(|i| !placed[*i])
                .min_by_key(|i| (luma[*i], *i))
        }) else {
            break;
        };
        placed[next] = true;
        order.push(next);
        if let Some(row) = inside.get(next) {
            for (b, v) in row.iter().enumerate() {
                if *v && b != next && b < n && !placed[b] {
                    below[b] = below[b].saturating_sub(1);
                }
            }
        }
    }
    order
}

/// The 4-connected patches of a labelling: each pixel's patch number
/// (`u32::MAX` for transparent pixels), and each patch's label, size,
/// edge count (pixel edges against another label or a transparent pixel)
/// and the labels it touches, most first, each with its edge count and
/// how many of those are against `solid` pixels.
struct Patches {
    of: Vec<u32>,
    label: Vec<usize>,
    area: Vec<u32>,
    edges: Vec<u32>,
    touches: Vec<Vec<(usize, u32, u32)>>,
}

fn label_patches(
    labels: &[usize],
    solid: &[bool],
    w: usize,
    h: usize,
    cancel: Option<&Cancel>,
) -> Option<Patches> {
    let mut p = Patches {
        of: vec![u32::MAX; w * h],
        label: Vec::new(),
        area: Vec::new(),
        edges: Vec::new(),
        touches: Vec::new(),
    };
    let mut stack: Vec<usize> = Vec::new();
    for start in 0..w * h {
        if start % CANCEL_EVERY == 0 && cancelled(cancel) {
            return None;
        }
        if p.of[start] != u32::MAX || labels[start] == usize::MAX {
            continue;
        }
        let l = labels[start];
        let id = p.label.len() as u32;
        let (mut area, mut edges) = (0u32, 0u32);
        let mut touches: Vec<(usize, u32, u32)> = Vec::new();
        p.of[start] = id;
        stack.push(start);
        while let Some(i) = stack.pop() {
            area += 1;
            let (x, y) = (i % w, i / w);
            let near = [
                (x > 0).then(|| i - 1),
                (x + 1 < w).then(|| i + 1),
                (y > 0).then(|| i - w),
                (y + 1 < h).then(|| i + w),
            ];
            for j in near.into_iter().flatten() {
                let m = labels[j];
                if m == l {
                    if p.of[j] == u32::MAX {
                        p.of[j] = id;
                        stack.push(j);
                    }
                    continue;
                }
                edges += 1;
                if m != usize::MAX {
                    let f = solid[j] as u32;
                    match touches.iter_mut().find(|(k, _, _)| *k == m) {
                        Some((_, n, nf)) => {
                            *n += 1;
                            *nf += f;
                        }
                        None => touches.push((m, 1, f)),
                    }
                }
            }
        }
        touches.sort_by_key(|(k, n, _)| (u32::MAX - *n, *k));
        p.label.push(l);
        p.area.push(area);
        p.edges.push(edges);
        p.touches.push(touches);
    }
    Some(p)
}

/// A picture enlarged before it is traced ([`Settings::pixel_scale`])
/// has its corners rounded a little less than the blur of a small
/// picture rounds them, and its edges placed more exactly: the measures
/// of the corner finder are this share of a pixel of the art, and the
/// curves may stray this share of what they may in a picture traced as
/// it is.
const ENLARGED_SHARP: f64 = 0.875;
const ENLARGED_TOLERANCE: f64 = 0.5;
/// How far (pixels of the picture, at the tolerance of full detail and
/// more as the detail falls) the curves of an enlarged picture may stray
/// from its smoothed borders.
const FAITHFUL_TOLERANCE: f64 = 0.85;
/// The tip of a narrow letter (the top of an A five pixels high) is as
/// the enlarging drew it: the place its two sides meet may lie this
/// share as far from it as in a picture traced as it is, where the blur
/// wore the tip down.
const ENLARGED_REACH: f64 = 0.4;
/// The finest grid an enlarged picture is put on (times its own).
const ENLARGED_FINE: usize = 2;
/// The square end of a thin stroke is rounded by the enlarging to half a
/// circle at most, which leaves its corners a fifth of its width from
/// the border: a round end whose corners would lie further than this
/// share of its width is a bend (of an S), and stays round.
const ENLARGED_CAP_NEAR: f64 = 0.25;

/// The largest piece of a shade (pixels) that joins a larger piece of a
/// shade near its own beside it, and how near (RGB distance) the two
/// shades are.
const SHADE_AREA: usize = 16;
const SHADE_NEAR: u32 = 48;

/// The shading of small print is no detail of the art: a patch of at
/// most `most` pixels beside a larger patch of a colour within
/// [`SHADE_NEAR`] of its own takes that colour (the largest such), so a
/// letter five pixels high is one colour, not three pieces with seams
/// between them.
fn join_shades(
    labels: &mut [usize],
    w: usize,
    h: usize,
    palette: &[[u8; 3]],
    colors: &[[u8; 3]],
    most: usize,
    cancel: Option<&Cancel>,
) -> Option<()> {
    if labels.len() != w * h || colors.len() != w * h || w < 3 || h < 3 || most == 0 {
        return Some(());
    }
    let solid = vec![false; w * h];
    for _ in 0..3 {
        let p = label_patches(labels, &solid, w, h, cancel)?;
        // The colour each patch has in the picture, all in all.
        let mut sums = vec![[0u64; 3]; p.label.len()];
        for (i, c) in colors.iter().enumerate() {
            if let Some(at) = p.of.get(i).and_then(|k| sums.get_mut(*k as usize)) {
                for q in 0..3 {
                    at[q] += c[q] as u64;
                }
            }
        }
        let mean = |k: usize| -> Option<[u8; 3]> {
            let (sum, n) = (sums.get(k)?, (*p.area.get(k)? as u64).max(1));
            Some(std::array::from_fn(|q| (sum[q] / n).min(255) as u8))
        };
        // The largest near patch beside each small one.
        let mut to: Vec<Option<u32>> = vec![None; p.label.len()];
        let mut offer = |k: u32, j: u32| {
            if k == j || k == u32::MAX || j == u32::MAX {
                return;
            }
            let (k, ju) = (k as usize, j as usize);
            if p.area[k] as usize > most || p.area[ju] <= p.area[k] {
                return;
            }
            if palette.get(p.label[k]).is_none() || palette.get(p.label[ju]).is_none() {
                return;
            }
            let (Some(a), Some(b)) = (mean(k), mean(ju)) else {
                return;
            };
            if distance2(a, b) > SHADE_NEAR * SHADE_NEAR {
                return;
            }
            if to[k].is_none_or(|t| p.area[t as usize] < p.area[ju]) {
                to[k] = Some(j);
            }
        };
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                if x + 1 < w {
                    offer(p.of[i], p.of[i + 1]);
                    offer(p.of[i + 1], p.of[i]);
                }
                if y + 1 < h {
                    offer(p.of[i], p.of[i + w]);
                    offer(p.of[i + w], p.of[i]);
                }
            }
        }
        let mut changed = false;
        for (i, l) in labels.iter_mut().enumerate() {
            if let Some(Some(j)) = to.get(p.of[i] as usize) {
                *l = p.label[*j as usize];
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    Some(())
}

/// The largest gap of a letter (pixels) that takes the colour of the
/// paper it opens onto, and how near (RGB distance) its colour must be
/// to the paper's.
const GAP_OPEN_AREA: usize = 40;
const GAP_OPEN_NEAR: u32 = 48;

/// The gap of a small letter that opens onto the paper (the inside of a
/// G or a C) is paper: a patch of at most `most` pixels, most of its
/// edge against a colour [`GAP_CONTRAST`] or more from its own, that
/// touches a colour within [`GAP_OPEN_NEAR`] of its own takes that
/// colour. The blur of small print leaves such a gap a shade darker
/// than the paper, and it was a region of its own closing the letter.
fn open_gaps(
    labels: &mut [usize],
    w: usize,
    h: usize,
    palette: &[[u8; 3]],
    most: usize,
    cancel: Option<&Cancel>,
) -> Option<()> {
    if labels.len() != w * h || w < 3 || h < 3 {
        return Some(());
    }
    let solid = vec![false; w * h];
    let p = label_patches(labels, &solid, w, h, cancel)?;
    let mut to: Vec<Option<usize>> = vec![None; p.label.len()];
    for k in 0..p.label.len() {
        if p.area[k] as usize > most {
            continue;
        }
        let Some(&own) = palette.get(p.label[k]) else {
            continue;
        };
        let touch = &p.touches[k];
        let Some(&(around, _, _)) = touch.first() else {
            continue;
        };
        if !palette
            .get(around)
            .is_some_and(|c| distance2(*c, own) >= GAP_CONTRAST * GAP_CONTRAST)
        {
            continue;
        }
        to[k] = touch
            .iter()
            .skip(1)
            .map(|t| t.0)
            .filter(|l| {
                palette
                    .get(*l)
                    .is_some_and(|c| distance2(*c, own) <= GAP_OPEN_NEAR * GAP_OPEN_NEAR)
            })
            .min_by_key(|l| distance2(palette[*l], own));
    }
    for (i, l) in labels.iter_mut().enumerate() {
        if let Some(Some(b)) = to.get(p.of[i] as usize) {
            *l = *b;
        }
    }
    Some(())
}

/// How thick (twice the area over the edge count, pixels) a band along
/// the edge of two colours can be and still be their soft edge.
const FRINGE_WIDTH: f64 = 5.0;
/// The thickest band (pixels) of any colour along an edge between two
/// others that Geometric shapes takes for a fringe, and how much darker
/// or lighter than both sides it may be.
const LOOSE_FRINGE_WIDTH: f64 = 2.5;
const LOOSE_FRINGE_LUMA: f64 = 40.0;

/// Give the patches that are the soft edge of two colours to those two
/// colours, each pixel to the one its own colour is nearer: a band no
/// more than `FRINGE_WIDTH` thick, of a colour between the two it
/// touches most (at least `EDGE_CONTRAST` apart, the second on a tenth of
/// its edges or more), both of them solid areas where they touch it (at
/// least half their pixels there flat or beside a flat pixel of their
/// colour). A blurred or compressed edge makes such bands thicker than
/// [`settle_edges`] reaches, and they were slivers and bumps along the
/// edge (#611, #1250). The pieces of a thin letter in two shades, one
/// touching the other and the fill, stay: the other shade is no solid
/// area.
fn dissolve_fringes(
    labels: &mut [usize],
    w: usize,
    h: usize,
    palette: &[[u8; 3]],
    colors: &[[u8; 3]],
    cancel: Option<&Cancel>,
) -> Option<()> {
    dissolve_fringes_of(labels, w, h, palette, colors, FRINGE_WIDTH, 0.0, cancel)
}

/// [`dissolve_fringes`] for bands up to `width` thick (a picture
/// enlarged has its soft edges as many times wider). Bands up to `loose`
/// thick go whatever their colour, as long as it is neither much darker
/// nor much lighter than both sides (an outline drawn between them
/// stays).
#[allow(clippy::too_many_arguments)]
fn dissolve_fringes_of(
    labels: &mut [usize],
    w: usize,
    h: usize,
    palette: &[[u8; 3]],
    colors: &[[u8; 3]],
    width: f64,
    loose: f64,
    cancel: Option<&Cancel>,
) -> Option<()> {
    if labels.len() != w * h || colors.len() != w * h || w < 3 || h < 3 {
        return Some(());
    }
    let flat = flat_pixels(labels, w, h);
    let solid: Vec<bool> = (0..w * h)
        .map(|i| {
            let (x, y) = (i % w, i / w);
            let l = labels[i];
            flat[i]
                || (l != usize::MAX
                    && (y.saturating_sub(1)..=(y + 1).min(h - 1)).any(|v| {
                        (x.saturating_sub(1)..=(x + 1).min(w - 1))
                            .any(|u| flat[v * w + u] && labels[v * w + u] == l)
                    }))
        })
        .collect();
    let p = label_patches(labels, &solid, w, h, cancel)?;
    let mut to: Vec<Option<(usize, usize)>> = vec![None; p.label.len()];
    for (k, t) in to.iter_mut().enumerate() {
        let (area, edges) = (p.area[k] as f64, p.edges[k] as f64);
        let touch = &p.touches[k];
        let thick = if edges > 0.0 { 2.0 * area / edges } else { f64::MAX };
        if edges <= 0.0 || touch.len() < 2 || thick > width.max(loose) {
            continue;
        }
        let ((a, na, sa), (b, nb, sb)) = (touch[0], touch[1]);
        let (Some(&ca), Some(&cb), Some(&cl)) =
            (palette.get(a), palette.get(b), palette.get(p.label[k]))
        else {
            continue;
        };
        // The band's colour lies between the two, or near one of them
        // (the ringing of a compressed edge overshoots the colour beside
        // it: a cream band along a yellow's edge with black).
        let lum = |c: [u8; 3]| 0.299 * c[0] as f64 + 0.587 * c[1] as f64 + 0.114 * c[2] as f64;
        let (la, lb, ll) = (lum(ca), lum(cb), lum(cl));
        let loosely = thick <= loose
            && ll >= la.min(lb) - LOOSE_FRINGE_LUMA
            && ll <= la.max(lb) + LOOSE_FRINGE_LUMA;
        if thick > width && !loosely {
            continue;
        }
        let explained = loosely
            || between_loosely(cl, ca, cb, 2.0)
            || within_blend(cl, ca, cb)
            || distance2(cl, ca) < EDGE_CONTRAST * EDGE_CONTRAST
            || distance2(cl, cb) < EDGE_CONTRAST * EDGE_CONTRAST;
        if (nb as f64) < 0.1 * edges
            || 2 * sa < na
            || 2 * sb < nb
            || distance2(ca, cb) < EDGE_CONTRAST * EDGE_CONTRAST
            || !explained
        {
            continue;
        }
        *t = Some((a, b));
    }
    if cancelled(cancel) {
        return None;
    }
    for i in 0..w * h {
        let Some((a, b)) = to.get(p.of[i] as usize).copied().flatten() else {
            continue;
        };
        labels[i] = if nearer_first(colors[i], palette[a], palette[b]) {
            a
        } else {
            b
        };
    }
    Some(())
}

/// How many times [`smooth_labels`] goes over the picture.
const SMOOTH_PASSES: usize = 2;

/// How many of its 8 neighbours must have one other colour for a pixel
/// to take it.
const SMOOTH_MAJORITY: u32 = 5;

/// Each pixel at least `SMOOTH_MAJORITY` of whose 8 neighbours have one
/// other colour takes it, `SMOOTH_PASSES` times over, so the one-pixel
/// bumps and notches noise leaves along an edge go and the edge is
/// smooth before it is fitted (#611, #1250). Only a pixel whose own
/// neighbours are the three on one side of it changes (a bump on a
/// straight edge, a notch in one): corners, the pixels of a line (their
/// own on two sides) and its end stay, so squares keep their corners and
/// lines one pixel wide their pixels.
fn smooth_labels(labels: &mut [usize], w: usize, h: usize, cancel: Option<&Cancel>) -> Option<()> {
    if labels.len() != w * h || w < 3 || h < 3 {
        return Some(());
    }
    // The eight neighbours in order round the pixel.
    let ring: [isize; 8] = {
        let w = w as isize;
        [-w - 1, -w, -w + 1, 1, w + 1, w, w - 1, -1]
    };
    for _ in 0..SMOOTH_PASSES {
        let prev = labels.to_vec();
        let mut changed = false;
        for y in 1..h - 1 {
            if (y - 1) % 64 == 0 && cancelled(cancel) {
                return None;
            }
            for x in 1..w - 1 {
                let i = y * w + x;
                let own = prev[i];
                if own == usize::MAX {
                    continue;
                }
                let mut around = [usize::MAX; 8];
                let mut mine = 0u8;
                for (k, d) in ring.iter().enumerate() {
                    let m = prev[(i as isize + d) as usize];
                    around[k] = m;
                    if m == own {
                        mine |= 1 << k;
                    }
                }
                // Its own neighbours: three in a straight row on one side
                // (ring places 0-2, 2-4, 4-6 or 6-0), so the pixel is a
                // bump on a straight edge or a notch in it, not a corner
                // (its own round a corner of the ring), part of a line
                // (its own on two sides) or a line's end.
                if ![0b0000_0111u8, 0b0001_1100, 0b0111_0000, 0b1100_0001].contains(&mine) {
                    continue;
                }
                let mut best = (0u32, usize::MAX);
                for m in around {
                    if m == own || m == usize::MAX {
                        continue;
                    }
                    let c = around.iter().filter(|o| **o == m).count() as u32;
                    if c > best.0 || (c == best.0 && m < best.1) {
                        best = (c, m);
                    }
                }
                if best.0 >= SMOOTH_MAJORITY {
                    labels[i] = best.1;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    Some(())
}

/// How far apart (RGB distance) two colours must be for a thin band
/// between them to count as their edge's anti-aliasing.
const EDGE_CONTRAST: u32 = 64;

fn distance2(a: [u8; 3], b: [u8; 3]) -> u32 {
    (0..3)
        .map(|k| (a[k] as i32 - b[k] as i32).pow(2) as u32)
        .sum()
}

/// Whether `c` lies between `a` and `b`: near the segment joining them in
/// RGB, as the anti-aliased pixels of their edge do.
fn between(c: [u8; 3], a: [u8; 3], b: [u8; 3]) -> bool {
    off_segment(c, a, b).is_some_and(|(d, slack)| d <= slack)
}

/// Whether `c` lies between `a` and `b` with `loose` times the usual
/// slack: the blends of a compressed edge drift off the segment (its
/// colour is stored coarser than its brightness), and a palette colour
/// made of them with it.
fn between_loosely(c: [u8; 3], a: [u8; 3], b: [u8; 3], loose: f64) -> bool {
    off_segment(c, a, b).is_some_and(|(d, slack)| d <= slack * loose)
}

/// Whether `c` is a blend of `a` and `b` as a compressed picture stores
/// one: its brightness between theirs, and each of its two colour
/// differences (blue and red against the brightness) between theirs,
/// each within the slack of [`off_segment`]. Compression keeps the
/// brightness of an edge sharp and smears its colour over several
/// pixels, so the pixels along an edge of black and yellow run from
/// black to a pale, greyish yellow instead of along the segment between
/// the two: in brightness and colour differences taken apart, they lie
/// between the two all the same.
fn within_blend(c: [u8; 3], a: [u8; 3], b: [u8; 3]) -> bool {
    let Some((_, slack)) = off_segment(c, a, b) else {
        return false;
    };
    let yc = |v: [u8; 3]| {
        let y = 0.299 * v[0] as f64 + 0.587 * v[1] as f64 + 0.114 * v[2] as f64;
        [y, v[2] as f64 - y, v[0] as f64 - y]
    };
    let (c, a, b) = (yc(c), yc(a), yc(b));
    (0..3).all(|k| c[k] >= a[k].min(b[k]) - slack && c[k] <= a[k].max(b[k]) + slack)
}

/// How far `c` lies from the segment joining `a` and `b` in RGB, and
/// the slack within which it counts as between them; `None` when `a`
/// and `b` are one colour.
fn off_segment(c: [u8; 3], a: [u8; 3], b: [u8; 3]) -> Option<(f64, f64)> {
    let f = |v: [u8; 3]| [v[0] as f64, v[1] as f64, v[2] as f64];
    let (c, a, b) = (f(c), f(a), f(b));
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let len2 = ab.iter().map(|v| v * v).sum::<f64>();
    if len2 < 1.0 {
        return None;
    }
    let t = (ab.iter().zip(&ac).map(|(p, q)| p * q).sum::<f64>() / len2).clamp(0.0, 1.0);
    let d2 = (0..3).map(|k| (ac[k] - t * ab[k]).powi(2)).sum::<f64>();
    let slack = (0.08 * len2.sqrt()).max(12.0);
    Some((d2.sqrt(), slack))
}

/// The farthest apart (RGB distance) two colours split from one are.
const SPLIT_REACH: f64 = 40.0;

/// An image is flat art when its `FLAT_ART_COLORS` commonest colours,
/// once joined, cover `FLAT_ART_SHARE` of it.
const FLAT_ART_COLORS: usize = 8;
const FLAT_ART_SHARE: f64 = 0.85;

/// Join the palette colours that are one colour split by its noise: two
/// colours nearer each other than their pixels lie, on average, from
/// them (and than `SPLIT_REACH`). Their pixels take the colour of the one
/// with more, weighted by count. The steps of a gradient, and distinct
/// colours however close, lie further apart than their own spread and
/// stay, and photos are left as they are. Returns the palette with each
/// joined colour at their mean.
fn merge_split_colors(
    labels: &mut [usize],
    w: usize,
    h: usize,
    mut palette: Vec<[u8; 3]>,
    colors: &[[u8; 3]],
) -> Vec<[u8; 3]> {
    let n = palette.len();
    if labels.len() != colors.len() || labels.len() != w * h || n < 2 {
        return palette;
    }
    let mut count = vec![0f64; n];
    let mut sum = vec![[0f64; 3]; n];
    let mut spread = vec![0f64; n];
    for (l, c) in labels.iter().zip(colors) {
        if let (Some(k), Some(p)) = (count.get_mut(*l), palette.get(*l)) {
            *k += 1.0;
            for d in 0..3 {
                sum[*l][d] += c[d] as f64;
            }
            spread[*l] += distance2(*c, *p) as f64;
        }
    }
    for l in 0..n {
        if count[l] > 0.0 {
            spread[l] = (spread[l] / count[l]).sqrt();
        }
    }
    // Union-find over the colours, joining the nearest pairs first.
    let mut root: Vec<usize> = (0..n).collect();
    fn find(root: &mut [usize], mut i: usize) -> usize {
        while root[i] != i {
            root[i] = root[root[i]];
            i = root[i];
        }
        i
    }
    // Shades scattered through each other (more pixel edges shared
    // than the smaller has pixels, nearly) are one colour's noise too,
    // however tight each shade is: with many colours asked for, the
    // colour reduction cuts a flat colour into shades so narrow that
    // none spreads as far as the next.
    let shared = shared_edges(labels, w, h, n);
    let mut pairs: Vec<(f64, usize, usize)> = Vec::new();
    for a in 0..n {
        for b in a + 1..n {
            if count[a] == 0.0 || count[b] == 0.0 {
                continue;
            }
            let d = (distance2(palette[a], palette[b]) as f64).sqrt();
            let mixed = shared[a * n + b] as f64 >= INTERLEAVED * count[a].min(count[b]);
            if d < SPLIT_REACH && (d < spread[a] + spread[b] || mixed) {
                pairs.push((d, a, b));
            }
        }
    }
    if pairs.is_empty() {
        return palette;
    }
    pairs.sort_by(|p, q| p.0.total_cmp(&q.0));
    // Each group's pixel count and colour sum; two groups join only when
    // their mean colours are near too, so in-between colours along an
    // edge do not chain two distinct colours into one.
    let mut total: Vec<(f64, [f64; 3])> = (0..n).map(|l| (count[l], sum[l])).collect();
    let mean = |t: &(f64, [f64; 3])| -> [f64; 3] { std::array::from_fn(|d| t.1[d] / t.0) };
    // The colours of each group: two groups join only when every colour
    // of one lies within `SPLIT_REACH` of every colour of the other (the
    // shades of one colour all lie near each other), so a run of shades
    // each near the next (a pink cheek, its blends with the yellow
    // around it, the yellow's own shades) does not chain into one mean
    // colour none of them had.
    let mut members: Vec<Vec<usize>> = (0..n).map(|l| vec![l]).collect();
    for (_, a, b) in pairs {
        let (ra, rb) = (find(&mut root, a), find(&mut root, b));
        if ra == rb {
            continue;
        }
        let (ma, mb) = (mean(&total[ra]), mean(&total[rb]));
        let d2: f64 = (0..3).map(|d| (ma[d] - mb[d]).powi(2)).sum();
        let near = |p: &usize, q: &usize| {
            (distance2(palette[*p], palette[*q]) as f64) < SPLIT_REACH * SPLIT_REACH
        };
        if d2 >= SPLIT_REACH * SPLIT_REACH
            || !members[ra]
                .iter()
                .all(|p| members[rb].iter().all(|q| near(p, q)))
        {
            continue;
        }
        root[rb] = ra;
        let t = total[rb];
        total[ra].0 += t.0;
        for d in 0..3 {
            total[ra].1[d] += t.1[d];
        }
        let moved = std::mem::take(&mut members[rb]);
        members[ra].extend(moved);
    }
    // Flat art only: once joined, a few colours cover nearly all of it.
    // In a photo neighbouring colours of a gradient lie as near as their
    // spread too, and are left apart.
    let mut groups: Vec<f64> = (0..n)
        .filter(|&r| root[r] == r)
        .map(|r| total[r].0)
        .collect();
    groups.sort_by(|a, b| b.total_cmp(a));
    let all: f64 = groups.iter().sum();
    let few: f64 = groups.iter().take(FLAT_ART_COLORS).sum();
    if all <= 0.0 || few < FLAT_ART_SHARE * all {
        return palette;
    }
    for (r, (k, s)) in total.iter().enumerate() {
        if *k > 0.0 && root[r] == r {
            palette[r] = std::array::from_fn(|d| (s[d] / k).round().clamp(0.0, 255.0) as u8);
        }
    }
    for l in labels.iter_mut() {
        if *l < n {
            *l = find(&mut root, *l);
        }
    }
    palette
}

/// How near (RGB distance) two colours of the palette must be to be one
/// colour: none from detail 95 up, 30 at the Clipart detail of 60, 44 at
/// 35.
fn near_reach(detail: f64) -> f64 {
    if detail >= 95.0 {
        0.0
    } else {
        10.0 + (100.0 - detail.clamp(0.0, 100.0)) * 0.5
    }
}

/// `palette` without the colours no label uses, the labels renumbered to
/// match (transparent pixels stay `usize::MAX`).
fn compact_palette(labels: &mut [usize], palette: Vec<[u8; 3]>) -> Vec<[u8; 3]> {
    let mut used = vec![false; palette.len()];
    for l in labels.iter() {
        if let Some(u) = used.get_mut(*l) {
            *u = true;
        }
    }
    if used.iter().all(|u| *u) {
        return palette;
    }
    let mut to = vec![usize::MAX; palette.len()];
    let mut kept = Vec::new();
    for (i, c) in palette.into_iter().enumerate() {
        if used[i] {
            to[i] = kept.len();
            kept.push(c);
        }
    }
    for l in labels.iter_mut() {
        if let Some(t) = to.get(*l) {
            *l = *t;
        }
    }
    kept
}

/// How many pixel edges two shades must share, per pixel of the smaller,
/// to be one colour's noise.
const INTERLEAVED: f64 = 0.6;

/// How many pixel edges each two of `n` labels share (`n` by `n`,
/// symmetric; transparent pixels count for nothing).
fn shared_edges(labels: &[usize], w: usize, h: usize, n: usize) -> Vec<u32> {
    let mut shared = vec![0u32; n * n];
    if labels.len() != w * h {
        return shared;
    }
    for y in 0..h {
        for x in 0..w {
            let a = labels[y * w + x];
            for b in [
                (x + 1 < w).then(|| labels[y * w + x + 1]),
                (y + 1 < h).then(|| labels[(y + 1) * w + x]),
            ]
            .into_iter()
            .flatten()
            {
                if a != b && a < n && b < n {
                    shared[a * n + b] += 1;
                    shared[b * n + a] += 1;
                }
            }
        }
    }
    shared
}

/// Whether each of the `n` labels is a solid colour: at least half its
/// pixels flat (their eight neighbours of the label too), as the
/// colours of the art are and the shades of its edges and noise are not.
fn solid_colors(labels: &[usize], w: usize, h: usize, n: usize) -> Vec<bool> {
    let mut count = vec![(0u32, 0u32); n];
    if labels.len() == w * h {
        for (l, f) in labels.iter().zip(flat_pixels(labels, w, h)) {
            if let Some(c) = count.get_mut(*l) {
                c.0 += 1;
                c.1 += f as u32;
            }
        }
    }
    count
        .iter()
        .map(|(all, f)| *all > 0 && 2 * f >= *all)
        .collect()
}

/// Which colours are colours of their own (`n` of them): at least half
/// their pixels flat (`flat`: the eight neighbours share the colour), or
/// a core somewhere, a flat pixel whose eight neighbours are flat too (a
/// block of five by five). The others only line edges: the fringes of a
/// blurred or compressed edge, a pixel or two wide however long. A small
/// feature (the cheek of a face a dozen pixels across) has few flat
/// pixels against its edge, but a core; a fringe band never has one.
/// Returns, per colour, whether it is a colour of its own and whether it
/// is a fringe: fewer than one pixel in `FRINGE_FLAT_SHARE` flat, as a
/// band a pixel or two wide is, while the bands of a photo's gradient,
/// no colours of their own either, are flat inside.
fn colors_of_their_own(
    labels: &[usize],
    flat: &[bool],
    w: usize,
    h: usize,
    n: usize,
) -> (Vec<bool>, Vec<bool>) {
    let mut count = vec![(0u32, 0u32, false); n];
    for (i, (l, f)) in labels.iter().zip(flat).enumerate() {
        let Some(c) = count.get_mut(*l) else { continue };
        c.0 += 1;
        c.1 += *f as u32;
        if *f && !c.2 {
            let (x, y) = (i % w, i / w);
            c.2 = x > 0
                && y > 0
                && x + 1 < w
                && y + 1 < h
                && (y - 1..=y + 1).all(|v| (x - 1..=x + 1).all(|u| flat[v * w + u]));
        }
    }
    let own = count
        .iter()
        .map(|(all, f, core)| *all > 0 && (2 * f >= *all || *core))
        .collect();
    let fringe = count
        .iter()
        .map(|(all, f, _)| *all > 0 && f * FRINGE_FLAT_SHARE < *all)
        .collect();
    (own, fringe)
}

/// One pixel in this many of a colour's pixels flat at most for the
/// colour to be a fringe.
const FRINGE_FLAT_SHARE: u32 = 8;

/// Which pixels lie in a band between two colours of their own: the
/// pixels of the other colours, taken together in connected regions
/// whatever their colours, where the region touches two colours of
/// their own, the second along a tenth of its edge at least. The
/// fringes of an edge do; the glint in an eye, a few shades of white
/// none of them a colour of its own, touches the dark of the eye alone
/// and is no fringe, whatever the colours further off.
fn band_regions(labels: &[usize], real: &[bool], w: usize, h: usize) -> Vec<bool> {
    let mut band = vec![false; w * h];
    let mut seen = vec![false; w * h];
    let mut stack: Vec<usize> = Vec::new();
    let mut region: Vec<usize> = Vec::new();
    let is_real = |l: usize| real.get(l).copied().unwrap_or(true);
    for start in 0..w * h {
        if seen[start] || is_real(labels[start]) {
            continue;
        }
        seen[start] = true;
        stack.push(start);
        region.clear();
        let mut edges = 0u32;
        let mut touches: Vec<(usize, u32)> = Vec::new();
        while let Some(i) = stack.pop() {
            region.push(i);
            let (x, y) = (i % w, i / w);
            let near = [
                (x > 0).then(|| i - 1),
                (x + 1 < w).then(|| i + 1),
                (y > 0).then(|| i - w),
                (y + 1 < h).then(|| i + w),
            ];
            for j in near.into_iter().flatten() {
                let m = labels[j];
                if !is_real(m) {
                    if !seen[j] {
                        seen[j] = true;
                        stack.push(j);
                    }
                    continue;
                }
                edges += 1;
                if m == usize::MAX {
                    continue;
                }
                match touches.iter_mut().find(|(k, _)| *k == m) {
                    Some((_, n)) => *n += 1,
                    None => touches.push((m, 1)),
                }
            }
        }
        touches.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        if touches.len() >= 2 && 10 * touches[1].1 >= edges {
            for &i in &region {
                band[i] = true;
            }
        }
    }
    band
}

/// Join the palette colours nearer each other than `reach` (RGB
/// distance) of which one at least is no solid colour, nearest pairs
/// first; two groups join only while the means
/// of their pixels stay within `reach`, so a run of steps (a gradient,
/// the in-between colours of an edge) does not chain far apart colours
/// into one. The labels of a joined colour take the group's, which keeps
/// the colour of its solid member.
fn merge_near_colors(
    labels: &mut [usize],
    w: usize,
    h: usize,
    palette: Vec<[u8; 3]>,
    colors: &[[u8; 3]],
    reach: f64,
) -> Vec<[u8; 3]> {
    let n = palette.len();
    if labels.len() != colors.len() || n < 2 || reach <= 0.0 {
        return palette;
    }
    let mut total = vec![(0f64, [0f64; 3]); n];
    for (l, c) in labels.iter().zip(colors) {
        if let Some(t) = total.get_mut(*l) {
            t.0 += 1.0;
            for (sum, v) in t.1.iter_mut().zip(c) {
                *sum += *v as f64;
            }
        }
    }
    let solid = solid_colors(labels, w, h, n);
    let shared = shared_edges(labels, w, h, n);
    let mut pairs: Vec<(f64, usize, usize)> = Vec::new();
    for a in 0..n {
        for b in a + 1..n {
            let d = (distance2(palette[a], palette[b]) as f64).sqrt();
            // A shade that lines the edges of a solid colour joins it,
            // and so do shades scattered through each other (a flat
            // colour's noise split in several, their pixels side by side
            // all over: more shared edges than the smaller has pixels).
            // Two solid colours, however close, are both meant (a 246
            // grey logo on white), and the steps of a gradient, bands
            // side by side, stay apart.
            let fewer = total[a].0.min(total[b].0);
            let mixed = fewer > 0.0 && shared[a * n + b] as f64 >= INTERLEAVED * fewer;
            if d < reach && (solid[a] != solid[b] || mixed) && fewer > 0.0 {
                pairs.push((d, a, b));
            }
        }
    }
    if pairs.is_empty() {
        return palette;
    }
    pairs.sort_by(|p, q| p.0.total_cmp(&q.0));
    let mut root: Vec<usize> = (0..n).collect();
    fn find(root: &mut [usize], mut i: usize) -> usize {
        while root[i] != i {
            root[i] = root[root[i]];
            i = root[i];
        }
        i
    }
    let mean = |t: &(f64, [f64; 3])| -> [f64; 3] { std::array::from_fn(|d| t.1[d] / t.0) };
    let mut has_solid = solid;
    let mut size = vec![1usize; n];
    for (_, a, b) in pairs {
        let (ra, rb) = (find(&mut root, a), find(&mut root, b));
        if ra == rb {
            continue;
        }
        let (ma, mb) = (mean(&total[ra]), mean(&total[rb]));
        let d2: f64 = (0..3).map(|d| (ma[d] - mb[d]).powi(2)).sum();
        // A group holds one solid colour at most, and stays its colour.
        // The means matter once a group has grown (a run of shades must
        // not chain); two lone colours within reach of each other join
        // whatever their pixels' means, which the palette colour (moved
        // onto its commonest pixel) may miss by a little.
        let grown = size[ra] > 1 || size[rb] > 1;
        if (grown && d2 >= reach * reach) || (has_solid[ra] && has_solid[rb]) {
            continue;
        }
        size[ra] += size[rb];
        let (keep, gone) = if has_solid[ra] || (!has_solid[rb] && total[ra].0 >= total[rb].0) {
            (ra, rb)
        } else {
            (rb, ra)
        };
        has_solid[keep] |= has_solid[gone];
        root[gone] = keep;
        let t = total[gone];
        total[keep].0 += t.0;
        for d in 0..3 {
            total[keep].1[d] += t.1[d];
        }
    }
    // Each group keeps the colour of its solid member (the root): a mean
    // drew a black joined with its dark brown fringe brown.
    for l in labels.iter_mut() {
        if *l < n {
            *l = find(&mut root, *l);
        }
    }
    palette
}

/// How near (RGB distance) an edge pixel's colour must be to the one
/// flat colour beside it to join it.
const EDGE_JOIN: u32 = 48;

/// How far, in pixels, an edge pixel looks for flat colours: across a
/// band of three in-between pixels (a blurred or compressed edge) to the
/// flat pixel past the next one.
const EDGE_REACH: usize = 4;

/// Give the pixels along the edges between flat colours (anti-aliasing,
/// the fringes of compressed images) to those colours, so the edge runs
/// where the art's did instead of along a ragged mix of in-between
/// colours. A pixel is flat when its eight neighbours share its colour;
/// a colour is a colour of its own when at least half its pixels are
/// flat, else it is only the fringe of edges (#1250). A pixel that is not
/// flat, or is of a fringe colour, with no flat pixel of its own colour
/// of its own within `EDGE_REACH`, looks at the flat pixels of colours of
/// their own there: between two of them it takes the one its colour is
/// nearer; beside one alone, it takes that one when its colour is near
/// it. Lines of their own colour (text, hairlines) keep it.
fn settle_edges(
    labels: &mut [usize],
    w: usize,
    h: usize,
    palette: &[[u8; 3]],
    colors: &[[u8; 3]],
    cancel: Option<&Cancel>,
) {
    if labels.len() != w * h || colors.len() != w * h || w < 3 || h < 3 {
        return;
    }
    let flat = flat_pixels(labels, w, h);
    // Colours of their own: half their pixels flat or more, or a core
    // of flat pixels somewhere. The others only line edges (the fringes
    // of a blurred or compressed edge) and take the colours of their own
    // beside them, flat pixels and all.
    let (real, fringes) = colors_of_their_own(labels, &flat, w, h, palette.len());
    let banded = band_regions(labels, &real, w, h);
    let mut changes: Vec<(usize, usize)> = Vec::new();
    let mut near: Vec<usize> = Vec::new();
    for i in 0..w * h {
        if i % CANCEL_EVERY == 0 && cancelled(cancel) {
            return;
        }
        let own = labels[i];
        if own == usize::MAX || (flat[i] && real.get(own).copied().unwrap_or(true)) {
            continue;
        }
        let (x, y) = (i % w, i / w);
        // A pixel of a fringe colour in a band between two colours of
        // their own looks twice as far: the fringes of a compressed edge
        // come in several shades, bands side by side, and from the inner
        // band the colour across the outer one lies past `EDGE_REACH`.
        let fringe = banded[i] && fringes.get(own).copied().unwrap_or(false);
        let reach = if fringe { 2 * EDGE_REACH } else { EDGE_REACH };
        near.clear();
        for v in y.saturating_sub(reach)..=(y + reach).min(h - 1) {
            for u in x.saturating_sub(reach)..=(x + reach).min(w - 1) {
                let j = v * w + u;
                if flat[j] && real[labels[j]] && !near.contains(&labels[j]) {
                    near.push(labels[j]);
                }
            }
        }
        if near.is_empty() || near.contains(&own) {
            continue;
        }
        let c = colors[i];
        // The two colours whose edge the pixel lies on: the pair of the
        // colours of their own around it whose segment its colour is
        // nearest, when it is between them (not the two nearest it: a
        // grey of a black edge on white lies nearer a pale blue beside
        // them than black). Of a fringe colour, the palette colour
        // stands for the pixel when its own colour is off every
        // segment (the blends of a compressed edge drift in colour), and
        // the slack is twice as wide, or the colour a compressed blend of
        // the two ([`within_blend`]); between two colours, or near one,
        // it is their edge all the same.
        let edge_of = |col: [u8; 3], blend: bool| -> Option<(usize, usize)> {
            let mut best: Option<(f64, usize, usize)> = None;
            for (k, &a) in near.iter().enumerate() {
                for &b in &near[k + 1..] {
                    let (Some(&pa), Some(&pb)) = (palette.get(a), palette.get(b)) else {
                        continue;
                    };
                    if let Some((d, slack)) = off_segment(col, pa, pb) {
                        let fits = if blend {
                            d <= 2.0 * slack || within_blend(col, pa, pb)
                        } else {
                            d <= slack
                        };
                        if fits && best.is_none_or(|(bd, _, _)| d < bd) {
                            best = Some((d, a, b));
                        }
                    }
                }
            }
            best.map(|(_, a, b)| (a, b))
        };
        let side = |(a, b): (usize, usize)| {
            if nearer_first(c, palette[a], palette[b]) {
                a
            } else {
                b
            }
        };
        let nearest = |col: [u8; 3], within: u32| -> Option<usize> {
            near.iter()
                .filter_map(|l| palette.get(*l).map(|p| (distance2(col, *p), *l)))
                .min()
                .filter(|(d, _)| *d < within * within)
                .map(|(_, l)| l)
        };
        let to = edge_of(c, false)
            .map(side)
            .or_else(|| nearest(c, EDGE_JOIN))
            .or_else(|| {
                let po = palette.get(own).copied().filter(|_| fringe)?;
                edge_of(po, true)
                    .map(side)
                    .or_else(|| nearest(po, EDGE_CONTRAST))
            });
        if let Some(to) = to {
            changes.push((i, to));
        }
    }
    for (i, to) in changes {
        labels[i] = to;
    }
}

/// Whether each pixel is flat: its eight neighbours share its colour.
fn flat_pixels(labels: &[usize], w: usize, h: usize) -> Vec<bool> {
    (0..w * h)
        .map(|i| {
            let (x, y) = (i % w, i / w);
            let l = labels[i];
            l != usize::MAX
                && x > 0
                && y > 0
                && x + 1 < w
                && y + 1 < h
                && (y - 1..=y + 1).all(|v| (x - 1..=x + 1).all(|u| labels[v * w + u] == l))
        })
        .collect()
}

/// Whether `c` lies nearer `a` than `b` along the way from one to the
/// other (its projection on the segment before the halfway mark).
fn nearer_first(c: [u8; 3], a: [u8; 3], b: [u8; 3]) -> bool {
    let ab: [i32; 3] = std::array::from_fn(|k| b[k] as i32 - a[k] as i32);
    let ac: [i32; 3] = std::array::from_fn(|k| c[k] as i32 - a[k] as i32);
    let dot: i32 = (0..3).map(|k| ab[k] * ac[k]).sum();
    let len2: i32 = (0..3).map(|k| ab[k] * ab[k]).sum();
    2 * dot < len2
}

/// `img` with the noise of photos and compressed images taken out (#611):
/// each colour channel of each pixel becomes the middle value of the
/// pixel and its four edge neighbours (a median over a plus-shaped
/// window). Isolated odd pixels take the value around them, while
/// straight edges, square corners and lines one pixel wide along the rows
/// or columns keep their place (a line's end pixels go), unlike a blur
/// that softens every edge.
/// Alpha is left as it is, and transparent pixels take no part.
/// `None` once `cancel` is set (looked at every few rows).
fn denoise(img: &RgbaImage, cancel: Option<&Cancel>) -> Option<RgbaImage> {
    let (w, h) = (img.width(), img.height());
    let mut out = img.clone();
    if w < 3 || h < 3 {
        return Some(out);
    }
    for y in 0..h {
        if y % 32 == 0 && cancelled(cancel) {
            return None;
        }
        for x in 0..w {
            let at = |dx: i64, dy: i64| {
                let px = (x as i64 + dx).clamp(0, w as i64 - 1) as u32;
                let py = (y as i64 + dy).clamp(0, h as i64 - 1) as u32;
                *img.get_pixel(px, py)
            };
            let mut p = *img.get_pixel(x, y);
            if p[3] < 128 {
                continue;
            }
            // Opaque pixels only: the colour under a transparent one
            // means nothing.
            let mut window = [p; 5];
            let mut n = 1;
            for q in [at(-1, 0), at(1, 0), at(0, -1), at(0, 1)] {
                if q[3] >= 128 {
                    window[n] = q;
                    n += 1;
                }
            }
            for k in 0..3 {
                let mut v = [0u8; 5];
                for (slot, q) in v.iter_mut().zip(&window[..n]) {
                    *slot = q[k];
                }
                let v = &mut v[..n];
                v.sort_unstable();
                p[k] = v[n / 2];
            }
            out.put_pixel(x, y, p);
        }
    }
    Some(out)
}

/// How many times noise is taken out: none from [`NOISE_STAYS`] up,
/// once at high detail, twice below.
fn denoise_passes(detail: f64) -> usize {
    if detail >= NOISE_STAYS {
        0
    } else if detail >= 75.0 {
        1
    } else {
        2
    }
}

/// The smallest patch of one colour that is kept, in pixels: every
/// patch at detail 100, two pixels just below it, a few at 80, about
/// 40 at 35.
fn speck_area(detail: f64) -> usize {
    if detail >= ALL_DETAIL {
        return 1;
    }
    let k = (100.0 - detail.clamp(0.0, 100.0)) / 100.0 * 10.0;
    (k * k).round().max(2.0) as usize
}

/// From this detail up nothing is cleaned: every pixel of the picture
/// is taken as art. Below it noise is taken out once at least and a
/// patch of a single pixel goes, however high the detail.
const ALL_DETAIL: f64 = 99.5;
/// From this detail up noise is not taken out of the picture first:
/// that would take the finest lines of the art with it.
const NOISE_STAYS: f64 = 88.0;

/// The smallest dot of the art kept however small the detail asks
/// patches to be: a quarter of [`speck_area`], at least 2 pixels (4 at
/// the Clipart detail of 60), so the dots of a halftone and the ends of
/// fine hatching stay.
fn dot_area(detail: f64) -> usize {
    (speck_area(detail) / 4).max(2)
}

/// How far (RGB distance) a small patch's colour must be from every
/// colour around it to be a dot of the art rather than noise.
const DOT_CONTRAST: u32 = 64;

/// How far (RGB distance) a dot must stand from every colour around it
/// to be kept from `GLINT_AREA` pixels at any detail.
const GLINT_CONTRAST: u32 = 160;
const GLINT_AREA: usize = 4;
/// How far (RGB distance) a patch of two pixels or more inside one
/// colour must stand from it to be kept as a gap in it.
const GAP_CONTRAST: u32 = 120;

/// Give every 4-connected patch of one label smaller than `min_area`
/// pixels the label of the neighbours outside it whose colour is
/// nearest its own, when one is within `SPLIT_REACH`, else the label most
/// of them have (transparent pixels are left alone and never spread). A
/// piece of a dark letter split into several dark shades joins the other
/// shades and stays a letter, where the colour around it would erase it
/// (#611). Patches joined into others are looked at again, until none
/// is left. First the colours with fewer pixels in all than `min_area`
/// join each other when within `TINY_REACH`: a glint blurred into a
/// white and two greys, a pixel or two of each, is one speck of nine
/// pixels, not nine specks of one that pass their pixels around.
fn despeckle(
    labels: &mut [usize],
    w: usize,
    h: usize,
    min_area: usize,
    keep_area: usize,
    palette: &[[u8; 3]],
    cancel: Option<&Cancel>,
) {
    despeckle_of(labels, w, h, min_area, keep_area, 1, palette, cancel);
}

/// [`despeckle`] of a picture with `unit` pixels to each pixel of the
/// art (16 for one enlarged four times): the areas given are in pixels
/// of the picture, and so are the fixed ones it uses, times `unit`.
#[allow(clippy::too_many_arguments)]
fn despeckle_of(
    labels: &mut [usize],
    w: usize,
    h: usize,
    min_area: usize,
    keep_area: usize,
    unit: usize,
    palette: &[[u8; 3]],
    cancel: Option<&Cancel>,
) {
    if min_area <= 1 || labels.len() != w * h {
        return;
    }
    join_tiny_colors(labels, min_area, palette);
    for _ in 0..4 {
        if !despeckle_pass(labels, w, h, min_area, keep_area, unit, palette, cancel)
            || cancelled(cancel)
        {
            break;
        }
    }
}

/// How far apart (RGB distance) two colours of fewer pixels than the
/// speck area can be and join: twice [`SPLIT_REACH`], as the shades a
/// small bright mark is blurred into lie further apart than the noise
/// of a flat colour.
const TINY_REACH: f64 = 2.0 * SPLIT_REACH;

/// Join the colours with fewer than `min_area` pixels in all to each
/// other, nearest pairs first, within `TINY_REACH`, each group keeping
/// the colour with most pixels. The colours of substance are left
/// alone: a speck beside one takes it or stands out from it by place.
fn join_tiny_colors(labels: &mut [usize], min_area: usize, palette: &[[u8; 3]]) {
    let n = palette.len();
    let mut count = vec![0usize; n];
    for l in labels.iter() {
        if let Some(c) = count.get_mut(*l) {
            *c += 1;
        }
    }
    let tiny: Vec<usize> = (0..n)
        .filter(|a| count[*a] > 0 && count[*a] < min_area)
        .collect();
    let mut pairs: Vec<(u32, usize, usize)> = Vec::new();
    for (k, &a) in tiny.iter().enumerate() {
        for &b in &tiny[k + 1..] {
            let d = distance2(palette[a], palette[b]);
            if d < (TINY_REACH * TINY_REACH) as u32 {
                pairs.push((d, a, b));
            }
        }
    }
    if pairs.is_empty() {
        return;
    }
    pairs.sort_unstable();
    let mut root: Vec<usize> = (0..n).collect();
    fn find(root: &mut [usize], mut i: usize) -> usize {
        while root[i] != i {
            root[i] = root[root[i]];
            i = root[i];
        }
        i
    }
    for (_, a, b) in pairs {
        let (ra, rb) = (find(&mut root, a), find(&mut root, b));
        if ra == rb {
            continue;
        }
        let (keep, gone) = if count[ra] >= count[rb] {
            (ra, rb)
        } else {
            (rb, ra)
        };
        root[gone] = keep;
        count[keep] += count[gone];
    }
    for l in labels.iter_mut() {
        if *l < n {
            *l = find(&mut root, *l);
        }
    }
}

/// Whether the pixels of the label at `start` reached from it through
/// sides and corners number at least `area`.
fn reaches_by_corners(labels: &[usize], w: usize, h: usize, start: usize, area: usize) -> bool {
    let l = labels[start];
    let mut found = vec![start];
    let mut next = 0;
    while next < found.len() && found.len() < area {
        let i = found[next];
        next += 1;
        let (x, y) = (i % w, i / w);
        for v in y.saturating_sub(1)..=(y + 1).min(h - 1) {
            for u in x.saturating_sub(1)..=(x + 1).min(w - 1) {
                let j = v * w + u;
                if labels[j] == l && !found.contains(&j) {
                    found.push(j);
                }
            }
        }
    }
    found.len() >= area
}

/// One pass of `despeckle`; whether it changed anything.
#[allow(clippy::too_many_arguments)]
fn despeckle_pass(
    labels: &mut [usize],
    w: usize,
    h: usize,
    min_area: usize,
    keep_area: usize,
    unit: usize,
    palette: &[[u8; 3]],
    cancel: Option<&Cancel>,
) -> bool {
    let mut changed = false;
    let mut seen = vec![false; w * h];
    // The patch each pixel was last gathered into: pixels given the
    // patch's label earlier in this pass (seen already, as part of the
    // patch they were) are gathered into it too, so a patch handed to
    // its neighbour joins it at once instead of leaving it cut off
    // (#1619: the steps of a slanted hairline went one by one to the
    // page around them).
    let mut gathered = vec![usize::MAX; w * h];
    let mut patch: Vec<usize> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    let mut counts: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for start in 0..w * h {
        if start % CANCEL_EVERY == 0 && cancelled(cancel) {
            return false;
        }
        if seen[start] || labels[start] == usize::MAX {
            continue;
        }
        let l = labels[start];
        patch.clear();
        counts.clear();
        stack.push(start);
        seen[start] = true;
        gathered[start] = start;
        while let Some(i) = stack.pop() {
            patch.push(i);
            let (x, y) = (i % w, i / w);
            let mut near = [usize::MAX; 4];
            if x > 0 {
                near[0] = i - 1;
            }
            if x + 1 < w {
                near[1] = i + 1;
            }
            if y > 0 {
                near[2] = i - w;
            }
            if y + 1 < h {
                near[3] = i + w;
            }
            for j in near.into_iter().filter(|j| *j != usize::MAX) {
                if labels[j] == l {
                    if gathered[j] != start {
                        seen[j] = true;
                        gathered[j] = start;
                        stack.push(j);
                    }
                } else if labels[j] != usize::MAX && patch.len() < min_area {
                    *counts.entry(labels[j]).or_default() += 1;
                }
            }
        }
        if patch.len() < min_area {
            let own = palette.get(l).copied();
            let far = |k: usize| match (own, palette.get(k)) {
                (Some(a), Some(b)) => distance2(a, *b),
                _ => u32::MAX,
            };
            let reach = (SPLIT_REACH * SPLIT_REACH) as u32;
            let near = counts
                .iter()
                .filter(|(k, _)| far(**k) < reach)
                .min_by_key(|(k, c)| (far(**k), usize::MAX - **c, **k));
            let most = || counts.iter().max_by_key(|(k, c)| (**c, usize::MAX - **k));
            // Between two neighbours (the anti-aliased part of a thin
            // dark line on a light fill): the one it is nearer.
            let side = || {
                let own = own?;
                let mut by: Vec<(&usize, &usize)> = counts.iter().collect();
                by.sort_by_key(|(k, c)| (usize::MAX - **c, **k));
                by.iter()
                    .enumerate()
                    .flat_map(|(n, a)| by[n + 1..].iter().map(move |b| (*a, *b)))
                    .find_map(|((a, _), (b, _))| {
                        let (pa, pb) = (*palette.get(*a)?, *palette.get(*b)?);
                        (distance2(pa, pb) >= EDGE_CONTRAST * EDGE_CONTRAST && between(own, pa, pb))
                            .then_some(if nearer_first(own, pa, pb) { a } else { b })
                    })
                    .map(|k| (k, k))
            };
            // A dot of the art (a halftone dot, the end of a hatch line):
            // no neighbour near its colour, standing out from every one of
            // them, big enough to be drawn and two pixels across both
            // ways, keeps its colour. Noise
            // differs little from what is around it and went to the
            // nearest colour above; the soft edge of two colours lies
            // between them and went to one of them.
            let stands_out = || {
                let own = own?;
                // Two pixels across at least, both ways: a column of one
                // (a colour bled sideways by compression) is no dot.
                let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
                for &i in &patch {
                    let (x, y) = (i % w, i / w);
                    (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
                }
                // A dot standing out strongly from everything around it
                // (the white glint in a black eye) is drawn from four
                // pixels however low the detail: noise differs from its
                // surroundings by far less once the picture is quietened.
                let contrast = counts
                    .keys()
                    .map(|k| palette.get(*k).map_or(0, |p| distance2(own, *p)))
                    .min()
                    .unwrap_or(0);
                let big_enough = patch.len() >= keep_area
                    || (patch.len() >= GLINT_AREA * unit
                        && contrast >= GLINT_CONTRAST * GLINT_CONTRAST);
                // The gap in a small letter (the bowls of a B seven
                // pixels high): two pixels or more (half a pixel of the
                // art in an enlarged picture, which draws the gap of an A
                // smaller than it was), one colour all round, far from
                // it.
                let gap = patch.len() >= (unit / 2).max(2)
                    && counts.len() == 1
                    && contrast >= GAP_CONTRAST * GAP_CONTRAST;
                (gap || big_enough && x1 > x0 && y1 > y0 && contrast >= DOT_CONTRAST * DOT_CONTRAST)
                    .then_some(())
            };
            if near.is_none() && side().is_none() && stands_out().is_some() {
                continue;
            }
            // A step of a thin slanted line: its pixels touch the line's
            // next step by a corner only, so each step is a patch of two
            // or three pixels, and the line went step by step (#2927).
            // Counted with the steps it touches by a corner, it is big
            // enough to stay.
            let line = || {
                let own = own?;
                let contrast = counts
                    .keys()
                    .map(|k| palette.get(*k).map_or(0, |p| distance2(own, *p)))
                    .min()?;
                (contrast >= DOT_CONTRAST * DOT_CONTRAST
                    && reaches_by_corners(labels, w, h, start, min_area))
                .then_some(())
            };
            if near.is_none() && side().is_none() && line().is_some() {
                continue;
            }
            // A piece of a thin line in a shade of its own (the dark core
            // of a slanted hairline between lighter steps of it): the
            // neighbour of a colour far nearer its own than the colour
            // most of them have, so the line is not cut where the shade
            // changes (#1619).
            let nearest = || {
                let (k, c) = counts
                    .iter()
                    .min_by_key(|(k, c)| (far(**k), usize::MAX - **c, **k))?;
                let (m, _) = most()?;
                (k != m && far(*k).saturating_mul(4) < far(*m)).then_some((k, c))
            };
            if let Some((&to, _)) = near.or_else(side).or_else(nearest).or_else(most) {
                for &i in &patch {
                    labels[i] = to;
                }
                changed = true;
            }
        }
    }
    changed
}

/// How many points away a fitted curve's end direction is taken from.
const TANGENT_REACH: usize = 4;

fn unit(v: tracedraw_core::geometry::Vec2) -> tracedraw_core::geometry::Vec2 {
    let l = v.hypot();
    if l > 1e-12 && l.is_finite() {
        v / l
    } else {
        tracedraw_core::geometry::Vec2::ZERO
    }
}

/// Fit cubic Beziers to `pts` (from the first point to the last), the
/// first leaving along `t1` and the last arriving back along `t2` (unit
/// vectors pointing into the curve from its ends), no point further than
/// `tolerance` from them, and append them to `out`: least squares on the
/// chord-length parameters, improved by Newton steps, split at the worst
/// point when it still does not fit (P. J. Schneider, "An Algorithm for
/// Automatically Fitting Digitized Curves", Graphics Gems, 1990).
pub(crate) fn fit_cubics(
    pts: &[Point],
    t1: tracedraw_core::geometry::Vec2,
    t2: tracedraw_core::geometry::Vec2,
    tolerance: f64,
    out: &mut BezPath,
) {
    fit_cubics_depth(pts, t1, t2, tolerance, out, 0);
}

fn fit_cubics_depth(
    pts: &[Point],
    t1: tracedraw_core::geometry::Vec2,
    t2: tracedraw_core::geometry::Vec2,
    tolerance: f64,
    out: &mut BezPath,
    depth: usize,
) {
    use tracedraw_core::geometry::{CubicBez, ParamCurve, ParamCurveDeriv};
    let n = pts.len();
    if n < 2 {
        return;
    }
    let (p0, p3) = (pts[0], pts[n - 1]);
    let dist = (p3 - p0).hypot();
    if n == 2 || depth > 24 {
        let h = dist / 3.0;
        out.curve_to(p0 + t1 * h, p3 + t2 * h, p3);
        return;
    }
    // Chord-length parameters.
    let mut u = vec![0.0; n];
    for i in 1..n {
        u[i] = u[i - 1] + (pts[i] - pts[i - 1]).hypot();
    }
    let total = u[n - 1];
    if total < 1e-12 {
        return;
    }
    for v in u.iter_mut() {
        *v /= total;
    }
    let generate = |u: &[f64]| -> CubicBez {
        // The handle lengths minimising the squared distances.
        let (mut c00, mut c01, mut c11, mut x0, mut x1) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for (p, &t) in pts.iter().zip(u) {
            let mt = 1.0 - t;
            let b0 = mt * mt * mt;
            let b1 = 3.0 * t * mt * mt;
            let b2 = 3.0 * t * t * mt;
            let b3 = t * t * t;
            let a1 = t1 * b1;
            let a2 = t2 * b2;
            c00 += a1.dot(a1);
            c01 += a1.dot(a2);
            c11 += a2.dot(a2);
            let tmp = p.to_vec2() - (p0.to_vec2() * (b0 + b1) + p3.to_vec2() * (b2 + b3));
            x0 += a1.dot(tmp);
            x1 += a2.dot(tmp);
        }
        let det = c00 * c11 - c01 * c01;
        let (mut l1, mut l2) = if det.abs() > 1e-12 {
            ((x0 * c11 - x1 * c01) / det, (c00 * x1 - c01 * x0) / det)
        } else {
            (0.0, 0.0)
        };
        let eps = 1e-6 * dist;
        if !(l1.is_finite() && l2.is_finite()) || l1 < eps || l2 < eps {
            l1 = dist / 3.0;
            l2 = dist / 3.0;
        }
        CubicBez::new(p0, p0 + t1 * l1, p3 + t2 * l2, p3)
    };
    let worst = |c: &CubicBez, u: &[f64]| -> (f64, usize) {
        let mut best = (0.0, n / 2);
        for i in 1..n - 1 {
            let d = (c.eval(u[i]) - pts[i]).hypot2();
            if d > best.0 {
                best = (d, i);
            }
        }
        best
    };
    let mut curve = generate(&u);
    let (mut err, mut split) = worst(&curve, &u);
    let tol2 = tolerance * tolerance;
    if err <= tol2 {
        out.curve_to(curve.p1, curve.p2, curve.p3);
        return;
    }
    if err < tol2 * 4.0 {
        for _ in 0..4 {
            // Newton steps towards each point's nearest parameter.
            let d1 = curve.deriv();
            let d2 = d1.deriv();
            for i in 0..n {
                let t = u[i];
                let diff = curve.eval(t) - pts[i];
                let q1 = d1.eval(t).to_vec2();
                let q2 = d2.eval(t).to_vec2();
                let den = q1.dot(q1) + diff.dot(q2);
                if den.abs() > 1e-12 {
                    u[i] = (t - diff.dot(q1) / den).clamp(0.0, 1.0);
                }
            }
            curve = generate(&u);
            (err, split) = worst(&curve, &u);
            if err <= tol2 {
                out.curve_to(curve.p1, curve.p2, curve.p3);
                return;
            }
        }
    }
    let split = split.clamp(1, n - 2);
    let reach = TANGENT_REACH.min(split).min(n - 1 - split).max(1);
    let centre = unit(pts[split - reach] - pts[split + reach]);
    let centre = if centre == tracedraw_core::geometry::Vec2::ZERO {
        unit(pts[split - 1] - pts[split])
    } else {
        centre
    };
    fit_cubics_depth(&pts[..=split], t1, centre, tolerance, out, depth + 1);
    fit_cubics_depth(&pts[split..], -centre, t2, tolerance, out, depth + 1);
}

fn luma(p: Rgba<u8>) -> f32 {
    0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32
}

fn nearest(palette: &[[u8; 3]], c: [u8; 3]) -> usize {
    let mut best = (u32::MAX, 0);
    for (i, p) in palette.iter().enumerate() {
        let d = (0..3)
            .map(|k| (p[k] as i32 - c[k] as i32).pow(2) as u32)
            .sum::<u32>();
        if d < best.0 {
            best = (d, i);
        }
    }
    best.1
}

/// Median cut colour quantisation to `n` colours, giving up (`None`)
/// once `cancel` is set.
pub fn median_cut(img: &RgbaImage, n: usize, cancel: Option<&Cancel>) -> Option<Vec<[u8; 3]>> {
    let pixels: Vec<[u8; 3]> = img
        .pixels()
        .filter(|p| p[3] >= 128)
        .map(|p| [p[0], p[1], p[2]])
        .collect();
    median_cut_of(pixels, n, cancel)
}

/// How near (RGB distance) a pixel's eight neighbours must be to it for
/// it to lie inside a flat colour of the art.
const FLAT_NOISE: u32 = 24;

/// The share of the opaque pixels that must lie inside flat colours for
/// the palette to be taken from them alone.
const FLAT_SHARE: f64 = 0.3;

/// The palette of `img` in `n` colours. In art drawn in flat colours (at
/// least `FLAT_SHARE` of its pixels inside one), from the pixels inside
/// the flat colours only: the in-between colours of anti-aliased and
/// compressed edges, a pixel or two wide but many, took palette places
/// of their own and left too few for the art's real colours, which came
/// out split or mixed. Photos take every pixel.
pub fn art_palette(img: &RgbaImage, n: usize, cancel: Option<&Cancel>) -> Option<Vec<[u8; 3]>> {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let raw = img.as_raw();
    let at = |i: usize| [raw[i * 4], raw[i * 4 + 1], raw[i * 4 + 2]];
    let opaque = |i: usize| raw[i * 4 + 3] >= 128;
    let mut flat: Vec<[u8; 3]> = Vec::new();
    let mut all = 0usize;
    for y in 0..h {
        if y % 64 == 0 && cancelled(cancel) {
            return None;
        }
        for x in 0..w {
            let i = y * w + x;
            if !opaque(i) {
                continue;
            }
            all += 1;
            if x == 0 || y == 0 || x + 1 == w || y + 1 == h {
                continue;
            }
            let c = at(i);
            let inside = (y - 1..=y + 1).all(|v| {
                (x - 1..=x + 1).all(|u| {
                    let j = v * w + u;
                    opaque(j) && distance2(at(j), c) <= FLAT_NOISE * FLAT_NOISE
                })
            });
            if inside {
                flat.push(c);
            }
        }
    }
    let every = median_cut(img, n, cancel)?;
    if all == 0 || (flat.len() as f64) < FLAT_SHARE * all as f64 {
        return Some(every);
    }
    // Colours of the art too thin to be flat anywhere (a hairline of a
    // colour of its own) come from the palette of every pixel: those far
    // from every flat colour and not the blend of the two nearest them,
    // as an anti-aliased edge's pixels are. They take up to a quarter of
    // the places, the flat colours the rest: never more colours than
    // asked for.
    let base = median_cut_of(flat.clone(), n, cancel)?;
    let extra: Vec<[u8; 3]> = every
        .into_iter()
        .filter(|c| unexplained(*c, &base))
        .take(n / 4)
        .collect();
    if extra.is_empty() {
        return Some(base);
    }
    let mut palette = median_cut_of(flat, n - extra.len(), cancel)?;
    palette.extend(extra);
    Some(palette)
}

/// How far (RGB distance) a colour must be from every colour of a
/// palette for the palette not to stand for it.
const UNEXPLAINED: u32 = 56;

/// Whether `c` is far from every colour of `palette` and not between
/// the two nearest it.
fn unexplained(c: [u8; 3], palette: &[[u8; 3]]) -> bool {
    let mut best = [(u32::MAX, 0usize); 2];
    for (i, p) in palette.iter().enumerate() {
        let d = distance2(c, *p);
        if d < best[0].0 {
            best[1] = best[0];
            best[0] = (d, i);
        } else if d < best[1].0 {
            best[1] = (d, i);
        }
    }
    if best[0].0 <= UNEXPLAINED * UNEXPLAINED {
        return false;
    }
    match (palette.get(best[0].1), palette.get(best[1].1)) {
        (Some(a), Some(b)) if best[1].0 != u32::MAX => !between(c, *a, *b),
        _ => true,
    }
}

/// [`median_cut`] of a list of colours.
fn median_cut_of(
    mut pixels: Vec<[u8; 3]>,
    n: usize,
    cancel: Option<&Cancel>,
) -> Option<Vec<[u8; 3]>> {
    if pixels.is_empty() {
        return Some(vec![[0, 0, 0]]);
    }
    // Subsample large images.
    if pixels.len() > 200_000 {
        let step = pixels.len() / 200_000 + 1;
        pixels = pixels.into_iter().step_by(step).collect();
    }
    let mut buckets: Vec<Vec<[u8; 3]>> = vec![pixels.clone()];
    while buckets.len() < n {
        if cancelled(cancel) {
            return None;
        }
        // Split the bucket with the largest range.
        let mut best: Option<(usize, usize, i32)> = None; // (bucket, channel, range)
        for (bi, b) in buckets.iter().enumerate() {
            if b.len() < 2 {
                continue;
            }
            for ch in 0..3 {
                let (mn, mx) = b.iter().fold((255i32, 0i32), |(mn, mx), p| {
                    (mn.min(p[ch] as i32), mx.max(p[ch] as i32))
                });
                let r = mx - mn;
                if best.map(|b| r > b.2).unwrap_or(true) {
                    best = Some((bi, ch, r));
                }
            }
        }
        let Some((bi, ch, r)) = best else { break };
        if r == 0 {
            break;
        }
        let mut b = buckets.remove(bi);
        b.sort_by_key(|p| p[ch]);
        // Cut at the median, but never through a run of equal values: a
        // picture of mostly black and white would otherwise put both in
        // one bucket and average them to grey.
        let v = b[b.len() / 2][ch];
        let mut mid = b.partition_point(|p| p[ch] < v);
        if mid == 0 {
            mid = b.partition_point(|p| p[ch] <= v);
        }
        let hi = b.split_off(mid);
        buckets.push(b);
        buckets.push(hi);
    }
    let start: Vec<[u8; 3]> = buckets
        .iter()
        .filter(|b| !b.is_empty())
        .map(|b| {
            let n = b.len() as u32;
            let s = b.iter().fold([0u32; 3], |acc, p| {
                [
                    acc[0] + p[0] as u32,
                    acc[1] + p[1] as u32,
                    acc[2] + p[2] as u32,
                ]
            });
            [(s[0] / n) as u8, (s[1] / n) as u8, (s[2] / n) as u8]
        })
        .collect();
    refine(&pixels, start, cancel)
}

/// How near (RGB distance) the commonest exact colour of a palette
/// colour's pixels must be to take its place.
const SNAP_REACH: u32 = 16;

/// Each colour of the palette moved onto the commonest exact colour of
/// its pixels, when that is near: the mean of a flat colour's noisy
/// pixels, and of colours joined into one, was a shade off the colour the
/// art was drawn in (a black traced as #0B0703), and a trace should keep
/// the art's own colours. Every other pixel is counted, which
/// finds the commonest colour of any region worth a curve.
fn snap_to_commonest(labels: &[usize], colors: &[[u8; 3]], palette: &mut [[u8; 3]]) {
    use std::collections::HashMap;
    let mut counts: Vec<HashMap<[u8; 3], u32>> = vec![HashMap::new(); palette.len()];
    for (l, c) in labels.iter().zip(colors).step_by(2) {
        if let Some(m) = counts.get_mut(*l) {
            *m.entry(*c).or_default() += 1;
        }
    }
    for (p, m) in palette.iter_mut().zip(&counts) {
        if let Some((best, _)) = m.iter().max_by_key(|(k, n)| (**n, **k)) {
            if distance2(*best, *p) <= SNAP_REACH * SNAP_REACH {
                *p = *best;
            }
        }
    }
}

/// A few rounds of k-means from the median cut's colours: each colour
/// moves to the mean of the pixels nearest it, so the palette lands on
/// the colours the picture really has.
fn refine(
    pixels: &[[u8; 3]],
    mut palette: Vec<[u8; 3]>,
    cancel: Option<&Cancel>,
) -> Option<Vec<[u8; 3]>> {
    for _ in 0..6 {
        if cancelled(cancel) {
            return None;
        }
        let mut sums = vec![[0u64; 4]; palette.len()];
        for p in pixels {
            let i = nearest(&palette, *p);
            for k in 0..3 {
                sums[i][k] += p[k] as u64;
            }
            sums[i][3] += 1;
        }
        let mut moved = false;
        for (c, s) in palette.iter_mut().zip(&sums) {
            if s[3] == 0 {
                continue;
            }
            let m = [
                (s[0] / s[3]) as u8,
                (s[1] / s[3]) as u8,
                (s[2] / s[3]) as u8,
            ];
            moved |= m != *c;
            *c = m;
        }
        if !moved {
            break;
        }
    }
    palette.dedup();
    Some(palette)
}

// ----- centreline mode --------------------------------------------------------

fn centerline(img: &RgbaImage, s: &Settings, cancel: Option<&Cancel>) -> Option<Vec<Traced>> {
    let (w, h) = (img.width() as usize, img.height() as usize);
    if w == 0 || h == 0 {
        return Some(Vec::new());
    }
    let mut mask: Vec<bool> = img
        .pixels()
        .map(|p| p[3] >= 128 && luma(*p) < 128.0)
        .collect();
    // Mean stroke thickness before thinning: area / skeleton length later.
    let area = mask.iter().filter(|m| **m).count() as f64;
    if !thin_unless(&mut mask, w, h, cancel) {
        return None;
    }
    let skeleton = mask.iter().filter(|m| **m).count().max(1) as f64;
    let width = (area / skeleton).clamp(1.0, 50.0);
    // Trace skeleton pixels into polylines by walking neighbours.
    let mut visited = vec![false; w * h];
    let idx = |x: usize, y: usize| y * w + x;
    let neighbours = |x: usize, y: usize, mask: &[bool]| -> Vec<(usize, usize)> {
        let mut v = Vec::new();
        for dy in -1i64..=1 {
            for dx in -1i64..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                if nx >= 0
                    && ny >= 0
                    && (nx as usize) < w
                    && (ny as usize) < h
                    && mask[ny as usize * w + nx as usize]
                {
                    v.push((nx as usize, ny as usize));
                }
            }
        }
        v
    };
    let mut lines: Vec<Vec<Point>> = Vec::new();
    // Start from endpoints first (one neighbour), then anything left.
    let mut starts: Vec<(usize, usize)> = Vec::new();
    for y in 0..h {
        for x in 0..w {
            if mask[idx(x, y)] && neighbours(x, y, &mask).len() == 1 {
                starts.push((x, y));
            }
        }
    }
    for y in 0..h {
        for x in 0..w {
            if mask[idx(x, y)] {
                starts.push((x, y));
            }
        }
    }
    for (sx, sy) in starts {
        if visited[idx(sx, sy)] {
            continue;
        }
        let mut line = vec![Point::new(sx as f64 + 0.5, sy as f64 + 0.5)];
        visited[idx(sx, sy)] = true;
        let (mut x, mut y) = (sx, sy);
        loop {
            let next = neighbours(x, y, &mask)
                .into_iter()
                .find(|(nx, ny)| !visited[idx(*nx, *ny)]);
            let Some((nx, ny)) = next else { break };
            visited[idx(nx, ny)] = true;
            line.push(Point::new(nx as f64 + 0.5, ny as f64 + 0.5));
            x = nx;
            y = ny;
        }
        if line.len() >= 2 {
            lines.push(line);
        }
    }
    if cancelled(cancel) {
        return None;
    }
    let tolerance = 0.5 + (100.0 - s.detail) / 100.0 * 2.0;
    let mut out = Vec::new();
    let c = s.shown_color([0, 0, 0]);
    for l in lines {
        let simplified = tracedraw_core::geometry::simplify(&l, tolerance);
        if simplified.len() < 2 {
            continue;
        }
        let path = if s.smoothing > 0.0 && simplified.len() > 2 {
            tracedraw_core::geometry::smooth_path(&simplified, false)
        } else {
            tracedraw_core::geometry::polyline_path(&simplified, false)
        };
        out.push(Traced {
            path,
            color: Color::rgb8(c[0], c[1], c[2]),
            closed: false,
            blend: None,
        });
    }
    // Encode the stroke width in the first result's colour alpha? No: the
    // caller reads `stroke_width` through the returned tuple below.
    STROKE_WIDTH.with(|c| c.set(width));
    Some(out)
}

thread_local! {
    static STROKE_WIDTH: std::cell::Cell<f64> = const { std::cell::Cell::new(1.0) };
}

/// Mean stroke width (pixels) measured by the last centreline trace.
pub fn last_stroke_width_px() -> f64 {
    STROKE_WIDTH.with(|c| c.get())
}

/// Zhang-Suen thinning, stopping between passes once `cancel` is set;
/// whether it finished.
pub(crate) fn thin_unless(mask: &mut [bool], w: usize, h: usize, cancel: Option<&Cancel>) -> bool {
    let at = |m: &[bool], x: i64, y: i64| -> bool {
        x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h && m[y as usize * w + x as usize]
    };
    loop {
        if cancelled(cancel) {
            return false;
        }
        let mut changed = false;
        for pass in 0..2 {
            let mut remove = Vec::new();
            for y in 0..h as i64 {
                for x in 0..w as i64 {
                    if !at(mask, x, y) {
                        continue;
                    }
                    let p2 = at(mask, x, y - 1);
                    let p3 = at(mask, x + 1, y - 1);
                    let p4 = at(mask, x + 1, y);
                    let p5 = at(mask, x + 1, y + 1);
                    let p6 = at(mask, x, y + 1);
                    let p7 = at(mask, x - 1, y + 1);
                    let p8 = at(mask, x - 1, y);
                    let p9 = at(mask, x - 1, y - 1);
                    let ring = [p2, p3, p4, p5, p6, p7, p8, p9];
                    let b = ring.iter().filter(|v| **v).count();
                    if !(2..=6).contains(&b) {
                        continue;
                    }
                    let a = (0..8).filter(|i| !ring[*i] && ring[(i + 1) % 8]).count();
                    if a != 1 {
                        continue;
                    }
                    let (c1, c2) = if pass == 0 {
                        (p2 && p4 && p6, p4 && p6 && p8)
                    } else {
                        (p2 && p4 && p8, p2 && p6 && p8)
                    };
                    if c1 || c2 {
                        continue;
                    }
                    remove.push((x as usize, y as usize));
                }
            }
            if !remove.is_empty() {
                changed = true;
            }
            for (x, y) in remove {
                mask[y * w + x] = false;
            }
        }
        if !changed {
            return true;
        }
    }
}

/// Draw traced shapes (pixel space, y down) into a transparent image of
/// `w` x `h`: closed shapes filled with their colour, centrelines stroked
/// `stroke_px` wide ([`optimize`] and the tests draw traces back this
/// way; the dialog's preview is [`render_traced_scaled`]).
pub fn render_traced(traced: &[Traced], w: u32, h: u32, stroke_px: f64) -> RgbaImage {
    paint_traced(traced, w, h, 1.0, Paint::Filled(stroke_px))
}

/// [`render_traced`] into an image `scale` times the trace's pixel space
/// (`w` x `h` is the trace's size): the dialog's preview of a trace made
/// on the whole picture.
pub fn render_traced_scaled(
    traced: &[Traced],
    w: u32,
    h: u32,
    scale: f32,
    stroke_px: f64,
) -> RgbaImage {
    paint_traced(traced, w, h, scale, Paint::Filled(stroke_px))
}

/// The curves of `traced` as thin lines of one colour on a transparent
/// image `scale` times the trace's `w` x `h`: the dialog's wireframe,
/// laid over the picture.
pub fn render_wireframe(traced: &[Traced], w: u32, h: u32, scale: f32) -> RgbaImage {
    paint_traced(traced, w, h, scale, Paint::Wire)
}

enum Paint {
    /// Fills and centrelines (this wide, pixels) in their colours.
    Filled(f64),
    /// Every curve as a one point line in the wireframe colour.
    Wire,
}

/// The colour of the wireframe's lines.
const WIRE: [u8; 4] = [0x1E, 0x5A, 0xC8, 0xFF];

fn paint_traced(traced: &[Traced], w: u32, h: u32, scale: f32, how: Paint) -> RgbaImage {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let (pw, ph) = (
        ((w as f32 * scale).round() as u32).max(1),
        ((h as f32 * scale).round() as u32).max(1),
    );
    paint_traced_at(traced, (pw, ph), scale, (0.0, 0.0), how)
}

/// A `size` part of `traced` drawn `scale` times the trace's pixels, its
/// top left `origin` (pixels of the result) from the trace's: the Trace
/// dialog's preview zoomed in, drawn sharp at the screen's pixels
/// (#2951). `stroke_px` as for [`render_traced_scaled`]; `None` draws the
/// wireframe.
pub fn render_traced_view(
    traced: &[Traced],
    size: (u32, u32),
    scale: f32,
    origin: (f32, f32),
    stroke_px: Option<f64>,
) -> RgbaImage {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let how = match stroke_px {
        Some(px) => Paint::Filled(px),
        None => Paint::Wire,
    };
    paint_traced_at(traced, (size.0.max(1), size.1.max(1)), scale, origin, how)
}

fn paint_traced_at(
    traced: &[Traced],
    (pw, ph): (u32, u32),
    scale: f32,
    (ox, oy): (f32, f32),
    how: Paint,
) -> RgbaImage {
    let Some(mut pm) = tiny_skia::Pixmap::new(pw, ph) else {
        return RgbaImage::new(pw, ph);
    };
    let transform = tiny_skia::Transform::from_row(scale, 0.0, 0.0, scale, -ox, -oy);
    for t in traced {
        let mut pb = tiny_skia::PathBuilder::new();
        for el in t.path.elements() {
            match *el {
                tracedraw_core::geometry::PathEl::MoveTo(p) => pb.move_to(p.x as f32, p.y as f32),
                tracedraw_core::geometry::PathEl::LineTo(p) => pb.line_to(p.x as f32, p.y as f32),
                tracedraw_core::geometry::PathEl::QuadTo(a, p) => {
                    pb.quad_to(a.x as f32, a.y as f32, p.x as f32, p.y as f32)
                }
                tracedraw_core::geometry::PathEl::CurveTo(a, b, p) => pb.cubic_to(
                    a.x as f32, a.y as f32, b.x as f32, b.y as f32, p.x as f32, p.y as f32,
                ),
                tracedraw_core::geometry::PathEl::ClosePath => pb.close(),
            }
        }
        let Some(path) = pb.finish() else {
            continue;
        };
        let [r, g, b] = t.color.to_rgb8();
        let mut paint = tiny_skia::Paint::default();
        paint.set_color_rgba8(r, g, b, 255);
        paint.anti_alias = true;
        if let (Some(blend), Paint::Filled(_)) = (&t.blend, &how) {
            let stop = |at: f32, c: [u8; 3]| {
                tiny_skia::GradientStop::new(
                    at,
                    tiny_skia::Color::from_rgba8(c[0], c[1], c[2], 255),
                )
            };
            if let Some(shader) = tiny_skia::LinearGradient::new(
                tiny_skia::Point::from_xy(blend.from.x as f32, blend.from.y as f32),
                tiny_skia::Point::from_xy(blend.to.x as f32, blend.to.y as f32),
                vec![stop(0.0, blend.start), stop(1.0, blend.end)],
                tiny_skia::SpreadMode::Pad,
                tiny_skia::Transform::identity(),
            ) {
                paint.shader = shader;
            }
        }
        match how {
            Paint::Wire => {
                paint.set_color_rgba8(WIRE[0], WIRE[1], WIRE[2], WIRE[3]);
                // About a pixel of the image, whatever the scale (the
                // curves run along pixel edges, so a line of exactly one
                // pixel would half cover two).
                let stroke = tiny_skia::Stroke {
                    width: 1.25 / scale,
                    ..Default::default()
                };
                pm.stroke_path(&path, &paint, &stroke, transform, None);
            }
            Paint::Filled(_) if t.closed => {
                pm.fill_path(&path, &paint, tiny_skia::FillRule::EvenOdd, transform, None);
            }
            Paint::Filled(stroke_px) => {
                let stroke = tiny_skia::Stroke {
                    width: stroke_px.max(0.5) as f32,
                    line_cap: tiny_skia::LineCap::Round,
                    line_join: tiny_skia::LineJoin::Round,
                    ..Default::default()
                };
                pm.stroke_path(&path, &paint, &stroke, transform, None);
            }
        }
    }
    let mut out = RgbaImage::new(pm.width(), pm.height());
    for (px, p) in out.pixels_mut().zip(pm.pixels()) {
        let c = p.demultiply();
        *px = image::Rgba([c.red(), c.green(), c.blue(), c.alpha()]);
    }
    out
}

/// Map traced pixel-space paths into a rectangle (page mm, Y up).
pub fn to_rect(path: &BezPath, img_w: u32, img_h: u32, rect: Rect) -> BezPath {
    let sx = rect.width() / img_w.max(1) as f64;
    let sy = rect.height() / img_h.max(1) as f64;
    tracedraw_core::geometry::Affine::new([sx, 0.0, 0.0, -sy, rect.x0, rect.y1]) * well_formed(path)
}

/// `path` without a close that closes nothing (at its start, or right
/// after another close): measuring such a path (its box, its segments)
/// stops the program, and a trace's curves are measured as they are
/// placed (#4119).
pub fn well_formed(path: &BezPath) -> BezPath {
    use tracedraw_core::geometry::PathEl;
    let els = path.elements();
    let stray = |i: usize| {
        matches!(els[i], PathEl::ClosePath)
            && (i == 0 || matches!(els[i - 1], PathEl::ClosePath))
    };
    if !(0..els.len()).any(stray) {
        return path.clone();
    }
    BezPath::from_vec(
        (0..els.len())
            .filter(|i| !stray(*i))
            .map(|i| els[i])
            .collect(),
    )
}

/// The fill of a traced region drawn as a blend: a linear fountain fill
/// from the blend's first colour to its last, its two nodes where the
/// blend's ends lie on the page (`rect` is the bitmap's rectangle, `b`
/// the region's box there).
pub fn blend_fill(
    blend: &crate::trace_blend::Blend,
    b: &Rect,
    img_w: u32,
    img_h: u32,
    rect: Rect,
) -> tracedraw_core::Fill {
    use tracedraw_core::style::FountainHandles;
    use tracedraw_core::{Fountain, FountainKind};
    let sx = rect.width() / img_w.max(1) as f64;
    let sy = rect.height() / img_h.max(1) as f64;
    let page = |p: Point| Point::new(rect.x0 + sx * p.x, rect.y1 - sy * p.y);
    let (from, to) = (page(blend.from), page(blend.to));
    let d = to - from;
    let color = |c: [u8; 3]| Color::rgb8(c[0], c[1], c[2]);
    let mut f = Fountain::two(
        FountainKind::Linear,
        color(blend.start),
        color(blend.end),
        0.0,
    );
    if d.hypot() > 1e-9 && b.width() > 1e-9 && b.height() > 1e-9 {
        let rel = |p: Point| Fountain::page_to_rel(*b, p);
        f.set_handles(
            *b,
            FountainHandles {
                start: rel(from),
                end: rel(to),
                skew: rel(from + tracedraw_core::geometry::Vec2::new(-d.y, d.x)),
            },
        );
    }
    tracedraw_core::Fill::Fountain(f)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::ImageBuffer;
    use tracedraw_core::geometry::Shape as _;

    /// Flat art saved with noise, as compressed images are (#611): the
    /// yellow disc crossed by a black band, each pixel off by up to 10 in
    /// every channel. Traced with the Clipart preset, the noise makes
    /// neither holes showing the page inside the art nor a crowd of
    /// specks: before noise was taken out, about 3800 pixels inside the
    /// disc showed the page and the trace held over 1400 subpaths.
    #[test]
    fn noisy_flat_art_traces_without_holes_or_specks() {
        use tracedraw_core::geometry::PathEl;
        const N: u32 = 160;
        let mut seed = 12345u32;
        let mut noise = move || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            ((seed >> 16) % 21) as f32 - 10.0
        };
        let mut img = RgbaImage::new(N, N);
        for y in 0..N {
            for x in 0..N {
                let (mut yl, mut bl) = (0.0f32, 0.0f32);
                for sy in 0..4 {
                    for sx in 0..4 {
                        let px = x as f32 + (sx as f32 + 0.5) / 4.0;
                        let py = y as f32 + (sy as f32 + 0.5) / 4.0;
                        let disc = (px - 80.0).powi(2) + (py - 80.0).powi(2) < 60.0f32.powi(2);
                        let band = (py - 0.3 * px - 50.0).abs() < 14.0;
                        if disc && band {
                            bl += 1.0 / 16.0;
                        } else if disc {
                            yl += 1.0 / 16.0;
                        }
                    }
                }
                let wh = 1.0 - yl - bl;
                let n = noise();
                let mix =
                    |a: f32, b: f32, c: f32| (a * yl + b * bl + c * wh + n).clamp(0.0, 255.0) as u8;
                img.put_pixel(
                    x,
                    y,
                    Rgba([
                        mix(250.0, 10.0, 255.0),
                        mix(205.0, 10.0, 255.0),
                        mix(40.0, 10.0, 255.0),
                        255,
                    ]),
                );
            }
        }
        let t = trace(&img, &Preset::Clipart.settings());
        let subpaths: usize = t
            .iter()
            .map(|r| {
                r.path
                    .elements()
                    .iter()
                    .filter(|e| matches!(e, PathEl::MoveTo(_)))
                    .count()
            })
            .sum();
        assert!(subpaths < 1000, "{subpaths} subpaths");
        let out = render_traced(&t, N, N, 1.0);
        let (mut inside, mut holes) = (0, 0);
        for y in 0..N {
            for x in 0..N {
                if (x as f32 - 80.0).hypot(y as f32 - 80.0) >= 56.0 {
                    continue;
                }
                inside += 1;
                let p = out.get_pixel(x, y);
                // Where two colours meet along one curve, each covers part
                // of the pixels it crosses, and the page shows through at
                // most a quarter of one (both half covered): a gap lets
                // more through.
                if p[3] < 180 || (p[0] > 230 && p[1] > 230 && p[2] > 230) {
                    holes += 1;
                }
            }
        }
        assert!(
            holes * 200 < inside,
            "{holes} of {inside} pixels show the page"
        );
    }

    /// Taking noise out replaces a lone odd pixel by its surroundings and
    /// keeps square corners, straight edges and lines one pixel wide along
    /// a row (all but its end pixels); transparent pixels stay as they are.
    #[test]
    fn denoise_removes_lone_pixels_and_keeps_edges() {
        let white = Rgba([255, 255, 255, 255]);
        let black = Rgba([0, 0, 0, 255]);
        let img: RgbaImage = ImageBuffer::from_fn(20, 20, |x, y| {
            let square = (5..12).contains(&x) && (5..12).contains(&y);
            let line = y == 15 && (2..18).contains(&x);
            let lone = x == 16 && y == 4;
            if (x, y) == (0, 0) {
                Rgba([9, 9, 9, 0])
            } else if square || line || lone {
                black
            } else {
                white
            }
        });
        let out = denoise(&img, None).expect("not cancelled");
        assert_eq!(*out.get_pixel(16, 4), white, "the lone pixel goes");
        for (x, y) in [
            (5, 5),
            (11, 5),
            (5, 11),
            (11, 11),
            (8, 8),
            (3, 15),
            (10, 15),
            (16, 15),
        ] {
            assert_eq!(*out.get_pixel(x, y), black, "kept at {x},{y}");
        }
        for (x, y) in [(4, 5), (12, 11), (8, 14), (8, 16)] {
            assert_eq!(*out.get_pixel(x, y), white, "kept at {x},{y}");
        }
        assert_eq!(*out.get_pixel(0, 0), Rgba([9, 9, 9, 0]));
        // Nothing to do on a tiny image.
        let tiny = RgbaImage::new(2, 1);
        assert_eq!(denoise(&tiny, None), Some(tiny));
    }

    fn circle_image() -> RgbaImage {
        ImageBuffer::from_fn(100, 100, |x, y| {
            let d = ((x as f32 - 50.0).powi(2) + (y as f32 - 50.0).powi(2)).sqrt();
            if d < 30.0 {
                Rgba([0, 0, 0, 255])
            } else {
                Rgba([255, 255, 255, 255])
            }
        })
    }

    #[test]
    fn black_and_white_logos_keep_their_colours() {
        // A white disc on black with a black square inside and a soft
        // edge, mostly black pixels: the disc stays white, not grey.
        let img: RgbaImage = ImageBuffer::from_fn(120, 120, |x, y| {
            let d = ((x as f32 - 60.0).powi(2) + (y as f32 - 60.0).powi(2)).sqrt();
            let inner = (45..75).contains(&x) && (45..75).contains(&y);
            let v = if inner {
                0.0
            } else {
                (255.0 * (40.0 - d).clamp(0.0, 1.0)).round()
            };
            let v = v as u8;
            Rgba([v, v, v, 255])
        });
        for colors in [2, 8] {
            let mut s = Preset::Logo.settings();
            s.colors = colors;
            let t = trace(&img, &s);
            // The black around the disc is no paper: it stays (#2928),
            // and the disc is the biggest region after it.
            let mut by_area: Vec<&Traced> = t.iter().collect();
            by_area.sort_by(|a, b| b.path.area().abs().total_cmp(&a.path.area().abs()));
            assert_eq!(by_area[0].color.to_rgb8(), [0, 0, 0], "the surround stays");
            let biggest = by_area.get(1).expect("the disc");
            let [r, g, b] = biggest.color.to_rgb8();
            assert!(
                r > 240 && g > 240 && b > 240,
                "{colors} colours: {r},{g},{b}"
            );
        }
        let pal = median_cut(&img, 2, None).expect("not cancelled");
        assert!(
            pal.contains(&[0, 0, 0]) && pal.iter().any(|c| c[0] > 245),
            "{pal:?}"
        );
    }

    #[test]
    fn outline_traces_a_circle_with_right_area() {
        let img = circle_image();
        let t = trace(&img, &Preset::LineArt.settings());
        assert_eq!(t.len(), 1, "background removed, one region");
        let area = t[0].path.area().abs();
        let expect = std::f64::consts::PI * 30.0 * 30.0;
        assert!(
            (area - expect).abs() / expect < 0.05,
            "area {area} vs {expect}"
        );
        assert_eq!(t[0].color.to_rgb8(), [0, 0, 0]);
    }

    #[test]
    fn centerline_traces_a_line() {
        let img = ImageBuffer::from_fn(100, 40, |x, y| {
            if (18..=22).contains(&y) && x > 5 && x < 95 {
                Rgba([0, 0, 0, 255])
            } else {
                Rgba([255, 255, 255, 255])
            }
        });
        let t = trace(&img, &Preset::Technical.settings());
        assert!(!t.is_empty());
        let b = t[0].path.bounding_box();
        assert!(b.width() > 80.0 && b.height() < 3.0, "{b:?}");
        assert!((last_stroke_width_px() - 5.0).abs() < 1.5);
    }

    /// Smooth outlines from pixels (#611): a circle becomes a few curves,
    /// no stair steps, at the right size; a square keeps four sharp
    /// corners on its pixel corners; stray specks make no curves.
    #[test]
    fn traces_are_smooth_curves_with_sharp_corners_and_no_specks() {
        use tracedraw_core::geometry::PathEl;
        let s = Preset::LineArt.settings();
        let t = trace(&circle_image(), &s);
        assert_eq!(t.len(), 1);
        let els = t[0].path.elements();
        let curves = els
            .iter()
            .filter(|e| matches!(e, PathEl::CurveTo(..)))
            .count();
        assert!(
            els.iter().all(|e| !matches!(e, PathEl::LineTo(..))),
            "no steps"
        );
        assert!((2..=12).contains(&curves), "{curves} curves");
        let area = t[0].path.area().abs();
        let expect = std::f64::consts::PI * 30.0 * 30.0;
        assert!((area - expect).abs() / expect < 0.02, "{area} vs {expect}");
        // No point of the curves strays from the circle by more than the
        // fitting tolerance (1.2 px at detail 40).
        for (p, _) in t[0].path.segments().flat_map(|seg| {
            (0..=8).map(move |k| {
                (
                    tracedraw_core::geometry::ParamCurve::eval(&seg, k as f64 / 8.0),
                    (),
                )
            })
        }) {
            let r = (p - Point::new(50.5, 50.5)).hypot();
            assert!((r - 30.0).abs() < 1.25, "{r}");
        }

        // A square with specks around it.
        let img: RgbaImage = ImageBuffer::from_fn(100, 100, |x, y| {
            let square = (20..80).contains(&x) && (20..80).contains(&y);
            let speck = (x % 13 == 3 && y % 17 == 5) && !square;
            if square || speck {
                Rgba([0, 0, 0, 255])
            } else {
                Rgba([255, 255, 255, 255])
            }
        });
        let t = trace(&img, &s);
        assert_eq!(t.len(), 1, "the specks are gone");
        let path = &t[0].path;
        let subpaths = path
            .elements()
            .iter()
            .filter(|e| matches!(e, PathEl::MoveTo(_)))
            .count();
        assert_eq!(subpaths, 1, "the square alone");
        let b = path.bounding_box();
        assert!(
            (b.x0 - 20.0).abs() < 0.01 && (b.x1 - 80.0).abs() < 0.01,
            "sharp corners on the pixel corners: {b:?}"
        );
        assert!((path.area().abs() - 3600.0).abs() < 1.0, "{}", path.area());
        // With all the detail, a speck of one pixel stays.
        let mut keep = s.clone();
        keep.detail = 100.0;
        assert!(
            trace(&img, &keep)[0]
                .path
                .elements()
                .iter()
                .filter(|e| matches!(e, PathEl::MoveTo(_)))
                .count()
                > 1
        );
    }

    /// Flat art with anti-aliased edges (#611): a yellow disc crossed by a
    /// black band on white, traced with the Clipart preset. Neighbouring
    /// regions meet with no page showing between them (every point well
    /// inside the art is inside a curve, of the colour of the art there),
    /// the in-between shades of the edges make no regions of their own,
    /// and the trace holds about as many regions as the art has colours.
    #[test]
    fn neighbouring_regions_leave_no_gaps_and_edges_make_no_slivers() {
        const N: u32 = 120;
        // Coverage of a pixel by the art, 4 x 4 samples: yellow, black.
        let cover = |x: u32, y: u32| {
            let (mut yellow, mut black) = (0.0f32, 0.0f32);
            for sy in 0..4 {
                for sx in 0..4 {
                    let (px, py) = (
                        x as f32 + (sx as f32 + 0.5) / 4.0,
                        y as f32 + (sy as f32 + 0.5) / 4.0,
                    );
                    let disc = (px - 60.0).powi(2) + (py - 60.0).powi(2) < 42.0f32.powi(2);
                    // A slanted band, so its edges are anti-aliased too.
                    let band = (py - 0.3 * px - 34.0).abs() < 8.0;
                    if disc && band {
                        black += 1.0 / 16.0;
                    } else if disc {
                        yellow += 1.0 / 16.0;
                    }
                }
            }
            (yellow, black)
        };
        let img: RgbaImage = ImageBuffer::from_fn(N, N, |x, y| {
            let (yl, bl) = cover(x, y);
            let wh = 1.0 - yl - bl;
            let mix = |y: f32, b: f32, w: f32| (y * yl + b * bl + w * wh).round() as u8;
            Rgba([
                mix(250.0, 10.0, 255.0),
                mix(205.0, 10.0, 255.0),
                mix(40.0, 10.0, 255.0),
                255,
            ])
        });
        let s = Preset::Clipart.settings();
        let t = trace(&img, &s);
        assert!((2..=3).contains(&t.len()), "{} regions", t.len());
        for y in 0..N {
            for x in 0..N {
                // Pixels well inside the art (it covers every pixel within
                // two of them, so the outline's own fitting at the art's
                // edge is left out): no page, along the edges between the
                // colours too.
                let inside = x > 1
                    && y > 1
                    && x + 2 < N
                    && y + 2 < N
                    && (y - 2..=y + 2).all(|v| {
                        (x - 2..=x + 2).all(|u| {
                            let (a, b) = cover(u, v);
                            a + b >= 1.0
                        })
                    });
                if !inside {
                    continue;
                }
                for k in 0..4 {
                    let p = Point::new(
                        x as f64 + 0.125 + 0.25 * k as f64,
                        y as f64 + 0.375 + 0.125 * k as f64,
                    );
                    let top = top_colour_at(&t, p);
                    assert!(top.is_some(), "page shows at {p:?}");
                    assert!(
                        top.is_some_and(|c| c.iter().any(|v| *v < 230)),
                        "white at {p:?}: {top:?}"
                    );
                }
            }
        }
        let (coincident, apart) = border_distances(&t);
        assert!(
            coincident > 200,
            "the band's edges are shared: {coincident}"
        );
        assert!(
            apart * 20 < coincident,
            "{apart} apart, {coincident} together"
        );
    }

    /// A pale logo close to its white ground keeps its colour, at the
    /// Logo preset and with two colours; a grey gradient keeps the colour
    /// count it was asked for (#611).
    #[test]
    fn close_colours_and_colour_counts_are_kept() {
        let img: RgbaImage = ImageBuffer::from_fn(80, 80, |x, y| {
            if (20..60).contains(&x) && (20..60).contains(&y) {
                Rgba([246, 246, 246, 255])
            } else {
                Rgba([255, 255, 255, 255])
            }
        });
        let mut s = Preset::Logo.settings();
        assert_eq!(trace(&img, &s).len(), 1, "the logo");
        s.colors = 2;
        assert_eq!(trace(&img, &s).len(), 1, "with two colours");
        let ramp: RgbaImage =
            ImageBuffer::from_fn(256, 8, |x, _| Rgba([x as u8, x as u8, x as u8, 255]));
        let mut s = Preset::HighQualityImage.settings();
        s.colors = 96;
        s.remove = Remove::None;
        // Its shades are kept: drawn as the blend they are (#2929), not
        // cut down to a few bands.
        let t = trace(&ramp, &s);
        assert!(
            t.len() > 24 || t.iter().all(|t| t.blend.is_some()),
            "{}",
            t.len()
        );
        let drawn = render_traced(&t, 256, 8, 1.0);
        let far = ramp
            .pixels()
            .zip(drawn.pixels())
            .map(|(a, b)| (a[0] as i32 - b[0] as i32).abs())
            .max()
            .unwrap_or(0);
        assert!(far <= 6, "a shade {far} levels off");
    }

    /// A hairline two pixels wide in a colour of its own, crossing a
    /// white page beside a black block, keeps its colour at the Clipart
    /// preset: no pixel of it is flat, but it is no edge of two colours.
    #[test]
    fn hairlines_of_their_own_colour_keep_it() {
        let img: RgbaImage = ImageBuffer::from_fn(120, 80, |x, y| {
            if (10..50).contains(&x) && (10..70).contains(&y) {
                Rgba([0, 0, 0, 255])
            } else if (60..110).contains(&x) && (39..41).contains(&y) {
                Rgba([220, 30, 40, 255])
            } else {
                Rgba([255, 255, 255, 255])
            }
        });
        let t = trace(&img, &Preset::Clipart.settings());
        let out = render_traced(&t, 120, 80, 1.0);
        for x in 66..104 {
            let p = out.get_pixel(x, 40);
            assert!(
                p[3] == 255 && p[0] > 150 && p[1] < 100,
                "the red line at {x}: {p:?} of {:?}",
                color_counts(&t)
            );
        }
    }

    /// #1619: a detailed logo with thin slanted spokes (dark blue, a
    /// pixel and a half wide, anti-aliased) around a yellow disc, traced
    /// with the Detailed Logo preset. The steps of a slanted hairline
    /// take several shades, its dark core among them; the core's small
    /// patches went to the white around them, so each spoke fell apart
    /// into dashes and dots. Every spoke stays one unbroken line.
    #[test]
    fn thin_slanted_lines_of_a_detailed_logo_stay_lines() {
        const N: u32 = 240;
        const SS: u32 = 4;
        let (cx, cy) = (120.0f64, 120.0f64);
        let angles: Vec<f64> = (0..12)
            .map(|i| i as f64 * std::f64::consts::PI / 6.0 + 0.2)
            .collect();
        let navy = [29.0, 41.0, 108.0];
        let yellow = [241.0, 181.0, 22.0];
        let img: RgbaImage = ImageBuffer::from_fn(N, N, |x, y| {
            let mut acc = [0.0f64; 3];
            for sy in 0..SS {
                for sx in 0..SS {
                    let px = x as f64 + (sx as f64 + 0.5) / SS as f64;
                    let py = y as f64 + (sy as f64 + 0.5) / SS as f64;
                    let (dx, dy) = (px - cx, py - cy);
                    let r = dx.hypot(dy);
                    let c = if r < 30.0 {
                        yellow
                    } else if r < 33.0 {
                        navy
                    } else if r < 105.0
                        && angles.iter().any(|a| {
                            (dx * a.sin() - dy * a.cos()).abs() < 0.75
                                && dx * a.cos() + dy * a.sin() > 0.0
                        })
                    {
                        navy
                    } else {
                        [255.0; 3]
                    };
                    for (a, c) in acc.iter_mut().zip(c) {
                        *a += c;
                    }
                }
            }
            let n = (SS * SS) as f64;
            Rgba([
                (acc[0] / n) as u8,
                (acc[1] / n) as u8,
                (acc[2] / n) as u8,
                255,
            ])
        });
        let t = trace(&img, &Preset::DetailedLogo.settings());
        let out = render_traced(&t, N, N, 1.0);
        for a in &angles {
            // Along the spoke from just outside the ring to near its end:
            // the longest run of page showing through.
            let (mut gap, mut worst) = (0, 0);
            for step in 0..(100 - 40) * 2 {
                let r = 40.0 + step as f64 / 2.0;
                let (x, y) = (cx + r * a.cos(), cy + r * a.sin());
                // The darkest pixel across the line here.
                let dark = (-1i64..=1)
                    .flat_map(|o| [(o, 0), (0, o)])
                    .map(|(ox, oy)| {
                        let p = out.get_pixel(
                            (x.round() as i64 + ox) as u32,
                            (y.round() as i64 + oy) as u32,
                        );
                        if p[3] < 128 {
                            255
                        } else {
                            p[0].max(p[1]).max(p[2]) as i32
                        }
                    })
                    .min()
                    .unwrap_or(255);
                if dark > 200 {
                    gap += 1;
                    worst = worst.max(gap);
                } else {
                    gap = 0;
                }
            }
            assert!(
                worst <= 2,
                "the spoke at {a:.2} rad breaks for {} px: {:?}",
                worst / 2,
                stats(&t)
            );
        }
    }

    /// A line one pixel wide of the colour half way between the two
    /// beside it is their edge's anti-aliasing at every detail, the
    /// highest too (#2929): no shape of its own.
    #[test]
    fn the_blend_of_two_colours_is_no_shape_at_any_detail() {
        let img: RgbaImage = ImageBuffer::from_fn(40, 40, |_, y| match y {
            0..=19 => Rgba([230, 20, 20, 255]),
            20 => Rgba([240, 120, 20, 255]),
            _ => Rgba([250, 220, 20, 255]),
        });
        for detail in [50.0, 100.0] {
            let mut s = Preset::Logo.settings();
            s.detail = detail;
            s.remove = Remove::None;
            let colours: Vec<[u8; 3]> = trace(&img, &s).iter().map(|t| t.color.to_rgb8()).collect();
            assert_eq!(colours.len(), 2, "detail {detail}: {colours:?}");
            assert!(
                !colours.iter().any(|c| c[1] > 90 && c[1] < 160),
                "detail {detail}, no orange band: {colours:?}"
            );
        }
    }

    /// Flat art saved as a compressed image (#611: a bee with a black
    /// band, a short dark bar like a letter's stroke, anti-aliased, with
    /// noise and its colours bled a pixel sideways as compression does),
    /// traced with the automatic trace's Clipart preset of 32 colours. Each colour
    /// stays one region with smooth edges: no notches, specks or slivers
    /// along the edges (before, a shade split by noise made regions of
    /// its own and the edges hundreds of bits), and the bar stays.
    #[test]
    fn compressed_flat_art_traces_as_few_clean_regions() {
        use tracedraw_core::geometry::PathEl;
        const N: u32 = 200;
        let mut seed = 777u32;
        let mut noise = move || {
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            ((seed >> 16) % 13) as f32 - 6.0
        };
        let white = [253.0f32, 253.0, 253.0];
        let yellow = [246.0f32, 205.0, 50.0];
        let black = [0.0f32, 0.0, 0.0];
        let at = |px: f32, py: f32| {
            let disc = (px - 100.0).powi(2) + (py - 100.0).powi(2) < 80.0f32.powi(2);
            let band = (py - 0.2 * px - 110.0).abs() < 14.0;
            let bar = (60.0..100.0).contains(&px) && (py - 60.0).abs() < 1.6;
            if !disc {
                white
            } else if band || bar {
                black
            } else {
                yellow
            }
        };
        let clean: Vec<[f32; 3]> = (0..N * N)
            .map(|i| {
                let (x, y) = ((i % N) as f32, (i / N) as f32);
                let mut c = [0.0f32; 3];
                for sy in 0..4 {
                    for sx in 0..4 {
                        let s = at(x + (sx as f32 + 0.5) / 4.0, y + (sy as f32 + 0.5) / 4.0);
                        for k in 0..3 {
                            c[k] += s[k] / 16.0;
                        }
                    }
                }
                c
            })
            .collect();
        let img: RgbaImage = ImageBuffer::from_fn(N, N, |x, y| {
            let i = (y * N + x) as usize;
            let j = (y * N + x.saturating_sub(1)) as usize;
            // Blue bled from the pixel to the left, plus noise.
            let c = clean[i];
            let b = (c[2] + clean[j][2]) / 2.0;
            let v = |f: f32| f.round().clamp(0.0, 255.0) as u8;
            Rgba([v(c[0] + noise()), v(c[1] + noise()), v(b + noise()), 255])
        });
        let t = trace(&img, &Preset::Clipart.settings());
        let subpaths: usize = t
            .iter()
            .map(|t| {
                t.path
                    .elements()
                    .iter()
                    .filter(|e| matches!(e, PathEl::MoveTo(_)))
                    .count()
            })
            .sum();
        assert!(t.len() <= 3, "{} regions: {:?}", t.len(), color_counts(&t));
        assert!(subpaths <= 8, "{subpaths} subpaths");
        let out = render_traced(&t, N, N, 1.0);
        // The bar is still there, dark.
        for x in 64..96 {
            let p = out.get_pixel(x, 60);
            assert!(p[3] == 255 && p[0] < 90, "bar at {x}: {p:?}");
        }
    }

    /// The in-between pixels along the edge of two flat colours go to the
    /// one their colour is nearer; a hairline on one colour, or of a
    /// colour that is not between them, stays.
    #[test]
    fn edge_pixels_go_to_the_flat_colours_beside_them() {
        assert!(between([130, 108, 25], [0, 0, 0], [250, 205, 40]));
        assert!(!between([250, 205, 40], [0, 0, 0], [255, 255, 255]));
        // A pale blue line between black and white is a colour of its own.
        assert!(!between([170, 200, 230], [0, 0, 0], [255, 255, 255]));
        assert!(between([128, 128, 128], [0, 0, 0], [255, 255, 255]));
        assert!(nearer_first([60, 50, 10], [0, 0, 0], [250, 205, 40]));
        assert!(!nearer_first([190, 160, 30], [0, 0, 0], [250, 205, 40]));
        let palette = [[0, 0, 0], [250, 205, 40], [125, 102, 20]];
        let (w, h) = (12, 16);
        // Black above, yellow below, a band of 2 px between: darker
        // olive on the black side, lighter on the yellow side.
        let mut labels: Vec<usize> = (0..w * h)
            .map(|i| match i / w {
                0..=6 => 0,
                7 | 8 => 2,
                _ => 1,
            })
            .collect();
        let colors: Vec<[u8; 3]> = (0..w * h)
            .map(|i| match i / w {
                7 => [70, 57, 11],
                8 => [180, 148, 29],
                r => palette[labels[r * w]],
            })
            .collect();
        settle_edges(&mut labels, w, h, &palette, &colors, None);
        assert!(labels[7 * w..8 * w].iter().all(|l| *l == 0), "{labels:?}");
        assert!(labels[8 * w..9 * w].iter().all(|l| *l == 1), "{labels:?}");
        // An olive hairline on yellow alone stays.
        let mut labels: Vec<usize> = (0..w * h).map(|i| if i / w == 8 { 2 } else { 1 }).collect();
        let colors: Vec<[u8; 3]> = labels.iter().map(|l| palette[*l]).collect();
        settle_edges(&mut labels, w, h, &palette, &colors, None);
        assert_eq!(labels.iter().filter(|l| **l == 2).count(), w);
    }

    /// Patches smaller than the detail allows take the colour around them.
    #[test]
    fn despeckle_fills_small_patches_with_their_surroundings() {
        let (w, h) = (6, 5);
        let mut labels = vec![0usize; w * h];
        labels[2 * w + 2] = 1; // a one-pixel speck
        for x in 0..w {
            labels[4 * w + x] = 2; // a row of six
        }
        labels[0] = usize::MAX; // transparent stays
        despeckle(
            &mut labels,
            w,
            h,
            3,
            2,
            &[[0, 0, 0], [90, 90, 90], [255, 255, 255]],
            None,
        );
        assert_eq!(labels[2 * w + 2], 0);
        assert!(labels[4 * w..].iter().all(|l| *l == 2));
        assert_eq!(labels[0], usize::MAX);
        assert_eq!(speck_area(100.0), 1);
        assert!(speck_area(35.0) > speck_area(80.0));
    }

    #[test]
    fn contours_of_a_square_with_hole() {
        let w = 10;
        let mut mask = vec![false; 100];
        for y in 2..8 {
            for x in 2..8 {
                mask[y * w + x] = true;
            }
        }
        for y in 4..6 {
            for x in 4..6 {
                mask[y * w + x] = false;
            }
        }
        let s = Preset::Logo.settings();
        let zero = tracedraw_core::geometry::Vec2::new(0.0, 0.0);
        let Some(path) = rings_path(&mask, w, 10, zero, 0.5, &s, None) else {
            panic!("not cancelled");
        };
        let rings = path
            .elements()
            .iter()
            .filter(|e| matches!(e, tracedraw_core::geometry::PathEl::MoveTo(_)))
            .count();
        assert_eq!(rings, 2);
    }

    /// A disc on white with white patches inside it (the whites of eyes):
    /// Remove Background takes out the white reaching the edge, and the
    /// white patches inside the disc are holes in it, taken out with the
    /// background (#4328), Entire color takes every
    /// white out, None traces the white too; a colour given is removed
    /// instead of the one found; a colour given that is not the picture's
    /// takes the palette colour nearest it.
    #[test]
    fn removing_the_background_or_the_entire_colour_or_nothing() {
        let img: RgbaImage = ImageBuffer::from_fn(120, 120, |x, y| {
            let d = ((x as f32 - 60.0).powi(2) + (y as f32 - 60.0).powi(2)).sqrt();
            let eye = ((x as f32 - 45.0).powi(2) + (y as f32 - 50.0).powi(2)).sqrt() < 6.0
                || ((x as f32 - 75.0).powi(2) + (y as f32 - 50.0).powi(2)).sqrt() < 6.0;
            if d < 45.0 && !eye {
                Rgba([0, 0, 0, 255])
            } else {
                Rgba([255, 255, 255, 255])
            }
        });
        let mut s = Preset::Logo.settings();
        s.remove = Remove::Background;
        let t = trace(&img, &s);
        let colours: Vec<[u8; 3]> = t.iter().map(|t| t.color.to_rgb8()).collect();
        assert_eq!(colours, vec![[0, 0, 0]], "the eyes are holes");
        let rings = t[0]
            .path
            .elements()
            .iter()
            .filter(|e| matches!(e, tracedraw_core::geometry::PathEl::MoveTo(_)))
            .count();
        assert_eq!(rings, 3, "the disc and its two eyes");
        s.remove = Remove::EntireColor;
        let t = trace(&img, &s);
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].color.to_rgb8(), [0, 0, 0]);
        s.remove = Remove::None;
        let t = trace(&img, &s);
        assert_eq!(t.len(), 2);
        // The black given as the background: the disc goes, the white stays
        // whole (it reaches the edge, and is not the background now).
        s.remove = Remove::EntireColor;
        s.background = Some([0, 0, 0]);
        let t = trace(&img, &s);
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].color.to_rgb8(), [255, 255, 255]);
        // A colour picked a little off (the anti-aliasing) still means
        // the nearest palette colour.
        s.background = Some([20, 20, 20]);
        assert_eq!(trace(&img, &s)[0].color.to_rgb8(), [255, 255, 255]);
        // Removing the background with a colour whose patches do not reach
        // the edge takes nothing out.
        s.remove = Remove::Background;
        s.background = Some([0, 0, 0]);
        assert_eq!(trace(&img, &s).len(), 2);
    }

    /// Merge adjacent off: each separate patch of a colour is a curve of
    /// its own, with its holes; on, one curve per colour.
    #[test]
    fn merge_adjacent_off_makes_a_curve_per_patch() {
        let img: RgbaImage = ImageBuffer::from_fn(100, 100, |x, y| {
            let a = (10..40).contains(&x) && (10..40).contains(&y);
            let hole = (20..30).contains(&x) && (20..30).contains(&y);
            let b = (60..90).contains(&x) && (60..90).contains(&y);
            if (a && !hole) || b {
                Rgba([200, 0, 0, 255])
            } else {
                Rgba([255, 255, 255, 255])
            }
        });
        let mut s = Preset::Logo.settings();
        // The hole's white does not reach the edge: Entire color, so the
        // red is all there is.
        s.remove = Remove::EntireColor;
        assert_eq!(trace(&img, &s).len(), 1);
        s.merge_adjacent = false;
        let t = trace(&img, &s);
        assert_eq!(t.len(), 2);
        let rings = |p: &BezPath| {
            p.elements()
                .iter()
                .filter(|e| matches!(e, tracedraw_core::geometry::PathEl::MoveTo(_)))
                .count()
        };
        let mut counts: Vec<usize> = t.iter().map(|t| rings(&t.path)).collect();
        counts.sort();
        assert_eq!(counts, vec![1, 2], "the square with the hole has two rings");
        // The patches of a mask, each in its own box.
        let mask: Vec<bool> = (0..6 * 4)
            .map(|i| matches!(i % 6, 0 | 1) && i / 6 < 2 || i == 6 * 3 + 5)
            .collect();
        let p = patches(&mask, 6, 4);
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].0, (0, 0, 1, 1));
        assert_eq!(p[0].1, vec![true; 4]);
        assert_eq!(p[1].0, (5, 3, 5, 3));
    }

    /// Remove overlap keeps each region to its own pixels: the black ring
    /// no longer reaches under the yellow inside it, as one disc, but
    /// keeps its hole.
    #[test]
    fn remove_overlap_keeps_regions_to_their_pixels() {
        let img: RgbaImage = ImageBuffer::from_fn(80, 80, |x, y| {
            let d = ((x as f32 - 40.0).powi(2) + (y as f32 - 40.0).powi(2)).sqrt();
            if d < 18.0 {
                Rgba([250, 200, 40, 255])
            } else if d < 30.0 {
                Rgba([0, 0, 0, 255])
            } else {
                Rgba([255, 255, 255, 255])
            }
        });
        let mut s = Preset::Logo.settings();
        s.smoothing = 0.0;
        let area = |t: &[Traced]| -> f64 {
            t.iter()
                .find(|t| t.color.to_rgb8()[0] < 50)
                .map(|t| t.path.area().abs())
                .unwrap_or(0.0)
        };
        let under = area(&trace(&img, &s));
        s.remove_overlap = true;
        let own = area(&trace(&img, &s));
        assert!(under > own + 20.0, "under {under}, own {own}");
    }

    /// The Colors tab: colours merged into one trace as one; colours
    /// edited are drawn in the new colour; grey mode traces greys.
    #[test]
    fn merged_and_edited_colours_and_grey_mode() {
        let img: RgbaImage = ImageBuffer::from_fn(90, 30, |x, _| match x / 30 {
            0 => Rgba([200, 0, 0, 255]),
            1 => Rgba([0, 0, 200, 255]),
            _ => Rgba([255, 255, 255, 255]),
        });
        let mut s = Preset::Logo.settings();
        s.remove = Remove::None;
        let t = trace(&img, &s);
        assert_eq!(t.len(), 3);
        s.merged = vec![vec![[200, 0, 0], [0, 0, 200]]];
        let t = trace(&img, &s);
        assert_eq!(t.len(), 2, "red and blue are one");
        assert!(t.iter().any(|t| t.color.to_rgb8() == [200, 0, 0]));
        // A colour that is not in the palette merges nothing.
        s.merged = vec![vec![[0, 200, 0], [0, 0, 200]]];
        assert_eq!(trace(&img, &s).len(), 3);
        s.merged.clear();
        s.recolored = vec![([0, 0, 200], [0, 120, 0])];
        let t = trace(&img, &s);
        assert!(t.iter().any(|t| t.color.to_rgb8() == [0, 120, 0]));
        assert!(!t.iter().any(|t| t.color.to_rgb8() == [0, 0, 200]));
        s.recolored.clear();
        s.mode = ColorMode::Grayscale;
        let t = trace(&img, &s);
        assert!(t.iter().all(|t| {
            let c = t.color.to_rgb8();
            c[0] == c[1] && c[1] == c[2]
        }));
        assert_eq!(stats(&t).colors, 3);
        assert_eq!(color_counts(&t).len(), 3);
    }

    /// A trace stops once cancelled, in both modes.
    #[test]
    fn a_cancelled_trace_gives_nothing() {
        let img: RgbaImage = ImageBuffer::from_fn(60, 60, |x, y| {
            if (x / 10 + y / 10) % 2 == 0 {
                Rgba([0, 0, 0, 255])
            } else {
                Rgba([255, 255, 255, 255])
            }
        });
        let cancel = Cancel::new(true);
        assert!(trace_unless(&img, &Preset::Logo.settings(), Some(&cancel)).is_none());
        assert!(trace_unless(&img, &Preset::Technical.settings(), Some(&cancel)).is_none());
        let go = Cancel::new(false);
        assert!(trace_unless(&img, &Preset::Logo.settings(), Some(&go)).is_some());
    }

    /// Every long stage of the outline trace looks at the flag: noise
    /// removal, colour reduction, despeckling and the fitting of each
    /// patch's rings.
    #[test]
    fn every_stage_stops_at_the_flag() {
        let raised = Cancel::new(true);
        let img: RgbaImage = ImageBuffer::from_fn(40, 40, |x, y| {
            Rgba([(x * 6) as u8, (y * 6) as u8, ((x + y) * 3) as u8, 255])
        });
        assert!(denoise(&img, Some(&raised)).is_none());
        assert!(denoise(&img, None).is_some());
        assert!(median_cut(&img, 8, Some(&raised)).is_none());
        assert!(median_cut(&img, 8, None).is_some_and(|p| !p.is_empty()));
        let (w, h) = (6, 5);
        let mut labels = vec![0usize; w * h];
        labels[2 * w + 2] = 1;
        despeckle(&mut labels, w, h, 3, 2, &[[0; 3], [9; 3]], Some(&raised));
        assert_eq!(labels[2 * w + 2], 1, "stopped before any change");
        let mask: Vec<bool> = (0..w * h).map(|i| i % w > 1).collect();
        let s = Preset::Logo.settings();
        let zero = tracedraw_core::geometry::Vec2::new(0.0, 0.0);
        assert!(rings_path(&mask, w, h, zero, 0.5, &s, Some(&raised)).is_none());
        assert!(rings_path(&mask, w, h, zero, 0.5, &s, None).is_some());
        let mut labels = vec![0usize; w * h];
        labels[2 * w + 2] = 1;
        labels[2 * w + 3] = 1;
        assert!(smooth_labels(&mut labels, w, h, Some(&raised)).is_none());
        assert!(dissolve_fringes(
            &mut labels,
            w,
            h,
            &[[0; 3], [9; 3]],
            &[[0; 3]; 30],
            Some(&raised)
        )
        .is_none());
    }

    /// A trace of a large noisy picture whose flag is raised while it
    /// runs ends soon after, wherever it was.
    #[test]
    fn a_trace_cancelled_while_it_runs_ends_soon() {
        let img: RgbaImage = ImageBuffer::from_fn(1200, 900, |x, y| {
            let n = (x.wrapping_mul(7919) ^ y.wrapping_mul(104_729)) % 23;
            let base = if (x / 60 + y / 60) % 2 == 0 { 40 } else { 200 };
            let v = (base + n) as u8;
            Rgba([v, v / 2, 255 - v, 255])
        });
        let mut s = Preset::Logo.settings();
        s.detail = 40.0;
        s.colors = 64;
        for wait_ms in [5u64, 60, 250] {
            let cancel = std::sync::Arc::new(Cancel::new(false));
            let flag = cancel.clone();
            let raise = std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(wait_ms));
                flag.store(true, std::sync::atomic::Ordering::Relaxed);
                std::time::Instant::now()
            });
            let out = trace_unless(&img, &s, Some(&cancel));
            let done = std::time::Instant::now();
            let raised_at = raise.join().expect("the thread");
            if out.is_some() {
                // Done before the flag went up (a fast build).
                assert!(
                    wait_ms > 5 && done <= raised_at,
                    "cancelled after {wait_ms} ms"
                );
                continue;
            }
            let late = done.saturating_duration_since(raised_at);
            assert!(
                late < std::time::Duration::from_millis(1500),
                "ended {late:?} after the flag (raised after {wait_ms} ms)"
            );
        }
    }

    /// The preview pictures: a trace drawn at another scale fills the
    /// scaled picture, the wireframe draws lines of its colour only
    /// along the curves; the counts of a trace.
    #[test]
    fn scaled_renders_and_wireframes_and_stats() {
        let img: RgbaImage = ImageBuffer::from_fn(40, 40, |x, y| {
            if (10..30).contains(&x) && (10..30).contains(&y) {
                Rgba([0, 0, 0, 255])
            } else {
                Rgba([255, 255, 255, 255])
            }
        });
        let t = trace(&img, &Preset::Logo.settings());
        let st = stats(&t);
        assert_eq!((st.curves, st.colors), (1, 1));
        assert!(st.nodes >= 4, "{st:?}");
        let big = render_traced_scaled(&t, 40, 40, 3.0, 1.0);
        assert_eq!(big.dimensions(), (120, 120));
        assert_eq!(big.get_pixel(60, 60).0, [0, 0, 0, 255]);
        assert_eq!(big.get_pixel(5, 5)[3], 0, "outside is clear");
        let wire = render_wireframe(&t, 40, 40, 2.0);
        assert_eq!(wire.dimensions(), (80, 80));
        assert_eq!(wire.get_pixel(40, 40)[3], 0, "the inside is clear");
        let on_edge = (0..80).any(|x| wire.get_pixel(x, 20)[3] > 0);
        assert!(on_edge, "the line along the top edge");
        // The wireframe colour (the edges' half covered pixels within
        // rounding).
        let coloured = wire
            .pixels()
            .filter(|p| p[3] >= 128)
            .all(|p| (0..3).all(|k| (p[k] as i32 - WIRE[k] as i32).abs() <= 2));
        assert!(coloured);
        assert!(wire.pixels().any(|p| p[3] >= 128));
    }
    /// #1250: art on a transparent background (a clip art file) has no
    /// colour on the picture's edge. The commonest of none was the last
    /// colour of the palette, which Remove entire color took out of the
    /// art: the yellow of the bee's head and stripes, leaving their
    /// fringes as outlines. Nothing is removed now.
    #[test]
    fn art_on_a_transparent_background_keeps_every_colour() {
        let img: RgbaImage = ImageBuffer::from_fn(100, 100, |x, y| {
            let d = ((x as f32 - 50.0).powi(2) + (y as f32 - 50.0).powi(2)).sqrt();
            if d >= 40.0 {
                Rgba([0, 0, 0, 0])
            } else if (y as f32 - 50.0).abs() < 8.0 {
                Rgba([0, 0, 0, 255])
            } else {
                Rgba([246, 205, 50, 255])
            }
        });
        for remove in Remove::ALL {
            let mut s = Preset::Clipart.settings();
            s.remove = remove;
            let t = trace(&img, &s);
            let colours: Vec<[u8; 3]> = t.iter().map(|t| t.color.to_rgb8()).collect();
            assert_eq!(colours.len(), 2, "{remove:?}: {colours:?}");
            let out = render_traced(&t, 100, 100, 1.0);
            let p = out.get_pixel(50, 25);
            assert!(
                p[3] == 255 && p[0] > 200 && p[1] > 160,
                "{remove:?}: head {p:?}"
            );
        }
    }

    /// #1930: a floral pattern on a transparent background whose stems
    /// run off the bottom of the picture. Their black was the only colour
    /// on the edge, so Remove background (automatically) took it for the
    /// background and erased every stem that touched the edge, leaving
    /// the dots and leaves inside. The edge is mostly transparent, so
    /// nothing is removed now.
    #[test]
    fn stems_running_off_a_transparent_edge_are_kept() {
        let img: RgbaImage = ImageBuffer::from_fn(120, 160, |x, y| {
            let (x, y) = (x as f32, y as f32);
            // A stem from the bottom edge curving up, and a dot inside.
            let stem_x = 50.0 + 25.0 * ((160.0 - y) / 70.0).sin();
            let stem = y > 30.0 && (x - stem_x).abs() < 6.0;
            let dot = (x - 95.0).powi(2) + (y - 40.0).powi(2) < 64.0;
            if stem || dot {
                Rgba([0, 0, 0, 255])
            } else {
                Rgba([0, 0, 0, 0])
            }
        });
        let mut s = Preset::LineArt.settings();
        s.detail = 100.0;
        s.smoothing = 100.0;
        s.corner_smoothness = 100.0;
        s.remove = Remove::Background;
        s.background = None;
        let t = trace(&img, &s);
        let out = render_traced(&t, 120, 160, 1.0);
        // The stem near the bottom edge and halfway up, and the dot.
        for (x, y) in [
            (50u32, 157u32),
            (50 + (25.0f32 * (80.0f32 / 70.0).sin()) as u32, 80),
            (95, 40),
        ] {
            assert_eq!(out.get_pixel(x, y)[3], 255, "lost at {x},{y}");
        }
    }

    /// #1250: flat art from a video frame or a scaled screenshot, where
    /// the edge of black and yellow is a soft band four pixels wide of
    /// in-between colours, with noise. The band goes to the two colours:
    /// one ring each and few nodes, no slivers or bumps of in-between
    /// colours along the edge (before: a dozen regions along it).
    #[test]
    fn blurred_edges_go_to_the_colours_either_side() {
        use tracedraw_core::geometry::PathEl;
        const N: u32 = 160;
        let mut seed = 99u32;
        let mut noise = move || {
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            ((seed >> 16) % 11) as f32 - 5.0
        };
        let black = [2.0f32, 1.0, 2.0];
        let yellow = [247.0f32, 205.0, 52.0];
        let img: RgbaImage = ImageBuffer::from_fn(N, N, |x, y| {
            let edge = 80.0 + 12.0 * (y as f32 / 25.0).sin();
            let t = ((x as f32 - edge) / 4.0 + 0.5).clamp(0.0, 1.0);
            let mut v = |k: usize| {
                (black[k] + (yellow[k] - black[k]) * t + noise())
                    .round()
                    .clamp(0.0, 255.0) as u8
            };
            Rgba([v(0), v(1), v(2), 255])
        });
        let mut s = Preset::Clipart.settings();
        s.remove = Remove::None;
        let t = trace(&img, &s);
        let rings = |t: &Traced| {
            t.path
                .elements()
                .iter()
                .filter(|e| matches!(e, PathEl::MoveTo(_)))
                .count()
        };
        assert_eq!(
            t.len(),
            2,
            "{:?}",
            t.iter().map(|t| t.color.to_rgb8()).collect::<Vec<_>>()
        );
        assert!(t.iter().all(|t| rings(t) == 1));
        let st = stats(&t);
        assert!(st.nodes <= 40, "{} nodes", st.nodes);
    }

    /// One-pixel bumps and notches along an edge go; a line one pixel
    /// wide and its ends stay.
    #[test]
    fn smoothing_takes_bumps_and_notches_off_edges_and_keeps_lines() {
        let (w, h) = (12, 10);
        let mut labels: Vec<usize> = (0..w * h).map(|i| if i / w < 5 { 0 } else { 1 }).collect();
        labels[5 * w + 4] = 0; // a bump of the top colour
        labels[4 * w + 8] = 1; // a notch in it
        for x in 1..w - 1 {
            labels[7 * w + x] = 2; // a line
        }
        smooth_labels(&mut labels, w, h, None).unwrap();
        assert_eq!(labels[5 * w + 4], 1, "bump");
        assert_eq!(labels[4 * w + 8], 0, "notch");
        assert!((1..w - 1).all(|x| labels[7 * w + x] == 2), "line");
        assert!(labels[..5 * w].iter().all(|l| *l == 0));
    }

    /// A band of an in-between colour along the edge of black and yellow
    /// goes to them, each pixel to the nearer; a thin stroke of that colour
    /// on yellow alone stays.
    #[test]
    fn fringes_go_to_the_two_sides_and_strokes_stay() {
        let palette = [[0, 0, 0], [250, 205, 40], [125, 102, 20]];
        let (w, h) = (20, 14);
        // Black above, yellow below, a band 3 px thick between.
        let mut labels: Vec<usize> = (0..w * h)
            .map(|i| match i / w {
                0..=5 => 0,
                6..=8 => 2,
                _ => 1,
            })
            .collect();
        let colors: Vec<[u8; 3]> = (0..w * h)
            .map(|i| match i / w {
                6 => [60, 50, 10],
                7 => [110, 90, 18],
                8 => [200, 165, 32],
                r => palette[labels[r * w]],
            })
            .collect();
        dissolve_fringes(&mut labels, w, h, &palette, &colors, None).unwrap();
        assert!(labels[6 * w..8 * w].iter().all(|l| *l == 0), "{labels:?}");
        assert!(labels[8 * w..9 * w].iter().all(|l| *l == 1), "{labels:?}");
        // A stroke two pixels wide of the in-between colour on yellow.
        let mut labels: Vec<usize> = (0..w * h)
            .map(|i| if (6..8).contains(&(i / w)) { 2 } else { 1 })
            .collect();
        let colors: Vec<[u8; 3]> = labels.iter().map(|l| palette[*l]).collect();
        dissolve_fringes(&mut labels, w, h, &palette, &colors, None).unwrap();
        assert_eq!(labels.iter().filter(|l| **l == 2).count(), 2 * w);
    }

    /// A deterministic noise of up to `amp` either way, per call.
    fn noise_source(amp: i32) -> impl FnMut() -> i32 {
        let mut seed = 12345u32;
        move || {
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            ((seed >> 16) % (2 * amp as u32 + 1)) as i32 - amp
        }
    }

    /// A sunflower's heart as halftone art: a black ring round a teal disc
    /// printed with small yellow-green dots, on a transparent background,
    /// with a little noise.
    fn halftone_image() -> (RgbaImage, Vec<(u32, u32)>) {
        let mut noise = noise_source(4);
        let (n, c) = (220u32, 110.0f32);
        let mut dots = Vec::new();
        let img = ImageBuffer::from_fn(n, n, |x, y| {
            let d = ((x as f32 - c).powi(2) + (y as f32 - c).powi(2)).sqrt();
            let base: [i32; 3] = if d >= 100.0 {
                return Rgba([0, 0, 0, 0]);
            } else if d >= 90.0 {
                [0, 0, 0]
            } else if x % 6 < 3 && y % 6 < 3 && d < 84.0 {
                [150, 170, 90]
            } else {
                [35, 60, 62]
            };
            let mut v = |k: usize| (base[k] + noise()).clamp(0, 255) as u8;
            Rgba([v(0), v(1), v(2), 255])
        });
        for y in (1..n).step_by(6) {
            for x in (1..n).step_by(6) {
                let d = ((x as f32 - c).powi(2) + (y as f32 - c).powi(2)).sqrt();
                if d < 80.0 {
                    dots.push((x, y));
                }
            }
        }
        (img, dots)
    }

    /// The dots of a halftone are art, not noise: at the Clipart preset
    /// they stay (every dot of three pixels standing out from its teal);
    /// before, every patch under 16 pixels went to the colour around it
    /// and the heart came out plain teal. The black ring, under all the
    /// rest, is one outline with its hole filled by the regions drawn
    /// over it, and the whole is drawn back nearly exactly.
    #[test]
    fn halftone_dots_stay_and_the_regions_below_are_whole() {
        use tracedraw_core::geometry::PathEl;
        let (img, dots) = halftone_image();
        let t = trace(&img, &Preset::Clipart.settings());
        let out = render_traced(&t, img.width(), img.height(), 1.0);
        let kept = dots
            .iter()
            .filter(|(x, y)| {
                let p = out.get_pixel(*x, *y);
                distance2([p[0], p[1], p[2]], [150, 170, 90])
                    < distance2([p[0], p[1], p[2]], [35, 60, 62])
            })
            .count();
        assert!(kept * 10 >= dots.len() * 9, "{kept} of {} dots", dots.len());
        assert!(t.len() <= 4, "{:?}", color_counts(&t));
        let black = t
            .iter()
            .find(|t| t.color.to_rgb8().iter().all(|c| *c < 16))
            .expect("the black ring");
        let rings = black
            .path
            .elements()
            .iter()
            .filter(|e| matches!(e, PathEl::MoveTo(_)))
            .count();
        assert_eq!(rings, 1, "the ring's hole is filled");
        let wrong = wrong_share(&img, &out, None).unwrap();
        assert!(wrong < 3.0, "{wrong}% drawn wrong");
    }

    /// Shades the colour reduction makes of one colour's noise and edges
    /// join that colour, which keeps its exact value: a black square with
    /// a dark brown fringe and grey-flecked white traces as #000000 and
    /// white, two colours, where each shade was a curve of its own and the
    /// black came out a brown mean.
    #[test]
    fn shades_join_their_colour_which_keeps_its_value() {
        let mut noise = noise_source(6);
        let img: RgbaImage = ImageBuffer::from_fn(120, 120, |x, y| {
            let inside = (30..90).contains(&x) && (30..90).contains(&y);
            let fringe = (28..92).contains(&x) && (28..92).contains(&y) && !inside;
            if inside {
                Rgba([0, 0, 0, 255])
            } else if fringe {
                Rgba([24, 16, 6, 255])
            } else {
                let v = (250 + noise().abs().min(5)) as u8;
                Rgba([v, v, v, 255])
            }
        });
        let mut s = Preset::Clipart.settings();
        s.remove = Remove::None;
        let t = trace(&img, &s);
        let colors: Vec<[u8; 3]> = color_counts(&t).into_iter().map(|(c, _)| c).collect();
        assert!(colors.contains(&[0, 0, 0]), "{colors:?}");
        assert!(colors.len() <= 2, "{colors:?}");
    }

    /// A small logo, anti-aliased: a red disc with a white bar and a blue
    /// triangle on white.
    fn logo_image() -> RgbaImage {
        let at = |x: f32, y: f32| -> [f32; 3] {
            let d = ((x - 150.0).powi(2) + (y - 150.0).powi(2)).sqrt();
            let tri = y > 40.0 && y < 125.0 && (x - 150.0).abs() < (y - 40.0) * 0.6;
            if tri {
                [20.0, 40.0, 120.0]
            } else if (70.0..230.0).contains(&x) && (130.0..190.0).contains(&y) {
                [255.0, 255.0, 255.0]
            } else if d < 120.0 {
                [220.0, 30.0, 40.0]
            } else {
                [255.0, 255.0, 255.0]
            }
        };
        ImageBuffer::from_fn(300, 300, |x, y| {
            let mut c = [0.0f32; 3];
            for sy in 0..4 {
                for sx in 0..4 {
                    let s = at(
                        x as f32 + (sx as f32 + 0.5) / 4.0,
                        y as f32 + (sy as f32 + 0.5) / 4.0,
                    );
                    for k in 0..3 {
                        c[k] += s[k] / 16.0;
                    }
                }
            }
            Rgba([c[0] as u8, c[1] as u8, c[2] as u8, 255])
        })
    }

    /// The Optimized preset picks a detail of its own for the picture and
    /// traces it as it is in few curves and nodes; the settings it finds
    /// trace as they are.
    #[test]
    fn the_optimized_preset_draws_the_picture_as_it_is_in_few_nodes() {
        let img = logo_image();
        let s = Preset::Optimized.settings();
        assert!(s.optimize && !s.centerline);
        let found = optimize(&img, &s, None).expect("settings");
        assert!(!found.optimize);
        assert!(OPTIMIZE_DETAILS.contains(&found.detail), "{}", found.detail);
        let t = trace(&img, &s);
        let st = stats(&t);
        let out = render_traced(&t, 300, 300, 1.0);
        let wrong = wrong_share(&img, &out, None).unwrap();
        assert!(wrong < 1.5, "{wrong}% drawn wrong");
        assert!(st.curves <= 4, "{:?}", color_counts(&t));
        assert!(st.nodes < 200, "{} nodes", st.nodes);
        // A cancelled search gives nothing.
        let raised = Cancel::new(true);
        assert!(optimize(&img, &s, Some(&raised)).is_none());
    }

    /// A picture drawn a pixel off along its edges is not drawn wrong; a
    /// lost dot is.
    #[test]
    fn wrong_share_forgives_edges_a_pixel_off_and_counts_lost_dots() {
        let disc = circle_image();
        assert_eq!(wrong_share(&disc, &disc, None), Some(0.0));
        let moved = ImageBuffer::from_fn(100, 100, |x, y| *disc.get_pixel(x.saturating_sub(1), y));
        assert_eq!(wrong_share(&disc, &moved, None), Some(0.0));
        let mut dotted = disc.clone();
        for y in 80..84 {
            for x in 80..84 {
                dotted.put_pixel(x, y, Rgba([0, 0, 0, 255]));
            }
        }
        assert!(wrong_share(&disc, &dotted, None).unwrap() > 0.0);
    }

    /// Black and white traces are #000000 and #FFFFFF, whatever the ink
    /// and paper of the scan: the mode's two colours are not moved onto
    /// the picture's commonest shades.
    #[test]
    fn black_and_white_stays_pure() {
        let img: RgbaImage = ImageBuffer::from_fn(100, 100, |x, y| {
            if (30..70).contains(&x) && (30..70).contains(&y) {
                Rgba([6, 6, 8, 255])
            } else {
                Rgba([250, 250, 250, 255])
            }
        });
        let mut s = Preset::LineArt.settings();
        s.remove = Remove::None;
        let colors: Vec<[u8; 3]> = color_counts(&trace(&img, &s))
            .into_iter()
            .map(|(c, _)| c)
            .collect();
        assert!(
            colors
                .iter()
                .all(|c| *c == [0, 0, 0] || *c == [255, 255, 255]),
            "{colors:?}"
        );
    }

    /// A background colour specified (or picked with the eyedropper) is
    /// matched among the colours of the trace: on noisy paper whose shades
    /// joined into one, white specified still takes the paper out. Before,
    /// a joined shade left in the palette stood nearer white than the
    /// paper's colour, matched no pixel, and the page was traced.
    #[test]
    fn a_specified_background_is_removed_after_shades_join() {
        let mut noise = noise_source(4);
        let img: RgbaImage = ImageBuffer::from_fn(120, 120, |x, y| {
            if (40..80).contains(&x) && (40..80).contains(&y) {
                Rgba([0, 0, 0, 255])
            } else {
                let v = (250 + noise()).clamp(0, 255) as u8;
                Rgba([v, v, v, 255])
            }
        });
        let mut s = Preset::Clipart.settings();
        s.remove = Remove::Background;
        s.background = Some([255, 255, 255]);
        let t = trace(&img, &s);
        let colors: Vec<[u8; 3]> = color_counts(&t).into_iter().map(|(c, _)| c).collect();
        assert_eq!(colors, vec![[0, 0, 0]], "the paper is gone");
    }

    /// Never more colours than asked for: flat art with hairlines of
    /// colours of their own, traced in 8 colours, keeps 8 at most.
    #[test]
    fn the_palette_keeps_to_the_number_of_colours() {
        let lines: [[u8; 3]; 6] = [
            [220, 30, 40],
            [30, 160, 60],
            [40, 60, 220],
            [230, 200, 20],
            [160, 40, 200],
            [20, 200, 210],
        ];
        let img: RgbaImage = ImageBuffer::from_fn(200, 200, |x, y| {
            let band = (y / 30) as usize;
            if y % 30 < 2 && band < lines.len() {
                let c = lines[band];
                Rgba([c[0], c[1], c[2], 255])
            } else if x < 100 {
                Rgba([0, 0, 0, 255])
            } else if x < 150 {
                Rgba([128, 128, 128, 255])
            } else {
                Rgba([255, 255, 255, 255])
            }
        });
        let mut s = Preset::Clipart.settings();
        s.colors = 8;
        s.remove = Remove::None;
        let n = color_counts(&trace(&img, &s)).len();
        assert!(n <= 8, "{n} colours");
        let palette = art_palette(&img, 8, None).unwrap();
        assert!(palette.len() <= 8, "{palette:?}");
    }

    /// The holes of a mask: the patch of unset pixels it closes round,
    /// not what reaches the box's edge (the trace fills a region's holes
    /// that hold only regions drawn above it).
    #[test]
    fn holes_of_regions_drawn_above_are_filled() {
        let (w, h) = (7, 7);
        let ring: Vec<bool> = (0..w * h)
            .map(|i| {
                let (x, y) = (i % w, i / w);
                (1..6).contains(&x)
                    && (1..6).contains(&y)
                    && !((2..5).contains(&x) && (2..5).contains(&y))
            })
            .collect();
        let mut found: Vec<Vec<usize>> = Vec::new();
        for_each_hole(&ring, w, h, |hole| found.push(hole.to_vec()));
        assert_eq!(found.len(), 1, "the hole, not the outside");
        found[0].sort_unstable();
        let want: Vec<usize> = (2..5)
            .flat_map(|y| (2..5).map(move |x| y * w + x))
            .collect();
        assert_eq!(found[0], want);
    }

    /// A logo of flat shapes saved with noise: a red dot, and a blue badge
    /// with rounded corners holding a green dot.
    fn shapes_logo() -> RgbaImage {
        const W: u32 = 240;
        const H: u32 = 150;
        let mut seed = 777u32;
        let mut noise = move || {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
            ((seed >> 16) % 13) as i32 - 6
        };
        let in_badge = |x: f32, y: f32| {
            let (dx, dy) = ((x - 160.0).abs() - 60.0 + 12.0, (y - 75.0).abs() - 40.0 + 12.0);
            let out = dx.max(0.0).hypot(dy.max(0.0));
            out + dx.max(dy).min(0.0) - 12.0 < 0.0
        };
        ImageBuffer::from_fn(W, H, |x, y| {
            let (mut c, mut n) = ([0.0f32; 3], 0.0);
            for sy in 0..4 {
                for sx in 0..4 {
                    let (px, py) = (x as f32 + (sx as f32 + 0.5) / 4.0, y as f32 + (sy as f32 + 0.5) / 4.0);
                    let col = if (px - 50.0).hypot(py - 75.0) < 30.0 {
                        [220.0, 30.0, 40.0]
                    } else if (px - 160.0).hypot(py - 75.0) < 22.0 {
                        [30.0, 170.0, 60.0]
                    } else if in_badge(px, py) {
                        [30.0, 60.0, 200.0]
                    } else {
                        [255.0, 255.0, 255.0]
                    };
                    for k in 0..3 {
                        c[k] += col[k];
                    }
                    n += 1.0;
                }
            }
            let mut v = |k: usize| ((c[k] / n) as i32 + noise()).clamp(0, 255) as u8;
            Rgba([v(0), v(1), v(2), 255])
        })
    }

    /// How far the points along `path` lie from the circle round `c`.
    fn off_circle(path: &BezPath, c: Point, r: f64) -> f64 {
        crate::trace_shapes::sampled(path, 0.25)
            .iter()
            .map(|p| ((*p - c).hypot() - r).abs())
            .fold(0.0, f64::max)
    }

    #[test]
    fn geometric_shapes_draw_true_circles_and_rounded_rectangles() {
        use tracedraw_core::geometry::PathEl;
        let img = shapes_logo();
        let plain = Preset::Logo.settings();
        assert!(!plain.geometric, "off at first");
        let s = Settings {
            geometric: true,
            ..plain.clone()
        };
        let t = trace(&img, &s);
        let of = |want: [u8; 3]| {
            t.iter()
                .find(|t| distance2(t.color.to_rgb8(), want) < 60 * 60)
                .unwrap_or_else(|| panic!("no {want:?} in {:?}", t.iter().map(|t| t.color.to_rgb8()).collect::<Vec<_>>()))
        };
        let count = |p: &BezPath, curves: bool| {
            p.elements()
                .iter()
                .filter(|e| {
                    if curves {
                        matches!(e, PathEl::CurveTo(..))
                    } else {
                        matches!(e, PathEl::LineTo(..))
                    }
                })
                .count()
        };
        // The red dot: a true circle, four cubics and no lines.
        let red = of([220, 30, 40]);
        assert_eq!((count(&red.path, true), count(&red.path, false)), (4, 0), "{:?}", red.path);
        assert!(off_circle(&red.path, Point::new(50.0, 75.0), 30.0) < 0.35);
        // The green dot inside the badge too.
        let green = of([30, 170, 60]);
        assert_eq!(count(&green.path, true), 4, "{:?}", green.path);
        assert!(off_circle(&green.path, Point::new(160.0, 75.0), 22.0) < 0.35);
        // The badge: straight sides, four rounded corners.
        let blue = of([30, 60, 200]);
        let outer = blue
            .path
            .elements()
            .iter()
            .skip(1)
            .take_while(|e| !matches!(e, PathEl::MoveTo(_)))
            .copied()
            .collect::<Vec<_>>();
        let cubics = outer.iter().filter(|e| matches!(e, PathEl::CurveTo(..))).count();
        assert!(cubics == 4 || cubics == 8, "{outer:?}");
        let b = blue.path.bounding_box();
        assert!((b.x0 - 100.0).abs() < 0.5 && (b.x1 - 220.0).abs() < 0.5, "{b:?}");
        assert!((b.y0 - 35.0).abs() < 0.5 && (b.y1 - 115.0).abs() < 0.5, "{b:?}");
        // The green dot and the badge share their edge: no page between.
        for k in 0..360 {
            let a = (k as f64).to_radians();
            for r in [21.5, 22.0, 22.5] {
                let p = Point::new(160.0 + r * a.cos(), 75.0 + r * a.sin());
                assert!(top_colour_at(&t, p).is_some(), "page shows at {p:?}");
            }
        }
        // Off, the shapes are traced as before: not whole circles.
        let before = trace(&img, &plain);
        let red = before
            .iter()
            .find(|t| distance2(t.color.to_rgb8(), [220, 30, 40]) < 60 * 60)
            .expect("red");
        assert!(red.path.elements().len() > 0);
    }

    /// The colour of the topmost curve whose fill holds `p` (even-odd).
    fn top_colour_at(t: &[Traced], p: Point) -> Option<[u8; 3]> {
        t.iter()
            .rev()
            .find(|t| t.closed && t.path.winding(p) % 2 != 0)
            .map(|t| t.color.to_rgb8())
    }

    /// Where the curves of a trace run along each other: the points along
    /// each curve (every quarter pixel) that lie on another curve (within
    /// 0.05 px), and those that run beside one, 0.05 to 2.5 px off it: a
    /// border drawn twice, once by each region beside it (#1250). Points
    /// within 3 px of a junction (a pixel corner that is a node of two
    /// curves or more, where regions meet) are left out: the curves
    /// leaving it part there.
    fn border_distances(t: &[Traced]) -> (usize, usize) {
        use tracedraw_core::geometry::PathEl;
        let mut nodes: std::collections::HashMap<(i64, i64), Vec<usize>> = Default::default();
        for (k, c) in t.iter().enumerate() {
            for el in c.path.elements() {
                if let PathEl::MoveTo(p) | PathEl::LineTo(p) | PathEl::CurveTo(_, _, p) = el {
                    // On a corner of the finer grid (a quarter pixel).
                    let (qx, qy) = (p.x * 4.0, p.y * 4.0);
                    if qx.fract() == 0.0 && qy.fract() == 0.0 {
                        let v = nodes.entry((qx as i64, qy as i64)).or_default();
                        if !v.contains(&k) {
                            v.push(k);
                        }
                    }
                }
            }
        }
        let junctions: Vec<Point> = nodes
            .iter()
            .filter(|(_, v)| v.len() >= 2)
            .map(|((x, y), _)| Point::new(*x as f64 / 4.0, *y as f64 / 4.0))
            .collect();
        let near_junction = |p: Point| junctions.iter().any(|j| (p - *j).hypot2() < 9.0);
        let mut segments: Vec<(usize, Point, Point)> = Vec::new();
        for (k, c) in t.iter().enumerate() {
            let mut at = Point::ZERO;
            let mut start = Point::ZERO;
            tracedraw_core::geometry::flatten(&c.path, 0.005, &mut |el| match el {
                PathEl::MoveTo(p) => {
                    at = p;
                    start = p;
                }
                PathEl::LineTo(p) => {
                    segments.push((k, at, p));
                    at = p;
                }
                PathEl::ClosePath => {
                    segments.push((k, at, start));
                    at = start;
                }
                _ => {}
            });
        }
        const CELL: f64 = 3.0;
        let mut grid: std::collections::HashMap<(i64, i64), Vec<usize>> = Default::default();
        for (i, (_, a, b)) in segments.iter().enumerate() {
            let (x0, x1) = (a.x.min(b.x), a.x.max(b.x));
            let (y0, y1) = (a.y.min(b.y), a.y.max(b.y));
            for gy in (y0 / CELL).floor() as i64..=(y1 / CELL).floor() as i64 {
                for gx in (x0 / CELL).floor() as i64..=(x1 / CELL).floor() as i64 {
                    grid.entry((gx, gy)).or_default().push(i);
                }
            }
        }
        let to_segment = |p: Point, a: Point, b: Point| {
            let ab = b - a;
            let l = ab.hypot2();
            let t = if l < 1e-12 {
                0.0
            } else {
                ((p - a).dot(ab) / l).clamp(0.0, 1.0)
            };
            (p - (a + ab * t)).hypot()
        };
        let (mut together, mut apart) = (0, 0);
        for (k, a, b) in &segments {
            let n = ((*b - *a).hypot() / 0.25).ceil().max(1.0) as usize;
            for s in 0..n {
                let p = a.lerp(*b, s as f64 / n as f64);
                if near_junction(p) {
                    continue;
                }
                let (gx, gy) = ((p.x / CELL).floor() as i64, (p.y / CELL).floor() as i64);
                let mut best = f64::INFINITY;
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        for &i in grid.get(&(gx + dx, gy + dy)).into_iter().flatten() {
                            let (j, c, d) = segments[i];
                            if j != *k {
                                best = best.min(to_segment(p, c, d));
                            }
                        }
                    }
                }
                if best < 0.05 {
                    together += 1;
                } else if best < 2.5 {
                    apart += 1;
                }
            }
        }
        (together, apart)
    }

    /// A bee's head with its eyes, cheeks, wings beside it and stripes
    /// below (#1250): every border between two colours is one curve that
    /// both regions use, so their outlines coincide (no second curve a
    /// pixel or two beside the first, as when each region was fitted on
    /// its own and reached under the next), no page shows between them,
    /// and the head, drawn under its eyes and cheeks, is one ring.
    #[test]
    fn neighbouring_regions_share_one_curve_on_each_border() {
        const N: u32 = 160;
        let yellow = Rgba([255, 204, 53, 255]);
        let black = Rgba([20, 18, 18, 255]);
        let blue = Rgba([148, 213, 217, 255]);
        let orange = Rgba([246, 153, 99, 255]);
        let white = Rgba([255, 255, 255, 255]);
        let colour = |x: f64, y: f64| -> Rgba<u8> {
            let d = |cx: f64, cy: f64| ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
            let ellipse = |cx: f64, cy: f64, rx: f64, ry: f64| {
                ((x - cx) / rx).powi(2) + ((y - cy) / ry).powi(2) < 1.0
            };
            if d(80.0, 62.0) < 40.0 {
                if d(64.0, 58.0) < 7.0 || d(96.0, 58.0) < 7.0 {
                    black
                } else if ellipse(60.0, 76.0, 7.0, 4.5) || ellipse(100.0, 76.0, 7.0, 4.5) {
                    orange
                } else {
                    yellow
                }
            } else if ellipse(80.0, 118.0, 32.0, 30.0) {
                // Stripes across the body, side by side.
                if ((y - 0.15 * x) as i64).rem_euclid(18) < 9 {
                    black
                } else {
                    yellow
                }
            } else if ellipse(38.0, 70.0, 30.0, 20.0) || ellipse(122.0, 70.0, 30.0, 20.0) {
                blue
            } else {
                white
            }
        };
        // Anti-aliased, 4 x 4 samples a pixel.
        let img: RgbaImage = ImageBuffer::from_fn(N, N, |x, y| {
            let mut sum = [0u32; 3];
            for sy in 0..4 {
                for sx in 0..4 {
                    let c = colour(
                        x as f64 + (sx as f64 + 0.5) / 4.0,
                        y as f64 + (sy as f64 + 0.5) / 4.0,
                    );
                    for k in 0..3 {
                        sum[k] += c[k] as u32;
                    }
                }
            }
            Rgba([
                (sum[0] / 16) as u8,
                (sum[1] / 16) as u8,
                (sum[2] / 16) as u8,
                255,
            ])
        });
        let mut s = Preset::Clipart.settings();
        let t = trace(&img, &s);
        let (together, apart) = border_distances(&t);
        assert!(together > 1000, "borders shared: {together}");
        assert!(
            apart * 20 < together,
            "{apart} beside another curve, {together} on one"
        );
        // No page between the regions: every point well inside the art
        // (three pixels from its outer edge) is inside a curve.
        let art = |x: f64, y: f64| colour(x, y) != white;
        for y in 0..N {
            for x in 0..N {
                let p = Point::new(x as f64 + 0.3, y as f64 + 0.6);
                let deep =
                    (-3..=3).all(|dy| (-3..=3).all(|dx| art(p.x + dx as f64, p.y + dy as f64)));
                if deep {
                    assert!(top_colour_at(&t, p).is_some(), "page shows at {p:?}");
                }
            }
        }
        // The head is one ring: its eyes and cheeks are drawn over it.
        let is_yellow = |c: &&Traced| {
            let [r, g, b] = c.color.to_rgb8();
            r > 200 && g > 180 && b < 120
        };
        let yellow_rings: Vec<usize> = t
            .iter()
            .filter(is_yellow)
            .map(|c| {
                c.path
                    .elements()
                    .iter()
                    .filter(|e| matches!(e, tracedraw_core::geometry::PathEl::MoveTo(_)))
                    .count()
            })
            .collect();
        assert_eq!(yellow_rings.len(), 1, "one yellow curve");
        let head_eye = Point::new(64.0, 58.0);
        let top = top_colour_at(&t, head_eye).unwrap_or([255; 3]);
        assert!(top[0] < 80, "the eye shows over the head: {top:?}");
        let yellow_curve = t.iter().find(is_yellow);
        assert!(
            yellow_curve.is_some_and(|c| c.path.winding(head_eye) % 2 != 0),
            "the head reaches under its eyes"
        );
        // Each curve one per patch too, and without smoothing.
        s.merge_adjacent = false;
        let (together, apart) = border_distances(&trace(&img, &s));
        assert!(
            apart * 20 < together,
            "patches: {apart} beside, {together} on one"
        );
        s.smoothing = 0.0;
        let (together, apart) = border_distances(&trace(&img, &s));
        assert!(
            apart * 20 < together,
            "unsmoothed: {apart} beside, {together} on one"
        );
    }

    /// The borders of a labelling: each edge between two regions on one
    /// border, borders ending at the corners where three regions meet,
    /// an island a loop; a ring put together from them runs forwards on
    /// one side and backwards on the other.
    #[test]
    fn borders_meet_at_junctions_and_islands_are_loops() {
        // 0 0 1 1
        // 0 2 2 1
        // 0 0 1 1, and a picture edge all round.
        let labels = vec![0, 0, 1, 1, 0, 2, 2, 1, 0, 0, 1, 1];
        let Some(b) = borders(&labels, 4, 3, None) else {
            panic!("not cancelled");
        };
        let edges: usize = b.iter().map(|b| b.steps.len()).sum();
        // Inner edges between different labels, plus the outline.
        let inner = 2 + 2 + 1 + 1 + 1 + 1;
        assert_eq!(edges, inner + 2 * (4 + 3));
        assert!(b.iter().all(|b| !b.closed));
        // A pixel of 1 inside 0: one loop.
        let mut island = vec![0usize; 25];
        island[12] = 1;
        let Some(b) = borders(&island, 5, 5, None) else {
            panic!("not cancelled");
        };
        assert_eq!(
            b.iter().filter(|b| b.closed).count(),
            2,
            "the island and the outline"
        );
        let raised = Cancel::new(true);
        assert!(borders(&island, 5, 5, Some(&raised)).is_none());
        // Reversed curves end where they start and keep their pieces.
        let mut c = BezPath::new();
        c.move_to(Point::new(0.0, 0.0));
        c.curve_to(
            Point::new(1.0, 0.0),
            Point::new(2.0, 1.0),
            Point::new(2.0, 2.0),
        );
        c.line_to(Point::new(0.0, 2.0));
        let mut r = BezPath::new();
        r.move_to(Point::new(0.0, 2.0));
        append_curve(&mut r, &c, false, false);
        use tracedraw_core::geometry::PathEl;
        assert_eq!(
            r.elements()[1..],
            [
                PathEl::LineTo(Point::new(2.0, 2.0)),
                PathEl::CurveTo(
                    Point::new(2.0, 1.0),
                    Point::new(1.0, 0.0),
                    Point::new(0.0, 0.0)
                ),
            ]
        );
    }

    /// Colours inside another's holes stack above it, the darkest first
    /// otherwise, and colours in each other's holes darkest first.
    #[test]
    fn colours_inside_holes_stack_above_the_one_around() {
        // 0 holds 2 in a hole, 1 is beside both.
        let mut inside = vec![vec![false; 3]; 3];
        inside[0][2] = true;
        assert_eq!(stacking_order(&inside, &[200, 10, 50]), vec![1, 0, 2]);
        // 0 and 2 in each other's holes: the darker first.
        inside[2][0] = true;
        assert_eq!(stacking_order(&inside, &[200, 10, 50]), vec![1, 2, 0]);
        assert_eq!(stacking_order(&[], &[]), Vec::<usize>::new());
    }

    /// The fringe of a compressed edge drifts off the segment between the
    /// two colours (its colour is stored coarser than its brightness): a
    /// band three pixels wide of a yellow overshooting towards orange,
    /// along the edge of black and yellow, is no colour of the art and
    /// goes to the colours either side, with the Photo presets' many
    /// colours as with Clipart (#611). Before, it lay too far from the
    /// segment to be the edge's blend and too far from yellow to join
    /// it, and became a sliver of its own along every such edge.
    #[test]
    fn off_segment_fringes_of_a_compressed_edge_go_to_the_colours_either_side() {
        let band = [232u8, 192, 90];
        let img: RgbaImage = ImageBuffer::from_fn(120, 80, |x, _| match x {
            0..=49 => Rgba([0, 0, 0, 255]),
            50..=52 => Rgba([band[0], band[1], band[2], 255]),
            _ => Rgba([250, 205, 40, 255]),
        });
        for preset in [
            Preset::HighQualityImage,
            Preset::LowQualityImage,
            Preset::Clipart,
        ] {
            let mut s = preset.settings();
            s.remove = Remove::None;
            let t = trace(&img, &s);
            let colours: Vec<[u8; 3]> = t.iter().map(|t| t.color.to_rgb8()).collect();
            assert!(
                colours.iter().all(|c| distance2(*c, band) > 30 * 30),
                "{preset:?}: the band stayed: {colours:?}"
            );
            assert!(
                colours.iter().any(|c| c[1] < 30) && colours.iter().any(|c| c[1] > 180),
                "{preset:?}: black and yellow: {colours:?}"
            );
            // Black and yellow meet: the band's pixels are covered.
            let out = render_traced(&t, 120, 80, 1.0);
            for x in 48..56 {
                assert_eq!(
                    out.get_pixel(x, 40)[3],
                    255,
                    "{preset:?}: page shows at {x}"
                );
            }
        }
    }

    /// Shades of one colour split by noise join, but a run of shades,
    /// each near the next, is not one colour: two groups join only when
    /// every colour of one is near every colour of the other (#611). A
    /// pink cheek, the blends of its blurred edge and the yellow around
    /// it chained into a mean colour none of them had.
    #[test]
    fn split_shades_join_but_a_run_of_shades_does_not_chain() {
        // Four colours in bands, each 23.6 apart from the next, the
        // first and third 47 apart. Their pixels scatter round them so
        // each pair lies nearer than the sum of their spreads.
        let shades = [
            [240u8, 150, 110],
            [226, 164, 96],
            [212, 178, 82],
            [198, 192, 68],
        ];
        let (w, h) = (80usize, 40usize);
        let mut labels = Vec::with_capacity(w * h);
        let mut colors = Vec::with_capacity(w * h);
        for i in 0..w * h {
            let (x, y) = (i % w, i / w);
            let l = y / 10;
            let sign = if (x + y) % 2 == 0 { 15i32 } else { -15 };
            labels.push(l);
            colors.push(std::array::from_fn(|k| {
                (shades[l][k] as i32 + sign).clamp(0, 255) as u8
            }));
        }
        let palette = merge_split_colors(&mut labels, w, h, shades.to_vec(), &colors);
        let at = |y: usize| labels[y * w + 5];
        assert_eq!(at(0), at(10), "the first two shades join: {palette:?}");
        assert_eq!(at(20), at(30), "the last two shades join: {palette:?}");
        assert_ne!(at(0), at(20), "the run does not chain: {palette:?}");
        let pink = palette[at(0)];
        assert!(
            distance2(pink, [233, 157, 103]) <= 4 * 4,
            "the pink group's colour is their mean: {pink:?}"
        );
    }

    /// The glint in a dark eye, a few white pixels standing out from the
    /// black around them, stays at the low detail of the Low quality
    /// image preset, which otherwise drops every patch under some forty
    /// pixels (#611). Noise never differs from its surroundings by that
    /// much once the picture is quietened.
    #[test]
    fn a_glint_standing_out_in_a_dark_eye_stays_at_low_detail() {
        let img: RgbaImage = ImageBuffer::from_fn(100, 100, |x, y| {
            let (dx, dy) = (x as f32 - 50.0, y as f32 - 50.0);
            if (44..47).contains(&x) && (44..47).contains(&y) {
                Rgba([255, 255, 255, 255])
            } else if dx * dx + dy * dy <= 15.0 * 15.0 {
                Rgba([0, 0, 0, 255])
            } else {
                Rgba([250, 205, 40, 255])
            }
        });
        let s = Preset::LowQualityImage.settings();
        let t = trace(&img, &s);
        let colours: Vec<[u8; 3]> = t.iter().map(|t| t.color.to_rgb8()).collect();
        assert!(
            colours.iter().any(|c| c[0] > 90 && c[2] > 80),
            "the glint: {colours:?}"
        );
        // Still no speck of noise: a lone off pixel in the yellow goes.
        let mut noisy = img.clone();
        noisy.put_pixel(20, 20, Rgba([240, 190, 60, 255]));
        let t = trace(&noisy, &s);
        assert_eq!(t.len(), colours.len(), "{:?}", color_counts(&t));
    }
    /// A picture of `n` x `n` pixels drawn with anti-aliasing: each pixel
    /// the mean of `colour` at 8 x 8 places inside it.
    fn antialiased(n: u32, colour: impl Fn(f64, f64) -> [u8; 3]) -> RgbaImage {
        ImageBuffer::from_fn(n, n, |x, y| {
            let mut sum = [0u32; 3];
            for sy in 0..8 {
                for sx in 0..8 {
                    let c = colour(
                        x as f64 + (sx as f64 + 0.5) / 8.0,
                        y as f64 + (sy as f64 + 0.5) / 8.0,
                    );
                    for k in 0..3 {
                        sum[k] += c[k] as u32;
                    }
                }
            }
            Rgba([
                (sum[0] / 64) as u8,
                (sum[1] / 64) as u8,
                (sum[2] / 64) as u8,
                255,
            ])
        })
    }

    const WHITE: [u8; 3] = [255, 255, 255];
    const BLACK: [u8; 3] = [0, 0, 0];

    /// With all the detail and no smoothing, anti-aliased art is fitted
    /// with curves and lines, not walked pixel by pixel (#2925): a disc
    /// is a few curves that stay on the circle, a tilted square four
    /// straight lines with its corners where the art has them.
    #[test]
    fn anti_aliased_art_is_fitted_at_the_highest_detail() {
        use tracedraw_core::geometry::PathEl;
        let mut s = Preset::Logo.settings();
        s.detail = 100.0;
        s.smoothing = 0.0;
        let centre = Point::new(60.3, 59.6);
        let disc = antialiased(120, |x, y| {
            if (Point::new(x, y) - centre).hypot() < 31.4 {
                BLACK
            } else {
                WHITE
            }
        });
        let t = trace(&disc, &s);
        assert_eq!(t.len(), 1, "the disc alone");
        let nodes = stats(&t).nodes;
        assert!(nodes <= 16, "{nodes} nodes for a disc");
        for seg in t[0].path.segments() {
            for k in 0..=8 {
                let p = tracedraw_core::geometry::ParamCurve::eval(&seg, k as f64 / 8.0);
                let r = (p - centre).hypot();
                assert!((r - 31.4).abs() < 0.25, "radius {r} at {p:?}");
            }
        }
        // A square turned by 30 degrees, its corners off the pixel grid.
        let (c, sn) = (30f64.to_radians().cos(), 30f64.to_radians().sin());
        let square = antialiased(120, |x, y| {
            let (dx, dy) = (x - 60.2, y - 60.7);
            let (u, v) = (dx * c + dy * sn, -dx * sn + dy * c);
            if u.abs() < 30.0 && v.abs() < 30.0 {
                BLACK
            } else {
                WHITE
            }
        });
        let t = trace(&square, &s);
        assert_eq!(t.len(), 1);
        let els = t[0].path.elements();
        let lines = els
            .iter()
            .filter(|e| matches!(e, PathEl::LineTo(_)))
            .count();
        let curves = els
            .iter()
            .filter(|e| matches!(e, PathEl::CurveTo(..)))
            .count();
        assert!((3..=4).contains(&lines) && curves == 0, "{els:?}");
        let corners: Vec<Point> = [(-30.0, -30.0), (30.0, -30.0), (30.0, 30.0), (-30.0, 30.0)]
            .iter()
            .map(|(u, v)| Point::new(60.2 + u * c - v * sn, 60.7 + u * sn + v * c))
            .collect();
        for corner in corners {
            let nearest = els
                .iter()
                .filter_map(|e| match e {
                    PathEl::MoveTo(p) | PathEl::LineTo(p) => Some((*p - corner).hypot()),
                    _ => None,
                })
                .fold(f64::MAX, f64::min);
            assert!(
                nearest < 0.3,
                "a node {nearest} px from the corner {corner:?}"
            );
        }
    }

    /// The edge of two colours is traced where it runs inside the pixels
    /// that blend them (#2925, #2926): an edge three tenths into a
    /// column of pixels is drawn there, not on the pixel grid.
    #[test]
    fn an_edge_is_traced_where_it_runs_inside_its_pixels() {
        let img = antialiased(60, |x, _| if x < 20.3 { BLACK } else { WHITE });
        let mut s = Preset::Logo.settings();
        s.remove = Remove::None;
        let t = trace(&img, &s);
        let black = t
            .iter()
            .find(|t| t.color.to_rgb8() == [0, 0, 0])
            .expect("the black half");
        let b = black.path.bounding_box();
        assert!((b.x1 - 20.3).abs() < 0.1, "{b:?}");
        assert!(b.x0.abs() < 1e-3 && b.y0.abs() < 1e-3 && (b.y1 - 60.0).abs() < 1e-3);
    }

    /// The pixels blending two colours along their edge are no shape of
    /// a third colour that looks like the blend (#2929), at any detail: a
    /// navy disc on white beside a square of the blue half way between
    /// them gives a disc and a square, no blue ring round the disc.
    #[test]
    fn edge_pixels_make_no_ring_of_a_look_alike_colour() {
        let navy = [0u8, 40, 104];
        let blue = [128u8, 148, 180];
        let img = antialiased(140, |x, y| {
            if (Point::new(x, y) - Point::new(45.3, 70.2)).hypot() < 30.0 {
                navy
            } else if (95.0..125.0).contains(&x) && (50.0..90.0).contains(&y) {
                blue
            } else {
                WHITE
            }
        });
        for detail in [60.0, 100.0] {
            let mut s = Preset::Logo.settings();
            s.detail = detail;
            s.merge_adjacent = false;
            let t = trace(&img, &s);
            let colours: Vec<[u8; 3]> = t.iter().map(|t| t.color.to_rgb8()).collect();
            assert_eq!(t.len(), 2, "detail {detail}: {colours:?}");
            let blue_shape = t
                .iter()
                .find(|t| t.color.to_rgb8()[0] > 100)
                .expect("the square");
            let b = blue_shape.path.bounding_box();
            assert!(b.x0 > 90.0, "detail {detail}: blue reaches {b:?}");
        }
    }

    /// An O on a light background with the Logo preset (#4328): the
    /// letter is one curve with a real hole, ready to cut, not a disc
    /// under a patch of the background colour.
    #[test]
    fn the_inside_of_a_letter_is_a_hole() {
        use tracedraw_core::geometry::PathEl;
        let cream = [246, 238, 220];
        let img = antialiased(120, |x, y| {
            let r = (Point::new(x, y) - Point::new(60.0, 60.0)).hypot();
            if (22.0..40.0).contains(&r) {
                BLACK
            } else {
                cream
            }
        });
        let t = trace(&img, &Preset::Logo.settings());
        let o = t
            .iter()
            .find(|t| t.color.to_rgb8() == [0, 0, 0])
            .expect("the letter");
        let rings = o
            .path
            .elements()
            .iter()
            .filter(|e| matches!(e, PathEl::MoveTo(_)))
            .count();
        assert_eq!(rings, 2, "the O has its hole");
        assert_eq!(o.path.winding(Point::new(60.0, 60.0)), 0);
    }

    /// Thin dark lines inside a light area that a dark frame holds, with
    /// the Logo preset (#4327): the lines show, as they do with Remove
    /// object overlap, not hidden under the light area filled over them.
    #[test]
    fn thin_lines_in_a_framed_area_stay_with_the_logo_preset() {
        let cream = [235, 205, 150];
        let img = antialiased(200, |x, y| {
            let r = (Point::new(x, y) - Point::new(100.0, 100.0)).hypot();
            // Fine lines across the middle, a pixel and a half wide.
            let line = r < 55.0 && ((x - y).abs() < 1.1 || (x + y - 200.0).abs() < 1.1);
            if (80.0..92.0).contains(&r) || line {
                BLACK
            } else if (60.0..80.0).contains(&r) {
                cream
            } else {
                WHITE
            }
        });
        for overlap in [false, true] {
            let mut s = Preset::Logo.settings();
            s.remove_overlap = overlap;
            let t = trace(&img, &s);
            for p in [Point::new(80.0, 80.0), Point::new(120.0, 80.0), Point::new(90.0, 110.0)] {
                assert_eq!(
                    top_colour_at(&t, p),
                    Some([0, 0, 0]),
                    "overlap {overlap}: the line at {p:?}"
                );
            }
        }
    }

    /// A thin faint stroke and small print stay (#2927): a ring seven
    /// pixels across drawn a pixel wide, never darker than mid grey,
    /// comes out as a black ring with its hole.
    #[test]
    fn thin_faint_strokes_keep_their_shape_and_take_the_full_colour() {
        use tracedraw_core::geometry::PathEl;
        let img = antialiased(80, |x, y| {
            let big = (10.0..40.0).contains(&x) && (10.0..30.0).contains(&y);
            let r = (Point::new(x, y) - Point::new(55.4, 55.6)).hypot();
            // The ring's ink is spread thin: grey at its darkest.
            let ring = (2.9..3.9).contains(&r);
            if big {
                BLACK
            } else if ring {
                [110, 110, 110]
            } else {
                WHITE
            }
        });
        for preset in [Preset::Logo, Preset::DetailedLogo, Preset::Optimized] {
            let mut s = preset.settings();
            s.merge_adjacent = false;
            let t = trace(&img, &s);
            let ring = t
                .iter()
                .find(|t| t.path.bounding_box().x0 > 45.0)
                .unwrap_or_else(|| panic!("{preset:?}: the ring is gone"));
            assert_eq!(ring.color.to_rgb8(), [0, 0, 0], "{preset:?}");
            let subpaths = ring
                .path
                .elements()
                .iter()
                .filter(|e| matches!(e, PathEl::MoveTo(_)))
                .count();
            // The hole: a ring of its own in the curve, or the white in
            // it drawn above.
            let above = t.iter().any(|o| {
                let c = o.path.bounding_box();
                o.color.to_rgb8() == [255, 255, 255] && c.x0 > 50.0 && c.y0 > 50.0
            });
            assert!(
                subpaths == 2 || above,
                "{preset:?}: the ring keeps its hole"
            );
            let b = ring.path.bounding_box();
            assert!(
                (b.width() - 7.8).abs() < 1.2 && (b.height() - 7.8).abs() < 1.2,
                "{preset:?}: {b:?}"
            );
        }
    }

    /// Removing the background automatically (#2928): a dark surround is
    /// part of the design and stays; light paper goes whole, its soft
    /// shades too, leaving the art alone.
    #[test]
    fn only_paper_is_removed_and_all_of_it() {
        let gold = [212u8, 170, 80];
        let dark = [24u8, 22, 28];
        let emblem = antialiased(100, |x, y| {
            if (Point::new(x, y) - Point::new(50.0, 50.0)).hypot() < 25.0 {
                gold
            } else {
                dark
            }
        });
        let t = trace(&emblem, &Preset::Logo.settings());
        let colours: Vec<[u8; 3]> = t.iter().map(|t| t.color.to_rgb8()).collect();
        assert_eq!(t.len(), 2, "{colours:?}");
        assert!(colours.iter().any(|c| c[0] < 60), "the dark square stays");
        // The dark colour specified goes all the same.
        let mut s = Preset::Logo.settings();
        s.background = Some(dark);
        assert_eq!(trace(&emblem, &s).len(), 1);
        // Paper that darkens towards the corners.
        let vignette = antialiased(120, |x, y| {
            let d = (Point::new(x, y) - Point::new(60.0, 54.0)).hypot();
            if d < 22.0 {
                [40, 60, 200]
            } else {
                let v = (246.0 - d * 0.45) as u8;
                [v, v, v]
            }
        });
        for preset in [Preset::Logo, Preset::DetailedLogo, Preset::Clipart] {
            let t = trace(&vignette, &preset.settings());
            let colours: Vec<[u8; 3]> = t.iter().map(|t| t.color.to_rgb8()).collect();
            assert_eq!(t.len(), 1, "{preset:?}: {colours:?}");
            assert!(colours[0][2] > 150 && colours[0][0] < 90, "{preset:?}");
        }
    }

    /// A gradient is one region filled with a blend, not a row of bands
    /// (#2929): a bar running from red to yellow comes out as one curve
    /// whose blend runs along the bar from red to yellow.
    #[test]
    fn a_gradient_is_one_region_with_a_blend() {
        let img = antialiased(160, |x, y| {
            if (20.0..140.0).contains(&x) && (50.0..110.0).contains(&y) {
                let t = (x - 20.0) / 120.0;
                [230, (30.0 + 190.0 * t) as u8, 20]
            } else {
                WHITE
            }
        });
        for preset in [Preset::DetailedLogo, Preset::Clipart, Preset::Optimized] {
            let t = trace(&img, &preset.settings());
            assert_eq!(t.len(), 1, "{preset:?}: {:?}", stats(&t));
            let blend = t[0].blend.expect("a blend");
            assert!(
                blend.start[1] < 60 && blend.end[1] > 190,
                "{preset:?}: {blend:?}"
            );
            let along = blend.to - blend.from;
            assert!(
                along.x > 100.0 && along.y.abs() < 0.1 * along.x,
                "{preset:?}: {blend:?}"
            );
            // Drawn back, it is the picture.
            let drawn = render_traced(&t, 160, 160, 1.0);
            let wrong = wrong_share(&img, &drawn, None).expect("compared");
            assert!(wrong < 0.5, "{preset:?}: {wrong}% wrong");
        }
        // Flat colours side by side stay apart, however soft their edge.
        let flat = antialiased(100, |x, _| {
            if x < 50.0 {
                [200, 60, 60]
            } else {
                [230, 120, 60]
            }
        });
        let soft = image::imageops::blur(&flat, 1.5);
        let mut s = Preset::DetailedLogo.settings();
        s.remove = Remove::None;
        let t = trace(&soft, &s);
        assert_eq!(t.len(), 2, "{:?}", stats(&t));
        assert!(t.iter().all(|t| t.blend.is_none()));
    }

    /// Best optimization never leaves a trace worse than the one it
    /// started from (#2930): the detail the settings have is among those
    /// tried, and a second search from the result changes nothing.
    #[test]
    fn the_search_keeps_the_detail_when_none_is_better() {
        let img = antialiased(120, |x, y| {
            let r = (Point::new(x, y) - Point::new(60.0, 60.0)).hypot();
            if r < 40.0 && !((57.0..63.0).contains(&x) && y > 40.0) {
                BLACK
            } else {
                WHITE
            }
        });
        let score = |s: &Settings| {
            let t = trace(&img, s);
            let drawn = render_traced(&t, 120, 120, 1.0);
            wrong_share(&img, &drawn, None).expect("compared")
                + NODE_COST * stats(&t).nodes as f64 / 240.0
        };
        for detail in [35.0, 73.0, 100.0] {
            let mut s = Preset::Logo.settings();
            s.detail = detail;
            let found = optimize(&img, &s, None).expect("searched");
            assert!(
                score(&found) <= score(&s) + 1e-9,
                "from {detail}: {} is worse than {}",
                score(&found),
                score(&s)
            );
            let again = optimize(&img, &found, None).expect("searched");
            assert_eq!(again.detail, found.detail, "from {detail}");
        }
    }

    /// Small print that the picture's blur has softened comes out with
    /// straight sides and sharp corners: an H and an L seven pixels
    /// high, strokes under two pixels wide, are straight lines only,
    /// each level or upright, twelve and six corners.
    #[test]
    fn soft_small_capitals_have_straight_sides_and_sharp_corners() {
        use tracedraw_core::geometry::PathEl;
        let orange = [255u8, 136, 0];
        let inside = |x: f64, y: f64, x0: f64, y0: f64, x1: f64, y1: f64| {
            x >= x0 && x < x1 && y >= y0 && y < y1
        };
        let crisp = antialiased(48, |x, y| {
            // H: two stems and a bar. L: a stem and a foot.
            let h = inside(x, y, 10.2, 16.3, 11.9, 23.9)
                || inside(x, y, 14.6, 16.3, 16.3, 23.9)
                || inside(x, y, 10.2, 19.3, 16.3, 20.9);
            let l = inside(x, y, 26.4, 16.3, 28.1, 23.9) || inside(x, y, 26.4, 22.2, 31.8, 23.9);
            if h || l {
                orange
            } else {
                WHITE
            }
        });
        let soft = image::imageops::blur(&crisp, 0.7);
        let t = trace(&soft, &Preset::Logo.settings());
        let letters: Vec<&Traced> = t.iter().filter(|t| t.color.to_rgb8()[2] < 128).collect();
        assert_eq!(letters.len(), 1, "one curve of the two letters");
        let els = letters[0].path.elements();
        let curves = els
            .iter()
            .filter(|e| matches!(e, PathEl::CurveTo(..)))
            .count();
        assert_eq!(curves, 0, "{els:?}");
        let nodes = stats(&[letters[0].clone()]).nodes;
        assert_eq!(nodes, 18, "{els:?}");
        for seg in letters[0].path.segments() {
            let (a, b) = (
                tracedraw_core::geometry::ParamCurve::eval(&seg, 0.0),
                tracedraw_core::geometry::ParamCurve::eval(&seg, 1.0),
            );
            assert!(
                (a.x - b.x).abs() < 0.05 || (a.y - b.y).abs() < 0.05,
                "{a:?} to {b:?} is neither level nor upright"
            );
        }
    }

    /// An edge between two colours equally bright is traced where it
    /// runs inside its pixels too: purple beside a blue of the same
    /// brightness, the edge three tenths into a column of pixels.
    #[test]
    fn an_edge_between_colours_equally_bright_is_found() {
        let (purple, blue) = ([170u8, 70, 220], [60u8, 120, 255]);
        let img = antialiased(60, |x, _| if x < 20.3 { purple } else { blue });
        let mut s = Preset::Logo.settings();
        s.remove = Remove::None;
        let t = trace(&img, &s);
        let left = t
            .iter()
            .find(|t| t.color.to_rgb8()[0] > 120)
            .expect("the purple part");
        let b = left.path.bounding_box();
        assert!((b.x1 - 20.3).abs() < 0.15, "{b:?}");
    }

    /// A shape filled with a gradient, alone on its paper, is one region
    /// with a blend, not bands of flat colours.
    #[test]
    fn a_single_shape_with_a_gradient_is_one_blend() {
        let img = antialiased(120, |x, y| {
            if (Point::new(x, y) - Point::new(60.0, 60.0)).hypot() < 40.0 {
                let t = ((x - 20.0) / 80.0).clamp(0.0, 1.0);
                [
                    (255.0 - 100.0 * t) as u8,
                    (120.0 + 60.0 * t) as u8,
                    (180.0 + 70.0 * t) as u8,
                ]
            } else {
                WHITE
            }
        });
        let mut s = Preset::Logo.settings();
        s.remove = Remove::None;
        let t = trace(&img, &s);
        assert_eq!(t.len(), 2, "the paper and the disc");
        assert!(t.iter().any(|t| t.blend.is_some()), "the disc is a blend");
    }

    /// A high Smoothing takes the ripples out of a round shape without
    /// drawing it in: a disc keeps its area.
    #[test]
    fn a_high_smoothing_keeps_the_size_of_a_round_shape() {
        let img = antialiased(60, |x, y| {
            if (Point::new(x, y) - Point::new(30.4, 29.7)).hypot() < 8.0 {
                BLACK
            } else {
                WHITE
            }
        });
        let mut s = Preset::Logo.settings();
        s.smoothing = 100.0;
        let t = trace(&img, &s);
        assert_eq!(t.len(), 1);
        let (area, disc) = (t[0].path.area().abs(), std::f64::consts::PI * 64.0);
        assert!((area - disc).abs() < 0.02 * disc, "{area} for {disc}");
    }

    /// Below detail 100 a patch of a single pixel is a speck, however
    /// high the detail; noise is not taken out of the picture first from
    /// detail 88 up, where that would take fine lines too.
    #[test]
    fn single_pixels_are_specks_below_the_highest_detail() {
        assert_eq!(speck_area(100.0), 1);
        assert_eq!(speck_area(95.0), 2);
        assert_eq!(speck_area(90.0), 2);
        assert_eq!(speck_area(80.0), 4);
        assert_eq!(denoise_passes(90.0), 0);
        assert_eq!(denoise_passes(80.0), 1);
        assert_eq!(denoise_passes(60.0), 2);
    }

    /// A picture enlarged four times is measured in pixels of the art:
    /// a dot of two by two pixels is a quarter of a pixel of the art and
    /// a speck, where it stays in a picture traced as it is; the curves
    /// stay in the pixels of the picture given.
    #[test]
    fn an_enlarged_picture_is_measured_in_pixels_of_the_art() {
        let black = [0u8, 0, 0];
        let img = antialiased(200, |x, y| {
            let square = (40.0..80.0).contains(&x) && (40.0..80.0).contains(&y);
            let dot = (140.0..142.0).contains(&x) && (100.0..102.0).contains(&y);
            if square || dot {
                black
            } else {
                WHITE
            }
        });
        let patches = |scale: f64| {
            let mut s = Preset::Logo.settings();
            s.pixel_scale = scale;
            let t = trace(&img, &s);
            assert_eq!(t.len(), 1, "one colour");
            let b = t[0].path.bounding_box();
            let moves = t[0]
                .path
                .elements()
                .iter()
                .filter(|e| matches!(e, tracedraw_core::geometry::PathEl::MoveTo(..)))
                .count();
            (moves, b)
        };
        let (as_it_is, _) = patches(1.0);
        let (enlarged, b) = patches(4.0);
        assert_eq!(as_it_is, 2, "the dot is kept");
        assert_eq!(enlarged, 1, "the dot is a speck");
        for (got, want) in [(b.x0, 40.0), (b.y0, 40.0), (b.x1, 80.0), (b.y1, 80.0)] {
            assert!((got - want).abs() < 0.6, "{b:?}");
        }
    }

    /// Removing a shaded background from a picture traced on its own
    /// pixels leaves no pale line round the letters: the edge pixels of
    /// a shade of the background go with the shade.
    #[test]
    fn no_pale_line_is_left_round_a_letter_on_a_shaded_background() {
        let crisp = antialiased(240, |x, y| {
            let r = ((x - 120.0).powi(2) + (y - 120.0).powi(2)).sqrt();
            if (22.0..40.0).contains(&r) {
                [50, 18, 81]
            } else {
                // Two flat shades of paper, the lighter round the letter.
                let v = (236.0 - ((r - 70.0) * 0.8).clamp(0.0, 24.0)).round() as u8;
                [v, v, v]
            }
        });
        let img = image::imageops::blur(&crisp, 1.5);
        let mut s = Preset::Logo.settings();
        s.pixel_scale = 4.0;
        let t = trace(&img, &s);
        assert!(t.iter().any(|t| t.color.to_rgb8()[0] < 100), "the letter");
        for t in &t {
            let c = t.color.to_rgb8();
            if c[0] < 150 {
                continue;
            }
            let b = t.path.bounding_box();
            assert!(
                b.x0 > 92.0 && b.x1 < 148.0 && b.y0 > 92.0 && b.y1 < 148.0,
                "a pale curve outside the letter: {c:?} {b:?}"
            );
        }
    }

    /// The pale inside of a small letter that opens onto the paper is
    /// paper; one closed all round (the bowl of a B) stays a gap.
    #[test]
    fn the_gap_of_a_letter_open_to_the_paper_is_paper() {
        let (w, h) = (12usize, 12usize);
        let palette = [[255u8; 3], [0u8; 3], [232u8; 3]];
        let draw = |open: bool| -> Vec<usize> {
            (0..w * h)
                .map(|i| {
                    let (x, y) = (i % w, i / w);
                    let ring = (2..10).contains(&x) && (2..10).contains(&y);
                    let inside = (4..8).contains(&x) && (4..8).contains(&y);
                    let mouth = open && (8..10).contains(&x) && (5..7).contains(&y);
                    if inside || mouth {
                        2
                    } else if ring {
                        1
                    } else {
                        0
                    }
                })
                .collect()
        };
        let mut c = draw(true);
        open_gaps(&mut c, w, h, &palette, 40, None).unwrap();
        assert!(c.iter().all(|l| *l != 2), "{c:?}");
        let mut b = draw(false);
        open_gaps(&mut b, w, h, &palette, 40, None).unwrap();
        assert_eq!(b.iter().filter(|l| **l == 2).count(), 16);
    }

    /// The gap of a small A in a picture enlarged four times, smaller
    /// than a pixel of the art (the enlarging draws it small), stays;
    /// a few pixels of the picture do not.
    #[test]
    fn the_gap_of_a_small_letter_in_an_enlarged_picture_stays() {
        let (w, h) = (40usize, 40usize);
        let palette = [[255u8; 3], [0u8; 3]];
        let mut labels: Vec<usize> = (0..w * h)
            .map(|i| {
                let (x, y) = (i % w, i / w);
                let letter = (4..36).contains(&x) && (4..36).contains(&y);
                let gap = (10..13).contains(&x) && (10..14).contains(&y);
                let speck = (24..26).contains(&x) && (24..26).contains(&y);
                (letter && !gap && !speck) as usize
            })
            .collect();
        despeckle_of(&mut labels, w, h, 64, 16, 16, &palette, None);
        let at = |x: usize, y: usize| labels[y * w + x];
        assert_eq!(at(11, 12), 0, "the gap of twelve pixels stays");
        assert_eq!(at(24, 24), 1, "four pixels are a speck");
    }

    /// Enlarging leaves a darker rim along the edge of a colour with a
    /// lighter one. The rim is the colour's, not a line of the nearest
    /// darker colour of the picture.
    #[test]
    fn the_dark_rim_of_an_enlarged_edge_is_no_line() {
        let (blue, rim, navy) = ([28u8, 92, 168], [5u8, 45, 125], [0u8, 40, 100]);
        let img = antialiased(200, |x, y| {
            let r = ((x - 90.0).powi(2) + (y - 110.0).powi(2)).sqrt();
            if (150.0..190.0).contains(&x) && (10.0..40.0).contains(&y) {
                navy
            } else if r < 59.0 {
                blue
            } else if r < 60.0 {
                rim
            } else {
                WHITE
            }
        });
        let mut s = Preset::Logo.settings();
        s.pixel_scale = 4.0;
        let t = trace(&img, &s);
        let dark: Vec<&Traced> = t.iter().filter(|c| c.color.to_rgb8()[2] < 130).collect();
        assert_eq!(
            dark.len(),
            1,
            "{:?}",
            t.iter().map(|c| c.color).collect::<Vec<_>>()
        );
        let b = dark[0].path.bounding_box();
        assert!(
            b.x0 > 145.0 && b.y1 < 45.0,
            "only the square is navy: {b:?}"
        );
    }

    /// A small piece of a letter in a second shade of its colour is the
    /// letter (its colour runs from one shade to the other); a small
    /// piece whose own colour in the picture is far from the letter's
    /// is a shape, though the two colours of the palette are as near.
    #[test]
    fn a_small_piece_of_a_shade_joins_the_letter_it_is_of() {
        let (w, h) = (24usize, 24usize);
        let palette = [[20u8, 20, 28], [204, 163, 84], [232, 189, 101]];
        let labels: Vec<usize> = (0..w * h)
            .map(|i| {
                let (x, y) = (i % w, i / w);
                if !(4..20).contains(&x) || !(4..20).contains(&y) {
                    0
                } else if y < 9 {
                    2
                } else {
                    1
                }
            })
            .collect();
        let picture = |light: [u8; 3]| -> Vec<[u8; 3]> {
            labels
                .iter()
                .map(|l| if *l == 2 { light } else { palette[*l] })
                .collect()
        };
        let mut joined = labels.clone();
        join_shades(
            &mut joined,
            w,
            h,
            &palette,
            &picture([226, 184, 98]),
            256,
            None,
        )
        .unwrap();
        assert!(joined.iter().all(|l| *l != 2), "one letter");
        assert_eq!(joined.iter().filter(|l| **l == 1).count(), 256);
        let mut apart = labels.clone();
        join_shades(
            &mut apart,
            w,
            h,
            &palette,
            &picture([250, 225, 150]),
            256,
            None,
        )
        .unwrap();
        assert_eq!(apart, labels, "a shape of its own");
        let mut large = labels.clone();
        join_shades(
            &mut large,
            w,
            h,
            &palette,
            &picture([226, 184, 98]),
            40,
            None,
        )
        .unwrap();
        assert_eq!(large, labels, "too large to be a piece of a shade");
    }

    /// The check draws the trace and compares it with the picture: a gap
    /// the trace closed is found and given the picture's colour there,
    /// and a shade of the colour drawn is left alone.
    #[test]
    fn the_check_finds_a_closed_gap_and_leaves_a_shade() {
        let ink = [30u8, 20, 90];
        let whole = antialiased(80, |x, y| {
            if (20.0..60.0).contains(&x) && (20.0..60.0).contains(&y) {
                ink
            } else {
                WHITE
            }
        });
        let mut s = Preset::Logo.settings();
        s.remove = Remove::None;
        let traced = trace(&whole, &s);
        let (w, h) = (80usize, 80usize);
        let palette = [WHITE, ink];
        let labels: Vec<usize> = (0..w * h)
            .map(|i| ((20..60).contains(&(i % w)) && (20..60).contains(&(i / w))) as usize)
            .collect();
        let with = |c: [u8; 3]| {
            let mut img = whole.clone();
            for y in 34..42 {
                for x in 34..42 {
                    img.put_pixel(x, y, Rgba([c[0], c[1], c[2], 255]));
                }
            }
            img
        };
        // A white gap of eight by eight the trace has not.
        let img = with(WHITE);
        let (wrong, drawn) = differs(&img, &traced, &palette, None, None).unwrap();
        assert_eq!(wrong.iter().filter(|x| **x).count(), 64);
        let mut l = labels.clone();
        let changes = recheck(&img, &wrong, &drawn, &mut l, &palette, &[None, None], 12);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].pixels.len(), 64);
        assert!(changes[0].was.iter().all(|k| *k == 1));
        assert!(changes[0].pixels.iter().all(|i| l[*i] == 0));
        assert_eq!(l.iter().filter(|k| **k == 0).count(), 80 * 80 - 1600 + 64);
        // Too small a patch to be made again.
        let mut l = labels.clone();
        assert!(recheck(&img, &wrong, &drawn, &mut l, &palette, &[None, None], 100).is_empty());
        assert_eq!(l, labels);
        // A patch a shade lighter than the ink differs from nothing.
        let img = with([50, 40, 110]);
        let (wrong, drawn) = differs(&img, &traced, &palette, None, None).unwrap();
        assert!(wrong.iter().all(|x| !*x));
        let mut l = labels.clone();
        assert!(recheck(&img, &wrong, &drawn, &mut l, &palette, &[None, None], 12).is_empty());
        // Sizes that do not match change nothing.
        assert!(recheck(&img, &wrong[..10], &drawn, &mut l, &palette, &[], 1).is_empty());
    }

    /// A small letter in a picture enlarged four times is traced as it
    /// is drawn: its straight sides are lines level and upright where
    /// the picture has them, its corners sharp, in as few nodes as the
    /// letter has corners.
    #[test]
    fn a_small_letter_of_an_enlarged_picture_is_traced_as_drawn() {
        // An L: a stem of 12 by 60 and a foot of 40 by 12, edges between
        // the pixels.
        let img = antialiased(160, |x, y| {
            let stem = (50.5..62.5).contains(&x) && (40.5..100.5).contains(&y);
            let foot = (50.5..90.5).contains(&x) && (88.5..100.5).contains(&y);
            if stem || foot {
                [255, 140, 0]
            } else {
                WHITE
            }
        });
        let mut s = Preset::Logo.settings();
        s.pixel_scale = 4.0;
        let t = trace(&img, &s);
        assert_eq!(t.len(), 1, "one colour");
        let path = &t[0].path;
        use tracedraw_core::geometry::ParamCurve;
        let segs: Vec<_> = path.segments().collect();
        assert_eq!(segs.len(), 6, "{path:?}");
        for seg in &segs {
            let (a, b) = (seg.start(), seg.end());
            assert!(
                (a.x - b.x).abs() < 0.05 || (a.y - b.y).abs() < 0.05,
                "a side neither level nor upright: {a:?} {b:?}"
            );
            for p in [a, b] {
                let near = |v: f64, at: &[f64]| at.iter().any(|w| (v - w).abs() < 0.3);
                assert!(
                    near(p.x, &[50.5, 62.5, 90.5]) && near(p.y, &[40.5, 88.5, 100.5]),
                    "a corner off the letter's: {p:?}"
                );
            }
        }
        let drawn = render_traced(&t, 160, 160, 1.0);
        assert_eq!(wrong_share(&img, &drawn, None), Some(0.0));
    }
}

/// A flat-colour logo saved as a compressed picture, traced with the
/// highest detail, leaves no page showing between its colours (#3727).
#[cfg(test)]
mod page_gaps_tests {
    use super::*;

    /// A head of four flat colours with a black outline and a black ring
    /// on a blue ground, drawn smooth and saved as a JPEG.
    fn compressed_logo() -> RgbaImage {
        let n = 240;
        let mut pm = tiny_skia::Pixmap::new(n, n).unwrap();
        pm.fill(tiny_skia::Color::from_rgba8(0x1f, 0x4e, 0x9a, 255));
        let shape = |pts: &[(f32, f32)]| {
            let mut pb = tiny_skia::PathBuilder::new();
            pb.move_to(pts[0].0, pts[0].1);
            for p in &pts[1..] {
                pb.line_to(p.0, p.1);
            }
            pb.close();
            pb.finish().unwrap()
        };
        let paint = |c: [u8; 3]| {
            let mut paint = tiny_skia::Paint::default();
            paint.set_color_rgba8(c[0], c[1], c[2], 255);
            paint.anti_alias = true;
            paint
        };
        let id = tiny_skia::Transform::identity();
        let head = shape(&[
            (60.0, 40.0),
            (120.0, 70.0),
            (180.0, 40.0),
            (200.0, 140.0),
            (120.0, 210.0),
            (40.0, 140.0),
        ]);
        let fill = |pm: &mut tiny_skia::Pixmap, pts: &[(f32, f32)], c: [u8; 3]| {
            pm.fill_path(
                &shape(pts),
                &paint(c),
                tiny_skia::FillRule::Winding,
                id,
                None,
            );
        };
        pm.fill_path(
            &head,
            &paint([0x10, 0x20, 0x40]),
            tiny_skia::FillRule::Winding,
            id,
            None,
        );
        fill(
            &mut pm,
            &[(66.0, 52.0), (118.0, 80.0), (110.0, 150.0), (52.0, 138.0)],
            [0x6c, 0xb4, 0xe8],
        );
        fill(
            &mut pm,
            &[(118.0, 80.0), (172.0, 52.0), (188.0, 138.0), (126.0, 150.0)],
            [0xc8, 0xcc, 0xd2],
        );
        fill(
            &mut pm,
            &[(80.0, 150.0), (160.0, 150.0), (120.0, 200.0)],
            [0xff, 0xff, 0xff],
        );
        fill(
            &mut pm,
            &[(100.0, 160.0), (140.0, 160.0), (120.0, 185.0)],
            [0, 0, 0],
        );
        let stroke = tiny_skia::Stroke {
            width: 4.0,
            ..Default::default()
        };
        pm.stroke_path(&head, &paint([0, 0, 0]), &stroke, id, None);
        let mut pb = tiny_skia::PathBuilder::new();
        pb.push_circle(150.0, 100.0, 9.0);
        pm.stroke_path(&pb.finish().unwrap(), &paint([0, 0, 0]), &stroke, id, None);
        let img = RgbaImage::from_raw(n, n, pm.data().to_vec()).unwrap();
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 70)
            .encode_image(&image::DynamicImage::ImageRgba8(img).to_rgb8())
            .unwrap();
        image::load_from_memory(&jpeg).unwrap().to_rgba8()
    }

    /// Blots of the ground left between the colours of the art made the
    /// ground "inside" the outline it holds: the outline went under it,
    /// each was cut to the edge they share, and drawn smooth the page
    /// showed between them as a broken light line. Now every pixel of the
    /// trace is covered, with the photo and the logo settings alike, and
    /// the ground is one shape under the art.
    #[test]
    fn a_compressed_flat_logo_shows_no_page_between_its_colours() {
        let img = compressed_logo();
        let mut photo = Preset::HighQualityImage.settings();
        photo.detail = 100.0;
        photo.smoothing = 60.0;
        for s in [photo, Preset::DetailedLogo.settings()] {
            let traced = trace(&img, &s);
            let drawn = render_traced_scaled(&traced, img.width(), img.height(), 3.0, 1.0);
            let page = drawn.pixels().filter(|p| p[3] < 250).count();
            assert_eq!(page, 0, "page showing through, detail {}", s.detail);
            let ground = traced
                .iter()
                .position(|t| t.color.to_rgb8()[2] > 140 && t.color.to_rgb8()[0] < 60)
                .expect("the blue ground");
            assert_eq!(ground, 0, "the ground goes under everything");
        }
    }
}
