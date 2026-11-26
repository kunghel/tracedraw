// TraceDraw: the application may stop egui-winit reading the clipboard's
// text for a paste shortcut (see vendor/README.md, #1488). Kept apart so
// that TraceDraw's own tests compile and run it
// (`clipboard::eframe_paste_gate::tests`): this copy is outside the
// workspace and its own tests run in no gate.
//
// egui-winit reads the clipboard as text on every paste shortcut, before
// the application sees the key. An X11 clipboard owner that holds only a
// picture may answer that text request with its picture in parts (INCR);
// the clipboard library stops after the first part, and the owner then
// waits for the next request of that transfer forever and answers nobody
// else, so the application's own picture read timed out and the window
// froze for the waits.

use std::sync::OnceLock;

/// The application's answer to "may the clipboard's text be read now?".
static GATE: OnceLock<fn() -> bool> = OnceLock::new();

/// Ask `gate` before each paste shortcut whether egui-winit may read the
/// clipboard's text. When it answers `false`, the key press is sent
/// directly to egui without egui-winit reading the clipboard. The
/// application handles it immediately with the modifiers held at the
/// press, even when Ctrl is released before V. Set once; later calls
/// are ignored.
#[allow(dead_code)]
pub fn set_paste_text_gate(gate: fn() -> bool) {
    let _ = GATE.set(gate);
}

/// Whether a key press is a paste shortcut as egui-winit tells it: the
/// logical key, or the physical one when the logical key is no key egui
/// knows (layouts without Latin letters), is V with Ctrl (Cmd), or the
/// Paste key. Shift+Insert counts on Windows only, as in egui-winit.
pub(crate) fn is_paste_key(
    logical: &winit::keyboard::Key,
    physical: winit::keyboard::PhysicalKey,
    command: bool,
    shift: bool,
) -> bool {
    match paste_key(logical, physical) {
        Some(egui::Key::Paste) => true,
        Some(egui::Key::V) => command,
        Some(egui::Key::Insert) => cfg!(target_os = "windows") && shift,
        _ => false,
    }
}

fn paste_key(
    logical: &winit::keyboard::Key,
    physical: winit::keyboard::PhysicalKey,
) -> Option<egui::Key> {
    use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};
    let from_logical = match logical {
        Key::Named(NamedKey::Paste) => Some(egui::Key::Paste),
        Key::Named(NamedKey::Insert) => Some(egui::Key::Insert),
        // Other named keys are no paste key; egui knows nearly all of them.
        Key::Named(_) => return None,
        Key::Character(s) => egui::Key::from_name(s.as_str()),
        Key::Unidentified(_) | Key::Dead(_) => None,
    };
    from_logical.or(match physical {
        PhysicalKey::Code(KeyCode::KeyV) => Some(egui::Key::V),
        PhysicalKey::Code(KeyCode::Paste) => Some(egui::Key::Paste),
        PhysicalKey::Code(KeyCode::Insert) => Some(egui::Key::Insert),
        _ => None,
    })
}

