//! A bitmap effect's settings in the Effects docker (#4253), an option
//! beside the dialog: the effect is picked from a list and its values set
//! with the sliders at the top of the docker, the drawing beside it
//! showing the result as the values change, as in the image editing
//! mode's Effects panel. Apply keeps the effect (one undo step), Cancel
//! takes the preview back. The dialog with Before and After stays the
//! default; its "Settings in the Effects panel" box (or this panel's)
//! switches between the two.
//!
//! The panel holds the same [`EffectState`] as the dialog, so presets,
//! Preview and All pages work the same way. A selection changed while the
//! panel is open cancels it, as the effect was meant for the pictures
//! that were selected.

use crate::app::{App, DockerTab};
use crate::i18n::tr;
use crate::theme::Tokens;
use crate::ui::btn_icons::IconUi;
use crate::ui::effect_dialog::EffectState;
use crate::ui::tips::RichTip;
use egui::Ui;

/// Effects whose settings the panel can hold: Image Adjustments and the
/// Tone Curve need their own large views and keep their dialogs.
pub fn fits(id: &str) -> bool {
    id != "image_adjustments" && id != "tone_curve"
}

/// Show `st` in the panel when the option is on and the effect fits;
/// true when it went there (the dialog then closes).
pub fn take_over(app: &mut App, st: &EffectState) -> bool {
    if !app.settings.fx_in_panel || !fits(&st.effect.id) || app.selection.is_empty() {
        return false;
    }
    app.fx_panel_sel = app.selection.clone();
    app.fx_panel = Some(st.clone());
    app.open_docker(DockerTab::Effects);
    true
}

/// Each frame: a panel whose pictures are no longer the selection, or
/// opened in the image editing mode, ends with its preview taken back.
pub fn watch(app: &mut App) {
    if app.fx_panel.is_none() {
        return;
    }
    if app.selection != app.fx_panel_sel || app.pixel_edit.is_some() {
        app.fx_panel = None;
        app.end_bitmap_effect_preview();
    }
}

/// What the buttons under the settings asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ask {
    Apply,
    Cancel,
}

