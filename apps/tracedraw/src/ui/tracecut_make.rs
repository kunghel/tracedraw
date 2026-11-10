//! The settings of TraceCut's Make tools in the TraceCut window: eyelets,
//! the box maker and the folded tray.
//! See docs/behavior/tracecut.md.

use crate::app::Units;
use crate::i18n::tr;
use crate::tracecut::{ToolId, TraceCutState};
use crate::ui::tips::RichTip;
use crate::ui::tracecut::{length, row};
use egui::Ui;
use tracedraw_tracecut::boxes::{self, Sizes};
use tracedraw_tracecut::eyelets::{self, Along};
use tracedraw_tracecut::tray::{self, EarSides, Pair, Place};

/// A whole-number field with its tip.
fn whole(ui: &mut Ui, v: &mut u32, range: (u32, u32), tip: &str) {
    let mut x = f64::from(*v);
    let r = ui
        .add_sized(
            [110.0, 22.0],
            crate::ui::field::NumField::new(&mut x)
                .range(f64::from(range.0)..=f64::from(range.1))
                .max_decimals(0),
        )
        .rich_tip(crate::ui::tips::keyed(tip));
    if r.changed() && x.is_finite() {
        *v = x.round().clamp(f64::from(range.0), f64::from(range.1)) as u32;
    }
}

/// A list of `options`, each named by its i18n key.
fn choice<T: PartialEq + Copy>(ui: &mut Ui, id: &str, v: &mut T, options: &[(T, &str)], tip: &str) {
    let shown = options
        .iter()
        .find(|(o, _)| o == v)
        .map(|(_, k)| tr(k))
        .unwrap_or_default();
    crate::ui::keynav::ComboBox::from_id_salt(("tracecut", id))
        .selected_text(shown)
        .width(200.0)
        .show_ui(ui, |ui| {
            for (o, k) in options {
                ui.selectable_value(v, *o, tr(k));
            }
        })
        .response
        .rich_tip(crate::ui::tips::keyed(tip));
}

/// Box Maker: the sizes, the material, the fingers and the layout.
fn box_settings(ui: &mut Ui, units: Units, st: &mut TraceCutState) {
    let s = &mut st.settings.boxes;
    for (key, v) in [
        ("tracecut.box_width", &mut s.width),
        ("tracecut.box_depth", &mut s.depth),
        ("tracecut.box_height", &mut s.height),
    ] {
        row(ui, &tr(key), |ui| {
            length(ui, units, v, boxes::SIZE_RANGE, key)
        });
    }
    row(ui, &tr("tracecut.box_sizes"), |ui| {
        choice(
            ui,
            "box_sizes",
            &mut s.sizes,
            &[
                (Sizes::Outside, "tracecut.box_outside"),
                (Sizes::Inside, "tracecut.box_inside"),
            ],
            "tracecut.box_sizes",
        )
    });
    row(ui, &tr("tracecut.box_thickness"), |ui| {
        length(
            ui,
            units,
            &mut s.thickness,
            boxes::THICKNESS_RANGE,
            "tracecut.box_thickness",
        )
    });
    row(ui, &tr("tracecut.box_finger"), |ui| {
        length(
            ui,
            units,
            &mut s.finger,
            boxes::FINGER_RANGE,
            "tracecut.box_finger",
        )
    });
    row(ui, "", |ui| {
        ui.checkbox(&mut s.open_top, tr("tracecut.box_open_top"))
            .rich_tip(crate::ui::tips::keyed("tracecut.box_open_top"));
    });
    row(ui, &tr("tracecut.box_gap"), |ui| {
        length(ui, units, &mut s.gap, boxes::GAP_RANGE, "tracecut.box_gap")
    });
}

/// A length row: its label is the key's string, its tip the key's.
fn len_row(ui: &mut Ui, units: Units, key: &str, v: &mut f64, range: (f64, f64)) {
    row(ui, &tr(key), |ui| length(ui, units, v, range, key));
}

/// A tick box on a row of its own, its tip the key's.
fn tick(ui: &mut Ui, key: &str, v: &mut bool) {
    row(ui, "", |ui| {
        ui.checkbox(v, tr(key))
            .rich_tip(crate::ui::tips::keyed(key));
    });
}

