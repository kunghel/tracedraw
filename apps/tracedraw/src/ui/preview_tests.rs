//! Full-window regressions for the viewport captured by F9.

use super::*;
use egui::{Event, Key, Modifiers, Pos2, Vec2, ViewportCommand, ViewportId};
use tracedraw_core::geometry::Rect;

const WINDOW: Vec2 = Vec2::new(1400.0, 900.0);

fn timed_frame(
    ctx: &egui::Context,
    app: &mut App,
    time: f64,
    size: Vec2,
    events: Vec<Event>,
) -> egui::FullOutput {
    let mut raw = input(size, app.fullscreen_preview, events);
    raw.time = Some(time);
    let mut output = ctx.run_ui(raw, |ui| crate::ui::root(app, ui));
    output.textures_delta.clear();
    output
}

fn has_exit_hint(output: &egui::FullOutput) -> bool {
    output.shapes.iter().any(|shape| {
        matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text().starts_with(&tr("preview.exit_hint")))
    })
}

fn has_exit_hint_background(output: &egui::FullOutput) -> bool {
    output.shapes.iter().any(|shape| {
        matches!(&shape.shape, egui::Shape::Rect(rect)
            if rect.fill == egui::Color32::from_gray(40) && rect.rect.height() < 50.0)
    })
}

#[test]
fn the_exit_hint_hides_after_one_and_a_half_seconds_and_stays_hidden() {
    let (ctx, mut app) = setup();
    let output = timed_frame(&ctx, &mut app, 10.0, WINDOW, vec![key(Key::F9, true)]);
    assert!(has_exit_hint(&output));
    assert!(has_exit_hint_background(&output));
    timed_frame(&ctx, &mut app, 10.1, WINDOW, vec![key(Key::F9, false)]);
    // Let egui finish the immediate repaints caused by opening the preview.
    for time in [10.2, 10.3, 10.4] {
        timed_frame(&ctx, &mut app, time, WINDOW, Vec::new());
    }
    let output = timed_frame(&ctx, &mut app, 11.0, WINDOW, Vec::new());
    let delay = output.viewport_output[&ViewportId::ROOT].repaint_delay;
    assert!(delay > std::time::Duration::ZERO);
    assert!(delay <= std::time::Duration::from_millis(500));
    let output = timed_frame(&ctx, &mut app, 11.49, WINDOW, Vec::new());
    assert!(has_exit_hint(&output));
    assert!(has_exit_hint_background(&output));

    let output = timed_frame(&ctx, &mut app, 11.5, WINDOW, Vec::new());
    assert!(!has_exit_hint(&output));
    assert!(!has_exit_hint_background(&output));
    assert!(app.fullscreen_preview, "only the hint disappears");
    let output = timed_frame(
        &ctx,
        &mut app,
        15.0,
        egui::vec2(1920.0, 1080.0),
        vec![Event::PointerMoved(egui::pos2(700.0, 450.0))],
    );
    assert!(
        !has_exit_hint(&output),
        "resize and pointer motion do not restart the hint"
    );
    assert!(!has_exit_hint_background(&output));
    let output = timed_frame(&ctx, &mut app, 20.0, WINDOW, vec![key(Key::Escape, true)]);
    assert!(!has_exit_hint(&output));
    assert!(
        !app.fullscreen_preview,
        "Escape still exits after the hint hides"
    );
    timed_frame(&ctx, &mut app, 20.1, WINDOW, vec![key(Key::Escape, false)]);
    let output = timed_frame(&ctx, &mut app, 30.0, WINDOW, vec![key(Key::F9, true)]);
    assert!(has_exit_hint(&output), "a new preview gets a fresh hint");
    assert!(has_exit_hint_background(&output));
    timed_frame(&ctx, &mut app, 30.1, WINDOW, vec![key(Key::F9, false)]);
    let output = timed_frame(&ctx, &mut app, 31.5, WINDOW, Vec::new());
    assert!(!has_exit_hint(&output));
    assert!(!has_exit_hint_background(&output));
}

