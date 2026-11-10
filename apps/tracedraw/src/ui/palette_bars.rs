//! Palette bars. Besides the default palette (and the document palette
//! row under it), each palette opened from Window > Color Palettes, the
//! Palette Manager, File > Open Palette or a palette's menu (Open as
//! Palette Bar) is a bar of its own: docked at any edge of the window
//! (a column of swatches at the left or the right) or floating in a
//! small window, each with its own rows and swatch size. A floating
//! palette is resized at its right and bottom edges; its swatches flow
//! into rows along a wide window and into columns down a tall one. A
//! bar is moved by its grip (docked) or its title (floating): near an
//! edge it docks there, elsewhere it floats. Every bar, its place, rows
//! and size are kept in the preferences and in workspace files.
//!
//! Docked at a side, a bar lies at one of the side's places
//! (`settings::BarSlot`): let go over another bar docked there (the
//! toolbox, the default palette, a docker column, a palette bar) it goes
//! just outside or just inside that bar ([`side_target`], [`place_bar`]).
//!
//! Along the top a bar lies beside the drawing, or among the toolbars:
//! dropped on the line between two rows of toolbars (or over or under
//! them all) it lies there, across the window, as a row of its own
//! between the toolbars (`PaletteBarPrefs::toolbar_row`); its menu's
//! Position > Top, Among the Toolbars puts it under them all.
//!
//! The default palette floating is drawn here too, so it flows and
//! resizes the same way; it keeps the bars' drag (see `bars.rs`) and its
//! own places beside the drawing.

use crate::app::App;
use crate::i18n::tr;
use crate::settings::{BarSlot, DOCUMENT_PALETTE, MAX_ROWS, ROWS_FIT, SWATCH_STEPS};
use crate::theme::Tokens;
use crate::ui::bars::{Bar, Dock, TITLE_H};
use crate::ui::palette::Row;
use crate::ui::tips::RichTip;
use egui::{pos2, vec2, Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};

/// A palette bar being dragged: which one, and where the pointer holds it
/// from its window's top-left corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PaletteBarDrag {
    pub bar: usize,
    pub grab: Vec2,
    pub last: Option<Pos2>,
}

/// The border of a floating palette's window.
const BORDER: f32 = 1.0;

/// The places a palette bar can take, with their names.
pub const PLACES: [(Dock, &str); 5] = [
    (Dock::Top, "bars.top"),
    (Dock::Bottom, "bars.bottom"),
    (Dock::Left, "bars.left"),
    (Dock::Right, "bars.right"),
    (Dock::Floating, "palette.float"),
];

/// The swatch side of `which` set to `size` (the default palette's for
/// the document palette row).
pub fn set_swatch(app: &mut App, which: Row, size: u8) {
    let size = size.clamp(crate::settings::SWATCH_MIN, crate::settings::SWATCH_MAX);
    match which {
        Row::Extra(k) => {
            if let Some(b) = app.settings.palette_bars.get_mut(k) {
                b.swatch_size = size;
            }
        }
        _ => app.settings.palette.swatch_size = size,
    }
}

/// The rows of `which` set to `rows` (1 to [`MAX_ROWS`], or
/// [`ROWS_FIT`]). Floating, the window takes that many rows (or columns)
/// at its length; Fit, as many as show every colour.
pub fn set_rows(app: &mut App, which: Row, rows: u8) {
    let rows = rows.min(MAX_ROWS);
    let prefs = crate::ui::palette::strip_prefs(app, which);
    let float = (prefs.dock == Dock::Floating).then(|| {
        let lines = match crate::settings::rows_of(rows) {
            Some(n) => n,
            None => {
                // Every colour at the window's length.
                let (flow, _) = crate::ui::palette::floating_body(app, which);
                let st = crate::ui::palette::stride_of(app, which);
                let cols = crate::ui::palette::visible_slots(flow.len, st);
                crate::ui::palette::swatch_count(app, which)
                    .div_ceil(cols.max(1))
                    .clamp(1, crate::ui::palette::MAX_FLOAT_LINES)
            }
        };
        crate::ui::palette::float_size_for_lines(app, which, lines)
    });
    match which {
        Row::Extra(k) => {
            if let Some(b) = app.settings.palette_bars.get_mut(k) {
                b.rows = rows;
                if let Some(f) = float {
                    b.float_size = f;
                }
            }
        }
        Row::Main => {
            app.settings.palette.rows = rows;
            if let Some(f) = float {
                app.settings.palette.float_size = f;
            }
        }
        Row::Document => {}
    }
}

/// Put `which` at `dock` (floating at `at` when given). The default
/// palette takes its first spot on that side.
pub fn set_dock(app: &mut App, which: Row, dock: Dock, at: Option<Pos2>) {
    match which {
        Row::Extra(k) => {
            if let Some(b) = app.settings.palette_bars.get_mut(k) {
                // Another side: that side's usual place among its bars.
                if b.dock != dock || b.among_toolbars() {
                    b.slot = None;
                }
                b.dock = dock;
                b.toolbar_row = None;
                if let (Dock::Floating, Some(p)) = (dock, at) {
                    b.pos = [p.x, p.y];
                }
            }
        }
        // The document palette row moves as a bar of its own.
        Row::Document => {
            if let Some(k) = detach_document_row(app) {
                set_dock(app, Row::Extra(k), dock, at);
            }
        }
        Row::Main => {
            let spot = if dock.horizontal() {
                crate::ui::bars::PaletteSpot::Beside
            } else {
                crate::ui::bars::PaletteSpot::Edge
            };
            crate::ui::bars::move_palette(app, dock, spot, at);
        }
    }
}

/// Put the palette bar `which` among the toolbars along the top, over
/// the row of toolbars `row` (past the last one: under them all). The
/// default palette keeps its own places.
pub fn set_among_toolbars(app: &mut App, which: Row, row: u32) {
    match which {
        Row::Extra(k) => {
            if let Some(b) = app.settings.palette_bars.get_mut(k) {
                b.dock = Dock::Top;
                b.toolbar_row = Some(row.min(crate::settings::MAX_TOOLBAR_ROW));
            }
        }
        Row::Document => {
            if let Some(k) = detach_document_row(app) {
                set_among_toolbars(app, Row::Extra(k), row);
            }
        }
        Row::Main => {}
    }
}

/// Whether the palette bar `which` lies among the toolbars.
pub fn among_toolbars(app: &App, which: Row) -> bool {
    match which {
        Row::Extra(k) => app
            .settings
            .palette_bars
            .get(k)
            .is_some_and(|b| b.among_toolbars()),
        _ => false,
    }
}

/// The palette bars among the toolbars, as (the row of toolbars each
/// lies over, the bar), top to bottom.
pub fn toolbar_bands(app: &App) -> Vec<(u32, usize)> {
    let mut bands: Vec<(u32, usize)> = app
        .settings
        .palette_bars
        .iter()
        .enumerate()
        .filter(|(k, b)| b.among_toolbars() && *k < app.extra_palettes.len())
        .filter_map(|(k, b)| Some((b.toolbar_row?, k)))
        .collect();
    bands.sort();
    bands
}

/// The palette bar `k` among the toolbars, over their row `row`: a band
/// across the window. Where it was drawn goes to `app.palette_bands` for
/// the drops of a dragged palette bar.
pub fn toolbar_band(app: &mut App, ui: &mut Ui, k: usize, row: u32) {
    let which = Row::Extra(k);
    let resp = egui::Panel::top(egui::Id::new(("palette_extra", which)))
        .exact_size(crate::ui::palette::thickness(app, which))
        .resizable(false)
        .show_separator_line(true)
        .frame(egui::Frame::new().fill(Tokens::panel()))
        .show(ui, |ui| crate::ui::palette::strip(app, ui, which, false));
    app.palette_bands.push((row, resp.response.rect));
}

