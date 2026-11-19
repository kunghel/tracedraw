//! Moving the selection to another page by dragging it onto that page's
//! tab (#1346). While the Pick tool moves the selection, the page tab
//! under the pointer lights up; resting there for
//! [`PAGE_TAB_HOVER_SECS`] sends the selection to that page, which
//! becomes the current one, and the move goes on there (the objects
//! stay where they were on the page while the pointer is over the tabs).
//! Let go on a tab, the selection goes to that page at once. Either way
//! the objects keep their place on the page; with Ctrl held (or a right
//! click during the move) copies go and the originals stay. The whole
//! drag is one undo step. A colour or a fill dragged (a palette swatch,
//! the status bar's or the property bar's fill, an object with the right
//! button) and rested on another page's tab opens that page the same way,
//! so it can be let go on an object there (#2704). See
//! `docs/behavior/pages.md`, Moving objects to another page by its tab.

use crate::app::App;
use crate::app::Drag;
use egui::Pos2;
use tracedraw_core::geometry::Rect;
use tracedraw_core::{Command, LayerId, ShapeId};

/// How long a moved selection rests on a page tab before that page
/// opens, in seconds.
pub const PAGE_TAB_HOVER_SECS: f64 = 0.6;

impl App {
    /// The page (by index) whose tab is at `at` (screen), as last drawn.
    pub fn page_tab_at(&self, at: Pos2) -> Option<usize> {
        self.page_tab_rects
            .iter()
            .find(|(_, r)| r.contains(at))
            .map(|(i, _)| *i)
    }

    /// The page tab under the pointer.
    pub fn page_tab_under_pointer(&self) -> Option<usize> {
        self.pointer_screen.and_then(|p| self.page_tab_at(p))
    }

    /// The layer of page `idx` that objects sent there go to: its
    /// topmost visible, unlocked layer, else its topmost unlocked one
    /// (as [`App::active_layer`] chooses for the current page).
    pub(crate) fn page_drop_layer(&self, idx: usize) -> Option<LayerId> {
        let page = self.doc().pages.get(idx)?;
        if page.id == self.page {
            return self.active_layer();
        }
        page.layers
            .iter()
            .rev()
            .find(|l| !l.locked && l.visible)
            .or_else(|| page.layers.iter().rev().find(|l| !l.locked))
            .map(|l| l.id)
    }

    /// The drawing's desktop layer (on the master page, shown from every
    /// page), when it is there and unlocked.
    pub(crate) fn desktop_layer(&self) -> Option<LayerId> {
        self.doc()
            .master
            .iter()
            .find(|l| l.desktop && !l.locked)
            .map(|l| l.id)
    }

    /// The layer objects with the box `bounds`, meant for `layer`, go
    /// to while the desktop takes objects beside the page (#3100): the
    /// desktop layer when they lie wholly beside the page and `layer` is
    /// one of the current page's own layers; the page's layer when
    /// `layer` is the desktop and they reach onto the page; else `layer`.
    pub(crate) fn layer_beside_page(&self, layer: LayerId, bounds: Rect) -> LayerId {
        if !self.settings.desktop_takes_objects_beside_page {
            return layer;
        }
        let Some(desktop) = self.desktop_layer() else {
            return layer;
        };
        let p = self.page_rect();
        let beside =
            bounds.x1 <= p.x0 || bounds.x0 >= p.x1 || bounds.y1 <= p.y0 || bounds.y0 >= p.y1;
        let page = self.doc().pages.iter().find(|pg| pg.id == self.page);
        let on_page = |l: LayerId| page.is_some_and(|pg| pg.layers.iter().any(|x| x.id == l));
        if beside && on_page(layer) {
            return desktop;
        }
        if !beside && layer == desktop {
            let back = self.active_layer().filter(|l| on_page(*l)).or_else(|| {
                page.and_then(|pg| {
                    pg.layers
                        .iter()
                        .rev()
                        .find(|l| !l.locked && l.visible)
                        .or_else(|| pg.layers.iter().rev().find(|l| !l.locked))
                        .map(|l| l.id)
                })
            });
            return back.unwrap_or(layer);
        }
        layer
    }

    /// After a move let go: each selected object (not one chosen inside
    /// a group) goes to the layer [`App::layer_beside_page`] gives it, in
    /// the move's undo step (the steps after `before` become one). The
    /// selection stays.
    pub(crate) fn settle_beside_page(&mut self, before: Option<u64>) {
        if !self.settings.desktop_takes_objects_beside_page
            || self.editing_clip_contents()
            || self.symmetry_edit.is_some()
        {
            return;
        }
        let mut to: Vec<(LayerId, Vec<ShapeId>)> = Vec::new();
        for l in self.doc().all_layers() {
            for s in l.shapes.iter().filter(|s| self.selection.contains(&s.id)) {
                let goes = self.layer_beside_page(l.id, s.bounds());
                if goes == l.id {
                    continue;
                }
                match to.iter_mut().find(|(t, _)| *t == goes) {
                    Some((_, ids)) => ids.push(s.id),
                    None => to.push((goes, vec![s.id])),
                }
            }
        }
        let cmds: Vec<Command> = to
            .iter()
            .flat_map(|(layer, ids)| crate::ui::objects_docker::move_commands(self, ids, *layer))
            .collect();
        if cmds.is_empty() {
            return;
        }
        let moved = self.engine.last_step_id() != before;
        let label = self.engine.undo_label().filter(|_| moved);
        if let Err(e) = self.engine.run_batch("Move to Layer", &cmds) {
            self.status = e.to_string();
            return;
        }
        if let Some(label) = label {
            self.engine.join_group(before, label);
        }
        let ids = self.selection.clone();
        self.select(ids);
    }