fn input(size: Vec2, fullscreen: bool, events: Vec<Event>) -> egui::RawInput {
    let mut input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, size)),
        events,
        ..Default::default()
    };
    input.viewports.insert(
        ViewportId::ROOT,
        egui::ViewportInfo {
            fullscreen: Some(fullscreen),
            focused: Some(true),
            ..Default::default()
        },
    );
    input
}

fn frame(
    ctx: &egui::Context,
    app: &mut App,
    size: Vec2,
    fullscreen: bool,
    events: Vec<Event>,
) -> Vec<ViewportCommand> {
    let mut output = ctx.run_ui(input(size, fullscreen, events), |ui| {
        crate::ui::root(app, ui);
    });
    output.textures_delta.clear();
    output.viewport_output[&ViewportId::ROOT].commands.clone()
}

fn key(key: Key, pressed: bool) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

fn setup() -> (egui::Context, App) {
    let ctx = crate::theme::ui_context();
    let mut app = App::headless();
    frame(&ctx, &mut app, WINDOW, false, Vec::new());
    (ctx, app)
}

fn assert_fullscreen(commands: &[ViewportCommand], expected: bool) {
    assert!(
        commands
            .iter()
            .any(|c| matches!(c, ViewportCommand::Fullscreen(value) if *value == expected)),
        "expected fullscreen={expected}, got {commands:?}"
    );
}

#[test]
fn f9_keeps_the_panned_center_and_zoom_while_the_fullscreen_range_expands() {
    let (ctx, mut app) = setup();
    app.new_shape(tracedraw_core::ShapeKind::Rect {
        rect: Rect::new(50_000.0, 50_000.0, 50_100.0, 50_100.0),
        radius: 0.0,
        corners: None,
    })
    .expect("a distant object");
    app.view.zoom = 7.5;
    app.view.origin = egui::pos2(-480.0, 1720.0);
    let view = app.view;
    let canvas = app.canvas_rect;
    let visible = view.visible_page_rect(canvas);

    let commands = frame(&ctx, &mut app, WINDOW, false, vec![key(Key::F9, true)]);
    assert_fullscreen(&commands, true);
    assert!(app.fullscreen_preview);
    assert_eq!(preview_bounds(&app), visible);
    let expanded = frame_bounds(&app, egui::vec2(1920.0, 1080.0));
    assert!(expanded.x0 <= visible.x0 && expanded.x1 >= visible.x1);
    assert!(expanded.y0 <= visible.y0 && expanded.y1 >= visible.y1);
    assert!((expanded.center() - visible.center()).hypot() < 1e-9);
    assert!((expanded.width() * f64::from(view.zoom) - 1920.0).abs() < 1e-6);
    assert!((expanded.height() * f64::from(view.zoom) - 1080.0).abs() < 1e-6);

    // The operating system reports the larger full-screen area one frame later.
    let commands = frame(
        &ctx,
        &mut app,
        egui::vec2(1920.0, 1080.0),
        true,
        vec![key(Key::F9, false)],
    );
    assert_eq!(preview_bounds(&app), visible);
    assert_eq!(app.view.zoom, view.zoom);
    assert_eq!(app.view.origin, view.origin);
    assert_eq!(app.canvas_rect, canvas);
    assert!(
        !commands
            .iter()
            .any(|c| matches!(c, ViewportCommand::Fullscreen(_))),
        "an idle preview does not repeat the window transition"
    );

    let commands = frame(&ctx, &mut app, WINDOW, true, vec![key(Key::F9, true)]);
    assert_fullscreen(&commands, false);
    assert!(!app.fullscreen_preview);
    assert!(app.preview_session.is_none());
    assert_eq!(app.view.zoom, view.zoom);
    assert_eq!(app.view.origin, view.origin);
}

#[test]
fn escape_and_a_click_restore_the_window_after_preview() {
    for click in [false, true] {
        let (ctx, mut app) = setup();
        frame(&ctx, &mut app, WINDOW, false, vec![key(Key::F9, true)]);
        frame(&ctx, &mut app, WINDOW, true, vec![key(Key::F9, false)]);
        let events = if click {
            let pos = egui::pos2(700.0, 450.0);
            let button = |pressed| Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            };
            vec![Event::PointerMoved(pos), button(true), button(false)]
        } else {
            vec![key(Key::Escape, true)]
        };
        let commands = frame(&ctx, &mut app, WINDOW, true, events);
        assert_fullscreen(&commands, false);
        assert!(!app.fullscreen_preview);
        assert!(!app.preview_selected_only);
        assert!(app.preview_session.is_none());
    }
}

