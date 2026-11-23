//! The Help > Benchmark window: a quick test (the processor, the memory
//! and the graphics, some seconds) or an advanced one (the same, then
//! real tasks on two large drawings, a minute or more), all in this
//! window. It shows the time of each task with a coloured bar, about how
//! many objects stay comfortable, a score for each part, suggests the
//! zoom that suits the computer (smooth, instant or light) and
//! measures with and without the graphics card, starting TraceDraw again
//! the other way when there is a card. See docs/behavior/benchmark.md.

use crate::app::App;
use crate::benchmark::{self as bench, Level, Measured, Rating, Task, ZoomStyle};
use crate::i18n::{tr, trf};
use crate::theme::Tokens;
use crate::ui::tips::RichTip;
use egui::{Color32, ColorImage, Context, RichText, TextureHandle, TextureOptions, Ui};

/// Frames of the graphics test before it starts timing (the textures
/// made, the window settled).
const WARM_FRAMES: usize = 5;

/// The colour of a low score, a slow task and the warning, readable on
/// light and dark themes.
const WARN: Color32 = Color32::from_rgb(0xD8, 0x3B, 0x2B);
/// A task that feels at once.
const GOOD: Color32 = Color32::from_rgb(0x2E, 0x9E, 0x4F);
/// A task that takes a moment.
const FAIR: Color32 = Color32::from_rgb(0xD9, 0x9A, 0x1B);

/// What the measuring thread tells the window.
enum Msg {
    /// A task begins: its key, and how many steps were done of how many.
    Step(&'static str, usize, usize),
    /// All measured: all cores, one core, memory, and the tasks.
    Done((f64, f64, f64), Vec<Task>),
}

/// Where a run stands.
#[derive(Default)]
enum Phase {
    #[default]
    Idle,
    /// The processor, memory and (advanced) task tests running on their
    /// thread, with the step it is at.
    #[cfg(not(target_arch = "wasm32"))]
    Processor {
        rx: std::sync::mpsc::Receiver<Msg>,
        level: Level,
        step: Option<(&'static str, usize, usize)>,
    },
    /// The graphics test: frames of the window, each handing it new
    /// pictures.
    Graphics(Box<Graphics>),
}

struct Graphics {
    level: Level,
    /// What the processor tests measured: all cores, one core, memory.
    cpu: (f64, f64, f64),
    tasks: Vec<Task>,
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
    /// The score is low, but the run used another renderer than a normal
    /// start does (Test Without / With the Graphics Card): Light zoom was
    /// left as it is (#3664).
    pub light_zoom_left_alone: bool,
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

/// Steps of a run on the measuring thread, to show how far it is.
fn steps(level: Level, card: bool) -> usize {
    match level {
        Level::Quick => 1,
        Level::Advanced => 1 + 6 * bench::ADVANCED_SIZES.len() + if card { 2 } else { 0 },
    }
}

/// Whether the graphics card can blur shadows in this start, so the
/// advanced test compares it with the processor.
fn card_blurs() -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        crate::gpu_fx::available()
    }
    #[cfg(target_arch = "wasm32")]
    {
        false
    }
}

/// Everything the measuring thread does: the processor and the memory,
/// then for the advanced test the tasks on the large drawings and the
/// shadows with and without the card.
/// `tell` stops the run with `false`: no one waits for it any more (the
/// window was closed or the run cancelled, #3665), so no further test
/// starts and its drawings are let go.
fn measure(
    level: Level,
    card: bool,
    tell: &mut dyn FnMut(&'static str, usize, usize) -> bool,
) -> Msg {
    let total = steps(level, card);
    let mut done = 0;
    let mut go = tell("benchmark.measuring_processor", done, total);
    let (all, one) = if go {
        bench::time_drawing()
    } else {
        (0.0, 0.0)
    };
    let memory = if go { bench::time_memory() } else { 0.0 };
    done += 1;
    let mut tasks = Vec::new();
    if level == Level::Advanced && go {
        for n in bench::ADVANCED_SIZES {
            tasks.extend(bench::time_tasks(n, &mut |key| {
                go = go && tell(key, done, total);
                done += 1;
                go
            }));
            if !go {
                break;
            }
        }
        if card && go {
            tasks.extend(bench::time_shadows(
                &mut |on| {
                    #[cfg(not(target_arch = "wasm32"))]
                    crate::gpu_fx::force(Some(on));
                    let _ = on;
                },
                &mut |key| {
                    go = go && tell(key, done, total);
                    done += 1;
                    go
                },
            ));
            #[cfg(not(target_arch = "wasm32"))]
            crate::gpu_fx::force(None);
        }
    }
    Msg::Done((all, one, memory), tasks)
}

/// Start a run.
pub fn start(app: &mut App, level: Level) {
    app.benchmark.done = None;
    app.benchmark.light_zoom_turned_on = false;
    app.benchmark.light_zoom_left_alone = false;
    let card = level == Level::Advanced && card_blurs();
    #[cfg(not(target_arch = "wasm32"))]
    {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let step = tx.clone();
            let done = measure(level, card, &mut |key, done, total| {
                step.send(Msg::Step(key, done, total)).is_ok()
            });
            let _ = tx.send(done);
        });
        app.benchmark.phase = Phase::Processor {
            rx,
            level,
            step: None,
        };
    }
    // No threads in the browser: measured in this frame.
    #[cfg(target_arch = "wasm32")]
    if let Msg::Done(cpu, tasks) = measure(level, card, &mut |_, _, _| true) {
        app.benchmark.phase = graphics(level, cpu, tasks);
    }
}

