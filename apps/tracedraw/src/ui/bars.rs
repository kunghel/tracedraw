//! Movable bars. The toolbox, the colour palette, the standard toolbar,
//! the property bar and the status bar dock on an edge of the window or
//! float over it (the status bar docks only). A docked bar starts with a
//! grip of dots; dragging the grip, or the title of a floating bar, shows
//! where the bar will go: near an edge it docks there, elsewhere it floats
//! where it is dropped. A right click on a grip or on the empty part of a
//! bar opens a menu with the bar's positions, the bars to show and Lock
//! Toolbars, which keeps every bar where it is. Double-clicking a floating
//! bar's title docks it back. The layout is kept in the preferences.

use crate::app::App;
use crate::i18n::tr;
use crate::theme::Tokens;
use egui::{pos2, vec2, Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use serde::{Deserialize, Serialize};

/// Where a bar sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Dock {
    #[default]
    Top,
    Bottom,
    Left,
    Right,
    Floating,
}

impl Dock {
    pub fn key(self) -> &'static str {
        match self {
            Dock::Top => "bars.top",
            Dock::Bottom => "bars.bottom",
            Dock::Left => "bars.left",
            Dock::Right => "bars.right",
            Dock::Floating => "bars.floating",
        }
    }

    /// True for the docks a bar lies along horizontally in.
    pub fn horizontal(self) -> bool {
        matches!(self, Dock::Top | Dock::Bottom)
    }
}

/// The bars that move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Bar {
    Toolbox,
    Palette,
    Standard,
    Property,
    Status,
}

impl Bar {
    pub const ALL: [Bar; 5] = [
        Bar::Standard,
        Bar::Property,
        Bar::Toolbox,
        Bar::Palette,
        Bar::Status,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Bar::Toolbox => "menu.window.toolbar_toolbox",
            Bar::Palette => "bars.palette",
            Bar::Standard => "menu.window.toolbar_standard",
            Bar::Property => "menu.window.toolbar_property_bar",
            Bar::Status => "menu.window.toolbar_status_bar",
        }
    }

    /// Where the bar can go.
    pub fn docks(self) -> &'static [Dock] {
        match self {
            Bar::Toolbox | Bar::Palette => &[
                Dock::Top,
                Dock::Bottom,
                Dock::Left,
                Dock::Right,
                Dock::Floating,
            ],
            Bar::Standard | Bar::Property => &[Dock::Top, Dock::Bottom, Dock::Floating],
            Bar::Status => &[Dock::Top, Dock::Bottom],
        }
    }

    pub fn default_dock(self) -> Dock {
        match self {
            Bar::Toolbox => Dock::Left,
            Bar::Palette => Dock::Bottom,
            Bar::Status => Dock::Bottom,
            Bar::Standard | Bar::Property => Dock::Top,
        }
    }

    pub fn visible(self, app: &App) -> bool {
        match self {
            Bar::Toolbox => app.show_toolbox,
            Bar::Palette => app.settings.palette.show_palette,
            Bar::Standard => app.show_standard_toolbar,
            Bar::Property => app.show_property_bar,
            Bar::Status => app.show_status_bar,
        }
    }

    pub fn set_visible(self, app: &mut App, on: bool) {
        match self {
            Bar::Toolbox => app.show_toolbox = on,
            Bar::Palette => app.settings.palette.show_palette = on,
            Bar::Standard => app.show_standard_toolbar = on,
            Bar::Property => app.show_property_bar = on,
            Bar::Status => app.show_status_bar = on,
        }
    }

    /// The size of the bar floating (points), title strip included.
    pub fn floating_size(self, app: &App) -> Vec2 {
        match self {
            Bar::Toolbox => vec2(crate::ui::toolbox::floating_width(app) + 8.0, 400.0),
            Bar::Palette => vec2(460.0, crate::ui::palette::ROW_H + TITLE_H + 4.0),
            Bar::Standard => vec2(940.0, 36.0 + TITLE_H + 4.0),
            Bar::Property => vec2(1100.0, 53.0 + TITLE_H + 4.0),
            Bar::Status => vec2(900.0, 40.0),
        }
    }
}

/// Height of a floating bar's title strip.
pub const TITLE_H: f32 = 18.0;
/// How near an edge a dropped bar docks there.
pub const EDGE_ZONE: f32 = 56.0;

