//! Dragging an interactive effect by its control line, in the real window
//! with pointer events: a drag pressed on the line, away from its handles
//! and sliders, moves the whole effect by the pointer's way (one undo
//! step, Ctrl keeping it straight); a drag on a handle still moves only
//! that handle; a slider on the line wins over it; the pointer is the move
//! cursor over the line. Interactive Fill (every fountain kind),
//! Transparency, Drop Shadow, Extrude, Blend and Distort.

use crate::app::App;
use crate::fill_tool::{fountain_handles, on_line};
use crate::tools::Tool;
use crate::ui::cursors::{for_tool, ToolCursor};
use egui::{Event, Modifiers, PointerButton, Pos2};
use tracedraw_core::geometry::{Point, Rect, Vec2};
use tracedraw_core::{Color, Fill, FountainKind, ShapeId, ShapeKind};

fn frame(ctx: &egui::Context, app: &mut App, events: Vec<Event>, modifiers: Modifiers) {
    let mut all = vec![Event::ModifiersChanged(modifiers)];
    all.extend(events);
    let mut input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            Pos2::ZERO,
            egui::vec2(1600.0, 1000.0),
        )),
        events: all,
        ..Default::default()
    };
    crate::pen::filter_raw_input(ctx, &mut input);
    crate::interaction::release_chorded_left(ctx, &mut input);
    let mut out = ctx.run_ui(input, |ui| crate::ui::root(app, ui));
    out.textures_delta.clear();
}

fn button(pos: Pos2, pressed: bool, modifiers: Modifiers) -> Event {
    Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers,
    }
}

/// A primary drag on the page from `from` to `to` (page points) with
/// `mods` held.
fn drag(ctx: &egui::Context, app: &mut App, from: Point, to: Point, mods: Modifiers) {
    let (from, to) = (app.view.to_screen(from), app.view.to_screen(to));
    frame(ctx, app, vec![Event::PointerMoved(from)], mods);
    frame(ctx, app, vec![button(from, true, mods)], mods);
    for i in 1..=4 {
        let p = from + (to - from) * (i as f32 / 4.0);
        frame(ctx, app, vec![Event::PointerMoved(p)], mods);
    }
    frame(ctx, app, vec![button(to, false, mods)], mods);
    frame(ctx, app, Vec::new(), Modifiers::NONE);
}

/// The pointer's shape at page point `p`.
fn cursor_at(ctx: &egui::Context, app: &mut App, p: Point) -> ToolCursor {
    let s = app.view.to_screen(p);
    frame(ctx, app, vec![Event::PointerMoved(s)], Modifiers::NONE);
    for_tool(app, false, false, false, Some(s))
}

const MOVE: ToolCursor = ToolCursor::System(egui::CursorIcon::Move);

/// A drawing with a 50 x 30 mm rectangle at (10, 10), selected, the page
/// in view, `tool` in hand.
fn setup(tool: Tool) -> (egui::Context, App, ShapeId) {
    let ctx = crate::theme::ui_context();
    let mut app = App::headless();
    app.show_welcome = false;
    let id = rect(&mut app, Rect::new(10.0, 10.0, 60.0, 40.0));
    app.select(vec![id]);
    for _ in 0..3 {
        frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    }
    app.zoom_to_page();
    app.set_tool(tool);
    for _ in 0..2 {
        frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    }
    (ctx, app, id)
}

fn rect(app: &mut App, r: Rect) -> ShapeId {
    let id = app
        .new_shape(ShapeKind::Rect {
            rect: r,
            radius: 0.0,
            corners: None,
        })
        .expect("rect");
    app.run(tracedraw_core::Command::SetFill {
        shapes: vec![id],
        fill: Fill::Solid(Color::rgb8(200, 30, 30)),
    });
    id
}

fn steps(app: &App) -> usize {
    app.engine.history_labels().0.len()
}

fn near(a: Point, b: Point) -> bool {
    (a - b).hypot() < 0.05
}