/// TraceDraw started again with or without the graphics card for the
/// benchmark: open it and run the test asked for.
pub fn resume(app: &mut App) {
    let Some(level) = app.settings.benchmark_resume.take() else {
        return;
    };
    app.settings.save();
    app.dialog = crate::ui::dialogs::Dialog::Benchmark;
    start(app, level);
}

/// Stop a run: the measuring thread starts no further test and lets its
/// drawings go, and the graphics test's pictures are dropped (#3665).
pub fn cancel(app: &mut App) {
    if !app.benchmark.running() {
        return;
    }
    // Dropping the phase drops the thread's receiver: its next step
    // finds no one listening and stops.
    app.benchmark.phase = Phase::Idle;
    #[cfg(not(target_arch = "wasm32"))]
    crate::gpu_fx::force(None);
    app.status = tr("benchmark.cancelled");
}

fn graphics(level: Level, cpu: (f64, f64, f64), tasks: Vec<Task>) -> Phase {
    Phase::Graphics(Box::new(Graphics {
        level,
        cpu,
        tasks,
        frames: 0,
        started: None,
        last: None,
        worst: 0.0,
        images: [test_image(false), test_image(true)],
        textures: Vec::new(),
    }))
}

/// What is being measured, with a bar of how far the run is.
fn progress(ui: &mut Ui, key: &str, done: usize, total: usize) {
    ui.label(tr(key));
    let share = if total > 0 {
        done as f32 / total as f32
    } else {
        0.0
    };
    ui.add(egui::ProgressBar::new(share).desired_width(320.0));
}

/// A run's frame: the measuring thread polled, or a frame of the
/// graphics test (its pictures shown small in `ui`). Ends the run once
/// all is measured.
fn step(app: &mut App, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    match &mut app.benchmark.phase {
        Phase::Idle => return,
        #[cfg(not(target_arch = "wasm32"))]
        Phase::Processor { rx, level, step } => {
            let level = *level;
            let mut next = None;
            loop {
                match rx.try_recv() {
                    Ok(Msg::Step(key, done, total)) => *step = Some((key, done, total)),
                    Ok(Msg::Done(cpu, tasks)) => {
                        next = Some(graphics(level, cpu, tasks));
                        break;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(_) => {
                        next = Some(Phase::Idle);
                        break;
                    }
                }
            }
            match next {
                Some(p) => app.benchmark.phase = p,
                None => {
                    let (key, done, total) =
                        step.unwrap_or(("benchmark.measuring_processor", 0, 1));
                    progress(ui, key, done, total);
                }
            }
        }
        Phase::Graphics(g) => {
            let total = bench::GRAPHICS_FRAMES + WARM_FRAMES;
            progress(ui, "benchmark.measuring_graphics", g.frames, total);
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
            if g.frames >= total {
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
                    level: g.level,
                    tasks: std::mem::take(&mut g.tasks),
                };
                app.benchmark.phase = Phase::Idle;
                finish(app, m);
            }
        }
    }
    ctx.request_repaint();
}

/// A run measured: kept, and Light zoom turned on when the computer is
/// slow for large drawings. A run made with another renderer than a
/// normal start uses (Test Without the Graphics Card) only compares: it
/// changes no option, or the software renderer's low score turned Light
/// zoom on for everyday use with the card (#3664).
pub fn finish(app: &mut App, m: Measured) {
    let usual = usual_renderer(&app.settings);
    finish_with(app, m, usual);
}

