//! Help > Benchmark: a score of this computer for TraceDraw, one number
//! for each part that makes the work fast or slow and an overall score
//! that is the lowest of them, all from 1.0 to 9.9.
//!
//! - Processor (all cores): a sample drawing of curves, gradients,
//!   transparency and shadows drawn on every core, the way the canvas is
//!   drawn after a zoom or a change.
//! - Processor (one core): the same drawing on one core, the way the
//!   previews and the small picture behind the canvas are drawn.
//! - Memory: how fast a large block of memory is copied (big drawings
//!   and pictures are copied whole).
//! - Graphics: how many frames a second the window shows while a full HD
//!   picture is handed to the window several times a frame, the work the
//!   window does at every step of a zoom or a pan. It measures the
//!   graphics card, or the processor when hardware acceleration is off.
//!
//! Each score is 5.0 at the reference time and one more for each halving
//! of it (one less for each doubling). Under [`SLOW_SCORE`] the computer
//! is slow for large drawings whatever TraceDraw does: the window says
//! so, and Light zoom is turned on. The results are kept, one with
//! hardware acceleration on and one with it off, to compare them. See
//! docs/behavior/benchmark.md.

use serde::{Deserialize, Serialize};
use tracedraw_core::color::Color;
use tracedraw_core::document::{Document, Shadow, Shape, ShapeKind};
use tracedraw_core::geometry::{BezPath, Rect, Size};
use tracedraw_core::style::{Fill, Fountain, FountainKind, Stroke};

/// Under this overall score large drawings are slow on this computer.
pub const SLOW_SCORE: f32 = 4.0;

/// The lowest and the highest score.
pub const MIN_SCORE: f32 = 1.0;
pub const MAX_SCORE: f32 = 9.9;

/// Seconds the sample drawing takes on all the cores of the reference
/// computer (score 5.0): four cores at about 2 GHz, a laptop processor
/// of about 2016 (a 4-core 2.1 GHz server processor measured 0.37 s on
/// all its cores and 1.06 s on one).
pub const REF_ALL_CORES: f64 = 0.40;

/// Seconds it takes on one core of the reference computer.
pub const REF_ONE_CORE: f64 = 1.20;

/// Gigabytes a second the reference computer copies.
pub const REF_MEMORY_GBS: f64 = 4.0;

/// Frames a second of the graphics test that score 5.0. Frames come at
/// most as often as the screen refreshes (60 a second on most screens):
/// that scores 7.0.
pub const REF_FPS: f64 = 15.0;

/// Pictures handed to the window each frame of the graphics test.
pub const UPLOADS_PER_FRAME: usize = 4;

/// Frames the graphics test measures.
pub const GRAPHICS_FRAMES: usize = 60;

/// The size of the picture of the drawing tests and of the graphics
/// test, pixels: a full HD screen.
pub const PICTURE: (u32, u32) = (1920, 1080);

/// The score for a measure `ratio` times as good as the reference one
/// (half the time: 2.0): 5.0 plus one for each doubling, kept between
/// [`MIN_SCORE`] and [`MAX_SCORE`], to one decimal.
pub fn score(ratio: f64) -> f32 {
    if !ratio.is_finite() || ratio <= 0.0 {
        return MIN_SCORE;
    }
    let s = (5.0 + ratio.log2()) as f32;
    ((s * 10.0).floor() / 10.0).clamp(MIN_SCORE, MAX_SCORE)
}

/// The score of a test that took `seconds` where the reference computer
/// takes `reference`.
pub fn time_score(reference: f64, seconds: f64) -> f32 {
    if seconds.is_nan() || seconds <= 0.0 {
        return MAX_SCORE;
    }
    score(reference / seconds)
}

/// One run of the benchmark: the scores, what was measured and how the
/// window was drawn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Measured {
    /// Hardware acceleration was on (Options > General) in the run.
    pub acceleration: bool,
    /// What drew the window: the graphics card's name, or the software
    /// adapter's.
    pub adapter: String,
    /// The window was drawn on the processor (a software adapter).
    pub software: bool,
    /// Processor cores (threads) the drawing used.
    pub cores: usize,
    pub all_cores_seconds: f64,
    pub one_core_seconds: f64,
    pub memory_gbs: f64,
    pub fps: f64,
    /// The slowest frame of the graphics test, seconds.
    pub worst_frame: f64,
    /// When, as the system clock gave it (seconds since 1970).
    pub when: u64,
    /// Quick (the four parts) or advanced (the tasks too).
    pub level: Level,
    /// The advanced test's tasks, timed on real drawings.
    pub tasks: Vec<Task>,
}

