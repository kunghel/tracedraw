//! The Contour tool's property bar and the controls it shares with the
//! Contour panel: To centre, Inside, Outside; steps and offset; the
//! corners; the colour direction and the last step's fill and outline
//! colours; the object and colour accelerations. They show the selected
//! contour and edit the selected contours live.

use crate::app::App;
use crate::contour_tool::{blend_key, join_key, ContourDirection};
use crate::i18n::tr;
use crate::theme::Tokens;
use crate::ui::field::NumField;
use crate::ui::fill_tool_bars::color_pick;
use crate::ui::propbar::sep;
use crate::ui::tips::RichTip;
use egui::Ui;
use tracedraw_core::shaping::Join;

fn dim(ui: &mut Ui, text: String) {
    ui.label(
        egui::RichText::new(text)
            .color(Tokens::text_dim())
            .size(11.0),
    );
}

/// An acceleration slider, -100 to 100 %.
fn accel(ui: &mut Ui, key: &str, v: &mut f64) -> bool {
    let mut p = (*v * 100.0).round() as i32;
    let r = crate::ui::toolbar::dim_labelled(ui, key, |ui| {
        ui.add(crate::ui::Slider::new(&mut p, -100..=100).suffix(" %"))
    });
    if r.changed() {
        *v = p as f64 / 100.0;
    }
    r.changed()
}

/// The controls, a group to a row.
pub fn contour_controls(app: &mut App, ui: &mut Ui) {
    app.sync_contour_settings();
    // A box of these controls typed into and left with Enter: the focus
    // it held before them is gone after them.
    let focus_before = ui.memory(|m| m.focused());
    let palette = app.palette.clone();
    let mut s = app.contour;
    let mut changed = false;
    ui.horizontal(|ui| {
        for d in ContourDirection::ALL {
            if ui
                .selectable_label(s.direction == d, tr(d.key()))
                .rich_tip(crate::ui::tips::keyed(d.key()))
                .clicked()
                && s.direction != d
            {
                s.direction = d;
                changed = true;
            }
        }
    });
    ui.horizontal(|ui| {
        crate::ui::toolbar::dim_labelled_as(ui, "docker.steps", "contour.steps_desc", |ui| {
            // A slider (with its number box), stepped on a logarithmic
            // scale so the first steps are easy to pick (#1265).
            ui.add_enabled_ui(s.direction != ContourDirection::ToCenter, |ui| {
                changed |= ui
                    .add(
                        crate::ui::Slider::new(&mut s.steps, 1..=999)
                            .logarithmic(true)
                            .width(120.0),
                    )
                    .changed();
            });
        });
        let mut o = app.units.world_from_mm(s.offset);
        if crate::ui::toolbar::dim_labelled_as(ui, "docker.offset", "contour.offset_desc", |ui| {
            ui.add(
                NumField::new(&mut o)
                    .range(0.0..=app.units.world_from_mm(1000.0))
                    .length_decimals(app.settings.precision.min(10) as usize)
                    .unit_mm(app.units.world_to_mm(1.0))
                    .suffix(format!(" {}", app.units.short()))
                    .width(76.0),
            )
        })
        .changed()
        {
            s.offset = app.units.world_to_mm(o);
            changed = true;
        }
        crate::ui::keynav::ComboBox::from_id_salt("contour_corners")
            .selected_text(tr(join_key(s.corners)))
            .width(80.0)
            .show_ui(ui, |ui| {
                for j in [Join::Miter, Join::Round, Join::Bevel] {
                    if ui
                        .selectable_label(s.corners == j, tr(join_key(j)))
                        .clicked()
                        && s.corners != j
                    {
                        s.corners = j;
                        changed = true;
                    }
                }
            })
            .response
            .rich_tip(tr("contour.corners"));
    });
    ui.horizontal(|ui| {
        crate::ui::keynav::ComboBox::from_id_salt("contour_blend")
            .selected_text(tr(blend_key(s.color_blend)))
            .width(110.0)
            .show_ui(ui, |ui| {
                for b in 0..3u8 {
                    if ui
                        .selectable_label(s.color_blend == b, tr(blend_key(b)))
                        .clicked()
                        && s.color_blend != b
                    {
                        s.color_blend = b;
                        changed = true;
                    }
                }
            })
            .response
            .rich_tip(tr("docker.color_blend"));
        changed |= color_pick(
            ui,
            &palette,
            "contour_fill_to",
            &mut s.fill_to,
            &tr("docker.fill_color"),
            true,
        );
        changed |= color_pick(
            ui,
            &palette,
            "contour_outline_to",
            &mut s.outline_to,
            &tr("docker.outline_color"),
            true,
        );
        // The end of a fountain fill (#1265): only fountain-filled
        // objects use it, their steps keeping the fountain.
        changed |= color_pick(
            ui,
            &palette,
            "contour_fill_end_to",
            &mut s.fill_end_to,
            &format!(
                "{}\n{}",
                tr("contour.last_fill_color"),
                tr("contour.last_fill_color_desc")
            ),
            true,
        );
    });
    ui.horizontal(|ui| {
        changed |= accel(ui, "docker.object_acceleration", &mut s.accel_objects);
        changed |= accel(ui, "docker.color_acceleration", &mut s.accel_colors);
    });
    if changed {
        app.contour = s;
        app.apply_contour_settings();
    }
    let entered = focus_before.is_some()
        && ui.memory(|m| m.focused()).is_none()
        && ui.input(|i| i.key_pressed(egui::Key::Enter));
    if entered {
        apply_on_enter(app);
    }
}