/// [`finish`] for a run made with the usual renderer or not.
fn finish_with(app: &mut App, m: Measured, usual: bool) {
    log::info!(
        "benchmark: {}",
        bench::report(&m, env!("CARGO_PKG_VERSION")).replace('\n', "; ")
    );
    if m.slow() && !app.settings.light_zoom {
        if usual {
            app.settings.light_zoom = true;
            app.benchmark.light_zoom_turned_on = true;
        } else {
            app.benchmark.light_zoom_left_alone = true;
        }
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

fn rating_color(r: Rating) -> Color32 {
    match r {
        Rating::Good => GOOD,
        Rating::Fair => FAIR,
        Rating::Slow => WARN,
    }
}

/// A bar as long as the task's share of the time that feels slow, in
/// the colour of how it feels.
fn bar(ui: &mut Ui, task: &Task) {
    let rating = bench::rating(&task.key, task.seconds);
    let slow_at = match rating_limit(&task.key) {
        l if l > 0.0 => l,
        _ => 1.0,
    };
    let share = (task.seconds / slow_at).clamp(0.02, 1.0) as f32;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(140.0, 12.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, Tokens::text_dim().gamma_multiply(0.25));
    let mut fill = rect;
    fill.set_width(rect.width() * share);
    painter.rect_filled(fill, 3.0, rating_color(rating));
}

/// Seconds from which a task is slow (the end of its bar).
fn rating_limit(key: &str) -> f64 {
    if bench::rating(key, 1.01) == Rating::Slow {
        1.0
    } else {
        4.0
    }
}

/// The tasks of the advanced test, a row each: what, on how many
/// objects, its bar and its time.
fn tasks(ui: &mut Ui, m: &Measured) {
    if m.tasks.is_empty() {
        return;
    }
    ui.add_space(6.0);
    ui.label(RichText::new(tr("benchmark.tasks")).strong());
    egui::Grid::new("benchmark_tasks")
        .num_columns(4)
        .spacing([14.0, 4.0])
        .show(ui, |ui| {
            for t in &m.tasks {
                ui.label(tr(&t.key));
                ui.label(
                    RichText::new(trf("benchmark.objects", &[("n", &group(t.objects as u64))]))
                        .color(Tokens::text_dim()),
                );
                bar(ui, t);
                ui.label(format!("{:.2} s", t.seconds));
                ui.end_row();
            }
        });
}

/// A count with its thousands apart by a narrow space, which every
/// language reads (10 000, 50 000).
fn group(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push('\u{202F}');
        }
        out.push(c);
    }
    out
}

/// The results of the run just finished.
fn results(app: &App, ui: &mut Ui, m: &Measured) {
    let comfortable = m.comfortable_objects();
    if comfortable > 0 {
        let color = if m.slow() { WARN } else { GOOD };
        ui.label(
            RichText::new(trf("benchmark.comfortable", &[("n", &group(comfortable))]))
                .size(16.0)
                .strong()
                .color(color),
        );
        ui.add_space(4.0);
    }
    scores(ui, m);
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
    if app.benchmark.light_zoom_left_alone {
        ui.label(tr("benchmark.light_zoom_left_alone"));
    }
    tasks(ui, m);
}

/// The zoom styles, the one in use picked and the one that suits this
/// computer marked once a run has measured it; a click uses it.
fn zoom(app: &mut App, ui: &mut Ui) {
    ui.label(RichText::new(tr("benchmark.zoom_title")).strong());
    let current = ZoomStyle::of(&app.settings);
    let suggested = app.benchmark.done.as_ref().map(|m| m.suggested_zoom());
    for style in ZoomStyle::ALL {
        let tip = format!("{}_tip", style.key());
        ui.horizontal(|ui| {
            if ui
                .radio(current == style, tr(style.key()))
                .rich_tip(crate::ui::tips::pair(style.key(), &tip))
                .clicked()
                && current != style
            {
                style.apply(&mut app.settings);
                app.settings.save();
            }
            if suggested == Some(style) {
                ui.label(
                    RichText::new(tr("benchmark.recommended"))
                        .strong()
                        .color(GOOD),
                );
            }
        });
        // What the style does, always shown under it.
        ui.label(RichText::new(tr(&tip)).color(Tokens::text_dim()));
    }
    ui.label(RichText::new(tr("benchmark.zoom_change_later")).color(Tokens::text_dim()));
    if let Some(s) = suggested {
        let why = if s == ZoomStyle::Light {
            "benchmark.zoom_why_light"
        } else {
            "benchmark.zoom_why_instant"
        };
        ui.label(RichText::new(tr(why)).color(Tokens::text_dim()));
    }
}

