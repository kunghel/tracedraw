//! A drawing in a window of its own (#2904), to put it on another
//! monitor. A drawing's tab dragged out of the main window, or Window >
//! Move to Own Window, gives it a window of the system; the tab leaves the
//! main window's tab strip. Its window has a strip with the drawing's tab
//! and a button that brings it back among the tabs, as dragging that tab
//! onto the main window's tabs does. Closing the window closes only that
//! drawing (asking first about unsaved changes, in the main window), and
//! minimizing the main window leaves it on screen.
//!
//! It is the same program and the same drawing, with its undo history:
//! only one drawing is active at a time (`App::active_doc`), the one whose
//! window the user last worked in. The active drawing's window shows it
//! with its rulers, scrollbars and tools; the tools, dockers, bars and
//! shortcuts act on it. Any other window, the main one too, shows its
//! drawing's page as its own view left it, as Window > Tile does
//! (`windows::paint_parked`), and a click in it makes that drawing the
//! active one.
//!
//! Where the system cannot open more windows (the web, Android: egui
//! then embeds them), the drawings stay in tabs.

use crate::app::App;
use crate::i18n::tr;
use crate::theme::Tokens;
use crate::ui::tips::RichTip;
use egui::{Pos2, Rect, Sense, Stroke, Ui, Vec2, ViewportBuilder, ViewportId};
use std::sync::atomic::{AtomicU64, Ordering};

/// Height of a window's strip, as a document tab.
const STRIP: f32 = crate::ui::tabs::TAB_HEIGHT;
/// The smallest window a drawing gets.
const MIN_SIZE: Vec2 = Vec2::new(480.0, 360.0);

/// Source of the windows' identities.
static NEXT_WINDOW: AtomicU64 = AtomicU64::new(1);

/// A drawing's window of its own.
#[derive(Debug, Clone, PartialEq)]
pub struct Detached {
    /// The drawing's slot: its `drawing` and `window` (slots move among
    /// the tabs and close, these stay).
    pub key: (u64, u32),
    /// Where the window opens, on the desktop, and its size.
    pub at: Option<Pos2>,
    pub size: Vec2,
    /// The system window's identity, for as long as it is open.
    pub id: u64,
}

impl Detached {
    fn viewport(&self) -> ViewportId {
        ViewportId::from_hash_of(("drawing_window", self.id))
    }
}

/// Whether drawings can have windows of their own here.
pub fn can_detach(ctx: &egui::Context) -> bool {
    !ctx.embed_viewports()
}

fn key_of(app: &App, i: usize) -> Option<(u64, u32)> {
    app.docs.get(i).map(|d| (d.drawing, d.window))
}

fn index_of(app: &App, key: (u64, u32)) -> Option<usize> {
    app.docs.iter().position(|d| (d.drawing, d.window) == key)
}

/// Whether drawing `i` is in a window of its own.
pub fn is_detached(app: &App, i: usize) -> bool {
    key_of(app, i).is_some_and(|k| app.detached.iter().any(|d| d.key == k))
}

/// The drawings whose tabs are in the main window, in their order.
pub fn main_tabs(app: &App) -> Vec<usize> {
    (0..app.docs.len())
        .filter(|&i| !is_detached(app, i))
        .collect()
}

/// The drawing the main window shows: the active one when it is there,
/// else the one last active there, else its first tab.
pub fn main_doc(app: &App) -> Option<usize> {
    if app.docs.is_empty() {
        return None;
    }
    if !is_detached(app, app.active_doc) {
        return Some(app.active_doc);
    }
    app.main_window_doc
        .and_then(|k| index_of(app, k))
        .filter(|&i| !is_detached(app, i))
        .or_else(|| main_tabs(app).first().copied())
}