#[test]
fn closing_preview_preserves_an_already_fullscreen_window() {
    let (ctx, mut app) = setup();
    let commands = frame(&ctx, &mut app, WINDOW, true, vec![key(Key::F9, true)]);
    assert_fullscreen(&commands, true);
    frame(&ctx, &mut app, WINDOW, true, vec![key(Key::F9, false)]);
    let commands = frame(&ctx, &mut app, WINDOW, true, vec![key(Key::Escape, true)]);
    assert_fullscreen(&commands, true);
    assert!(!commands
        .iter()
        .any(|c| matches!(c, ViewportCommand::Fullscreen(false))));
    assert!(!app.fullscreen_preview);
}

#[test]
fn editing_and_zoom_shortcuts_cannot_change_a_previewed_drawing() {
    let (ctx, mut app) = setup();
    let id = app
        .new_shape(tracedraw_core::ShapeKind::Rect {
            rect: Rect::new(20.0, 30.0, 90.0, 80.0),
            radius: 0.0,
            corners: None,
        })
        .expect("a selected rectangle");
    app.select(vec![id]);
    let document = app.doc().to_json().expect("document before preview");
    let history = app.engine.history_labels().0.len();
    let view = app.view;
    frame(&ctx, &mut app, WINDOW, false, vec![key(Key::F9, true)]);
    for pressed in [Key::Delete, Key::ArrowRight, Key::F3, Key::F4] {
        frame(&ctx, &mut app, WINDOW, true, vec![key(pressed, true)]);
        frame(&ctx, &mut app, WINDOW, true, vec![key(pressed, false)]);
        assert!(app.fullscreen_preview);
    }
    assert_eq!(
        app.doc().to_json().expect("document after preview"),
        document
    );
    assert_eq!(app.engine.history_labels().0.len(), history);
    assert_eq!(app.view.zoom, view.zoom);
    assert_eq!(app.view.origin, view.origin);
}

#[test]
fn selected_only_captures_the_selection_instead_of_the_visible_window() {
    let (ctx, mut app) = setup();
    let id = app
        .new_shape(tracedraw_core::ShapeKind::Rect {
            rect: Rect::new(5000.0, 4000.0, 5100.0, 4060.0),
            radius: 0.0,
            corners: None,
        })
        .expect("a rectangle outside the window");
    app.select(vec![id]);
    let selected = app.selection_bounds().expect("selection bounds");
    let mut output = ctx.run_ui(input(WINDOW, false, Vec::new()), |ui| {
        begin(&mut app, ui.ctx(), true);
        crate::ui::root(&mut app, ui);
    });
    output.textures_delta.clear();
    assert_fullscreen(&output.viewport_output[&ViewportId::ROOT].commands, true);
    assert!(app.preview_selected_only);
    assert_eq!(preview_bounds(&app), selected);
    frame(&ctx, &mut app, WINDOW, true, vec![key(Key::Escape, true)]);
    assert!(!app.preview_selected_only);
}

#[test]
fn reopening_preview_captures_the_new_view() {
    let (ctx, mut app) = setup();
    frame(&ctx, &mut app, WINDOW, false, vec![key(Key::F9, true)]);
    let first = preview_bounds(&app);
    frame(&ctx, &mut app, WINDOW, true, vec![key(Key::F9, false)]);
    frame(&ctx, &mut app, WINDOW, true, vec![key(Key::Escape, true)]);
    frame(&ctx, &mut app, WINDOW, false, vec![key(Key::Escape, false)]);
    app.view.pan(egui::vec2(320.0, -150.0));
    let visible = app.view.visible_page_rect(app.canvas_rect);
    frame(&ctx, &mut app, WINDOW, false, vec![key(Key::F9, true)]);
    assert_ne!(preview_bounds(&app), first);
    assert_eq!(preview_bounds(&app), visible);
}

