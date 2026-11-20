//! Touch bars beside the selection, and a second finger that leaves a
//! copy (#1691). On a touch screen without a mouse there is no right
//! click, and no Ctrl+D, Ctrl+R or L, R, T, B to press. While the Pick
//! tool has objects selected, a column of buttons stands beside the
//! selection: Duplicate, Repeat, Group, Ungroup, To Front of Page, To
//! Back of Page, Delete and the object's menu (what a right click or a
//! held finger opens). With two objects or more, a bar over the top of
//! the drawing holds the shaping commands (Weld, Trim, Intersect, Front
//! Minus Back, Back Minus Front, Combine) and the alignments. Both show
//! where the keys on the drawing show (Android, or once a finger has
//! touched the window) unless Options > Display turns them off, and hide
//! while the selection is dragged. Options > Display can show them with
//! a mouse too (off at first, #3456).
//!
//! A second finger touching the drawing while a first one moves, scales,
//! turns or slants the selection does what a right click does then: the
//! original stays and the change goes to a copy (a second touch takes it
//! back). The second finger is taken out of the input, so it is not a
//! pinch or a pan that ends the drag. See docs/behavior/android.md.

use crate::app::App;
use crate::i18n::tr;
use crate::ops::{Align, Shaping};

/// The side of a button of the touch bars, in points: room for a finger.
pub const BUTTON: f32 = 36.0;
/// The side of the picture in a button.
const PICTURE: f32 = 20.0;
/// The room between the selection and the column beside it.
const GAP: f32 = 12.0;
/// The frame's inner margin.
const MARGIN: f32 = 3.0;

/// A command of the touch bars.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarCommand {
    Duplicate,
    Repeat,
    Group,
    Ungroup,
    ToFront,
    ToBack,
    Delete,
    Menu,
    Shape(Shaping),
    Combine,
    Align(Align),
}

impl BarCommand {
    /// The column beside the selection.
    pub const SIDE: [BarCommand; 8] = [
        BarCommand::Duplicate,
        BarCommand::Repeat,
        BarCommand::Group,
        BarCommand::Ungroup,
        BarCommand::ToFront,
        BarCommand::ToBack,
        BarCommand::Delete,
        BarCommand::Menu,
    ];
    /// The shaping half of the bar over the drawing.
    pub const SHAPING: [BarCommand; 6] = [
        BarCommand::Shape(Shaping::Weld),
        BarCommand::Shape(Shaping::Trim),
        BarCommand::Shape(Shaping::Intersect),
        BarCommand::Shape(Shaping::FrontMinusBack),
        BarCommand::Shape(Shaping::BackMinusFront),
        BarCommand::Combine,
    ];
    /// The alignment half of the bar over the drawing.
    pub const ALIGN: [BarCommand; 6] = [
        BarCommand::Align(Align::Left),
        BarCommand::Align(Align::CenterH),
        BarCommand::Align(Align::Right),
        BarCommand::Align(Align::Top),
        BarCommand::Align(Align::CenterV),
        BarCommand::Align(Align::Bottom),
    ];