/// Give drawing `i` a window of its own, opening at `at` on the desktop
/// (`None`: a little down and right of the main window), `size` big. The
/// drawing becomes the active one.
pub fn detach(app: &mut App, i: usize, at: Option<Pos2>, size: Vec2) {
    let Some(key) = key_of(app, i) else {
        return;
    };
    if is_detached(app, i) {
        return;
    }
    // The main window keeps showing a drawing of its tabs.
    let stays = main_tabs(app).into_iter().find(|&j| j != i);
    if let Some(j) = (app.active_doc != i && !is_detached(app, app.active_doc))
        .then_some(app.active_doc)
        .or(stays)
    {
        app.main_window_doc = key_of(app, j);
    }
    app.detached.push(Detached {
        key,
        at,
        size: size.max(MIN_SIZE),
        id: NEXT_WINDOW.fetch_add(1, Ordering::Relaxed),
    });
    app.switch_document(i);
}

/// Put drawing `i` back among the main window's tabs, as the active one.
pub fn attach(app: &mut App, i: usize) {
    let Some(key) = key_of(app, i) else {
        return;
    };
    app.detached.retain(|d| d.key != key);
    app.main_window_doc = Some(key);
    app.switch_document(i);
}

/// The size a new window gets: most of the main window's drawing area.
pub fn default_size(ctx: &egui::Context) -> Vec2 {
    (ctx.content_rect().size() * 0.75).max(MIN_SIZE)
}

/// Where a new window opens when no place was given: down and right of
/// the main window.
fn default_place(ctx: &egui::Context, n: usize) -> Option<Pos2> {
    let main = ctx.input_for(ViewportId::ROOT, |i| i.viewport().inner_rect)?;
    Some(main.min + Vec2::splat(60.0 + 28.0 * n as f32))
}

/// The main window's point `p` on the desktop.
pub fn desktop_point(ctx: &egui::Context, p: Pos2) -> Option<Pos2> {
    let main = ctx.input_for(ViewportId::ROOT, |i| i.viewport().inner_rect)?;
    Some(main.min + p.to_vec2())
}

/// Window > Move to Own Window: the active drawing.
pub fn detach_active(app: &mut App, ctx: &egui::Context) {
    if !can_detach(ctx) || !app.has_document() || app.show_welcome {
        return;
    }
    let at = default_place(ctx, app.detached.len());
    let size = default_size(ctx);
    detach(app, app.active_doc, at, size);
}

/// Where the main window's tab strip was last drawn (its own points).
pub fn note_tab_strip(ctx: &egui::Context, strip: Rect) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("main_tab_strip"), strip));
}

fn main_tab_strip_on_desktop(ctx: &egui::Context) -> Option<Rect> {
    let strip = ctx.data(|d| d.get_temp::<Rect>(egui::Id::new("main_tab_strip")))?;
    let main = ctx.input_for(ViewportId::ROOT, |i| i.viewport().inner_rect)?;
    Some(strip.translate(main.min.to_vec2()))
}

/// The drawings' own windows, each frame, after the main window.
pub fn show(app: &mut App, ctx: &egui::Context) {
    // Windows of drawings that closed go with them.
    let keys: Vec<(u64, u32)> = app.docs.iter().map(|d| (d.drawing, d.window)).collect();
    app.detached.retain(|d| keys.contains(&d.key));
    if !is_detached(app, app.active_doc) {
        app.main_window_doc = key_of(app, app.active_doc);
    }
    if app.detached.is_empty() || app.show_welcome && !app.has_document() {
        return;
    }
    for d in app.detached.clone() {
        let Some(i) = index_of(app, d.key) else {
            continue;
        };
        let title = app.document_tab_title(i);
        let builder = ViewportBuilder::default().with_title(format!("TraceDraw - {title}"));
        // The splash, which names the application to the window system,
        // is not in the browser build (it broke that build).
        #[cfg(not(target_arch = "wasm32"))]
        let builder = builder.with_app_id(crate::splash::APP_ID);
        let mut builder = builder
            .with_inner_size(d.size)
            .with_min_inner_size(MIN_SIZE);
        if let Some(at) = d.at {
            builder = builder.with_position(at);
        }
        ctx.show_viewport_immediate(d.viewport(), builder, |ui, _| {
            window(app, ui, d.key);
        });
    }
}

/// What the strip of a drawing's window asked for.
enum Ask {
    Back,
    Activate,
}

