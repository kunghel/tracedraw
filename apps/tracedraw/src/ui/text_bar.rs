//! The Text tool's property bar beyond the font, size, style and
//! alignment (#1822): the character and paragraph settings of the Text
//! docker (range kerning, character width, all caps, change case, line,
//! character and word spacing, the indents, the space before and after a
//! paragraph, tabs, columns, bullets, drop cap) and the Text docker and
//! Edit Text buttons, so the docker need not be opened for them. A "+"
//! button at the bar's end lists them, each with a box that shows or
//! hides it on the bar, and puts the bar back as it comes. What is hidden
//! is kept with the bars' settings ([`HIDDEN`]). The settings change the
//! selected text as in the docker: the characters selected while
//! editing, else the whole text; the paragraphs at the caret, else every
//! paragraph. See docs/behavior/toolbars.md, Text tool bar.

use crate::app::App;
use crate::i18n::tr;
use crate::ui::field::NumField;
use crate::ui::propbar::{pic_button, Pic};
use crate::ui::text_docker::{edit_paragraphs, edit_spans, ParaIcon};
use crate::ui::tips::RichTip;
use egui::Ui;
use tracedraw_core::{LeadingUnit, ParagraphStyle, Shape, ShapeKind, TextAlign};

/// The key the hidden items are kept under in `Settings::bar_items`.
pub const HIDDEN: &str = "text_hidden";

/// A separator between groups of items.
const SEP: &str = "sep";

/// The bar's optional items in their order, groups apart. Bullets and
/// drop cap come first, next to the alignment, so they stay in view
/// when the object fields of the Pick tool come before the text fields
/// (#3586, #3592).
pub const ITEMS: [&str; 23] = [
    "bullets",
    "drop_cap",
    SEP,
    "kerning",
    "char_width",
    "all_caps",
    "change_case",
    SEP,
    "line_spacing",
    "char_spacing",
    "word_spacing",
    SEP,
    "left_indent",
    "first_indent",
    "right_indent",
    "space_before",
    "space_after",
    SEP,
    "tabs",
    "columns",
    SEP,
    "text_docker",
    "edit_text",
];

/// The name of item `id`, as the docker and the menus call it.
pub fn name_key(id: &str) -> &'static str {
    match id {
        "kerning" => "docker.tracking",
        "char_width" => "docker.char_width",
        "all_caps" => "docker.all_caps",
        "change_case" => "menu.text.change_case",
        "line_spacing" => "docker.leading",
        "char_spacing" => "docker.char_spacing",
        "word_spacing" => "docker.word_spacing",
        "left_indent" => "docker.left_indent",
        "first_indent" => "docker.first_line_indent",
        "right_indent" => "docker.right_indent",
        "space_before" => "docker.space_before",
        "space_after" => "docker.space_after",
        "tabs" => "menu.text.tabs",
        "columns" => "menu.text.columns",
        "bullets" => "menu.text.bullets",
        "drop_cap" => "menu.text.drop_cap",
        "text_docker" => "toolbar.text_properties",
        _ => "menu.text.edit_text",
    }
}

/// Item `id`'s name without the colon or the dots of a menu item.
pub fn name(id: &str) -> String {
    tr(name_key(id))
        .trim_end_matches(['.', '\u{2026}', ':', '\u{FF1A}', ' '])
        .to_string()
}

/// Whether item `id` is on the bar.
pub fn shown(app: &App, id: &str) -> bool {
    !app.settings
        .bar_items
        .get(HIDDEN)
        .is_some_and(|h| h.iter().any(|x| x == id))
}

/// Show or hide item `id`.
pub fn set_shown(app: &mut App, id: &str, on: bool) {
    let mut hidden = app.settings.bar_items.remove(HIDDEN).unwrap_or_default();
    hidden.retain(|x| x != id);
    if !on {
        hidden.push(id.to_string());
    }
    if !hidden.is_empty() {
        app.settings.bar_items.insert(HIDDEN.to_string(), hidden);
    }
}

