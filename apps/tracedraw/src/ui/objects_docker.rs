//! The Objects docker: every page with its layers, topmost first (or
//! only the current page, a choice of the bottom bar), then the Master
//! Page with the master layers, the guides and the document grid. A layer row has a collapse arrow, an eye (show or
//! hide), a printer (print and export or not), a padlock (lock or
//! unlock), the layer icon and the name, in bold on a blue row for the
//! active layer, the one new objects go to; clicking a layer makes it
//! active. Under each open layer come its objects, topmost first, each
//! with a thumbnail (the object itself drawn small, see
//! `object_thumbs.rs`) and its name (or what it is when it has no name).
//! The thumbnail size is set by the control in the bottom bar, the wheel
//! over it, or Ctrl+wheel over the list. Groups open to show their objects. Clicking an object
//! selects it (Shift adds it) and makes its layer active; double-clicking
//! a name renames the layer or object; a layer's context menu moves,
//! renames, moves, shows, locks, empties into the master page or deletes
//! it, and an object's context menu moves it to a layer of any page or of
//! the master page (#1477). The bar at the bottom adds a layer or a master
//! layer (its menu also sets which master layers are drawn in front of
//! the page's layers, #1132), shows all pages or the current one, and
//! deletes the selected objects or, with none, the active layer.

use crate::ui::btn_icons::IconUi;
use crate::app::App;
use crate::i18n::{tr, trf};
use crate::theme::Tokens;
use crate::ui::dockers::kind_name;
use crate::ui::tips::RichTip;
use egui::{Color32, Painter, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use tracedraw_core::document::GuideLine;
use tracedraw_core::{
    Command, Layer, LayerId, MasterScope, PageId, PageRanges, Shape, ShapeId, ShapeKind,
};

/// Height of a row.
const ROW_H: f32 = 22.0;
/// Indent per nesting level.
const INDENT: f32 = 16.0;
/// The nesting level of a layer's own objects (under the page and the
/// layer); a group's objects come deeper.
const LAYER_OBJECT_DEPTH: usize = 2;
/// Height of the bar with the New Layer, New Master Layer and Delete
/// buttons.
const BAR_H: f32 = 30.0;
/// The top sheet of the layer icon: blue for a page's layers, amber for
/// master layers.
const LAYER_TOP: Color32 = Color32::from_rgb(0x4A, 0x90, 0xD9);
const MASTER_TOP: Color32 = Color32::from_rgb(0xE0, 0x9A, 0x2E);
const GUIDE_BLUE: Color32 = Color32::from_rgb(0x1E, 0x6F, 0xD9);
const PLUS_GREEN: Color32 = Color32::from_rgb(0x1E, 0x9E, 0x3C);

/// What the docker asks for after drawing.
#[derive(Default)]
struct Actions {
    /// The search text (lower case) while objects not matching it are
    /// left out of the rows drawn now (#4258); empty: all are listed.
    object_filter: String,
    /// Edit Across Layers turned on or off (#3276).
    across: Option<bool>,
    visible: Option<(LayerId, bool)>,
    printable: Option<(LayerId, bool)>,
    locked: Option<(LayerId, bool)>,
    scope: Option<(LayerId, MasterScope)>,
    in_front: Option<(LayerId, bool)>,
    /// The desktop takes objects put beside the page, or not (#3100).
    desktop_takes: Option<bool>,
    pick: Option<LayerId>,
    select: Option<ShapeId>,
    /// An object's eye or padlock was clicked (#832).
    object_visible: Option<(ShapeId, bool)>,
    object_locked: Option<(ShapeId, bool)>,
    rename_layer: Option<(LayerId, String)>,
    rename_object: Option<(ShapeId, String)>,
    rename_page: Option<String>,
    delete_layer: Option<LayerId>,
    reorder: Option<(LayerId, usize)>,
    /// Each object row drawn and where, and each layer row
    /// that takes objects, for a row dragged along the list (live test
    /// of 0.41).
    object_rows: Vec<(ShapeId, Rect)>,
    /// Every object row of the list, top to bottom (laid out or not), for
    /// Shift+click's range (#2483).
    row_order: Vec<ShapeId>,
    /// Every layer row drawn, top to bottom as they show, for
    /// Shift+click's range (#3966).
    layer_order: Vec<LayerId>,
    /// The selection as a set: each row asks whether its object is
    /// selected, and asking the list for each of 50,000 selected objects
    /// took most of a second a frame.
    selected: std::collections::HashSet<ShapeId>,
    layer_drop_rows: Vec<(LayerId, Rect)>,
    collapse: Option<LayerId>,
    expand_group: Option<ShapeId>,
    toggle_page: Option<PageId>,
    toggle_master: bool,
    toggle_ranges: bool,
    /// A row of another page was used: that page becomes the current one
    /// first.
    goto: Option<PageId>,
    select_layer: Option<LayerId>,
    /// Cut, copy or paste a whole layer's objects from its menu (#1992).
    layer_clip: Option<(LayerId, LayerClip)>,
    new_layer: bool,
    new_master: Option<MasterScope>,
    new_range: bool,
    edit_range: Option<LayerId>,
    /// An object row was right-clicked there: its context menu (#2601).
    object_menu: Option<(ShapeId, Pos2)>,
    layer_to_master: Option<LayerId>,
    master_to_page: Option<LayerId>,
    /// Not an action: the page whose rows are being drawn (None for the
    /// master page).
    row_page: Option<PageId>,
    toggle_guides: Option<(Option<PageId>, bool)>,
    /// A Guides row was clicked: the master page's guidelines (true) or
    /// the page's own (false) are the ones edited from now (#1928).
    guides_master: Option<bool>,
    toggle_grid: bool,
    /// Lock or unlock all the guidelines of a Guides row: the master
    /// page's (true) or the page's own, and locked or not.
    lock_guides: Option<(bool, bool)>,
    /// One guideline's row: its set (master page or not), its index, and
    /// what was done: its eye (hide or show), its padlock, or a click
    /// that selects it (#1775).
    guide_row: Option<(bool, usize, GuideRowAct)>,
    /// A guideline row dragged onto a Guides row and let go there (#3599):
    /// its set (`None` the master page's), its index, and the set it was
    /// dropped on.
    move_guides: Option<(Option<PageId>, usize, Option<PageId>)>,
    /// Paste from a Guides row's right-click menu, into its set (#3599).
    paste_guides: Option<Option<PageId>>,
    /// A command of a Guides row's own right-click menu, on all of its
    /// guidelines: its set (master page or not) and the command (#3578).
    guides_cmd: Option<(bool, GuidesCmd)>,
    /// A layer moved to another page, from its menu (#1477).
    layer_to_page: Option<(LayerId, PageId)>,
    /// A layer's Properties, from its menu (#1477).
    layer_props: Option<LayerId>,
    /// Not an action: the object whose row is scrolled into view.
    scroll_to: Option<ShapeId>,
    /// Not an action: the thumbnail cache, taken from the app while the
    /// tree is drawn, the thumbnail size (points) and the drawing's
    /// (instance, revision).
    thumbs: crate::object_thumbs::ObjectThumbs,
    thumb_size: f32,
    drawing: (u64, u64),
}

/// What a guideline's own row did (#1775).
#[derive(Clone, Copy, Debug, PartialEq)]
enum GuideRowAct {
    Hide(bool),
    Lock(bool),
    /// A click: alone it selects the guideline; with Ctrl it is added to
    /// or taken from the selection, with Shift the guidelines from the
    /// last one clicked to it are selected (#3575).
    Select { ctrl: bool, shift: bool },
    /// A command of its right-click menu (#3012).
    Menu(GuideCmd),
}

/// The commands of a guideline row's right-click menu (#3012): they act
/// on the selected guidelines, the one clicked among them.
#[derive(Clone, Copy, Debug, PartialEq)]
enum GuideCmd {
    Delete,
    Cut,
    Copy,
    Paste,
    Lock(bool),
    Hide(bool),
    /// Name the guideline (#3574).
    Rename,
}

/// The commands of a Guides row's right-click menu (#3578): they act on
/// every guideline of the row.
#[derive(Clone, Copy, Debug, PartialEq)]
enum GuidesCmd {
    Cut,
    Copy,
    /// Move them to the guidelines of all pages, or to the own
    /// guidelines of the odd or even pages.
    MoveTo(crate::guides::GuidesTarget),
}

/// Where a layer sits: its index in its list (0 at the bottom), how many
/// the list holds, whether it is a master layer and whether it shows on
/// the current page.
#[derive(Clone, Copy)]
struct Place {
    index: usize,
    count: usize,
    master: bool,
    shown: bool,
    /// The page the layer belongs to (None for a master layer).
    page: Option<PageId>,
}

/// How a row is painted.
#[derive(Clone, Copy, PartialEq)]
enum Look {
    Object { selected: bool },
    Layer { active: bool, selected: bool },
    Node,
}

/// The label an object shows: its name, else what it is.
pub fn object_label(s: &Shape) -> String {
    match s.name.as_deref().map(str::trim) {
        Some(n) if !n.is_empty() => n.to_string(),
        _ => {
            let kind = match &s.kind {
                // A bitmap shows its colour mode too, as in the status
                // bar (#3557).
                k @ ShapeKind::Bitmap { png, .. } => trf(
                    "status.bitmap_mode",
                    &[
                        ("k", &kind_name(k)),
                        ("mode", &tr(crate::bitmap_fx::mode_key(s.cmyk_bitmap, png))),
                    ],
                ),
                k => kind_name(k),
            };
            crate::ui::dockers::tagged_kind(s, kind)
        }
    }
}

/// An eye, open when `on`, else closed (a lid line), centred in `r`.
pub(crate) fn paint_eye(ui: &Ui, r: Rect, on: bool) {
    let p = ui.painter();
    let c = r.center();
    let color = if on {
        Tokens::icon()
    } else {
        Tokens::icon_off()
    };
    let w = 6.0;
    let pts_top: Vec<Pos2> = (0..=12)
        .map(|i| {
            let t = i as f32 / 12.0;
            let x = -w + 2.0 * w * t;
            Pos2::new(c.x + x, c.y - 3.6 * (1.0 - (x / w).powi(2)))
        })
        .collect();
    let pts_bottom: Vec<Pos2> = pts_top
        .iter()
        .map(|q| Pos2::new(q.x, c.y + (c.y - q.y)))
        .collect();
    let stroke = Stroke::new(1.1, color);
    p.add(egui::Shape::line(pts_top, stroke));
    if on {
        p.add(egui::Shape::line(pts_bottom, stroke));
        p.circle_filled(c, 1.9, color);
    } else {
        // Closed: only the lower lid with lashes.
        p.add(egui::Shape::line(pts_bottom.clone(), stroke));
        for dx in [-3.5f32, 0.0, 3.5] {
            let base = Pos2::new(c.x + dx, c.y + 3.0 * (1.0 - (dx / w).powi(2)));
            p.line_segment([base, base + Vec2::new(dx * 0.15, 2.0)], stroke);
        }
    }
}

/// A printer, dark when `on`, else pale, centred in `r`.
fn paint_printer(ui: &Ui, r: Rect, on: bool) {
    let p = ui.painter();
    let c = r.center();
    let color = if on {
        Tokens::icon()
    } else {
        Tokens::icon_off()
    };
    let stroke = Stroke::new(1.0, color);
    // The sheet going in at the top.
    p.rect_stroke(
        Rect::from_min_max(c + Vec2::new(-3.5, -6.5), c + Vec2::new(3.5, -2.0)),
        0.0,
        stroke,
        egui::epaint::StrokeKind::Inside,
    );
    // The body.
    p.rect_filled(
        Rect::from_min_max(c + Vec2::new(-6.5, -2.5), c + Vec2::new(6.5, 3.5)),
        1.0,
        color,
    );
    // The printed sheet coming out.
    let out = Rect::from_min_max(c + Vec2::new(-3.5, 1.0), c + Vec2::new(3.5, 6.5));
    p.rect_filled(out, 0.0, Color32::WHITE);
    p.rect_stroke(out, 0.0, stroke, egui::epaint::StrokeKind::Inside);
}

/// A padlock, shut when `locked`, else open, centred in `r`: dark when
/// shut, pale when open.
fn paint_lock(ui: &Ui, r: Rect, locked: bool) {
    let color = if locked {
        Tokens::icon()
    } else {
        Tokens::icon_off()
    };
    paint_padlock(ui.painter(), r, locked, color);
}

/// A padlock in `color`, shut when `locked`, else open.
pub(crate) fn paint_padlock(p: &Painter, r: Rect, locked: bool, color: Color32) {
    let c = r.center();
    let body = Rect::from_center_size(c + Vec2::new(0.0, 2.0), Vec2::new(9.0, 7.0));
    p.rect_filled(body, 1.0, color);
    let stroke = Stroke::new(1.3, color);
    let left = body.left() + 2.2;
    let right = body.right() - 2.2;
    let top = body.top() - 4.0;
    let shackle_right = if locked { right } else { right + 3.0 };
    let arc: Vec<Pos2> = (0..=10)
        .map(|i| {
            let a = std::f32::consts::PI * i as f32 / 10.0;
            let mid = (left + shackle_right) / 2.0;
            let rx = (shackle_right - left) / 2.0;
            Pos2::new(mid - rx * a.cos(), top - 0.5 - 2.0 * a.sin() + 2.0)
        })
        .collect();
    p.add(egui::Shape::line(arc, stroke));
    p.line_segment(
        [Pos2::new(left, top + 1.5), Pos2::new(left, body.top())],
        stroke,
    );
    let right_end = if locked { body.top() } else { top + 3.0 };
    p.line_segment(
        [
            Pos2::new(shackle_right, top + 1.5),
            Pos2::new(shackle_right, right_end),
        ],
        stroke,
    );
}

/// Three stacked sheets: the layer icon, its top sheet amber for a master
/// layer.
fn paint_layer_icon(p: &Painter, r: Rect, master: bool) {
    let c = r.center();
    for (i, dy) in [3.0f32, 0.0, -3.0].iter().enumerate() {
        let pts = vec![
            Pos2::new(c.x - 6.0, c.y + dy),
            Pos2::new(c.x, c.y + dy - 2.8),
            Pos2::new(c.x + 6.0, c.y + dy),
            Pos2::new(c.x, c.y + dy + 2.8),
        ];
        let fill = match (i, master) {
            (2, false) => LAYER_TOP,
            (2, true) => MASTER_TOP,
            _ => Color32::from_rgb(0xC9, 0xDD, 0xF2),
        };
        p.add(egui::Shape::convex_polygon(
            pts,
            fill,
            Stroke::new(0.8, Color32::from_rgb(0x2F, 0x6D, 0xB5)),
        ));
    }
}

/// A sheet with a folded corner: the page and master page nodes.
fn paint_page_icon(p: &Painter, r: Rect, master: bool) {
    let c = r.center();
    let (x0, x1, y0, y1) = (c.x - 5.0, c.x + 5.0, c.y - 6.5, c.y + 6.5);
    let fold = 3.5;
    let outline = vec![
        Pos2::new(x0, y0),
        Pos2::new(x1 - fold, y0),
        Pos2::new(x1, y0 + fold),
        Pos2::new(x1, y1),
        Pos2::new(x0, y1),
    ];
    let fill = if master {
        Color32::from_rgb(0xFB, 0xE7, 0xC6)
    } else {
        crate::theme::Tokens::field()
    };
    p.add(egui::Shape::convex_polygon(
        outline,
        fill,
        Stroke::new(1.0, Tokens::icon()),
    ));
    p.add(egui::Shape::line(
        vec![
            Pos2::new(x1 - fold, y0),
            Pos2::new(x1 - fold, y0 + fold),
            Pos2::new(x1, y0 + fold),
        ],
        Stroke::new(0.8, Tokens::icon()),
    ));
}

/// Two crossing guidelines.
fn paint_guides_icon(p: &Painter, r: Rect) {
    let c = r.center();
    let stroke = Stroke::new(1.2, GUIDE_BLUE);
    p.line_segment(
        [c + Vec2::new(-7.0, -2.5), c + Vec2::new(7.0, -2.5)],
        stroke,
    );
    p.line_segment([c + Vec2::new(2.5, -7.0), c + Vec2::new(2.5, 7.0)], stroke);
    p.circle_filled(c + Vec2::new(2.5, -2.5), 1.6, GUIDE_BLUE);
}

/// A three by three grid.
fn paint_grid_icon(p: &Painter, r: Rect) {
    let c = r.center();
    let stroke = Stroke::new(1.0, crate::theme::Tokens::text_dim());
    for i in 0..4 {
        let t = -6.0 + 4.0 * i as f32;
        p.line_segment([c + Vec2::new(t, -6.0), c + Vec2::new(t, 6.0)], stroke);
        p.line_segment([c + Vec2::new(-6.0, t), c + Vec2::new(6.0, t)], stroke);
    }
}

/// The open (pointing down) or closed (pointing right) arrow.
fn paint_arrow(ui: &Ui, r: Rect, open: bool) {
    let c = r.center();
    let pts = if open {
        vec![
            c + Vec2::new(-3.5, -1.8),
            c + Vec2::new(3.5, -1.8),
            c + Vec2::new(0.0, 2.2),
        ]
    } else {
        vec![
            c + Vec2::new(-1.8, -3.5),
            c + Vec2::new(2.2, 0.0),
            c + Vec2::new(-1.8, 3.5),
        ]
    };
    ui.painter().add(egui::Shape::convex_polygon(
        pts,
        Tokens::text_dim(),
        Stroke::NONE,
    ));
}

/// The swatch of an object: its fill (or a crossed box for none) inside
/// its outline colour.
fn paint_swatch(ui: &Ui, r: Rect, s: &Shape) {
    let p = ui.painter();
    let fill = crate::app::fill_preview_color(&s.fill);
    let edge = s
        .stroke
        .as_ref()
        .map(|st| crate::canvas::to_color32(st.color))
        .unwrap_or(Tokens::text_dim());
    match fill {
        Some(c) => {
            p.rect_filled(r, 1.0, c);
        }
        None => {
            p.rect_filled(r, 1.0, Color32::WHITE);
            p.line_segment(
                [r.left_bottom(), r.right_top()],
                Stroke::new(1.0, Color32::from_rgb(0xE0, 0x40, 0x40)),
            );
        }
    }
    p.rect_stroke(
        r,
        1.0,
        Stroke::new(1.0, edge),
        egui::epaint::StrokeKind::Inside,
    );
}

/// A small green badge with a white plus at the top right of `r`.
fn paint_plus_badge(p: &Painter, r: Rect) {
    let badge = Rect::from_min_size(r.right_top() + Vec2::new(-7.0, -1.0), Vec2::splat(8.0));
    p.rect_filled(badge, 1.0, PLUS_GREEN);
    let c = badge.center();
    let w = Stroke::new(1.3, Color32::WHITE);
    p.line_segment([c - Vec2::new(2.5, 0.0), c + Vec2::new(2.5, 0.0)], w);
    p.line_segment([c - Vec2::new(0.0, 2.5), c + Vec2::new(0.0, 2.5)], w);
}

/// A waste bin, pale when `on` is false.
fn paint_bin(p: &Painter, r: Rect, on: bool) {
    let c = r.center();
    let color = if on {
        Tokens::icon()
    } else {
        Tokens::icon_off()
    };
    let stroke = Stroke::new(1.2, color);
    p.line_segment(
        [c + Vec2::new(-6.0, -4.5), c + Vec2::new(6.0, -4.5)],
        stroke,
    );
    p.line_segment(
        [c + Vec2::new(-2.0, -6.5), c + Vec2::new(2.0, -6.5)],
        stroke,
    );
    p.add(egui::Shape::closed_line(
        vec![
            c + Vec2::new(-4.5, -4.5),
            c + Vec2::new(4.5, -4.5),
            c + Vec2::new(3.5, 6.5),
            c + Vec2::new(-3.5, 6.5),
        ],
        stroke,
    ));
    for dx in [-1.6f32, 1.6] {
        p.line_segment(
            [c + Vec2::new(dx, -2.0), c + Vec2::new(dx * 0.8, 4.5)],
            Stroke::new(0.9, color),
        );
    }
}

/// A full-width row; returns its rectangle and its response.
fn row(ui: &mut Ui, look: Look) -> (Rect, egui::Response) {
    row_of(ui, look, ROW_H)
}

/// A full-width row `height` high.
fn row_of(ui: &mut Ui, look: Look, height: f32) -> (Rect, egui::Response) {
    let (rect, resp) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), height),
        Sense::click_and_drag(),
    );
    paint_row(ui, look, rect, &resp);
    (rect, resp)
}

/// The background of a row: selected, active, hovered or plain.
fn paint_row(ui: &Ui, look: Look, rect: Rect, resp: &egui::Response) {
    let p = ui.painter();
    match look {
        Look::Object { selected: true } | Look::Layer { active: false, selected: true } => {
            p.rect_filled(rect, 0.0, crate::theme::Tokens::selected_row());
            p.rect_stroke(
                rect,
                0.0,
                Stroke::new(1.0, crate::theme::Tokens::selected_row_line()),
                egui::epaint::StrokeKind::Inside,
            );
        }
        Look::Layer { active: true, .. } => {
            p.rect_filled(rect, 0.0, crate::theme::Tokens::tool_active());
            p.rect_filled(
                Rect::from_min_size(rect.min, Vec2::new(3.0, rect.height())),
                0.0,
                Tokens::ACCENT,
            );
        }
        _ if resp.hovered() => {
            p.rect_filled(rect, 0.0, crate::theme::Tokens::hovered_row());
        }
        Look::Layer { active: false, .. } => {
            p.rect_filled(rect, 0.0, crate::theme::Tokens::tab_strip());
        }
        _ => {}
    }
}

/// A clickable square of `size` at `x` in the row `rect`.
fn hot(ui: &mut Ui, rect: Rect, x: f32, size: f32, id: egui::Id) -> (Rect, egui::Response) {
    let r = square(rect, x, size);
    (r, ui.interact(r, id, Sense::click()))
}

/// The square of `size` at `x`, centred on the row `rect`.
fn square(rect: Rect, x: f32, size: f32) -> Rect {
    Rect::from_min_size(
        Pos2::new(x, rect.center().y - size / 2.0),
        Vec2::splat(size),
    )
}

/// The layer search text kept between frames (see [`layer_search`]).
const LAYER_FILTER: &str = "objects_layer_filter";

/// The search field at the top of the panel, over the tree: the layers whose names hold the text
/// (any case) stay listed, the others are hidden while it is not empty.
pub(crate) fn layer_search(ui: &mut Ui) {
    let id = egui::Id::new(LAYER_FILTER);
    let mut text = ui.data_mut(|d| d.get_temp::<String>(id).unwrap_or_default());
    ui.horizontal(|ui| {
        let width = (ui.available_width() - 24.0).max(40.0);
        ui.add(
            egui::TextEdit::singleline(&mut text)
                .hint_text(tr("docker.search_layers"))
                .desired_width(width),
        )
        .rich_tip(tr("docker.search_layers"));
        if ui
            .add_enabled(!text.is_empty(), egui::Button::new("×"))
            .rich_tip(tr("docker.clear"))
            .clicked()
        {
            text.clear();
        }
    });
    ui.data_mut(|d| d.insert_temp(id, text));
}

/// The search text in lower case, empty when no search is on.
fn layer_needle(ui: &Ui) -> String {
    ui.data_mut(|d| d.get_temp::<String>(egui::Id::new(LAYER_FILTER)))
        .unwrap_or_default()
        .trim()
        .to_lowercase()
}

/// Whether a layer is listed under the search `needle` (empty lists
/// all): its name holds the text, or one of its objects' (#4258).
fn layer_hit(layer: &Layer, needle: &str) -> bool {
    needle.is_empty() || layer_name_hit(layer, needle) || layer.shapes.iter().any(|s| shape_hit(s, needle))
}

/// Whether the layer's own name holds the search `needle`.
fn layer_name_hit(layer: &Layer, needle: &str) -> bool {
    layer.name.to_lowercase().contains(needle)
}

/// Whether the label of `s` (its name, else what it is) holds the
/// search `needle` (#4258).
fn own_hit(s: &Shape, needle: &str) -> bool {
    object_label(s).to_lowercase().contains(needle)
}

/// Whether `s` or an object inside it (a group's, a PowerClip's
/// contents) holds the search `needle` in its label (#4258).
fn shape_hit(s: &Shape, needle: &str) -> bool {
    own_hit(s, needle)
        || match &s.kind {
            ShapeKind::Group { children } => children.iter().any(|c| shape_hit(c, needle)),
            ShapeKind::ClipFrame { contents, .. } => contents.iter().any(|c| shape_hit(c, needle)),
            _ => false,
        }
}

/// The docker: the tree above the bar with its buttons.
pub fn objects_docker(app: &mut App, ui: &mut Ui) {
    // The search field at the top of the panel (#4258).
    layer_search(ui);
    let content_h = (ui.available_height() - BAR_H - 4.0).max(40.0);
    let out = egui::ScrollArea::vertical()
        .max_height(content_h)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            objects(app, ui);
        });
    // Ctrl+wheel over the list makes the thumbnails larger or smaller, a
    // step a notch on the desktop and in the browser alike (#1406).
    if let Some(steps) =
        crate::ui::bars::ctrl_wheel_steps(ui, out.inner_rect, egui::Id::new("objects_thumb_wheel"))
    {
        zoom_thumbs(app, steps);
    }
    bottom_bar(app, ui);
}

/// The thumbnails `steps` steps larger (smaller when negative).
fn zoom_thumbs(app: &mut App, steps: i32) {
    if steps == 0 {
        return;
    }
    let steps = steps.clamp(-4, 4) as f32;
    app.settings.object_thumb_size = crate::object_thumbs::clamp_size(
        app.settings.object_thumb_size + steps * crate::object_thumbs::STEP,
    );
}

/// The selection the list last showed (see [`scroll_target`]).
const SEEN_SELECTION: &str = "objects_seen_selection";

/// The object whose row the list scrolls to this frame: the first
/// selected one, when the selection is not the one the list last showed
/// (a pick on the page, a Select All, an undo), so a selection far down a
/// long list comes into view (#1044).
fn scroll_target(app: &App, ui: &Ui) -> Option<ShapeId> {
    let seen = egui::Id::new(SEEN_SELECTION);
    let changed = ui.data_mut(|d| {
        let last = d.get_temp::<Vec<ShapeId>>(seen);
        d.insert_temp(seen, app.selection.clone());
        last.as_ref() != Some(&app.selection)
    });
    changed.then(|| app.selection.first().copied()).flatten()
}

/// A newly picked object inside groups or clip frames (a Ctrl+click on
/// the page, Preflight's Locate object) shows in the list: the groups and
/// frames holding it open, and its layer too (#4400).
pub fn reveal_selection(app: &mut App) {
    let doc = app.doc();
    let mut open = Vec::new();
    let mut layers = Vec::new();
    for id in &app.selection {
        let up = doc.ancestors(*id);
        let Some(top) = up.first().copied() else {
            continue;
        };
        if let Some(l) = doc
            .pages
            .iter()
            .flat_map(|p| &p.layers)
            .find(|l| l.shapes.iter().any(|s| s.id == top))
        {
            layers.push(l.id);
        }
        open.extend(up);
    }
    app.objects_open_groups.extend(open);
    for l in layers {
        app.objects_collapsed.remove(&l);
    }
}

/// The tree: the pages and their layers (all of them, or the current
/// one), then the Master Page.
pub fn objects(app: &mut App, ui: &mut Ui) {
    let scroll_to = scroll_target(app, ui);
    if scroll_to.is_some() {
        reveal_selection(app);
    }
    let mut act = Actions {
        scroll_to,
        thumbs: std::mem::take(&mut app.object_thumbs),
        thumb_size: crate::object_thumbs::clamp_size(app.settings.object_thumb_size),
        drawing: (app.engine.instance(), app.engine.revision()),
        selected: app.selection.iter().copied().collect(),
        ..Default::default()
    };
    act.thumbs.begin_frame();
    {
        let doc = app.doc();
        if doc.page(app.page).is_err() {
            return;
        }
        let index = doc.pages.iter().position(|p| p.id == app.page).unwrap_or(0);
        let active = app.active_layer();
        ui.spacing_mut().item_spacing.y = 0.0;
        let all = app.settings.objects_all_pages;
        let needle = layer_needle(ui);
        for page in doc.pages.iter().filter(|p| {
            (all || p.id == app.page)
                && (needle.is_empty() || p.layers.iter().any(|l| layer_hit(l, &needle)))
        }) {
            let current = page.id == app.page;
            let page_open = !app.objects_closed_pages.contains(&page.id);
            let (toggle, resp) = node_row(
                ui,
                ("objects_page", page.id.raw()),
                &page.name,
                page_open,
                false,
                current || !all,
                "docker.page_row_desc",
            );
            if toggle {
                act.toggle_page = Some(page.id);
            }
            // Double-click (or the context menu) renames the page.
            if resp.double_clicked() {
                act.rename_page = Some(page.name.clone());
                if !current {
                    act.goto = Some(page.id);
                }
            }
            filter_page_row_popup(
                egui::Popup::context_menu(&resp),
                &app.settings.context_menus,
                current,
            )
            .show(|ui| {
                use crate::ui::context_menus::show_item;
                let _scope = crate::ui::context_menus::Scope::new(
                    &app.settings.context_menus,
                    "objects_page",
                );
                for menu_key in crate::ui::context_menu_filter::ordered_keys(&[
                    "docker.go_to_page",
                    "docker.rename",
                ])
                .iter()
                .copied()
                {
                    match menu_key {
                        "docker.go_to_page" => {
                            if !current
                                && show_item(ui, "docker.go_to_page")
                                && ui.ibutton("docker.go_to_page").clicked()
                            {
                                act.goto = Some(page.id);
                                ui.close();
                            }
                        }
                        "docker.rename"
                            if show_item(ui, "docker.rename")
                                && ui.ibutton("docker.rename").clicked() =>
                        {
                            act.rename_page = Some(page.name.clone());
                            if !current {
                                act.goto = Some(page.id);
                            }
                            ui.close();
                        }
                        _ => {}
                    }
                }
            });
            if page_open {
                act.row_page = Some(page.id);
                let count = page.layers.len();
                for (i, layer) in page
                    .layers
                    .iter()
                    .enumerate()
                    .rev()
                    .filter(|(_, l)| layer_hit(l, &needle))
                {
                    let place = Place {
                        index: i,
                        count,
                        master: false,
                        shown: true,
                        page: Some(page.id),
                    };
                    layer_rows(app, ui, layer, place, active, &mut act);
                }
                // The page's own guidelines (#1928).
                if needle.is_empty() {
                    guides_row(app, ui, &page.guides, Some(page.id), &mut act);
                }
            }
        }
        act.row_page = None;
        let master_hit = doc.master.iter().any(|l| layer_hit(l, &needle));
        let master_open = !app.objects_master_closed;
        if needle.is_empty() || master_hit {
            act.toggle_master = node_row(
                ui,
                "objects_master",
                &tr("docker.master_page"),
                master_open,
                true,
                true,
                "docker.page_row_desc",
            )
            .0;
        }
        if master_open && (needle.is_empty() || master_hit) {
            let count = doc.master.len();
            for (i, layer) in doc
                .master
                .iter()
                .enumerate()
                .rev()
                .filter(|(_, l)| l.page_range.is_none() && layer_hit(l, &needle))
            {
                let place = Place {
                    index: i,
                    count,
                    master: true,
                    shown: layer.applies_to(index),
                    page: None,
                };
                layer_rows(app, ui, layer, place, active, &mut act);
            }
            if needle.is_empty() {
                guides_row(app, ui, &doc.master_guides, None, &mut act);
                grid_row(app, ui, &mut act);
            }
        }
        let ranges_open = !app.objects_ranges_closed;
        let (toggle, response) = node_row(
            ui,
            "objects_ranges",
            &tr("docker.range_pages"),
            ranges_open,
            true,
            true,
            "docker.range_pages_desc",
        );
        act.toggle_ranges = toggle;
        response.context_menu(|ui| {
            if ui.ibutton("docker.new_range_layer").clicked() {
                act.new_range = true;
                ui.close();
            }
        });
        if ranges_open {
            if !doc.master.iter().any(|layer| layer.page_range.is_some()) {
                ui.horizontal(|ui| {
                    ui.add_space(INDENT + 20.0);
                    if ui
                        .small_button(tr("docker.new_range_layer"))
                        .rich_tip(crate::ui::tips::pair(
                            "docker.new_range_layer",
                            "docker.page_range_desc",
                        ))
                        .clicked()
                    {
                        act.new_range = true;
                    }
                });
            }
            let count = doc.master.len();
            for (i, layer) in doc
                .master
                .iter()
                .enumerate()
                .rev()
                .filter(|(_, l)| l.page_range.is_some() && layer_hit(l, &needle))
            {
                layer_rows(
                    app,
                    ui,
                    layer,
                    Place {
                        index: i,
                        count,
                        master: true,
                        shown: layer.applies_to(index),
                        page: None,
                    },
                    active,
                    &mut act,
                );
            }
        }
    }
    let mut thumbs = std::mem::take(&mut act.thumbs);
    thumbs.trim(crate::object_thumbs::MAX_BYTES);
    if thumbs.waiting {
        // New pictures wait for the next frames.
        ui.ctx().request_repaint();
    }
    app.object_thumbs = thumbs;
    apply(app, ui, act);
}