/// Where each bar is, and whether bars are locked.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BarLayout {
    pub toolbox: Dock,
    pub palette: Dock,
    pub standard: Dock,
    pub property: Dock,
    pub status: Dock,
    /// Top-left corners of the floating bars (screen points).
    pub toolbox_pos: [f32; 2],
    pub palette_pos: [f32; 2],
    pub standard_pos: [f32; 2],
    pub property_pos: [f32; 2],
    pub locked: bool,
    /// The layout's version: preferences written at 1 had the palette at
    /// the right edge by default (see [`BarLayout::migrate`]).
    #[serde(default)]
    pub version: u32,
}

/// The current layout version.
pub const LAYOUT_VERSION: u32 = 2;

impl Default for BarLayout {
    fn default() -> Self {
        BarLayout {
            toolbox: Dock::Left,
            palette: Dock::Bottom,
            standard: Dock::Top,
            property: Dock::Top,
            status: Dock::Bottom,
            toolbox_pos: [120.0, 160.0],
            palette_pos: [300.0, 600.0],
            standard_pos: [200.0, 140.0],
            property_pos: [200.0, 200.0],
            locked: false,
            version: LAYOUT_VERSION,
        }
    }
}

impl BarLayout {
    /// Bring a layout of older preferences up to date. Version 1 stood the
    /// palette at the right edge by default (and moved it there from the
    /// bottom); its first place is the bottom again, so a version 1
    /// palette at the right goes back there. Earlier layouts, and a
    /// palette put anywhere else, stay as they were; whatever the user
    /// chooses from now on is kept.
    pub fn migrate(&mut self) {
        if self.version == 1 && self.palette == Dock::Right {
            self.palette = Dock::Bottom;
        }
        self.version = self.version.max(LAYOUT_VERSION);
    }

    pub fn dock(&self, bar: Bar) -> Dock {
        let d = match bar {
            Bar::Toolbox => self.toolbox,
            Bar::Palette => self.palette,
            Bar::Standard => self.standard,
            Bar::Property => self.property,
            Bar::Status => self.status,
        };
        if bar.docks().contains(&d) {
            d
        } else {
            bar.default_dock()
        }
    }

    /// Put `bar` at `dock`; false (and nothing changes) where it cannot go.
    pub fn set_dock(&mut self, bar: Bar, dock: Dock) -> bool {
        if !bar.docks().contains(&dock) {
            return false;
        }
        match bar {
            Bar::Toolbox => self.toolbox = dock,
            Bar::Palette => self.palette = dock,
            Bar::Standard => self.standard = dock,
            Bar::Property => self.property = dock,
            Bar::Status => self.status = dock,
        }
        true
    }

    pub fn pos(&self, bar: Bar) -> Pos2 {
        let p = match bar {
            Bar::Toolbox => self.toolbox_pos,
            Bar::Palette => self.palette_pos,
            Bar::Standard => self.standard_pos,
            Bar::Property | Bar::Status => self.property_pos,
        };
        pos2(p[0], p[1])
    }

    pub fn set_pos(&mut self, bar: Bar, p: Pos2) {
        let v = [p.x, p.y];
        match bar {
            Bar::Toolbox => self.toolbox_pos = v,
            Bar::Palette => self.palette_pos = v,
            Bar::Standard => self.standard_pos = v,
            Bar::Property | Bar::Status => self.property_pos = v,
        }
    }
}

/// A bar being dragged to a new place: which one, and where the pointer
/// holds it from its top-left corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BarDrag {
    pub bar: Bar,
    pub grab: Vec2,
    /// The last pointer position seen: a release outside the window drops
    /// the bar there.
    pub last: Option<Pos2>,
}

/// Where `bar` goes when dropped with the pointer at `p` in a window of
/// `screen` (the menu bar's height below its top): the nearest edge
/// within [`EDGE_ZONE`] it can dock at, else floating, else (a bar that
/// cannot float) the nearest edge it can dock at.
pub fn target_dock(bar: Bar, screen: Rect, menu_h: f32, p: Pos2) -> Dock {
    let distances = [
        (Dock::Left, p.x - screen.left()),
        (Dock::Right, screen.right() - p.x),
        (Dock::Top, p.y - (screen.top() + menu_h)),
        (Dock::Bottom, screen.bottom() - p.y),
    ];
    let mut allowed: Vec<(Dock, f32)> = distances
        .iter()
        .copied()
        .filter(|(d, _)| bar.docks().contains(d))
        .collect();
    allowed.sort_by(|a, b| a.1.total_cmp(&b.1));
    match allowed.first() {
        Some(&(d, dist)) if dist <= EDGE_ZONE => d,
        _ if bar.docks().contains(&Dock::Floating) => Dock::Floating,
        Some(&(d, _)) => d,
        None => bar.default_dock(),
    }
}

