//! Options > Global: Printing, File Formats, Bitmap Tracing and Warnings
//! (see `prefs.rs` for what each choice does and docs/behavior/options.md).

use crate::app::App;
use crate::i18n::tr;
use crate::prefs::ImportPlace;
use crate::theme::Tokens;
use crate::ui::chrome;
use crate::ui::dialogs::{sheet_name, EXPORT_FORMATS, PDF_STANDARDS, SHEETS};
use crate::ui::options::{combo, grid, row_label};
use egui::Ui;

fn hint(ui: &mut Ui, key: &str) {
    ui.label(egui::RichText::new(tr(key)).color(Tokens::text_dim()));
}

/// Options > Global > Printing: what the Print dialog starts with, whether
/// printing keeps its choices, and the preflight summary.
pub fn printing_page(app: &mut App, ui: &mut Ui) {
    let p = &mut app.settings.printing;
    chrome::section(ui, &tr("options.print_starts_with"));
    ui.checkbox(&mut p.fit_to_page, tr("dialog.fit_to_page"));
    ui.checkbox(&mut p.separations, tr("dialog.separations"));
    ui.checkbox(&mut p.bleed, tr("dialog.print_bleed"));
    ui.checkbox(&mut p.crop_marks, tr("dialog.crop_marks"));
    ui.checkbox(&mut p.registration_marks, tr("dialog.registration_marks"));
    ui.checkbox(&mut p.color_bars, tr("print.color_bars"));
    ui.checkbox(&mut p.densitometer, tr("print.densitometer"));
    ui.checkbox(&mut p.page_info, tr("print.page_info_mark"));
    grid("printing_layout").show(ui, |ui| {
        row_label(ui, &tr("print.sheet"));
        ui.horizontal(|ui| {
            let sheets: Vec<usize> = (0..SHEETS.len()).collect();
            if p.sheet >= SHEETS.len() {
                p.sheet = 0;
            }
            combo(
                ui,
                "opt_print_sheet",
                &mut p.sheet,
                &sheets,
                sheet_name,
                130.0,
            );
            ui.add_enabled(
                p.sheet > 0,
                egui::Checkbox::new(&mut p.landscape, tr("print.landscape")),
            );
        });
        ui.end_row();
        row_label(ui, &tr("print.gutter"));
        ui.add(
            crate::ui::field::NumField::new(&mut p.gutter)
                .range(0.0..=50.0)
                .speed(0.5)
                .suffix(" mm"),
        );
        ui.end_row();
    });
    ui.add_space(6.0);
    ui.checkbox(&mut p.remember_last, tr("options.print_remember"));
    chrome::section(ui, &tr("options.print_checks"));
    ui.checkbox(&mut p.preflight, tr("options.print_preflight"));
}

