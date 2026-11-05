//! The TraceCut window (Launch > TraceCut, Tools > TraceCut): the tools
//! grouped on the left (Cut, Nesting, Measure); on the right the chosen
//! tool's presets and settings, a preview of the selection with what the
//! tool finds, a status line, and Apply, OK and Cancel. Apply works the
//! result out into the preview without touching the drawing; OK applies
//! it in one undo step and closes. See docs/behavior/tracecut.md.
//!
//! The preset row and the preview are adapted from the Sign & Cut window
//! kit of #170 by LaMka21.

use crate::app::{App, Units};
use crate::i18n::tr;
use crate::theme::Tokens;
use crate::tracecut::{Group, Scene, ToolId, TraceCutState, TOLERANCE_RANGE};
use crate::ui::tips::RichTip;
use egui::{
    Align2, Color32, Context, CornerRadius, FontId, Pos2, Sense, Stroke, StrokeKind, Ui, Vec2,
};
use tracedraw_core::geometry::{PathEl, Point};

/// Width of the tool list.
const LIST_W: f32 = 210.0;
/// The window's first and smallest size.
const DEFAULT_SIZE: Vec2 = Vec2::new(880.0, 600.0);
const MIN_SIZE: Vec2 = Vec2::new(660.0, 460.0);
/// Width of the labels of the settings rows.
const LABEL_W: f32 = 170.0;
/// Room kept under the preview for the status line and the buttons.
const BELOW_PREVIEW: f32 = 74.0;
/// Colours of the preview.
const OBJECT: Color32 = Color32::from_gray(70);
const MARKED: Color32 = Color32::from_rgb(0xD0, 0x20, 0x20);
/// Moves with the knife up.
const MOVE: Color32 = Color32::from_rgb(0x30, 0x80, 0xD0);
/// The status line under the preview.
const STRIP: Color32 = Color32::from_rgb(0xFF, 0xF4, 0xB0);

/// Which bottom button was pressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Button {
    Apply,
    Ok,
    Cancel,
}

/// Draw the TraceCut window.
pub fn dialog(app: &mut App, ctx: &Context, st: &mut TraceCutState, close: &mut bool) {
    let title = tr("tracecut.title");
    let mut pressed = None;
    crate::ui::chrome::window(title)
        .resizable(DEFAULT_SIZE, MIN_SIZE)
        .show(ctx, |ui| {
            let body = ui.available_size();
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    Vec2::new(LIST_W, body.y),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(LIST_W);
                        tool_list(app, ui, st);
                    },
                );
                ui.separator();
                let right_w = ui.available_width().max(300.0);
                ui.allocate_ui_with_layout(
                    Vec2::new(right_w, body.y),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(right_w);
                        pressed = tool_pane(app, ui, st, right_w);
                    },
                );
            });
            if pressed.is_none() && crate::ui::dialogs::enter_pressed(ui) {
                pressed = Some(Button::Ok);
            }
        });
    match pressed {
        Some(Button::Apply) => {
            st.worked = Some(app.tracecut_work_out(st.tool, &st.settings));
        }
        Some(Button::Ok) => {
            let worked = match st.worked.take() {
                Some(w) if app.tracecut_current(&w, st.tool, &st.settings) => w,
                _ => app.tracecut_work_out(st.tool, &st.settings),
            };
            if !worked.targets.is_empty() || st.tool == ToolId::RegistrationMarks {
                app.tracecut_commit(&worked, &st.settings, Some(ctx.clone()));
            }
            *close = true;
        }
        Some(Button::Cancel) => *close = true,
        None => {}
    }
}

/// The groups and their tools.
fn tool_list(app: &mut App, ui: &mut Ui, st: &mut TraceCutState) {
    ui.add_space(4.0);
    for g in Group::ALL {
        let tools = g.tools();
        if tools.is_empty() {
            continue;
        }
        crate::ui::chrome::section(ui, &tr(g.key()));
        for t in tools {
            let on = st.tool == t;
            if ui
                .add_sized(
                    [LIST_W - 8.0, 24.0],
                    egui::Button::selectable(on, tr(t.key())),
                )
                .rich_tip(crate::ui::tips::keyed(t.key()))
                .clicked()
                && !on
            {
                st.tool = t;
                st.worked = None;
                st.preset_edit = false;
                st.preset_name.clear();
                app.settings.tracecut.tool = t;
            }
        }
    }
}