    /// The menu item it does: its name, tip and picture.
    pub fn key(self) -> &'static str {
        match self {
            BarCommand::Duplicate => "menu.edit.duplicate",
            BarCommand::Repeat => "menu.edit.repeat",
            BarCommand::Group => "menu.object.group_group",
            BarCommand::Ungroup => "menu.object.group_ungroup",
            BarCommand::ToFront => "menu.object.to_front_of_page",
            BarCommand::ToBack => "menu.object.to_back_of_page",
            BarCommand::Delete => "menu.edit.delete",
            BarCommand::Menu => "touch_bar.menu",
            BarCommand::Shape(Shaping::Weld) => "menu.object.weld",
            BarCommand::Shape(Shaping::Trim) => "menu.object.trim",
            BarCommand::Shape(Shaping::Intersect) => "menu.object.intersect",
            BarCommand::Shape(Shaping::Simplify) => "menu.object.simplify",
            BarCommand::Shape(Shaping::FrontMinusBack) => "menu.object.front_minus_back",
            BarCommand::Shape(Shaping::BackMinusFront) => "menu.object.back_minus_front",
            BarCommand::Shape(Shaping::Boundary) => "menu.object.boundary",
            BarCommand::Combine => "menu.object.combine",
            BarCommand::Align(Align::Left) => "menu.object.align_left",
            BarCommand::Align(Align::Right) => "menu.object.align_right",
            BarCommand::Align(Align::Top) => "menu.object.align_top",
            BarCommand::Align(Align::Bottom) => "menu.object.align_bottom",
            BarCommand::Align(Align::CenterH) => "menu.object.align_centers_h",
            BarCommand::Align(Align::CenterV) => "menu.object.align_centers_v",
            BarCommand::Align(Align::CenterPage) => "menu.object.center_to_page",
        }
    }

    /// Whether it can be done now, as its menu item is enabled.
    pub fn enabled(self, app: &App) -> bool {
        let has = !app.selection.is_empty();
        match self {
            BarCommand::Duplicate | BarCommand::Ungroup | BarCommand::ToFront => has,
            BarCommand::ToBack | BarCommand::Menu => has,
            BarCommand::Repeat => app.last_repeatable().is_some(),
            BarCommand::Group => app.can_group(),
            BarCommand::Delete => app.can_delete(),
            BarCommand::Shape(op) => app.shaping_ready(op),
            BarCommand::Combine => app.selection.len() >= 2,
            BarCommand::Align(_) => has,
        }
    }

    /// Do it (the Menu opens the object's menu beside `at`).
    pub fn run(self, app: &mut App, at: egui::Pos2) {
        match self {
            BarCommand::Duplicate => app.duplicate(),
            BarCommand::Repeat => app.repeat_last(),
            BarCommand::Group => app.group_selection(),
            BarCommand::Ungroup => app.ungroup_selection(),
            BarCommand::ToFront => app.order_page(true),
            BarCommand::ToBack => app.order_page(false),
            BarCommand::Delete => app.delete_command(),
            BarCommand::Menu => {
                let p = app
                    .selection_bounds()
                    .map(|b| b.center())
                    .unwrap_or_else(|| app.view.to_page(at));
                app.context_menu = Some((at, p));
            }
            BarCommand::Shape(op) => app.shaping(op),
            BarCommand::Combine => app.combine(),
            BarCommand::Align(a) => app.align(a),
        }
    }
}

/// Whether the touch bars are shown now: on a touch screen with the
/// option on (or with a mouse too, when Options > Display asks for it,
/// #3456), the Pick tool's selection standing still and nothing over the
/// drawing.
pub fn shown(app: &App, ctx: &egui::Context) -> bool {
    let wanted = app.settings.selection_bar_mouse
        || (app.settings.touch_selection_bar && crate::touch_modifiers::offered(ctx));
    wanted
        && app.has_document()
        && matches!(
            app.tool,
            crate::tools::Tool::Pick | crate::tools::Tool::FreeformPick
        )
        && !app.selection.is_empty()
        && matches!(app.drag, crate::app::Drag::None)
        && app.context_menu.is_none()
        && !app.editing_text()
        && matches!(app.dialog, crate::ui::dialogs::Dialog::None)
}

/// Where the column of `height` goes beside the selection `sel` in the
/// drawing `canvas`: its top left corner. To the right of the selection
/// when there is room, else to its left, else at the drawing's right
/// edge; level with the selection's top, kept inside the drawing.
pub fn side_place(sel: egui::Rect, canvas: egui::Rect, height: f32) -> egui::Pos2 {
    let width = BUTTON + 2.0 * MARGIN;
    let x = if canvas.right() - sel.right() >= width + GAP {
        sel.right() + GAP
    } else if sel.left() - canvas.left() >= width + GAP {
        sel.left() - GAP - width
    } else {
        canvas.right() - width - 4.0
    };
    let top = canvas.top() + 4.0;
    let bottom = (canvas.bottom() - height - 4.0).max(top);
    egui::pos2(x, sel.top().clamp(top, bottom))
}

