//! The Grid panel (Window > Panels > Grid, #4395): the document grid's
//! visibility, snapping and axis controls, presets (built in and saved),
//! three modes (Automatic, Basic, Advanced) with their settings, the
//! grid's origin, and how its lines and subdivisions look. Every change
//! shows on the canvas at once and is one undo step.

use crate::app::{App, Units};
use crate::i18n::{tr, trf};
use crate::settings::GridPreset;
use crate::ui::btn_icons::Ib;
use crate::ui::docker_form::{indented, row, section};
use crate::ui::tips::{pair, RichTip};
use egui::Ui;
use tracedraw_core::document::{GridDisplay, GridKind, GridMode, GridSettings, Metadata};
use tracedraw_core::{Color, Command};

/// The presets built in: a name key and the grid it sets.
pub fn builtin_presets() -> Vec<(&'static str, GridSettings)> {
    let px = Units::Pixels.mm();
    let base = GridSettings::default();
    vec![
        ("grid.p_default", base),
        (
            "grid.p_auto",
            GridSettings {
                mode: GridMode::Automatic,
                ..base
            },
        ),
        (
            "grid.p_icons",
            GridSettings {
                spacing_x: 24.0 * px,
                spacing_y: 24.0 * px,
                divisions_x: 24,
                divisions_y: 24,
                ..base
            },
        ),
        (
            "grid.p_layout",
            GridSettings {
                mode: GridMode::Advanced,
                spacing_x: 20.0,
                spacing_y: 5.0,
                gutter_x: 5.0,
                divisions_x: 1,
                divisions_y: 1,
                ..base
            },
        ),
        (
            "grid.p_isometric",
            GridSettings {
                mode: GridMode::Advanced,
                kind: GridKind::Isometric,
                uniform: true,
                third_axis: true,
                ..base
            },
        ),
        (
            "grid.p_pixel_art",
            GridSettings {
                mode: GridMode::Advanced,
                kind: GridKind::Dimetric,
                spacing_x: 8.0 * px,
                spacing_y: 8.0 * px,
                uniform: true,
                third_axis: true,
                ..base
            },
        ),
        (
            "grid.p_trimetric",
            GridSettings {
                mode: GridMode::Advanced,
                kind: GridKind::Trimetric,
                spacing_x: 10.0,
                spacing_y: 7.0,
                third_axis: true,
                ..base
            },
        ),
        (
            "grid.p_oblique",
            GridSettings {
                mode: GridMode::Advanced,
                kind: GridKind::Oblique,
                spacing_x: 10.0,
                spacing_y: 5.0,
                ..base
            },
        ),
    ]
}

/// `preset` laid over `current`: the document grid's settings change, the
/// baseline grid's and the pixel grid's stay.
pub fn apply_preset(current: &GridSettings, preset: &GridSettings) -> GridSettings {
    GridSettings {
        baseline_spacing: current.baseline_spacing,
        baseline_start: current.baseline_start,
        baseline_color: current.baseline_color,
        pixel_color: current.pixel_color,
        pixel_opacity: current.pixel_opacity,
        ..*preset
    }
}

/// `a` and `b` set the same document grid (baseline and pixel grid
/// aside): the preset the grid matches shows in the list.
pub fn same_grid(a: &GridSettings, b: &GridSettings) -> bool {
    apply_preset(a, b) == *a
}

fn kind_key(k: GridKind) -> &'static str {
    match k {
        GridKind::Standard => "grid.kind_standard",
        GridKind::Isometric => "grid.kind_isometric",
        GridKind::Dimetric => "grid.kind_dimetric",
        GridKind::Trimetric => "grid.kind_trimetric",
        GridKind::Oblique => "grid.kind_oblique",
        GridKind::Custom => "grid.kind_custom",
    }
}

fn mode_key(m: GridMode) -> &'static str {
    match m {
        GridMode::Automatic => "grid.mode_automatic",
        GridMode::Basic => "grid.mode_basic",
        GridMode::Advanced => "grid.mode_advanced",
    }
}