#[test]
fn escape_still_leaves_preview_after_tapping_alt() {
    let (ctx, mut app) = setup();
    frame(&ctx, &mut app, WINDOW, false, vec![key(Key::F9, true)]);
    frame(
        &ctx,
        &mut app,
        WINDOW,
        true,
        vec![
            key(Key::F9, false),
            Event::ModifiersChanged(Modifiers::ALT),
            Event::ModifiersChanged(Modifiers::NONE),
        ],
    );
    let commands = frame(&ctx, &mut app, WINDOW, true, vec![key(Key::Escape, true)]);
    assert_fullscreen(&commands, false);
    assert!(!app.fullscreen_preview, "a hidden menu cannot take Escape");
}

#[test]
fn a_hidpi_preview_is_sharp_without_enlarging_its_logical_picture() {
    let (ctx, mut app) = setup();
    let screen = egui::vec2(1920.0, 1080.0);
    let mut raw = input(screen, true, Vec::new());
    raw.max_texture_side = Some(8192);
    raw.viewports
        .get_mut(&ViewportId::ROOT)
        .expect("root viewport")
        .native_pixels_per_point = Some(2.0);
    let mut output = ctx.run_ui(raw, |ui| {
        begin(&mut app, ui.ctx(), false);
        crate::ui::root(&mut app, ui);
    });
    let texture = &app
        .preview_session
        .as_ref()
        .expect("preview session")
        .texture
        .as_ref()
        .expect("preview picture")
        .1;
    let picture = output
        .shapes
        .iter()
        .find_map(|clipped| match &clipped.shape {
            egui::epaint::Shape::Mesh(mesh) if mesh.texture_id == texture.id() => {
                Some(mesh.calc_bounds())
            }
            _ => None,
        })
        .expect("preview image mesh");
    let pixels_per_point = ctx.pixels_per_point();
    assert!((pixels_per_point - 2.0).abs() < 0.001);
    assert!(picture.min.distance(Pos2::ZERO) < 0.001);
    assert!(
        (picture.size() - screen).length() < 0.001,
        "no letterbox: {picture:?}"
    );
    let pixels = texture.size();
    assert!((pixels[0] as f32 - screen.x * pixels_per_point).abs() <= 1.0);
    assert!((pixels[1] as f32 - screen.y * pixels_per_point).abs() <= 1.0);
    output.textures_delta.clear();
}

/// F9 draws the drawing once, when the window fills the screen (#3830):
/// not first in the window as it was and again in the whole screen, which
/// jumped and flickered. A window that never says so gets it in a moment.
#[test]
fn the_preview_waits_for_the_full_screen_before_drawing() {
    let has_picture = |app: &App| {
        app.preview_session
            .as_ref()
            .is_some_and(|s| s.texture.is_some())
    };
    let (ctx, mut app) = setup();
    timed_frame(&ctx, &mut app, 10.0, WINDOW, vec![key(Key::F9, true)]);
    assert!(app.fullscreen_preview);
    assert!(!has_picture(&app), "the window is not full screen yet");
    let screen = egui::vec2(1920.0, 1080.0);
    let mut raw = input(screen, true, vec![key(Key::F9, false)]);
    raw.time = Some(10.1);
    let mut output = ctx.run_ui(raw, |ui| crate::ui::root(&mut app, ui));
    output.textures_delta.clear();
    let session = app.preview_session.as_ref().expect("session");
    let pixels = session.texture.as_ref().expect("picture").1.size();
    let ppp = ctx.pixels_per_point();
    let wide = pixels[0] as f32 - screen.x * ppp;
    assert!(wide.abs() <= 1.0, "the whole screen: {pixels:?}");

    // A window that stays as it was says nothing of full screen.
    let windowed = |ctx: &egui::Context, app: &mut App, time: f64, events: Vec<Event>| {
        let mut raw = input(WINDOW, false, events);
        raw.time = Some(time);
        let mut output = ctx.run_ui(raw, |ui| crate::ui::root(app, ui));
        output.textures_delta.clear();
    };
    let (ctx, mut app) = setup();
    windowed(&ctx, &mut app, 10.0, vec![key(Key::F9, true)]);
    windowed(&ctx, &mut app, 10.2, vec![key(Key::F9, false)]);
    assert!(!has_picture(&app));
    windowed(&ctx, &mut app, 10.5, Vec::new());
    assert!(has_picture(&app), "drawn after a moment anyway");
}