    /// Send the selection to page `idx`, at the same place on the page:
    /// moved, or copied when `copy` (the originals stay). That page
    /// becomes the current one with the objects (or their copies)
    /// selected. False when nothing went (no selection, the page has no
    /// unlocked layer, it is the current page).
    pub fn send_selection_to_page(&mut self, idx: usize, copy: bool) -> bool {
        if self.selection.is_empty() || idx == self.page_index() {
            return false;
        }
        let Some(layer) = self.page_drop_layer(idx) else {
            self.status = crate::i18n::tr("shelf.no_layer");
            return false;
        };
        if !copy {
            let ids = self.selection.clone();
            crate::ui::objects_docker::move_to_layer(self, &ids, layer);
            return true;
        }
        let mut cmds = Vec::new();
        let mut copies: Vec<ShapeId> = Vec::new();
        for s in self.selected_shapes() {
            let id = self.engine.new_shape_id();
            let shape = crate::app::reid_pub(s, id, &mut self.engine);
            copies.push(shape.id);
            cmds.push(Command::AddShape { layer, shape });
        }
        if cmds.is_empty() {
            return false;
        }
        if let Err(e) = self.engine.run_batch("Copy to Page", &cmds) {
            self.status = e.to_string();
            return false;
        }
        self.goto_page(idx);
        self.picked_layer = Some(layer);
        self.select(copies);
        true
    }

    /// Each frame, after the page tabs are drawn: while the selection is
    /// moved, the tab under the pointer and how long it has been there;
    /// resting [`PAGE_TAB_HOVER_SECS`] on another page's tab sends the
    /// selection there and the move goes on on that page. `time` is the
    /// frame's time (seconds), `ctrl` whether Ctrl (Cmd) is held.
    pub fn track_page_tab_drag(&mut self, time: f64, ctrl: bool) {
        if !matches!(self.drag, Drag::Move { .. }) {
            let hover = self.page_tab_hover;
            self.end_page_drag_group();
            self.page_tab_hover = hover;
            self.track_page_tab_color_drag(time);
            return;
        }
        self.page_drag_ctrl = ctrl;
        let over = self.page_tab_under_pointer();
        match over {
            Some(i) if i != self.page_index() => match self.page_tab_hover {
                Some((j, since)) if j == i => {
                    if time - since >= PAGE_TAB_HOVER_SECS {
                        self.page_tab_hover = None;
                        if self.page_drag_group.is_none() {
                            self.page_drag_group = Some(self.engine.begin_group());
                        }
                        let copy = ctrl || std::mem::take(&mut self.drag_copy);
                        self.send_selection_to_page(i, copy);
                    }
                }
                _ => self.page_tab_hover = Some((i, time)),
            },
            _ => self.page_tab_hover = None,
        }
        // On the tabs after the page changed: the objects stay where they
        // are on the page, and the move goes on from the pointer.
        if over.is_some() && self.page_drag_group.is_some() {
            if let Some(at) = self.pointer_screen {
                let p = self.view.to_page(at);
                self.start_move(p);
            }
        }
    }

    /// Whether a colour or a fill is being dragged: a palette swatch, the
    /// status bar's fill, the property bar's fill swatch or an object
    /// with the right mouse button.
    pub fn carrying_color(&self) -> bool {
        self.palette_drag.is_some()
            || self.fill_drag.is_some()
            || self.fill_swatch_drag.is_some()
            || self.right_drag.is_some()
    }

    /// While a colour or a fill is dragged (see [`App::carrying_color`]),
    /// resting [`PAGE_TAB_HOVER_SECS`] on another page's tab opens that
    /// page, so the colour can be let go on an object there (#2704). A
    /// right-button drag keeps the object it started from, whose fill
    /// (or outline) then goes where it is let go.
    fn track_page_tab_color_drag(&mut self, time: f64) {
        if !self.carrying_color() {
            self.page_tab_hover = None;
            return;
        }
        match self.page_tab_under_pointer() {
            Some(i) if i != self.page_index() => match self.page_tab_hover {
                Some((j, since)) if j == i => {
                    if time - since >= PAGE_TAB_HOVER_SECS {
                        self.page_tab_hover = None;
                        if self.right_drag.is_some() && self.right_drag_style.is_none() {
                            self.right_drag_style = self.selected_shapes().first().cloned();
                        }
                        self.goto_page(i);
                    }
                }
                _ => self.page_tab_hover = Some((i, time)),
            },
            _ => self.page_tab_hover = None,
        }
    }

    /// A move let go on a page tab: the selection goes to that page, at
    /// its place there (the move's way to the tab is not applied).
    pub fn drop_on_page_tab(&mut self, idx: usize) {
        let copy = std::mem::take(&mut self.drag_copy) || self.page_drag_ctrl;
        self.drop_frame = None;
        self.send_selection_to_page(idx, copy);
        self.end_page_drag_group();
    }

    /// The move that changed page is over: its steps become one.
    pub fn end_page_drag_group(&mut self) {
        self.page_tab_hover = None;
        if let Some(mark) = self.page_drag_group.take() {
            self.engine.end_group(mark, "Move to Page");
        }
    }
}