/// Every item back on the bar.
pub fn reset(app: &mut App) {
    app.settings.bar_items.remove(HIDDEN);
}

/// The text the bar reads: the active table cell's, else the first
/// selected text.
fn first_text(app: &App) -> Option<Shape> {
    app.active_cell_text_shape().or_else(|| {
        app.selected_shapes()
            .into_iter()
            .find(|s| matches!(s.kind, ShapeKind::Text { .. }))
    })
}

/// The largest font size of text `t`, points.
fn size_pt(t: &Shape) -> f64 {
    match &t.kind {
        ShapeKind::Text { spans, .. } => {
            spans.iter().map(|s| s.size_pt).fold(0.0, f64::max).max(1.0)
        }
        _ => 12.0,
    }
}

/// A number field `w` points wide with `suffix`, kept in `range`.
fn number(
    ui: &mut Ui,
    value: &mut f64,
    range: std::ops::RangeInclusive<f64>,
    suffix: &str,
    tip: &str,
) -> bool {
    let before = *value;
    ui.add_sized(
        [64.0, 22.0],
        NumField::new(value)
            .range(range)
            .max_decimals(1)
            .suffix(suffix.to_string()),
    )
    .rich_tip(tip);
    (*value - before).abs() > 1e-12
}

/// A distance field in the ruler unit.
fn length(ui: &mut Ui, app: &App, mm: &mut f64, tip: &str) -> bool {
    let u = app.units;
    let unit = u.mm();
    let mut v = *mm / unit;
    let d = app.settings.precision.min(6) as usize;
    let r = ui
        .add_sized(
            [72.0, 22.0],
            NumField::new(&mut v)
                .length_decimals(d)
                .suffix(format!(" {}", u.short()))
                .unit_mm(u.mm()),
        )
        .rich_tip(tip);
    if r.changed() && (v * unit - *mm).abs() > 1e-12 {
        *mm = v * unit;
        true
    } else {
        false
    }
}

/// A paragraph icon of the docker before a field.
fn icon(ui: &mut Ui, pic: ParaIcon, tip: &str) {
    let (r, resp) = ui.allocate_exact_size(egui::vec2(16.0, 22.0), egui::Sense::hover());
    for shape in crate::ui::text_docker::para_icon_shapes(pic, r, ui.visuals().text_color()) {
        ui.painter().add(shape);
    }
    resp.rich_tip(tip);
}

/// A short text before a field.
fn tag(ui: &mut Ui, text: &str, tip: &str) {
    ui.label(egui::RichText::new(text).strong().size(11.0))
        .rich_tip(tip);
}