/// Physical keys that can carry the shortcut on letter and symbol layouts.
fn physical_paste_key(physical: winit::keyboard::PhysicalKey) -> Option<egui::Key> {
    use winit::keyboard::{KeyCode, PhysicalKey};
    let PhysicalKey::Code(code) = physical else {
        return None;
    };
    Some(match code {
        KeyCode::KeyA => egui::Key::A,
        KeyCode::KeyB => egui::Key::B,
        KeyCode::KeyC => egui::Key::C,
        KeyCode::KeyD => egui::Key::D,
        KeyCode::KeyE => egui::Key::E,
        KeyCode::KeyF => egui::Key::F,
        KeyCode::KeyG => egui::Key::G,
        KeyCode::KeyH => egui::Key::H,
        KeyCode::KeyI => egui::Key::I,
        KeyCode::KeyJ => egui::Key::J,
        KeyCode::KeyK => egui::Key::K,
        KeyCode::KeyL => egui::Key::L,
        KeyCode::KeyM => egui::Key::M,
        KeyCode::KeyN => egui::Key::N,
        KeyCode::KeyO => egui::Key::O,
        KeyCode::KeyP => egui::Key::P,
        KeyCode::KeyQ => egui::Key::Q,
        KeyCode::KeyR => egui::Key::R,
        KeyCode::KeyS => egui::Key::S,
        KeyCode::KeyT => egui::Key::T,
        KeyCode::KeyU => egui::Key::U,
        KeyCode::KeyV => egui::Key::V,
        KeyCode::KeyW => egui::Key::W,
        KeyCode::KeyX => egui::Key::X,
        KeyCode::KeyY => egui::Key::Y,
        KeyCode::KeyZ => egui::Key::Z,
        KeyCode::Comma => egui::Key::Comma,
        KeyCode::Period => egui::Key::Period,
        KeyCode::Semicolon => egui::Key::Semicolon,
        KeyCode::Slash => egui::Key::Slash,
        KeyCode::Backslash => egui::Key::Backslash,
        KeyCode::Minus => egui::Key::Minus,
        KeyCode::Equal => egui::Key::Equals,
        KeyCode::Quote => egui::Key::Quote,
        KeyCode::Backquote => egui::Key::Backtick,
        KeyCode::BracketLeft => egui::Key::OpenBracket,
        KeyCode::BracketRight => egui::Key::CloseBracket,
        KeyCode::Paste => egui::Key::Paste,
        KeyCode::Insert => egui::Key::Insert,
        _ => return None,
    })
}

/// A gated shortcut keeps the original key press without asking for text.
fn gated_key_press(
    logical: &winit::keyboard::Key,
    physical: winit::keyboard::PhysicalKey,
    held: winit::keyboard::ModifiersState,
    pressed: bool,
    repeat: bool,
    may_read: bool,
) -> Option<egui::Event> {
    let modifiers = egui::Modifiers {
        alt: held.alt_key(),
        ctrl: held.control_key(),
        shift: held.shift_key(),
        mac_cmd: cfg!(target_os = "macos") && held.super_key(),
        command: if cfg!(target_os = "macos") {
            held.super_key()
        } else {
            held.control_key()
        },
    };
    if !pressed || may_read || !is_paste_key(logical, physical, modifiers.command, modifiers.shift)
    {
        return None;
    }
    Some(egui::Event::Key {
        key: paste_key(logical, physical)?,
        physical_key: physical_paste_key(physical),
        pressed: true,
        repeat,
        modifiers,
    })
}

thread_local! {
    /// The modifier keys held, as the window's events last gave them.
    static MODIFIERS: std::cell::Cell<winit::keyboard::ModifiersState> =
        const { std::cell::Cell::new(winit::keyboard::ModifiersState::empty()) };
}