/// Where a palette bar dropped at `p` goes among the toolbars along the
/// top, and the line that marks it: near the line between two rows of
/// toolbars, or the top or bottom line of them all (within
/// [`ROW_EDGE`]), that place; over a palette bar among them, its place.
/// `None` elsewhere: over the middle of a row the bar docks as before.
///
/// [`ROW_EDGE`]: crate::ui::bars::ROW_EDGE
pub fn toolbar_target(
    rows: &[crate::ui::bars::RowSpot],
    bands: &[(u32, Rect)],
    p: Pos2,
) -> Option<(u32, Rect)> {
    let line =
        |r: Rect, y: f32| Rect::from_min_max(pos2(r.left(), y - 3.0), pos2(r.right(), y + 3.0));
    if let Some((row, r)) = bands.iter().find(|(_, r)| r.contains(p)) {
        return Some((*row, line(*r, r.top())));
    }
    let edge = crate::ui::bars::ROW_EDGE;
    let spot = rows
        .iter()
        .find(|s| s.dock == Dock::Top && s.rect.contains(p))?;
    let row = u32::try_from(spot.row).unwrap_or(crate::settings::MAX_TOOLBAR_ROW);
    if p.y < spot.rect.top() + edge {
        return Some((row, line(spot.rect, spot.rect.top())));
    }
    if p.y > spot.rect.bottom() - edge {
        return Some((row.saturating_add(1), line(spot.rect, spot.rect.bottom())));
    }
    None
}

/// The name a bar shows: its palette's (as the user named it).
pub fn bar_title(app: &App, which: Row) -> String {
    match which {
        Row::Main => app
            .visible_palettes
            .first()
            .map(|i| app.palette_name(*i))
            .unwrap_or_else(|| tr("bars.palette")),
        Row::Document => app.document_palette_name(),
        Row::Extra(k) => match app.extra_palettes.get(k) {
            Some(r) if r.document => app.document_palette_name(),
            Some(r) => app.palette_name(r.index),
            None => String::new(),
        },
    }
}

/// The id of the palette `which` shows (as the names are kept by).
pub fn palette_id_of(app: &App, which: Row) -> Option<String> {
    let index = match which {
        Row::Main => *app.visible_palettes.first()?,
        Row::Document => return Some(DOCUMENT_PALETTE.to_string()),
        Row::Extra(k) => {
            let r = app.extra_palettes.get(k)?;
            if r.document {
                return Some(DOCUMENT_PALETTE.to_string());
            }
            r.index
        }
    };
    app.palettes.get(index).map(crate::settings::palette_id)
}

/// The palette menu's items for the strip `which`: Swatch Size, Rows and
/// Position.
pub fn strip_items(app: &mut App, ui: &mut Ui, which: Row) {
    use crate::ui::menus::{item_label, radio, sub};
    let prefs = crate::ui::palette::strip_prefs(app, which);
    sub(ui, "palette.size", |ui| {
        for (size, key) in SWATCH_STEPS {
            if radio(ui, key, "", prefs.swatch == size) {
                set_swatch(app, which, size);
            }
        }
        // A size between the named ones (Ctrl+wheel steps finer).
        if crate::settings::swatch_step_of(prefs.swatch).is_none() {
            let label =
                crate::i18n::trf("palette.size_custom", &[("n", &prefs.swatch.to_string())]);
            item_label(ui, &label, "", false);
        }
    });
    sub(ui, "palette.rows", |ui| {
        let floating = prefs.dock == Dock::Floating;
        let now = if floating {
            Some(crate::ui::palette::lines_of(app, which) as u8)
        } else {
            Some(prefs.rows)
        };
        for n in 1..=MAX_ROWS {
            let key = format!("palette.rows_{n}");
            if radio(ui, &key, "", now == Some(n)) {
                set_rows(app, which, n);
            }
        }
        if radio(
            ui,
            "palette.rows_fit",
            "",
            !floating && prefs.rows == ROWS_FIT,
        ) {
            set_rows(app, which, ROWS_FIT);
        }
    });
    sub(ui, "palette.position", |ui| {
        let locked = app.settings.bars.locked;
        let among = among_toolbars(app, which);
        for (dock, key) in PLACES {
            let on = prefs.dock == dock && !(dock == Dock::Top && among);
            if crate::ui::menus::radio_if(ui, key, "", on, !locked) {
                set_dock(app, which, dock, None);
            }
        }
        // A palette bar also lies among the toolbars along the top (the
        // default palette has its own places at the top).
        if which != Row::Main
            && crate::ui::menus::radio_if(ui, "palette.among_toolbars", "", among, !locked)
        {
            set_among_toolbars(app, which, crate::settings::MAX_TOOLBAR_ROW);
        }
    });
}

/// Open as Palette Bar: every palette not shown yet, and the document
/// palette.
pub fn open_bar_items(app: &mut App, ui: &mut Ui) {
    use crate::ui::menus::{item_label, sub};
    sub(ui, "palette.open_bar", |ui| {
        let doc_open = app.extra_palettes.iter().any(|r| r.document);
        let name = app.document_palette_name();
        if item_label(ui, &name, "", !doc_open) {
            app.open_palette_bar(DOCUMENT_PALETTE);
        }
        crate::ui::menus::sep(ui);
        for i in 0..app.palettes.len() {
            let shown = app.visible_palettes.contains(&i);
            let name = app.palette_name(i);
            if item_label(ui, &name, "", !shown) {
                let id = crate::settings::palette_id(&app.palettes[i]);
                app.open_palette_bar(&id);
            }
        }
    });
}

/// Close the palette bar `k`.
pub fn close_bar(app: &mut App, k: usize) {
    if k < app.settings.palette_bars.len() {
        app.settings.palette_bars.remove(k);
        if app
            .palette_expanded
            .is_some_and(|w| matches!(w, Row::Extra(_)))
        {
            app.palette_expanded = None;
        }
        app.rebuild_palette();
    }
}

/// Start renaming the palette `which` shows.
pub fn start_rename(app: &mut App, which: Row) {
    if let Some(id) = palette_id_of(app, which) {
        let name = bar_title(app, which);
        app.palette_rename = Some((id, name));
    }
}

/// The Rename Palette window: the name, OK, Reset Name and Cancel.
pub fn rename_window(app: &mut App, ctx: &egui::Context) {
    let Some((id, mut name)) = app.palette_rename.take() else {
        return;
    };
    let mut keep = true;
    let mut done: Option<String> = None;
    egui::Window::new(tr("palette.rename_title"))
        .id(egui::Id::new("palette_rename"))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.label(tr("palette.rename_label"));
            let resp = ui.add(egui::TextEdit::singleline(&mut name).desired_width(260.0));
            if !resp.has_focus() && !resp.lost_focus() {
                resp.request_focus();
            }
            let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button(tr("dialog.ok")).clicked() || enter {
                    done = Some(name.clone());
                }
                if ui
                    .button(tr("palette.reset_name"))
                    .rich_tip(tr("palette.reset_name_tip"))
                    .clicked()
                {
                    done = Some(String::new());
                }
                if ui.button(tr("dialog.cancel")).clicked()
                    || ui.input(|i| i.key_pressed(egui::Key::Escape))
                {
                    keep = false;
                }
            });
        });
    if let Some(n) = done {
        app.rename_palette(&id, &n);
        app.settings.save();
    } else if keep {
        app.palette_rename = Some((id, name));
    }
}

/// New Palette: ask for the new palette's name ("Palette 1", "Palette
/// 2"... at first).
pub fn start_new_palette(app: &mut App) {
    app.palette_new = Some(app.next_palette_name());
}

/// The New Palette window: the name, OK and Cancel. OK makes the empty
/// palette under My Palettes and shows it in a bar of its own.
pub fn new_palette_window(app: &mut App, ctx: &egui::Context) {
    let Some(mut name) = app.palette_new.take() else {
        return;
    };
    let mut keep = true;
    let mut done = false;
    egui::Window::new(tr("palette.new_title"))
        .id(egui::Id::new("palette_new"))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.label(tr("palette.new_label"));
            let resp = ui.add(egui::TextEdit::singleline(&mut name).desired_width(260.0));
            if !resp.has_focus() && !resp.lost_focus() {
                resp.request_focus();
            }
            let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(tr("palette.new_help"))
                    .small()
                    .color(Tokens::text_dim()),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button(tr("dialog.ok")).clicked() || enter {
                    done = true;
                }
                if ui.button(tr("dialog.cancel")).clicked()
                    || ui.input(|i| i.key_pressed(egui::Key::Escape))
                {
                    keep = false;
                }
            });
        });
    if done {
        app.new_palette(&name);
        app.settings.save();
    } else if keep {
        app.palette_new = Some(name);
    }
}

