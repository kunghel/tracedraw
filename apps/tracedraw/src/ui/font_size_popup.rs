//! Text > Font Size Box (#4285): a small box beside the selected text
//! with its font size ready to type over, opened from the menu or its
//! shortcut (Ctrl+Alt+Z, which Options > Customization > Commands can
//! change). The text shows each size as it is typed or pointed at in the
//! list, as a preview; Enter or a size picked from the list applies it
//! (one undo step), Escape or a click elsewhere leaves the text as it
//! was.

use crate::app::App;
use crate::i18n::tr;
use crate::theme::Tokens;
use egui::{Context, Id, Stroke};

/// The box's state between frames: the text typed, and whether the field
/// still has to take the keys (the first frame after opening).
#[derive(Debug, Clone, PartialEq)]
struct BoxState {
    text: String,
    fresh: bool,
}

fn state_id() -> Id {
    Id::new("font_size_popup")
}

/// The preview owner of the box (see [`App::preview_text`]).
const OWNER: &str = "font_size_popup";

/// Whether the box is open.
pub fn is_open(ctx: &Context) -> bool {
    ctx.data(|d| d.get_temp::<BoxState>(state_id()).is_some())
}

/// Open the box for the selected or edited text, its size shown in the
/// text units; nothing happens without text to change.
pub fn open(app: &App, ctx: &Context) {
    if !app.has_text_target() {
        return;
    }
    let u = app.text_units();
    let size = app.text_settings_shown().3;
    let text = crate::ui::field::format_number(u.from_pt(size), false, 0, Some(2));
    ctx.data_mut(|d| d.insert_temp(state_id(), BoxState { text, fresh: true }));
}

fn close(app: &mut App, ctx: &Context) {
    ctx.data_mut(|d| d.remove::<BoxState>(state_id()));
    if app.live_font.as_ref().is_some_and(|l| l.owner == OWNER) {
        app.end_font_preview();
    }
}

/// The size typed, in points, when it is one (1 to 3000 pt).
pub fn typed_pt(app: &App, text: &str) -> Option<f64> {
    let v: f64 = text.trim().replace(',', ".").parse().ok()?;
    // To a thousandth of a point: 24 typed is 24, not 23.999...
    let pt = (app.text_units().to_pt(v) * 1000.0).round() / 1000.0;
    (pt.is_finite() && (1.0..=3000.0).contains(&pt)).then_some(pt)
}

/// Give the selected text the size `pt` for good: one undo step.
pub fn apply(app: &mut App, pt: f64) {
    if app.live_font.as_ref().is_some_and(|l| l.owner == OWNER) {
        app.end_font_preview();
    }
    app.text_size_pt = pt;
    if let Some(t) = app.text_shown.as_mut() {
        t.size_pt = None;
    }
    app.apply_text_style();
}

/// Draw the box while it is open, beside the selection's top right
/// corner on the canvas.
pub fn show(app: &mut App, ctx: &Context) {
    let Some(mut st) = ctx.data(|d| d.get_temp::<BoxState>(state_id())) else {
        return;
    };
    let Some(bounds) = app.selection_bounds().filter(|_| app.has_text_target()) else {
        close(app, ctx);
        return;
    };
    let r = app.view.rect_to_screen(bounds);
    let canvas = app.canvas_rect;
    let mut at = egui::pos2(r.right() + 8.0, r.top() - 4.0);
    at.x = at.x.clamp(canvas.left() + 4.0, (canvas.right() - 190.0).max(canvas.left() + 4.0));
    at.y = at.y.clamp(canvas.top() + 4.0, (canvas.bottom() - 40.0).max(canvas.top() + 4.0));
    let u = app.text_units();
    let mut done: Option<Option<f64>> = None;
    let mut hovered: Option<f64> = None;
    let area = egui::Area::new(Id::new("font_size_popup_area"))
        .fixed_pos(at)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(Tokens::panel())
                .stroke(Stroke::new(1.0, Tokens::border()))
                .inner_margin(egui::Margin::symmetric(8, 6))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(tr("toolbar.font_size"));
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut st.text).desired_width(56.0),
                        );
                        if st.fresh {
                            resp.request_focus();
                        }
                        crate::ui::field::select_all_when_focused(ui, &resp, st.fresh, &st.text);
                        if resp.has_focus() {
                            st.fresh = false;
                        }
                        ui.label(u.short());
                        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if (resp.lost_focus() && enter) || (resp.has_focus() && enter) {
                            done = Some(typed_pt(app, &st.text));
                        }
                        // The usual sizes, pointed at for a preview.
                        egui::ComboBox::from_id_salt("font_size_popup_list")
                            .selected_text("")
                            .width(24.0)
                            .show_ui(ui, |ui| {
                                for s in crate::ui::toolbar::TEXT_SIZES {
                                    let label = crate::ui::field::format_number(
                                        u.from_pt(s),
                                        false,
                                        0,
                                        Some(2),
                                    );
                                    let r = ui.selectable_label(false, label);
                                    if r.hovered() {
                                        hovered = Some(s);
                                    }
                                    if r.clicked() {
                                        done = Some(Some(s));
                                    }
                                }
                            });
                    });
                });
        });
    if ui_escape(ctx) {
        close(app, ctx);
        return;
    }
    if let Some(choice) = done {
        if let Some(pt) = choice {
            apply(app, pt);
        }
        close(app, ctx);
        return;
    }
    // A click elsewhere closes it; a click inside its list is its own.
    let clicked_out = ctx.input(|i| i.pointer.any_pressed())
        && ctx
            .pointer_interact_pos()
            .is_some_and(|p| !area.response.rect.contains(p))
        && !egui::Popup::is_any_open(ctx);
    if clicked_out && !st.fresh {
        close(app, ctx);
        return;
    }
    // The text shows the size typed, or the one pointed at in the list.
    let want = hovered.or_else(|| typed_pt(app, &st.text));
    app.preview_text(OWNER, want.map(crate::ui::font_list::LiveText::Size));
    ctx.data_mut(|d| d.insert_temp(state_id(), st));
}