/// With and without the graphics card: the results of both kept, and a
/// button that starts TraceDraw again the other way and runs the quick
/// test there, when there is a card.
fn card(app: &mut App, ui: &mut Ui, close: &mut bool) {
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
            |r| {
                trf(
                    "benchmark.compare_value",
                    &[
                        ("score", &format!("{:.1}", r.overall())),
                        ("graphics", &format!("{:.1}", r.graphics())),
                    ],
                )
            },
        );
        ui.label(format!("{label}: {value}"));
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let Some(name) = crate::gpu::card_found() else {
            ui.label(RichText::new(tr("benchmark.no_card")).color(Tokens::text_dim()));
            return;
        };
        let software = gpu_software();
        let (key, mode) = if software {
            ("benchmark.test_with_card", crate::gpu::Mode::Hardware)
        } else {
            ("benchmark.test_without_card", crate::gpu::Mode::Software)
        };
        let running = app.benchmark.running();
        if ui
            .add_enabled(!running, egui::Button::new(trf(key, &[("name", name)])))
            .rich_tip_always(crate::ui::tips::pair(key, "benchmark.restart_tip"))
            .clicked()
        {
            app.settings.benchmark_resume = Some(Level::Quick);
            app.settings.save();
            crate::gpu::restart_after_exit(Some(mode));
            app.request_exit();
            *close = true;
        }
    }
    #[cfg(target_arch = "wasm32")]
    let _ = close;
}

/// The window.
pub fn dialog(app: &mut App, ctx: &Context, close: &mut bool) {
    crate::ui::dialogs::window(ctx, tr("benchmark.title")).show(ctx, |ui| {
        ui.set_max_width(600.0);
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
        let running = app.benchmark.running();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!running, egui::Button::new(tr("benchmark.quick")))
                .rich_tip_always(crate::ui::tips::pair(
                    "benchmark.quick",
                    "benchmark.quick_tip",
                ))
                .clicked()
            {
                start(app, Level::Quick);
            }
            // The browser has no thread to measure on: the advanced test
            // would hold the page for a minute.
            if cfg!(not(target_arch = "wasm32"))
                && ui
                    .add_enabled(!running, egui::Button::new(tr("benchmark.advanced")))
                    .rich_tip_always(crate::ui::tips::pair(
                        "benchmark.advanced",
                        "benchmark.advanced_tip",
                    ))
                    .clicked()
            {
                start(app, Level::Advanced);
            }
        });
        if cfg!(not(target_arch = "wasm32")) {
            ui.label(
                RichText::new(tr("benchmark.advanced_warning"))
                    .color(Tokens::text_dim())
                    .size(11.0),
            );
        }
        ui.add_space(4.0);
        if app.benchmark.running() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.vertical(|ui| step(app, ui));
            });
        } else if let Some(m) = app.benchmark.done.clone() {
            results(app, ui, &m);
        }
        ui.separator();
        zoom(app, ui);
        ui.separator();
        card(app, ui, close);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let running = app.benchmark.running();
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
            // Options takes the place of this window: the dialogs show
            // one at a time, and a window left open would put itself back
            // over the one it opened (#3351).
            if ui
                .add_enabled(!running, egui::Button::new(tr("menu.tools.options")))
                .rich_tip(crate::ui::tips::pair(
                    "menu.tools.options",
                    "benchmark.options_tip",
                ))
                .clicked()
            {
                app.open_options_page(crate::ui::dialogs::OptionsPage::General);
                *close = true;
            }
            // Close stops a run too: a test that takes too long (the
            // window drawn on the processor) can be left (#3665).
            if ui
                .button(tr("dialog.close"))
                .rich_tip_always(crate::ui::tips::keyed("dialog.close"))
                .clicked()
            {
                *close = true;
            }
        });
    });
    if *close {
        cancel(app);
    }
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

