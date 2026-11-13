//! Import PDF: how a PDF's (or AI file's) text is read in, asked when one
//! is opened or imported (#386): as editable text in the fonts installed
//! here, or as curves drawn from the fonts the file embeds, which keeps
//! its look exactly. File > Import can also bring each page in as one
//! picture (#1916).

use crate::app::App;
use crate::i18n::{tr, trf};
use crate::ui::dialogs::{ok_cancel, window};
use crate::ui::tips::RichTip;
use egui::Context;
use std::path::PathBuf;

/// The dialog's state.
#[derive(Debug, Clone, PartialEq)]
pub struct PdfTextState {
    pub path: PathBuf,
    /// Import into the drawing (File > Import) rather than open.
    pub import: bool,
    pub curves: bool,
    /// Lines joined into paragraph text (#2323).
    pub paragraphs: bool,
    /// Each page as one picture at `dpi` (File > Import only, #1916).
    pub picture: bool,
    pub dpi: f64,
    /// Reduce very large pictures (#1835).
    pub reduce_pictures: bool,
    /// Keep the choice and stop asking (Options > File Formats).
    pub dont_ask: bool,
}

impl App {
    /// Before a PDF is read: ask how to read its text, when the
    /// preferences say so and it has not been asked for this file yet.
    /// True when the dialog was opened (the caller stops there).
    pub fn ask_pdf_text(&mut self, path: &std::path::Path, import: bool) -> bool {
        if !self.settings.formats.pdf_ask_text || std::mem::take(&mut self.pdf_text_chosen) {
            return false;
        }
        self.dialog = crate::ui::dialogs::Dialog::PdfText(PdfTextState {
            path: path.to_path_buf(),
            import,
            curves: self.settings.formats.pdf_text_as_curves,
            paragraphs: self.settings.formats.pdf_text_paragraphs,
            picture: self.settings.formats.pdf_as_picture,
            dpi: self.settings.formats.pdf_picture_dpi,
            reduce_pictures: self.settings.formats.pdf_reduce_pictures,
            dont_ask: false,
        });
        true
    }

    /// The import options the preferences give. Pictures come in at
    /// their own size up to the memory guard, unless Reduce very large
    /// pictures is on; the browser and Android always reduce them, having
    /// less memory to give (#1835).
    pub fn pdf_options(&self) -> tracedraw_io::pdf_import::ImportOptions {
        use tracedraw_io::pdf_import::{FULL_PICTURE_PIXELS, REDUCED_PICTURE_PIXELS};
        let reduce = self.settings.formats.pdf_reduce_pictures || always_reduce_pictures();
        tracedraw_io::pdf_import::ImportOptions {
            // A page made a picture keeps the file's look: its text is
            // drawn from the fonts the file embeds (#1916).
            text_as_curves: self.settings.formats.pdf_text_as_curves
                || self.settings.formats.pdf_as_picture,
            max_picture_pixels: if reduce {
                REDUCED_PICTURE_PIXELS
            } else {
                FULL_PICTURE_PIXELS
            },
            text_as_paragraphs: self.settings.formats.pdf_text_paragraphs,
        }
    }
}

/// The browser and Android reduce very large pictures whatever the
/// choice (#1835).
pub fn always_reduce_pictures() -> bool {
    crate::files::WEB || crate::files::ANDROID
}

/// The Reduce very large pictures check box, ticked and greyed where
/// pictures are always reduced.
pub fn reduce_pictures_checkbox(ui: &mut egui::Ui, reduce: &mut bool) {
    if always_reduce_pictures() {
        let mut on = true;
        ui.add_enabled(
            false,
            egui::Checkbox::new(&mut on, tr("pdf_import.reduce_pictures")),
        )
        .rich_tip(crate::ui::tips::pair(
            "pdf_import.reduce_pictures",
            "pdf_import.reduce_pictures_always_tip",
        ));
    } else {
        ui.checkbox(reduce, tr("pdf_import.reduce_pictures"))
            .rich_tip(crate::ui::tips::pair(
                "pdf_import.reduce_pictures",
                "pdf_import.reduce_pictures_tip",
            ));
    }
}

