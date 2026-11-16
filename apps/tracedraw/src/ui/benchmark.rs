//! The Help > Benchmark window: runs the tests of `crate::benchmark`
//! (the processor ones on a thread of their own, the graphics one over a
//! few frames of the window), shows a score for each part and the
//! overall one, warns when the computer is slow for large drawings
//! (and turns on Light zoom then), and keeps the result with hardware
//! acceleration on and off side by side.

use crate::app::App;
use crate::benchmark::{self as bench, Measured};
use crate::i18n::{tr, trf};
use crate::theme::Tokens;
use crate::ui::tips::RichTip;
use egui::{Color32, ColorImage, Context, RichText, TextureHandle, TextureOptions, Ui};

/// Frames of the graphics test before it starts timing (the textures
/// made, the window settled).
const WARM_FRAMES: usize = 5;

/// The colour of a low score and of the warning, readable on light
/// and dark themes.
const WARN: Color32 = Color32::from_rgb(0xD8, 0x3B, 0x2B);

/// Where a run stands.
#[derive(Default)]
enum Phase {
    #[default]
    Idle,
    /// The processor and memory tests running on their thread.
    #[cfg(not(target_arch = "wasm32"))]
    Processor(std::sync::mpsc::Receiver<(f64, f64, f64)>),
    /// The graphics test: frames of the window, each handing it new
    /// pictures.
    Graphics(Box<Graphics>),
}

struct Graphics {
    /// What the processor tests measured: all cores, one core, memory.
    cpu: (f64, f64, f64),
    /// Frames shown so far.
    frames: usize,
    /// When the timed frames began, and the last frame.
    started: Option<web_time::Instant>,
    last: Option<web_time::Instant>,
    worst: f64,
    images: [ColorImage; 2],
    textures: Vec<TextureHandle>,
}

/// The benchmark window's state, kept between frames.
#[derive(Default)]
pub struct Bench {
    phase: Phase,
    /// The run just finished.
    pub done: Option<Measured>,
    /// Light zoom was turned on because the score is low.
    pub light_zoom_turned_on: bool,
}

impl Bench {
    pub fn running(&self) -> bool {
        !matches!(self.phase, Phase::Idle)
    }
}

/// A full HD test picture, stripes that differ between the two.
fn test_image(odd: bool) -> ColorImage {
    let (w, h) = bench::PICTURE;
    let mut img = ColorImage::new(
        [w as usize, h as usize],
        vec![Color32::WHITE; (w * h) as usize],
    );
    for (i, p) in img.pixels.iter_mut().enumerate() {
        let x = (i % w as usize) as u32;
        let y = (i / w as usize) as u32;
        let v = ((x / 16 + y / 16 + odd as u32) % 2) as u8;
        *p = Color32::from_rgb(40 + v * 160, 90 + (x % 256) as u8 / 4, 200 - v * 100);
    }
    img
}

/// Start a run.
pub fn start(app: &mut App) {
    app.benchmark.done = None;
    app.benchmark.light_zoom_turned_on = false;
    #[cfg(not(target_arch = "wasm32"))]
    {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let (all, one) = bench::time_drawing();
            let _ = tx.send((all, one, bench::time_memory()));
        });
        app.benchmark.phase = Phase::Processor(rx);
    }
    // No threads in the browser: measured in this frame.
    #[cfg(target_arch = "wasm32")]
    {
        let (all, one) = bench::time_drawing();
        app.benchmark.phase = graphics((all, one, bench::time_memory()));
    }
}

fn graphics(cpu: (f64, f64, f64)) -> Phase {
    Phase::Graphics(Box::new(Graphics {
        cpu,
        frames: 0,
        started: None,
        last: None,
        worst: 0.0,
        images: [test_image(false), test_image(true)],
        textures: Vec::new(),
    }))
}

