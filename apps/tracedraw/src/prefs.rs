//! The preferences of Options > Global > Printing, File Formats, Bitmap
//! Tracing and Warnings: what the Print and Export dialogs start with,
//! where imported objects go, what every trace starts with, and which
//! messages show. See docs/behavior/options.md.

use crate::ui::dialogs::{ExportState, PrintState, EXPORT_FORMATS, PDF_STANDARDS, SHEETS};
use serde::{Deserialize, Serialize};

fn yes() -> bool {
    true
}

/// Options > Global > Printing: what the Print dialog starts with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PrintPrefs {
    pub fit_to_page: bool,
    pub separations: bool,
    pub bleed: bool,
    pub crop_marks: bool,
    pub registration_marks: bool,
    pub color_bars: bool,
    pub densitometer: bool,
    pub page_info: bool,
    /// An index into the Print dialog's sheets (0 fits the sheet to the
    /// pages).
    pub sheet: usize,
    pub landscape: bool,
    /// Space between pages on the sheet, mm.
    pub gutter: f64,
    /// Printing keeps the dialog's choices here for the next time.
    pub remember_last: bool,
    /// The Print dialog runs the preflight checks and shows what they find.
    #[serde(default = "yes")]
    pub preflight: bool,
    pub destination: usize,
    /// The system printer printed to last (`destination` 0), `None` for
    /// the PDF viewer.
    pub printer: Option<String>,
    pub orientation: usize,
    #[serde(default = "yes")]
    pub collate: bool,
    pub spot_to_cmyk: bool,
    pub text_black: bool,
    pub overprint_black: bool,
    pub bleed_limit: f64,
    pub page_numbers: bool,
    pub ps_level: u8,
    /// Pages keep their size on a fixed sheet when marks need room (#388).
    pub keep_size: bool,
}

impl Default for PrintPrefs {
    fn default() -> Self {
        let st = PrintState::default();
        PrintPrefs {
            fit_to_page: st.fit_to_page,
            separations: st.separations,
            bleed: st.bleed,
            crop_marks: st.crop_marks,
            registration_marks: st.registration_marks,
            color_bars: st.color_bars,
            densitometer: st.densitometer,
            page_info: st.page_info,
            sheet: st.sheet,
            landscape: st.landscape,
            gutter: st.gutter,
            remember_last: false,
            preflight: true,
            destination: st.destination,
            printer: st.printer.clone(),
            orientation: st.orientation,
            collate: st.collate,
            spot_to_cmyk: st.spot_to_cmyk,
            text_black: st.text_black,
            overprint_black: st.overprint_black,
            bleed_limit: st.bleed_limit,
            page_numbers: st.page_numbers,
            ps_level: st.ps_level,
            keep_size: st.keep_size,
        }
    }
}

impl PrintPrefs {
    /// A Print dialog starting with these choices (the rest as always:
    /// one copy, all pages, 100 %, as in the document).
    pub fn state(&self) -> PrintState {
        PrintState {
            fit_to_page: self.fit_to_page,
            separations: self.separations,
            bleed: self.bleed,
            crop_marks: self.crop_marks,
            registration_marks: self.registration_marks,
            color_bars: self.color_bars,
            densitometer: self.densitometer,
            page_info: self.page_info,
            sheet: if self.sheet < SHEETS.len() {
                self.sheet
            } else {
                0
            },
            landscape: self.landscape,
            destination: self.destination.min(2),
            printer: self.printer.clone(),
            orientation: self.orientation.min(2),
            collate: self.collate,
            spot_to_cmyk: self.spot_to_cmyk,
            text_black: self.text_black,
            overprint_black: self.overprint_black,
            bleed_limit: if self.bleed_limit.is_finite() && self.bleed_limit > 0.0 {
                self.bleed_limit.min(50.0)
            } else {
                PrintState::default().bleed_limit
            },
            page_numbers: self.page_numbers,
            ps_level: self.ps_level.clamp(2, 3),
            keep_size: self.keep_size,
            gutter: if self.gutter.is_finite() {
                self.gutter.clamp(0.0, 100.0)
            } else {
                PrintState::default().gutter
            },
            ..PrintState::default()
        }
    }

