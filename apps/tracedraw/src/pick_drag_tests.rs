//! The Pick tool's drags in the real window, with pointer events and
//! modifier keys: Ctrl keeps a move straight, a right click during a drag
//! leaves the original, Space drops a copy, Shift scales from the centre,
//! Ctrl scales in steps of 100 % (mirror at the same size), Alt frees a
//! corner, the side handles of rotate mode skew, and the centre of
//! rotation moves. Also the pointer's shape over handles and objects.

use crate::app::{App, Drag, Handle, HandleMods};
use crate::interaction::skew_transform;
use crate::tools::Tool;
use crate::ui::cursors::{for_tool, resize_icon, ShapeMark, ToolCursor};
use egui::{Event, Modifiers, PointerButton, Pos2};
use tracedraw_core::geometry::{Affine, Point, Rect, Vec2};
use tracedraw_core::{ShapeId, ShapeKind};

fn frame(ctx: &egui::Context, app: &mut App, events: Vec<Event>, modifiers: Modifiers) {
    // The keys held come first, as the windowing layer sends them.
    let mut all = vec![Event::ModifiersChanged(modifiers)];
    all.extend(events);
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            Pos2::ZERO,
            egui::vec2(1600.0, 1000.0),
        )),
        events: all,
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, |ui| crate::ui::root(app, ui));
    out.textures_delta.clear();
}

fn button(pos: Pos2, pressed: bool, button: PointerButton, modifiers: Modifiers) -> Event {
    Event::PointerButton {
        pos,
        button,
        pressed,
        modifiers,
    }
}