/// A run's frame: the processor tests polled, or a frame of the graphics
/// test (its pictures shown small in `ui`). Ends the run once all is
/// measured.
fn step(app: &mut App, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    match &mut app.benchmark.phase {
        Phase::Idle => return,
        #[cfg(not(target_arch = "wasm32"))]
        Phase::Processor(rx) => match rx.try_recv() {
            Ok(cpu) => app.benchmark.phase = graphics(cpu),
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                ui.label(tr("benchmark.measuring_processor"));
            }
            Err(_) => app.benchmark.phase = Phase::Idle,
        },
        Phase::Graphics(g) => {
            ui.label(tr("benchmark.measuring_graphics"));
            let now = web_time::Instant::now();
            if g.frames == WARM_FRAMES {
                g.started = Some(now);
            } else if g.frames > WARM_FRAMES {
                if let Some(last) = g.last {
                    g.worst = g.worst.max(now.duration_since(last).as_secs_f64());
                }
            }
            g.last = Some(now);
            let pick = g.frames % 2;
            for k in 0..bench::UPLOADS_PER_FRAME {
                let image = g.images[(pick + k) % 2].clone();
                match g.textures.get_mut(k) {
                    Some(t) => t.set(image, TextureOptions::LINEAR),
                    None => g.textures.push(ctx.load_texture(
                        format!("benchmark-{k}"),
                        image,
                        TextureOptions::LINEAR,
                    )),
                }
            }
            ui.horizontal(|ui| {
                for t in &g.textures {
                    ui.image((t.id(), egui::vec2(96.0, 54.0)));
                }
            });
            g.frames += 1;
            if g.frames >= WARM_FRAMES + bench::GRAPHICS_FRAMES {
                let timed = (g.frames - WARM_FRAMES - 1) as f64;
                let secs = g
                    .started
                    .map_or(0.0, |s| now.duration_since(s).as_secs_f64());
                let fps = if secs > 0.0 { timed / secs } else { 0.0 };
                let (all, one, memory) = g.cpu;
                let m = Measured {
                    acceleration: !gpu_software(),
                    adapter: gpu_in_use().unwrap_or_default().to_string(),
                    software: gpu_software(),
                    cores: bench::cores(),
                    all_cores_seconds: all,
                    one_core_seconds: one,
                    memory_gbs: memory,
                    fps,
                    worst_frame: g.worst,
                    when: bench::now(),
                };
                app.benchmark.phase = Phase::Idle;
                finish(app, m);
            }
        }
    }
    ctx.request_repaint();
}

/// A run measured: kept, and Light zoom turned on when the computer is
/// slow for large drawings.
pub fn finish(app: &mut App, m: Measured) {
    log::info!(
        "benchmark: {}",
        bench::report(&m, env!("CARGO_PKG_VERSION")).replace('\n', "; ")
    );
    if m.slow() && !app.settings.light_zoom {
        app.settings.light_zoom = true;
        app.benchmark.light_zoom_turned_on = true;
    }
    bench::remember(&mut app.settings.benchmarks, m.clone());
    app.settings.save();
    app.benchmark.done = Some(m);
}

/// One row of scores: the part, its score and what was measured.
fn row(ui: &mut Ui, key: &str, score: f32, detail: String) {
    ui.label(tr(key));
    ui.label(RichText::new(format!("{score:.1}")).strong());
    ui.label(RichText::new(detail).color(Tokens::text_dim()));
    ui.end_row();
}

fn scores(ui: &mut Ui, m: &Measured) {
    egui::Grid::new("benchmark_scores")
        .num_columns(3)
        .spacing([18.0, 6.0])
        .show(ui, |ui| {
            row(
                ui,
                "benchmark.all_cores",
                m.all_cores(),
                trf(
                    "benchmark.ms_cores",
                    &[
                        ("ms", &format!("{:.0}", m.all_cores_seconds * 1000.0)),
                        ("cores", &m.cores.to_string()),
                    ],
                ),
            );
            row(
                ui,
                "benchmark.one_core",
                m.one_core(),
                format!("{:.0} ms", m.one_core_seconds * 1000.0),
            );
            row(
                ui,
                "benchmark.memory",
                m.memory(),
                format!("{:.1} GB/s", m.memory_gbs),
            );
            row(
                ui,
                "benchmark.graphics",
                m.graphics(),
                trf(
                    "benchmark.fps",
                    &[
                        ("fps", &format!("{:.1}", m.fps)),
                        ("ms", &format!("{:.0}", m.worst_frame * 1000.0)),
                    ],
                ),
            );
        });
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(tr("benchmark.overall")).size(16.0));
        let color = if m.slow() { WARN } else { Tokens::text() };
        ui.label(
            RichText::new(format!("{:.1}", m.overall()))
                .size(26.0)
                .strong()
                .color(color),
        );
        ui.label(RichText::new(tr("benchmark.scale")).color(Tokens::text_dim()));
    });
}

