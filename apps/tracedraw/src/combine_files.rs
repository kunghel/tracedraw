//! File > Combine Files and File > Save Pages as Files (#3358): the pages
//! of other drawings added to this one, and each page of this drawing
//! saved as a drawing of its own.

use std::path::{Path, PathBuf};

use tracedraw_core::document::{Document, Shape, Symbol};
use tracedraw_core::Command;

use crate::app::App;
use crate::i18n::{tr, trf};

/// The drawings Combine Files reads: TraceDraw's own and the `.cdr`
/// and `.cmx` formats.
pub const COMBINE_EXTENSIONS: [&str; 5] = ["tdraw", "cdr", "cdt", "cdrt", "cmx"];

impl App {
    /// File > Combine Files...: the drawings to add, picked together.
    pub fn combine_files(&mut self) {
        crate::files::Dialog::new()
            .add_filter(tr("file.drawings"), &COMBINE_EXTENSIONS)
            .pick_files(self, |app, paths| app.combine_paths(&paths));
    }

    /// A drawing read whole for Combine Files.
    pub(crate) fn read_drawing(&mut self, path: &Path) -> Result<Document, String> {
        let ext = App::file_kind(path);
        if ext == "tdraw" {
            // A package's fonts, beside the drawing, before its text is
            // laid out, as File > Open has them (#852).
            self.load_package_fonts(path);
            return crate::files::read(path)
                .and_then(|bytes| tracedraw_core::document::tdraw_text(&bytes))
                .map_err(|e| e.to_string())
                .and_then(|json| Document::from_json(&json).map_err(|e| e.to_string()));
        }
        if ext == "cdr" || ext == "cmx" {
            return App::open_cdr(path).map(|(doc, _)| doc);
        }
        Err(trf("status.not_a_drawing", &[("ext", &ext)]))
    }

    /// Every page of the drawings at `paths`, in the order given, added
    /// after the last page of this one, with their names, sizes and
    /// objects (the layers of each page as one layer, in their order) and
    /// the symbols they show, as one undo step. A file that cannot be read
    /// is left out and named on the status bar.
    pub fn combine_paths(&mut self, paths: &[PathBuf]) {
        if !self.has_document() || paths.is_empty() {
            return;
        }
        let mut symbols: Vec<Symbol> = self
            .doc()
            .symbols
            .iter()
            .cloned()
            .map(crate::app::symbol_without_uids)
            .collect();
        let had_symbols = symbols.len();
        let mut pages = Vec::new();
        let mut failed = Vec::new();
        let mut files = 0;
        for path in paths {
            let doc = match self.read_drawing(path) {
                Ok(doc) => doc,
                Err(e) => {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    failed.push(format!("{name}: {e}"));
                    continue;
                }
            };
            files += 1;
            self.add_carried_fonts(&doc.fonts);
            let from: Vec<Symbol> = doc
                .symbols
                .iter()
                .cloned()
                .map(crate::app::symbol_without_uids)
                .collect();
            for page in &doc.pages {
                let mut shapes: Vec<Shape> = page
                    .layers
                    .iter()
                    .flat_map(|l| l.shapes.iter().cloned())
                    .collect();
                self.apply_import_policy(&mut shapes, doc.cmyk_profile.as_ref());
                for s in &mut shapes {
                    crate::symbol_paste::bring(s, &from, &mut symbols);
                }
                let layer = page.layers.first().map(|l| l.name.clone());
                pages.push((page.name.clone(), page.size, layer, shapes));
            }
        }
        if !pages.is_empty() {
            let at = self.doc().pages.len();
            let count = pages.len();
            let mut cmds: Vec<Command> = symbols
                .drain(had_symbols..)
                .map(|symbol| Command::AddSymbol { symbol })
                .collect();
            for (k, (name, size, layer, shapes)) in pages.into_iter().enumerate() {
                cmds.push(Command::InsertPage {
                    at: at + k,
                    name: Some(name),
                    size,
                    layer,
                    shapes,
                });
            }
            if let Err(e) = self.engine.run_batch("Combine Files", &cmds) {
                self.status = e.to_string();
                return;
            }
            if let Some(p) = self.doc().pages.get(at) {
                self.page = p.id;
            }
            self.selection.clear();
            self.fit_pending = true;
            self.status = trf(
                "status.files_combined",
                &[("n", &count.to_string()), ("f", &files.to_string())],
            );
        }
        if !failed.is_empty() {
            let e = failed.join("; ");
            let note = trf("status.import_failed", &[("e", &e)]);
            self.status = if self.status.is_empty() || files == 0 {
                note
            } else {
                format!("{} {note}", self.status)
            };
        }
    }

