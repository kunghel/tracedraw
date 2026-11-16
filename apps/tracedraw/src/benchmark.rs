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
    )
}

/// The sample drawing: an A4 page with closed curves in solid colours,
/// gradients, some see-through and some with a shadow, and outlines.
/// The same every run.
pub fn sample_document() -> Document {
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
    for i in 0..1500 {
        let (x, y) = (r() * 280.0 + 8.0, r() * 195.0 + 8.0);
        let mut p = BezPath::new();
        p.move_to((x, y));
        for _ in 0..6 {
            let mut q = || (x + r() * 30.0 - 15.0, y + r() * 30.0 - 15.0);
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