/// The chosen tool: presets, settings, preview, status and buttons.
fn tool_pane(app: &mut App, ui: &mut Ui, st: &mut TraceCutState, width: f32) -> Option<Button> {
    ui.label(egui::RichText::new(tr(st.tool.key())).size(15.0).strong());
    ui.label(egui::RichText::new(tr(&format!("{}_desc", st.tool.key()))).color(Tokens::text_dim()));
    ui.add_space(4.0);
    preset_row(ui, st);
    ui.add_space(4.0);
    let before = st.settings.clone();
    // A long list of settings scrolls; the preview keeps its room.
    let max_h = (ui.available_height() * 0.5).max(120.0);
    let action = egui::ScrollArea::vertical()
        .id_salt(("tracecut_settings", st.tool.id()))
        .max_height(max_h)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            let a = crate::ui::tracecut_cut::settings(ui, app.units, st);
            crate::ui::tracecut_nest::settings(ui, app.units, st);
            settings(ui, app.units, st);
            a
        })
        .inner;
    if action == Some(crate::ui::tracecut_cut::Action::TestCut) {
        let plot = app.tracecut_test_job(&st.settings.plotter);
        let s = st.settings.plotter.clone();
        app.tracecut_output(&s, &plot, Some(ui.ctx().clone()));
    }
    if st.settings != before {
        // A setting changed: the result shown is not this one's.
        st.worked = None;
    }
    let current = st
        .worked
        .as_ref()
        .filter(|w| app.tracecut_current(w, st.tool, &st.settings));
    let scene = app.tracecut_scene(current);
    // A calculation's result, to read.
    if let Some(crate::tracecut::Outcome::Report(r)) = current.map(|w| &w.outcome) {
        egui::Frame::new()
            .fill(Tokens::field())
            .stroke(Stroke::new(1.0, Tokens::border()))
            .inner_margin(egui::Margin::same(6))
            .show(ui, |ui| {
                ui.set_width(width - 18.0);
                for (i, line) in r.lines.iter().enumerate() {
                    let text = egui::RichText::new(line);
                    ui.label(if i == 0 {
                        text.strong().size(14.0)
                    } else {
                        text
                    });
                }
            });
    }
    let h = (ui.available_height() - BELOW_PREVIEW).max(120.0);
    preview(ui, Vec2::new(width - 4.0, h), &scene);
    let has_targets = !scene.objects.is_empty() || !app.selection.is_empty();
    let line = match (current, &st.worked) {
        _ if !has_targets => tr("tracecut.select_first"),
        (Some(w), _) => w.outcome.summary(),
        (None, Some(_)) => tr("tracecut.out_of_date"),
        (None, None) => tr("tracecut.press_apply"),
    };
    status_strip(ui, width - 4.0, &line);
    ui.add_space(8.0);
    buttons(ui, has_targets)
}

/// The preset list, with a button that shows the row to save, delete or
/// reset them.
fn preset_row(ui: &mut Ui, st: &mut TraceCutState) {
    let tool = st.tool;
    let (names, current) = {
        let p = st.presets.tool(tool);
        (
            p.items.keys().cloned().collect::<Vec<String>>(),
            p.current.clone(),
        )
    };
    let mut picked: Option<String> = None;
    ui.horizontal(|ui| {
        ui.add_sized(
            [LABEL_W, 22.0],
            egui::Label::new(tr("tracecut.preset")).truncate(),
        );
        let shown = current
            .clone()
            .unwrap_or_else(|| tr("tracecut.preset_none"));
        crate::ui::keynav::ComboBox::from_id_salt(("tracecut_presets", tool.id()))
            .selected_text(shown)
            .width(200.0)
            .show_ui(ui, |ui| {
                for n in &names {
                    if ui
                        .selectable_label(current.as_ref() == Some(n), n)
                        .clicked()
                    {
                        picked = Some(n.clone());
                    }
                }
            })
            .response
            .rich_tip(crate::ui::tips::keyed("tracecut.preset"));
        if ui
            .add(egui::Button::selectable(st.preset_edit, "\u{2699}"))
            .rich_tip(crate::ui::tips::keyed("tracecut.preset_manage"))
            .clicked()
        {
            st.preset_edit = !st.preset_edit;
            if st.preset_name.is_empty() {
                st.preset_name = current.clone().unwrap_or_default();
            }
        }
    });
    if let Some(n) = picked {
        st.presets.apply_preset(tool, &n, &mut st.settings);
        st.preset_name = n;
        st.worked = None;
    }
    if !st.preset_edit {
        return;
    }
    ui.horizontal(|ui| {
        ui.add_space(LABEL_W + ui.spacing().item_spacing.x);
        ui.add_sized(
            [160.0, 22.0],
            egui::TextEdit::singleline(&mut st.preset_name).hint_text(tr("tracecut.preset_name")),
        );
        let named = !st.preset_name.trim().is_empty();
        if ui
            .add_enabled(named, egui::Button::new(tr("tracecut.preset_save")))
            .clicked()
        {
            let name = st.preset_name.clone();
            st.presets.save_preset(tool, &name, &st.settings);
        }
        if ui
            .add_enabled(named, egui::Button::new(tr("tracecut.preset_delete")))
            .clicked()
        {
            let name = st.preset_name.clone();
            st.presets.delete_preset(tool, &name);
        }
        if ui.button(tr("tracecut.preset_reset")).clicked() {
            st.settings.reset(tool);
            st.worked = None;
        }
    });
}

