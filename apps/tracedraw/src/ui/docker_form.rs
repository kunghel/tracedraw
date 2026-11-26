//! The form layout of the effect dockers: sections with a bold title and
//! a line above, rows with the label right-aligned in a column of its own
//! and the field after it, and a full-width button at the end.

use egui::{Align, Layout, RichText, Ui, Vec2};

/// Width of the label column.
pub const LABEL_W: f32 = 104.0;
/// Height of a form row.
pub const ROW_H: f32 = 24.0;

/// A line across the docker, then the section's bold title (when it has
/// one).
pub fn section(ui: &mut Ui, title: Option<&str>) {
    ui.add_space(4.0);
    ui.separator();
    if let Some(t) = title {
        ui.add_space(2.0);
        ui.label(RichText::new(t).strong());
    }
    ui.add_space(2.0);
}

/// The label column's width in `ui`: [`LABEL_W`], or less in a narrow
/// panel, so the field beside it keeps room (#4323).
pub fn label_w(ui: &Ui) -> f32 {
    LABEL_W.min((ui.available_width() * 0.45).max(40.0))
}

/// Notes the right edge of the panel page that `ui` begins, for
/// [`fit_w`]. Called at the top of every panel page.
pub fn begin_page(ui: &Ui) {
    let right = ui.max_rect().right();
    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("docker_page_right"), right));
}

/// Forgets the page edge noted by [`begin_page`].
pub fn end_page(ui: &Ui) {
    ui.ctx().data_mut(|d| d.remove::<f32>(egui::Id::new("docker_page_right")));
}

/// The room from the cursor to the panel's right edge, no wider than
/// `max`: a preview or a field sized from it stays inside a narrow panel
/// even after a wide row has widened the page (#4323).
pub fn fit_w(ui: &Ui, max: f32) -> f32 {
    let mut w = ui.available_width();
    let right: Option<f32> = ui.ctx().data(|d| d.get_temp(egui::Id::new("docker_page_right")));
    if let Some(r) = right {
        w = w.min(r - ui.cursor().left());
    }
    w.min(max).max(1.0)
}

/// Sets the slider width of a form page from the room beside the label
/// column (at most 110 points), so a slider row fits a narrow panel, and
/// keeps the page as wide as the panel (#4323).
pub fn fit_sliders(ui: &mut Ui) {
    ui.set_max_width(ui.available_width());
    let room = fit_w(ui, f32::INFINITY) - label_w(ui) - 90.0;
    ui.spacing_mut().slider_width = room.clamp(40.0, 110.0);
}

/// A row: `label` right-aligned in the label column, then `add`. A name
/// longer than the column wraps onto a second line inside it rather than
/// running out past the panel's left edge, cut off (#4323).
pub fn row<R>(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    let w = label_w(ui);
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            Vec2::new(w, ROW_H),
            Layout::right_to_left(Align::Center),
            |ui| {
                ui.set_min_width(w);
                ui.set_max_width(w);
                ui.add(egui::Label::new(label).wrap());
            },
        );
        add(ui)
    })
    .inner
}

/// A row whose label column is empty (a check box under the fields).
pub fn indented<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let w = label_w(ui);
    ui.horizontal(|ui| {
        ui.add_space(w + ui.spacing().item_spacing.x);
        add(ui)
    })
    .inner
}

/// The docker's full-width button (Apply) with its tip; true when clicked.
pub fn wide_button(ui: &mut Ui, key: &str, enabled: bool, tip: &str) -> bool {
    use crate::ui::tips::RichTip;
    ui.add_space(6.0);
    let w = fit_w(ui, f32::INFINITY);
    ui.add_enabled(
        enabled,
        crate::ui::btn_icons::Ib::new(key).min_size(Vec2::new(w, 26.0)),
    )
    .rich_tip(tip)
    .rich_disabled_tip(tip)
    .clicked()
}

/// The Reset to Defaults button of a generator, under its Apply button;
/// true when clicked.
pub fn reset_button(ui: &mut Ui) -> bool {
    use crate::ui::tips::RichTip;
    ui.add(crate::ui::btn_icons::Ib::new("docker.reset_defaults"))
        .rich_tip(crate::ui::tips::keyed("docker.reset_defaults"))
        .clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A name longer than the label column, in a panel of any width,
    /// stays inside the panel: it wraps in the column (#4323).
    #[test]
    fn a_long_label_stays_inside_a_narrow_panel() {
        let ctx = crate::theme::ui_context();
        for width in [160.0, 240.0, 320.0] {
            let rect = egui::Rect::from_min_size(egui::pos2(300.0, 40.0), egui::vec2(width, 400.0));
            let mut out = ctx.run_ui(Default::default(), |ui| {
                let mut page = ui.new_child(egui::UiBuilder::new().max_rect(rect));
                page.set_clip_rect(rect);
                row(&mut page, "Passos de relaxamento da grade", |ui| {
                    ui.add(egui::DragValue::new(&mut 3.0_f64));
                });
                indented(&mut page, |ui| ui.label("Combinar em uma curva"));
            });
            out.textures_delta.clear();
            let mut texts = 0;
            for c in &out.shapes {
                if let egui::Shape::Text(t) = &c.shape {
                    texts += 1;
                    let r = t.visual_bounding_rect();
                    assert!(
                        r.left() >= rect.left() - 0.5,
                        "{width}: {:?} starts at {} left of {}",
                        t.galley.text(),
                        r.left(),
                        rect.left()
                    );
                }
            }
            assert!(texts >= 3, "{width}: {texts} texts drawn");
        }
    }
}