/// The Join lines into paragraph text check box (#2323), for text read
/// as text only (`text`).
pub fn paragraphs_checkbox(ui: &mut egui::Ui, paragraphs: &mut bool, text: bool) {
    ui.add_enabled(
        text,
        egui::Checkbox::new(paragraphs, tr("pdf_import.paragraphs")),
    )
    .rich_tip(crate::ui::tips::pair(
        "pdf_import.paragraphs",
        "pdf_import.paragraphs_tip",
    ));
}

/// The Text, Curves and Picture choices of the dialog and of Options >
/// File Formats; Picture (and its resolution) where `picture` is given.
pub fn read_as_choices(
    ui: &mut egui::Ui,
    curves: &mut bool,
    picture: Option<(&mut bool, &mut f64)>,
) {
    // 0 text, 1 curves, 2 picture.
    let before = match &picture {
        Some((p, _)) if **p => 2,
        _ => u8::from(*curves),
    };
    let mut mode = before;
    ui.radio_value(&mut mode, 0, tr("pdf_import.text"))
        .rich_tip(crate::ui::tips::pair(
            "pdf_import.text",
            "pdf_import.text_tip",
        ));
    ui.radio_value(&mut mode, 1, tr("pdf_import.curves"))
        .rich_tip(crate::ui::tips::pair(
            "pdf_import.curves",
            "pdf_import.curves_tip",
        ));
    let Some((picture, dpi)) = picture else {
        *curves = mode == 1;
        return;
    };
    ui.radio_value(&mut mode, 2, tr("pdf_import.picture"))
        .rich_tip(crate::ui::tips::pair(
            "pdf_import.picture",
            "pdf_import.picture_tip",
        ));
    if mode != before {
        // The text choice stays as it was under a picture.
        if mode < 2 {
            *curves = mode == 1;
        }
        *picture = mode == 2;
    }
    if *picture {
        ui.horizontal(|ui| {
            ui.add_space(24.0);
            ui.label(tr("dialog.resolution"));
            ui.add(
                crate::ui::field::NumField::new(dpi)
                    .range(36.0..=2400.0)
                    .suffix(" dpi"),
            )
            .rich_tip(crate::ui::tips::pair(
                "dialog.resolution",
                "dialog.convert_dpi_desc",
            ));
        });
    }
}

/// Text whose font the file does not embed, left as text by the import:
/// as curves too, in the font that stands in for it here.
pub fn remaining_text_to_curves(pages: &mut [tracedraw_io::pdf_import::ImportedPage]) {
    let mut ids = tracedraw_core::id::IdSource::default();
    let mut next = || ids.shape();
    for p in pages {
        for s in &mut p.shapes {
            to_curves(s, &mut next);
        }
    }
}

fn to_curves(s: &mut tracedraw_core::Shape, next: &mut dyn FnMut() -> tracedraw_core::ShapeId) {
    use tracedraw_core::ShapeKind;
    match &mut s.kind {
        ShapeKind::Text { .. } => {
            if let Some(kind) = s.curves_kind(&[], next) {
                s.kind = kind;
            }
        }
        ShapeKind::Group { children } => {
            for c in children {
                to_curves(c, next);
            }
        }
        ShapeKind::ClipFrame { contents, .. } => {
            for c in contents {
                to_curves(c, next);
            }
        }
        _ => {}
    }
}

