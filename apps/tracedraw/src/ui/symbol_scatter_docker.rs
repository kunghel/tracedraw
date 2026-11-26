//! The Symbol Scatter docker (Effects > Symbol Scatter): the symbols
//! (built-in, or objects picked with Use Selection) with a preview, the
//! spacing, the size range and random turn, Stay inside, the colours, the
//! seed with Shuffle, Keep objects, and a full-width Apply.

use crate::ui::btn_icons::IconUi;
use crate::app::App;
use crate::i18n::{tr, trf};
use crate::symbol_scatter::{unit, Builtin, ColorMode, Source};
use crate::theme::Tokens;
use crate::ui::docker_form::{indented, row, section};
use crate::ui::tips::RichTip;
use egui::{Color32, Sense, Stroke, Ui, Vec2};
use tracedraw_core::geometry::{Affine, BezPath, PathEl};

/// The symbols' preview: up to five of them side by side in a strip.
fn preview(ui: &mut Ui, app: &App) {
    let s = &app.symbol_scatter;
    // Narrower in a narrow panel, never past its edge (#4323).
    let w = crate::ui::docker_form::fit_w(ui, 150.0);
    let (r, _) = ui.allocate_exact_size(Vec2::new(w, 40.0), Sense::hover());
    let painter = ui.painter_at(r);
    painter.rect_filled(r, 0.0, Color32::WHITE);
    painter.rect_stroke(
        r,
        0.0,
        Stroke::new(1.0, Tokens::border()),
        egui::StrokeKind::Inside,
    );
    let paths: Vec<BezPath> = match s.source {
        Source::Picked => s
            .picked
            .iter()
            .filter_map(|sh| unit(&sh.page_path()))
            .take(5)
            .collect(),
        Source::Builtin => match Builtin::ALL.get(s.builtin) {
            Some(b) => vec![b.path()],
            None => Builtin::ALL.iter().take(5).map(|b| b.path()).collect(),
        },
    };
    if paths.is_empty() {
        painter.text(
            r.center(),
            egui::Align2::CENTER_CENTER,
            tr("docker.scatter_none_picked"),
            egui::FontId::proportional(11.0),
            Tokens::text_dim(),
        );
        return;
    }
    let side = (r.height() * 0.7) as f64;
    let n = paths.len() as f32;
    let fill = Color32::from_gray(0x40);
    for (i, p) in paths.iter().enumerate() {
        let cx = r.left() + r.width() * (i as f32 + 0.5) / n;
        // The unit symbol (y up) drawn y down.
        let m = Affine::translate((cx as f64, r.center().y as f64))
            * Affine::scale_non_uniform(side, -side);
        let mut pts = Vec::new();
        tracedraw_core::geometry::flatten(&(m * p.clone()), 0.2, &mut |el| match el {
            PathEl::MoveTo(q) | PathEl::LineTo(q) => pts.push(egui::pos2(q.x as f32, q.y as f32)),
            _ => {}
        });
        if pts.len() > 2 {
            painter.add(egui::Shape::closed_line(pts, Stroke::new(1.5, fill)));
        }
    }
}