/// The panel, at the top of the Effects docker while an effect is set
/// there: the effect's list, its settings, Reset, the presets, Preview,
/// All pages, Apply and Cancel.
pub fn panel(app: &mut App, ui: &mut Ui) {
    let Some(mut st) = app.fx_panel.take() else {
        return;
    };
    let Some(spec) = crate::fx::spec(&st.effect.id) else {
        app.end_bitmap_effect_preview();
        return;
    };
    let mut ask = None;
    let mut switch = None;
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(egui::RichText::new(tr("fx_panel.title")).font(crate::theme::bold(13.0)));
        ui.add_space(2.0);
        // Adding: any effect can be picked from the list; editing one of
        // the picture's effects keeps that effect.
        let name = tr(&format!("fx.{}", spec.id));
        if st.edit.is_none() {
            let mut picked = None;
            crate::ui::keynav::ComboBox::from_id_salt("fx_panel_effect")
                .selected_text(name)
                .width(crate::ui::docker_form::fit_w(ui, 240.0))
                .show_ui(ui, |ui| {
                    for (group, list) in crate::fx::groups() {
                        ui.label(egui::RichText::new(tr(group)).color(Tokens::text_dim()));
                        for s in list.iter().filter(|s| fits(s.id) && !s.params.is_empty()) {
                            if ui
                                .selectable_label(s.id == spec.id, tr(&format!("fx.{}", s.id)))
                                .clicked()
                            {
                                picked = Some(s.id);
                            }
                        }
                    }
                })
                .response
                .rich_tip(crate::ui::tips::keyed("fx_panel.effect"));
            switch = picked.filter(|id| *id != spec.id);
        } else {
            ui.strong(name);
        }
        ui.add_space(4.0);
        if spec.params.is_empty() {
            ui.label(tr("dialog.no_settings"));
        }
        for p in spec.params {
            ui.vertical(|ui| {
                crate::ui::effect_dialog::param_stacked(ui, p, &mut st.effect.params, None)
            });
            ui.add_space(2.0);
        }
        if ui
            .ibutton("dialog.reset")
            .rich_tip(crate::ui::tips::keyed("dialog.reset"))
            .clicked()
        {
            st.effect.params = crate::fx::new_effect(spec.id).params;
        }
        ui.add_space(4.0);
        crate::ui::effect_dialog::presets_row(ui, app, &mut st);
        crate::ui::effect_dialog::preview_row(ui, app, &mut st.preview);
        crate::ui::effect_dialog::all_pages_row(ui, app, &mut st);
        let mut on = app.settings.fx_in_panel;
        if ui
            .checkbox(&mut on, tr("dialog.fx_in_panel"))
            .rich_tip(crate::ui::tips::keyed("dialog.fx_in_panel"))
            .changed()
        {
            app.settings.fx_in_panel = on;
            app.settings.save();
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui
                .ibutton("fx_panel.apply")
                .rich_tip(crate::ui::tips::keyed("fx_panel.apply"))
                .clicked()
            {
                ask = Some(Ask::Apply);
            }
            if ui
                .button(tr("dialog.cancel"))
                .rich_tip(crate::ui::tips::pair("dialog.cancel", "fx_panel.cancel_desc"))
                .clicked()
            {
                ask = Some(Ask::Cancel);
            }
        });
    });
    if let Some(id) = switch {
        // Another effect: its own settings, previewed from the next frame.
        st.effect = crate::fx::new_effect(id);
        st.pick = None;
        app.fx_panel = Some(st);
        return;
    }
    match ask {
        Some(Ask::Apply) => app.commit_bitmap_effect(&st),
        Some(Ask::Cancel) => app.end_bitmap_effect_preview(),
        None => {
            app.preview_bitmap_effect(ui.ctx(), &st);
            app.fx_panel = Some(st);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::{Rect, ShapeKind};

    fn picture(app: &mut App) -> tracedraw_core::ShapeId {
        let img = image::RgbaImage::from_fn(8, 6, |x, y| {
            image::Rgba([(x * 30) as u8, (y * 40) as u8, 90, 255])
        });
        app.new_shape(ShapeKind::Bitmap {
            rect: Rect::new(0.0, 0.0, 8.0, 6.0),
            width_px: 8,
            height_px: 6,
            png: crate::bitmap_fx::encode(&img).expect("png").into(),
            fx: None,
        })
        .expect("bitmap")
    }

    fn effects(app: &App, id: tracedraw_core::ShapeId) -> usize {
        app.bitmap_effects(id).len()
    }

    fn draw(ctx: &egui::Context, app: &mut App) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(320.0, 900.0),
            )),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| panel(app, ui));
        out.textures_delta.clear();
    }

    /// With the option on, an effect's settings go to the panel instead
    /// of the dialog; Apply keeps the effect in one step.
    #[test]
    fn the_panel_takes_the_effect_and_apply_keeps_it() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        let id = picture(&mut app);
        app.select(vec![id]);
        let mut st = EffectState::new("gaussian_blur");
        st.preview = false;
        assert!(!take_over(&mut app, &st), "off by default: the dialog");
        app.settings.fx_in_panel = true;
        assert!(!take_over(&mut app, &EffectState::new("tone_curve")));
        assert!(take_over(&mut app, &st));
        assert!(app.open_dockers.contains(&DockerTab::Effects));
        draw(&ctx, &mut app);
        assert!(app.fx_panel.is_some(), "still open");
        let st = app.fx_panel.take().expect("state");
        app.commit_bitmap_effect(&st);
        assert_eq!(effects(&app, id), 1);
        assert_eq!(app.engine.undo_label(), Some("Add Effect"));
    }

    /// Another selection cancels the panel, adding nothing.
    #[test]
    fn a_new_selection_ends_the_panel() {
        let mut app = App::headless();
        let id = picture(&mut app);
        let other = picture(&mut app);
        app.select(vec![id]);
        app.settings.fx_in_panel = true;
        assert!(take_over(&mut app, &EffectState::new("gaussian_blur")));
        watch(&mut app);
        assert!(app.fx_panel.is_some());
        app.select(vec![other]);
        watch(&mut app);
        assert!(app.fx_panel.is_none());
        assert_eq!(effects(&app, id), 0);
    }
}