    /// Keep the choices of a print for the next one.
    pub fn remember(&mut self, st: &PrintState) {
        self.fit_to_page = st.fit_to_page;
        self.separations = st.separations;
        self.bleed = st.bleed;
        self.crop_marks = st.crop_marks;
        self.registration_marks = st.registration_marks;
        self.color_bars = st.color_bars;
        self.densitometer = st.densitometer;
        self.page_info = st.page_info;
        self.sheet = st.sheet;
        self.landscape = st.landscape;
        self.gutter = st.gutter;
        self.destination = st.destination;
        self.printer = st.printer.clone();
        self.orientation = st.orientation;
        self.collate = st.collate;
        self.spot_to_cmyk = st.spot_to_cmyk;
        self.text_black = st.text_black;
        self.overprint_black = st.overprint_black;
        self.bleed_limit = st.bleed_limit;
        self.page_numbers = st.page_numbers;
        self.ps_level = st.ps_level;
        self.keep_size = st.keep_size;
    }
}

/// Where imported objects go.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportPlace {
    /// Where the file has them.
    #[default]
    AsInFile,
    /// Centred in the middle of the view.
    ViewCenter,
}

/// Options > Global > File Formats: what File > Export starts with,
/// where imported objects go and the `.cdr` version of new drawings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FormatPrefs {
    /// An index into the export formats.
    pub export_format: usize,
    /// Bitmap formats, dots per inch.
    pub export_dpi: f64,
    /// JPEG and WebP quality, 1 to 100.
    pub export_quality: u8,
    /// PNG, WebP and GIF.
    pub export_transparent: bool,
    /// PDF: 0 plain, 1 X-1a, 2 X-3, 3 X-4.
    pub pdf_standard: usize,
    /// PDF/X output condition.
    pub output_condition: String,
    /// PDF bleed, mm.
    pub pdf_bleed_mm: f64,
    /// Exporting keeps the dialog's choices here for the next time.
    pub remember_last: bool,
    pub import_place: ImportPlace,
    /// The `.cdr` version new drawings are saved in (Save As changes it).
    pub cdr_version: u16,
    /// File > Export goes straight to the file with the last choices
    /// ("Do not show filter dialog").
    pub skip_export_dialog: bool,
    /// PDF and AI files: text read in as curves from the fonts the file
    /// embeds (else as editable text, #386).
    pub pdf_text_as_curves: bool,
    /// Ask how to read a PDF's text each time one is opened or imported.
    pub pdf_ask_text: bool,
}

impl Default for FormatPrefs {
    fn default() -> Self {
        let st = ExportState::default();
        FormatPrefs {
            export_format: st.format,
            export_dpi: st.dpi,
            export_quality: st.quality,
            export_transparent: st.transparent,
            pdf_standard: st.pdf_standard,
            output_condition: st.output_condition,
            pdf_bleed_mm: st.bleed_mm,
            remember_last: false,
            import_place: ImportPlace::AsInFile,
            cdr_version: tracedraw_cdr::write::NEWEST_VERSION,
            skip_export_dialog: false,
            pdf_text_as_curves: false,
            pdf_ask_text: true,
        }
    }
}

impl FormatPrefs {
    /// The Export dialog starting with these choices; values out of range
    /// fall back to the usual ones.
    pub fn state(&self) -> ExportState {
        let d = ExportState::default();
        ExportState {
            format: if self.export_format < EXPORT_FORMATS.len() {
                self.export_format
            } else {
                d.format
            },
            dpi: if self.export_dpi.is_finite() {
                self.export_dpi
                    .clamp(crate::ui::dialogs::MIN_EXPORT_DPI, 2400.0)
            } else {
                d.dpi
            },
            quality: self.export_quality.clamp(1, 100),
            transparent: self.export_transparent,
            pdf_standard: if self.pdf_standard < PDF_STANDARDS.len() {
                self.pdf_standard
            } else {
                0
            },
            output_condition: if self.output_condition.trim().is_empty() {
                d.output_condition.clone()
            } else {
                self.output_condition.chars().take(64).collect()
            },
            bleed_mm: if self.pdf_bleed_mm.is_finite() {
                self.pdf_bleed_mm.clamp(0.0, 50.0)
            } else {
                d.bleed_mm
            },
            remember: self.remember_last,
            ..d
        }
    }

    /// Keep the choices of an export for the next one (not the presets for
    /// the web and office, which have their own).
    pub fn remember(&mut self, st: &ExportState) {
        self.export_format = st.format;
        self.export_dpi = st.dpi;
        self.export_quality = st.quality;
        self.export_transparent = st.transparent;
        self.pdf_standard = st.pdf_standard;
        self.output_condition = st.output_condition.clone();
        self.pdf_bleed_mm = st.bleed_mm;
    }
}

