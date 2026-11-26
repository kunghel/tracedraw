//! The Layer Style part of the image editing mode's Layers panel
//! (#4369): the effects of the layer in use, each turned on with its box
//! and set under it, shown on the picture at once and changeable until
//! the layers are flattened or the editing is finished. A drag of a
//! setting is one undo step. See `pixel_style.rs`.

use crate::i18n::tr;
use crate::pixel_edit_app::PixelEdit;
use crate::pixel_style::{BevelStyle, GlowSource, LayerStyle, StrokeAt};
use crate::theme::Tokens;
use crate::ui::pixel_layers::{icon_button, LayerIcon};
use crate::ui::tips::RichTip;
use egui::Ui;
use tracedraw_core::live::MergeMode;

/// Where a copied layer style is kept while the program runs.
fn clip_id() -> egui::Id {
    egui::Id::new("pixel_layer_style_clip")
}

pub(crate) fn style_section(ui: &mut Ui, pe: &mut PixelEdit) {
    let i = pe.pi.active;
    let Some(l) = pe.pi.layers.get(i) else {
        return;
    };
    if l.adjust.is_some() {
        return;
    }
    let had = l.style;
    let mut st = had.unwrap_or_default();
    let count = st.count();
    let title = if count > 0 {
        format!("{} ({count})", tr("pixel.style.title"))
    } else {
        tr("pixel.style.title")
    };
    let mut clear = false;
    let mut paste: Option<LayerStyle> = None;
    egui::CollapsingHeader::new(egui::RichText::new(title).strong())
        .id_salt(("pixel_layer_style", pe.session))
        .default_open(had.is_some())
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                ui.checkbox(&mut st.on, tr("pixel.style.show"))
                    .rich_tip(tr("pixel.style.show_tip"));
                ui.add_space(6.0);
                if icon_button(ui, LayerIcon::Duplicate, &tr("pixel.style.copy"), had.is_some(), false).clicked() {
                    ui.data_mut(|d| d.insert_temp(clip_id(), st));
                }
                let kept: Option<LayerStyle> = ui.data(|d| d.get_temp(clip_id()));
                if icon_button(ui, LayerIcon::Paste, &tr("pixel.style.paste"), kept.is_some(), false).clicked() {
                    paste = kept;
                }
                if icon_button(ui, LayerIcon::Delete, &tr("pixel.style.clear"), had.is_some(), false).clicked() {
                    clear = true;
                }
            });
            let mut fill = (st.fill_opacity * 100.0).round();
            if ui
                .add(egui::Slider::new(&mut fill, 0.0..=100.0).suffix(" %").text(tr("pixel.style.fill_opacity")))
                .rich_tip(tr("pixel.style.fill_opacity_tip"))
                .changed()
            {
                st.fill_opacity = fill / 100.0;
            }
            ui.add_space(2.0);
            let mut on = st.drop_shadow.on;
            effect(ui, &mut on, "pixel.style.drop_shadow", |ui| {
                shadow_rows(ui, &mut st.drop_shadow, "drop");
            });
            st.drop_shadow.on = on;
            let mut on = st.outer_glow.on;
            effect(ui, &mut on, "pixel.style.outer_glow", |ui| {
                glow_rows(ui, &mut st.outer_glow, "outer", false);
            });
            st.outer_glow.on = on;
            let mut on = st.color_overlay.on;
            effect(ui, &mut on, "pixel.style.color_overlay", |ui| {
                let c = &mut st.color_overlay;
                color_row(ui, &mut c.color);
                merge_row(ui, &mut c.merge, "overlay");
                percent(ui, "pixel.opt.opacity", &mut c.opacity);
            });
            st.color_overlay.on = on;
            let mut on = st.gradient_overlay.on;
            effect(ui, &mut on, "pixel.style.gradient_overlay", |ui| {
                let g = &mut st.gradient_overlay;
                ui.horizontal(|ui| {
                    ui.label(tr("pixel.style.from"));
                    color4(ui, &mut g.from);
                    ui.label(tr("pixel.style.to"));
                    color4(ui, &mut g.to);
                    ui.checkbox(&mut g.reverse, tr("pixel.style.reverse"));
                });
                ui.checkbox(&mut g.radial, tr("pixel.opt.radial"));
                if !g.radial {
                    angle(ui, &mut g.angle);
                }
                let mut sc = (g.scale * 100.0).round();
                if ui
                    .add(egui::Slider::new(&mut sc, 10.0..=150.0).suffix(" %").text(tr("pixel.style.scale")))
                    .changed()
                {
                    g.scale = sc / 100.0;
                }
                merge_row(ui, &mut g.merge, "gradient");
                percent(ui, "pixel.opt.opacity", &mut g.opacity);
            });
            st.gradient_overlay.on = on;
            let mut on = st.inner_glow.on;
            effect(ui, &mut on, "pixel.style.inner_glow", |ui| {
                glow_rows(ui, &mut st.inner_glow, "inner", true);
            });
            st.inner_glow.on = on;
            let mut on = st.inner_shadow.on;
            effect(ui, &mut on, "pixel.style.inner_shadow", |ui| {
                shadow_rows(ui, &mut st.inner_shadow, "inner");
            });
            st.inner_shadow.on = on;
            let mut on = st.bevel.on;
            effect(ui, &mut on, "pixel.style.bevel", |ui| {
                let b = &mut st.bevel;
                egui::ComboBox::from_id_salt("pixel_style_bevel_kind")
                    .selected_text(tr(b.style.key()))
                    .show_ui(ui, |ui| {
                        for k in BevelStyle::ALL {
                            ui.selectable_value(&mut b.style, k, tr(k.key()));
                        }
                    });
                ui.checkbox(&mut b.down, tr("pixel.style.bevel_down"));
                let mut d = (b.depth * 100.0).round();
                if ui
                    .add(egui::Slider::new(&mut d, 10.0..=1000.0).logarithmic(true).suffix(" %").text(tr("pixel.style.depth")))
                    .changed()
                {
                    b.depth = d / 100.0;
                }
                px(ui, "pixel.style.size", &mut b.size, 1.0..=250.0);
                px(ui, "pixel.style.soften", &mut b.soften, 0.0..=50.0);
                angle(ui, &mut b.angle);
                ui.add(
                    egui::Slider::new(&mut b.altitude, 0.0..=90.0)
                        .suffix("\u{b0}")
                        .text(tr("pixel.style.altitude")),
                );
                ui.horizontal(|ui| {
                    ui.label(tr("pixel.style.highlight"));
                    ui.color_edit_button_srgb(&mut b.highlight);
                    percent_short(ui, &mut b.highlight_opacity);
                });
                ui.horizontal(|ui| {
                    ui.label(tr("pixel.style.shade"));
                    ui.color_edit_button_srgb(&mut b.shadow);
                    percent_short(ui, &mut b.shadow_opacity);
                });
            });
            st.bevel.on = on;
            let mut on = st.stroke.on;
            effect(ui, &mut on, "pixel.style.stroke", |ui| {
                let s = &mut st.stroke;
                px(ui, "pixel.style.size", &mut s.size, 1.0..=250.0);
                ui.horizontal(|ui| {
                    for at in StrokeAt::ALL {
                        ui.radio_value(&mut s.at, at, tr(at.key()));
                    }
                });
                color_row(ui, &mut s.color);
                merge_row(ui, &mut s.merge, "stroke");
                percent(ui, "pixel.opt.opacity", &mut s.opacity);
            });
            st.stroke.on = on;
        });
    let next = if clear {
        None
    } else if let Some(p) = paste {
        Some(p)
    } else if had.is_none() && st == LayerStyle::default() {
        None
    } else {
        Some(st)
    };
    let held = ui.input(|i| i.pointer.any_down());
    if next != had {
        if !pe.style_drag {
            pe.remember();
        }
        if let Some(l) = pe.pi.layers.get_mut(i) {
            l.style = next;
        }
        pe.view.borrow_mut().full = true;
        pe.bump();
        pe.style_drag = held;
    } else if !held {
        pe.style_drag = false;
    }
}