/// A settings row: the label on the left, `add` after it.
pub(crate) fn row<R>(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.horizontal(|ui| {
        if label.is_empty() {
            ui.add_space(LABEL_W);
        } else {
            ui.add_sized([LABEL_W, 22.0], egui::Label::new(label).truncate());
        }
        add(ui)
    })
    .inner
}

/// A length field in the document's units holding millimetres.
pub(crate) fn length(ui: &mut Ui, u: Units, mm: &mut f64, range: (f64, f64), tip: &str) {
    let mut v = u.world_from_mm(*mm);
    let r = ui
        .add_sized(
            [110.0, 22.0],
            crate::ui::field::NumField::new(&mut v)
                .range(u.world_from_mm(range.0)..=u.world_from_mm(range.1))
                .max_decimals(3)
                .suffix(format!(" {}", u.short()))
                .unit_mm(u.mm()),
        )
        .rich_tip(crate::ui::tips::keyed(tip));
    if r.changed() && v.is_finite() {
        *mm = u.world_to_mm(v).clamp(range.0, range.1);
    }
}

/// The chosen tool's settings.
fn settings(ui: &mut Ui, units: Units, st: &mut TraceCutState) {
    match st.tool {
        ToolId::FindDuplicates => {
            let s = &mut st.settings.duplicates;
            row(ui, &tr("tracecut.tolerance"), |ui| {
                length(
                    ui,
                    units,
                    &mut s.tolerance,
                    TOLERANCE_RANGE,
                    "tracecut.tolerance",
                )
            });
            row(ui, &tr("tracecut.on_ok"), |ui| {
                ui.radio_value(&mut s.delete, true, tr("tracecut.delete_duplicates"))
                    .rich_tip(crate::ui::tips::keyed("tracecut.delete_duplicates"));
                ui.radio_value(&mut s.delete, false, tr("tracecut.select_duplicates"))
                    .rich_tip(crate::ui::tips::keyed("tracecut.select_duplicates"));
            });
        }
        ToolId::FindCrossings => {
            let s = &mut st.settings.crossings;
            row(ui, &tr("tracecut.tolerance"), |ui| {
                length(
                    ui,
                    units,
                    &mut s.tolerance,
                    TOLERANCE_RANGE,
                    "tracecut.tolerance",
                )
            });
            row(ui, "", |ui| {
                ui.checkbox(&mut s.between, tr("tracecut.crossings_between"))
                    .rich_tip(crate::ui::tips::keyed("tracecut.crossings_between"));
            });
        }
        // The other Cut tools: ui/tracecut_cut.rs.
        _ => {}
    }
}

