//! The picture editing bar and panel: while a picture is edited in place
//! (Bitmaps > Edit Inside, a double click on a picture), a bar at the
//! top left of the drawing window with Finish, Cancel, Undo, Redo, the
//! tools and the settings of the tool in use, and a panel at the top
//! right with the layers, the selection commands, retouching, the bitmap
//! effects (Light among them) run inside the selection, and flipping. See
//! docs/behavior/edit-inside-picture.md.

use crate::app::App;
use crate::i18n::tr;
use crate::pixel_edit::{self, PixelTool};
use crate::theme::Tokens;
use crate::ui::tips::RichTip;
use egui::{Color32, Context, RichText, Stroke};

const GREEN: Color32 = Color32::from_rgb(0x1E, 0x9E, 0x3C);

pub fn show(app: &mut App, ctx: &Context) {
    if app.pixel_edit.is_none() {
        return;
    }
    // The picture went away under the edit (undone, deleted elsewhere).
    let gone = app
        .pixel_edit
        .as_ref()
        .is_some_and(|pe| app.doc().find_shape(pe.shape).is_none());
    if gone {
        app.cancel_pixel_edit();
        return;
    }
    // The lights stay live only while the Light tool is in use.
    if let Some(pe) = app.pixel_edit.as_mut() {
        if pe.tool != PixelTool::Light {
            pe.lights.bake();
        }
    }
    bar(app, ctx);
    panel(app, ctx);
}

fn frame() -> egui::Frame {
    egui::Frame::new()
        .fill(Tokens::panel())
        .stroke(Stroke::new(1.0, Tokens::border()))
        .inner_margin(egui::Margin::symmetric(10, 6))
}

fn bar(app: &mut App, ctx: &Context) {
    let at = app.canvas_rect.min + egui::vec2(14.0, 14.0);
    let max_w = (app.canvas_rect.width() - 300.0).max(320.0);
    egui::Area::new(egui::Id::new("pixel_bar"))
        .fixed_pos(at)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            frame().show(ui, |ui| {
                ui.set_max_width(max_w);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(tr("pixel.title"))
                            .strong()
                            .color(Tokens::text()),
                    );
                    ui.separator();
                    let finish = ui
                        .add(
                            egui::Button::new(
                                RichText::new(format!("      {}", tr("pixel.finish")))
                                    .color(Tokens::text()),
                            )
                            .frame(false),
                        )
                        .rich_tip(tr("pixel.finish_tip"));
                    let r = finish.rect;
                    ui.painter().text(
                        egui::pos2(r.left() + 2.0, r.center().y),
                        egui::Align2::LEFT_CENTER,
                        "\u{2714}",
                        egui::FontId::proportional(16.0),
                        GREEN,
                    );
                    if finish.clicked() {
                        app.finish_pixel_edit();
                        return;
                    }
                    if ui
                        .button(tr("pixel.cancel"))
                        .rich_tip(tr("pixel.cancel_tip"))
                        .clicked()
                    {
                        app.cancel_pixel_edit();
                        return;
                    }
                    ui.separator();
                    let Some(pe) = app.pixel_edit.as_mut() else {
                        return;
                    };
                    if ui
                        .add_enabled(pe.history.can_undo(), egui::Button::new(tr("pixel.undo")))
                        .clicked()
                    {
                        pe.undo();
                    }
                    if ui
                        .add_enabled(pe.history.can_redo(), egui::Button::new(tr("pixel.redo")))
                        .clicked()
                    {
                        pe.redo();
                    }
                });
                let Some(pe) = app.pixel_edit.as_mut() else {
                    return;
                };
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    for t in PixelTool::ALL {
                        let key = t.key();
                        if ui
                            .selectable_label(pe.tool == t, tr(key))
                            .rich_tip(tr(&format!("{key}_tip")))
                            .clicked()
                        {
                            pe.tool = t;
                            pe.drag_from = None;
                            pe.drag_to = None;
                            pe.lasso.clear();
                        }
                    }
                });
                ui.separator();
                ui.horizontal_wrapped(|ui| tool_options(ui, pe));
            });
        });
}

fn percent(ui: &mut egui::Ui, key: &str, v: &mut f32) {
    let mut p = (*v * 100.0).round();
    if ui
        .add(
            egui::Slider::new(&mut p, 0.0..=100.0)
                .suffix(" %")
                .text(tr(key)),
        )
        .changed()
    {
        *v = p / 100.0;
    }
}