/// The grip of a docked palette bar: dragging it moves the bar, a right
/// click opens the palette menu (the strip's own).
pub fn grip_zone(app: &mut App, ui: &mut Ui, k: usize, r: Rect) {
    let resp = ui
        .interact(
            r,
            egui::Id::new(("palette_bar_grip", k)),
            Sense::click_and_drag(),
        )
        .rich_tip(crate::ui::tips::pair(
            "palette.bar",
            if app.settings.bars.locked {
                "bars.grip_locked_desc"
            } else {
                "bars.grip_desc"
            },
        ));
    if app.settings.bars.locked {
        return;
    }
    let resp = resp.on_hover_cursor(egui::CursorIcon::Move);
    if resp.drag_started() {
        app.palette_bar_drag = Some(PaletteBarDrag {
            bar: k,
            grab: vec2(12.0, TITLE_H / 2.0),
            last: resp.interact_pointer_pos(),
        });
    }
}

/// The side the document palette row lies at: over the drawing while the
/// default palette is docked at the top, else under it.
pub fn document_row_dock(app: &App) -> Dock {
    if app.settings.bars.dock(Bar::Palette) == Dock::Top {
        Dock::Top
    } else {
        Dock::Bottom
    }
}

/// Take the document palette row out into a palette bar of its own, at
/// the row's side, so it moves by itself and the default palette stays
/// where it is. The bar showing the document palette, if one is open
/// already; nothing when it cannot open.
pub fn detach_document_row(app: &mut App) -> Option<usize> {
    if let Some(k) = app.extra_palettes.iter().position(|r| r.document) {
        app.settings.palette.show_document = false;
        return Some(k);
    }
    let dock = document_row_dock(app);
    app.open_palette_bar(DOCUMENT_PALETTE);
    let k = app.extra_palettes.iter().position(|r| r.document)?;
    if let Some(b) = app.settings.palette_bars.get_mut(k) {
        b.dock = dock;
        b.rows = 1;
    }
    app.settings.palette.show_document = false;
    Some(k)
}

/// The document palette bar `k` dropped at `dock`: at the row's own side
/// (with the default palette shown) it goes back to being the row under
/// the drawing. Says whether it did.
fn redock_document_row(app: &mut App, k: usize, dock: Dock) -> bool {
    let document = app.extra_palettes.get(k).is_some_and(|r| r.document);
    if !document || !app.settings.palette.show_palette || dock != document_row_dock(app) {
        return false;
    }
    close_bar(app, k);
    app.settings.palette.show_document = true;
    true
}

/// The grip of the document palette row: dragging it takes the row out
/// into a palette bar of its own and moves that bar (never the default
/// palette); a right click opens the bars' menu.
pub fn document_grip_zone(app: &mut App, ui: &mut Ui, r: Rect) {
    let resp = ui
        .interact(
            r,
            egui::Id::new("document_palette_grip"),
            Sense::click_and_drag(),
        )
        .rich_tip(crate::ui::tips::pair(
            "palette.bar",
            if app.settings.bars.locked {
                "bars.grip_locked_desc"
            } else {
                "bars.grip_desc"
            },
        ));
    if !app.settings.bars.locked {
        let resp = resp.clone().on_hover_cursor(egui::CursorIcon::Move);
        if resp.drag_started() {
            if let Some(k) = detach_document_row(app) {
                app.palette_bar_drag = Some(PaletteBarDrag {
                    bar: k,
                    grab: vec2(12.0, TITLE_H / 2.0),
                    last: resp.interact_pointer_pos(),
                });
            }
        }
    }
    crate::ui::bars::bar_menu(app, &resp, Bar::Palette);
}

/// A bar docked at a side, or a place between them, as the frame drew
/// them: in order from each edge inwards (see [`side_target`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SideMark {
    /// The palette bars at `slot` of `dock` come here.
    Place(Dock, BarSlot),
    /// A bar drawn at `dock` over `rect`: palette bar `bar`, or another
    /// bar (the toolbox, the default palette, a docker column).
    Bar {
        dock: Dock,
        rect: Rect,
        bar: Option<usize>,
    },
}

/// Note a bar drawn at `dock` over `rect` (not a palette bar of its own).
pub fn mark_bar(app: &mut App, dock: Dock, rect: Rect) {
    if dock != Dock::Floating {
        app.side_marks.push(SideMark::Bar {
            dock,
            rect,
            bar: None,
        });
    }
}

/// Where a dragged palette bar dropped beside another bar goes, in the
/// order the bars are drawn from the edge inwards.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SideDrop {
    /// Just outside or inside palette bar `k` (`outer`), in its place.
    Beside { k: usize, outer: bool },
    /// The first (`first`) or last palette bar of `slot` at `dock`.
    At {
        dock: Dock,
        slot: BarSlot,
        first: bool,
    },
    /// Over the dragged bar itself: it stays.
    Stay,
}

/// Where a palette bar (`dragged`) let go at `p` goes when `p` lies over
/// a bar docked at a side, and the line that shows it: on the half of a
/// bar towards the window's edge it goes just outside that bar, on the
/// other half just inside it. Beside a palette bar it takes that bar's
/// place in the order; beside another bar the place before or after it
/// (`marks` says which places lie where). `None` over no docked bar.
pub fn side_target(marks: &[SideMark], p: Pos2, dragged: usize) -> Option<(SideDrop, Rect)> {
    let (i, dock, rect, bar) = marks.iter().enumerate().find_map(|(i, m)| match *m {
        SideMark::Bar { dock, rect, bar } if rect.contains(p) => Some((i, dock, rect, bar)),
        _ => None,
    })?;
    if bar == Some(dragged) {
        return Some((SideDrop::Stay, rect));
    }
    let outer = match dock {
        Dock::Left => p.x < rect.center().x,
        Dock::Right => p.x > rect.center().x,
        Dock::Top => p.y < rect.center().y,
        _ => p.y > rect.center().y,
    };
    // The edge of the bar the dropped one will touch: the left or top
    // one is the outer edge at the left and the top.
    const LINE: f32 = 4.0;
    let low = outer == matches!(dock, Dock::Left | Dock::Top);
    let line = if dock.horizontal() {
        let y = if low { rect.top() } else { rect.bottom() };
        Rect::from_min_max(
            pos2(rect.left(), y - LINE / 2.0),
            pos2(rect.right(), y + LINE / 2.0),
        )
    } else {
        let x = if low { rect.left() } else { rect.right() };
        Rect::from_min_max(
            pos2(x - LINE / 2.0, rect.top()),
            pos2(x + LINE / 2.0, rect.bottom()),
        )
    };
    if let Some(k) = bar {
        return Some((SideDrop::Beside { k, outer }, line));
    }
    let place = |m: &SideMark| match *m {
        SideMark::Place(d, s) if d == dock => Some(s),
        _ => None,
    };
    let drop = if outer {
        let slot = marks[..i]
            .iter()
            .rev()
            .find_map(place)
            .unwrap_or(BarSlot::Outer);
        SideDrop::At {
            dock,
            slot,
            first: false,
        }
    } else {
        let slot = marks[i + 1..]
            .iter()
            .find_map(place)
            .unwrap_or(BarSlot::Drawing);
        SideDrop::At {
            dock,
            slot,
            first: true,
        }
    };
    Some((drop, line))
}

