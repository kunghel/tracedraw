//! The property bar of a text fitted to a path (Pick tool): orientation,
//! vertical placement, placement along the path, distance from it, offset
//! along it, the two mirrors and Straighten Text.

use crate::app::App;
use crate::i18n::tr;
use crate::ui::field::NumField;
use crate::ui::propbar::{pic_button, sep, Pic};
use crate::ui::tips::RichTip;
use egui::Ui;
use tracedraw_core::document::{PathTextOrientation, PathTextVertical, TextAlign};

const ORIENTATIONS: [(PathTextOrientation, &str); 4] = [
    (PathTextOrientation::Rotate, "toolbar.path_rotate"),
    (
        PathTextOrientation::VerticalSkew,
        "toolbar.path_vertical_skew",
    ),
    (
        PathTextOrientation::HorizontalSkew,
        "toolbar.path_horizontal_skew",
    ),
    (PathTextOrientation::Upright, "toolbar.path_upright"),
];

const VERTICALS: [(PathTextVertical, &str); 4] = [
    (PathTextVertical::Baseline, "toolbar.path_on_baseline"),
    (PathTextVertical::Ascender, "toolbar.path_on_ascender"),
    (PathTextVertical::Descender, "toolbar.path_on_descender"),
    (PathTextVertical::Center, "toolbar.path_on_center"),
];

/// Where along the path the text sits: an alignment, or on a closed
/// path centred on its top or bottom.
#[derive(Clone, Copy, PartialEq)]
enum Placement {
    Along(TextAlign),
    Top,
    Bottom,
}

const PLACEMENTS: [(Placement, &str); 5] = [
    (Placement::Along(TextAlign::Left), "toolbar.path_place_left"),
    (
        Placement::Along(TextAlign::Center),
        "toolbar.path_place_center",
    ),
    (
        Placement::Along(TextAlign::Right),
        "toolbar.path_place_right",
    ),
    (Placement::Top, "toolbar.path_place_top"),
    (Placement::Bottom, "toolbar.path_place_bottom"),
];

/// A drop-down list of `items`; returns the one picked when it changed.
fn choice<T: Copy + PartialEq>(
    ui: &mut Ui,
    id: &str,
    tip: &str,
    current: T,
    items: &[(T, &str)],
    width: f32,
) -> Option<T> {
    let mut picked = None;
    let label = items
        .iter()
        .find(|(v, _)| *v == current)
        .map(|(_, k)| tr(k))
        .unwrap_or_default();
    egui::ComboBox::from_id_salt(id)
        .width(width)
        .selected_text(label)
        .show_ui(ui, |ui| {
            for (v, key) in items {
                if ui.selectable_label(*v == current, tr(key)).clicked() && *v != current {
                    picked = Some(*v);
                }
            }
        })
        .response
        .rich_tip(tip);
    picked
}

pub fn text_path_bar(app: &mut App, ui: &mut Ui) {
    let Some(ft) = app.fitted_text() else {
        return;
    };
    let tp = ft.on_path.clone();
    if let Some(o) = choice(
        ui,
        "path_orientation",
        &tr("toolbar.path_orientation"),
        tp.orientation,
        &ORIENTATIONS,
        130.0,
    ) {
        app.edit_fitted_text(|t, _| t.orientation = o);
    }
    if let Some(v) = choice(
        ui,
        "path_vertical",
        &tr("toolbar.path_vertical"),
        tp.vertical,
        &VERTICALS,
        130.0,
    ) {
        app.edit_fitted_text(|t, _| t.vertical = v);
    }
    let along = match ft.align {
        TextAlign::Center | TextAlign::Right => ft.align,
        _ => TextAlign::Left,
    };
    let current = match app.fitted_text_round_placement() {
        Some(false) => Placement::Top,
        Some(true) => Placement::Bottom,
        None => Placement::Along(along),
    };
    // Top and bottom only on a closed path.
    let shown = if app.fitted_text_closed() {
        &PLACEMENTS[..]
    } else {
        &PLACEMENTS[..3]
    };
    match choice(
        ui,
        "path_placement",
        &tr("toolbar.path_placement"),
        current,
        shown,
        110.0,
    ) {
        Some(Placement::Along(a)) => app.edit_fitted_text(|_, align| *align = a),
        Some(Placement::Top) => app.place_fitted_text_round(false),
        Some(Placement::Bottom) => app.place_fitted_text_round(true),
        None => {}
    }
    sep(ui);
    let u = app.units;
    for (key, value, is_distance) in [
        ("toolbar.path_distance", tp.distance, true),
        ("toolbar.path_offset", tp.offset, false),
    ] {
        let mut v = u.from_mm(value);
        let r = ui
            .add(
                NumField::new(&mut v)
                    .speed(0.1)
                    .suffix(format!(" {}", u.short())),
            )
            .rich_tip(tr(key));
        if r.changed() {
            let mm = u.to_mm(v);
            app.edit_fitted_text(|t, _| {
                if is_distance {
                    t.set_distance(mm);
                } else {
                    t.offset = mm;
                }
            });
        }
    }
    // Tick snapping: a drag off the path moves the text in steps.
    let snap = app.settings.path_tick_snap;
    if ui
        .selectable_label(snap, tr("toolbar.path_tick_snap"))
        .rich_tip(tr("toolbar.path_tick_snap_tip"))
        .clicked()
    {
        app.settings.path_tick_snap = !snap;
    }
    let mut step = u.from_mm(app.settings.path_tick_mm);
    let r = ui
        .add_enabled(
            snap,
            NumField::new(&mut step)
                .speed(0.1)
                .suffix(format!(" {}", u.short())),
        )
        .rich_tip(tr("toolbar.path_tick_spacing"));
    if r.changed() {
        let mm = u.to_mm(step);
        if mm.is_finite() && mm > 0.0 {
            app.settings.path_tick_mm = mm.min(1000.0);
        }
    }
    sep(ui);
    let (h, v) = tp.mirrors();
    if pic_button(ui, Pic::MirrorH, &tr("toolbar.mirror_h"), true, h).clicked() {
        app.mirror_fitted_text(!h, v);
    }
    if pic_button(ui, Pic::MirrorV, &tr("toolbar.mirror_v"), true, v).clicked() {
        app.mirror_fitted_text(h, !v);
    }
    sep(ui);
    if ui
        .button(tr("menu.text.straighten_text"))
        .rich_tip(tr("toolbar.path_straighten_tip"))
        .clicked()
    {
        app.straighten_text();
    }
}