/// The selected fill's (or mask's) start, end and skew handle.
fn nodes(app: &App) -> [Point; 3] {
    let (_, f, b) = app.selected_fountain().expect("a fountain");
    let hs = fountain_handles(&f, b);
    [hs[0].1, hs[1].1, hs[2].1]
}

#[test]
fn the_fill_line_moves_the_whole_fill_for_every_kind() {
    for kind in [
        FountainKind::Linear,
        FountainKind::Radial,
        FountainKind::Conical,
        FountainKind::Square,
    ] {
        let (ctx, mut app, _) = setup(Tool::InteractiveFill);
        let f = tracedraw_core::style::Fountain::two(kind, Color::BLACK, Color::WHITE, 0.0);
        app.apply_fill(Fill::Fountain(f));
        let [s, e, k] = nodes(&app);
        // Away from the nodes and the midpoint slider (at the middle).
        let on = on_line(s, e, 0.3);
        assert_eq!(cursor_at(&ctx, &mut app, on), MOVE, "{kind:?}");
        let before = steps(&app);
        let d = Vec2::new(6.0, 4.0);
        drag(&ctx, &mut app, on, on + d, Modifiers::NONE);
        let [s2, e2, k2] = nodes(&app);
        assert!(
            near(s2, s + d) && near(e2, e + d),
            "{kind:?}: {s2:?} {e2:?}"
        );
        assert!(near(k2, k + d), "{kind:?}: the skew handle rides along");
        assert_eq!(steps(&app), before + 1, "{kind:?}: one undo step");
        assert_eq!(app.engine.undo_label(), Some("Fill"));

        // Ctrl keeps the move horizontal.
        let on = on_line(s2, e2, 0.3);
        drag(
            &ctx,
            &mut app,
            on,
            on + Vec2::new(8.0, 2.0),
            Modifiers::CTRL,
        );
        let [s3, e3, _] = nodes(&app);
        assert!(near(s3, s2 + Vec2::new(8.0, 0.0)), "{kind:?}: {s3:?}");
        assert!(near(e3, e2 + Vec2::new(8.0, 0.0)), "{kind:?}: {e3:?}");

        // The end handle still moves only the end.
        drag(
            &ctx,
            &mut app,
            e3,
            e3 + Vec2::new(0.0, 5.0),
            Modifiers::NONE,
        );
        let [s4, e4, _] = nodes(&app);
        assert!(near(s4, s3), "{kind:?}: the start stays");
        assert!(near(e4, e3 + Vec2::new(0.0, 5.0)), "{kind:?}: {e4:?}");
    }
}

#[test]
fn the_midpoint_slider_wins_over_the_fill_line() {
    let (ctx, mut app, _) = setup(Tool::InteractiveFill);
    app.apply_fill(Fill::linear(Color::BLACK, Color::WHITE, 0.0));
    let [s, e, _] = nodes(&app);
    let mid = on_line(s, e, 0.5);
    drag(&ctx, &mut app, mid, on_line(s, e, 0.3), Modifiers::NONE);
    let (_, f, _) = app.selected_fountain().expect("fountain");
    assert!((f.stops[0].mid - 0.3).abs() < 0.01, "{}", f.stops[0].mid);
    let [s2, e2, _] = nodes(&app);
    assert!(near(s2, s) && near(e2, e), "the line stays");
}

