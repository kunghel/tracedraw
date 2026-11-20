//! The Vector Mosaic docker (Effects > Vector Mosaic): density, scale and
//! screen angle with Keep original source, Limit colors, the Tracking
//! section (method, merge adjacent, weld adjacent overlap), the tile shape
//! with Select for a custom curve and its preview (Glyphs: random symbols
//! and their seed; Characters: the character set; both: invert tones), One
//! color, and a full-width Apply.

use crate::app::App;
use crate::i18n::tr;
use crate::theme::Tokens;
use crate::ui::docker_form::{indented, row, section};
use crate::ui::tips::RichTip;
use crate::vector_mosaic::{tone_set, unit_tile, TileShape, TrackMethod};
use egui::{Color32, Sense, Stroke, Ui, Vec2};
use tracedraw_core::geometry::{Affine, BezPath, Shape as _};

/// A unit tile (y up) filled at `c`, `side` points across, turned by
/// `angle` degrees.
fn paint_unit(painter: &egui::Painter, unit: &BezPath, c: egui::Pos2, side: f32, angle: f64) {
    let m = Affine::translate((c.x as f64, c.y as f64))
        * Affine::scale_non_uniform(side as f64, -(side as f64))
        * Affine::rotate(angle.to_radians());
    let path = m * unit.clone();
    let mut rings: Vec<Vec<egui::Pos2>> = Vec::new();
    tracedraw_core::geometry::flatten(&path, 0.2, &mut |el| match el {
        tracedraw_core::geometry::PathEl::MoveTo(p) => {
            rings.push(vec![egui::pos2(p.x as f32, p.y as f32)])
        }
        tracedraw_core::geometry::PathEl::LineTo(p) => {
            if let Some(r) = rings.last_mut() {
                r.push(egui::pos2(p.x as f32, p.y as f32));
            }
        }
        _ => {}
    });
    for r in rings.into_iter().filter(|r| r.len() > 2) {
        painter.add(egui::Shape::closed_line(
            r,
            Stroke::new(1.2, Color32::from_gray(0x40)),
        ));
    }
}

/// The tile's preview: the shape at the screen angle in a small box.
fn preview(ui: &mut Ui, app: &App) {
    let s = &app.vector_mosaic;
    // A square box under the shape row, in the field column.
    let (r, _) = ui.allocate_exact_size(Vec2::new(100.0, 100.0), Sense::hover());
    let painter = ui.painter_at(r);
    painter.rect_filled(r, 0.0, Color32::WHITE);
    painter.rect_stroke(
        r,
        0.0,
        Stroke::new(1.0, Tokens::border()),
        egui::StrokeKind::Inside,
    );
    let side = r.height() * 0.6;
    let c = r.center();
    let fill = Color32::from_gray(0x40);
    match (s.shape, &s.custom) {
        (TileShape::Circle, _) => {
            painter.circle_filled(c, side / 2.0, fill);
        }
        (TileShape::Square, _) => {
            let a = -(s.angle as f32).to_radians();
            let pts: Vec<egui::Pos2> = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
                .iter()
                .map(|(x, y)| {
                    let (x, y) = (x * side / 2.0, y * side / 2.0);
                    c + Vec2::new(x * a.cos() - y * a.sin(), x * a.sin() + y * a.cos())
                })
                .collect();
            painter.add(egui::Shape::convex_polygon(pts, fill, Stroke::NONE));
        }
        (TileShape::Custom, Some(unit)) => {
            // The unit tile (y up) drawn y down, turned by the angle.
            let m = Affine::translate((c.x as f64, c.y as f64))
                * Affine::scale_non_uniform(side as f64, -(side as f64))
                * Affine::rotate(s.angle.to_radians());
            let mut pts = Vec::new();
            tracedraw_core::geometry::flatten(&(m * unit.clone()), 0.2, &mut |el| match el {
                tracedraw_core::geometry::PathEl::MoveTo(p)
                | tracedraw_core::geometry::PathEl::LineTo(p) => {
                    pts.push(egui::pos2(p.x as f32, p.y as f32))
                }
                _ => {}
            });
            if pts.len() > 2 {
                painter.add(egui::Shape::closed_line(pts, Stroke::new(1.5, fill)));
            }
        }
        (TileShape::Glyphs | TileShape::Characters, _) => {
            // The set, light to dark, in rows of four.
            let set = tone_set(s);
            let shown: Vec<&BezPath> = set.iter().take(12).collect();
            let cell = r.width() / 4.0;
            for (k, unit) in shown.iter().enumerate() {
                let (col, line) = ((k % 4) as f32, (k / 4) as f32);
                let at = egui::pos2(
                    r.left() + cell * (col + 0.5),
                    r.top() + cell * (line + 0.5) + (r.height() - cell * 3.0) / 2.0,
                );
                paint_unit(&painter, unit, at, cell * 0.8, 0.0);
            }
        }
        (TileShape::Custom, None) => {
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                tr("docker.vector_mosaic_no_custom"),
                egui::FontId::proportional(11.0),
                Tokens::text_dim(),
            );
        }
    }
}