/// The page or Master Page node: an arrow, the page icon and the title in
/// bold. True when clicked (to open or close it), and the row's response.
fn node_row(
    ui: &mut Ui,
    id: impl std::hash::Hash + Copy + std::fmt::Debug,
    title: &str,
    open: bool,
    master: bool,
    bold: bool,
    tip_key: &str,
) -> (bool, egui::Response) {
    let (rect, resp) = row(ui, Look::Node);
    let resp = resp.rich_tip_with(|| format!("{title}\n{}", tr(tip_key)));
    let x = rect.left() + 3.0;
    let (ar, a) = hot(ui, rect, x, 14.0, egui::Id::new((id, "arrow")));
    let a = a.rich_tip_with(|| crate::ui::tips::keyed("docker.expand_row"));
    paint_arrow(ui, ar, open);
    paint_page_icon(ui.painter(), square(rect, x + 16.0, 16.0), master);
    // The current page in bold among the others.
    ui.painter().text(
        Pos2::new(x + 38.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        title,
        if bold {
            crate::theme::bold(13.0)
        } else {
            egui::FontId::proportional(13.0)
        },
        Tokens::text(),
    );
    // A double-click is two clicks, so renaming closes and reopens the
    // node and leaves it as it was.
    (a.clicked() || resp.clicked(), resp)
}

/// `s` is selected, or holds a selected object: one of a group's or a
/// clip frame's own (#4135).
fn holds_selection(selection: &[ShapeId], s: &Shape) -> bool {
    if selection.contains(&s.id) {
        return true;
    }
    match &s.kind {
        ShapeKind::Group { children } => children.iter().any(|c| holds_selection(selection, c)),
        ShapeKind::ClipFrame { frame, contents } => {
            holds_selection(selection, frame)
                || contents.iter().any(|c| holds_selection(selection, c))
        }
        _ => false,
    }
}

/// A layer's row, then its objects when it is open.
fn layer_rows(
    app: &App,
    ui: &mut Ui,
    layer: &Layer,
    at: Place,
    active: Option<LayerId>,
    act: &mut Actions,
) {
    let open = !app.objects_collapsed.contains(&layer.id);
    let is_active = active == Some(layer.id);
    let is_selected = app.selected_layers.contains(&layer.id);
    act.layer_order.push(layer.id);
    let (rect, resp) = row(ui, Look::Layer {
        active: is_active,
        selected: is_selected,
    });
    // A collapsed layer shows that it holds the selection: a mark at the
    // end of its row, as its hidden rows would be highlighted (#4135).
    let holds = !open && layer.shapes.iter().any(|s| holds_selection(&app.selection, s));
    let resp = resp.rich_tip_with(|| {
        let range = layer
            .page_range
            .as_ref()
            .map(|range| format!("\n{range}"))
            .unwrap_or_default();
        let mut tip = format!("{}{range}\n{}", layer.name, tr("docker.layer_row_desc"));
        if holds {
            tip.push('\n');
            tip.push_str(&tr("docker.layer_holds_selection"));
        }
        tip
    });
    if holds {
        let c = Pos2::new(rect.right() - 10.0, rect.center().y);
        let mark = Rect::from_center_size(c, Vec2::splat(7.0));
        ui.painter().rect_filled(mark, 1.0, Tokens::ACCENT);
        ui.painter().rect_stroke(
            mark,
            1.0,
            Stroke::new(1.0, crate::theme::Tokens::selected_row_line()),
            egui::epaint::StrokeKind::Outside,
        );
    }
    let base = egui::Id::new(("objects_layer", layer.id.raw()));
    let mut x = rect.left() + 3.0 + INDENT;
    let (ar, a) = hot(ui, rect, x, 14.0, base.with("arrow"));
    let a = a.rich_tip_with(|| crate::ui::tips::keyed("docker.expand_row"));
    paint_arrow(ui, ar, open);
    if a.clicked() {
        act.collapse = Some(layer.id);
    }
    x += 16.0;
    let (er, e) = hot(ui, rect, x, 18.0, base.with("eye"));
    paint_eye(ui, er, layer.visible);
    if e.rich_tip(tr("docker.show_hide")).clicked() {
        act.visible = Some((layer.id, !layer.visible));
    }
    x += 20.0;
    let (pr, p) = hot(ui, rect, x, 18.0, base.with("print"));
    paint_printer(ui, pr, layer.printable);
    if p.rich_tip(tr("docker.print_layer")).clicked() {
        act.printable = Some((layer.id, !layer.printable));
    }
    x += 20.0;
    let (lr, l) = hot(ui, rect, x, 18.0, base.with("lock"));
    paint_lock(ui, lr, layer.locked);
    if l.rich_tip(tr("docker.lock_layer")).clicked() {
        act.locked = Some((layer.id, !layer.locked));
    }
    x += 22.0;
    paint_layer_icon(ui.painter(), square(rect, x, 16.0), at.master);
    x += 22.0;
    let font = if is_active {
        crate::theme::bold(13.0)
    } else {
        egui::FontId::proportional(13.0)
    };
    let color = if at.shown {
        Tokens::text()
    } else {
        Tokens::text_dim()
    };
    let name = ui.painter().text(
        Pos2::new(x, rect.center().y),
        egui::Align2::LEFT_CENTER,
        &layer.name,
        font,
        color,
    );
    // A master layer's pages and, when it is drawn over the page's own
    // layers, "In front" (it is listed under them all the same, #1132).
    let mut notes = Vec::new();
    if let Some(range) = &layer.page_range {
        notes.push(range.to_string());
    } else if at.master && layer.scope != MasterScope::All {
        notes.push(tr(scope_key(layer.scope)));
    }
    if at.master && layer.in_front {
        notes.push(tr("docker.in_front"));
    }
    if !notes.is_empty() {
        ui.painter().text(
            Pos2::new(name.right() + 6.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            notes.join(", "),
            egui::FontId::proportional(11.0),
            Tokens::text_dim(),
        );
    }
    // An object row let go here goes to the top of this layer.
    if !layer.locked {
        act.layer_drop_rows.push((layer.id, rect));
    }
    // A layer of another page: that page comes first.
    let elsewhere = at.page.filter(|p| *p != app.page);
    if resp.double_clicked() {
        act.rename_layer = Some((layer.id, layer.name.clone()));
    } else if resp.clicked() {
        act.goto = elsewhere;
        act.pick = Some(layer.id);
    }
    crate::ui::context_menus::filter_popup(
        egui::Popup::context_menu(&resp),
        &app.settings.context_menus,
        "objects_layer",
    )
    .show(|ui| {
        use crate::ui::context_menus::show_item;
        let _scope =
            crate::ui::context_menus::Scope::new(&app.settings.context_menus, "objects_layer");
        let new_key = if at.master && layer.page_range.is_some() {
            "docker.new_range_layer"
        } else if at.master {
            "docker.new_master_layer_short"
        } else {
            "docker.new_layer"
        };
        // With several layers picked, the commands act on them all (#3966).
        let aimed = target_layers(app, layer.id);
        let has = aimed
            .iter()
            .any(|id| app.doc().layer(*id).is_ok_and(|l| !l.shapes.is_empty()));
        let cuttable = aimed.iter().any(|id| {
            app.doc()
                .layer(*id)
                .is_ok_and(|l| !l.shapes.is_empty() && !l.locked)
        });
        let range_available = layer
            .page_range
            .as_ref()
            .is_none_or(|range| range.count_within(app.doc().pages.len()) > 0);
        let range_visible = layer.page_range.is_none() || layer.visible;
        let can_paste =
            app.clipboard.is_some() && !layer.locked && range_available && range_visible;
        for key in crate::ui::context_menu_filter::ordered_keys(&[
            "docker.rename",
            new_key,
            "docker.select_layer_objects",
            "menu.edit.cut",
            "menu.edit.copy",
            "menu.edit.paste",
            "docker.show_hide",
            "docker.print_layer",
            "docker.lock_layer",
            "docker.edit_across_layers",
            "docker.move_up",
            "docker.move_down",
            "docker.show_on",
            "docker.master_in_front",
            "docker.desktop_takes_beside_page",
            "docker.master_to_page",
            "docker.layer_to_master",
            "docker.move_to_page",
            "docker.layer_properties",
            "docker.delete_layer",
        ])
        .iter()
        .copied()
        {
            match key {
                "docker.rename" => {
                    if show_item(ui, "docker.rename") && ui.ibutton("docker.rename").clicked() {
                        act.rename_layer = Some((layer.id, layer.name.clone()));
                        ui.close();
                    }
                }
                candidate if candidate == new_key => {
                    if show_item(ui, new_key) && ui.ibutton(new_key).clicked() {
                        if at.master && layer.page_range.is_some() {
                            act.new_range = true;
                        } else if at.master {
                            act.new_master = Some(layer.scope);
                        } else {
                            act.goto = elsewhere;
                            act.new_layer = true;
                        }
                        ui.close();
                    }
                }
                "docker.select_layer_objects" => {
                    if show_item(ui, "docker.select_layer_objects")
                        && ui
                            .add_enabled(
                                has,
                                crate::ui::btn_icons::Ib::new("docker.select_layer_objects"),
                            )
                            .clicked()
                    {
                        act.goto = elsewhere;
                        act.select_layer = Some(layer.id);
                        ui.close();
                    }
                }
                "menu.edit.cut" | "menu.edit.copy" | "menu.edit.paste" => {
                    if key == "menu.edit.cut" {
                        crate::ui::menus::sep(ui);
                    }
                    if let Some((what, _, on)) = [
                        (LayerClip::Cut, "menu.edit.cut", cuttable),
                        (LayerClip::Copy, "menu.edit.copy", has),
                        (LayerClip::Paste, "menu.edit.paste", can_paste),
                    ]
                    .into_iter()
                    .find(|(_, candidate, _)| *candidate == key)
                    {
                        if !show_item(ui, key) {
                            continue;
                        }
                        let action = ui.add_enabled(on, crate::ui::btn_icons::Ib::new(key));
                        let action = if what == LayerClip::Paste && !range_available {
                            action.rich_disabled_tip(tr("docker.range_no_current_page"))
                        } else {
                            action
                        };
                        if action.clicked() {
                            act.goto = elsewhere;
                            act.layer_clip = Some((layer.id, what));
                            ui.close();
                        }
                    }
                }
                "docker.show_hide" => {
                    crate::ui::menus::sep(ui);
                    let mut visible = layer.visible;
                    if show_item(ui, "docker.show_hide")
                        && ui.checkbox(&mut visible, tr("docker.show_hide")).clicked()
                    {
                        act.visible = Some((layer.id, visible));
                        ui.close();
                    }
                }
                "docker.print_layer" => {
                    let mut printable = layer.printable;
                    if show_item(ui, "docker.print_layer")
                        && ui
                            .checkbox(&mut printable, tr("docker.print_layer"))
                            .clicked()
                    {
                        act.printable = Some((layer.id, printable));
                        ui.close();
                    }
                }
                "docker.lock_layer" => {
                    let mut locked = layer.locked;
                    if show_item(ui, "docker.lock_layer")
                        && ui.checkbox(&mut locked, tr("docker.lock_layer")).clicked()
                    {
                        act.locked = Some((layer.id, locked));
                        ui.close();
                    }
                }
                "docker.edit_across_layers" => {
                    let mut across = app.settings.edit_across_layers;
                    if show_item(ui, key)
                        && ui
                            .checkbox(&mut across, tr(key))
                            .rich_tip(crate::ui::tips::pair(
                                "docker.edit_across_layers",
                                "docker.edit_across_layers_tip",
                            ))
                            .clicked()
                    {
                        act.across = Some(across);
                        ui.close();
                    }
                }
                "docker.move_up" => {
                    crate::ui::menus::sep(ui);
                    if show_item(ui, "docker.move_up")
                        && ui
                            .add_enabled(
                                at.index + 1 < at.count,
                                crate::ui::btn_icons::Ib::new("docker.move_up"),
                            )
                            .clicked()
                    {
                        act.reorder = Some((layer.id, at.index + 1));
                        ui.close();
                    }
                }
                "docker.move_down" => {
                    if show_item(ui, "docker.move_down")
                        && ui
                            .add_enabled(
                                at.index > 0,
                                crate::ui::btn_icons::Ib::new("docker.move_down"),
                            )
                            .clicked()
                    {
                        act.reorder = Some((layer.id, at.index.saturating_sub(1)));
                        ui.close();
                    }
                }
                "docker.show_on" => {
                    if at.master {
                        let has_scope = [
                            "options.scope_all",
                            "options.scope_odd",
                            "options.scope_even",
                        ]
                        .iter()
                        .any(|key| app.settings.context_menus.is_visible("objects_layer", key))
                            || (!layer.desktop
                                && app
                                    .settings
                                    .context_menus
                                    .is_visible("objects_layer", "docker.range_pages"));
                        if has_scope && show_item(ui, "docker.show_on") {
                            ui.imenu_button("docker.show_on", |ui| {
                                let _branch =
                                    crate::ui::context_menu_filter::Branch::new("docker.show_on");
                                let scopes =
                                    [MasterScope::All, MasterScope::Odd, MasterScope::Even];
                                let keys = scopes.map(scope_key);
                                for key in crate::ui::context_menu_filter::ordered_keys(&keys)
                                    .iter()
                                    .copied()
                                {
                                    let Some(scope) =
                                        scopes.into_iter().find(|scope| scope_key(*scope) == key)
                                    else {
                                        continue;
                                    };
                                    if show_item(ui, scope_key(scope))
                                        && ui
                                            .selectable_label(
                                                layer.page_range.is_none() && layer.scope == scope,
                                                tr(scope_key(scope)),
                                            )
                                            .clicked()
                                    {
                                        act.scope = Some((layer.id, scope));
                                        ui.close();
                                    }
                                }
                                if !layer.desktop
                                    && show_item(ui, "docker.range_pages")
                                    && ui
                                        .selectable_label(
                                            layer.page_range.is_some(),
                                            tr("docker.range_pages"),
                                        )
                                        .clicked()
                                {
                                    act.edit_range = Some(layer.id);
                                    ui.close();
                                }
                            });
                        }
                    }
                }
                "docker.master_in_front" => {
                    if at.master {
                        let mut front = layer.in_front;
                        if show_item(ui, "docker.master_in_front")
                            && ui
                                .checkbox(&mut front, tr("docker.master_in_front"))
                                .clicked()
                        {
                            act.in_front = Some((layer.id, front));
                            ui.close();
                        }
                    }
                }
                "docker.desktop_takes_beside_page" => {
                    if at.master && layer.desktop && show_item(ui, key) {
                        let mut takes = app.settings.desktop_takes_objects_beside_page;
                        if ui
                            .checkbox(&mut takes, tr(key))
                            .on_hover_text(tr("docker.desktop_takes_beside_page_tip"))
                            .clicked()
                        {
                            act.desktop_takes = Some(takes);
                            ui.close();
                        }
                    }
                }
                "docker.master_to_page" => {
                    if at.master
                        && show_item(ui, "docker.master_to_page")
                        && ui.ibutton("docker.master_to_page").clicked()
                    {
                        act.master_to_page = Some(layer.id);
                        ui.close();
                    }
                }
                "docker.layer_to_master" => {
                    if !at.master
                        && show_item(ui, "docker.layer_to_master")
                        && ui.ibutton("docker.layer_to_master").clicked()
                    {
                        act.layer_to_master = Some(layer.id);
                        ui.close();
                    }
                }
                "docker.move_to_page" => {
                    // To another page, with its objects and look (#1477).
                    let others: Vec<(PageId, String)> = app
                        .doc()
                        .pages
                        .iter()
                        .filter(|p| Some(p.id) != at.page)
                        .map(|p| (p.id, p.name.clone()))
                        .collect();
                    if !others.is_empty() && show_item(ui, "docker.move_to_page") {
                        ui.imenu_button("docker.move_to_page", |ui| {
                            let _branch =
                                crate::ui::context_menu_filter::Branch::new("docker.move_to_page");
                            for (page, name) in &others {
                                if crate::ui::menus::item_label_keyed(
                                    ui,
                                    "docker.move_to_page",
                                    name,
                                    "",
                                    true,
                                ) {
                                    act.layer_to_page = Some((layer.id, *page));
                                    ui.close();
                                }
                            }
                        });
                    }
                }
                "docker.layer_properties" => {
                    crate::ui::menus::sep(ui);
                    if show_item(ui, "docker.layer_properties")
                        && ui.ibutton("docker.layer_properties").clicked()
                    {
                        act.layer_props = Some(layer.id);
                        ui.close();
                    }
                }
                "docker.delete_layer" => {
                    crate::ui::menus::sep(ui);
                    if show_item(ui, "docker.delete_layer")
                        && ui
                            .add_enabled(
                                !deletable_layers_from(app.doc(), &target_layers(app, layer.id))
                                    .is_empty(),
                                crate::ui::btn_icons::Ib::new("docker.delete_layer"),
                            )
                            .clicked()
                    {
                        act.delete_layer = Some(layer.id);
                        ui.close();
                    }
                }
                _ => {}
            }
        }
    });
    if open {
        // While a PowerClip's contents are edited they stay listed inside
        // it (#3846: the frame showed as an object of its own beside
        // them), so a content can still be dragged out of it.
        let contents = app.clip_edit_contents();
        let frame = app.clip_frame_edit.as_ref().map(|(f, _)| *f);
        // Objects of the facing page selected with the current page's are
        // brought onto it only while selected (#3488, #3817): with every
        // page listed they stay under their own page and layer, where
        // they go back, instead of all showing under the current page as
        // if selecting had moved them there (#3935).
        let (away, here) = spread_borrowed(app, at.page, layer.id);
        // A search that the layer's own name does not answer lists only
        // the objects that do, and the groups holding them (#4258).
        let needle = layer_needle(ui);
        act.object_filter = if needle.is_empty() || layer_name_hit(layer, &needle) {
            String::new()
        } else {
            needle
        };
        for s in here.iter().filter_map(|id| app.doc().find_shape(*id)) {
            if act.object_filter.is_empty() || shape_hit(s, &act.object_filter) {
                object_rows(app, ui, s, LAYER_OBJECT_DEPTH, act);
            }
        }
        for s in layer.shapes.iter().rev() {
            if contents.contains(&s.id) || away.contains(&s.id) {
                continue;
            }
            if !act.object_filter.is_empty() && !shape_hit(s, &act.object_filter) {
                continue;
            }
            object_rows(app, ui, s, LAYER_OBJECT_DEPTH, act);
            if Some(s.id) == frame {
                for c in app
                    .page_layers()
                    .iter()
                    .flat_map(|l| l.shapes.iter().rev())
                    .filter(|c| contents.contains(&c.id))
                {
                    object_rows(app, ui, c, LAYER_OBJECT_DEPTH + 1, act);
                }
            }
        }
        act.object_filter.clear();
    }
}

/// With every page listed, the objects of the facing page brought onto
/// the page `page` to be selected with its own (#3488): the ones listed
/// on `layer` of `page` that belong elsewhere, and the ones whose own
/// page and layer these are, to list here (#3935).
fn spread_borrowed(
    app: &App,
    page: Option<PageId>,
    layer: LayerId,
) -> (Vec<ShapeId>, Vec<ShapeId>) {
    let Some(page) = page.filter(|_| app.settings.objects_all_pages) else {
        return (Vec::new(), Vec::new());
    };
    let away = app
        .spread_adopted
        .iter()
        .filter(|a| a.host == page)
        .map(|a| a.shape)
        .collect();
    let here = app
        .spread_adopted
        .iter()
        .rev()
        .filter(|a| a.home == page && a.layer == layer && a.host != page)
        .map(|a| a.shape)
        .collect();
    (away, here)
}

/// Opens the context menu of the object `id` from its Objects docker
/// row at `pos`: the menu the drawing shows for it (#2601). A locked
/// object gets the drawing's locked-object menu; another object is
/// selected first, unless it is part of the selection already (then the
/// menu is for the whole selection, as on the drawing).
pub(crate) fn open_row_menu(app: &mut App, id: ShapeId, pos: Pos2) {
    if let Some(layer) = layer_holding(app, id) {
        let outside_range = app
            .doc()
            .layer(layer)
            .is_ok_and(|layer| layer.page_range.is_some() && !layer.applies_to(app.page_index()));
        if outside_range && !activate_range_layer(app, layer) {
            app.context_menu = None;
            app.context_docker = None;
            return;
        }
    }
    let Some(s) = app.doc().find_shape(id) else {
        return;
    };
    let (locked, centre) = (s.locked, s.bounds().center());
    app.context_locked = None;
    app.tab_ruler_menu = None;
    if locked {
        app.context_locked = Some(id);
    } else if !app.selection.contains(&id) {
        app.select(vec![id]);
    }
    app.context_docker = Some(id);
    app.context_menu = Some((pos, centre));
}

/// Every layer an object can go to: each page's layers, then the master
/// layers (page name or none for the master page, layer id and name).
fn layer_targets(app: &App) -> Vec<(Option<String>, LayerId, String)> {
    let doc = app.doc();
    let mut out = Vec::new();
    for page in &doc.pages {
        for l in page.layers.iter().rev() {
            out.push((Some(page.name.clone()), l.id, l.name.clone()));
        }
    }
    for l in doc
        .master
        .iter()
        .rev()
        .filter(|layer| layer.page_range.is_none())
    {
        out.push((None, l.id, l.name.clone()));
    }
    for l in doc
        .master
        .iter()
        .rev()
        .filter(|layer| layer.page_range.is_some())
    {
        out.push((Some(tr("docker.range_pages")), l.id, l.name.clone()));
    }
    out
}

/// The Objects docker's own commands at the top of an object row's
/// context menu (#2601): Rename, Move to Layer and Copy to Layer for the selection
/// (or the object when it is not selected).
#[cfg(test)]
pub(crate) fn row_menu_items(app: &mut App, ui: &mut Ui, id: ShapeId, close: &mut bool) {
    for key in crate::ui::context_menu_filter::ordered_keys(&[
        "docker.rename",
        "docker.move_to_layer",
        "docker.copy_to_layer",
    ])
    .iter()
    .copied()
    {
        row_menu_item(app, ui, id, close, key);
    }
}

/// Draw one docker action so the shared object menu can order every root row.
pub(crate) fn row_menu_item(app: &mut App, ui: &mut Ui, id: ShapeId, close: &mut bool, key: &str) {
    use crate::ui::menus::{item, sub};
    match key {
        "docker.rename" => {
            let Some(name) = app.doc().find_shape(id).map(object_label) else {
                return;
            };
            if item(ui, "docker.rename", "", true) {
                *close = true;
                app.dialog = crate::ui::dialogs::Dialog::RenameObject { shape: id, name };
            }
        }
        "docker.move_to_layer" | "docker.copy_to_layer" => {
            let ids: Vec<ShapeId> = if app.selection.contains(&id) {
                app.selection.clone()
            } else {
                vec![id]
            };
            let own = layer_holding(app, id);
            let targets = layer_targets(app);
            let mut to = None;
            let copying = key == "docker.copy_to_layer";
            sub(ui, key, |ui| {
                to = layer_list(ui, key, &targets, |layer| copying || own != Some(layer));
            });
            if let Some(layer) = to {
                *close = true;
                if copying {
                    copy_to_layer(app, &ids, layer);
                } else {
                    move_to_layer(app, &ids, layer);
                }
            }
        }
        _ => {}
    }
}

/// The layers of [`layer_targets`] under their page names; the one
/// clicked, if any. `enabled` greys out the layers that cannot be picked.
fn layer_list(
    ui: &mut Ui,
    key: &str,
    targets: &[(Option<String>, LayerId, String)],
    enabled: impl Fn(LayerId) -> bool,
) -> Option<LayerId> {
    if !crate::ui::context_menus::show_item(ui, key) {
        return None;
    }
    let mut picked = None;
    let mut heading: Option<Option<String>> = None;
    for (page, layer, name) in targets {
        if heading.as_ref() != Some(page) {
            if heading.is_some() {
                crate::ui::menus::sep(ui);
            }
            ui.label(
                egui::RichText::new(page.clone().unwrap_or_else(|| tr("docker.master_page")))
                    .strong(),
            );
            heading = Some(page.clone());
        }
        if crate::ui::menus::item_label_keyed(ui, key, name, "", enabled(*layer)) {
            picked = Some(*layer);
        }
    }
    picked
}

/// The text naming the pages a master layer shows on.
fn scope_key(scope: MasterScope) -> &'static str {
    match scope {
        MasterScope::All => "options.scope_all",
        MasterScope::Odd => "options.scope_odd",
        MasterScope::Even => "options.scope_even",
    }
}

/// A Guides row: under the Master Page the guidelines every page shows
/// ("Guides (all pages)"), under a page (`page`) that page's own
/// (#1928). A click on the row makes them the guidelines edited (new
/// ones go there too); the eye shows or hides the row's guidelines
/// (#1775; with guidelines off, showing them turns them on), the
/// padlock locks or unlocks all of the row's; guidelines never print.
fn guides_row(
    app: &App,
    ui: &mut Ui,
    guides: &[tracedraw_core::document::Guide],
    page: Option<PageId>,
    act: &mut Actions,
) {
    let master = page.is_none();
    let chosen = app.edit_master_guides == master && page.is_none_or(|p| p == app.page);
    let (rect, resp) = row(ui, Look::Layer { active: chosen, selected: false });
    let desc = if master {
        "docker.guides_master_row_desc"
    } else {
        "docker.guides_row_desc"
    };
    let label = if master {
        tr("docker.guides_all_pages")
    } else {
        tr("docker.guides")
    };
    let resp = resp.rich_tip_with(|| format!("{label}\n{}", tr(desc)));
    if resp.clicked() {
        act.guides_master = Some(master);
        if let Some(p) = page.filter(|p| *p != app.page) {
            act.goto = Some(p);
        }
    }
    // A guideline row dragged here from another set: the row lights up,
    // and letting go moves the guideline (with the others selected with
    // it) into this set (#3599).
    if let Some((from, index)) = ui
        .ctx()
        .data(|d| d.get_temp::<(Option<PageId>, usize)>(egui::Id::new(GUIDE_DRAG)))
    {
        let (over, released) = ui.input(|i| {
            (
                i.pointer.interact_pos().is_some_and(|p| rect.contains(p)),
                i.pointer.any_released(),
            )
        });
        if over && from != page {
            ui.painter().rect_stroke(
                rect.shrink(1.0),
                2.0,
                Stroke::new(1.5, GUIDE_BLUE),
                egui::StrokeKind::Inside,
            );
            if released {
                act.move_guides = Some((from, index, page));
            }
        }
    }
    // Right-click: Cut, Copy and Paste all of the row's guidelines, and
    // Move them to all pages, the odd pages or the even pages (#3578).
    resp.context_menu(|ui| {
        use crate::guides::GuidesTarget;
        let any = !guides.is_empty();
        let can_paste = app.clipboard.as_ref().is_some_and(|c| !c.guides.is_empty());
        let mut pick = None;
        for (key, enabled, cmd) in [
            ("menu.edit.cut", any && guides.iter().any(|g| !g.locked), GuidesCmd::Cut),
            ("menu.edit.copy", any, GuidesCmd::Copy),
                    ] {
            if ui
                .add_enabled(enabled, crate::ui::btn_icons::Ib::new(key))
                .clicked()
            {
                pick = Some(cmd);
                ui.close();
            }
        }
        // Paste into this set, also when it has none yet (#3599).
        if ui
            .add_enabled(can_paste, crate::ui::btn_icons::Ib::new("menu.edit.paste"))
            .clicked()
        {
            act.paste_guides = Some(page);
            ui.close();
        }
        ui.separator();
        ui.add_enabled_ui(any, |ui| {
            ui.imenu_button("guides.move_to", |ui| {
                for (key, target) in [
                    ("dialog.all_pages", GuidesTarget::AllPages),
                    ("options.scope_odd", GuidesTarget::OddPages),
                    ("options.scope_even", GuidesTarget::EvenPages),
                ] {
                    let enabled = !(master && target == GuidesTarget::AllPages);
                    if ui
                        .add_enabled(enabled, crate::ui::btn_icons::Ib::new(key))
                        .clicked()
                    {
                        pick = Some(GuidesCmd::MoveTo(target));
                        ui.close();
                    }
                }
            });
        });
        if let Some(cmd) = pick {
            act.guides_cmd = Some((master, cmd));
            if let Some(p) = page.filter(|p| *p != app.page) {
                act.goto = Some(p);
            }
        }
    });
    let base = match page {
        Some(p) => egui::Id::new(("objects_guides", p.raw())),
        None => egui::Id::new("objects_guides"),
    };
    // The row opens to list its guidelines one by one, each with its
    // own eye and padlock (#1775).
    let open_id = base.with("open");
    let mut open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);
    // A guideline just selected in this row's set (dragged out of a
    // ruler, picked on the page or added in Guidelines Setup) opens the
    // row and scrolls to it, so a new guideline shows in the docker
    // (#2882).
    let seen_id = base.with("seen");
    let picked: Vec<usize> = if chosen {
        app.selected_guides
            .iter()
            .copied()
            .filter(|i| *i < guides.len())
            .collect()
    } else {
        Vec::new()
    };
    let seen = ui.data(|d| d.get_temp::<Vec<usize>>(seen_id));
    let reveal = !picked.is_empty() && seen.as_ref() != Some(&picked);
    if seen.as_ref() != Some(&picked) {
        ui.data_mut(|d| d.insert_temp(seen_id, picked.clone()));
    }
    if reveal && !open {
        open = true;
        ui.data_mut(|d| d.insert_temp(open_id, true));
    }
    if !guides.is_empty() {
        let (ar, a) = hot(
            ui,
            rect,
            rect.left() + 3.0 + INDENT,
            14.0,
            base.with("arrow"),
        );
        let a = a.rich_tip_with(|| crate::ui::tips::keyed("docker.expand_row"));
        paint_arrow(ui, ar, open);
        if a.clicked() {
            ui.data_mut(|d| d.insert_temp(open_id, !open));
        }
    }
    let mut x = rect.left() + 3.0 + INDENT + 16.0;
    // Each guidelines row has its own eye (#1775).
    let shown = app.show_guides && app.doc().guides_shown(page);
    let (er, e) = hot(ui, rect, x, 18.0, base.with("eye"));
    paint_eye(ui, er, shown);
    if e.rich_tip(tr("docker.show_hide")).clicked() {
        act.toggle_guides = Some((page, !shown));
    }
    x += 20.0;
    paint_printer(ui, square(rect, x, 18.0), false);
    x += 20.0;
    let any = !guides.is_empty();
    let locked = any && guides.iter().all(|g| g.locked);
    let (lr, l) = hot(ui, rect, x, 18.0, base.with("lock"));
    paint_lock(ui, lr, locked);
    if l.rich_tip(tr("docker.lock_guides")).clicked() && any {
        act.lock_guides = Some((master, !locked));
        if let Some(p) = page.filter(|p| *p != app.page) {
            act.goto = Some(p);
        }
    }
    x += 22.0;
    paint_guides_icon(ui.painter(), square(rect, x, 16.0));
    x += 22.0;
    ui.painter().text(
        Pos2::new(x, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(13.0),
        Tokens::text(),
    );
    if open && !guides.is_empty() {
        let scroll_to = reveal.then(|| picked[0]);
        guide_rows(app, ui, guides, page, base, shown, scroll_to, act);
    }
}

/// The guidelines of a Guides row, one row each: its eye, its padlock
/// and where it is; a click selects it on the page (#1775).
#[allow(clippy::too_many_arguments)]
fn guide_rows(
    app: &App,
    ui: &mut Ui,
    guides: &[tracedraw_core::document::Guide],
    page: Option<PageId>,
    base: egui::Id,
    set_shown: bool,
    scroll_to: Option<usize>,
    act: &mut Actions,
) {
    let master = page.is_none();
    let edited = app.edit_master_guides == master && page.is_none_or(|p| p == app.page);
    for (i, g) in guides.iter().enumerate() {
        let selected = edited && app.selected_guides.contains(&i);
        let (rect, resp) = row(ui, Look::Object { selected });
        if scroll_to == Some(i) {
            ui.scroll_to_rect(rect, None);
        }
        let text = crate::ui::layout_options::guide_text(app, g);
        let kind = match g.line {
            GuideLine::Horizontal { .. } => tr("options.horizontal"),
            GuideLine::Vertical { .. } => tr("options.vertical"),
            GuideLine::Angled { .. } => tr("options.angled"),
        };
        let resp = resp.rich_tip_with(|| format!("{kind} {text}\n{}", tr("docker.guide_row_desc")));
        let id = base.with(("guide", i));
        let mut x = rect.left() + 3.0 + LAYER_OBJECT_DEPTH as f32 * INDENT + 16.0;
        let (er, e) = hot(ui, rect, x, 18.0, id.with("eye"));
        paint_eye(ui, er, set_shown && !g.hidden);
        let mut done = None;
        if e.rich_tip(tr("docker.show_hide")).clicked() {
            done = Some(GuideRowAct::Hide(!g.hidden));
        }
        x += 20.0;
        let (lr, l) = hot(ui, rect, x, 18.0, id.with("lock"));
        paint_lock(ui, lr, g.locked);
        if l.rich_tip(tr("docker.lock_guides")).clicked() {
            done = Some(GuideRowAct::Lock(!g.locked));
        }
        x += 22.0;
        let color = if g.hidden || !set_shown {
            Tokens::text_dim()
        } else {
            Tokens::text()
        };
        // A named guideline shows its name, where it is beside it.
        let shown_text = match g.name.as_deref().map(str::trim) {
            Some(n) if !n.is_empty() => format!("{n}  ({kind} {text})"),
            _ => format!("{kind}  {text}"),
        };
        ui.painter().text(
            Pos2::new(x, rect.center().y),
            egui::Align2::LEFT_CENTER,
            shown_text,
            egui::FontId::proportional(12.0),
            color,
        );
        if done.is_none() && resp.clicked() {
            let m = ui.input(|i| i.modifiers);
            done = Some(GuideRowAct::Select {
                ctrl: m.command,
                shift: m.shift,
            });
        }
        // Dragged onto another Guides row, it moves there (#3599).
        if resp.drag_started() && !g.locked {
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::new(GUIDE_DRAG), (page, i)));
        }
        // Right-click: Delete, Cut, Copy, Paste, Lock or Unlock, Hide or
        // Show (#3012).
        crate::ui::context_menus::filter_popup(
            egui::Popup::context_menu(&resp),
            &app.settings.context_menus,
            "objects_guide",
        )
        .show(|ui| {
            if let Some(cmd) = guide_menu_items(app, ui, g) {
                done = Some(GuideRowAct::Menu(cmd));
            }
        });
        if let Some(d) = done {
            act.guide_row = Some((master, i, d));
            if let Some(p) = page.filter(|p| *p != app.page) {
                act.goto = Some(p);
            }
        }
    }
}