/// Put palette bar `k` where `drop` says, among the bars of a side: the
/// bars of one place are drawn in their order in the preferences (along
/// the bottom from the bottom up, see [`docked`]), so the bar moves in
/// that list. Kept in the preferences and in workspace files.
pub fn place_bar(app: &mut App, k: usize, drop: SideDrop) {
    if k >= app.settings.palette_bars.len() {
        return;
    }
    let (dock, slot) = match drop {
        SideDrop::Stay => return,
        SideDrop::At { dock, slot, .. } => (dock, slot),
        SideDrop::Beside { k: j, .. } => match app.settings.palette_bars.get(j) {
            Some(b) if j != k => (b.dock, b.slot_or_default()),
            _ => return,
        },
    };
    let mut bars = std::mem::take(&mut app.settings.palette_bars);
    let mut moved = bars.remove(k);
    moved.dock = dock;
    moved.toolbar_row = None;
    moved.slot = Some(slot);
    // Along the bottom the list is drawn from its end.
    let reversed = dock == Dock::Bottom;
    let same: Vec<usize> = bars
        .iter()
        .enumerate()
        .filter(|(_, b)| b.dock == dock && !b.among_toolbars() && b.slot_or_default() == slot)
        .map(|(i, _)| i)
        .collect();
    let end = bars.len();
    let before_first = same.first().copied().unwrap_or(end);
    let after_last = same.last().map_or(end, |i| i + 1);
    let at = match drop {
        SideDrop::Beside { k: j, outer } => {
            let j = if j > k { j - 1 } else { j };
            if outer != reversed {
                j
            } else {
                j + 1
            }
        }
        SideDrop::At { first, .. } => {
            if first != reversed {
                before_first
            } else {
                after_last
            }
        }
        SideDrop::Stay => return,
    };
    bars.insert(at.min(end), moved);
    app.settings.palette_bars = bars;
    if app
        .palette_expanded
        .is_some_and(|w| matches!(w, Row::Extra(_)))
    {
        app.palette_expanded = None;
    }
    app.rebuild_palette();
}

/// The palette bars docked at `slot` of `dock`, a panel each (a row along
/// the top or the bottom, a column at the left or the right), in order;
/// the place itself is noted for the drops of a dragged bar.
pub fn docked(app: &mut App, ui: &mut Ui, dock: Dock, slot: BarSlot) {
    if dock == Dock::Floating {
        return;
    }
    app.side_marks.push(SideMark::Place(dock, slot));
    // Those among the toolbars come with the toolbars (`toolbar_band`).
    let mut bars: Vec<usize> = (0..app.extra_palettes.len())
        .filter(|k| {
            app.settings.palette_bars.get(*k).is_some_and(|b| {
                b.dock == dock && !b.among_toolbars() && b.slot_or_default() == slot
            })
        })
        .collect();
    // Along the bottom a panel added later lies above the earlier ones:
    // the last bar goes first, so the bars read from the top down in
    // their order (a new palette's bar under the others).
    if dock == Dock::Bottom {
        bars.reverse();
    }
    for k in bars {
        let which = Row::Extra(k);
        let id = egui::Id::new(("palette_extra", which));
        let panel = match dock {
            Dock::Top => egui::Panel::top(id),
            Dock::Bottom => egui::Panel::bottom(id),
            Dock::Left => egui::Panel::left(id),
            _ => egui::Panel::right(id),
        };
        let rect = panel
            .exact_size(crate::ui::palette::thickness(app, which))
            .resizable(false)
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(Tokens::panel()))
            .show(ui, |ui| {
                crate::ui::palette::strip(app, ui, which, !dock.horizontal())
            })
            .response
            .rect;
        app.side_marks.push(SideMark::Bar {
            dock,
            rect,
            bar: Some(k),
        });
    }
}

/// The size of the floating window of `which`, its title and border
/// included.
pub fn floating_window_size(app: &App, which: Row) -> Vec2 {
    let (_, body) = crate::ui::palette::floating_body(app, which);
    vec2(body.x + 2.0 * BORDER, body.y + TITLE_H + 2.0 * BORDER)
}

/// The id of the floating window of `which`.
pub fn window_id(which: Row) -> egui::Id {
    egui::Id::new(("floating_palette", which))
}

/// The floating palettes: the default one when it floats, then each
/// floating palette bar.
pub fn floating(app: &mut App, ctx: &egui::Context) {
    if Bar::Palette.visible(app) && app.settings.bars.dock(Bar::Palette) == Dock::Floating {
        floating_window(app, ctx, Row::Main);
    }
    for k in 0..app.extra_palettes.len() {
        if app
            .settings
            .palette_bars
            .get(k)
            .is_some_and(|b| b.dock == Dock::Floating)
        {
            floating_window(app, ctx, Row::Extra(k));
        }
    }
}

fn float_pos(app: &App, which: Row) -> Pos2 {
    match which {
        Row::Extra(k) => app
            .settings
            .palette_bars
            .get(k)
            .map_or(pos2(320.0, 420.0), |b| pos2(b.pos[0], b.pos[1])),
        _ => app.settings.bars.pos(Bar::Palette),
    }
}

fn set_float_size(app: &mut App, which: Row, size: [f32; 2]) {
    match which {
        Row::Extra(k) => {
            if let Some(b) = app.settings.palette_bars.get_mut(k) {
                b.float_size = size;
            }
        }
        _ => app.settings.palette.float_size = size,
    }
}

fn floating_window(app: &mut App, ctx: &egui::Context, which: Row) {
    let screen = ctx.content_rect();
    let (flow, body) = crate::ui::palette::floating_body(app, which);
    let mut pos = float_pos(app, which);
    pos.x = pos
        .x
        .clamp(screen.left(), (screen.right() - 60.0).max(screen.left()));
    pos.y = pos
        .y
        .clamp(screen.top(), (screen.bottom() - TITLE_H).max(screen.top()));
    egui::Area::new(window_id(which))
        .fixed_pos(pos)
        .order(egui::Order::Middle)
        .show(ctx, |ui| {
            let frame = egui::Frame::new()
                .fill(Tokens::panel())
                .stroke(Stroke::new(BORDER, Tokens::window_border()))
                .shadow(egui::epaint::Shadow {
                    offset: [0, 3],
                    blur: 10,
                    spread: 0,
                    color: Color32::from_black_alpha(50),
                })
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    ui.set_width(body.x);
                    title(app, ui, which, body.x);
                    let (r, _) = ui.allocate_exact_size(body, Sense::hover());
                    let mut child = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(r)
                            .layout(egui::Layout::top_down(egui::Align::Min)),
                    );
                    child.set_clip_rect(r.intersect(ui.clip_rect()));
                    crate::ui::palette::strip(app, &mut child, which, flow.vertical);
                    r
                });
            let body_rect = frame.inner;
            resize_handles(app, ui, which, frame.response.rect, body_rect);
        });
}

/// The floating window's title: the palette's name and a close button.
/// Dragging it moves the palette (docking it near an edge), a double
/// click docks it at the bottom, a right click opens the palette menu.
fn title(app: &mut App, ui: &mut Ui, which: Row, width: f32) {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, TITLE_H), Sense::click_and_drag());
    let name = bar_title(app, which);
    let galley = crate::ui::bars::title_galley(ui, &name, crate::ui::bars::title_room(width));
    let p = ui.painter();
    p.rect_filled(rect, 0.0, Tokens::tab_strip());
    let cut = galley.elided;
    p.galley(
        pos2(rect.left() + 6.0, rect.center().y - galley.size().y / 2.0),
        galley,
        Tokens::text(),
    );
    let resp = if cut { resp.rich_tip(&name) } else { resp };
    let close = Rect::from_center_size(
        pos2(rect.right() - TITLE_H / 2.0, rect.center().y),
        vec2(TITLE_H, TITLE_H),
    );
    let close_resp = ui
        .interact(
            close,
            egui::Id::new(("palette_bar_close", which)),
            Sense::click(),
        )
        .rich_tip(tr("palette.close"));
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
        match which {
            Row::Extra(k) => close_bar(app, k),
            _ => Bar::Palette.set_visible(app, false),
        }
        return;
    }
    if resp.double_clicked() {
        set_dock(app, which, Dock::Bottom, None);
        return;
    }
    if resp.drag_started() && !app.settings.bars.locked {
        let grab = resp
            .interact_pointer_pos()
            .map(|q| q - rect.min)
            .unwrap_or(vec2(12.0, TITLE_H / 2.0));
        let last = resp.interact_pointer_pos();
        match which {
            Row::Extra(k) => {
                app.palette_bar_drag = Some(PaletteBarDrag { bar: k, grab, last });
            }
            _ => {
                app.bar_drag = Some(crate::ui::bars::BarDrag {
                    bar: Bar::Palette,
                    grab,
                    last,
                });
            }
        }
    }
    egui::Popup::context_menu(&resp)
        .id(egui::Id::new(("palette_title_menu", which)))
        .style(crate::ui::menus::menu_popup_style)
        .show(|ui| crate::ui::menus::body(ui, |ui| crate::ui::palette::menu(app, ui, which)));
}

