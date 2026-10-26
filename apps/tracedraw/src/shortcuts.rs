//! Keyboard shortcuts as text ("Ctrl+Shift+E", "F5", "Alt+Enter"), the
//! way the menus show them and the preferences keep them
//! (`Settings::shortcuts`: a tool id or a menu item's key, and its
//! shortcut). Menu commands with a shortcut of the user's run through
//! `ui::menus::run_command`.

use egui::{Key, Modifiers};

/// The name a key goes by in shortcut text.
pub fn key_name(key: Key) -> String {
    match key {
        Key::PageUp => "PgUp".into(),
        Key::PageDown => "PgDn".into(),
        Key::Plus => "+".into(),
        Key::Minus => "-".into(),
        Key::Equals => "=".into(),
        Key::Escape => "Esc".into(),
        Key::ArrowLeft => "Left".into(),
        Key::ArrowRight => "Right".into(),
        Key::ArrowUp => "Up".into(),
        Key::ArrowDown => "Down".into(),
        other => other.name().to_string(),
    }
}

fn key_from_name(name: &str) -> Option<Key> {
    Some(match name {
        "PgUp" => Key::PageUp,
        "PgDn" => Key::PageDown,
        "+" => Key::Plus,
        "-" => Key::Minus,
        "=" => Key::Equals,
        "Esc" => Key::Escape,
        "Left" => Key::ArrowLeft,
        "Right" => Key::ArrowRight,
        "Up" => Key::ArrowUp,
        "Down" => Key::ArrowDown,
        other => {
            return Key::from_name(other).or_else(|| Key::from_name(&other.to_ascii_uppercase()))
        }
    })
}

/// Shortcut text: Ctrl, Alt and Shift in that order, then the key.
pub fn format(mods: Modifiers, key: Key) -> String {
    let mut s = String::new();
    if mods.command || mods.ctrl {
        s.push_str("Ctrl+");
    }
    if mods.alt {
        s.push_str("Alt+");
    }
    if mods.shift {
        s.push_str("Shift+");
    }
    s.push_str(&key_name(key));
    s
}

/// The modifiers and key of shortcut text; `None` when it names no key.
pub fn parse(text: &str) -> Option<(Modifiers, Key)> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    // "Ctrl++": the key itself is a plus.
    let (mods_part, key_part) = match text.strip_suffix("++") {
        Some(rest) => (rest, "+"),
        None => match text.rfind('+') {
            Some(i) if i + 1 < text.len() => (&text[..i], &text[i + 1..]),
            _ => ("", text),
        },
    };
    let mut mods = Modifiers::NONE;
    for m in mods_part
        .split('+')
        .map(str::trim)
        .filter(|m| !m.is_empty())
    {
        match m.to_ascii_lowercase().as_str() {
            "ctrl" | "cmd" | "command" => mods |= Modifiers::COMMAND,
            "alt" | "option" => mods.alt = true,
            "shift" => mods.shift = true,
            _ => return None,
        }
    }
    Some((mods, key_from_name(key_part.trim())?))
}

/// Whether two shortcut texts name the same keys.
pub fn same(a: &str, b: &str) -> bool {
    match (parse(a), parse(b)) {
        (Some((ma, ka)), Some((mb, kb))) => {
            ka == kb
                && (ma.command || ma.ctrl) == (mb.command || mb.ctrl)
                && ma.alt == mb.alt
                && ma.shift == mb.shift
        }
        _ => false,
    }
}

/// The keys that run `id` (a menu command's key or a tool's id): the
/// user's keys for it when there are any (a command may have several,
/// #822), else its `built_in` keys. An empty entry of the user's means
/// the built-in keys were deleted.
pub fn keys_for(user: &[(String, String)], id: &str, built_in: Option<&str>) -> Vec<String> {
    let mut mine = user.iter().filter(|(k, _)| k == id).peekable();
    if mine.peek().is_none() {
        return built_in
            .filter(|b| !b.is_empty())
            .map(|b| vec![b.to_string()])
            .unwrap_or_default();
    }
    mine.filter(|(_, v)| !v.is_empty())
        .map(|(_, v)| v.clone())
        .collect()
}

/// Shift, Ctrl, Alt and the system key pressed on their own (they arrive
/// as key presses too, before the key they modify).
pub fn is_modifier(key: Key) -> bool {
    matches!(
        key,
        Key::ShiftLeft
            | Key::ShiftRight
            | Key::ControlLeft
            | Key::ControlRight
            | Key::AltLeft
            | Key::AltRight
            | Key::SuperLeft
            | Key::SuperRight
    )
}

/// Whether a key press can be a shortcut: not a modifier alone, and not
/// a key that edits text, moves focus or closes dialogs without Ctrl or
/// Alt.
pub fn allowed(mods: Modifiers, key: Key) -> bool {
    let bare = !(mods.command || mods.ctrl || mods.alt);
    !is_modifier(key)
        && !(bare
            && matches!(
                key,
                Key::Escape | Key::Tab | Key::Enter | Key::Backspace | Key::Space
            ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcut_text_round_trips() {
        let ctrl_shift = Modifiers::COMMAND | Modifiers::SHIFT;
        assert_eq!(format(ctrl_shift, Key::E), "Ctrl+Shift+E");
        assert_eq!(format(Modifiers::ALT, Key::Enter), "Alt+Enter");
        assert_eq!(format(Modifiers::COMMAND, Key::PageDown), "Ctrl+PgDn");
        assert_eq!(format(Modifiers::NONE, Key::F5), "F5");
        for text in [
            "Ctrl+Shift+E",
            "Alt+Enter",
            "Ctrl+PgDn",
            "F5",
            "Ctrl++",
            "Alt+Shift+A",
            "Shift+F12",
        ] {
            let (m, k) = parse(text).expect(text);
            assert_eq!(format(m, k), text);
        }
        assert_eq!(parse("Ctrl++").map(|(_, k)| k), Some(Key::Plus));
        assert!(same("ctrl+shift+e", "Ctrl+Shift+E"));
        assert!(!same("Ctrl+E", "Ctrl+Shift+E"));
        assert!(
            parse("Hyper+E").is_none() && parse("").is_none() && parse("Ctrl+Nothing").is_none()
        );
        assert!(!allowed(Modifiers::NONE, Key::Escape));
        assert!(allowed(Modifiers::COMMAND, Key::Enter) && allowed(Modifiers::NONE, Key::F9));
        assert!(
            !allowed(Modifiers::ALT, Key::AltLeft)
                && !allowed(Modifiers::COMMAND, Key::ControlRight)
        );
    }
}