/// The optional items of the Text tool's bar that are shown, then the
/// "+" button.
pub fn optional_part(app: &mut App, ui: &mut Ui) {
    let first = first_text(app);
    let span = first.as_ref().and_then(|t| app.shown_span(t));
    let para = first
        .as_ref()
        .and_then(|t| app.paragraph_style_of(t))
        .map(|(_, p)| p);
    let is_text = first.is_some();
    let is_para = first
        .as_ref()
        .is_some_and(|s| matches!(s.kind, ShapeKind::Text { frame: Some(_), .. }));
    let size = first.as_ref().map_or(12.0, size_pt);
    let mut p = para.clone().unwrap_or_default();
    let mut pc = false;
    // A separator only between two groups that show something.
    let mut pending_sep = false;
    let mut any = false;
    for id in ITEMS {
        if id == SEP {
            pending_sep = any;
            continue;
        }
        if !shown(app, id) {
            continue;
        }
        if pending_sep {
            crate::ui::propbar::sep(ui);
            pending_sep = false;
        }
        any = true;
        let tip = name(id);
        match id {
            "kerning" => {
                tag(ui, "AV", &tip);
                let mut v = span.as_ref().map_or(0.0, |s| s.tracking_pct);
                ui.add_enabled_ui(span.is_some(), |ui| {
                    if number(ui, &mut v, -50.0..=200.0, " %", &tip) {
                        edit_spans(app, move |sp| sp.tracking_pct = v);
                    }
                });
            }
            "char_width" => {
                crate::ui::text_docker::char_width_icon(ui).rich_tip(&tip);
                let mut v = span.as_ref().map_or(100.0, |s| s.width_pct);
                ui.add_enabled_ui(span.is_some(), |ui| {
                    let range = crate::ui::text_docker::CHAR_WIDTH_RANGE;
                    if number(ui, &mut v, range, " %", &tip) {
                        edit_spans(app, move |sp| sp.width_pct = v);
                    }
                });
            }
            "all_caps" => {
                let on = span.as_ref().is_some_and(|s| s.all_caps);
                let b = egui::Button::selectable(on, egui::RichText::new("AB").size(12.0));
                if ui
                    .add_enabled(span.is_some(), b)
                    .rich_tip(crate::ui::tips::keyed("docker.all_caps"))
                    .clicked()
                {
                    edit_spans(app, move |s| s.all_caps = !on);
                }
            }
            "change_case" => {
                let b = egui::Button::new(egui::RichText::new("Aa").size(12.0));
                if ui
                    .add_enabled(is_text, b)
                    .rich_tip(crate::shortcuts::shown_in(format!("{tip} (Shift+F3)")))
                    .clicked()
                {
                    app.dialog =
                        crate::ui::dialogs::Dialog::ChangeCase(crate::ops2::CaseMode::Sentence);
                }
            }
            "line_spacing" => {
                icon(ui, ParaIcon::Leading, &tip);
                let suffix = match p.leading_unit {
                    LeadingUnit::Points => " pt",
                    _ => " %",
                };
                ui.add_enabled_ui(para.is_some(), |ui| {
                    pc |= number(ui, &mut p.leading_pct, 0.0..=2000.0, suffix, &tip);
                });
            }
            "char_spacing" => {
                tag(ui, "A\u{2194}A", &tip);
                ui.add_enabled_ui(para.is_some(), |ui| {
                    pc |= number(ui, &mut p.char_spacing_pct, -100.0..=2000.0, " %", &tip);
                });
            }
            "word_spacing" => {
                tag(ui, "W\u{2194}W", &tip);
                ui.add_enabled_ui(para.is_some(), |ui| {
                    pc |= number(ui, &mut p.word_spacing_pct, 0.0..=2000.0, " %", &tip);
                });
            }
            "left_indent" | "first_indent" | "right_indent" => {
                let (pic, v) = match id {
                    "left_indent" => (ParaIcon::LeftIndent, &mut p.left_indent),
                    "first_indent" => (ParaIcon::FirstLineIndent, &mut p.first_line_indent),
                    _ => (ParaIcon::RightIndent, &mut p.right_indent),
                };
                icon(ui, pic, &tip);
                ui.add_enabled_ui(para.is_some(), |ui| {
                    pc |= length(ui, app, v, &tip);
                });
            }
            "space_before" | "space_after" => {
                let before = id == "space_before";
                let pic = if before {
                    ParaIcon::SpaceBefore
                } else {
                    ParaIcon::SpaceAfter
                };
                icon(ui, pic, &tip);
                ui.add_enabled_ui(para.is_some(), |ui| {
                    pc |= crate::ui::tips::tip_over(ui, &tip, |ui| {
                        crate::ui::text_docker::space_field(ui, app, &mut p, before, size)
                    });
                });
            }
            "tabs" | "columns" => {
                let b = egui::Button::new(&tip);
                let enabled = if id == "tabs" { is_text } else { is_para };
                if ui.add_enabled(enabled, b).rich_tip(&tip).clicked() {
                    app.dialog = if id == "tabs" {
                        crate::ui::dialogs::Dialog::TextTabs
                    } else {
                        crate::ui::dialogs::Dialog::TextColumns
                    };
                }
            }
            "bullets" => {
                let resp = pic_button(ui, Pic::Bullets, &tr("menu.text.bullets"), is_para, false);
                record_bullets(ui, resp.rect);
                if resp.clicked() {
                    app.dialog = crate::ui::dialogs::Dialog::TextBullets;
                }
            }
            "drop_cap" => {
                if pic_button(ui, Pic::DropCap, &tr("menu.text.drop_cap"), is_para, false).clicked()
                {
                    app.dialog = crate::ui::dialogs::Dialog::TextDropCap;
                }
            }
            "text_docker" => {
                let open = app.docker_shown(crate::app::DockerTab::Text);
                let tip = crate::shortcuts::shown_in(format!(
                    "{} (Ctrl+T)",
                    tr("toolbar.text_properties")
                ));
                if pic_button(ui, Pic::TextProperties, &tip, true, open).clicked() {
                    app.toggle_docker(crate::app::DockerTab::Text);
                }
            }
            _ => {
                let tip = crate::shortcuts::shown_in(format!(
                    "{} (Ctrl+Shift+T)",
                    tr("menu.text.edit_text")
                ));
                if pic_button(ui, Pic::EditText, &tip, is_text, false).clicked() {
                    app.open_edit_text_dialog();
                }
            }
        }
    }
    if pc {
        if let Some(before) = para {
            apply_paragraph_change(app, &before, &p);
        }
    }
    customize_button(app, ui);
}