/// The right and bottom edges and the corner between them of a floating
/// palette's window: dragging one asks for the room up to the pointer,
/// and the swatches flow into it (see `palette::floating_flow`).
fn resize_handles(app: &mut App, ui: &mut Ui, which: Row, window: Rect, body: Rect) {
    const GRAB: f32 = 5.0;
    const OUT: f32 = 3.0;
    const CORNER: f32 = 12.0;
    let (l, t, r, b) = (window.left(), window.top(), window.right(), window.bottom());
    let handles = [
        (
            true,
            true,
            Rect::from_min_max(pos2(r - CORNER, b - CORNER), pos2(r + OUT, b + OUT)),
            egui::CursorIcon::ResizeNwSe,
        ),
        (
            true,
            false,
            Rect::from_min_max(pos2(r - GRAB, t + TITLE_H), pos2(r + OUT, b - CORNER)),
            egui::CursorIcon::ResizeHorizontal,
        ),
        (
            false,
            true,
            Rect::from_min_max(pos2(l, b - GRAB), pos2(r - CORNER, b + OUT)),
            egui::CursorIcon::ResizeVertical,
        ),
    ];
    let now = crate::ui::palette::strip_prefs(app, which).float_size;
    for (k, (right, bottom, zone, cursor)) in handles.into_iter().enumerate() {
        let resp = ui.interact(
            zone,
            egui::Id::new(("palette_resize", which, k)),
            Sense::drag(),
        );
        if resp.hovered() || resp.dragged() {
            ui.ctx().set_cursor_icon(cursor);
        }
        let Some(p) = resp.interact_pointer_pos().filter(|_| resp.dragged()) else {
            continue;
        };
        let mut asked = now;
        if right {
            asked[0] = (p.x - body.left()).max(1.0);
        } else {
            asked[0] = body.width();
        }
        if bottom {
            asked[1] = (p.y - body.top()).max(1.0);
        } else {
            asked[1] = body.height();
        }
        set_float_size(app, which, asked);
        ui.ctx().request_repaint();
        break;
    }
}

/// While a palette bar is dragged: the place it would go, highlighted;
/// on release it goes there.
pub fn drag_overlay(app: &mut App, ctx: &egui::Context, menu_h: f32) {
    let Some(mut drag) = app.palette_bar_drag else {
        return;
    };
    if drag.bar >= app.settings.palette_bars.len() {
        app.palette_bar_drag = None;
        return;
    }
    let screen = ctx.content_rect();
    let (pointer, down) = ctx.input(|i| (i.pointer.latest_pos(), i.pointer.primary_down()));
    let Some(p) = pointer.or(drag.last) else {
        if !down {
            app.palette_bar_drag = None;
        }
        return;
    };
    drag.last = Some(p);
    app.palette_bar_drag = Some(drag);
    ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Tooltip,
        egui::Id::new("palette_bar_drag_overlay"),
    ));
    // On the line between two rows of toolbars along the top: a row of
    // its own there, among them.
    if let Some((row, mark)) = toolbar_target(&app.bar_rows, &app.palette_bands, p) {
        painter.rect_filled(mark, 1.0, Color32::from_rgb(0x1E, 0x78, 0xD7));
        if !down {
            app.palette_bar_drag = None;
            set_among_toolbars(app, Row::Extra(drag.bar), row);
        }
        return;
    }
    // Over a bar docked at a side (the toolbox, the default palette, a
    // docker column, another palette bar): beside it, the line showing on
    // which side (live test of 0.41).
    if let Some((drop, mark)) = side_target(&app.side_marks, p, drag.bar) {
        if drop == SideDrop::Stay {
            painter.rect_stroke(
                mark,
                2.0,
                Stroke::new(1.5, Color32::from_rgb(0x1E, 0x78, 0xD7)),
                egui::StrokeKind::Inside,
            );
        } else {
            painter.rect_filled(mark, 1.0, Color32::from_rgb(0x1E, 0x78, 0xD7));
        }
        if !down {
            app.palette_bar_drag = None;
            place_bar(app, drag.bar, drop);
        }
        return;
    }
    let dock = crate::ui::bars::target_dock(Bar::Palette, screen, menu_h, p);
    let zone = drop_zone(app, drag, dock, screen, menu_h, p);
    painter.rect(
        zone,
        2.0,
        Color32::from_rgba_unmultiplied(0x1E, 0x78, 0xD7, 46),
        Stroke::new(1.5, Color32::from_rgb(0x1E, 0x78, 0xD7)),
        egui::StrokeKind::Inside,
    );
    if !down {
        app.palette_bar_drag = None;
        if !redock_document_row(app, drag.bar, dock) {
            set_dock(app, Row::Extra(drag.bar), dock, Some(p - drag.grab));
        }
    }
}