fn key(key: egui::Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

/// A primary drag from `from` to `to` with `mods` held, `during` sent in
/// the middle of it.
fn drag(
    ctx: &egui::Context,
    app: &mut App,
    from: Pos2,
    to: Pos2,
    mods: Modifiers,
    during: Vec<Event>,
) {
    frame(ctx, app, vec![Event::PointerMoved(from)], mods);
    frame(
        ctx,
        app,
        vec![button(from, true, PointerButton::Primary, mods)],
        mods,
    );
    for i in 1..=4 {
        let p = from + (to - from) * (i as f32 / 4.0);
        let mut events = vec![Event::PointerMoved(p)];
        if i == 2 {
            events.extend(during.clone());
        }
        frame(ctx, app, events, mods);
    }
    frame(
        ctx,
        app,
        vec![button(to, false, PointerButton::Primary, mods)],
        mods,
    );
    frame(ctx, app, Vec::new(), Modifiers::NONE);
}

/// A drawing with a 50 x 30 mm rectangle at (10, 10), selected, the
/// page in view.
fn setup() -> (egui::Context, App, ShapeId) {
    let ctx = crate::theme::ui_context();
    let mut app = App::headless();
    app.show_welcome = false;
    let id = app
        .new_shape(ShapeKind::Rect {
            rect: Rect::new(10.0, 10.0, 60.0, 40.0),
            radius: 0.0,
            corners: None,
        })
        .expect("rect");
    app.set_tool(Tool::Pick);
    app.select(vec![id]);
    // Filled, so that a press inside it picks it.
    app.apply_fill(tracedraw_core::Fill::Solid(tracedraw_core::Color::rgb8(
        200, 30, 30,
    )));
    // The window's bars settle over the first frames; then the page fits.
    for _ in 0..3 {
        frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    }
    app.zoom_to_page();
    for _ in 0..2 {
        frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    }
    (ctx, app, id)
}

fn bounds(app: &App, id: ShapeId) -> Rect {
    app.doc().find_shape(id).map(|s| s.bounds()).expect("shape")
}

fn handle(app: &App, which: Handle) -> Pos2 {
    let b = app.selection_bounds().expect("selection");
    app.handle_positions(b)
        .iter()
        .find(|(h, _)| *h == which)
        .map(|(_, p)| *p)
        .expect("handle")
}

fn shape_count(app: &App) -> usize {
    app.doc()
        .all_layers()
        .map(|l| l.shapes.len())
        .sum::<usize>()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.05
}

#[test]
fn ctrl_keeps_a_move_straight() {
    let (ctx, mut app, id) = setup();
    let from = app.view.to_screen(Point::new(35.0, 25.0));
    drag(
        &ctx,
        &mut app,
        from,
        from + egui::vec2(120.0, 35.0),
        Modifiers::CTRL,
        Vec::new(),
    );
    let b = bounds(&app, id);
    assert!(b.x0 > 15.0, "moved sideways: {b:?}");
    assert!(close(b.y0, 10.0), "kept its height: {b:?}");
    // Without Ctrl it follows the pointer both ways.
    let from = app.view.to_screen(b.center());
    drag(
        &ctx,
        &mut app,
        from,
        from + egui::vec2(0.0, -60.0),
        Modifiers::NONE,
        Vec::new(),
    );
    assert!(bounds(&app, id).y0 > b.y0 + 5.0);
}

#[test]
fn a_right_click_while_dragging_leaves_the_original() {
    let (ctx, mut app, id) = setup();
    let from = app.view.to_screen(Point::new(35.0, 25.0));
    let to = from + egui::vec2(150.0, 0.0);
    let right = |p: Pos2, down: bool| button(p, down, PointerButton::Secondary, Modifiers::NONE);
    let mid = from + (to - from) * 0.5;
    drag(
        &ctx,
        &mut app,
        from,
        to,
        Modifiers::NONE,
        vec![right(mid, true), right(mid, false)],
    );
    assert_eq!(shape_count(&app), 2);
    assert!(close(bounds(&app, id).x0, 10.0), "the original stays");
    // The copy is the selection, moved.
    let copy = app.selection.first().copied().expect("copy");
    assert_ne!(copy, id);
    assert!(bounds(&app, copy).x0 > 15.0);
    assert!(app.dialog_is_none_or_menu_closed());
}

/// Repeat after a copy made with a right click during a drag makes
/// another copy, as far again from the last one (#363).
#[test]
fn repeat_after_a_right_click_copy_makes_another_copy() {
    let (ctx, mut app, id) = setup();
    let from = app.view.to_screen(Point::new(35.0, 25.0));
    let to = from + egui::vec2(150.0, 0.0);
    let right = |p: Pos2, down: bool| button(p, down, PointerButton::Secondary, Modifiers::NONE);
    let mid = from + (to - from) * 0.5;
    drag(
        &ctx,
        &mut app,
        from,
        to,
        Modifiers::NONE,
        vec![right(mid, true), right(mid, false)],
    );
    assert_eq!(shape_count(&app), 2);
    let first = app.selection[0];
    let step = bounds(&app, first).x0 - bounds(&app, id).x0;
    assert!(step > 5.0, "{step}");
    app.repeat_last();
    assert_eq!(shape_count(&app), 3, "Repeat makes a third copy");
    let second = app.selection[0];
    assert!(second != first && second != id);
    assert!(close(
        bounds(&app, second).x0,
        bounds(&app, first).x0 + step
    ));
    assert!(
        close(bounds(&app, first).x0, bounds(&app, id).x0 + step),
        "the first copy stays"
    );
    app.repeat_last();
    assert_eq!(shape_count(&app), 4);
    // Each copy, the drag's and Repeat's, is one undo step: one Undo
    // takes the newest copy away whole, not just its move (#471).
    let third = app.selection[0];
    let steps = app.engine.history_labels().0.len();
    app.undo();
    assert_eq!(app.engine.history_labels().0.len(), steps - 1);
    assert_eq!(shape_count(&app), 3);
    assert!(app.doc().find_shape(third).is_none());
    app.undo();
    app.undo();
    assert_eq!(shape_count(&app), 1, "three Undos for three copies");
}

/// Repeat after an undo repeats the last action still in effect, not the
/// one just undone; after a redo the redone action is the last again
/// (#366).
#[test]
fn repeat_after_an_undo_takes_the_last_action_still_in_effect() {
    use tracedraw_core::{Affine, Command};
    let (_ctx, mut app, id) = setup();
    app.select(vec![id]);
    // A: a move; B: a rotation.
    app.transform_selection(Affine::translate((10.0, 0.0)));
    let after_a = bounds(&app, id);
    let c = after_a.center();
    app.transform_selection(Affine::rotate_about(std::f64::consts::FRAC_PI_2, c));
    assert!(!close(bounds(&app, id).width(), after_a.width()));
    app.undo();
    assert!(close(bounds(&app, id).width(), after_a.width()));
    // Repeat: the move again, not the rotation.
    app.repeat_last();
    let b = bounds(&app, id);
    assert!(close(b.x0, after_a.x0 + 10.0), "{b:?} {after_a:?}");
    assert!(close(b.width(), after_a.width()), "not turned: {b:?}");
    // Undo that, redo the rotation: Repeat turns again.
    app.undo();
    app.redo();
    let turned = bounds(&app, id);
    app.repeat_last();
    assert!(
        close(bounds(&app, id).width(), after_a.width()),
        "turned a second time: {:?} {turned:?}",
        bounds(&app, id)
    );
    // An undone action followed by another change is never repeated.
    app.undo();
    app.undo();
    app.run(Command::SetOpacity {
        shapes: vec![id],
        opacity: 0.5,
    });
    app.run(Command::SetOpacity {
        shapes: vec![id],
        opacity: 1.0,
    });
    app.undo();
    // The opacity of 0.5 is the last action in effect.
    app.repeat_last();
    let op = app.doc().find_shape(id).map(|s| s.opacity).expect("shape");
    assert!(close(op, 0.5), "{op}");
}

#[test]
fn esc_cancels_a_drag() {
    let (ctx, mut app, id) = setup();
    let from = app.view.to_screen(Point::new(35.0, 25.0));
    drag(
        &ctx,
        &mut app,
        from,
        from + egui::vec2(150.0, 40.0),
        Modifiers::NONE,
        vec![key(egui::Key::Escape, Modifiers::NONE)],
    );
    assert!(close(bounds(&app, id).x0, 10.0), "nothing moved");
    assert_eq!(app.selection, vec![id], "the selection stays");
    assert!(matches!(app.drag, Drag::None));
}

#[test]
fn space_while_moving_drops_a_copy() {
    let (ctx, mut app, id) = setup();
    let from = app.view.to_screen(Point::new(35.0, 25.0));
    drag(
        &ctx,
        &mut app,
        from,
        from + egui::vec2(160.0, 0.0),
        Modifiers::NONE,
        vec![key(egui::Key::Space, Modifiers::NONE)],
    );
    // The original went all the way; a copy stayed half way.
    assert_eq!(shape_count(&app), 2);
    let moved = bounds(&app, id);
    let copy = app
        .doc()
        .all_layers()
        .flat_map(|l| &l.shapes)
        .find(|s| s.id != id)
        .map(|s| s.bounds())
        .expect("copy");
    assert!(
        copy.x0 > 10.5 && copy.x0 < moved.x0 - 0.5,
        "{copy:?} {moved:?}"
    );
    assert_eq!(app.tool, Tool::Pick);
}

#[test]
fn shift_scales_from_the_centre_and_ctrl_mirrors() {
    let (ctx, mut app, id) = setup();
    // Shift: the right side goes out, the left side as far the other way.
    let e = handle(&app, Handle::E);
    drag(
        &ctx,
        &mut app,
        e,
        e + egui::vec2(60.0, 0.0),
        Modifiers::SHIFT,
        Vec::new(),
    );
    let b = bounds(&app, id);
    assert!(close(b.center().x, 35.0), "{b:?}");
    assert!(b.width() > 55.0);
    app.undo();
    // Ctrl, dragged past the other side: the mirror image at 100 %.
    let e = handle(&app, Handle::E);
    let w = handle(&app, Handle::W);
    let past = w - egui::vec2((e.x - w.x) * 0.8, 0.0);
    drag(&ctx, &mut app, e, past, Modifiers::CTRL, Vec::new());
    let b = bounds(&app, id);
    assert!(close(b.width(), 50.0), "{b:?}");
    assert!(close(b.x1, 10.0) && close(b.x0, -40.0), "{b:?}");
    let t = app
        .doc()
        .find_shape(id)
        .map(|s| s.transform)
        .expect("shape");
    assert!(t.determinant() < 0.0, "mirrored");
}

#[test]
fn rotate_mode_skews_with_the_sides_and_turns_about_the_moved_centre() {
    let (ctx, mut app, id) = setup();
    app.rotate_mode = true;
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    // The top handle slants the rectangle sideways; the bottom stays.
    let n = handle(&app, Handle::N);
    drag(
        &ctx,
        &mut app,
        n,
        n + egui::vec2(80.0, 0.0),
        Modifiers::NONE,
        Vec::new(),
    );
    let b = bounds(&app, id);
    assert!(b.width() > 55.0, "slanted: {b:?}");
    assert!(close(b.y0, 10.0) && close(b.y1, 40.0), "{b:?}");
    app.undo();
    // Move the centre of rotation onto the lower left corner, then turn.
    app.rotate_mode = true;
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    let pivot = app.view.to_screen(Point::new(35.0, 25.0));
    let corner = app.view.to_screen(Point::new(10.0, 10.0));
    drag(&ctx, &mut app, pivot, corner, Modifiers::NONE, Vec::new());
    assert!(app.rotate_mode, "moving the centre keeps rotate mode");
    let c = app.rotation_center().expect("centre");
    assert!(close(c.x, 10.0) && close(c.y, 10.0), "{c:?}");
    // A quarter turn about (10, 10) with Ctrl (15 degree steps).
    let ne = handle(&app, Handle::NE);
    let centre = app.view.to_screen(c);
    let r = ne - centre;
    let quarter = centre + egui::vec2(-r.y, r.x);
    drag(&ctx, &mut app, ne, quarter, Modifiers::CTRL, Vec::new());
    let b = bounds(&app, id);
    // The corner on the centre stays where it was.
    assert!(
        close(b.x0, 10.0) || close(b.x1, 10.0),
        "turned about the corner: {b:?}"
    );
    assert!(close(b.width(), 30.0) && close(b.height(), 50.0), "{b:?}");
}

#[test]
fn handle_modifiers_shape_the_scale() {
    let app = App::headless();
    let b = Rect::new(0.0, 0.0, 40.0, 20.0);
    let scale = |h: Handle, anchor: Point, current: Point, mods: HandleMods| {
        app.preview_transform_of(&Drag::Scale {
            handle: h,
            anchor,
            start_bounds: b,
            current,
            grab: tracedraw_core::geometry::Vec2::ZERO,
            mods,
        })
        .expect("transform")
    };
    let none = HandleMods::default();
    let shift = HandleMods {
        from_center: true,
        ..none
    };
    let ctrl = HandleMods {
        steps: true,
        ..none
    };
    let alt = HandleMods { free: true, ..none };
    let at = |t: Affine, x: f64, y: f64| t * Point::new(x, y);
    // Plain: the left side stays.
    let t = scale(
        Handle::E,
        Point::new(0.0, 10.0),
        Point::new(80.0, 10.0),
        none,
    );
    assert!(close(at(t, 40.0, 10.0).x, 80.0) && close(at(t, 0.0, 10.0).x, 0.0));
    // Shift: about the centre (20).
    let t = scale(
        Handle::E,
        Point::new(0.0, 10.0),
        Point::new(60.0, 10.0),
        shift,
    );
    assert!(close(at(t, 40.0, 10.0).x, 60.0) && close(at(t, 0.0, 10.0).x, -20.0));
    // Ctrl: whole steps; past the anchor the mirror image at 100 %.
    let t = scale(
        Handle::E,
        Point::new(0.0, 10.0),
        Point::new(-35.0, 10.0),
        ctrl,
    );
    assert!(close(at(t, 40.0, 10.0).x, -40.0));
    let t = scale(
        Handle::E,
        Point::new(0.0, 10.0),
        Point::new(70.0, 10.0),
        ctrl,
    );
    assert!(close(at(t, 40.0, 10.0).x, 80.0));
    // Ctrl and Shift: steps about the centre.
    let both = HandleMods {
        from_center: true,
        steps: true,
        ..none
    };
    let t = scale(
        Handle::E,
        Point::new(0.0, 10.0),
        Point::new(-15.0, 10.0),
        both,
    );
    assert!(close(at(t, 40.0, 10.0).x, -20.0));
    // A corner keeps the proportions unless Alt frees it, by where the
    // pointer falls along the box's diagonal (#383): (2 * 40² + 1 * 20²) /
    // (40² + 20²) = 1.8; on the diagonal the corner follows the pointer.
    let t = scale(
        Handle::NE,
        Point::new(0.0, 0.0),
        Point::new(80.0, 20.0),
        none,
    );
    assert!(close(at(t, 40.0, 20.0).y, 36.0) && close(at(t, 40.0, 20.0).x, 72.0));
    let t = scale(
        Handle::NE,
        Point::new(0.0, 0.0),
        Point::new(60.0, 30.0),
        none,
    );
    assert!(close(at(t, 40.0, 20.0).x, 60.0) && close(at(t, 40.0, 20.0).y, 30.0));
    // On a wide, low box a 1 mm move up grows it less than a 1 mm move to
    // the right: the long side sets the pace (with the larger of the two
    // ratios the move up grew it twice as fast).
    let up = scale(
        Handle::NE,
        Point::new(0.0, 0.0),
        Point::new(40.0, 21.0),
        none,
    );
    let right = scale(
        Handle::NE,
        Point::new(0.0, 0.0),
        Point::new(41.0, 20.0),
        none,
    );
    let k = |t: Affine| t.as_coeffs()[0];
    assert!(k(up) < k(right) && k(up) > 1.0, "{} {}", k(up), k(right));
    let t = scale(
        Handle::NE,
        Point::new(0.0, 0.0),
        Point::new(80.0, 20.0),
        alt,
    );
    assert!(close(at(t, 40.0, 20.0).x, 80.0) && close(at(t, 40.0, 20.0).y, 20.0));
}

#[test]
fn side_handles_skew() {
    let b = Rect::new(0.0, 0.0, 40.0, 20.0);
    let none = HandleMods::default();
    let at = |t: Affine, x: f64, y: f64| t * Point::new(x, y);
    // Top handle: the top moves by the drag, the bottom stays.
    let t = skew_transform(Handle::N, b, Vec2::new(20.0, 0.0), none, 15.0).expect("skew");
    assert!(close(at(t, 0.0, 20.0).x, 20.0) && close(at(t, 0.0, 0.0).x, 0.0));
    // Bottom handle: the other way round.
    let t = skew_transform(Handle::S, b, Vec2::new(20.0, 0.0), none, 15.0).expect("skew");
    assert!(close(at(t, 0.0, 0.0).x, 20.0) && close(at(t, 0.0, 20.0).x, 0.0));
    // Right handle: up and down.
    let t = skew_transform(Handle::E, b, Vec2::new(0.0, 10.0), none, 15.0).expect("skew");
    assert!(close(at(t, 40.0, 0.0).y, 10.0) && close(at(t, 0.0, 0.0).y, 0.0));
    // Shift: about the centre line.
    let shift = HandleMods {
        from_center: true,
        ..none
    };
    let t = skew_transform(Handle::N, b, Vec2::new(20.0, 0.0), shift, 15.0).expect("skew");
    assert!(close(at(t, 0.0, 20.0).x, 20.0) && close(at(t, 0.0, 0.0).x, -20.0));
    // Ctrl: the slant in 15 degree steps (37 degrees becomes 30).
    let ctrl = HandleMods {
        steps: true,
        ..none
    };
    let t = skew_transform(Handle::N, b, Vec2::new(15.0, 0.0), ctrl, 15.0).expect("skew");
    let x = at(t, 0.0, 20.0).x;
    assert!((x - 20.0 * 30f64.to_radians().tan()).abs() < 1e-6, "{x}");
    // Corners and flat boxes make none.
    assert!(skew_transform(Handle::NE, b, Vec2::new(5.0, 0.0), none, 15.0).is_none());
    let flat = Rect::new(0.0, 0.0, 40.0, 0.0);
    assert!(skew_transform(Handle::N, flat, Vec2::new(5.0, 0.0), none, 15.0).is_none());
}

#[test]
fn the_pointer_shows_what_a_press_would_do() {
    let (ctx, mut app, id) = setup();
    let cursor = |app: &App, at: Pos2| for_tool(app, false, false, false, Some(at));
    // Scale handles: resize arrows along their direction.
    for h in [
        Handle::N,
        Handle::S,
        Handle::E,
        Handle::W,
        Handle::NE,
        Handle::NW,
        Handle::SE,
        Handle::SW,
    ] {
        assert_eq!(
            cursor(&app, handle(&app, h)),
            ToolCursor::System(resize_icon(h)),
            "{h:?}"
        );
    }
    // The selected object: the move arrows; elsewhere the plain arrow.
    let inside = app.view.to_screen(Point::new(35.0, 25.0));
    assert_eq!(
        cursor(&app, inside),
        ToolCursor::System(egui::CursorIcon::Move)
    );
    let empty = app.view.to_screen(Point::new(150.0, 250.0));
    assert_eq!(
        cursor(&app, empty),
        ToolCursor::System(egui::CursorIcon::Default)
    );
    // Rotate mode: corners turn, sides slant, the centre moves.
    app.rotate_mode = true;
    assert_eq!(cursor(&app, handle(&app, Handle::NE)), ToolCursor::Rotate);
    assert_eq!(
        cursor(&app, handle(&app, Handle::N)),
        ToolCursor::Skew { horizontal: true }
    );
    assert_eq!(
        cursor(&app, handle(&app, Handle::E)),
        ToolCursor::Skew { horizontal: false }
    );
    assert_eq!(
        cursor(&app, inside),
        ToolCursor::System(egui::CursorIcon::Move)
    );
    app.rotate_mode = false;
    // The Shape tool: its arrow, marked over a node and over a segment of
    // a curve.
    app.select(vec![id]);
    app.convert_to_curves();
    app.set_tool(Tool::Shape);
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    let node = app.view.to_screen(Point::new(10.0, 10.0));
    let segment = app.view.to_screen(Point::new(35.0, 10.0));
    assert_eq!(cursor(&app, node), ToolCursor::ShapeArrow(ShapeMark::Node));
    assert_eq!(
        cursor(&app, segment),
        ToolCursor::ShapeArrow(ShapeMark::Segment)
    );
    assert_eq!(
        cursor(&app, empty),
        ToolCursor::ShapeArrow(ShapeMark::Nothing)
    );
    // The handles framing selected nodes: resize arrows, or turning and
    // slant arrows.
    app.select_all_nodes();
    app.node_transform = crate::node_edit::NodeTransformMode::StretchScale;
    let at = |app: &App, h: Handle| {
        let p = app
            .node_transform_handles()
            .into_iter()
            .find(|(k, _)| *k == h)
            .map(|(_, p)| p)
            .expect("handle");
        app.view.to_screen(p)
    };
    assert_eq!(
        cursor(&app, at(&app, Handle::E)),
        ToolCursor::System(resize_icon(Handle::E))
    );
    app.node_transform = crate::node_edit::NodeTransformMode::RotateSkew;
    assert_eq!(cursor(&app, at(&app, Handle::NE)), ToolCursor::Rotate);
    assert_eq!(
        cursor(&app, at(&app, Handle::S)),
        ToolCursor::Skew { horizontal: true }
    );
    app.node_transform = crate::node_edit::NodeTransformMode::None;
    // While dragging, the pointer keeps the drag's shape.
    app.set_tool(Tool::Pick);
    app.drag = Drag::Rotate {
        center: Point::new(0.0, 0.0),
        start_angle: 0.0,
        current_angle: 0.0,
    };
    assert_eq!(cursor(&app, empty), ToolCursor::Rotate);
    // Moving: the arrow with a dotted box; once a right click makes the
    // move leave the original, the box gets a plus.
    app.drag = Drag::Move {
        start: Point::ZERO,
        last: Point::ZERO,
        total: Vec2::ZERO,
        start_bounds: Rect::ZERO,
        source: None,
    };
    assert_eq!(cursor(&app, empty), ToolCursor::DragMove { copy: false });
    app.drag_copy = true;
    assert_eq!(cursor(&app, empty), ToolCursor::DragMove { copy: true });
    app.drag_copy = false;
    app.drag = Drag::None;
}

impl App {
    /// No context menu or drop menu was left open by a right click
    /// during a drag.
    fn dialog_is_none_or_menu_closed(&self) -> bool {
        self.context_menu.is_none() && self.drop_menu.is_none()
    }
}

#[test]
fn the_status_bar_tells_how_far_how_much_and_what_angle() {
    let (_ctx, mut app, _id) = setup();
    let info = |app: &App| crate::ui::status::drag_info(app).unwrap_or_default();
    assert!(info(&app).is_empty(), "nothing while not dragging");
    app.drag = Drag::Move {
        start: Point::new(0.0, 0.0),
        last: Point::new(0.0, 0.0),
        total: Vec2::new(30.0, 40.0),
        start_bounds: Rect::new(10.0, 10.0, 60.0, 40.0),
        source: None,
    };
    let text = info(&app);
    assert!(text.contains("50.000") && text.contains("53.1"), "{text}");
    app.drag = Drag::Scale {
        handle: Handle::E,
        anchor: Point::new(10.0, 25.0),
        start_bounds: Rect::new(10.0, 10.0, 60.0, 40.0),
        current: Point::new(85.0, 25.0),
        grab: tracedraw_core::geometry::Vec2::ZERO,
        mods: HandleMods::default(),
    };
    let text = info(&app);
    assert!(text.contains("150.0") && text.contains("100.0"), "{text}");
    app.drag = Drag::Rotate {
        center: Point::new(0.0, 0.0),
        start_angle: 0.0,
        current_angle: 30f64.to_radians(),
    };
    assert!(info(&app).contains("30.0"), "{}", info(&app));
    app.drag = Drag::Skew {
        handle: Handle::N,
        start_bounds: Rect::new(10.0, 10.0, 60.0, 40.0),
        start: Point::new(35.0, 40.0),
        current: Point::new(65.0, 40.0),
        mods: HandleMods::default(),
    };
    assert!(info(&app).contains("45.0"), "{}", info(&app));
    // A copy says so.
    app.drag_copy = true;
    assert!(info(&app).contains(&crate::i18n::tr("status.drag_copy_short")));
    app.drag = Drag::None;
    app.drag_copy = false;
}

#[test]
fn objects_dropped_on_the_document_palette_give_their_colours() {
    let (ctx, mut app, id) = setup();
    app.settings.palette.show_document = true;
    for _ in 0..2 {
        frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    }
    let row = app.doc_palette_rect.expect("the document palette shows");
    let from = app.view.to_screen(Point::new(35.0, 25.0));
    drag(
        &ctx,
        &mut app,
        from,
        row.center(),
        Modifiers::NONE,
        Vec::new(),
    );
    // The object stays; its fill joined the document palette.
    assert!(close(bounds(&app, id).x0, 10.0), "{:?}", bounds(&app, id));
    assert!(app
        .doc()
        .palette
        .contains(&tracedraw_core::Color::rgb8(200, 30, 30)));
}

fn click_at(ctx: &egui::Context, app: &mut App, at: Pos2, mods: Modifiers) {
    frame(ctx, app, vec![Event::PointerMoved(at)], mods);
    frame(
        ctx,
        app,
        vec![button(at, true, PointerButton::Primary, mods)],
        mods,
    );
    frame(
        ctx,
        app,
        vec![button(at, false, PointerButton::Primary, mods)],
        mods,
    );
    // Past the double-click time (frames advance the input clock by a
    // sixtieth of a second each), so the next click is a click of its own.
    for _ in 0..20 {
        frame(ctx, app, Vec::new(), mods);
    }
}

fn last_path(app: &App) -> tracedraw_core::BezPath {
    app.doc()
        .all_layers()
        .flat_map(|l| &l.shapes)
        .filter_map(|s| match &s.kind {
            ShapeKind::Path { .. } => Some(s.page_path()),
            _ => None,
        })
        .last()
        .expect("a curve")
}

#[test]
fn ctrl_keeps_lines_to_the_constrain_angle() {
    use crate::interaction::constrain_from;
    let o = Point::new(0.0, 0.0);
    // 5 degrees off the horizontal comes back to it, at the same reach
    // along it.
    let p = constrain_from(o, Point::new(10.0, 5f64.to_radians().tan() * 10.0), 15.0);
    assert!(p.y.abs() < 1e-9 && (p.x - 10.0).abs() < 1e-9, "{p:?}");
    // 40 degrees goes to 45.
    let a = 40f64.to_radians();
    let p = constrain_from(o, Point::new(a.cos() * 10.0, a.sin() * 10.0), 15.0);
    assert!((p.y.atan2(p.x).to_degrees() - 45.0).abs() < 1e-9);
    assert_eq!(constrain_from(o, o, 15.0), o);

    // Freehand: two clicks make a straight line; with Ctrl a level one.
    let (ctx, mut app, _id) = setup();
    app.set_tool(Tool::Freehand);
    let a = app.view.to_screen(Point::new(100.0, 100.0));
    let b = app.view.to_screen(Point::new(160.0, 104.0));
    click_at(&ctx, &mut app, a, Modifiers::NONE);
    click_at(&ctx, &mut app, b, Modifiers::CTRL);
    let path = last_path(&app);
    let pts: Vec<Point> = path
        .elements()
        .iter()
        .filter_map(|e| match e {
            tracedraw_core::geometry::PathEl::MoveTo(p)
            | tracedraw_core::geometry::PathEl::LineTo(p) => Some(*p),
            _ => None,
        })
        .collect();
    assert_eq!(pts.len(), 2, "{pts:?}");
    assert!((pts[0].y - pts[1].y).abs() < 1e-6, "level: {pts:?}");
    assert!(pts[1].x > pts[0].x + 50.0);

    // Polyline: Ctrl keeps the next segment to 15 degree steps.
    app.set_tool(Tool::Polyline);
    let a = app.view.to_screen(Point::new(100.0, 150.0));
    let b = app.view.to_screen(Point::new(150.0, 196.0));
    click_at(&ctx, &mut app, a, Modifiers::NONE);
    click_at(&ctx, &mut app, b, Modifiers::CTRL);
    app.finish_curve();
    let path = last_path(&app);
    let pts: Vec<Point> = path
        .elements()
        .iter()
        .filter_map(|e| match e {
            tracedraw_core::geometry::PathEl::MoveTo(p)
            | tracedraw_core::geometry::PathEl::LineTo(p) => Some(*p),
            _ => None,
        })
        .collect();
    let d = pts[1] - pts[0];
    assert!(
        (d.y.atan2(d.x).to_degrees() - 45.0).abs() < 1e-6,
        "45 degrees: {pts:?}"
    );
}

/// Two filled 20 x 20 mm squares grouped (at x 100 and 140, y 100) and a
/// third square on its own at (100, 150), nothing selected.
fn grouped() -> (egui::Context, App, [ShapeId; 3], ShapeId) {
    let (ctx, mut app, first) = setup();
    app.select(vec![first]);
    app.delete_selection();
    let mut ids = Vec::new();
    for (x, y) in [(100.0, 100.0), (140.0, 100.0), (100.0, 150.0)] {
        let id = app
            .new_shape(ShapeKind::Rect {
                rect: Rect::new(x, y, x + 20.0, y + 20.0),
                radius: 0.0,
                corners: None,
            })
            .expect("rect");
        app.select(vec![id]);
        app.apply_fill(tracedraw_core::Fill::Solid(tracedraw_core::Color::rgb8(
            30, 30, 200,
        )));
        ids.push(id);
    }
    app.select(vec![ids[0], ids[1]]);
    app.group_selection();
    let group = app.selection[0];
    // The group is moved, so its objects' space is not the page's.
    app.run(tracedraw_core::Command::TransformShapes {
        shapes: vec![group],
        transform: Affine::translate((5.0, 0.0)),
    });
    app.select(Vec::new());
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    (ctx, app, [ids[0], ids[1], ids[2]], group)
}

fn page_bounds(app: &App, id: ShapeId) -> Rect {
    app.doc()
        .shape_on_page(id)
        .map(|s| s.bounds())
        .expect("shape")
}

#[test]
fn ctrl_click_chooses_an_object_inside_a_group() {
    let (ctx, mut app, [a, b, _c], group) = grouped();
    let on_a = app.view.to_screen(Point::new(115.0, 110.0));
    // A plain click takes the group.
    click_at(&ctx, &mut app, on_a, Modifiers::NONE);
    assert_eq!(app.selection, vec![group]);
    // Ctrl+click: the square inside it, with round handles and the status
    // bar saying where it is.
    click_at(&ctx, &mut app, on_a, Modifiers::CTRL);
    assert_eq!(app.selection, vec![a]);
    assert!(app.selection_nested());
    assert!(crate::ui::status::object_info(&app).contains("in a group"));
    let before_b = page_bounds(&app, b);
    // Dragging it moves that square only, by the drag on the page.
    let before = page_bounds(&app, a);
    let to = app.view.to_screen(Point::new(115.0, 130.0));
    drag(&ctx, &mut app, on_a, to, Modifiers::NONE, Vec::new());
    let after = page_bounds(&app, a);
    assert!(close(after.y0 - before.y0, 20.0), "{before:?} {after:?}");
    assert!(close(after.x0, before.x0));
    assert_eq!(page_bounds(&app, b), before_b);
    assert_eq!(app.doc().top_level_of(a), Some(group));
    // Undo and redo keep it chosen.
    app.undo();
    assert_eq!(app.selection, vec![a]);
    assert!(close(page_bounds(&app, a).y0, before.y0));
    app.redo();
    assert_eq!(app.selection, vec![a]);
    // A colour reaches it alone.
    app.apply_fill(tracedraw_core::Fill::Solid(tracedraw_core::Color::rgb8(
        0, 200, 0,
    )));
    let fill_of = |app: &App, id| app.doc().find_shape(id).map(|s| s.fill.clone());
    assert_ne!(fill_of(&app, a), fill_of(&app, b));
    // Ctrl+Shift+click adds the other square; the group is never selected
    // with its own objects.
    let on_b = app.view.to_screen(Point::new(155.0, 110.0));
    click_at(&ctx, &mut app, on_b, Modifiers::CTRL | Modifiers::SHIFT);
    assert_eq!(app.selection, vec![a, b]);
    app.select_add(group);
    assert_eq!(app.selection, vec![group]);
    // Delete takes the chosen square out of the group.
    app.select(vec![a]);
    app.delete_selection();
    assert!(app.doc().find_shape(a).is_none());
    assert_eq!(app.doc().top_level_of(b), Some(group));
}

#[test]
fn alt_click_digs_to_the_object_beneath() {
    let (ctx, mut app, [_a, _b, c], _group) = grouped();
    // A fourth square over the third one.
    let top = app
        .new_shape(ShapeKind::Rect {
            rect: Rect::new(105.0, 155.0, 125.0, 175.0),
            radius: 0.0,
            corners: None,
        })
        .expect("rect");
    app.select(vec![top]);
    app.apply_fill(tracedraw_core::Fill::Solid(tracedraw_core::Color::rgb8(
        200, 200, 0,
    )));
    let both = app.view.to_screen(Point::new(110.0, 160.0));
    click_at(&ctx, &mut app, both, Modifiers::NONE);
    assert_eq!(app.selection, vec![top]);
    click_at(&ctx, &mut app, both, Modifiers::ALT);
    assert_eq!(app.selection, vec![c]);
    // Once more: round to the top again.
    click_at(&ctx, &mut app, both, Modifiers::ALT);
    assert_eq!(app.selection, vec![top]);
    assert!(!app.rotate_mode);
}

#[test]
fn alt_marquees_take_what_they_touch() {
    let (ctx, mut app, [_a, _b, c], group) = grouped();
    // Across the lone square and the group's first square, holding
    // neither wholly.
    let from = app.view.to_screen(Point::new(90.0, 112.0));
    let to = app.view.to_screen(Point::new(112.0, 158.0));
    drag(&ctx, &mut app, from, to, Modifiers::NONE, Vec::new());
    assert!(app.selection.is_empty());
    drag(&ctx, &mut app, from, to, Modifiers::ALT, Vec::new());
    let mut sel = app.selection.clone();
    sel.sort_by_key(|s| s.raw());
    let mut want = vec![group, c];
    want.sort_by_key(|s| s.raw());
    assert_eq!(sel, want);
    // Round all three: everything.
    app.select(Vec::new());
    let from = app.view.to_screen(Point::new(90.0, 90.0));
    let to = app.view.to_screen(Point::new(180.0, 180.0));
    drag(&ctx, &mut app, from, to, Modifiers::NONE, Vec::new());
    assert_eq!(app.selection.len(), 2);
}

#[test]
fn the_touching_option_swaps_what_alt_does() {
    let (ctx, mut app, [_a, _b, c], group) = grouped();
    app.settings.marquee_touching = true;
    let from = app.view.to_screen(Point::new(90.0, 112.0));
    let to = app.view.to_screen(Point::new(112.0, 158.0));
    drag(&ctx, &mut app, from, to, Modifiers::NONE, Vec::new());
    let mut sel = app.selection.clone();
    sel.sort_by_key(|s| s.raw());
    let mut want = vec![group, c];
    want.sort_by_key(|s| s.raw());
    assert_eq!(sel, want);
    app.select(Vec::new());
    drag(&ctx, &mut app, from, to, Modifiers::ALT, Vec::new());
    assert!(app.selection.is_empty());
}

/// The screen points of a lasso through `pts` (page), in small steps as a
/// hand draws it.
fn lasso_path(app: &App, pts: &[Point]) -> Vec<Pos2> {
    let mut out = Vec::new();
    for w in pts.windows(2) {
        for i in 0..8 {
            let t = i as f64 / 8.0;
            out.push(app.view.to_screen(w[0] + (w[1] - w[0]) * t));
        }
    }
    if let Some(last) = pts.last() {
        out.push(app.view.to_screen(*last));
    }
    out
}

#[test]
fn the_freehand_pick_lassoes_and_has_the_pick_tools_handles() {
    let (ctx, mut app, [_a, _b, c], group) = grouped();
    app.set_tool(Tool::FreeformPick);
    // A loop round the lone square only.
    let pts = [
        Point::new(90.0, 140.0),
        Point::new(130.0, 140.0),
        Point::new(130.0, 180.0),
        Point::new(90.0, 180.0),
        Point::new(90.0, 141.0),
    ];
    let screen = lasso_path(&app, &pts);
    let lasso = |ctx: &egui::Context, app: &mut App, mods: Modifiers| {
        frame(ctx, app, vec![Event::PointerMoved(screen[0])], mods);
        frame(
            ctx,
            app,
            vec![button(screen[0], true, PointerButton::Primary, mods)],
            mods,
        );
        for p in &screen[1..] {
            frame(ctx, app, vec![Event::PointerMoved(*p)], mods);
        }
        frame(
            ctx,
            app,
            vec![button(
                *screen.last().expect("point"),
                false,
                PointerButton::Primary,
                mods,
            )],
            mods,
        );
        frame(ctx, app, Vec::new(), Modifiers::NONE);
    };
    lasso(&ctx, &mut app, Modifiers::NONE);
    assert_eq!(app.selection, vec![c]);
    // The handles scale as with the Pick tool.
    let before = page_bounds(&app, c);
    let ne = handle(&app, Handle::NE);
    drag(
        &ctx,
        &mut app,
        ne,
        ne + egui::vec2(40.0, -40.0),
        Modifiers::NONE,
        Vec::new(),
    );
    assert!(page_bounds(&app, c).width() > before.width() + 1.0);
    // A loop through the group's first square takes the group with Alt.
    app.select(Vec::new());
    let screen2 = lasso_path(
        &app,
        &[
            Point::new(110.0, 90.0),
            Point::new(130.0, 90.0),
            Point::new(130.0, 130.0),
            Point::new(110.0, 130.0),
        ],
    );
    let mods = Modifiers::ALT;
    frame(&ctx, &mut app, vec![Event::PointerMoved(screen2[0])], mods);
    frame(
        &ctx,
        &mut app,
        vec![button(screen2[0], true, PointerButton::Primary, mods)],
        mods,
    );
    for p in &screen2[1..] {
        frame(&ctx, &mut app, vec![Event::PointerMoved(*p)], mods);
    }
    frame(
        &ctx,
        &mut app,
        vec![button(
            *screen2.last().expect("point"),
            false,
            PointerButton::Primary,
            mods,
        )],
        mods,
    );
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    assert_eq!(app.selection, vec![group]);
}

#[test]
fn clicking_the_selection_or_its_centre_mark_turns_the_handles() {
    let (ctx, mut app, [_a, _b, c], group) = grouped();
    app.select(vec![group, c]);
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    // Any selected object toggles rotate mode for the whole selection.
    let on_c = app.view.to_screen(Point::new(110.0, 160.0));
    click_at(&ctx, &mut app, on_c, Modifiers::NONE);
    assert!(app.rotate_mode);
    assert_eq!(app.selection.len(), 2);
    // A moved centre of rotation stays where it was put when rotate mode
    // goes off and on again.
    let moved = Point::new(105.0, 105.0);
    app.set_pivot(moved);
    click_at(&ctx, &mut app, on_c, Modifiers::NONE);
    assert!(!app.rotate_mode);
    click_at(&ctx, &mut app, on_c, Modifiers::NONE);
    assert!(app.rotate_mode);
    assert_eq!(app.rotation_center(), Some(moved));
    // A double click on it puts it back in the middle.
    let middle = app.selection_bounds().expect("bounds").center();
    app.reset_pivot();
    assert_eq!(app.rotation_center(), Some(middle));
    // The centre mark of an outline-only ring: it lies on nothing (with
    // objects not treated as filled), yet a click there turns the handles
    // and a drag moves the ring.
    app.settings.treat_all_filled = false;
    app.select(Vec::new());
    let ring = app
        .new_shape(ShapeKind::Ellipse {
            rect: Rect::new(20.0, 200.0, 60.0, 240.0),
            arc: None,
        })
        .expect("ellipse");
    app.select(vec![ring]);
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    let mark = app.view.to_screen(Point::new(40.0, 220.0));
    assert_eq!(app.hit_test(Point::new(40.0, 220.0)), None);
    assert_eq!(
        for_tool(&app, false, false, false, Some(mark)),
        ToolCursor::System(egui::CursorIcon::Move)
    );
    let before = page_bounds(&app, ring);
    let to = app.view.to_screen(Point::new(70.0, 220.0));
    drag(&ctx, &mut app, mark, to, Modifiers::NONE, Vec::new());
    assert!(close(page_bounds(&app, ring).x0 - before.x0, 30.0));
    assert_eq!(app.selection, vec![ring]);
    let moved = app.view.to_screen(Point::new(70.0, 220.0));
    click_at(&ctx, &mut app, moved, Modifiers::NONE);
    assert!(app.rotate_mode);
}

#[test]
fn the_pointer_shows_moves_for_objects_and_guidelines() {
    let (_ctx, mut app, [a, _b, _c], _group) = grouped();
    let cursor = |app: &App, at: Pos2| for_tool(app, false, false, false, Some(at));
    // Unselected objects show the move arrows too.
    assert!(app.selection.is_empty());
    let on_a = app.view.to_screen(Point::new(115.0, 110.0));
    assert_eq!(
        cursor(&app, on_a),
        ToolCursor::System(egui::CursorIcon::Move)
    );
    assert!(app.doc().is_nested(a));
    // Guidelines: arrows across them; a locked one keeps the arrow.
    app.add_guide(tracedraw_core::document::Guide::horizontal(250.0));
    app.add_guide(tracedraw_core::document::Guide::vertical(250.0));
    let on_h = app.view.to_screen(Point::new(200.0, 250.0));
    let on_v = app.view.to_screen(Point::new(250.0, 200.0));
    assert_eq!(
        cursor(&app, on_h),
        ToolCursor::System(egui::CursorIcon::ResizeVertical)
    );
    assert_eq!(
        cursor(&app, on_v),
        ToolCursor::System(egui::CursorIcon::ResizeHorizontal)
    );
    app.set_guides_locked(&[0], true);
    assert_eq!(
        cursor(&app, on_h),
        ToolCursor::System(egui::CursorIcon::Default)
    );
    // Dragging a new guideline out of a ruler.
    app.drag = Drag::NewGuide {
        horizontal: true,
        pos: Point::ZERO,
    };
    assert_eq!(
        crate::ui::cursors::ruler_drag_cursor(&app),
        Some(egui::CursorIcon::ResizeVertical)
    );
    app.drag = Drag::None;
}

#[test]
fn a_right_click_in_the_shape_tool_takes_the_node_or_segment_under_it() {
    let (ctx, mut app, id) = setup();
    app.convert_to_curves();
    app.set_tool(Tool::Shape);
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    let right_click = |ctx: &egui::Context, app: &mut App, at: Pos2| {
        frame(ctx, app, vec![Event::PointerMoved(at)], Modifiers::NONE);
        frame(
            ctx,
            app,
            vec![button(at, true, PointerButton::Secondary, Modifiers::NONE)],
            Modifiers::NONE,
        );
        frame(
            ctx,
            app,
            vec![button(at, false, PointerButton::Secondary, Modifiers::NONE)],
            Modifiers::NONE,
        );
    };
    // The bottom side's middle: that segment's end node is chosen.
    let on_segment = app.view.to_screen(Point::new(35.0, 10.0));
    let (seg_shape, seg_index, _) = app.segment_at(Point::new(35.0, 10.0)).expect("segment");
    right_click(&ctx, &mut app, on_segment);
    assert!(app.context_menu.is_some());
    assert_eq!(app.node_selection, vec![(seg_shape, seg_index)]);
    assert_eq!(seg_shape, id);
    app.context_menu = None;
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    // A corner: that node.
    let corner = app.view.to_screen(Point::new(60.0, 40.0));
    let hit = app.node_at(Point::new(60.0, 40.0)).expect("node");
    right_click(&ctx, &mut app, corner);
    assert_eq!(app.node_selection, vec![(hit.shape, hit.index)]);
    app.context_menu = None;
}

#[test]
fn scale_handles_snap_to_other_objects_not_to_the_selection() {
    let (ctx, mut app, id) = setup();
    // A second rectangle whose left edge is at x = 80.
    let other = app
        .new_shape(ShapeKind::Rect {
            rect: Rect::new(80.0, 10.0, 100.0, 40.0),
            radius: 0.0,
            corners: None,
        })
        .expect("rect");
    app.select(vec![id]);
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    // Drag the right side handle to a little short of the other's edge.
    let e = handle(&app, Handle::E);
    let short = app.view.to_screen(Point::new(80.0, 25.0)) - egui::vec2(2.0, 0.0);
    drag(&ctx, &mut app, e, short, Modifiers::NONE, Vec::new());
    let b = bounds(&app, id);
    assert!((b.x1 - 80.0).abs() < 1e-6, "{b:?}");
    assert!((b.x0 - 10.0).abs() < 1e-6);
    // With snapping off the handle goes where the pointer is.
    app.undo();
    app.snap.off = true;
    let e = handle(&app, Handle::E);
    drag(&ctx, &mut app, e, short, Modifiers::NONE, Vec::new());
    let b = bounds(&app, id);
    assert!((b.x1 - 80.0).abs() > 0.1, "{b:?}");
    assert!(app.doc().find_shape(other).is_some());
}

#[test]
fn plus_duplicates_in_place_and_edits_nodes_in_the_shape_tool() {
    let (ctx, mut app, id) = setup();
    let n = shape_count(&app);
    frame(
        &ctx,
        &mut app,
        vec![key(egui::Key::Plus, Modifiers::NONE)],
        Modifiers::NONE,
    );
    assert_eq!(shape_count(&app), n + 1);
    let copy = app.selection[0];
    assert_ne!(copy, id);
    assert_eq!(bounds(&app, copy), bounds(&app, id));
    // The Shape tool: + adds a node in the middle of the segment ending at
    // the chosen node, - deletes the chosen nodes.
    app.select(vec![id]);
    app.convert_to_curves();
    app.set_tool(Tool::Shape);
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    let count = |app: &App| {
        app.doc()
            .find_shape(id)
            .and_then(|s| match &s.kind {
                ShapeKind::Path { path, .. } => Some(tracedraw_core::nodes::nodes(path).len()),
                _ => None,
            })
            .expect("curve")
    };
    let before = count(&app);
    let hit = app.node_at(Point::new(60.0, 40.0)).expect("node");
    app.node_selection = vec![(hit.shape, hit.index)];
    frame(
        &ctx,
        &mut app,
        vec![key(egui::Key::Plus, Modifiers::NONE)],
        Modifiers::NONE,
    );
    assert_eq!(count(&app), before + 1);
    let hit = app.node_at(Point::new(60.0, 40.0)).expect("node");
    app.node_selection = vec![(hit.shape, hit.index)];
    frame(
        &ctx,
        &mut app,
        vec![key(egui::Key::Minus, Modifiers::NONE)],
        Modifiers::NONE,
    );
    assert_eq!(count(&app), before);
    // Nothing chosen: + zooms.
    app.node_selection.clear();
    let zoom = app.view.zoom;
    frame(
        &ctx,
        &mut app,
        vec![key(egui::Key::Plus, Modifiers::NONE)],
        Modifiers::NONE,
    );
    assert!(app.view.zoom > zoom);
}

#[test]
fn dragging_a_segment_bends_it_and_a_click_marks_where_a_node_goes() {
    let (ctx, mut app, id) = setup();
    app.convert_to_curves();
    app.set_tool(Tool::Shape);
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    let path_of = |app: &App| match app.doc().find_shape(id).map(|s| s.kind.clone()) {
        Some(ShapeKind::Path { path, .. }) => path,
        other => panic!("not a curve: {other:?}"),
    };
    let before = path_of(&app);
    // Drag the middle of the bottom side 10 mm down.
    let (_, index, t) = app.segment_at(Point::new(35.0, 10.0)).expect("segment");
    let from = app.view.to_screen(Point::new(35.0, 10.0));
    let to = app.view.to_screen(Point::new(35.0, 0.0));
    drag(&ctx, &mut app, from, to, Modifiers::NONE, Vec::new());
    let bent = path_of(&app);
    let mid = tracedraw_core::nodes::point_on_segment(&bent, index, t).expect("point");
    assert!((mid - Point::new(35.0, 0.0)).hypot() < 0.3, "{mid:?}");
    // The corners stay; one undo puts the side back.
    let corners = |p: &tracedraw_core::BezPath| {
        tracedraw_core::nodes::nodes(p)
            .iter()
            .map(|n| n.pos)
            .collect::<Vec<_>>()
    };
    assert_eq!(corners(&bent), corners(&before));
    app.undo();
    assert_eq!(corners(&path_of(&app)), corners(&before));
    let mid = tracedraw_core::nodes::point_on_segment(&path_of(&app), index, t).expect("point");
    assert!((mid.y - 10.0).abs() < 1e-6);
    // A click on the right side chooses it and marks the spot; + puts a
    // node there.
    let n = corners(&path_of(&app)).len();
    let spot = Point::new(60.0, 32.0);
    let at = app.view.to_screen(spot);
    click_at(&ctx, &mut app, at, Modifiers::NONE);
    let (shape, seg, _) = app.segment_spot.expect("spot");
    assert_eq!(shape, id);
    assert_eq!(app.node_selection, vec![(id, seg)]);
    frame(
        &ctx,
        &mut app,
        vec![key(egui::Key::Plus, Modifiers::NONE)],
        Modifiers::NONE,
    );
    let after = corners(&path_of(&app));
    assert_eq!(after.len(), n + 1);
    assert!(after.iter().any(|q| (*q - spot).hypot() < 0.3), "{after:?}");
}

/// Two open curves for the join tests, the rectangle of `setup` gone.
fn two_open_curves(ctx: &egui::Context, app: &mut App, id: ShapeId) -> (ShapeId, ShapeId) {
    app.select(vec![id]);
    app.delete_selection();
    let mut a = tracedraw_core::BezPath::new();
    a.move_to((10.0, 10.0));
    a.curve_to((15.0, 20.0), (25.0, 20.0), (30.0, 10.0));
    let mut b = tracedraw_core::BezPath::new();
    b.move_to((44.0, 10.0));
    b.line_to((60.0, 30.0));
    let a = app
        .new_shape(ShapeKind::Path {
            path: a,
            closed: false,
        })
        .expect("curve a");
    let b = app
        .new_shape(ShapeKind::Path {
            path: b,
            closed: false,
        })
        .expect("curve b");
    app.select(vec![a, b]);
    app.set_tool(Tool::Shape);
    frame(ctx, app, Vec::new(), Modifiers::NONE);
    (a, b)
}

fn curve_path(app: &App, id: ShapeId) -> tracedraw_core::BezPath {
    match app.doc().find_shape(id).map(|s| s.kind.clone()) {
        Some(ShapeKind::Path { path, .. }) => path,
        other => panic!("not a curve: {other:?}"),
    }
}

#[test]
fn join_fuses_two_end_nodes_of_two_curves_without_a_line() {
    use tracedraw_core::geometry::PathEl;
    let (ctx, mut app, id) = setup();
    let (a, b) = two_open_curves(&ctx, &mut app, id);
    // The end of a (element 1) and the start of b (element 0).
    app.node_selection = vec![(a, 1), (b, 0)];
    app.join_selected_nodes();
    // One curve is left, one subpath through the joined node, no line.
    assert!(app.doc().find_shape(b).is_none());
    let p = curve_path(&app, a);
    assert_eq!(
        p.elements(),
        &[
            PathEl::MoveTo(Point::new(10.0, 10.0)),
            PathEl::CurveTo(
                Point::new(15.0, 20.0),
                Point::new(32.0, 20.0),
                Point::new(37.0, 10.0)
            ),
            PathEl::LineTo(Point::new(60.0, 30.0)),
        ]
    );
    // One undo step brings both curves back.
    app.undo();
    assert!(app.doc().find_shape(b).is_some());
    assert_eq!(tracedraw_core::nodes::nodes(&curve_path(&app, a)).len(), 2);
}

#[test]
fn join_of_the_two_ends_of_one_curve_closes_it_at_one_node() {
    use tracedraw_core::geometry::PathEl;
    let (ctx, mut app, id) = setup();
    let (a, _) = two_open_curves(&ctx, &mut app, id);
    let start = app.node_at(Point::new(10.0, 10.0)).expect("start");
    let end = app.node_at(Point::new(30.0, 10.0)).expect("end");
    app.node_selection = vec![(start.shape, start.index), (end.shape, end.index)];
    app.join_selected_nodes();
    let p = curve_path(&app, a);
    assert!(
        matches!(p.elements().last(), Some(PathEl::ClosePath)),
        "{p:?}"
    );
    // The two ends are one node now: the curve has a single node.
    assert_eq!(tracedraw_core::nodes::nodes(&p).len(), 1);
    assert!(matches!(
        app.doc().find_shape(a).map(|s| &s.kind),
        Some(ShapeKind::Path { closed: true, .. })
    ));
}

#[test]
fn dropping_an_end_node_on_another_end_node_joins_them() {
    let (ctx, mut app, id) = setup();
    let (a, b) = two_open_curves(&ctx, &mut app, id);
    // Drag the start of b onto the end of a.
    let from = app.view.to_screen(Point::new(44.0, 10.0));
    let to = app.view.to_screen(Point::new(30.0, 10.0));
    click_at(&ctx, &mut app, from, Modifiers::NONE);
    drag(&ctx, &mut app, from, to, Modifiers::NONE, Vec::new());
    assert!(app.doc().find_shape(b).is_none(), "b was not joined");
    let p = curve_path(&app, a);
    assert_eq!(tracedraw_core::nodes::nodes(&p).len(), 3);
    assert_eq!(
        p.elements()
            .iter()
            .filter(|e| matches!(e, tracedraw_core::geometry::PathEl::MoveTo(_)))
            .count(),
        1
    );
}

#[test]
fn delete_removes_a_clicked_segment_and_keeps_its_nodes() {
    let (ctx, mut app, id) = setup();
    app.convert_to_curves();
    app.set_tool(Tool::Shape);
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    let path_of = |app: &App| match app.doc().find_shape(id).map(|s| s.kind.clone()) {
        Some(ShapeKind::Path { path, .. }) => path,
        other => panic!("not a curve: {other:?}"),
    };
    let before = path_of(&app);
    let corners = |p: &tracedraw_core::BezPath| {
        tracedraw_core::nodes::nodes(p)
            .iter()
            .map(|n| n.pos)
            .collect::<Vec<_>>()
    };
    let has_close = |p: &tracedraw_core::BezPath| {
        p.elements()
            .iter()
            .any(|e| matches!(e, tracedraw_core::geometry::PathEl::ClosePath))
    };
    assert!(has_close(&before));
    // A click on the right side chooses the segment, not a node.
    let at = app.view.to_screen(Point::new(60.0, 32.0));
    click_at(&ctx, &mut app, at, Modifiers::NONE);
    assert!(app.chosen_segment().is_some());
    frame(
        &ctx,
        &mut app,
        vec![key(egui::Key::Delete, Modifiers::NONE)],
        Modifiers::NONE,
    );
    // The side went: the curve is open there, every corner stays.
    let after = path_of(&app);
    assert!(!has_close(&after), "{after:?}");
    let mut a = corners(&after);
    let mut b = corners(&before);
    let key_of = |p: &Point| ((p.x * 1000.0).round() as i64, (p.y * 1000.0).round() as i64);
    a.sort_by_key(key_of);
    b.sort_by_key(key_of);
    assert_eq!(a, b);
    assert!(app.chosen_segment().is_none());
    // One undo step puts the side back.
    app.undo();
    assert_eq!(path_of(&app), before);
}

#[test]
fn the_pen_closes_on_its_first_node_with_auto_add_delete_on() {
    let (ctx, mut app, id) = setup();
    app.select(vec![id]);
    app.delete_selection();
    app.set_tool(Tool::Pen);
    app.settings.curves.pen_auto_add_delete = true;
    // A pick reach wider than the join reach, as with large nodes.
    app.settings.curves.join_px = 3.0;
    app.settings.nodes.pick_distance = 10.0;
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    for q in [
        Point::new(20.0, 20.0),
        Point::new(60.0, 20.0),
        Point::new(40.0, 50.0),
    ] {
        let at = app.view.to_screen(q);
        click_at(&ctx, &mut app, at, Modifiers::NONE);
    }
    assert_eq!(app.curve.as_ref().map(|c| c.nodes.len()), Some(3));
    // A click 7 px off the first node: beyond the join reach, within the
    // pick reach. It closes the curve; it used to delete the first node.
    let first = app.view.to_screen(Point::new(20.0, 20.0));
    click_at(
        &ctx,
        &mut app,
        first + egui::vec2(7.0, 0.0),
        Modifiers::NONE,
    );
    assert!(app.curve.is_none(), "still drawing: {:?}", app.curve);
    let made = app.selected_shapes();
    let path = match made.first().map(|s| &s.kind) {
        Some(ShapeKind::Path { path, .. }) => path.clone(),
        other => panic!("no curve made: {other:?}"),
    };
    assert!(
        path.elements()
            .iter()
            .any(|e| matches!(e, tracedraw_core::geometry::PathEl::ClosePath)),
        "{path:?}"
    );
    assert_eq!(tracedraw_core::nodes::nodes(&path).len(), 3);
}

#[test]
fn node_keys_in_the_shape_tool() {
    let (ctx, mut app, id) = setup();
    // A curve with a smooth-able middle node.
    let mut path = tracedraw_core::BezPath::new();
    path.move_to((10.0, 10.0));
    path.curve_to((20.0, 10.0), (30.0, 20.0), (40.0, 30.0));
    path.curve_to((45.0, 40.0), (55.0, 30.0), (70.0, 20.0));
    app.select(vec![id]);
    app.delete_selection();
    let c = app
        .new_shape(ShapeKind::Path {
            path,
            closed: false,
        })
        .expect("curve");
    app.select(vec![c]);
    app.set_tool(Tool::Shape);
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    let hit = app.node_at(Point::new(40.0, 30.0)).expect("node");
    app.node_selection = vec![(hit.shape, hit.index)];
    let ty = |app: &App| app.node_type_of(c, hit.index);
    use tracedraw_core::nodes::NodeType;
    assert_eq!(ty(&app), NodeType::Cusp);
    // C switches cusp and smooth, S symmetrical and smooth (a cusp node
    // becomes symmetrical), Y makes it symmetrical. The type is kept with
    // the curve, so the way back shows although the handles stay aligned.
    for (k, want) in [
        (egui::Key::C, NodeType::Smooth),
        (egui::Key::C, NodeType::Cusp),
        (egui::Key::S, NodeType::Symmetrical),
        (egui::Key::S, NodeType::Smooth),
        (egui::Key::S, NodeType::Symmetrical),
        (egui::Key::C, NodeType::Cusp),
        (egui::Key::Y, NodeType::Symmetrical),
    ] {
        frame(
            &ctx,
            &mut app,
            vec![key(k, Modifiers::NONE)],
            Modifiers::NONE,
        );
        assert_eq!(ty(&app), want, "{k:?}");
        // Still the Shape tool, nothing aligned.
        assert_eq!(app.tool, Tool::Shape);
    }
    // C makes the symmetrical node cusp; its handles stay aligned, and a
    // handle dragged now moves alone.
    frame(
        &ctx,
        &mut app,
        vec![key(egui::Key::C, Modifiers::NONE)],
        Modifiers::NONE,
    );
    assert_eq!(ty(&app), NodeType::Cusp);
    let handles = |app: &App| match app.doc().find_shape(c).map(|s| s.kind.clone()) {
        Some(ShapeKind::Path { path, .. }) => tracedraw_core::nodes::nodes(&path)
            .into_iter()
            .find(|n| n.index == hit.index)
            .map(|n| (n.ctrl_in, n.ctrl_out))
            .expect("node"),
        _ => panic!("curve"),
    };
    let (before_in, before_out) = handles(&app);
    let out = before_out.expect("out handle");
    let from = app.view.to_screen(out);
    drag(
        &ctx,
        &mut app,
        from,
        from + egui::vec2(0.0, -25.0),
        Modifiers::NONE,
        Vec::new(),
    );
    let (after_in, after_out) = handles(&app);
    assert_ne!(after_out, before_out, "the dragged handle moved");
    assert_eq!(after_in, before_in, "the other handle stayed");
    assert_eq!(ty(&app), NodeType::Cusp);
    app.node_selection = vec![(hit.shape, hit.index)];
    // Home and End choose the curve's first and last node.
    frame(
        &ctx,
        &mut app,
        vec![key(egui::Key::Home, Modifiers::NONE)],
        Modifiers::NONE,
    );
    assert_eq!(app.node_selection, vec![(c, 0)]);
    frame(
        &ctx,
        &mut app,
        vec![key(egui::Key::End, Modifiers::NONE)],
        Modifiers::NONE,
    );
    let last = app.node_selection.clone();
    assert_eq!(last.len(), 1);
    assert!(last[0].1 > hit.index, "{last:?}");
    // The arrow keys move the chosen node, not the curve.
    let before = app.doc().find_shape(c).map(|s| s.bounds()).expect("curve");
    frame(
        &ctx,
        &mut app,
        vec![key(egui::Key::ArrowRight, Modifiers::NONE)],
        Modifiers::NONE,
    );
    let after = app.doc().find_shape(c).map(|s| s.bounds()).expect("curve");
    assert!(
        (after.x0 - before.x0).abs() < 1e-9,
        "the start stays: {before:?} {after:?}"
    );
    assert!(
        after.x1 > before.x1 + 1e-6,
        "the end node moved right: {before:?} {after:?}"
    );
    // Ctrl+Space: the Pick tool.
    frame(
        &ctx,
        &mut app,
        vec![key(egui::Key::Space, Modifiers::COMMAND)],
        Modifiers::COMMAND,
    );
    assert_eq!(app.tool, Tool::Pick);
}

#[test]
fn two_fingers_pan_without_dragging_anything() {
    let (ctx, mut app, id) = setup();
    let before = bounds(&app, id);
    let origin = app.view.to_screen(Point::new(0.0, 0.0));
    let touch = |id: u64, phase: egui::TouchPhase, pos: Pos2| Event::Touch {
        device_id: egui::TouchDeviceId(1),
        id: egui::TouchId(id),
        phase,
        pos,
        force: None,
    };
    // Both fingers on the object, then both move 60 points right. The
    // windowing layer also turns the first finger into the pointer.
    let (a, b) = (
        app.view.to_screen(Point::new(30.0, 25.0)),
        app.view.to_screen(Point::new(40.0, 25.0)),
    );
    frame(
        &ctx,
        &mut app,
        vec![
            Event::PointerMoved(a),
            button(a, true, PointerButton::Primary, Modifiers::NONE),
            touch(0, egui::TouchPhase::Start, a),
            touch(1, egui::TouchPhase::Start, b),
        ],
        Modifiers::NONE,
    );
    for i in 1..=6 {
        let d = egui::vec2(10.0 * i as f32, 0.0);
        frame(
            &ctx,
            &mut app,
            vec![
                Event::PointerMoved(a + d),
                touch(0, egui::TouchPhase::Move, a + d),
                touch(1, egui::TouchPhase::Move, b + d),
            ],
            Modifiers::NONE,
        );
    }
    let end = egui::vec2(60.0, 0.0);
    frame(
        &ctx,
        &mut app,
        vec![
            button(a + end, false, PointerButton::Primary, Modifiers::NONE),
            touch(0, egui::TouchPhase::End, a + end),
            touch(1, egui::TouchPhase::End, b + end),
        ],
        Modifiers::NONE,
    );
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    // The view moved with the fingers; the object did not.
    let moved = app.view.to_screen(Point::new(0.0, 0.0)) - origin;
    assert!(moved.x > 30.0, "{moved:?}");
    assert_eq!(bounds(&app, id), before);
    assert!(matches!(app.drag, Drag::None));
}

#[test]
fn a_moved_centre_of_rotation_is_kept_with_the_object() {
    let (ctx, mut app, id) = setup();
    // Rotate mode: drag the centre of rotation to the rectangle's lower
    // left corner area.
    let inside = app.view.to_screen(Point::new(35.0, 25.0));
    click_at(&ctx, &mut app, inside, Modifiers::NONE);
    assert!(app.rotate_mode);
    let centre = app.view.to_screen(app.rotation_center().expect("centre"));
    let target = app.view.to_screen(Point::new(20.0, 15.0));
    app.snap.off = true;
    drag(&ctx, &mut app, centre, target, Modifiers::NONE, Vec::new());
    let put = app.rotation_center().expect("centre");
    assert!((put - Point::new(20.0, 15.0)).hypot() < 0.5, "{put:?}");
    // Click away (nothing selected), then select the rectangle again and
    // turn rotate mode on: the centre is where it was put.
    let empty = app.view.to_screen(Point::new(150.0, 250.0));
    click_at(&ctx, &mut app, empty, Modifiers::NONE);
    assert!(app.selection.is_empty());
    click_at(&ctx, &mut app, inside, Modifiers::NONE);
    assert_eq!(app.selection, vec![id]);
    click_at(&ctx, &mut app, inside, Modifiers::NONE);
    assert!(app.rotate_mode);
    assert_eq!(app.rotation_center(), Some(put));
    // It travels with the object.
    app.transform_selection(Affine::translate((10.0, 5.0)));
    let after = app.rotation_center().expect("centre");
    assert!(
        (after - (put + Vec2::new(10.0, 5.0))).hypot() < 1e-9,
        "{after:?}"
    );
    // Another drawing has its own centres.
    app.new_document();
    assert!(app.pivots.single.is_empty());
}

/// The windowing layer delivers Ctrl+C, Ctrl+X and Ctrl+V as copy, cut
/// and paste events (a paste only while the system clipboard holds text),
/// never as key presses: those events copy, cut and paste objects.
#[test]
fn copy_cut_and_paste_events_work_on_objects() {
    let (ctx, mut app, id) = setup();
    let n = shape_count(&app);
    frame(&ctx, &mut app, vec![Event::Copy], Modifiers::COMMAND);
    assert!(app.clipboard.is_some());
    frame(
        &ctx,
        &mut app,
        vec![Event::Paste("<svg/>".into())],
        Modifiers::COMMAND,
    );
    assert_eq!(shape_count(&app), n + 1);
    let copy = app.selection[0];
    assert_ne!(copy, id);
    // One paste per frame, even if a key press comes along with it.
    frame(
        &ctx,
        &mut app,
        vec![
            key(egui::Key::V, Modifiers::COMMAND),
            Event::Paste("x".into()),
        ],
        Modifiers::COMMAND,
    );
    assert_eq!(shape_count(&app), n + 2);
    // Cut takes the selection away; paste brings it back.
    frame(&ctx, &mut app, vec![Event::Cut], Modifiers::COMMAND);
    assert_eq!(shape_count(&app), n + 1);
    frame(
        &ctx,
        &mut app,
        vec![Event::Paste("x".into())],
        Modifiers::COMMAND,
    );
    assert_eq!(shape_count(&app), n + 2);
    // With Shift held the paste lands in the middle of the view.
    frame(
        &ctx,
        &mut app,
        vec![Event::Paste("x".into())],
        Modifiers::COMMAND | Modifiers::SHIFT,
    );
    assert_eq!(shape_count(&app), n + 3);
    let view = app.view.visible_page_rect(app.canvas_rect).center();
    let b = app.selection_bounds().expect("bounds");
    assert!((b.center() - view).hypot() < 1e-6);
}

/// With only a picture or files on the system clipboard the windowing
/// layer drops the Ctrl+V press and sends no paste event: the release of
/// V with Ctrl still pastes, once; a release after a paste event does not.
#[test]
fn ctrl_v_pastes_when_only_its_release_comes_through() {
    let (ctx, mut app, _) = setup();
    let n = shape_count(&app);
    let v_up = |mods: Modifiers| Event::Key {
        key: egui::Key::V,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers: mods,
    };
    frame(&ctx, &mut app, vec![Event::Copy], Modifiers::COMMAND);
    frame(
        &ctx,
        &mut app,
        vec![v_up(Modifiers::COMMAND)],
        Modifiers::COMMAND,
    );
    assert_eq!(shape_count(&app), n + 1);
    // A paste event, then the release in a later frame: one paste.
    frame(
        &ctx,
        &mut app,
        vec![Event::Paste("x".into())],
        Modifiers::COMMAND,
    );
    frame(
        &ctx,
        &mut app,
        vec![v_up(Modifiers::COMMAND)],
        Modifiers::COMMAND,
    );
    assert_eq!(shape_count(&app), n + 2);
    // V let go without Ctrl is typing, not a paste.
    frame(&ctx, &mut app, vec![v_up(Modifiers::NONE)], Modifiers::NONE);
    assert_eq!(shape_count(&app), n + 2);
}

#[test]
fn handles_stand_outside_the_box_and_grabbing_one_does_not_jump() {
    let (ctx, mut app, id) = setup();
    let before = bounds(&app, id);
    let r = app.view.rect_to_screen(before);
    let off = crate::interaction::HANDLE_OFFSET;
    // Each handle stands off the box, so the outline stays in view.
    let e = handle(&app, Handle::E);
    let nw = handle(&app, Handle::NW);
    assert!((e.x - (r.right() + off)).abs() < 1e-3, "{e:?} {r:?}");
    assert!((nw.x - (r.left() - off)).abs() < 1e-3, "{nw:?} {r:?}");
    assert!((nw.y - (r.top() - off)).abs() < 1e-3, "{nw:?} {r:?}");
    // Dragging the right handle 20 points widens the object by 20 points,
    // not by the handle's distance from the box as well.
    drag(
        &ctx,
        &mut app,
        e,
        e + egui::vec2(20.0, 0.0),
        Modifiers::NONE,
        Vec::new(),
    );
    let after = bounds(&app, id);
    let grown = 20.0 / app.view.zoom as f64;
    assert!(
        close(after.width(), before.width() + grown),
        "{before:?} {after:?}"
    );
    assert!(close(after.x0, before.x0), "{before:?} {after:?}");
}

#[test]
fn turning_snaps_the_pointer_to_another_objects_node() {
    let (ctx, mut app, id) = setup();
    // Another rectangle up and to the right; its bottom left corner is a
    // node at (100, 80).
    let other = app
        .new_shape(ShapeKind::Rect {
            rect: Rect::new(100.0, 80.0, 130.0, 110.0),
            radius: 0.0,
            corners: None,
        })
        .expect("rect");
    app.select(vec![id]);
    app.rotate_mode = true;
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    let c = app.rotation_center().expect("centre");
    let ne = handle(&app, Handle::NE);
    let start = app.view.to_page(ne);
    // Released a few points off the node: the turn ends on the node.
    let node = Point::new(100.0, 80.0);
    let near = app.view.to_screen(node) + egui::vec2(3.0, 2.0);
    drag(&ctx, &mut app, ne, near, Modifiers::NONE, Vec::new());
    let t = app
        .doc()
        .find_shape(id)
        .map(|s| s.transform)
        .expect("shape");
    let k = t.as_coeffs();
    let turned = k[1].atan2(k[0]);
    let want = (node - c).atan2() - (start - c).atan2();
    assert!((turned - want).abs() < 1e-6, "{turned} {want}");
    assert!(app.doc().find_shape(other).is_some());
}

#[test]
fn drawing_a_new_shape_lets_the_selection_go_at_once() {
    let (ctx, mut app, id) = setup();
    assert_eq!(app.selection, vec![id]);
    app.set_tool(Tool::Rectangle);
    // Somewhere empty on the page, away from the rectangle.
    let from = app.view.to_screen(Point::new(100.0, 150.0));
    let to = app.view.to_screen(Point::new(140.0, 120.0));
    let none = Modifiers::NONE;
    frame(&ctx, &mut app, vec![Event::PointerMoved(from)], none);
    frame(
        &ctx,
        &mut app,
        vec![button(from, true, PointerButton::Primary, none)],
        none,
    );
    for i in 1..=3 {
        let p = from + (to - from) * (i as f32 / 3.0);
        frame(&ctx, &mut app, vec![Event::PointerMoved(p)], none);
    }
    // While the new rectangle is being drawn, nothing is selected.
    assert!(matches!(app.drag, Drag::Box { .. }));
    assert!(app.selection.is_empty(), "{:?}", app.selection);
    frame(
        &ctx,
        &mut app,
        vec![button(to, false, PointerButton::Primary, none)],
        none,
    );
    frame(&ctx, &mut app, Vec::new(), none);
    // Then the new rectangle alone is.
    assert_eq!(app.selection.len(), 1);
    assert_ne!(app.selection[0], id);
}

#[test]
fn ctrl_click_goes_one_group_level_in_at_a_time() {
    let (ctx, mut app, [a, _b, c], inner) = grouped();
    // The group of two squares and the third square in an outer group.
    app.select(vec![inner, c]);
    app.group_selection();
    let outer = app.selection[0];
    app.select(Vec::new());
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    let on_a = app.view.to_screen(Point::new(115.0, 110.0));
    click_at(&ctx, &mut app, on_a, Modifiers::NONE);
    assert_eq!(app.selection, vec![outer]);
    // Each Ctrl+click goes one level further in, then stays on the square.
    click_at(&ctx, &mut app, on_a, Modifiers::CTRL);
    assert_eq!(app.selection, vec![inner]);
    click_at(&ctx, &mut app, on_a, Modifiers::CTRL);
    assert_eq!(app.selection, vec![a]);
    click_at(&ctx, &mut app, on_a, Modifiers::CTRL);
    assert_eq!(app.selection, vec![a]);
}

#[test]
fn ctrl_keeps_the_marquee_square_and_space_copies_while_turning() {
    let (ctx, mut app, id) = setup();
    // Space while turning leaves a copy at the angle reached.
    app.rotate_mode = true;
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    let ne = handle(&app, Handle::NE);
    let count = shape_count(&app);
    drag(
        &ctx,
        &mut app,
        ne,
        ne + egui::vec2(-40.0, -60.0),
        Modifiers::NONE,
        vec![key(egui::Key::Space, Modifiers::NONE)],
    );
    assert_eq!(shape_count(&app), count + 1);
    assert!(app.doc().find_shape(id).is_some());
    // A marquee dragged with Ctrl is square.
    app.rotate_mode = false;
    app.select(Vec::new());
    let from = app.view.to_screen(Point::new(150.0, 250.0));
    frame(
        &ctx,
        &mut app,
        vec![Event::PointerMoved(from)],
        Modifiers::CTRL,
    );
    frame(
        &ctx,
        &mut app,
        vec![button(from, true, PointerButton::Primary, Modifiers::CTRL)],
        Modifiers::CTRL,
    );
    for i in 1..=3 {
        let p = from + egui::vec2(60.0, 20.0) * (i as f32 / 3.0);
        frame(
            &ctx,
            &mut app,
            vec![Event::PointerMoved(p)],
            Modifiers::CTRL,
        );
    }
    match app.drag {
        Drag::Marquee { start, current, .. } => {
            let d = current - start;
            assert!((d.x.abs() - d.y.abs()).abs() < 1e-9, "{d:?}");
        }
        ref other => panic!("{other:?}"),
    }
    let end = from + egui::vec2(60.0, 20.0);
    frame(
        &ctx,
        &mut app,
        vec![button(end, false, PointerButton::Primary, Modifiers::CTRL)],
        Modifiers::CTRL,
    );
}

#[test]
fn selection_groups_store_recall_add_and_zoom() {
    let (ctx, mut app, id) = setup();
    let none = Modifiers::NONE;
    let press = |app: &mut App, k: egui::Key, m: Modifiers| frame(&ctx, app, vec![key(k, m)], m);
    // Ctrl+3 stores the selection; 3 selects it again.
    press(&mut app, egui::Key::Num3, Modifiers::COMMAND);
    assert_eq!(app.doc().selection_groups.get(&3), Some(&vec![id]));
    app.select(Vec::new());
    press(&mut app, egui::Key::Num3, none);
    assert_eq!(app.selection, vec![id]);
    // 3 again zooms to it.
    let zoom = app.view.zoom;
    press(&mut app, egui::Key::Num3, none);
    assert!(
        (app.view.zoom - zoom).abs() > 1e-6,
        "{zoom} {}",
        app.view.zoom
    );
    // Alt+3 adds another object to the group.
    let other = app
        .new_shape(ShapeKind::Rect {
            rect: Rect::new(100.0, 100.0, 120.0, 120.0),
            radius: 0.0,
            corners: None,
        })
        .expect("rect");
    app.select(vec![other]);
    press(&mut app, egui::Key::Num3, Modifiers::ALT);
    app.select(Vec::new());
    press(&mut app, egui::Key::Num3, none);
    assert_eq!(app.selection.len(), 2);
    // A deleted object drops out; an unused number does nothing.
    app.select(vec![other]);
    app.delete_selection();
    press(&mut app, egui::Key::Num3, none);
    assert_eq!(app.selection, vec![id]);
    press(&mut app, egui::Key::Num7, none);
    assert_eq!(app.selection, vec![id]);
}

#[test]
fn ctrl_keeps_the_freehand_selection_a_rectangle() {
    let (ctx, mut app, _id) = setup();
    app.set_tool(Tool::FreeformPick);
    app.select(Vec::new());
    let ctrl = Modifiers::CTRL;
    let from = app.view.to_screen(Point::new(150.0, 250.0));
    frame(&ctx, &mut app, vec![Event::PointerMoved(from)], ctrl);
    frame(
        &ctx,
        &mut app,
        vec![button(from, true, PointerButton::Primary, ctrl)],
        ctrl,
    );
    for i in 1..=4 {
        let p = from
            + egui::vec2(50.0, 30.0) * (i as f32 / 4.0)
            + egui::vec2(0.0, (i % 2) as f32 * 7.0);
        frame(&ctx, &mut app, vec![Event::PointerMoved(p)], ctrl);
    }
    match &app.drag {
        Drag::Lasso { points, .. } => {
            assert_eq!(points.len(), 4, "{points:?}");
            assert!(
                (points[0].y - points[1].y).abs() < 1e-9
                    && (points[1].x - points[2].x).abs() < 1e-9
            );
        }
        other => panic!("{other:?}"),
    }
    let end = from + egui::vec2(50.0, 30.0);
    frame(
        &ctx,
        &mut app,
        vec![button(end, false, PointerButton::Primary, ctrl)],
        ctrl,
    );
}

#[test]
fn node_tracking_drags_a_curves_node_with_the_pick_tool() {
    let (ctx, mut app, id) = setup();
    app.convert_to_curves();
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    // The rectangle's corner node at (60, 40), dragged 10 mm right.
    let corner = app.view.to_screen(Point::new(60.0, 40.0));
    let to = app.view.to_screen(Point::new(70.0, 40.0));
    // Off (the default): the whole object moves.
    assert!(!app.settings.node_tracking);
    let before = bounds(&app, id);
    drag(&ctx, &mut app, corner, to, Modifiers::NONE, Vec::new());
    let after = bounds(&app, id);
    assert!(close(after.width(), before.width()), "{before:?} {after:?}");
    assert!(close(after.x0 - before.x0, 10.0), "{before:?} {after:?}");
    app.undo();
    // On: only that node moves; the object widens on its right side.
    app.settings.node_tracking = true;
    app.select(vec![id]);
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    let before = bounds(&app, id);
    drag(&ctx, &mut app, corner, to, Modifiers::NONE, Vec::new());
    let after = bounds(&app, id);
    assert!(close(after.x0, before.x0), "{before:?} {after:?}");
    assert!(close(after.x1, before.x1 + 10.0), "{before:?} {after:?}");
    assert_eq!(app.tool, Tool::Pick);
    // One undo step puts the node back.
    app.undo();
    assert!(close(bounds(&app, id).x1, before.x1));
}

#[test]
fn bspline_clicks_place_control_points_of_a_smooth_curve() {
    let (ctx, mut app, _) = setup();
    app.select(Vec::new());
    app.set_tool(Tool::BSpline);
    let pts = [
        Point::new(100.0, 150.0),
        Point::new(130.0, 190.0),
        Point::new(180.0, 180.0),
        Point::new(190.0, 140.0),
    ];
    for p in pts {
        let at = app.view.to_screen(p);
        click_at(&ctx, &mut app, at, Modifiers::NONE);
    }
    app.finish_curve();
    let path = last_path(&app);
    let segs: Vec<_> = path.segments().collect();
    // Four control points: one cubic from the first to the last point,
    // the middle two as its handles (not four corners joined by lines).
    assert_eq!(segs.len(), 1, "{segs:?}");
    let tracedraw_core::geometry::PathSeg::Cubic(c) = segs[0] else {
        panic!("a cubic: {segs:?}");
    };
    let near = |a: Point, b: Point| (a - b).hypot() < 0.5;
    assert!(near(c.p0, pts[0]) && near(c.p3, pts[3]), "{c:?}");
    assert!(near(c.p1, pts[1]) && near(c.p2, pts[2]), "{c:?}");
}

/// A B-spline keeps its control points: the Shape tool shows them
/// instead of the curve's nodes, dragging one reshapes the spline, a
/// double-click on the control polygon adds one and a double-click on a
/// control point removes it. Any other edit of the outline leaves an
/// ordinary curve.
#[test]
fn a_bspline_keeps_its_control_points_for_the_shape_tool() {
    use tracedraw_core::geometry::bspline_path;
    let (ctx, mut app, _) = setup();
    app.select(Vec::new());
    app.set_tool(Tool::BSpline);
    let pts = [
        Point::new(100.0, 150.0),
        Point::new(130.0, 190.0),
        Point::new(180.0, 180.0),
        Point::new(190.0, 140.0),
    ];
    for p in pts {
        let at = app.view.to_screen(p);
        click_at(&ctx, &mut app, at, Modifiers::NONE);
    }
    app.finish_curve();
    let id = app.selection[0];
    let spline = |app: &App| app.doc().find_shape(id).expect("shape").spline.clone();
    let near = |a: &[Point], b: &[Point]| {
        a.len() == b.len() && a.iter().zip(b).all(|(p, q)| (*p - *q).hypot() < 0.5)
    };
    assert!(
        near(&spline(&app).expect("spline"), &pts),
        "{:?}",
        spline(&app)
    );
    // One undo step for the whole curve.
    app.undo();
    assert!(app.doc().find_shape(id).is_none());
    app.redo();
    assert!(
        near(&spline(&app).expect("spline"), &pts),
        "{:?}",
        spline(&app)
    );

    // The Shape tool: the control points are the nodes shown; a drag of
    // the second one reshapes the spline.
    app.select(vec![id]);
    app.set_tool(Tool::Shape);
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    assert!(app.node_at(pts[1]).is_none());
    assert!(matches!(
        app.kind_node_at(pts[1]),
        Some((_, crate::kind_nodes::KindNode::Spline { index: 1 }))
    ));
    let moved = Point::new(130.0, 210.0);
    let (from, to) = (app.view.to_screen(pts[1]), app.view.to_screen(moved));
    drag(&ctx, &mut app, from, to, Modifiers::NONE, Vec::new());
    let mut now = pts.to_vec();
    now[1] = moved;
    assert!(
        near(&spline(&app).expect("spline"), &now),
        "{:?}",
        spline(&app)
    );
    assert_eq!(
        last_path(&app),
        bspline_path(&spline(&app).expect("spline"), false)
    );
    assert_eq!(app.engine.undo_label(), Some("Edit B-Spline"));

    // A double-click on the control polygon (between the third and the
    // fourth point) adds a control point there.
    let double = |ctx: &egui::Context, app: &mut App, at: Pos2| {
        // Well past the triple-click time since the last click, so these
        // two clicks make a double-click and not a triple.
        for _ in 0..40 {
            frame(ctx, app, Vec::new(), Modifiers::NONE);
        }
        frame(ctx, app, vec![Event::PointerMoved(at)], Modifiers::NONE);
        for _ in 0..2 {
            frame(
                ctx,
                app,
                vec![button(at, true, PointerButton::Primary, Modifiers::NONE)],
                Modifiers::NONE,
            );
            frame(
                ctx,
                app,
                vec![button(at, false, PointerButton::Primary, Modifiers::NONE)],
                Modifiers::NONE,
            );
        }
        for _ in 0..20 {
            frame(ctx, app, Vec::new(), Modifiers::NONE);
        }
    };
    let mid = now[2].midpoint(now[3]);
    let at = app.view.to_screen(mid);
    double(&ctx, &mut app, at);
    let five = spline(&app).expect("spline");
    assert_eq!(five.len(), 5, "{five:?}");
    assert!((five[3] - mid).hypot() < 0.5, "{five:?}");
    // A double-click on a control point removes it.
    let at = app.view.to_screen(five[3]);
    double(&ctx, &mut app, at);
    assert!(
        near(&spline(&app).expect("spline"), &now),
        "{:?}",
        spline(&app)
    );

    // Convert To Curves: the outline stays, the control points go, and
    // the Shape tool shows the curve's nodes.
    let outline = last_path(&app);
    app.select(vec![id]);
    assert!(app.can_convert_to_curves());
    app.convert_to_curves();
    assert_eq!(spline(&app), None);
    assert_eq!(last_path(&app), outline);
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    assert!(app.node_at(pts[0]).is_some());
}

/// Typing while a field has the keyboard (the font search field of the
/// property bar) goes to the field, not to the text being edited (#342).
#[test]
fn typing_in_a_focused_field_leaves_the_edited_text_alone() {
    let (ctx, mut app, _) = setup();
    app.select(Vec::new());
    app.settings.autocorrect.enabled = false;
    app.start_text(Point::new(30.0, 50.0), None);
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    // Typed on the canvas: into the text.
    frame(
        &ctx,
        &mut app,
        vec![Event::Text("ab".into())],
        Modifiers::NONE,
    );
    assert_eq!(app.edit_text(), "ab");
    // A field takes the keyboard (as a click in the font search field
    // does): what is typed goes there.
    let field = egui::Id::new("some_field");
    ctx.memory_mut(|m| m.request_focus(field));
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    ctx.memory_mut(|m| m.request_focus(field));
    frame(
        &ctx,
        &mut app,
        vec![Event::Text("cd".into())],
        Modifiers::NONE,
    );
    assert_eq!(app.edit_text(), "ab");
    assert!(app.text_edit.is_some());
    // The keyboard back on the canvas: typing edits the text again.
    ctx.memory_mut(|m| m.surrender_focus(field));
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    frame(
        &ctx,
        &mut app,
        vec![Event::Text("e".into())],
        Modifiers::NONE,
    );
    assert_eq!(app.edit_text(), "abe");
}

/// With alignment guides on (and every other snapping off), a move of the
/// rectangle near another one lands its top edge on the other's top and
/// shows the line while the button is down; a scale handle lines up the
/// same way.
#[test]
fn alignment_guides_line_up_a_moved_object_and_show_the_line() {
    let (ctx, mut app, id) = setup();
    let other = app
        .new_shape(ShapeKind::Rect {
            rect: Rect::new(100.0, 60.0, 130.0, 90.0),
            radius: 0.0,
            corners: None,
        })
        .expect("rect");
    app.select(vec![id]);
    app.snap.objects = false;
    app.snap.page = false;
    app.snap.guides = false;
    app.snap.alignment_guides = true;
    // The radius is 10 px; the top edge goes 1 mm past y = 90.
    let from = app.view.to_screen(Point::new(35.0, 25.0));
    let to = app.view.to_screen(Point::new(35.0, 25.0 + 51.0));
    let none = Modifiers::NONE;
    frame(&ctx, &mut app, vec![Event::PointerMoved(from)], none);
    frame(
        &ctx,
        &mut app,
        vec![button(from, true, PointerButton::Primary, none)],
        none,
    );
    for i in 1..=4 {
        let p = from + (to - from) * (i as f32 / 4.0);
        frame(&ctx, &mut app, vec![Event::PointerMoved(p)], none);
    }
    {
        let o = app.live_guides.borrow();
        assert!(
            o.lines
                .iter()
                .any(|l| (l.a.y - 90.0).abs() < 1e-9 && (l.b.y - 90.0).abs() < 1e-9),
            "{o:?}"
        );
    }
    frame(
        &ctx,
        &mut app,
        vec![button(to, false, PointerButton::Primary, none)],
        none,
    );
    frame(&ctx, &mut app, Vec::new(), none);
    let b = bounds(&app, id);
    assert!((b.y1 - 90.0).abs() < 1e-6, "{b:?}");
    assert!(close(b.x0, 10.0), "{b:?}");
    // The guides go when the button is up.
    assert!(app.live_guides.borrow().is_empty());
    // A right side handle dragged near the other's left edge stops on it.
    let h = handle(&app, Handle::E);
    let target = app.view.to_screen(Point::new(99.0, b.center().y));
    drag(
        &ctx,
        &mut app,
        h,
        egui::pos2(target.x, h.y),
        none,
        Vec::new(),
    );
    let b = bounds(&app, id);
    assert!((b.x1 - 100.0).abs() < 1e-6, "{b:?}");
    // With snapping off (Alt+Q) the same move goes where the pointer goes.
    app.snap.off = true;
    let from = app.view.to_screen(b.center());
    let to = app.view.to_screen(b.center() + Vec2::new(0.0, -5.0));
    drag(&ctx, &mut app, from, to, none, Vec::new());
    let after = bounds(&app, id);
    assert!((after.y1 - 85.0).abs() < 0.2, "{after:?}");
    let _ = other;
}

/// With alignment guides on, a rectangle drawn near the size of another
/// object takes its width and height.
#[test]
fn a_rectangle_drawn_with_alignment_guides_takes_another_objects_size() {
    let (ctx, mut app, id) = setup();
    app.snap.objects = false;
    app.snap.page = false;
    app.snap.guides = false;
    app.snap.alignment_guides = true;
    app.settings.snap.align.page = false;
    app.set_tool(Tool::Rectangle);
    // The other rectangle is 50 x 30 mm; this one is dragged 51 x 31.
    let from = app.view.to_screen(Point::new(100.0, 100.0));
    let to = app.view.to_screen(Point::new(151.0, 131.0));
    drag(&ctx, &mut app, from, to, Modifiers::NONE, Vec::new());
    let new = app.selection.first().copied().expect("a new rectangle");
    assert_ne!(new, id);
    let b = bounds(&app, new);
    assert!((b.width() - 50.0).abs() < 1e-6, "{b:?}");
    assert!((b.height() - 30.0).abs() < 1e-6, "{b:?}");
    // Without the option it keeps the height dragged.
    app.settings.snap.align.equal_size = false;
    let from = app.view.to_screen(Point::new(100.0, 200.0));
    // (Far from the first one's edges, which it would line up with.)
    let to = app.view.to_screen(Point::new(157.0, 231.0));
    drag(&ctx, &mut app, from, to, Modifiers::NONE, Vec::new());
    let b = bounds(&app, app.selection.first().copied().expect("another"));
    assert!((b.height() - 31.0).abs() < 0.1, "{b:?}");
}

/// A curve tool's drag from `from` to `to`, node placed at the press.
fn pull(ctx: &egui::Context, app: &mut App, from: Point, to: Point) {
    let (a, b) = (app.view.to_screen(from), app.view.to_screen(to));
    drag(ctx, app, a, b, Modifiers::NONE, Vec::new());
    for _ in 0..20 {
        frame(ctx, app, Vec::new(), Modifiers::NONE);
    }
}

fn click_page(ctx: &egui::Context, app: &mut App, p: Point) {
    let at = app.view.to_screen(p);
    click_at(ctx, app, at, Modifiers::NONE);
}

fn cubic(seg: tracedraw_core::geometry::PathSeg) -> tracedraw_core::geometry::CubicBez {
    match seg {
        tracedraw_core::geometry::PathSeg::Cubic(c) => c,
        other => panic!("a cubic: {other:?}"),
    }
}

fn near(a: Point, b: Point) -> bool {
    (a - b).hypot() < 0.6
}

/// A click after a pulled node gives the segment the preview showed: a
/// curve leaving along the pulled handle and arriving straight at the
/// new node, which has no handle of its own (#314).
#[test]
fn a_pen_click_after_a_pulled_node_follows_its_handle() {
    let (ctx, mut app, _) = setup();
    app.select(Vec::new());
    app.set_tool(Tool::Pen);
    let (a, b, c) = (
        Point::new(100.0, 150.0),
        Point::new(140.0, 150.0),
        Point::new(180.0, 150.0),
    );
    click_page(&ctx, &mut app, a);
    pull(&ctx, &mut app, b, Point::new(150.0, 170.0));
    click_page(&ctx, &mut app, c);
    app.finish_curve();
    let segs: Vec<_> = last_path(&app).segments().collect();
    assert_eq!(segs.len(), 2, "{segs:?}");
    // Into the pulled node: a curve arriving along its mirrored handle.
    let s0 = cubic(segs[0]);
    assert!(near(s0.p2, Point::new(130.0, 130.0)), "{s0:?}");
    // Out of it after a plain click: a curve leaving along the pulled
    // handle, arriving straight at the clicked node.
    let s1 = cubic(segs[1]);
    assert!(near(s1.p1, Point::new(150.0, 170.0)), "{s1:?}");
    assert!(near(s1.p2, c) && near(s1.p3, c), "{s1:?}");
    // The next segment leaves the clicked node straight.
    app.set_tool(Tool::Pen);
    app.select(Vec::new());
    click_page(&ctx, &mut app, a);
    pull(&ctx, &mut app, b, Point::new(150.0, 170.0));
    click_page(&ctx, &mut app, c);
    click_page(&ctx, &mut app, Point::new(220.0, 150.0));
    app.finish_curve();
    let segs: Vec<_> = last_path(&app).segments().collect();
    assert_eq!(segs.len(), 3, "{segs:?}");
    assert!(
        matches!(segs[2], tracedraw_core::geometry::PathSeg::Line(_)),
        "{segs:?}"
    );
}

/// A 3-point curve whose chord starts on an end node of the selected
/// curve continues it, and one that ends on the other end node closes
/// it (the owner's report: the second curve left the object open).
#[test]
fn a_three_point_curve_continues_and_closes_the_selected_curve() {
    let (ctx, mut app, first) = setup();
    app.select(vec![first]);
    app.delete_selection();
    app.set_tool(Tool::ThreePointCurve);
    let before = shape_count(&app);
    let (a, b) = (Point::new(100.0, 150.0), Point::new(160.0, 150.0));
    // The chord, then the point the curve passes through.
    pull(&ctx, &mut app, a, b);
    click_page(&ctx, &mut app, Point::new(130.0, 170.0));
    assert_eq!(shape_count(&app), before + 1);
    let id = app.selection[0];
    assert_eq!(last_path(&app).segments().count(), 1);
    // A second curve from the end node back to the start node: the same
    // object, closed, with two segments.
    pull(&ctx, &mut app, b, a);
    click_page(&ctx, &mut app, Point::new(130.0, 130.0));
    assert_eq!(shape_count(&app), before + 1, "no new object");
    assert_eq!(app.selection, vec![id]);
    let p = last_path(&app);
    assert_eq!(p.segments().count(), 2, "{p:?}");
    assert!(
        p.elements()
            .iter()
            .any(|e| matches!(e, tracedraw_core::geometry::PathEl::ClosePath)),
        "closed: {p:?}"
    );
    // A chord away from the curve: a curve of its own.
    pull(
        &ctx,
        &mut app,
        Point::new(200.0, 200.0),
        Point::new(240.0, 200.0),
    );
    click_page(&ctx, &mut app, Point::new(220.0, 220.0));
    assert_eq!(shape_count(&app), before + 2);
}

#[test]
fn clicking_the_last_node_again_makes_it_a_cusp() {
    let (ctx, mut app, _) = setup();
    app.select(Vec::new());
    app.set_tool(Tool::Pen);
    let (a, b, c) = (
        Point::new(100.0, 150.0),
        Point::new(140.0, 150.0),
        Point::new(180.0, 150.0),
    );
    click_page(&ctx, &mut app, a);
    pull(&ctx, &mut app, b, Point::new(150.0, 170.0));
    // The pointer over the last node shows that a click edits it.
    let over = app.view.to_screen(b);
    frame(
        &ctx,
        &mut app,
        vec![Event::PointerMoved(over)],
        Modifiers::NONE,
    );
    assert_eq!(
        for_tool(&app, false, false, false, Some(over)),
        ToolCursor::PenNib(Some(crate::ui::cursors::CurveMark::EditNode))
    );
    click_page(&ctx, &mut app, b);
    assert_eq!(app.curve.as_ref().map(|c| c.nodes.len()), Some(2));
    // The next node is pulled: the segment leaves the cusp without a
    // handle and arrives along the new one; the segment before keeps its
    // curve into the cusp.
    pull(&ctx, &mut app, c, Point::new(190.0, 130.0));
    app.finish_curve();
    let segs: Vec<_> = last_path(&app).segments().collect();
    assert_eq!(segs.len(), 2, "{segs:?}");
    let s0 = cubic(segs[0]);
    assert!(near(s0.p2, Point::new(130.0, 130.0)), "{s0:?}");
    let s1 = cubic(segs[1]);
    assert!(near(s1.p1, b), "{s1:?}");
    assert!(near(s1.p2, Point::new(170.0, 170.0)), "{s1:?}");
}

#[test]
fn dragging_from_the_first_node_shapes_only_the_closing_segment() {
    let (ctx, mut app, _) = setup();
    app.select(Vec::new());
    app.set_tool(Tool::Pen);
    let a = Point::new(100.0, 150.0);
    let a_out = Point::new(110.0, 170.0);
    pull(&ctx, &mut app, a, a_out);
    pull(
        &ctx,
        &mut app,
        Point::new(150.0, 190.0),
        Point::new(170.0, 190.0),
    );
    click_page(&ctx, &mut app, Point::new(160.0, 130.0));
    // The pointer over the first node shows a close.
    let over = app.view.to_screen(a);
    assert_eq!(
        for_tool(&app, false, false, false, Some(over)),
        ToolCursor::PenNib(Some(crate::ui::cursors::CurveMark::Close))
    );
    pull(&ctx, &mut app, a, Point::new(90.0, 140.0));
    assert!(app.curve.is_none());
    let path = last_path(&app);
    let segs: Vec<_> = path.segments().collect();
    // The first node keeps its own outgoing handle.
    let s0 = cubic(segs[0]);
    assert!(near(s0.p1, a_out), "{s0:?}");
    // The closing segment arrives at the first node along the drag: its
    // handle at the first node is where the pointer was let go (#358).
    let last = segs
        .iter()
        .rev()
        .find_map(|s| match s {
            tracedraw_core::geometry::PathSeg::Cubic(c) if near(c.p3, a) => Some(*c),
            _ => None,
        })
        .expect("a curved closing segment");
    assert!(near(last.p2, Point::new(90.0, 140.0)), "{last:?}");
    assert!(near(last.p1, Point::new(160.0, 130.0)), "{last:?}");
}

/// A click on the first node closes the curve with the segment the
/// preview showed: it leaves the last node along its handle and arrives
/// at the first with no handle (#359). Two curved nodes close into a
/// leaf (#356).
#[test]
fn a_click_on_the_first_node_keeps_the_last_handle_and_two_nodes_close() {
    let (ctx, mut app, _) = setup();
    app.select(Vec::new());
    app.set_tool(Tool::Pen);
    let a = Point::new(100.0, 150.0);
    pull(&ctx, &mut app, a, Point::new(110.0, 170.0));
    let b = Point::new(160.0, 150.0);
    let b_out = Point::new(170.0, 130.0);
    pull(&ctx, &mut app, b, b_out);
    // Two nodes: the pointer over the first one shows a close.
    let over = app.view.to_screen(a);
    assert_eq!(
        for_tool(&app, false, false, false, Some(over)),
        ToolCursor::PenNib(Some(crate::ui::cursors::CurveMark::Close))
    );
    click_page(&ctx, &mut app, a);
    assert!(app.curve.is_none(), "closed");
    let path = last_path(&app);
    assert!(
        path.elements()
            .iter()
            .any(|e| matches!(e, tracedraw_core::geometry::PathEl::ClosePath)),
        "{path:?}"
    );
    assert_eq!(tracedraw_core::nodes::nodes(&path).len(), 2, "{path:?}");
    let closing = path
        .segments()
        .filter_map(|s| match s {
            tracedraw_core::geometry::PathSeg::Cubic(c) if near(c.p3, a) => Some(c),
            _ => None,
        })
        .last()
        .expect("a curved closing segment");
    // It leaves the last node along its handle and arrives with none.
    assert!(
        near(closing.p0, b) && near(closing.p1, b_out),
        "{closing:?}"
    );
    assert!(near(closing.p2, a), "{closing:?}");
    // A polyline still needs three nodes to close.
    app.select(Vec::new());
    app.set_tool(Tool::Polyline);
    click_page(&ctx, &mut app, Point::new(20.0, 200.0));
    click_page(&ctx, &mut app, Point::new(60.0, 200.0));
    let over = app.view.to_screen(Point::new(20.0, 200.0));
    assert_ne!(
        for_tool(&app, false, false, false, Some(over)),
        ToolCursor::CurveBadge(Tool::Polyline, crate::ui::cursors::CurveMark::Close)
    );
}

/// A curve from (100, 100) through (150, 100) to (200, 100), selected,
/// with the Shape tool.
fn line_curve(ctx: &egui::Context, app: &mut App) -> ShapeId {
    let mut path = tracedraw_core::BezPath::new();
    path.move_to((100.0, 100.0));
    path.line_to((150.0, 100.0));
    path.line_to((200.0, 100.0));
    let id = app
        .new_shape(ShapeKind::Path {
            path,
            closed: false,
        })
        .expect("curve");
    app.select(vec![id]);
    for _ in 0..2 {
        frame(ctx, app, Vec::new(), Modifiers::NONE);
    }
    id
}

fn node_count(app: &App, id: ShapeId) -> usize {
    match &app.doc().find_shape(id).expect("curve").kind {
        ShapeKind::Path { path, .. } => tracedraw_core::nodes::nodes(path).len(),
        _ => 0,
    }
}

#[test]
fn the_pen_adds_a_node_on_a_segment_and_drags_a_node_of_the_selected_curve() {
    let (ctx, mut app, _) = setup();
    let id = line_curve(&ctx, &mut app);
    app.set_tool(Tool::Pen);
    use crate::ui::cursors::CurveMark;
    let on_segment = app.view.to_screen(Point::new(125.0, 100.0));
    let on_node = app.view.to_screen(Point::new(150.0, 100.0));
    assert_eq!(
        for_tool(&app, false, false, false, Some(on_segment)),
        ToolCursor::PenNib(Some(CurveMark::AddNode))
    );
    // With Auto add/delete (the default) a click on the node deletes it.
    assert_eq!(
        for_tool(&app, false, false, false, Some(on_node)),
        ToolCursor::PenNib(Some(CurveMark::DeleteNode))
    );
    click_at(&ctx, &mut app, on_segment, Modifiers::NONE);
    assert_eq!(node_count(&app, id), 4);
    assert!(app.curve.is_none(), "no new curve was started");
    // Dragging the middle node moves it.
    let to = app.view.to_screen(Point::new(150.0, 130.0));
    drag(&ctx, &mut app, on_node, to, Modifiers::NONE, Vec::new());
    assert!(!app.pen_editing);
    let path = match &app.doc().find_shape(id).expect("curve").kind {
        ShapeKind::Path { path, .. } => path.clone(),
        _ => unreachable!(),
    };
    assert!(
        tracedraw_core::nodes::nodes(&path)
            .iter()
            .any(|n| near(n.pos, Point::new(150.0, 130.0))),
        "{path:?}"
    );
}

#[test]
fn nodes_are_taken_within_the_pick_distance_and_a_near_miss_keeps_the_selection() {
    let (ctx, mut app, _) = setup();
    let id = line_curve(&ctx, &mut app);
    app.set_tool(Tool::Shape);
    // A press 4 screen pixels off the middle node takes it.
    let node = app.view.to_screen(Point::new(150.0, 100.0));
    let from = node + egui::vec2(3.0, 2.5);
    let to = app.view.to_screen(Point::new(150.0, 120.0));
    drag(&ctx, &mut app, from, to, Modifiers::NONE, Vec::new());
    let moved = match &app.doc().find_shape(id).expect("curve").kind {
        ShapeKind::Path { path, .. } => tracedraw_core::nodes::nodes(path)
            .iter()
            .any(|n| (n.pos.y - 100.0).abs() > 5.0),
        _ => false,
    };
    assert!(moved, "the node moved");
    let _ = app.undo();
    // A click 10 pixels from the end node and the segment: a near miss,
    // the curve stays selected.
    let end = app.view.to_screen(Point::new(200.0, 100.0));
    let miss = end + egui::vec2(10.0, 0.0);
    click_at(&ctx, &mut app, miss, Modifiers::NONE);
    assert_eq!(app.selection, vec![id]);
    // Clearly on empty canvas: the selection goes.
    let far = end + egui::vec2(120.0, 120.0);
    click_at(&ctx, &mut app, far, Modifiers::NONE);
    assert!(app.selection.is_empty());
}

#[test]
fn the_shape_tool_says_a_rectangle_is_still_a_rectangle() {
    let (ctx, mut app, id) = setup();
    app.set_tool(Tool::Shape);
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    assert_eq!(
        crate::ui::status::object_info(&app),
        crate::i18n::tr("status.shape_rectangle")
    );
    app.select(vec![id]);
    app.convert_to_curves();
    let info = crate::ui::status::object_info(&app);
    assert!(info.contains('4'), "{info}");
    assert_ne!(info, crate::i18n::tr("status.shape_rectangle"));
}

#[test]
fn rounded_corners_keep_their_size_when_relative_scaling_is_off() {
    let (ctx, mut app, id) = setup();
    for fixed in [true, false] {
        let mut c = tracedraw_core::Corners::uniform(5.0, tracedraw_core::CornerKind::Round);
        c.fixed = fixed;
        let _ = app.engine.run(&tracedraw_core::Command::SetShapeKind {
            shape: id,
            kind: ShapeKind::rect_with_corners(Rect::new(10.0, 10.0, 60.0, 40.0), c),
        });
        let _ = app.engine.run(&tracedraw_core::Command::TransformShapes {
            shapes: vec![id],
            transform: Affine::IDENTITY,
        });
        app.select(vec![id]);
        app.set_tool(Tool::Pick);
        frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
        // Twice as wide and high with the corner handle.
        let from = handle(&app, Handle::NE);
        let b = bounds(&app, id);
        let target = Point::new(b.x0 + 2.0 * b.width(), b.y0 + 2.0 * b.height());
        let to = app.view.to_screen(target);
        drag(&ctx, &mut app, from, to, Modifiers::NONE, Vec::new());
        let r = app
            .doc()
            .find_shape(id)
            .and_then(|s| s.page_corners())
            .expect("corners")
            .radii[0];
        if fixed {
            assert!(close(r, 5.0), "fixed: {r}");
        } else {
            assert!(r > 8.0, "relative: {r}");
        }
        let _ = app.undo();
    }
}

/// A rectangle and a line of text over it, the rectangle filled or not
/// and below or above the text.
fn text_and_frame(app: &mut App, filled: bool, frame_on_top: bool) -> (ShapeId, ShapeId) {
    use tracedraw_core::{Color, Command, Fill, TextSpan};
    let rect = |app: &mut App| {
        let id = app
            .new_shape(ShapeKind::Rect {
                rect: Rect::new(10.0, 10.0, 150.0, 80.0),
                radius: 0.0,
                corners: None,
            })
            .expect("rect");
        app.run(Command::SetFill {
            shapes: vec![id],
            fill: if filled {
                Fill::Solid(Color::rgb8(240, 240, 200))
            } else {
                Fill::None
            },
        });
        id
    };
    let text = |app: &mut App| {
        app.new_shape(ShapeKind::Text {
            spans: vec![TextSpan::new("Invitation", "Sans", 36.0)],
            origin: Point::new(30.0, 50.0),
            frame: None,
            align: tracedraw_core::document::TextAlign::Left,
            para: Default::default(),
            on_path: None,
        })
        .expect("text")
    };
    if frame_on_top {
        let t = text(app);
        (rect(app), t)
    } else {
        let r = rect(app);
        (r, text(app))
    }
}

/// The middle of the text object, in page space.
fn text_middle(app: &App, id: ShapeId) -> Point {
    app.doc().shape_on_page(id).expect("text").bounds().center()
}

/// Write the drawing as `.cdr`, read it back and open it.
fn through_cdr(app: &App) -> App {
    let bytes = tracedraw_cdr::write::document_to_cdr(app.doc());
    let (doc, _) = tracedraw_cdr::open_bytes(&bytes, "t").expect("read back");
    let mut back = App::headless();
    back.open_document(doc, None);
    back.view.zoom = 1.0;
    back
}

/// The ids of the objects on the first layer, bottom to top, by kind.
fn kinds_bottom_up(app: &App) -> Vec<&'static str> {
    app.doc().pages[0]
        .layers
        .iter()
        .flat_map(|l| &l.shapes)
        .map(|s| match s.kind {
            ShapeKind::Text { .. } => "text",
            ShapeKind::ClipFrame { .. } => "clip",
            ShapeKind::Group { .. } => "group",
            _ => "shape",
        })
        .collect()
}