/// The preview: the scene fitted in a white box, y up.
fn preview(ui: &mut Ui, size: Vec2, scene: &Scene) {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter_at(rect);
    p.rect(
        rect,
        CornerRadius::ZERO,
        Color32::WHITE,
        Stroke::new(1.0, Tokens::border()),
        StrokeKind::Inside,
    );
    let area = rect.shrink(16.0);
    let Some(e) = scene.extent else {
        return;
    };
    let (ew, eh) = (e.width().max(1e-6), e.height().max(1e-6));
    let s = (f64::from(area.width()) / ew).min(f64::from(area.height()) / eh);
    if !(s.is_finite() && s > 0.0) {
        return;
    }
    let ox = f64::from(area.center().x) - (e.x0 + ew / 2.0) * s;
    let oy = f64::from(area.center().y) + (e.y0 + eh / 2.0) * s;
    let to = |q: Point| Pos2::new((ox + q.x * s) as f32, (oy - q.y * s) as f32);
    let tol = (0.5 / s).max(1e-4);
    let draw = |path: &tracedraw_core::geometry::BezPath, stroke: Stroke| {
        let mut pts: Vec<Pos2> = Vec::new();
        let mut start: Option<Pos2> = None;
        let flush = |pts: &mut Vec<Pos2>| {
            if pts.len() > 1 {
                p.add(egui::Shape::line(std::mem::take(pts), stroke));
            } else {
                pts.clear();
            }
        };
        tracedraw_core::geometry::flatten(path, tol, &mut |el| match el {
            PathEl::MoveTo(q) => {
                flush(&mut pts);
                let a = to(q);
                start = Some(a);
                pts.push(a);
            }
            PathEl::LineTo(q) => pts.push(to(q)),
            PathEl::ClosePath => {
                if let Some(a) = start {
                    pts.push(a);
                }
            }
            _ => {}
        });
        flush(&mut pts);
    };
    for path in &scene.objects {
        draw(path, Stroke::new(1.0, OBJECT));
    }
    for path in &scene.marked {
        draw(path, Stroke::new(2.5, MARKED));
    }
    for q in &scene.points {
        p.circle_stroke(to(*q), 6.0, Stroke::new(2.0, MARKED));
    }
    for (a, b) in &scene.moves {
        p.line_segment([to(*a), to(*b)], Stroke::new(1.0, MOVE));
    }
}

/// The status line under the preview.
fn status_strip(ui: &mut Ui, width: f32, text: &str) {
    let (r, _) = ui.allocate_exact_size(Vec2::new(width, 22.0), Sense::hover());
    ui.painter().rect_filled(r, 0.0, STRIP);
    ui.painter().text(
        Pos2::new(r.left() + 6.0, r.center().y),
        Align2::LEFT_CENTER,
        text,
        FontId::proportional(12.0),
        Color32::from_gray(30),
    );
}

/// Apply, OK and Cancel at the bottom right.
fn buttons(ui: &mut Ui, enabled: bool) -> Option<Button> {
    let labels = [tr("tracecut.apply"), tr("dialog.ok"), tr("dialog.cancel")];
    let mut pressed = None;
    ui.horizontal(|ui| {
        let w = 100.0;
        let room = ui.available_width() - 3.0 * w - 2.0 * ui.spacing().item_spacing.x;
        ui.add_space(room.max(0.0));
        for (i, l) in labels.iter().enumerate() {
            let on = enabled || i == 2;
            let tip = match i {
                0 => crate::ui::tips::keyed("tracecut.apply"),
                1 => crate::ui::tips::pair("dialog.ok", "tracecut.ok_desc"),
                _ => crate::ui::tips::pair("dialog.cancel", "tracecut.cancel_desc"),
            };
            if ui
                .add_enabled(on, egui::Button::new(l).min_size(Vec2::new(w, 27.0)))
                .rich_tip(tip)
                .clicked()
            {
                pressed = Some(match i {
                    0 => Button::Apply,
                    1 => Button::Ok,
                    _ => Button::Cancel,
                });
            }
        }
    });
    pressed
}