/// The row keeps its existing commands; preferences only choose their visibility
/// and sibling order. Conditional lock and visibility actions use their own keys.
fn guide_menu_items(
    app: &App,
    ui: &mut Ui,
    guide: &tracedraw_core::document::Guide,
) -> Option<GuideCmd> {
    let _scope = crate::ui::context_menus::Scope::new(&app.settings.context_menus, "objects_guide");
    let can_paste = app.clipboard.as_ref().is_some_and(|c| !c.guides.is_empty());
    let lock_key = if guide.locked {
        "guides.unlock"
    } else {
        "guides.lock"
    };
    let hide_key = if guide.hidden {
        "guides.show"
    } else {
        "menu.object.hide"
    };
    let actions = [
        ("menu.edit.delete", !guide.locked, GuideCmd::Delete),
        ("menu.edit.cut", !guide.locked, GuideCmd::Cut),
        ("menu.edit.copy", true, GuideCmd::Copy),
        ("menu.edit.paste", can_paste, GuideCmd::Paste),
        (lock_key, true, GuideCmd::Lock(!guide.locked)),
        (hide_key, true, GuideCmd::Hide(!guide.hidden)),
        ("docker.rename", true, GuideCmd::Rename),
    ];
    let keys = actions.map(|(key, _, _)| key);
    let mut chosen = None;
    for key in crate::ui::context_menu_filter::ordered_keys(&keys)
        .iter()
        .copied()
    {
        let Some(&(_, enabled, cmd)) = actions.iter().find(|(candidate, _, _)| *candidate == key)
        else {
            continue;
        };
        if !crate::ui::context_menus::show_item(ui, key) {
            continue;
        }
        let response = ui.add_enabled(enabled, crate::ui::btn_icons::Ib::new(key));
        #[cfg(test)]
        ui.ctx().data_mut(|data| {
            data.insert_temp(
                egui::Id::new(("objects_guide_menu_row", key)),
                response.rect,
            );
        });
        if response.clicked() {
            chosen = Some(cmd);
            ui.close();
        }
    }
    chosen
}

/// A command of a guideline row's menu (#3012), on the selected
/// guidelines when the row is one of them, else on the row's alone.
fn guide_menu_command(app: &mut App, i: usize, cmd: GuideCmd) {
    if app.page_guides().get(i).is_none() {
        return;
    }
    let before = app.selected_guides.clone();
    app.select(Vec::new());
    app.selected_guides = if before.contains(&i) {
        before
    } else {
        app.guide_rotate = None;
        vec![i]
    };
    let chosen = app.selected_guides.clone();
    match cmd {
        GuideCmd::Delete => app.delete_command(),
        GuideCmd::Cut => app.cut(),
        GuideCmd::Copy => {
            app.copy_selected_guides();
        }
        GuideCmd::Paste => app.paste(),
        GuideCmd::Lock(locked) => app.set_guides_locked(&chosen, locked),
        GuideCmd::Hide(hidden) => {
            for g in chosen {
                app.set_guide_hidden(g, hidden);
            }
        }
        GuideCmd::Rename => {
            let name = app
                .page_guides()
                .get(i)
                .and_then(|g| g.name.clone())
                .unwrap_or_default();
            app.dialog = crate::ui::dialogs::Dialog::RenameGuide { index: i, name };
        }
    }
}

/// A click on a guideline's row (#3575): alone it selects that one; with
/// Ctrl it joins the selection or leaves it; with Shift the guidelines
/// from the last one clicked to it (in the docker's order) are selected.
/// Hidden guidelines are never selected.
fn select_guide_row(app: &mut App, i: usize, ctrl: bool, shift: bool) {
    let guides = app.page_guides();
    if guides.get(i).is_none_or(|g| g.hidden) {
        return;
    }
    app.select(Vec::new());
    app.selected_layers.clear();
    app.guide_rotate = None;
    if shift {
        let from = app.selected_guides.last().copied().unwrap_or(i);
        let (a, b) = (from.min(i), from.max(i));
        let mut chosen: Vec<usize> = if ctrl {
            app.selected_guides.clone()
        } else {
            Vec::new()
        };
        for k in a..=b {
            if guides.get(k).is_some_and(|g| !g.hidden) && !chosen.contains(&k) {
                chosen.push(k);
            }
        }
        // The one clicked last stays the anchor of the next range.
        chosen.retain(|k| *k != i);
        chosen.push(i);
        app.selected_guides = chosen;
    } else if ctrl {
        if app.selected_guides.contains(&i) {
            app.selected_guides.retain(|k| *k != i);
        } else {
            app.selected_guides.push(i);
        }
    } else {
        app.selected_guides = vec![i];
    }
}

/// A command of a Guides row's menu (#3578), on all of the row's
/// guidelines (the edited set is the row's already).
fn guides_menu_command(app: &mut App, cmd: GuidesCmd) {
    let all: Vec<usize> = (0..app.page_guides().len()).collect();
    if all.is_empty() {
        return;
    }
    match cmd {
        GuidesCmd::Copy => {
            app.select(Vec::new());
            app.selected_guides = all;
            app.copy_selected_guides();
        }
        GuidesCmd::Cut => {
            app.select(Vec::new());
            app.selected_guides = all;
            if app.copy_selected_guides() {
                app.delete_selected_guides();
            }
        }
        GuidesCmd::MoveTo(target) => app.move_guides_to(target),
    }
}

/// The Document Grid row of the Master Page: the eye shows or hides the
/// grid, which never prints and cannot be edited.
fn grid_row(app: &App, ui: &mut Ui, act: &mut Actions) {
    let (rect, resp) = row(ui, Look::Layer { active: false, selected: false });
    resp.rich_tip_with(|| crate::ui::tips::pair("menu.view.document_grid", "docker.grid_row_desc"));
    let mut x = rect.left() + 3.0 + INDENT + 16.0;
    let (er, e) = hot(ui, rect, x, 18.0, egui::Id::new("objects_grid_eye"));
    paint_eye(ui, er, app.show_grid);
    if e.rich_tip(tr("docker.show_hide")).clicked() {
        act.toggle_grid = true;
    }
    x += 20.0;
    paint_printer(ui, square(rect, x, 18.0), false);
    x += 20.0;
    paint_padlock(
        ui.painter(),
        square(rect, x, 18.0),
        true,
        Tokens::icon_off(),
    );
    x += 22.0;
    paint_grid_icon(ui.painter(), square(rect, x, 16.0));
    x += 22.0;
    ui.painter().text(
        Pos2::new(x, rect.center().y),
        egui::Align2::LEFT_CENTER,
        tr("menu.view.document_grid"),
        egui::FontId::proportional(13.0),
        Tokens::text(),
    );
}

/// The row of `s` at nesting `depth`, then its children when it is an
/// open group.
fn object_rows(app: &App, ui: &mut Ui, s: &Shape, depth: usize, act: &mut Actions) {
    let selected = act.selected.contains(&s.id);
    act.row_order.push(s.id);
    let height = (act.thumb_size + 6.0).max(ROW_H);
    // A row scrolled out of sight only takes its place: asking egui
    // whether the pointer is on each of 50,000 rows took most of a second
    // a frame. The row being dragged still answers, so a drag that
    // scrolls it away goes on.
    let (id, rect) = ui.allocate_space(Vec2::new(ui.available_width(), height));
    if act.scroll_to == Some(s.id) {
        ui.scroll_to_rect(rect, None);
    }
    let visible = ui.is_rect_visible(rect);
    let resp = (visible || ui.ctx().is_being_dragged(id))
        .then(|| ui.interact(rect, id, Sense::click_and_drag()));
    let children: Option<&Vec<Shape>> = match &s.kind {
        ShapeKind::Group { children } => Some(children),
        // A PowerClip opens to its contents, front first (#3053).
        ShapeKind::ClipFrame { contents, .. } if !contents.is_empty() => Some(contents),
        _ => None,
    };
    // While a search lists only matching objects, a group that holds one
    // (not matching itself) shows open with only those (#4258); a group
    // that matches shows all it holds.
    let filter = std::mem::take(&mut act.object_filter);
    let inner = if filter.is_empty() || own_hit(s, &filter) {
        String::new()
    } else {
        filter.clone()
    };
    let expanded = app.objects_open_groups.contains(&s.id) || !inner.is_empty();
    // Rows scrolled out of sight are only laid out: nothing is drawn or
    // asked of them, so a long list costs little.
    if let (Some(resp), true) = (resp, visible) {
        paint_row(ui, Look::Object { selected }, rect, &resp);
        object_row(app, ui, s, (rect, resp), depth, expanded, act);
    }
    if let (Some(children), true) = (children, expanded) {
        for c in children.iter().rev() {
            if inner.is_empty() || shape_hit(c, &inner) {
                act.object_filter = inner.clone();
                object_rows(app, ui, c, depth + 1, act);
            }
        }
    }
    act.object_filter = filter;
}

/// The contents of the row of `s`: the group arrow, the thumbnail, the
/// name, the eye and the padlock.
fn object_row(
    app: &App,
    ui: &mut Ui,
    s: &Shape,
    (rect, resp): (Rect, egui::Response),
    depth: usize,
    expanded: bool,
    act: &mut Actions,
) {
    let mut x = rect.left() + 3.0 + depth as f32 * INDENT;
    let opens = match &s.kind {
        ShapeKind::Group { .. } => true,
        ShapeKind::ClipFrame { contents, .. } => !contents.is_empty(),
        _ => false,
    };
    if opens {
        let (ar, a) = hot(
            ui,
            rect,
            x,
            14.0,
            egui::Id::new(("objects_group", s.id.raw())),
        );
        let a = a.rich_tip_with(|| crate::ui::tips::keyed("docker.expand_row"));
        paint_arrow(ui, ar, expanded);
        if a.clicked() {
            act.expand_group = Some(s.id);
        }
    }
    x += 16.0;
    let size = act.thumb_size;
    paint_thumb(
        app,
        ui,
        Rect::from_min_size(
            Pos2::new(x, rect.center().y - size / 2.0),
            Vec2::splat(size),
        ),
        s,
        act,
    );
    x += size + 7.0;
    let label = object_label(s);
    let resp = resp.rich_tip_with(|| format!("{label}\n{}", tr("docker.object_row_desc")));
    let color = if s.visible {
        Tokens::text()
    } else {
        Tokens::text_dim()
    };
    // The name stops short of the eye and padlock.
    let name_room = Rect::from_min_max(rect.min, Pos2::new(rect.right() - 48.0, rect.max.y));
    ui.painter().with_clip_rect(name_room).text(
        Pos2::new(x, rect.center().y),
        egui::Align2::LEFT_CENTER,
        &label,
        egui::FontId::proportional(13.0),
        color,
    );
    // Its own eye and padlock at the right end of the row, so an object
    // is hidden or locked from the list, and a locked one unlocked (#832).
    let base = egui::Id::new(("objects_object", s.id.raw()));
    let lock_x = rect.right() - 22.0;
    let eye_x = lock_x - 20.0;
    let (er, e) = hot(ui, rect, eye_x, 18.0, base.with("eye"));
    paint_eye(ui, er, s.visible);
    if e.rich_tip(tr("docker.show_hide")).clicked() {
        act.object_visible = Some((s.id, !s.visible));
    }
    let (lr, l) = hot(ui, rect, lock_x, 18.0, base.with("lock"));
    paint_lock(ui, lr, s.locked);
    let lock_tip = if s.locked {
        "menu.object.unlock_object"
    } else {
        "menu.object.lock_object"
    };
    if l.rich_tip(tr(lock_tip)).clicked() {
        act.object_locked = Some((s.id, !s.locked));
    }
    // A row dragged along the list moves its object (with the rest of
    // the selection when it is selected) in the stacking order where it
    // is let go (see `drag_rows`): among a layer's objects, among a
    // group's, into a group or out of one (#2672).
    act.object_rows.push((s.id, rect));
    if resp.drag_started() && !s.locked {
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::new(ROW_DRAG), s.id));
    }
    if resp.double_clicked() {
        act.rename_object = Some((s.id, s.name.clone().unwrap_or_else(|| label.clone())));
    } else if resp.clicked() {
        act.goto = act.row_page.filter(|p| *p != app.page);
        act.select = Some(s.id);
    }
    // A right click opens the drawing's own object menu for it (#2601).
    // A finger held on the row (which egui takes for a right click, ending
    // any drag) starts the row's drag here: moved, the finger drags the
    // row; lifted where it was, it opens the menu (#2948). The menu opened
    // under the held finger and the row could not be dragged.
    if resp.long_touched() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let page = act.row_page.filter(|p| *p != app.page);
            ui.ctx().data_mut(|d| {
                d.insert_temp(egui::Id::new(ROW_HOLD), (s.id, pos, page));
                if !s.locked {
                    d.insert_temp(egui::Id::new(ROW_DRAG), s.id);
                }
            });
        }
    } else if resp.secondary_clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            act.goto = act.goto.or(act.row_page.filter(|p| *p != app.page));
            act.object_menu = Some((s.id, pos));
        }
    }
}

/// The thumbnail of `s` in the square `r`: the object drawn small on a
/// white tile, centred; dimmed when the object is hidden. Until its
/// picture is drawn (a few per frame), the swatch of its fill and outline
/// stands in.
fn paint_thumb(app: &App, ui: &Ui, r: Rect, s: &Shape, act: &mut Actions) {
    let p = ui.painter();
    p.rect_filled(r, 2.0, Color32::WHITE);
    p.rect_stroke(
        r,
        2.0,
        Stroke::new(1.0, Tokens::border()),
        egui::epaint::StrokeKind::Inside,
    );
    let ppp = ui.ctx().pixels_per_point().max(0.5);
    let inner = r.shrink(2.0);
    let px = (inner.width() * ppp).round().max(4.0) as u32;
    match act.thumbs.get(ui.ctx(), app.doc(), act.drawing, s, px) {
        crate::object_thumbs::Thumb::Picture(id, [w, h]) => {
            let size = Vec2::new(w as f32, h as f32) / ppp;
            // An older picture of another size still fits the tile.
            let fit = (inner.width() / size.x.max(size.y).max(1.0)).min(1.0);
            let at = Rect::from_center_size(inner.center(), size * fit);
            let tint = if s.visible {
                Color32::WHITE
            } else {
                Color32::from_white_alpha(90)
            };
            p.image(
                id,
                at,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                tint,
            );
        }
        crate::object_thumbs::Thumb::Empty => {}
        crate::object_thumbs::Thumb::Pending => {
            let side = (inner.width() * 0.6).max(8.0);
            paint_swatch(
                ui,
                Rect::from_center_size(inner.center(), Vec2::splat(side)),
                s,
            );
        }
    }
}

/// The bar under the tree: New Layer and New Master Layer (all pages; its
/// context menu offers odd or even pages) at the left, Delete at the right.
fn bottom_bar(app: &mut App, ui: &mut Ui) {
    let (bar, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), BAR_H), Sense::hover());
    ui.painter()
        .hline(bar.x_range(), bar.min.y, Stroke::new(1.0, Tokens::border()));
    let slot =
        |x: f32| Rect::from_center_size(Pos2::new(x, bar.center().y + 1.0), Vec2::splat(24.0));
    let new_r = slot(bar.min.x + 14.0);
    let master_r = slot(bar.min.x + 42.0);
    let pages_r = slot(bar.min.x + 70.0);
    let delete_r = slot(bar.max.x - 14.0);
    let can_delete = !app.selection.is_empty() || deletable_layer(app).is_some();
    let new = ui
        .interact(new_r, egui::Id::new("objects_new_layer"), Sense::click())
        .rich_tip(tr("docker.new_layer"));
    let master = ui
        .interact(
            master_r,
            egui::Id::new("objects_new_master"),
            Sense::click(),
        )
        .rich_tip(tr("docker.new_master_layer"));
    let all_pages = app.settings.objects_all_pages;
    let pages = ui
        .interact(pages_r, egui::Id::new("objects_all_pages"), Sense::click())
        .rich_tip(tr("docker.all_pages"));
    let delete = ui
        .interact(
            delete_r,
            egui::Id::new("objects_delete"),
            if can_delete {
                Sense::click()
            } else {
                Sense::hover()
            },
        )
        .rich_tip(tr("docker.delete_hint"));
    let p = ui.painter();
    for (r, resp, on) in [
        (new_r, &new, true),
        (master_r, &master, true),
        (pages_r, &pages, true),
        (delete_r, &delete, can_delete),
    ] {
        if on && resp.hovered() {
            p.rect_filled(r, 2.0, Tokens::tool_hover());
        }
    }
    // Show all pages: on like a pressed tool button.
    if all_pages {
        p.rect_filled(pages_r, 2.0, Tokens::tool_active());
    }
    let sheet =
        |dx: f32| Rect::from_center_size(pages_r.center() + Vec2::new(dx, dx), Vec2::splat(13.0));
    paint_page_icon(p, sheet(-2.5), false);
    paint_page_icon(p, sheet(2.5), false);
    let icon = |r: Rect| Rect::from_center_size(r.center(), Vec2::splat(16.0));
    paint_layer_icon(p, icon(new_r), false);
    paint_plus_badge(p, icon(new_r));
    paint_layer_icon(p, icon(master_r), true);
    paint_plus_badge(p, icon(master_r));
    paint_bin(p, icon(delete_r), can_delete);
    let mut scoped = None;
    let mut new_range = false;
    crate::ui::context_menus::filter_popup(
        egui::Popup::context_menu(&master),
        &app.settings.context_menus,
        "objects_new_master",
    )
    .show(|ui| {
        use crate::ui::context_menus::show_item;
        let _scope =
            crate::ui::context_menus::Scope::new(&app.settings.context_menus, "objects_new_master");
        const NEW_LAYERS: [(MasterScope, &str); 3] = [
            (MasterScope::All, "options.new_master_all"),
            (MasterScope::Odd, "options.new_master_odd"),
            (MasterScope::Even, "options.new_master_even"),
        ];
        for key in crate::ui::context_menu_filter::ordered_keys(&[
            "options.new_master_all",
            "options.new_master_odd",
            "options.new_master_even",
            "docker.new_range_layer",
            "docker.master_in_front",
        ])
        .iter()
        .copied()
        {
            if let Some((scope, _)) = NEW_LAYERS
                .iter()
                .copied()
                .find(|(_, candidate)| *candidate == key)
            {
                if show_item(ui, key) && ui.ibutton(key).clicked() {
                    scoped = Some(scope);
                    ui.close();
                }
            } else if key == "docker.new_range_layer" {
                if show_item(ui, key) && ui.ibutton(key).clicked() {
                    new_range = true;
                    ui.close();
                }
            } else {
                // Which kinds of master layer are drawn over the page's own
                // layers: the ones the drawing has and the new ones (#1132).
                crate::ui::menus::sep(ui);
                let has_scope = [
                    "options.scope_all",
                    "options.scope_odd",
                    "options.scope_even",
                ]
                .iter()
                .any(|key| {
                    app.settings
                        .context_menus
                        .is_visible("objects_new_master", key)
                });
                if has_scope && show_item(ui, "docker.master_in_front") {
                    master_front_choices(app, ui);
                }
            }
        }
    });
    if new.clicked() {
        new_layer(app);
    }
    if master.clicked() {
        new_master_layer(app, MasterScope::All);
    }
    if pages.clicked() {
        app.settings.objects_all_pages = !all_pages;
    }
    if let Some(scope) = scoped {
        new_master_layer(app, scope);
    }
    if new_range {
        open_new_range_layer(app);
    }
    if can_delete && delete.clicked() {
        delete_current(app);
    }
    // The thumbnail size between the buttons, when there is room.
    let zoom = Rect::from_min_max(
        Pos2::new(pages_r.right() + 10.0, bar.min.y + 2.0),
        Pos2::new(delete_r.left() - 10.0, bar.max.y),
    );
    if zoom.width() >= 70.0 {
        thumb_size_control(app, ui, zoom);
    }
}

/// The thumbnail size control in `r`: a small tile, a slider and a large
/// tile; the wheel over it changes the size too.
fn thumb_size_control(app: &mut App, ui: &mut Ui, r: Rect) {
    use crate::object_thumbs::{clamp_size, MAX_SIZE, MIN_SIZE, STEP};
    let small = Rect::from_center_size(Pos2::new(r.min.x + 4.0, r.center().y), Vec2::splat(7.0));
    let large = Rect::from_center_size(Pos2::new(r.max.x - 7.0, r.center().y), Vec2::splat(13.0));
    for t in [small, large] {
        ui.painter().rect_stroke(
            t,
            1.0,
            Stroke::new(1.0, Tokens::icon()),
            egui::epaint::StrokeKind::Inside,
        );
    }
    let groove = Rect::from_min_max(
        Pos2::new(small.right() + 6.0, r.min.y),
        Pos2::new(large.left() - 6.0, r.max.y),
    );
    let mut size = clamp_size(app.settings.object_thumb_size);
    let slider = ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(groove)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
        |ui| {
            ui.spacing_mut().slider_width = groove.width();
            ui.add(
                crate::ui::Slider::new(&mut size, MIN_SIZE..=MAX_SIZE)
                    .step_by(f64::from(STEP))
                    .show_value(false),
            )
            .rich_tip(tr("docker.thumb_size"))
            .rect
        },
    );
    app.settings.object_thumb_size = clamp_size(size);
    // The slider steps with the wheel over itself (#1335); the rest of
    // the control (the tiles, around the groove) steps here, once a notch.
    if ui.rect_contains_pointer(r) && !ui.rect_contains_pointer(slider.inner) {
        if let Some(steps) = crate::ui::field::wheel_steps(ui, egui::Id::new("objects_bar_wheel")) {
            zoom_thumbs(app, steps);
        }
    }
}

/// `key`'s name with the first number from `from` up that `names` does
/// not use yet.
fn free_name(names: &[String], key: &str, from: usize) -> String {
    (from..)
        .map(|n| trf(key, &[("n", &n.to_string())]))
        .find(|name| !names.contains(name))
        .unwrap_or_default()
}

/// Add a layer on top of the current page's and make it active.
pub fn new_layer(app: &mut App) {
    let page = app.page;
    let names: Vec<String> = app
        .doc()
        .page(page)
        .map(|p| p.layers.iter().map(|l| l.name.clone()).collect())
        .unwrap_or_default();
    let name = free_name(&names, "docker.layer_n", names.len() + 1);
    app.run(Command::AddLayer { page, name });
    app.picked_layer = app
        .doc()
        .page(page)
        .ok()
        .and_then(|p| p.layers.last())
        .map(|l| l.id);
    app.selected_layers = app.picked_layer.into_iter().collect();
    app.objects_closed_pages.remove(&page);
}

/// Add a master layer showing on the pages `scope` names and make it
/// active.
pub fn new_master_layer(app: &mut App, scope: MasterScope) {
    // The desktop layer is not one of the numbered master layers.
    let names: Vec<String> = app
        .doc()
        .master
        .iter()
        .filter(|l| !l.desktop)
        .map(|l| l.name.clone())
        .collect();
    let name = free_name(&names, "docker.master_layer_n", names.len() + 1);
    app.run(Command::AddMasterLayer { name, scope });
    app.picked_layer = app.doc().master.last().map(|l| l.id);
    app.selected_layers = app.picked_layer.into_iter().collect();
    app.objects_master_closed = false;
    // In front of the page's layers when the preferences say so for
    // master layers of these pages (#1132, #1477): one undo step.
    if master_front_default(app, scope) {
        if let Some(layer) = app.picked_layer {
            if let Err(e) = app.engine.amend(&Command::SetLayerInFront {
                layer,
                in_front: true,
            }) {
                app.status = e.to_string();
            }
        }
    }
}

/// The editable draft of a new shared layer for a page range.
#[derive(Debug, Clone, PartialEq)]
pub struct NewRangeLayer {
    pub name: String,
    pub page_range: String,
}

impl NewRangeLayer {
    pub fn of(app: &App) -> Self {
        let names = app
            .doc()
            .master
            .iter()
            .map(|layer| layer.name.clone())
            .collect::<Vec<_>>();
        Self {
            name: free_name(&names, "docker.range_layer_n", 1),
            page_range: (app.page_index() + 1).to_string(),
        }
    }

    pub fn valid(&self) -> bool {
        !self.name.trim().is_empty() && parsed_range(&self.page_range).is_some()
    }
}

pub fn open_new_range_layer(app: &mut App) {
    app.dialog = crate::ui::dialogs::Dialog::NewRangeLayer(NewRangeLayer::of(app));
}

pub fn open_layer_range(app: &mut App, layer: LayerId) {
    if let Some(mut state) = LayerProps::of(app, layer) {
        if state.master && !state.desktop {
            state.use_page_range = true;
            app.dialog = crate::ui::dialogs::Dialog::LayerProperties(state);
        }
    }
}

/// Create the complete layer as one document command and one undo step.
pub fn create_range_layer(app: &mut App, state: &NewRangeLayer) -> bool {
    if !state.valid() {
        return false;
    }
    let Some(page_range) = parsed_range(&state.page_range) else {
        return false;
    };
    let before = app.doc().master.len();
    app.run(Command::AddRangeLayer {
        name: state.name.trim().to_string(),
        page_range,
    });
    if app.doc().master.len() == before {
        return false;
    }
    if let Some(layer) = app.doc().master.last().map(|layer| layer.id) {
        activate_range_layer(app, layer);
    }
    app.objects_ranges_closed = false;
    true
}

/// Picking a range layer takes the view to a page where it can be edited.
fn activate_range_layer(app: &mut App, id: LayerId) -> bool {
    let first = app
        .doc()
        .master
        .iter()
        .find(|layer| layer.id == id)
        .and_then(|layer| {
            let range = layer.page_range.as_ref()?;
            if range.contains(app.page_index()) {
                Some(app.page_index())
            } else {
                app.doc()
                    .pages
                    .iter()
                    .enumerate()
                    .find_map(|(index, _)| range.contains(index).then_some(index))
            }
        });
    if let Some(source) = app.active_layer() {
        app.prepare_layer_range_change(source);
    }
    if let Some(index) = first {
        if index != app.page_index() {
            app.goto_page(index);
        }
        app.select(Vec::new());
        app.picked_layer = Some(id);
        true
    } else {
        app.select(Vec::new());
        app.picked_layer = None;
        app.status = tr("docker.range_no_current_page");
        false
    }
}

fn parsed_range(text: &str) -> Option<PageRanges> {
    PageRanges::parse(text).filter(|range| !range.is_empty())
}

/// The range field is shared by creation and Layer Properties.
pub fn page_range_field(ui: &mut Ui, text: &mut String, page_count: usize) {
    ui.horizontal(|ui| {
        ui.label(tr("docker.page_range"));
        ui.add(
            egui::TextEdit::singleline(text)
                .desired_width(250.0)
                .hint_text(tr("docker.page_range_example")),
        )
        .rich_tip(tr("docker.page_range_desc"));
    });
    ui.label(
        egui::RichText::new(tr("docker.page_range_desc"))
            .size(11.0)
            .color(Tokens::text_dim()),
    );
    let parsed = parsed_range(text);
    let (message, color) = match parsed {
        None => (tr("docker.page_range_invalid"), ui.visuals().error_fg_color),
        Some(range) => {
            let matching = range.count_within(page_count);
            (
                trf(
                    "docker.page_range_matches",
                    &[
                        ("n", &matching.to_string()),
                        ("total", &page_count.to_string()),
                    ],
                ),
                Tokens::text_dim(),
            )
        }
    };
    ui.label(egui::RichText::new(message).size(12.0).color(color));
}

pub fn new_range_layer_fields(ui: &mut Ui, state: &mut NewRangeLayer, page_count: usize) {
    ui.horizontal(|ui| {
        ui.label(tr("dialog.layer_name"));
        ui.text_edit_singleline(&mut state.name);
    });
    if state.name.trim().is_empty() {
        ui.colored_label(
            ui.visuals().error_fg_color,
            tr("docker.range_name_required"),
        );
    }
    page_range_field(ui, &mut state.page_range, page_count);
}

/// Which preference holds the in-front choice of master layers showing
/// on `scope`'s pages.
fn scope_slot(scope: MasterScope) -> usize {
    match scope {
        MasterScope::All => 0,
        MasterScope::Odd => 1,
        MasterScope::Even => 2,
    }
}

/// New master layers of `scope` are drawn in front of the page's layers.
pub fn master_front_default(app: &App, scope: MasterScope) -> bool {
    app.settings.master_in_front[scope_slot(scope)]
}

/// Master layers showing on `scope`'s pages in front of the page's own
/// layers (`front`) or behind them: the master layers of that kind the
/// drawing has, in one undo step, and the ones made from now on (the
/// choice is kept in the preferences, #1132, #1477).
pub fn set_master_front(app: &mut App, scope: MasterScope, front: bool) {
    app.settings.master_in_front[scope_slot(scope)] = front;
    let cmds: Vec<Command> = app
        .doc()
        .master
        .iter()
        .filter(|l| l.page_range.is_none() && l.scope == scope && l.in_front != front && !l.desktop)
        .map(|l| Command::SetLayerInFront {
            layer: l.id,
            in_front: front,
        })
        .collect();
    if !cmds.is_empty() {
        if let Err(e) = app.engine.run_batch("Master Layer Order", &cmds) {
            app.status = e.to_string();
        }
    }
}

/// The three in-front choices, one per kind of master layer, as
/// checkboxes (the New Master Layer menu and Options > Page).
pub fn master_front_choices(app: &mut App, ui: &mut Ui) {
    ui.label(egui::RichText::new(tr("docker.master_in_front")).strong());
    let _branch = crate::ui::context_menu_filter::Branch::new("docker.master_in_front");
    let scopes = [
        (MasterScope::All, "options.scope_all"),
        (MasterScope::Odd, "options.scope_odd"),
        (MasterScope::Even, "options.scope_even"),
    ];
    let keys = scopes.map(|(_, key)| key);
    for key in crate::ui::context_menu_filter::ordered_keys(&keys)
        .iter()
        .copied()
    {
        let Some((scope, _)) = scopes
            .iter()
            .copied()
            .find(|(_, candidate)| *candidate == key)
        else {
            continue;
        };
        let mut on = master_front_default(app, scope);
        if crate::ui::context_menus::show_item(ui, key)
            && ui
                .checkbox(&mut on, tr(key))
                .rich_tip_with(|| {
                    format!(
                        "{}\n{}",
                        tr("docker.master_in_front"),
                        tr(&format!("{key}_tip"))
                    )
                })
                .clicked()
        {
            set_master_front(app, scope, on);
        }
    }
}

/// What a layer's menu does with the clipboard (#1992).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum LayerClip {
    Cut,
    Copy,
    Paste,
}

/// The layers a click on `clicked` acts on: all the layers picked in the
/// docker when the clicked one is among them, else that one alone
/// (#3966). Layers another window removed are left out.
fn target_layers(app: &App, clicked: LayerId) -> Vec<LayerId> {
    if app.selected_layers.len() > 1 && app.selected_layers.contains(&clicked) {
        app.selected_layers
            .iter()
            .copied()
            .filter(|id| app.doc().layer(*id).is_ok())
            .collect()
    } else {
        vec![clicked]
    }
}

/// Select the objects of the aimed layers that can be (shown and not
/// locked), in document order; the layer selection is let go. A layer
/// with a page range first goes to a page it shows on; false when it
/// shows on none.
fn select_layer_objects(app: &mut App, layer: LayerId) -> bool {
    let aimed = target_layers(app, layer);
    if app
        .doc()
        .layer(layer)
        .is_ok_and(|layer| layer.page_range.is_some())
        && !activate_range_layer(app, layer)
    {
        return false;
    }
    let mut ids: Vec<ShapeId> = Vec::new();
    for l in app.doc().all_layers() {
        if aimed.contains(&l.id) {
            for s in &l.shapes {
                if s.visible && !s.locked {
                    ids.push(s.id);
                }
            }
        }
    }
    app.picked_layer = Some(layer);
    app.selected_layers.clear();
    app.select(ids);
    true
}

fn paste_layer(app: &mut App, layer: LayerId) -> bool {
    if app
        .doc()
        .layer(layer)
        .is_ok_and(|layer| layer.page_range.is_some())
    {
        if !activate_range_layer(app, layer) || app.active_layer() != Some(layer) {
            return false;
        }
    } else {
        app.picked_layer = Some(layer);
    }
    app.paste();
    true
}

/// The commands that move the top-level objects `ids` to the top of
/// `layer`, keeping their order (bottom first) and their place on the
/// page; objects already there stay.
pub(crate) fn move_commands(app: &App, ids: &[ShapeId], layer: LayerId) -> Vec<Command> {
    let doc = app.doc();
    let mut found: Vec<(usize, usize, Shape)> = Vec::new();
    for (rank, l) in doc.all_layers().enumerate() {
        if l.id == layer {
            continue;
        }
        for (i, s) in l.shapes.iter().enumerate() {
            if ids.contains(&s.id) {
                found.push((rank, i, s.clone()));
            }
        }
    }
    if found.is_empty() {
        return Vec::new();
    }
    found.sort_by_key(|(rank, i, _)| (*rank, *i));
    let mut cmds = vec![Command::DeleteShapes {
        shapes: found.iter().map(|(_, _, s)| s.id).collect(),
    }];
    cmds.extend(
        found
            .into_iter()
            .map(|(_, _, shape)| Command::AddShape { layer, shape }),
    );
    cmds
}

/// Move the objects `ids` to the top of `layer` (a layer of any page or
/// of the master page), one undo step; they stay selected, on the page
/// they now are on (#1477).
pub fn move_to_layer(app: &mut App, ids: &[ShapeId], layer: LayerId) {
    let mut requested_ids = ids.to_vec();
    let mut cmds = move_commands(app, &requested_ids, layer);
    if cmds.is_empty() {
        return;
    }
    if app
        .doc()
        .layer(layer)
        .is_ok_and(|layer| layer.page_range.is_some())
        && app.prepare_shapes_for_range_move(&requested_ids)
    {
        requested_ids.retain(|id| app.doc().find_shape(*id).is_some());
        cmds = move_commands(app, &requested_ids, layer);
        if cmds.is_empty() {
            return;
        }
    }
    if let Err(e) = app.engine.run_batch("Move to Layer", &cmds) {
        app.status = e.to_string();
        return;
    }
    if !activate_destination_layer(app, layer) {
        return;
    }
    let moved: Vec<ShapeId> = requested_ids
        .iter()
        .copied()
        .filter(|id| layer_holding(app, *id) == Some(layer))
        .collect();
    app.select(moved);
}

/// Make a destination editable after its objects have been transferred.
fn activate_destination_layer(app: &mut App, layer: LayerId) -> bool {
    if app
        .doc()
        .layer(layer)
        .is_ok_and(|layer| layer.page_range.is_some())
    {
        return activate_range_layer(app, layer);
    }
    let page = app
        .doc()
        .pages
        .iter()
        .position(|p| p.layers.iter().any(|l| l.id == layer));
    if let Some(i) = page {
        if app.doc().pages.get(i).map(|p| p.id) != Some(app.page) {
            app.goto_page(i);
        }
    }
    app.picked_layer = Some(layer);
    true
}

/// Objects panel > Edit Across Layers on or off, kept in the settings;
/// off, the selected objects of other layers than the active one are
/// let go (#3276).
pub fn set_edit_across_layers(app: &mut App, on: bool) {
    app.settings.edit_across_layers = on;
    app.settings.save();
    if !on {
        let active = app.active_layer();
        let keep: Vec<ShapeId> = app
            .selection
            .iter()
            .copied()
            .filter(|id| active.is_some() && layer_holding(app, *id) == active)
            .collect();
        if keep != app.selection {
            app.select(keep);
        }
    }
}

