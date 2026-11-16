//! Object > Insert > Insert Object (#1025): programs the user adds once
//! (an equation editor, a chart maker, any program that copies or saves
//! pictures), each opened from the menu with one click. What the program
//! makes comes in through the clipboard (Paste Object, Paste Special) or
//! a saved file (From File), as any picture or drawing does. Embedding a
//! live object of another program is not offered: the drawing keeps what
//! was pasted. See docs/behavior/insert-object.md.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// At most this many programs are kept.
pub const MAX_OBJECT_APPS: usize = 30;

/// A program of Insert Object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectApp {
    /// The name shown in the menu.
    pub name: String,
    /// The program: an executable, or on macOS an application bundle.
    pub path: String,
}

impl ObjectApp {
    /// The program at `path`, named after its file without the extension
    /// ("Formula" for `C:\Program Files\Formula\Formula.exe`).
    pub fn from_path(path: &Path) -> Self {
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| path.display().to_string());
        Self {
            name,
            path: path.display().to_string(),
        }
    }

    /// The program and arguments that start it: the program itself, or
    /// on macOS an application bundle through `open -a`.
    pub fn command(&self) -> (String, Vec<String>) {
        let path = self.path.trim().trim_matches('"');
        if cfg!(target_os = "macos") && path.trim_end_matches('/').ends_with(".app") {
            return ("open".into(), vec!["-a".into(), path.into()]);
        }
        (path.into(), Vec::new())
    }
}

/// Programs can be started here: not in a browser, and not on Android,
/// whose apps are not programs to name.
pub const OFFERED: bool = !crate::files::WEB && !crate::files::ANDROID;

impl crate::app::App {
    /// Insert Object > Add Program: pick a program and add it to the list
    /// (once; the same path again is not added twice).
    pub fn add_object_app(&mut self) {
        crate::files::Dialog::new().pick_file(self, |app, path| {
            app.add_object_app_path(&path);
        });
    }

    pub fn add_object_app_path(&mut self, path: &Path) {
        let a = ObjectApp::from_path(path);
        if self.settings.object_apps.iter().any(|x| x.path == a.path) {
            return;
        }
        self.settings.object_apps.push(a);
        self.settings.object_apps.truncate(MAX_OBJECT_APPS);
        self.settings.save();
    }

    pub fn remove_object_app(&mut self, i: usize) {
        if i < self.settings.object_apps.len() {
            self.settings.object_apps.remove(i);
            self.settings.save();
        }
    }

    /// Start the program `i` of the list; the status bar says how what it
    /// makes comes in.
    pub fn open_object_app(&mut self, i: usize) {
        let Some(a) = self.settings.object_apps.get(i).cloned() else {
            return;
        };
        let started = start(&a);
        self.status = match started {
            Ok(()) => crate::i18n::tr("status.object_app_opened").replace("{name}", &a.name),
            Err(e) => crate::i18n::tr("status.object_app_failed")
                .replace("{name}", &a.name)
                .replace("{e}", &e),
        };
    }
}

#[cfg(all(not(test), not(target_arch = "wasm32"), not(target_os = "android")))]
fn start(a: &ObjectApp) -> Result<(), String> {
    let (program, args) = a.command();
    std::process::Command::new(&program)
        .args(&args)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Tests start nothing; elsewhere there is nothing to start.
#[cfg(any(test, target_arch = "wasm32", target_os = "android"))]
fn start(a: &ObjectApp) -> Result<(), String> {
    if a.path.trim().is_empty() {
        Err("no program".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn programs_are_named_after_their_file() {
        let a = ObjectApp::from_path(Path::new("/opt/Formula/Formula.exe"));
        assert_eq!(a.name, "Formula");
        assert_eq!(a.path, "/opt/Formula/Formula.exe");
        let (program, args) = a.command();
        assert_eq!(program, "/opt/Formula/Formula.exe");
        assert!(args.is_empty());
    }

    #[test]
    fn programs_are_added_once_opened_and_removed() {
        let mut app = crate::app::App::headless();
        app.settings.object_apps.clear();
        app.add_object_app_path(Path::new("/opt/eq/Equations"));
        app.add_object_app_path(Path::new("/opt/chart/Charts.bin"));
        app.add_object_app_path(Path::new("/opt/eq/Equations"));
        let names: Vec<&str> = app
            .settings
            .object_apps
            .iter()
            .map(|a| a.name.as_str())
            .collect();
        assert_eq!(names, ["Equations", "Charts"]);
        app.open_object_app(1);
        assert!(app.status.contains("Charts"), "{}", app.status);
        app.remove_object_app(0);
        assert_eq!(app.settings.object_apps.len(), 1);
        assert_eq!(app.settings.object_apps[0].name, "Charts");
    }
}