/// How far a run goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Level {
    /// The four parts: some seconds.
    #[default]
    Quick,
    /// The four parts, then real tasks on two large drawings: a minute
    /// or more.
    Advanced,
}

/// One task of the advanced test: what was done (its key in the
/// language files), on a drawing of how many objects, and how long it
/// took.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Task {
    pub key: String,
    pub objects: usize,
    pub seconds: f64,
}

/// How a task's time feels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rating {
    Good,
    Fair,
    Slow,
}

/// Tasks done while drawing (a redraw, a zoom, a move): under a quarter
/// of a second feels at once, over a second feels slow. Tasks on files
/// (save, open, PDF) have four times as long.
pub fn rating(key: &str, seconds: f64) -> Rating {
    let file = matches!(key, TASK_SAVE | TASK_OPEN | TASK_PDF);
    let (good, fair) = if file { (1.0, 4.0) } else { (0.25, 1.0) };
    if seconds <= good {
        Rating::Good
    } else if seconds <= fair {
        Rating::Fair
    } else {
        Rating::Slow
    }
}

pub const TASK_REDRAW: &str = "benchmark.task_redraw";
pub const TASK_ZOOM: &str = "benchmark.task_zoom";
pub const TASK_MOVE: &str = "benchmark.task_move";
pub const TASK_SAVE: &str = "benchmark.task_save";
pub const TASK_OPEN: &str = "benchmark.task_open";
pub const TASK_PDF: &str = "benchmark.task_pdf";
pub const TASK_SHADOWS_CARD: &str = "benchmark.task_shadows_card";
pub const TASK_SHADOWS_PROCESSOR: &str = "benchmark.task_shadows_processor";

/// The drawings of the advanced test, objects.
pub const ADVANCED_SIZES: [usize; 2] = [10_000, 50_000];

/// Objects of the quick test's sample drawing.
pub const SAMPLE_OBJECTS: usize = 1501;

/// A redraw of the whole view that takes no longer than this feels at
/// once: what "comfortable" means in [`Measured::comfortable_objects`].
pub const COMFORT_SECONDS: f64 = 0.25;

/// The zoom that suits a computer (Options > Tools > Zoom/Pan).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoomStyle {
    /// A wheel notch zooms in a short glide, drawn at every step.
    Smooth,
    /// As in CorelDRAW: a wheel notch zooms at once, drawn sharp at
    /// once, the point under the pointer kept.
    Corel,
    /// For slow computers: a notch zooms at once and the drawing is drawn
    /// again only once the zoom stops.
    Light,
}

impl ZoomStyle {
    pub const ALL: [ZoomStyle; 3] = [ZoomStyle::Smooth, ZoomStyle::Corel, ZoomStyle::Light];

    /// The style the settings give.
    pub fn of(s: &crate::settings::Settings) -> ZoomStyle {
        if s.light_zoom {
            ZoomStyle::Light
        } else if s.smooth_zoom {
            ZoomStyle::Smooth
        } else {
            ZoomStyle::Corel
        }
    }

    /// Set the zoom options for this style.
    pub fn apply(self, s: &mut crate::settings::Settings) {
        s.light_zoom = self == ZoomStyle::Light;
        s.smooth_zoom = self == ZoomStyle::Smooth;
        if self == ZoomStyle::Corel {
            s.wheel_zoom_centers = false;
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            ZoomStyle::Smooth => "benchmark.zoom_smooth",
            ZoomStyle::Corel => "benchmark.zoom_corel",
            ZoomStyle::Light => "benchmark.zoom_light",
        }
    }
}

