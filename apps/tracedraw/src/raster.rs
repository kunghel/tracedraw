//! Keeps the rasterized page as an egui texture, re-rendering only when the
//! document, view, canvas size or drag preview changed. While the view
//! moves (a pan, a scroll, a zoom) the last render is shown shifted or
//! scaled into place and the page is rendered anew once the view has
//! rested for a moment, so a page of thousands of objects pans smoothly.
//! A page that takes long to draw is drawn on a thread of its own: the
//! window keeps answering, showing the last picture moved into place,
//! until the new one is ready (#345).
//!
//! A zoom on the main canvas does not wait for the view to rest: the
//! last picture, scaled, looks blurry, so the page is rendered for the
//! new zoom on a thread at once. A zoom step that comes while that render
//! runs does not restart it: its picture is taken when ready (closer to
//! the zoom shown than the one before) and the next render starts at once
//! for the latest zoom, so a burst of wheel notches shows a sharper
//! picture every render and the last one about a render after the last
//! step.

use crate::view::View;
use egui::{ColorImage, Context, Rect, TextureHandle, TextureId, TextureOptions};
use tracedraw_core::{geometry::Affine, Document, PageId, ShapeId};
use tracedraw_render::{render_page, Preview, RenderOptions, ViewTransform};
#[cfg(not(target_arch = "wasm32"))]
use tracedraw_render::{
    render_page_cancellable, render_page_on_one_thread, render_page_progressive,
};

/// How long the view must rest, seconds, before a moved view is
/// rendered anew instead of showing the last render moved into place.
pub const SETTLE_SECONDS: f64 = 0.12;

/// The shortest wait, seconds, after the last step of a zoom before a
/// render still running for an earlier zoom is asked to give up: a few
/// frames, enough to tell a wheel that has stopped from one that goes on.
pub const ZOOM_SETTLE_MIN: f64 = 0.04;

/// How much longer than the gap between the last two steps of a zoom the
/// wait grows: notches a beat apart wait the shortest wait, slower turns
/// wait long enough that the render in flight still lands between the
/// notches.
pub const ZOOM_SETTLE_PACE: f64 = 1.5;

/// With Light zoom (Options > Tools > Zoom/Pan): how long the view rests
/// after a zoom before the drawing is drawn anew for it, seconds. Long
/// enough for the notches of one turn of the wheel to make one render.
pub const LIGHT_SETTLE_SECONDS: f64 = 0.3;

/// In tests, a render that took longer than this, seconds, makes the
/// next ones run on a thread of their own; the program draws every
/// picture after the first on its thread (#3440).
#[cfg(test)]
const SLOW_RENDER: f64 = 0.03;

/// How long the page's picture must have been the one asked for, seconds,
/// before the whole page is drawn small for the backdrop: while the
/// drawing or the view keeps changing no backdrop is drawn.
#[cfg(not(target_arch = "wasm32"))]
pub const BACKDROP_REST: f64 = 0.4;

/// How long until the backdrop is tried again when the pointer was
/// moving in the frame it came due, seconds.
#[cfg(not(target_arch = "wasm32"))]
const BACKDROP_IDLE: f64 = 0.1;

/// Largest side of the backdrop, pixels.
#[cfg(not(target_arch = "wasm32"))]
const BACKDROP_MAX: f64 = 2048.0;

/// What a backdrop shows: the drawing at a revision, drawn one way.
#[cfg(not(target_arch = "wasm32"))]
#[derive(PartialEq, Clone, Debug)]
struct BackKey {
    revision: u64,
    page: PageId,
    /// The part of the page it covers, thousandths of a millimetre.
    area: [i64; 4],
    size: (u32, u32),
    wireframe: bool,
    simulate_overprints: bool,
    complex_effects: bool,
    proof: bool,
}

/// The whole page drawn small (see [`Raster::backdrop`]).
#[cfg(not(target_arch = "wasm32"))]
struct Backdrop {
    texture: TextureHandle,
    key: BackKey,
    page_rect: tracedraw_core::geometry::Rect,
}

/// A backdrop being drawn on its thread.
#[cfg(not(target_arch = "wasm32"))]
struct BackJob {
    key: BackKey,
    page_rect: tracedraw_core::geometry::Rect,
    rx: std::sync::mpsc::Receiver<Option<ColorImage>>,
}

/// A render running on its own thread: what it is for, and where its
/// picture will come from.
#[cfg(not(target_arch = "wasm32"))]
struct Job {
    key: Key,
    page_rect: Option<tracedraw_core::geometry::Rect>,
    rx: std::sync::mpsc::Receiver<Option<(ColorImage, f64)>>,
    /// Set to make the render give up: the view zoomed on before it
    /// was ready (#2522).
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// The main canvas's picture a band of rows at a time as the bands
    /// come done (#2389): the first row of each and its pixels.
    bands: Option<std::sync::mpsc::Receiver<(u32, ColorImage)>>,
    /// The bands come so far, on a see-through texture the picture's
    /// size, shown over the last picture until the whole one is ready.
    incoming: Option<TextureHandle>,
    /// The rows of the picture the bands so far cover, first and past
    /// the last: the last picture is not shown under them (#3639).
    arrived: Vec<(u32, u32)>,
    /// The picture's size, pixels.
    size: (u32, u32),
    /// How the picture is laid on screen: document pixels (the Pixels
    /// view) as blocks.
    filter: TextureOptions,
}

/// Turns to render on a thread: one page job runs at a time in the whole
/// program (each holds a copy of the drawing and decodes on band threads
/// of its own; Multipage view used to start one per slow page at once,
/// and a job made stale kept running beside the next one). The page being
/// edited goes first: when it had to wait, the turn is kept for it for a
/// moment after the running job ends, and other pages wait.
#[cfg(not(target_arch = "wasm32"))]
struct Turns(std::sync::Mutex<TurnState>);

#[cfg(not(target_arch = "wasm32"))]
#[derive(Default)]
struct TurnState {
    busy: bool,
    /// The current page asked while a job ran.
    current_waits: bool,
    /// Until when a freed turn is kept for the current page.
    kept_until: Option<web_time::Instant>,
    /// Turns given to other pages, for tests.
    #[cfg(test)]
    to_others: usize,
}

/// How long a freed turn waits for the current page that asked for it.
#[cfg(not(target_arch = "wasm32"))]
const KEEP_FOR_CURRENT: std::time::Duration = std::time::Duration::from_secs(1);

#[cfg(not(target_arch = "wasm32"))]
impl Turns {
    const fn new() -> Self {
        Turns(std::sync::Mutex::new(TurnState {
            busy: false,
            current_waits: false,
            kept_until: None,
            #[cfg(test)]
            to_others: 0,
        }))
    }

    fn state(&self) -> std::sync::MutexGuard<'_, TurnState> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The turn for the current page (`current`) or another one, unless a
    /// job holds it or it is kept for the current page.
    fn take(&'static self, current: bool) -> Option<Turn> {
        let mut st = self.state();
        if st.busy {
            st.current_waits |= current;
            return None;
        }
        if !current && st.kept_until.is_some_and(|t| web_time::Instant::now() < t) {
            return None;
        }
        st.busy = true;
        #[cfg(test)]
        if !current {
            st.to_others += 1;
        }
        if current {
            st.current_waits = false;
            st.kept_until = None;
        }
        Some(Turn(self))
    }

    /// How long until a refused page should ask again, or `None` when a
    /// job holds the turn (its end repaints the window). A turn that is
    /// only kept for the current page has no job to wake the window, so
    /// whoever was refused must ask for a repaint of its own (#1384).
    fn retry_after(&self) -> Option<std::time::Duration> {
        let st = self.state();
        if st.busy {
            return None;
        }
        Some(st.kept_until.map_or(std::time::Duration::ZERO, |t| {
            t.saturating_duration_since(web_time::Instant::now())
        }))
    }
}

/// A job's turn, given back when it is dropped.
#[cfg(not(target_arch = "wasm32"))]
struct Turn(&'static Turns);

#[cfg(not(target_arch = "wasm32"))]
impl Drop for Turn {
    fn drop(&mut self) {
        let mut st = self.0.state();
        st.busy = false;
        if std::mem::take(&mut st.current_waits) {
            st.kept_until = Some(web_time::Instant::now() + KEEP_FOR_CURRENT);
        }
    }
}

#[cfg(all(not(target_arch = "wasm32"), not(test)))]
static RENDER_TURNS: Turns = Turns::new();

/// The program's turns, which every page of its window shares. Under
/// test each test thread has turns of its own: the tests run at the same
/// time, each with a window of its own, and one test's job would hold
/// back another's page, whose picture then came at a moment the test
/// could not foresee (a render starting in the middle of a drag).
#[cfg(not(target_arch = "wasm32"))]
fn program_turns() -> &'static Turns {
    #[cfg(test)]
    {
        thread_local! {
            static TURNS: &'static Turns = Box::leak(Box::new(Turns::new()));
        }
        TURNS.with(|t| *t)
    }
    #[cfg(not(test))]
    {
        &RENDER_TURNS
    }
}

/// A picture kept of a zoom the view has left (#3894).
struct Level {
    key: Key,
    texture: TextureHandle,
    page_rect: tracedraw_core::geometry::Rect,
}

/// At most this many pictures of earlier zooms are kept (#3894)...
const MAX_LEVELS: usize = 6;
/// ...and at most this many bytes of them (a 1600 x 1000 canvas is
/// 6.4 MB a picture).
const MAX_LEVEL_BYTES: usize = 96 << 20;

/// Which of the pictures (their zoom and the part of the page they show,
/// page mm) to show for a view at `zoom` showing `visible`: among those
/// that cover all of it, the nearest in zoom; `None` when none does
/// (#3894).
fn nearest_level(
    pictures: &[(f32, tracedraw_core::geometry::Rect)],
    zoom: f32,
    visible: tracedraw_core::geometry::Rect,
) -> Option<usize> {
    let covers = |r: &tracedraw_core::geometry::Rect| {
        let slack = 1e-6 * (visible.width() + visible.height()).max(1.0);
        r.x0 <= visible.x0 + slack
            && r.y0 <= visible.y0 + slack
            && r.x1 >= visible.x1 - slack
            && r.y1 >= visible.y1 - slack
    };
    pictures
        .iter()
        .enumerate()
        .filter(|(_, (z, r))| *z > 0.0 && zoom > 0.0 && covers(r))
        .min_by(|a, b| {
            let d = |z: f32| (z / zoom).ln().abs();
            d(a.1 .0).total_cmp(&d(b.1 .0))
        })
        .map(|(i, _)| i)
}

#[derive(Default)]
pub struct Raster {
    texture: Option<TextureHandle>,
    key: Option<Key>,
    /// The part of the page the texture shows, page mm.
    page_rect: Option<tracedraw_core::geometry::Rect>,
    /// The pictures of the zooms the view showed last, of the drawing as
    /// it stands (#3894): a zoom shows the nearest of them while its own
    /// is drawn, sharper than the last picture scaled, and coming back to
    /// one of them shows it at once.
    levels: Vec<Level>,
    /// While the view moves: the view last asked for and when (egui
    /// time) it will be rendered if it stays.
    settle: Option<(Key, f64)>,
    /// When (egui time) the view last stepped to a new zoom while a
    /// render ran: the wait before a render in flight for an earlier
    /// zoom is asked to give up follows the pace of these steps.
    zoom_step: Option<f64>,
    /// Proofed colours on a grid of screen colours (View > Proof Colors),
    /// made when first needed and dropped with the texture (the colour
    /// settings changed).
    proof: Option<std::sync::Arc<ProofLut>>,
    /// How long the last render took, seconds.
    last_seconds: f64,
    /// The render running on its own thread, if any.
    #[cfg(not(target_arch = "wasm32"))]
    job: Option<Job>,
    /// A picture whose canvas follows the zoom (a page beside the current
    /// one in the Multipage View): the last render stays moved and scaled
    /// into place while the view moves even though the canvas changed
    /// size, instead of rendering anew every frame, and once there is a
    /// picture the next ones are drawn on a thread of their own
    /// (#902, #903).
    pub follows_zoom: bool,
    /// No render this frame (the frame's time for them is spent): the
    /// last picture shows where its part of the page now is, and the
    /// render waits for a later frame (#903).
    pub defer: bool,
    /// Light zoom (Options > Tools > Zoom/Pan, for slow computers): a
    /// zoom shows the last picture scaled, like a pan, and the drawing
    /// is drawn anew once, when the view has rested
    /// [`LIGHT_SETTLE_SECONDS`], instead of at every step of the zoom.
    pub light_zoom: bool,
    /// The view the wheel's glide lands at, set by the canvas every
    /// frame while the glide goes: the picture is drawn for it while the
    /// view is still on its way, and the glide puts the view at it when
    /// it ends, so the picture fits the landing exactly.
    pub zoom_target: Option<View>,
    /// The drawing as given, shared, for a render on its own thread to
    /// take instead of a copy of its own (no threads in the browser).
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub snapshot: Option<std::sync::Arc<Document>>,
    /// What the page's objects cover (page mm), with the revision it was
    /// measured at: a page beside the current one shows its objects off
    /// the page too, and measuring them every frame would cost time
    /// (#2055).
    pub content: Option<(u64, Option<tracedraw_core::geometry::Rect>)>,
    /// The copy of the drawing the last render on a thread took, with
    /// its revision: the next renders of the same drawing (every step of
    /// a zoom) share it instead of copying the drawing again on the
    /// window's thread (a copy of a large drawing takes a frame's time).
    #[cfg(not(target_arch = "wasm32"))]
    kept: Option<(u64, std::sync::Arc<Document>)>,
    /// While a fill (or an outline colour) is dragged in a colour picker,
    /// the part of the page the drag can change (page mm: the selection's
    /// boxes, widened by its outlines and shadows; set by the canvas every
    /// frame, `None` otherwise): only it is rendered anew at every move of
    /// the pointer, into the picture already shown, instead of the whole
    /// page being drawn again for each move (#3559). The Pixels view, a
    /// drag preview, a focus and the pages beside the current one never
    /// take this path.
    pub live_region: Option<tracedraw_core::geometry::Rect>,
    /// Renders started so far, on this thread or another.
    renders: u64,
    /// Since when (egui time) the picture has been the one asked for,
    /// frame after frame: nothing moved and nothing changed.
    #[cfg(not(target_arch = "wasm32"))]
    rested: Option<f64>,
    /// The whole page drawn small, shown where the picture does not
    /// reach while the view moves (#2048).
    #[cfg(not(target_arch = "wasm32"))]
    backdrop: Option<Backdrop>,
    /// The backdrop being drawn on its thread, if any.
    #[cfg(not(target_arch = "wasm32"))]
    backdrop_job: Option<BackJob>,
    /// Backdrops started so far.
    #[cfg(not(target_arch = "wasm32"))]
    backdrops: u64,
    /// Turns other than the program's, for tests.
    #[cfg(not(target_arch = "wasm32"))]
    turns: Option<&'static Turns>,
    /// Every render counts as slow, for tests.
    #[cfg(test)]
    always_slow: bool,
    /// Every picture after the first drawn on its thread, as the program
    /// does, for tests (#3440).
    #[cfg(test)]
    threaded: bool,
    /// A lock a render on its thread takes before drawing, for tests that
    /// hold a render while the view moves on.
    #[cfg(test)]
    hold: Option<std::sync::Arc<std::sync::Mutex<()>>>,
}