/// A distance field in the ruler unit (through the drawing scale); the
/// new value in mm when it changed.
fn distance(ui: &mut Ui, units: Units, mm: f64, tip: &str, min: f64) -> Option<f64> {
    let unit_mm = units.world_mm();
    let unit_mm = if unit_mm.is_finite() && unit_mm > 0.0 {
        unit_mm
    } else {
        1.0
    };
    let mut v = mm / unit_mm;
    let r = ui
        .add(
            crate::ui::field::NumField::new(&mut v)
                .speed(0.1)
                .max_decimals(4)
                .range(min / unit_mm..=100_000.0)
                .suffix(format!(" {}", units.short())),
        )
        .rich_tip(tip);
    r.changed().then_some(v * unit_mm)
}

fn count(ui: &mut Ui, n: &mut u32, tip: &str) -> bool {
    ui.add(crate::ui::field::NumField::new(n).range(1.0..=100.0))
        .rich_tip(tip)
        .changed()
}

fn opacity(ui: &mut Ui, v: &mut f64, tip: &str) -> bool {
    let mut pc = (*v * 100.0).clamp(0.0, 100.0);
    let changed = crate::ui::tips::tip_over(ui, tip, |ui| {
        ui.add(crate::ui::Slider::new(&mut pc, 0.0..=100.0).suffix(" %"))
            .changed()
    });
    if changed {
        *v = (pc / 100.0).clamp(0.0, 1.0);
    }
    changed
}

fn color(ui: &mut Ui, c: Option<Color>, fallback: Color, tip: &str) -> Option<Color> {
    let mut v = c.unwrap_or(fallback);
    let changed = crate::ui::tips::tip_over(ui, tip, |ui| {
        crate::ui::color_picker::color_button(ui, &mut v)
    });
    changed.then_some(v)
}

/// The grey the grid lines take without a colour of their own.
fn default_line() -> Color {
    Color::rgb8(0xDC, 0xDC, 0xDC)
}

/// The grey of the grid's dots without a colour of their own.
fn default_dot() -> Color {
    Color::rgb8(0x8C, 0x8C, 0x8C)
}