pub fn dialog(app: &mut App, ctx: &Context, st: &mut PdfTextState, close: &mut bool) {
    window(ctx, tr("pdf_import.title")).show(ctx, |ui| {
        ui.set_max_width(420.0);
        let name = st
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        ui.label(trf("pdf_import.prompt", &[("name", &name)]));
        ui.add_space(6.0);
        ui.label(tr("pdf_import.text_as"));
        // A picture only where the file comes into a drawing: opened, it
        // is a drawing of its own to edit.
        let picture = st.import.then_some((&mut st.picture, &mut st.dpi));
        read_as_choices(ui, &mut st.curves, picture);
        let text = !st.curves && !(st.import && st.picture);
        ui.horizontal(|ui| {
            ui.add_space(24.0);
            paragraphs_checkbox(ui, &mut st.paragraphs, text);
        });
        ui.add_space(4.0);
        reduce_pictures_checkbox(ui, &mut st.reduce_pictures);
        ui.add_space(4.0);
        ui.checkbox(&mut st.dont_ask, tr("pdf_import.dont_ask"));
        if ok_cancel(ui, close) {
            app.settings.formats.pdf_text_as_curves = st.curves;
            app.settings.formats.pdf_text_paragraphs = st.paragraphs;
            app.settings.formats.pdf_reduce_pictures = st.reduce_pictures;
            if st.import {
                app.settings.formats.pdf_as_picture = st.picture;
                app.settings.formats.pdf_picture_dpi = st.dpi;
            }
            if st.dont_ask {
                app.settings.formats.pdf_ask_text = false;
            }
            app.pdf_text_chosen = true;
            let path = st.path.clone();
            if st.import {
                app.import_pdf(&path);
            } else {
                app.open_path(path);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Opening a PDF asks how to read its text once; the answer is kept,
    /// and Do not ask again stops the question.
    #[test]
    fn opening_a_pdf_asks_how_to_read_its_text() {
        let mut app = App::headless();
        app.settings.formats.pdf_ask_text = true;
        let p = std::path::Path::new("x.pdf");
        assert!(app.ask_pdf_text(p, false));
        assert!(matches!(
            app.dialog,
            crate::ui::dialogs::Dialog::PdfText(PdfTextState { import: false, .. })
        ));
        // Answered: the next read goes through without asking.
        app.pdf_text_chosen = true;
        assert!(!app.ask_pdf_text(p, false));
        assert!(app.ask_pdf_text(p, true));
        app.settings.formats.pdf_ask_text = false;
        assert!(!app.ask_pdf_text(p, true));
        app.settings.formats.pdf_text_as_curves = true;
        assert!(app.pdf_options().text_as_curves);
    }

    /// Text read as text joins its lines into paragraph text unless the
    /// choice is turned off (#2323).
    #[test]
    fn pdf_text_comes_in_as_paragraphs_by_default() {
        let mut app = App::headless();
        assert!(app.pdf_options().text_as_paragraphs);
        app.settings.formats.pdf_text_paragraphs = false;
        assert!(!app.pdf_options().text_as_paragraphs);
    }

    /// Pictures come in at their own size on the desktop unless Reduce
    /// very large pictures is ticked, which is off by default (#1835).
    #[test]
    fn pictures_are_reduced_only_when_asked_on_the_desktop() {
        use tracedraw_io::pdf_import::{FULL_PICTURE_PIXELS, REDUCED_PICTURE_PIXELS};
        let mut app = App::headless();
        assert!(!app.settings.formats.pdf_reduce_pictures);
        assert!(!always_reduce_pictures());
        assert_eq!(app.pdf_options().max_picture_pixels, FULL_PICTURE_PIXELS);
        app.settings.formats.pdf_reduce_pictures = true;
        assert_eq!(app.pdf_options().max_picture_pixels, REDUCED_PICTURE_PIXELS);
    }

    /// A picture that came in smaller is told after opening, in a message
    /// of its own, whatever the warnings say (#1835).
    #[test]
    fn a_shrunk_picture_is_told_after_opening() {
        use crate::ui::dialogs::Dialog;
        let mut app = App::headless();
        app.settings.warnings.unread_objects = false;
        app.settings.warnings.missing_fonts = false;
        app.pdf_shrunk = vec![tracedraw_io::pdf_import::ShrunkPicture {
            page: 0,
            width: 11_160,
            height: 14_794,
            new_width: 5_580,
            new_height: 7_397,
        }];
        app.show_open_notes("test_file.pdf".into(), 0);
        let Dialog::OpenNotes(notes) = &app.dialog else {
            panic!("no message: {:?}", app.dialog);
        };
        let text = notes.text();
        assert!(
            text.contains("test_file.pdf") && text.contains("11160 x 14794 to 5580 x 7397"),
            "{text}"
        );
        assert!(app.pdf_shrunk.is_empty(), "told once");
        // Nothing shrunk: no message.
        app.dialog = Dialog::None;
        app.show_open_notes("other.pdf".into(), 0);
        assert!(matches!(app.dialog, Dialog::None));
    }
}