/// Copies of the top-level objects `ids` put on top of `layer` (a layer
/// of any page or of the master page) at the same place on the page,
/// keeping their order; the originals stay. One undo step; the copies
/// are selected, on the page they are on (#3277).
pub fn copy_to_layer(app: &mut App, ids: &[ShapeId], layer: LayerId) {
    let doc = app.doc();
    let mut found: Vec<(usize, usize, Shape)> = Vec::new();
    for (rank, l) in doc.all_layers().enumerate() {
        for (i, s) in l.shapes.iter().enumerate() {
            if ids.contains(&s.id) {
                found.push((rank, i, s.clone()));
            }
        }
    }
    if found.is_empty() {
        return;
    }
    found.sort_by_key(|(rank, i, _)| (*rank, *i));
    let originals: Vec<Shape> = found.into_iter().map(|(_, _, s)| s).collect();
    let mut copies: Vec<Shape> = originals
        .iter()
        .map(|s| {
            let id = app.engine.new_shape_id();
            crate::app::reid_pub(s.clone(), id, &mut app.engine)
        })
        .collect();
    crate::textflow::relink_copies(&originals, &mut copies);
    let new_ids: Vec<ShapeId> = copies.iter().map(|s| s.id).collect();
    let cmds: Vec<Command> = copies
        .into_iter()
        .map(|shape| Command::AddShape { layer, shape })
        .collect();
    if let Err(e) = app.engine.run_batch("Copy to Layer", &cmds) {
        app.status = e.to_string();
        return;
    }
    let page = app
        .doc()
        .pages
        .iter()
        .position(|p| p.layers.iter().any(|l| l.id == layer));
    if let Some(i) = page {
        if app.doc().pages.get(i).map(|p| p.id) != Some(app.page) {
            app.goto_page(i);
        }
    }
    app.picked_layer = Some(layer);
    app.select(new_ids);
    app.selected_layers = vec![layer];
}

/// The look of `from` given to the new layer `to`, its objects moved
/// over, and `from` deleted (kept, empty, when it is its page's last).
fn hand_over(app: &mut App, from: &Layer, to: LayerId, keep: bool) {
    let mut cmds = vec![
        Command::SetLayerVisible {
            layer: to,
            visible: from.visible,
        },
        Command::SetLayerLocked {
            layer: to,
            locked: from.locked,
        },
        Command::SetLayerPrintable {
            layer: to,
            printable: from.printable,
        },
    ];
    let ids: Vec<ShapeId> = from.shapes.iter().map(|s| s.id).collect();
    cmds.extend(move_commands(app, &ids, to));
    if !keep {
        cmds.push(Command::DeleteLayer { layer: from.id });
    }
    if let Err(e) = app.engine.amend_batch("Move Layer", &cmds) {
        app.status = e.to_string();
    }
    if app.picked_layer == Some(from.id) || !keep {
        app.picked_layer = Some(to);
    }
    if app.selected_layers.contains(&from.id) {
        app.selected_layers.retain(|id| *id != from.id);
        app.selected_layers.push(to);
    } else {
        app.selected_layers.retain(|id| *id != from.id);
    }
    app.selection.clear();
}

/// A page's layer becomes a master layer showing on every page (with its
/// objects, name and look), one undo step (#1477). With several layers
/// picked they all go, joining the one undo step.
fn layer_to_master(app: &mut App, layer: LayerId) {
    let aimed: Vec<LayerId> = target_layers(app, layer)
        .into_iter()
        .filter(|id| {
            app.doc()
                .pages
                .iter()
                .any(|p| p.layers.iter().any(|l| l.id == *id))
        })
        .collect();
    let mut started = false;
    for id in aimed {
        let found = {
            let doc = app.doc();
            doc.pages
                .iter()
                .find(|p| p.layers.iter().any(|l| l.id == id))
                .and_then(|p| {
                    p.layers
                        .iter()
                        .find(|l| l.id == id)
                        .cloned()
                        .map(|l| (p.layers.len() <= 1, l))
                })
        };
        let Some((keep, from)) = found else {
            continue;
        };
        let add = Command::AddMasterLayer {
            name: from.name.clone(),
            scope: MasterScope::All,
        };
        if !started {
            if let Err(e) = app.engine.run_with_label(&add, "Move Layer") {
                app.status = e.to_string();
                return;
            }
            started = true;
        } else if let Err(e) = app.engine.amend(&add) {
            app.status = e.to_string();
            return;
        }
        let Some(to) = app.doc().master.last().map(|l| l.id) else {
            return;
        };
        hand_over(app, &from, to, keep);
        if master_front_default(app, MasterScope::All) {
            let _ = app.engine.amend(&Command::SetLayerInFront {
                layer: to,
                in_front: true,
            });
        }
        app.objects_master_closed = false;
    }
}

/// A master layer becomes a layer on top of the current page (with its
/// objects, name and look), one undo step: what it held shows on that
/// page only (#1477). With several layers picked they all go, joining
/// the one undo step.
fn master_to_page(app: &mut App, layer: LayerId) {
    let aimed: Vec<LayerId> = target_layers(app, layer)
        .into_iter()
        .filter(|id| app.doc().master.iter().any(|l| l.id == *id))
        .collect();
    for id in aimed.iter().copied() {
        if app
            .doc()
            .layer(id)
            .is_ok_and(|layer| layer.page_range.is_some())
        {
            app.prepare_layer_range_change(id);
        }
    }
    let page = app.page;
    let mut started = false;
    for id in aimed {
        let from = {
            let doc = app.doc();
            doc.master.iter().find(|l| l.id == id).cloned()
        };
        let Some(from) = from else {
            continue;
        };
        let add = Command::AddLayer {
            page,
            name: from.name.clone(),
        };
        if !started {
            if let Err(e) = app.engine.run_with_label(&add, "Move Layer") {
                app.status = e.to_string();
                return;
            }
            started = true;
        } else if let Err(e) = app.engine.amend(&add) {
            app.status = e.to_string();
            return;
        }
        let Some(to) = app
            .doc()
            .page(page)
            .ok()
            .and_then(|p| p.layers.last())
            .map(|l| l.id)
        else {
            return;
        };
        hand_over(app, &from, to, false);
        app.objects_closed_pages.remove(&page);
    }
}

/// A layer, of a page or the master page, becomes a layer on top of
/// `page` (with its objects, name and look), one undo step, and that
/// page is shown; a page's last layer stays behind, empty (#1477).
/// With several layers picked they all go, joining the one undo step.
pub(crate) fn layer_to_page(app: &mut App, layer: LayerId, page: PageId) {
    if app.doc().page(page).is_err() {
        return;
    }
    // The clicked layer must be one that can go: a master layer, or a
    // layer of another page. The others picked go with it.
    let aimed: Vec<LayerId> = target_layers(app, layer)
        .into_iter()
        .filter(|id| {
            let doc = app.doc();
            if doc.master.iter().any(|l| l.id == *id) {
                return true;
            }
            doc.pages.iter().any(|p| {
                p.id != page && p.layers.iter().any(|l| l.id == *id)
            })
        })
        .collect();
    if !aimed.contains(&layer) {
        return;
    }
    for id in aimed.iter().copied() {
        if app
            .doc()
            .layer(id)
            .is_ok_and(|layer| layer.page_range.is_some())
        {
            app.prepare_layer_range_change(id);
        }
    }
    if let Some(i) = app.doc().pages.iter().position(|p| p.id == page) {
        if page != app.page {
            app.goto_page(i);
        }
    }
    let mut started = false;
    for id in aimed {
        let found = {
            let doc = app.doc();
            match doc.master.iter().find(|l| l.id == id) {
                Some(l) => Some((l.clone(), false)),
                None => doc
                    .pages
                    .iter()
                    .find(|p| p.layers.iter().any(|l| l.id == id))
                    .and_then(|p| {
                        if p.id == page {
                            return None;
                        }
                        p.layers
                            .iter()
                            .find(|l| l.id == id)
                            .cloned()
                            .map(|l| (l, p.layers.len() <= 1))
                    }),
            }
        };
        let Some((from, keep)) = found else {
            continue;
        };
        let add = Command::AddLayer {
            page,
            name: from.name.clone(),
        };
        if !started {
            if let Err(e) = app.engine.run_with_label(&add, "Move Layer") {
                app.status = e.to_string();
                return;
            }
            started = true;
        } else if let Err(e) = app.engine.amend(&add) {
            app.status = e.to_string();
            return;
        }
        let Some(to) = app
            .doc()
            .page(page)
            .ok()
            .and_then(|p| p.layers.last())
            .map(|l| l.id)
        else {
            return;
        };
        hand_over(app, &from, to, keep);
        app.objects_closed_pages.remove(&page);
        app.picked_layer = Some(to);
    }
}

/// The Layer Properties dialog's values (#1477): the name and switches of
/// the layer, and for a master layer the pages it shows on and whether it
/// is drawn in front of the pages' own layers.
#[derive(Debug, Clone, PartialEq)]
pub struct LayerProps {
    pub layer: LayerId,
    pub name: String,
    pub visible: bool,
    pub printable: bool,
    pub locked: bool,
    pub master: bool,
    pub desktop: bool,
    pub scope: MasterScope,
    pub use_page_range: bool,
    pub page_range: String,
    pub in_front: bool,
}

impl LayerProps {
    /// The values of `layer` as it is.
    pub fn of(app: &App, layer: LayerId) -> Option<LayerProps> {
        let doc = app.doc();
        let master = doc.master.iter().any(|l| l.id == layer);
        let l = doc.layer(layer).ok()?;
        Some(LayerProps {
            layer,
            name: l.name.clone(),
            visible: l.visible,
            printable: l.printable,
            locked: l.locked,
            master,
            desktop: l.desktop,
            scope: l.scope,
            use_page_range: l.page_range.is_some(),
            page_range: l
                .page_range
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| (app.page_index() + 1).to_string()),
            in_front: l.in_front,
        })
    }

    /// What changes the layer to these values, nothing when it has them.
    pub fn commands(&self, app: &App) -> Vec<Command> {
        if !self.valid() {
            return Vec::new();
        }
        let Some(now) = LayerProps::of(app, self.layer) else {
            return Vec::new();
        };
        let layer = self.layer;
        let mut cmds = Vec::new();
        let name = self.name.trim();
        if !name.is_empty() && name != now.name {
            cmds.push(Command::RenameLayer {
                layer,
                name: name.to_string(),
            });
        }
        if self.visible != now.visible {
            cmds.push(Command::SetLayerVisible {
                layer,
                visible: self.visible,
            });
        }
        if self.printable != now.printable {
            cmds.push(Command::SetLayerPrintable {
                layer,
                printable: self.printable,
            });
        }
        if self.locked != now.locked {
            cmds.push(Command::SetLayerLocked {
                layer,
                locked: self.locked,
            });
        }
        if now.master {
            if self.use_page_range {
                if let Some(page_range) = parsed_range(&self.page_range) {
                    let current = app
                        .doc()
                        .layer(layer)
                        .ok()
                        .and_then(|layer| layer.page_range.as_ref());
                    if current != Some(&page_range) {
                        cmds.push(Command::SetLayerPageRange {
                            layer,
                            page_range: Some(page_range),
                        });
                    }
                }
            } else if self.scope != now.scope || now.use_page_range {
                cmds.push(Command::SetLayerScope {
                    layer,
                    scope: self.scope,
                });
            }
        }
        if now.master && self.in_front != now.in_front {
            cmds.push(Command::SetLayerInFront {
                layer,
                in_front: self.in_front,
            });
        }
        cmds
    }

    pub fn valid(&self) -> bool {
        !self.use_page_range
            || (self.master && !self.desktop && parsed_range(&self.page_range).is_some())
    }
}

/// The Layer Properties dialog's fields (#1477).
pub fn layer_properties_fields(ui: &mut Ui, st: &mut LayerProps, page_count: usize) {
    egui::Grid::new("layer_properties")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label(tr("dialog.layer_name"));
            ui.text_edit_singleline(&mut st.name)
                .rich_tip(crate::ui::tips::keyed("dialog.layer_name"));
            ui.end_row();
        });
    ui.checkbox(&mut st.visible, tr("docker.show_hide"))
        .rich_tip(tr("docker.show_hide"));
    ui.checkbox(&mut st.printable, tr("docker.print_layer"))
        .rich_tip(tr("docker.print_layer"));
    ui.checkbox(&mut st.locked, tr("docker.lock_layer"))
        .rich_tip(tr("docker.lock_layer"));
    if st.master {
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            ui.label(tr("docker.show_on"));
            for scope in [MasterScope::All, MasterScope::Odd, MasterScope::Even] {
                let key = scope_key(scope);
                if ui
                    .radio(!st.use_page_range && st.scope == scope, tr(key))
                    .rich_tip_with(|| tr(&format!("{key}_tip")))
                    .clicked()
                {
                    st.use_page_range = false;
                    st.scope = scope;
                }
            }
            if !st.desktop
                && ui
                    .radio(st.use_page_range, tr("docker.range_pages"))
                    .rich_tip(crate::ui::tips::pair(
                        "docker.range_pages",
                        "docker.page_range_desc",
                    ))
                    .clicked()
            {
                st.use_page_range = true;
            }
        });
        if st.use_page_range {
            page_range_field(ui, &mut st.page_range, page_count);
        }
        ui.checkbox(&mut st.in_front, tr("docker.master_in_front"))
            .rich_tip(tr("docker.master_in_front"));
    }
}

/// The layers that may be deleted from `candidates`: master layers
/// always, and of any page at most all but one of its layers in
/// `candidates`, so every page keeps at least one layer (#3966).
pub(crate) fn deletable_layers_from(
    doc: &tracedraw_core::Document,
    candidates: &[LayerId],
) -> Vec<LayerId> {
    let mut out: Vec<LayerId> = Vec::new();
    for id in candidates {
        if doc.layer(*id).is_err() {
            continue;
        }
        if doc.master.iter().any(|l| l.id == *id) {
            if !out.contains(id) {
                out.push(*id);
            }
            continue;
        }
        let Some(page) = doc
            .pages
            .iter()
            .find(|p| p.layers.iter().any(|l| l.id == *id))
        else {
            continue;
        };
        let taken = out
            .iter()
            .filter(|taken| page.layers.iter().any(|l| l.id == **taken))
            .count();
        if taken < page.layers.len().saturating_sub(1) && !out.contains(id) {
            out.push(*id);
        }
    }
    out
}

/// The layers the docker may delete now: the ones picked, else the
/// active one.
fn deletable_layers(app: &App) -> Vec<LayerId> {
    let candidates: Vec<LayerId> = if app.selected_layers.is_empty() {
        app.active_layer().into_iter().collect()
    } else {
        app.selected_layers.clone()
    };
    deletable_layers_from(app.doc(), &candidates)
}

/// The active layer when Delete can take it: a master layer, or one of
/// several layers of the page (a page keeps its last layer).
fn deletable_layer(app: &App) -> Option<LayerId> {
    deletable_layers(app).first().copied()
}

/// The bar's Delete: the selected objects, else the active layer.
pub fn delete_current(app: &mut App) {
    if !app.selection.is_empty() {
        app.delete_selection();
    } else if let Some(layer) = deletable_layer(app) {
        delete_layer(app, layer);
    }
}

fn delete_layer(app: &mut App, layer: LayerId) {
    let aimed = deletable_layers_from(app.doc(), &target_layers(app, layer));
    if aimed.is_empty() {
        return;
    }
    app.selection.clear();
    // A frame of a linked chain on a layer leaves its text to the
    // frames left, as a deleted frame does (#1986).
    let mut ids: Vec<ShapeId> = Vec::new();
    for id in &aimed {
        if let Ok(l) = app.doc().layer(*id) {
            ids.extend(l.shapes.iter().map(|s| s.id));
        }
    }
    let flow = app.flow_around_deleted(&ids);
    let revision = app.engine.revision();
    if let [one] = aimed.as_slice() {
        app.run(Command::DeleteLayer { layer: *one });
    } else {
        let cmds: Vec<Command> = aimed
            .iter()
            .map(|id| Command::DeleteLayer { layer: *id })
            .collect();
        if let Err(e) = app.engine.run_batch("Delete Layer", &cmds) {
            app.status = e.to_string();
        }
    }
    if app.engine.revision() != revision {
        app.amend_flow("Delete Layer", flow);
    }
    let keep: Vec<LayerId> = app
        .selected_layers
        .iter()
        .copied()
        .filter(|id| app.doc().layer(*id).is_ok() && !aimed.contains(id))
        .collect();
    app.selected_layers = keep;
    if app
        .picked_layer
        .is_some_and(|id| aimed.contains(&id) || app.doc().layer(id).is_err())
    {
        app.picked_layer = app.selected_layers.last().copied();
    }
}

/// The layer holding `id` (inside groups too).
fn layer_holding(app: &App, id: ShapeId) -> Option<LayerId> {
    let doc = app.doc();
    let top = doc.top_level_of(id)?;
    doc.all_layers()
        .find(|l| l.shapes.iter().any(|s| s.id == top))
        .map(|l| l.id)
}

/// The object row being dragged, kept in the context's memory.
const ROW_DRAG: &str = "objects_row_drag";

/// The guideline row being dragged (#3599): its set (`None` the master
/// page's) and its index, kept in the context's memory.
const GUIDE_DRAG: &str = "objects_guide_drag";
/// The object row a finger is held on: (object, where, its page when not
/// the current one); its menu opens when the finger is lifted there.
const ROW_HOLD: &str = "objects_row_hold";
/// Where the pointer last was while a row was dragged: a finger lifted
/// leaves no pointer position on the frame it ends (#2948).
const ROW_DRAG_POS: &str = "objects_row_drag_pos";
/// A held finger lifted within this distance (points) of where it was
/// held opens the row's menu; farther, it was a drag.
const HOLD_SLOP: f32 = 8.0;

/// Where an object row let go goes.
#[derive(Debug, Clone, Copy, PartialEq)]
enum RowDrop {
    /// In front of `reference` (`in_front`) or behind it, in its layer or
    /// in its group.
    Object { reference: ShapeId, in_front: bool },
    /// Into the group, over its objects (#2672).
    Into(ShapeId),
    /// On the top of a layer.
    Layer(LayerId),
    /// Into the object as a PowerClip: the object becomes the frame, or
    /// the objects join a PowerClip's contents (#3053).
    Clip(ShapeId),
}

/// The objects a dragged row carries, bottom first as they are drawn: the
/// selected objects when the row's object is selected, on layers or in
/// groups (an object in a selected group goes with its group), else that
/// object; locked ones stay where they are.
fn dragged_objects(app: &App, dragged: ShapeId) -> Vec<ShapeId> {
    fn walk(s: &Shape, take: &dyn Fn(&Shape) -> bool, out: &mut Vec<ShapeId>) {
        if take(s) {
            if !s.locked {
                out.push(s.id);
            }
            return;
        }
        if let ShapeKind::Group { children } = &s.kind {
            for c in children {
                walk(c, take, out);
            }
        }
    }
    let with_selection = app.selection.contains(&dragged);
    let take = |s: &Shape| s.id == dragged || (with_selection && app.selection.contains(&s.id));
    let mut out = Vec::new();
    // Layer after layer as their objects are kept, each bottom first.
    for l in app.doc().all_layers() {
        for s in &l.shapes {
            walk(s, &take, &mut out);
        }
    }
    out
}

/// The objects lie on layers, not in groups.
fn all_on_layers(app: &App, ids: &[ShapeId]) -> bool {
    ids.iter().all(|id| app.doc().shape(*id).is_ok())
}

/// The commands that put `ids` (bottom first) where `drop` says: objects
/// of other layers (of any page or of the master page) go over to the
/// target's layer first, keeping their place on the page. Empty when
/// the target cannot take them (a locked layer, an object among those
/// dragged).
fn drop_commands(app: &App, ids: &[ShapeId], drop: RowDrop) -> Vec<Command> {
    let doc = app.doc();
    if ids.is_empty() {
        return Vec::new();
    }
    // Into a group, beside an object in one, or objects out of groups
    // (#2672): one move that keeps them where they show.
    let nested = match drop {
        RowDrop::Object { reference, .. } => doc.is_nested(reference) || !all_on_layers(app, ids),
        RowDrop::Into(_) => true,
        RowDrop::Layer(_) => !all_on_layers(app, ids),
        RowDrop::Clip(_) => return Vec::new(),
    };
    if nested {
        return nested_drop_commands(app, ids, drop);
    }
    let layer = match drop {
        RowDrop::Object { reference, .. } => {
            if ids.contains(&reference) {
                return Vec::new();
            }
            match doc.locate(reference) {
                Ok((l, _)) => l,
                Err(_) => return Vec::new(),
            }
        }
        RowDrop::Layer(l) => l,
        RowDrop::Into(_) | RowDrop::Clip(_) => return Vec::new(),
    };
    if doc.all_layers().any(|l| l.id == layer && l.locked) {
        return Vec::new();
    }
    let mut cmds = move_commands(app, ids, layer);
    if let RowDrop::Object {
        reference,
        in_front,
    } = drop
    {
        cmds.push(Command::OrderRelative {
            shapes: ids.to_vec(),
            reference,
            in_front,
        });
    }
    cmds
}

/// The command for a drop that involves groups (#2672): `ids` go next to
/// the reference in whatever holds it, into the group, or on the top of
/// the layer (next to its top object; an empty layer takes nothing so).
/// Empty when the target's layer is locked, the target is among the
/// objects dragged, or nothing would move.
fn nested_drop_commands(app: &App, ids: &[ShapeId], drop: RowDrop) -> Vec<Command> {
    let doc = app.doc();
    let target_layer = match drop {
        RowDrop::Object { reference: t, .. } | RowDrop::Into(t) => layer_holding(app, t),
        RowDrop::Layer(l) => Some(l),
        RowDrop::Clip(_) => return Vec::new(),
    };
    let Some(layer) = target_layer else {
        return Vec::new();
    };
    if doc.all_layers().any(|l| l.id == layer && l.locked) {
        return Vec::new();
    }
    let cmd = match drop {
        RowDrop::Object {
            reference,
            in_front,
        } => Command::PlaceBeside {
            shapes: ids.to_vec(),
            reference,
            in_front,
        },
        RowDrop::Into(group) => Command::PlaceInGroup {
            shapes: ids.to_vec(),
            group,
        },
        RowDrop::Clip(_) => return Vec::new(),
        RowDrop::Layer(l) => {
            let top = doc
                .all_layers()
                .find(|x| x.id == l)
                .and_then(|x| x.shapes.last())
                .map(|s| s.id);
            match top {
                Some(reference) => Command::PlaceBeside {
                    shapes: ids.to_vec(),
                    reference,
                    in_front: true,
                },
                None => return Vec::new(),
            }
        }
    };
    // The target, or a group holding it, among the objects dragged: it
    // cannot go inside itself.
    let target = match drop {
        RowDrop::Object { reference: t, .. } | RowDrop::Into(t) | RowDrop::Clip(t) => Some(t),
        RowDrop::Layer(_) => None,
    };
    if let Some(t) = target {
        let mut above = doc.ancestors(t);
        above.push(t);
        if ids.iter().all(|id| above.contains(id)) {
            return Vec::new();
        }
    }
    vec![cmd]
}

/// Put the objects `ids` (bottom first) where `drop` says, one undo
/// step; they stay selected, and their layer becomes the active one, on
/// its page.
fn drop_rows(app: &mut App, ids: &[ShapeId], drop: RowDrop) {
    if let RowDrop::Clip(frame) = drop {
        clip_rows(app, ids, frame);
        return;
    }
    let layer = match drop {
        RowDrop::Object { reference, .. } | RowDrop::Into(reference) | RowDrop::Clip(reference) => {
            layer_holding(app, reference)
        }
        RowDrop::Layer(l) => Some(l),
    };
    let frees = frees_clip_contents(app, ids, drop);
    let mut requested_ids = ids.to_vec();
    let mut cmds = drop_commands(app, &requested_ids, drop);
    if cmds.is_empty() && frees {
        // Already on that layer: only out of the PowerClip.
        free_clip_contents(app, ids);
        return;
    }
    let Some(layer) = layer.filter(|_| !cmds.is_empty()) else {
        return;
    };
    if app
        .doc()
        .layer(layer)
        .is_ok_and(|layer| layer.page_range.is_some())
        && app.prepare_shapes_for_range_move(&requested_ids)
    {
        requested_ids.retain(|id| app.doc().find_shape(*id).is_some());
        cmds = drop_commands(app, &requested_ids, drop);
        if cmds.is_empty() {
            return;
        }
    }
    if let [one] = cmds.as_slice() {
        app.run(one.clone());
    } else if let Err(e) = app.engine.run_batch("Move to Layer", &cmds) {
        app.status = e.to_string();
        return;
    }
    if !activate_destination_layer(app, layer) {
        return;
    }
    app.select(requested_ids.clone());
    app.selected_layers = vec![layer];
    if frees && app.clip_frame_edit.is_some() {
        free_clip_contents(app, &requested_ids);
    }
}

/// `ids`, contents of the PowerClip being edited, stay out of it when the
/// edit ends (#3846); with nothing left inside, the edit ends now.
fn free_clip_contents(app: &mut App, ids: &[ShapeId]) {
    if let Some((_, outside)) = app.clip_frame_edit.as_mut() {
        outside.extend(ids.iter().copied());
    }
    if app.clip_edit_contents().is_empty() {
        app.finish_clip_frame_edit();
    }
    app.select(ids.to_vec());
}

/// Whether dropping the rows of `ids` where `drop` says takes them out
/// of the PowerClip whose contents are being edited (#3846): they are all
/// contents, and they go onto a layer, or beside the frame or an object
/// outside it. They then stay out when the edit ends.
fn frees_clip_contents(app: &App, ids: &[ShapeId], drop: RowDrop) -> bool {
    let Some((frame, outside)) = &app.clip_frame_edit else {
        return false;
    };
    let contents = app.clip_edit_contents();
    if ids.is_empty() || !ids.iter().all(|id| contents.contains(id)) {
        return false;
    }
    let out = |id: ShapeId| id == *frame || outside.contains(&id);
    match drop {
        RowDrop::Layer(_) => true,
        RowDrop::Object { reference, .. } => {
            out(reference) || app.doc().ancestors(reference).into_iter().any(out)
        }
        RowDrop::Into(_) | RowDrop::Clip(_) => false,
    }
}

/// Whether rows carrying `ids` can be dropped into `target` as a
/// PowerClip (#3053): the target lies on a layer (not in a group), is no
/// group, is not locked nor on a locked layer, and is not among them; the
/// objects carried lie on layers and are not locked.
fn can_clip_into(app: &App, ids: &[ShapeId], target: ShapeId) -> bool {
    let doc = app.doc();
    // A clip frame inside another one takes objects too (#4227); other
    // objects must lie on a layer to become a frame.
    let t = match doc.shape(target) {
        Ok((_, t)) => t,
        Err(_) => match doc.find_shape(target) {
            Some(t) if matches!(t.kind, ShapeKind::ClipFrame { .. }) => t,
            _ => return false,
        },
    };
    if t.locked || matches!(t.kind, ShapeKind::Group { .. }) || ids.contains(&target) {
        return false;
    }
    if doc.top_level_of(target).is_some_and(|top| ids.contains(&top)) {
        return false;
    }
    if layer_holding(app, target)
        .is_some_and(|l| doc.all_layers().any(|x| x.id == l && x.locked))
    {
        return false;
    }
    !ids.is_empty() && ids.iter().all(|id| doc.shape(*id).is_ok_and(|(_, s)| !s.locked))
}

/// Rows let go in the middle of another object's row (#3053): the objects
/// go inside it as a PowerClip, as Place Inside Frame puts them (the
/// object becomes the frame, or they join a PowerClip's contents), one
/// undo step; the PowerClip is selected and opened in the list.
fn clip_rows(app: &mut App, ids: &[ShapeId], frame: ShapeId) {
    if !can_clip_into(app, ids, frame) {
        return;
    }
    app.place_inside(ids.to_vec(), frame);
    if app.doc().find_shape(frame).is_some_and(|s| matches!(s.kind, ShapeKind::ClipFrame { .. })) {
        app.objects_open_groups.insert(frame);
        app.select(vec![frame]);
    }
}

/// A click on the row of `id`, an object inside a PowerClip (#3053):
/// the PowerClip's contents are taken out for editing (Edit Contents), so
/// the object can be moved, edited and put in another order among the
/// contents, and the object is selected. False when `id` is in no
/// PowerClip.
fn select_clip_content(app: &mut App, id: ShapeId) -> bool {
    // Into each clip frame on the way to the object, outermost first: a
    // content of a clip frame inside another is reached through both
    // (#4110: only the innermost one was looked at, and editing it while
    // the outer one was edited dropped the outer one's editing).
    let mut entered = false;
    for _ in 0..32 {
        let doc = app.doc();
        let ancestors = doc.ancestors(id);
        let Some(&top) = ancestors.first() else {
            break;
        };
        let next = ancestors.get(1).copied().unwrap_or(id);
        let in_contents = doc.find_shape(top).is_some_and(|f| match &f.kind {
            ShapeKind::ClipFrame { contents, .. } => contents.iter().any(|c| c.id == next),
            _ => false,
        });
        let next_is_clip = doc
            .find_shape(next)
            .is_some_and(|s| matches!(s.kind, ShapeKind::ClipFrame { .. }));
        if !in_contents || doc.shape(top).is_err() || (next != id && !next_is_clip) {
            break;
        }
        app.select(vec![top]);
        app.edit_clip_frame();
        if app.doc().shape(next).is_err() {
            break;
        }
        entered = true;
    }
    if !entered {
        return false;
    }
    if app.doc().shape(id).is_ok() {
        app.select(vec![id]);
    }
    true
}

/// An object row dragged along the list: a line shows where it goes. Let
/// go over another object's row, the object goes in front of that object
/// when dropped on the row's upper half, behind it on the lower half (the
/// list shows the front object first), in the layer and page of that
/// object; let go over a layer's row, it goes on the top of that layer. A
/// selected row carries the rest of the selection along. One undo step.
/// Follow a row dragged along the list and drop it where it is let go;
/// gives the menu to open when a finger held on a row is lifted without
/// moving (object, where, its page when not the current one).
fn drag_rows(
    app: &mut App,
    ui: &Ui,
    rows: &[(ShapeId, Rect)],
    layers: &[(LayerId, Rect)],
) -> Option<(ShapeId, Pos2, Option<PageId>)> {
    #[cfg(test)]
    ui.ctx().data_mut(|d| {
        d.insert_temp(egui::Id::new("objects_rows_drawn"), rows.to_vec());
        d.insert_temp(egui::Id::new("objects_layer_rows_drawn"), layers.to_vec());
    });
    let id = egui::Id::new(ROW_DRAG);
    let hold_id = egui::Id::new(ROW_HOLD);
    let pos_id = egui::Id::new(ROW_DRAG_POS);
    let (latest, down, released) = ui.input(|i| {
        (
            i.pointer.latest_pos(),
            i.pointer.primary_down(),
            i.pointer.primary_released(),
        )
    });
    let held = ui
        .ctx()
        .data(|d| d.get_temp::<(ShapeId, Pos2, Option<PageId>)>(hold_id));
    let Some(dragged) = ui.ctx().data(|d| d.get_temp::<ShapeId>(id)) else {
        if !down {
            ui.ctx()
                .data_mut(|d| d.remove::<(ShapeId, Pos2, Option<PageId>)>(hold_id));
            return held.filter(|_| released);
        }
        return None;
    };
    if let Some(p) = latest {
        ui.ctx().data_mut(|d| d.insert_temp(pos_id, p));
    }
    // A finger lifted leaves no position on its last frame: where it was
    // last stands for it (#2948).
    let pos = latest.or_else(|| ui.ctx().data(|d| d.get_temp::<Pos2>(pos_id)));
    // Held and lifted where it was: the row's menu, not a drop.
    let hold_menu = held.filter(|(_, at, _)| pos.is_none_or(|p| p.distance(*at) <= HOLD_SLOP));
    let target = pos.and_then(|p| {
        if let Some((sid, r)) = rows.iter().find(|(_, r)| r.contains(p)) {
            if *sid == dragged {
                return None;
            }
            // A group's row: its middle half takes the objects into the
            // group, its top and bottom quarters put them in front of it
            // or behind it (#2672). Another row: its upper half in front,
            // its lower half behind.
            let group = app
                .doc()
                .find_shape(*sid)
                .is_some_and(|s| matches!(s.kind, ShapeKind::Group { .. }));
            if group && (p.y - r.center().y).abs() < r.height() / 4.0 {
                return Some((RowDrop::Into(*sid), r.center().y, *r));
            }
            // Another object's row: its middle third puts the objects
            // inside it as a PowerClip (#3053).
            if !group
                && (p.y - r.center().y).abs() < r.height() / 6.0
                && can_clip_into(app, &dragged_objects(app, dragged), *sid)
            {
                return Some((RowDrop::Clip(*sid), r.center().y, *r));
            }
            let in_front = p.y < r.center().y;
            let y = if in_front { r.top() } else { r.bottom() };
            let drop = RowDrop::Object {
                reference: *sid,
                in_front,
            };
            return Some((drop, y, *r));
        }
        layers
            .iter()
            .find(|(_, r)| r.contains(p))
            .map(|(l, r)| (RowDrop::Layer(*l), r.bottom(), *r))
    });
    if down {
        match target {
            // Into a group or a PowerClip: its row is framed.
            Some((RowDrop::Into(_) | RowDrop::Clip(_), _, r)) => {
                ui.painter().rect_stroke(
                    r.shrink(1.0),
                    3.0,
                    Stroke::new(2.0, Tokens::ACCENT),
                    egui::StrokeKind::Inside,
                );
            }
            Some((_, y, r)) => {
                ui.painter().line_segment(
                    [Pos2::new(r.left(), y), Pos2::new(r.right(), y)],
                    Stroke::new(2.0, Tokens::ACCENT),
                );
            }
            None => {}
        }
        return None;
    }
    // The button is up: the drag ends, and moves the objects only when it
    // was let go over a row this frame (not when the list was away).
    ui.ctx().data_mut(|d| {
        d.remove::<ShapeId>(id);
        d.remove::<Pos2>(pos_id);
        d.remove::<(ShapeId, Pos2, Option<PageId>)>(hold_id);
    });
    if !released {
        return None;
    }
    if hold_menu.is_some() {
        return hold_menu;
    }
    if let Some((drop, _, _)) = target {
        // A content's row of a PowerClip not being edited, dragged out
        // straight away: the contents are taken out for editing with it
        // chosen, as a click on its row does, and then it is dropped, so
        // one drag frees it with no click first (#4061).
        if app.clip_frame_edit.is_none() && !app.doc().ancestors(dragged).is_empty() {
            select_clip_content(app, dragged);
        }
        let ids = dragged_objects(app, dragged);
        drop_rows(app, &ids, drop);
    }
    None
}