/// Options > Global > Bitmap Tracing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TracePrefs {
    /// The preset Bitmaps > Quick Trace uses.
    pub quick_preset: crate::trace::Preset,
    /// Every trace starts with these.
    pub remove_background: bool,
    pub delete_original: bool,
}

impl Default for TracePrefs {
    fn default() -> Self {
        let s = crate::trace::Preset::Clipart.settings();
        TracePrefs {
            quick_preset: crate::trace::Preset::Clipart,
            remove_background: s.remove_background,
            delete_original: s.delete_original,
        }
    }
}

impl TracePrefs {
    /// The settings of `preset` with the choices every trace starts with.
    pub fn settings(&self, preset: crate::trace::Preset) -> crate::trace::Settings {
        crate::trace::Settings {
            remove_background: self.remove_background,
            delete_original: self.delete_original,
            ..preset.settings()
        }
    }
}

/// Options > Global > Warnings: the messages that show (each also has
/// "Do not show this message again").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WarningPrefs {
    /// When objects of an opened file could not be read.
    pub unread_objects: bool,
    /// When an opened drawing uses fonts that are not installed.
    pub missing_fonts: bool,
    /// Before an item is taken off the Shelf.
    pub shelf_remove: bool,
}

impl Default for WarningPrefs {
    fn default() -> Self {
        WarningPrefs {
            unread_objects: true,
            missing_fonts: true,
            shelf_remove: true,
        }
    }
}

/// Options > Tools > Curves: how the Freehand stroke is smoothed, how
/// near an end node a new line must start to continue that curve, and the
/// Pen's preview and automatic adding and deleting of nodes.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CurvePrefs {
    /// Freehand smoothing, 0 (keeps every wobble) to 100.
    pub smoothing: u32,
    /// Screen pixels within which a new line joins an end node of the
    /// selected curve (and a curve closes on its first node).
    pub join_px: f32,
    /// Pen: a dashed segment runs from the last node to the pointer while
    /// the button is up, showing the next segment before the click.
    pub pen_preview: bool,
    /// Pen: a click on a segment (of the curve being drawn or of the
    /// selected curve) adds a node there, a click on a node deletes it.
    pub pen_auto_add_delete: bool,
    /// The key held to keep the next segment (and a pulled handle) to
    /// steps of the constrain angle while a curve is drawn.
    #[serde(default)]
    pub constrain_key: ConstrainKey,
}

/// The modifier key that constrains the angle while drawing curves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConstrainKey {
    #[default]
    Ctrl,
    Shift,
    Alt,
}

impl ConstrainKey {
    pub const ALL: [ConstrainKey; 3] = [ConstrainKey::Ctrl, ConstrainKey::Shift, ConstrainKey::Alt];

    /// True when this key is among the modifiers held.
    pub fn held(self, mods: &egui::Modifiers) -> bool {
        match self {
            ConstrainKey::Ctrl => mods.ctrl,
            ConstrainKey::Shift => mods.shift,
            ConstrainKey::Alt => mods.alt,
        }
    }

    pub fn i18n_key(self) -> &'static str {
        match self {
            ConstrainKey::Ctrl => "options.key_ctrl",
            ConstrainKey::Shift => "options.key_shift",
            ConstrainKey::Alt => "options.key_alt",
        }
    }
}

impl Default for CurvePrefs {
    fn default() -> Self {
        CurvePrefs {
            smoothing: 50,
            join_px: 5.0,
            pen_preview: true,
            pen_auto_add_delete: true,
            constrain_key: ConstrainKey::Ctrl,
        }
    }
}

impl CurvePrefs {
    /// The join distance, kept sensible.
    pub fn join_px(&self) -> f64 {
        if self.join_px.is_finite() {
            f64::from(self.join_px.clamp(1.0, 50.0))
        } else {
            5.0
        }
    }