/// Whether this run draws with the renderer a normal start uses (what
/// Options asks for, or the one that last opened the window when that
/// could not): false for a run started by Test Without / With the
/// Graphics Card. Always true in the browser and in tests, which have no
/// renderer of their own to compare.
fn usual_renderer(s: &crate::settings::Settings) -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        match crate::gpu::mode_in_use() {
            Some(m) => m == crate::gpu::Mode::for_start(&[], s, None),
            None => true,
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = s;
        true
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

    /// #3664: a slow run made with another renderer than a normal start
    /// (Test Without the Graphics Card) is kept but turns no option on,
    /// and the window says so.
    #[test]
    fn a_comparison_run_leaves_light_zoom_alone() {
        let mut app = App::headless();
        app.settings.light_zoom = false;
        finish_with(&mut app, slow_run(), false);
        assert!(!app.settings.light_zoom);
        assert!(!app.benchmark.light_zoom_turned_on);
        assert!(app.benchmark.light_zoom_left_alone);
        assert_eq!(app.settings.benchmarks.len(), 1);
    }

    /// Closing the window stops a run: the measuring thread is told no
    /// one listens and starts no further test (#3665).
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn closing_the_window_stops_a_run() {
        let mut told = 0;
        let msg = measure(Level::Advanced, false, &mut |_, _, _| {
            told += 1;
            false
        });
        assert_eq!(told, 1);
        let Msg::Done(_, tasks) = msg else {
            panic!("a stopped run still ends");
        };
        assert!(tasks.is_empty());
        let mut app = App::headless();
        start(&mut app, Level::Quick);
        assert!(app.benchmark.running());
        cancel(&mut app);
        assert!(!app.benchmark.running());
        assert!(app.benchmark.done.is_none());
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
        app.benchmark.phase = graphics(Level::Quick, (0.4, 0.8, 4.0), Vec::new());
        run(&mut app, &ctx, WARM_FRAMES + bench::GRAPHICS_FRAMES + 1);
        assert!(!app.benchmark.running());
        let m = app.benchmark.done.clone().expect("measured");
        assert!(m.fps > 0.0);
        assert_eq!(m.all_cores(), 5.0);
        // The scores and the comparison show.
        run(&mut app, &ctx, 2);
        assert!(app.settings.benchmarks.iter().any(|r| r == &m));
        // The thread's steps show, then its answer starts the graphics
        // test (the tests themselves are slow in a debug build).
        let (tx, rx) = std::sync::mpsc::channel();
        app.benchmark.phase = Phase::Processor {
            rx,
            level: Level::Advanced,
            step: None,
        };
        run(&mut app, &ctx, 1);
        assert!(matches!(app.benchmark.phase, Phase::Processor { .. }));
        tx.send(Msg::Step(bench::TASK_ZOOM, 3, 13)).unwrap();
        run(&mut app, &ctx, 1);
        assert!(matches!(
            app.benchmark.phase,
            Phase::Processor {
                step: Some((bench::TASK_ZOOM, 3, 13)),
                ..
            }
        ));
        let tasks = vec![Task {
            key: bench::TASK_REDRAW.into(),
            objects: 50_000,
            seconds: 2.5,
        }];
        tx.send(Msg::Done((0.2, 0.4, 8.0), tasks.clone())).unwrap();
        run(&mut app, &ctx, 1);
        assert!(matches!(app.benchmark.phase, Phase::Graphics(_)));
        run(&mut app, &ctx, WARM_FRAMES + bench::GRAPHICS_FRAMES + 1);
        let m = app.benchmark.done.clone().unwrap();
        assert_eq!(m.all_cores(), 6.0);
        assert_eq!(m.level, Level::Advanced);
        assert_eq!(m.tasks, tasks);
        // 50 000 objects in 2.5 s: 5 000 in a quarter of a second.
        assert_eq!(m.comfortable_objects(), 5_000);
        run(&mut app, &ctx, 2);
    }

    /// The zoom styles in the window: the one in use picked; a click on
    /// another sets the zoom options it stands for.
    #[test]
    fn a_zoom_style_is_used_from_the_window() {
        let mut app = App::headless();
        app.settings.smooth_zoom = true;
        app.settings.light_zoom = false;
        assert_eq!(ZoomStyle::of(&app.settings), ZoomStyle::Smooth);
        ZoomStyle::Instant.apply(&mut app.settings);
        assert!(!app.settings.smooth_zoom && !app.settings.light_zoom);
        assert_eq!(ZoomStyle::of(&app.settings), ZoomStyle::Instant);
        ZoomStyle::Light.apply(&mut app.settings);
        assert_eq!(ZoomStyle::of(&app.settings), ZoomStyle::Light);
        // A slow computer is told Light, the others Instant.
        let mut m = slow_run();
        assert_eq!(m.suggested_zoom(), ZoomStyle::Light);
        m.all_cores_seconds = 0.1;
        m.one_core_seconds = 0.2;
        m.memory_gbs = 16.0;
        m.fps = 60.0;
        assert_eq!(m.suggested_zoom(), ZoomStyle::Instant);
    }

    /// TraceDraw started again for the benchmark opens it and runs the
    /// test asked for, once.
    #[test]
    fn a_start_for_the_benchmark_runs_it() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        app.settings.benchmark_resume = Some(Level::Quick);
        app.show_welcome = false;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 800.0),
            )),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| crate::ui::root(&mut app, ui));
        out.textures_delta.clear();
        assert!(app.settings.benchmark_resume.is_none());
        assert!(matches!(app.dialog, crate::ui::dialogs::Dialog::Benchmark));
        assert!(app.benchmark.running());
    }
}
