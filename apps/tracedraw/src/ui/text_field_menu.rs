//! A right click on any text box (a font's name, a size, a field of a
//! dialog box) opens Cut, Copy, Paste, Select All and Delete for its text
//! (#3798). The box is found under the pointer among the frame's widgets
//! that keep a text box's state; the command picked reaches it as the
//! keyboard's would, the next frame, with the box given the focus.

use egui::{Context, Event, Id, Key, Modifiers, Pos2, RawInput};
use std::cell::RefCell;

/// What the menu does to the text of its box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Cut,
    Copy,
    Paste,
    SelectAll,
    Delete,
}

#[derive(Debug, Clone, Copy)]
struct Open {
    field: Id,
    at: Pos2,
    /// The box had text selected when the menu opened.
    selected: bool,
}

thread_local! {
    static OPEN: RefCell<Option<Open>> = const { RefCell::new(None) };
    static PENDING: RefCell<Option<(Id, Action)>> = const { RefCell::new(None) };
}

/// The events that make a text box with the focus do `action`, with
/// `clipboard` as the text a paste puts in.
pub fn events_for(action: Action, clipboard: Option<String>) -> Vec<Event> {
    let key = |key: Key, modifiers: Modifiers| Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    };
    match action {
        Action::Cut => vec![Event::Cut],
        Action::Copy => vec![Event::Copy],
        Action::Paste => clipboard
            .filter(|t| !t.is_empty())
            .map(|t| vec![Event::Paste(t)])
            .unwrap_or_default(),
        Action::SelectAll => vec![key(Key::A, Modifiers::COMMAND)],
        Action::Delete => vec![key(Key::Delete, Modifiers::NONE)],
    }
}

/// The command picked last frame reaches its box with this frame's input.
pub fn raw_input(raw: &mut RawInput) {
    let Some((_, action)) = PENDING.with(|p| *p.borrow()) else {
        return;
    };
    PENDING.with(|p| *p.borrow_mut() = None);
    let clipboard = if action == Action::Paste {
        crate::clipboard::system_text()
    } else {
        None
    };
    raw.events.extend(events_for(action, clipboard));
}

/// The text box under `pos`: the topmost widget there that keeps a text
/// box's state.
fn field_at(ctx: &Context, pos: Pos2) -> Option<Id> {
    let layer = ctx.layer_id_at(pos)?;
    let ids: Vec<Id> = ctx.viewport(|v| {
        v.this_pass
            .widgets
            .get_layer(layer)
            .filter(|w| w.interact_rect.contains(pos))
            .map(|w| w.id)
            .collect()
    });
    ids.into_iter()
        .rev()
        .find(|id| egui::TextEdit::load_state(ctx, *id).is_some())
}