    /// The simplification tolerance of a freehand stroke in screen
    /// pixels: 0.2 at no smoothing, 1.3 at the default 50, 2.4 at 100.
    pub fn freehand_tolerance_px(&self) -> f64 {
        0.2 + f64::from(self.smoothing.min(100)) / 100.0 * 2.2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn print_preferences_start_the_print_dialog() {
        let mut p = PrintPrefs::default();
        // The defaults change nothing.
        let st = p.state();
        let d = PrintState::default();
        assert_eq!(
            (st.crop_marks, st.sheet, st.gutter),
            (d.crop_marks, d.sheet, d.gutter)
        );
        p.crop_marks = true;
        p.page_info = true;
        p.sheet = 2;
        p.landscape = true;
        p.gutter = 8.0;
        let st = p.state();
        assert!(st.crop_marks && st.page_info && st.landscape);
        assert_eq!((st.sheet, st.gutter, st.copies), (2, 8.0, 1));
        // Nonsense from a file falls back.
        p.sheet = 99;
        p.gutter = f64::NAN;
        let st = p.state();
        assert_eq!((st.sheet, st.gutter), (0, d.gutter));
        // A print remembered.
        let mut q = PrintPrefs::default();
        let printed = PrintState {
            bleed: true,
            color_bars: true,
            sheet: 3,
            ..PrintState::default()
        };
        q.remember(&printed);
        assert!(q.bleed && q.color_bars);
        assert_eq!(q.state().sheet, 3);
    }

    #[test]
    fn format_preferences_start_the_export_dialog() {
        let mut f = FormatPrefs::default();
        let d = ExportState::default();
        let st = f.state();
        assert_eq!(
            (st.format, st.dpi, st.quality),
            (d.format, d.dpi, d.quality)
        );
        f.export_format = 3;
        f.export_dpi = 150.0;
        f.export_quality = 70;
        f.pdf_standard = 2;
        f.output_condition = "GRACoL".into();
        let st = f.state();
        assert_eq!(
            (st.format, st.dpi, st.quality, st.pdf_standard),
            (3, 150.0, 70, 2)
        );
        assert_eq!(st.output_condition, "GRACoL");
        // Out of range values from a file.
        f.export_format = 999;
        f.export_dpi = 1.0e9;
        f.export_quality = 0;
        f.pdf_standard = 9;
        f.output_condition = "  ".into();
        f.pdf_bleed_mm = -3.0;
        let st = f.state();
        assert_eq!(st.format, d.format);
        assert_eq!(st.dpi, 2400.0);
        assert_eq!((st.quality, st.pdf_standard), (1, 0));
        assert_eq!(st.output_condition, d.output_condition);
        assert_eq!(st.bleed_mm, 0.0);
        // An export remembered.
        let mut g = FormatPrefs::default();
        g.remember(&ExportState {
            format: 1,
            pdf_standard: 3,
            ..ExportState::default()
        });
        assert_eq!((g.state().format, g.state().pdf_standard), (1, 3));
    }

    #[test]
    fn the_dialogs_start_with_the_preferences_and_remember() {
        use crate::ui::dialogs::Dialog;
        let mut app = crate::app::App::headless();
        app.settings.printing.crop_marks = true;
        app.print();
        assert!(matches!(&app.dialog, Dialog::Print(st) if st.crop_marks));
        app.settings.formats.export_format = 1;
        app.settings.formats.remember_last = true;
        app.export();
        assert!(matches!(&app.dialog, Dialog::Export(st) if st.format == 1 && st.remember));
        // The presets for the web and office never overwrite the choices.
        assert!(!ExportState::web().remember && !ExportState::office().remember);
        app.dialog = Dialog::None;
    }

    #[test]
    fn imported_objects_can_go_to_the_middle_of_the_view() {
        let dir = std::env::temp_dir().join(format!("tracedraw-import-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let svg = dir.join("box.svg");
        std::fs::write(
            &svg,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="100mm" viewBox="0 0 100 100"><rect x="0" y="0" width="10" height="10" fill="red"/></svg>"#,
        )
        .expect("svg");
        let mut app = crate::app::App::headless();
        app.canvas_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let count = |app: &crate::app::App| {
            app.doc()
                .all_layers()
                .map(|l| l.shapes.len())
                .sum::<usize>()
        };
        // As in the file.
        app.import_path(&svg);
        let as_in_file = app.selection_bounds().expect("imported");
        let n = count(&app);
        assert!(n > 0);
        // Centred in the view, in the same undo step as the import.
        app.settings.formats.import_place = ImportPlace::ViewCenter;
        app.import_path(&svg);
        let centred = app.selection_bounds().expect("imported");
        let view = app.view.visible_page_rect(app.canvas_rect).center();
        assert!((centred.center().x - view.x).abs() < 1e-6);
        assert!((centred.center().y - view.y).abs() < 1e-6);
        assert!((centred.width() - as_in_file.width()).abs() < 1e-6);
        app.undo();
        assert_eq!(count(&app), n);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn opening_tells_what_was_left_out_as_the_warnings_say() {
        use crate::ui::dialogs::Dialog;
        let mut app = crate::app::App::headless();
        app.missing_fonts = vec!["Nowhere Sans".into()];
        app.show_open_notes("a.cdr".into(), 3);
        let Dialog::OpenNotes(notes) = &app.dialog else {
            panic!("no message");
        };
        assert_eq!((notes.unread, notes.fonts.len()), (3, 1));
        let text = notes.text();
        assert!(
            text.contains("a.cdr") && text.contains("Nowhere Sans"),
            "{text}"
        );
        // With the font that draws its text in the meantime.
        if let Some(s) = tracedraw_text::fonts().substitute("Nowhere Sans") {
            assert!(text.contains(&format!("Nowhere Sans ({s})")), "{text}");
        }
        // Nothing to say, or the warnings turned off: no message.
        app.dialog = Dialog::None;
        app.missing_fonts.clear();
        app.show_open_notes("b.tdraw".into(), 0);
        assert!(matches!(app.dialog, Dialog::None));
        app.settings.warnings.unread_objects = false;
        app.show_open_notes("c.cdr".into(), 5);
        assert!(matches!(app.dialog, Dialog::None));
        // Another dialog is never replaced.
        app.settings.warnings = WarningPrefs::default();
        app.dialog = Dialog::About;
        app.show_open_notes("d.cdr".into(), 5);
        assert!(matches!(app.dialog, Dialog::About));
    }

    #[test]
    fn the_shelf_asks_before_removing_unless_told_not_to() {
        use crate::ui::dialogs::Dialog;
        let ctx = crate::theme::ui_context();
        let mut app = crate::app::App::headless();
        let id = app
            .new_shape(tracedraw_core::ShapeKind::Rect {
                rect: tracedraw_core::geometry::Rect::new(0.0, 0.0, 5.0, 5.0),
                radius: 0.0,
                corners: None,
            })
            .expect("rect");
        app.select(vec![id]);
        assert!(app.shelf_add_selection());
        let uid = app.shelf.items[0].uid;
        crate::ui::shelf_docker::ask_to_remove(&mut app, uid);
        assert!(matches!(app.dialog, Dialog::ConfirmShelfRemove { .. }));
        assert_eq!(app.shelf.items.len(), 1);
        // Remove, with "Do not ask again".
        if let Dialog::ConfirmShelfRemove { dont_ask, .. } = &mut app.dialog {
            *dont_ask = true;
        }
        let enter = egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let mut out = ctx.run_ui(
            egui::RawInput {
                events: vec![enter],
                ..Default::default()
            },
            |ui| crate::ui::dialogs::show(&mut app, ui.ctx()),
        );
        out.textures_delta.clear();
        assert!(app.shelf.items.is_empty());
        assert!(!app.settings.warnings.shelf_remove);
        // Now it goes at once.
        app.select(vec![id]);
        assert!(app.shelf_add_selection());
        let uid = app.shelf.items[0].uid;
        crate::ui::shelf_docker::ask_to_remove(&mut app, uid);
        assert!(app.shelf.items.is_empty());
        assert!(matches!(app.dialog, Dialog::None));
    }

    #[test]
    fn trace_preferences_apply_to_every_preset() {
        let t = TracePrefs {
            quick_preset: crate::trace::Preset::Logo,
            remove_background: false,
            delete_original: true,
        };
        let s = t.settings(crate::trace::Preset::HighQualityImage);
        let p = crate::trace::Preset::HighQualityImage.settings();
        assert!(!s.remove_background && s.delete_original);
        assert_eq!((s.detail, s.colors), (p.detail, p.colors));
        // Preferences written before these existed read as the defaults.
        let old: crate::settings::Settings = serde_json::from_str("{}").expect("settings");
        assert_eq!(old.tracing, TracePrefs::default());
        assert_eq!(old.warnings, WarningPrefs::default());
        assert!(old.printing.preflight);
        assert_eq!(old.formats.import_place, ImportPlace::AsInFile);
    }
}