fn tool_options(ui: &mut egui::Ui, pe: &mut crate::pixel_edit_app::PixelEdit) {
    let t = pe.tool;
    if t == PixelTool::Light {
        super::pixel_lights::options(ui, pe);
        return;
    }
    let o = &mut pe.opts;
    if t.is_brush() {
        ui.add(
            egui::Slider::new(&mut o.size, 1.0..=1000.0)
                .logarithmic(true)
                .suffix(" px")
                .text(tr("pixel.opt.size")),
        );
        percent(ui, "pixel.opt.hardness", &mut o.hardness);
        percent(ui, "pixel.opt.opacity", &mut o.opacity);
        if t != PixelTool::Smudge {
            percent(ui, "pixel.opt.flow", &mut o.flow);
        }
    }
    if matches!(
        t,
        PixelTool::Brush | PixelTool::Fill | PixelTool::Gradient | PixelTool::Eyedropper
    ) {
        let mut c = Color32::from_rgba_unmultiplied(o.color[0], o.color[1], o.color[2], o.color[3]);
        ui.label(tr("pixel.opt.color"));
        if egui::color_picker::color_edit_button_srgba(
            ui,
            &mut c,
            egui::color_picker::Alpha::OnlyBlend,
        )
        .changed()
        {
            let [r, g, b, a] = c.to_srgba_unmultiplied();
            o.color = [r, g, b, a];
        }
    }
    if matches!(t, PixelTool::Fill | PixelTool::Gradient) {
        percent(ui, "pixel.opt.opacity", &mut o.opacity);
    }
    if matches!(t, PixelTool::MagicWand | PixelTool::Fill) {
        ui.add(egui::Slider::new(&mut o.tolerance, 0..=255).text(tr("pixel.opt.tolerance")));
        ui.checkbox(&mut o.contiguous, tr("pixel.opt.contiguous"));
    }
    match t {
        PixelTool::Sponge => {
            ui.checkbox(&mut o.desaturate, tr("pixel.opt.desaturate"));
        }
        PixelTool::Gradient => {
            ui.checkbox(&mut o.gradient_to_clear, tr("pixel.opt.to_clear"));
        }
        _ => {}
    }
    // What to do with the tool.
    ui.label(
        RichText::new(tr(&format!("{}_hint", t.key())))
            .color(Tokens::text_dim())
            .small(),
    );
}

fn panel(app: &mut App, ctx: &Context) {
    let w = 250.0;
    let at = egui::pos2(
        app.canvas_rect.max.x - w - 24.0,
        app.canvas_rect.min.y + 14.0,
    );
    let max_h = (app.canvas_rect.height() - 40.0).max(200.0);
    egui::Area::new(egui::Id::new("pixel_panel"))
        .fixed_pos(at)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            frame().show(ui, |ui| {
                ui.set_width(w);
                egui::ScrollArea::vertical()
                    .max_height(max_h)
                    .show(ui, |ui| {
                        let Some(pe) = app.pixel_edit.as_mut() else {
                            return;
                        };
                        layers_section(ui, pe);
                        selection_section(ui, pe);
                        retouch_section(ui, pe);
                        effects_section(ui, pe);
                        picture_section(ui, pe);
                    });
            });
        });
}

/// The name a new layer gets: "Layer" and the first number not in use.
fn new_layer_name(pe: &crate::pixel_edit_app::PixelEdit) -> String {
    let base = tr("pixel.layer.name");
    let mut n = pe.pi.layers.len();
    loop {
        let name = format!("{base} {n}");
        if !pe.pi.layers.iter().any(|l| l.name == name) {
            return name;
        }
        n += 1;
    }
}

/// How a layer is named in the list: its name, or Background for the
/// first one when it has none.
fn layer_label(i: usize, name: &str) -> String {
    if name.is_empty() {
        if i == 0 {
            tr("pixel.layer.background")
        } else {
            format!("{} {i}", tr("pixel.layer.name"))
        }
    } else {
        name.to_string()
    }
}