/// Folded Tray: the face, the returns, the folds, the corners, the ears,
/// the holes, the window and the place. Returns whether "Size from
/// selection" was pressed.
fn tray_settings(ui: &mut Ui, units: Units, st: &mut TraceCutState, has_selection: bool) -> bool {
    let s = &mut st.settings.tray;
    len_row(
        ui,
        units,
        "tracecut.tray_width",
        &mut s.width,
        tray::SIZE_RANGE,
    );
    len_row(
        ui,
        units,
        "tracecut.tray_height",
        &mut s.height,
        tray::SIZE_RANGE,
    );
    let take = row(ui, "", |ui| {
        ui.add_enabled(
            has_selection,
            egui::Button::new(tr("tracecut.tray_take_size")),
        )
        .rich_tip(crate::ui::tips::keyed("tracecut.tray_take_size"))
        .clicked()
    });
    tick(ui, "tracecut.tray_per_side", &mut s.per_side);
    if s.per_side {
        for (i, key) in [
            "tracecut.tray_left",
            "tracecut.tray_right",
            "tracecut.tray_top",
            "tracecut.tray_bottom",
        ]
        .into_iter()
        .enumerate()
        {
            if let Some(v) = s.depths.get_mut(i) {
                len_row(ui, units, key, v, tray::DEPTH_RANGE);
            }
        }
    } else {
        len_row(
            ui,
            units,
            "tracecut.tray_depth",
            &mut s.depth,
            tray::DEPTH_RANGE,
        );
    }
    len_row(
        ui,
        units,
        "tracecut.tray_thickness",
        &mut s.thickness,
        tray::THICKNESS_RANGE,
    );
    tick(ui, "tracecut.tray_auto_allowance", &mut s.auto_allowance);
    if s.auto_allowance {
        // Shows the allowance worked out from the thickness.
        let mut shown = s.allowance();
        ui.add_enabled_ui(false, |ui| {
            len_row(
                ui,
                units,
                "tracecut.tray_allowance",
                &mut shown,
                tray::ALLOWANCE_RANGE,
            )
        });
    } else {
        len_row(
            ui,
            units,
            "tracecut.tray_allowance",
            &mut s.allowance,
            tray::ALLOWANCE_RANGE,
        );
    }
    row(ui, &tr("tracecut.tray_through"), |ui| {
        choice(
            ui,
            "tray_through",
            &mut s.through,
            &[
                (Pair::LeftRight, "tracecut.tray_left_right"),
                (Pair::TopBottom, "tracecut.tray_top_bottom"),
            ],
            "tracecut.tray_through",
        )
    });
    len_row(
        ui,
        units,
        "tracecut.tray_relief",
        &mut s.relief,
        tray::RELIEF_RANGE,
    );
    tick(ui, "tracecut.tray_flaps", &mut s.flaps);
    if s.flaps {
        len_row(
            ui,
            units,
            "tracecut.tray_flap_length",
            &mut s.flap_length,
            tray::FLAP_RANGE,
        );
    }
    tick(ui, "tracecut.tray_ears", &mut s.ears);
    if s.ears {
        row(ui, &tr("tracecut.tray_ear_sides"), |ui| {
            choice(
                ui,
                "tray_ear_sides",
                &mut s.ear_sides,
                &[
                    (EarSides::All, "tracecut.tray_all_sides"),
                    (EarSides::LeftRight, "tracecut.tray_left_right"),
                    (EarSides::TopBottom, "tracecut.tray_top_bottom"),
                ],
                "tracecut.tray_ear_sides",
            )
        });
        len_row(
            ui,
            units,
            "tracecut.tray_ear_width",
            &mut s.ear_width,
            tray::EAR_RANGE,
        );
        len_row(
            ui,
            units,
            "tracecut.tray_ear_length",
            &mut s.ear_length,
            tray::EAR_RANGE,
        );
        row(ui, &tr("tracecut.tray_ear_place"), |ui| {
            choice(
                ui,
                "tray_ear_place",
                &mut s.ear_by_count,
                &[
                    (false, "tracecut.tray_by_spacing"),
                    (true, "tracecut.tray_by_count"),
                ],
                "tracecut.tray_ear_place",
            )
        });
        if s.ear_by_count {
            row(ui, &tr("tracecut.tray_ear_count"), |ui| {
                whole(
                    ui,
                    &mut s.ear_count,
                    tray::COUNT_RANGE,
                    "tracecut.tray_ear_count",
                )
            });
        } else {
            len_row(
                ui,
                units,
                "tracecut.tray_ear_spacing",
                &mut s.ear_spacing,
                tray::SPACING_RANGE,
            );
        }
        len_row(
            ui,
            units,
            "tracecut.tray_ear_hole",
            &mut s.ear_hole,
            tray::HOLE_RANGE,
        );
        len_row(
            ui,
            units,
            "tracecut.tray_ear_hole_offset",
            &mut s.ear_hole_offset,
            tray::OFFSET_RANGE,
        );
    }
    tick(ui, "tracecut.tray_holes", &mut s.holes);
    if s.holes {
        len_row(
            ui,
            units,
            "tracecut.tray_hole_diameter",
            &mut s.hole_diameter,
            tray::HOLE_RANGE,
        );
        len_row(
            ui,
            units,
            "tracecut.tray_hole_offset",
            &mut s.hole_offset,
            tray::OFFSET_RANGE,
        );
        len_row(
            ui,
            units,
            "tracecut.tray_hole_spacing",
            &mut s.hole_spacing,
            tray::SPACING_RANGE,
        );
    }
    tick(ui, "tracecut.tray_window", &mut s.window);
    if s.window {
        len_row(
            ui,
            units,
            "tracecut.tray_window_inset",
            &mut s.window_inset,
            tray::INSET_RANGE,
        );
        len_row(
            ui,
            units,
            "tracecut.tray_window_radius",
            &mut s.window_radius,
            tray::RADIUS_RANGE,
        );
    }
    row(ui, &tr("tracecut.tray_place"), |ui| {
        choice(
            ui,
            "tray_place",
            &mut s.place,
            &[
                (Place::PageCorner, "tracecut.tray_place_page"),
                (Place::BesideSelection, "tracecut.tray_place_selection"),
            ],
            "tracecut.tray_place",
        )
    });
    len_row(ui, units, "tracecut.tray_gap", &mut s.gap, tray::GAP_RANGE);
    take
}