#[test]
fn idle_preview_frames_reuse_the_picture_without_uploading_it_again() {
    let (ctx, mut app) = setup();
    frame(&ctx, &mut app, WINDOW, false, vec![key(Key::F9, true)]);
    frame(&ctx, &mut app, WINDOW, true, vec![key(Key::F9, false)]);
    let texture = app
        .preview_session
        .as_ref()
        .expect("preview session")
        .texture
        .as_ref()
        .expect("preview picture")
        .1
        .id();
    for _ in 0..3 {
        let mut output = ctx.run_ui(input(WINDOW, true, Vec::new()), |ui| {
            crate::ui::root(&mut app, ui);
        });
        assert!(app.fullscreen_preview);
        let reused = app
            .preview_session
            .as_ref()
            .expect("preview session")
            .texture
            .as_ref()
            .expect("preview picture")
            .1
            .id();
        assert_eq!(reused, texture, "an idle preview keeps its texture");
        assert!(
            !output
                .textures_delta
                .set
                .iter()
                .any(|(id, _)| *id == texture),
            "the same picture is not uploaded again"
        );
        output.textures_delta.clear();
    }
}

#[test]
fn entering_preview_stops_view_inertia_and_returning_does_not_resume_it() {
    let (ctx, mut app) = setup();
    app.settings.pan_inertia = true;
    app.zoom_glide.add(2.0, 1.15, app.canvas_rect.center(), 1.0, &app.view);
    app.pan_inertia
        .track(egui::vec2(24.0, 12.0), 1.0 / 60.0, 1.0);
    app.pan_inertia.tick(1.0 / 60.0, 1.01, false, true);
    assert!(app.zoom_glide.active());
    assert!(app.pan_inertia.sliding());
    let view = app.view;
    frame(&ctx, &mut app, WINDOW, false, vec![key(Key::F9, true)]);
    assert!(!app.zoom_glide.active());
    assert!(!app.pan_inertia.sliding());
    frame(&ctx, &mut app, WINDOW, true, vec![key(Key::Escape, true)]);
    frame(&ctx, &mut app, WINDOW, false, vec![key(Key::Escape, false)]);
    assert_eq!(app.view.zoom, view.zoom);
    assert_eq!(app.view.origin, view.origin);
}

#[test]
fn fullscreen_reveals_nearby_objects_outside_the_editor_without_fitting_distant_art() {
    let (ctx, mut app) = setup();
    let id = app
        .new_shape(tracedraw_core::ShapeKind::Rect {
            rect: Rect::new(220.0, 180.0, 240.0, 200.0),
            radius: 0.0,
            corners: None,
        })
        .expect("a nearby object");
    app.run(Command::SetFill {
        shapes: vec![id],
        fill: tracedraw_core::Fill::Solid(tracedraw_core::Color::rgb8(240, 20, 30)),
    });
    app.new_shape(tracedraw_core::ShapeKind::Rect {
        rect: Rect::new(50_000.0, 50_000.0, 50_100.0, 50_100.0),
        radius: 0.0,
        corners: None,
    })
    .expect("a distant object");
    app.canvas_rect = egui::Rect::from_min_size(egui::pos2(100.0, 100.0), egui::vec2(800.0, 500.0));
    app.view.zoom = 4.0;
    app.view.origin = egui::pos2(100.0, 1100.0);
    let source = app.view.visible_page_rect(app.canvas_rect);
    assert!(source.x1 < 220.0, "the nearby object is outside the editor");
    frame(&ctx, &mut app, WINDOW, false, vec![key(Key::F9, true)]);
    let screen = egui::vec2(1920.0, 1080.0);
    let mut output = ctx.run_ui(input(screen, true, vec![key(Key::F9, false)]), |ui| {
        crate::ui::root(&mut app, ui);
    });
    let bounds = frame_bounds(&app, screen);
    assert!(bounds.x1 > 240.0 && bounds.x1 < 50_000.0);
    assert_eq!(app.preview_session.as_ref().expect("session").zoom, 4.0);
    let texture = app
        .preview_session
        .as_ref()
        .expect("session")
        .texture
        .as_ref()
        .expect("picture")
        .1
        .id();
    let image = output
        .textures_delta
        .set
        .iter()
        .find_map(|(id, deltas)| {
            if *id == texture {
                deltas.last().map(|delta| &delta.image)
            } else {
                None
            }
        })
        .expect("new full-screen picture");
    let egui::ImageData::Color(image) = image;
    let x = ((230.0 - bounds.x0) * 4.0) as usize;
    let y = ((bounds.y1 - 190.0) * 4.0) as usize;
    let color = image.pixels[y * image.size[0] + x];
    assert!(
        color.r() > 200 && color.g() < 50 && color.b() < 50,
        "nearby object appears at the original scale: {color:?}"
    );
    output.textures_delta.clear();
}