fn ui_escape(ctx: &Context) -> bool {
    ctx.input(|i| i.key_pressed(egui::Key::Escape))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::{ShapeKind, TextSpan};

    fn app_with_text() -> (App, tracedraw_core::ShapeId) {
        let mut app = App::headless();
        let id = app
            .new_shape(ShapeKind::Text {
                spans: vec![TextSpan::new("Hello", "Arial", 12.0)],
                origin: tracedraw_core::geometry::Point::new(10.0, 80.0),
                frame: None,
                align: Default::default(),
                para: Default::default(),
                on_path: None,
            })
            .expect("text");
        app.select(vec![id]);
        (app, id)
    }

    fn size_of(app: &App, id: tracedraw_core::ShapeId) -> f64 {
        match &app.doc().find_shape(id).expect("text").kind {
            ShapeKind::Text { spans, .. } => spans[0].size_pt,
            _ => panic!("text"),
        }
    }

    /// The box opens only with text to change, reads a typed size in the
    /// text units (a comma too) and gives it to the text as one undo
    /// step; nonsense and sizes out of range are no size (#4285).
    #[test]
    fn the_font_size_box_applies_a_typed_size() {
        let ctx = crate::theme::ui_context();
        let mut empty = App::headless();
        empty.select(Vec::new());
        open(&empty, &ctx);
        assert!(!is_open(&ctx), "no text, no box");
        let (mut app, id) = app_with_text();
        app.settings.text_units = "pt".into();
        open(&app, &ctx);
        assert!(is_open(&ctx));
        assert_eq!(typed_pt(&app, "24"), Some(24.0));
        assert_eq!(typed_pt(&app, "10,5"), Some(10.5));
        assert_eq!(typed_pt(&app, "abc"), None);
        assert_eq!(typed_pt(&app, "0"), None);
        apply(&mut app, 36.0);
        assert!((size_of(&app, id) - 36.0).abs() < 1e-6);
        app.undo();
        assert!((size_of(&app, id) - 12.0).abs() < 1e-6);
        close(&mut app, &ctx);
        assert!(!is_open(&ctx));
    }

    /// The menu command opens the box: it is a Text menu command with
    /// Ctrl+Alt+Z, so its keys can be changed like any other.
    #[test]
    fn the_box_is_a_text_menu_command_with_its_keys() {
        let ctx = crate::theme::ui_context();
        let (mut app, _) = app_with_text();
        let mut listed = false;
        let mut ran = false;
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            let cmds = crate::ui::menus::commands(&mut app, ui.ctx());
            listed = cmds
                .iter()
                .any(|c| c.key == "menu.text.font_size_box" && c.shortcut == "Ctrl+Alt+Z");
            ran = crate::ui::menus::run_command(&mut app, ui.ctx(), "menu.text.font_size_box");
        });
        out.textures_delta.clear();
        assert!(listed, "listed with its keys");
        assert!(ran);
        assert!(is_open(&ctx));
    }
}