/// Turn a layer switch (`on`) on every layer the click aims at that does
/// not have it yet, one undo step; one layer goes through `app.run` as
/// before (#3966).
fn apply_layer_switch(
    app: &mut App,
    clicked: LayerId,
    on: bool,
    label: &'static str,
    make: impl Fn(LayerId, bool) -> Command,
    current: impl Fn(&Layer) -> bool,
) {
    let cmds: Vec<Command> = target_layers(app, clicked)
        .into_iter()
        .filter(|id| app.doc().layer(*id).is_ok_and(|l| current(l) != on))
        .map(|id| make(id, on))
        .collect();
    if let [one] = cmds.as_slice() {
        app.run(one.clone());
    } else if !cmds.is_empty() {
        if let Err(e) = app.engine.run_batch(label, &cmds) {
            app.status = e.to_string();
        }
    }
}

fn apply(app: &mut App, ui: &Ui, mut act: Actions) {
    if let Some((id, pos, page)) = drag_rows(app, ui, &act.object_rows, &act.layer_drop_rows) {
        act.goto = act.goto.or(page);
        act.object_menu = Some((id, pos));
    }
    if let Some((layer, visible)) = act.visible {
        apply_layer_switch(
            app,
            layer,
            visible,
            "Layer Visible",
            |id, on| Command::SetLayerVisible { layer: id, visible: on },
            |l| l.visible,
        );
    }
    if let Some((layer, printable)) = act.printable {
        apply_layer_switch(
            app,
            layer,
            printable,
            "Layer Print",
            |id, on| Command::SetLayerPrintable { layer: id, printable: on },
            |l| l.printable,
        );
    }
    if let Some((layer, locked)) = act.locked {
        apply_layer_switch(
            app,
            layer,
            locked,
            "Layer Lock",
            |id, on| Command::SetLayerLocked { layer: id, locked: on },
            |l| l.locked,
        );
    }
    if let Some(on) = act.across {
        set_edit_across_layers(app, on);
    }
    if let Some((id, visible)) = act.object_visible {
        // The eye of one of several selected objects hides or shows them
        // all, as the clicked one goes, in one undo step; an object out
        // of the selection goes alone (#2571).
        let shapes = if app.selection.len() > 1 && app.selection.contains(&id) {
            app.selection.clone()
        } else {
            vec![id]
        };
        app.run(Command::SetVisible { shapes, visible });
    }
    if let Some((id, locked)) = act.object_locked {
        app.run(Command::SetLocked {
            shapes: vec![id],
            locked,
        });
    }
    if let Some((layer, scope)) = act.scope {
        app.run(Command::SetLayerScope { layer, scope });
    }
    if let Some((layer, in_front)) = act.in_front {
        app.run(Command::SetLayerInFront { layer, in_front });
    }
    if let Some(takes) = act.desktop_takes {
        app.settings.desktop_takes_objects_beside_page = takes;
    }
    if let Some(layer) = act.collapse {
        if !app.objects_collapsed.remove(&layer) {
            app.objects_collapsed.insert(layer);
        }
    }
    if let Some(group) = act.expand_group {
        if !app.objects_open_groups.remove(&group) {
            app.objects_open_groups.insert(group);
        }
    }
    if let Some(page) = act.toggle_page {
        if !app.objects_closed_pages.remove(&page) {
            app.objects_closed_pages.insert(page);
        }
    }
    // A row of another page: its page first (the selection there is new,
    // except a Ctrl/Shift layer click across pages keeps the picked layers).
    if let Some(page) = act.goto {
        let keep_layers = act.pick.is_some()
            && ui.input(|i| i.modifiers.shift || i.modifiers.command || i.modifiers.ctrl);
        let kept = keep_layers.then(|| app.selected_layers.clone());
        if let Some(i) = app.doc().pages.iter().position(|p| p.id == page) {
            app.goto_page(i);
        }
        if let Some(layers) = kept {
            app.selected_layers = layers;
        }
    }
    if act.toggle_master {
        app.objects_master_closed = !app.objects_master_closed;
    }
    if act.toggle_ranges {
        app.objects_ranges_closed = !app.objects_ranges_closed;
    }
    if let Some((page, shown)) = act.toggle_guides {
        app.set_guides_shown(page, shown);
    }
    if act.toggle_grid {
        app.show_grid = !app.show_grid;
    }
    if let Some(master) = act.guides_master {
        app.choose_master_guides(master);
    }
    if let Some((master, i, what)) = act.guide_row {
        app.choose_master_guides(master);
        match what {
            GuideRowAct::Hide(hidden) => app.set_guide_hidden(i, hidden),
            GuideRowAct::Lock(locked) => app.set_guides_locked(&[i], locked),
            GuideRowAct::Select { ctrl, shift } => select_guide_row(app, i, ctrl, shift),
            GuideRowAct::Menu(cmd) => guide_menu_command(app, i, cmd),
        }
    }
    if let Some((from, index, to)) = act.move_guides {
        // The selected guidelines go with the one dragged when it is one
        // of them.
        let edited = from.is_none() == app.edit_master_guides && from.is_none_or(|p| p == app.page);
        let indices = if edited && app.selected_guides.contains(&index) {
            app.selected_guides.clone()
        } else {
            vec![index]
        };
        app.move_guides_between(from, &indices, to);
    }
    // The drag of a guideline row ends when the button is let go.
    if ui.input(|i| i.pointer.any_released() || !i.pointer.any_down()) {
        ui.ctx()
            .data_mut(|d| d.remove_temp::<(Option<PageId>, usize)>(egui::Id::new(GUIDE_DRAG)));
    }
    if let Some(page) = act.paste_guides {
        if let Some(i) = page.and_then(|p| app.doc().pages.iter().position(|pg| pg.id == p)) {
            app.goto_page(i);
        }
        app.choose_master_guides(page.is_none());
        app.paste();
    }
    if let Some((master, cmd)) = act.guides_cmd {
        app.choose_master_guides(master);
        guides_menu_command(app, cmd);
    }
    if let Some((layer, page)) = act.layer_to_page {
        layer_to_page(app, layer, page);
    }
    if let Some(layer) = act.layer_props {
        if let Some(st) = LayerProps::of(app, layer) {
            app.dialog = crate::ui::dialogs::Dialog::LayerProperties(st);
        }
    }
    if let Some(layer) = act.edit_range {
        open_layer_range(app, layer);
    }
    if let Some((master, locked)) = act.lock_guides {
        app.choose_master_guides(master);
        let all: Vec<usize> = (0..app.page_guides().len()).collect();
        app.set_guides_locked(&all, locked);
    }
    if let Some((layer, name)) = act.rename_layer {
        app.dialog = crate::ui::dialogs::Dialog::RenameLayer { layer, name };
    }
    if let Some(name) = act.rename_page {
        app.dialog = crate::ui::dialogs::Dialog::RenamePage { name };
    }
    if let Some((shape, name)) = act.rename_object {
        app.dialog = crate::ui::dialogs::Dialog::RenameObject { shape, name };
    }
    if let Some(layer) = act.delete_layer {
        delete_layer(app, layer);
    }
    if let Some((layer, index)) = act.reorder {
        app.run(Command::ReorderLayer { layer, index });
    }
    if let Some(layer) = act.pick {
        // A layer with a page range goes to a page it shows on first.
        let ranged = app
            .doc()
            .layer(layer)
            .is_ok_and(|layer| layer.page_range.is_some());
        if !ranged || activate_range_layer(app, layer) {
            let (shift, ctrl) = ui.input(|i| {
                (
                    i.modifiers.shift,
                    i.modifiers.command || i.modifiers.ctrl,
                )
            });
            let key = egui::Id::new("objects_docker_layer_anchor");
            let anchor: Option<LayerId> = ui.data(|d| d.get_temp(key));
            let anchor = select_layer_row(app, layer, &act.layer_order, anchor, shift, ctrl);
            ui.data_mut(|d| d.insert_temp(key, anchor));
            // A pick made in the list itself is in view already: the list
            // stays where it is scrolled, not jumping to the first selected
            // object of a selection just let go (#3822).
            ui.data_mut(|d| d.insert_temp(egui::Id::new(SEEN_SELECTION), app.selection.clone()));
        }
    }
    if act.new_layer {
        new_layer(app);
    }
    if let Some(scope) = act.new_master {
        new_master_layer(app, scope);
    }
    if act.new_range {
        open_new_range_layer(app);
    }
    if let Some(layer) = act.select_layer {
        select_layer_objects(app, layer);
    }
    if let Some((layer, what)) = act.layer_clip {
        match what {
            LayerClip::Cut => {
                if select_layer_objects(app, layer) {
                    app.cut();
                }
            }
            LayerClip::Copy => {
                if select_layer_objects(app, layer) {
                    app.copy();
                }
            }
            LayerClip::Paste => {
                paste_layer(app, layer);
            }
        }
    }
    if let Some((id, pos)) = act.object_menu {
        open_row_menu(app, id, pos);
    }
    if let Some(layer) = act.layer_to_master {
        layer_to_master(app, layer);
    }
    if let Some(layer) = act.master_to_page {
        master_to_page(app, layer);
    }
    let mut select = act.select;
    if let Some(id) = select {
        if let Some(layer) = layer_holding(app, id) {
            let outside_range = app.doc().layer(layer).is_ok_and(|layer| {
                layer.page_range.is_some() && !layer.applies_to(app.page_index())
            });
            if outside_range && !activate_range_layer(app, layer) {
                select = None;
            }
        }
    }
    if let Some(id) = select.filter(|id| !select_clip_content(app, *id)) {
        app.selected_layers.clear();
        let (shift, ctrl) = ui.input(|i| (i.modifiers.shift, i.modifiers.command));
        let key = egui::Id::new("objects_docker_anchor");
        let anchor: Option<ShapeId> = ui.data(|d| d.get_temp(key));
        let anchor = select_row(app, id, &act.row_order, anchor, shift, ctrl);
        ui.data_mut(|d| d.insert_temp(key, anchor));
        // A selection made in the list itself is in view already: the list
        // stays where it is scrolled, not jumping back to the first
        // selected row on a Ctrl+click (#3822).
        ui.data_mut(|d| d.insert_temp(egui::Id::new(SEEN_SELECTION), app.selection.clone()));
        if let Some(layer) = layer_holding(app, id) {
            app.picked_layer = Some(layer);
        }
    }
}

/// A click on the row of `layer` in the list whose layer rows are
/// `order` (top to bottom as they show), as in a file list (#3966):
/// alone it picks that layer; Shift selects every layer row from the
/// row last clicked (`anchor`) to this one, Ctrl+Shift adds that range;
/// Ctrl adds the layer or takes it out. Objects and guidelines are let
/// go. Gives the row a next Shift+click counts from.
pub(crate) fn select_layer_row(
    app: &mut App,
    layer: LayerId,
    order: &[LayerId],
    anchor: Option<LayerId>,
    shift: bool,
    ctrl: bool,
) -> LayerId {
    app.select(Vec::new());
    app.selected_guides.clear();
    let mut base: Vec<LayerId> = app
        .selected_layers
        .iter()
        .copied()
        .filter(|id| app.doc().layer(*id).is_ok())
        .collect();
    if base.is_empty() && (ctrl || shift) {
        // Ctrl or Shift right after a drawing was opened: the layer that
        // was active already belongs to the selection.
        base = app.active_layer().into_iter().collect();
    }
    if !shift && !ctrl {
        app.selected_layers = vec![layer];
        app.picked_layer = Some(layer);
        return layer;
    }
    if ctrl && !shift {
        if let Some(i) = base.iter().position(|id| *id == layer) {
            base.remove(i);
            app.selected_layers = base;
            if app.picked_layer == Some(layer) {
                app.picked_layer = app.selected_layers.last().copied();
            }
            return anchor.unwrap_or(layer);
        }
        base.push(layer);
        app.selected_layers = base;
        app.picked_layer = Some(layer);
        return layer;
    }
    let from = anchor
        .filter(|a| order.contains(a))
        .or_else(|| base.last().copied().filter(|a| order.contains(a)))
        .unwrap_or(layer);
    let (ia, ib) = match (
        order.iter().position(|id| *id == from),
        order.iter().position(|id| *id == layer),
    ) {
        (Some(a), Some(b)) => (a, b),
        _ => {
            app.selected_layers = vec![layer];
            app.picked_layer = Some(layer);
            return layer;
        }
    };
    let range = &order[ia.min(ib)..=ia.max(ib)];
    if ctrl {
        for id in range {
            if !base.contains(id) {
                base.push(*id);
            }
        }
        app.selected_layers = base;
    } else {
        app.selected_layers = range.to_vec();
    }
    app.picked_layer = Some(layer);
    from
}