fn first_of(app: &App, kind: &str) -> ShapeId {
    let all: Vec<_> = app.doc().pages[0]
        .layers
        .iter()
        .flat_map(|l| &l.shapes)
        .collect();
    all.iter()
        .zip(kinds_bottom_up(app))
        .find(|(_, k)| *k == kind)
        .map(|(s, _)| s.id)
        .expect("kind")
}

#[test]
fn text_over_a_rectangle_read_from_cdr_is_hit_first() {
    let mut app = App::headless();
    app.view.zoom = 1.0;
    let (rect, text) = text_and_frame(&mut app, true, false);
    let p = text_middle(&app, text);
    assert_eq!(app.hit_test(p), Some(text));
    assert_eq!(app.hits_at(p, false), vec![text, rect]);
    // The file keeps the stacking order: the text is still on top.
    let back = through_cdr(&app);
    assert_eq!(kinds_bottom_up(&back), ["shape", "text"]);
    let t = first_of(&back, "text");
    assert_eq!(back.hit_test(text_middle(&back, t)), Some(t));
}

#[test]
fn an_unfilled_rectangle_over_text_takes_the_click_and_alt_reaches_the_text() {
    let (ctx, mut app, first) = setup();
    app.select(vec![first]);
    app.delete_selection();
    let (rect, text) = text_and_frame(&mut app, false, true);
    let at = app.view.to_screen(text_middle(&app, text));
    // Treated as filled (the default), the empty rectangle on top is hit
    // inside; Alt digs to the text under it.
    click_at(&ctx, &mut app, at, Modifiers::NONE);
    assert_eq!(app.selection, vec![rect]);
    click_at(&ctx, &mut app, at, Modifiers::ALT);
    assert_eq!(app.selection, vec![text]);
    // Not treated as filled, the text is hit straight away.
    app.settings.treat_all_filled = false;
    app.select(Vec::new());
    click_at(&ctx, &mut app, at, Modifiers::NONE);
    assert_eq!(app.selection, vec![text]);
    // The same stacking after a round trip through `.cdr`.
    let back = through_cdr(&app);
    assert_eq!(kinds_bottom_up(&back), ["text", "shape"]);
}