/// The Grid panel.
pub fn grid_panel(app: &mut App, ui: &mut Ui) {
    crate::ui::docker_form::begin_page(ui);
    crate::ui::docker_form::fit_sliders(ui);
    let mut meta: Metadata = app.doc().metadata.clone();
    let units = app.units;
    let mut g = meta.grid;

    // Visibility, snapping and the axis controls.
    ui.checkbox(&mut app.show_grid, tr("grid.show"))
        .rich_tip(pair("grid.show", "grid.show_tip"));
    ui.checkbox(&mut app.snap.grid, tr("grid.snap"))
        .rich_tip(pair("grid.snap", "grid.snap_tip"));
    if ui
        .checkbox(&mut app.grid_axis_controls, tr("grid.axis_controls"))
        .rich_tip(pair("grid.axis_controls", "grid.axis_controls_tip"))
        .changed()
        && app.grid_axis_controls
    {
        app.show_grid = true;
    }

    // Presets: built in, then the user's own.
    section(ui, Some(&tr("grid.preset")));
    let builtins = builtin_presets();
    let own = app.settings.grid_presets.clone();
    let current_builtin = builtins.iter().find(|(_, p)| same_grid(&g, p));
    let current_own = own.iter().position(|p| same_grid(&g, &p.grid));
    let shown = match (current_own, current_builtin) {
        (Some(i), _) => own[i].name.clone(),
        (None, Some((k, _))) => tr(k),
        _ => tr("grid.preset_none"),
    };
    crate::ui::keynav::ComboBox::from_id_salt("grid_preset")
        .selected_text(shown)
        .width(crate::ui::docker_form::fit_w(ui, 220.0))
        .show_ui(ui, |ui| {
            for (k, p) in &builtins {
                if ui
                    .selectable_label(same_grid(&g, p), tr(k))
                    .rich_tip(pair("grid.preset", "grid.preset_tip"))
                    .clicked()
                {
                    g = apply_preset(&g, p);
                }
            }
            if !own.is_empty() {
                ui.separator();
            }
            for p in &own {
                if ui
                    .selectable_label(same_grid(&g, &p.grid), &p.name)
                    .rich_tip(pair("grid.preset", "grid.preset_tip"))
                    .clicked()
                {
                    g = apply_preset(&g, &p.grid);
                }
            }
        })
        .response
        .rich_tip(pair("grid.preset", "grid.preset_tip"));
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut app.grid_preset_name)
                .hint_text(tr("grid.preset_name_hint"))
                .desired_width(crate::ui::docker_form::fit_w(ui, 160.0)),
        )
        .rich_tip(pair("grid.preset_name_hint", "grid.preset_name_tip"));
    });
    ui.horizontal_wrapped(|ui| {
        let name = app.grid_preset_name.trim().to_string();
        if ui
            .add_enabled(!name.is_empty(), Ib::new("grid.save_preset"))
            .rich_tip(pair("grid.save_preset", "grid.save_preset_tip"))
            .rich_disabled_tip(pair("grid.save_preset", "grid.save_preset_tip"))
            .clicked()
        {
            save_preset(&mut app.settings.grid_presets, &name, g);
            app.grid_preset_name.clear();
        }
        let renaming = current_own.filter(|_| !name.is_empty());
        if ui
            .add_enabled(renaming.is_some(), Ib::new("grid.rename_preset"))
            .rich_tip(pair("grid.rename_preset", "grid.rename_preset_tip"))
            .rich_disabled_tip(pair("grid.rename_preset", "grid.rename_preset_tip"))
            .clicked()
        {
            if let Some(i) = renaming {
                app.settings.grid_presets[i].name = name.clone();
                app.grid_preset_name.clear();
            }
        }
        if ui
            .add_enabled(current_own.is_some(), Ib::new("grid.delete_preset"))
            .rich_tip(pair("grid.delete_preset", "grid.delete_preset_tip"))
            .rich_disabled_tip(pair("grid.delete_preset", "grid.delete_preset_tip"))
            .clicked()
        {
            if let Some(i) = current_own {
                app.settings.grid_presets.remove(i);
            }
        }
    });

    // The mode.
    section(ui, Some(&tr("grid.mode")));
    ui.horizontal_wrapped(|ui| {
        for m in GridMode::ALL {
            let key = mode_key(m);
            if ui
                .selectable_label(g.mode == m, tr(key))
                .rich_tip(pair(key, &format!("{key}_tip")))
                .clicked()
            {
                g.mode = m;
            }
        }
    });
    match g.mode {
        GridMode::Automatic => {
            let (s, d) =
                crate::grid_geom::auto_spacing(f64::from(app.view.zoom), units.world_mm());
            let fmt = |mm: f64| {
                let v = units.world_from_mm(mm);
                format!("{} {}", trim_number(v), units.short())
            };
            ui.label(
                egui::RichText::new(trf(
                    "grid.auto_info",
                    &[
                        ("size", &fmt(s)),
                        ("n", &d.to_string()),
                        ("sub", &fmt(s / f64::from(d))),
                    ],
                ))
                .color(crate::theme::Tokens::text_dim()),
            )
            .rich_tip(pair("grid.mode_automatic", "grid.mode_automatic_tip"));
        }
        GridMode::Basic => {
            row(ui, &tr("grid.spacing_h"), |ui| {
                if let Some(v) = distance(ui, units, g.spacing_x, &pair("grid.spacing_h", "grid.spacing_tip"), 0.001) {
                    set_spacing(&mut g, 0, v);
                }
            });
            row(ui, &tr("grid.spacing_v"), |ui| {
                if let Some(v) = distance(ui, units, g.spacing_y, &pair("grid.spacing_v", "grid.spacing_tip"), 0.001) {
                    set_spacing(&mut g, 1, v);
                }
            });
            row(ui, &tr("grid.divisions"), |ui| {
                count(ui, &mut g.divisions_x, &pair("grid.divisions", "grid.divisions_tip"));
            });
            indented(ui, |ui| {
                ui.checkbox(&mut g.fixed_ratio, tr("grid.fixed_ratio"))
                    .rich_tip(pair("grid.fixed_ratio", "grid.fixed_ratio_tip"));
            });
        }
        GridMode::Advanced => advanced(ui, units, &mut g),
    }

    // The origin.
    section(ui, Some(&tr("grid.origin")));
    row(ui, &tr("grid.offset_x"), |ui| {
        if let Some(v) = distance(ui, units, g.offset_x, &pair("grid.offset_x", "grid.offset_tip"), -100_000.0) {
            g.offset_x = v;
        }
    });
    row(ui, &tr("grid.offset_y"), |ui| {
        if let Some(v) = distance(ui, units, g.offset_y, &pair("grid.offset_y", "grid.offset_tip"), -100_000.0) {
            g.offset_y = v;
        }
    });
    indented(ui, |ui| {
        if ui
            .add_enabled(g.offset_x != 0.0 || g.offset_y != 0.0, Ib::new("grid.reset_origin"))
            .rich_tip(pair("grid.reset_origin", "grid.reset_origin_tip"))
            .rich_disabled_tip(pair("grid.reset_origin", "grid.reset_origin_tip"))
            .clicked()
        {
            g.offset_x = 0.0;
            g.offset_y = 0.0;
        }
    });

    // How it looks.
    section(ui, Some(&tr("grid.appearance")));
    row(ui, &tr("grid.show_as"), |ui| {
        ui.radio_value(&mut g.display, GridDisplay::Lines, tr("grid.lines"))
            .rich_tip(pair("grid.lines", "grid.lines_tip"));
        ui.radio_value(&mut g.display, GridDisplay::Dots, tr("grid.dots"))
            .rich_tip(pair("grid.dots", "grid.dots_tip"));
    });
    let fallback = if g.display == GridDisplay::Dots {
        default_dot()
    } else {
        default_line()
    };
    row(ui, &tr("grid.line_color"), |ui| {
        if let Some(c) = color(ui, g.line_color, fallback, &pair("grid.line_color", "grid.line_color_tip")) {
            g.line_color = Some(c);
        }
        opacity(ui, &mut g.line_opacity, &pair("grid.opacity", "grid.opacity_tip"));
    });
    indented(ui, |ui| {
        ui.checkbox(&mut g.link_colors, tr("grid.link_colors"))
            .rich_tip(pair("grid.link_colors", "grid.link_colors_tip"));
    });
    row(ui, &tr("grid.sub_color"), |ui| {
        let base = g.line_color.unwrap_or(fallback);
        let shown = if g.link_colors { None } else { g.sub_color };
        ui.add_enabled_ui(!g.link_colors, |ui| {
            if let Some(c) = color(ui, shown, base, &pair("grid.sub_color", "grid.sub_color_tip")) {
                g.sub_color = Some(c);
            }
        });
        opacity(ui, &mut g.sub_opacity, &pair("grid.opacity", "grid.opacity_tip"));
    });
    indented(ui, |ui| {
        let custom = g.line_color.is_some()
            || g.sub_color.is_some()
            || g.line_opacity != 1.0
            || g.sub_opacity != 0.5
            || !g.link_colors;
        if ui
            .add_enabled(custom, Ib::new("grid.reset_colors"))
            .rich_tip(pair("grid.reset_colors", "grid.reset_colors_tip"))
            .rich_disabled_tip(pair("grid.reset_colors", "grid.reset_colors_tip"))
            .clicked()
        {
            let d = GridSettings::default();
            g.line_color = d.line_color;
            g.sub_color = d.sub_color;
            g.line_opacity = d.line_opacity;
            g.sub_opacity = d.sub_opacity;
            g.link_colors = d.link_colors;
        }
    });
    crate::ui::docker_form::end_page(ui);

    if g != meta.grid {
        // A grid changed with the grid hidden shows, so the change is
        // seen on the canvas.
        app.show_grid = true;
        meta.grid = g;
        app.run(Command::SetMetadata { metadata: meta });
    }
}