/// A click on the row of `id` in the list whose rows are `rows` (top to
/// bottom), as in a file list (#2483): alone it selects that object;
/// Shift selects every row from the row last clicked (`anchor`) to this
/// one, Ctrl+Shift adds that range; Ctrl adds the object or takes it out.
/// A locked object stays out of the selection (#2347). Gives the row a
/// next Shift+click counts from.
fn select_row(
    app: &mut App,
    id: ShapeId,
    rows: &[ShapeId],
    anchor: Option<ShapeId>,
    shift: bool,
    ctrl: bool,
) -> ShapeId {
    fn exclude(shape: &Shape, unavailable: &mut std::collections::HashSet<ShapeId>) {
        unavailable.insert(shape.id);
        match &shape.kind {
            ShapeKind::Group { children } => {
                for child in children {
                    exclude(child, unavailable);
                }
            }
            ShapeKind::ClipFrame { frame, contents } => {
                exclude(frame, unavailable);
                for child in contents {
                    exclude(child, unavailable);
                }
            }
            _ => {}
        }
    }

    let mut unavailable = app.locked_ids();
    let index = app.page_index();
    for layer in &app.doc().master {
        if layer.page_range.is_some() && !layer.applies_to(index) {
            for shape in &layer.shapes {
                exclude(shape, &mut unavailable);
            }
        }
    }
    let free = |i: &ShapeId| !unavailable.contains(i);
    if shift {
        let from = anchor.and_then(|a| rows.iter().position(|r| *r == a));
        let to = rows.iter().position(|r| *r == id);
        let range: Vec<ShapeId> = match (from, to) {
            (Some(a), Some(b)) => rows[a.min(b)..=a.max(b)].to_vec(),
            _ => vec![id],
        };
        let mut picked = if ctrl {
            app.selection.clone()
        } else {
            Vec::new()
        };
        for r in range.into_iter().filter(free) {
            if !picked.contains(&r) {
                picked.push(r);
            }
        }
        picked.retain(free);
        app.select(picked);
        // The range keeps counting from where it started.
        return anchor.filter(|a| rows.contains(a)).unwrap_or(id);
    }
    if ctrl {
        let mut picked = app.selection.clone();
        if let Some(i) = picked.iter().position(|r| *r == id) {
            picked.remove(i);
        } else if free(&id) {
            picked.push(id);
        }
        picked.retain(free);
        app.select(picked);
    } else {
        let mut picked = vec![id];
        picked.retain(free);
        app.select(picked);
    }
    id
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// #2483: in the Objects docker Shift+click selects every row from the
    /// row clicked before to this one, as in a file list, and Ctrl+click
    /// adds or takes out one object. Shift+click added the one object
    /// only, leaving the rows between unselected.
    #[test]
    fn shift_click_selects_a_range_of_rows_and_ctrl_click_one_more() {
        let mut app = App::headless();
        let ids: Vec<ShapeId> = (0..5)
            .map(|_| app.new_shape(rect_kind()).expect("shape"))
            .collect();
        // The list shows the top object first.
        let rows: Vec<ShapeId> = ids.iter().rev().copied().collect();
        let a = select_row(&mut app, rows[1], &rows, None, false, false);
        assert_eq!(app.selection, vec![rows[1]]);
        // Shift on the fourth row: the second to the fourth.
        let a = select_row(&mut app, rows[3], &rows, Some(a), true, false);
        assert_eq!(app.selection, rows[1..=3].to_vec());
        assert_eq!(a, rows[1], "the range counts from its first row");
        // Shift upwards from the same start: the first two.
        select_row(&mut app, rows[0], &rows, Some(a), true, false);
        assert_eq!(app.selection, rows[0..=1].to_vec());
        // Ctrl adds the last row alone, and takes it out again.
        let a = select_row(&mut app, rows[4], &rows, Some(a), false, true);
        assert_eq!(app.selection, vec![rows[0], rows[1], rows[4]]);
        select_row(&mut app, rows[4], &rows, Some(a), false, true);
        assert_eq!(app.selection, vec![rows[0], rows[1]]);
        // A locked object in the range stays out of it (#2347).
        app.run(Command::SetLocked {
            shapes: vec![rows[2]],
            locked: true,
        });
        let a = select_row(&mut app, rows[1], &rows, None, false, false);
        select_row(&mut app, rows[3], &rows, Some(a), true, false);
        assert_eq!(app.selection, vec![rows[1], rows[3]]);
        // No row clicked before: Shift selects the one row.
        select_row(&mut app, rows[4], &rows, None, true, false);
        assert_eq!(app.selection, vec![rows[4]]);
    }

    /// A new drawing without its desktop layer, for the tests of the
    /// numbered master layers.
    pub(crate) fn without_desktop() -> App {
        let mut app = App::headless();
        let desktop: Vec<LayerId> = app
            .doc()
            .master
            .iter()
            .filter(|l| l.desktop)
            .map(|l| l.id)
            .collect();
        for layer in desktop {
            app.run(Command::DeleteLayer { layer });
        }
        app
    }

    fn rect_kind() -> ShapeKind {
        ShapeKind::Rect {
            rect: tracedraw_core::Rect::new(0.0, 0.0, 10.0, 10.0),
            radius: 0.0,
            corners: None,
        }
    }

    fn with_ui(mut f: impl FnMut(&mut Ui)) {
        let ctx = crate::theme::ui_context();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400.0, 600.0),
            )),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| f(ui));
        out.textures_delta.clear();
    }

    fn draw(app: &mut App) {
        with_ui(|ui| objects_docker(app, ui));
    }

    /// The search field finds objects by name too (#4258): a layer is
    /// listed when its name or one of its objects' names holds the text
    /// (any case), a group when it or an object inside it does.
    #[test]
    fn the_search_finds_objects_by_name() {
        crate::i18n::set_language("en");
        let mut water = Shape::new(ShapeId(1), rect_kind());
        water.name = Some("Water Tank".into());
        let mut roof = Shape::new(ShapeId(2), rect_kind());
        roof.name = Some("Roof".into());
        let group = Shape::new(
            ShapeId(3),
            ShapeKind::Group {
                children: vec![water.clone(), roof.clone()],
            },
        );
        assert!(own_hit(&water, "tank") && !own_hit(&roof, "tank"));
        assert!(shape_hit(&group, "tank"), "found inside the group");
        assert!(!own_hit(&group, "tank"), "the group itself does not match");
        assert!(shape_hit(&Shape::new(ShapeId(4), rect_kind()), "rectangle"), "an unnamed kind matches too");
        let mut app = App::headless();
        let id = app.new_shape(rect_kind()).expect("rect");
        let mut layer = app
            .page_layers()
            .into_iter()
            .find(|l| l.shapes.iter().any(|s| s.id == id))
            .cloned()
            .expect("layer");
        for s in &mut layer.shapes {
            if s.id == id {
                s.name = Some("Logo Front".into());
            }
        }
        assert!(layer_hit(&layer, "logo"));
        assert!(!layer_hit(&layer, "nothing like it"));
        assert!(layer_hit(&layer, ""), "no search lists everything");
        draw(&mut app);
    }

    #[test]
    fn objects_show_their_names_or_what_they_are() {
        crate::i18n::set_language("en");
        let mut s = Shape::new(ShapeId(1), rect_kind());
        assert_eq!(object_label(&s), "Rectangle");
        s.name = Some("Water".into());
        assert_eq!(object_label(&s), "Water");
        s.name = Some("   ".into());
        assert_eq!(object_label(&s), "Rectangle");
    }

    /// A bitmap's row names its colour mode, as the status bar does
    /// (#3557); a name still wins.
    #[test]
    fn a_bitmap_row_shows_its_colour_mode() {
        crate::i18n::set_language("en");
        let png = {
            let mut pm = tiny_skia::Pixmap::new(1, 1).expect("pixmap");
            pm.fill(tiny_skia::Color::WHITE);
            pm.encode_png().expect("png")
        };
        let mut s = Shape::new(
            ShapeId(1),
            ShapeKind::Bitmap {
                rect: tracedraw_core::geometry::Rect::new(0.0, 0.0, 10.0, 10.0),
                width_px: 1,
                height_px: 1,
                png: png.into(),
                fx: None,
            },
        );
        assert_eq!(object_label(&s), "Bitmap 1 x 1 px (RGB)");
        s.cmyk_bitmap = true;
        assert_eq!(object_label(&s), "Bitmap 1 x 1 px (CMYK)");
        s.name = Some("Photo".into());
        assert_eq!(object_label(&s), "Photo");
    }

    /// Each object's row has its own eye and padlock: a click hides or
    /// shows it, locks or unlocks it, one undo step each (#832).
    #[test]
    fn an_objects_eye_and_padlock_hide_and_lock_it() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        let id = app.new_shape(rect_kind()).expect("shape");
        let run = |app: &mut App, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400.0, 600.0),
                )),
                events,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| objects_docker(app, ui));
            out.textures_delta.clear();
        };
        let click = |app: &mut App, what: &str| {
            run(app, Vec::new());
            let wid = egui::Id::new(("objects_object", id.raw())).with(what);
            let at = ctx.read_response(wid).expect("drawn").rect.center();
            run(app, vec![egui::Event::PointerMoved(at)]);
            let button = |pressed| egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            run(app, vec![button(true)]);
            run(app, vec![button(false)]);
        };
        click(&mut app, "eye");
        assert!(!app.doc().find_shape(id).expect("shape").visible);
        click(&mut app, "eye");
        assert!(app.doc().find_shape(id).expect("shape").visible);
        click(&mut app, "lock");
        assert!(app.doc().find_shape(id).expect("shape").locked);
        app.undo();
        assert!(!app.doc().find_shape(id).expect("shape").locked);
    }

    /// A Guides row opens to list its guidelines; each has its own eye,
    /// which hides that guideline alone (not drawn, picked or snapped
    /// to), and its own padlock; a click on its row selects it. Each is
    /// one undo step, and a hidden guideline stays hidden when saved
    /// (#1775).
    #[test]
    fn each_guideline_has_its_own_row_eye_and_padlock() {
        use tracedraw_core::document::Guide;
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        app.choose_master_guides(true);
        app.add_guide(Guide::vertical(50.0));
        app.add_guide(Guide::horizontal(80.0));
        let run = |app: &mut App, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400.0, 800.0),
                )),
                events,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| objects_docker(app, ui));
            out.textures_delta.clear();
        };
        let click = |app: &mut App, wid: egui::Id| {
            run(app, Vec::new());
            let at = ctx.read_response(wid).expect("drawn").rect.center();
            run(app, vec![egui::Event::PointerMoved(at)]);
            let button = |pressed| egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            run(app, vec![button(true)]);
            run(app, vec![button(false)]);
        };
        let base = egui::Id::new("objects_guides");
        run(&mut app, Vec::new());
        assert!(
            ctx.read_response(base.with(("guide", 0)).with("eye"))
                .is_none(),
            "closed at first"
        );
        click(&mut app, base.with("arrow"));
        click(&mut app, base.with(("guide", 0)).with("eye"));
        assert!(app.doc().master_guides[0].hidden);
        assert!(!app.doc().master_guides[1].hidden);
        let page = app.page;
        let shown = app.doc().guides_on_page(page).expect("page");
        assert_eq!(shown.len(), 1, "the hidden one is not snapped to");
        assert_eq!(
            app.guide_at(tracedraw_core::geometry::Point::new(50.0, 10.0)),
            None
        );
        // Saved and opened again, it stays hidden.
        let json = app.doc().to_json().expect("save");
        let back = tracedraw_core::Document::from_json(&json).expect("open");
        assert!(back.master_guides[0].hidden);
        app.undo();
        assert!(!app.doc().master_guides[0].hidden, "one undo step");
        click(&mut app, base.with(("guide", 1)).with("lock"));
        assert!(app.doc().master_guides[1].locked);
        assert!(!app.doc().master_guides[0].locked);
        // A click on the row selects that guideline.
        app.choose_master_guides(false);
        run(&mut app, Vec::new());
        let row = ctx
            .read_response(base.with(("guide", 1)).with("lock"))
            .expect("drawn")
            .rect;
        let pos = egui::pos2(row.right() + 120.0, row.center().y);
        run(&mut app, vec![egui::Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            run(
                &mut app,
                vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }
        assert!(app.edit_master_guides);
        assert_eq!(app.selected_guides, vec![1]);
    }

    /// A guideline dragged out of a ruler opens its Guides row in the
    /// docker and shows highlighted there; closed by hand, the row stays
    /// closed until another guideline is picked (#2882).
    #[test]
    fn a_new_guideline_opens_its_guides_row() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        app.canvas_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(4000.0, 4000.0));
        let run = |app: &mut App| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400.0, 800.0),
                )),
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| objects_docker(app, ui));
            out.textures_delta.clear();
        };
        // New guidelines go to every page by default: the master Guides row.
        assert!(app.edit_master_guides);
        let base = egui::Id::new("objects_guides");
        run(&mut app);
        assert!(
            ctx.read_response(base.with("arrow")).is_none(),
            "no guidelines yet"
        );
        let pos = app.view.to_page(egui::pos2(200.0, 200.0));
        app.drag = crate::app::Drag::NewGuide {
            horizontal: true,
            pos,
        };
        app.finish_guide_drag();
        assert_eq!(
            app.selected_guides,
            vec![0],
            "the new guideline is selected"
        );
        run(&mut app);
        run(&mut app);
        assert!(
            ctx.read_response(base.with(("guide", 0)).with("eye"))
                .is_some(),
            "the row opened on the new guideline"
        );
        // Closed by hand, it stays closed while the selection is the same.
        let at = ctx
            .read_response(base.with("arrow"))
            .expect("drawn")
            .rect
            .center();
        for events in [
            vec![egui::Event::PointerMoved(at)],
            vec![egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }],
            vec![egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        ] {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400.0, 800.0),
                )),
                events,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| objects_docker(&mut app, ui));
            out.textures_delta.clear();
        }
        // Widgets of the frame before are still read: one more frame.
        run(&mut app);
        run(&mut app);
        assert!(
            ctx.read_response(base.with(("guide", 0)).with("eye"))
                .is_none(),
            "closed by hand"
        );
        // Another guideline dragged out opens it again.
        let pos = app.view.to_page(egui::pos2(300.0, 300.0));
        app.drag = crate::app::Drag::NewGuide {
            horizontal: false,
            pos,
        };
        app.finish_guide_drag();
        assert_eq!(app.selected_guides, vec![1]);
        run(&mut app);
        run(&mut app);
        assert!(ctx
            .read_response(base.with(("guide", 1)).with("eye"))
            .is_some());
    }

    /// Guidelines move between sets (#3599): a guideline row dropped on
    /// the master page's Guides row moves it there, with the guidelines
    /// selected with it, in one undo step, a locked one staying; a Guides
    /// row with no guidelines takes a paste from its own menu.
    #[test]
    fn guidelines_move_between_sets_and_paste_into_an_empty_one() {
        use tracedraw_core::document::Guide;
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        let page = app.page;
        app.choose_master_guides(false);
        app.add_guide(Guide::vertical(50.0));
        app.add_guide(Guide::horizontal(80.0));
        app.add_guide(Guide::vertical(120.0));
        app.set_guides_locked(&[2], true);
        assert!(app.doc().master_guides.is_empty());
        let apply_now = |app: &mut App, act: Actions| {
            let mut act = Some(act);
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                if let Some(act) = act.take() {
                    apply(app, ui, act);
                }
            });
            out.textures_delta.clear();
        };
        // Dropped on the master page's row: the row's guideline and the
        // other one selected with it go; the locked one stays.
        app.selected_guides = vec![0, 2];
        apply_now(
            &mut app,
            Actions {
                move_guides: Some((Some(page), 0, None)),
                ..Default::default()
            },
        );
        let master = app.doc().master_guides.clone();
        assert_eq!(master.len(), 1, "{master:?}");
        assert!(matches!(master[0].line, GuideLine::Vertical { .. }));
        assert_eq!(app.doc().page(page).unwrap().guides.len(), 2);
        assert!(app.edit_master_guides);
        assert_eq!(app.selected_guides, vec![0]);
        app.engine.undo().unwrap();
        assert!(app.doc().master_guides.is_empty());
        assert_eq!(app.doc().page(page).unwrap().guides.len(), 3);
        // Two moved back and forth, the selection going with the row.
        assert_eq!(app.move_guides_between(Some(page), &[0, 1], None), 2);
        assert_eq!(app.move_guides_between(None, &[1], Some(page)), 1);
        assert_eq!(app.doc().master_guides.len(), 1);
        assert_eq!(app.doc().page(page).unwrap().guides.len(), 2);
        assert_eq!(app.move_guides_between(Some(page), &[0], Some(page)), 0);
        // An empty set takes a paste from its row's menu.
        app.engine.undo().unwrap();
        app.engine.undo().unwrap();
        app.choose_master_guides(false);
        app.selected_guides = vec![1];
        assert!(app.copy_selected_guides());
        apply_now(
            &mut app,
            Actions {
                paste_guides: Some(None),
                ..Default::default()
            },
        );
        assert_eq!(app.doc().master_guides.len(), 1);
        assert!(matches!(
            app.doc().master_guides[0].line,
            GuideLine::Horizontal { .. }
        ));
    }

    /// A guideline chosen by its row in the Objects docker goes with the
    /// Delete key, and its row's right-click menu deletes, cuts, copies,
    /// pastes, locks and hides it; a locked one is not deleted (#3012).
    #[test]
    fn a_guideline_row_deletes_and_has_a_menu() {
        use tracedraw_core::document::Guide;
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        app.choose_master_guides(false);
        app.add_guide(Guide::vertical(50.0));
        app.add_guide(Guide::horizontal(80.0));
        app.add_guide(Guide::vertical(120.0));
        let run = |app: &mut App, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400.0, 800.0),
                )),
                events,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                objects_docker(app, ui);
                app.keyboard(ui.ctx());
            });
            out.textures_delta.clear();
        };
        let press = |app: &mut App, at: egui::Pos2, button: egui::PointerButton| {
            run(app, vec![egui::Event::PointerMoved(at)]);
            for pressed in [true, false] {
                run(
                    app,
                    vec![egui::Event::PointerButton {
                        pos: at,
                        button,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    }],
                );
            }
        };
        let page = app.page;
        let base = egui::Id::new(("objects_guides", page.raw()));
        run(&mut app, Vec::new());
        // The page's Guides row, opened.
        let at = ctx
            .read_response(base.with("arrow"))
            .expect("arrow")
            .rect
            .center();
        press(&mut app, at, egui::PointerButton::Primary);
        let row_at = |app: &mut App, i: usize| {
            run(app, Vec::new());
            let lock = ctx
                .read_response(base.with(("guide", i)).with("lock"))
                .expect("drawn")
                .rect;
            egui::pos2(lock.right() + 120.0, lock.center().y)
        };
        let p = row_at(&mut app, 1);
        press(&mut app, p, egui::PointerButton::Primary);
        assert_eq!(app.selected_guides, vec![1]);
        let key = |pressed| egui::Event::Key {
            key: egui::Key::Delete,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        run(&mut app, vec![key(true), key(false)]);
        assert_eq!(
            app.page_guides().len(),
            2,
            "Delete took the chosen guideline"
        );
        assert!(app
            .page_guides()
            .iter()
            .all(|g| !matches!(g.line, GuideLine::Horizontal { .. })));
        // The row's menu opens on a right-click.
        let p = row_at(&mut app, 0);
        press(&mut app, p, egui::PointerButton::Secondary);
        run(&mut app, Vec::new());
        assert!(egui::Popup::is_any_open(&ctx), "a menu");
        // Its commands.
        guide_menu_command(&mut app, 0, GuideCmd::Copy);
        guide_menu_command(&mut app, 0, GuideCmd::Paste);
        assert_eq!(app.page_guides().len(), 3, "pasted");
        guide_menu_command(&mut app, 0, GuideCmd::Lock(true));
        assert!(app.page_guides()[0].locked);
        guide_menu_command(&mut app, 0, GuideCmd::Delete);
        assert_eq!(app.page_guides().len(), 3, "a locked one stays");
        guide_menu_command(&mut app, 0, GuideCmd::Lock(false));
        guide_menu_command(&mut app, 0, GuideCmd::Cut);
        assert_eq!(app.page_guides().len(), 2, "cut");
        assert!(app.clipboard.as_ref().is_some_and(|c| c.guides.len() == 1));
        guide_menu_command(&mut app, 1, GuideCmd::Hide(true));
        assert!(app.page_guides()[1].hidden);
        assert!(app.selected_guides.is_empty());
        guide_menu_command(&mut app, 0, GuideCmd::Delete);
        assert_eq!(app.page_guides().len(), 1);
    }

    /// A layer's menu moves it to another page with its objects and
    /// look, in one undo step, and that page is shown; a master layer
    /// goes the same way. Its Properties change its name and switches in
    /// one undo step (#1477).
    #[test]
    fn a_layer_moves_to_another_page_and_has_properties() {
        let mut app = without_desktop();
        let first = app.page;
        app.run(Command::AddLayer {
            page: first,
            name: "Art".into(),
        });
        let art = app.doc().pages[0].layers[1].id;
        app.picked_layer = Some(art);
        let id = app.new_shape(rect_kind()).expect("shape");
        assert_eq!(layer_holding(&app, id), Some(art));
        app.run(Command::SetLayerPrintable {
            layer: art,
            printable: false,
        });
        app.add_page();
        let second = app.doc().pages[1].id;
        app.page = first;
        layer_to_page(&mut app, art, second);
        assert_eq!(app.page, second);
        let doc = app.doc();
        assert!(doc.pages[0].layers.iter().all(|l| l.id != art));
        let moved = doc.pages[1].layers.last().expect("layer");
        assert_eq!(moved.name, "Art");
        assert!(!moved.printable, "its look goes with it");
        assert_eq!(layer_holding(&app, id), Some(moved.id));
        let moved = moved.id;
        app.undo();
        assert_eq!(layer_holding(&app, id), Some(art), "one undo step");
        app.redo();
        // A master layer to the first page.
        new_master_layer(&mut app, MasterScope::All);
        let master = app.doc().master[0].id;
        app.picked_layer = Some(master);
        let on_master = app.new_shape(rect_kind()).expect("shape");
        layer_to_page(&mut app, master, first);
        assert!(app.doc().master.is_empty());
        assert_eq!(app.page, first);
        let there = layer_holding(&app, on_master).expect("layer");
        assert!(app.doc().pages[0].layers.iter().any(|l| l.id == there));
        // Properties: name, eye, printer and padlock in one step.
        let mut st = LayerProps::of(&app, moved).expect("layer");
        assert!(!st.master);
        st.name = "Final art".into();
        st.locked = true;
        st.printable = true;
        let cmds = st.commands(&app);
        assert_eq!(cmds.len(), 3);
        app.engine
            .run_batch("Layer Properties", &cmds)
            .expect("properties");
        let l = app.doc().layer(moved).expect("layer").clone();
        assert_eq!(l.name, "Final art");
        assert!(l.locked && l.printable);
        app.undo();
        let l = app.doc().layer(moved).expect("layer").clone();
        assert_eq!(l.name, "Art");
        assert!(!l.locked && !l.printable);
        // Nothing changed, nothing to do; a master layer's pages too.
        assert!(LayerProps::of(&app, moved)
            .expect("layer")
            .commands(&app)
            .is_empty());
        new_master_layer(&mut app, MasterScope::All);
        let m = app.doc().master[0].id;
        let mut st = LayerProps::of(&app, m).expect("layer");
        assert!(st.master);
        st.scope = MasterScope::Even;
        st.in_front = !st.in_front;
        app.engine
            .run_batch("Layer Properties", &st.commands(&app))
            .expect("properties");
        assert_eq!(app.doc().master[0].scope, MasterScope::Even);
    }

    /// With several objects selected, the eye of one of them hides them
    /// all, or shows them all when it is hidden, whatever each was; they
    /// stay selected and one undo brings them back. An object outside
    /// the selection goes alone (#2571).
    #[test]
    fn the_eye_of_a_selected_object_hides_and_shows_the_whole_selection() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        let a = app.new_shape(rect_kind()).expect("shape");
        let b = app.new_shape(rect_kind()).expect("shape");
        let c = app.new_shape(rect_kind()).expect("shape");
        let other = app.new_shape(rect_kind()).expect("shape");
        // One of the selection already hidden.
        app.run(Command::SetVisible {
            shapes: vec![c],
            visible: false,
        });
        app.select(vec![a, b, c]);
        let run = |app: &mut App, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400.0, 600.0),
                )),
                events,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| objects_docker(app, ui));
            out.textures_delta.clear();
        };
        let eye = |app: &mut App, id: ShapeId| {
            run(app, Vec::new());
            let wid = egui::Id::new(("objects_object", id.raw())).with("eye");
            let at = ctx.read_response(wid).expect("drawn").rect.center();
            run(app, vec![egui::Event::PointerMoved(at)]);
            let button = |pressed| egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            run(app, vec![button(true)]);
            run(app, vec![button(false)]);
        };
        let visible = |app: &App, id: ShapeId| app.doc().find_shape(id).expect("shape").visible;
        // A visible one's eye hides the three.
        eye(&mut app, a);
        assert!(!visible(&app, a) && !visible(&app, b) && !visible(&app, c));
        assert!(visible(&app, other));
        let mut sel = app.selection.clone();
        sel.sort();
        let mut want = vec![a, b, c];
        want.sort();
        assert_eq!(sel, want, "the selection stays");
        // One undo step brings back each as it was.
        app.undo();
        assert!(visible(&app, a) && visible(&app, b) && !visible(&app, c));
        // A hidden one's eye shows them all.
        eye(&mut app, c);
        assert!(visible(&app, a) && visible(&app, b) && visible(&app, c));
        // The eye of an object outside the selection hides it alone.
        eye(&mut app, other);
        assert!(!visible(&app, other));
        assert!(visible(&app, a) && visible(&app, b) && visible(&app, c));
    }

    /// Selecting an object on the page scrolls the list to its row when
    /// it is out of view, as when it lies far down a long list (#1044).
    /// A selection that stays the same leaves the scroll alone.
    /// One frame of the docker, `size` wide and high, with `events`.
    fn frame(ctx: &egui::Context, app: &mut App, size: f32, events: Vec<egui::Event>) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(size, size),
            )),
            events,
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| objects_docker(app, ui));
        out.textures_delta.clear();
    }

    fn wheel(at: Pos2, notches: f32, modifiers: egui::Modifiers) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(at),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: Vec2::new(0.0, notches),
                phase: egui::TouchPhase::Move,
                modifiers,
            },
        ]
    }

    /// Each object row shows a thumbnail of the object, drawn once and
    /// drawn again only when that object changes (#896).
    #[test]
    fn rows_show_thumbnails_drawn_once_per_change() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        let ids: Vec<ShapeId> = (0..3)
            .map(|_| app.new_shape(rect_kind()).expect("shape"))
            .collect();
        app.selection.clear();
        for _ in 0..3 {
            frame(&ctx, &mut app, 400.0, Vec::new());
        }
        assert_eq!(app.object_thumbs.renders, 3);
        assert_eq!(app.object_thumbs.len(), 3);
        assert!(app.object_thumbs.bytes() > 0);
        // A change to one object draws that one again, the others stay.
        app.run(Command::SetFill {
            shapes: vec![ids[1]],
            fill: tracedraw_core::Fill::Solid(tracedraw_core::Color::rgb8(0, 200, 0)),
        });
        for _ in 0..3 {
            frame(&ctx, &mut app, 400.0, Vec::new());
        }
        assert_eq!(app.object_thumbs.renders, 4);
        // Selecting (no change to the drawing) draws nothing.
        app.select(vec![ids[0]]);
        frame(&ctx, &mut app, 400.0, Vec::new());
        assert_eq!(app.object_thumbs.renders, 4);
    }

    /// Ctrl+wheel over the list and the wheel over the bar's slider
    /// change the thumbnail size by one step a notch; the rows grow.
    #[test]
    fn ctrl_wheel_and_the_bar_change_the_thumbnail_size() {
        use crate::object_thumbs::{DEFAULT_SIZE, MAX_SIZE, STEP};
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        let id = app.new_shape(rect_kind()).expect("shape");
        app.selection.clear();
        assert_eq!(app.settings.object_thumb_size, DEFAULT_SIZE);
        let eye = |ctx: &egui::Context| {
            ctx.read_response(egui::Id::new(("objects_object", id.raw())).with("eye"))
                .map(|r| r.rect)
        };
        frame(&ctx, &mut app, 400.0, Vec::new());
        let before = eye(&ctx).expect("row drawn");
        // A plain wheel over the list scrolls; it leaves the size.
        frame(
            &ctx,
            &mut app,
            400.0,
            wheel(Pos2::new(200.0, 60.0), 1.0, egui::Modifiers::NONE),
        );
        assert_eq!(app.settings.object_thumb_size, DEFAULT_SIZE);
        for _ in 0..2 {
            frame(
                &ctx,
                &mut app,
                400.0,
                wheel(Pos2::new(200.0, 60.0), 1.0, egui::Modifiers::CTRL),
            );
        }
        assert_eq!(app.settings.object_thumb_size, DEFAULT_SIZE + 2.0 * STEP);
        // (Responses are read a frame late.)
        frame(&ctx, &mut app, 400.0, Vec::new());
        frame(&ctx, &mut app, 400.0, Vec::new());
        // The row is taller; the thumbnail is drawn again at the new size.
        let after = eye(&ctx).expect("row drawn");
        assert!(after.center().y > before.center().y, "{before:?} {after:?}");
        // The wheel over the slider in the bottom bar, down a notch.
        frame(
            &ctx,
            &mut app,
            400.0,
            wheel(Pos2::new(200.0, 385.0), -1.0, egui::Modifiers::NONE),
        );
        assert_eq!(app.settings.object_thumb_size, DEFAULT_SIZE + STEP);
        // Never past the largest size.
        for _ in 0..30 {
            frame(
                &ctx,
                &mut app,
                400.0,
                wheel(Pos2::new(200.0, 60.0), 1.0, egui::Modifiers::CTRL),
            );
        }
        assert_eq!(app.settings.object_thumb_size, MAX_SIZE);
    }

    /// In the browser a wheel notch comes as three lines (Firefox) or as
    /// 100 pixels (Chromium): Ctrl+wheel over the list and the wheel over
    /// the bar's slider still change the thumbnail size by one step a
    /// notch, as on the desktop; they took three or two (#1406). Half a
    /// browser notch from a touchpad adds up to a step with the next half.
    #[test]
    fn a_browser_wheel_notch_steps_the_thumbnail_size_once() {
        use crate::object_thumbs::{DEFAULT_SIZE, STEP};
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        app.new_shape(rect_kind()).expect("shape");
        app.selection.clear();
        frame(&ctx, &mut app, 400.0, Vec::new());
        let turn = |at: Pos2, unit, dy: f32, modifiers| {
            vec![
                egui::Event::PointerMoved(at),
                egui::Event::MouseWheel {
                    unit,
                    delta: Vec2::new(0.0, dy),
                    phase: egui::TouchPhase::Move,
                    modifiers,
                },
            ]
        };
        let list = Pos2::new(200.0, 60.0);
        let bar = Pos2::new(200.0, 385.0);
        let ctrl = egui::Modifiers::CTRL;
        let none = egui::Modifiers::NONE;
        use egui::MouseWheelUnit::{Line, Point};
        crate::ui::field::set_web_wheel(true);
        let mut size = DEFAULT_SIZE;
        for (at, unit, dy, mods, step) in [
            (list, Line, 3.0, ctrl, 1.0),
            (list, Point, 100.0, ctrl, 1.0),
            (list, Line, -3.0, ctrl, -1.0),
            (bar, Point, -100.0, none, -1.0),
            (bar, Line, 3.0, none, 1.0),
            (list, Point, 50.0, ctrl, 0.0),
            (list, Point, 50.0, ctrl, 1.0),
        ] {
            frame(&ctx, &mut app, 400.0, turn(at, unit, dy, mods));
            size += step * STEP;
            assert_eq!(app.settings.object_thumb_size, size, "{unit:?} {dy}");
        }
        // The desktop's notch, one line or 50 points, is one step as before.
        crate::ui::field::set_web_wheel(false);
        for (unit, dy) in [(Line, -1.0), (Point, -50.0)] {
            frame(&ctx, &mut app, 400.0, turn(list, unit, dy, ctrl));
            size -= STEP;
            assert_eq!(app.settings.object_thumb_size, size, "{unit:?} {dy}");
        }
    }

    /// A drawing of thousands of objects: only the rows in sight get
    /// thumbnails, a frame with nothing changed draws none, and the cache
    /// stays small.
    #[test]
    fn a_long_list_draws_only_the_rows_in_sight() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        for i in 0..3000 {
            let x = f64::from(i % 50) * 4.0;
            let y = f64::from(i / 50) * 4.0;
            app.new_shape(ShapeKind::Rect {
                rect: tracedraw_core::Rect::new(x, y, x + 3.0, y + 3.0),
                radius: 0.0,
                corners: None,
            });
        }
        app.selection.clear();
        // A few thumbnails are drawn per frame: frames until the rows in
        // sight all have theirs (how many fit depends on the fonts).
        let mut drawn = u64::MAX;
        for _ in 0..30 {
            frame(&ctx, &mut app, 400.0, Vec::new());
            if app.object_thumbs.renders == drawn {
                break;
            }
            drawn = app.object_thumbs.renders;
        }
        assert!((5..40).contains(&drawn), "{drawn}");
        frame(&ctx, &mut app, 400.0, Vec::new());
        assert_eq!(app.object_thumbs.renders, drawn);
        assert!(app.object_thumbs.bytes() < 2 * 1024 * 1024);
    }

    /// Frame times of the docker over 3000 objects (`cargo test --release
    /// -p tracedraw -- --ignored objects_docker_frame_times --nocapture`):
    /// the first frames (thumbnails drawn), a steady frame, and frames
    /// scrolling the list.
    #[test]
    #[ignore]
    fn objects_docker_frame_times() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        for i in 0..3000 {
            let x = f64::from(i % 50) * 4.0;
            let y = f64::from(i / 50) * 4.0;
            app.new_shape(ShapeKind::Ellipse {
                rect: tracedraw_core::Rect::new(x, y, x + 3.0, y + 3.0),
                arc: None,
            });
        }
        app.selection.clear();
        let time = |app: &mut App, events: Vec<egui::Event>| {
            let t = web_time::Instant::now();
            frame(&ctx, app, 600.0, events);
            t.elapsed().as_secs_f64() * 1000.0
        };
        let first = time(&mut app, Vec::new());
        let mut fill = 0.0;
        for _ in 0..10 {
            fill += time(&mut app, Vec::new());
        }
        let steady: f64 = (0..20).map(|_| time(&mut app, Vec::new())).sum::<f64>() / 20.0;
        let scroll: f64 = (0..40)
            .map(|_| {
                time(
                    &mut app,
                    wheel(Pos2::new(300.0, 200.0), -3.0, egui::Modifiers::NONE),
                )
            })
            .sum::<f64>()
            / 40.0;
        println!(
            "objects docker, 3000 objects: first {first:.1} ms, next 10 {fill:.1} ms, \
             steady {steady:.2} ms, scrolling {scroll:.2} ms; {} thumbnails drawn, \
             {} kept, {} KB",
            app.object_thumbs.renders,
            app.object_thumbs.len(),
            app.object_thumbs.bytes() / 1024
        );
    }

    #[test]
    fn the_list_scrolls_to_an_object_selected_on_the_page() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        let ids: Vec<ShapeId> = (0..80)
            .map(|_| app.new_shape(rect_kind()).expect("shape"))
            .collect();
        app.selection.clear();
        // The scroll lands in one frame.
        ctx.all_styles_mut(|s| s.scroll_animation = egui::style::ScrollAnimation::none());
        let run = |app: &mut App, scroll: f32| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(300.0, 300.0),
                )),
                events: if scroll != 0.0 {
                    vec![
                        egui::Event::PointerMoved(Pos2::new(150.0, 100.0)),
                        egui::Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta: Vec2::new(0.0, scroll),
                            phase: egui::TouchPhase::Move,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ]
                } else {
                    Vec::new()
                },
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| objects_docker(app, ui));
            out.textures_delta.clear();
        };
        let row_of = |id: ShapeId| {
            ctx.read_response(egui::Id::new(("objects_object", id.raw())).with("eye"))
                .map(|r| r.rect)
        };
        let shown =
            |r: Option<egui::Rect>| r.is_some_and(|r| r.top() >= 0.0 && r.bottom() <= 300.0);
        for _ in 0..4 {
            run(&mut app, 0.0);
        }
        // The first object drawn is the bottom of the list.
        let far = ids[0];
        assert!(!shown(row_of(far)), "{:?}", row_of(far));
        app.select(vec![far]);
        for _ in 0..4 {
            run(&mut app, 0.0);
        }
        assert!(shown(row_of(far)), "{:?}", row_of(far));
        // Scrolled away by hand, the unchanged selection stays out of view.
        for _ in 0..4 {
            run(&mut app, 2000.0);
        }
        assert!(!shown(row_of(far)), "{:?}", row_of(far));
    }

    /// #3822: with an object far down the list selected, the list
    /// scrolled up by hand and a row there Ctrl+clicked, the list jumped
    /// back down to the first selected object; it stays where it is.
    #[test]
    fn a_ctrl_click_in_the_list_keeps_its_scroll() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        let ids: Vec<ShapeId> = (0..80)
            .map(|_| app.new_shape(rect_kind()).expect("shape"))
            .collect();
        ctx.all_styles_mut(|s| s.scroll_animation = egui::style::ScrollAnimation::none());
        let run = |app: &mut App, events: Vec<egui::Event>, ctrl: bool| {
            let mut all = vec![egui::Event::ModifiersChanged(if ctrl {
                egui::Modifiers::COMMAND
            } else {
                egui::Modifiers::NONE
            })];
            all.extend(events);
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(300.0, 300.0),
                )),
                events: all,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| objects_docker(app, ui));
            out.textures_delta.clear();
        };
        let row_of = |id: ShapeId| {
            ctx.read_response(egui::Id::new(("objects_object", id.raw())).with("eye"))
                .map(|r| r.rect)
        };
        let shown =
            |r: Option<egui::Rect>| r.is_some_and(|r| r.top() >= 0.0 && r.bottom() <= 300.0);
        let far = ids[0];
        app.select(vec![far]);
        for _ in 0..4 {
            run(&mut app, Vec::new(), false);
        }
        assert!(shown(row_of(far)), "{:?}", row_of(far));
        // Back up to the top of the list by hand.
        for _ in 0..4 {
            run(
                &mut app,
                vec![
                    egui::Event::PointerMoved(Pos2::new(150.0, 150.0)),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: Vec2::new(0.0, 5000.0),
                        phase: egui::TouchPhase::Move,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                false,
            );
        }
        let top = *ids.last().expect("ids");
        let r = row_of(top)
            .filter(|r| shown(Some(*r)))
            .expect("top row in view");
        // Ctrl+click on its name, left of its eye.
        let at = Pos2::new(r.left() - 100.0, r.center().y);
        let press = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::COMMAND,
        };
        run(&mut app, vec![egui::Event::PointerMoved(at)], true);
        run(&mut app, vec![press(true)], true);
        run(&mut app, vec![press(false)], true);
        for _ in 0..3 {
            run(&mut app, Vec::new(), false);
        }
        assert!(app.selection.contains(&top), "{:?}", app.selection);
        assert!(app.selection.contains(&far), "{:?}", app.selection);
        assert!(shown(row_of(top)), "jumped away: {:?}", row_of(top));
    }

    #[test]
    fn the_docker_draws_layers_groups_and_selection() {
        let mut app = App::headless();
        let id = app.new_shape(rect_kind()).expect("shape");
        app.select(vec![id]);
        new_master_layer(&mut app, MasterScope::Odd);
        let layer = app.doc().pages[0].layers[0].id;
        for collapsed in [false, true] {
            if collapsed {
                app.objects_collapsed.insert(layer);
                app.objects_master_closed = true;
            }
            draw(&mut app);
        }
        assert_eq!(app.selection, vec![id]);
    }

    /// #4400: an object picked inside a group in a clip frame opens
    /// both in the list, and its collapsed layer.
    #[test]
    fn a_nested_pick_opens_its_groups_in_the_list() {
        let mut app = App::headless();
        let mk = |app: &mut App, x: f64| {
            app.new_shape(ShapeKind::Rect {
                rect: tracedraw_core::geometry::Rect::new(x, 0.0, x + 10.0, 10.0),
                radius: 0.0,
                corners: None,
            })
            .expect("rect")
        };
        let a = mk(&mut app, 0.0);
        let b = mk(&mut app, 20.0);
        app.select(vec![a, b]);
        app.group_selection();
        let group = app.selection[0];
        let layer = app.active_layer().expect("layer");
        app.objects_collapsed.insert(layer);
        app.select(vec![a]);
        reveal_selection(&mut app);
        assert!(app.objects_open_groups.contains(&group));
        assert!(!app.objects_collapsed.contains(&layer));
        // A top-level pick opens nothing.
        app.objects_open_groups.clear();
        app.select(vec![group]);
        reveal_selection(&mut app);
        assert!(app.objects_open_groups.is_empty());
    }

    /// A collapsed layer marks that it holds the selection, a group
    /// selected in it or an object inside one of its groups (#4135).
    #[test]
    fn a_collapsed_layer_shows_it_holds_the_selection() {
        let mut app = App::headless();
        let a = app.new_shape(rect_kind()).expect("a");
        let b = app.new_shape(rect_kind()).expect("b");
        app.select(vec![a, b]);
        app.group_selection();
        let g = app.selection[0];
        let layer = app.doc().pages[0].layers[0].id;
        let shapes = app.doc().pages[0].layers[0].shapes.clone();
        let holds = |sel: &[ShapeId]| shapes.iter().any(|s| holds_selection(sel, s));
        assert!(holds(&[g]));
        assert!(holds(&[b]), "an object inside a group");
        assert!(!holds(&[]));
        app.objects_collapsed.insert(layer);
        draw(&mut app);
        assert_eq!(app.selection, vec![g]);
    }

    #[test]
    fn the_layer_search_matches_names_in_any_case() {
        crate::i18n::set_language("en");
        let mut app = without_desktop();
        new_layer(&mut app);
        let layers = &app.doc().pages[0].layers;
        assert!(layer_hit(&layers[1], ""));
        assert!(layer_hit(&layers[1], &"LAYER 2".to_lowercase()));
        assert!(!layer_hit(&layers[0], "layer 2"));
        let mut seen = String::new();
        with_ui(|ui| {
            ui.data_mut(|d| d.insert_temp(egui::Id::new(LAYER_FILTER), "  Layer 2 ".to_string()));
            seen = layer_needle(ui);
        });
        assert_eq!(seen, "layer 2");
        draw(&mut app);
    }

    #[test]
    fn new_layers_get_free_names_and_become_active() {
        crate::i18n::set_language("en");
        let mut app = without_desktop();
        let first = app.doc().pages[0].layers[0].id;
        new_layer(&mut app);
        new_layer(&mut app);
        let names: Vec<String> = app.doc().pages[0]
            .layers
            .iter()
            .map(|l| l.name.clone())
            .collect();
        assert_eq!(names, ["Layer 1", "Layer 2", "Layer 3"]);
        let top = app.doc().pages[0].layers[2].id;
        assert_eq!(app.active_layer(), Some(top));
        // Names in use are skipped.
        app.run(Command::DeleteLayer {
            layer: app.doc().pages[0].layers[1].id,
        });
        new_layer(&mut app);
        assert_eq!(app.doc().pages[0].layers[2].name, "Layer 4");
        // Picking a lower layer sends new objects there.
        app.picked_layer = Some(first);
        let id = app.new_shape(rect_kind()).expect("shape");
        assert!(app.doc().pages[0].layers[0]
            .shapes
            .iter()
            .any(|s| s.id == id));
        // A hidden or locked pick does not take them.
        app.run(Command::SetLayerVisible {
            layer: first,
            visible: false,
        });
        assert_ne!(app.active_layer(), Some(first));
        new_master_layer(&mut app, MasterScope::All);
        new_master_layer(&mut app, MasterScope::Even);
        let master: Vec<String> = app.doc().master.iter().map(|l| l.name.clone()).collect();
        assert_eq!(master, ["Master Layer 1", "Master Layer 2"]);
        // An even-pages master layer does not show on page 1, so it is
        // not where page 1's objects go.
        assert_ne!(app.active_layer(), app.doc().master.last().map(|l| l.id));
        app.picked_layer = app.doc().master.first().map(|l| l.id);
        assert_eq!(app.active_layer(), app.picked_layer);
    }

    /// Edit Across Layers off: a click and Select All take the objects of
    /// the active layer alone, and the selected objects of other layers
    /// are let go; on again, every layer is in reach (#3276).
    #[test]
    fn edit_across_layers_off_keeps_picking_to_the_active_layer() {
        let mut app = without_desktop();
        let first = app.doc().pages[0].layers[0].id;
        app.picked_layer = Some(first);
        let a = app.new_shape(rect_kind()).expect("a");
        app.run(Command::SetFill {
            shapes: vec![a],
            fill: tracedraw_core::Fill::Solid(tracedraw_core::Color::rgb8(200, 0, 0)),
        });
        new_layer(&mut app);
        let second = app.picked_layer.expect("new layer");
        let b = app.new_shape(rect_kind()).expect("b");
        app.run(Command::SetFill {
            shapes: vec![b],
            fill: tracedraw_core::Fill::Solid(tracedraw_core::Color::rgb8(0, 0, 200)),
        });
        let p = tracedraw_core::geometry::Point::new(5.0, 5.0);
        // On: the topmost object under the pointer, on the upper layer.
        assert!(app.settings.edit_across_layers);
        assert_eq!(app.hit_test(p), Some(b));
        app.select(vec![a, b]);
        // Off with the first layer active: its object alone.
        app.picked_layer = Some(first);
        set_edit_across_layers(&mut app, false);
        assert_eq!(app.selection, vec![a], "b let go");
        assert_eq!(app.hit_test(p), Some(a));
        assert_eq!(app.selectable_ids(), vec![a]);
        app.picked_layer = Some(second);
        assert_eq!(app.hit_test(p), Some(b));
        assert_eq!(app.selectable_ids(), vec![b]);
        set_edit_across_layers(&mut app, true);
        app.picked_layer = Some(first);
        assert_eq!(app.hit_test(p), Some(b));
        assert_eq!(app.selectable_ids().len(), 2);
    }

    /// Copy to Layer puts copies on the other layer at the same place,
    /// in the same order, and keeps the originals where they were; the
    /// copies are selected; one undo step takes them away (#3277).
    #[test]
    fn copy_to_layer_leaves_the_originals_and_selects_the_copies() {
        let mut app = without_desktop();
        let first = app.doc().pages[0].layers[0].id;
        app.picked_layer = Some(first);
        let a = app.new_shape(rect_kind()).expect("a");
        let b = app.new_shape(rect_kind()).expect("b");
        app.run(Command::TransformShapes {
            shapes: vec![b],
            transform: tracedraw_core::geometry::Affine::translate((20.0, 5.0)),
        });
        new_layer(&mut app);
        let second = app.picked_layer.expect("new layer");
        assert_ne!(first, second);
        copy_to_layer(&mut app, &[b, a], second);
        let layer = |app: &App, id| app.doc().layer(id).expect("layer").shapes.clone();
        let originals: Vec<ShapeId> = layer(&app, first).iter().map(|s| s.id).collect();
        assert_eq!(originals, vec![a, b], "the originals stay");
        let copies = layer(&app, second);
        assert_eq!(copies.len(), 2);
        assert!(copies.iter().all(|c| c.id != a && c.id != b));
        let tb = app.doc().find_shape(b).expect("b").transform;
        assert_eq!(copies[1].transform, tb, "same place, same order");
        let ids: Vec<ShapeId> = copies.iter().map(|c| c.id).collect();
        assert_eq!(app.selection, ids);
        app.undo();
        assert!(layer(&app, second).is_empty());
        assert_eq!(layer(&app, first).len(), 2);
    }

    /// #1132, #1477: the in-front choice per kind of master layer moves
    /// the master layers of that kind the drawing has in one step and
    /// is taken by the new ones; a master object can be moved onto a
    /// page's layer, a page layer into the master page and back.
    #[test]
    fn master_layers_in_front_by_choice_and_objects_move_between_layers() {
        let mut app = without_desktop();
        app.settings.master_in_front = [false; 3];
        new_master_layer(&mut app, MasterScope::All);
        new_master_layer(&mut app, MasterScope::Odd);
        assert!(app.doc().master.iter().all(|l| !l.in_front));
        set_master_front(&mut app, MasterScope::All, true);
        assert!(app.doc().master[0].in_front);
        assert!(!app.doc().master[1].in_front, "odd pages keep theirs");
        // One undo step for the drawing; the choice stays.
        app.undo();
        assert!(!app.doc().master[0].in_front);
        assert!(master_front_default(&app, MasterScope::All));
        // A new all-pages master layer comes in front, in one step.
        new_master_layer(&mut app, MasterScope::All);
        let front = app.doc().master[2].id;
        assert!(app.doc().master[2].in_front);
        app.undo();
        assert!(app.doc().master.iter().all(|l| l.id != front));
        // It is drawn over the page's own layers.
        new_master_layer(&mut app, MasterScope::All);
        let master = app.doc().master.last().expect("master").id;
        let id = app.new_shape(rect_kind()).expect("shape");
        assert_eq!(layer_holding(&app, id), Some(master));
        let layers: Vec<LayerId> = app.page_layers().iter().map(|l| l.id).collect();
        assert_eq!(layers.last(), Some(&master));
        // Move to a layer of the page: it shows on this page only.
        let page_layer = app.doc().pages[0].layers[0].id;
        move_to_layer(&mut app, &[id], page_layer);
        assert_eq!(layer_holding(&app, id), Some(page_layer));
        assert_eq!(app.selection, vec![id]);
        assert!(app.doc().master.last().expect("master").shapes.is_empty());
        app.undo();
        assert_eq!(layer_holding(&app, id), Some(master), "one undo step");
        app.redo();
        // To a layer of another page: that page becomes the current one.
        let first = app.page;
        app.run(Command::duplicate_page(first));
        let second = app.doc().pages[1].id;
        let there = app.doc().pages[1].layers[0].id;
        move_to_layer(&mut app, &[id], there);
        assert_eq!(app.page, second);
        assert_eq!(layer_holding(&app, id), Some(there));
        // A page layer into the master page, with its objects, and back.
        layer_to_master(&mut app, there);
        let moved = app.doc().master.last().expect("master").id;
        assert_eq!(layer_holding(&app, id), Some(moved));
        assert_eq!(
            app.doc().pages[1].layers.len(),
            1,
            "a page keeps its last layer"
        );
        master_to_page(&mut app, moved);
        assert!(app.doc().master.iter().all(|l| l.id != moved));
        let back = layer_holding(&app, id).expect("on a layer");
        assert!(app.doc().pages[1].layers.iter().any(|l| l.id == back));
        // The docker lists every page, or the current one.
        draw(&mut app);
        app.settings.objects_all_pages = false;
        draw(&mut app);
    }

    /// #1161, #1775: a new drawing has the master page's desktop layer,
    /// behind the pages; new objects still go to the page's own layer,
    /// and an object moved onto the desktop shows and is picked from
    /// every page, beside the page or on it. It is not one of the
    /// numbered master layers, and the in-front choice leaves it behind.
    #[test]
    fn a_new_drawing_has_a_desktop_shared_by_every_page() {
        let mut app = App::headless();
        let desktop: Vec<&tracedraw_core::Layer> =
            app.doc().master.iter().filter(|l| l.desktop).collect();
        assert_eq!(desktop.len(), 1);
        assert_eq!(desktop[0].name, "Desktop");
        assert_eq!(desktop[0].scope, MasterScope::All);
        assert!(!desktop[0].in_front);
        let desktop = desktop[0].id;
        let page_layer = app.doc().pages[0].layers[0].id;
        assert_eq!(app.active_layer(), Some(page_layer));
        // An object off the page, moved onto the desktop.
        let id = app
            .new_shape(tracedraw_core::document::ShapeKind::Rect {
                rect: tracedraw_core::geometry::Rect::new(-60.0, 10.0, -20.0, 40.0),
                radius: 0.0,
                corners: None,
            })
            .expect("shape");
        assert_eq!(layer_holding(&app, id), Some(page_layer));
        move_to_layer(&mut app, &[id], desktop);
        assert_eq!(layer_holding(&app, id), Some(desktop));
        // From a new page it is there and picked.
        app.add_page();
        assert_ne!(app.page, app.doc().pages[0].id);
        let edge = tracedraw_core::Point::new(-60.0, 25.0);
        assert_eq!(app.hit_test(edge), Some(id));
        assert!(app
            .page_layers()
            .iter()
            .any(|l| l.id == desktop && l.shapes.iter().any(|s| s.id == id)));
        // Numbered master layers start at 1; the in-front choice for
        // all pages leaves the desktop behind.
        app.settings.master_in_front = [false; 3];
        new_master_layer(&mut app, MasterScope::All);
        assert_eq!(
            app.doc().master.last().expect("master").name,
            "Master Layer 1"
        );
        set_master_front(&mut app, MasterScope::All, true);
        let d = app
            .doc()
            .master
            .iter()
            .find(|l| l.id == desktop)
            .expect("desktop");
        assert!(!d.in_front);
        assert!(app.doc().master.last().expect("master").in_front);
    }

    #[test]
    fn master_layer_objects_are_picked_where_their_layer_shows() {
        let mut app = without_desktop();
        new_master_layer(&mut app, MasterScope::Odd);
        let master = app.doc().master[0].id;
        assert_eq!(app.active_layer(), Some(master));
        let id = app.new_shape(rect_kind()).expect("shape");
        assert_eq!(app.doc().master[0].shapes.len(), 1);
        app.select(Vec::new());
        // On its outline (new rectangles have no fill).
        let inside = tracedraw_core::Point::new(0.0, 5.0);
        assert_eq!(app.hit_test(inside), Some(id));
        app.select_all();
        assert_eq!(app.selection, vec![id]);
        // Page 2 is even: the layer does not show there.
        let page = app.page;
        app.run(Command::duplicate_page(page));
        app.page = app.doc().pages[1].id;
        assert_eq!(app.hit_test(inside), None);
        app.page = page;
        app.run(Command::SetLayerLocked {
            layer: master,
            locked: true,
        });
        assert_eq!(app.hit_test(inside), None);
    }

    #[test]
    fn delete_takes_the_selection_else_the_active_layer() {
        let mut app = without_desktop();
        let only = app.doc().pages[0].layers[0].id;
        let id = app.new_shape(rect_kind()).expect("shape");
        app.select(vec![id]);
        delete_current(&mut app);
        assert!(app.doc().find_shape(id).is_none());
        // A page keeps its last layer.
        assert_eq!(deletable_layer(&app), None);
        delete_current(&mut app);
        assert_eq!(app.doc().pages[0].layers.len(), 1);
        new_layer(&mut app);
        delete_current(&mut app);
        assert_eq!(app.doc().pages[0].layers.len(), 1);
        assert_eq!(app.doc().pages[0].layers[0].id, only);
        new_master_layer(&mut app, MasterScope::All);
        delete_current(&mut app);
        assert!(app.doc().master.is_empty());
        assert_eq!(app.picked_layer, None);
        assert_eq!(app.active_layer(), Some(only));
    }

    #[test]
    fn clicking_an_object_in_the_docker_makes_its_layer_active() {
        let mut app = App::headless();
        let low = app.doc().pages[0].layers[0].id;
        let id = app.new_shape(rect_kind()).expect("shape");
        new_layer(&mut app);
        assert_ne!(app.active_layer(), Some(low));
        with_ui(|ui| {
            apply(
                &mut app,
                ui,
                Actions {
                    select: Some(id),
                    ..Default::default()
                },
            );
        });
        assert_eq!(app.selection, vec![id]);
        assert_eq!(app.active_layer(), Some(low));
    }

    /// A layer's menu copies or cuts its objects and pastes the
    /// clipboard onto it (#1992).
    #[test]
    fn a_layers_menu_cuts_copies_and_pastes_its_objects() {
        let mut app = App::headless();
        let low = app.doc().pages[0].layers[0].id;
        app.new_shape(rect_kind()).expect("shape");
        app.new_shape(rect_kind()).expect("shape");
        new_layer(&mut app);
        let high = app.active_layer().expect("new layer");
        assert_ne!(high, low);
        let count = |app: &App, id: LayerId| {
            app.doc()
                .all_layers()
                .find(|l| l.id == id)
                .map_or(0, |l| l.shapes.len())
        };
        let run = |app: &mut App, clip: (LayerId, LayerClip)| {
            with_ui(|ui| {
                apply(
                    app,
                    ui,
                    Actions {
                        layer_clip: Some(clip),
                        ..Default::default()
                    },
                );
            });
        };
        run(&mut app, (low, LayerClip::Copy));
        run(&mut app, (high, LayerClip::Paste));
        assert_eq!((count(&app, low), count(&app, high)), (2, 2));
        run(&mut app, (low, LayerClip::Cut));
        assert_eq!(count(&app, low), 0);
        run(&mut app, (low, LayerClip::Paste));
        assert_eq!(count(&app, low), 2);
    }

    /// #3966: in the Objects docker a layer row takes the same clicks as
    /// an object row: alone it is picked, Ctrl+click adds it or takes it
    /// out, Shift+click selects the run from the last one clicked (across
    /// the page's layers and the master layers), Ctrl+Shift+click adds
    /// that run. Picking an object on the page lets the layers go.
    #[test]
    fn ctrl_and_shift_clicks_select_multiple_layers() {
        let mut app = without_desktop();
        let first = app.doc().pages[0].layers[0].id;
        new_layer(&mut app);
        let second = app.picked_layer.expect("new layer");
        new_master_layer(&mut app, MasterScope::All);
        let master = app.picked_layer.expect("master layer");
        // The list shows the page's layers topmost first, then the
        // Master Page's.
        let order = vec![second, first, master];
        // A plain click picks that layer alone, and lets the objects go.
        let a = select_layer_row(&mut app, first, &order, None, false, false);
        assert_eq!(a, first);
        assert_eq!(app.selected_layers, vec![first]);
        assert_eq!(app.picked_layer, Some(first));
        assert!(app.selection.is_empty());
        // Ctrl+click adds a layer.
        let a = select_layer_row(&mut app, second, &order, Some(a), false, true);
        assert_eq!(a, second);
        assert_eq!(app.selected_layers, vec![first, second]);
        assert_eq!(app.picked_layer, Some(second));
        // Ctrl+click again takes it out; the pick moves to the last left.
        let a = select_layer_row(&mut app, second, &order, Some(a), false, true);
        assert_eq!(a, second, "the anchor if there was one");
        assert_eq!(app.selected_layers, vec![first]);
        assert_eq!(app.picked_layer, Some(first));
        // Taking the picked one out leaves no pick.
        select_layer_row(&mut app, first, &order, Some(a), false, true);
        assert!(app.selected_layers.is_empty());
        assert_eq!(app.picked_layer, None);
        // Ctrl+click with nothing picked seeds from the active layer, the
        // page's topmost one, so it is in the selection already.
        select_layer_row(&mut app, first, &order, None, false, true);
        assert_eq!(app.selected_layers, vec![second, first]);
        assert_eq!(app.picked_layer, Some(first));
        // Shift+click from the top page layer to the master layer takes
        // every layer row between, across the page and the Master Page.
        let a = select_layer_row(&mut app, second, &order, None, false, false);
        select_layer_row(&mut app, master, &order, Some(a), true, false);
        assert_eq!(app.selected_layers, vec![second, first, master]);
        assert_eq!(app.picked_layer, Some(master));
        // Shift to a layer in the middle, still counting from the first
        // row clicked.
        select_layer_row(&mut app, first, &order, Some(a), true, false);
        assert_eq!(app.selected_layers, vec![second, first]);
        // Ctrl+Shift adds the run to what was picked already.
        select_layer_row(&mut app, first, &order, None, false, false);
        select_layer_row(&mut app, master, &order, None, false, true);
        assert_eq!(app.selected_layers, vec![first, master]);
        select_layer_row(&mut app, second, &order, Some(master), true, true);
        assert_eq!(app.selected_layers, vec![first, master, second]);
        // A layer out of the list falls back to a plain click.
        let other = {
            app.add_page();
            app.doc().pages[1].layers[0].id
        };
        let a = select_layer_row(&mut app, other, &order, Some(master), true, false);
        assert_eq!(app.selected_layers, vec![other]);
        assert_eq!(a, other);
        // Picking an object on the page lets the layers go.
        app.page = app.doc().pages[0].id;
        app.selected_layers = vec![first, second];
        let id = app.new_shape(rect_kind()).expect("shape");
        app.select(vec![id]);
        assert!(app.selected_layers.is_empty(), "objects let the layers go");
        assert_eq!(app.selection, vec![id]);
    }

    /// #3966: with several layers picked, the eye, the printer, the
    /// padlock, Select the Layer's Objects and Delete act on all of them
    /// in one undo step; a page always keeps one layer.
    #[test]
    fn selected_layers_batch_visibility_lock_print_select_and_delete() {
        let mut app = without_desktop();
        let first = app.doc().pages[0].layers[0].id;
        app.picked_layer = Some(first);
        let on_first = app.new_shape(rect_kind()).expect("shape");
        new_layer(&mut app);
        let second = app.picked_layer.expect("second");
        let on_second = app.new_shape(rect_kind()).expect("shape");
        new_layer(&mut app);
        let third = app.picked_layer.expect("third");
        new_master_layer(&mut app, MasterScope::All);
        let master = app.picked_layer.expect("master");
        let on_master = app.new_shape(rect_kind()).expect("shape");
        let run = |app: &mut App, act: Actions| {
            let mut once = Some(act);
            with_ui(|ui| {
                if let Some(act) = once.take() {
                    apply(app, ui, act);
                }
            });
        };
        // The eye of one of them hides or shows them all.
        app.selected_layers = vec![first, second, master];
        app.picked_layer = Some(master);
        run(
            &mut app,
            Actions {
                visible: Some((second, false)),
                ..Default::default()
            },
        );
        assert!(!app.doc().layer(first).expect("first").visible);
        assert!(!app.doc().layer(second).expect("second").visible);
        assert!(app.doc().layer(third).expect("third").visible);
        assert!(!app.doc().layer(master).expect("master").visible);
        app.undo();
        assert!(app.doc().layer(first).expect("first").visible);
        assert!(app.doc().layer(second).expect("second").visible);
        assert!(app.doc().layer(master).expect("master").visible);
        // The printer, one undo step.
        run(
            &mut app,
            Actions {
                printable: Some((second, false)),
                ..Default::default()
            },
        );
        assert!(!app.doc().layer(first).expect("first").printable);
        assert!(!app.doc().layer(second).expect("second").printable);
        assert!(app.doc().layer(third).expect("third").printable);
        assert!(!app.doc().layer(master).expect("master").printable);
        app.undo();
        assert!(app.doc().layer(first).expect("first").printable);
        assert!(app.doc().layer(master).expect("master").printable);
        // The padlock, one undo step.
        run(
            &mut app,
            Actions {
                locked: Some((second, true)),
                ..Default::default()
            },
        );
        assert!(app.doc().layer(first).expect("first").locked);
        assert!(app.doc().layer(second).expect("second").locked);
        assert!(!app.doc().layer(third).expect("third").locked);
        assert!(app.doc().layer(master).expect("master").locked);
        app.undo();
        assert!(!app.doc().layer(first).expect("first").locked);
        assert!(!app.doc().layer(master).expect("master").locked);
        // Select the Layer's Objects takes them from every picked layer.
        run(
            &mut app,
            Actions {
                select_layer: Some(second),
                ..Default::default()
            },
        );
        let mut want = vec![on_first, on_second, on_master];
        want.sort();
        let mut got = app.selection.clone();
        got.sort();
        assert_eq!(got, want);
        assert_eq!(app.picked_layer, Some(second));
        assert!(app.selected_layers.is_empty(), "objects let the layers go");
        // Delete two of the three page layers and the master layer in one
        // undo step; the page keeps its last layer.
        app.select(Vec::new());
        app.selected_layers = vec![first, second, master];
        app.picked_layer = Some(second);
        assert!(deletable_layer(&app).is_some());
        delete_current(&mut app);
        assert_eq!(app.doc().pages[0].layers.len(), 1, "one layer left");
        assert_eq!(app.doc().pages[0].layers[0].id, third);
        assert!(app.doc().master.is_empty());
        assert!(app.selected_layers.is_empty());
        app.undo();
        assert_eq!(app.doc().pages[0].layers.len(), 3);
        assert!(app.doc().master.iter().any(|l| l.id == master));
        // The layer menu's Delete takes the same set.
        app.selected_layers = vec![first, master];
        app.picked_layer = Some(first);
        run(
            &mut app,
            Actions {
                delete_layer: Some(first),
                ..Default::default()
            },
        );
        assert_eq!(app.doc().pages[0].layers.len(), 2);
        assert!(app.doc().master.is_empty());
        app.undo();
        assert_eq!(app.doc().pages[0].layers.len(), 3);
        assert!(app.doc().master.iter().any(|l| l.id == master));
        // All three page layers picked: only two may go, the page keeps
        // one.
        app.selected_layers = vec![first, second, third];
        app.picked_layer = Some(first);
        assert_eq!(
            deletable_layers_from(app.doc(), &[first, second, third]),
            vec![first, second]
        );
        delete_current(&mut app);
        assert_eq!(app.doc().pages[0].layers.len(), 1);
        assert_eq!(app.doc().pages[0].layers[0].id, third);
        app.undo();
        assert_eq!(app.doc().pages[0].layers.len(), 3);
    }
}