pub fn vector_mosaic(app: &mut App, ui: &mut Ui) {
    ui.spacing_mut().slider_width = 110.0;
    {
        let s = &mut app.vector_mosaic;
        row(ui, &tr("docker.vector_mosaic_density"), |ui| {
            ui.add(crate::ui::Slider::new(&mut s.density, 1.0..=100.0).max_decimals(1))
                .rich_tip(crate::ui::tips::keyed("docker.vector_mosaic_density"));
        });
        row(ui, &tr("docker.vector_mosaic_scale"), |ui| {
            ui.add(
                crate::ui::Slider::new(&mut s.scale, 0.1..=5.0)
                    .step_by(0.05)
                    .max_decimals(2),
            )
            .rich_tip(crate::ui::tips::keyed("docker.vector_mosaic_scale"));
        });
        row(ui, &tr("docker.vector_mosaic_angle"), |ui| {
            ui.add(
                crate::ui::field::NumField::new(&mut s.angle)
                    .range(-90.0..=90.0)
                    .speed(0.5)
                    .max_decimals(1)
                    .suffix("\u{b0}"),
            )
            .rich_tip(crate::ui::tips::keyed("docker.vector_mosaic_angle"));
        });
        indented(ui, |ui| {
            ui.checkbox(
                &mut s.keep_original,
                tr("docker.vector_mosaic_keep_original"),
            )
            .rich_tip(crate::ui::tips::keyed("docker.vector_mosaic_keep_original"));
        });
        section(ui, None);
        ui.horizontal(|ui| {
            ui.checkbox(&mut s.mono, tr("docker.vector_mosaic_mono"))
                .rich_tip(crate::ui::tips::keyed("docker.vector_mosaic_mono"));
            let mut rgb = s.mono_color;
            if ui
                .add_enabled_ui(s.mono, |ui| ui.color_edit_button_srgb(&mut rgb))
                .inner
                .changed()
            {
                s.mono_color = rgb;
            }
        });
        // Remove background: on, the tolerance, and Auto (the edges'
        // colour) or a picked colour.
        ui.horizontal(|ui| {
            ui.checkbox(
                &mut s.skip_background,
                tr("docker.vector_mosaic_skip_background"),
            )
            .rich_tip(crate::ui::tips::keyed("docker.vector_mosaic_skip_background"));
            let tip = crate::ui::tips::keyed("docker.vector_mosaic_background_tolerance");
            ui.add_enabled(
                s.skip_background,
                crate::ui::field::NumField::new(&mut s.background_tolerance)
                    .range(0.0..=100.0)
                    .speed(0.5)
                    .max_decimals(0)
                    .suffix(" %"),
            )
            .rich_tip(tip.clone())
            .rich_disabled_tip(tip);
        });
        indented(ui, |ui| {
            ui.add_enabled_ui(s.skip_background, |ui| {
                ui.horizontal(|ui| {
                    let mut auto = s.background.is_none();
                    if ui
                        .checkbox(&mut auto, tr("docker.vector_mosaic_background_auto"))
                        .rich_tip(crate::ui::tips::keyed(
                            "docker.vector_mosaic_background_auto",
                        ))
                        .changed()
                    {
                        s.background = if auto { None } else { Some([255, 255, 255]) };
                    }
                    let mut rgb = s.background.unwrap_or([255, 255, 255]);
                    if ui
                        .add_enabled_ui(!auto, |ui| ui.color_edit_button_srgb(&mut rgb))
                        .inner
                        .changed()
                    {
                        s.background = Some(rgb);
                    }
                });
            });
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut s.limit_colors, tr("docker.vector_mosaic_limit_colors"))
                .rich_tip(crate::ui::tips::keyed("docker.vector_mosaic_limit_colors"));
            ui.add_enabled(
                s.limit_colors,
                crate::ui::field::NumField::new(&mut s.colors).range(2..=256),
            )
            .rich_tip(crate::ui::tips::pair(
                "docker.vector_mosaic_colors",
                "docker.vector_mosaic_colors_desc",
            ))
            .rich_disabled_tip(crate::ui::tips::pair(
                "docker.vector_mosaic_colors",
                "docker.vector_mosaic_colors_desc",
            ));
        });
        section(ui, Some(&tr("docker.vector_mosaic_tracking")));
        row(ui, &tr("docker.vector_mosaic_method"), |ui| {
            crate::ui::keynav::ComboBox::from_id_salt("vector_mosaic_method")
                .selected_text(tr(s.method.key()))
                .width(ui.available_width().clamp(120.0, 190.0))
                .show_ui(ui, |ui| {
                    for m in TrackMethod::ALL {
                        ui.selectable_value(&mut s.method, m, tr(m.key()));
                    }
                })
                .response
                .rich_tip(crate::ui::tips::keyed("docker.vector_mosaic_method"));
        });
        row(ui, &tr("docker.vector_mosaic_merge"), |ui| {
            ui.add(crate::ui::field::NumField::new(&mut s.merge).range(1..=36))
                .rich_tip(crate::ui::tips::keyed("docker.vector_mosaic_merge"));
        });
        ui.checkbox(&mut s.weld, tr("docker.vector_mosaic_weld"))
            .rich_tip(crate::ui::tips::keyed("docker.vector_mosaic_weld"));
    }
    // Shape: the list, Select (a custom curve from the selection) beside
    // it, and the preview under them.
    let custom = app.vector_mosaic.shape == TileShape::Custom;
    let mut select = false;
    row(ui, &tr("docker.vector_mosaic_shape"), |ui| {
        let s = &mut app.vector_mosaic;
        crate::ui::keynav::ComboBox::from_id_salt("vector_mosaic_shape")
            .selected_text(tr(s.shape.key()))
            .width(110.0)
            .show_ui(ui, |ui| {
                for t in TileShape::ALL {
                    ui.selectable_value(&mut s.shape, t, tr(t.key()));
                }
            })
            .response
            .rich_tip(crate::ui::tips::keyed("docker.vector_mosaic_shape"));
        select = ui
            .add_enabled(
                custom && !app.selection.is_empty(),
                egui::Button::new(tr("docker.vector_mosaic_select")),
            )
            .rich_tip(tr("docker.vector_mosaic_select_tip"))
            .clicked();
    });
    {
        let s = &mut app.vector_mosaic;
        match s.shape {
            TileShape::Glyphs => {
                indented(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut s.random_glyphs, tr("docker.vector_mosaic_random"))
                            .rich_tip(crate::ui::tips::keyed("docker.vector_mosaic_random"));
                        ui.add_enabled(
                            s.random_glyphs,
                            crate::ui::field::NumField::new(&mut s.seed).range(0..=9999),
                        )
                        .rich_tip(crate::ui::tips::keyed("docker.vector_mosaic_seed"))
                        .rich_disabled_tip(crate::ui::tips::keyed("docker.vector_mosaic_seed"));
                    });
                });
            }
            TileShape::Characters => {
                row(ui, &tr("docker.vector_mosaic_chars"), |ui| {
                    ui.add(egui::TextEdit::singleline(&mut s.chars).desired_width(140.0))
                        .rich_tip(crate::ui::tips::keyed("docker.vector_mosaic_chars"));
                });
            }
            _ => {}
        }
        if s.shape.picks_by_tone() {
            indented(ui, |ui| {
                ui.checkbox(&mut s.invert_tone, tr("docker.vector_mosaic_invert"))
                    .rich_tip(crate::ui::tips::keyed("docker.vector_mosaic_invert"));
            });
        }
    }
    indented(ui, |ui| preview(ui, app));
    if select {
        let tile = app
            .selected_shapes()
            .first()
            .map(|sh| sh.page_path())
            .filter(|p| p.bounding_box().area() > 0.0)
            .and_then(|p| unit_tile(&p));
        match tile {
            Some(t) => app.vector_mosaic.custom = Some(t),
            None => app.status = tr("status.vector_mosaic_no_curve"),
        }
    }
    let custom = app.vector_mosaic.shape == TileShape::Custom;
    let ready = !app.selection.is_empty() && (!custom || app.vector_mosaic.custom.is_some());
    let tip = crate::ui::tips::pair("docker.apply", "docker.vector_mosaic_apply_desc");
    if crate::ui::docker_form::wide_button(ui, &tr("docker.apply"), ready, &tip) {
        let s = app.vector_mosaic.clone();
        app.apply_vector_mosaic(&s);
    }
}