/// Send what changed between `before` and `after` to the paragraphs, each
/// keeping the rest (as the docker does).
fn apply_paragraph_change(app: &mut App, before: &ParagraphStyle, after: &ParagraphStyle) {
    let before = tracedraw_core::ParagraphFormat::of(TextAlign::Left, before);
    let after = tracedraw_core::ParagraphFormat::of(TextAlign::Left, after);
    edit_paragraphs(app, move |q| {
        let mut f = tracedraw_core::ParagraphFormat::of(TextAlign::Left, q);
        f.take_changes(&before, &after);
        f.apply_to(q);
    });
}

/// The "+" button: every optional item with a box that shows or hides
/// it, and the bar back as it comes.
fn customize_button(app: &mut App, ui: &mut Ui) {
    let resp = ui.menu_button(egui::RichText::new("+").size(16.0), |ui| {
        ui.set_min_width(200.0);
        for id in ITEMS {
            if id == SEP {
                ui.separator();
                continue;
            }
            let mut on = shown(app, id);
            if ui.checkbox(&mut on, name(id)).changed() {
                set_shown(app, id, on);
            }
        }
        ui.separator();
        if ui.button(tr("options.reset_to_default")).clicked() {
            reset(app);
            ui.close();
        }
    });
    let rect = resp.response.rect;
    resp.response.rich_tip(tr("toolbox.quick_customize"));
    record_plus(ui, rect);
}

/// Where the "+" button was drawn last, for tests.
#[cfg(test)]
pub(crate) fn plus_rect(ctx: &egui::Context) -> Option<egui::Rect> {
    ctx.data(|d| d.get_temp(egui::Id::new("text_bar_plus")))
}

#[cfg(test)]
fn record_plus(ui: &Ui, rect: egui::Rect) {
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::new("text_bar_plus"), rect));
}

#[cfg(not(test))]
fn record_plus(_ui: &Ui, _rect: egui::Rect) {}

/// Where the Bullets button was drawn last, for tests.
#[cfg(test)]
pub(crate) fn bullets_rect(ctx: &egui::Context) -> Option<egui::Rect> {
    ctx.data(|d| d.get_temp(egui::Id::new("text_bar_bullets")))
}

#[cfg(test)]
fn record_bullets(ui: &Ui, rect: egui::Rect) {
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::new("text_bar_bullets"), rect));
}

#[cfg(not(test))]
fn record_bullets(_ui: &Ui, _rect: egui::Rect) {}