/// Advanced mode: the grid's kind, the axes' spacing, subdivisions,
/// gutter and angle, uniform axes, fixed proportions and the third
/// direction.
fn advanced(ui: &mut Ui, units: Units, g: &mut GridSettings) {
    row(ui, &tr("grid.type"), |ui| {
        crate::ui::keynav::ComboBox::from_id_salt("grid_kind")
            .selected_text(tr(kind_key(g.kind)))
            .width(crate::ui::docker_form::fit_w(ui, 160.0))
            .show_ui(ui, |ui| {
                for k in GridKind::ALL {
                    if ui
                        .selectable_label(g.kind == k, tr(kind_key(k)))
                        .rich_tip(pair("grid.type", "grid.type_tip"))
                        .clicked()
                    {
                        set_kind(g, k);
                    }
                }
            })
            .response
            .rich_tip(pair("grid.type", "grid.type_tip"));
    });
    indented(ui, |ui| {
        ui.checkbox(&mut g.uniform, tr("grid.uniform"))
            .rich_tip(pair("grid.uniform", "grid.uniform_tip"));
    });
    indented(ui, |ui| {
        ui.checkbox(&mut g.fixed_ratio, tr("grid.fixed_ratio"))
            .rich_tip(pair("grid.fixed_ratio", "grid.fixed_ratio_tip"));
    });
    let angled = g.kind != GridKind::Standard;
    if angled {
        indented(ui, |ui| {
            ui.checkbox(&mut g.third_axis, tr("grid.third_axis"))
                .rich_tip(pair("grid.third_axis", "grid.third_axis_tip"));
        });
    }
    let axes: &[usize] = if g.uniform { &[0] } else { &[0, 1] };
    for &k in axes {
        let title = if g.uniform {
            tr("grid.both_axes")
        } else if k == 0 {
            tr("grid.first_axis")
        } else {
            tr("grid.second_axis")
        };
        ui.add_space(2.0);
        ui.label(egui::RichText::new(title).strong());
        let (spacing, divisions, gutter, angle) = if k == 0 {
            (g.spacing_x, g.divisions_x, g.gutter_x, g.angle_x)
        } else {
            (g.spacing_y, g.divisions_y, g.gutter_y, g.angle_y)
        };
        row(ui, &tr("grid.spacing"), |ui| {
            if let Some(v) = distance(ui, units, spacing, &pair("grid.spacing", "grid.spacing_tip"), 0.001) {
                set_spacing(g, k, v);
            }
        });
        let mut d = divisions;
        row(ui, &tr("grid.divisions"), |ui| {
            if count(ui, &mut d, &pair("grid.divisions", "grid.divisions_tip")) {
                if k == 0 {
                    g.divisions_x = d;
                } else {
                    g.divisions_y = d;
                }
            }
        });
        row(ui, &tr("grid.gutter"), |ui| {
            if let Some(v) = distance(ui, units, gutter, &pair("grid.gutter", "grid.gutter_tip"), 0.0) {
                if k == 0 {
                    g.gutter_x = v.max(0.0);
                } else {
                    g.gutter_y = v.max(0.0);
                }
            }
        });
        if g.kind == GridKind::Custom {
            let mut a = angle;
            row(ui, &tr("grid.angle"), |ui| {
                if ui
                    .add(
                        crate::ui::field::NumField::new(&mut a)
                            .range(-360.0..=360.0)
                            .max_decimals(3)
                            .suffix("°"),
                    )
                    .rich_tip(pair("grid.angle", "grid.angle_tip"))
                    .changed()
                {
                    if k == 0 {
                        g.angle_x = a;
                    } else {
                        g.angle_y = a;
                    }
                }
            });
        }
    }
}