pub fn symbol_scatter(app: &mut App, ui: &mut Ui) {
    crate::ui::docker_form::fit_sliders(ui);
    let mut pick = false;
    {
        let s = &mut app.symbol_scatter;
        row(ui, &tr("docker.scatter_symbols"), |ui| {
            let name = |source: Source, i: usize| match (source, Builtin::ALL.get(i)) {
                (Source::Picked, _) => tr("docker.scatter_picked"),
                (Source::Builtin, Some(b)) => tr(b.key()),
                (Source::Builtin, None) => tr("docker.scatter_mixed"),
            };
            crate::ui::keynav::ComboBox::from_id_salt("symbol_scatter_symbols")
                .selected_text(name(s.source, s.builtin))
                .width(crate::ui::docker_form::fit_w(ui, 190.0))
                .truncate()
                .show_ui(ui, |ui| {
                    for i in 0..=Builtin::ALL.len() {
                        let on = s.source == Source::Builtin && s.builtin == i;
                        if ui.selectable_label(on, name(Source::Builtin, i)).clicked() {
                            s.source = Source::Builtin;
                            s.builtin = i;
                        }
                    }
                    let on = s.source == Source::Picked;
                    if ui
                        .iselectable(on, "docker.scatter_picked")
                        .clicked()
                    {
                        s.source = Source::Picked;
                    }
                })
                .response
                .rich_tip(crate::ui::tips::keyed("docker.scatter_symbols"));
        });
    }
    indented(ui, |ui| {
        ui.horizontal(|ui| {
            pick = ui
                .add_enabled(
                    !app.selection.is_empty(),
                    crate::ui::btn_icons::Ib::new("docker.scatter_use_selection"),
                )
                .rich_tip(crate::ui::tips::keyed("docker.scatter_use_selection"))
                .rich_disabled_tip(crate::ui::tips::keyed("docker.scatter_use_selection"))
                .clicked();
            if !app.symbol_scatter.picked.is_empty() {
                ui.label(trf(
                    "docker.scatter_picked_count",
                    &[("n", &app.symbol_scatter.picked.len().to_string())],
                ));
            }
        });
        preview(ui, app);
    });
    if pick && !app.pick_scatter_symbols() {
        app.status = tr("status.scatter_no_symbols");
    }
    {
        let s = &mut app.symbol_scatter;
        section(ui, None);
        row(ui, &tr("docker.scatter_spacing"), |ui| {
            ui.add(
                crate::ui::field::NumField::new(&mut s.spacing)
                    .range(0.1..=1000.0)
                    .speed(0.1)
                    .max_decimals(2)
                    .suffix(" mm"),
            )
            .rich_tip(crate::ui::tips::keyed("docker.scatter_spacing"));
        });
        row(ui, &tr("docker.scatter_size"), |ui| {
            // The two fields share the room left in a narrow panel (#4323).
            let w = ((crate::ui::docker_form::fit_w(ui, f32::INFINITY) - 20.0) / 2.0).clamp(36.0, 64.0);
            ui.add(
                crate::ui::field::NumField::new(&mut s.size_min)
                    .range(1.0..=400.0)
                    .speed(1.0)
                    .max_decimals(0)
                    .suffix(" %")
                    .width(w),
            )
            .rich_tip(crate::ui::tips::keyed("docker.scatter_size"));
            ui.label("-");
            ui.add(
                crate::ui::field::NumField::new(&mut s.size_max)
                    .range(1.0..=400.0)
                    .speed(1.0)
                    .max_decimals(0)
                    .suffix(" %")
                    .width(w),
            )
            .rich_tip(crate::ui::tips::keyed("docker.scatter_size"));
        });
        row(ui, &tr("docker.scatter_rotation"), |ui| {
            ui.add(crate::ui::Slider::new(&mut s.rotation, 0.0..=180.0).max_decimals(0))
                .rich_tip(crate::ui::tips::keyed("docker.scatter_rotation"));
        });
        indented(ui, |ui| {
            ui.checkbox(&mut s.inside, tr("docker.scatter_inside"))
                .rich_tip(crate::ui::tips::keyed("docker.scatter_inside"));
        });
        section(ui, None);
        row(ui, &tr("docker.scatter_color"), |ui| {
            crate::ui::keynav::ComboBox::from_id_salt("symbol_scatter_color")
                .selected_text(tr(s.color.key()))
                .width(crate::ui::docker_form::fit_w(ui, 190.0))
                .truncate()
                .show_ui(ui, |ui| {
                    for m in ColorMode::ALL {
                        ui.selectable_value(&mut s.color, m, tr(m.key()));
                    }
                })
                .response
                .rich_tip(crate::ui::tips::keyed("docker.scatter_color"));
        });
        if s.color == ColorMode::One {
            indented(ui, |ui| {
                let mut c = s.one_color.to_rgb8();
                if ui.color_edit_button_srgb(&mut c).changed() {
                    s.one_color = tracedraw_core::Color::rgb8(c[0], c[1], c[2]);
                }
            });
        }
        row(ui, &tr("docker.scatter_seed"), |ui| {
            // Shuffle goes under the field when a narrow panel has no
            // room beside it (#4323).
            ui.horizontal_wrapped(|ui| {
                let mut seed = s.seed as u32;
                if ui
                    .add(crate::ui::field::NumField::new(&mut seed).range(0..=999_999))
                    .rich_tip(crate::ui::tips::keyed("docker.scatter_seed"))
                    .changed()
                {
                    s.seed = seed as u64;
                }
                if ui
                    .ibutton("docker.scatter_shuffle")
                    .rich_tip(crate::ui::tips::keyed("docker.scatter_shuffle"))
                    .clicked()
                {
                    s.seed = (s.seed + 1) % 1_000_000;
                }
            });
        });
        indented(ui, |ui| {
            ui.checkbox(&mut s.keep_container, tr("docker.scatter_keep"))
                .rich_tip(crate::ui::tips::keyed("docker.scatter_keep"));
        });
    }
    let ready = !app.selection.is_empty()
        && (app.symbol_scatter.source == Source::Builtin || !app.symbol_scatter.picked.is_empty());
    let tip = crate::ui::tips::pair("docker.apply", "docker.scatter_apply_desc");
    if crate::ui::docker_form::wide_button(ui, "docker.apply", ready, &tip) {
        let s = app.symbol_scatter.clone();
        app.apply_symbol_scatter(&s);
    }
    if crate::ui::docker_form::reset_button(ui) {
        app.symbol_scatter.reset();
    }
    brush_section(app, ui);
}