/// Where `drop` of a palette bar at `dock` would lie.
pub fn drop_zone(
    app: &App,
    drag: PaletteBarDrag,
    dock: Dock,
    screen: Rect,
    menu_h: f32,
    p: Pos2,
) -> Rect {
    let which = Row::Extra(drag.bar);
    let side = crate::ui::palette::swatch_side_of(app, which);
    let lines = app
        .settings
        .palette_bars
        .get(drag.bar)
        .and_then(|b| crate::settings::rows_of(b.rows))
        .unwrap_or(1);
    let thick = crate::ui::palette::thickness_for(side, lines);
    let top = screen.top() + menu_h;
    match dock {
        Dock::Left => Rect::from_min_max(
            pos2(screen.left(), top),
            pos2(screen.left() + thick, screen.bottom()),
        ),
        Dock::Right => Rect::from_min_max(pos2(screen.right() - thick, top), screen.max),
        Dock::Top => {
            Rect::from_min_max(pos2(screen.left(), top), pos2(screen.right(), top + thick))
        }
        Dock::Bottom => {
            Rect::from_min_max(pos2(screen.left(), screen.bottom() - thick), screen.max)
        }
        Dock::Floating => Rect::from_min_size(p - drag.grab, floating_window_size(app, which)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::palette::{floating_flow, min_len, thickness, Flow};

    /// The keys of every item `menu` shows, walked with the menu replay
    /// (each walk clicks one item, so a fresh app each time).
    fn menu_keys(which: Row, setup: impl Fn(&mut App)) -> Vec<String> {
        use crate::ui::menus::replay;
        let ctx = crate::theme::ui_context();
        let mut keys = Vec::new();
        for target in 0..200 {
            let mut app = App::headless();
            setup(&mut app);
            replay::start(target);
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                crate::ui::palette::menu(&mut app, ui, which)
            });
            out.textures_delta.clear();
            let (hit, seen) = replay::finish();
            if let Some(k) = hit {
                keys.push(k);
            }
            if target >= seen {
                break;
            }
        }
        keys
    }

    /// Clicks the item `key` of `which`'s menu in `app`; the item is
    /// found in a fresh app with a bar open when `which` is one (the
    /// menu's rows are the same).
    fn click_item(app: &mut App, which: Row, key: &str) {
        use crate::ui::menus::replay;
        let ctx = crate::theme::ui_context();
        for target in 0..200 {
            let mut probe = App::headless();
            if let Row::Extra(_) = which {
                let id = crate::settings::palette_id(&probe.palettes[3]);
                probe.open_palette_bar(&id);
            }
            replay::start(target);
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                crate::ui::palette::menu(&mut probe, ui, which)
            });
            out.textures_delta.clear();
            let (hit, seen) = replay::finish();
            if hit.as_deref() == Some(key) {
                replay::start(target);
                let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                    crate::ui::palette::menu(app, ui, which)
                });
                out.textures_delta.clear();
                replay::finish();
                return;
            }
            if target >= seen {
                break;
            }
        }
        panic!("no item {key} in the menu");
    }

    #[test]
    fn the_palette_menu_offers_five_sizes_rows_one_to_eight_and_places() {
        let keys = menu_keys(Row::Main, |_| {});
        for (_, key) in SWATCH_STEPS {
            assert!(keys.iter().any(|k| k == key), "{key} in {keys:?}");
        }
        for n in 1..=8 {
            let key = format!("palette.rows_{n}");
            assert!(keys.contains(&key), "{key} in {keys:?}");
        }
        for key in [
            "palette.rows_fit",
            "bars.top",
            "bars.bottom",
            "bars.left",
            "bars.right",
            "palette.float",
            "palette.rename",
        ] {
            assert!(keys.iter().any(|k| k == key), "{key} in {keys:?}");
        }
        // A palette bar's own menu has them too, and Close Palette.
        let bar_keys = menu_keys(Row::Extra(0), |app| {
            let id = crate::settings::palette_id(&app.palettes[2]);
            app.open_palette_bar(&id);
        });
        assert!(bar_keys.iter().any(|k| k == "palette.close"));
        assert!(bar_keys.iter().any(|k| k == "palette.rows_8"));
        assert!(bar_keys.iter().any(|k| k == "palette.size_xl"));
    }

    /// A palette bar's menu puts it among the toolbars along the top,
    /// under them all; Dock at Top puts it back beside the drawing. The
    /// default palette's menu has no such place. A hand-edited file
    /// keeps the place only at the top.
    #[test]
    fn the_menu_puts_a_bar_among_the_toolbars() {
        let mut app = App::headless();
        let id = crate::settings::palette_id(&app.palettes[3]);
        app.open_palette_bar(&id);
        click_item(&mut app, Row::Extra(0), "palette.among_toolbars");
        let bar = &app.settings.palette_bars[0];
        assert_eq!(bar.dock, Dock::Top);
        assert_eq!(bar.toolbar_row, Some(crate::settings::MAX_TOOLBAR_ROW));
        assert!(among_toolbars(&app, Row::Extra(0)));
        assert_eq!(
            toolbar_bands(&app),
            vec![(crate::settings::MAX_TOOLBAR_ROW, 0)]
        );
        click_item(&mut app, Row::Extra(0), "bars.top");
        let bar = &app.settings.palette_bars[0];
        assert_eq!((bar.dock, bar.toolbar_row), (Dock::Top, None));
        assert!(toolbar_bands(&app).is_empty());
        assert!(!menu_keys(Row::Main, |_| {}).contains(&"palette.among_toolbars".to_string()));
        let mut odd = crate::settings::PaletteBarPrefs {
            dock: Dock::Left,
            toolbar_row: Some(2),
            ..Default::default()
        };
        odd.sanitize();
        assert_eq!(odd.toolbar_row, None);
        let mut far = crate::settings::PaletteBarPrefs {
            dock: Dock::Top,
            toolbar_row: Some(u32::MAX),
            ..Default::default()
        };
        far.sanitize();
        assert_eq!(far.toolbar_row, Some(crate::settings::MAX_TOOLBAR_ROW));
    }

    /// A palette bar dragged near the line between two rows of toolbars
    /// goes between them; near the top or the bottom of them all, over or
    /// under them; over a palette bar among them, to its place; over the
    /// middle of a row or anywhere else, nowhere among them.
    #[test]
    fn a_drop_near_the_line_between_toolbar_rows_goes_there() {
        use crate::ui::bars::{Bar, RowSpot, ROW_EDGE};
        let spot = |row: usize, top: f32, h: f32| RowSpot {
            dock: Dock::Top,
            row,
            rect: Rect::from_min_size(pos2(0.0, top), vec2(1600.0, h)),
            slots: vec![(Bar::Standard, Rect::NOTHING)],
        };
        let rows = [spot(0, 50.0, 36.0), spot(1, 86.0, 53.0)];
        let at = |y: f32| toolbar_target(&rows, &[], pos2(400.0, y)).map(|(r, _)| r);
        assert_eq!(at(51.0), Some(0));
        assert_eq!(at(84.0), Some(1));
        assert_eq!(at(87.0), Some(1));
        assert_eq!(at(138.0), Some(2));
        assert_eq!(at(70.0), None, "the middle of a row");
        assert_eq!(at(110.0), None);
        assert_eq!(at(300.0), None, "off the rows");
        assert_eq!(at(86.0 + ROW_EDGE + 1.0), None);
        // The mark is a line across the rows at the place.
        let (_, mark) = toolbar_target(&rows, &[], pos2(400.0, 84.0)).expect("a place");
        assert!(mark.contains(pos2(10.0, 86.0)) && mark.width() == 1600.0);
        // Over a palette bar among the toolbars: its place.
        let band = Rect::from_min_size(pos2(0.0, 139.0), vec2(1600.0, 30.0));
        let bands = [(2, band)];
        assert_eq!(
            toolbar_target(&rows, &bands, pos2(400.0, 150.0)).map(|(r, _)| r),
            Some(2)
        );
        // Rows at the bottom take no palette bar among them.
        let bottom = RowSpot {
            dock: Dock::Bottom,
            ..spot(0, 900.0, 36.0)
        };
        assert_eq!(toolbar_target(&[bottom], &[], pos2(400.0, 901.0)), None);
    }

    #[test]
    fn the_menu_sets_the_rows_size_and_place_of_its_bar() {
        let mut app = App::headless();
        click_item(&mut app, Row::Main, "palette.rows_5");
        assert_eq!(app.settings.palette.rows, 5);
        assert_eq!(thickness(&app, Row::Main), 20.0 + 5.0 + 4.0 * 21.0);
        click_item(&mut app, Row::Main, "palette.size_l");
        assert_eq!(app.settings.palette.swatch_size, 28);
        // A bar of its own keeps its own; the default palette's stay.
        let id = crate::settings::palette_id(&app.palettes[3]);
        app.open_palette_bar(&id);
        click_item(&mut app, Row::Extra(0), "palette.rows_2");
        click_item(&mut app, Row::Extra(0), "palette.size_xs");
        click_item(&mut app, Row::Extra(0), "bars.right");
        let bar = &app.settings.palette_bars[0];
        assert_eq!((bar.rows, bar.swatch_size, bar.dock), (2, 10, Dock::Right));
        assert_eq!(app.settings.palette.rows, 5);
        assert_eq!(app.settings.palette.swatch_size, 28);
        click_item(&mut app, Row::Extra(0), "palette.close");
        assert!(app.settings.palette_bars.is_empty());
        assert!(app.extra_palettes.is_empty());
    }

    #[test]
    fn a_floating_palette_flows_into_rows_or_columns_from_its_size() {
        // Wide and short: one row along it.
        let f = floating_flow([460.0, 25.0], 20.0);
        assert_eq!(
            f,
            Flow {
                vertical: false,
                lines: 1,
                len: 460.0
            }
        );
        // Taller: as many rows as whole swatches fit.
        assert_eq!(floating_flow([460.0, 70.0], 20.0).lines, 3);
        assert_eq!(floating_flow([460.0, 66.0], 20.0).lines, 2);
        // Taller than wide: columns down it.
        let f = floating_flow([100.0, 400.0], 20.0);
        assert!(f.vertical);
        assert_eq!((f.lines, f.len), (4, 400.0));
        // Smaller swatches, more columns in the same width.
        assert_eq!(floating_flow([100.0, 400.0], 10.0).lines, 8);
        // Never shorter than its buttons and two swatches.
        let f = floating_flow([10.0, 5.0], 20.0);
        assert_eq!((f.lines, f.len), (1, min_len(20.0)));
        let f = floating_flow([f32::NAN, f32::INFINITY], 20.0);
        assert!(f.len.is_finite() && f.lines >= 1);
    }

    /// Dragging the floating palette's corner down asks for more rows;
    /// the window grows to hold them, whole.
    #[test]
    fn dragging_a_floating_palette_s_corner_reflows_it() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        let id = crate::settings::palette_id(&app.palettes[2]);
        app.open_palette_bar(&id);
        app.settings.palette_bars[0].dock = Dock::Floating;
        app.settings.palette_bars[0].pos = [100.0, 100.0];
        let screen = Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 800.0));
        let clock = std::cell::Cell::new(0.0);
        let run = |app: &mut App, events: Vec<egui::Event>| {
            clock.set(clock.get() + 0.05);
            let input = egui::RawInput {
                screen_rect: Some(screen),
                events,
                time: Some(clock.get()),
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| floating(app, ui.ctx()));
            out.textures_delta.clear();
        };
        run(&mut app, Vec::new());
        run(&mut app, Vec::new());
        let which = Row::Extra(0);
        let window = ctx
            .memory(|m| m.area_rect(window_id(which)))
            .expect("a window");
        assert_eq!(window.size(), floating_window_size(&app, which));
        assert_eq!(crate::ui::palette::lines_of(&app, which), 1);
        let corner = window.right_bottom() - vec2(2.0, 2.0);
        let to = corner + vec2(0.0, 60.0);
        let press = |p: Pos2, pressed: bool| egui::Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        run(&mut app, vec![egui::Event::PointerMoved(corner)]);
        run(&mut app, vec![press(corner, true)]);
        for k in 1..=6 {
            let p = corner + vec2(0.0, 10.0 * k as f32);
            run(&mut app, vec![egui::Event::PointerMoved(p)]);
        }
        run(&mut app, vec![press(to, false)]);
        run(&mut app, Vec::new());
        let lines = crate::ui::palette::lines_of(&app, which);
        assert_eq!(lines, 3, "{:?}", app.settings.palette_bars[0].float_size);
        let window = ctx
            .memory(|m| m.area_rect(window_id(which)))
            .expect("a window");
        assert_eq!(window.size(), floating_window_size(&app, which));
        assert_eq!(
            window.height(),
            TITLE_H + 2.0 + crate::ui::palette::thickness_for(20.0, 3)
        );
    }

    /// A second palette bar, its place, rows and size, and the palettes'
    /// names come back from a workspace file and from the preferences.
    #[test]
    fn palette_bars_persist_in_workspace_files_and_preferences() {
        let mut app = App::headless();
        let gray = app
            .palettes
            .iter()
            .position(|p| p.key == "palette.grayscale")
            .expect("grayscale");
        let id = crate::settings::palette_id(&app.palettes[gray]);
        app.open_palette_bar(&id);
        app.open_palette_bar(DOCUMENT_PALETTE);
        app.settings.palette_bars[0].dock = Dock::Right;
        app.settings.palette_bars[0].rows = 3;
        app.settings.palette_bars[0].swatch_size = 28;
        app.settings.palette_bars[1].dock = Dock::Floating;
        app.settings.palette_bars[1].pos = [640.0, 220.0];
        app.rename_palette(&id, "Greys");
        let bars = app.settings.palette_bars.clone();
        let file = app.export_workspace(crate::workspace_file::WorkspaceParts::ALL);
        let bytes = file.to_bytes();
        let read = crate::workspace_file::WorkspaceFile::from_bytes(&bytes).expect("read");
        let mut other = App::headless();
        assert!(other.extra_palettes.is_empty());
        other.import_workspace(&read, crate::workspace_file::WorkspaceParts::ALL);
        assert_eq!(other.settings.palette_bars, bars);
        assert_eq!(other.extra_palettes.len(), 2);
        assert_eq!(other.extra_palettes[0].index, gray);
        assert!(other.extra_palettes[1].document);
        assert_eq!(other.visible_palettes, vec![0, gray]);
        assert_eq!(other.palette_name(gray), "Greys");
        // The preferences, saved and read back.
        let json = serde_json::to_string(&app.settings).expect("saved");
        let back: crate::settings::Settings = serde_json::from_str(&json).expect("read");
        let mut again = App::headless();
        again.apply_settings(back);
        assert_eq!(again.settings.palette_bars, bars);
        assert_eq!(again.extra_palettes.len(), 2);
        assert_eq!(again.palette_name(gray), "Greys");
    }

    /// Preferences written before palette bars existed give the one
    /// palette at the bottom, with the rows and swatch size they had.
    #[test]
    fn old_preferences_give_one_palette_at_the_bottom() {
        let old: crate::settings::Settings = serde_json::from_str(
            r#"{"palette":{"rows":2,"swatch_size":24},"bars":{"palette":"bottom"}}"#,
        )
        .expect("old preferences");
        assert!(old.palette_bars.is_empty());
        let mut app = App::headless();
        app.apply_settings(old);
        assert!(app.extra_palettes.is_empty());
        assert!(app.settings.palette_bars.is_empty());
        assert_eq!(app.visible_palettes, vec![0]);
        assert_eq!(app.settings.bars.dock(Bar::Palette), Dock::Bottom);
        assert_eq!(app.settings.palette.float_size, crate::settings::FLOAT_SIZE);
        assert_eq!(thickness(&app, Row::Main), 24.0 + 5.0 + 25.0);
        // And none at all: the first palette, one row of 20 px.
        let none: crate::settings::Settings = serde_json::from_str("{}").expect("empty");
        let mut app = App::headless();
        app.apply_settings(none);
        assert!(app.extra_palettes.is_empty());
        assert_eq!(thickness(&app, Row::Main), crate::ui::palette::ROW_H);
    }

    /// A palette renamed shows its new name everywhere and keeps it in
    /// the preferences; Reset Name gives back its own. Built-in palettes'
    /// own names never change.
    #[test]
    fn a_renamed_palette_keeps_its_name_until_reset() {
        let mut app = App::headless();
        let own = app.palette_name(0);
        let id = crate::settings::palette_id(&app.palettes[0]);
        app.rename_palette(&id, "  House colours ");
        assert_eq!(app.palette_name(0), "House colours");
        assert_eq!(bar_title(&app, Row::Main), "House colours");
        assert_eq!(
            app.palettes[0].display_name(),
            own,
            "the palette is untouched"
        );
        app.rename_palette(DOCUMENT_PALETTE, "Job colours");
        assert_eq!(app.document_palette_name(), "Job colours");
        let json = serde_json::to_string(&app.settings).expect("saved");
        let back: crate::settings::Settings = serde_json::from_str(&json).expect("read");
        let mut again = App::headless();
        again.apply_settings(back);
        assert_eq!(again.palette_name(0), "House colours");
        assert_eq!(again.document_palette_name(), "Job colours");
        again.rename_palette(&id, "");
        assert_eq!(again.palette_name(0), own);
        again.rename_palette(DOCUMENT_PALETTE, &tr("palette.show_document"));
        assert!(again.settings.palette_names.is_empty());
    }

    /// A palette of the user's in a GIMP palette file keeps its new name
    /// in the file, and the file read again names it so.
    #[test]
    fn a_user_palette_file_keeps_its_name() {
        let dir = std::env::temp_dir().join(format!("td_palname_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("brand.gpl");
        std::fs::write(&path, "GIMP Palette\nName: brand\n#\n255 0 0 Red\n").expect("write");
        let mut app = App::headless();
        let p = crate::palette::load_palette(&path).expect("load");
        assert_eq!(p.name, "brand");
        app.add_palette(p);
        let id = path.display().to_string();
        app.rename_palette(&id, "Brand 2026");
        let again = crate::palette::load_palette(&path).expect("load again");
        assert_eq!(again.name, "Brand 2026");
        assert_eq!(again.colors.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod rename_tests {
    use super::*;

    /// The Rename Palette window takes the name typed; OK keeps it.
    #[test]
    fn the_rename_window_takes_the_typed_name() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        start_rename(&mut app, Row::Main);
        let clock = std::cell::Cell::new(0.0);
        let run = |app: &mut App, events: Vec<egui::Event>| {
            clock.set(clock.get() + 0.05);
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 800.0))),
                events,
                time: Some(clock.get()),
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| rename_window(app, ui.ctx()));
            out.textures_delta.clear();
        };
        run(&mut app, Vec::new());
        run(&mut app, Vec::new());
        let all = egui::Event::Key {
            key: egui::Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        };
        run(&mut app, vec![all]);
        run(&mut app, vec![egui::Event::Text("Studio".into())]);
        assert_eq!(
            app.palette_rename.as_ref().map(|(_, n)| n.as_str()),
            Some("Studio")
        );
        let enter = egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        run(&mut app, vec![enter]);
        run(&mut app, Vec::new());
        assert!(app.palette_rename.is_none());
        assert_eq!(app.palette_name(0), "Studio");
    }

    /// Live test of 0.41: palette bars docked only at the window's edges.
    /// Let go over a bar docked at a side, a palette bar goes beside it:
    /// on its half towards the edge just outside it, on the other half
    /// just inside, with a line on that edge; over a palette bar it takes
    /// that bar's place in the order, over itself it stays.
    #[test]
    fn a_bar_let_go_over_a_docked_bar_goes_beside_it() {
        let col = |x0: f32, x1: f32| Rect::from_min_max(pos2(x0, 50.0), pos2(x1, 900.0));
        let marks = [
            SideMark::Place(Dock::Left, BarSlot::Outer),
            SideMark::Place(Dock::Left, BarSlot::AfterPalette),
            // The toolbox, then palette bar 1 after it.
            SideMark::Bar {
                dock: Dock::Left,
                rect: col(0.0, 40.0),
                bar: None,
            },
            SideMark::Place(Dock::Left, BarSlot::AfterToolbox),
            SideMark::Bar {
                dock: Dock::Left,
                rect: col(40.0, 60.0),
                bar: Some(1),
            },
            SideMark::Place(Dock::Left, BarSlot::Inner),
            // A docker column at the right, then the drawing.
            SideMark::Place(Dock::Right, BarSlot::AfterToolbox),
            SideMark::Bar {
                dock: Dock::Right,
                rect: col(1200.0, 1600.0),
                bar: None,
            },
            SideMark::Place(Dock::Right, BarSlot::Inner),
            SideMark::Place(Dock::Right, BarSlot::Drawing),
        ];
        let at = |x: f32| side_target(&marks, pos2(x, 400.0), 0);
        // The toolbox's inner half: first of the places after it.
        let (drop, line) = at(30.0).expect("beside the toolbox");
        assert_eq!(
            drop,
            SideDrop::At {
                dock: Dock::Left,
                slot: BarSlot::AfterToolbox,
                first: true
            }
        );
        assert!((line.center().x - 40.0).abs() < 0.1, "{line:?}");
        // Its outer half: last of the places before it.
        let (drop, line) = at(10.0).expect("outside the toolbox");
        assert_eq!(
            drop,
            SideDrop::At {
                dock: Dock::Left,
                slot: BarSlot::AfterPalette,
                first: false
            }
        );
        assert!(line.center().x.abs() < 0.1, "{line:?}");
        // Palette bar 1: beside it, outside or inside.
        assert_eq!(
            at(45.0).map(|t| t.0),
            Some(SideDrop::Beside { k: 1, outer: true })
        );
        assert_eq!(
            at(55.0).map(|t| t.0),
            Some(SideDrop::Beside { k: 1, outer: false })
        );
        // At the right the outer half is the right one.
        assert_eq!(
            at(1500.0).map(|t| t.0),
            Some(SideDrop::At {
                dock: Dock::Right,
                slot: BarSlot::AfterToolbox,
                first: false
            })
        );
        assert_eq!(
            at(1300.0).map(|t| t.0),
            Some(SideDrop::At {
                dock: Dock::Right,
                slot: BarSlot::Inner,
                first: true
            })
        );
        // Over itself it stays; over the drawing nothing here.
        assert_eq!(
            side_target(&marks, pos2(50.0, 400.0), 1).map(|t| t.0),
            Some(SideDrop::Stay)
        );
        assert_eq!(at(700.0), None);
    }

    /// The bars of one place lie in their order; a bar dropped beside
    /// another takes its place in that order (along the bottom, drawn
    /// from the bottom up, too), keeps its palette and rows, and the
    /// place is kept in the preferences and in workspace files.
    #[test]
    fn a_bar_dropped_beside_another_takes_its_place_in_the_order() {
        let mut app = App::headless();
        let ids: Vec<String> = (1..4)
            .map(|i| crate::settings::palette_id(&app.palettes[i]))
            .collect();
        for id in &ids {
            app.open_palette_bar(id);
        }
        for b in &mut app.settings.palette_bars {
            b.dock = Dock::Left;
        }
        let order = |app: &App| -> Vec<String> {
            app.settings
                .palette_bars
                .iter()
                .map(|b| b.palette.clone())
                .collect()
        };
        // The third outside the first: first in the order.
        place_bar(&mut app, 2, SideDrop::Beside { k: 0, outer: true });
        assert_eq!(
            order(&app),
            [ids[2].clone(), ids[0].clone(), ids[1].clone()]
        );
        assert_eq!(app.extra_palettes.len(), 3, "the bars follow");
        assert_eq!(
            app.settings.palette_bars[0].slot,
            Some(BarSlot::AfterPalette)
        );
        // Inside the toolbox: a place of its own.
        place_bar(
            &mut app,
            0,
            SideDrop::At {
                dock: Dock::Left,
                slot: BarSlot::AfterToolbox,
                first: true,
            },
        );
        let moved = app
            .settings
            .palette_bars
            .iter()
            .find(|b| b.palette == ids[2])
            .expect("bar");
        assert_eq!(
            (moved.dock, moved.slot),
            (Dock::Left, Some(BarSlot::AfterToolbox))
        );
        // A second bar beside it, inside: after it in the order.
        let k = app
            .settings
            .palette_bars
            .iter()
            .position(|b| b.palette == ids[1])
            .expect("bar");
        let j = app
            .settings
            .palette_bars
            .iter()
            .position(|b| b.palette == ids[2])
            .expect("bar");
        place_bar(&mut app, k, SideDrop::Beside { k: j, outer: false });
        let slots: Vec<(String, Option<BarSlot>)> = app
            .settings
            .palette_bars
            .iter()
            .map(|b| (b.palette.clone(), b.slot))
            .collect();
        assert_eq!(
            slots,
            [
                (ids[0].clone(), None),
                (ids[2].clone(), Some(BarSlot::AfterToolbox)),
                (ids[1].clone(), Some(BarSlot::AfterToolbox)),
            ]
        );
        // Along the bottom, drawn from the bottom up: outside (under) a
        // bar comes after it in the list.
        for b in &mut app.settings.palette_bars {
            b.dock = Dock::Bottom;
            b.slot = None;
        }
        place_bar(&mut app, 0, SideDrop::Beside { k: 2, outer: true });
        assert_eq!(
            order(&app),
            [ids[2].clone(), ids[1].clone(), ids[0].clone()]
        );
        place_bar(&mut app, 2, SideDrop::Beside { k: 0, outer: false });
        assert_eq!(
            order(&app),
            [ids[0].clone(), ids[2].clone(), ids[1].clone()]
        );
        // Another side from the menu: that side's usual place.
        set_dock(&mut app, Row::Extra(0), Dock::Bottom, None);
        assert_eq!(
            app.settings.palette_bars[0].slot,
            Some(BarSlot::Drawing),
            "the same side keeps it"
        );
        set_dock(&mut app, Row::Extra(0), Dock::Right, None);
        assert_eq!(app.settings.palette_bars[0].slot, None);
        place_bar(
            &mut app,
            1,
            SideDrop::At {
                dock: Dock::Top,
                slot: BarSlot::Outer,
                first: false,
            },
        );
        // Kept in a workspace file.
        let file = app.export_workspace(crate::workspace_file::WorkspaceParts::ALL);
        let bytes = file.to_bytes();
        let back = crate::workspace_file::WorkspaceFile::from_bytes(&bytes).expect("reads");
        let mut other = App::headless();
        other.import_workspace(&back, crate::workspace_file::WorkspaceParts::ALL);
        assert_eq!(other.settings.palette_bars, app.settings.palette_bars);
        // A floating bar has no place.
        let mut b = app.settings.palette_bars[0].clone();
        b.dock = Dock::Floating;
        b.slot = Some(BarSlot::Inner);
        b.sanitize();
        assert_eq!(b.slot, None);
    }
}