/// The inside of the window of the drawing `key`.
fn window(app: &mut App, ui: &mut Ui, key: (u64, u32)) {
    let Some(i) = index_of(app, key) else {
        return;
    };
    let ctx = ui.ctx().clone();
    // Closing the window closes the drawing; the question about unsaved
    // changes comes up in the main window, and the window goes once the
    // drawing has.
    if ctx.input(|inp| inp.viewport().close_requested()) {
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        app.switch_document(i);
        app.close_document();
        return;
    }
    let active = i == app.active_doc && !app.show_welcome;
    let mut ask = None;
    egui::Panel::top(egui::Id::new(("drawing_window_strip", key)))
        .frame(egui::Frame::new().fill(Tokens::tab_strip()))
        .show_separator_line(false)
        .show(ui, |ui| {
            ask = strip(app, ui, i, active);
        });
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE)
        .show(ui, |ui| {
            let full = ui.available_rect_before_wrap();
            if active {
                // The keys typed in this window go to its drawing.
                app.keyboard(&ctx);
                if let Some(cmd) = app.pending_command.take() {
                    if !crate::ui::menus::run_command(app, &ctx, &cmd) {
                        app.status =
                            crate::i18n::trf("status.command_unavailable", &[("name", &tr(&cmd))]);
                    }
                }
                crate::ui::drawing_window(app, ui, full);
            } else {
                let resp = ui.interact(
                    full,
                    egui::Id::new(("drawing_window_parked", key)),
                    Sense::click(),
                );
                crate::ui::windows::paint_parked(app, ui, i, full);
                if resp.clicked() {
                    ask = Some(Ask::Activate);
                }
            }
        });
    match ask {
        Some(Ask::Back) => attach(app, i),
        Some(Ask::Activate) => app.switch_document(i),
        None => {}
    }
}