fn layers_section(ui: &mut egui::Ui, pe: &mut crate::pixel_edit_app::PixelEdit) {
    egui::CollapsingHeader::new(tr("pixel.section.layers"))
        .default_open(true)
        .show(ui, |ui| {
            // Top layer first, as they stack.
            let n = pe.pi.layers.len();
            let mut pick: Option<usize> = None;
            let mut toggled = false;
            for i in (0..n).rev() {
                let Some(l) = pe.pi.layers.get(i) else {
                    continue;
                };
                let (mut vis, mut lock) = (l.visible, l.locked);
                let label = layer_label(i, &l.name);
                ui.horizontal(|ui| {
                    if ui
                        .checkbox(&mut vis, "")
                        .rich_tip(tr("pixel.layer.visible_tip"))
                        .changed()
                    {
                        toggled = true;
                    }
                    if ui
                        .checkbox(&mut lock, "")
                        .rich_tip(tr("pixel.layer.locked_tip"))
                        .changed()
                    {
                        toggled = true;
                    }
                    if ui.selectable_label(pe.pi.active == i, label).clicked() {
                        pick = Some(i);
                    }
                });
                if (vis, lock) != (l.visible, l.locked) {
                    pe.apply_all(true, |pi| {
                        let l = pi.layers.get_mut(i)?;
                        l.visible = vis;
                        l.locked = lock;
                        Some([0, 0, pi.img.width(), pi.img.height()])
                    });
                }
            }
            if let Some(i) = pick {
                if !toggled {
                    pe.apply_all(true, |pi| {
                        pi.set_active(i);
                        Some([0, 0, 0, 0])
                    });
                }
            }
            // The settings of the layer in use. A drag of the opacity
            // slider is one undo step.
            let i = pe.pi.active;
            if let Some(l) = pe.pi.layers.get(i).cloned() {
                let mut name = l.name.clone();
                let mut op = (l.opacity * 100.0).round();
                let mut merge = l.merge;
                ui.horizontal(|ui| {
                    ui.label(tr("pixel.layer.rename"));
                    ui.text_edit_singleline(&mut name);
                });
                let slider = ui.add(
                    egui::Slider::new(&mut op, 0.0..=100.0)
                        .suffix(" %")
                        .text(tr("pixel.opt.opacity")),
                );
                egui::ComboBox::from_id_salt("pixel_layer_merge")
                    .width(160.0)
                    .selected_text(merge.name())
                    .show_ui(ui, |ui| {
                        for m in pixel_edit::MergeMode::ALL {
                            ui.selectable_value(&mut merge, m, m.name());
                        }
                    });
                let changed =
                    name != l.name || (op / 100.0 - l.opacity).abs() > 1e-4 || merge != l.merge;
                if changed {
                    if !pe.layer_drag {
                        pe.remember();
                    }
                    if let Some(m) = pe.pi.layers.get_mut(i) {
                        m.name = name;
                        m.opacity = op / 100.0;
                        m.merge = merge;
                    }
                    pe.view.borrow_mut().full = true;
                }
                pe.layer_drag = slider.dragged();
            }
            ui.horizontal_wrapped(|ui| {
                let name = new_layer_name(pe);
                if ui.button(tr("pixel.layer.new")).clicked() {
                    pe.apply_all(true, |pi| {
                        pi.add_layer(name.clone());
                        Some([0, 0, pi.img.width(), pi.img.height()])
                    });
                }
                if ui.button(tr("pixel.layer.duplicate")).clicked() {
                    pe.apply_all(true, |pi| {
                        pi.duplicate_layer(name.clone());
                        Some([0, 0, pi.img.width(), pi.img.height()])
                    });
                }
                if ui
                    .add_enabled(n > 1, egui::Button::new(tr("pixel.layer.delete")))
                    .clicked()
                {
                    pe.apply_all(true, |pi| {
                        pi.delete_layer()
                            .then(|| [0, 0, pi.img.width(), pi.img.height()])
                    });
                }
                if ui
                    .add_enabled(i + 1 < n, egui::Button::new(tr("pixel.layer.up")))
                    .clicked()
                {
                    pe.apply_all(true, |pi| {
                        pi.move_layer(true)
                            .then(|| [0, 0, pi.img.width(), pi.img.height()])
                    });
                }
                if ui
                    .add_enabled(i > 0, egui::Button::new(tr("pixel.layer.down")))
                    .clicked()
                {
                    pe.apply_all(true, |pi| {
                        pi.move_layer(false)
                            .then(|| [0, 0, pi.img.width(), pi.img.height()])
                    });
                }
                if ui
                    .add_enabled(i > 0, egui::Button::new(tr("pixel.layer.merge_down")))
                    .clicked()
                {
                    pe.apply_all(true, |pi| {
                        pi.merge_down()
                            .then(|| [0, 0, pi.img.width(), pi.img.height()])
                    });
                }
                if ui
                    .add_enabled(
                        pe.pi.is_layered(),
                        egui::Button::new(tr("pixel.layer.flatten")),
                    )
                    .clicked()
                {
                    pe.apply_all(true, |pi| {
                        pi.flatten()
                            .then(|| [0, 0, pi.img.width(), pi.img.height()])
                    });
                }
            });
            ui.horizontal_wrapped(|ui| {
                let name = new_layer_name(pe);
                if ui
                    .button(tr("pixel.layer.copy_to"))
                    .rich_tip(tr("pixel.layer.copy_to_tip"))
                    .clicked()
                {
                    pe.apply_all(true, |pi| {
                        pi.selection_to_layer(name.clone(), false)
                            .then(|| [0, 0, pi.img.width(), pi.img.height()])
                    });
                }
                if ui
                    .button(tr("pixel.layer.cut_to"))
                    .rich_tip(tr("pixel.layer.cut_to_tip"))
                    .clicked()
                {
                    pe.apply_all(true, |pi| {
                        pi.selection_to_layer(name.clone(), true)
                            .then(|| [0, 0, pi.img.width(), pi.img.height()])
                    });
                }
                if ui
                    .button(tr("pixel.layer.paste"))
                    .rich_tip(tr("pixel.layer.paste_tip"))
                    .clicked()
                {
                    if let Some(img) = crate::clipboard::read_system().image {
                        pe.apply_all(true, |pi| {
                            pixel_edit::paste_as_layer(pi, name.clone(), &img)
                        });
                    }
                }
            });
        });
}