#[test]
fn the_transparency_line_moves_the_whole_mask() {
    let (ctx, mut app, id) = setup(Tool::Transparency);
    app.transparency_from_drag_on(&[id], Point::new(15.0, 25.0), Point::new(55.0, 25.0));
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    let [s, e, _] = nodes(&app);
    let on = on_line(s, e, 0.25);
    assert_eq!(cursor_at(&ctx, &mut app, on), MOVE);
    let before = steps(&app);
    let d = Vec2::new(-4.0, 6.0);
    drag(&ctx, &mut app, on, on + d, Modifiers::NONE);
    let [s2, e2, _] = nodes(&app);
    assert!(near(s2, s + d) && near(e2, e + d), "{s2:?} {e2:?}");
    assert_eq!(steps(&app), before + 1);
    assert_eq!(app.engine.undo_label(), Some("Transparency"));
    // The fill is left alone: still the solid colour.
    let fill = &app.doc().find_shape(id).expect("rect").fill;
    assert!(matches!(fill, Fill::Solid(_)), "{fill:?}");
    // The start handle moves only the start (a linear mask).
    drag(
        &ctx,
        &mut app,
        s2,
        s2 + Vec2::new(0.0, 4.0),
        Modifiers::NONE,
    );
    let [s3, e3, _] = nodes(&app);
    assert!(near(s3, s2 + Vec2::new(0.0, 4.0)) && near(e3, e2));
}

#[test]
fn the_shadow_line_moves_the_shadow() {
    let (ctx, mut app, id) = setup(Tool::DropShadow);
    app.set_shadow(
        Some(tracedraw_core::Shadow {
            offset: Vec2::new(30.0, -20.0),
            ..Default::default()
        }),
        false,
    );
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    let ends = |app: &App| {
        let (_, sh, b) = app.selected_shadow().expect("shadow");
        (sh.anchor(b), sh.end_point(b), sh.opacity)
    };
    let (a, z, opacity) = ends(&app);
    // A quarter of the way: off the handles and the opacity slider (half
    // way), on the object itself.
    let on = a + (z - a) * 0.25;
    assert_eq!(cursor_at(&ctx, &mut app, on), MOVE);
    let before = steps(&app);
    let d = Vec2::new(5.0, 3.0);
    drag(&ctx, &mut app, on, on + d, Modifiers::NONE);
    let (a2, z2, o2) = ends(&app);
    assert!(near(a2, a), "the start stands on the object");
    assert!(near(z2, z + d), "{z2:?}");
    assert_eq!(o2, opacity);
    assert_eq!(steps(&app), before + 1, "one undo step");
    assert_eq!(app.selection, vec![id]);
    // The opacity slider still takes the opacity, the shadow staying.
    let slider = a2 + (z2 - a2) * opacity;
    drag(
        &ctx,
        &mut app,
        slider,
        a2 + (z2 - a2) * 0.8,
        Modifiers::NONE,
    );
    let (_, z3, o3) = ends(&app);
    assert!(near(z3, z2));
    assert!((o3 - 0.8).abs() < 0.02, "{o3}");
}

#[test]
fn the_extrusion_line_moves_its_vanishing_point() {
    let (ctx, mut app, id) = setup(Tool::Extrude);
    assert!(app.begin_extrude(Point::new(35.0, 25.0)));
    app.drag_extrude_to(Point::new(100.0, 60.0), false);
    app.extrude_drag = None;
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    let line = |app: &App| {
        let (i, h, mid) = app.extrusion_handles()[0];
        assert_eq!(i, id);
        (h, mid)
    };
    let (h, mid) = line(&app);
    let on = mid + (h - mid) * 0.5;
    assert_eq!(cursor_at(&ctx, &mut app, on), MOVE);
    let before = steps(&app);
    let d = Vec2::new(-10.0, 5.0);
    drag(&ctx, &mut app, on, on + d, Modifiers::NONE);
    let (h2, mid2) = line(&app);
    assert!(near(mid2, mid), "the object stays");
    assert!(near(h2, h + d), "{h2:?} vs {:?}", h + d);
    assert_eq!(steps(&app), before + 1, "one undo step");
    // The handle still goes where the pointer takes it.
    let to = h2 + Vec2::new(0.0, 10.0);
    drag(&ctx, &mut app, h2, to, Modifiers::NONE);
    let (h3, _) = line(&app);
    assert!(near(h3, to), "{h3:?}");
}