/// One button of the bars: the menu item's picture, its name as the tip.
fn button(ui: &mut egui::Ui, app: &mut App, cmd: BarCommand) {
    let enabled = cmd.enabled(app);
    let (rect, resp) = ui.allocate_exact_size(
        egui::Vec2::splat(BUTTON),
        if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        },
    );
    crate::theme::paint_icon_button(ui.painter(), rect, enabled && resp.hovered(), false);
    let color = if enabled {
        crate::theme::Tokens::icon()
    } else {
        crate::theme::Tokens::icon_off()
    };
    let pic = egui::Rect::from_center_size(rect.center(), egui::Vec2::splat(PICTURE));
    if cmd == BarCommand::Menu {
        // Three dots: more, the object's menu.
        for dx in [-6.0, 0.0, 6.0] {
            ui.painter()
                .circle_filled(rect.center() + egui::vec2(dx, 0.0), 2.0, color);
        }
    } else {
        crate::ui::menus::draw_menu_picture(ui.painter(), pic, cmd.key(), color);
    }
    let key = cmd.key();
    let resp = resp.on_hover_ui(|ui| crate::ui::toolbar::rich_tip(ui, &tr(key)));
    app.touch_bar_shown.push((cmd, rect));
    if resp.clicked() {
        cmd.run(app, rect.right_top());
    }
}

fn frame(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(crate::theme::Tokens::panel())
        .stroke(egui::Stroke::new(
            1.0,
            crate::theme::Tokens::control_border(),
        ))
        .corner_radius(4.0)
        .inner_margin(MARGIN)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(2.0, 2.0);
            add(ui)
        });
}

/// The column beside the selection and, with two objects or more, the
/// bar over the top of the drawing `canvas` (#1691).
pub fn bars_on_drawing(app: &mut App, ui: &mut egui::Ui, canvas: egui::Rect) {
    app.touch_bar_shown.clear();
    let ctx = ui.ctx().clone();
    if !shown(app, &ctx) {
        return;
    }
    let Some(sel) = app.selection_bounds().map(|b| app.view.rect_to_screen(b)) else {
        return;
    };
    if !sel.intersects(canvas) {
        return;
    }
    let n = BarCommand::SIDE.len() as f32;
    let height = n * BUTTON + (n - 1.0) * 2.0 + 2.0 * MARGIN;
    egui::Area::new(egui::Id::new("touch_selection_column"))
        .order(egui::Order::Middle)
        .fixed_pos(side_place(sel, canvas, height))
        .constrain_to(canvas)
        .show(&ctx, |ui| {
            frame(ui, |ui| {
                ui.vertical(|ui| {
                    for cmd in BarCommand::SIDE {
                        button(ui, app, cmd);
                    }
                });
            });
        });
    if app.selection.len() < 2 {
        return;
    }
    let all = (BarCommand::SHAPING.len() + BarCommand::ALIGN.len()) as f32;
    let wide = canvas.width() >= all * (BUTTON + 2.0) + 40.0;
    egui::Area::new(egui::Id::new("touch_selection_top"))
        .order(egui::Order::Middle)
        .pivot(egui::Align2::CENTER_TOP)
        .fixed_pos(egui::pos2(canvas.center().x, canvas.top() + 6.0))
        .constrain_to(canvas)
        .show(&ctx, |ui| {
            frame(ui, |ui| {
                let shaping = |ui: &mut egui::Ui, app: &mut App| {
                    ui.horizontal(|ui| {
                        for cmd in BarCommand::SHAPING {
                            button(ui, app, cmd);
                        }
                    });
                };
                let align = |ui: &mut egui::Ui, app: &mut App| {
                    ui.horizontal(|ui| {
                        for cmd in BarCommand::ALIGN {
                            button(ui, app, cmd);
                        }
                    });
                };
                if wide {
                    ui.horizontal(|ui| {
                        shaping(ui, app);
                        ui.separator();
                        align(ui, app);
                    });
                } else {
                    ui.vertical(|ui| {
                        shaping(ui, app);
                        align(ui, app);
                    });
                }
            });
        });
}

/// The fingers on the window, for the second finger that leaves a copy:
/// the ones down, and the ones taken out of the input.
#[derive(Debug, Clone, Default)]
pub struct CopyFingers {
    pub down: Vec<u64>,
    pub taken: Vec<u64>,
}