/// Round `n` down to a round number of objects (500, 1 000, 2 000,
/// 5 000, 10 000...); 0 under 500.
fn round_objects(n: f64) -> u64 {
    let mut best = 0;
    let mut step = 500u64;
    while (step as f64) <= n && step <= 10_000_000 {
        best = step;
        let lead = step.to_string().as_bytes()[0];
        step = if lead == b'2' { step / 2 * 5 } else { step * 2 };
    }
    best
}

impl Measured {
    pub fn all_cores(&self) -> f32 {
        time_score(REF_ALL_CORES, self.all_cores_seconds)
    }
    pub fn one_core(&self) -> f32 {
        time_score(REF_ONE_CORE, self.one_core_seconds)
    }
    pub fn memory(&self) -> f32 {
        score(self.memory_gbs / REF_MEMORY_GBS)
    }
    pub fn graphics(&self) -> f32 {
        score(self.fps / REF_FPS)
    }
    /// The overall score: the lowest of the parts, since the slowest one
    /// sets the pace.
    pub fn overall(&self) -> f32 {
        [
            self.all_cores(),
            self.one_core(),
            self.memory(),
            self.graphics(),
        ]
        .into_iter()
        .fold(MAX_SCORE, f32::min)
    }
    /// About how many objects a drawing can have before a redraw of the
    /// whole view takes longer than [`COMFORT_SECONDS`] here, from the
    /// largest drawing timed (the advanced test's, else the sample
    /// drawing), rounded down; 0 when nothing was timed.
    pub fn comfortable_objects(&self) -> u64 {
        let largest = self
            .tasks
            .iter()
            .filter(|t| t.key == TASK_REDRAW && t.seconds > 0.0 && t.objects > 0)
            .max_by_key(|t| t.objects)
            .map(|t| t.seconds / t.objects as f64);
        let per_object = largest.or_else(|| {
            (self.all_cores_seconds > 0.0).then(|| self.all_cores_seconds / SAMPLE_OBJECTS as f64)
        });
        match per_object {
            Some(p) if p > 0.0 => round_objects(COMFORT_SECONDS / p),
            _ => 0,
        }
    }

    /// The zoom to suggest: Light on a slow computer; else as in
    /// CorelDRAW, which most people coming from it prefer, and which
    /// draws less than the glide.
    pub fn suggested_zoom(&self) -> ZoomStyle {
        if self.slow() {
            ZoomStyle::Light
        } else {
            ZoomStyle::Corel
        }
    }

    /// Large drawings are slow on this computer.
    pub fn slow(&self) -> bool {
        self.overall() < SLOW_SCORE
    }
    /// The part with the lowest score, as its key in the language files.
    pub fn weakest(&self) -> &'static str {
        let parts = [
            (self.all_cores(), "benchmark.all_cores"),
            (self.one_core(), "benchmark.one_core"),
            (self.memory(), "benchmark.memory"),
            (self.graphics(), "benchmark.graphics"),
        ];
        parts
            .iter()
            .fold(parts[0], |a, b| if b.0 < a.0 { *b } else { a })
            .1
    }
}

/// Keep `m` among the results: it replaces the one measured with
/// hardware acceleration the same way.
pub fn remember(list: &mut Vec<Measured>, m: Measured) {
    list.retain(|r| r.acceleration != m.acceleration);
    list.push(m);
    list.sort_by_key(|r| !r.acceleration);
}

/// A plain-text report of a run, to paste into a question or a report
/// about speed.
pub fn report(m: &Measured, version: &str) -> String {
    format!(
        "TraceDraw {version} benchmark\n\
         Overall: {:.1}\n\
         Processor (all {} cores): {:.1} ({:.0} ms)\n\
         Processor (one core): {:.1} ({:.0} ms)\n\
         Memory: {:.1} ({:.1} GB/s)\n\
         Graphics: {:.1} ({:.1} frames/s, slowest frame {:.0} ms)\n\
         Window drawn by: {}{}\n\
         Hardware acceleration: {}\n\
         System: {} {}",
        m.overall(),
        m.cores,
        m.all_cores(),
        m.all_cores_seconds * 1000.0,
        m.one_core(),
        m.one_core_seconds * 1000.0,
        m.memory(),
        m.memory_gbs,
        m.graphics(),
        m.fps,
        m.worst_frame * 1000.0,
        m.adapter,
        if m.software { " (software)" } else { "" },
        if m.acceleration { "on" } else { "off" },
        std::env::consts::OS,
        std::env::consts::ARCH,
    ) + &tasks_report(m)
}

