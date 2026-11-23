//! The Help's "how to use" screenshots: the whole window drawn
//! headlessly, in English, with a small drawing set up for one tool or
//! command and the pointer driven through the steps a person takes
//! (press, drag, let go), captured at the moment the page describes.
//!
//! `web/help_shots.sh` runs `cargo test -p tracedraw --lib help_scenes
//! -- --ignored` with `HELP_SHOTS` set to the output folder
//! (`docs/help/img`). Each picture is named after the Help topic it
//! illustrates (`/` and `#` as `-`) with a step suffix (`-1`, `-2`) when
//! a section shows several; see docs/behavior/help.md.

use crate::app::App;
use crate::soft_canvas::{Canvas, Textures};
use tracedraw_core::geometry::{Point, Rect};
use tracedraw_core::{Color, Fill, ShapeId, ShapeKind, Stroke};

/// The window the scenes are drawn in, in interface points (one pixel
/// each).
const W: usize = 1200;
const H: usize = 780;

/// One pointer step, in page millimetres; each takes one frame.
#[derive(Clone, Copy)]
pub(crate) enum Step {
    /// The pointer rests here, no button held.
    Move(f64, f64),
    /// The primary button goes down here.
    Press(f64, f64),
    /// The pointer moves here with the button held.
    Drag(f64, f64),
    /// The button comes up here.
    Release(f64, f64),
    /// The pointer rests at this point of the window (interface
    /// points), as over a menu or a panel.
    Hover(f32, f32),
    /// A click at this point of the window.
    Click(f32, f32),
}

/// What part of the window the picture keeps.
#[derive(Clone, Copy)]
pub(crate) enum Crop {
    /// Menus, bars, toolbox and drawing window: all but the panels on
    /// the right.
    Work,
    /// The whole window, panels included.
    Window,
    /// The drawing window alone.
    Canvas,
}

pub(crate) struct Scene {
    /// The file name, without `.png`.
    pub name: &'static str,
    /// Builds the drawing and chooses the tool; runs before the first
    /// frame.
    pub setup: fn(&mut App),
    /// Runs once the window is laid out (the drawing window's size is
    /// known), before the zoom and the pointer steps: commands that place
    /// their result in the middle of the window, a zoom of its own.
    pub ready: Option<fn(&mut App, &egui::Context)>,
    /// Zoom to fit the drawing once laid out.
    pub fit: bool,
    pub steps: &'static [Step],
    /// Runs after the pointer steps, before the picture is taken.
    pub after: Option<fn(&mut App)>,
    pub crop: Crop,
    /// The window's height in interface points (its width is `W`).
    pub height: usize,
}

impl Default for Scene {
    fn default() -> Self {
        Scene {
            name: "",
            setup: |_| {},
            ready: None,
            fit: true,
            steps: &[],
            after: None,
            crop: Crop::Work,
            height: H,
        }
    }
}

/// A fresh app for a scene: an empty page in English, panels closed.
pub(crate) fn scene_app() -> App {
    // The texts of the scenes are in the bundled Selawik, so they look the
    // same on any computer, whatever fonts it has.
    static FONTS: std::sync::Once = std::sync::Once::new();
    FONTS.call_once(|| {
        tracedraw_text::install();
        let _ = tracedraw_text::fonts().add_fonts(vec![
            include_bytes!("../assets/fonts/Selawik-Regular.ttf").to_vec(),
            include_bytes!("../assets/fonts/Selawik-Bold.ttf").to_vec(),
        ]);
    });
    let mut app = App::headless();
    crate::i18n::set_language("en");
    app.show_welcome = false;
    app.show_dockers = false;
    app
}

/// Adds a shape with `fill` and an outline of `stroke` (none when
/// `None`), returning its id.
pub(crate) fn add(app: &mut App, kind: ShapeKind, fill: Fill, stroke: Option<Stroke>) -> ShapeId {
    app.new_shape_styled(kind, |s| {
        s.fill = fill;
        s.stroke = stroke;
    })
    .expect("a layer to draw on")
}

pub(crate) fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> ShapeKind {
    ShapeKind::Rect {
        rect: Rect::new(x0, y0, x1, y1),
        radius: 0.0,
        corners: None,
    }
}

pub(crate) fn ellipse(x0: f64, y0: f64, x1: f64, y1: f64) -> ShapeKind {
    ShapeKind::Ellipse {
        rect: Rect::new(x0, y0, x1, y1),
        arc: None,
    }
}

pub(crate) fn solid(r: u8, g: u8, b: u8) -> Fill {
    Fill::Solid(Color::rgb8(r, g, b))
}

pub(crate) fn line(width: f64) -> Option<Stroke> {
    Some(Stroke::new(Color::BLACK, width))
}

struct Driver {
    ctx: egui::Context,
    tex: Textures,
    time: f64,
    h: usize,
}

impl Driver {
    fn new(h: usize) -> Self {
        let ctx = egui::Context::default();
        crate::theme::install_fonts_bundled(&ctx);
        App::setup_context(&ctx);
        Driver {
            ctx,
            tex: Textures::default(),
            time: 0.0,
            h,
        }
    }