/// Paint a grip of dots in `r`, along its longer side.
fn paint_grip(ui: &Ui, r: Rect) {
    let p = ui.painter();
    let color = crate::theme::Tokens::text_dim();
    let vertical = r.height() >= r.width();
    let n = ((if vertical { r.height() } else { r.width() } - 6.0) / 4.0)
        .floor()
        .clamp(1.0, 8.0) as i32;
    let c = r.center();
    for i in 0..n {
        let t = (i as f32 - (n - 1) as f32 / 2.0) * 4.0;
        let (a, b) = if vertical {
            (pos2(c.x - 1.5, c.y + t), pos2(c.x + 1.5, c.y + t + 2.0))
        } else {
            (pos2(c.x + t, c.y - 1.5), pos2(c.x + t + 2.0, c.y + 1.5))
        };
        p.rect_filled(Rect::from_center_size(a, vec2(1.5, 1.5)), 0.0, color);
        p.rect_filled(Rect::from_center_size(b, vec2(1.5, 1.5)), 0.0, color);
    }
}

/// The grip at the start of a docked bar: `horizontal` for a bar running
/// left to right (the grip is then a narrow column of dots).
pub fn grip(app: &mut App, ui: &mut Ui, bar: Bar, horizontal: bool) {
    let size = if horizontal {
        vec2(9.0, ui.available_height().max(16.0))
    } else {
        vec2(ui.available_width().max(16.0), 9.0)
    };
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    if !app.settings.bars.locked {
        paint_grip(ui, rect.shrink2(vec2(0.0, 3.0)));
    }
    grip_zone(app, ui, bar, rect, 0);
}

/// Make `r` (already painted as a grip) drag `bar` and open its menu;
/// `salt` tells apart several grips of one bar (the palette's rows).
pub fn grip_zone(app: &mut App, ui: &mut Ui, bar: Bar, r: Rect, salt: u8) {
    let id = egui::Id::new(("bar_grip", bar as u8, salt));
    let resp = ui.interact(r, id, Sense::click_and_drag());
    let resp = if app.settings.bars.locked {
        resp
    } else {
        resp.on_hover_cursor(egui::CursorIcon::Move)
    };
    if resp.drag_started() && !app.settings.bars.locked {
        // The floating bar comes up with its title under the pointer.
        let size = bar.floating_size(app);
        app.bar_drag = Some(BarDrag {
            bar,
            grab: vec2(12.0f32.min(size.x / 2.0), (TITLE_H / 2.0).min(size.y / 2.0)),
            last: resp.interact_pointer_pos(),
        });
    }
    bar_menu(app, &resp, bar);
}

/// A background response for the empty part of a bar: a right click there
/// opens the bar's menu. Call before drawing the bar's controls.
pub fn background(app: &mut App, ui: &mut Ui, bar: Bar) {
    let id = egui::Id::new(("bar_background", bar as u8));
    let resp = ui.interact(ui.max_rect(), id, Sense::click());
    bar_menu(app, &resp, bar);
}

/// The bar's context menu on `resp`. Each place that opens it (a grip, the
/// bar's background, a floating title) keeps its own popup, so one menu
/// never shows twice in a frame.
pub fn bar_menu(app: &mut App, resp: &egui::Response, bar: Bar) {
    egui::Popup::context_menu(resp)
        .id(resp.id.with("bar_menu"))
        .style(crate::ui::menus::menu_popup_style)
        .show(|ui| crate::ui::menus::body(ui, |ui| menu_body(app, ui, bar)));
}