/// The brush: ticked, a drag over the selected group changes the objects
/// under the nib (`symbol_brush`).
fn brush_section(app: &mut App, ui: &mut Ui) {
    use crate::symbol_brush::SymbolBrush;
    let s = &mut app.symbol_scatter;
    section(ui, None);
    indented(ui, |ui| {
        ui.checkbox(&mut s.brush_on, tr("docker.scatter_brush"))
            .rich_tip(crate::ui::tips::keyed("docker.scatter_brush"));
    });
    if !s.brush_on {
        return;
    }
    row(ui, &tr("docker.scatter_brush_mode"), |ui| {
        crate::ui::keynav::ComboBox::from_id_salt("symbol_brush_mode")
            .selected_text(tr(s.brush.key()))
            .width(crate::ui::docker_form::fit_w(ui, 190.0))
            .truncate()
            .show_ui(ui, |ui| {
                for m in SymbolBrush::ALL {
                    ui.selectable_value(&mut s.brush, m, tr(m.key()))
                        .rich_tip(crate::ui::tips::keyed(m.key()));
                }
            })
            .response
            .rich_tip(crate::ui::tips::keyed(s.brush.key()));
    });
    row(ui, &tr("docker.scatter_brush_radius"), |ui| {
        ui.add(
            crate::ui::field::NumField::new(&mut s.brush_radius)
                .range(0.5..=500.0)
                .speed(0.1)
                .max_decimals(1)
                .suffix(" mm"),
        )
        .rich_tip(crate::ui::tips::keyed("docker.scatter_brush_radius"));
    });
    row(ui, &tr("docker.scatter_brush_strength"), |ui| {
        ui.add(
            crate::ui::field::NumField::new(&mut s.brush_strength)
                .range(1.0..=100.0)
                .speed(1.0)
                .max_decimals(0)
                .suffix(" %"),
        )
        .rich_tip(crate::ui::tips::keyed("docker.scatter_brush_strength"));
    });
    if s.brush == SymbolBrush::Tint {
        indented(ui, |ui| {
            let mut c = s.one_color.to_rgb8();
            if ui.color_edit_button_srgb(&mut c).changed() {
                s.one_color = tracedraw_core::Color::rgb8(c[0], c[1], c[2]);
            }
        });
    }
}