    /// One frame of the window; painted into a picture only with `paint`
    /// (the frames in between only run the app).
    fn frame_with(
        &mut self,
        app: &mut App,
        events: Vec<egui::Event>,
        paint: bool,
    ) -> Option<Canvas> {
        self.time += 0.25;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(W as f32, self.h as f32),
            )),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let mut out = self.ctx.run_ui(input, |ui| {
            let ctx = ui.ctx().clone();
            crate::ui::apply_theme(&ctx, &crate::settings::effective_theme(&app.settings));
            crate::ui::apply_shadows(&ctx, app.settings.ui_shadows);
            crate::ui::root(app, ui);
        });
        self.tex.apply(&out.textures_delta);
        let c = paint.then(|| {
            let shapes = std::mem::take(&mut out.shapes);
            let prims = self.ctx.tessellate(shapes, out.pixels_per_point);
            let mut c = Canvas::new(W, self.h);
            c.paint(&self.tex, &prims);
            c
        });
        for id in &out.textures_delta.free {
            self.tex.0.remove(id);
        }
        out.textures_delta.clear();
        c
    }

    fn frame(&mut self, app: &mut App, events: Vec<egui::Event>) {
        let _ = self.frame_with(app, events, false);
    }
}

impl Driver {
    /// Frames until the drawing window's picture, rendered on its own
    /// thread, has come in (two minutes at most), then one more.
    fn settle(&mut self, app: &mut App) {
        let start = web_time::Instant::now();
        loop {
            self.frame(app, Vec::new());
            if !app.raster.borrow().drawing() || start.elapsed().as_secs() > 120 {
                self.frame(app, Vec::new());
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
    }
}

fn pointer_events(app: &App, step: Step) -> Vec<egui::Event> {
    use egui::{Event, Modifiers, PointerButton};
    let at = |x: f64, y: f64| app.view.to_screen(Point::new(x, y));
    let button = |pos, pressed| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    match step {
        Step::Move(x, y) | Step::Drag(x, y) => vec![Event::PointerMoved(at(x, y))],
        Step::Press(x, y) => vec![Event::PointerMoved(at(x, y)), button(at(x, y), true)],
        Step::Release(x, y) => vec![Event::PointerMoved(at(x, y)), button(at(x, y), false)],
        Step::Hover(x, y) => vec![Event::PointerMoved(egui::pos2(x, y))],
        Step::Click(x, y) => {
            let p = egui::pos2(x, y);
            vec![Event::PointerMoved(p), button(p, true), button(p, false)]
        }
    }
}

/// Draws `scene`; the frame and the area of it to keep.
pub(crate) fn render(scene: &Scene) -> (Canvas, (usize, usize, usize, usize)) {
    let mut d = Driver::new(scene.height);
    let h = scene.height;
    let mut app = scene_app();
    (scene.setup)(&mut app);
    d.settle(&mut app);
    if let Some(f) = scene.ready {
        let ctx = d.ctx.clone();
        f(&mut app, &ctx);
        d.settle(&mut app);
    }
    if scene.fit {
        app.zoom_to_fit();
    }
    d.settle(&mut app);
    for s in scene.steps {
        let ev = pointer_events(&app, *s);
        d.frame(&mut app, ev);
        // Each step settles, as between two real pointer events.
        d.settle(&mut app);
    }
    if let Some(f) = scene.after {
        f(&mut app);
    }
    d.settle(&mut app);
    let c = d
        .frame_with(&mut app, Vec::new(), true)
        .unwrap_or_else(|| Canvas::new(W, h));
    let canvas = app.canvas_rect;
    let area = match scene.crop {
        Crop::Window => (0, 0, W, h),
        Crop::Work => (0, 0, (canvas.max.x.ceil() as usize + 18).min(W), h),
        Crop::Canvas => (
            canvas.min.x.floor().max(0.0) as usize,
            canvas.min.y.floor().max(0.0) as usize,
            (canvas.max.x.ceil() as usize).min(W),
            (canvas.max.y.ceil() as usize).min(h),
        ),
    };
    (c, area)
}

/// The scenes of the Help pages.
pub(crate) fn scenes() -> Vec<Scene> {
    crate::help_scene_list::all()
}

/// Draw every scene into `$HELP_SHOTS` (run by `web/help_shots.sh`);
/// `HELP_SHOTS_ONLY` (comma separated name prefixes) draws some.
#[test]
#[ignore]
fn help_scenes() {
    let Some(dir) = std::env::var_os("HELP_SHOTS") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).unwrap();
    let only: Vec<String> = std::env::var("HELP_SHOTS_ONLY")
        .map(|v| v.split(',').map(str::to_string).collect())
        .unwrap_or_default();
    for sc in scenes() {
        if !only.is_empty() && !only.iter().any(|o| sc.name.starts_with(o.as_str())) {
            continue;
        }
        eprintln!("scene {}", sc.name);
        let (c, area) = render(&sc);
        c.save(&dir.join(format!("{}.png", sc.name)), area);
    }
    crate::i18n::set_language("en");
}

/// Every scene has a name of its own, and every Help page that shows a
/// scene's picture names one that exists in `docs/help/img`.
#[test]
fn scene_names_are_unique_and_their_pictures_exist() {
    let img = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/help/img");
    let mut seen = std::collections::BTreeSet::new();
    for sc in scenes() {
        assert!(seen.insert(sc.name), "{}: two scenes of this name", sc.name);
        assert!(
            sc.name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
            "{}: lowercase ASCII, digits and hyphens",
            sc.name
        );
        assert!(
            img.join(format!("{}.png", sc.name)).exists(),
            "docs/help/img/{}.png is missing: run web/help_shots.sh",
            sc.name
        );
    }
}