/// The settings of a Make tool; nothing for the other tools. Returns
/// whether Folded Tray's "Size from selection" was pressed.
pub fn settings(ui: &mut Ui, units: Units, st: &mut TraceCutState, has_selection: bool) -> bool {
    if st.tool == ToolId::FoldedTray {
        return tray_settings(ui, units, st, has_selection);
    }
    if st.tool == ToolId::BoxMaker {
        box_settings(ui, units, st);
    }
    if st.tool == ToolId::Eyelets {
        let s = &mut st.settings.eyelets;
        row(ui, &tr("tracecut.eyelet_along"), |ui| {
            choice(
                ui,
                "eyelet_along",
                &mut s.along,
                &[
                    (Along::Box, "tracecut.eyelet_along_box"),
                    (Along::Curve, "tracecut.eyelet_along_curve"),
                ],
                "tracecut.eyelet_along",
            )
        });
        row(ui, &tr("tracecut.eyelet_diameter"), |ui| {
            length(
                ui,
                units,
                &mut s.diameter,
                eyelets::DIAMETER_RANGE,
                "tracecut.eyelet_diameter",
            )
        });
        row(ui, &tr("tracecut.eyelet_inset"), |ui| {
            length(
                ui,
                units,
                &mut s.inset,
                eyelets::INSET_RANGE,
                "tracecut.eyelet_inset",
            )
        });
        row(ui, &tr("tracecut.eyelet_place"), |ui| {
            choice(
                ui,
                "eyelet_place",
                &mut s.by_count,
                &[
                    (false, "tracecut.eyelet_by_spacing"),
                    (true, "tracecut.eyelet_by_count"),
                ],
                "tracecut.eyelet_place",
            )
        });
        if s.by_count {
            row(ui, &tr("tracecut.eyelet_count"), |ui| {
                whole(
                    ui,
                    &mut s.count,
                    eyelets::COUNT_RANGE,
                    "tracecut.eyelet_count",
                )
            });
        } else {
            row(ui, &tr("tracecut.eyelet_spacing"), |ui| {
                length(
                    ui,
                    units,
                    &mut s.spacing,
                    eyelets::SPACING_RANGE,
                    "tracecut.eyelet_spacing",
                )
            });
        }
    }
    false
}