/// A new kind: Custom starts from the angles the grid had.
pub fn set_kind(g: &mut GridSettings, k: GridKind) {
    if k == GridKind::Custom {
        let (a, b) = g.kind.angles().unwrap_or((g.angle_x, g.angle_y));
        g.angle_x = a;
        g.angle_y = b;
    }
    g.kind = k;
}

/// The spacing of axis `k` set to `v` mm, the other axis scaled in
/// proportion with Fixed Ratio on.
pub fn set_spacing(g: &mut GridSettings, k: usize, v: f64) {
    if !(v.is_finite() && v > 0.0) {
        return;
    }
    let (old, other) = if k == 0 {
        (g.spacing_x, g.spacing_y)
    } else {
        (g.spacing_y, g.spacing_x)
    };
    let scaled = if g.fixed_ratio && old > 0.0 {
        other * v / old
    } else {
        other
    };
    if k == 0 {
        g.spacing_x = v;
        g.spacing_y = scaled;
    } else {
        g.spacing_y = v;
        g.spacing_x = scaled;
    }
}

/// `grid` saved as `name`: a preset of that name is replaced.
pub fn save_preset(list: &mut Vec<GridPreset>, name: &str, grid: GridSettings) {
    let preset = GridPreset {
        name: name.to_string(),
        grid,
    };
    match list.iter_mut().find(|p| p.name == name) {
        Some(p) => *p = preset,
        None => list.push(preset),
    }
}