/// Whether TraceCut is listed in Launch and in the Tools menu.
pub fn shown(app: &App) -> bool {
    app.settings.tracecut.show
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tracecut::Outcome;
    use crate::ui::dialogs::Dialog;
    use tracedraw_core::ShapeKind;

    fn frame(ctx: &Context, app: &mut App, enter: bool) {
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                Vec2::new(1400.0, 900.0),
            )),
            ..Default::default()
        };
        if enter {
            input.events.push(egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            });
        }
        let mut out = ctx.run_ui(input, |ui| crate::ui::dialogs::show(app, &ui.ctx().clone()));
        out.textures_delta.clear();
    }

    fn two_squares(app: &mut App) -> (tracedraw_core::ShapeId, tracedraw_core::ShapeId) {
        let kind = ShapeKind::Rect {
            rect: tracedraw_core::geometry::Rect::new(10.0, 10.0, 40.0, 40.0),
            radius: 0.0,
            corners: None,
        };
        let a = app.new_shape(kind.clone()).expect("a");
        let b = app.new_shape(kind).expect("b");
        app.select(vec![a, b]);
        (a, b)
    }

    #[test]
    fn every_tool_draws_with_and_without_a_selection() {
        let ctx = crate::theme::ui_context();
        for selected in [false, true] {
            for t in ToolId::ALL {
                let mut app = App::headless();
                two_squares(&mut app);
                if !selected {
                    app.select(Vec::new());
                }
                let mut st = TraceCutState::new(t);
                st.preset_edit = true;
                app.dialog = Dialog::TraceCut(Box::new(st));
                frame(&ctx, &mut app, false);
                frame(&ctx, &mut app, false);
                assert!(
                    matches!(&app.dialog, Dialog::TraceCut(s) if s.tool == t),
                    "{t:?} stays open"
                );
            }
        }
    }

    #[test]
    fn enter_is_ok_and_ok_is_one_undo_step() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        let (a, b) = two_squares(&mut app);
        app.settings.tracecut.tool = ToolId::FindDuplicates;
        app.open_tracecut();
        frame(&ctx, &mut app, false);
        let before = app.engine.history_labels().0.len();
        frame(&ctx, &mut app, true);
        frame(&ctx, &mut app, false);
        assert!(matches!(app.dialog, Dialog::None));
        assert!(app.doc().find_shape(a).is_some());
        assert!(app.doc().find_shape(b).is_none());
        assert_eq!(app.engine.history_labels().0.len(), before + 1);
    }

    #[test]
    fn a_window_without_a_selection_changes_nothing_on_ok() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        two_squares(&mut app);
        app.select(Vec::new());
        app.open_tracecut();
        let rev = app.engine.revision();
        frame(&ctx, &mut app, false);
        frame(&ctx, &mut app, true);
        assert_eq!(app.engine.revision(), rev);
    }

    #[test]
    fn the_window_keeps_its_size_from_frame_to_frame() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        two_squares(&mut app);
        let mut st = TraceCutState::new(ToolId::FindDuplicates);
        st.worked = Some(app.tracecut_work_out(st.tool, &st.settings));
        assert!(matches!(
            st.worked.as_ref().map(|w| &w.outcome),
            Some(Outcome::Duplicates(d)) if d.len() == 1
        ));
        app.dialog = Dialog::TraceCut(Box::new(st));
        let mut sizes = Vec::new();
        for _ in 0..6 {
            frame(&ctx, &mut app, false);
            let id = egui::Id::new(("chrome_dialog", tr("tracecut.title")));
            if let Some(r) = ctx.memory(|m| m.area_rect(id)) {
                sizes.push(r.size());
            }
        }
        assert!(sizes.len() >= 2, "the window was drawn");
        let last = sizes[sizes.len() - 1];
        let before = sizes[sizes.len() - 2];
        assert!((last - before).length() < 0.5, "{sizes:?}");
    }

    /// Run `f` inside a frame, where the menus can be walked.
    fn in_frame(ctx: &Context, app: &mut App, f: impl FnOnce(&mut App, &Context)) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                Vec2::new(1400.0, 900.0),
            )),
            ..Default::default()
        };
        let mut f = Some(f);
        let mut out = ctx.run_ui(input, |ui| {
            if let Some(f) = f.take() {
                f(app, &ui.ctx().clone());
            }
        });
        out.textures_delta.clear();
    }

    #[test]
    fn tools_tracecut_opens_the_window_and_options_can_hide_it() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        let listed = |app: &mut App| {
            let mut found = false;
            in_frame(&ctx, app, |app, c| {
                found = crate::ui::menus::commands(app, c)
                    .iter()
                    .any(|e| e.key == "menu.tools.tracecut");
            });
            found
        };
        assert!(listed(&mut app));
        let mut ran = false;
        in_frame(&ctx, &mut app, |app, c| {
            ran = crate::ui::menus::run_command(app, c, "menu.tools.tracecut");
        });
        assert!(ran);
        assert!(matches!(app.dialog, Dialog::TraceCut(_)));
        app.dialog = Dialog::None;
        app.settings.tracecut.show = false;
        assert!(!listed(&mut app));
    }
}