/// The advanced test's lines of the report, in English.
fn tasks_report(m: &Measured) -> String {
    let mut out = String::new();
    let comfortable = m.comfortable_objects();
    if comfortable > 0 {
        out.push_str(&format!("\nComfortable up to about {comfortable} objects"));
    }
    for t in &m.tasks {
        let name = t
            .key
            .trim_start_matches("benchmark.task_")
            .replace('_', " ");
        out.push_str(&format!(
            "\n{name}, {} objects: {:.0} ms",
            t.objects,
            t.seconds * 1000.0
        ));
    }
    out
}

/// The sample drawing: an A4 page with closed curves in solid colours,
/// gradients, some see-through and some with a shadow, and outlines.
/// The same every run.
pub fn sample_document() -> Document {
    sample_document_of(SAMPLE_OBJECTS - 1)
}

/// The sample drawing with `n` curves (and the rectangle under them),
/// smaller the more there are, so that a large one stays a page of
/// separate objects.
pub fn sample_document_of(n: usize) -> Document {
    let size = (30.0 * (1500.0 / n.max(1) as f64).sqrt()).clamp(3.0, 30.0);
    let mut doc = Document::new("benchmark", Size::new(297.0, 210.0));
    let Some(layer) = doc
        .pages
        .first()
        .and_then(|p| doc.layers_for_page(p.id).ok())
        .and_then(|l| l.first().map(|l| l.id))
    else {
        return doc;
    };
    let mut seed = 7u64;
    let mut r = move || {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((seed >> 33) as f64) / (1u64 << 31) as f64
    };
    let mut shapes = Vec::new();
    for i in 0..n {
        let (x, y) = (r() * 280.0 + 8.0, r() * 195.0 + 8.0);
        let mut p = BezPath::new();
        p.move_to((x, y));
        for _ in 0..6 {
            let mut q = || (x + (r() - 0.5) * size, y + (r() - 0.5) * size);
            p.curve_to(q(), q(), q());
        }
        p.close_path();
        let id = doc.ids_mut().shape();
        let mut s = Shape::new(
            id,
            ShapeKind::Path {
                path: p,
                closed: true,
            },
        );
        let c = Color::rgb8(
            (r() * 255.0) as u8,
            (r() * 255.0) as u8,
            (r() * 255.0) as u8,
        );
        s.fill = if i % 3 == 0 {
            Fill::Fountain(Fountain::two(
                FountainKind::Linear,
                c,
                Color::BLACK,
                r() * 360.0,
            ))
        } else {
            Fill::Solid(c)
        };
        s.stroke = Some(Stroke {
            width: 0.3,
            ..Stroke::default()
        });
        if i % 5 == 0 {
            s.opacity = 0.6;
        }
        if i % 50 == 0 {
            s.shadow = Some(Shadow::default());
        }
        shapes.push(s);
    }
    let rect = Rect::new(20.0, 20.0, 277.0, 190.0);
    let id = doc.ids_mut().shape();
    let mut back = Shape::new(
        id,
        ShapeKind::Rect {
            rect,
            radius: 0.0,
            corners: None,
        },
    );
    back.fill = Fill::Solid(Color::rgb8(240, 240, 235));
    shapes.insert(0, back);
    if let Ok(l) = doc.layer_mut(layer) {
        l.shapes.extend(shapes);
    }
    doc
}

/// What the drawing tests draw: the whole sample page fitted into a full
/// HD picture.
pub fn sample_options() -> tracedraw_render::RenderOptions {
    let (w, h) = PICTURE;
    let zoom = (w as f64 / 297.0).min(h as f64 / 210.0);
    tracedraw_render::RenderOptions {
        width: w,
        height: h,
        view: tracedraw_render::ViewTransform {
            zoom,
            origin_x: (w as f64 - 297.0 * zoom) / 2.0,
            origin_y: h as f64 - (h as f64 - 210.0 * zoom) / 2.0,
            stretch_y: 1.0,
        },
        anti_alias: true,
        draft: true,
        ..Default::default()
    }
}