/// Options > Global > File Formats: what File > Export starts with,
/// where imported objects go and the `.cdr` version new drawings are
/// saved in.
pub fn file_formats_page(app: &mut App, ui: &mut Ui) {
    let f = &mut app.settings.formats;
    chrome::section(ui, &tr("options.export_starts_with"));
    if f.export_format >= EXPORT_FORMATS.len() {
        f.export_format = 0;
    }
    if f.pdf_standard >= PDF_STANDARDS.len() {
        f.pdf_standard = 0;
    }
    grid("formats_export").show(ui, |ui| {
        row_label(ui, &tr("dialog.format"));
        let formats: Vec<usize> = (0..EXPORT_FORMATS.len()).collect();
        combo(
            ui,
            "opt_export_format",
            &mut f.export_format,
            &formats,
            |i| EXPORT_FORMATS[i].0.to_string(),
            130.0,
        );
        ui.end_row();
        row_label(ui, &tr("dialog.resolution"));
        ui.add(
            crate::ui::field::NumField::new(&mut f.export_dpi)
                .range(crate::ui::dialogs::MIN_EXPORT_DPI..=2400.0)
                .suffix(" dpi"),
        );
        ui.end_row();
        row_label(ui, &tr("dialog.quality"));
        ui.add(crate::ui::field::NumField::new(&mut f.export_quality).range(1..=100));
        ui.end_row();
        row_label(ui, &tr("dialog.pdf_standard"));
        let standards: Vec<usize> = (0..PDF_STANDARDS.len()).collect();
        combo(
            ui,
            "opt_pdf_standard",
            &mut f.pdf_standard,
            &standards,
            |i| PDF_STANDARDS[i].to_string(),
            130.0,
        );
        ui.end_row();
        row_label(ui, &tr("dialog.output_condition"));
        ui.add(egui::TextEdit::singleline(&mut f.output_condition).desired_width(130.0));
        ui.end_row();
        row_label(ui, &tr("dialog.bleed"));
        ui.add(
            crate::ui::field::NumField::new(&mut f.pdf_bleed_mm)
                .range(0.0..=50.0)
                .speed(0.5)
                .suffix(" mm"),
        );
        ui.end_row();
    });
    ui.checkbox(
        &mut f.export_transparent,
        tr("dialog.transparent_background"),
    );
    ui.checkbox(&mut f.remember_last, tr("options.export_remember"));
    hint(ui, "options.export_presets_hint");
    chrome::section(ui, &tr("options.import_section"));
    ui.label(tr("options.import_place"));
    ui.radio_value(
        &mut f.import_place,
        ImportPlace::AsInFile,
        tr("options.import_as_in_file"),
    );
    ui.radio_value(
        &mut f.import_place,
        ImportPlace::ViewCenter,
        tr("options.import_view_center"),
    );
    ui.horizontal(|ui| {
        ui.label(tr("pdf_import.text_as"));
        ui.radio_value(&mut f.pdf_text_as_curves, false, tr("pdf_import.text"));
        ui.radio_value(&mut f.pdf_text_as_curves, true, tr("pdf_import.curves"));
    });
    ui.checkbox(&mut f.pdf_ask_text, tr("pdf_import.ask"));
    chrome::section(ui, &tr("options.cdr_save_section"));
    f.cdr_version = f.cdr_version.clamp(
        tracedraw_cdr::write::CHECKED_SINCE,
        tracedraw_cdr::write::NEWEST_VERSION,
    );
    grid("formats_cdr").show(ui, |ui| {
        row_label(ui, &tr("dialog.cdr_version"));
        combo(
            ui,
            "opt_cdr_version",
            &mut f.cdr_version,
            &crate::ui::save_as::version_choices(),
            crate::ui::save_as::version_label,
            130.0,
        );
        ui.end_row();
    });
    hint(ui, "options.cdr_version_hint");
}

/// Options > Global > Bitmap Tracing: Quick Trace's preset and what every
/// trace starts with.
pub fn bitmap_tracing_page(app: &mut App, ui: &mut Ui) {
    let t = &mut app.settings.tracing;
    grid("tracing").show(ui, |ui| {
        row_label(ui, &tr("options.quick_trace_with"));
        combo(
            ui,
            "opt_quick_trace",
            &mut t.quick_preset,
            &crate::trace::Preset::ALL,
            // The menu's names, without the dots of a menu item.
            |p| {
                tr(p.key())
                    .trim_end_matches("...")
                    .trim_end_matches('\u{2026}')
                    .to_string()
            },
            200.0,
        );
        ui.end_row();
    });
    chrome::section(ui, &tr("options.trace_starts_with"));
    ui.checkbox(&mut t.remove_background, tr("dialog.remove_background"));
    ui.checkbox(&mut t.delete_original, tr("dialog.delete_original"));
    hint(ui, "options.trace_hint");
}

/// Options > Global > Warnings: the messages that show.
pub fn warnings_page(app: &mut App, ui: &mut Ui) {
    let w = &mut app.settings.warnings;
    ui.label(tr("options.warnings_show"));
    ui.add_space(4.0);
    ui.checkbox(&mut w.unread_objects, tr("options.warn_unread_objects"));
    ui.checkbox(&mut w.missing_fonts, tr("options.warn_missing_fonts"));
    ui.checkbox(&mut w.shelf_remove, tr("options.warn_shelf_remove"));
    ui.add_space(6.0);
    hint(ui, "options.warnings_hint");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every new page draws, with nonsense values put right.
    #[test]
    fn the_global_pages_draw() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        app.settings.printing.sheet = 99;
        app.settings.formats.export_format = 99;
        app.settings.formats.pdf_standard = 42;
        for page in [
            printing_page as fn(&mut App, &mut Ui),
            file_formats_page,
            bitmap_tracing_page,
            warnings_page,
        ] {
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| page(&mut app, ui));
            out.textures_delta.clear();
        }
        assert_eq!(app.settings.printing.sheet, 0);
        assert_eq!(app.settings.formats.export_format, 0);
        assert_eq!(app.settings.formats.pdf_standard, 0);
    }
}