/// Opens the menu on a right click over a text box that no menu of its
/// own answered, and shows it while open. Runs after the rest of the
/// frame.
pub fn show(ctx: &Context) {
    let (clicked, pos) = ctx.input(|i| (i.pointer.secondary_clicked(), i.pointer.interact_pos()));
    if clicked && !egui::Popup::is_any_open(ctx) {
        if let Some(field) = pos.and_then(|p| field_at(ctx, p)) {
            let selected = egui::TextEdit::load_state(ctx, field)
                .and_then(|s| s.cursor.char_range())
                .is_some_and(|r| !r.is_empty());
            ctx.memory_mut(|m| m.request_focus(field));
            OPEN.with(|o| {
                *o.borrow_mut() = Some(Open {
                    field,
                    at: pos.unwrap_or_default(),
                    selected,
                })
            });
            return;
        }
    }
    let Some(open) = OPEN.with(|o| *o.borrow()) else {
        return;
    };
    let mut picked: Option<Action> = None;
    let area = egui::Area::new(Id::new("text_field_menu"))
        .order(egui::Order::Foreground)
        .fixed_pos(open.at)
        .show(ctx, |ui| {
            egui::Frame::menu(ui.style()).show(ui, |ui| {
                ui.set_min_width(150.0);
                use crate::ui::menus::item;
                for (key, shortcut, action, enabled) in [
                    ("menu.edit.cut", "Ctrl+X", Action::Cut, open.selected),
                    ("menu.edit.copy", "Ctrl+C", Action::Copy, open.selected),
                    ("menu.edit.paste", "Ctrl+V", Action::Paste, true),
                    ("menu.edit.select_all", "Ctrl+A", Action::SelectAll, true),
                    ("menu.edit.delete", "Del", Action::Delete, open.selected),
                ] {
                    if item(ui, key, shortcut, enabled) {
                        picked = Some(action);
                    }
                }
            });
        });
    let outside = ctx.input(|i| {
        (i.pointer.any_pressed() || i.pointer.any_click())
            && i
                .pointer
                .interact_pos()
                .is_some_and(|p| !area.response.rect.contains(p))
    });
    if let Some(action) = picked {
        ctx.memory_mut(|m| m.request_focus(open.field));
        PENDING.with(|p| *p.borrow_mut() = Some((open.field, action)));
        ctx.request_repaint();
    }
    if picked.is_some() || outside || ctx.input(|i| i.key_pressed(Key::Escape)) {
        OPEN.with(|o| *o.borrow_mut() = None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_command_reaches_the_text_box_as_its_keys_would() {
        assert_eq!(events_for(Action::Cut, None), vec![Event::Cut]);
        assert_eq!(events_for(Action::Copy, None), vec![Event::Copy]);
        assert_eq!(
            events_for(Action::Paste, Some("abc".into())),
            vec![Event::Paste("abc".into())]
        );
        assert!(events_for(Action::Paste, None).is_empty());
        assert!(matches!(
            events_for(Action::SelectAll, None)[..],
            [Event::Key { key: Key::A, modifiers, .. }] if modifiers.command
        ));
        assert!(matches!(
            events_for(Action::Delete, None)[..],
            [Event::Key { key: Key::Delete, .. }]
        ));
    }

    #[test]
    fn a_right_click_on_a_text_box_opens_the_menu_and_its_paste_types_the_text() {
        let ctx = Context::default();
        let mut text = String::from("hello");
        let field = Id::new("field");
        let run = |ctx: &Context, text: &mut String, raw: RawInput| {
            let mut out = ctx.run_ui(raw, |ui| {
                egui::CentralPanel::default().show_inside(ui, |ui| {
                    ui.add(egui::TextEdit::singleline(text).id(field));
                });
                show(ui.ctx());
            });
            out.textures_delta.clear();
        };
        let screen = egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 300.0));
        let input = |events: Vec<Event>| RawInput {
            screen_rect: Some(screen),
            events,
            ..Default::default()
        };
        run(&ctx, &mut text, input(vec![]));
        let r = ctx.read_response(field).expect("the box").rect;
        let at = r.center();
        let click = |pressed| Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        run(
            &ctx,
            &mut text,
            input(vec![Event::PointerMoved(at), click(true), click(false)]),
        );
        assert!(OPEN.with(|o| o.borrow().is_some()), "the menu opens");
        assert!(ctx.memory(|m| m.has_focus(field)));
        // Select All then Delete, as the menu's items would ask.
        PENDING.with(|p| *p.borrow_mut() = Some((field, Action::SelectAll)));
        OPEN.with(|o| *o.borrow_mut() = None);
        let mut raw = input(vec![]);
        raw_input(&mut raw);
        run(&ctx, &mut text, raw);
        PENDING.with(|p| *p.borrow_mut() = Some((field, Action::Delete)));
        let mut raw = input(vec![]);
        raw_input(&mut raw);
        run(&ctx, &mut text, raw);
        assert_eq!(text, "");
        let mut raw = input(events_for(Action::Paste, Some("pasted".into())));
        raw_input(&mut raw);
        run(&ctx, &mut text, raw);
        assert_eq!(text, "pasted");
    }
}