/// Moving objects over a clip frame: dropped as they are they land over
/// it; with W held they go into it; with K held they replace its
/// contents. The frame under the pointer is noted during the move.
#[test]
fn dropping_on_a_clip_frame_with_w_or_k_puts_the_objects_in() {
    let (ctx, mut app, first) = setup();
    app.select(vec![first]);
    app.delete_selection();
    let (rect, text) = text_and_frame(&mut app, true, false);
    app.place_inside(vec![text], rect);
    let clip = app.doc().top_level_of(text).expect("clip frame");
    app.settings.clip_frame.auto_center = crate::settings::AutoCenter::Never;
    // A small square away from the frame.
    let square = |app: &mut App| {
        app.new_shape(ShapeKind::Rect {
            rect: Rect::new(200.0, 20.0, 220.0, 40.0),
            radius: 0.0,
            corners: None,
        })
        .expect("square")
    };
    let from = app.view.to_screen(Point::new(210.0, 30.0));
    let to = app.view.to_screen(Point::new(60.0, 45.0));
    let hold = |k: egui::Key| vec![key(k, Modifiers::NONE)];
    let release = |k: egui::Key| Event::Key {
        key: k,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers: Modifiers::NONE,
    };
    // No key: the square lands over the frame, on its own.
    let a = square(&mut app);
    app.select(vec![a]);
    drag(&ctx, &mut app, from, to, Modifiers::NONE, Vec::new());
    assert_eq!(app.doc().top_level_of(a), Some(a));
    assert!(app.drop_frame.is_none());
    app.select(vec![a]);
    app.delete_selection();
    // W held during the move: into the frame, which is then selected.
    let b = square(&mut app);
    app.select(vec![b]);
    drag(
        &ctx,
        &mut app,
        from,
        to,
        Modifiers::NONE,
        hold(egui::Key::W),
    );
    frame(&ctx, &mut app, vec![release(egui::Key::W)], Modifiers::NONE);
    assert_eq!(app.doc().top_level_of(b), Some(clip), "in the frame");
    assert!(
        app.doc().find_shape(text).is_some(),
        "the old contents stay"
    );
    assert_eq!(app.selection, vec![clip]);
    assert_eq!(app.tool, Tool::Pick, "W picked no tool");
    // K held: the old contents go, the square takes their place.
    let c = square(&mut app);
    app.select(vec![c]);
    let depth = app.engine.history_labels().0.len();
    drag(
        &ctx,
        &mut app,
        from,
        to,
        Modifiers::NONE,
        hold(egui::Key::K),
    );
    frame(&ctx, &mut app, vec![release(egui::Key::K)], Modifiers::NONE);
    assert_eq!(app.doc().top_level_of(c), Some(clip));
    assert!(app.doc().find_shape(text).is_none(), "replaced");
    assert!(app.doc().find_shape(b).is_none(), "replaced");
    assert_eq!(
        app.engine.history_labels().0.len(),
        depth + 2,
        "the move, then the drop"
    );
    assert_eq!(app.tool, Tool::Pick);
}