/// The best of `runs` times of `f`, seconds.
fn best_of(runs: usize, mut f: impl FnMut()) -> f64 {
    let mut best = f64::INFINITY;
    for _ in 0..runs.max(1) {
        let t = web_time::Instant::now();
        f();
        best = best.min(t.elapsed().as_secs_f64());
    }
    best
}

/// The processor tests: the sample drawing on all the cores and on one,
/// best of three each, seconds.
pub fn time_drawing() -> (f64, f64) {
    let doc = sample_document();
    let Some(page) = doc.pages.first().map(|p| p.id) else {
        return (0.0, 0.0);
    };
    let opts = sample_options();
    // One untimed run: fonts, caches and threads ready.
    let _ = tracedraw_render::render_page(&doc, page, &opts);
    let all = best_of(3, || {
        let _ = tracedraw_render::render_page(&doc, page, &opts);
    });
    let one = best_of(2, || {
        let _ = tracedraw_render::render_page_on_one_thread(&doc, page, &opts);
    });
    (all, one)
}

/// The advanced test on a drawing of `objects` objects: the whole view
/// drawn, drawn zoomed in four times, every object moved, the drawing
/// saved and opened as `.tdraw`, and exported as PDF. `step` is told
/// each task before it runs.
pub fn time_tasks(objects: usize, step: &mut dyn FnMut(&'static str)) -> Vec<Task> {
    let mut out = Vec::new();
    let doc = sample_document_of(objects.saturating_sub(1));
    let Some(page) = doc.pages.first().map(|p| p.id) else {
        return out;
    };
    let mut task = |key: &'static str, seconds: f64| {
        out.push(Task {
            key: key.to_string(),
            objects,
            seconds,
        })
    };
    let opts = sample_options();
    step(TASK_REDRAW);
    task(
        TASK_REDRAW,
        best_of(2, || {
            let _ = tracedraw_render::render_page(&doc, page, &opts);
        }),
    );
    step(TASK_ZOOM);
    let zoomed = zoomed_options(4.0);
    task(
        TASK_ZOOM,
        best_of(2, || {
            let _ = tracedraw_render::render_page(&doc, page, &zoomed);
        }),
    );
    // Every object moved, with the copy of the drawing that Undo keeps.
    step(TASK_MOVE);
    let mut moved = doc.clone();
    let shift = tracedraw_core::geometry::Affine::translate((5.0, 5.0));
    task(
        TASK_MOVE,
        best_of(1, || {
            let copy = moved.clone();
            for layer in moved.pages.iter_mut().flat_map(|p| p.layers.iter_mut()) {
                for s in layer.shapes.iter_mut() {
                    s.transform_by(shift);
                }
            }
            std::hint::black_box(copy);
        }),
    );
    step(TASK_SAVE);
    let mut bytes = Vec::new();
    task(
        TASK_SAVE,
        best_of(1, || {
            bytes.clear();
            let _ =
                doc.write_tdraw_version(&mut bytes, tracedraw_core::document::TDRAW_FORMAT_VERSION);
        }),
    );
    step(TASK_OPEN);
    task(
        TASK_OPEN,
        best_of(1, || {
            let _ = std::hint::black_box(Document::from_tdraw_bytes(&bytes));
        }),
    );
    step(TASK_PDF);
    task(
        TASK_PDF,
        best_of(1, || {
            std::hint::black_box(tracedraw_io::pdf::document_to_pdf(&doc));
        }),
    );
    out
}