#[cfg(test)]
mod drag_tests {
    use super::*;

    /// #3575: in the docker a guideline row's click selects it alone,
    /// Ctrl+click adds it or takes it away, Shift+click selects the run
    /// from the last one clicked, skipping hidden ones.
    #[test]
    fn guideline_rows_take_ctrl_and_shift_clicks() {
        let mut app = App::headless();
        for x in [10.0, 20.0, 30.0, 40.0, 50.0] {
            app.add_guide(tracedraw_core::document::Guide::vertical(x));
        }
        app.set_guide_hidden(3, true);
        select_guide_row(&mut app, 1, false, false);
        assert_eq!(app.selected_guides, vec![1]);
        select_guide_row(&mut app, 4, false, true);
        assert_eq!(app.selected_guides, vec![1, 2, 4], "the hidden one left out");
        select_guide_row(&mut app, 2, true, false);
        assert_eq!(app.selected_guides, vec![1, 4]);
        select_guide_row(&mut app, 0, true, false);
        assert_eq!(app.selected_guides, vec![1, 4, 0]);
        select_guide_row(&mut app, 3, true, false);
        assert_eq!(app.selected_guides, vec![1, 4, 0], "a hidden one is not picked");
        select_guide_row(&mut app, 2, false, false);
        assert_eq!(app.selected_guides, vec![2]);
    }

    fn rect_kind() -> ShapeKind {
        ShapeKind::Rect {
            rect: tracedraw_core::Rect::new(0.0, 0.0, 10.0, 10.0),
            radius: 0.0,
            corners: None,
        }
    }

    fn frame(ctx: &egui::Context, app: &mut App, events: Vec<egui::Event>) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400.0, 800.0),
            )),
            events,
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| objects_docker(app, ui));
        out.textures_delta.clear();
    }

    fn object_row(ctx: &egui::Context, id: ShapeId) -> Rect {
        ctx.data(|d| d.get_temp::<Vec<(ShapeId, Rect)>>(egui::Id::new("objects_rows_drawn")))
            .and_then(|rows| rows.into_iter().find(|(s, _)| *s == id))
            .map(|(_, r)| r)
            .expect("the object's row")
    }

    fn layer_row(ctx: &egui::Context, id: LayerId) -> Rect {
        ctx.data(|d| d.get_temp::<Vec<(LayerId, Rect)>>(egui::Id::new("objects_layer_rows_drawn")))
            .and_then(|rows| rows.into_iter().find(|(l, _)| *l == id))
            .map(|(_, r)| r)
            .expect("the layer's row")
    }

    /// Press at `from`, drag to `to` over a few frames and let go there.
    fn drag(ctx: &egui::Context, app: &mut App, from: Pos2, to: Pos2) {
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(ctx, app, vec![egui::Event::PointerMoved(from)]);
        frame(ctx, app, vec![button(from, true)]);
        let mid = from + (to - from) * 0.5;
        frame(ctx, app, vec![egui::Event::PointerMoved(mid)]);
        frame(ctx, app, vec![egui::Event::PointerMoved(to)]);
        frame(ctx, app, vec![button(to, false)]);
        frame(ctx, app, Vec::new());
    }

    fn order(app: &App, layer: usize) -> Vec<ShapeId> {
        app.doc().pages[0].layers[layer]
            .shapes
            .iter()
            .map(|s| s.id)
            .collect()
    }

    /// Live test of 0.41: an object's row dragged along the Objects
    /// docker and let go on another object's row puts it in front of
    /// that object (the row's upper half) or behind it (its lower half),
    /// one undo step; a selected row carries the selection along.
    #[test]
    fn a_row_dragged_along_the_list_changes_the_stacking_order() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        let a = app.new_shape(rect_kind()).expect("a");
        let b = app.new_shape(rect_kind()).expect("b");
        let c = app.new_shape(rect_kind()).expect("c");
        app.select(Vec::new());
        assert_eq!(order(&app, 0), [a, b, c]);
        frame(&ctx, &mut app, Vec::new());
        // The list shows the front object first: c, b, a.
        let (ra, rc) = (object_row(&ctx, a), object_row(&ctx, c));
        assert!(rc.top() < ra.top());
        // a onto the upper half of c's row: in front of c.
        let to = Pos2::new(rc.center().x, rc.top() + 2.0);
        drag(&ctx, &mut app, ra.center(), to);
        assert_eq!(order(&app, 0), [b, c, a]);
        assert_eq!(app.selection, vec![a]);
        app.undo();
        assert_eq!(order(&app, 0), [a, b, c], "one undo step");
        // c onto the lower half of a's row: behind a.
        frame(&ctx, &mut app, Vec::new());
        let (ra, rc) = (object_row(&ctx, a), object_row(&ctx, c));
        let to = Pos2::new(ra.center().x, ra.bottom() - 2.0);
        drag(&ctx, &mut app, rc.center(), to);
        assert_eq!(order(&app, 0), [c, a, b]);
        app.undo();
        // a and c selected, a's row dragged onto b's upper half: both go
        // in front of b, keeping their order.
        app.select(vec![a, c]);
        frame(&ctx, &mut app, Vec::new());
        let (ra, rb) = (object_row(&ctx, a), object_row(&ctx, b));
        let to = Pos2::new(rb.center().x, rb.top() + 2.0);
        drag(&ctx, &mut app, ra.center(), to);
        assert_eq!(order(&app, 0), [b, a, c]);
        app.undo();
        assert_eq!(order(&app, 0), [a, b, c]);
        // Let go on its own row, or below every row: nothing changes.
        app.select(Vec::new());
        frame(&ctx, &mut app, Vec::new());
        let ra = object_row(&ctx, a);
        drag(
            &ctx,
            &mut app,
            ra.center(),
            ra.center() + Vec2::new(30.0, 0.0),
        );
        drag(&ctx, &mut app, ra.center(), Pos2::new(200.0, 700.0));
        assert_eq!(order(&app, 0), [a, b, c]);
        // A locked object's row does not move.
        app.run(Command::SetLocked {
            shapes: vec![a],
            locked: true,
        });
        frame(&ctx, &mut app, Vec::new());
        let (ra, rc) = (object_row(&ctx, a), object_row(&ctx, c));
        let to = Pos2::new(rc.center().x, rc.top() + 2.0);
        drag(&ctx, &mut app, ra.center(), to);
        assert_eq!(order(&app, 0), [a, b, c]);
    }

    /// One frame of a finger at `pos` (`phase`) at time `t`, as a touch
    /// screen sends it: the touch and the pointer it moves.
    fn finger(ctx: &egui::Context, app: &mut App, t: f64, pos: Pos2, phase: egui::TouchPhase) {
        let mut events = vec![egui::Event::Touch {
            device_id: egui::TouchDeviceId(1),
            id: egui::TouchId(1),
            phase,
            pos,
            force: None,
        }];
        let button = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        match phase {
            egui::TouchPhase::Start => {
                events.push(egui::Event::PointerMoved(pos));
                events.push(button(true));
            }
            egui::TouchPhase::Move => events.push(egui::Event::PointerMoved(pos)),
            _ => {
                events.push(button(false));
                events.push(egui::Event::PointerGone);
            }
        }
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400.0, 800.0),
            )),
            time: Some(t),
            events,
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| objects_docker(app, ui));
        out.textures_delta.clear();
    }

    /// On a touch screen a finger held on an object's row and then moved
    /// drags the row, as the mouse does, and changes the stacking order;
    /// a finger held still opens the object's full menu, as a right
    /// click does (#2948).
    #[test]
    fn a_finger_held_on_a_row_drags_it_or_opens_its_menu() {
        use egui::TouchPhase::{End, Move, Start};
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        let a = app.new_shape(rect_kind()).expect("a");
        let b = app.new_shape(rect_kind()).expect("b");
        let c = app.new_shape(rect_kind()).expect("c");
        app.select(Vec::new());
        frame(&ctx, &mut app, Vec::new());
        let (ra, rc) = (object_row(&ctx, a), object_row(&ctx, c));
        // Held a second on a's row, then carried onto the upper half of c's.
        let mut t = 10.0;
        finger(&ctx, &mut app, t, ra.center(), Start);
        for _ in 0..6 {
            t += 0.2;
            finger(&ctx, &mut app, t, ra.center(), Move);
        }
        let to = Pos2::new(rc.center().x, rc.top() + 2.0);
        for k in 1..=4 {
            t += 0.05;
            let p = ra.center() + (to - ra.center()) * (k as f32 / 4.0);
            finger(&ctx, &mut app, t, p, Move);
        }
        t += 0.05;
        finger(&ctx, &mut app, t, to, End);
        frame(&ctx, &mut app, Vec::new());
        assert_eq!(order(&app, 0), [b, c, a], "held and dragged");
        assert!(app.context_menu.is_none(), "a drag opens no menu");
        app.undo();
        // Held still: the object's own menu, as a right click opens it.
        frame(&ctx, &mut app, Vec::new());
        let rb = object_row(&ctx, b);
        t += 1.0;
        finger(&ctx, &mut app, t, rb.center(), Start);
        for _ in 0..6 {
            t += 0.2;
            finger(&ctx, &mut app, t, rb.center(), Move);
        }
        t += 0.05;
        finger(&ctx, &mut app, t, rb.center(), End);
        frame(&ctx, &mut app, Vec::new());
        assert_eq!(app.context_docker, Some(b), "the row's full menu");
        assert!(app.context_menu.is_some());
        assert_eq!(order(&app, 0), [a, b, c]);
    }

    /// A row let go on an object of another layer goes over to that layer
    /// beside the object, and let go on a layer's row on the top of that
    /// layer: one undo step each. A locked layer takes nothing.
    #[test]
    fn a_row_dragged_onto_another_layer_moves_the_object_there() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        let low = app.doc().pages[0].layers[0].id;
        let a = app.new_shape(rect_kind()).expect("a");
        let b = app.new_shape(rect_kind()).expect("b");
        let page = app.page;
        app.run(Command::AddLayer {
            page,
            name: "Top".into(),
        });
        let high = app.doc().pages[0].layers[1].id;
        app.picked_layer = Some(high);
        let c = app.new_shape(rect_kind()).expect("c");
        let d = app.new_shape(rect_kind()).expect("d");
        app.select(Vec::new());
        assert_eq!(order(&app, 0), [a, b]);
        assert_eq!(order(&app, 1), [c, d]);
        frame(&ctx, &mut app, Vec::new());
        // b onto the lower half of d's row: behind d, in the top layer.
        let (rb, rd) = (object_row(&ctx, b), object_row(&ctx, d));
        let to = Pos2::new(rd.center().x, rd.bottom() - 2.0);
        drag(&ctx, &mut app, rb.center(), to);
        assert_eq!(order(&app, 0), [a]);
        assert_eq!(order(&app, 1), [c, b, d]);
        assert_eq!(app.active_layer(), Some(high));
        app.undo();
        assert_eq!(order(&app, 0), [a, b], "one undo step");
        assert_eq!(order(&app, 1), [c, d]);
        // c onto the lower layer's row: on the top of that layer.
        frame(&ctx, &mut app, Vec::new());
        let (rc, rl) = (object_row(&ctx, c), layer_row(&ctx, low));
        drag(&ctx, &mut app, rc.center(), rl.center());
        assert_eq!(order(&app, 0), [a, b, c]);
        assert_eq!(order(&app, 1), [d]);
        app.undo();
        assert_eq!(order(&app, 1), [c, d]);
        // A locked layer takes nothing, nor does an object dropped on
        // itself.
        app.run(Command::SetLayerLocked {
            layer: low,
            locked: true,
        });
        let ids = [d];
        let on = |reference| RowDrop::Object {
            reference,
            in_front: true,
        };
        assert!(drop_commands(&app, &ids, RowDrop::Layer(low)).is_empty());
        assert!(drop_commands(&app, &ids, on(a)).is_empty());
        assert!(drop_commands(&app, &ids, on(d)).is_empty());
        assert_eq!(drop_commands(&app, &ids, on(c)).len(), 1);
    }

    /// A right click on an object's row opens the drawing's own object
    /// menu for it: the object is selected first (the whole selection
    /// stays when it is part of it), a locked object gets the locked-object
    /// menu, and the menu is the object menu whatever the tool (#2601).
    #[test]
    fn a_right_click_on_an_object_row_opens_the_drawings_object_menu() {
        let mut app = App::headless();
        let a = app.new_shape(rect_kind()).expect("a");
        let b = app.new_shape(rect_kind()).expect("b");
        let pos = Pos2::new(40.0, 50.0);
        app.select(vec![a]);
        open_row_menu(&mut app, b, pos);
        assert_eq!(app.selection, vec![b]);
        assert_eq!(app.context_docker, Some(b));
        assert_eq!(app.context_menu.map(|(p, _)| p), Some(pos));
        assert_eq!(app.context_locked, None);
        app.select(vec![a, b]);
        open_row_menu(&mut app, a, pos);
        assert_eq!(app.selection, vec![a, b]);
        app.set_locked(true);
        app.select(Vec::new());
        open_row_menu(&mut app, a, pos);
        assert_eq!(app.context_locked, Some(a));
        assert!(app.selection.is_empty());
    }

    /// The row's menu holds the docker's Rename and Move to Layer and the
    /// drawing's object commands, also with the Shape tool on; Move to
    /// Layer moves the object (#2601).
    #[test]
    fn an_object_rows_menu_is_the_full_object_menu_with_rename_and_move() {
        let ctx = egui::Context::default();
        let fresh = || {
            let mut app = App::headless();
            let a = app.new_shape(rect_kind()).expect("a");
            new_layer(&mut app);
            app.tool = crate::tools::Tool::Shape;
            open_row_menu(&mut app, a, Pos2::new(40.0, 50.0));
            (app, a)
        };
        let frame = |app: &mut App, target: usize| {
            crate::ui::menus::replay::start(target);
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1400.0, 900.0))),
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                let mut close = false;
                crate::ui::context::menu_items(app, ui, &mut close);
            });
            out.textures_delta.clear();
            crate::ui::menus::replay::finish()
        };
        let (mut app, _) = fresh();
        let (_, rows) = frame(&mut app, usize::MAX);
        let mut keys = Vec::new();
        let mut moved = false;
        for t in 0..rows {
            let (mut app, a) = fresh();
            let before = layer_holding(&app, a);
            let (hit, _) = frame(&mut app, t);
            let Some(key) = hit else { continue };
            if layer_holding(&app, a) != before {
                moved = true;
            }
            if key == "docker.rename" {
                assert!(matches!(
                    app.dialog,
                    crate::ui::dialogs::Dialog::RenameObject { shape, .. } if shape == a
                ));
            }
            keys.push(key);
        }
        for want in [
            "docker.rename",
            "menu.edit.duplicate",
            "menu.object.to_front_of_page",
            "menu.object.hide_object",
            "menu.object.properties",
        ] {
            assert!(
                keys.iter().any(|k| k == want),
                "{want} missing from {keys:?}"
            );
        }
        assert!(moved, "Move to Layer moved nothing: {keys:?}");
    }

    fn children(app: &App, group: ShapeId) -> Vec<ShapeId> {
        match app.doc().find_shape(group).map(|s| &s.kind) {
            Some(ShapeKind::Group { children }) => children.iter().map(|s| s.id).collect(),
            _ => Vec::new(),
        }
    }

    /// #2672: a row let go on the middle of a group's row goes into the
    /// group, on top; a group's object dragged along its rows changes
    /// places inside it; dragged onto an object of the layer it comes out
    /// of the group. One undo step each.
    #[test]
    fn rows_go_into_groups_and_change_places_inside_them() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        let a = app.new_shape(rect_kind()).expect("a");
        let b = app.new_shape(rect_kind()).expect("b");
        let c = app.new_shape(rect_kind()).expect("c");
        app.run(Command::Group { shapes: vec![b, c] });
        let g = app.doc().pages[0].layers[0].shapes[1].id;
        app.select(Vec::new());
        assert_eq!(order(&app, 0), [a, g]);
        assert_eq!(children(&app, g), [b, c]);
        // a onto the middle of the group's row: into the group, on top.
        frame(&ctx, &mut app, Vec::new());
        let (ra, rg) = (object_row(&ctx, a), object_row(&ctx, g));
        drag(&ctx, &mut app, ra.center(), rg.center());
        assert_eq!(order(&app, 0), [g]);
        assert_eq!(children(&app, g), [b, c, a]);
        assert_eq!(app.selection, vec![a]);
        app.undo();
        assert_eq!(order(&app, 0), [a, g], "one undo step");
        // The top quarter of the group's row: in front of the group.
        frame(&ctx, &mut app, Vec::new());
        let (ra, rg) = (object_row(&ctx, a), object_row(&ctx, g));
        drag(
            &ctx,
            &mut app,
            ra.center(),
            Pos2::new(rg.center().x, rg.top() + 2.0),
        );
        assert_eq!(order(&app, 0), [g, a]);
        app.undo();
        // The group opened, b's row dragged onto the upper half of c's:
        // b goes in front of c inside the group.
        app.objects_open_groups.insert(g);
        app.select(Vec::new());
        frame(&ctx, &mut app, Vec::new());
        let (rb, rc) = (object_row(&ctx, b), object_row(&ctx, c));
        drag(
            &ctx,
            &mut app,
            rb.center(),
            Pos2::new(rc.center().x, rc.top() + 2.0),
        );
        assert_eq!(children(&app, g), [c, b]);
        assert_eq!(order(&app, 0), [a, g]);
        // c out of the group, behind a.
        frame(&ctx, &mut app, Vec::new());
        let (rc, ra) = (object_row(&ctx, c), object_row(&ctx, a));
        drag(
            &ctx,
            &mut app,
            rc.center(),
            Pos2::new(ra.center().x, ra.bottom() - 2.0),
        );
        assert_eq!(order(&app, 0), [c, a, g]);
        assert_eq!(children(&app, g), [b]);
        // The group onto its own object's row: nothing changes.
        frame(&ctx, &mut app, Vec::new());
        let (rg, rb) = (object_row(&ctx, g), object_row(&ctx, b));
        drag(
            &ctx,
            &mut app,
            rg.center(),
            Pos2::new(rb.center().x, rb.top() + 2.0),
        );
        assert_eq!(order(&app, 0), [c, a, g]);
        assert_eq!(children(&app, g), [b]);
    }

    /// #3846: while a PowerClip's contents are edited (after a click on a
    /// content's row) they stay listed inside the frame's row, and a
    /// content's row dragged onto the layer's row takes it out of the
    /// PowerClip for good; the others go back in when the edit ends.
    #[test]
    fn a_content_row_dragged_out_of_a_powerclip_frees_it() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        let a = app.new_shape(rect_kind()).expect("a");
        let b = app.new_shape(rect_kind()).expect("b");
        let c = app.new_shape(rect_kind()).expect("c");
        clip_rows(&mut app, &[b, c], a);
        app.select(vec![a]);
        app.edit_clip_frame();
        assert!(app.clip_frame_edit.is_some());
        frame(&ctx, &mut app, Vec::new());
        let (ra, rb, rc) = (object_row(&ctx, a), object_row(&ctx, b), object_row(&ctx, c));
        // Listed under the frame, one step in.
        assert!(rb.top() >= ra.bottom() - 0.5 && rc.top() >= ra.bottom() - 0.5, "{ra:?} {rb:?} {rc:?}");
        let layer = app.active_layer().expect("layer");
        let rl = layer_row(&ctx, layer);
        app.select(vec![c]);
        drag(&ctx, &mut app, rc.center(), rl.center());
        app.finish_clip_frame_edit();
        let contents = match &app.doc().find_shape(a).expect("a").kind {
            ShapeKind::ClipFrame { contents, .. } => contents.iter().map(|s| s.id).collect::<Vec<_>>(),
            other => panic!("{other:?}"),
        };
        assert_eq!(contents, [b]);
        assert!(app.doc().shape(c).is_ok(), "c on the layer, free");
    }

    /// A clip frame inside another (#4110): a click on a content of the
    /// inner one edits both, the inner inside the outer; finishing the
    /// inner one goes back to editing the outer one, and finishing that
    /// leaves both clip frames whole, nothing loose on the page.
    #[test]
    fn a_clip_frame_inside_another_is_edited_inside_it() {
        let mut app = App::headless();
        let content = app.new_shape(rect_kind()).expect("content");
        let inner = app.new_shape(rect_kind()).expect("inner");
        let outer = app.new_shape(rect_kind()).expect("outer");
        clip_rows(&mut app, &[content], inner);
        clip_rows(&mut app, &[inner], outer);
        let layer_count = |app: &App| app.page_layers().iter().map(|l| l.shapes.len()).sum::<usize>();
        assert_eq!(layer_count(&app), 1, "one clip frame on the page");
        assert!(select_clip_content(&mut app, content));
        assert_eq!(app.clip_frame_edit.as_ref().map(|(f, _)| *f), Some(inner));
        assert_eq!(app.clip_edit_outer.len(), 1, "the outer one waits");
        assert_eq!(app.selection, vec![content]);
        app.finish_clip_frame_edit();
        assert_eq!(app.clip_frame_edit.as_ref().map(|(f, _)| *f), Some(outer), "back in the outer one");
        assert_eq!(app.selection, vec![inner]);
        app.finish_clip_frame_edit();
        assert!(app.clip_frame_edit.is_none());
        assert_eq!(layer_count(&app), 1, "nothing left loose");
        let ShapeKind::ClipFrame { contents, .. } = &app.doc().find_shape(outer).expect("outer").kind else {
            panic!("the outer clip frame");
        };
        assert_eq!(contents.len(), 1);
        let ShapeKind::ClipFrame { contents, .. } = &contents[0].kind else {
            panic!("the inner clip frame inside it");
        };
        assert_eq!(contents[0].id, content);
    }

    /// An envelope on a clip frame stays when one of its contents is
    /// clicked in the list to be edited: the contents come out, and when
    /// the edit ends the clip frame has its envelope again, bending it all
    /// as before (#4070: it was gone).
    #[test]
    fn a_clip_frames_envelope_stays_through_editing_its_contents() {
        use tracedraw_core::live::Effect;
        let mut app = App::headless();
        let a = app.new_shape(rect_kind()).expect("a");
        let b = app.new_shape(rect_kind()).expect("b");
        clip_rows(&mut app, &[b], a);
        let clip = app.doc().find_shape(a).expect("clip").clone();
        let mut env = Effect::envelope_around(clip.bounds(), false);
        if let Effect::Envelope { nodes, .. } = &mut env {
            nodes[0].x -= 5.0;
            nodes[0].y -= 5.0;
        }
        app.run(Command::SetEffects {
            shape: a,
            effects: vec![env.clone()],
        });
        let drawn = app.doc().find_shape(a).expect("clip").visual_bounds();
        assert!(select_clip_content(&mut app, b));
        assert!(app.clip_frame_edit.is_some());
        app.finish_clip_frame_edit();
        let back = app.doc().find_shape(a).expect("clip");
        assert!(matches!(back.kind, ShapeKind::ClipFrame { .. }));
        assert_eq!(back.effects, vec![env]);
        let now = back.visual_bounds();
        for (x, y) in [(now.x0, drawn.x0), (now.y0, drawn.y0), (now.x1, drawn.x1), (now.y1, drawn.y1)] {
            assert!((x - y).abs() < 1e-6, "{now:?} {drawn:?}");
        }
    }

    /// #4061: a content's row dragged onto the layer's row frees it from
    /// the PowerClip with no click on it first; the other contents stay
    /// inside.
    #[test]
    fn a_content_row_drags_out_of_a_powerclip_without_a_click_first() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        let a = app.new_shape(rect_kind()).expect("a");
        let b = app.new_shape(rect_kind()).expect("b");
        let c = app.new_shape(rect_kind()).expect("c");
        clip_rows(&mut app, &[b, c], a);
        app.select(Vec::new());
        frame(&ctx, &mut app, Vec::new());
        frame(&ctx, &mut app, Vec::new());
        assert!(app.clip_frame_edit.is_none());
        let rc = object_row(&ctx, c);
        let layer = app.active_layer().expect("layer");
        let rl = layer_row(&ctx, layer);
        drag(&ctx, &mut app, rc.center(), rl.center());
        if app.clip_frame_edit.is_some() {
            app.finish_clip_frame_edit();
        }
        let contents = match &app.doc().find_shape(a).expect("a").kind {
            ShapeKind::ClipFrame { contents, .. } => contents.iter().map(|s| s.id).collect::<Vec<_>>(),
            other => panic!("{other:?}"),
        };
        assert_eq!(contents, [b]);
        assert!(app.doc().shape(c).is_ok(), "c on the layer, free");
    }

    /// PowerClips in the list (#3053): a row let go in the middle of
    /// another object's row puts the object inside it as a PowerClip, one
    /// undo step; the PowerClip's row opens to its contents, and a click
    /// on a content's row takes the contents out for editing with that
    /// object selected. A group's middle still takes objects into the
    /// group, and a locked object takes none.
    #[test]
    fn a_row_dropped_in_the_middle_of_another_powerclips_it() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        let a = app.new_shape(rect_kind()).expect("a");
        let b = app.new_shape(rect_kind()).expect("b");
        let c = app.new_shape(rect_kind()).expect("c");
        app.select(Vec::new());
        frame(&ctx, &mut app, Vec::new());
        let (ra, rc) = (object_row(&ctx, a), object_row(&ctx, c));
        // c into the middle of a's row: a becomes the frame.
        drag(&ctx, &mut app, rc.center(), ra.center());
        let frame_kind = |app: &App| app.doc().find_shape(a).map(|s| s.kind.clone());
        let Some(ShapeKind::ClipFrame { contents, .. }) = frame_kind(&app) else {
            panic!("a PowerClip: {:?}", frame_kind(&app));
        };
        assert_eq!(contents.iter().map(|s| s.id).collect::<Vec<_>>(), [c]);
        assert_eq!(order(&app, 0), [a, b]);
        assert_eq!(app.selection, vec![a]);
        // Its row opens to its contents, one step in.
        frame(&ctx, &mut app, Vec::new());
        let (ra, rcin) = (object_row(&ctx, a), object_row(&ctx, c));
        assert!(rcin.top() >= ra.bottom() - 0.5, "{ra:?} {rcin:?}");
        // A click on the content's row: Edit Contents, the object chosen.
        let click = |pos| {
            vec![
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        };
        frame(&ctx, &mut app, vec![egui::Event::PointerMoved(rcin.center())]);
        frame(&ctx, &mut app, click(rcin.center()));
        assert!(app.clip_frame_edit.is_some(), "editing the contents");
        assert_eq!(app.selection, vec![c]);
        app.finish_clip_frame_edit();
        // One undo step undid the PowerClip itself.
        let mut app2 = App::headless();
        let x = app2.new_shape(rect_kind()).expect("x");
        let y = app2.new_shape(rect_kind()).expect("y");
        clip_rows(&mut app2, &[y], x);
        assert!(matches!(app2.doc().find_shape(x).unwrap().kind, ShapeKind::ClipFrame { .. }));
        app2.undo();
        assert!(matches!(app2.doc().find_shape(x).unwrap().kind, ShapeKind::Rect { .. }));
        assert_eq!(order(&app2, 0), [x, y]);
        // A locked object and a group take nothing as a PowerClip.
        app2.run(Command::SetLocked { shapes: vec![x], locked: true });
        assert!(!can_clip_into(&app2, &[y], x));
        assert!(!can_clip_into(&app2, &[y], y), "not into itself");
    }

    /// #3935: an object of the facing page selected with the current
    /// page's is brought onto the current page while selected (#3488),
    /// but with every page listed the docker keeps it under its own page
    /// and layer, not under the current page as if it had moved there.
    #[test]
    fn objects_selected_across_the_spine_stay_listed_on_their_page() {
        let mut app = App::headless();
        app.show_welcome = false;
        for _ in 0..3 {
            app.add_page();
        }
        app.set_facing_pages(true, tracedraw_core::spreads::PageSide::Right);
        let (p2, p3) = (app.doc().pages[1].id, app.doc().pages[2].id);
        app.page = p3;
        let b = app.new_shape(rect_kind()).expect("b");
        let home = app.doc().page(p3).expect("p3").layers[0].id;
        app.page = p2;
        let a = app.new_shape(rect_kind()).expect("a");
        let host = app.doc().page(p2).expect("p2").layers[0].id;
        assert_eq!(app.adopt_spread_objects(&[b]), vec![b]);
        app.select(vec![a, b]);
        app.settings.objects_all_pages = true;
        assert_eq!(spread_borrowed(&app, Some(p2), host).0, vec![b]);
        assert_eq!(spread_borrowed(&app, Some(p3), home).1, vec![b]);
        assert!(spread_borrowed(&app, Some(p3), home).0.is_empty());
        // The current page alone listed: it shows there, being selected.
        app.settings.objects_all_pages = false;
        assert!(spread_borrowed(&app, Some(p2), host).0.is_empty());
    }
}

