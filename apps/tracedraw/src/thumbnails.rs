//! Small pictures of recent drawings for the Welcome Screen: the first
//! page with what lies beside it on the desktop (#2357), at most 160 pixels on its long side, kept as PNG in the
//! preferences folder (`thumbnails/<hash of the path>.png`) when a drawing
//! is opened or saved. The browser keeps no recent drawings, so none are
//! made there.

use crate::app::App;
use std::path::{Path, PathBuf};

/// The long side of a thumbnail, pixels.
const SIZE: f64 = 160.0;

/// The thumbnail's file name: an FNV-1a hash of the drawing's path, the
/// same from one run to the next.
fn file_name(path: &Path) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in path.to_string_lossy().as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}.png")
}

fn folder() -> Option<PathBuf> {
    crate::settings::config_dir().map(|d| d.join("thumbnails"))
}

/// Where the thumbnail of the drawing at `path` is kept.
pub fn thumbnail_path(path: &Path) -> Option<PathBuf> {
    folder().map(|d| d.join(file_name(path)))
}

/// Forget the thumbnail of one drawing (it left the recent list).
pub fn forget(path: &Path) {
    if crate::files::WEB {
        return;
    }
    if let Some(p) = thumbnail_path(path) {
        if crate::files::exists(&p) {
            if let Err(e) = crate::files::remove(&p) {
                log::warn!("could not remove the thumbnail of {}: {e}", path.display());
            }
        }
    }
}