    /// File > Save Pages as Files...: a name for the files, then one
    /// drawing for each page.
    pub fn save_pages_as_files(&mut self) {
        let name = format!("{}.tdraw", self.doc().title);
        crate::files::Dialog::new()
            .add_filter("TraceDraw (*.tdraw)", &["tdraw"])
            .set_file_name(name)
            .save_file(self, |app, path| {
                app.save_pages_to(&path);
            });
    }

    /// Each page of the drawing saved beside `base` as a TraceDraw drawing
    /// of its own, named after `base` with the page's number ("Poster-1",
    /// "Poster-2"), holding that page with its layers, guides and
    /// background and everything the drawing keeps for all its pages
    /// (symbols, styles, colours, inks, fonts). The drawing itself stays
    /// as it is, unsaved changes and all. The paths written.
    pub fn save_pages_to(&mut self, base: &Path) -> Vec<PathBuf> {
        let dir = base.parent().map(Path::to_path_buf).unwrap_or_default();
        let stem = base
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| self.doc().title.clone());
        let doc = self.doc().clone();
        let mut written = Vec::new();
        let mut errors = Vec::new();
        for (i, page) in doc.pages.iter().enumerate() {
            let mut one = doc.clone();
            one.pages = vec![page.clone()];
            one.title = format!("{stem}-{}", i + 1);
            let path = dir.join(format!("{stem}-{}.tdraw", i + 1));
            let result = crate::files::write_with(&path, |w| {
                one.write_tdraw_version(w, tracedraw_core::document::TDRAW_FORMAT_VERSION)
                    .map_err(std::io::Error::other)
            });
            match result {
                Ok(()) => written.push(path),
                Err(e) => errors.push(e.to_string()),
            }
        }
        self.status = if errors.is_empty() {
            trf(
                "status.pages_saved_as_files",
                &[
                    ("n", &written.len().to_string()),
                    ("dir", &dir.display().to_string()),
                ],
            )
        } else {
            trf("status.save_failed", &[("e", &errors.join("; "))])
        };
        written
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::document::ShapeKind;
    use tracedraw_core::geometry::Rect;

    fn rect(app: &mut App, x: f64) -> tracedraw_core::ShapeId {
        app.new_shape(ShapeKind::Rect {
            rect: Rect::new(x, 10.0, x + 20.0, 30.0),
            radius: 0.0,
            corners: None,
        })
        .expect("rect")
    }

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("tracedraw-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("dir");
        d
    }

    /// Save Pages as Files writes one drawing a page, each holding that
    /// page alone; Combine Files brings them back as pages after the
    /// drawing's own, with their names, sizes and objects, in one undo
    /// step (#3358).
    #[test]
    fn pages_saved_as_files_combine_back() {
        let d = dir("combine");
        let mut app = App::headless();
        rect(&mut app, 10.0);
        app.add_page();
        rect(&mut app, 50.0);
        rect(&mut app, 80.0);
        let second = app.doc().pages[1].id;
        app.run(Command::ResizePage {
            page: second,
            size: tracedraw_core::Size::new(300.0, 200.0),
        });
        let written = app.save_pages_to(&d.join("Poster.tdraw"));
        assert_eq!(
            written,
            vec![d.join("Poster-1.tdraw"), d.join("Poster-2.tdraw")]
        );
        let mut other = App::headless();
        let one = other.read_drawing(&written[1]).expect("read");
        assert_eq!(one.pages.len(), 1);
        assert_eq!(one.pages[0].size, tracedraw_core::Size::new(300.0, 200.0));
        assert_eq!(one.pages[0].layers.iter().map(|l| l.shapes.len()).sum::<usize>(), 2);
        // Combined into a drawing of one page: three pages after it.
        let mut target = App::headless();
        rect(&mut target, 0.0);
        let mut paths = written.clone();
        paths.push(d.join("missing.tdraw"));
        target.combine_paths(&paths);
        let pages = &target.doc().pages;
        assert_eq!(pages.len(), 3);
        assert_eq!(pages[2].size, tracedraw_core::Size::new(300.0, 200.0));
        let count = |i: usize| -> usize {
            target.doc().pages[i]
                .layers
                .iter()
                .map(|l| l.shapes.len())
                .sum()
        };
        assert_eq!((count(0), count(1), count(2)), (1, 1, 2));
        assert_eq!(target.page, target.doc().pages[1].id, "the first added page is shown");
        assert!(target.status.contains("missing.tdraw"), "{}", target.status);
        // One undo step takes them all away.
        target.undo();
        assert_eq!(target.doc().pages.len(), 1);
        let _ = std::fs::remove_dir_all(&d);
    }
}