fn menu_body(app: &mut App, ui: &mut Ui, bar: Bar) {
    use crate::ui::menus::{check, item, radio, radio_if, sep, sub};
    ui.set_min_width(200.0);
    let current = app.settings.bars.dock(bar);
    let free = !app.settings.bars.locked;
    for d in bar.docks() {
        if radio_if(ui, d.key(), "", current == *d, free) {
            move_bar(app, bar, *d, None);
        }
    }
    sep(ui);
    // Every bar, as Window > Toolbars lists them.
    crate::ui::menus::toolbars_list(app, ui);
    sep(ui);
    sub(ui, "bars.customize", |ui| {
        sub(ui, "bars.default_button_size", |ui| {
            for size in crate::settings::ButtonSize::ALL {
                if radio(ui, size.key(), "", app.settings.button_size == size) {
                    app.settings.button_size = size;
                }
            }
        });
        sub(ui, "bars.toolbox_toolbar", |ui| {
            if item(ui, "bars.toolbox_reset", "", true) {
                crate::ui::toolbox::reset_toolbox(app);
            }
            sep(ui);
            sub(ui, "bars.button_size", |ui| {
                let own = app.settings.toolbox_button_size;
                if radio(ui, "bars.size_default", "", own.is_none()) {
                    app.settings.toolbox_button_size = None;
                }
                for size in crate::settings::ButtonSize::ALL {
                    if radio(ui, size.key(), "", own == Some(size)) {
                        app.settings.toolbox_button_size = Some(size);
                    }
                }
            });
            sep(ui);
            for c in crate::settings::Captions::ALL {
                if radio(ui, c.key(), "", app.settings.toolbox_captions == c) {
                    app.settings.toolbox_captions = c;
                }
            }
        });
        sub(ui, "menu.window.workspace", |ui| {
            if item(ui, "menu.window.workspace_export", "", true) {
                app.export_workspace_dialog(crate::workspace_file::WorkspaceParts::ALL);
            }
            if item(ui, "menu.window.workspace_import", "", true) {
                app.import_workspace_dialog(crate::workspace_file::WorkspaceParts::ALL, true);
            }
        });
        sep(ui);
        // The Command Bars options: every bar, its lock, the toolbox.
        if item(ui, "bars.customize_options", "", true) {
            app.options_page = crate::ui::options::OptionsPage::CommandBars;
            app.dialog = crate::ui::dialogs::Dialog::Options;
        }
    });
    sep(ui);
    let locked = app.settings.bars.locked;
    if check(ui, "bars.lock", "", locked) {
        app.settings.bars.locked = !locked;
    }
    if item(ui, "bars.reset", "", true) {
        reset(app);
    }
}

/// Every bar back at its first place and shown; Lock Toolbars stays as it
/// was.
pub fn reset(app: &mut App) {
    let locked = app.settings.bars.locked;
    app.settings.bars = BarLayout {
        locked,
        ..BarLayout::default()
    };
    for b in Bar::ALL {
        b.set_visible(app, true);
    }
}

/// Put `bar` at `dock` (floating at `at` when given).
pub fn move_bar(app: &mut App, bar: Bar, dock: Dock, at: Option<Pos2>) {
    if app.settings.bars.set_dock(bar, dock) {
        if let (Dock::Floating, Some(p)) = (dock, at) {
            app.settings.bars.set_pos(bar, p);
        }
        bar.set_visible(app, true);
    }
}

/// While a bar is dragged: the place it would go, highlighted; on release
/// it goes there.
pub fn drag_overlay(app: &mut App, ctx: &egui::Context, menu_h: f32) {
    let Some(mut drag) = app.bar_drag else {
        return;
    };
    let screen = ctx.content_rect();
    let (pointer, down) = ctx.input(|i| (i.pointer.latest_pos(), i.pointer.primary_down()));
    let Some(p) = pointer.or(drag.last) else {
        if !down {
            app.bar_drag = None;
        }
        return;
    };
    drag.last = Some(p);
    app.bar_drag = Some(drag);
    let dock = target_dock(drag.bar, screen, menu_h, p);
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Tooltip,
        egui::Id::new("bar_drag_overlay"),
    ));
    let fill = Color32::from_rgba_unmultiplied(0x1E, 0x78, 0xD7, 46);
    let edge = Stroke::new(1.5, Color32::from_rgb(0x1E, 0x78, 0xD7));
    let zone = drop_zone(app, drag, dock, screen, menu_h, p);
    painter.rect(zone, 2.0, fill, edge, egui::StrokeKind::Inside);
    ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
    if !down {
        app.bar_drag = None;
        move_bar(app, drag.bar, dock, Some(p - drag.grab));
    }
}

