//! A right click on any text box (a font's name, a size, a field of a
//! dialog box), or a finger resting on it on a touch screen (#4262),
//! opens Cut, Copy, Paste, Select All and Delete for its text (#3798). The box is found under the pointer among the frame's widgets
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

/// How long a finger rests on a text box, seconds, before its menu opens
/// as a right click would open it (#4262).
const LONG_PRESS: f64 = 0.5;

/// How far a finger may wander during a long press, points.
const LONG_PRESS_SLOP: f32 = 8.0;

thread_local! {
    /// When the press that already opened the menu began.
    static LONG_FIRED: RefCell<Option<f64>> = const { RefCell::new(None) };
}

/// A finger has rested on a text box at `pos` long enough: a touch
/// screen's right click (#4262). True once per press.
fn long_press(ctx: &Context, pos: Option<Pos2>) -> bool {
    let (touch, down, start, still, time) = ctx.input(|i| {
        let p = &i.pointer;
        let still = p
            .press_origin()
            .zip(p.interact_pos())
            .is_some_and(|(a, b)| a.distance(b) < LONG_PRESS_SLOP);
        (
            i.any_touches() || cfg!(target_os = "android"),
            p.primary_down(),
            p.press_start_time(),
            still,
            i.time,
        )
    });
    if !(touch && down && still) {
        if !down {
            LONG_FIRED.with(|f| *f.borrow_mut() = None);
        }
        return false;
    }
    let Some(start) = start else {
        return false;
    };
    if LONG_FIRED.with(|f| *f.borrow()) == Some(start) {
        return false;
    }
    if !pos.is_some_and(|p| field_at(ctx, p).is_some()) {
        return false;
    }
    let held = time - start;
    if held < LONG_PRESS {
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(LONG_PRESS - held));
        return false;
    }
    LONG_FIRED.with(|f| *f.borrow_mut() = Some(start));
    true
}

/// Opens the menu on a right click over a text box that no menu of its
/// own answered, and shows it while open. Runs after the rest of the
/// frame.
pub fn show(ctx: &Context) {
    let (right, pos) = ctx.input(|i| (i.pointer.secondary_clicked(), i.pointer.interact_pos()));
    let clicked = right || long_press(ctx, pos);
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

    /// #4262: the font size box of the property bar (a number field on a
    /// bar whose empty part has a menu of its own) takes the menu too,
    /// and its Select All, Delete and Paste reach its text.
    #[test]
    fn a_number_field_on_a_bar_takes_the_menu() {
        let ctx = Context::default();
        let mut value = 12.0_f64;
        let field_rect = std::cell::Cell::new(egui::Rect::NOTHING);
        let run = |ctx: &Context, value: &mut f64, raw: RawInput| {
            let mut out = ctx.run_ui(raw, |ui| {
                egui::CentralPanel::default().show_inside(ui, |ui| {
                    let bg = ui.interact(ui.max_rect(), Id::new("bar_bg"), egui::Sense::click());
                    egui::Popup::context_menu(&bg).show(|ui| {
                        ui.label("bar menu");
                    });
                    ui.horizontal(|ui| {
                        let r = ui.add(crate::ui::field::NumField::new(value).width(62.0));
                        field_rect.set(r.rect);
                    });
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
        OPEN.with(|o| *o.borrow_mut() = None);
        run(&ctx, &mut value, input(vec![]));
        let at = field_rect.get().left_center() + egui::vec2(10.0, 0.0);
        let click = |pressed| Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        run(
            &ctx,
            &mut value,
            input(vec![Event::PointerMoved(at), click(true), click(false)]),
        );
        assert!(OPEN.with(|o| o.borrow().is_some()), "the menu opens");
        let field = OPEN.with(|o| o.borrow().map(|o| o.field)).unwrap();
        OPEN.with(|o| *o.borrow_mut() = None);
        for action in [Action::SelectAll, Action::Delete] {
            PENDING.with(|p| *p.borrow_mut() = Some((field, action)));
            let mut raw = input(vec![]);
            raw_input(&mut raw);
            run(&ctx, &mut value, raw);
        }
        let mut raw = input(events_for(Action::Paste, Some("36".into())));
        raw_input(&mut raw);
        run(&ctx, &mut value, raw);
        let mut raw = input(vec![Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }]);
        raw_input(&mut raw);
        run(&ctx, &mut value, raw);
        assert_eq!(value, 36.0);
    }

    /// #4262: on a touch screen a finger resting on a text box opens the
    /// menu, once for the press; a short tap does not.
    #[test]
    fn a_long_press_on_a_text_box_opens_the_menu() {
        let ctx = Context::default();
        let mut text = String::from("hello");
        let field = Id::new("touch_field");
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
        let input = |time: f64, events: Vec<Event>| RawInput {
            screen_rect: Some(screen),
            time: Some(time),
            events,
            ..Default::default()
        };
        OPEN.with(|o| *o.borrow_mut() = None);
        LONG_FIRED.with(|f| *f.borrow_mut() = None);
        run(&ctx, &mut text, input(0.0, vec![]));
        let at = ctx.read_response(field).expect("the box").rect.center();
        let touch = |phase| Event::Touch {
            device_id: egui::TouchDeviceId(1),
            id: egui::TouchId(1),
            phase,
            pos: at,
            force: None,
        };
        let button = |pressed| Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        run(
            &ctx,
            &mut text,
            input(1.0, vec![touch(egui::TouchPhase::Start), Event::PointerMoved(at), button(true)]),
        );
        run(&ctx, &mut text, input(1.2, vec![]));
        assert!(OPEN.with(|o| o.borrow().is_none()), "not yet");
        run(&ctx, &mut text, input(1.7, vec![]));
        assert!(OPEN.with(|o| o.borrow().is_some()), "the menu opens");
        OPEN.with(|o| *o.borrow_mut() = None);
        run(&ctx, &mut text, input(1.9, vec![]));
        assert!(OPEN.with(|o| o.borrow().is_none()), "once for the press");
        run(
            &ctx,
            &mut text,
            input(2.0, vec![touch(egui::TouchPhase::End), button(false)]),
        );
        assert!(OPEN.with(|o| o.borrow().is_none()));
    }
}