/// A number with no trailing zeros, at most four decimals.
fn trim_number(v: f64) -> String {
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".to_string()
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_preset_changes_the_document_grid_only() {
        let mut current = GridSettings {
            baseline_spacing: 7.0,
            pixel_opacity: 0.3,
            ..Default::default()
        };
        current.spacing_x = 3.0;
        let iso = builtin_presets()
            .into_iter()
            .find(|(k, _)| *k == "grid.p_isometric")
            .unwrap()
            .1;
        let g = apply_preset(&current, &iso);
        assert_eq!(g.kind, GridKind::Isometric);
        assert_eq!(g.mode, GridMode::Advanced);
        assert_eq!(g.baseline_spacing, 7.0);
        assert_eq!(g.pixel_opacity, 0.3);
        assert!(same_grid(&g, &iso));
        assert!(!same_grid(&current, &iso));
        // The default grid is the Default preset.
        assert!(same_grid(&GridSettings::default(), &builtin_presets()[0].1));
    }

    #[test]
    fn fixed_ratio_scales_the_other_axis() {
        let mut g = GridSettings {
            spacing_x: 10.0,
            spacing_y: 5.0,
            fixed_ratio: true,
            ..Default::default()
        };
        set_spacing(&mut g, 0, 20.0);
        assert_eq!((g.spacing_x, g.spacing_y), (20.0, 10.0));
        g.fixed_ratio = false;
        set_spacing(&mut g, 1, 4.0);
        assert_eq!((g.spacing_x, g.spacing_y), (20.0, 4.0));
        set_spacing(&mut g, 1, -1.0);
        assert_eq!(g.spacing_y, 4.0);
    }

    #[test]
    fn custom_starts_from_the_kinds_angles_and_presets_save_by_name() {
        let mut g = GridSettings {
            kind: GridKind::Isometric,
            ..Default::default()
        };
        set_kind(&mut g, GridKind::Custom);
        assert_eq!((g.angle_x, g.angle_y), (30.0, 150.0));
        let mut list = Vec::new();
        save_preset(&mut list, "Mine", g);
        save_preset(&mut list, "Mine", GridSettings::default());
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].grid, GridSettings::default());
        assert_eq!(trim_number(2.5000), "2.5");
        assert_eq!(trim_number(10.0), "10");
    }
}