/// One effect: its box, and its settings under it while it is on.
fn effect(ui: &mut Ui, on: &mut bool, key: &str, rows: impl FnOnce(&mut Ui)) {
    ui.checkbox(on, egui::RichText::new(tr(key)))
        .rich_tip(tr(&format!("{key}_tip")));
    if *on {
        egui::Frame::new()
            .inner_margin(egui::Margin { left: 18, right: 0, top: 0, bottom: 4 })
            .show(ui, rows);
    }
}

fn percent(ui: &mut Ui, key: &str, v: &mut f32) {
    let mut p = (*v * 100.0).round();
    if ui
        .add(egui::Slider::new(&mut p, 0.0..=100.0).suffix(" %").text(tr(key)))
        .changed()
    {
        *v = p / 100.0;
    }
}

fn percent_short(ui: &mut Ui, v: &mut f32) {
    let mut p = (*v * 100.0).round();
    if ui
        .add(egui::DragValue::new(&mut p).range(0.0..=100.0).suffix(" %"))
        .changed()
    {
        *v = p / 100.0;
    }
}

fn px(ui: &mut Ui, key: &str, v: &mut f32, range: std::ops::RangeInclusive<f32>) {
    ui.add(egui::Slider::new(v, range).suffix(" px").text(tr(key)));
}