#[test]
fn ctrl_click_takes_the_contents_of_a_clip_frame() {
    let (ctx, mut app, first) = setup();
    app.select(vec![first]);
    app.delete_selection();
    let (rect, text) = text_and_frame(&mut app, true, false);
    app.place_inside(vec![text], rect);
    let clip = app.doc().top_level_of(text).expect("clip frame");
    let inside = app.view.to_screen(text_middle(&app, text));
    click_at(&ctx, &mut app, inside, Modifiers::NONE);
    assert_eq!(app.selection, vec![clip]);
    click_at(&ctx, &mut app, inside, Modifiers::CTRL);
    assert_eq!(app.selection, vec![text]);
    // Ctrl+Alt digs through the contents to the frame.
    assert_eq!(app.hits_at(text_middle(&app, text), true), vec![text, clip]);
    // Inside the frame but off the text: the frame.
    app.select(Vec::new());
    let off = app.view.to_screen(Point::new(12.0, 12.0));
    click_at(&ctx, &mut app, off, Modifiers::CTRL);
    assert_eq!(app.selection, vec![clip]);
}

#[test]
fn clipped_away_contents_are_not_hit() {
    use tracedraw_core::TextSpan;
    let mut app = App::headless();
    app.view.zoom = 1.0;
    let frame = app
        .new_shape(ShapeKind::Rect {
            rect: Rect::new(10.0, 10.0, 40.0, 80.0),
            radius: 0.0,
            corners: None,
        })
        .expect("rect");
    let text = app
        .new_shape(ShapeKind::Text {
            spans: vec![TextSpan::new("Invitation", "Sans", 36.0)],
            origin: Point::new(30.0, 50.0),
            frame: None,
            align: tracedraw_core::document::TextAlign::Left,
            para: Default::default(),
            on_path: None,
        })
        .expect("text");
    let b = app.doc().shape(text).expect("text").1.bounds();
    assert!(b.x1 > 50.0, "{b:?}");
    app.place_inside(vec![text], frame);
    let clip = app.doc().top_level_of(text).expect("clip frame");
    let y = b.center().y;
    // On the text inside the frame: Ctrl reaches it.
    assert_eq!(app.hit_test_step(Point::new(35.0, y)), Some(text));
    // Its letters past the frame do not show and are not hit.
    let past = Point::new((40.0 + b.x1) / 2.0, y);
    assert_eq!(app.hit_test(past), None);
    assert!(app.hits_at(past, true).is_empty());
    assert_eq!(app.hit_test(Point::new(35.0, y)), Some(clip));
}

