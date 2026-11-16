//! Ctrl+K and the other command shortcuts after a property bar field
//! was typed into: the video of #2893 typed in the bar, then pressed
//! Ctrl+K, and nothing broke apart.
use super::*;

/// The rectangle of `setup` with a drop shadow, the bar's width field
/// clicked and "60" typed in it, not yet entered.
fn typed_in_the_width_field() -> (egui::Context, App, ShapeId) {
    let (ctx, mut app, id) = setup();
    app.set_shadow(Some(tracedraw_core::Shadow::default()), false);
    let texts = texts_drawn(&ctx, &mut app, Vec::new());
    let at = text_at(&texts, "50.0 mm").expect("the width field");
    click_at(&ctx, &mut app, at, Modifiers::NONE);
    assert!(
        ctx.egui_wants_keyboard_input(),
        "the field has the keyboard"
    );
    frame(
        &ctx,
        &mut app,
        vec![key(egui::Key::A, Modifiers::COMMAND)],
        Modifiers::COMMAND,
    );
    frame(
        &ctx,
        &mut app,
        vec![key_up(egui::Key::A, Modifiers::COMMAND)],
        Modifiers::NONE,
    );
    frame(
        &ctx,
        &mut app,
        vec![Event::Text("60".into())],
        Modifiers::NONE,
    );
    assert!(ctx.egui_wants_keyboard_input(), "still typing");
    (ctx, app, id)
}

fn tap(ctx: &egui::Context, app: &mut App, k: egui::Key, m: Modifiers) {
    frame(ctx, app, vec![key(k, m)], m);
    frame(ctx, app, vec![key_up(k, m)], Modifiers::NONE);
    frame(ctx, app, Vec::new(), Modifiers::NONE);
}

fn broken_apart(app: &App, id: ShapeId) -> bool {
    app.doc().find_shape(id).expect("rect").shadow.is_none() && shape_count(app) == 2
}

/// Enter takes the value and gives the keyboard back to the drawing:
/// Ctrl+K then breaks the shadow off.
#[test]
fn ctrl_k_after_enter_in_a_bar_field() {
    let (ctx, mut app, id) = typed_in_the_width_field();
    tap(&ctx, &mut app, egui::Key::Enter, Modifiers::NONE);
    assert!(
        close(bounds(&app, id).width(), 60.0),
        "{:?}",
        bounds(&app, id)
    );
    assert!(!ctx.egui_wants_keyboard_input(), "back to the drawing");
    tap(&ctx, &mut app, egui::Key::K, Modifiers::COMMAND);
    assert!(broken_apart(&app, id));
}

/// A click on the object takes the value and gives the keyboard back
/// to the drawing, the object still selected.
#[test]
fn ctrl_k_after_a_click_on_the_object() {
    let (ctx, mut app, id) = typed_in_the_width_field();
    let on = app.view.to_screen(Point::new(30.0, 25.0));
    click_at(&ctx, &mut app, on, Modifiers::NONE);
    assert!(
        close(bounds(&app, id).width(), 60.0),
        "{:?}",
        bounds(&app, id)
    );
    assert!(!ctx.egui_wants_keyboard_input(), "back to the drawing");
    assert_eq!(app.selection, vec![id]);
    tap(&ctx, &mut app, egui::Key::K, Modifiers::COMMAND);
    assert!(broken_apart(&app, id));
}

/// Ctrl+K pressed while the field still has the keyboard: the field
/// takes its value and the command runs all the same.
#[test]
fn ctrl_k_while_a_bar_field_has_the_keyboard() {
    let (ctx, mut app, id) = typed_in_the_width_field();
    tap(&ctx, &mut app, egui::Key::K, Modifiers::COMMAND);
    for _ in 0..2 {
        frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    }
    assert!(broken_apart(&app, id), "{}", shape_count(&app));
    assert!(
        close(bounds(&app, id).width(), 60.0),
        "{:?}",
        bounds(&app, id)
    );
    assert!(!ctx.egui_wants_keyboard_input(), "back to the drawing");
}

/// The keys that edit the text of a field stay the field's: Ctrl+Z,
/// Ctrl+C and Ctrl+A do not undo, copy or select in the drawing.
#[test]
fn editing_keys_stay_in_a_bar_field() {
    let (ctx, mut app, id) = typed_in_the_width_field();
    let steps = app.engine.history_labels().0.len();
    let selected = app.selection.clone();
    for k in [
        egui::Key::Z,
        egui::Key::C,
        egui::Key::A,
        egui::Key::X,
        egui::Key::V,
    ] {
        frame(
            &ctx,
            &mut app,
            vec![key(k, Modifiers::COMMAND)],
            Modifiers::COMMAND,
        );
        frame(
            &ctx,
            &mut app,
            vec![key_up(k, Modifiers::COMMAND)],
            Modifiers::NONE,
        );
        assert!(ctx.egui_wants_keyboard_input(), "{k:?}: the field keeps it");
    }
    assert_eq!(app.engine.history_labels().0.len(), steps, "nothing undone");
    assert_eq!(app.selection, selected);
    assert_eq!(shape_count(&app), 1, "nothing pasted or cut");
    assert!(app.doc().find_shape(id).expect("rect").shadow.is_some());
}