/// The real paste key press the application's gate keeps from
/// egui-winit's clipboard read. Every window event goes through here, so the
/// modifier keys are followed as egui-winit follows them.
#[allow(dead_code)]
pub(crate) fn blocked_key(event: &winit::event::WindowEvent) -> Option<egui::Event> {
    use winit::event::WindowEvent;
    let event = match event {
        WindowEvent::ModifiersChanged(m) => {
            MODIFIERS.with(|cell| cell.set(m.state()));
            return None;
        }
        WindowEvent::KeyboardInput {
            event,
            is_synthetic: false,
            ..
        } => event,
        _ => return None,
    };
    if event.state != winit::event::ElementState::Pressed || GATE.get().is_none() {
        return None;
    }
    let held = MODIFIERS.with(|cell| cell.get());
    // Cmd on macOS, Ctrl elsewhere, as egui's `command`.
    let command = if cfg!(target_os = "macos") {
        held.super_key()
    } else {
        held.control_key()
    };
    if !is_paste_key(
        &event.logical_key,
        event.physical_key,
        command,
        held.shift_key(),
    ) {
        return None;
    }
    gated_key_press(
        &event.logical_key,
        event.physical_key,
        held,
        true,
        event.repeat,
        GATE.get().is_none_or(|may_read| may_read()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::{Key, KeyCode, NamedKey, NativeKeyCode, PhysicalKey, SmolStr};

    fn ch(s: &str) -> Key {
        Key::Character(SmolStr::new(s))
    }

    #[test]
    fn a_gated_paste_keeps_the_press_and_its_modifiers_without_reading_text() {
        let physical = PhysicalKey::Code(KeyCode::KeyV);
        let command = if cfg!(target_os = "macos") {
            winit::keyboard::ModifiersState::SUPER
        } else {
            winit::keyboard::ModifiersState::CONTROL
        };
        let held = command | winit::keyboard::ModifiersState::SHIFT;
        let event = gated_key_press(&ch("V"), physical, held, true, false, false)
            .expect("the gated shortcut keeps its press");
        assert!(matches!(event, egui::Event::Key {
            key: egui::Key::V,
            physical_key: Some(egui::Key::V),
            pressed: true,
            repeat: false,
            modifiers,
        } if modifiers.command && modifiers.shift));
        assert!(gated_key_press(&ch("v"), physical, command, false, false, false).is_none());
        assert!(gated_key_press(&ch("v"), physical, command, true, false, true).is_none());
        assert!(gated_key_press(
            &ch("v"),
            physical,
            winit::keyboard::ModifiersState::empty(),
            true,
            false,
            false
        )
        .is_none());
    }

    #[test]
    fn a_gated_shortcut_keeps_the_layout_and_repeat_information() {
        let held = if cfg!(target_os = "macos") {
            winit::keyboard::ModifiersState::SUPER
        } else {
            winit::keyboard::ModifiersState::CONTROL
        };
        let physical_v = PhysicalKey::Code(KeyCode::KeyV);
        assert!(matches!(
            gated_key_press(&ch("м"), physical_v, held, true, true, false),
            Some(egui::Event::Key {
                key: egui::Key::V,
                physical_key: Some(egui::Key::V),
                repeat: true,
                ..
            })
        ));
        assert!(matches!(
            gated_key_press(
                &ch("v"),
                PhysicalKey::Code(KeyCode::KeyB),
                held,
                true,
                false,
                false
            ),
            Some(egui::Event::Key {
                key: egui::Key::V,
                physical_key: Some(egui::Key::B),
                ..
            })
        ));
        assert!(gated_key_press(&ch("b"), physical_v, held, true, false, false).is_none());
        assert!(matches!(
            gated_key_press(
                &Key::Named(NamedKey::Paste),
                PhysicalKey::Code(KeyCode::Paste),
                held,
                true,
                false,
                false
            ),
            Some(egui::Event::Key {
                key: egui::Key::Paste,
                ..
            })
        ));
    }

    #[test]
    fn paste_keys_are_told_as_egui_winit_tells_them() {
        let v = PhysicalKey::Code(KeyCode::KeyV);
        assert!(is_paste_key(&ch("v"), v, true, false));
        assert!(is_paste_key(&ch("V"), v, true, true));
        // V without Ctrl is typing.
        assert!(!is_paste_key(&ch("v"), v, false, false));
        // The Paste key, with or without modifiers.
        let paste = PhysicalKey::Code(KeyCode::Paste);
        assert!(is_paste_key(
            &Key::Named(NamedKey::Paste),
            paste,
            false,
            false
        ));
        // A layout without Latin letters: the physical V key decides.
        assert!(is_paste_key(&ch("м"), v, true, false));
        assert!(!is_paste_key(&ch("м"), v, false, false));
        // A Latin layout where V sits elsewhere: the logical key decides.
        let b = PhysicalKey::Code(KeyCode::KeyB);
        assert!(is_paste_key(&ch("v"), b, true, false));
        assert!(!is_paste_key(&ch("b"), v, true, false));
        // Other keys and named keys never.
        assert!(!is_paste_key(
            &ch("c"),
            PhysicalKey::Code(KeyCode::KeyC),
            true,
            false
        ));
        assert!(!is_paste_key(&Key::Named(NamedKey::Enter), v, true, false));
        let unknown = PhysicalKey::Unidentified(NativeKeyCode::Xkb(0));
        assert!(!is_paste_key(&ch("x"), unknown, true, false));
        // Shift+Insert pastes on Windows only.
        let insert = PhysicalKey::Code(KeyCode::Insert);
        assert_eq!(
            is_paste_key(&Key::Named(NamedKey::Insert), insert, false, true),
            cfg!(target_os = "windows")
        );
        assert!(!is_paste_key(
            &Key::Named(NamedKey::Insert),
            insert,
            false,
            false
        ));
    }
}