fn selection_section(ui: &mut egui::Ui, pe: &mut crate::pixel_edit_app::PixelEdit) {
    egui::CollapsingHeader::new(tr("pixel.section.selection"))
        .default_open(true)
        .show(ui, |ui| {
            let (w, h) = pe.pi.img.dimensions();
            ui.horizontal_wrapped(|ui| {
                if ui.button(tr("pixel.sel.all")).clicked() {
                    pe.set_selection(Some(image::GrayImage::from_pixel(w, h, image::Luma([255]))));
                }
                if ui
                    .add_enabled(pe.pi.sel.is_some(), egui::Button::new(tr("pixel.sel.none")))
                    .clicked()
                {
                    pe.set_selection(None);
                }
                if ui.button(tr("pixel.sel.invert")).clicked() {
                    let inv = pixel_edit::invert(pe.pi.sel.as_ref(), w, h);
                    pe.set_selection(Some(inv));
                }
            });
            let has = pe.pi.sel.is_some();
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut pe.feather_px)
                        .range(1..=200)
                        .suffix(" px"),
                );
                if ui
                    .add_enabled(has, egui::Button::new(tr("pixel.sel.feather")))
                    .clicked()
                {
                    if let Some(m) = pe
                        .pi
                        .sel
                        .as_ref()
                        .map(|m| pixel_edit::feather(m, pe.feather_px))
                    {
                        pe.set_selection(Some(m));
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut pe.grow_px)
                        .range(-200..=200)
                        .suffix(" px"),
                );
                if ui
                    .add_enabled(has, egui::Button::new(tr("pixel.sel.grow")))
                    .clicked()
                {
                    if let Some(m) = pe.pi.sel.as_ref().map(|m| pixel_edit::grow(m, pe.grow_px)) {
                        pe.set_selection(Some(m));
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                if ui
                    .button(tr("pixel.sel.delete"))
                    .rich_tip(tr("pixel.sel.delete_tip"))
                    .clicked()
                {
                    pe.apply(false, pixel_edit::clear_selection);
                }
                if ui.button(tr("pixel.sel.fill")).clicked() {
                    let (c, o) = (pe.opts.color, pe.opts.opacity);
                    pe.apply(false, |pi| pixel_edit::fill_selection(pi, c, o));
                }
                if ui
                    .add_enabled(has, egui::Button::new(tr("pixel.sel.crop")))
                    .clicked()
                {
                    pe.apply_all(true, pixel_edit::crop_to_selection);
                }
            });
        });
}