/// Sixty rectangles with shadows drawn for the screen, with the graphics
/// card blurring the shadows and without it (Options > General, "Blur
/// shadows on the graphics card"); `card` turns that on or off for the
/// time of each half.
pub fn time_shadows(card: &mut dyn FnMut(bool), step: &mut dyn FnMut(&'static str)) -> Vec<Task> {
    let mut doc = sample_document_of(60);
    let mut k = 0;
    for layer in doc.pages.iter_mut().flat_map(|p| p.layers.iter_mut()) {
        for s in layer.shapes.iter_mut().skip(1) {
            let (x, y) = ((k % 10) as f64 * 27.0 + 15.0, (k / 10) as f64 * 30.0 + 15.0);
            k += 1;
            s.kind = ShapeKind::Rect {
                rect: Rect::new(x, y, x + 24.0, y + 24.0),
                radius: 2.0,
                corners: None,
            };
            s.shadow = Some(Shadow::default());
        }
    }
    let Some(page) = doc.pages.first().map(|p| p.id) else {
        return Vec::new();
    };
    let opts = sample_options();
    let mut out = Vec::new();
    for (on, key) in [(true, TASK_SHADOWS_CARD), (false, TASK_SHADOWS_PROCESSOR)] {
        step(key);
        card(on);
        let _ = tracedraw_render::render_page(&doc, page, &opts);
        let seconds = best_of(2, || {
            let _ = tracedraw_render::render_page(&doc, page, &opts);
        });
        out.push(Task {
            key: key.to_string(),
            objects: k + 1,
            seconds,
        });
    }
    out
}

/// [`sample_options`] zoomed in `times` about the middle of the page.
pub fn zoomed_options(times: f64) -> tracedraw_render::RenderOptions {
    let mut o = sample_options();
    let (w, h) = PICTURE;
    o.view.zoom *= times;
    o.view.origin_x = w as f64 / 2.0 - 148.5 * o.view.zoom;
    o.view.origin_y = h as f64 / 2.0 + 105.0 * o.view.zoom;
    o
}

/// The memory test: gigabytes a second copying a 64 MB block, best of
/// five.
pub fn time_memory() -> f64 {
    const SIZE: usize = 64 << 20;
    let src: Vec<u8> = (0..SIZE).map(|i| (i % 251) as u8).collect();
    let mut dst = vec![0u8; SIZE];
    let t = best_of(5, || {
        dst.copy_from_slice(&src);
        std::hint::black_box(&mut dst);
    });
    if t > 0.0 {
        SIZE as f64 / t / 1e9
    } else {
        0.0
    }
}

/// Processor cores (threads) the drawing uses.
pub fn cores() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get())
}