/// The window's strip: the drawing's tab, which dragged onto the main
/// window's tabs puts it back there, and the button that does the same.
fn strip(app: &App, ui: &mut Ui, i: usize, active: bool) -> Option<Ask> {
    let title = app.document_tab_title(i);
    let row = ui.max_rect();
    let row = Rect::from_min_size(row.min, Vec2::new(row.width(), STRIP));
    ui.allocate_rect(row, Sense::hover());
    let galley = ui.painter().layout_no_wrap(
        title.clone(),
        egui::FontId::proportional(13.0),
        Tokens::text(),
    );
    let tab = Rect::from_min_size(row.min, Vec2::new(galley.size().x + 30.0, STRIP));
    let resp = ui
        .interact(
            tab,
            egui::Id::new(("drawing_window_tab", i)),
            Sense::click_and_drag(),
        )
        .rich_tip(format!("{title}\n{}", tr("tabs.detached_desc")));
    let fill = if active {
        Tokens::tab_active()
    } else if resp.hovered() {
        Tokens::tool_hover()
    } else {
        Tokens::tab_strip()
    };
    ui.painter().rect_filled(tab, 0.0, fill);
    ui.painter().galley(
        Pos2::new(
            tab.left() + 15.0,
            (tab.center().y - galley.size().y / 2.0).round(),
        ),
        galley,
        Tokens::text(),
    );
    let back_text = tr("menu.window.attach_window");
    let back_w = ui
        .painter()
        .layout_no_wrap(
            back_text.clone(),
            egui::FontId::proportional(12.5),
            Tokens::text(),
        )
        .size()
        .x
        + 20.0;
    let back = Rect::from_min_size(
        Pos2::new(row.right() - back_w - 4.0, row.top() + 2.0),
        Vec2::new(back_w, STRIP - 4.0),
    );
    let back_resp = ui
        .interact(
            back,
            egui::Id::new(("drawing_window_back", i)),
            Sense::click(),
        )
        .rich_tip(tr("tabs.detached_desc"));
    if back_resp.hovered() {
        ui.painter().rect_filled(back, 2.0, Tokens::tool_hover());
    }
    ui.painter().rect_stroke(
        back,
        2.0,
        Stroke::new(1.0, Tokens::text_dim()),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        back.center(),
        egui::Align2::CENTER_CENTER,
        back_text,
        egui::FontId::proportional(12.5),
        Tokens::text(),
    );
    ui.painter().rect_filled(
        Rect::from_min_max(
            Pos2::new(row.left(), row.bottom()),
            Pos2::new(row.right(), row.bottom() + 1.0),
        ),
        0.0,
        Tokens::ACCENT,
    );
    if back_resp.clicked() {
        return Some(Ask::Back);
    }
    // The tab let go over the main window's tabs.
    if resp.drag_stopped() {
        let here = ui.ctx().input(|inp| inp.viewport().inner_rect);
        let pos = ui.ctx().input(|inp| inp.pointer.latest_pos());
        let over = here
            .zip(pos)
            .map(|(r, p)| r.min + p.to_vec2())
            .zip(main_tab_strip_on_desktop(ui.ctx()))
            .is_some_and(|(p, strip)| strip.expand(8.0).contains(p));
        if over {
            return Some(Ask::Back);
        }
    }
    if resp.clicked() && !active {
        return Some(Ask::Activate);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn three() -> App {
        let mut app = App::headless();
        app.show_welcome = false;
        app.new_document();
        app.new_document();
        assert_eq!(app.docs.len(), 3);
        app
    }

    /// A drawing given a window leaves the main window's tabs and becomes
    /// active; the main window keeps showing the drawing it showed; back
    /// among the tabs it is active there again.
    #[test]
    fn a_drawing_moves_to_its_own_window_and_back() {
        let mut app = three();
        app.switch_document(0);
        detach(&mut app, 1, None, Vec2::new(800.0, 600.0));
        assert!(is_detached(&app, 1));
        assert_eq!(app.active_doc, 1);
        assert_eq!(main_tabs(&app), vec![0, 2]);
        assert_eq!(main_doc(&app), Some(0));
        // Twice is still one window.
        detach(&mut app, 1, None, Vec2::ZERO);
        assert_eq!(app.detached.len(), 1);
        // Working in the main window makes its drawing active; the window
        // keeps its drawing.
        app.switch_document(2);
        assert_eq!(main_doc(&app), Some(2));
        attach(&mut app, 1);
        assert!(app.detached.is_empty());
        assert_eq!(app.active_doc, 1);
        assert_eq!(main_tabs(&app), vec![0, 1, 2]);
    }

    /// The window follows its drawing when tabs move, and goes when the
    /// drawing closes; with every drawing in a window the main window
    /// shows none.
    #[test]
    fn a_window_follows_its_drawing() {
        let mut app = three();
        detach(&mut app, 2, None, Vec2::new(800.0, 600.0));
        app.move_document(2, 0);
        assert!(is_detached(&app, 0) && !is_detached(&app, 2));
        detach(&mut app, 1, None, Vec2::new(800.0, 600.0));
        detach(&mut app, 2, None, Vec2::new(800.0, 600.0));
        assert!(main_tabs(&app).is_empty());
        assert_eq!(main_doc(&app), None);
        let ctx = crate::theme::ui_context();
        app.switch_document(0);
        app.close_active_document();
        let mut out = ctx.run_ui(Default::default(), |ui| show(&mut app, ui.ctx()));
        out.textures_delta.clear();
        assert_eq!(app.detached.len(), 2);
        assert!(app.detached.iter().all(|d| d.size.x >= MIN_SIZE.x));
    }

    /// Drawn with the main window (embedded here, as a test's egui does):
    /// the main tab strip leaves the drawing out, every drawing stays as
    /// it was, and a click in the main window's drawing makes it active.
    #[test]
    fn the_main_window_and_a_drawing_window_draw_together() {
        let ctx = crate::theme::ui_context();
        let mut app = three();
        app.switch_document(0);
        let page0 = app.page;
        detach(&mut app, 1, None, Vec2::new(700.0, 500.0));
        let draw = |app: &mut App| {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1400.0, 900.0))),
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| crate::ui::root(app, ui));
            out.textures_delta.clear();
        };
        draw(&mut app);
        draw(&mut app);
        assert_eq!(app.active_doc, 1);
        assert!(ctx
            .data(|d| d.get_temp::<Rect>(egui::Id::new("main_tab_strip")))
            .is_some());
        assert_eq!(main_doc(&app), Some(0));
        app.switch_document(0);
        assert_eq!(app.page, page0);
        draw(&mut app);
        assert_eq!(app.active_doc, 0);
        assert!(is_detached(&app, 1));
    }
}