fn retouch_section(ui: &mut egui::Ui, pe: &mut crate::pixel_edit_app::PixelEdit) {
    egui::CollapsingHeader::new(tr("pixel.section.retouch"))
        .default_open(true)
        .show(ui, |ui| {
            let has = pe.pi.sel.is_some();
            if ui
                .add_enabled(has, egui::Button::new(tr("pixel.content_fill")))
                .rich_tip(tr("pixel.content_fill_tip"))
                .clicked()
            {
                pe.apply(false, pixel_edit::content_fill);
            }
            if ui
                .button(tr("pixel.red_eye"))
                .rich_tip(tr("pixel.red_eye_tip"))
                .clicked()
            {
                pe.apply(false, pixel_edit::red_eye_selection);
            }
        });
}

fn effects_section(ui: &mut egui::Ui, pe: &mut crate::pixel_edit_app::PixelEdit) {
    egui::CollapsingHeader::new(tr("pixel.section.effects"))
        .default_open(true)
        .show(ui, |ui| {
            ui.label(
                RichText::new(tr("pixel.effects_hint"))
                    .small()
                    .color(Tokens::text_dim()),
            );
            // Light first: the effects most asked of a photo.
            ui.horizontal_wrapped(|ui| {
                for spec in crate::fx::light::EFFECTS {
                    let on = pe.effect.as_ref().is_some_and(|e| e.id == spec.id);
                    if ui
                        .selectable_label(on, tr(&format!("fx.{}", spec.id)))
                        .clicked()
                    {
                        pe.effect = Some(crate::fx::new_effect(spec.id));
                        pe.refresh_preview();
                    }
                }
            });
            let current = pe
                .effect
                .as_ref()
                .map(|e| tr(&format!("fx.{}", e.id)))
                .unwrap_or_else(|| tr("pixel.choose_effect"));
            let mut chosen: Option<&'static str> = None;
            egui::ComboBox::from_id_salt("pixel_effect")
                .width(220.0)
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for (group, list) in crate::fx::groups() {
                        ui.label(RichText::new(tr(group)).strong());
                        for spec in list.iter() {
                            if ui
                                .selectable_label(
                                    false,
                                    format!("   {}", tr(&format!("fx.{}", spec.id))),
                                )
                                .clicked()
                            {
                                chosen = Some(spec.id);
                            }
                        }
                    }
                });
            if let Some(id) = chosen {
                pe.effect = Some(crate::fx::new_effect(id));
                pe.refresh_preview();
            }
            let Some(spec) = pe.effect.as_ref().and_then(|e| crate::fx::spec(&e.id)) else {
                return;
            };
            let mut changed = false;
            if let Some(e) = pe.effect.as_mut() {
                for p in spec.params {
                    changed |= crate::ui::effect_dialog::param_stacked(ui, p, &mut e.params, None);
                }
            }
            ui.horizontal(|ui| {
                if ui
                    .checkbox(&mut pe.preview_on, tr("pixel.preview"))
                    .changed()
                {
                    changed = true;
                }
                if ui.button(tr("pixel.apply")).clicked() {
                    if let Some(e) = pe.effect.clone() {
                        pe.apply(true, |pi| pixel_edit::apply_effect(pi, &e));
                    }
                }
            });
            if changed {
                pe.refresh_preview();
            }
        });
}

fn picture_section(ui: &mut egui::Ui, pe: &mut crate::pixel_edit_app::PixelEdit) {
    egui::CollapsingHeader::new(tr("pixel.section.picture"))
        .default_open(false)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button(tr("pixel.flip_h")).clicked() {
                    pe.apply_all(true, |pi| {
                        pixel_edit::flip(pi, true);
                        Some([0, 0, pi.img.width(), pi.img.height()])
                    });
                }
                if ui.button(tr("pixel.flip_v")).clicked() {
                    pe.apply_all(true, |pi| {
                        pixel_edit::flip(pi, false);
                        Some([0, 0, pi.img.width(), pi.img.height()])
                    });
                }
            });
            let (w, h) = pe.pi.img.dimensions();
            ui.label(
                RichText::new(format!("{w} × {h} px"))
                    .small()
                    .color(Tokens::text_dim()),
            );
        });
}