/// Enter in a box of the controls applies them as the Apply button does:
/// objects with no contour get one (d#2945; a contour already there
/// follows every change).
fn apply_on_enter(app: &mut App) {
    if app.selected_contour().is_none() && has_bare(app) {
        app.add_contour();
    }
}

/// Selected objects that can take a contour and have none: a group too,
/// drawn round its objects (#3245); Apply was not offered for one, so a
/// selected group seemed unable to take a contour (#3667).
fn has_bare(app: &App) -> bool {
    app.selected_shapes().iter().any(|s| {
        !s.effects
            .iter()
            .any(|e| matches!(e, tracedraw_core::live::Effect::Contour { .. }))
            && !matches!(s.kind, tracedraw_core::ShapeKind::Bitmap { .. })
    })
}

/// Copy Contour From: the next object clicked gives the selected objects
/// its contour, as Object > Copy Effect > Contour From does (#3667).
fn start_copy_contour(app: &mut App) {
    app.pending_copy_effect = Some(crate::copy_effect::CopyKind::Contour);
    app.status = tr("status.click_source_object");
}

/// Apply (for selected objects with no contour), Clear Contour, Break
/// Apart and Copy Contour From.
pub fn contour_actions(app: &mut App, ui: &mut Ui) {
    let has = app.selected_contour().is_some();
    let bare = has_bare(app);
    ui.horizontal(|ui| {
        if bare
            && ui
                .button(tr("docker.apply"))
                .rich_tip(crate::ui::tips::pair("docker.apply", "contour.apply_desc"))
                .clicked()
        {
            app.add_contour();
        }
        if ui
            .add_enabled(has, egui::Button::new(tr("contour.clear")))
            .rich_tip(crate::ui::tips::keyed("contour.clear"))
            .clicked()
        {
            app.clear_contour();
        }
        if ui
            .add_enabled(has, egui::Button::new(tr("docker.break_apart")))
            .rich_tip(crate::ui::tips::pair(
                "docker.break_apart",
                "toolbar.effect_break_apart_desc",
            ))
            .clicked()
        {
            app.break_effect_groups();
        }
        if ui
            .add_enabled(
                !app.selection.is_empty(),
                egui::Button::new(tr("contour.copy_from")),
            )
            .rich_tip(crate::ui::tips::pair(
                "contour.copy_from",
                "contour.copy_from_desc",
            ))
            .clicked()
        {
            start_copy_contour(app);
        }
    });
}

