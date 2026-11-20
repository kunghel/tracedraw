//! The 3D Revolve and Inflate docker (Effects > 3D Revolve and Inflate):
//! the mode, Revolve's position (ready-made views), tilt, turn, roll,
//! facets, sweep, cap, offset and axis side, Inflate's depth, the surface
//! (plastic, diffuse, flat, wireframe), the light (direction, height,
//! shine, colour, intensity, softness, ambient light), the art (Use
//! Selection or a symbol, place and size, Fit, Shade art, Only the art),
//! Keep original and a full-width Apply (Update and Release while a 3D
//! object is selected, which stays editable); and Halftone Sphere, which
//! makes a lit halftone ball without a selection.

use crate::app::App;
use crate::i18n::{tr, trf};
use crate::shape3d::{position_of, Mode, Surface, POSITIONS};
use crate::ui::docker_form::{indented, row, section};
use crate::ui::tips::RichTip;
use egui::Ui;

fn slider(ui: &mut Ui, v: &mut f64, lo: f64, hi: f64, key: &str) {
    ui.add(crate::ui::Slider::new(v, lo..=hi).max_decimals(0))
        .rich_tip(crate::ui::tips::keyed(key));
}

pub fn shape_3d(app: &mut App, ui: &mut Ui) {
    ui.spacing_mut().slider_width = 110.0;
    // A 3D object selected: the panel shows what it was made with, and
    // Apply makes it again in place.
    let editing = app.selected_3d();
    match &editing {
        Some((id, live)) if app.shape_3d.editing != Some(*id) => {
            app.shape_3d = crate::shape3d::Settings3d {
                editing: Some(*id),
                ..live.settings.clone()
            };
        }
        None => app.shape_3d.editing = None,
        _ => {}
    }
    let mut pick = false;
    let mut release = false;
    if editing.is_some() {
        indented(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(tr("docker.shape3d_editing"));
                release = ui
                    .button(tr("docker.shape3d_release"))
                    .rich_tip(crate::ui::tips::keyed("docker.shape3d_release"))
                    .clicked();
            });
        });
    }
    if release {
        app.release_3d();
        return;
    }
    {
        let s = &mut app.shape_3d;
        row(ui, &tr("docker.shape3d_mode"), |ui| {
            for (m, key) in [
                (Mode::Revolve, "docker.shape3d_revolve"),
                (Mode::Inflate, "docker.shape3d_inflate"),
            ] {
                ui.selectable_value(&mut s.mode, m, tr(key))
                    .rich_tip(crate::ui::tips::keyed(key));
            }
        });
        section(ui, None);
        match s.mode {
            Mode::Revolve => {
                row(ui, &tr("docker.shape3d_segments"), |ui| {
                    let mut n = s.segments;
                    if ui
                        .add(crate::ui::field::NumField::new(&mut n).range(3..=256))
                        .rich_tip(crate::ui::tips::keyed("docker.shape3d_segments"))
                        .changed()
                    {
                        s.segments = n;
                    }
                });
                row(ui, &tr("docker.shape3d_position"), |ui| {
                    let now = position_of(s).map_or_else(|| tr("docker.shape3d_pos_custom"), tr);
                    crate::ui::keynav::ComboBox::from_id_salt("shape3d_position")
                        .selected_text(now)
                        .width(ui.available_width().clamp(120.0, 190.0))
                        .show_ui(ui, |ui| {
                            for (key, t, u, r) in POSITIONS {
                                if ui
                                    .selectable_label(position_of(s) == Some(key), tr(key))
                                    .clicked()
                                {
                                    s.tilt = t;
                                    s.turn = u;
                                    s.roll = r;
                                }
                            }
                        })
                        .response
                        .rich_tip(crate::ui::tips::keyed("docker.shape3d_position"));
                });
                row(ui, &tr("docker.shape3d_tilt"), |ui| {
                    slider(ui, &mut s.tilt, -90.0, 90.0, "docker.shape3d_tilt")
                });
                row(ui, &tr("docker.shape3d_turn"), |ui| {
                    slider(ui, &mut s.turn, 0.0, 360.0, "docker.shape3d_turn")
                });
                row(ui, &tr("docker.shape3d_roll"), |ui| {
                    slider(ui, &mut s.roll, -180.0, 180.0, "docker.shape3d_roll")
                });
                row(ui, &tr("docker.shape3d_sweep"), |ui| {
                    slider(ui, &mut s.sweep, 10.0, 360.0, "docker.shape3d_sweep")
                });
                row(ui, &tr("docker.shape3d_offset"), |ui| {
                    ui.add(
                        crate::ui::field::NumField::new(&mut s.offset)
                            .range(0.0..=10_000.0)
                            .speed(0.5)
                            .max_decimals(2)
                            .suffix(" mm"),
                    )
                    .rich_tip(crate::ui::tips::keyed("docker.shape3d_offset"));
                });
                indented(ui, |ui| {
                    ui.checkbox(&mut s.axis_right, tr("docker.shape3d_axis_right"))
                        .rich_tip(crate::ui::tips::keyed("docker.shape3d_axis_right"));
                    ui.checkbox(&mut s.cap, tr("docker.shape3d_cap"))
                        .rich_tip(crate::ui::tips::keyed("docker.shape3d_cap"));
                });
            }
            Mode::Inflate => {
                row(ui, &tr("docker.shape3d_depth"), |ui| {
                    slider(ui, &mut s.depth, 10.0, 200.0, "docker.shape3d_depth")
                });
            }
        }
        section(ui, None);
        row(ui, &tr("docker.shape3d_surface"), |ui| {
            crate::ui::keynav::ComboBox::from_id_salt("shape3d_surface")
                .selected_text(tr(s.surface.key()))
                .width(ui.available_width().clamp(120.0, 190.0))
                .show_ui(ui, |ui| {
                    for m in Surface::ALL {
                        ui.selectable_value(&mut s.surface, m, tr(m.key()));
                    }
                })
                .response
                .rich_tip(crate::ui::tips::keyed("docker.shape3d_surface"));
        });
        if matches!(s.surface, Surface::Plastic | Surface::Diffuse) {
            row(ui, &tr("docker.shape3d_light_angle"), |ui| {
                slider(
                    ui,
                    &mut s.light_angle,
                    0.0,
                    360.0,
                    "docker.shape3d_light_angle",
                )
            });
            row(ui, &tr("docker.shape3d_light_height"), |ui| {
                slider(
                    ui,
                    &mut s.light_height,
                    0.0,
                    90.0,
                    "docker.shape3d_light_height",
                )
            });
            if s.surface == Surface::Plastic {
                row(ui, &tr("docker.shape3d_shine"), |ui| {
                    slider(ui, &mut s.shine, 0.0, 100.0, "docker.shape3d_shine")
                });
            }
            row(ui, &tr("docker.shape3d_light_color"), |ui| {
                let mut c = s.light_color.to_rgb8();
                if ui
                    .color_edit_button_srgb(&mut c)
                    .rich_tip(crate::ui::tips::keyed("docker.shape3d_light_color"))
                    .changed()
                {
                    s.light_color = tracedraw_core::Color::rgb8(c[0], c[1], c[2]);
                }
            });
            row(ui, &tr("docker.shape3d_intensity"), |ui| {
                slider(ui, &mut s.intensity, 0.0, 200.0, "docker.shape3d_intensity")
            });
            row(ui, &tr("docker.shape3d_softness"), |ui| {
                slider(ui, &mut s.softness, 0.0, 100.0, "docker.shape3d_softness")
            });
            row(ui, &tr("docker.shape3d_ambient"), |ui| {
                ui.checkbox(&mut s.ambient, "")
                    .rich_tip(crate::ui::tips::keyed("docker.shape3d_ambient"));
                ui.add_enabled_ui(s.ambient, |ui| {
                    slider(
                        ui,
                        &mut s.ambient_intensity,
                        0.0,
                        100.0,
                        "docker.shape3d_ambient",
                    )
                });
            });
        }
        section(ui, None);
    }
    let mut symbol: Option<usize> = None;
    let mut fit = false;
    row(ui, &tr("docker.shape3d_art"), |ui| {
        pick = ui
            .add_enabled(
                !app.selection.is_empty() && editing.is_none(),
                egui::Button::new(tr("docker.scatter_use_selection")),
            )
            .rich_tip(crate::ui::tips::keyed("docker.shape3d_art"))
            .rich_disabled_tip(crate::ui::tips::keyed("docker.shape3d_art"))
            .clicked();
        if !app.shape_3d.art.is_empty() {
            ui.label(trf(
                "docker.scatter_picked_count",
                &[("n", &app.shape_3d.art.len().to_string())],
            ));
            if ui
                .small_button("×")
                .rich_tip(tr("docker.shape3d_art_clear"))
                .clicked()
            {
                app.shape_3d.art.clear();
            }
        }
    });
    let names: Vec<String> = app.doc().symbols.iter().map(|s| s.name.clone()).collect();
    if !names.is_empty() {
        row(ui, &tr("docker.shape3d_symbol"), |ui| {
            crate::ui::keynav::ComboBox::from_id_salt("shape3d_symbol")
                .selected_text(tr("docker.shape3d_symbol_pick"))
                .width(ui.available_width().clamp(120.0, 190.0))
                .show_ui(ui, |ui| {
                    for (i, n) in names.iter().enumerate() {
                        if ui.selectable_label(false, n).clicked() {
                            symbol = Some(i);
                        }
                    }
                })
                .response
                .rich_tip(crate::ui::tips::keyed("docker.shape3d_symbol"));
        });
    }
    if pick && !app.pick_3d_art() {
        app.status = tr("status.scatter_no_symbols");
    }
    if let Some(i) = symbol {
        if !app.pick_3d_symbol(i) {
            app.status = tr("status.scatter_no_symbols");
        }
    }
    {
        let s = &mut app.shape_3d;
        if !s.art.is_empty() {
            row(ui, &tr("docker.shape3d_art_u"), |ui| {
                slider(ui, &mut s.art_u, 0.0, 100.0, "docker.shape3d_art_u")
            });
            row(ui, &tr("docker.shape3d_art_v"), |ui| {
                slider(ui, &mut s.art_v, 0.0, 100.0, "docker.shape3d_art_v")
            });
            row(ui, &tr("docker.shape3d_art_size"), |ui| {
                slider(ui, &mut s.art_size, 1.0, 100.0, "docker.shape3d_art_size")
            });
            indented(ui, |ui| {
                ui.horizontal(|ui| {
                    fit = ui
                        .button(tr("docker.shape3d_fit"))
                        .rich_tip(crate::ui::tips::keyed("docker.shape3d_fit"))
                        .clicked();
                    ui.checkbox(&mut s.art_fit, tr("docker.shape3d_stretch"))
                        .rich_tip(crate::ui::tips::keyed("docker.shape3d_stretch"));
                });
                ui.checkbox(&mut s.shade_art, tr("docker.shape3d_shade_art"))
                    .rich_tip(crate::ui::tips::keyed("docker.shape3d_shade_art"));
                if s.mode == Mode::Revolve {
                    ui.checkbox(&mut s.hide_surface, tr("docker.shape3d_hide_surface"))
                        .rich_tip(crate::ui::tips::keyed("docker.shape3d_hide_surface"));
                }
            });
        }
        if editing.is_none() {
            indented(ui, |ui| {
                ui.checkbox(&mut s.keep_original, tr("docker.shape3d_keep"))
                    .rich_tip(crate::ui::tips::keyed("docker.shape3d_keep"));
            });
        }
    }
    if fit {
        app.fit_3d_art();
    }
    let (label, desc) = if editing.is_some() {
        ("docker.shape3d_update", "docker.shape3d_update_desc")
    } else {
        ("docker.apply", "docker.shape3d_apply_desc")
    };
    let tip = crate::ui::tips::pair(label, desc);
    let ready = !app.selection.is_empty();
    if crate::ui::docker_form::wide_button(ui, &tr(label), ready, &tip) {
        let s = app.shape_3d.clone();
        app.apply_3d(&s);
    }
    // A lit halftone ball in one click: no selection needed.
    section(ui, None);
    let mut make = false;
    row(ui, &tr("docker.shape3d_halftone_sphere"), |ui| {
        let s = &mut app.shape_3d;
        let mut n = s.halftone_dots;
        if ui
            .add(crate::ui::field::NumField::new(&mut n).range(6..=80))
            .rich_tip(crate::ui::tips::keyed("docker.shape3d_halftone_dots"))
            .changed()
        {
            s.halftone_dots = n;
        }
        make = ui
            .button(tr("docker.shape3d_halftone_create"))
            .rich_tip(crate::ui::tips::keyed("docker.shape3d_halftone_sphere"))
            .clicked();
    });
    if make {
        let s = app.shape_3d.clone();
        app.create_halftone_sphere(&s, s.halftone_dots);
    }
}