#[cfg(test)]
mod range_layer_tests {
    use super::*;

    fn book(count: usize) -> App {
        let mut app = App::headless();
        while app.doc().pages.len() < count {
            app.add_page();
        }
        app.goto_page(0);
        app
    }

    fn draft(range: &str) -> NewRangeLayer {
        NewRangeLayer {
            name: "Chapter header".into(),
            page_range: range.into(),
        }
    }

    fn dialog_frame(ctx: &egui::Context, app: &mut App, key: egui::Key) {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 800.0))),
            events: vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| crate::ui::dialogs::show(app, ui.ctx()));
        out.textures_delta.clear();
    }

    #[test]
    fn range_row_selection_excludes_hidden_shared_objects_after_navigation() {
        let mut app = book(2);
        let rectangle = || ShapeKind::Rect {
            rect: tracedraw_core::Rect::new(0.0, 0.0, 10.0, 10.0),
            radius: 0.0,
            corners: None,
        };
        let ordinary = app.new_shape(rectangle()).expect("page object");
        new_master_layer(&mut app, MasterScope::All);
        let shared = app.new_shape(rectangle()).expect("shared object");
        assert!(create_range_layer(&mut app, &draft("1")));
        let first = app.doc().master.last().expect("first range").id;
        let child = Shape::new(app.engine.new_shape_id(), rectangle());
        let hidden_child = child.id;
        let hidden_group = app
            .new_shape(ShapeKind::Group {
                children: vec![child],
            })
            .expect("first range group");
        assert!(create_range_layer(&mut app, &draft("2")));
        let second = app.doc().master.last().expect("second range").id;
        let target = app.new_shape(rectangle()).expect("second range object");
        let rows = [ordinary, shared, target, hidden_group, hidden_child];

        assert!(activate_range_layer(&mut app, first));
        let anchor = select_row(&mut app, hidden_child, &rows, None, false, false);
        assert!(activate_range_layer(&mut app, second));
        assert_eq!(app.page_index(), 1);
        select_row(&mut app, target, &rows, Some(anchor), true, false);
        assert_eq!(app.selection, vec![target]);

        // Ctrl+Shift keeps ordinary cross-page and unrestricted shared objects.
        app.select(vec![ordinary, shared, hidden_child]);
        select_row(&mut app, target, &rows, Some(anchor), true, true);
        assert_eq!(app.selection, vec![ordinary, shared, target]);
        select_row(&mut app, target, &rows, Some(anchor), false, true);
        assert_eq!(app.selection, vec![ordinary, shared]);

        select_row(&mut app, target, &rows, Some(ordinary), true, false);
        assert_eq!(app.selection, vec![ordinary, shared, target]);
    }

    #[test]
    fn range_layer_selection_finishes_source_text_before_navigation() {
        for (source_range, target_range) in [
            (None, "1"),
            (Some("1-2"), "1-2"),
            (Some("1"), "2"),
            (Some("1"), "10"),
        ] {
            let mut app = book(2);
            assert!(create_range_layer(&mut app, &draft(target_range)));
            let target = app.doc().master.last().expect("target range").id;
            if let Some(source) = source_range {
                assert!(create_range_layer(&mut app, &draft(source)));
            } else {
                app.goto_page(0);
                app.picked_layer = Some(app.doc().pages[0].layers[0].id);
            }
            let source = app.active_layer().expect("source layer");
            app.start_text(tracedraw_core::Point::new(20.0, 20.0), None);
            app.text_insert("Header");
            let text = app.text_edit.as_ref().expect("editing header").shape;
            app.ime_preedit = "pending composition".into();

            assert_eq!(activate_range_layer(&mut app, target), target_range != "10");
            assert!(
                app.text_edit.is_none(),
                "{source_range:?} to {target_range}"
            );
            assert!(app.ime_preedit.is_empty());
            assert_eq!(layer_holding(&app, text), Some(source));
            let before = app
                .doc()
                .find_shape(text)
                .expect("preserved header")
                .clone();
            app.text_insert("Leak");
            assert_eq!(app.doc().find_shape(text), Some(&before));
            let ShapeKind::Text { spans, .. } = &before.kind else {
                panic!("header text");
            };
            assert_eq!(tracedraw_core::document::spans_text(spans), "Header");
        }
    }

    #[test]
    fn range_layer_navigation_finishes_a_crop_before_clearing_its_selection() {
        let mut app = book(2);
        assert!(create_range_layer(&mut app, &draft("2")));
        let target = app.doc().master.last().expect("target range").id;
        assert!(create_range_layer(&mut app, &draft("1")));
        let source = app.doc().master.last().expect("source range").id;
        let shape = app
            .new_shape(ShapeKind::Rect {
                rect: tracedraw_core::Rect::new(0.0, 0.0, 10.0, 10.0),
                radius: 0.0,
                corners: None,
            })
            .expect("crop object");
        app.select(vec![shape]);
        app.crop_area = Some(tracedraw_core::Rect::new(1.0, 1.0, 9.0, 9.0));

        assert!(activate_range_layer(&mut app, target));
        assert_eq!(app.page_index(), 1);
        assert!(app.crop_area.is_none());
        assert!(app.selection.is_empty());
        assert_eq!(layer_holding(&app, shape), Some(source));
    }

    #[test]
    fn a_range_layer_starts_on_the_current_page_and_undoes_in_one_step() {
        let mut app = book(4);
        app.goto_page(1);
        let state = NewRangeLayer::of(&app);
        assert_eq!(state.page_range, "2");
        let before = app.doc().clone();
        assert!(create_range_layer(&mut app, &state));
        let layer = app.doc().master.last().expect("range layer");
        assert_eq!(layer.scope, MasterScope::All);
        assert!(!layer.in_front && !layer.desktop);
        assert!(layer.page_range.as_ref().expect("range").contains(1));
        assert_eq!(app.active_layer(), Some(layer.id));
        app.undo();
        assert_eq!(app.doc().master, before.master);
        assert_eq!(app.doc().pages, before.pages);
    }

    #[test]
    fn future_ranges_survive_and_become_editable_when_pages_are_added() {
        let mut app = book(4);
        let page = app.page;
        assert!(create_range_layer(&mut app, &draft("10-12")));
        let layer = app.doc().master.last().expect("range layer").id;
        assert_eq!(app.page, page);
        assert!(app.picked_layer.is_none());
        assert!(!app.status.is_empty());
        while app.doc().pages.len() < 10 {
            app.add_page();
        }
        app.goto_page(0);
        activate_range_layer(&mut app, layer);
        assert_eq!(app.page_index(), 9);
        assert_eq!(app.active_layer(), Some(layer));
    }

    #[test]
    fn range_layers_have_their_own_tree_group_and_ordinary_front_settings() {
        let mut app = book(3);
        new_master_layer(&mut app, MasterScope::All);
        let ordinary = app.doc().master.last().expect("ordinary master").id;
        assert!(create_range_layer(&mut app, &draft("2-3")));
        let ranged = app.doc().master.last().expect("range master").id;
        app.objects_master_closed = true;
        let ctx = crate::theme::ui_context();
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 1000.0))),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| objects_docker(&mut app, ui));
        out.textures_delta.clear();
        assert!(ctx
            .read_response(egui::Id::new(("objects_ranges", "arrow")))
            .is_some());
        assert!(ctx
            .read_response(egui::Id::new(("objects_layer", ranged.raw())).with("eye"))
            .is_some());
        assert!(ctx
            .read_response(egui::Id::new(("objects_layer", ordinary.raw())).with("eye"))
            .is_none());
        set_master_front(&mut app, MasterScope::All, true);
        assert!(app.doc().layer(ordinary).expect("ordinary").in_front);
        assert!(!app.doc().layer(ranged).expect("range").in_front);
        let targets = layer_targets(&app);
        assert!(targets.iter().any(|(heading, id, _)| *id == ranged
            && heading.as_deref() == Some(tr("docker.range_pages").as_str())));
    }

    #[test]
    fn editing_a_range_clears_off_page_selection_and_scope_restores_an_ordinary_master() {
        let mut app = book(3);
        assert!(create_range_layer(&mut app, &draft("1-2")));
        let layer = app.doc().master.last().expect("range").id;
        let shape = app
            .new_shape(ShapeKind::Rect {
                rect: tracedraw_core::Rect::new(0.0, 0.0, 10.0, 10.0),
                radius: 0.0,
                corners: None,
            })
            .expect("shape");
        app.select(vec![shape]);
        let mut state = LayerProps::of(&app, layer).expect("properties");
        state.page_range = "3".into();
        app.engine
            .run_batch("Layer Properties", &state.commands(&app))
            .expect("edit");
        app.sync_layer_range(layer);
        assert!(app.selection.is_empty());
        assert!(app.picked_layer.is_none());
        let mut state = LayerProps::of(&app, layer).expect("properties");
        state.use_page_range = false;
        state.scope = MasterScope::Odd;
        app.engine
            .run_batch("Layer Properties", &state.commands(&app))
            .expect("ordinary scope");
        assert!(app.doc().layer(layer).expect("layer").page_range.is_none());
        assert_eq!(
            app.doc().layer(layer).expect("layer").scope,
            MasterScope::Odd
        );
    }

    #[test]
    fn invalid_ranges_keep_the_properties_dialog_open_and_cancel_preserves_the_drawing() {
        let mut app = book(3);
        assert!(create_range_layer(&mut app, &draft("1-2")));
        let layer = app.doc().master.last().expect("range").id;
        let before = app.doc().clone();
        let mut state = LayerProps::of(&app, layer).expect("properties");
        state.page_range = "0, bad".into();
        state.name = "Uncommitted name".into();
        assert!(!state.valid());
        assert!(state.commands(&app).is_empty());
        app.dialog = crate::ui::dialogs::Dialog::LayerProperties(state);
        let ctx = crate::theme::ui_context();
        dialog_frame(&ctx, &mut app, egui::Key::Enter);
        assert!(matches!(
            app.dialog,
            crate::ui::dialogs::Dialog::LayerProperties(_)
        ));
        assert_eq!(app.doc(), &before);
        dialog_frame(&ctx, &mut app, egui::Key::Escape);
        assert!(matches!(app.dialog, crate::ui::dialogs::Dialog::None));
        assert_eq!(app.doc(), &before);
        app.dialog = crate::ui::dialogs::Dialog::NewRangeLayer(draft("1-2"));
        dialog_frame(&ctx, &mut app, egui::Key::Escape);
        assert!(matches!(app.dialog, crate::ui::dialogs::Dialog::None));
        assert_eq!(app.doc(), &before);
        for range in ["", "0", "x", "1-"] {
            assert!(!create_range_layer(&mut app, &draft(range)));
        }
        assert_eq!(app.doc(), &before);
    }

    #[test]
    fn an_object_rows_menu_opens_on_a_page_in_its_range() {
        let mut app = book(3);
        assert!(create_range_layer(&mut app, &draft("2-3")));
        let layer = app.doc().master.last().expect("range").id;
        let shape = app
            .new_shape(ShapeKind::Rect {
                rect: tracedraw_core::Rect::new(0.0, 0.0, 10.0, 10.0),
                radius: 0.0,
                corners: None,
            })
            .expect("shape");
        app.goto_page(0);
        open_row_menu(&mut app, shape, Pos2::new(100.0, 100.0));
        assert_eq!(app.page_index(), 1);
        assert_eq!(app.selection, vec![shape]);
        assert_eq!(app.context_docker, Some(shape));
        app.run(Command::SetLayerPageRange {
            layer,
            page_range: PageRanges::parse("10-12"),
        });
        let page = app.page;
        open_row_menu(&mut app, shape, Pos2::new(100.0, 100.0));
        assert_eq!(app.page, page);
        assert!(app.selection.is_empty());
        assert!(app.context_menu.is_none());
    }

    fn assert_transfer_activates_the_range(transfer: fn(&mut App, &[ShapeId], LayerId)) {
        for (range, from, expected, editable) in [
            ("3-5", 0, 2, true),
            ("3-5", 3, 3, true),
            ("10-12", 0, 0, false),
        ] {
            let mut app = book(5);
            app.goto_page(from);
            let shape = app
                .new_shape(ShapeKind::Rect {
                    rect: tracedraw_core::Rect::new(0.0, 0.0, 10.0, 10.0),
                    radius: 0.0,
                    corners: None,
                })
                .expect("source object");
            assert!(create_range_layer(&mut app, &draft(range)));
            let layer = app.doc().master.last().expect("range layer").id;
            app.goto_page(from);
            app.select(vec![shape]);
            transfer(&mut app, &[shape], layer);
            assert_eq!(layer_holding(&app, shape), Some(layer));
            assert_eq!(app.page_index(), expected, "{range} from {from}");
            if editable {
                assert_eq!(app.active_layer(), Some(layer));
                assert_eq!(app.selection, vec![shape]);
            } else {
                assert!(app.selection.is_empty());
                assert!(app.picked_layer.is_none());
                assert!(!app.status.is_empty());
            }
        }
    }

    #[test]
    fn move_to_a_range_layer_activates_a_matching_page_or_clears_editing_state() {
        assert_transfer_activates_the_range(move_to_layer);
    }

    #[test]
    fn dropping_into_a_range_layer_activates_a_matching_page_or_clears_editing_state() {
        assert_transfer_activates_the_range(|app, ids, layer| {
            drop_rows(app, ids, RowDrop::Layer(layer));
        });
    }

    #[test]
    fn layer_selection_and_paste_use_a_matching_range_page() {
        let mut app = book(5);
        let source = app
            .new_shape(ShapeKind::Rect {
                rect: tracedraw_core::Rect::new(0.0, 0.0, 10.0, 10.0),
                radius: 0.0,
                corners: None,
            })
            .expect("source object");
        app.select(vec![source]);
        app.copy();
        assert!(app.clipboard.is_some());
        assert!(create_range_layer(&mut app, &draft("3-5")));
        let layer = app.doc().master.last().expect("range layer").id;
        app.goto_page(0);
        assert!(paste_layer(&mut app, layer));
        assert_eq!(app.page_index(), 2);
        assert_eq!(app.active_layer(), Some(layer));
        assert!(!app.selection.is_empty());
        assert!(app
            .selection
            .iter()
            .all(|id| layer_holding(&app, *id) == Some(layer)));
        app.goto_page(0);
        assert!(select_layer_objects(&mut app, layer));
        assert_eq!(app.page_index(), 2);
        assert!(!app.selection.is_empty());
        app.run(Command::SetLayerPageRange {
            layer,
            page_range: PageRanges::parse("10-12"),
        });
        let page = app.page;
        let document = app.doc().clone();
        assert!(!paste_layer(&mut app, layer));
        assert_eq!(app.page, page);
        assert_eq!(app.doc(), &document);
        assert!(app.selection.is_empty());
        assert!(app.picked_layer.is_none());
        app.select(vec![source]);
        assert!(!select_layer_objects(&mut app, layer));
        assert_eq!(app.doc(), &document);
        assert!(app.selection.is_empty());
        assert!(app.picked_layer.is_none());
    }

    fn clip_with_contents(app: &mut App) -> (ShapeId, ShapeId) {
        let frame = Shape::new(
            app.engine.new_shape_id(),
            ShapeKind::Rect {
                rect: tracedraw_core::Rect::new(0.0, 0.0, 20.0, 20.0),
                radius: 0.0,
                corners: None,
            },
        );
        let content = Shape::new(
            app.engine.new_shape_id(),
            ShapeKind::Rect {
                rect: tracedraw_core::Rect::new(2.0, 2.0, 8.0, 8.0),
                radius: 0.0,
                corners: None,
            },
        );
        let content_id = content.id;
        let clip = app
            .new_shape(ShapeKind::ClipFrame {
                frame: Box::new(frame),
                contents: vec![content],
            })
            .expect("clip frame");
        (clip, content_id)
    }

    fn assert_clip_contents(app: &App, clip: ShapeId, content: ShapeId) {
        let ShapeKind::ClipFrame { contents, .. } =
            &app.doc().find_shape(clip).expect("frame").kind
        else {
            panic!("clip frame");
        };
        assert_eq!(
            contents.iter().map(|shape| shape.id).collect::<Vec<_>>(),
            vec![content]
        );
    }

    #[test]
    fn moving_edited_text_to_a_range_finishes_editing_before_navigation() {
        let mut app = book(5);
        assert!(create_range_layer(&mut app, &draft("3-5")));
        let layer = app.doc().master.last().expect("range").id;
        app.goto_page(0);
        app.start_text(tracedraw_core::Point::new(20.0, 20.0), None);
        app.insert_text("Header");
        let text = app.text_edit.as_ref().expect("text edit").shape;
        move_to_layer(&mut app, &[text], layer);
        assert!(app.text_edit.is_none());
        assert_eq!(app.page_index(), 2);
        assert_eq!(layer_holding(&app, text), Some(layer));
        let ShapeKind::Text { spans, .. } = &app.doc().find_shape(text).expect("text").kind else {
            panic!("text");
        };
        assert_eq!(
            spans
                .iter()
                .map(|span| span.text.as_str())
                .collect::<String>(),
            "Header"
        );
        app.goto_page(0);
        app.start_text(tracedraw_core::Point::new(30.0, 30.0), None);
        let empty = app.text_edit.as_ref().expect("empty text edit").shape;
        move_to_layer(&mut app, &[empty], layer);
        assert!(app.text_edit.is_none());
        assert!(app.doc().find_shape(empty).is_none());
        assert_eq!(app.doc().layer(layer).expect("range").shapes.len(), 1);
    }

    #[test]
    fn dropping_an_edited_clip_frame_into_a_future_range_keeps_its_contents() {
        let mut app = book(5);
        assert!(create_range_layer(&mut app, &draft("10-12")));
        let layer = app.doc().master.last().expect("range").id;
        let (clip, content) = clip_with_contents(&mut app);
        app.select(vec![clip]);
        app.edit_clip_frame();
        assert!(app.clip_frame_edit.is_some());
        assert_eq!(app.selection, vec![content]);
        drop_rows(&mut app, &[clip], RowDrop::Layer(layer));
        assert!(app.clip_frame_edit.is_none());
        assert_eq!(layer_holding(&app, clip), Some(layer));
        assert_eq!(app.page_index(), 0);
        assert!(app.selection.is_empty());
        assert!(app.picked_layer.is_none());
        assert_clip_contents(&app, clip, content);
        assert!(!app
            .doc()
            .pages
            .iter()
            .flat_map(|page| &page.layers)
            .flat_map(|layer| &layer.shapes)
            .any(|shape| shape.id == content));
    }

    #[test]
    fn hiding_a_range_during_clip_editing_restores_the_original_frame_contents() {
        let mut app = book(5);
        assert!(create_range_layer(&mut app, &draft("1-2")));
        let layer = app.doc().master.last().expect("range").id;
        let (clip, content) = clip_with_contents(&mut app);
        app.select(vec![clip]);
        app.edit_clip_frame();
        assert!(app.clip_frame_edit.is_some());
        app.run(Command::SetLayerPageRange {
            layer,
            page_range: PageRanges::parse("3-5"),
        });
        assert!(app.clip_frame_edit.is_none());
        assert!(app.selection.is_empty());
        assert!(app.picked_layer.is_none());
        assert_clip_contents(&app, clip, content);
        assert_eq!(app.doc().layer(layer).expect("range").shapes.len(), 1);
    }

    #[test]
    fn moving_a_range_layer_to_a_page_finishes_clip_editing_before_copying_the_layer() {
        for other_page in [false, true] {
            let mut app = book(5);
            assert!(create_range_layer(&mut app, &draft("1-2")));
            let layer = app.doc().master.last().expect("range").id;
            let (clip, content) = clip_with_contents(&mut app);
            app.select(vec![clip]);
            app.edit_clip_frame();
            assert!(app.clip_frame_edit.is_some());
            let target = if other_page {
                app.doc().pages[3].id
            } else {
                app.page
            };
            if other_page {
                layer_to_page(&mut app, layer, target);
            } else {
                master_to_page(&mut app, layer);
            }
            assert!(app.clip_frame_edit.is_none());
            assert_eq!(app.page, target);
            assert_clip_contents(&app, clip, content);
            let destination = layer_holding(&app, clip).expect("destination");
            assert!(app
                .doc()
                .page(target)
                .expect("page")
                .layers
                .iter()
                .any(|layer| layer.id == destination));
            assert_eq!(app.doc().layer(destination).expect("layer").shapes.len(), 1);
            assert!(app.doc().layer(layer).is_err());
        }
    }
}

/// A current page omits Go to Page instead of showing it disabled.
fn filter_page_row_popup<'a>(
    popup: egui::Popup<'a>,
    preferences: &crate::ui::context_menus::Preferences,
    current: bool,
) -> egui::Popup<'a> {
    let visible = preferences.is_visible("objects_page", "docker.rename")
        || !current && preferences.is_visible("objects_page", "docker.go_to_page");
    if crate::ui::menus::invoke::active() || visible {
        popup
    } else {
        egui::Popup::close_id(popup.ctx(), popup.get_id());
        popup.open(false)
    }
}

#[cfg(test)]
mod conditional_page_popup_tests {
    use super::*;

    fn popup_drawn(preferences: &crate::ui::context_menus::Preferences, current: bool) -> bool {
        let context = crate::theme::ui_context();
        let id = egui::Id::new("conditional_objects_page_popup");
        egui::Popup::open_id(&context, id);
        let mut drawn = false;
        let mut output = context.run_ui(Default::default(), |ui| {
            let popup = egui::Popup::new(
                id,
                context.clone(),
                egui::PopupAnchor::Position(egui::pos2(20.0, 20.0)),
                ui.layer_id(),
            )
            .open_memory(None);
            drawn = filter_page_row_popup(popup, preferences, current)
                .show(|ui| {
                    let _ = ui.add_enabled(false, egui::Button::new("Visible row"));
                })
                .is_some();
        });
        output.textures_delta.clear();
        assert_eq!(egui::Popup::is_id_open(&context, id), drawn);
        drawn
    }

    #[test]
    fn hidden_rename_closes_the_current_page_popup_but_keeps_other_pages_available() {
        let mut preferences = crate::ui::context_menus::Preferences::default();
        assert!(popup_drawn(&preferences, true));
        preferences.set_visible("objects_page", "docker.rename", false);
        assert!(!popup_drawn(&preferences, true));
        assert!(popup_drawn(&preferences, false));
        preferences.set_visible("objects_page", "docker.go_to_page", false);
        assert!(!popup_drawn(&preferences, false));
        preferences.set_visible("objects_page", "docker.rename", true);
        assert!(popup_drawn(&preferences, true));
    }
}

#[cfg(test)]
mod context_menu_settings_tests {
    use super::*;

    #[test]
    fn hiding_the_object_move_group_preserves_direct_layer_actions() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        let id = app
            .new_shape(ShapeKind::Rect {
                rect: tracedraw_core::Rect::new(0.0, 0.0, 10.0, 10.0),
                radius: 0.0,
                corners: None,
            })
            .expect("object");
        let own = layer_holding(&app, id);
        new_layer(&mut app);
        let destination = app.active_layer();
        app.select(vec![id]);
        app.settings
            .context_menus
            .set_visible("objects_object", "docker.move_to_layer", false);
        let run = |app: &mut App, scoped: bool| {
            crate::ui::menus::replay::start(1);
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                let _scope = scoped.then(|| {
                    crate::ui::context_menus::Scope::new(
                        &app.settings.context_menus,
                        "objects_object",
                    )
                });
                let mut close = false;
                row_menu_items(app, ui, id, &mut close);
            });
            out.textures_delta.clear();
            crate::ui::menus::replay::finish()
        };
        let (hit, _) = run(&mut app, true);
        assert_eq!(hit.as_deref(), Some("docker.copy_to_layer"));
        assert_eq!(layer_holding(&app, id), own);
        let [copy] = app.selection.as_slice() else {
            panic!("one copy should be selected");
        };
        assert_ne!(*copy, id);
        assert_eq!(layer_holding(&app, *copy), destination);
        app.undo();
        app.select(vec![id]);
        app.settings
            .context_menus
            .set_visible("objects_object", "docker.copy_to_layer", false);
        let (hit, rows) = run(&mut app, true);
        assert!(hit.is_none());
        assert_eq!(rows, 1, "only Rename remains");
        assert_eq!(layer_holding(&app, id), own);
        let (hit, _) = run(&mut app, false);
        assert_eq!(hit.as_deref(), Some("docker.move_to_layer"));
        assert_eq!(layer_holding(&app, id), destination);
    }

    fn contains_text(shape: &egui::Shape, expected: &str) -> bool {
        match shape {
            egui::Shape::Text(text) => text.galley.text() == expected,
            egui::Shape::Vec(children) => {
                children.iter().any(|child| contains_text(child, expected))
            }
            _ => false,
        }
    }

    #[test]
    fn hidden_master_menu_choices_remain_in_options_controls() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        app.settings
            .context_menus
            .set_visible("objects_new_master", "options.scope_odd", false);
        let expected = tr("options.scope_odd");
        let draw = |app: &mut App, scoped: bool| {
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                let _scope = scoped.then(|| {
                    crate::ui::context_menus::Scope::new(
                        &app.settings.context_menus,
                        "objects_new_master",
                    )
                });
                master_front_choices(app, ui);
            });
            out.textures_delta.clear();
            out.shapes
                .iter()
                .any(|shape| contains_text(&shape.shape, &expected))
        };
        assert!(!draw(&mut app, true));
        assert!(draw(&mut app, false));
    }
}

#[cfg(test)]
mod guide_context_menu_settings_tests {
    use super::*;
    use tracedraw_core::document::Guide;

    fn frame(
        app: &App,
        ctx: &egui::Context,
        guide: &Guide,
        events: Vec<egui::Event>,
    ) -> Option<GuideCmd> {
        let mut chosen = None;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(600.0, 500.0),
                )),
                events,
                ..Default::default()
            },
            |ui| chosen = guide_menu_items(app, ui, guide),
        );
        output.textures_delta.clear();
        chosen
    }

    fn row_rect(ctx: &egui::Context, key: &str) -> Rect {
        ctx.data(|data| data.get_temp(egui::Id::new(("objects_guide_menu_row", key))))
            .expect("drawn guideline menu row")
    }

    #[test]
    fn context_menu_guide_defaults_retain_order_and_other_locations_are_independent() {
        let mut app = App::headless();
        let ctx = crate::theme::ui_context();
        let guide = Guide::vertical(50.0);
        app.settings.context_menus.set_order(
            "canvas_guide",
            "",
            &["menu.object.properties", "menu.edit.delete"],
        );
        app.settings
            .context_menus
            .set_visible("canvas_guide", "menu.edit.delete", false);
        assert!(frame(&app, &ctx, &guide, Vec::new()).is_none());
        let keys = [
            "menu.edit.delete",
            "menu.edit.cut",
            "menu.edit.copy",
            "menu.edit.paste",
            "guides.lock",
            "menu.object.hide",
        ];
        for pair in keys.windows(2) {
            assert!(row_rect(&ctx, pair[0]).bottom() <= row_rect(&ctx, pair[1]).top());
        }
        app.settings
            .context_menus
            .set_visible("objects_guide", "menu.edit.copy", false);
        assert!(app
            .settings
            .context_menus
            .is_visible("canvas_guide", "menu.edit.copy"));
        assert!(!app
            .settings
            .context_menus
            .is_visible("canvas_guide", "menu.edit.delete"));
    }

    #[test]
    fn context_menu_guide_reordered_button_click_runs_its_original_command() {
        let mut app = App::headless();
        app.choose_master_guides(false);
        app.add_guide(Guide::vertical(50.0));
        app.settings.context_menus.set_order(
            "objects_guide",
            "",
            &["guides.lock", "menu.edit.copy"],
        );
        let ctx = crate::theme::ui_context();
        let guide = app.page_guides().first().expect("guide").clone();
        assert!(frame(&app, &ctx, &guide, Vec::new()).is_none());
        let lock = row_rect(&ctx, "guides.lock");
        assert!(lock.bottom() <= row_rect(&ctx, "menu.edit.copy").top());
        assert!(
            row_rect(&ctx, "menu.edit.copy").bottom() <= row_rect(&ctx, "menu.edit.delete").top()
        );
        let at = lock.center();
        let mut chosen = None;
        for pressed in [true, false] {
            chosen = frame(
                &app,
                &ctx,
                &guide,
                vec![
                    egui::Event::PointerMoved(at),
                    egui::Event::PointerButton {
                        pos: at,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            )
            .or(chosen);
        }
        assert_eq!(chosen, Some(GuideCmd::Lock(true)));
        guide_menu_command(&mut app, 0, chosen.expect("clicked command"));
        assert!(app.page_guides().first().expect("guide").locked);
    }

    fn popup(app: &App, guide: &Guide) -> bool {
        let ctx = crate::theme::ui_context();
        let id = egui::Id::new("customized_guideline_menu");
        egui::Popup::open_id(&ctx, id);
        let mut shown = false;
        let mut output = ctx.run_ui(Default::default(), |ui| {
            let popup = egui::Popup::new(
                id,
                ctx.clone(),
                egui::PopupAnchor::Position(egui::pos2(20.0, 20.0)),
                ui.layer_id(),
            )
            .open_memory(None);
            shown = crate::ui::context_menus::filter_popup(
                popup,
                &app.settings.context_menus,
                "objects_guide",
            )
            .show(|ui| guide_menu_items(app, ui, guide))
            .is_some();
        });
        output.textures_delta.clear();
        assert_eq!(egui::Popup::is_id_open(&ctx, id), shown);
        shown
    }

    #[test]
    fn context_menu_guide_inapplicable_variants_close_empty_popups_but_disabled_rows_remain() {
        let mut app = App::headless();
        let definition = crate::ui::context_menus::CATALOG
            .iter()
            .find(|menu| menu.id == "objects_guide")
            .expect("guide location");
        for key in definition.items {
            app.settings
                .context_menus
                .set_visible(definition.id, key, false);
        }
        app.settings
            .context_menus
            .set_visible(definition.id, "guides.unlock", true);
        app.settings
            .context_menus
            .set_visible(definition.id, "guides.show", true);
        let mut guide = Guide::vertical(50.0);
        assert!(
            !popup(&app, &guide),
            "only unavailable conditional variants remain"
        );
        guide.locked = true;
        assert!(popup(&app, &guide), "unlock is now offered");
        app.settings
            .context_menus
            .set_visible(definition.id, "guides.unlock", false);
        assert!(!popup(&app, &guide));
        app.settings
            .context_menus
            .set_visible(definition.id, "menu.edit.delete", true);
        assert!(
            popup(&app, &guide),
            "disabled Delete is still a visible operation"
        );
    }
}