/// The bar.
pub fn contour_bar(app: &mut App, ui: &mut Ui) {
    contour_controls(app, ui);
    sep(ui);
    contour_actions(app, ui);
    if app.selection.is_empty() {
        sep(ui);
        dim(ui, tr("toolbar.contour_hint"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::{geometry::Rect, ShapeKind};

    #[test]
    fn the_bar_and_the_panel_draw() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        app.tool = crate::tools::Tool::Contour;
        let id = app
            .new_shape(ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 20.0, 20.0),
                radius: 0.0,
                corners: None,
            })
            .expect("square");
        let frame = |app: &mut App, panel: bool| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1600.0, 400.0),
                )),
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                if panel {
                    ui.vertical(|ui| {
                        contour_controls(app, ui);
                        contour_actions(app, ui);
                    });
                } else {
                    ui.horizontal(|ui| contour_bar(app, ui));
                }
            });
            out.textures_delta.clear();
        };
        for d in ContourDirection::ALL {
            app.contour.direction = d;
            frame(&mut app, false);
        }
        app.select(vec![id]);
        app.contour.direction = ContourDirection::Inside;
        app.add_contour();
        app.contour.direction = ContourDirection::Outside;
        frame(&mut app, true);
        assert_eq!(app.contour.direction, ContourDirection::Inside);
    }

    /// A selected group is offered Apply, and Copy Contour From gives
    /// the selection the contour of the object clicked next (#3667).
    #[test]
    fn a_group_is_offered_apply_and_a_contour_can_be_copied() {
        let mut app = App::headless();
        app.tool = crate::tools::Tool::Contour;
        let square = |app: &mut App, x: f64| {
            app.new_shape(ShapeKind::Rect {
                rect: Rect::new(x, 0.0, x + 20.0, 20.0),
                radius: 0.0,
                corners: None,
            })
            .expect("square")
        };
        let (a, b) = (square(&mut app, 0.0), square(&mut app, 30.0));
        app.select(vec![a, b]);
        app.group_selection();
        let g = app.selection[0];
        assert!(has_bare(&app), "a group without a contour is offered Apply");
        // The source: a third object with a contour of 4 steps.
        let src = square(&mut app, 100.0);
        app.select(vec![src]);
        app.contour.steps = 4;
        app.add_contour();
        let contour = |app: &App, id| {
            app.doc().find_shape(id).and_then(|s| {
                s.effects
                    .iter()
                    .find(|e| matches!(e, tracedraw_core::live::Effect::Contour { .. }))
                    .cloned()
            })
        };
        let want = contour(&app, src).expect("the source has a contour");
        app.select(vec![g]);
        start_copy_contour(&mut app);
        assert_eq!(
            app.pending_copy_effect,
            Some(crate::copy_effect::CopyKind::Contour)
        );
        let kind = app.pending_copy_effect.take().expect("waiting");
        app.copy_effect_from(src, kind);
        assert_eq!(contour(&app, g), Some(want));
    }

    #[test]
    fn enter_in_a_box_of_the_panel_applies_the_contour() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        app.tool = crate::tools::Tool::Contour;
        let id = app
            .new_shape(ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 20.0, 20.0),
                radius: 0.0,
                corners: None,
            })
            .expect("square");
        app.select(vec![id]);
        let key = |key: egui::Key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        };
        let frame = |app: &mut App, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400.0, 800.0),
                )),
                events,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                ui.vertical(|ui| {
                    contour_controls(app, ui);
                    contour_actions(app, ui);
                });
            });
            out.textures_delta.clear();
        };
        frame(&mut app, Vec::new());
        // Tab to the first number box (a text field) of the panel.
        let mut in_box = false;
        for _ in 0..40 {
            frame(&mut app, vec![key(egui::Key::Tab)]);
            frame(&mut app, Vec::new());
            let focused = ctx.memory(|m| m.focused());
            if focused.is_some_and(|f| egui::TextEdit::load_state(&ctx, f).is_some()) {
                in_box = true;
                break;
            }
        }
        assert!(in_box, "a number box takes the focus");
        assert!(app.selected_contour().is_none());
        frame(&mut app, vec![key(egui::Key::Enter)]);
        assert!(
            app.selected_contour().is_some(),
            "Enter applied the contour as Apply does"
        );
        // Enter again does not stack a second contour.
        let effects = |app: &App| app.selected_shapes()[0].effects.len();
        let n = effects(&app);
        apply_on_enter(&mut app);
        assert_eq!(effects(&app), n);
    }
}