/// The band (or, floating, the window) a dragged bar would take at `dock`.
fn drop_zone(app: &App, drag: BarDrag, dock: Dock, screen: Rect, menu_h: f32, p: Pos2) -> Rect {
    let thick = match drag.bar {
        Bar::Toolbox => crate::ui::toolbox::column_width(app),
        Bar::Palette => crate::ui::palette::ROW_H,
        Bar::Standard => 36.0,
        Bar::Property => 53.0,
        Bar::Status => crate::ui::status::BAR_H,
    };
    // Bars dock inside the status bar, which keeps the outer band.
    let status = app.show_status_bar && drag.bar != Bar::Status;
    let status_dock = app.settings.bars.dock(Bar::Status);
    let top = screen.top()
        + menu_h
        + if status && status_dock == Dock::Top {
            crate::ui::status::BAR_H
        } else {
            0.0
        };
    let bottom = screen.bottom()
        - if status && status_dock == Dock::Bottom {
            crate::ui::status::BAR_H
        } else {
            0.0
        };
    match dock {
        Dock::Left => Rect::from_min_max(
            pos2(screen.left(), top),
            pos2(screen.left() + thick, bottom),
        ),
        Dock::Right => Rect::from_min_max(
            pos2(screen.right() - thick, top),
            pos2(screen.right(), bottom),
        ),
        Dock::Top => {
            Rect::from_min_max(pos2(screen.left(), top), pos2(screen.right(), top + thick))
        }
        Dock::Bottom => Rect::from_min_max(
            pos2(screen.left(), bottom - thick),
            pos2(screen.right(), bottom),
        ),
        Dock::Floating => Rect::from_min_size(p - drag.grab, drag.bar.floating_size(app)),
    }
}

/// The floating bars, each in a small window with its name, at its place.
pub fn floating_bars(app: &mut App, ctx: &egui::Context) {
    for bar in [Bar::Standard, Bar::Property, Bar::Toolbox, Bar::Palette] {
        if !bar.visible(app) || app.settings.bars.dock(bar) != Dock::Floating {
            continue;
        }
        let screen = ctx.content_rect();
        let size = bar.floating_size(app);
        let mut pos = app.settings.bars.pos(bar);
        // Keep the title on the screen.
        pos.x = pos
            .x
            .clamp(screen.left(), (screen.right() - 60.0).max(screen.left()));
        pos.y = pos
            .y
            .clamp(screen.top(), (screen.bottom() - TITLE_H).max(screen.top()));
        egui::Area::new(egui::Id::new(("floating_bar", bar as u8)))
            .fixed_pos(pos)
            .order(egui::Order::Middle)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(Tokens::panel())
                    .stroke(Stroke::new(1.0, crate::theme::Tokens::window_border()))
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 3],
                        blur: 10,
                        spread: 0,
                        color: Color32::from_black_alpha(50),
                    })
                    .show(ui, |ui| {
                        ui.set_width(size.x);
                        title_strip(app, ui, bar, size.x);
                        floating_body(app, ui, bar, size);
                    });
            });
    }
}

/// A floating bar's title: its name and a close button. Dragging it moves
/// the bar (docking it near an edge); a double click docks it back.
fn title_strip(app: &mut App, ui: &mut Ui, bar: Bar, width: f32) {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, TITLE_H), Sense::click_and_drag());
    let p = ui.painter();
    p.rect_filled(rect, 0.0, crate::theme::Tokens::tab_strip());
    p.text(
        pos2(rect.left() + 6.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        tr(bar.key()),
        egui::FontId::proportional(11.0),
        Tokens::text(),
    );
    let close = Rect::from_center_size(
        pos2(rect.right() - TITLE_H / 2.0, rect.center().y),
        vec2(TITLE_H, TITLE_H),
    );
    let close_resp = ui.interact(
        close,
        egui::Id::new(("bar_close", bar as u8)),
        Sense::click(),
    );
    let c = close.center();
    let s = Stroke::new(
        1.2,
        if close_resp.hovered() {
            Color32::from_rgb(0xE8, 0x11, 0x23)
        } else {
            Tokens::text_dim()
        },
    );
    p.line_segment([c + vec2(-3.5, -3.5), c + vec2(3.5, 3.5)], s);
    p.line_segment([c + vec2(-3.5, 3.5), c + vec2(3.5, -3.5)], s);
    if close_resp.clicked() {
        bar.set_visible(app, false);
        return;
    }
    if resp.double_clicked() {
        move_bar(app, bar, bar.default_dock(), None);
        return;
    }
    if resp.drag_started() && !app.settings.bars.locked {
        let grab = resp
            .interact_pointer_pos()
            .map(|q| q - rect.min)
            .unwrap_or(vec2(12.0, TITLE_H / 2.0));
        app.bar_drag = Some(BarDrag {
            bar,
            grab,
            last: resp.interact_pointer_pos(),
        });
    }
    bar_menu(app, &resp, bar);
}

