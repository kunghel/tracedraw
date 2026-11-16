//! Minimize, Maximize and Close at the right end of the menu bar (#2574),
//! for a window without the desktop's title bar (a tiling window manager,
//! a desktop set to draw none). Options > General turns them on. With
//! them on, the empty part of the menu bar moves the window when dragged
//! and maximizes or restores it on a double click, as a title bar does.
//! Close asks about unsaved drawings as File > Exit does.

use crate::app::App;
use crate::ui::tips::RichTip as _;
use egui::{Color32, Rect, Sense, Stroke, Ui, Vec2};

/// One button's width (points).
const BUTTON_W: f32 = 40.0;

/// The width the three buttons take at the bar's right end.
pub const WIDTH: f32 = 3.0 * BUTTON_W;

/// Whether the buttons show: turned on, on a desktop window.
pub fn shown(app: &App) -> bool {
    cfg!(not(any(target_arch = "wasm32", target_os = "android"))) && app.settings.window_buttons
}

/// What a button does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Minimize,
    Maximize,
    Close,
}

impl Button {
    const ALL: [Button; 3] = [Button::Minimize, Button::Maximize, Button::Close];

    fn key(self, maximized: bool) -> &'static str {
        match self {
            Button::Minimize => "window.minimize",
            Button::Maximize if maximized => "window.restore",
            Button::Maximize => "window.maximize",
            Button::Close => "window.close",
        }
    }
}

/// The viewport command a button sends (`None`: Close, which goes
/// through the app's own exit, asking about unsaved drawings).
pub fn command(button: Button, maximized: bool) -> Option<egui::ViewportCommand> {
    match button {
        Button::Minimize => Some(egui::ViewportCommand::Minimized(true)),
        Button::Maximize => Some(egui::ViewportCommand::Maximized(!maximized)),
        Button::Close => None,
    }
}

fn press(app: &mut App, ctx: &egui::Context, button: Button, maximized: bool) {
    match command(button, maximized) {
        Some(cmd) => ctx.send_viewport_cmd(cmd),
        None => app.request_exit(),
    }
}

/// The three buttons, filling `ui` (the strip at the bar's right end).
pub fn show(app: &mut App, ui: &mut Ui) {
    let maximized = ui.ctx().input(|i| i.viewport().maximized).unwrap_or(false);
    let h = ui.max_rect().height();
    let mut pressed = None;
    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for b in Button::ALL {
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(BUTTON_W, h), Sense::click());
            let resp = resp.rich_tip(crate::ui::tips::keyed(b.key(maximized)));
            let ink = if resp.hovered() && b == Button::Close {
                Color32::WHITE
            } else {
                crate::theme::Tokens::text()
            };
            if resp.hovered() {
                let fill = if b == Button::Close {
                    Color32::from_rgb(196, 43, 28)
                } else {
                    ui.visuals().widgets.hovered.weak_bg_fill
                };
                ui.painter().rect_filled(rect, 0.0, fill);
            }
            paint(ui, rect, b, maximized, ink);
            if resp.clicked() {
                pressed = Some(b);
            }
        }
    });
    if let Some(b) = pressed {
        let ctx = ui.ctx().clone();
        press(app, &ctx, b, maximized);
    }
}

/// A button's mark, 10 points wide, in the middle of `rect`.
fn paint(ui: &Ui, rect: Rect, b: Button, maximized: bool, ink: Color32) {
    let p = ui.painter();
    let c = rect.center();
    let s = 5.0;
    let stroke = Stroke::new(1.0, ink);
    match b {
        Button::Minimize => {
            p.line_segment([c + Vec2::new(-s, 0.0), c + Vec2::new(s, 0.0)], stroke);
        }
        Button::Maximize if maximized => {
            // Two boxes, one behind the other.
            let front =
                Rect::from_center_size(c + Vec2::new(-1.0, 1.0), Vec2::splat(2.0 * s - 2.0));
            p.rect_stroke(front, 0.0, stroke, egui::StrokeKind::Middle);
            let back = front.translate(Vec2::new(2.0, -2.0));
            p.line_segment([back.left_top(), back.right_top()], stroke);
            p.line_segment([back.right_top(), back.right_bottom()], stroke);
        }
        Button::Maximize => {
            let r = Rect::from_center_size(c, Vec2::splat(2.0 * s));
            p.rect_stroke(r, 0.0, stroke, egui::StrokeKind::Middle);
        }
        Button::Close => {
            p.line_segment([c + Vec2::new(-s, -s), c + Vec2::new(s, s)], stroke);
            p.line_segment([c + Vec2::new(-s, s), c + Vec2::new(s, -s)], stroke);
        }
    }
}

/// The empty part of the menu bar, `rect`, as a title bar: a drag moves
/// the window, a double click maximizes or restores it.
pub fn title_area(ui: &mut Ui, rect: Rect) {
    if rect.width() <= 0.0 {
        return;
    }
    let resp = ui.interact(
        rect,
        ui.id().with("window_title_area"),
        Sense::click_and_drag(),
    );
    let ctx = ui.ctx().clone();
    if resp.double_clicked() {
        let maximized = ctx.input(|i| i.viewport().maximized).unwrap_or(false);
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
    } else if resp.drag_started() {
        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_button_sends_its_command() {
        use egui::ViewportCommand as V;
        assert_eq!(command(Button::Minimize, false), Some(V::Minimized(true)));
        assert_eq!(command(Button::Maximize, false), Some(V::Maximized(true)));
        assert_eq!(command(Button::Maximize, true), Some(V::Maximized(false)));
        assert_eq!(command(Button::Close, false), None);
        assert_eq!(Button::Maximize.key(true), "window.restore");
    }

    #[test]
    fn close_exits_like_file_exit() {
        let mut app = App::headless();
        let ctx = egui::Context::default();
        press(&mut app, &ctx, Button::Close, false);
        assert!(app.quit_now);
    }

    #[test]
    fn the_buttons_follow_the_option() {
        let mut app = App::headless();
        assert!(!shown(&app), "off by default");
        app.settings.window_buttons = true;
        assert!(shown(&app));
    }
}