#[test]
fn new_rounded_rectangles_keep_circular_corners_when_stretched() {
    use crate::ui::propbar::{edit_corners, CornerEdit};
    use tracedraw_core::geometry::Shape as _;
    let (ctx, mut app, _) = setup();
    // Relative corner scaling is off for new rectangles.
    assert!(app.rect_corners.fixed);
    app.select(Vec::new());
    edit_corners(&mut app, CornerEdit::Size(0, 5.0));
    app.set_tool(Tool::Rectangle);
    app.create_box_shape(Point::new(100.0, 100.0), Point::new(150.0, 130.0));
    let id = app.selected_shapes()[0].id;
    app.set_tool(Tool::Pick);
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    // Only twice as wide, with the side handle.
    let stretch = |app: &mut App, which: Handle, fx: f64, fy: f64| {
        let from = handle(app, which);
        let b = bounds(app, id);
        let target = Point::new(b.x0 + fx * b.width(), b.y0 + fy * b.height());
        let mut to = app.view.to_screen(target);
        match which {
            Handle::E => to.y = from.y,
            _ => to.x = from.x,
        }
        drag(&ctx, app, from, to, Modifiers::NONE, Vec::new());
    };
    stretch(&mut app, Handle::E, 2.0, 1.0);
    let s = app.doc().find_shape(id).expect("rect").clone();
    let c = s.page_corners().expect("corners");
    assert!(c.fixed && c.radii.iter().all(|r| close(*r, 5.0)), "{c:?}");
    // Circles of 5 mm: the area the four corners cut off is (4 - pi) r^2.
    let path = s.page_path();
    let b = path.bounding_box();
    assert!(b.width() > 90.0, "{b:?}");
    let cut = b.width() * b.height() - path.area().abs();
    let circles = (4.0 - std::f64::consts::PI) * 25.0;
    assert!((cut - circles).abs() < 0.2, "{cut} vs {circles}");
    // Relative scaling turned on: the corners grow with the rectangle.
    app.select(vec![id]);
    edit_corners(&mut app, CornerEdit::Scaling(true));
    let s = app.doc().find_shape(id).expect("rect").clone();
    let c = s.page_corners().expect("corners");
    assert!(!c.fixed && close(c.radii[0], 5.0), "{c:?}");
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    stretch(&mut app, Handle::N, 1.0, 2.0);
    let r = app
        .doc()
        .find_shape(id)
        .and_then(|s| s.page_corners())
        .expect("corners")
        .radii[0];
    assert!(r > 8.0, "relative: {r}");
}

#[test]
fn the_pen_deletes_a_node_of_the_selected_curve_and_the_toggle_turns_it_off() {
    use crate::ui::cursors::CurveMark;
    let (ctx, mut app, _) = setup();
    let id = line_curve(&ctx, &mut app);
    app.set_tool(Tool::Pen);
    let on_node = app.view.to_screen(Point::new(150.0, 100.0));
    let on_segment = app.view.to_screen(Point::new(125.0, 100.0));
    click_at(&ctx, &mut app, on_node, Modifiers::NONE);
    assert_eq!(node_count(&app, id), 2);
    assert!(app.curve.is_none(), "no new curve was started");
    // One undo step; a curve keeps two nodes.
    let _ = app.undo();
    assert_eq!(node_count(&app, id), 3);
    // Auto add/delete off: the node is only edited, a segment click adds
    // nothing.
    app.settings.curves.pen_auto_add_delete = false;
    assert_eq!(
        for_tool(&app, false, false, false, Some(on_node)),
        ToolCursor::PenNib(Some(CurveMark::EditNode))
    );
    assert_eq!(
        for_tool(&app, false, false, false, Some(on_segment)),
        ToolCursor::PenNib(None)
    );
    click_at(&ctx, &mut app, on_node, Modifiers::NONE);
    assert_eq!(node_count(&app, id), 3);
    // The Bezier tool adds on a segment and never deletes.
    app.settings.curves.pen_auto_add_delete = true;
    app.finish_curve();
    app.select(vec![id]);
    app.set_tool(Tool::Bezier);
    assert_eq!(
        for_tool(&app, false, false, false, Some(on_node)),
        ToolCursor::CurveBadge(Tool::Bezier, CurveMark::EditNode)
    );
    assert_eq!(
        for_tool(&app, false, false, false, Some(on_segment)),
        ToolCursor::CurveBadge(Tool::Bezier, CurveMark::AddNode)
    );
    click_at(&ctx, &mut app, on_node, Modifiers::NONE);
    assert!(node_count(&app, id) >= 3);
}

#[test]
fn the_pen_adds_and_deletes_nodes_on_the_curve_being_drawn() {
    use crate::ui::cursors::CurveMark;
    let (ctx, mut app, _) = setup();
    app.select(Vec::new());
    app.set_tool(Tool::Pen);
    for x in [100.0, 140.0, 180.0] {
        click_page(&ctx, &mut app, Point::new(x, 150.0));
    }
    assert_eq!(app.curve.as_ref().map(|c| c.nodes.len()), Some(3));
    let on_segment = app.view.to_screen(Point::new(120.0, 150.0));
    let on_node = app.view.to_screen(Point::new(140.0, 150.0));
    frame(
        &ctx,
        &mut app,
        vec![Event::PointerMoved(on_segment)],
        Modifiers::NONE,
    );
    assert_eq!(
        for_tool(&app, false, false, false, Some(on_segment)),
        ToolCursor::PenNib(Some(CurveMark::AddNode))
    );
    assert_eq!(
        for_tool(&app, false, false, false, Some(on_node)),
        ToolCursor::PenNib(Some(CurveMark::DeleteNode))
    );
    click_page(&ctx, &mut app, Point::new(120.0, 150.0));
    let xs: Vec<f64> = app
        .curve
        .as_ref()
        .map(|c| c.nodes.iter().map(|n| n.0.x.round()).collect())
        .unwrap_or_default();
    assert_eq!(xs, vec![100.0, 120.0, 140.0, 180.0]);
    click_page(&ctx, &mut app, Point::new(140.0, 150.0));
    let xs: Vec<f64> = app
        .curve
        .as_ref()
        .map(|c| c.nodes.iter().map(|n| n.0.x.round()).collect())
        .unwrap_or_default();
    assert_eq!(xs, vec![100.0, 120.0, 180.0]);
    // Off: a click there places a node.
    app.settings.curves.pen_auto_add_delete = false;
    click_page(&ctx, &mut app, Point::new(110.0, 150.0));
    assert_eq!(app.curve.as_ref().map(|c| c.nodes.len()), Some(4));
}

#[test]
fn c_held_while_pulling_makes_a_cusp_that_stays_and_does_not_align() {
    let (ctx, mut app, rect) = setup();
    // The rectangle stays selected: C must not align it.
    let before = bounds(&app, rect);
    app.set_tool(Tool::Pen);
    app.select(vec![rect]);
    let (a, b, c) = (
        Point::new(100.0, 150.0),
        Point::new(140.0, 150.0),
        Point::new(180.0, 150.0),
    );
    click_page(&ctx, &mut app, a);
    let (from, to) = (
        app.view.to_screen(b),
        app.view.to_screen(Point::new(160.0, 170.0)),
    );
    drag(
        &ctx,
        &mut app,
        from,
        to,
        Modifiers::NONE,
        vec![key(egui::Key::C, Modifiers::NONE)],
    );
    let release = Event::Key {
        key: egui::Key::C,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers: Modifiers::NONE,
    };
    frame(&ctx, &mut app, vec![release.clone()], Modifiers::NONE);
    let cv = app.curve.clone().expect("drawing");
    // The incoming handle was frozen before the pull ended: it is not the
    // mirror of the outgoing one.
    let out = cv.nodes[1].1.expect("outgoing handle");
    assert!(near(out, Point::new(160.0, 170.0)), "{out:?}");
    let frozen = cv.in_handles.get(&1).copied().expect("a cusp");
    assert!(
        frozen.is_none_or(|h| !near(h, Point::new(120.0, 130.0))),
        "{frozen:?}"
    );
    assert_eq!(bounds(&app, rect), before, "C did not align the rectangle");
    // C pressed before the press: the node is a cusp from the start (the
    // segment arrives without a handle), and it stays one after C is let
    // go.
    frame(
        &ctx,
        &mut app,
        vec![key(egui::Key::C, Modifiers::NONE)],
        Modifiers::NONE,
    );
    pull(&ctx, &mut app, c, Point::new(190.0, 130.0));
    frame(&ctx, &mut app, vec![release], Modifiers::NONE);
    let cv = app.curve.clone().expect("drawing");
    assert_eq!(cv.in_handles.get(&2).copied(), Some(None));
    assert!(cv.in_handles.contains_key(&1), "the first cusp stayed");
    // Without C the next pull is smooth.
    pull(
        &ctx,
        &mut app,
        Point::new(220.0, 150.0),
        Point::new(230.0, 160.0),
    );
    let cv = app.curve.clone().expect("drawing");
    assert!(!cv.in_handles.contains_key(&3));
    assert_eq!(bounds(&app, rect), before);
}

#[test]
fn c_during_the_closing_drag_keeps_the_first_node_handle() {
    let (ctx, mut app, _) = setup();
    app.select(Vec::new());
    app.set_tool(Tool::Pen);
    let a = Point::new(100.0, 150.0);
    let a_out = Point::new(110.0, 170.0);
    pull(&ctx, &mut app, a, a_out);
    pull(
        &ctx,
        &mut app,
        Point::new(150.0, 190.0),
        Point::new(170.0, 190.0),
    );
    click_page(&ctx, &mut app, Point::new(160.0, 130.0));
    let (from, to) = (
        app.view.to_screen(a),
        app.view.to_screen(Point::new(90.0, 140.0)),
    );
    drag(
        &ctx,
        &mut app,
        from,
        to,
        Modifiers::NONE,
        vec![key(egui::Key::C, Modifiers::NONE)],
    );
    assert!(app.curve.is_none());
    let segs: Vec<_> = last_path(&app).segments().collect();
    assert!(near(cubic(segs[0]).p1, a_out), "{segs:?}");
    let closing = segs
        .iter()
        .rev()
        .find_map(|s| match s {
            tracedraw_core::geometry::PathSeg::Cubic(c) if near(c.p3, a) => Some(*c),
            _ => None,
        })
        .expect("a curved closing segment");
    assert!(near(closing.p2, Point::new(90.0, 140.0)), "{closing:?}");
}

#[test]
fn the_snap_tip_shows_while_placing_and_pulling_with_the_pen() {
    use crate::snap_points::SnapMode;
    let (ctx, mut app, _) = setup();
    app.select(Vec::new());
    app.snap.off = false;
    app.snap.grid = true;
    app.snap.objects = false;
    app.snap.guides = false;
    app.snap.page = false;
    app.set_tool(Tool::Pen);
    click_page(&ctx, &mut app, Point::new(100.2, 150.3));
    // Placing: the pointer near a grid crossing is marked "grid".
    let near_grid = app.view.to_screen(Point::new(120.3, 140.2));
    frame(
        &ctx,
        &mut app,
        vec![Event::PointerMoved(near_grid)],
        Modifiers::NONE,
    );
    let mark = app.snap_mark.get().expect("a snap mark");
    assert_eq!(mark.mode, SnapMode::Grid);
    assert!(
        (mark.point - Point::new(120.0, 140.0)).hypot() < 1e-6,
        "{mark:?}"
    );
    // Pulling a handle: the handle snaps and the mark shows too.
    let from = app.view.to_screen(Point::new(140.0, 150.0));
    frame(
        &ctx,
        &mut app,
        vec![Event::PointerMoved(from)],
        Modifiers::NONE,
    );
    frame(
        &ctx,
        &mut app,
        vec![button(from, true, PointerButton::Primary, Modifiers::NONE)],
        Modifiers::NONE,
    );
    for p in [Point::new(150.0, 160.0), Point::new(160.3, 169.8)] {
        let at = app.view.to_screen(p);
        frame(
            &ctx,
            &mut app,
            vec![Event::PointerMoved(at)],
            Modifiers::NONE,
        );
    }
    assert!(app.curve.as_ref().is_some_and(|c| c.dragging_handle));
    let mark = app.snap_mark.get().expect("a snap mark while pulling");
    assert_eq!(mark.mode, SnapMode::Grid);
    let handle = app
        .curve
        .as_ref()
        .and_then(|c| c.nodes.last())
        .and_then(|n| n.1);
    assert!(
        handle.is_some_and(|h| (h - Point::new(160.0, 170.0)).hypot() < 1e-6),
        "{handle:?}"
    );
    let up = app.view.to_screen(Point::new(160.3, 169.8));
    frame(
        &ctx,
        &mut app,
        vec![button(up, false, PointerButton::Primary, Modifiers::NONE)],
        Modifiers::NONE,
    );
}

#[test]
fn space_finishes_a_bezier_curve_then_picks() {
    let (ctx, mut app, _) = setup();
    app.select(Vec::new());
    app.set_tool(Tool::Bezier);
    let before = shape_count(&app);
    let pts = [
        Point::new(100.0, 150.0),
        Point::new(140.0, 170.0),
        Point::new(180.0, 150.0),
    ];
    for p in pts {
        let at = app.view.to_screen(p);
        click_at(&ctx, &mut app, at, Modifiers::NONE);
    }
    assert!(app.curve.is_some());
    frame(
        &ctx,
        &mut app,
        vec![key(egui::Key::Space, Modifiers::NONE)],
        Modifiers::NONE,
    );
    assert!(app.curve.is_none(), "Space ends the curve");
    assert_eq!(shape_count(&app), before + 1);
    // Space is also the Pick tool's key: the curve is kept, then Pick.
    assert_eq!(app.tool, Tool::Pick);
    let nodes = tracedraw_core::nodes::nodes(&last_path(&app));
    assert_eq!(nodes.len(), 3, "{nodes:?}");
}

/// Esc while a curve is drawn with the Pen (or Bezier) finishes it as
/// it stands, keeping the nodes placed; the tool stays. A lone first
/// node is dropped (#314).
#[test]
fn esc_finishes_the_curve_being_drawn_and_keeps_it() {
    let (ctx, mut app, _) = setup();
    app.select(Vec::new());
    app.set_tool(Tool::Pen);
    let before = shape_count(&app);
    let pts = [
        Point::new(100.0, 150.0),
        Point::new(140.0, 170.0),
        Point::new(180.0, 150.0),
    ];
    for p in pts {
        let at = app.view.to_screen(p);
        click_at(&ctx, &mut app, at, Modifiers::NONE);
    }
    assert!(app.curve.is_some());
    let esc = || vec![key(egui::Key::Escape, Modifiers::NONE)];
    frame(&ctx, &mut app, esc(), Modifiers::NONE);
    assert!(app.curve.is_none(), "Esc ends the curve");
    assert_eq!(shape_count(&app), before + 1, "the curve is kept");
    assert_eq!(app.tool, Tool::Pen, "the tool stays");
    let nodes = tracedraw_core::nodes::nodes(&last_path(&app));
    assert_eq!(nodes.len(), 3, "{nodes:?}");
    // One node only: nothing to keep.
    app.select(Vec::new());
    let at = app.view.to_screen(Point::new(60.0, 60.0));
    click_at(&ctx, &mut app, at, Modifiers::NONE);
    assert!(app.curve.is_some());
    frame(&ctx, &mut app, esc(), Modifiers::NONE);
    assert!(app.curve.is_none());
    assert_eq!(shape_count(&app), before + 1);
}

/// With a real mouse the right button goes down and up in frames of its
/// own while the left one is held; the move goes on after it.
#[test]
fn the_move_goes_on_after_a_right_click() {
    let (ctx, mut app, id) = setup();
    let from = app.view.to_screen(Point::new(35.0, 25.0));
    let n = Modifiers::NONE;
    let right = |p: Pos2, down: bool| button(p, down, PointerButton::Secondary, n);
    frame(&ctx, &mut app, vec![Event::PointerMoved(from)], n);
    frame(
        &ctx,
        &mut app,
        vec![button(from, true, PointerButton::Primary, n)],
        n,
    );
    let mut p = from;
    for _ in 0..4 {
        p += egui::vec2(20.0, 0.0);
        frame(&ctx, &mut app, vec![Event::PointerMoved(p)], n);
    }
    frame(&ctx, &mut app, vec![right(p, true)], n);
    p += egui::vec2(5.0, 0.0);
    frame(&ctx, &mut app, vec![Event::PointerMoved(p)], n);
    frame(&ctx, &mut app, vec![right(p, false)], n);
    for _ in 0..4 {
        p += egui::vec2(20.0, 10.0);
        frame(&ctx, &mut app, vec![Event::PointerMoved(p)], n);
    }
    assert!(app.transforming(), "the move goes on");
    frame(
        &ctx,
        &mut app,
        vec![button(p, false, PointerButton::Primary, n)],
        n,
    );
    frame(&ctx, &mut app, Vec::new(), n);
    assert_eq!(shape_count(&app), 2);
    assert!(close(bounds(&app, id).x0, 10.0), "the original stays");
    let copy = app.selection.first().copied().expect("copy");
    assert_ne!(copy, id);
    let moved = app.view.to_page(p) - app.view.to_page(from);
    let b = bounds(&app, copy);
    assert!(close(b.x0, 10.0 + moved.x), "{b:?} {moved:?}");
    assert!(close(b.y0, 10.0 + moved.y), "{b:?} {moved:?}");
    assert!(!app.transforming());
}

#[test]
fn ctrl_z_takes_back_pen_nodes_one_by_one() {
    let (ctx, mut app, id) = setup();
    app.set_tool(Tool::Pen);
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    let undo_before = app.engine.undo_label();
    for q in [
        Point::new(20.0, 60.0),
        Point::new(60.0, 60.0),
        Point::new(40.0, 90.0),
    ] {
        let at = app.view.to_screen(q);
        click_at(&ctx, &mut app, at, Modifiers::NONE);
    }
    let count = |app: &App| app.curve.as_ref().map(|c| c.nodes.len());
    assert_eq!(count(&app), Some(3));
    let ctrl = Modifiers::COMMAND | Modifiers::CTRL;
    let press = |ctx: &egui::Context, app: &mut App, mods: Modifiers| {
        frame(ctx, app, vec![key(egui::Key::Z, mods)], mods);
    };
    // Ctrl+Z takes the last node back, Ctrl+Shift+Z puts it again.
    press(&ctx, &mut app, ctrl);
    assert_eq!(count(&app), Some(2));
    press(&ctx, &mut app, ctrl | Modifiers::SHIFT);
    assert_eq!(count(&app), Some(3));
    // Node by node until the curve is gone; the document is untouched.
    for want in [Some(2), Some(1), None] {
        press(&ctx, &mut app, ctrl);
        assert_eq!(count(&app), want);
    }
    assert!(app.doc().find_shape(id).is_some());
    assert_eq!(app.engine.undo_label(), undo_before);
    // Redo brings the nodes back one by one.
    press(&ctx, &mut app, ctrl | Modifiers::SHIFT);
    assert_eq!(count(&app), Some(1));
    press(&ctx, &mut app, ctrl | Modifiers::SHIFT);
    assert_eq!(count(&app), Some(2));
    // Once the curve is finished it is one step of the document's history.
    frame(
        &ctx,
        &mut app,
        vec![key(egui::Key::Enter, Modifiers::NONE)],
        Modifiers::NONE,
    );
    assert!(app.curve.is_none());
    assert_ne!(app.engine.undo_label(), undo_before);
    press(&ctx, &mut app, ctrl);
    assert!(app.curve.is_none());
    assert_eq!(app.engine.undo_label(), undo_before);
}

#[test]
fn a_right_click_on_a_locked_object_offers_to_unlock_it() {
    let (ctx, mut app, id) = setup();
    app.select(vec![id]);
    app.set_locked(true);
    assert!(app.selection.is_empty());
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    let right_click = |ctx: &egui::Context, app: &mut App, at: Pos2| {
        frame(ctx, app, vec![Event::PointerMoved(at)], Modifiers::NONE);
        frame(
            ctx,
            app,
            vec![button(at, true, PointerButton::Secondary, Modifiers::NONE)],
            Modifiers::NONE,
        );
        frame(
            ctx,
            app,
            vec![button(at, false, PointerButton::Secondary, Modifiers::NONE)],
            Modifiers::NONE,
        );
    };
    // On the locked rectangle's outline: its menu opens for it, and it is
    // not selected (so it cannot be dragged or edited).
    let on = app.view.to_screen(Point::new(35.0, 10.0));
    right_click(&ctx, &mut app, on);
    assert!(app.context_menu.is_some());
    assert_eq!(app.context_locked, Some(id));
    assert!(app.selection.is_empty());
    // Unlock Object: unlocked and selected.
    app.unlock_object(id);
    assert!(app.doc().find_shape(id).is_some_and(|s| !s.locked));
    assert_eq!(app.selection, vec![id]);
    // Away from it, or on an unlocked object, there is no locked object.
    app.context_menu = None;
    app.context_locked = None;
    right_click(&ctx, &mut app, on);
    assert_eq!(app.context_locked, None);
    let empty = app.view.to_screen(Point::new(150.0, 150.0));
    right_click(&ctx, &mut app, empty);
    assert_eq!(app.context_locked, None);
}

#[test]
fn removed_and_alt_shortcuts_act_on_the_keyboard() {
    let (ctx, mut app, id) = setup();
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    let ctrl = Modifiers::COMMAND | Modifiers::CTRL;
    let press = |ctx: &egui::Context, app: &mut App, k: egui::Key, mods: Modifiers| {
        frame(ctx, app, vec![key(k, mods)], mods);
    };
    // Undo's Ctrl+Z removed: the key does nothing, nothing is undone.
    app.settings
        .shortcuts
        .push(("menu.edit.undo".into(), String::new()));
    let last = app.engine.undo_label();
    assert!(last.is_some());
    press(&ctx, &mut app, egui::Key::Z, ctrl);
    assert_eq!(app.engine.undo_label(), last);
    assert!(app.doc().find_shape(id).is_some());
    // Given back: Ctrl+Z undoes again.
    app.settings.shortcuts.clear();
    press(&ctx, &mut app, egui::Key::Z, ctrl);
    assert_ne!(app.engine.undo_label(), last);
    // A tool takes Alt with a key as menu commands do.
    app.settings
        .shortcuts
        .push(("ellipse".into(), "Alt+Shift+K".into()));
    assert_ne!(app.tool, Tool::Ellipse);
    press(
        &ctx,
        &mut app,
        egui::Key::K,
        Modifiers::ALT | Modifiers::SHIFT,
    );
    assert_eq!(app.tool, Tool::Ellipse);
}