/// Screen colours as they would print, on a `N` x `N` x `N` grid of the
/// screen's red, green and blue; colours between are interpolated.
struct ProofLut {
    table: Vec<[f32; 3]>,
}

impl ProofLut {
    const N: usize = 17;

    fn new() -> Self {
        let n = Self::N;
        let step = 1.0 / (n - 1) as f32;
        let mut table = Vec::with_capacity(n * n * n);
        for r in 0..n {
            for g in 0..n {
                for b in 0..n {
                    table.push(tracedraw_core::color::proof_rgb([
                        r as f32 * step,
                        g as f32 * step,
                        b as f32 * step,
                    ]));
                }
            }
        }
        ProofLut { table }
    }

    /// The proofed colour of `rgb` (0 to 1), interpolated between the
    /// eight grid colours around it.
    fn apply(&self, rgb: [f32; 3]) -> [f32; 3] {
        let n = Self::N;
        let scale = (n - 1) as f32;
        let mut i0 = [0usize; 3];
        let mut t = [0.0f32; 3];
        for k in 0..3 {
            let x = (rgb[k].clamp(0.0, 1.0)) * scale;
            let i = (x.floor() as usize).min(n - 2);
            i0[k] = i;
            t[k] = x - i as f32;
        }
        let at = |r: usize, g: usize, b: usize| self.table[(r * n + g) * n + b];
        let mut out = [0.0f32; 3];
        for (dr, wr) in [(0, 1.0 - t[0]), (1, t[0])] {
            for (dg, wg) in [(0, 1.0 - t[1]), (1, t[1])] {
                for (db, wb) in [(0, 1.0 - t[2]), (1, t[2])] {
                    let w = wr * wg * wb;
                    if w == 0.0 {
                        continue;
                    }
                    let c = at(i0[0] + dr, i0[1] + dg, i0[2] + db);
                    for k in 0..3 {
                        out[k] += c[k] * w;
                    }
                }
            }
        }
        out
    }

    /// A premultiplied pixel, proofed.
    fn pixel(&self, r: u8, g: u8, b: u8, a: u8) -> egui::Color32 {
        if a == 0 {
            return egui::Color32::TRANSPARENT;
        }
        let af = a as f32 / 255.0;
        let un = |v: u8| (v as f32 / 255.0 / af).min(1.0);
        let p = self.apply([un(r), un(g), un(b)]);
        let q = |v: f32| (v.clamp(0.0, 1.0) * af * 255.0).round() as u8;
        egui::Color32::from_rgba_premultiplied(q(p[0]), q(p[1]), q(p[2]), a)
    }
}

/// A rendered pixmap as an egui image, proofed through `lut` when given.
fn to_image(pixmap: &tiny_skia::Pixmap, lut: Option<&ProofLut>) -> ColorImage {
    let (w, h) = (pixmap.width(), pixmap.height());
    let pixels: Vec<egui::Color32> = pixmap
        .pixels()
        .iter()
        .map(|p| match lut {
            Some(lut) => lut.pixel(p.red(), p.green(), p.blue(), p.alpha()),
            None => egui::Color32::from_rgba_premultiplied(p.red(), p.green(), p.blue(), p.alpha()),
        })
        .collect();
    ColorImage {
        size: [w as usize, h as usize],
        source_size: egui::vec2(w as f32, h as f32),
        pixels,
    }
}

#[derive(PartialEq, Clone, Debug)]
struct Key {
    revision: u64,
    page: PageId,
    zoom: f32,
    /// The page origin relative to the canvas: what the render uses. A
    /// canvas that moves (a bar docked elsewhere) without changing size
    /// still needs a new render when the view stays put.
    offset: egui::Vec2,
    size: (u32, u32),
    /// The dragged objects as a set: the order the selection was made in
    /// must not invalidate the picture, and the render asks membership.
    preview: Option<(std::collections::HashSet<ShapeId>, [f64; 6])>,
    wireframe: bool,
    simulate_overprints: bool,
    complex_effects: bool,
    proof: bool,
    focus: Option<(Vec<ShapeId>, u32)>,
    /// The Pixels view: the document pixels drawn (dpi, first column, top
    /// row).
    pixels: Option<(u64, i64, i64)>,
}

/// The document pixels that cover a screen rectangle: `w` by `h` pixels
/// whose top-left one is column `ix0`, row `iy1` (rows counted up from the
/// page's bottom edge, the row above `iy1` excluded), and where they land
/// on screen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PixelWindow {
    pub ix0: i64,
    pub iy1: i64,
    pub w: u32,
    pub h: u32,
    pub screen: Rect,
}

/// Largest side of the Pixels view texture.
const MAX_PIXELS: i64 = 8192;

/// The document pixels at `dpi` behind `rect`, when one of them spans at
/// least one physical screen pixel (`ppp` per point); below that the
/// normal render already shows them.
pub fn pixel_window(view: &View, rect: Rect, dpi: f64, ppp: f32) -> Option<PixelWindow> {
    if !dpi.is_finite() || dpi <= 0.0 || !view.zoom.is_finite() || view.zoom <= 0.0 {
        return None;
    }
    let k = dpi / 25.4;
    if (view.zoom * ppp) as f64 / k < 1.0 {
        return None;
    }
    let tl = view.to_page(rect.left_top());
    let br = view.to_page(rect.right_bottom());
    let ix0 = (tl.x * k).floor() as i64;
    let ix1 = (br.x * k).ceil() as i64;
    let iy0 = (br.y * k).floor() as i64;
    let iy1 = (tl.y * k).ceil() as i64;
    let (w, h) = (ix1 - ix0, iy1 - iy0);
    if w <= 0 || h <= 0 || w > MAX_PIXELS || h > MAX_PIXELS {
        return None;
    }
    let a = view.to_screen(tracedraw_core::geometry::Point::new(
        ix0 as f64 / k,
        iy1 as f64 / k,
    ));
    let b = view.to_screen(tracedraw_core::geometry::Point::new(
        ix1 as f64 / k,
        iy0 as f64 / k,
    ));
    Some(PixelWindow {
        ix0,
        iy1,
        w: w as u32,
        h: h as u32,
        screen: Rect::from_min_max(a, b),
    })
}

impl Raster {
    /// One move of a fill (or an outline colour) dragged in a colour
    /// picker: the selection's part of the page (`region`, page mm)
    /// rendered anew into the picture shown, which the next moves render
    /// into again. `None` when the render failed: the caller then
    /// renders the whole page as usual.
    #[allow(clippy::too_many_arguments)]
    fn live_frame(
        &mut self,
        doc: &Document,
        page: PageId,
        region: tracedraw_core::geometry::Rect,
        key: &Key,
        vt: ViewTransform,
        screen: Rect,
    ) -> Option<(TextureId, Rect)> {
        let (w, h) = key.size;
        let Some((x0, y0, rw, rh, sub)) = sub_view(vt, w, h, region) else {
            // The objects lie off the canvas: the picture shown already
            // is the page as it stands.
            self.key = Some(key.clone());
            return Some((self.texture.as_ref()?.id(), screen));
        };
        let opts = RenderOptions {
            width: rw,
            height: rh,
            view: sub,
            preview: None,
            wireframe: key.wireframe,
            simulate_overprints: key.simulate_overprints,
            complex_effects: key.complex_effects,
            anti_alias: true,
            draft: true,
            focus: None,
        };
        let lut = key.proof.then(|| {
            self.proof
                .get_or_insert_with(|| std::sync::Arc::new(ProofLut::new()))
                .clone()
        });
        let t = web_time::Instant::now();
        let pixmap = render_page(doc, page, &opts)?;
        let image = to_image(&pixmap, lut.as_deref());
        self.last_seconds = t.elapsed().as_secs_f64();
        self.renders += 1;
        let texture = self.texture.as_mut()?;
        texture.set_partial([x0 as usize, y0 as usize], image, TextureOptions::LINEAR);
        self.key = Some(key.clone());
        self.settle = None;
        Some((texture.id(), screen))
    }

    /// The colour drag is over: forget the key of the picture shown, and
    /// keep the picture itself, so the next frame renders the whole page
    /// once. The live region covers only what the fast path can prove
    /// the colour changes; a corner of a mitered join, a faint shadow
    /// tail or anything rarer the bounds miss can reach a little past
    /// it, and this draws it as the page stands (#3559).
    pub fn end_live_colour_drag(&mut self) {
        self.key = None;
    }