/// The window.
pub fn dialog(app: &mut App, ctx: &Context, close: &mut bool) {
    crate::ui::dialogs::window(ctx, tr("benchmark.title")).show(ctx, |ui| {
        ui.set_max_width(560.0);
        ui.label(tr("benchmark.intro"));
        ui.add_space(6.0);
        let window_by = gpu_in_use().map(|n| n.to_string());
        if let Some(name) = window_by {
            let key = if gpu_software() {
                "benchmark.window_software"
            } else {
                "benchmark.window_card"
            };
            ui.label(RichText::new(trf(key, &[("name", &name)])).color(Tokens::text_dim()));
        }
        ui.separator();
        if app.benchmark.running() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.vertical(|ui| step(app, ui));
            });
        } else if let Some(m) = app.benchmark.done.clone() {
            scores(ui, &m);
            if m.slow() {
                ui.add_space(4.0);
                ui.label(
                    RichText::new(trf(
                        "benchmark.slow",
                        &[
                            ("score", &format!("{:.1}", m.overall())),
                            ("limit", &format!("{:.1}", bench::SLOW_SCORE)),
                            ("part", &tr(m.weakest())),
                        ],
                    ))
                    .color(WARN),
                );
                if m.software && m.graphics() < bench::SLOW_SCORE {
                    ui.label(tr("benchmark.slow_software"));
                }
            } else {
                ui.label(tr("benchmark.fast"));
            }
            if app.benchmark.light_zoom_turned_on {
                ui.label(RichText::new(tr("benchmark.light_zoom_turned_on")).strong());
            }
        }
        ui.add_space(4.0);
        if ui
            .checkbox(&mut app.settings.light_zoom, tr("options.light_zoom"))
            .rich_tip(crate::ui::tips::pair(
                "options.light_zoom",
                "options.light_zoom_tip",
            ))
            .changed()
        {
            app.settings.save();
        }
        // With hardware acceleration and without, to compare.
        ui.separator();
        ui.label(RichText::new(tr("benchmark.compare")).strong());
        for on in [true, false] {
            let found = app
                .settings
                .benchmarks
                .iter()
                .find(|r| r.acceleration == on);
            let label = tr(if on {
                "benchmark.with_acceleration"
            } else {
                "benchmark.without_acceleration"
            });
            let value = found.map_or_else(
                || tr("benchmark.not_measured"),
                |r| format!("{:.1}", r.overall()),
            );
            ui.label(format!("{label}: {value}"));
        }
        ui.label(RichText::new(tr("benchmark.compare_how")).color(Tokens::text_dim()));
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let running = app.benchmark.running();
            if ui
                .add_enabled(!running, egui::Button::new(tr("benchmark.run")))
                .rich_tip_always(crate::ui::tips::pair(
                    "benchmark.run",
                    "menu.help.benchmark_tip",
                ))
                .clicked()
            {
                start(app);
            }
            let report = app
                .benchmark
                .done
                .as_ref()
                .map(|m| bench::report(m, env!("CARGO_PKG_VERSION")));
            if ui
                .add_enabled(report.is_some(), egui::Button::new(tr("benchmark.copy")))
                .rich_tip_always(crate::ui::tips::pair(
                    "benchmark.copy",
                    "benchmark.copy_tip",
                ))
                .clicked()
            {
                if let Some(r) = report {
                    ui.ctx().copy_text(r);
                    app.status = tr("benchmark.copied");
                }
            }
            if ui
                .button(tr("menu.tools.options"))
                .rich_tip(crate::ui::tips::pair(
                    "menu.tools.options",
                    "benchmark.compare_how",
                ))
                .clicked()
            {
                app.open_options_page(crate::ui::dialogs::OptionsPage::General);
            }
            if ui
                .add_enabled(!running, egui::Button::new(tr("dialog.close")))
                .rich_tip_always(crate::ui::tips::keyed("dialog.close"))
                .clicked()
            {
                *close = true;
            }
        });
    });
}