#[test]
fn ctrl_keeps_a_dimension_leg_to_the_constrain_angle() {
    let (ctx, mut app, _) = setup();
    app.select(Vec::new());
    app.set_tool(Tool::ParallelDimension);
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    let ctrl = Modifiers::COMMAND | Modifiers::CTRL;
    let first = Point::new(100.0, 150.0);
    let at = app.view.to_screen(first);
    click_at(&ctx, &mut app, at, Modifiers::NONE);
    assert_eq!(app.dimension_points.len(), 1);
    let a = app.dimension_points[0];
    // A second point a little off the horizontal, Ctrl held: the leg lies
    // on it.
    let off = Point::new(160.0, 154.0);
    let at = app.view.to_screen(off);
    click_at(&ctx, &mut app, at, ctrl);
    assert_eq!(app.dimension_points.len(), 2);
    let b = app.dimension_points[1];
    assert!((b.y - a.y).abs() < 1e-6, "{a:?} {b:?}");
    assert!(b.x > a.x + 50.0);
    // Without Ctrl the point is taken as it is.
    app.dimension_points = vec![a];
    let at = app.view.to_screen(off);
    click_at(&ctx, &mut app, at, Modifiers::NONE);
    let free = app.dimension_points[1];
    assert!((free.y - a.y).abs() > 2.0, "{free:?}");
    // The angular tool keeps each side from the vertex.
    app.set_tool(Tool::AngularDimension);
    app.dimension_points = vec![first];
    let near_vertical = Point::new(103.0, 200.0);
    let at = app.view.to_screen(near_vertical);
    click_at(&ctx, &mut app, at, ctrl);
    let side = app.dimension_points[1];
    assert!((side.x - first.x).abs() < 1e-6, "{side:?}");
}

#[test]
fn the_page_bar_does_nothing_on_the_welcome_screen() {
    let (ctx, mut app, _) = setup();
    app.select(Vec::new());
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    let size = |app: &App| {
        let p = app.doc().page(app.page).expect("page");
        (p.size.width, p.size.height)
    };
    let portrait = size(&app);
    assert!(portrait.1 > portrait.0, "{portrait:?}");
    // The page bar's landscape button: found along the bar's row, as
    // the first click there that turns the page.
    let mut landscape = None;
    for x in (380..560).step_by(4) {
        let at = egui::pos2(x as f32, 87.0);
        click_at(&ctx, &mut app, at, Modifiers::NONE);
        if size(&app).0 > size(&app).1 {
            landscape = Some(at);
            break;
        }
    }
    let landscape = landscape.expect("the landscape button is on the bar");
    let _ = app.engine.undo();
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    assert_eq!(size(&app), portrait);
    // The Welcome Screen shown (the drawing behind it): the bar is pale.
    app.show_welcome = true;
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    click_at(&ctx, &mut app, landscape, Modifiers::NONE);
    assert_eq!(size(&app), portrait);
    // Back on the drawing, the same click turns the page.
    app.show_welcome = false;
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    click_at(&ctx, &mut app, landscape, Modifiers::NONE);
    let turned = size(&app);
    assert!(turned.0 > turned.1, "{turned:?}");
}

#[test]
fn the_shape_tool_takes_the_nearest_node_of_a_scaled_curve() {
    let (ctx, mut app, id) = setup();
    app.select(vec![id]);
    app.delete_selection();
    // A small curve in its own units, then scaled ten times on the page,
    // as text converted to curves can be: nodes 1 mm apart locally are
    // 10 mm apart on the page.
    let mut path = tracedraw_core::BezPath::new();
    path.move_to((2.0, 2.0));
    path.line_to((3.0, 2.0));
    path.line_to((4.0, 2.0));
    path.line_to((4.0, 3.0));
    let c = app
        .new_shape(ShapeKind::Path {
            path,
            closed: false,
        })
        .expect("curve");
    app.select(vec![c]);
    app.transform_selection(tracedraw_core::geometry::Affine::scale(10.0));
    app.set_tool(Tool::Shape);
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    let page = |x: f64, y: f64| Point::new(x * 10.0, y * 10.0);
    // On each node, that node; a little off it, still the nearest.
    for (i, q) in [
        (0usize, page(2.0, 2.0)),
        (1, page(3.0, 2.0)),
        (2, page(4.0, 2.0)),
        (3, page(4.0, 3.0)),
    ] {
        let hit = app.node_at(q).expect("node");
        assert_eq!((hit.shape, hit.index), (c, i), "at {q:?}");
        let off = Point::new(q.x + 0.4, q.y - 0.3);
        assert_eq!(app.node_at(off).map(|h| h.index), Some(i), "near {q:?}");
    }
    // Halfway between two nodes (5 mm from each): beyond the pick
    // distance, no node.
    assert!(app.node_at(page(3.5, 2.0)).is_none());
    // A segment is found on the page too: the middle of the last one.
    let (_, seg, _) = app.segment_at(page(4.0, 2.5)).expect("segment");
    assert_eq!(seg, 3);
}

/// A browser gives a right button pressed while the left one is held as
/// a pointer move, so it is noted outside egui's events (`web.rs`); the
/// next frame of the drag takes it as the right click that leaves the
/// original. The desktop never notes one.
#[test]
fn a_noted_chorded_right_press_leaves_the_original_too() {
    assert!(crate::interaction::is_chorded_right_press(2, 3));
    assert!(
        !crate::interaction::is_chorded_right_press(2, 2),
        "left not held"
    );
    assert!(
        !crate::interaction::is_chorded_right_press(0, 1),
        "the left button"
    );
    let (ctx, mut app, id) = setup();
    let from = app.view.to_screen(Point::new(35.0, 25.0));
    let to = from + egui::vec2(150.0, 0.0);
    frame(
        &ctx,
        &mut app,
        vec![Event::PointerMoved(from)],
        Modifiers::NONE,
    );
    frame(
        &ctx,
        &mut app,
        vec![button(from, true, PointerButton::Primary, Modifiers::NONE)],
        Modifiers::NONE,
    );
    let mid = from + (to - from) * 0.5;
    frame(
        &ctx,
        &mut app,
        vec![Event::PointerMoved(mid)],
        Modifiers::NONE,
    );
    assert!(!app.drag_copy);
    crate::interaction::note_chorded_right_press();
    frame(
        &ctx,
        &mut app,
        vec![Event::PointerMoved(mid + egui::vec2(10.0, 0.0))],
        Modifiers::NONE,
    );
    assert!(app.drag_copy, "the press is taken on the next frame");
    assert_eq!(app.status, crate::i18n::tr("status.drag_copy_on"));
    // Taken once: the next frame does not take it back.
    frame(
        &ctx,
        &mut app,
        vec![Event::PointerMoved(mid + egui::vec2(20.0, 0.0))],
        Modifiers::NONE,
    );
    assert!(app.drag_copy);
    // A second press takes it back, a third sets it again.
    crate::interaction::note_chorded_right_press();
    frame(
        &ctx,
        &mut app,
        vec![Event::PointerMoved(mid + egui::vec2(30.0, 0.0))],
        Modifiers::NONE,
    );
    assert!(!app.drag_copy);
    crate::interaction::note_chorded_right_press();
    frame(
        &ctx,
        &mut app,
        vec![Event::PointerMoved(to)],
        Modifiers::NONE,
    );
    frame(
        &ctx,
        &mut app,
        vec![button(to, false, PointerButton::Primary, Modifiers::NONE)],
        Modifiers::NONE,
    );
    assert_eq!(shape_count(&app), 2);
    assert!(close(bounds(&app, id).x0, 10.0), "the original stays");
    let copy = app.selection.first().copied().expect("copy");
    assert_ne!(copy, id);
    assert!(bounds(&app, copy).x0 > 15.0);
}

#[test]
fn a_clip_frame_is_painted_and_reshaped_through_its_frame() {
    let (ctx, mut app, a) = setup();
    let b = app
        .new_shape(ShapeKind::Rect {
            rect: Rect::new(20.0, 15.0, 40.0, 30.0),
            radius: 0.0,
            corners: None,
        })
        .expect("rect");
    app.place_inside(vec![b], a);
    app.select(vec![a]);
    let frame_of = |app: &App| match &app.doc().find_shape(a).expect("clip").kind {
        ShapeKind::ClipFrame { frame, .. } => (**frame).clone(),
        other => panic!("not a clip frame: {other:?}"),
    };
    let fid = frame_of(&app).id;
    assert_ne!(fid, a);
    // Painting.
    let blue = tracedraw_core::Fill::Solid(tracedraw_core::Color::rgb8(0, 0, 200));
    app.apply_fill(blue.clone());
    assert_eq!(frame_of(&app).fill, blue);
    assert_eq!(
        app.paint_targets()[0].fill,
        blue,
        "the readouts show the frame's"
    );
    assert_ne!(
        app.doc().find_shape(b).expect("contents").fill,
        blue,
        "the contents keep theirs"
    );
    app.apply_outline_width(1.5);
    assert_eq!(frame_of(&app).stroke.map(|s| s.width), Some(1.5));
    // The Shape tool: the frame's corner node, dragged along the side,
    // rounds the corners.
    app.set_tool(Tool::Shape);
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    let (s, _) = app
        .kind_node_at(Point::new(60.0, 40.0))
        .expect("a corner node");
    assert_eq!(s.id, fid);
    let corner = app.view.to_screen(Point::new(60.0, 40.0));
    let along = app.view.to_screen(Point::new(50.0, 40.0));
    drag(&ctx, &mut app, corner, along, Modifiers::NONE, Vec::new());
    let f = frame_of(&app);
    assert!(
        matches!(f.kind, ShapeKind::Rect { radius, .. } if radius > 5.0),
        "{:?}",
        f.kind
    );
    assert!(
        app.doc().find_shape(b).is_some(),
        "the contents are still inside"
    );
    // Still movable with the Pick tool.
    app.set_tool(Tool::Pick);
    app.select(vec![a]);
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    let before = bounds(&app, a);
    let from = app.view.to_screen(Point::new(15.0, 15.0));
    drag(
        &ctx,
        &mut app,
        from,
        from + egui::vec2(100.0, 0.0),
        Modifiers::NONE,
        Vec::new(),
    );
    assert!(bounds(&app, a).x0 > before.x0 + 5.0);
    assert!(matches!(
        app.doc().find_shape(a).expect("clip").kind,
        ShapeKind::ClipFrame { .. }
    ));
}

/// The key that keeps a drawn segment to the constrain angle is chosen
/// in Options > Tools > Curve tools: Ctrl at first, Shift or Alt on
/// request; the others then do nothing of the kind (#247).
#[test]
fn the_constrain_key_can_be_shift_or_alt() {
    let level = |app: &App| {
        let pts: Vec<Point> = last_path(app)
            .elements()
            .iter()
            .filter_map(|e| match e {
                tracedraw_core::geometry::PathEl::MoveTo(p)
                | tracedraw_core::geometry::PathEl::LineTo(p) => Some(*p),
                _ => None,
            })
            .collect();
        pts.len() == 2 && (pts[0].y - pts[1].y).abs() < 1e-6
    };
    for (key, mods, other) in [
        (
            crate::prefs::ConstrainKey::Shift,
            Modifiers::SHIFT,
            Modifiers::CTRL,
        ),
        (
            crate::prefs::ConstrainKey::Alt,
            Modifiers::ALT,
            Modifiers::CTRL,
        ),
        (
            crate::prefs::ConstrainKey::Ctrl,
            Modifiers::CTRL,
            Modifiers::SHIFT,
        ),
    ] {
        let (ctx, mut app, _id) = setup();
        app.settings.curves.constrain_key = key;
        app.set_tool(Tool::Freehand);
        let a = app.view.to_screen(Point::new(100.0, 100.0));
        let b = app.view.to_screen(Point::new(160.0, 104.0));
        click_at(&ctx, &mut app, a, Modifiers::NONE);
        click_at(&ctx, &mut app, b, mods);
        assert!(level(&app), "{key:?} held: level");
        app.set_tool(Tool::Freehand);
        let a = app.view.to_screen(Point::new(100.0, 150.0));
        let b = app.view.to_screen(Point::new(160.0, 154.0));
        click_at(&ctx, &mut app, a, Modifiers::NONE);
        click_at(&ctx, &mut app, b, other);
        assert!(!level(&app), "{key:?}: another key does not constrain");
    }
}

/// Shift with a marquee turns over the selection of the objects it takes
/// (#401): one picked, a Shift marquee round all three leaves the other
/// two picked.
#[test]
fn a_shift_marquee_inverts_the_objects_it_takes() {
    let (ctx, mut app, first) = setup();
    let rect = |app: &mut App, x: f64| {
        app.new_shape(ShapeKind::Rect {
            rect: Rect::new(x, 10.0, x + 20.0, 40.0),
            radius: 0.0,
            corners: None,
        })
        .expect("rect")
    };
    let b = rect(&mut app, 80.0);
    let c = rect(&mut app, 120.0);
    app.select(vec![first]);
    let from = app.view.to_screen(Point::new(0.0, 60.0));
    let to = app.view.to_screen(Point::new(150.0, 0.0));
    drag(&ctx, &mut app, from, to, Modifiers::SHIFT, Vec::new());
    let mut got = app.selection.clone();
    got.sort();
    let mut want = vec![b, c];
    want.sort();
    assert_eq!(got, want);
    // Without Shift a marquee picks all three.
    app.select(vec![first]);
    drag(&ctx, &mut app, from, to, Modifiers::NONE, Vec::new());
    assert_eq!(app.selection.len(), 3);
}

/// With the Shape tool, the align keys align the selected nodes (#364):
/// T to the top node, L to the leftmost, E to the middle of their box;
/// handles move with their node; one undo step each.
#[test]
fn align_keys_align_selected_nodes_in_the_shape_tool() {
    let (ctx, mut app, id) = setup();
    let mut path = tracedraw_core::BezPath::new();
    path.move_to((10.0, 10.0));
    path.curve_to((20.0, 10.0), (30.0, 20.0), (40.0, 30.0));
    path.line_to((70.0, 20.0));
    app.select(vec![id]);
    app.delete_selection();
    let c = app
        .new_shape(ShapeKind::Path {
            path,
            closed: false,
        })
        .expect("curve");
    app.select(vec![c]);
    app.set_tool(Tool::Shape);
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    let ns = |app: &App| match app.doc().find_shape(c).map(|s| s.kind.clone()) {
        Some(ShapeKind::Path { path, .. }) => tracedraw_core::nodes::nodes(&path),
        _ => panic!("curve"),
    };
    let all: Vec<_> = ns(&app).iter().map(|n| (c, n.index)).collect();
    assert_eq!(all.len(), 3);
    app.node_selection = all;
    let press = |app: &mut App, k: egui::Key| {
        frame(&ctx, app, vec![key(k, Modifiers::NONE)], Modifiers::NONE);
    };
    press(&mut app, egui::Key::T);
    let after = ns(&app);
    assert!(
        after.iter().all(|n| (n.pos.y - 30.0).abs() < 1e-9),
        "{after:?}"
    );
    // The first node's outgoing handle moved with it, 20 mm up.
    let first = &after[0];
    assert!((first.ctrl_out.expect("handle").y - 30.0).abs() < 1e-9);
    assert_eq!(app.tool, Tool::Shape);
    assert_eq!(app.selection, vec![c]);
    press(&mut app, egui::Key::L);
    assert!(ns(&app).iter().all(|n| (n.pos.x - 10.0).abs() < 1e-9));
    app.undo();
    let back = ns(&app);
    assert!((back[2].pos.x - 70.0).abs() < 1e-9, "{back:?}");
    press(&mut app, egui::Key::E);
    assert!(ns(&app).iter().all(|n| (n.pos.y - 30.0).abs() < 1e-9));
    // One node: the keys do what they did before (nothing to the nodes).
    app.node_selection.truncate(1);
    assert!(!app.align_nodes_to(crate::ops::Align::Bottom));
}

/// A Pen curve keeps its anchors in the history once it is closed and
/// deselected (#360): Ctrl+Z opens it first, then takes the anchors back
/// one by one, and redo builds it again.
#[test]
fn a_finished_pen_curve_undoes_anchor_by_anchor() {
    let (ctx, mut app, _) = setup();
    app.select(Vec::new());
    app.set_tool(Tool::Pen);
    let pts = [
        Point::new(100.0, 150.0),
        Point::new(150.0, 150.0),
        Point::new(150.0, 200.0),
        Point::new(100.0, 200.0),
    ];
    for p in pts {
        click_page(&ctx, &mut app, p);
        for _ in 0..40 {
            frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
        }
    }
    // A click on the first node closes it.
    click_page(&ctx, &mut app, pts[0]);
    assert!(app.curve.is_none());
    let id = *app.selection.first().expect("the new curve");
    let path_of = |app: &App| match app.doc().find_shape(id).map(|s| s.kind.clone()) {
        Some(ShapeKind::Path { path, closed }) => Some((path, closed)),
        _ => None,
    };
    let count = |p: &tracedraw_core::BezPath| tracedraw_core::nodes::nodes(p).len();
    let (closed_path, closed) = path_of(&app).expect("curve");
    assert!(closed);
    assert_eq!(count(&closed_path), 4);
    // Deselected, then undone step by step.
    app.select(Vec::new());
    app.undo();
    let (p, closed) = path_of(&app).expect("open again");
    assert!(!closed && count(&p) == 4, "{p:?}");
    app.undo();
    assert_eq!(path_of(&app).map(|(p, _)| count(&p)), Some(3));
    app.undo();
    assert_eq!(path_of(&app).map(|(p, _)| count(&p)), Some(2));
    app.undo();
    assert!(path_of(&app).is_none());
    // Redo brings it back the same way, closed at the end.
    for _ in 0..4 {
        app.redo();
    }
    assert_eq!(path_of(&app), Some((closed_path, true)));
    // The curve still has the default fill and outline of a closed curve.
    let s = app.doc().find_shape(id).cloned().expect("curve");
    let mut want = tracedraw_core::Shape::new(
        id,
        ShapeKind::Path {
            path: s.page_path(),
            closed: true,
        },
    );
    app.doc().defaults.apply_to(&mut want);
    assert_eq!((s.fill, s.stroke), (want.fill, want.stroke));
}

/// A scale handle follows the pointer from the frame the drag starts: no
/// jump of the few pixels the pointer goes before a press becomes a drag
/// (#383).
#[test]
fn a_scale_handle_follows_the_pointer_from_the_start() {
    let (ctx, mut app, _) = setup();
    for _ in 0..3 {
        frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    }
    let e = handle(&app, Handle::E);
    let w = app.selection_bounds().expect("bounds").width() as f32 * app.view.zoom;
    frame(
        &ctx,
        &mut app,
        vec![Event::PointerMoved(e)],
        Modifiers::NONE,
    );
    frame(
        &ctx,
        &mut app,
        vec![button(e, true, PointerButton::Primary, Modifiers::NONE)],
        Modifiers::NONE,
    );
    let mut seen = Vec::new();
    for i in 1..=20 {
        let p = e + egui::vec2(i as f32, 0.0);
        frame(
            &ctx,
            &mut app,
            vec![Event::PointerMoved(p)],
            Modifiers::NONE,
        );
        if let Some(t) = app.preview_transform_of(&app.drag) {
            let want = 1.0 + i as f64 / f64::from(w);
            seen.push((i, t.as_coeffs()[0], want));
        }
    }
    assert!(!seen.is_empty());
    for (i, got, want) in seen {
        assert!((got - want).abs() < 0.01, "step {i}: {got} {want}");
    }
}

/// The Shape tool on a B-spline: a marquee round one control point
/// chooses it alone, Delete removes that point and keeps the curve, a
/// click chooses one and Shift+click adds another, and a drag of a chosen
/// point moves the chosen ones together (a delete took the whole curve).
#[test]
fn bspline_control_points_are_chosen_and_deleted_alone() {
    let (ctx, mut app, _) = setup();
    app.select(Vec::new());
    app.set_tool(Tool::BSpline);
    let pts = [
        Point::new(100.0, 150.0),
        Point::new(130.0, 190.0),
        Point::new(180.0, 180.0),
        Point::new(190.0, 140.0),
        Point::new(150.0, 120.0),
    ];
    for p in pts {
        let at = app.view.to_screen(p);
        click_at(&ctx, &mut app, at, Modifiers::NONE);
    }
    app.finish_curve();
    let id = app.selection[0];
    let spline = |app: &App| app.doc().find_shape(id).and_then(|s| s.spline.clone());
    app.select(vec![id]);
    app.set_tool(Tool::Shape);
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    // A marquee round the second point only.
    let a = app
        .view
        .to_screen(pts[1] + tracedraw_core::geometry::Vec2::new(-6.0, -6.0));
    let b = app
        .view
        .to_screen(pts[1] + tracedraw_core::geometry::Vec2::new(6.0, 6.0));
    drag(&ctx, &mut app, a, b, Modifiers::NONE, Vec::new());
    assert_eq!(app.node_selection, vec![(id, 1)]);
    frame(
        &ctx,
        &mut app,
        vec![key(egui::Key::Delete, Modifiers::NONE)],
        Modifiers::NONE,
    );
    let left = spline(&app).expect("the curve stays");
    assert_eq!(left.len(), 4, "{left:?}");
    assert!(left.iter().all(|p| (*p - pts[1]).hypot() > 1.0));
    // A click chooses one point, Shift+click another; a drag moves both.
    let at = app.view.to_screen(pts[2]);
    click_at(&ctx, &mut app, at, Modifiers::NONE);
    assert_eq!(app.node_selection, vec![(id, 1)]);
    let at = app.view.to_screen(pts[3]);
    click_at(&ctx, &mut app, at, Modifiers::SHIFT);
    assert_eq!(app.node_selection, vec![(id, 1), (id, 2)]);
    let from = app.view.to_screen(pts[2]);
    let to = app
        .view
        .to_screen(pts[2] + tracedraw_core::geometry::Vec2::new(10.0, 0.0));
    drag(&ctx, &mut app, from, to, Modifiers::NONE, Vec::new());
    let moved = spline(&app).expect("spline");
    assert!((moved[1].x - (pts[2].x + 10.0)).abs() < 1.0, "{moved:?}");
    assert!((moved[2].x - (pts[3].x + 10.0)).abs() < 1.0, "{moved:?}");
    assert!((moved[0] - pts[0]).hypot() < 0.5, "{moved:?}");
}