#[test]
fn the_blend_line_moves_the_whole_blend() {
    let (ctx, mut app, a) = setup(Tool::Pick);
    let b = rect(&mut app, Rect::new(110.0, 60.0, 130.0, 80.0));
    app.selection = vec![a, b];
    app.blend_selection(5, 0.0, 0.0, 0.0);
    app.set_tool(Tool::Blend);
    app.select(vec![a]);
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    let centre =
        |app: &App, id: ShapeId| app.doc().find_shape(id).expect("shape").bounds().center();
    let (ca, cb) = (centre(&app, a), centre(&app, b));
    let c = app.blend_controls().expect("blend controls");
    assert!(near(c.start, ca) && near(c.end, cb));
    // A quarter of the way: off the squares and the sliders (half way).
    let on = ca + (cb - ca) * 0.25;
    assert_eq!(cursor_at(&ctx, &mut app, on), MOVE);
    let before = steps(&app);
    let d = Vec2::new(4.0, -6.0);
    drag(&ctx, &mut app, on, on + d, Modifiers::NONE);
    assert!(near(centre(&app, a), ca + d), "{:?}", centre(&app, a));
    assert!(near(centre(&app, b), cb + d), "{:?}", centre(&app, b));
    assert_eq!(steps(&app), before + 1, "one undo step");
    let c = app.blend_controls().expect("still a blend");
    assert!(near(c.start, ca + d) && near(c.end, cb + d));
    // The slider still takes the acceleration, nothing moving.
    let (ca, cb) = (centre(&app, a), centre(&app, b));
    let n = crate::blend_tool::screen_normal(app.view.to_screen(ca), app.view.to_screen(cb));
    let slider_px =
        app.view.to_screen(ca + (cb - ca) * 0.5) + n * (crate::blend_tool::SLIDER_OFFSET_PX + 2.0);
    let slider = app.view.to_page(slider_px);
    drag(
        &ctx,
        &mut app,
        slider,
        ca + (cb - ca) * 0.3,
        Modifiers::NONE,
    );
    assert!(near(centre(&app, a), ca) && near(centre(&app, b), cb));
    let c = app.blend_controls().expect("blend");
    assert!(c.accel_objects > 0.1, "{}", c.accel_objects);
    // The end square moves only the end object.
    drag(
        &ctx,
        &mut app,
        cb,
        cb + Vec2::new(5.0, 0.0),
        Modifiers::NONE,
    );
    assert!(near(centre(&app, a), ca));
    assert!(near(centre(&app, b), cb + Vec2::new(5.0, 0.0)));
}

#[test]
fn the_distortion_line_moves_its_centre() {
    let (ctx, mut app, id) = setup(Tool::Distort);
    // Push and Pull about the middle, its line to the right.
    assert!(app.begin_distort(Point::new(35.0, 25.0)));
    app.drag_distort_to(Point::new(50.0, 25.0), false);
    app.drag_distort_to(Point::new(55.0, 25.0), false);
    app.distort_drag = None;
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    let line = |app: &App| {
        let h = app
            .distortion_handles()
            .into_iter()
            .next()
            .expect("handles");
        assert_eq!(h.id, id);
        (h.center, h.end)
    };
    let (c, e) = line(&app);
    let on = c + (e - c) * 0.5;
    assert_eq!(cursor_at(&ctx, &mut app, on), MOVE);
    let before = steps(&app);
    let d = Vec2::new(3.0, 5.0);
    drag(&ctx, &mut app, on, on + d, Modifiers::NONE);
    let (c2, e2) = line(&app);
    assert!(near(c2, c + d), "{c2:?}");
    assert!(near(e2, e + d), "{e2:?}");
    assert_eq!(steps(&app), before + 1, "one undo step");
    // The end handle still changes the amount, the centre staying.
    drag(
        &ctx,
        &mut app,
        e2,
        e2 + Vec2::new(5.0, 0.0),
        Modifiers::NONE,
    );
    let (c3, e3) = line(&app);
    assert!(near(c3, c2));
    assert!((e3 - e2).hypot() > 1.0, "{e3:?}");
}