/// Seconds since 1970, 0 when the clock is not there (never panics in a
/// browser).
pub fn now() -> u64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scores_go_up_one_for_each_halving_of_the_time() {
        assert_eq!(time_score(0.4, 0.4), 5.0);
        assert_eq!(time_score(0.4, 0.2), 6.0);
        assert_eq!(time_score(0.4, 0.8), 4.0);
        assert_eq!(time_score(0.4, 0.1), 7.0);
        // Kept between 1.0 and 9.9.
        assert_eq!(time_score(0.4, 1000.0), MIN_SCORE);
        assert_eq!(time_score(0.4, 0.000_001), MAX_SCORE);
        assert_eq!(time_score(0.4, 0.0), MAX_SCORE);
        assert_eq!(score(f64::NAN), MIN_SCORE);
        assert_eq!(score(0.0), MIN_SCORE);
        // A screen's 60 frames a second scores 7.0.
        assert_eq!(score(60.0 / REF_FPS), 7.0);
    }

    fn sample(fps: f64) -> Measured {
        Measured {
            acceleration: true,
            adapter: "Card".into(),
            cores: 4,
            all_cores_seconds: 0.1,
            one_core_seconds: 0.3,
            memory_gbs: 16.0,
            fps,
            worst_frame: 0.02,
            ..Default::default()
        }
    }

    #[test]
    fn the_overall_score_is_the_lowest_part() {
        let m = sample(60.0);
        assert_eq!(m.all_cores(), 7.0);
        assert_eq!(m.one_core(), 7.0);
        assert_eq!(m.memory(), 7.0);
        assert_eq!(m.graphics(), 7.0);
        assert_eq!(m.overall(), 7.0);
        assert!(!m.slow());
        // The window drawn on the processor, 4 frames a second.
        let m = sample(4.0);
        assert_eq!(m.graphics(), 3.0);
        assert_eq!(m.overall(), 3.0);
        assert!(m.slow());
        assert_eq!(m.weakest(), "benchmark.graphics");
    }

    #[test]
    fn one_result_is_kept_for_each_acceleration_setting() {
        let mut list = Vec::new();
        remember(&mut list, sample(60.0));
        let mut off = sample(5.0);
        off.acceleration = false;
        remember(&mut list, off.clone());
        remember(&mut list, sample(50.0));
        assert_eq!(list.len(), 2);
        assert!(list[0].acceleration && list[0].fps == 50.0);
        assert_eq!(list[1], off);
    }

    #[test]
    fn the_report_has_every_part() {
        let r = report(&sample(4.0), "1.2.3");
        for want in [
            "TraceDraw 1.2.3",
            "Overall: 3.0",
            "all 4 cores",
            "Graphics: 3.0",
            "Hardware acceleration: on",
        ] {
            assert!(r.contains(want), "{want} in {r}");
        }
    }

    #[test]
    fn the_sample_drawing_draws() {
        let doc = sample_document();
        let page = doc.pages[0].id;
        let n: usize = doc
            .layers_for_page(page)
            .unwrap()
            .iter()
            .map(|l| l.shapes.len())
            .sum();
        assert_eq!(n, 1501);
        let mut opts = sample_options();
        // Small, to keep the test quick.
        opts.width /= 8;
        opts.height /= 8;
        opts.view.zoom /= 8.0;
        opts.view.origin_x /= 8.0;
        opts.view.origin_y /= 8.0;
        let pm = tracedraw_render::render_page(&doc, page, &opts).unwrap();
        // The page's middle is painted.
        let px = pm.pixel(opts.width / 2, opts.height / 2).unwrap();
        assert!(px.alpha() > 0);
        assert!(time_memory() > 0.0);
        assert!(cores() >= 1);
    }

    #[test]
    fn tasks_are_rated_and_objects_rounded() {
        assert_eq!(rating(TASK_REDRAW, 0.2), Rating::Good);
        assert_eq!(rating(TASK_REDRAW, 0.6), Rating::Fair);
        assert_eq!(rating(TASK_REDRAW, 1.5), Rating::Slow);
        assert_eq!(rating(TASK_PDF, 1.5), Rating::Fair);
        assert_eq!(rating(TASK_SAVE, 0.9), Rating::Good);
        assert_eq!(round_objects(400.0), 0);
        assert_eq!(round_objects(999.0), 500);
        assert_eq!(round_objects(4_800.0), 2_000);
        assert_eq!(round_objects(5_000.0), 5_000);
        assert_eq!(round_objects(73_000.0), 50_000);
        // The quick test's drawing: 1501 objects in 0.1 s, 3 750 in 0.25 s.
        let m = sample(60.0);
        assert_eq!(m.comfortable_objects(), 2_000);
    }

    /// The advanced test's tasks on a small drawing: every task, in
    /// order, each told before it runs.
    #[test]
    fn the_tasks_of_the_advanced_test_run() {
        let mut told = Vec::new();
        let tasks = time_tasks(200, &mut |k| told.push(k));
        let keys: Vec<&str> = tasks.iter().map(|t| t.key.as_str()).collect();
        let want = [
            TASK_REDRAW,
            TASK_ZOOM,
            TASK_MOVE,
            TASK_SAVE,
            TASK_OPEN,
            TASK_PDF,
        ];
        assert_eq!(keys, want);
        assert_eq!(told, want);
        assert!(tasks.iter().all(|t| t.objects == 200 && t.seconds >= 0.0));
        let mut card = Vec::new();
        let shadows = time_shadows(&mut |on| card.push(on), &mut |_| {});
        assert_eq!(card, [true, false]);
        assert_eq!(shadows.len(), 2);
        assert_eq!(shadows[0].objects, 61);
    }

    /// The whole benchmark on this computer, to see the numbers:
    /// `cargo test --release -p tracedraw -- --ignored benchmark_here --nocapture`.
    #[test]
    #[ignore]
    fn benchmark_here() {
        let (all, one) = time_drawing();
        let m = Measured {
            cores: cores(),
            all_cores_seconds: all,
            one_core_seconds: one,
            memory_gbs: time_memory(),
            fps: 60.0,
            ..Default::default()
        };
        eprintln!("{}", report(&m, "test"));
    }
}