/// In the full-screen preview, + and - zoom about the middle, Page Down
/// and Page Up show the next and previous page fitted to the screen, and
/// the colour round the page is the one chosen in the options (#3930).
#[test]
fn preview_keys_zoom_and_turn_the_pages() {
    let (ctx, mut app) = setup();
    app.add_page();
    app.goto_page(0);
    let first = app.page;
    app.settings.preview_background = [30, 30, 30];
    frame(&ctx, &mut app, WINDOW, false, vec![key(Key::F9, true)]);
    let zoom = app.preview_session.as_ref().expect("preview").zoom;
    frame(&ctx, &mut app, WINDOW, true, vec![key(Key::Plus, true)]);
    let zoomed = app.preview_session.as_ref().expect("preview").zoom;
    assert!((zoomed / zoom - 1.25).abs() < 1e-9, "{zoom} -> {zoomed}");
    frame(&ctx, &mut app, WINDOW, true, vec![key(Key::Minus, true)]);
    let back = app.preview_session.as_ref().expect("preview").zoom;
    assert!((back - zoom).abs() < 1e-9, "{zoom} -> {back}");
    frame(&ctx, &mut app, WINDOW, true, vec![key(Key::PageDown, true)]);
    assert!(app.fullscreen_preview, "the keys keep the preview open");
    assert_ne!(app.page, first, "Page Down shows the next page");
    let session = app.preview_session.as_ref().expect("preview");
    assert_eq!(session.bounds, app.page_rect(), "fitted to the page");
    frame(&ctx, &mut app, WINDOW, true, vec![key(Key::PageDown, true)]);
    assert_ne!(app.page, first, "no page after the last");
    frame(&ctx, &mut app, WINDOW, true, vec![key(Key::PageUp, true)]);
    assert_eq!(app.page, first, "Page Up goes back");
    let mut out = ctx.run_ui(input(WINDOW, true, Vec::new()), |ui| crate::ui::root(&mut app, ui));
    out.textures_delta.clear();
    let dark = egui::Color32::from_rgb(30, 30, 30);
    assert!(out.shapes.iter().any(|s| matches!(&s.shape,
        egui::Shape::Rect(r) if r.fill == dark && r.rect.width() > 1000.0)));
}

/// The full-screen preview's colour round the page follows the drawing
/// window's while none is picked (#4027: a dark theme's grey turned
/// white); a picked colour, white too, stays.
#[test]
fn the_preview_background_follows_the_desktop_until_one_is_picked() {
    let (_ctx, mut app) = setup();
    app.settings.desktop_rgb = [61, 61, 61];
    app.settings.desktop_custom = true;
    assert_eq!(app.preview_background_color(), egui::Color32::from_rgb(61, 61, 61));
    app.settings.preview_background_custom = true;
    assert_eq!(app.preview_background_color(), egui::Color32::WHITE);
    app.settings.preview_background = [30, 30, 30];
    assert_eq!(app.preview_background_color(), egui::Color32::from_rgb(30, 30, 30));
}