/// The adapter that draws the window; the browser build has no wgpu
/// adapter list of its own.
fn gpu_in_use() -> Option<&'static str> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        crate::gpu::in_use()
    }
    #[cfg(target_arch = "wasm32")]
    {
        None
    }
}

/// Whether the window is drawn by a software adapter (never known in the
/// browser).
fn gpu_software() -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        crate::gpu::software()
    }
    #[cfg(target_arch = "wasm32")]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slow_run() -> Measured {
        Measured {
            acceleration: false,
            software: true,
            adapter: "llvmpipe".into(),
            cores: 2,
            all_cores_seconds: 1.6,
            one_core_seconds: 3.2,
            memory_gbs: 2.0,
            fps: 3.0,
            worst_frame: 0.5,
            ..Default::default()
        }
    }

    #[test]
    fn a_slow_computer_turns_light_zoom_on() {
        let mut app = App::headless();
        app.settings.light_zoom = false;
        finish(&mut app, slow_run());
        assert!(app.settings.light_zoom);
        assert!(app.benchmark.light_zoom_turned_on);
        assert_eq!(app.settings.benchmarks.len(), 1);
        // A fast one leaves the choice alone.
        let mut app = App::headless();
        app.settings.light_zoom = false;
        let mut fast = slow_run();
        fast.all_cores_seconds = 0.1;
        fast.one_core_seconds = 0.2;
        fast.memory_gbs = 16.0;
        fast.fps = 60.0;
        assert!(!fast.slow());
        finish(&mut app, fast);
        assert!(!app.settings.light_zoom);
        assert!(!app.benchmark.light_zoom_turned_on);
    }

    fn run(app: &mut App, ctx: &Context, frames: usize) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 800.0),
            )),
            ..Default::default()
        };
        for _ in 0..frames {
            let mut out = ctx.run_ui(input.clone(), |ui| {
                crate::ui::dialogs::show(app, ui.ctx());
            });
            out.textures_delta.clear();
        }
    }

    #[test]
    fn the_window_draws_runs_and_shows_the_scores() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        app.dialog = crate::ui::dialogs::Dialog::Benchmark;
        run(&mut app, &ctx, 2);
        assert!(matches!(app.dialog, crate::ui::dialogs::Dialog::Benchmark));
        // The graphics test straight away (the processor one is slow in
        // a debug build).
        app.benchmark.phase = graphics((0.4, 0.8, 4.0));
        run(&mut app, &ctx, WARM_FRAMES + bench::GRAPHICS_FRAMES + 1);
        assert!(!app.benchmark.running());
        let m = app.benchmark.done.clone().expect("measured");
        assert!(m.fps > 0.0);
        assert_eq!(m.all_cores(), 5.0);
        // The scores and the comparison show.
        run(&mut app, &ctx, 2);
        assert!(app.settings.benchmarks.iter().any(|r| r == &m));
        // The processor tests' answer from their thread starts the
        // graphics test (the tests themselves are slow in a debug build).
        let (tx, rx) = std::sync::mpsc::channel();
        app.benchmark.phase = Phase::Processor(rx);
        run(&mut app, &ctx, 1);
        assert!(matches!(app.benchmark.phase, Phase::Processor(_)));
        tx.send((0.2, 0.4, 8.0)).unwrap();
        run(&mut app, &ctx, 1);
        assert!(matches!(app.benchmark.phase, Phase::Graphics(_)));
        run(&mut app, &ctx, WARM_FRAMES + bench::GRAPHICS_FRAMES + 1);
        assert_eq!(app.benchmark.done.as_ref().unwrap().all_cores(), 6.0);
    }
}