/// In this frame's input `raw`: a finger that touches the drawing while
/// one finger alone moves, scales, turns or slants the selection turns
/// the copy on release on or off, as a right click does, and is taken
/// out of the input with all it does after (no pinch, no pan, no tap
/// that undoes). Runs in the raw input hook, after the keys' fingers.
pub fn second_finger_copies(app: &mut App, raw: &mut egui::RawInput) {
    use egui::{Event, TouchPhase};
    let mut out = Vec::with_capacity(raw.events.len());
    for e in std::mem::take(&mut raw.events) {
        let Event::Touch { id, phase, pos, .. } = e else {
            out.push(e);
            continue;
        };
        let id = id.0;
        let taken = app.copy_fingers.taken.contains(&id);
        match phase {
            TouchPhase::Start => {
                if !taken
                    && app.copy_fingers.down.len() == 1
                    && app.transforming()
                    && app.canvas_rect.contains(pos)
                {
                    app.copy_fingers.taken.push(id);
                    app.drag_copy = !app.drag_copy;
                    app.status = tr(if app.drag_copy {
                        "status.drag_copy_on"
                    } else {
                        "status.drag_copy_off"
                    });
                    continue;
                }
                if !app.copy_fingers.down.contains(&id) {
                    app.copy_fingers.down.push(id);
                }
                out.push(e);
            }
            TouchPhase::Move => {
                if !taken {
                    out.push(e);
                }
            }
            TouchPhase::End | TouchPhase::Cancel => {
                if taken {
                    app.copy_fingers.taken.retain(|t| *t != id);
                    continue;
                }
                app.copy_fingers.down.retain(|d| *d != id);
                out.push(e);
            }
        }
    }
    raw.events = out;
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, TouchDeviceId, TouchId, TouchPhase};

    fn touch(id: u64, phase: TouchPhase, x: f32, y: f32) -> Event {
        Event::Touch {
            device_id: TouchDeviceId(1),
            id: TouchId(id),
            phase,
            pos: egui::pos2(x, y),
            force: None,
        }
    }

    /// Every button has a picture of its menu item (the Menu draws dots)
    /// and a name in English.
    #[test]
    fn every_button_has_a_picture_and_a_name() {
        let all = BarCommand::SIDE
            .iter()
            .chain(BarCommand::SHAPING.iter())
            .chain(BarCommand::ALIGN.iter());
        for cmd in all {
            if *cmd != BarCommand::Menu {
                assert!(
                    crate::ui::menus::has_own_picture(cmd.key()),
                    "{}",
                    cmd.key()
                );
            }
            assert_ne!(tr(cmd.key()), cmd.key(), "{}", cmd.key());
        }
    }

    /// The column goes right of the selection, else left of it, else at
    /// the drawing's right edge, level with the selection's top and kept
    /// inside the drawing.
    #[test]
    fn the_column_finds_room_beside_the_selection() {
        let canvas = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(360.0, 640.0));
        let h = 300.0;
        let sel = egui::Rect::from_min_size(egui::pos2(50.0, 100.0), egui::vec2(100.0, 80.0));
        assert_eq!(side_place(sel, canvas, h), egui::pos2(162.0, 100.0));
        let right = egui::Rect::from_min_size(egui::pos2(250.0, 100.0), egui::vec2(100.0, 80.0));
        assert_eq!(
            side_place(right, canvas, h).x,
            250.0 - GAP - BUTTON - 2.0 * MARGIN
        );
        let wide = egui::Rect::from_min_size(egui::pos2(10.0, -50.0), egui::vec2(340.0, 80.0));
        let p = side_place(wide, canvas, h);
        assert_eq!(p.x, 360.0 - BUTTON - 2.0 * MARGIN - 4.0);
        assert_eq!(p.y, 4.0);
        let low = egui::Rect::from_min_size(egui::pos2(50.0, 600.0), egui::vec2(10.0, 10.0));
        assert_eq!(side_place(low, canvas, h).y, 640.0 - h - 4.0);
    }

    /// A second finger during a finger's move turns the copy on (and a
    /// third touch off again) and never reaches egui; with no move going
    /// on it passes as it came.
    #[test]
    fn a_second_finger_during_a_move_leaves_a_copy() {
        let mut app = App::headless();
        app.new_shape(tracedraw_core::ShapeKind::Rect {
            rect: tracedraw_core::geometry::Rect::new(10.0, 10.0, 60.0, 40.0),
            radius: 0.0,
            corners: None,
        })
        .expect("rect");
        app.canvas_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let mut raw = egui::RawInput {
            events: vec![touch(1, TouchPhase::Start, 100.0, 100.0)],
            ..Default::default()
        };
        second_finger_copies(&mut app, &mut raw);
        assert_eq!(raw.events.len(), 1);
        // No move yet: the second finger is a second finger (a pinch).
        let mut raw = egui::RawInput {
            events: vec![touch(2, TouchPhase::Start, 300.0, 300.0)],
            ..Default::default()
        };
        second_finger_copies(&mut app, &mut raw);
        assert_eq!(raw.events.len(), 1);
        assert!(!app.drag_copy);
        let mut raw = egui::RawInput {
            events: vec![touch(2, TouchPhase::End, 300.0, 300.0)],
            ..Default::default()
        };
        second_finger_copies(&mut app, &mut raw);
        // The first finger moves the selection.
        app.tool = crate::tools::Tool::Pick;
        let p = tracedraw_core::geometry::Point::new(20.0, 20.0);
        app.drag = crate::app::Drag::Move {
            start: p,
            last: p,
            total: Default::default(),
            start_bounds: tracedraw_core::geometry::Rect::new(10.0, 10.0, 60.0, 40.0),
            source: None,
            start_geometry: None,
        };
        assert!(app.transforming());
        let mut raw = egui::RawInput {
            events: vec![
                touch(1, TouchPhase::Move, 120.0, 100.0),
                touch(3, TouchPhase::Start, 300.0, 300.0),
                touch(3, TouchPhase::Move, 310.0, 300.0),
                touch(3, TouchPhase::End, 310.0, 300.0),
            ],
            ..Default::default()
        };
        second_finger_copies(&mut app, &mut raw);
        assert_eq!(raw.events, vec![touch(1, TouchPhase::Move, 120.0, 100.0)]);
        assert!(app.drag_copy);
        assert_eq!(app.status, tr("status.drag_copy_on"));
        let mut raw = egui::RawInput {
            events: vec![touch(4, TouchPhase::Start, 300.0, 300.0)],
            ..Default::default()
        };
        second_finger_copies(&mut app, &mut raw);
        assert!(raw.events.is_empty());
        assert!(!app.drag_copy);
    }

    /// The bars show only where a finger has touched (or on Android),
    /// with the option on, the Pick tool and a selection standing still;
    /// their buttons run the commands.
    #[test]
    fn the_bars_show_for_a_finger_and_run_their_commands() {
        let mut app = App::headless();
        let ctx = egui::Context::default();
        let a = app
            .new_shape(tracedraw_core::ShapeKind::Rect {
                rect: tracedraw_core::geometry::Rect::new(10.0, 10.0, 60.0, 40.0),
                radius: 0.0,
                corners: None,
            })
            .expect("rect");
        app.tool = crate::tools::Tool::Pick;
        app.select(vec![a]);
        assert_eq!(shown(&app, &ctx), crate::files::ANDROID);
        let count = |app: &App| {
            app.doc()
                .all_layers()
                .map(|l| l.shapes.len())
                .sum::<usize>()
        };
        BarCommand::Duplicate.run(&mut app, egui::Pos2::ZERO);
        assert_eq!(count(&app), 2);
        assert_eq!(
            BarCommand::Repeat.enabled(&app),
            app.last_repeatable().is_some()
        );
        // Two objects: they align to the left.
        app.select(vec![a, app.selection[0]]);
        BarCommand::Align(Align::Left).run(&mut app, egui::Pos2::ZERO);
        assert!(BarCommand::Combine.enabled(&app));
        BarCommand::Menu.run(&mut app, egui::pos2(5.0, 6.0));
        assert_eq!(app.context_menu.map(|(s, _)| s), Some(egui::pos2(5.0, 6.0)));
        app.context_menu = None;
        BarCommand::Delete.run(&mut app, egui::Pos2::ZERO);
        assert_eq!(count(&app), 0);
    }
}