    /// Drop the cached texture so the next frame re-renders (and the
    /// proofed colours, which the colour settings decide).
    pub fn invalidate(&mut self) {
        self.key = None;
        self.levels.clear();
        self.proof = None;
        self.settle = None;
        self.zoom_step = None;
        // A render under way is for what no longer holds.
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.job = None;
            self.rested = None;
            self.backdrop = None;
            self.backdrop_job = None;
            // Freed on a thread of its own, as a render frees its copy.
            if let Some((_, d)) = self.kept.take() {
                std::thread::spawn(move || drop(d));
            }
        }
    }

    /// True once a picture has been rendered.
    pub fn has_picture(&self) -> bool {
        self.texture.is_some()
    }

    /// How many renders this picture has started.
    /// The part of the page the picture shows, page mm.
    #[cfg(test)]
    pub fn shown_part(&self) -> Option<tracedraw_core::geometry::Rect> {
        self.page_rect
    }

    pub fn render_count(&self) -> u64 {
        self.renders
    }

    /// True while a picture is on its way: a render running on its
    /// thread, or a moved view waiting to rest before it is rendered.
    #[cfg(test)]
    pub fn drawing(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        if self.job.is_some() {
            return true;
        }
        self.settle.is_some()
    }

    /// The zoom the picture shown was rendered at, for tests.
    #[cfg(test)]
    pub fn picture_zoom(&self) -> Option<f32> {
        self.texture.as_ref()?;
        self.key.as_ref().map(|k| k.zoom)
    }

    /// The drawing revision of the picture currently held in `texture`.
    pub fn shown_revision(&self) -> Option<u64> {
        self.texture.as_ref()?;
        self.key.as_ref().map(|k| k.revision)
    }

    /// True when `texture` already holds an up-to-date plain render of `page`
    /// at `revision` for `(view, rect, ppp, complex_effects)` with no job in
    /// flight, so [`Raster::texture`] will return it without reading `doc`.
    pub fn has_exact_plain_picture(
        &self,
        revision: u64,
        page: PageId,
        view: &View,
        rect: Rect,
        ppp: f32,
        complex_effects: bool,
    ) -> bool {
        if self.texture.is_none() {
            return false;
        }
        #[cfg(not(target_arch = "wasm32"))]
        if self.job.is_some() {
            return false;
        }
        let w = (rect.width() * ppp).round().max(1.0) as u32;
        let h = (rect.height() * ppp).round().max(1.0) as u32;
        let want = Key {
            revision,
            page,
            zoom: view.zoom,
            offset: view.origin - rect.min,
            size: (w, h),
            preview: None,
            wireframe: false,
            simulate_overprints: false,
            complex_effects,
            proof: false,
            focus: None,
            pixels: None,
        };
        self.key.as_ref() == Some(&want)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn texture(
        &mut self,
        ctx: &Context,
        doc: &Document,
        page: PageId,
        view: &View,
        rect: Rect,
        preview: Option<(Vec<ShapeId>, Affine)>,
        revision: u64,
        wireframe: bool,
        simulate_overprints: bool,
        complex_effects: bool,
        proof: bool,
        focus: Option<(Vec<ShapeId>, f32)>,
        pixel_dpi: Option<f64>,
    ) -> Option<(TextureId, Rect)> {
        let ppp = ctx.pixels_per_point();
        // The picture is drawn for the view the wheel's glide lands at,
        // not for the view on its way to it: one render a turn, not one
        // for a zoom the glide has already passed. The Pixels view is
        // drawn at one render pixel per document pixel, and where it is
        // drawn is the view; no aim there.
        let zoom_target = self.zoom_target.take();
        let key_view = if pixel_dpi.is_some() {
            *view
        } else {
            zoom_target.unwrap_or(*view)
        };
        let window = pixel_dpi.and_then(|dpi| pixel_window(&key_view, rect, dpi, ppp));
        let (w, h) = match window {
            Some(win) => (win.w, win.h),
            None => (
                (rect.width() * ppp).round().max(1.0) as u32,
                (rect.height() * ppp).round().max(1.0) as u32,
            ),
        };
        let screen = window.map_or(rect, |win| win.screen);
        let key = Key {
            revision,
            page,
            zoom: key_view.zoom,
            offset: key_view.origin - rect.min,
            size: (w, h),
            preview: preview
                .as_ref()
                .map(|(s, t)| (s.iter().copied().collect(), t.as_coeffs())),
            wireframe,
            simulate_overprints,
            complex_effects,
            proof,
            focus: focus
                .as_ref()
                .map(|(ids, v)| (ids.clone(), (v * 1000.0) as u32)),
            pixels: window
                .zip(pixel_dpi)
                .map(|(win, dpi)| ((dpi * 1000.0).round() as u64, win.ix0, win.iy1)),
        };
        let vt = match (window, pixel_dpi) {
            // One render pixel per document pixel, the window's top-left
            // one at the corner.
            (Some(win), Some(dpi)) => ViewTransform {
                zoom: dpi / 25.4,
                origin_x: -(win.ix0 as f64),
                origin_y: win.iy1 as f64,
                stretch_y: 1.0,
            },
            _ => ViewTransform {
                zoom: (key_view.zoom * ppp) as f64,
                origin_x: ((key_view.origin.x - rect.min.x) * ppp) as f64,
                origin_y: ((key_view.origin.y - rect.min.y) * ppp) as f64,
                stretch_y: 1.0,
            },
        };
        // A fill or an outline colour dragged in a colour picker changes
        // only the selection's look: the part of the page it covers is
        // rendered anew into the picture shown, the rest of which still
        // is the page as it stands (each move only takes the move before
        // back and colours the same objects again, #3559). Skipped while
        // the picture shown is of another view or drawing state: the
        // usual render brings it up to date first.
        let live = self.live_region.filter(|r| {
            window.is_none()
                && key.preview.is_none()
                && key.focus.is_none()
                && !self.follows_zoom
                && !self.defer
                && r.is_finite()
                && self.texture.is_some()
                && self
                    .key
                    .as_ref()
                    .is_some_and(|k| k.same_but_revision(&key))
        });
        if let Some(region) = live {
            // A render of the whole page still on its thread is of a
            // state the drag has left behind: its picture would paint
            // over the region rendered here.
            #[cfg(not(target_arch = "wasm32"))]
            if let Some(job) = self.job.take() {
                job.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            if let Some(out) = self.live_frame(doc, page, region, &key, vt, screen) {
                return Some(out);
            }
        }
        // Rested only while the picture stays the one asked for.
        #[cfg(not(target_arch = "wasm32"))]
        let rested = self.rested.take();
        // A render on its thread: taken when ready; while it runs for the
        // view asked for (or the drawing changes faster than it draws), the
        // last picture shows, moved into place.
        #[cfg(not(target_arch = "wasm32"))]
        if self.poll_job(ctx) {
            let now = ctx.input(|i| i.time);
            // The view keeps zooming, a step every few frames: the running
            // render keeps its turn and is not given up, or every render
            // would give up before any of them is done, and the one for
            // the zoom the wheel stopped at would start over from nothing.
            // Each step re-arms the wait, so a wheel that goes on never
            // rests it.
            let stale_zoom = self.job.as_ref().is_some_and(|job| {
                job.key != key
                    && job.key.same_but_view(&key, self.follows_zoom)
                    && self.zooms_at_once(&job.key, &key)
            });
            // A render for an earlier revision of the main canvas is
            // obsolete too (#3903): letting it run to completion wastes
            // CPU while holding both the render turn and the old document
            // snapshot.
            if let Some(job) = &self.job {
                if !self.follows_zoom && job.key.revision != key.revision {
                    job.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            }
            if stale_zoom {
                let due = match &self.settle {
                    Some((k, due)) if *k == key => *due,
                    _ => {
                        // The wait follows the wheel's own pace, the way
                        // the glide's length does: notches a beat apart
                        // wait a few frames, so a wheel that has stopped
                        // is told from one that goes on soon; slower
                        // turns wait longer, and the render in flight
                        // lands between the notches all the same.
                        let gap = self.zoom_step.take().map_or(0.0, |t| (now - t).max(0.0));
                        self.zoom_step = Some(now);
                        now + (gap * ZOOM_SETTLE_PACE).clamp(ZOOM_SETTLE_MIN, SETTLE_SECONDS)
                    }
                };
                if now < due {
                    self.settle = Some((key, due));
                    ctx.request_repaint_after(std::time::Duration::from_secs_f64(due - now));
                } else {
                    // The view has settled: a render for a zoom it has
                    // already left. Nobody will see its picture, and the
                    // one for this zoom would wait for all of it, so a
                    // large drawing stayed blurry for two whole renders
                    // after each zoom step (#2522). It gives up at its
                    // next object; the scaled picture shows meanwhile, and
                    // the settled zoom's render starts when it has.
                    self.zoom_step = None;
                    if let Some(job) = &self.job {
                        job.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                }
                return self.shown_meanwhile(view, screen);
            }
            let running_for_this = self.job.as_ref().is_some_and(|j| j.key == key);
            if running_for_this || window.is_none() {
                return self.shown_meanwhile(view, screen);
            }
        }
        // Back at a zoom the view showed: its kept picture at once, no
        // render (#3894).
        if window.is_none()
            && !self.follows_zoom
            && self.key.as_ref() != Some(&key)
            && self.back_to_level(&key)
        {
            #[cfg(not(target_arch = "wasm32"))]
            if let Some(job) = self.job.take() {
                job.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        }
        if self.key.as_ref() == Some(&key) {
            if let Some(t) = &self.texture {
                self.settle = None;
                self.zoom_step = None;
                #[cfg(not(target_arch = "wasm32"))]
                {
                    self.rested = rested.or_else(|| Some(ctx.input(|i| i.time)));
                }
                // The picture of the view the glide lands at, shown while
                // the view is still on its way to it: placed by the view
                // the canvas shows, filling the window exactly once the
                // view has landed on it.
                let carried = zoom_target.is_some()
                    && (key_view.zoom != view.zoom || key_view.origin != view.origin);
                let at = if carried {
                    self.page_rect.map_or(screen, |r| view.rect_to_screen(r))
                } else {
                    screen
                };
                return Some((t.id(), at));
            }
        }
        // The view moved (pan, scroll, zoom) and nothing else changed:
        // show the last render moved into place until the view rests. A
        // pan shows the old picture as sharp as it was; a zoom scales it,
        // blurry, so the main canvas renders the new zoom at once on a
        // thread. One render at a time: one still running for an earlier
        // step of the zoom keeps its turn (waited for above); once the
        // turn is free, the zoom the view is at goes next, without
        // waiting for the wheel to rest (with the turn idle, the wait
        // would only keep the next picture from ever coming); and a
        // render running when the wheel has stopped, for a zoom the view
        // has left, is asked to give up (above), the settled zoom going
        // next.
        let mut zoom_now = false;
        if window.is_none() {
            if let (Some(old), Some(t), Some(page_rect)) =
                (self.key.as_ref(), self.texture.as_ref(), self.page_rect)
            {
                let moved = old.same_but_view(&key, self.follows_zoom);
                zoom_now = moved && self.zooms_at_once(old, &key);
                if moved && !zoom_now {
                    let now = ctx.input(|i| i.time);
                    let due = match &self.settle {
                        Some((k, due)) if *k == key => *due,
                        _ => {
                            now + if self.light_zoom && old.zoom != key.zoom {
                                LIGHT_SETTLE_SECONDS
                            } else {
                                SETTLE_SECONDS
                            }
                        }
                    };
                    if now < due {
                        self.settle = Some((key, due));
                        ctx.request_repaint_after(std::time::Duration::from_secs_f64(due - now));
                        return Some((t.id(), view.rect_to_screen(page_rect)));
                    }
                }
            }
        }
        // The browser draws on its one thread: never set there; the
        // program draws on a thread whatever the zoom (#3440).
        #[cfg(any(target_arch = "wasm32", not(test)))]
        let _ = zoom_now;
        if self.defer {
            ctx.request_repaint();
            let t = self.texture.as_ref()?;
            let page_rect = self.page_rect?;
            return Some((t.id(), view.rect_to_screen(page_rect)));
        }
        self.settle = None;
        self.zoom_step = None;
        let opts = RenderOptions {
            width: w,
            height: h,
            view: vt,
            preview: preview.map(|(shapes, transform)| Preview {
                shapes: shapes.into_iter().collect(),
                transform,
            }),
            wireframe,
            simulate_overprints,
            complex_effects,
            anti_alias: true,
            draft: true,
            focus,
        };
        let lut = proof.then(|| {
            self.proof
                .get_or_insert_with(|| std::sync::Arc::new(ProofLut::new()))
                .clone()
        });
        let picture_rect = tracedraw_core::geometry::Rect::new(
            -vt.origin_x / vt.zoom,
            (vt.origin_y - h as f64) / vt.zoom,
            (w as f64 - vt.origin_x) / vt.zoom,
            vt.origin_y / vt.zoom,
        );
        let page_rect = window.is_none().then_some(picture_rect);
        // Document pixels are blown up as blocks, the way they will look.
        let filter = if window.is_some() {
            TextureOptions::NEAREST
        } else {
            TextureOptions::LINEAR
        };
        // With a picture to show meanwhile, the page is drawn on a
        // thread of its own, the window answering all along: every
        // change and every step of the Pixels view too, not only a page
        // that drew slowly, which a quick page that a change made slow
        // (and every edit of a page just under that) held the frame for
        // (#3440). A page beside the current one always was: many of
        // them come due in the same moment, once a zoom rests (#903).
        #[cfg(test)]
        if self.always_slow {
            self.last_seconds = 1.0;
        }
        #[cfg(test)]
        let threaded = self.threaded
            || (window.is_none()
                && (self.last_seconds > SLOW_RENDER || self.follows_zoom || zoom_now));
        #[cfg(not(test))]
        let threaded = true;
        #[cfg(not(target_arch = "wasm32"))]
        if threaded && self.texture.is_some() {
            // The Pixels view's picture shows where its document pixels
            // now are while the next one is drawn.
            let page_rect = Some(picture_rect);
            // Another page's job (or a stale one) is running: the last
            // picture shows meanwhile, and the job holding the turn
            // repaints when it ends. A page beside the current one
            // yields to it.
            let turns = self.turns.unwrap_or_else(program_turns);
            let Some(turn) = turns.take(!self.follows_zoom) else {
                if let Some(wait) = turns.retry_after() {
                    ctx.request_repaint_after(wait);
                }
                return self.shown_meanwhile(view, screen);
            };
            let (doc, stale) = match self.snapshot.clone() {
                Some(shared) => (shared, None),
                None => self.copy_of(doc, key.revision),
            };
            let (tx, rx) = std::sync::mpsc::channel();
            let wake = ctx.clone();
            #[cfg(test)]
            let hold = self.hold.clone();
            let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let gives_up = cancel.clone();
            // The main canvas's picture comes a band at a time, the middle
            // of the window first, laid over the last one as it comes
            // (#2389); the pages beside it come whole. Only over a picture
            // of the same drawing (the view moved): over one from before a
            // change, the bands showed two drawings at once, cut along the
            // bands' edges, so a shadow being dragged flickered in pieces
            // as each step of the drag came (#3562).
            let same_drawing = self
                .key
                .as_ref()
                .is_some_and(|old| old.same_but_view(&key, true));
            let (bands_tx, bands) = if self.follows_zoom || !same_drawing {
                (None, None)
            } else {
                let (t, r) = std::sync::mpsc::channel();
                (Some(t), Some(r))
            };
            std::thread::spawn(move || {
                #[cfg(test)]
                let _held = hold.as_ref().map(|m| m.lock());
                let t = web_time::Instant::now();
                let image = match &bands_tx {
                    Some(bands_tx) => {
                        let band_wake = wake.clone();
                        let lut = lut.as_deref();
                        let band = move |y0: u32, rows: &tiny_skia::Pixmap| {
                            if bands_tx.send((y0, to_image(rows, lut))).is_ok() {
                                band_wake.request_repaint();
                            }
                        };
                        render_page_progressive(&doc, page, &opts, &gives_up, h / 2, &band)
                    }
                    None => render_page_cancellable(&doc, page, &opts, &gives_up),
                }
                .map(|pm| to_image(&pm, lut.as_deref()));
                let _ = tx.send(image.map(|i| (i, t.elapsed().as_secs_f64())));
                drop(doc);
                drop(stale);
                drop(turn);
                wake.request_repaint();
            });
            self.renders += 1;
            self.job = Some(Job {
                key,
                page_rect,
                rx,
                cancel,
                bands,
                incoming: None,
                arrived: Vec::new(),
                size: (w, h),
                filter,
            });
            return self.shown_meanwhile(view, screen);
        }
        let t = web_time::Instant::now();
        self.renders += 1;
        let pixmap = render_page(doc, page, &opts)?;
        let image = to_image(&pixmap, lut.as_deref());
        self.last_seconds = t.elapsed().as_secs_f64();
        self.take_picture(ctx, image, filter, key, page_rect);
        self.texture.as_ref().map(|t| (t.id(), screen))
    }

    /// The whole page drawn small, for the parts of the canvas the page's
    /// picture does not reach while the view moves (#2048). The picture
    /// covers the canvas only, the part of the page on screen when it was
    /// rendered: zoomed out, or panned, it is shown scaled or shifted into
    /// place until the new one is ready, and the rest of the page showed
    /// as blank paper meanwhile, a white flash on a dark design at every
    /// zoom step. The backdrop is `area` (page mm: the page and what is drawn
    /// beside it, see `App::backdrop_area`) fitted into
    /// the canvas `rect`, at most [`BACKDROP_MAX`] pixels a side: one more
    /// texture no larger than the picture's own, whatever the zoom.
    ///
    /// It is drawn on one thread of its own (not in bands on every core
    /// as a picture somebody waits for is), once per change of the
    /// drawing, after the picture has been the one asked for during
    /// [`BACKDROP_REST`] seconds (so never while the drawing changes or
    /// the view moves) and once the pointer is still with no button or
    /// key held: the drawing is copied for the thread in that frame,
    /// which on a large drawing takes a frame's time nobody then sees.
    /// It takes no render turn: a zoom that starts meanwhile renders at
    /// once all the same. Returns the backdrop of
    /// the drawing as it is now and where it shows on screen, or `None`
    /// while there is none (a backdrop of an earlier state is never
    /// shown: it would bring back what was deleted).
    ///
    /// Call it after [`Raster::texture`], every frame.
    #[cfg(not(target_arch = "wasm32"))]
    #[allow(clippy::too_many_arguments)]
    pub fn backdrop(
        &mut self,
        ctx: &Context,
        doc: &Document,
        page: PageId,
        area: tracedraw_core::geometry::Rect,
        view: &View,
        rect: Rect,
        revision: u64,
        wireframe: bool,
        simulate_overprints: bool,
        complex_effects: bool,
        proof: bool,
    ) -> Option<(TextureId, Rect)> {
        // The one on its thread: taken when ready.
        if let Some(job) = &self.backdrop_job {
            match job.rx.try_recv() {
                Ok(image) => {
                    let job = self.backdrop_job.take()?;
                    if let Some(image) = image {
                        let texture = match self.backdrop.take() {
                            Some(mut b) => {
                                b.texture.set(image, TextureOptions::LINEAR);
                                b.texture
                            }
                            None => {
                                ctx.load_texture("page backdrop", image, TextureOptions::LINEAR)
                            }
                        };
                        self.backdrop = Some(Backdrop {
                            texture,
                            key: job.key,
                            page_rect: job.page_rect,
                        });
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.backdrop_job = None,
            }
        }
        let ppp = ctx.pixels_per_point();
        let (aw, ah) = (area.width(), area.height());
        if !(aw.is_finite() && ah.is_finite() && aw > 0.0 && ah > 0.0) {
            return None;
        }
        // The page fitted into the canvas, as Zoom To Page shows it.
        let scale = ((rect.width() * ppp) as f64 / aw)
            .min((rect.height() * ppp) as f64 / ah)
            .min(BACKDROP_MAX / aw.max(ah));
        if !(scale.is_finite() && scale > 0.0) {
            return None;
        }
        let w = (aw * scale).ceil().max(1.0) as u32;
        let h = (ah * scale).ceil().max(1.0) as u32;
        let q = |v: f64| (v * 1000.0).round() as i64;
        let key = BackKey {
            revision,
            page,
            area: [q(area.x0), q(area.y0), q(area.x1), q(area.y1)],
            size: (w, h),
            wireframe,
            simulate_overprints,
            complex_effects,
            proof,
        };
        let have = self.backdrop.as_ref().is_some_and(|b| b.key == key);
        if !have && self.backdrop_job.is_none() {
            let now = ctx.input(|i| i.time);
            // Not in a frame the hand is moving in: the copy of the
            // drawing made below may take that frame's time. (The
            // pointer's speed reads still for the first frames of a
            // move, so a move in this very frame counts too.)
            let idle = ctx.input(|i| {
                i.pointer.is_still()
                    && i.pointer.time_since_last_movement() > 0.0
                    && !i.pointer.any_down()
                    && i.keys_down.is_empty()
            });
            match self.rested {
                Some(since) if now - since >= BACKDROP_REST && !idle => {
                    ctx.request_repaint_after(std::time::Duration::from_secs_f64(BACKDROP_IDLE));
                }
                Some(since) if now - since >= BACKDROP_REST => {
                    let opts = RenderOptions {
                        width: w,
                        height: h,
                        view: ViewTransform {
                            zoom: scale,
                            origin_x: -area.x0 * scale,
                            origin_y: area.y1 * scale,
                            stretch_y: 1.0,
                        },
                        preview: None,
                        wireframe,
                        simulate_overprints,
                        complex_effects,
                        anti_alias: true,
                        draft: true,
                        focus: None,
                    };
                    let page_rect = tracedraw_core::geometry::Rect::new(
                        area.x0,
                        area.y1 - h as f64 / scale,
                        area.x0 + w as f64 / scale,
                        area.y1,
                    );
                    let lut = proof.then(|| {
                        self.proof
                            .get_or_insert_with(|| std::sync::Arc::new(ProofLut::new()))
                            .clone()
                    });
                    // The drawing shared by the caller, or the copy a
                    // zoom's renders share: made here, at rest, it is
                    // there for the first step of the next zoom.
                    let (doc, stale) = match self.snapshot.clone() {
                        Some(shared) => (shared, None),
                        None => self.copy_of(doc, revision),
                    };
                    let (tx, rx) = std::sync::mpsc::channel();
                    let wake = ctx.clone();
                    std::thread::spawn(move || {
                        let image = render_page_on_one_thread(&doc, page, &opts)
                            .map(|pm| to_image(&pm, lut.as_deref()));
                        let _ = tx.send(image);
                        drop(doc);
                        drop(stale);
                        wake.request_repaint();
                    });
                    self.backdrops += 1;
                    self.backdrop_job = Some(BackJob { key, page_rect, rx });
                    return None;
                }
                Some(since) => {
                    let wait = (BACKDROP_REST - (now - since)).max(0.0);
                    ctx.request_repaint_after(std::time::Duration::from_secs_f64(wait));
                }
                None => {}
            }
        }
        let b = self.backdrop.as_ref().filter(|b| b.key == key)?;
        Some((b.texture.id(), view.rect_to_screen(b.page_rect)))
    }

    /// True once the backdrop of the drawing at `revision` is there.
    #[cfg(all(test, not(target_arch = "wasm32")))]
    pub fn has_backdrop(&self, revision: u64) -> bool {
        self.backdrop
            .as_ref()
            .is_some_and(|b| b.key.revision == revision)
    }

    /// True while a backdrop is being drawn on its thread, for tests.
    #[cfg(all(test, not(target_arch = "wasm32")))]
    pub fn backdrop_drawing(&self) -> bool {
        self.backdrop_job.is_some()
    }

    /// The backdrop's texture, for tests.
    #[cfg(all(test, not(target_arch = "wasm32")))]
    pub fn backdrop_texture(&self) -> Option<TextureId> {
        self.backdrop.as_ref().map(|b| b.texture.id())
    }

    /// The drawing at `revision` for a render on a thread: the copy kept
    /// from the last render on a thread when it is of the same revision,
    /// or a new copy, kept for the next renders of the same drawing (the
    /// steps of a zoom, every picture of a drag). Also the copy no longer
    /// kept, for the render's thread to free (freeing a large drawing
    /// takes time the window's thread does not have). A drag on a large
    /// drawing copied all of it again for every picture of the drag, on
    /// the window's thread, so the object stalled before it followed the
    /// pointer (#2522).
    #[cfg(not(target_arch = "wasm32"))]
    fn copy_of(
        &mut self,
        doc: &Document,
        revision: u64,
    ) -> (std::sync::Arc<Document>, Option<std::sync::Arc<Document>>) {
        match &self.kept {
            Some((r, d)) if *r == revision => (d.clone(), None),
            _ => {
                let d = std::sync::Arc::new(doc.clone());
                let keep = Some((revision, d.clone()));
                let stale = std::mem::replace(&mut self.kept, keep);
                (d, stale.map(|(_, d)| d))
            }
        }
    }

    /// The copy of the drawing kept for renders on a thread, with its
    /// revision, for other pictures of the same drawing to share.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn kept_copy(&self) -> Option<(u64, std::sync::Arc<Document>)> {
        self.kept.clone()
    }

    /// True when the view asked for (`new`) differs from the picture's
    /// (`old`) by its zoom and the picture is the main canvas's: rendered
    /// at once on a thread instead of shown scaled until the view rests.
    /// The pages beside the current one in the Multipage View keep waiting
    /// (many come due at once, #903), as does every zoom with Light zoom
    /// on, and the browser has no threads.
    fn zooms_at_once(&self, old: &Key, new: &Key) -> bool {
        cfg!(not(target_arch = "wasm32"))
            && !self.follows_zoom
            && !self.light_zoom
            && old.zoom != new.zoom
    }

    /// The bands of the picture being drawn on its thread that are done,
    /// on a see-through texture, and where it shows on screen: drawn
    /// over the last picture (shown scaled or moved into place
    /// meanwhile), so the window sharpens a band at a time instead of
    /// all at once at the end (#2389). `None` while no band has come, or
    /// for a render that gave up. With it, the parts of the screen the
    /// bands cover: the last picture must not show under them, or a
    /// see-through object there is drawn twice, darker, until the whole
    /// picture comes (#3639).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn incoming(&self, view: &View) -> Option<(TextureId, Rect, Vec<Rect>)> {
        let job = self.job.as_ref()?;
        if job.cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return None;
        }
        let tex = job.incoming.as_ref()?.id();
        let at = view.rect_to_screen(job.page_rect?);
        let covered = band_rects(at, job.size.1, &job.arrived);
        Some((tex, at, covered))
    }

    /// The last picture, where its part of the page now is on screen
    /// (while a new one is drawn on its thread).
    #[cfg(not(target_arch = "wasm32"))]
    fn shown_meanwhile(&self, view: &View, screen: Rect) -> Option<(TextureId, Rect)> {
        let t = self.texture.as_ref()?;
        // A kept picture of a zoom nearer the view's, covering what the
        // window shows, is sharper than the last one scaled (#3894).
        if let (Some(cur), Some(cur_rect)) = (self.key.as_ref(), self.page_rect) {
            if !self.levels.is_empty() && !self.follows_zoom {
                let a = view.to_page(screen.left_top());
                let b = view.to_page(screen.right_bottom());
                let visible = tracedraw_core::geometry::Rect::new(
                    a.x.min(b.x),
                    a.y.min(b.y),
                    a.x.max(b.x),
                    a.y.max(b.y),
                );
                let mut pictures = vec![(cur.zoom, cur_rect)];
                pictures.extend(self.levels.iter().map(|l| (l.key.zoom, l.page_rect)));
                if let Some(i) = nearest_level(&pictures, view.zoom, visible).filter(|i| *i > 0) {
                    let l = &self.levels[i - 1];
                    return Some((l.texture.id(), view.rect_to_screen(l.page_rect)));
                }
            }
        }
        Some((
            t.id(),
            self.page_rect.map_or(screen, |r| view.rect_to_screen(r)),
        ))
    }

    /// Take `image`, the picture of `key`: the picture it replaces is
    /// kept as a level when it is of another zoom of the drawing as it
    /// stands, and levels of anything else go (#3894).
    fn take_picture(
        &mut self,
        ctx: &Context,
        image: ColorImage,
        filter: TextureOptions,
        key: Key,
        page_rect: Option<tracedraw_core::geometry::Rect>,
    ) {
        let keep = cfg!(not(target_arch = "wasm32"))
            && !self.follows_zoom
            && page_rect.is_some()
            && self
                .key
                .as_ref()
                .is_some_and(|old| old.zoom != key.zoom && old.same_but_view(&key, false));
        if keep && self.texture.is_some() {
            let old = self.texture.take().expect("texture");
            if let (Some(k), Some(r)) = (self.key.take(), self.page_rect) {
                self.levels.push(Level {
                    key: k,
                    texture: old,
                    page_rect: r,
                });
            }
            self.texture = Some(ctx.load_texture("page", image, filter));
        } else if let Some(t) = &mut self.texture {
            t.set(image, filter);
        } else {
            self.texture = Some(ctx.load_texture("page", image, filter));
        }
        self.levels
            .retain(|l| l.key.zoom != key.zoom && l.key.same_but_view(&key, false));
        // The oldest go first, past the count or the bytes.
        let bytes = |l: &Level| {
            let [w, h] = l.texture.size();
            w * h * 4
        };
        while self.levels.len() > MAX_LEVELS
            || self.levels.iter().map(bytes).sum::<usize>() > MAX_LEVEL_BYTES
        {
            self.levels.remove(0);
        }
        self.key = Some(key);
        self.page_rect = page_rect;
    }

    /// Let go of the pictures kept of other zooms, as when the view has
    /// shown no other (tests of the wait for a new picture).
    #[cfg(test)]
    pub fn forget_kept_zooms(&mut self) {
        self.levels.clear();
    }

    /// A kept picture of exactly the view asked for (the view came back
    /// to a zoom it showed): made the picture shown, the one it replaces
    /// kept in its place (#3894). True when there was one.
    fn back_to_level(&mut self, key: &Key) -> bool {
        let same = |k: &Key| {
            k.same_but_view(key, false)
                && (k.zoom / key.zoom - 1.0).abs() < 1e-6
                && (k.offset - key.offset).length() < 0.01
        };
        let Some(i) = self.levels.iter().position(|l| same(&l.key)) else {
            return false;
        };
        let level = self.levels.remove(i);
        if let (Some(t), Some(k), Some(r)) = (self.texture.take(), self.key.take(), self.page_rect)
        {
            if k.same_but_view(key, false) {
                self.levels.push(Level {
                    key: k,
                    texture: t,
                    page_rect: r,
                });
            }
        }
        self.texture = Some(level.texture);
        self.page_rect = Some(level.page_rect);
        self.key = Some(key.clone());
        true
    }

    /// A render on its thread: its picture taken when ready (`true` when
    /// it is still running).
    #[cfg(not(target_arch = "wasm32"))]
    fn poll_job(&mut self, ctx: &Context) -> bool {
        let Some(job) = &mut self.job else {
            return false;
        };
        job.take_bands(ctx);
        match job.rx.try_recv() {
            Ok(result) => {
                let job = self.job.take().expect("job");
                if let Some((image, seconds)) = result {
                    self.last_seconds = seconds;
                    self.take_picture(ctx, image, job.filter, job.key, job.page_rect);
                }
                false
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => true,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.job = None;
                false
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Job {
    /// The bands that came since the last frame, onto the texture of the
    /// bands so far.
    fn take_bands(&mut self, ctx: &Context) {
        let Some(rx) = &self.bands else {
            return;
        };
        let (w, h) = (self.size.0 as usize, self.size.1 as usize);
        // A texture over the graphics card's largest side cannot be
        // made: such a picture comes all at once at the end, as before.
        let side = ctx.input(|i| i.max_texture_side);
        if w > side || h > side {
            self.bands = None;
            return;
        }
        while let Ok((y0, rows)) = rx.try_recv() {
            let y0 = y0 as usize;
            if rows.size[0] != w || y0 + rows.size[1] > h {
                continue;
            }
            let filter = self.filter;
            let t = self.incoming.get_or_insert_with(|| {
                ctx.load_texture(
                    "page-bands",
                    ColorImage::filled([w, h], egui::Color32::TRANSPARENT),
                    filter,
                )
            });
            let y1 = y0 + rows.size[1];
            t.set_partial([0, y0], rows, filter);
            self.arrived.push((y0 as u32, y1 as u32));
        }
    }
}

/// Where rows of a picture `h` pixels tall, shown at `at`, fall on
/// screen: one rectangle the picture's width per run of rows (runs that
/// touch made one).
#[cfg(not(target_arch = "wasm32"))]
fn band_rects(at: Rect, h: u32, rows: &[(u32, u32)]) -> Vec<Rect> {
    if h == 0 {
        return Vec::new();
    }
    let mut runs: Vec<(u32, u32)> = rows.iter().copied().filter(|r| r.1 > r.0).collect();
    runs.sort_unstable();
    let mut merged: Vec<(u32, u32)> = Vec::new();
    for (a, b) in runs {
        match merged.last_mut() {
            Some(last) if a <= last.1 => last.1 = last.1.max(b),
            _ => merged.push((a, b)),
        }
    }
    let y = |row: u32| at.top() + at.height() * (row.min(h) as f32 / h as f32);
    merged
        .into_iter()
        .map(|(a, b)| Rect::from_min_max(egui::pos2(at.left(), y(a)), egui::pos2(at.right(), y(b))))
        .collect()
}

/// The pixels of a `w` by `h` picture rendered with `vt` that the page
/// rectangle `region` (page mm) covers, widened to whole pixels: their
/// place in the picture, their size, and the view that renders exactly
/// them. `None` when the region lies off the picture.
fn sub_view(
    vt: ViewTransform,
    w: u32,
    h: u32,
    region: tracedraw_core::geometry::Rect,
) -> Option<(u32, u32, u32, u32, ViewTransform)> {
    // The picture's y runs down, the page's up.
    let (x0, x1) = (
        (vt.zoom * region.x0 + vt.origin_x).floor().clamp(0.0, w as f64),
        (vt.zoom * region.x1 + vt.origin_x).ceil().clamp(0.0, w as f64),
    );
    let (y0, y1) = (
        (vt.origin_y - vt.zoom * region.y1)
            .floor()
            .clamp(0.0, h as f64),
        (vt.origin_y - vt.zoom * region.y0)
            .ceil()
            .clamp(0.0, h as f64),
    );
    let (x0, x1, y0, y1) = (x0 as u32, x1 as u32, y0 as u32, y1 as u32);
    (x1 > x0 && y1 > y0).then(|| {
        (
            x0,
            y0,
            x1 - x0,
            y1 - y0,
            ViewTransform {
                zoom: vt.zoom,
                origin_x: vt.origin_x - x0 as f64,
                origin_y: vt.origin_y - y0 as f64,
                stretch_y: vt.stretch_y,
            },
        )
    })
}

impl Key {
    /// True when `other` asks for the same page shown the same way, only
    /// the drawing's revision moved on: the steps a colour dragged in a
    /// picker makes and takes back leave the picture's every pixel but
    /// the selection's part as it is (#3559).
    fn same_but_revision(&self, other: &Key) -> bool {
        self.page == other.page
            && self.zoom == other.zoom
            && self.offset == other.offset
            && self.size == other.size
            && self.preview == other.preview
            && self.wireframe == other.wireframe
            && self.simulate_overprints == other.simulate_overprints
            && self.complex_effects == other.complex_effects
            && self.proof == other.proof
            && self.focus == other.focus
            && self.pixels == other.pixels
    }
}

impl Key {
    /// True when `other` shows the same drawing the same way, only from
    /// another place or at another zoom (the canvas the same size, unless
    /// `any_size`).
    fn same_but_view(&self, other: &Key, any_size: bool) -> bool {
        self.revision == other.revision
            && self.page == other.page
            && (any_size || self.size == other.size)
            && self.preview == other.preview
            && self.wireframe == other.wireframe
            && self.simulate_overprints == other.simulate_overprints
            && self.complex_effects == other.complex_effects
            && self.proof == other.proof
            && self.focus == other.focus
            && self.pixels.is_none()
            && other.pixels.is_none()
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    /// The bands so far cover the rows they hold on screen, runs that
    /// touch as one, so the last picture is left out just there (#3639).
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn the_bands_so_far_cover_their_rows_on_screen() {
        let at = Rect::from_min_max(egui::pos2(10.0, 100.0), egui::pos2(210.0, 300.0));
        // A 100-row picture shown 200 points tall: rows 0-25 and 25-50
        // touch; 75-100 stands apart; an empty band covers nothing.
        let r = band_rects(at, 100, &[(75, 100), (0, 25), (25, 50), (60, 60)]);
        assert_eq!(r.len(), 2);
        assert_eq!(
            r[0],
            Rect::from_min_max(egui::pos2(10.0, 100.0), egui::pos2(210.0, 200.0))
        );
        assert_eq!(
            r[1],
            Rect::from_min_max(egui::pos2(10.0, 250.0), egui::pos2(210.0, 300.0))
        );
        assert!(band_rects(at, 0, &[(0, 10)]).is_empty());
        // What is left of the last picture is what no band covers yet.
        let left = crate::canvas::outside(at, &r);
        assert_eq!(
            left,
            vec![Rect::from_min_max(
                egui::pos2(10.0, 200.0),
                egui::pos2(210.0, 250.0)
            )]
        );
    }

    /// One page job at a time, and the current page first: another page
    /// that asks first every frame waits while the turn is kept for the
    /// current page.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn one_render_job_runs_at_a_time_and_the_current_page_goes_first() {
        static TURNS: Turns = Turns::new();
        let job = TURNS.take(false).expect("free");
        assert!(TURNS.take(false).is_none());
        // The current page asks while the job runs, and so waits.
        assert!(TURNS.take(true).is_none());
        std::thread::spawn(move || drop(job)).join().unwrap();
        // Freed: another page asking first does not get it.
        assert!(TURNS.take(false).is_none());
        let current = TURNS.take(true).expect("kept for the current page");
        drop(current);
        // Nobody waits: whoever asks.
        assert!(TURNS.take(false).is_some());
    }

    /// Tests running at the same time do not share the program's turns:
    /// a job of another test holding its turn leaves this test's pages
    /// free to start theirs, while the pages of one test (one window)
    /// still take turns.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn a_test_has_render_turns_of_its_own() {
        let (held_tx, held_rx) = std::sync::mpsc::channel();
        let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
        let other = std::thread::spawn(move || {
            let turn = program_turns().take(true).expect("free");
            held_tx.send(()).expect("sent");
            let _ = done_rx.recv();
            drop(turn);
        });
        held_rx.recv().expect("held");
        let mine = program_turns()
            .take(true)
            .expect("not held by the other test");
        assert!(
            program_turns().take(false).is_none(),
            "one job at a time in this test"
        );
        drop(mine);
        done_tx.send(()).expect("sent");
        other.join().expect("joined");
    }

    /// Two slow pages drawn every frame, the other page asked first and
    /// the drawing changing each frame: the current page's render starts
    /// within one release of the turn.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn the_current_page_gets_the_next_turn() {
        static TURNS: Turns = Turns::new();
        let ctx = Context::default();
        let doc = Document::new("t", Size::new(100.0, 50.0));
        let rect = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(320.0, 200.0));
        let view = View::default();
        let mut current = Raster {
            turns: Some(&TURNS),
            ..Default::default()
        };
        let mut other = Raster {
            turns: Some(&TURNS),
            follows_zoom: true,
            ..Default::default()
        };
        let draw = |r: &mut Raster, revision: u64| {
            let page = doc.pages[0].id;
            let input = egui::RawInput {
                time: Some(revision as f64),
                ..Default::default()
            };
            let mut full = ctx.run_ui(input, |ui| {
                r.texture(
                    ui.ctx(),
                    &doc,
                    page,
                    &view,
                    rect,
                    None,
                    revision,
                    false,
                    false,
                    false,
                    false,
                    None,
                    None,
                );
            });
            full.textures_delta.clear();
        };
        draw(&mut other, 0);
        draw(&mut current, 0);
        other.always_slow = true;
        current.always_slow = true;
        // The other page takes the turn first.
        draw(&mut other, 1);
        assert!(other.job.is_some());
        // From the first time the current page waits, no other page may
        // take a turn before it.
        let mut given = None;
        for revision in 2..2000 {
            draw(&mut other, revision);
            draw(&mut current, revision);
            if current.job.is_some() {
                break;
            }
            let st = TURNS.state();
            if st.current_waits && given.is_none() {
                given = Some(st.to_others);
            }
            if let Some(n) = given {
                assert_eq!(st.to_others, n, "another page went first");
            }
            drop(st);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(given.is_some(), "the current page had to wait");
        assert!(current.job.is_some(), "the current page got a turn");
    }
    use tracedraw_core::geometry::Size;

    /// A page beside the current one that is refused its turn only because
    /// the turn is kept for the current page, with no job running to wake
    /// the window when it ends: a repaint is asked for when the turn is
    /// free again, or the page keeps its old, blurry picture until the next
    /// pointer move or key press (#1384).
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn a_page_refused_a_kept_turn_asks_for_a_repaint() {
        static TURNS: Turns = Turns::new();
        let ctx = Context::default();
        let doc = Document::new("t", Size::new(100.0, 50.0));
        let page = doc.pages[0].id;
        let rect = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(320.0, 200.0));
        let view = View::default();
        let mut other = Raster {
            turns: Some(&TURNS),
            follows_zoom: true,
            ..Default::default()
        };
        // One frame at egui time `time`; how long egui may wait before the
        // next one.
        let draw = |r: &mut Raster, revision: u64, time: f64| {
            let input = egui::RawInput {
                time: Some(time),
                ..Default::default()
            };
            let mut full = ctx.run_ui(input, |ui| {
                r.texture(
                    ui.ctx(),
                    &doc,
                    page,
                    &view,
                    rect,
                    None,
                    revision,
                    false,
                    false,
                    false,
                    false,
                    None,
                    None,
                );
            });
            full.textures_delta.clear();
            full.viewport_output
                .get(&egui::ViewportId::ROOT)
                .map_or(std::time::Duration::MAX, |v| v.repaint_delay)
        };
        draw(&mut other, 0, 0.0);
        assert!(other.has_picture());
        // The frame after a texture is loaded repaints once on its own;
        // measure from the next one.
        draw(&mut other, 0, 0.5);
        // A job holds the turn and the current page asks for it meanwhile;
        // when the job ends the turn is kept for the current page.
        let job = TURNS.take(false).expect("free");
        assert!(TURNS.take(true).is_none());
        drop(job);
        // The drawing changed: the page wants a new render and is refused.
        let delay = draw(&mut other, 1, 1.0);
        assert!(other.job.is_none(), "the turn is kept for the current page");
        assert_eq!(other.render_count(), 1);
        assert!(
            delay <= KEEP_FOR_CURRENT,
            "no repaint asked for: waits {delay:?}"
        );
    }

    /// The region a colour drag renders: exactly the pixels of the whole
    /// page's picture the region covers, whole pixels at its edge
    /// (#3559).
    #[test]
    fn a_live_region_render_matches_the_full_pages_pixels() {
        use tracedraw_core::document::ShapeKind;
        use tracedraw_core::{Color, Fill, Shape};
        let mut doc = Document::new("t", Size::new(100.0, 50.0));
        let page = doc.pages[0].id;
        let layer = doc.pages[0].layers[0].id;
        let mut a = Shape::new(
            ShapeId(1),
            ShapeKind::Path {
                path: tracedraw_core::geometry::rect_path(
                    tracedraw_core::geometry::Rect::new(5.0, 3.0, 40.0, 30.0),
                    0.0,
                ),
                closed: true,
            },
        );
        a.fill = Fill::Solid(Color::rgb8(200, 30, 30));
        a.stroke = None;
        let mut b = Shape::new(
            ShapeId(2),
            ShapeKind::Path {
                path: tracedraw_core::geometry::rect_path(
                    tracedraw_core::geometry::Rect::new(30.0, 20.0, 60.0, 45.0),
                    0.0,
                ),
                closed: true,
            },
        );
        b.fill = Fill::Solid(Color::rgb8(30, 30, 200));
        b.stroke = None;
        doc.layer_mut(layer).unwrap().shapes.extend([a, b]);
        let vt = ViewTransform {
            zoom: 3.7,
            origin_x: 11.0,
            origin_y: 190.0,
            stretch_y: 1.0,
        };
        let opts = |width, height, view| RenderOptions {
            width,
            height,
            view,
            preview: None,
            wireframe: false,
            simulate_overprints: false,
            complex_effects: false,
            anti_alias: true,
            draft: true,
            focus: None,
        };
        let full = render_page(&doc, page, &opts(320, 200, vt)).expect("rendered");
        // A region part of it covered by a shape, part of it bare page.
        let region = tracedraw_core::geometry::Rect::new(5.2, 3.1, 45.7, 33.9);
        let (x0, y0, w, h, sub) = sub_view(vt, 320, 200, region).expect("on the page");
        let part = render_page(&doc, page, &opts(w, h, sub)).expect("rendered");
        assert_eq!(part.width(), w);
        assert_eq!(part.height(), h);
        for y in 0..h {
            for x in 0..w {
                let want = full.pixels()[(y0 + y) as usize * 320 + (x0 + x) as usize];
                let got = part.pixels()[(y * w + x) as usize];
                assert_eq!(got, want, "pixel ({x}, {y})");
            }
        }
        // Off the picture: nothing to render.
        let off = tracedraw_core::geometry::Rect::new(1000.0, 1000.0, 1010.0, 1010.0);
        assert!(sub_view(vt, 320, 200, off).is_none());
    }

    /// A colour dragged in a picker: every move renders only the
    /// selection's part of the page, into the picture shown, on the
    /// window's thread, without waiting for the view to rest or a render
    /// on a thread of its own (#3559).
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn a_colour_dragged_in_a_picker_renders_only_the_selections_part() {
        use tracedraw_core::document::ShapeKind;
        use tracedraw_core::{Color, Fill, Shape};
        static TURNS: Turns = Turns::new();
        let ctx = Context::default();
        let mut doc = Document::new("t", Size::new(100.0, 50.0));
        let page = doc.pages[0].id;
        let layer = doc.pages[0].layers[0].id;
        let mut a = Shape::new(
            ShapeId(1),
            ShapeKind::Path {
                path: tracedraw_core::geometry::rect_path(
                    tracedraw_core::geometry::Rect::new(10.0, 10.0, 40.0, 30.0),
                    0.0,
                ),
                closed: true,
            },
        );
        a.fill = Fill::Solid(Color::rgb8(200, 30, 30));
        a.stroke = None;
        doc.layer_mut(layer).unwrap().shapes.push(a);
        let rect = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(320.0, 200.0));
        let view = View {
            zoom: 4.0,
            origin: egui::pos2(10.0, 190.0),
        };
        let mut raster = Raster {
            turns: Some(&TURNS),
            ..Default::default()
        };
        let pass = |raster: &mut Raster, doc: &Document, revision: u64| {
            let mut out = None;
            let mut full = ctx.run_ui(egui::RawInput::default(), |ui| {
                out = raster.texture(
                    ui.ctx(),
                    doc,
                    page,
                    &view,
                    rect,
                    None,
                    revision,
                    false,
                    false,
                    false,
                    false,
                    None,
                    None,
                );
            });
            full.textures_delta.clear();
            out.expect("texture")
        };
        let (first, _) = pass(&mut raster, &doc, 1);
        assert_eq!(raster.render_count(), 1);
        // The drag's moves: each colours the shape again and renders only
        // the shape's part of the page.
        raster.live_region = Some(tracedraw_core::geometry::Rect::new(5.0, 5.0, 45.0, 35.0));
        for (revision, c) in [(2, (30, 200, 30)), (3, (30, 30, 200))] {
            doc.layer_mut(layer).unwrap().shapes[0].fill =
                Fill::Solid(Color::rgb8(c.0, c.1, c.2));
            let (tex, at) = pass(&mut raster, &doc, revision);
            assert_eq!(tex, first, "the picture shown, rendered into");
            assert_eq!(at, rect);
            assert!(raster.job.is_none(), "no render on a thread of its own");
            assert!(raster.settle.is_none(), "no waiting for the view to rest");
            assert_eq!(
                raster.key.as_ref().map(|k| k.revision),
                Some(revision),
                "the picture is of the drawing as it stands"
            );
        }
        assert_eq!(raster.render_count(), 3, "one render a move");
        // A move whose objects lie off the canvas renders nothing.
        raster.live_region =
            Some(tracedraw_core::geometry::Rect::new(1000.0, 1000.0, 1010.0, 1010.0));
        pass(&mut raster, &doc, 4);
        assert_eq!(raster.render_count(), 3);
        assert_eq!(raster.key.as_ref().map(|k| k.revision), Some(4));
        // The drag over: the raster alone takes the picture as it is;
        // the canvas then calls end_live_colour_drag, and the next frame
        // renders the whole page once, so nothing the fast path could
        // have missed (a corner of a mitered join, a faint shadow tail)
        // stays stale (#3559).
        raster.live_region = None;
        let (tex, _) = pass(&mut raster, &doc, 4);
        assert_eq!(tex, first);
        assert_eq!(raster.render_count(), 4 - 1);
        raster.end_live_colour_drag();
        let (_, _) = pass(&mut raster, &doc, 4);
        assert_eq!(raster.render_count(), 4, "one whole-page render at the end");
        assert_eq!(raster.key.as_ref().map(|k| k.revision), Some(4));
    }

    #[test]
    fn proofed_pixels_follow_the_grid_of_proofed_colours() {
        let lut = ProofLut::new();
        // On the grid, the proofed colour itself.
        let blue = lut.apply([0.0, 0.0, 1.0]);
        let want = tracedraw_core::color::proof_rgb([0.0, 0.0, 1.0]);
        for (a, b) in blue.iter().zip(want) {
            assert!((a - b).abs() < 1e-5);
        }
        // Between, close to the colour proofed directly.
        let c = [0.31, 0.62, 0.18];
        let (got, want) = (lut.apply(c), tracedraw_core::color::proof_rgb(c));
        for (a, b) in got.iter().zip(want) {
            assert!((a - b).abs() < 0.03, "{got:?} {want:?}");
        }
        // Transparent stays transparent; half-covered pixels keep their
        // coverage.
        assert_eq!(lut.pixel(0, 0, 0, 0), egui::Color32::TRANSPARENT);
        let half = lut.pixel(0, 0, 128, 128);
        assert_eq!(half.a(), 128);
        assert!(half.b() <= 128);
    }

    /// One frame at egui time `time`: the page texture for `view` on the
    /// canvas `rect`.
    fn pass(
        ctx: &Context,
        raster: &mut Raster,
        doc: &Document,
        view: &View,
        rect: Rect,
        time: f64,
    ) -> (TextureId, Rect) {
        let page = doc.pages[0].id;
        let mut out = None;
        let input = egui::RawInput {
            time: Some(time),
            ..Default::default()
        };
        let mut full = ctx.run_ui(input, |ui| {
            out = raster.texture(
                ui.ctx(),
                doc,
                page,
                view,
                rect,
                None,
                1,
                false,
                false,
                false,
                false,
                None,
                None,
            );
        });
        full.textures_delta.clear();
        out.expect("texture")
    }

    /// A pan shows the last render moved into place at once, and renders
    /// the page anew once the view has rested; a zoom renders at once (on
    /// its thread), the old picture scaled into place meanwhile.
    #[test]
    fn a_moving_view_shows_the_last_render_until_it_rests() {
        static TURNS: Turns = Turns::new();
        let ctx = Context::default();
        let doc = Document::new("t", Size::new(100.0, 50.0));
        let rect = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(320.0, 200.0));
        let mut raster = Raster {
            turns: Some(&TURNS),
            ..Default::default()
        };
        let view = View::default();
        let (first, screen) = pass(&ctx, &mut raster, &doc, &view, rect, 0.0);
        assert_eq!(screen, rect);
        let key = raster.key.clone();
        // Panned 10 points right: the same texture, 10 points to the right,
        // nothing rendered.
        let mut moved = view;
        moved.pan(egui::vec2(10.0, 0.0));
        let (tex, screen) = pass(&ctx, &mut raster, &doc, &moved, rect, 0.01);
        assert_eq!(tex, first);
        assert_eq!(raster.key, key, "no new render yet");
        assert!((screen.min.x - 10.0).abs() < 1e-3, "{screen:?}");
        assert!(raster.settle.is_some());
        // The view rests: rendered anew for where it is now.
        let (_, screen) = pass(
            &ctx,
            &mut raster,
            &doc,
            &moved,
            rect,
            0.01 + SETTLE_SECONDS + 0.01,
        );
        assert_ne!(raster.key, key);
        assert_eq!(screen, rect);
        assert!(raster.settle.is_none());
        // Moving on at once re-arms the wait from the latest move.
        let mut again = moved;
        again.pan(egui::vec2(0.0, 5.0));
        let before = raster.key.clone();
        pass(&ctx, &mut raster, &doc, &again, rect, 1.0);
        pass(
            &ctx,
            &mut raster,
            &doc,
            &again,
            rect,
            1.0 + SETTLE_SECONDS / 2.0,
        );
        assert_eq!(raster.key, before);
        pass(
            &ctx,
            &mut raster,
            &doc,
            &again,
            rect,
            1.0 + SETTLE_SECONDS + 0.01,
        );
        assert_ne!(raster.key, before);
        // Zoomed: no wait for the view to rest. The render starts in the
        // same frame, on its thread, the old texture scaled about the page
        // meanwhile.
        let shown = raster.texture.as_ref().map(|t| t.id());
        let renders = raster.render_count();
        let mut zoomed = again;
        zoomed.zoom *= 2.0;
        let (tex, screen) = pass(&ctx, &mut raster, &doc, &zoomed, rect, 2.0);
        assert_eq!(Some(tex), shown);
        assert!((screen.width() - 640.0).abs() < 1e-3, "{screen:?}");
        assert_eq!(raster.render_count(), renders + 1, "rendered at once");
        assert!(raster.settle.is_none(), "no wait for the view to rest");
        assert!(raster.job.is_some(), "on its thread");
        let mut done = false;
        for i in 0..200 {
            std::thread::sleep(std::time::Duration::from_millis(5));
            let (_, screen) = pass(
                &ctx,
                &mut raster,
                &doc,
                &zoomed,
                rect,
                2.0 + 0.001 * i as f64,
            );
            if raster.picture_zoom() == Some(zoomed.zoom) {
                assert_eq!(screen, rect);
                done = true;
                break;
            }
        }
        assert!(done, "the zoomed picture came well before the view rested");
        assert_eq!(raster.render_count(), renders + 1);
    }

    /// Light zoom: a zoom shows the last picture scaled, renders nothing
    /// while the zoom goes on, and renders once when the view rests.
    #[test]
    fn light_zoom_renders_once_when_the_zoom_stops() {
        static TURNS: Turns = Turns::new();
        let ctx = Context::default();
        let doc = Document::new("t", Size::new(100.0, 50.0));
        let rect = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(320.0, 200.0));
        let mut raster = Raster {
            turns: Some(&TURNS),
            light_zoom: true,
            ..Default::default()
        };
        let mut view = View::default();
        let (first, _) = pass(&ctx, &mut raster, &doc, &view, rect, 0.0);
        let renders = raster.render_count();
        // Ten steps of a zoom, a frame apart: the old picture, scaled.
        let mut t = 1.0;
        for _ in 0..10 {
            view.zoom *= 1.05;
            t += 1.0 / 60.0;
            let (tex, screen) = pass(&ctx, &mut raster, &doc, &view, rect, t);
            assert_eq!(tex, first);
            assert!(screen.width() > rect.width());
        }
        assert_eq!(raster.render_count(), renders, "nothing drawn meanwhile");
        assert!(raster.job.is_none());
        // Still within the wait after the last step.
        pass(
            &ctx,
            &mut raster,
            &doc,
            &view,
            rect,
            t + LIGHT_SETTLE_SECONDS / 2.0,
        );
        assert_eq!(raster.render_count(), renders);
        // Rested: one render for the zoom it stopped at.
        let (_, screen) = pass(
            &ctx,
            &mut raster,
            &doc,
            &view,
            rect,
            t + LIGHT_SETTLE_SECONDS + 0.01,
        );
        assert_eq!(raster.render_count(), renders + 1);
        assert_eq!(raster.picture_zoom(), Some(view.zoom));
        assert_eq!(screen, rect);
    }

    /// A slow page is drawn on a thread of its own: the frame that asks
    /// for it returns at once with the last picture moved into place, and
    /// the new picture is taken once it is ready (#345).
    #[test]
    fn a_slow_page_is_drawn_on_its_own_thread() {
        // Turns of its own: the program's are shared with the other tests
        // running at the same time, whose jobs could keep it waiting.
        static TURNS: Turns = Turns::new();
        let ctx = Context::default();
        let doc = Document::new("t", Size::new(100.0, 50.0));
        let rect = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(320.0, 200.0));
        let mut raster = Raster {
            turns: Some(&TURNS),
            ..Default::default()
        };
        let view = View::default();
        let (first, _) = pass(&ctx, &mut raster, &doc, &view, rect, 0.0);
        let key = raster.key.clone();
        // Taken for slow: the next render runs on a thread. Panned, so the
        // render waits for the view to rest first (a zoom would not).
        raster.last_seconds = 1.0;
        let mut zoomed = view;
        zoomed.pan(egui::vec2(40.0, 0.0));
        let t = SETTLE_SECONDS + 0.01;
        pass(&ctx, &mut raster, &doc, &zoomed, rect, 0.0);
        let (mut tex, mut screen) = pass(&ctx, &mut raster, &doc, &zoomed, rect, t);
        for _ in 0..500 {
            if raster.job.is_some() {
                break;
            }
            assert_eq!(tex, first, "the old picture shows while waiting");
            std::thread::sleep(std::time::Duration::from_millis(10));
            (tex, screen) = pass(&ctx, &mut raster, &doc, &zoomed, rect, t);
        }
        assert_eq!(tex, first);
        assert!(raster.job.is_some(), "drawn on its thread");
        assert_eq!(raster.key, key, "the old picture shows meanwhile");
        assert!((screen.min.x - 40.0).abs() < 1e-3, "{screen:?}");
        // Ready a moment later: taken, for the view asked for.
        let mut done = false;
        for i in 0..200 {
            std::thread::sleep(std::time::Duration::from_millis(10));
            let (_, screen) = pass(&ctx, &mut raster, &doc, &zoomed, rect, t + 0.01 * i as f64);
            if raster.job.is_none() && raster.key != key {
                assert_eq!(screen, rect);
                done = true;
                break;
            }
        }
        assert!(done, "the picture from the thread was taken");
        assert!(raster.last_seconds < 1.0, "its time measured");
    }

    /// #3562: a change to the drawing is not laid over the last picture
    /// a band at a time: the bands showed the drawing before and after
    /// the change at once, cut along their edges (a shadow being dragged
    /// flickered in pieces). A view that moved still comes in bands.
    #[test]
    fn a_change_comes_whole_and_a_moved_view_in_bands() {
        static TURNS: Turns = Turns::new();
        let ctx = Context::default();
        let doc = Document::new("t", Size::new(100.0, 50.0));
        let page = doc.pages[0].id;
        let rect = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(320.0, 200.0));
        let mut raster = Raster {
            turns: Some(&TURNS),
            threaded: true,
            ..Default::default()
        };
        let mut pass = |raster: &mut Raster, revision: u64, zoom: f32, time: f64| {
            let view = View {
                zoom,
                origin: egui::pos2(0.0, 400.0),
            };
            let input = egui::RawInput {
                time: Some(time),
                ..Default::default()
            };
            let mut full = ctx.run_ui(input, |ui| {
                raster.texture(
                    ui.ctx(),
                    &doc,
                    page,
                    &view,
                    rect,
                    None,
                    revision,
                    false,
                    false,
                    false,
                    false,
                    None,
                    None,
                );
            });
            full.textures_delta.clear();
        };
        let wait = |raster: &mut Raster, pass: &mut dyn FnMut(&mut Raster)| {
            for _ in 0..500 {
                if raster.job.is_none() {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
                pass(raster);
            }
            panic!("the render ends");
        };
        pass(&mut raster, 1, 40.0, 0.0);
        pass(&mut raster, 2, 40.0, 0.1);
        let job = raster.job.as_ref().expect("on its thread");
        assert!(job.bands.is_none(), "a change comes whole");
        wait(&mut raster, &mut |r| pass(r, 2, 40.0, 0.2));
        // The same drawing at another zoom, once the view rests.
        let mut t = 1.0;
        while raster.job.is_none() && t < 100.0 {
            pass(&mut raster, 2, 60.0, t);
            t += 1.0;
        }
        let job = raster.job.as_ref().expect("the new zoom on its thread");
        assert!(job.bands.is_some(), "a moved view comes in bands");
        wait(&mut raster, &mut |r| pass(r, 2, 60.0, 200.0));
    }

    /// #3440: every picture after the first is drawn on its thread, as
    /// the program does: a change shows the last picture meanwhile, and
    /// once the changes stop the picture is of the latest one, never of
    /// one before it. The Pixels view goes the same way.
    #[test]
    fn every_change_draws_on_a_thread_and_ends_on_the_latest() {
        static TURNS: Turns = Turns::new();
        let ctx = Context::default();
        let doc = Document::new("t", Size::new(100.0, 50.0));
        let page = doc.pages[0].id;
        let rect = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(320.0, 200.0));
        let view = View {
            zoom: 40.0,
            origin: egui::pos2(0.0, 400.0),
        };
        let mut raster = Raster {
            turns: Some(&TURNS),
            threaded: true,
            ..Default::default()
        };
        let mut pass_at = |raster: &mut Raster, revision: u64, dpi: Option<f64>| {
            let mut out = None;
            let mut full = ctx.run_ui(egui::RawInput::default(), |ui| {
                out = raster.texture(
                    ui.ctx(),
                    &doc,
                    page,
                    &view,
                    rect,
                    None,
                    revision,
                    false,
                    false,
                    false,
                    false,
                    None,
                    dpi,
                );
            });
            full.textures_delta.clear();
            out.expect("texture")
        };
        pass_at(&mut raster, 1, None);
        assert!(raster.job.is_none(), "the first picture is drawn at once");
        assert_eq!(raster.key.as_ref().map(|k| k.revision), Some(1));
        // Changes one after another: each frame answers with the last
        // picture while the next is drawn on its thread.
        for revision in 2..=6 {
            let (_, at) = pass_at(&mut raster, revision, None);
            assert_eq!(at, rect);
            assert!(raster.job.is_some(), "revision {revision}: on its thread");
            assert_ne!(raster.key.as_ref().map(|k| k.revision), Some(revision));
        }
        let mut settled = false;
        for _ in 0..500 {
            std::thread::sleep(std::time::Duration::from_millis(5));
            pass_at(&mut raster, 6, None);
            if raster.job.is_none() && raster.key.as_ref().map(|k| k.revision) == Some(6) {
                settled = true;
                break;
            }
        }
        assert!(settled, "the picture of the latest change comes");
        // The Pixels view: drawn on its thread too, as blocks once there.
        let win = pixel_window(&view, rect, 96.0, 1.0).expect("window");
        pass_at(&mut raster, 6, Some(96.0));
        assert!(raster.job.is_some(), "the Pixels view on its thread");
        let mut there = None;
        for _ in 0..500 {
            std::thread::sleep(std::time::Duration::from_millis(5));
            let (_, at) = pass_at(&mut raster, 6, Some(96.0));
            if raster.job.is_none() && raster.key.as_ref().is_some_and(|k| k.pixels.is_some()) {
                there = Some(at);
                break;
            }
        }
        let at = there.expect("the Pixels view's picture comes");
        assert!((at.min - win.screen.min).length() < 1e-3, "{at:?} {:?}", win.screen);
        assert!((at.max - win.screen.max).length() < 1e-3, "{at:?} {:?}", win.screen);
        assert_eq!(
            raster.texture.as_ref().map(|t| t.size()),
            Some([win.w as usize, win.h as usize])
        );
    }

    /// The whole page is drawn small once the picture has rested, on a
    /// thread and without counting as a render of the picture; it shows
    /// where the page is on screen at any zoom, only for the drawing as
    /// it is now, and is not drawn while the view keeps moving (#2048).
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn the_whole_page_is_drawn_small_once_the_picture_rests() {
        static TURNS: Turns = Turns::new();
        let ctx = Context::default();
        let doc = Document::new("t", Size::new(100.0, 50.0));
        let page = doc.pages[0].id;
        let area = tracedraw_core::geometry::Rect::new(0.0, 0.0, 100.0, 50.0);
        let rect = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(320.0, 200.0));
        let mut raster = Raster {
            turns: Some(&TURNS),
            ..Default::default()
        };
        // Where the pointer moves to in the next frame, if it does.
        let pointer = std::cell::Cell::new(None::<egui::Pos2>);
        // One frame: the picture, then the backdrop.
        let frame = |raster: &mut Raster, view: &View, revision: u64, time: f64| {
            let mut out = (None, None);
            let input = egui::RawInput {
                time: Some(time),
                events: pointer
                    .take()
                    .map(egui::Event::PointerMoved)
                    .into_iter()
                    .collect(),
                ..Default::default()
            };
            let mut full = ctx.run_ui(input, |ui| {
                let main = raster.texture(
                    ui.ctx(),
                    &doc,
                    page,
                    view,
                    rect,
                    None,
                    revision,
                    false,
                    false,
                    false,
                    false,
                    None,
                    None,
                );
                let back = raster.backdrop(
                    ui.ctx(),
                    &doc,
                    page,
                    area,
                    view,
                    rect,
                    revision,
                    false,
                    false,
                    false,
                    false,
                );
                out = (main, back);
            });
            full.textures_delta.clear();
            out
        };
        // Frames at rest until the backdrop of `revision` is there.
        let wait = |raster: &mut Raster, view: &View, revision: u64, from: f64| {
            for i in 0..400 {
                let (_, back) = frame(raster, view, revision, from + 0.001 * i as f64);
                if let Some(b) = back {
                    return b;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            panic!("the backdrop never came");
        };
        // Zoomed in on the middle of the page.
        let mut view = View::default();
        view.zoom = 16.0;
        // Moving every frame, for longer than the rest: no backdrop.
        let mut t = 0.0;
        for _ in 0..20 {
            view.pan(egui::vec2(1.0, 0.0));
            let (main, back) = frame(&mut raster, &view, 1, t);
            assert!(main.is_some());
            assert!(back.is_none());
            t += BACKDROP_REST / 10.0;
        }
        assert_eq!(raster.backdrops, 0, "none while the view moves");
        // At rest: not before the rest is over, then once.
        frame(&mut raster, &view, 1, 10.0);
        frame(&mut raster, &view, 1, 10.0 + SETTLE_SECONDS + 0.01);
        let rested = 10.0 + SETTLE_SECONDS + 0.02;
        frame(&mut raster, &view, 1, rested);
        frame(&mut raster, &view, 1, rested + BACKDROP_REST / 2.0);
        assert_eq!(raster.backdrops, 0, "not before the picture has rested");
        // Rested, but the pointer is moving over the page: not in such a
        // frame (the drawing is copied for the thread when it starts).
        for i in 0..10 {
            pointer.set(Some(egui::pos2(20.0 + 9.0 * i as f32, 30.0)));
            let t = rested + BACKDROP_REST + 0.01 + 0.016 * i as f64;
            frame(&mut raster, &view, 1, t);
        }
        assert_eq!(raster.backdrops, 0, "not while the pointer moves");
        let rested = rested + 1.0;
        let renders = raster.render_count();
        let (_, back) = frame(&mut raster, &view, 1, rested + BACKDROP_REST + 0.01);
        assert!(back.is_none(), "on its thread");
        assert_eq!(raster.backdrops, 1);
        let (tex, at) = wait(&mut raster, &view, 1, rested + BACKDROP_REST + 0.02);
        assert_eq!(raster.backdrops, 1);
        assert_eq!(raster.render_count(), renders, "no render of the picture");
        // The page, where it is on screen.
        let page_at = view.rect_to_screen(area);
        assert!((at.min - page_at.min).length() < 1e-2, "{at:?} {page_at:?}");
        assert!((at.max - page_at.max).length() < 1e-2, "{at:?} {page_at:?}");
        // Zoomed far out: the picture, scaled into place, covers a small
        // part of the page; the backdrop covers the page.
        let mut out = view;
        out.zoom_at(rect.center(), 1.0 / 16.0);
        let (main, back) = frame(&mut raster, &out, 1, 20.0);
        let (_, main_at) = main.expect("the last picture");
        let (back_tex, back_at) = back.expect("the backdrop");
        assert_eq!(back_tex, tex);
        let page_at = out.rect_to_screen(area);
        assert!(main_at.width() < page_at.width() / 4.0, "{main_at:?}");
        assert!((back_at.min - page_at.min).length() < 1e-2, "{back_at:?}");
        assert!((back_at.max - page_at.max).length() < 1e-2, "{back_at:?}");
        // The drawing changed: the old backdrop is not shown, and the new
        // one waits for the picture to rest again.
        let (_, back) = frame(&mut raster, &out, 2, 30.0);
        assert!(back.is_none(), "never the backdrop of an earlier state");
        assert_eq!(raster.backdrops, 1);
        for i in 0..400 {
            frame(&mut raster, &out, 2, 30.0 + 0.001 * i as f64);
            if !raster.drawing() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        frame(&mut raster, &out, 2, 31.0);
        frame(&mut raster, &out, 2, 31.0 + BACKDROP_REST + 0.01);
        wait(&mut raster, &out, 2, 31.0 + BACKDROP_REST + 0.02);
        assert_eq!(raster.backdrops, 2);
        assert!(raster.has_backdrop(2));
        // Dropped with the picture.
        raster.invalidate();
        assert!(!raster.has_backdrop(2));
    }

    /// The picture to show for a zoom (#3894): among the kept ones that
    /// cover what the window shows, the nearest in zoom.
    #[test]
    fn the_nearest_covering_level_is_shown() {
        use tracedraw_core::geometry::Rect as R;
        let visible = R::new(10.0, 10.0, 50.0, 40.0);
        let pictures = [
            (1.0, R::new(0.0, 0.0, 100.0, 100.0)),
            (2.0, R::new(5.0, 5.0, 60.0, 45.0)),
            (3.9, R::new(20.0, 20.0, 30.0, 30.0)),
        ];
        // 3.9 is nearest to 4 but shows too little; 2 covers it all.
        assert_eq!(nearest_level(&pictures, 4.0, visible), Some(1));
        assert_eq!(nearest_level(&pictures, 1.2, visible), Some(0));
        assert_eq!(nearest_level(&pictures[2..], 4.0, visible), None);
    }

    /// Zoomed out and back (#3894): the picture of the first zoom is kept
    /// while the second is drawn, and coming back shows it at once, with
    /// no render; a change of the drawing drops what was kept.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn coming_back_to_a_zoom_shows_its_kept_picture_at_once() {
        static TURNS: Turns = Turns::new();
        let ctx = Context::default();
        let doc = Document::new("t", Size::new(100.0, 50.0));
        let rect = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(320.0, 200.0));
        let mut raster = Raster {
            turns: Some(&TURNS),
            ..Default::default()
        };
        let mut view = View::default();
        let mut t = 0.0;
        let mut until = |raster: &mut Raster, view: &View, zoom: f32| {
            for _ in 0..400 {
                t += 0.05;
                pass(&ctx, raster, &doc, view, rect, t);
                if raster.picture_zoom() == Some(zoom) && raster.job.is_none() {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            panic!("no picture at zoom {zoom}");
        };
        let first = view.zoom;
        until(&mut raster, &view, first);
        let at = egui::pos2(160.0, 100.0);
        view.zoom_at(at, 0.5);
        let out = view.zoom;
        until(&mut raster, &view, out);
        assert_eq!(raster.levels.len(), 1, "the first zoom's picture is kept");
        let renders = raster.render_count();
        view.zoom_at(at, 2.0);
        let (tex, _) = pass(&ctx, &mut raster, &doc, &view, rect, 100.0);
        assert_eq!(raster.render_count(), renders, "no render to come back");
        assert_eq!(raster.picture_zoom(), Some(first));
        assert_eq!(Some(tex), raster.texture.as_ref().map(|t| t.id()));
        assert_eq!(raster.levels.len(), 1, "the zoomed-out picture kept in turn");
        raster.invalidate();
        assert!(raster.levels.is_empty());
    }

    /// A burst of zoom steps while a render runs: no render is started
    /// for a step, and the running one is not given up while the wheel
    /// goes on: it keeps its turn, the last picture shows scaled, and the
    /// steps wait for the view to settle. Once it has, the render, for a
    /// zoom the view has left, gives up (its picture would only be shown
    /// scaled again, and the latest zoom's render waited for all of it,
    /// #2522), and the next render starts as soon as it has, for the
    /// latest zoom only, covering only the canvas, the part of the page
    /// on screen.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn the_latest_zoom_wins_without_restarting_renders() {
        static TURNS: Turns = Turns::new();
        let ctx = Context::default();
        let doc = Document::new("t", Size::new(100.0, 50.0));
        let rect = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(320.0, 200.0));
        let hold = std::sync::Arc::new(std::sync::Mutex::new(()));
        let mut raster = Raster {
            turns: Some(&TURNS),
            hold: Some(hold.clone()),
            ..Default::default()
        };
        let mut view = View::default();
        pass(&ctx, &mut raster, &doc, &view, rect, 0.0);
        let renders = raster.render_count();
        // The first step starts a render, held on its thread.
        let held = hold.lock().expect("lock");
        let at = egui::pos2(160.0, 100.0);
        view.zoom_at(at, 1.15);
        let first_step = view.zoom;
        pass(&ctx, &mut raster, &doc, &view, rect, 0.01);
        assert_eq!(raster.render_count(), renders + 1);
        let copy = raster.kept_copy().expect("the drawing copied").1;
        // Nineteen more steps while it runs: nothing restarted, nothing
        // started beside it.
        for i in 0..19 {
            view.zoom_at(at, 1.15f32.powf(0.25));
            pass(
                &ctx,
                &mut raster,
                &doc,
                &view,
                rect,
                0.02 + 0.016 * i as f64,
            );
            assert_eq!(raster.render_count(), renders + 1, "restarted");
            assert_eq!(raster.job.as_ref().map(|j| j.key.zoom), Some(first_step));
            assert!(
                !raster
                    .job
                    .as_ref()
                    .is_some_and(|j| j.cancel.load(std::sync::atomic::Ordering::Relaxed)),
                "the running render is not given up while the wheel goes on"
            );
            assert!(raster.settle.is_some(), "the steps wait to settle");
        }
        // The wheel stops: once the view has settled, the render, for a
        // zoom the view has left, is told to give up.
        let mut told = false;
        for i in 0..400 {
            pass(&ctx, &mut raster, &doc, &view, rect, 0.4 + 0.001 * i as f64);
            told = raster
                .job
                .as_ref()
                .is_some_and(|j| j.cancel.load(std::sync::atomic::Ordering::Relaxed));
            if told {
                break;
            }
        }
        assert!(told, "never told to give up");
        assert_eq!(raster.render_count(), renders + 1);
        let before = raster.picture_zoom();
        drop(held);
        // It gave up, and the latest zoom's render started at once.
        let mut next = None;
        for i in 0..400 {
            std::thread::sleep(std::time::Duration::from_millis(5));
            pass(&ctx, &mut raster, &doc, &view, rect, 1.0 + 0.001 * i as f64);
            if raster.render_count() > renders + 1 {
                next = Some(i);
                break;
            }
        }
        assert!(next.is_some(), "the latest zoom was never rendered");
        assert_eq!(
            raster.picture_zoom(),
            before,
            "no picture of the first step"
        );
        assert_eq!(raster.job.as_ref().map(|j| j.key.zoom), Some(view.zoom));
        assert_eq!(raster.render_count(), renders + 2);
        // The second render shares the first one's copy of the drawing.
        let again = raster.kept_copy().expect("kept").1;
        assert!(std::sync::Arc::ptr_eq(&copy, &again), "copied again");
        for i in 0..400 {
            if raster.picture_zoom() == Some(view.zoom) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
            pass(&ctx, &mut raster, &doc, &view, rect, 2.0 + 0.001 * i as f64);
        }
        assert_eq!(raster.picture_zoom(), Some(view.zoom));
        assert_eq!(
            raster.render_count(),
            renders + 2,
            "two renders for twenty steps"
        );
        // The picture is the canvas's size and shows only the part of the
        // page on screen.
        assert_eq!(raster.texture.as_ref().map(|t| t.size()), Some([320, 200]));
        let shown = raster.page_rect.expect("page rect");
        let want = view.visible_page_rect(rect);
        for (a, b) in [
            (shown.x0, want.x0),
            (shown.y0, want.y0),
            (shown.x1, want.x1),
            (shown.y1, want.y1),
        ] {
            assert!((a - b).abs() < 1e-3, "{shown:?} {want:?}");
        }
    }

    /// A render slower than the wheel: while the wheel goes on, the steps
    /// are skipped (the last picture shows scaled) and the running render
    /// is not given up; when it is done, for a zoom the view has already
    /// left, its picture shows scaled and the next render starts at once,
    /// for the zoom the view is at, the turn not left idle; and when the
    /// wheel has stopped, one render draws the zoom it stopped at.
    #[test]
    fn a_zoom_burst_skips_steps_until_the_view_settles() {
        static TURNS: Turns = Turns::new();
        let ctx = Context::default();
        let doc = Document::new("t", Size::new(100.0, 50.0));
        let rect = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(320.0, 200.0));
        let hold = std::sync::Arc::new(std::sync::Mutex::new(()));
        let mut raster = Raster {
            turns: Some(&TURNS),
            hold: Some(hold.clone()),
            ..Default::default()
        };
        let mut view = View::default();
        let at = egui::pos2(160.0, 100.0);
        pass(&ctx, &mut raster, &doc, &view, rect, 0.0);
        let renders = raster.render_count();
        // The first step starts a render, held on its thread.
        let held = hold.lock().expect("lock");
        view.zoom_at(at, 1.15);
        let first_step = view.zoom;
        let (first_tex, _) = pass(&ctx, &mut raster, &doc, &view, rect, 0.01);
        assert_eq!(raster.render_count(), renders + 1);
        // The next step: skipped, the running render keeps its turn.
        view.zoom_at(at, 1.15);
        let (tex, screen) = pass(&ctx, &mut raster, &doc, &view, rect, 0.03);
        assert_eq!(tex, first_tex, "the last picture, scaled");
        assert!(screen.width() > rect.width());
        assert_eq!(raster.render_count(), renders + 1, "no render for the step");
        assert_eq!(raster.job.as_ref().map(|j| j.key.zoom), Some(first_step));
        assert!(
            !raster
                .job
                .as_ref()
                .is_some_and(|j| j.cancel.load(std::sync::atomic::Ordering::Relaxed)),
            "not given up while the wheel goes on"
        );
        assert!(raster.settle.is_some(), "the steps wait to settle");
        // It is done, for a zoom the view has already left: its picture
        // shows scaled, and the next render starts at once, for the zoom
        // the view is at, the turn not left idle till the wheel rests.
        drop(held);
        let mut shown = None;
        let mut relayed = false;
        for i in 0..25 {
            std::thread::sleep(std::time::Duration::from_millis(5));
            pass(&ctx, &mut raster, &doc, &view, rect, 0.04 + 0.001 * i as f64);
            shown = raster.picture_zoom();
            if raster.render_count() > renders + 1 {
                relayed = true;
                break;
            }
        }
        assert!(relayed, "the free turn was left idle");
        assert_eq!(shown, Some(first_step), "the done render never showed");
        assert_eq!(
            raster.job.as_ref().map(|j| j.key.zoom),
            Some(view.zoom),
            "the zoom the view is at goes next"
        );
        assert_eq!(raster.settle, None, "no wait once the turn is free");
        // The wheel has stopped: that render draws the zoom it stopped at.
        for i in 0..400 {
            if raster.picture_zoom() == Some(view.zoom) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
            pass(&ctx, &mut raster, &doc, &view, rect, 0.2 + 0.001 * i as f64);
        }
        assert_eq!(raster.picture_zoom(), Some(view.zoom));
        assert_eq!(
            raster.render_count(),
            renders + 2,
            "two renders for the two steps"
        );
    }

    /// While the wheel's glide is on its way, the canvas asks for the
    /// picture of the view the glide lands at (Raster::zoom_target): the
    /// picture is that zoom, drawn at once, and while it draws (and
    /// after) it is placed by the view the canvas shows, the one still on
    /// its way; when the view lands on the aim, the picture fits it
    /// exactly and nothing is rendered for the bits the frames'
    /// multiplying was off by.
    #[test]
    fn the_picture_of_a_glide_is_drawn_for_the_view_it_lands_at() {
        static TURNS: Turns = Turns::new();
        let ctx = Context::default();
        let doc = Document::new("t", Size::new(100.0, 50.0));
        let rect = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(320.0, 200.0));
        let mut raster = Raster {
            turns: Some(&TURNS),
            ..Default::default()
        };
        let view = View::default();
        pass(&ctx, &mut raster, &doc, &view, rect, 0.0);
        let renders = raster.render_count();
        let at = egui::pos2(160.0, 100.0);
        // The glide's aim, a zoom 1.15 ahead about the canvas middle, and
        // the view a step into the glide.
        let mut aim = view;
        aim.zoom_at(at, 1.15);
        let mut on_the_way = view;
        on_the_way.zoom_at(at, 1.04);
        raster.zoom_target = Some(aim);
        pass(&ctx, &mut raster, &doc, &on_the_way, rect, 0.01);
        assert_eq!(raster.render_count(), renders + 1, "rendered at once");
        assert_eq!(
            raster.job.as_ref().map(|j| j.key.zoom),
            Some(aim.zoom),
            "the render on its way is for the aim"
        );
        // It comes a picture of the aim, placed by the view on its way:
        // the aim zooms in, so the part of the page it covers takes less
        // than the canvas so far.
        let mut landed = false;
        for i in 0..400 {
            std::thread::sleep(std::time::Duration::from_millis(5));
            raster.zoom_target = Some(aim);
            let (_, screen) = pass(
                &ctx,
                &mut raster,
                &doc,
                &on_the_way,
                rect,
                0.02 + 0.001 * i as f64,
            );
            if raster.picture_zoom() == Some(aim.zoom) {
                assert!(
                    screen.width() < rect.width(),
                    "placed by the view on its way: {screen:?}"
                );
                landed = true;
                break;
            }
        }
        assert!(landed, "the aim was never rendered");
        // The view lands on the aim, the aim dropped: the picture fits it
        // exactly, and no render is started for the difference.
        let rendered = raster.render_count();
        raster.zoom_target = None;
        for i in 0..400 {
            std::thread::sleep(std::time::Duration::from_millis(5));
            let (_, screen) =
                pass(&ctx, &mut raster, &doc, &aim, rect, 0.3 + 0.001 * i as f64);
            if raster.job.is_none() {
                assert_eq!(screen, rect, "the picture of the landed view");
                break;
            }
        }
        assert_eq!(raster.picture_zoom(), Some(aim.zoom));
        assert_eq!(raster.render_count(), rendered, "a render for the bits");
    }

    /// A page beside the current one in the Multipage View is rendered
    /// on a canvas the size of its paper, which changes with the zoom:
    /// while the view moves, the last picture is scaled into place, the
    /// page's corner and size following the zoom, and nothing is rendered
    /// until the view rests (#902, #903).
    #[test]
    fn a_picture_that_follows_the_zoom_keeps_its_place_while_resized() {
        // Turns of its own, as above: a turn kept for another test's
        // current page would defer this page's render.
        static TURNS: Turns = Turns::new();
        let ctx = Context::default();
        let doc = Document::new("t", Size::new(100.0, 50.0));
        let mut raster = Raster {
            follows_zoom: true,
            turns: Some(&TURNS),
            ..Default::default()
        };
        let view = View {
            zoom: 2.0,
            origin: egui::pos2(10.0, 110.0),
        };
        let paper =
            |v: &View| v.rect_to_screen(tracedraw_core::geometry::Rect::new(0.0, 0.0, 100.0, 50.0));
        let (first, at) = pass(&ctx, &mut raster, &doc, &view, paper(&view), 0.0);
        assert_eq!(at, paper(&view));
        let renders = raster.render_count();
        // Zoomed out about a point: the paper is smaller, the canvas with
        // it. The same picture, exactly on the paper.
        let mut zoomed = view;
        zoomed.zoom_at(egui::pos2(300.0, 300.0), 0.8);
        let (tex, at) = pass(&ctx, &mut raster, &doc, &zoomed, paper(&zoomed), 0.02);
        assert_eq!(tex, first);
        assert_eq!(raster.render_count(), renders, "nothing rendered");
        let want = paper(&zoomed);
        assert!((at.min - want.min).length() < 1e-3, "{at:?} {want:?}");
        assert!((at.max - want.max).length() < 1e-3, "{at:?} {want:?}");
        // At rest: rendered for the new size.
        pass(
            &ctx,
            &mut raster,
            &doc,
            &zoomed,
            paper(&zoomed),
            0.02 + SETTLE_SECONDS + 0.01,
        );
        assert_eq!(raster.render_count(), renders + 1);
        // The main canvas keeps rendering when its size changes.
        let mut main = Raster {
            turns: Some(&TURNS),
            ..Default::default()
        };
        let rect = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(320.0, 200.0));
        pass(&ctx, &mut main, &doc, &view, rect, 0.0);
        let wider = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(330.0, 200.0));
        pass(&ctx, &mut main, &doc, &view, wider, 0.01);
        assert_eq!(main.render_count(), 2);
    }

    /// A deferred render shows the last picture where its part of the
    /// page now is and renders nothing; the next frame without the
    /// deferral renders.
    #[test]
    fn a_deferred_render_waits_for_a_later_frame() {
        let ctx = Context::default();
        let doc = Document::new("t", Size::new(100.0, 50.0));
        let page = doc.pages[0].id;
        let rect = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(320.0, 200.0));
        let view = View::default();
        let mut raster = Raster {
            defer: true,
            ..Default::default()
        };
        let at_revision = |raster: &mut Raster, revision: u64| {
            raster.texture(
                &ctx, &doc, page, &view, rect, None, revision, false, false, false, false, None,
                None,
            )
        };
        // Nothing to show yet: nothing shown, nothing rendered.
        assert!(at_revision(&mut raster, 1).is_none());
        assert_eq!(raster.render_count(), 0);
        raster.defer = false;
        let (first, _) = at_revision(&mut raster, 1).expect("rendered");
        // The drawing changed: deferred, the old picture stays.
        raster.defer = true;
        let (tex, at) = at_revision(&mut raster, 2).expect("old picture");
        assert_eq!(tex, first);
        assert_eq!(at, rect);
        assert_eq!(raster.render_count(), 1);
        raster.defer = false;
        at_revision(&mut raster, 2).expect("rendered");
        assert_eq!(raster.render_count(), 2);
    }

    #[test]
    fn a_canvas_that_moves_renders_again() {
        let ctx = Context::default();
        let doc = Document::new("t", Size::new(100.0, 50.0));
        let view = View::default();
        let mut raster = Raster::default();
        let at = |x: f32, y: f32| Rect::from_min_size(egui::pos2(x, y), egui::vec2(320.0, 200.0));
        pass(&ctx, &mut raster, &doc, &view, at(40.0, 160.0), 0.0);
        let first = raster.key.clone();
        // Same canvas, same view: the cached texture.
        pass(&ctx, &mut raster, &doc, &view, at(40.0, 160.0), 0.01);
        assert_eq!(raster.key, first);
        // The canvas moves up by a bar's height and keeps its size: the
        // page sits elsewhere inside it. The last render is shown where
        // the page now is, and the page is rendered anew once the view
        // rests, with the new offset.
        let (_, screen) = pass(&ctx, &mut raster, &doc, &view, at(40.0, 107.0), 0.02);
        assert_eq!(
            screen,
            view.rect_to_screen(raster.page_rect.expect("page rect"))
        );
        pass(
            &ctx,
            &mut raster,
            &doc,
            &view,
            at(40.0, 107.0),
            0.02 + SETTLE_SECONDS + 0.01,
        );
        assert_ne!(raster.key, first);
        assert_eq!(
            raster.key.as_ref().map(|k| k.offset),
            Some(view.origin - egui::pos2(40.0, 107.0))
        );
    }

    #[test]
    fn the_pixels_view_draws_whole_document_pixels_as_blocks() {
        // 72 dpi: a document pixel is 0.3528 mm; at 10 screen pixels per
        // mm it covers 3.5 screen pixels.
        let view = View {
            zoom: 10.0,
            origin: egui::pos2(0.0, 500.0),
        };
        let rect = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(400.0, 500.0));
        let win = pixel_window(&view, rect, 72.0, 1.0).expect("window");
        assert_eq!(win.ix0, 0);
        // 50 mm of page height: 141.7 pixels, rounded up to whole ones.
        assert_eq!(win.iy1, 142);
        assert_eq!(win.w, 114);
        assert_eq!(win.h, 142);
        // Whole pixels: the blocks start on the page corner and may run
        // past the canvas a little.
        assert!(win.screen.min.x <= rect.min.x && win.screen.max.x >= rect.max.x);
        assert!(win.screen.min.y <= rect.min.y);
        assert!((win.screen.max.y - 500.0).abs() < 1e-3);
        // Zoomed out below one screen pixel per document pixel: the normal
        // render.
        let far = View { zoom: 1.0, ..view };
        assert_eq!(pixel_window(&far, rect, 72.0, 1.0), None);
        assert_eq!(pixel_window(&view, rect, 0.0, 1.0), None);
        assert_eq!(pixel_window(&view, rect, f64::NAN, 1.0), None);
        // A huge window (very high resolution) falls back too.
        assert_eq!(pixel_window(&view, rect, 1.0e6, 1.0), None);
    }

    #[test]
    fn the_pixels_view_texture_is_one_texel_per_document_pixel() {
        let ctx = Context::default();
        let doc = Document::new("t", Size::new(100.0, 50.0));
        let page = doc.pages[0].id;
        let view = View {
            zoom: 40.0,
            origin: egui::pos2(0.0, 400.0),
        };
        let rect = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(400.0, 400.0));
        let mut raster = Raster::default();
        let (_, at) = raster
            .texture(
                &ctx,
                &doc,
                page,
                &view,
                rect,
                None,
                1,
                false,
                false,
                false,
                false,
                None,
                Some(96.0),
            )
            .expect("texture");
        let size = raster.texture.as_ref().map(|t| t.size());
        let win = pixel_window(&view, rect, 96.0, 1.0).expect("window");
        assert_eq!(size, Some([win.w as usize, win.h as usize]));
        assert_eq!(at, win.screen);
        // Without the Pixels view: screen-sized, painted on the canvas.
        let (_, at) = raster
            .texture(
                &ctx, &doc, page, &view, rect, None, 1, false, false, false, false, None, None,
            )
            .expect("texture");
        assert_eq!(at, rect);
        assert_eq!(raster.texture.as_ref().map(|t| t.size()), Some([400, 400]));
    }

    /// The browser has no clock behind std::time: asking it the time
    /// panics, and the page froze on the first drawing it showed. Code
    /// outside tests asks web_time instead (mcp_link is not in the web
    /// build).
    #[test]
    fn the_web_build_never_asks_std_for_the_time() {
        fn scan(dir: &std::path::Path, found: &mut Vec<String>) {
            for e in std::fs::read_dir(dir).expect("src").flatten() {
                let p = e.path();
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if p.is_dir() {
                    scan(&p, found);
                    continue;
                }
                if !name.ends_with(".rs") || name.ends_with("_tests.rs") || name == "mcp_link.rs" {
                    continue;
                }
                let text = std::fs::read_to_string(&p).expect("source");
                for (i, line) in text.lines().enumerate() {
                    if line.trim_start().starts_with("#[cfg(test)]") {
                        break;
                    }
                    if line.contains(concat!("std::time::", "Instant::now"))
                        || line.contains(concat!("std::time::", "SystemTime::now"))
                    {
                        found.push(format!("{}:{}", p.display(), i + 1));
                    }
                }
            }
        }
        let mut found = Vec::new();
        scan(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut found,
        );
        assert!(found.is_empty(), "use web_time: {found:?}");
    }

    /// #3903: when the drawing's revision moves on while a render job for an
    /// earlier revision is still running on its thread, the obsolete job is
    /// told to give up instead of running to completion and uploading an
    /// outdated picture, and the latest revision renders right after.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn a_newer_revision_cancels_a_stale_render_in_flight() {
        use tracedraw_core::document::ShapeKind;
        use tracedraw_core::{Color, Fill, Shape};
        static TURNS: Turns = Turns::new();
        let ctx = Context::default();
        let mut doc = Document::new("t", Size::new(100.0, 50.0));
        let page = doc.pages[0].id;
        let layer = doc.pages[0].layers[0].id;
        let mut box_shape = Shape::new(
            ShapeId(1),
            ShapeKind::Rect {
                rect: tracedraw_core::geometry::Rect::new(10.0, 10.0, 40.0, 30.0),
                radius: 0.0,
                corners: None,
            },
        );
        box_shape.fill = Fill::Solid(Color::rgb8(200, 40, 40));
        doc.layer_mut(layer).unwrap().shapes.push(box_shape);
        let rect = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(320.0, 200.0));
        let view = View::default();
        let hold = std::sync::Arc::new(std::sync::Mutex::new(()));
        let mut raster = Raster {
            turns: Some(&TURNS),
            threaded: true,
            hold: Some(hold.clone()),
            ..Default::default()
        };
        let mut frame = |raster: &mut Raster, revision: u64| {
            let mut full = ctx.run_ui(egui::RawInput::default(), |ui| {
                raster.texture(
                    ui.ctx(),
                    &doc,
                    page,
                    &view,
                    rect,
                    None,
                    revision,
                    false,
                    false,
                    false,
                    false,
                    None,
                    None,
                );
            });
            full.textures_delta.clear();
        };
        frame(&mut raster, 1);
        assert_eq!(raster.shown_revision(), Some(1));
        let renders = raster.render_count();

        let held = hold.lock().expect("lock");
        frame(&mut raster, 2);
        assert_eq!(raster.render_count(), renders + 1);
        assert!(!raster
            .job
            .as_ref()
            .expect("job 2")
            .cancel
            .load(std::sync::atomic::Ordering::Relaxed));
        for rev in 3..=5 {
            frame(&mut raster, rev);
            assert_eq!(raster.render_count(), renders + 1);
            assert!(
                raster
                    .job
                    .as_ref()
                    .expect("still job 2")
                    .cancel
                    .load(std::sync::atomic::Ordering::Relaxed),
                "revision {rev} should cancel stale job 2"
            );
        }
        drop(held);
        let mut settled = false;
        for _ in 0..400 {
            std::thread::sleep(std::time::Duration::from_millis(5));
            frame(&mut raster, 5);
            assert_ne!(
                raster.shown_revision(),
                Some(2),
                "cancelled revision 2 must not overwrite the shown picture"
            );
            if raster.shown_revision() == Some(5) && raster.job.is_none() {
                settled = true;
                break;
            }
        }
        assert!(settled, "revision 5 rendered once job 2 aborted");
        assert_eq!(raster.render_count(), renders + 2);
    }
}