fn angle(ui: &mut Ui, v: &mut f32) {
    ui.add(
        egui::Slider::new(v, -180.0..=180.0)
            .suffix("\u{b0}")
            .text(tr("pixel.style.angle")),
    )
    .rich_tip(tr("pixel.style.angle_tip"));
}

fn color_row(ui: &mut Ui, c: &mut [u8; 3]) {
    ui.horizontal(|ui| {
        ui.label(tr("pixel.opt.color"));
        ui.color_edit_button_srgb(c);
    });
}

fn color4(ui: &mut Ui, c: &mut [u8; 4]) {
    let mut col = egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]);
    if egui::color_picker::color_edit_button_srgba(ui, &mut col, egui::color_picker::Alpha::OnlyBlend).changed() {
        *c = col.to_srgba_unmultiplied();
    }
}

fn merge_row(ui: &mut Ui, m: &mut MergeMode, salt: &str) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(tr("pixel.style.merge")).color(Tokens::text_dim()));
        egui::ComboBox::from_id_salt(("pixel_style_merge", salt))
            .selected_text(m.name())
            .show_ui(ui, |ui| {
                for k in MergeMode::ALL {
                    ui.selectable_value(m, k, k.name());
                }
            });
    });
}

fn shadow_rows(ui: &mut Ui, s: &mut crate::pixel_style::Shadow, salt: &str) {
    color_row(ui, &mut s.color);
    merge_row(ui, &mut s.merge, salt);
    percent(ui, "pixel.opt.opacity", &mut s.opacity);
    angle(ui, &mut s.angle);
    px(ui, "pixel.style.distance", &mut s.distance, 0.0..=500.0);
    px(ui, "pixel.style.size", &mut s.size, 0.0..=250.0);
    percent(ui, "pixel.style.spread", &mut s.spread);
}

fn glow_rows(ui: &mut Ui, g: &mut crate::pixel_style::Glow, salt: &str, inner: bool) {
    color_row(ui, &mut g.color);
    merge_row(ui, &mut g.merge, salt);
    percent(ui, "pixel.opt.opacity", &mut g.opacity);
    px(ui, "pixel.style.size", &mut g.size, 0.0..=250.0);
    percent(ui, "pixel.style.spread", &mut g.spread);
    if inner {
        ui.horizontal(|ui| {
            ui.radio_value(&mut g.source, GlowSource::Edge, tr("pixel.style.from_edge"));
            ui.radio_value(&mut g.source, GlowSource::Center, tr("pixel.style.from_center"));
        });
    }
}
