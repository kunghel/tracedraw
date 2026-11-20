//! The settings of the Light tool in the picture editing bar: the
//! lights (each with its colour), the active light's strength, mix,
//! direction and relief, and the light brush. See `pixel_light.rs` and
//! docs/behavior/edit-inside-picture.md.

use crate::i18n::tr;
use crate::pixel_edit_app::PixelEdit;
use crate::pixel_light::LightBlend;
use crate::theme::Tokens;
use crate::ui::tips::RichTip;
use egui::{Color32, RichText};

pub fn options(ui: &mut egui::Ui, pe: &mut PixelEdit) {
    // The lights: a colour and a name each; the chosen one is painted.
    let mut pick = None;
    let mut add = false;
    let mut remove = false;
    let n = pe.lights.list.len();
    pe.change_lights(|lights| {
        for i in 0..n {
            let Some(l) = lights.list.get_mut(i) else {
                continue;
            };
            let mut c = Color32::from_rgb(l.color[0], l.color[1], l.color[2]);
            if egui::color_picker::color_edit_button_srgba(
                ui,
                &mut c,
                egui::color_picker::Alpha::Opaque,
            )
            .changed()
            {
                l.color = [c.r(), c.g(), c.b()];
            }
            let name = format!("{} {}", tr("pixel.light.name"), i + 1);
            if ui
                .selectable_label(lights.active == i, name)
                .rich_tip(tr("pixel.light.name_tip"))
                .clicked()
            {
                pick = Some(i);
            }
        }
    });
    if let Some(i) = pick {
        pe.lights.active = i;
    }
    if ui
        .button(tr("pixel.light.add"))
        .rich_tip(tr("pixel.light.add_tip"))
        .clicked()
    {
        add = true;
    }
    if ui
        .add_enabled(n > 1, egui::Button::new(tr("pixel.light.remove")))
        .clicked()
    {
        remove = true;
    }
    if add {
        pe.add_light();
    }
    if remove {
        pe.remove_light();
    }
    ui.end_row();
    // The active light.
    pe.change_lights(|lights| {
        let i = lights.active.min(lights.list.len().saturating_sub(1));
        let Some(l) = lights.list.get_mut(i) else {
            return;
        };
        ui.checkbox(&mut l.visible, tr("pixel.light.visible"));
        let mut s = (l.strength * 100.0).round();
        if ui
            .add(
                egui::Slider::new(&mut s, 0.0..=300.0)
                    .suffix(" %")
                    .text(tr("pixel.light.strength")),
            )
            .changed()
        {
            l.strength = s / 100.0;
        }
        egui::ComboBox::from_id_salt("pixel_light_blend")
            .selected_text(tr(l.blend.key()))
            .show_ui(ui, |ui| {
                for b in LightBlend::ALL {
                    ui.selectable_value(&mut l.blend, b, tr(b.key()))
                        .rich_tip(tr(&format!("{}_tip", b.key())));
                }
            });
        ui.add(
            egui::Slider::new(&mut l.direction, 0.0..=360.0)
                .suffix("°")
                .text(tr("pixel.light.direction")),
        )
        .rich_tip(tr("pixel.light.direction_tip"));
        let mut r = (l.relief * 100.0).round();
        if ui
            .add(
                egui::Slider::new(&mut r, 0.0..=100.0)
                    .suffix(" %")
                    .text(tr("pixel.light.relief")),
            )
            .rich_tip(tr("pixel.light.relief_tip"))
            .changed()
        {
            l.relief = r / 100.0;
        }
    });
    ui.end_row();
    // The light brush.
    let o = &mut pe.opts;
    ui.add(
        egui::Slider::new(&mut o.size, 1.0..=1000.0)
            .logarithmic(true)
            .suffix(" px")
            .text(tr("pixel.opt.size")),
    );
    for (key, v) in [
        ("pixel.opt.hardness", &mut o.hardness),
        ("pixel.opt.flow", &mut o.flow),
    ] {
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
    ui.checkbox(&mut pe.lights.erase, tr("pixel.light.erase"))
        .rich_tip(tr("pixel.light.erase_tip"));
    if ui
        .button(tr("pixel.light.fill"))
        .rich_tip(tr("pixel.light.fill_tip"))
        .clicked()
    {
        pe.fill_light();
    }
    let painted = pe
        .lights
        .list
        .get(pe.lights.active)
        .is_some_and(|l| l.is_painted());
    if ui
        .add_enabled(painted, egui::Button::new(tr("pixel.light.clear")))
        .clicked()
    {
        pe.clear_light();
    }
    ui.label(
        RichText::new(tr("pixel.tool.light_hint"))
            .color(Tokens::text_dim())
            .small(),
    );
}