/// Editing a symmetry as in the reference program: a click picks one of
/// its objects (not the group, not a copy) and the mode stays; a drag on a
/// mirror line moves the centre; a click on a line shows handles that turn
/// the lines; a marquee takes members; a new object joins.
#[test]
fn symmetry_editing_picks_members_and_drags_lines() {
    use tracedraw_core::live::Effect;
    let (ctx, mut app, a) = setup();
    let b = app
        .new_shape(ShapeKind::Rect {
            rect: Rect::new(10.0, 60.0, 30.0, 80.0),
            radius: 0.0,
            corners: None,
        })
        .expect("rect");
    app.select(vec![a, b]);
    app.create_symmetry();
    let g = app.symmetry_edit.expect("editing");
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    // The rectangle a spans 10..60 x 10..40; the line is at x = 60.
    let (_, c0, ang0, _) = app.edited_symmetry().expect("symmetry");
    // A click on b picks b alone; the mode and its bar stay.
    let at = app.view.to_screen(Point::new(20.0, 70.0));
    click_at(&ctx, &mut app, at, Modifiers::NONE);
    assert_eq!(app.selection, vec![b]);
    assert_eq!(app.symmetry_edit, Some(g));
    assert!(app.edited_symmetry().is_some());
    // A click on a copy (mirrored to the right of the line) picks nothing
    // of the symmetry.
    let at = app.view.to_screen(Point::new(100.0, 70.0));
    click_at(&ctx, &mut app, at, Modifiers::NONE);
    assert!(!app.selection.contains(&g));
    // A drag on the line, away from the centre, moves the centre.
    let from = app.view.to_screen(Point::new(c0.x, c0.y + 40.0));
    let to = app.view.to_screen(Point::new(c0.x + 15.0, c0.y + 40.0));
    drag(&ctx, &mut app, from, to, Modifiers::NONE, Vec::new());
    let (_, c1, ang1, _) = app.edited_symmetry().expect("symmetry");
    assert!((c1.x - (c0.x + 15.0)).abs() < 0.6, "{c0:?} -> {c1:?}");
    assert!((ang1 - ang0).abs() < 1e-9);
    // One undo takes the move back.
    app.undo();
    assert!((app.edited_symmetry().unwrap().1.x - c0.x).abs() < 1e-6);
    // A click on the line shows the turning handles; dragging one turns.
    let on_line = app.view.to_screen(Point::new(c0.x, c0.y + 30.0));
    click_at(&ctx, &mut app, on_line, Modifiers::NONE);
    assert!(app.symmetry_turning);
    let hs = app.symmetry_handles();
    assert_eq!(hs.len(), 2);
    let h = hs[0];
    let v = h - c0;
    let turned = c0 + tracedraw_core::geometry::Vec2::new(-v.y, v.x);
    let (hf, ht) = (app.view.to_screen(h), app.view.to_screen(turned));
    drag(&ctx, &mut app, hf, ht, Modifiers::NONE, Vec::new());
    let (_, _, ang2, _) = app.edited_symmetry().expect("symmetry");
    let diff = (ang2 - ang0).rem_euclid(360.0);
    assert!((diff - 90.0).abs() < 2.0, "{ang0} -> {ang2}");
    // A marquee round both originals takes the two members.
    app.set_tool(Tool::Pick);
    let m0 = app.view.to_screen(Point::new(5.0, 5.0));
    let m1 = app.view.to_screen(Point::new(63.0, 85.0));
    // Start the marquee off the lines and the objects.
    app.select(Vec::new());
    drag(&ctx, &mut app, m0, m1, Modifiers::NONE, Vec::new());
    let mut sel = app.selection.clone();
    sel.sort();
    assert_eq!(sel, vec![a, b]);
    // Still one symmetry group holding both.
    assert!(app.doc().find_shape(g).is_some_and(|s| s
        .effects
        .iter()
        .any(|e| matches!(e, Effect::Symmetry { .. }))));
}

/// Frame times on a page of many objects (#345): run with
/// `cargo test --release -- --ignored frame_times_on_a_heavy_page --nocapture`.
#[test]
#[ignore]
fn frame_times_on_a_heavy_page() {
    let (ctx, mut app, _) = setup();
    let mut s = 7u64;
    let mut r = || {
        s = s
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((s >> 33) as f64) / (1u64 << 31) as f64
    };
    let n: usize = std::env::var("FRAME_N")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5000);
    let only: Option<String> = std::env::var("FRAME_ONLY").ok();
    for _ in 0..n {
        let (x, y) = (r() * 200.0, r() * 280.0);
        let mut p = tracedraw_core::geometry::BezPath::new();
        p.move_to((x, y));
        for _ in 0..6 {
            p.curve_to(
                (x + r() * 20.0 - 10.0, y + r() * 20.0 - 10.0),
                (x + r() * 20.0 - 10.0, y + r() * 20.0 - 10.0),
                (x + r() * 20.0 - 10.0, y + r() * 20.0 - 10.0),
            );
        }
        p.close_path();
        app.new_shape(ShapeKind::Path {
            path: p,
            closed: true,
        });
    }
    app.select(Vec::new());
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    let time = |label: &str, app: &mut App, f: &mut dyn FnMut(&mut App)| {
        if only.as_deref().is_some_and(|o| !label.starts_with(o)) {
            return;
        }
        let t = std::time::Instant::now();
        for _ in 0..10 {
            f(app);
        }
        eprintln!("{label}: {:?} per frame", t.elapsed() / 10);
    };
    let mut x = 300.0f32;
    time("hover", &mut app, &mut |app| {
        x += 3.0;
        frame(
            &ctx,
            app,
            vec![Event::PointerMoved(egui::pos2(x, 300.0))],
            Modifiers::NONE,
        );
    });
    let at = app.view.to_screen(Point::new(100.0, 100.0));
    frame(
        &ctx,
        &mut app,
        vec![Event::PointerMoved(at)],
        Modifiers::NONE,
    );
    frame(
        &ctx,
        &mut app,
        vec![button(at, true, PointerButton::Primary, Modifiers::NONE)],
        Modifiers::NONE,
    );
    let mut d = 0.0f32;
    time("drag (marquee or move)", &mut app, &mut |app| {
        d += 4.0;
        frame(
            &ctx,
            app,
            vec![Event::PointerMoved(at + egui::vec2(d, d))],
            Modifiers::NONE,
        );
    });
    frame(
        &ctx,
        &mut app,
        vec![button(
            at + egui::vec2(d, d),
            false,
            PointerButton::Primary,
            Modifiers::NONE,
        )],
        Modifiers::NONE,
    );
    app.select_all();
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    time("all selected, hover", &mut app, &mut |app| {
        x += 3.0;
        frame(
            &ctx,
            app,
            vec![Event::PointerMoved(egui::pos2(x, 320.0))],
            Modifiers::NONE,
        );
    });
}

/// A symmetry made from one object, edited: a click on the object shows
/// its own box, not the box of its copies, and a rectangle drawn with the
/// tool joins the symmetry (the two become its group).
#[test]
fn a_lone_symmetry_object_is_picked_alone_and_takes_new_drawings() {
    let (ctx, mut app, a) = setup();
    app.create_symmetry();
    assert_eq!(app.symmetry_edit, Some(a));
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    app.select(Vec::new());
    let at = app.view.to_screen(Point::new(30.0, 25.0));
    click_at(&ctx, &mut app, at, Modifiers::NONE);
    assert_eq!(app.selection, vec![a]);
    let b = app.selection_bounds().expect("bounds");
    assert!(
        (b.x0 - 10.0).abs() < 1.0 && (b.x1 - 60.0).abs() < 1.0,
        "the object's own box: {b:?}"
    );
    // Where a mirror line runs over the object, a click picks the
    // object rather than showing the line's turning handles.
    let (_, c, ..) = app.edited_symmetry().expect("symmetry");
    app.select(Vec::new());
    let at = app.view.to_screen(Point::new(c.x - 1.0, 12.0));
    click_at(&ctx, &mut app, at, Modifiers::NONE);
    assert_eq!(app.selection, vec![a], "centre at {c:?}");
    assert!(!app.symmetry_turning);
    // Drawn with the rectangle tool, below the object.
    app.set_tool(Tool::Rectangle);
    let from = app.view.to_screen(Point::new(10.0, 60.0));
    let to = app.view.to_screen(Point::new(30.0, 80.0));
    drag(&ctx, &mut app, from, to, Modifiers::NONE, Vec::new());
    let g = app.symmetry_edit.expect("still editing");
    assert_ne!(g, a, "the object and the drawing became a group");
    let kids = match app.doc().find_shape(g).map(|s| &s.kind) {
        Some(ShapeKind::Group { children }) => children.len(),
        _ => 0,
    };
    assert_eq!(kids, 2);
}

fn double_click_at(ctx: &egui::Context, app: &mut App, at: Pos2) {
    let m = Modifiers::NONE;
    // Past the triple-click time since the last click.
    for _ in 0..40 {
        frame(ctx, app, Vec::new(), m);
    }
    frame(ctx, app, vec![Event::PointerMoved(at)], m);
    for _ in 0..2 {
        frame(
            ctx,
            app,
            vec![button(at, true, PointerButton::Primary, m)],
            m,
        );
        frame(
            ctx,
            app,
            vec![button(at, false, PointerButton::Primary, m)],
            m,
        );
    }
    for _ in 0..20 {
        frame(ctx, app, Vec::new(), m);
    }
}

/// A finished symmetry is one object, as in the reference program: a
/// click picks the whole of it and a drag moves it all; a double click
/// goes back into editing it, where a click picks one object inside.
#[test]
fn a_finished_symmetry_is_picked_whole_and_a_double_click_edits_it() {
    let (ctx, mut app, a) = setup();
    let b = app
        .new_shape(ShapeKind::Rect {
            rect: Rect::new(10.0, 60.0, 30.0, 80.0),
            radius: 0.0,
            corners: None,
        })
        .expect("rect");
    app.select(vec![a, b]);
    app.apply_fill(tracedraw_core::Fill::Solid(tracedraw_core::Color::rgb8(
        200, 30, 30,
    )));
    app.create_symmetry();
    let g = app.symmetry_edit.expect("editing");
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    // The bar's Finish, clicked.
    let finish = app.canvas_rect.min + egui::vec2(14.0 + 10.0 + 30.0, 14.0 + 6.0 + 9.0);
    click_at(&ctx, &mut app, finish, Modifiers::NONE);
    assert_eq!(app.symmetry_edit, None, "Finish ends the editing");
    app.select(Vec::new());
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    let on_a = app.view.to_screen(Point::new(30.0, 25.0));
    click_at(&ctx, &mut app, on_a, Modifiers::NONE);
    assert_eq!(app.symmetry_edit, None, "a click does not edit");
    assert_eq!(app.selection, vec![g], "the whole symmetry");
    let before = app.doc().shape_on_page(g).expect("g").bounds();
    let to = app.view.to_screen(Point::new(50.0, 25.0));
    drag(&ctx, &mut app, on_a, to, Modifiers::NONE, Vec::new());
    let after = app.doc().shape_on_page(g).expect("g").bounds();
    assert!(
        (after.x0 - before.x0 - 20.0).abs() < 1.0,
        "{before:?} -> {after:?}"
    );
    // Off the selection's centre mark (a double click there resets it).
    let on_a = app.view.to_screen(Point::new(36.0, 14.0));
    double_click_at(&ctx, &mut app, on_a);
    assert_eq!(app.symmetry_edit, Some(g), "a double click edits it");
    click_at(&ctx, &mut app, on_a, Modifiers::NONE);
    assert_eq!(app.selection, vec![a], "one object inside");
}

/// Shape tool: Ctrl or Shift while dragging a handle keeps it to the
/// constrain angle about its node, and a smooth node's other handle keeps
/// following it (Ctrl made the node a cusp instead, Shift did nothing;
/// #510).
#[test]
fn ctrl_or_shift_keeps_a_handle_to_the_constrain_angle() {
    use tracedraw_core::geometry::BezPath;
    use tracedraw_core::nodes;
    for mods in [Modifiers::CTRL, Modifiers::SHIFT] {
        let (ctx, mut app, _) = setup();
        let mut path = BezPath::new();
        path.move_to((10.0, 50.0));
        path.curve_to((20.0, 50.0), (30.0, 60.0), (40.0, 60.0));
        path.curve_to((50.0, 60.0), (60.0, 50.0), (70.0, 50.0));
        let id = app
            .new_shape(ShapeKind::Path {
                path,
                closed: false,
            })
            .expect("curve");
        app.select(vec![id]);
        app.set_tool(Tool::Shape);
        app.node_selection = vec![(id, 1)];
        frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
        let from = app.view.to_screen(Point::new(50.0, 60.0));
        let to = app.view.to_screen(Point::new(58.0, 63.0));
        drag(&ctx, &mut app, from, to, mods, Vec::new());
        let ShapeKind::Path { path, .. } = &app.doc().find_shape(id).expect("curve").kind else {
            panic!("a curve");
        };
        let n = nodes::nodes(path)
            .into_iter()
            .find(|n| n.index == 1)
            .expect("node");
        let out = n.ctrl_out.expect("out") - n.pos;
        let back = n.ctrl_in.expect("in") - n.pos;
        let a = out.y.atan2(out.x).to_degrees();
        assert!((a - 15.0).abs() < 0.5, "{mods:?}: {a}");
        let b = back.y.atan2(back.x).to_degrees();
        assert!(
            ((b - a).abs() - 180.0).abs() < 0.5,
            "{mods:?}: still smooth, {b}"
        );
    }
}

/// Pen: Ctrl while dragging on the first node to close the curve keeps
/// the closing handle to the constrain angle, as on every other node
/// (#473).
#[test]
fn ctrl_keeps_the_closing_handle_to_the_constrain_angle() {
    let (ctx, mut app, _) = setup();
    app.select(Vec::new());
    app.set_tool(Tool::Pen);
    let a = Point::new(100.0, 150.0);
    pull(&ctx, &mut app, a, Point::new(110.0, 170.0));
    pull(
        &ctx,
        &mut app,
        Point::new(150.0, 190.0),
        Point::new(170.0, 190.0),
    );
    click_page(&ctx, &mut app, Point::new(160.0, 130.0));
    let (from, to) = (
        app.view.to_screen(a),
        app.view.to_screen(Point::new(88.0, 146.0)),
    );
    drag(
        &ctx,
        &mut app,
        from,
        to,
        Modifiers::CTRL,
        vec![key(egui::Key::C, Modifiers::CTRL)],
    );
    assert!(app.curve.is_none());
    let segs: Vec<_> = last_path(&app).segments().collect();
    let closing = segs
        .iter()
        .rev()
        .find_map(|s| match s {
            tracedraw_core::geometry::PathSeg::Cubic(c) if near(c.p3, a) => Some(*c),
            _ => None,
        })
        .expect("a curved closing segment");
    let d = closing.p2 - a;
    let angle = d.y.atan2(d.x).to_degrees().rem_euclid(360.0);
    // Free, the handle would point at 198.4°; kept to 15° steps, 195°.
    assert!((angle - 195.0).abs() < 0.5, "{angle} {closing:?}");
}

/// While a text is edited, Space types a space and Ctrl+Space leaves the
/// text for the Pick tool, the text kept as typed (autocorrect starting
/// the sentence with a capital) (#478).
#[test]
fn ctrl_space_leaves_text_editing_for_the_pick_tool() {
    let (ctx, mut app, _) = setup();
    app.select(Vec::new());
    app.start_text(Point::new(50.0, 50.0), None);
    app.text_insert("ab");
    frame(
        &ctx,
        &mut app,
        vec![
            key(egui::Key::Space, Modifiers::NONE),
            Event::Text(" ".into()),
        ],
        Modifiers::NONE,
    );
    assert!(app.text_edit.is_some());
    assert_eq!(
        app.edit_spans()
            .map(|s| tracedraw_core::document::spans_text(&s)),
        Some("Ab ".into())
    );
    frame(
        &ctx,
        &mut app,
        vec![
            key(egui::Key::Space, Modifiers::CTRL),
            Event::Text(" ".into()),
        ],
        Modifiers::CTRL,
    );
    assert!(app.text_edit.is_none(), "editing ended");
    assert_eq!(app.tool, Tool::Pick);
    let id = app.selection[0];
    match &app.doc().find_shape(id).expect("text").kind {
        ShapeKind::Text { spans, .. } => {
            assert_eq!(tracedraw_core::document::spans_text(spans), "Ab ")
        }
        other => panic!("{other:?}"),
    }
}

/// Alt held while 0193 is typed on the keypad gives Á when Alt is let
/// go, the keystroke the Glyphs docker shows; with Num Lock off the
/// keypad sends Insert, End, PageDown and PageDown, read by their
/// physical keys (#469).
#[test]
fn alt_codes_typed_on_the_keypad_give_their_character() {
    let (ctx, mut app, _) = setup();
    app.select(Vec::new());
    app.start_text(Point::new(50.0, 50.0), None);
    let pad = |logical: egui::Key, physical: egui::Key| Event::Key {
        key: logical,
        physical_key: Some(physical),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::ALT,
    };
    frame(
        &ctx,
        &mut app,
        vec![
            pad(egui::Key::Insert, egui::Key::Num0),
            pad(egui::Key::End, egui::Key::Num1),
            pad(egui::Key::PageDown, egui::Key::Num9),
            pad(egui::Key::PageDown, egui::Key::Num3),
        ],
        Modifiers::ALT,
    );
    assert_eq!(
        app.edit_spans()
            .map(|s| tracedraw_core::document::spans_text(&s)),
        Some(String::new()),
        "nothing typed while Alt is held"
    );
    frame(&ctx, &mut app, vec![], Modifiers::NONE);
    assert_eq!(
        app.edit_spans()
            .map(|s| tracedraw_core::document::spans_text(&s)),
        Some("\u{c1}".into())
    );
}

/// Artistic text converted to a paragraph frame (Text > Convert) grows
/// down with its bottom handle: the flow tab sat under that handle and
/// took the press, so the next click linked the text to another object
/// (#503).
#[test]
fn the_bottom_handle_of_a_converted_frame_drags_it_taller() {
    let (ctx, mut app, rect) = setup();
    app.select(Vec::new());
    app.start_text(Point::new(80.0, 80.0), None);
    app.text_insert("Hello world");
    app.finish_text();
    let id = app.selection[0];
    app.toggle_text_kind();
    assert_eq!(app.selection, vec![id]);
    app.set_tool(Tool::Pick);
    for _ in 0..2 {
        frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    }
    let size = |app: &App| match &app.doc().find_shape(id).expect("text").kind {
        ShapeKind::Text { frame, .. } => frame.expect("a paragraph frame"),
        other => panic!("{other:?}"),
    };
    let before = size(&app);
    // The lower half of the handle, where the tab was.
    let from = handle(&app, Handle::S) + egui::vec2(0.0, 3.0);
    drag(
        &ctx,
        &mut app,
        from,
        from + egui::vec2(0.0, 80.0),
        Modifiers::NONE,
        vec![],
    );
    assert!(app.pending_text_link.is_none(), "no text link started");
    assert!(size(&app).height > before.height + 1.0, "{before:?}");
    // A click on the rectangle afterwards selects it; nothing links.
    let c = app.view.to_screen(bounds(&app, rect).center());
    click_at(&ctx, &mut app, c, Modifiers::NONE);
    assert!(app.next_frame(id).is_none());
}

/// Editing a clip frame's contents, a group inside drags, ungroups, and
/// its members (and an object drawn meanwhile) go back into the frame on
/// Finish; the ungrouped members were left out and the frame lost its
/// contents (#537).
#[test]
fn a_group_inside_a_clip_frame_being_edited_drags_and_ungroups() {
    let (ctx, mut app, frame_id) = setup();
    let mk = |app: &mut App, r: Rect| {
        let id = app
            .new_shape(ShapeKind::Rect {
                rect: r,
                radius: 0.0,
                corners: None,
            })
            .expect("rect");
        app.select(vec![id]);
        app.apply_fill(tracedraw_core::Fill::Solid(tracedraw_core::Color::rgb8(
            0, 0, 200,
        )));
        id
    };
    let a = mk(&mut app, Rect::new(15.0, 15.0, 30.0, 30.0));
    let b = mk(&mut app, Rect::new(32.0, 15.0, 45.0, 30.0));
    let outside = mk(&mut app, Rect::new(150.0, 150.0, 160.0, 160.0));
    app.select(vec![a, b]);
    app.group_selection();
    let g = app.selection[0];
    app.place_inside(vec![g], frame_id);
    app.select(vec![frame_id]);
    app.edit_clip_frame();
    assert_eq!(app.selection, vec![g]);
    for _ in 0..2 {
        frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    }
    let x0 = bounds(&app, g).x0;
    let from = app.view.to_screen(Point::new(22.0, 22.0));
    drag(
        &ctx,
        &mut app,
        from,
        from + egui::vec2(60.0, 0.0),
        Modifiers::NONE,
        vec![],
    );
    assert!(bounds(&app, g).x0 > x0 + 1.0, "the group moved");
    app.ungroup_selection();
    assert_eq!(app.selection, vec![a, b]);
    let (focus, _) = app.render_focus().expect("editing");
    assert!(
        focus.contains(&a) && focus.contains(&b),
        "the members stay in full"
    );
    assert!(!focus.contains(&outside));
    let c = mk(&mut app, Rect::new(20.0, 32.0, 25.0, 36.0));
    app.finish_clip_frame_edit();
    match &app.doc().find_shape(frame_id).expect("frame").kind {
        ShapeKind::ClipFrame { contents, .. } => {
            let ids: Vec<ShapeId> = contents.iter().map(|s| s.id).collect();
            assert_eq!(ids, vec![a, b, c]);
        }
        other => panic!("not a clip frame: {other:?}"),
    }
    assert!(app.doc().find_shape(outside).is_some());
}

/// Finish on the symmetry bar ends the editing: the bar and the mirror
/// lines go, the symmetry stays selected whole, a click takes it whole
/// and a drag moves it whole.
#[test]
fn finish_on_the_symmetry_bar_leaves_one_object() {
    let (ctx, mut app, id) = setup();
    app.select(vec![id]);
    app.create_symmetry();
    let sym = app.selection[0];
    assert_eq!(
        app.symmetry_edit,
        Some(sym),
        "a new symmetry opens for editing"
    );
    let texts = |app: &mut App| {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                egui::vec2(1600.0, 1000.0),
            )),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| crate::ui::root(app, ui));
        out.textures_delta.clear();
        let mut all: Vec<(String, Pos2)> = Vec::new();
        fn walk(s: &egui::Shape, all: &mut Vec<(String, Pos2)>) {
            match s {
                egui::Shape::Text(t) => all.push((
                    t.galley.text().to_string(),
                    t.pos + t.galley.rect.center().to_vec2(),
                )),
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, all)),
                _ => {}
            }
        }
        out.shapes.iter().for_each(|c| walk(&c.shape, &mut all));
        all
    };
    texts(&mut app);
    let finish = texts(&mut app)
        .into_iter()
        .find(|(t, _)| t.trim() == "Finish")
        .map(|(_, p)| p)
        .expect("the bar's Finish");
    for _ in 0..40 {
        frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    }
    click_at(&ctx, &mut app, finish, Modifiers::NONE);
    assert_eq!(app.symmetry_edit, None, "editing ended");
    assert!(app.edited_symmetry().is_none());
    assert_eq!(app.selection, vec![sym]);
    assert!(
        !texts(&mut app).iter().any(|(t, _)| t.trim() == "Finish"),
        "the bar is gone"
    );
    // One object: a click takes it whole, a drag moves it whole.
    for _ in 0..40 {
        frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    }
    let c = app.view.to_screen(bounds(&app, sym).center());
    click_at(&ctx, &mut app, c, Modifiers::NONE);
    assert_eq!(app.selection, vec![sym]);
    assert_eq!(app.symmetry_edit, None);
    let x0 = bounds(&app, sym).x0;
    drag(
        &ctx,
        &mut app,
        c,
        c + egui::vec2(80.0, 0.0),
        Modifiers::NONE,
        vec![],
    );
    assert!(bounds(&app, sym).x0 > x0 + 5.0);
    assert_eq!(app.symmetry_edit, None);
}