/// The bar's controls inside its floating window.
fn floating_body(app: &mut App, ui: &mut Ui, bar: Bar, size: Vec2) {
    match bar {
        Bar::Toolbox => {
            // As tall as its buttons: the size above is for the preview.
            egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(4, 3))
                .show(ui, |ui| {
                    crate::ui::toolbox::toolbox(app, ui, Dock::Floating)
                });
        }
        Bar::Palette => {
            ui.set_width(size.x);
            crate::ui::palette::palette_row(app, ui);
        }
        Bar::Standard => {
            egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(4, 2))
                .show(ui, |ui| {
                    ui.set_height(32.0);
                    crate::ui::toolbar::standard_toolbar(app, ui);
                });
        }
        Bar::Property => {
            egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(4, 8))
                .show(ui, |ui| {
                    ui.set_height(37.0);
                    crate::ui::toolbar::property_bar(app, ui);
                });
        }
        Bar::Status => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen() -> Rect {
        Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 1000.0))
    }

    #[test]
    fn a_dropped_bar_docks_at_the_nearest_edge_or_floats() {
        let s = screen();
        assert_eq!(
            target_dock(Bar::Palette, s, 24.0, pos2(1590.0, 500.0)),
            Dock::Right
        );
        assert_eq!(
            target_dock(Bar::Palette, s, 24.0, pos2(10.0, 500.0)),
            Dock::Left
        );
        assert_eq!(
            target_dock(Bar::Palette, s, 24.0, pos2(800.0, 40.0)),
            Dock::Top
        );
        assert_eq!(
            target_dock(Bar::Palette, s, 24.0, pos2(800.0, 990.0)),
            Dock::Bottom
        );
        assert_eq!(
            target_dock(Bar::Palette, s, 24.0, pos2(800.0, 500.0)),
            Dock::Floating
        );
        // The standard toolbar cannot dock on a side: it floats there.
        assert_eq!(
            target_dock(Bar::Standard, s, 24.0, pos2(5.0, 500.0)),
            Dock::Floating
        );
        // The status bar cannot float: the nearest edge it can take.
        assert_eq!(
            target_dock(Bar::Status, s, 24.0, pos2(800.0, 300.0)),
            Dock::Top
        );
        assert_eq!(
            target_dock(Bar::Status, s, 24.0, pos2(800.0, 900.0)),
            Dock::Bottom
        );
    }

    #[test]
    fn bars_only_go_where_they_can() {
        let mut l = BarLayout::default();
        assert_eq!(l.dock(Bar::Toolbox), Dock::Left);
        assert!(l.set_dock(Bar::Toolbox, Dock::Right));
        assert_eq!(l.dock(Bar::Toolbox), Dock::Right);
        assert!(!l.set_dock(Bar::Status, Dock::Floating));
        assert_eq!(l.dock(Bar::Status), Dock::Bottom);
        // A stored dock a bar cannot take reads as its default.
        l.standard = Dock::Left;
        assert_eq!(l.dock(Bar::Standard), Dock::Top);
        l.set_pos(Bar::Palette, pos2(10.0, 20.0));
        assert_eq!(l.pos(Bar::Palette), pos2(10.0, 20.0));
    }

    #[test]
    fn the_drop_zone_leaves_the_status_bar_outside() {
        let mut app = App::headless();
        app.show_status_bar = true;
        let s = screen();
        let drag = BarDrag {
            bar: Bar::Property,
            grab: vec2(12.0, 9.0),
            last: None,
        };
        let status = crate::ui::status::BAR_H;
        let z = drop_zone(&app, drag, Dock::Bottom, s, 24.0, pos2(800.0, 990.0));
        assert_eq!(z.bottom(), 1000.0 - status);
        assert_eq!(z.height(), 53.0);
        let z = drop_zone(&app, drag, Dock::Top, s, 24.0, pos2(800.0, 30.0));
        assert_eq!(z.top(), 24.0);
        // The status bar itself takes the outer band.
        let drag = BarDrag {
            bar: Bar::Status,
            ..drag
        };
        let z = drop_zone(&app, drag, Dock::Bottom, s, 24.0, pos2(800.0, 990.0));
        assert_eq!(z.bottom(), 1000.0);
        // Floating: the bar's window, held where it was grabbed.
        let drag = BarDrag {
            bar: Bar::Palette,
            ..drag
        };
        let z = drop_zone(&app, drag, Dock::Floating, s, 24.0, pos2(500.0, 500.0));
        assert_eq!(z.min, pos2(488.0, 491.0));
        assert_eq!(z.size(), Bar::Palette.floating_size(&app));
    }

    #[test]
    fn reset_brings_every_bar_back_and_keeps_the_lock() {
        let mut app = App::headless();
        move_bar(
            &mut app,
            Bar::Palette,
            Dock::Floating,
            Some(pos2(300.0, 300.0)),
        );
        move_bar(&mut app, Bar::Toolbox, Dock::Right, None);
        Bar::Standard.set_visible(&mut app, false);
        app.settings.bars.locked = true;
        reset(&mut app);
        assert_eq!(app.settings.bars.dock(Bar::Palette), Dock::Bottom);
        assert_eq!(app.settings.bars.dock(Bar::Toolbox), Dock::Left);
        assert!(Bar::ALL.iter().all(|b| b.visible(&app)));
        assert!(app.settings.bars.locked);
        // A move puts the bar there and shows it.
        Bar::Property.set_visible(&mut app, false);
        move_bar(&mut app, Bar::Property, Dock::Bottom, None);
        assert!(Bar::Property.visible(&app));
        assert_eq!(app.settings.bars.dock(Bar::Property), Dock::Bottom);
    }

    #[test]
    fn the_layout_survives_the_preferences() {
        let mut l = BarLayout::default();
        l.palette = Dock::Right;
        l.locked = true;
        let json = serde_json::to_string(&l).expect("json");
        let back: BarLayout = serde_json::from_str(&json).expect("back");
        assert_eq!(back, l);
        // Older preferences without bars read as the default layout, once
        // brought up to date.
        let mut empty: BarLayout = serde_json::from_str("{}").expect("empty");
        empty.migrate();
        assert_eq!(empty, BarLayout::default());
    }

    #[test]
    fn the_palette_starts_at_the_bottom_and_stays_where_it_is_put() {
        let layout = |palette: &str, version: Option<u32>| -> BarLayout {
            let version = version.map(|v| format!(r#", "version": {v}"#));
            let json = format!(
                r#"{{"toolbox": "left", "palette": "{palette}", "standard": "top",
                    "property": "top", "status": "bottom"{}}}"#,
                version.unwrap_or_default()
            );
            let mut l: BarLayout = serde_json::from_str(&json).expect("layout");
            l.migrate();
            assert_eq!(l.version, LAYOUT_VERSION);
            l
        };
        // A fresh install: the bottom.
        assert_eq!(BarLayout::default().palette, Dock::Bottom);
        assert_eq!(Bar::Palette.default_dock(), Dock::Bottom);
        // Version 1 put the palette at the right by default: back to the
        // bottom; anywhere else it stays.
        assert_eq!(layout("right", Some(1)).palette, Dock::Bottom);
        assert_eq!(layout("left", Some(1)).palette, Dock::Left);
        assert_eq!(layout("floating", Some(1)).palette, Dock::Floating);
        // Before that the bottom was the first place: a palette at the
        // right was the user's choice.
        assert_eq!(layout("right", None).palette, Dock::Right);
        assert_eq!(layout("bottom", None).palette, Dock::Bottom);
        // Put at the right now, it stays there across restarts.
        let mut l = BarLayout::default();
        assert!(l.set_dock(Bar::Palette, Dock::Right));
        let mut back: BarLayout =
            serde_json::from_str(&serde_json::to_string(&l).expect("json")).expect("back");
        back.migrate();
        assert_eq!(back.palette, Dock::Right);
    }
}
