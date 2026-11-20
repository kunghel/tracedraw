//! Object > Insert > Insert Object (#1025): programs the user adds once
//! (an equation editor, a chart maker, any program that copies or saves
//! pictures), each opened from the menu with one click. What the program
//! makes comes in through the clipboard (Paste Object, Paste Special) or
//! a saved file (From File), as any picture or drawing does. Embedding a
//! live object of another program is not offered: the drawing keeps what
//! was pasted. With Insert What It Saves on (#3391, off by default) the
//! program gets a blank picture to work on, and what it saves there comes
//! in by itself. See docs/behavior/insert-object.md.

use std::path::{Path, PathBuf};

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
        self.command_with(None)
    }

    /// The same, handed the file `file` to open (Insert What It Saves).
    pub fn command_with(&self, file: Option<&Path>) -> (String, Vec<String>) {
        let path = self.path.trim().trim_matches('"');
        let file: Vec<String> = file.map(|f| f.display().to_string()).into_iter().collect();
        if cfg!(target_os = "macos") && path.trim_end_matches('/').ends_with(".app") {
            let mut args = vec!["-a".into(), path.into()];
            args.extend(file);
            return ("open".into(), args);
        }
        (path.into(), file)
    }
}

/// The blank picture of Insert What It Saves: white, this many pixels
/// across and down (a page width at 96 dpi, give or take).
pub const BLANK_SIZE: (u32, u32) = (800, 600);

/// Write a blank white PNG for the program `name` in `dir`, under a name
/// of its own (`Formula-object-3.png`), and give its path.
pub fn write_blank(dir: &Path, name: &str) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let stem: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let path = (1..)
        .map(|n| dir.join(format!("{stem}-object-{n}.png")))
        .find(|p| !p.exists())
        .expect("a free name");
    let img = image::RgbaImage::from_pixel(BLANK_SIZE.0, BLANK_SIZE.1, image::Rgba([255; 4]));
    img.save_with_format(&path, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(path)
}

fn modified(path: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
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
    /// makes comes in. With Insert What It Saves on, the program opens a
    /// blank picture, watched until it is saved (`watch_object_files`).
    pub fn open_object_app(&mut self, i: usize) {
        let Some(a) = self.settings.object_apps.get(i).cloned() else {
            return;
        };
        let blank = if self.settings.object_apps_insert_saved {
            let dir = std::env::temp_dir().join("tracedraw-objects");
            match write_blank(&dir, &a.name) {
                Ok(p) => Some(p),
                Err(e) => {
                    self.status = crate::i18n::tr("status.object_app_failed")
                        .replace("{name}", &a.name)
                        .replace("{e}", &e);
                    return;
                }
            }
        } else {
            None
        };
        let started = start(&a, blank.as_deref());
        self.status = match started {
            Ok(()) => match blank {
                Some(p) => {
                    let at = modified(&p);
                    self.object_files.push((p, at));
                    crate::i18n::tr("status.object_app_opened_saved").replace("{name}", &a.name)
                }
                None => crate::i18n::tr("status.object_app_opened").replace("{name}", &a.name),
            },
            Err(e) => crate::i18n::tr("status.object_app_failed")
                .replace("{name}", &a.name)
                .replace("{e}", &e),
        };
    }

    /// Insert What It Saves: a blank picture whose time stamp moved on
    /// was saved by its program; it comes in at the page centre, selected
    /// and linked to its file, so every later save shows here as an edited
    /// bitmap's does (`watch_linked_bitmaps`). A picture gone from the disk
    /// is let go.
    pub fn watch_object_files(&mut self) {
        if self.object_files.is_empty() {
            return;
        }
        let files = std::mem::take(&mut self.object_files);
        for (path, at) in files {
            let now = modified(&path);
            if now.is_none() {
                continue;
            }
            if now == at {
                self.object_files.push((path, at));
                continue;
            }
            self.insert_saved_object(&path);
        }
    }

    fn insert_saved_object(&mut self, path: &Path) {
        let before = self.engine.revision();
        self.import_bitmap(path);
        if self.engine.revision() == before {
            return;
        }
        let Some(&id) = self.selection.first() else {
            return;
        };
        let linked = self
            .doc()
            .find_shape(id)
            .is_some_and(|s| s.data.iter().any(|(k, _)| k == crate::package::LINK_KEY));
        if !linked {
            let mut data = self
                .doc()
                .find_shape(id)
                .map(|s| s.data.clone())
                .unwrap_or_default();
            data.push((crate::package::LINK_KEY.into(), path.display().to_string()));
            let _ = self
                .engine
                .amend(&tracedraw_core::Command::SetObjectData { shape: id, data });
        }
    }
}

