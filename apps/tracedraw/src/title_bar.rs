//! The window's title bar on Linux (#2070). On X11 the window manager
//! always draws it, and on a Wayland desktop that offers title bars to
//! applications (the `xdg-decoration` protocol: KDE Plasma, Sway and
//! most others) the windowing layer asks for the desktop's. A Wayland
//! compositor without that protocol (GNOME's) leaves the title bar to
//! the application, and the windowing layer draws a plain one of its
//! own; it answers a finger and a pen since #2070 (the patched winit,
//! vendor/README.md), but it does not look like the desktop's.
//!
//! Options > General > "Use the desktop's title bar" (`system_title_bar`)
//! opens the window through XWayland on such a desktop, from the next
//! start, where the desktop draws its own title bar, as for any X11
//! program; `--x11` does the same for one start. It is off by default:
//! through XWayland, a monitor scaled by a fraction shows the window
//! slightly blurred, and on a desktop that offers title bars it changes
//! nothing. See docs/behavior/about-and-startup.md.

/// Whether this start opens the window through X11 for the desktop's
/// title bar: asked for (`asked`) in a Wayland session (`wayland`) with
/// an X server to open it on (`display`), when the compositor offers no
/// title bars (`offers_title_bars`, asked only then; `None` when it
/// could not be asked, which keeps Wayland).
pub fn through_x11(
    asked: bool,
    wayland: bool,
    display: bool,
    offers_title_bars: impl FnOnce() -> Option<bool>,
) -> bool {
    asked && wayland && display && offers_title_bars() == Some(false)
}

/// Whether the environment names a display server `name` (set and not
/// empty), as the windowing layer reads `WAYLAND_DISPLAY` and `DISPLAY`.
#[cfg(target_os = "linux")]
pub fn env_set(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|v| !v.is_empty())
}

/// Whether the Wayland compositor offers title bars to applications (it
/// announces `zxdg_decoration_manager_v1`); `None` when it cannot be
/// reached. One connection and one round trip, made before the window
/// is.
#[cfg(target_os = "linux")]
pub fn compositor_offers_title_bars() -> Option<bool> {
    use wayland_client::globals::{registry_queue_init, GlobalListContents};
    use wayland_client::protocol::wl_registry::{self, WlRegistry};
    use wayland_client::{Connection, Dispatch, QueueHandle};

    struct Probe;
    impl Dispatch<WlRegistry, GlobalListContents> for Probe {
        fn event(
            _: &mut Self,
            _: &WlRegistry,
            _: wl_registry::Event,
            _: &GlobalListContents,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    let connection = Connection::connect_to_env().ok()?;
    let (globals, _queue) = registry_queue_init::<Probe>(&connection).ok()?;
    Some(globals.contents().with_list(|list| {
        list.iter()
            .any(|g| g.interface == "zxdg_decoration_manager_v1")
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The window goes through X11 for the desktop's title bar only when
    /// asked, on Wayland, with XWayland there, and when the compositor
    /// draws no title bars itself (#2070).
    #[test]
    fn x11_only_when_asked_and_the_compositor_draws_no_title_bar() {
        assert!(through_x11(true, true, true, || Some(false)));
        // Not asked: Wayland, with the windowing layer's own title bar.
        assert!(!through_x11(false, true, true, || Some(false)));
        // An X11 session already has the window manager's.
        assert!(!through_x11(true, false, true, || Some(false)));
        // No XWayland to open the window on.
        assert!(!through_x11(true, true, false, || Some(false)));
        // KDE Plasma, Sway...: the desktop draws it on Wayland already.
        assert!(!through_x11(true, true, true, || Some(true)));
        // A compositor that could not be asked keeps Wayland.
        assert!(!through_x11(true, true, true, || None));
    }

    /// The compositor is not asked when nothing would come of it.
    #[test]
    fn the_compositor_is_asked_only_when_it_matters() {
        let asked = std::cell::Cell::new(false);
        let probe = || {
            asked.set(true);
            Some(false)
        };
        assert!(!through_x11(false, true, true, probe));
        assert!(!asked.get());
    }

    /// The setting is off unless chosen, and kept (#2070).
    #[test]
    fn the_setting_is_off_by_default_and_kept() {
        let settings = crate::settings::Settings::default();
        assert!(!settings.system_title_bar);
        let mut on = settings.clone();
        on.system_title_bar = true;
        let text = serde_json::to_string(&on).expect("settings serialize");
        let back: crate::settings::Settings = serde_json::from_str(&text).expect("read back");
        assert!(back.system_title_bar);
    }
}