/// Forget every thumbnail (the recent list was cleared).
pub fn clear() {
    if crate::files::WEB {
        return;
    }
    if let Some(dir) = folder() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// The first page of the active drawing as a PNG thumbnail, with what
/// lies beside it on the desktop (#2357).
fn first_page_png(app: &App) -> Option<Vec<u8>> {
    page_png(app.doc())
}

/// The first page of `doc` as a PNG thumbnail.
fn page_png(doc: &tracedraw_core::Document) -> Option<Vec<u8>> {
    let page = doc.pages.first()?;
    let pm = tracedraw_render::render_whole_page_image(doc, page.id, SIZE)?;
    // The background is opaque white, so premultiplied data is plain RGBA.
    let img = image::RgbaImage::from_raw(pm.width(), pm.height(), pm.data().to_vec())?;
    crate::bitmap_fx::encode(&img)
}

impl App {
    /// Keep a thumbnail of the active drawing for the recent list entry
    /// `path`; the picture shown for it is drawn again.
    /// Take one drawing off the recent documents list, with its
    /// thumbnail; the others stay (#1152). The file itself is left alone.
    pub fn remove_recent(&mut self, path: &Path) {
        let before = self.settings.recent_files.len();
        self.settings.recent_files.retain(|p| p != path);
        if self.settings.recent_files.len() != before {
            self.settings.save();
        }
        self.recent_thumbs.remove(path);
        forget(path);
    }

    pub fn store_thumbnail(&mut self, path: &Path) {
        if crate::files::WEB {
            return;
        }
        self.recent_thumbs.remove(path);
        let Some(out) = thumbnail_path(path) else {
            return;
        };
        // Drawn on a thread of its own (#3914): a drawing with a picture
        // of some hundred million pixels took the whole picture apart
        // for 160 pixels before its first frame showed.
        let doc = self.doc().clone();
        let path = path.to_path_buf();
        let job = move || {
            let Some(png) = page_png(&doc) else {
                return;
            };
            if let Err(e) = crate::files::write_creating(&out, png) {
                log::warn!("could not keep a thumbnail for {}: {e}", path.display());
            }
        };
        if cfg!(test) {
            job();
        } else if let Err(e) = std::thread::Builder::new()
            .name("thumbnail".into())
            .spawn(job)
        {
            log::warn!("no thread for a thumbnail: {e}");
        }
    }

    /// The thumbnail of the recent drawing `path` as a texture, loaded once;
    /// `None` when there is none.
    pub fn recent_thumbnail(
        &mut self,
        ctx: &egui::Context,
        path: &Path,
    ) -> Option<egui::TextureHandle> {
        if let Some(t) = self.recent_thumbs.get(path) {
            return t.clone();
        }
        let texture = thumbnail_path(path)
            .and_then(|p| crate::files::read(p).ok())
            .and_then(|bytes| image::load_from_memory(&bytes).ok())
            .map(|img| {
                let rgba = img.to_rgba8();
                let size = [rgba.width() as usize, rgba.height() as usize];
                ctx.load_texture(
                    format!("recent:{}", path.display()),
                    egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()),
                    egui::TextureOptions::LINEAR,
                )
            });
        self.recent_thumbs
            .insert(path.to_path_buf(), texture.clone());
        texture
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::{geometry::Rect, ShapeKind};

    #[test]
    fn opened_and_saved_drawings_get_a_thumbnail() {
        let mut app = App::headless();
        assert_eq!(
            file_name(Path::new("/a/b.tdraw")),
            file_name(Path::new("/a/b.tdraw"))
        );
        assert_ne!(
            file_name(Path::new("/a/b.tdraw")),
            file_name(Path::new("/a/c.tdraw"))
        );
        app.new_shape(ShapeKind::Rect {
            rect: Rect::new(20.0, 20.0, 150.0, 200.0),
            radius: 0.0,
            corners: None,
        });
        let dir = std::env::temp_dir().join(format!("tracedraw-thumbs-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let drawing = dir.join("poster.tdraw");
        app.save_to(drawing.clone());
        let thumb = thumbnail_path(&drawing).expect("a place for it");
        let img = image::open(&thumb).expect("a PNG");
        assert_eq!(
            img.width().max(img.height()),
            160,
            "{}x{}",
            img.width(),
            img.height()
        );
        // A4 is upright: taller than wide.
        assert!(img.height() > img.width());
        // The Welcome Screen loads it once and draws.
        let ctx = crate::theme::ui_context();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 800.0),
            )),
            ..Default::default()
        };
        app.show_welcome = true;
        let mut out = ctx.run_ui(input, |ui| crate::ui::welcome::welcome_screen(&mut app, ui));
        out.textures_delta.clear();
        assert!(app.recent_thumbs.get(&drawing).is_some_and(|t| t.is_some()));
        let _ = std::fs::remove_file(&thumb);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An object beside the page on the desktop is in the thumbnail too
    /// (#2357), which shows the drawing's extent rather than the page's
    /// (#3052); one far out does not shrink the rest to a dot.
    #[test]
    fn the_thumbnail_shows_what_lies_beside_the_page() {
        let mut app = App::headless();
        let page = app.page_rect();
        // A4 upright: a picture of the page alone is taller than wide.
        let only = tracedraw_render::whole_page_area(app.doc(), app.page).expect("area");
        assert_eq!(only, page);
        app.new_shape(ShapeKind::Rect {
            rect: Rect::new(page.x1 + 20.0, 20.0, page.x1 + 200.0, 100.0),
            radius: 0.0,
            corners: None,
        });
        let area = tracedraw_render::whole_page_area(app.doc(), app.page).expect("area");
        assert!((area.x1 - (page.x1 + 200.0)).abs() < 1.0, "{area:?}");
        // The drawing's extent, not the page's (#3052).
        assert!((area.x0 - (page.x1 + 20.0)).abs() < 1.0, "{area:?}");
        let png = first_page_png(&app).expect("a thumbnail");
        let img = image::load_from_memory(&png).expect("a PNG");
        assert_eq!(img.width().max(img.height()), 160);
        assert!(img.width() > img.height(), "the object, wider than tall");
        // The object, on the right, is drawn (not white).
        let rgba = img.to_rgba8();
        let x = img.width() - img.width() / 8;
        let any_ink = (0..img.height()).any(|yy| rgba.get_pixel(x, yy).0 != [255, 255, 255, 255]);
        assert!(any_ink, "the object beside the page shows (column {x})");
        // The thumbnail a .cdr file keeps shows it too; its page pictures
        // stay the page alone.
        let cdr = crate::app::cdr_previews(app.doc());
        let t = cdr.thumbnail.expect("a .cdr thumbnail");
        assert!(t.width > t.height, "{}x{}", t.width, t.height);
        assert!(cdr.pages[0].height > cdr.pages[0].width);
        // Far out on the desktop: the area stops a few page sizes away.
        app.new_shape(ShapeKind::Rect {
            rect: Rect::new(50_000.0, 0.0, 50_010.0, 10.0),
            radius: 0.0,
            corners: None,
        });
        let far = tracedraw_render::whole_page_area(app.doc(), app.page).expect("area");
        let reach = page.width() * (1.0 + tracedraw_render::WHOLE_PAGE_REACH);
        assert!(far.x1 <= reach + 1e-6, "{far:?}");
        // An object on the page: the thumbnail is the object, not the page.
        let mut app = App::headless();
        app.new_shape(ShapeKind::Rect {
            rect: Rect::new(40.0, 40.0, 140.0, 90.0),
            radius: 0.0,
            corners: None,
        });
        let area = tracedraw_render::whole_page_area(app.doc(), app.page).expect("area");
        assert!(area.width() < 110.0 && area.height() < 60.0, "{area:?}");
    }
}