#[cfg(all(not(test), not(target_arch = "wasm32"), not(target_os = "android")))]
fn start(a: &ObjectApp, file: Option<&Path>) -> Result<(), String> {
    let (program, args) = a.command_with(file);
    std::process::Command::new(&program)
        .args(&args)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Tests start nothing; elsewhere there is nothing to start.
#[cfg(any(test, target_arch = "wasm32", target_os = "android"))]
fn start(a: &ObjectApp, _file: Option<&Path>) -> Result<(), String> {
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

    /// Insert What It Saves (#3391): the program opens a blank picture;
    /// nothing comes in until it is saved, then the saved picture comes
    /// in once, selected and linked to its file.
    #[test]
    fn what_the_program_saves_comes_into_the_drawing() {
        if crate::files::WEB {
            return;
        }
        let mut app = crate::app::App::headless();
        app.settings.object_apps.clear();
        app.add_object_app_path(Path::new("/opt/paint/Paint Pro.exe"));
        let shapes = |app: &crate::app::App| {
            app.doc()
                .all_layers()
                .map(|l| l.shapes.len())
                .sum::<usize>()
        };
        let n = shapes(&app);
        app.settings.object_apps_insert_saved = true;
        app.open_object_app(0);
        assert_eq!(app.object_files.len(), 1, "{}", app.status);
        let path = app.object_files[0].0.clone();
        let file = path
            .file_name()
            .expect("name")
            .to_string_lossy()
            .into_owned();
        assert!(file.starts_with("Paint_Pro-object-"), "{file}");
        let (w, h) = image::image_dimensions(&path).expect("a picture");
        assert_eq!((w, h), BLANK_SIZE);
        app.watch_object_files();
        assert_eq!(shapes(&app), n, "not saved yet: nothing comes in");
        // The program saves: a 4 x 3 picture, its time stamp moved on.
        image::RgbaImage::from_pixel(4, 3, image::Rgba([200, 0, 0, 255]))
            .save(&path)
            .expect("saved");
        let later = std::time::SystemTime::now() + std::time::Duration::from_secs(2);
        std::fs::File::options()
            .write(true)
            .open(&path)
            .and_then(|f| f.set_modified(later))
            .expect("time");
        app.watch_object_files();
        assert_eq!(shapes(&app), n + 1);
        assert!(app.object_files.is_empty());
        let id = app.selection[0];
        let s = app.doc().find_shape(id).expect("picture");
        let tracedraw_core::ShapeKind::Bitmap { width_px, .. } = &s.kind else {
            panic!("a picture");
        };
        assert_eq!(*width_px, 4);
        let link = s.data.iter().find(|(k, _)| k == crate::package::LINK_KEY);
        assert_eq!(
            link.map(|(_, v)| v.as_str()),
            Some(path.display().to_string().as_str())
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn the_blank_picture_is_handed_to_the_program() {
        let a = ObjectApp::from_path(Path::new("/opt/Formula/Formula.exe"));
        let (program, args) = a.command_with(Some(Path::new("/tmp/x/Formula-object-1.png")));
        assert_eq!(program, "/opt/Formula/Formula.exe");
        assert_eq!(args, ["/tmp/x/Formula-object-1.png"]);
    }
}
