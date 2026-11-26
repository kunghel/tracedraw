//! Bitmaps > Edit Inside: a double click on a picture opens its pixels
//! for editing in place, as a clip frame's contents are edited. The
//! picture's pixels are worked on in memory (see `pixel_edit.rs`) and
//! drawn over the page where the picture lies; Finish puts them back in
//! the picture in one undo step, Cancel leaves it as it was. See
//! `docs/behavior/edit-inside-picture.md`.

use crate::app::App;
use crate::i18n::tr;
use crate::pixel_edit::{
    self, BrushOpts, Dirty, History, MaskStroke, PixelImage, PixelTool, SelectMode, Stroke,
};

/// What the brush and the eraser paint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MaskTarget {
    /// The pixels of the layer in use.
    #[default]
    Pixels,
    /// The mask of the layer in use: the brush hides, the eraser shows.
    LayerMask,
    /// The selection, shown in red where it leaves out (quick mask): the
    /// brush selects, the eraser takes out.
    QuickMask,
}
use egui::{Modifiers, PointerButton, Response};
use std::cell::RefCell;
use tracedraw_core::geometry::{Affine, Point, Rect};
use tracedraw_core::{BitmapEffect, Command, Document, ShapeId, ShapeKind};

/// The undo label of Finish.
const LABEL: &str = "Edit Picture";

/// What the screen holds of the picture being edited.
#[derive(Default)]
pub struct PixelTextures {
    pub tex: Option<egui::TextureHandle>,
    /// The selection's outline: runs of pixel edges, round it clockwise
    /// (see `pixel_edit::selection_edges`).
    pub edges: Vec<[(f32, f32); 2]>,
    /// Pixels changed since the picture was last sent to the screen.
    pub dirty: Option<Dirty>,
    /// The whole picture is sent again (new size, undo, preview).
    pub full: bool,
    /// The selection outline is made again.
    pub sel_changed: bool,
    /// The red view of the mask being painted (quick mask, layer mask).
    pub overlay: Option<egui::TextureHandle>,
    /// The effect preview was shown last frame.
    pub showed_preview: bool,
    /// The Layers panel's miniatures.
    pub thumbs: crate::ui::pixel_layers::LayerThumbs,
}

/// A drag of the Move tool's box.
#[derive(Debug, Clone, Copy)]
pub struct MoveDrag {
    pub handle: crate::pixel_ops::BoxHandle,
    pub from: (f32, f32),
    pub start: crate::pixel_ops::Placement,
    pub b: Dirty,
}

/// What the Move tool's box was found for: the change, the layer in use,
/// whether there was a selection and the picture's size.
pub type PickKey = (u64, usize, bool, (u32, u32));

/// How near a handle of the Move tool's box a press must be, and how far
/// the turning handle stands over the box, in screen points.
pub const BOX_GRAB: f32 = 7.0;
pub const BOX_KNOB: f32 = 22.0;

/// A picture being edited in place.
pub struct PixelEdit {
    pub shape: ShapeId,
    /// The picture's pixel space as first opened (y down) to the page.
    pub img_to_page: Affine,
    /// The bitmap's box and size when it was opened.
    pub rect: Rect,
    pub size_px: (u32, u32),
    pub pi: PixelImage,
    pub history: History,
    pub tool: PixelTool,
    pub opts: BrushOpts,
    pub stroke: Option<Stroke>,
    /// Selections and the gradient: where the drag started and is now.
    pub drag_from: Option<(f32, f32)>,
    pub drag_to: Option<(f32, f32)>,
    pub lasso: Vec<(f32, f32)>,
    /// The pen outline being made (Pen mask), and whether a press is
    /// pulling the handles of its last point.
    pub pen: Vec<pixel_edit::PenNode>,
    pub pen_drag: bool,
    /// Move: the handle of the box being dragged, where the drag began
    /// and the placement and box it began on (#4261).
    pub move_drag: Option<MoveDrag>,
    /// The placement the Move tool's preview shows, when it shows one.
    pub placed_for: Option<crate::pixel_ops::Placement>,
    /// The whole layer's pixels before the Move tool's first placement on
    /// it, the placements put in since, the layer and the change after
    /// the last: a later move works from these, so what a move took past
    /// the picture's edge comes back (#4374). Any other change ends it.
    pub move_memory: Option<MoveMemory>,
    /// Counts the changes seen on the screen, for what is kept from them
    /// (the Move tool's box, the effect preview).
    pub rev: std::cell::Cell<u64>,
    /// The Move tool's box as last found, and what it was found for.
    pub pick_cache: std::cell::Cell<Option<(PickKey, Dirty)>>,
    /// The change the effect preview was made for.
    pub preview_rev: u64,
    /// What Ctrl+C or Ctrl+X took, and where its top left lay (#4263).
    pub clip: Option<(image::RgbaImage, (u32, u32))>,
    /// The opacity slider of the layer in use is being dragged (its
    /// undo step is already remembered).
    pub layer_drag: bool,
    /// What the brush and the eraser paint, and the stroke on a mask.
    pub target: MaskTarget,
    pub mask_stroke: Option<MaskStroke>,
    /// Selections kept to be loaded again while editing.
    pub saved_sels: Vec<image::GrayImage>,
    /// Warp: the grid being dragged, the box it covers and its points
    /// across and down.
    pub warp: Option<crate::pixel_warp::Warp>,
    pub warp_box: Dirty,
    pub warp_grid: usize,
    /// The picture as it was when the editing began (Restore paints it
    /// back).
    pub original: std::rc::Rc<image::RgbaImage>,
    /// The Liquify stroke under way.
    pub liquify: Option<crate::pixel_liquify::Liquify>,
    /// The sprayer's drag under way.
    pub spray: Option<crate::pixel_spray::Spray>,
    /// The Pick tool's numbers on the property bar (move, scale, turn,
    /// flips), put in the layer with Apply.
    pub placement: crate::pixel_ops::Placement,
    /// Smart Resize: the new width and height, percent of the picture's.
    pub smart_size: (f32, f32),
    /// A depth map of the picture (its size, 255 nearest) that stands for
    /// the one read from its light when following the surface.
    pub depth: Option<image::GrayImage>,
    /// Remove: the place painted so far (0 where painted).
    pub remove: Option<image::GrayImage>,
    /// An adjustment layer's setting is being dragged (its undo step is
    /// already remembered).
    pub adjust_drag: bool,
    /// Clone and heal: the source set with Alt+click, and the offset
    /// fixed by the first stroke after it (the source follows the brush).
    pub clone_src: Option<(f32, f32)>,
    pub clone_offset: Option<(f32, f32)>,
    pub changed: bool,
    /// The effect chosen in the panel, its settings and whether its
    /// result shows before Apply.
    pub effect: Option<BitmapEffect>,
    pub preview_on: bool,
    /// The effect's settings were applied and not changed since: Apply
    /// waits for a change, so a second click does not apply the effect
    /// again on its own result (#4376).
    pub effect_applied: bool,
    pub preview: Option<image::RgbaImage>,
    pub feather_px: u32,
    pub grow_px: i32,
    pub view: RefCell<PixelTextures>,
    /// Where the picture lies in the image editing mode's window: its
    /// own zoom and scroll, apart from the drawing's view.
    pub ws: crate::pixel_view::PixelView,
    /// The image window's area on the screen as last drawn.
    pub ws_area: egui::Rect,
    /// The pointer over the picture, in its pixels (the brush outline).
    pub pointer_px: Option<(f32, f32)>,
    /// The dockers of the drawing and whether their column was open,
    /// kept while the image dockers show.
    pub main_dockers: Option<(crate::settings::DockerSetup, bool)>,
    /// The tool each toolbox button shows last picked from its flyout.
    pub group_pick: Vec<PixelTool>,
    /// The drawing without the picture, for the page under the edit.
    pub hidden: RefCell<Option<(u64, std::rc::Rc<Document>)>>,
    /// The Light tool's lights (see `pixel_light.rs`).
    pub lights: crate::pixel_light::Lights,
    /// Which time Edit Inside was entered since the program started: the
    /// panels key their remembered sizes by it, so an entry never starts
    /// from the sizes the last one was left at (#3852).
    pub session: u64,
    /// The text layer being typed into with the Text tool (#4370).
    pub text_edit: Option<TextEditing>,
    /// The Text tool dragging a text layer: where the press was and
    /// where the text stood then.
    pub text_drag: Option<((f32, f32), (f32, f32))>,
    /// The settings a new text layer starts with (the property bar's).
    pub text_opts: crate::pixel_text::PixelText,
    /// A change of a text layer's settings is under way, its undo step
    /// already remembered.
    pub text_undo_open: bool,
    /// A command waiting for its pixels (and colour) in a small box
    /// (#4393).
    pub px_ask: Option<PxAsk>,
    /// The pixels each such command was last given, and where the last
    /// stroke of the selection lay.
    pub ask_px: std::collections::HashMap<crate::ui::pixel_cmds::PixelCmd, f32>,
    pub stroke_at: crate::pixel_style::StrokeAt,
    /// A layer style setting is being dragged (its undo step is already
    /// remembered).
    pub style_drag: bool,
}

/// The text layer the Text tool types into.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextEditing {
    pub layer: usize,
    /// The typing box takes the keyboard on its next frame.
    pub focus: bool,
}

/// A selection command asking for its number of pixels first (Feather,
/// Smooth, Border, Expand, Contract, Stroke, Fill), with what it was
/// last given (#4393).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PxAsk {
    pub cmd: crate::ui::pixel_cmds::PixelCmd,
    pub px: f32,
    pub color: [u8; 4],
    pub opacity: f32,
    pub at: crate::pixel_style::StrokeAt,
    /// Where the box opens on the screen.
    pub pos: Option<egui::Pos2>,
}

impl PixelEdit {
    /// The page point `p` in the picture's pixels (as it is now).
    pub fn to_px(&self, p: Point) -> (f32, f32) {
        let q = self.img_to_page.inverse() * p;
        (
            q.x as f32 - self.pi.origin.0 as f32,
            q.y as f32 - self.pi.origin.1 as f32,
        )
    }

    /// The picture pixel (x, y) of the picture as it is now on the page.
    pub fn to_page(&self, x: f32, y: f32) -> Point {
        self.img_to_page
            * Point::new(
                (x + self.pi.origin.0 as f32) as f64,
                (y + self.pi.origin.1 as f32) as f64,
            )
    }

    fn mark(&self, b: Option<Dirty>) {
        if let Some(b) = b {
            let mut v = self.view.borrow_mut();
            v.dirty = Some(pixel_edit::union(v.dirty, b));
            self.bump();
        }
    }

    fn mark_all(&self) {
        let mut v = self.view.borrow_mut();
        v.full = true;
        v.sel_changed = true;
        self.bump();
    }

    /// A change that shows: what was kept from the pixels is found again.
    pub fn bump(&self) {
        self.rev.set(self.rev.get().wrapping_add(1));
    }

    /// The box the Move tool works on (see `pixel_ops::pick_box`), kept
    /// until the pixels, the layer in use or the selection change.
    pub fn move_box(&self) -> Dirty {
        let key = (
            self.rev.get(),
            self.pi.active,
            self.pi.sel.is_some(),
            self.pi.img.dimensions(),
        );
        if let Some((k, b)) = self.pick_cache.get() {
            if k == key {
                return b;
            }
        }
        let b = crate::pixel_ops::pick_box(&self.pi);
        self.pick_cache.set(Some((key, b)));
        b
    }

    /// The Move tool's placement shown on the layer in use while it is
    /// set (by a drag of the box or by the numbers on the property bar),
    /// and taken off once it is put in or reset.
    pub fn sync_place_preview(&mut self) {
        let want = (self.tool == PixelTool::Move
            && self.warp.is_none()
            && !self.placement.is_identity())
        .then_some(self.placement);
        if want == self.placed_for {
            return;
        }
        let had = self.placed_for.is_some();
        self.placed_for = want;
        match want {
            Some(p) => {
                let b = self.move_drag.map(|d| d.b).unwrap_or_else(|| self.move_box());
                self.preview = self.placed_layer(b, &p).or_else(|| {
                    crate::pixel_ops::place_image(&self.pi.img, self.pi.sel.as_ref(), b, &p)
                        .map(|(img, _)| img)
                });
                self.view.borrow_mut().full = true;
            }
            None if had => {
                self.preview = None;
                self.view.borrow_mut().full = true;
            }
            None => {}
        }
    }

    /// The Move tool's memory of the layer in use, while nothing else
    /// changed it since the last placement and nothing is selected.
    fn live_move_memory(&self) -> Option<&MoveMemory> {
        self.move_memory.as_ref().filter(|m| {
            m.layer == self.pi.active && m.rev == self.rev.get() && self.pi.sel.is_none()
        })
    }

    /// The layer in use with placement `p` about box `b` put in, from the
    /// pixels the Move tool remembers when it can (#4374).
    fn placed_layer(&self, b: Dirty, p: &crate::pixel_ops::Placement) -> Option<image::RgbaImage> {
        let m = self.live_move_memory()?;
        let mut steps = m.steps.clone();
        steps.push((b, *p));
        crate::pixel_ops::place_chain(&m.source, &steps)
    }

    /// The Move tool's placement put in the layer in use, one undo step.
    pub fn apply_placement(&mut self, b: Option<Dirty>) {
        let p = self.placement;
        self.placement = Default::default();
        self.move_drag = None;
        if p.is_identity() {
            self.sync_place_preview();
            return;
        }
        // A text layer moved as a whole stays text: its place changes
        // (scaled, turned or mirrored it becomes pixels) (#4370).
        let shift_only = p.sx == 1.0 && p.sy == 1.0 && p.angle == 0.0 && !p.flip_h && !p.flip_v;
        if shift_only && self.pi.sel.is_none() && self.active_text().is_some() {
            self.text_undo_open = false;
            self.change_text(|t| {
                t.at.0 += p.dx;
                t.at.1 += p.dy;
            });
            self.text_undo_open = false;
            self.preview = None;
            self.placed_for = None;
            self.view.borrow_mut().full = true;
            return;
        }
        let b = b.unwrap_or_else(|| self.move_box());
        self.preview = None;
        self.placed_for = None;
        let whole_layer = self.pi.sel.is_none();
        let (source, mut steps) = match self.live_move_memory() {
            Some(m) => (m.source.clone(), m.steps.clone()),
            None => (self.pi.img.clone(), Vec::new()),
        };
        let from_memory = self.placed_layer(b, &p);
        let done = match from_memory {
            Some(img) => self.apply(true, move |pi| {
                pi.img = img;
                Some([0, 0, pi.img.width(), pi.img.height()])
            }),
            None => self.apply(true, |pi| crate::pixel_ops::place_in(pi, b, &p)),
        };
        self.move_memory = (whole_layer && done).then(|| {
            steps.push((b, p));
            MoveMemory {
                layer: self.pi.active,
                source,
                steps,
                rev: self.rev.get(),
            }
        });
        self.view.borrow_mut().full = true;
    }

    /// The effect open in the Effects panel shown again on the pixels as
    /// they are now, once a stroke or a change has ended: painting with
    /// the effect open took its preview off, and the preview did not come
    /// back until a setting was changed (#4264).
    pub fn keep_preview_current(&mut self, busy: bool) {
        if busy
            || self.effect.is_none()
            || !self.preview_on
            || self.warp.is_some()
            || self.placed_for.is_some()
            || self.stroke.is_some()
            || self.mask_stroke.is_some()
            || self.liquify.is_some()
            || self.spray.is_some()
            || self.preview_rev == self.rev.get()
        {
            return;
        }
        self.refresh_preview();
    }

    /// Remember the picture before a change made outside `apply` (the
    /// layer settings).
    pub fn remember(&mut self) {
        self.step();
    }

    /// Remember the picture before a change.
    fn step(&mut self) {
        self.lights.bake();
        self.history.push(&self.pi);
        self.changed = true;
        self.preview = None;
        self.placed_for = None;
        self.bump();
    }

    pub fn set_selection(&mut self, sel: Option<image::GrayImage>) {
        self.step();
        self.pi.sel = sel;
        self.view.borrow_mut().sel_changed = true;
        self.bump();
    }

    pub fn undo(&mut self) {
        // Live lights undo their strokes one by one first.
        if self.lights.is_live() && self.undo_light() {
            return;
        }
        if self.history.undo(&mut self.pi) {
            self.mark_all();
            self.preview = None;
            self.placed_for = None;
            self.placement = Default::default();
        }
    }

    pub fn redo(&mut self) {
        self.lights.bake();
        if self.history.redo(&mut self.pi) {
            self.mark_all();
            self.preview = None;
            self.placed_for = None;
            self.placement = Default::default();
        }
    }

    /// Run an operation on the pixels of the layer in use as one undo
    /// step; `full` when it may change the size or every pixel. Nothing
    /// happens on a locked layer.
    pub fn apply(&mut self, full: bool, f: impl FnOnce(&mut PixelImage) -> Option<Dirty>) -> bool {
        if self.pi.active_locked() {
            return false;
        }
        // A text layer whose pixels are changed becomes plain pixels.
        let i = self.pi.active;
        self.apply_all(full, |pi| {
            let b = f(pi)?;
            if let Some(l) = pi.layers.get_mut(i) {
                l.text = None;
            }
            Some(b)
        })
    }

    /// The text layer in use, when it is one.
    pub fn active_text(&self) -> Option<&crate::pixel_text::PixelText> {
        self.pi.layers.get(self.pi.active)?.text.as_ref()
    }

    /// Layer `i`'s pixels drawn again from its text.
    pub fn render_text_layer(&mut self, i: usize) {
        let (w, h) = self.pi.img.dimensions();
        let origin = self.pi.origin;
        let Some(t) = self.pi.layers.get(i).and_then(|l| l.text.clone()) else {
            return;
        };
        let img = t.render(w, h, origin);
        if i == self.pi.active {
            self.pi.img = img;
        } else if let Some(l) = self.pi.layers.get_mut(i) {
            l.img = img;
        }
        self.changed = true;
        self.mark_all();
    }

    /// Change the text layer in use with `f` and draw it again; the first
    /// change of a run is one undo step (#4370).
    pub fn change_text(&mut self, f: impl FnOnce(&mut crate::pixel_text::PixelText)) {
        if self.active_text().is_none() {
            return;
        }
        if !self.text_undo_open {
            self.remember();
            self.text_undo_open = true;
        }
        let i = self.pi.active;
        if let Some(t) = self.pi.layers.get_mut(i).and_then(|l| l.text.as_mut()) {
            f(t);
        }
        self.render_text_layer(i);
    }

    /// A new text layer with the property bar's settings, its first
    /// baseline starting at (x, y), typed into at once.
    pub fn start_text(&mut self, x: f32, y: f32) {
        self.end_text();
        self.remember();
        self.text_undo_open = true;
        let mut t = self.text_opts.clone();
        t.text.clear();
        t.at = (x + self.pi.origin.0 as f32, y + self.pi.origin.1 as f32);
        self.pi.add_layer(tr("pixel.text.layer"));
        let i = self.pi.active;
        if let Some(l) = self.pi.layers.get_mut(i) {
            l.text = Some(t);
        }
        self.text_edit = Some(TextEditing { layer: i, focus: true });
        self.mark_all();
    }

    /// Typing into the text layer `i` (it is used).
    pub fn edit_text(&mut self, i: usize) {
        if self.pi.layers.get(i).is_none_or(|l| l.text.is_none()) {
            return;
        }
        if self.text_edit.is_some_and(|e| e.layer == i) {
            return;
        }
        self.end_text();
        self.pi.set_active(i);
        self.text_edit = Some(TextEditing { layer: i, focus: true });
        self.mark_all();
    }

    /// The typing ends: a text layer left empty goes; one still named as
    /// new takes its first words as its name.
    pub fn end_text(&mut self) {
        let Some(e) = self.text_edit.take() else {
            return;
        };
        self.text_undo_open = false;
        let Some(l) = self.pi.layers.get(e.layer) else {
            return;
        };
        let Some(t) = l.text.as_ref() else {
            return;
        };
        if t.text.trim().is_empty() {
            if self.pi.layers.len() > 1 {
                self.pi.set_active(e.layer);
                self.pi.delete_layer();
            } else if let Some(l) = self.pi.layers.get_mut(e.layer) {
                l.text = None;
            }
            self.mark_all();
            return;
        }
        if l.name == tr("pixel.text.layer") {
            let first: String = t.text.lines().next().unwrap_or("").trim().chars().take(24).collect();
            if !first.is_empty() {
                if let Some(l) = self.pi.layers.get_mut(e.layer) {
                    l.name = first;
                }
            }
        }
    }

    /// The topmost shown text layer whose letters are under (x, y), or
    /// whose box is, a few pixels round.
    pub fn text_layer_at(&self, x: f32, y: f32, slack: f32) -> Option<usize> {
        (0..self.pi.layers.len()).rev().find(|&i| {
            let Some(l) = self.pi.layers.get(i) else {
                return false;
            };
            if l.text.is_none() || !l.visible {
                return false;
            }
            let Some(px) = self.pi.layer_pixels(i) else {
                return false;
            };
            crate::pixel_text::shown_box(px).is_some_and(|b| {
                x >= b[0] as f32 - slack
                    && y >= b[1] as f32 - slack
                    && x <= b[2] as f32 + slack
                    && y <= b[3] as f32 + slack
            })
        })
    }

    /// New selections from the selection tools: feathered by the tool's
    /// feather (#4393), joined to the one there is.
    pub fn select_new(&mut self, m: image::GrayImage, mode: SelectMode) {
        let f = self.opts.sel_feather.round() as u32;
        let m = if f > 0 { pixel_edit::feather(&m, f) } else { m };
        let sel = pixel_edit::combine(self.pi.sel.as_ref(), m, mode);
        self.set_selection(sel);
    }

    /// Run an operation on the whole picture (every layer: crop, flip,
    /// the layer commands) as one undo step.
    pub fn apply_all(
        &mut self,
        full: bool,
        f: impl FnOnce(&mut PixelImage) -> Option<Dirty>,
    ) -> bool {
        self.lights.bake();
        let before = self.pi.clone();
        let dims = self.pi.img.dimensions();
        match f(&mut self.pi) {
            Some(b) => {
                self.history.push(&before);
                self.changed = true;
                self.preview = None;
                self.placed_for = None;
                if full || self.pi.img.dimensions() != dims {
                    self.mark_all();
                } else {
                    self.mark(Some(b));
                }
                true
            }
            None => {
                self.pi = before;
                false
            }
        }
    }

    /// Quick mask on or off: the selection painted with the brush (red
    /// where it leaves out). Turned on with no selection, nothing is
    /// selected to start with; turned off with nothing painted, there is
    /// no selection.
    pub fn set_quick_mask(&mut self, on: bool) {
        if on == (self.target == MaskTarget::QuickMask) {
            return;
        }
        if on {
            if self.pi.sel.is_none() {
                let (w, h) = self.pi.img.dimensions();
                self.set_selection(Some(image::GrayImage::new(w, h)));
            }
            self.target = MaskTarget::QuickMask;
        } else {
            self.target = MaskTarget::Pixels;
            if self
                .pi
                .sel
                .as_ref()
                .is_some_and(|m| m.pixels().all(|v| v[0] == 0))
            {
                self.pi.sel = None;
            }
        }
        self.mask_stroke = None;
        self.view.borrow_mut().sel_changed = true;
    }

    /// Painting the mask of the layer in use on or off.
    pub fn set_edit_layer_mask(&mut self, on: bool) {
        let has = self
            .pi
            .layers
            .get(self.pi.active)
            .is_some_and(|l| l.mask.is_some());
        self.target = if on && has {
            MaskTarget::LayerMask
        } else if self.target == MaskTarget::QuickMask {
            MaskTarget::QuickMask
        } else {
            MaskTarget::Pixels
        };
        self.mask_stroke = None;
        self.view.borrow_mut().sel_changed = true;
    }

    /// The mask being painted, as it is.
    pub fn target_mask(&self) -> Option<&image::GrayImage> {
        if let Some(m) = &self.remove {
            return Some(m);
        }
        match self.target {
            MaskTarget::Pixels => None,
            MaskTarget::QuickMask => self.pi.sel.as_ref(),
            MaskTarget::LayerMask => self.pi.layers.get(self.pi.active)?.mask.as_ref(),
        }
    }

    /// The mask stroke taken to (x, y).
    fn mask_to(&mut self, x: f32, y: f32) -> Option<Dirty> {
        let ms = self.mask_stroke.as_mut()?;
        if let Some(m) = self.remove.as_mut() {
            return ms.to(m, None, x, y);
        }
        match self.target {
            MaskTarget::Pixels => None,
            MaskTarget::QuickMask => ms.to(self.pi.sel.as_mut()?, None, x, y),
            MaskTarget::LayerMask => {
                let PixelImage {
                    layers,
                    sel,
                    active,
                    ..
                } = &mut self.pi;
                let m = layers.get_mut(*active)?.mask.as_mut()?;
                ms.to(m, sel.as_ref(), x, y)
            }
        }
    }

    /// A warp grid over the layer in use: over the selection's box, else
    /// over the layer's pixels that are not clear, else the picture.
    pub fn start_warp(&mut self) {
        let (w, h) = self.pi.img.dimensions();
        if w == 0 || h == 0 {
            return;
        }
        let b = if self.pi.sel.is_some() {
            self.pi.sel_bounds()
        } else {
            let mut b: Option<Dirty> = None;
            for (x, y, q) in self.pi.img.enumerate_pixels() {
                if q[3] > 0 {
                    b = Some(pixel_edit::union(b, [x, y, x + 1, y + 1]));
                }
            }
            b
        }
        .unwrap_or([0, 0, w, h]);
        self.warp_box = b;
        self.warp = Some(crate::pixel_warp::Warp::new(
            self.pi.img.clone(),
            b,
            self.warp_grid,
            self.warp_grid,
        ));
        self.effect = None;
        self.preview = None;
    }

    /// The warp as it is now shown in place of the layer in use.
    /// The relief of what is painted on: what is beneath the layer in
    /// use, or the layer itself when it is the bottom one.
    pub fn surface_map(&self) -> crate::pixel_surface::SurfaceMap {
        let seen = if self.pi.active == 0 {
            self.pi.composite()
        } else {
            crate::pixel_surface::seen_beneath(&self.pi)
        };
        crate::pixel_surface::SurfaceMap::new(&seen, self.depth.as_ref())
    }

    pub fn refresh_warp(&mut self) {
        if let Some(w) = &self.warp {
            self.preview = Some(w.render(self.warp_box));
            self.view.borrow_mut().full = true;
        }
    }

    /// The warp put into the layer's pixels, one undo step.
    pub fn apply_warp(&mut self) {
        let Some(w) = self.warp.take() else {
            return;
        };
        let out = w.render(self.warp_box);
        let b = self.warp_box;
        self.preview = None;
        self.apply(true, |pi| {
            pi.img = out;
            Some(b)
        });
        self.view.borrow_mut().full = true;
    }

    /// The warp left off; the layer as it was.
    pub fn cancel_warp(&mut self) {
        if self.warp.take().is_some() {
            self.preview = None;
            self.view.borrow_mut().full = true;
        }
    }

    /// Ctrl+C, Ctrl+X: the selected pixels of the layer in use kept to be
    /// pasted where they were, and put on the system clipboard as a
    /// picture; a cut clears them, one undo step. False when nothing that
    /// shows is selected (or the layer is locked, for a cut).
    pub fn copy_pixels(&mut self, cut: bool) -> bool {
        let Some((img, at)) = pixel_edit::copy_selection(&self.pi) else {
            return false;
        };
        if cut && !self.apply(false, pixel_edit::clear_selection) {
            return false;
        }
        crate::clipboard::write_image(img.clone());
        self.clip = Some((img, at));
        true
    }

    /// Ctrl+V: what Ctrl+C or Ctrl+X took, pasted as a new layer where it
    /// was taken from; a picture copied in another program goes at the
    /// selection's top left. Nothing is selected after, so the Move tool
    /// takes the pasted pixels (#4263).
    pub fn paste_pixels(&mut self, name: String) -> bool {
        let sys = crate::clipboard::read_system().image;
        let (img, at) = match (self.clip.as_ref(), sys) {
            (Some((c, at)), Some(s)) if s.dimensions() == c.dimensions() => (c.clone(), *at),
            (Some((c, at)), None) => (c.clone(), *at),
            (_, Some(s)) => {
                let at = match self.pi.sel {
                    Some(_) => self.pi.sel_bounds().map(|b| (b[0], b[1])).unwrap_or((0, 0)),
                    None => (0, 0),
                };
                (s, at)
            }
            (None, None) => return false,
        };
        self.apply_all(true, |pi| pixel_edit::paste_layer_at(pi, name, &img, at))
    }

    /// The effect being set up, run on the picture inside the selection,
    /// for the preview.
    pub fn refresh_preview(&mut self) {
        self.preview = None;
        self.preview_rev = self.rev.get();
        // Just applied: the picture shows its result, not the effect run
        // on it once more (#4376).
        if !self.preview_on || self.effect_applied {
            self.view.borrow_mut().full = true;
            return;
        }
        if let Some(e) = &self.effect {
            let mut copy = self.pi.clone();
            if pixel_edit::apply_effect(&mut copy, e).is_some() {
                self.preview = Some(copy.img);
            }
        }
        self.view.borrow_mut().full = true;
    }
}

/// What the Move tool keeps of a layer it moved (see
/// [`PixelEdit::move_memory`]).
#[derive(Clone)]
pub struct MoveMemory {
    pub layer: usize,
    pub source: image::RgbaImage,
    pub steps: Vec<(Dirty, crate::pixel_ops::Placement)>,
    pub rev: u64,
}

impl App {
    /// Copy or cut the selected pixels of the picture being edited.
    pub fn pixel_copy(&mut self, cut: bool) {
        let Some(pe) = self.pixel_edit.as_mut() else {
            return;
        };
        if pe.target == MaskTarget::Pixels || !cut {
            if pe.copy_pixels(cut) {
                self.status = tr("pixel.status.copied");
                return;
            }
        }
        self.status = if cut && pe.pi.active_locked() {
            tr("pixel.status.layer_locked")
        } else {
            tr("pixel.status.nothing_to_copy")
        };
    }

    /// Paste what was copied as a new layer of the picture being edited.
    pub fn pixel_paste(&mut self) {
        let Some(pe) = self.pixel_edit.as_mut() else {
            return;
        };
        let name = crate::ui::pixel_bar::new_layer_name(pe);
        if !pe.paste_pixels(name) {
            self.status = tr("pixel.status.nothing_to_paste");
        }
    }

    /// Open the picture `id` for editing in place (Bitmaps > Edit Inside,
    /// a double click on it). False when it is not a picture or its
    /// pixels cannot be read.
    pub fn edit_bitmap_inside(&mut self, id: ShapeId) -> bool {
        if self.pixel_edit.is_some() {
            self.finish_pixel_edit_now();
        }
        let Some(s) = self.doc().find_shape(id).cloned() else {
            return false;
        };
        if s.locked {
            self.status = tr("pixel.status.locked");
            return false;
        }
        let ShapeKind::Bitmap {
            rect,
            width_px,
            height_px,
            png,
            ..
        } = &s.kind
        else {
            return false;
        };
        let Some(img) = crate::bitmap_fx::decode(png) else {
            self.status = tr("pixel.status.unreadable");
            return false;
        };
        let parent = self.doc().parent_transform(id).unwrap_or(Affine::IDENTITY);
        let (w0, h0) = ((*width_px).max(1) as f64, (*height_px).max(1) as f64);
        let img_to_page = parent
            * s.transform
            * Affine::new([
                rect.width() / w0,
                0.0,
                0.0,
                -rect.height() / h0,
                rect.x0,
                rect.y1,
            ]);
        // The decoded picture may differ from the size the object says.
        let size_px = img.dimensions();
        let img_to_page = img_to_page
            * Affine::scale_non_uniform(w0 / size_px.0.max(1) as f64, h0 / size_px.1.max(1) as f64);
        // The layers kept from the last Finish on this picture, while
        // the picture is still what they were put together into (#4112).
        let mut pi = PixelImage::new(img.clone());
        let kept = self
            .pixel_kept
            .get(id, &png[..])
            .map(|k| (k.layers.clone(), k.active))
            .or_else(|| {
                let (_, text) = s.data.iter().find(|(k, _)| k == crate::pixel_keep::DATA_KEY)?;
                crate::pixel_keep::load(text, &png[..])
            });
        if let Some((layers, active)) = kept {
            crate::pixel_keep::restore(&mut pi, layers, active);
        }
        static SESSIONS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let session = SESSIONS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.pixel_edit = Some(Box::new(PixelEdit {
            session,
            shape: id,
            img_to_page,
            rect: *rect,
            size_px,
            original: std::rc::Rc::new(img),
            liquify: None,
            spray: None,
            placement: Default::default(),
            smart_size: (100.0, 100.0),
            depth: None,
            remove: None,
            adjust_drag: false,
            pi,
            history: History::default(),
            tool: PixelTool::Brush,
            opts: self.pixel_opts,
            stroke: None,
            drag_from: None,
            drag_to: None,
            lasso: Vec::new(),
            pen: Vec::new(),
            pen_drag: false,
            move_drag: None,
            placed_for: None,
            move_memory: None,
            rev: Default::default(),
            pick_cache: Default::default(),
            preview_rev: 0,
            clip: None,
            layer_drag: false,
            target: MaskTarget::Pixels,
            mask_stroke: None,
            saved_sels: Vec::new(),
            warp: None,
            warp_box: [0, 0, 0, 0],
            warp_grid: 4,
            clone_src: None,
            clone_offset: None,
            changed: false,
            effect: None,
            preview_on: true,
            effect_applied: false,
            preview: None,
            feather_px: 4,
            grow_px: 2,
            ws: Default::default(),
            ws_area: egui::Rect::NOTHING,
            pointer_px: None,
            main_dockers: None,
            group_pick: Vec::new(),
            view: RefCell::new(PixelTextures {
                full: true,
                sel_changed: true,
                ..Default::default()
            }),
            hidden: RefCell::new(None),
            lights: Default::default(),
            text_edit: None,
            text_drag: None,
            text_opts: crate::pixel_text::PixelText {
                color: self.pixel_opts.color,
                ..Default::default()
            },
            text_undo_open: false,
            px_ask: None,
            ask_px: Default::default(),
            stroke_at: Default::default(),
            style_drag: false,
        }));
        // No selection handles over the pixels being edited.
        self.selection.clear();
        self.status = tr("pixel.status.editing");
        crate::ui::pixel_mode::enter(self);
        true
    }

    /// Finish editing the picture: its pixels go back into it in one undo
    /// step (nothing changes when nothing was changed). When the layers
    /// would be put together for good (kept neither in memory nor in the
    /// file), ask first (#4112).
    pub fn finish_pixel_edit(&mut self) {
        // A text layer being typed into is done (an empty one goes).
        if let Some(pe) = self.pixel_edit.as_mut() {
            pe.end_text();
        }
        let Some(pe) = self.pixel_edit.as_ref() else {
            return;
        };
        if pe.changed
            && pe.pi.is_layered()
            && !self.settings.pixel_keep_layers
            && !self.settings.pixel_save_layers
            && self.settings.warnings.pixel_flatten
        {
            self.dialog = crate::ui::dialogs::Dialog::ConfirmPixelFlatten { dont_ask: false };
            return;
        }
        self.finish_pixel_edit_now();
    }

    /// [`Self::finish_pixel_edit`] without asking.
    pub fn finish_pixel_edit_now(&mut self) {
        let Some(pe) = self.pixel_edit.take() else {
            return;
        };
        crate::ui::pixel_mode::leave(self, &pe);
        self.pixel_opts = pe.opts;
        if self.doc().find_shape(pe.shape).is_some() {
            self.selection = vec![pe.shape];
        }
        if !pe.changed {
            self.status.clear();
            return;
        }
        let Some(s) = self.doc().find_shape(pe.shape).cloned() else {
            return;
        };
        // The layers put together: what was seen goes back in the picture.
        let pixels = if pe.pi.is_layered() {
            pe.pi.composite()
        } else {
            pe.pi.img.clone()
        };
        let Some(png) = crate::bitmap_fx::encode(&pixels) else {
            self.status = tr("pixel.status.unreadable");
            return;
        };
        // The layers kept while the drawing is open, and in its file when
        // asked; a picture of one plain layer keeps none.
        let layered = pe.pi.is_layered();
        if layered && self.settings.pixel_keep_layers {
            self.pixel_kept.keep(pe.shape, &png, &pe.pi);
        } else {
            self.pixel_kept.forget(pe.shape);
        }
        let saved = (layered && self.settings.pixel_save_layers)
            .then(|| crate::pixel_keep::save(&pe.pi, &png))
            .flatten();
        let (w, h) = pixels.dimensions();
        // The box of the pixels as they are now, in the object's own
        // space: a crop keeps the pixels where they were.
        let sx = pe.rect.width() / pe.size_px.0.max(1) as f64;
        let sy = pe.rect.height() / pe.size_px.1.max(1) as f64;
        let x0 = pe.rect.x0 + pe.pi.origin.0 as f64 * sx;
        let y1 = pe.rect.y1 - pe.pi.origin.1 as f64 * sy;
        let rect = Rect::new(x0, y1 - h as f64 * sy, x0 + w as f64 * sx, y1);
        let mut cmds = vec![Command::SetShapeKind {
            shape: pe.shape,
            kind: ShapeKind::Bitmap {
                rect,
                width_px: w,
                height_px: h,
                png: png.into(),
                fx: None,
            },
        }];
        // The inks a CMYK picture printed from no longer fit its pixels.
        if s.picture_inks.is_some() {
            cmds.push(Command::SetPictureInks {
                shape: pe.shape,
                cmyk: s.cmyk_bitmap,
                inks: None,
            });
        }
        // A linked picture would be read again from its file over the
        // edit: it stands alone now. Layers saved before give way to the
        // ones of this edit (or to none).
        let mut data = s.data.clone();
        data.retain(|(k, _)| k != "bitmap.link" && k != crate::pixel_keep::DATA_KEY);
        if let Some(text) = saved {
            data.push((crate::pixel_keep::DATA_KEY.to_string(), text));
        }
        if data != s.data {
            cmds.push(Command::SetObjectData {
                shape: pe.shape,
                data,
            });
        }
        match self.engine.run_batch(LABEL, &cmds) {
            Ok(_) => self.status = tr("pixel.status.finished"),
            Err(e) => self.status = e.to_string(),
        }
    }

    /// Leave the picture as it was.
    pub fn cancel_pixel_edit(&mut self) {
        if let Some(pe) = self.pixel_edit.take() {
            crate::ui::pixel_mode::leave(self, &pe);
            self.pixel_opts = pe.opts;
            if self.doc().find_shape(pe.shape).is_some() {
                self.selection = vec![pe.shape];
            }
        }
        self.status = tr("pixel.status.cancelled");
    }

    /// The drawing with the picture being edited hidden: the page drawn
    /// under the pixels being edited. Made once per change of the drawing.
    pub fn pixel_edit_doc(&self) -> Option<std::rc::Rc<Document>> {
        let pe = self.pixel_edit.as_ref()?;
        let rev = self.engine.revision();
        if let Some((r, doc)) = pe.hidden.borrow().as_ref() {
            if *r == rev {
                return Some(doc.clone());
            }
        }
        let mut doc = self.doc().clone();
        doc.find_shape_mut(pe.shape)?.visible = false;
        let doc = std::rc::Rc::new(doc);
        *pe.hidden.borrow_mut() = Some((rev, doc.clone()));
        Some(doc)
    }

    /// The pointer on the drawing while a picture is edited: the picture
    /// tools take every press. `pointer` is in page space.
    pub fn pixel_edit_input(
        &mut self,
        response: &Response,
        pointer: Option<Point>,
        mods: Modifiers,
    ) {
        let Some(pe) = self.pixel_edit.as_ref() else {
            return;
        };
        let px = pointer.map(|p| pe.to_px(p));
        let s0 = self.view.to_screen(pe.to_page(0.0, 0.0));
        let s1 = self.view.to_screen(pe.to_page(1.0, 0.0));
        self.pixel_px_input(response, px, mods, (s1 - s0).length());
    }

    /// The pointer over the picture being edited, at (x, y) in its pixels,
    /// shown `screen_per_px` screen points to a pixel: the picture tools
    /// take every press.
    pub fn pixel_px_input(
        &mut self,
        response: &Response,
        px: Option<(f32, f32)>,
        mods: Modifiers,
        screen_per_px: f32,
    ) {
        response.ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
        let Some(pe) = self.pixel_edit.as_mut() else {
            return;
        };
        // The Move tool's preview goes with the tool; the effect open in
        // the Effects panel shows on the pixels as they are once nothing
        // is being dragged.
        if pe.tool != PixelTool::Move {
            pe.sync_place_preview();
        }
        let busy = response.is_pointer_button_down_on();
        pe.keep_preview_current(busy);
        // A run of changes to a text layer's settings ends once nothing
        // is held and nothing is typed into it.
        if pe.text_edit.is_none() && !response.ctx.input(|i| i.pointer.any_down()) {
            pe.text_undo_open = false;
        }
        // The typing ends when another tool is taken.
        if pe.tool != PixelTool::Text && pe.text_edit.is_some() {
            pe.end_text();
        }
        let Some((x, y)) = px else {
            if response.drag_stopped() {
                pe.lights.end_stroke();
                if let Some(d) = pe.move_drag.take() {
                    pe.apply_placement(Some(d.b));
                }
                pixel_release(pe, mods);
            }
            return;
        };
        let started = response.drag_started_by(PointerButton::Primary);
        let dragged = response.dragged_by(PointerButton::Primary);
        let stopped = response.drag_stopped_by(PointerButton::Primary);
        let clicked = response.clicked_by(PointerButton::Primary);
        let (w, h) = pe.pi.img.dimensions();
        let inside = x >= 0.0 && y >= 0.0 && x < w as f32 && y < h as f32;
        let mode = select_mode(pe.opts.sel_mode, mods);
        let tool = pe.tool;
        // The Text tool: a click on a text layer types into it, a drag
        // of one moves it, a click elsewhere starts a new one (#4370).
        if tool == PixelTool::Text {
            let slack = 4.0 / screen_per_px.max(1e-6);
            let pressed = response.contains_pointer()
                && response.ctx.input(|i| i.pointer.primary_pressed());
            if pressed {
                pe.text_drag = None;
                if let Some(i) = pe.text_layer_at(x, y, slack) {
                    let at = pe.pi.layers.get(i).and_then(|l| l.text.as_ref()).map(|t| t.at);
                    if let Some(at) = at {
                        pe.edit_text(i);
                        pe.text_drag = Some(((x, y), at));
                    }
                }
            }
            if dragged {
                if let Some(((px0, py0), at)) = pe.text_drag {
                    let (dx, dy) = (x - px0, y - py0);
                    if dx.abs() + dy.abs() > 0.5 {
                        pe.change_text(|t| t.at = (at.0 + dx, at.1 + dy));
                    }
                }
                response.ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
            } else if pe.text_layer_at(x, y, slack).is_some() {
                response.ctx.set_cursor_icon(egui::CursorIcon::Move);
            } else {
                response.ctx.set_cursor_icon(egui::CursorIcon::Text);
            }
            if clicked && pe.text_drag.is_none() && inside {
                pe.start_text(x, y);
            } else if clicked {
                if let Some(e) = pe.text_edit.as_mut() {
                    e.focus = true;
                }
            }
            if stopped || clicked {
                pe.text_drag = None;
            }
            return;
        }
        // Alt+click sets the source of the clone and heal brushes.
        if matches!(tool, PixelTool::Clone | PixelTool::Heal) && mods.alt {
            if clicked || started {
                pe.clone_src = Some((x, y));
                pe.clone_offset = None;
                self.status = tr("pixel.status.source_set");
            }
            return;
        }
        // Painting a mask: the brush and the eraser only.
        if pe.target != MaskTarget::Pixels
            && (tool.is_brush()
                || matches!(
                    tool,
                    PixelTool::Fill
                        | PixelTool::Gradient
                        | PixelTool::Move
                        | PixelTool::Shape
                        | PixelTool::Sprayer
                        | PixelTool::LayerFade
                        | PixelTool::ColorToClear
                ) || tool.is_liquify())
        {
            if !matches!(tool, PixelTool::Brush | PixelTool::Eraser) {
                if started || clicked {
                    self.status = tr("pixel.status.mask_tools");
                }
                return;
            }
            if pe.target == MaskTarget::LayerMask && pe.pi.active_lock_box() {
                if started || clicked {
                    self.status = tr("pixel.status.layer_locked");
                }
                return;
            }
            if started || (clicked && pe.mask_stroke.is_none()) {
                let value = match (pe.target, tool) {
                    (MaskTarget::QuickMask, PixelTool::Brush) => 255,
                    (MaskTarget::QuickMask, _) => 0,
                    (_, PixelTool::Brush) => 0,
                    _ => 255,
                };
                pe.step();
                let Some(m) = pe.target_mask() else {
                    return;
                };
                pe.mask_stroke = Some(MaskStroke::new(pe.opts, m, value));
                let b = pe.mask_to(x, y);
                pe.mark(b);
                if clicked {
                    pe.mask_stroke = None;
                }
            } else if dragged {
                let b = pe.mask_to(x, y);
                pe.mark(b);
            }
            if stopped {
                pe.mask_stroke = None;
                pe.view.borrow_mut().sel_changed = true;
            }
            return;
        }
        // A locked layer keeps its pixels: only the selections and the
        // eyedropper work on it.
        let paints = tool.is_brush()
            || tool == PixelTool::Light
            || matches!(
                tool,
                PixelTool::Fill
                    | PixelTool::Gradient
                    | PixelTool::Move
                    | PixelTool::Warp
                    | PixelTool::Shape
                    | PixelTool::Remove
                    | PixelTool::Sprayer
                    | PixelTool::LayerFade
                    | PixelTool::ColorToClear
            )
            || tool.is_liquify();
        // Painting on a text layer makes it plain pixels first (one
        // undo step back brings the text again).
        if paints && tool != PixelTool::Move && (started || clicked) && pe.active_text().is_some() && !pe.pi.active_locked() {
            pe.remember();
            let i = pe.pi.active;
            if let Some(l) = pe.pi.layers.get_mut(i) {
                l.text = None;
            }
            self.status = tr("pixel.status.text_to_pixels");
        }
        if paints && pe.pi.active_locked() {
            if started || clicked {
                // An adjustment layer has no pixels to paint: say so,
                // rather than that it is locked (#4264).
                self.status = if pe.pi.active_lock_box() {
                    tr("pixel.status.layer_locked")
                } else {
                    tr("pixel.status.adjust_layer")
                };
            }
            return;
        }
        if tool == PixelTool::Warp {
            let pressed = response.contains_pointer()
                && response.ctx.input(|i| i.pointer.primary_pressed());
            if pe.warp.is_none() && (pressed || started || clicked) {
                pe.start_warp();
            }
            // The nearest point within a few screen points of the press is
            // taken, where the button went down rather than where the drag
            // was first noticed, so a quick drag still takes it (#4116).
            let grab = crate::pixel_warp::GRAB_RADIUS / screen_per_px.max(1e-6);
            if let Some(w) = pe.warp.as_mut() {
                if pressed {
                    w.take(x, y, grab);
                } else if dragged {
                    w.drag_to(x, y);
                }
                let cursor = if w.drag.is_some() && dragged {
                    Some(egui::CursorIcon::Grabbing)
                } else if w.pick(x, y, grab).is_some() {
                    Some(egui::CursorIcon::Grab)
                } else {
                    None
                };
                if let Some(c) = cursor {
                    response.ctx.set_cursor_icon(c);
                }
            }
            if stopped {
                if let Some(w) = pe.warp.as_mut() {
                    w.drag = None;
                }
                pe.refresh_warp();
            }
            return;
        }
        if tool == PixelTool::Move {
            move_input(pe, response, x, y, mods, screen_per_px);
            return;
        }
        if tool == PixelTool::Light {
            use crate::pixel_light::LightEvent;
            let ev = if started || (clicked && !dragged) {
                Some(if started {
                    LightEvent::Press
                } else {
                    LightEvent::Click
                })
            } else if dragged {
                Some(LightEvent::Drag)
            } else {
                None
            };
            if let Some(ev) = ev {
                pe.light_input(x, y, ev, mods.alt);
            }
            if stopped {
                pe.light_input(x, y, LightEvent::Release, mods.alt);
            }
            return;
        }
        if tool == PixelTool::Remove {
            if started || (clicked && pe.remove.is_none()) {
                let (w, h) = pe.pi.img.dimensions();
                let m = image::GrayImage::from_pixel(w, h, image::Luma([255]));
                pe.mask_stroke = Some(MaskStroke::new(pe.opts, &m, 0));
                pe.remove = Some(m);
            }
            if started || dragged || clicked {
                let b = pe.mask_to(x, y);
                pe.mark(b);
            }
            if stopped || clicked {
                pe.mask_stroke = None;
                if let Some(m) = pe.remove.take() {
                    let grow_by = pe.opts.remove_grow.max(0.0).round() as u32;
                    pe.apply(false, |pi| pixel_edit::remove_painted(pi, &m, grow_by));
                }
                pe.view.borrow_mut().sel_changed = true;
            }
            return;
        }
        // The mask brush paints the selection itself.
        if tool == PixelTool::MaskBrush {
            let take = mods.alt || mods.command || mods.ctrl || mode == SelectMode::Subtract;
            if started || (clicked && pe.mask_stroke.is_none()) {
                pe.step();
                let (w, h) = pe.pi.img.dimensions();
                if mode == SelectMode::Replace && !mods.shift && pe.pi.sel.is_some() && started {
                    // A new stroke in Normal starts a new selection.
                    pe.pi.sel = Some(image::GrayImage::new(w, h));
                }
                let m = pe
                    .pi
                    .sel
                    .get_or_insert_with(|| image::GrayImage::new(w, h));
                pe.mask_stroke = Some(MaskStroke::new(pe.opts, m, if take { 0 } else { 255 }));
            }
            if started || dragged || clicked {
                if let (Some(ms), Some(m)) = (pe.mask_stroke.as_mut(), pe.pi.sel.as_mut()) {
                    ms.to(m, None, x, y);
                }
                pe.view.borrow_mut().sel_changed = true;
            }
            if stopped || clicked {
                pe.mask_stroke = None;
                pe.view.borrow_mut().sel_changed = true;
            }
            return;
        }
        if tool == PixelTool::Sprayer {
            if started || (clicked && pe.spray.is_none()) {
                pe.step();
                let mut sp = crate::pixel_spray::Spray::new(pe.opts);
                let b = sp.to(&mut pe.pi, x, y);
                pe.mark(b);
                pe.spray = (!clicked).then_some(sp);
            } else if dragged {
                let pe = &mut **pe;
                let b = match pe.spray.as_mut() {
                    Some(sp) => sp.to(&mut pe.pi, x, y),
                    None => None,
                };
                pe.mark(b);
            }
            if stopped {
                pe.spray = None;
            }
            return;
        }
        if let Some(lmode) = tool.liquify_mode(pe.opts.liquify, mods.alt) {
            if started || (clicked && pe.liquify.is_none()) {
                pe.step();
                let mut lq = crate::pixel_liquify::Liquify::new(lmode, pe.opts, &pe.pi);
                let b = lq.to(&mut pe.pi, x, y);
                pe.mark(b);
                pe.liquify = (!clicked).then_some(lq);
            } else if dragged {
                let pe = &mut **pe;
                let b = match pe.liquify.as_mut() {
                    Some(lq) => lq.to(&mut pe.pi, x, y),
                    None => None,
                };
                pe.mark(b);
            }
            if stopped {
                pe.liquify = None;
            }
            return;
        }
        if tool.is_brush() {
            if started || (clicked && pe.stroke.is_none()) {
                let offset = if matches!(tool, PixelTool::Clone | PixelTool::Heal) {
                    match (pe.clone_offset, pe.clone_src) {
                        (Some(o), _) => o,
                        (None, Some(src)) => {
                            let o = (src.0 - x, src.1 - y);
                            pe.clone_offset = Some(o);
                            o
                        }
                        (None, None) => {
                            self.status = tr("pixel.status.need_source");
                            return;
                        }
                    }
                } else {
                    (0.0, 0.0)
                };
                pe.step();
                let mut s = Stroke::new(tool, pe.opts, &pe.pi, offset);
                if matches!(tool, PixelTool::Clone | PixelTool::Heal) {
                    // The source turned, scaled and mirrored, from the
                    // layer or what is seen (#4394).
                    let o = pe.opts;
                    let src = pe.clone_src.unwrap_or((x + offset.0, y + offset.1));
                    let anchor = if o.clone_aligned {
                        (src.0 - offset.0, src.1 - offset.1)
                    } else {
                        (x, y)
                    };
                    let xform = pixel_edit::CloneXform {
                        anchor,
                        src,
                        angle: o.clone_angle,
                        scale: o.clone_scale,
                        flip_h: o.clone_flip_h,
                        flip_v: o.clone_flip_v,
                    };
                    let sample = match o.clone_sample {
                        pixel_edit::CloneSample::Layer => None,
                        pixel_edit::CloneSample::Below => Some(std::rc::Rc::new(pe.pi.composite_below())),
                        pixel_edit::CloneSample::All => Some(std::rc::Rc::new(pe.pi.composite())),
                    };
                    s = s.with_clone(xform, sample);
                }
                if tool == PixelTool::Restore {
                    s = s.with_original(pe.original.clone());
                }
                if pe.opts.surface && matches!(tool, PixelTool::Brush | PixelTool::Clone) {
                    s = s.with_surface(std::rc::Rc::new(pe.surface_map()));
                }
                let b = s.to(&mut pe.pi, x, y);
                pe.mark(b);
                pe.stroke = Some(s);
                if clicked {
                    pe.stroke = None;
                }
            } else if dragged {
                let pe = &mut **pe;
                let b = match pe.stroke.as_mut() {
                    Some(s) => s.to(&mut pe.pi, x, y),
                    None => None,
                };
                pe.mark(b);
            }
            if stopped {
                pe.stroke = None;
            }
            return;
        }
        match tool {
            PixelTool::RectSelect | PixelTool::EllipseSelect
                if clicked && pe.opts.sel_fixed && inside =>
            {
                // A box of the width and height chosen, from the click.
                let (fw, fh) = pe.opts.sel_size;
                pe.drag_from = Some((x, y));
                pe.drag_to = Some((x + fw.max(1.0), y + fh.max(1.0)));
                pixel_release(pe, mods);
            }
            PixelTool::RectSelect
            | PixelTool::EllipseSelect
            | PixelTool::Gradient
            | PixelTool::Shape
            | PixelTool::BandMask
            | PixelTool::LayerFade => {
                if started {
                    pe.drag_from = Some((x, y));
                    pe.drag_to = Some((x, y));
                } else if dragged {
                    pe.drag_to = Some((x, y));
                } else if clicked && tool.is_selection() && mode == SelectMode::Replace {
                    // A click with no drag: nothing selected.
                    if pe.pi.sel.is_some() {
                        pe.set_selection(None);
                    }
                }
                if stopped {
                    pe.drag_to = Some((x, y));
                    pixel_release(pe, mods);
                }
            }
            PixelTool::FreehandMask => {
                // Clicks set corners, drags draw freehand; a double
                // click, or a click on the first corner, closes it.
                let near_first = pe.lasso.first().is_some_and(|f| {
                    (f.0 - x).hypot(f.1 - y) * screen_per_px.max(1e-6) <= 8.0
                });
                if response.double_clicked() || (clicked && near_first && pe.lasso.len() > 2) {
                    close_freehand(pe, mode);
                } else if clicked
                    || started
                    || (dragged
                        && pe
                            .lasso
                            .last()
                            .is_none_or(|l| (l.0 - x).abs() + (l.1 - y).abs() > 1.0))
                {
                    pe.lasso.push((x, y));
                }
            }
            PixelTool::PenMask => {
                // A press sets a point, a drag from it pulls its handles
                // (Alt the one going on only); a double click, or a
                // press on the first point, closes it.
                let spp = screen_per_px.max(1e-6);
                let near = |p: (f32, f32), r: f32| (p.0 - x).hypot(p.1 - y) * spp <= r;
                let pressed = response.contains_pointer()
                    && response.ctx.input(|i| i.pointer.primary_pressed());
                if response.double_clicked() {
                    pe.pen_drag = false;
                    if pe.pen.len() > 2 {
                        close_pen(pe, mode);
                    }
                } else if pressed {
                    if pe.pen.len() > 2 && pe.pen.first().is_some_and(|f| near(f.p, 8.0)) {
                        pe.pen_drag = false;
                        close_pen(pe, mode);
                    } else if pe.pen.last().is_some_and(|l| near(l.p, 3.0)) {
                        // The second press of a double click.
                        pe.pen_drag = false;
                    } else {
                        pe.pen.push(pixel_edit::PenNode::corner((x, y)));
                        pe.pen_drag = true;
                    }
                } else if dragged && pe.pen_drag {
                    if let Some(l) = pe.pen.last_mut() {
                        l.out = (x - l.p.0, y - l.p.1);
                        if !mods.alt {
                            l.inn = (-l.out.0, -l.out.1);
                        }
                    }
                }
                if stopped || clicked {
                    pe.pen_drag = false;
                }
            }
            PixelTool::EdgeMask => {
                let reach = pe.opts.magnet_reach;
                if started {
                    let p = crate::pixel_ops::snap_to_edge(&pe.pi.img, x, y, reach);
                    pe.lasso = vec![p];
                } else if dragged
                    && pe
                        .lasso
                        .last()
                        .is_none_or(|l| (l.0 - x).abs() + (l.1 - y).abs() > 2.0)
                {
                    let p = crate::pixel_ops::snap_to_edge(&pe.pi.img, x, y, reach);
                    pe.lasso.push(p);
                }
                if stopped {
                    let mode_now = mode;
                    close_freehand(pe, mode_now);
                }
            }
            PixelTool::ColorToClear if clicked && inside => {
                let o = pe.opts;
                pe.apply(false, |pi| {
                    crate::pixel_ops::color_to_clear(pi, x as u32, y as u32, &o)
                });
            }
            PixelTool::Lasso => {
                if started {
                    pe.lasso = vec![(x, y)];
                } else if dragged
                    && pe
                        .lasso
                        .last()
                        .is_none_or(|l| (l.0 - x).abs() + (l.1 - y).abs() > 1.0)
                {
                    pe.lasso.push((x, y));
                }
                if stopped {
                    pixel_release(pe, mods);
                }
            }
            PixelTool::MagicWand if clicked && inside => {
                if let Some(m) = pixel_edit::similar_mask(
                    &pe.pi.img,
                    x as u32,
                    y as u32,
                    pe.opts.tolerance,
                    pe.opts.contiguous,
                ) {
                    pe.select_new(m, mode);
                }
            }
            PixelTool::Fill if clicked && inside => {
                let o = pe.opts;
                pe.apply(false, |pi| {
                    pixel_edit::bucket_fill(pi, x as u32, y as u32, &o)
                });
            }
            PixelTool::Eyedropper if clicked && inside => {
                if let Some(q) = pe.pi.seen_at(x as u32, y as u32) {
                    pe.opts.color = [q[0], q[1], q[2], 255];
                }
            }
            _ => {}
        }
    }

    /// The keys while a picture is edited. True when a key was taken.
    pub fn pixel_edit_keys(&mut self, ctx: &egui::Context) {
        use egui::Key;
        let cmd = Modifiers::COMMAND;
        let pressed = |k: Key, m: Modifiers| ctx.input_mut(|i| i.consume_key(m, k));
        if pressed(Key::Enter, Modifiers::NONE) {
            // Enter closes a freehand outline, puts a warp under way in
            // the layer, else finishes.
            match self.pixel_edit.as_mut() {
                Some(pe) if pe.tool == PixelTool::FreehandMask && pe.lasso.len() > 2 => {
                    let mode = pe.opts.sel_mode;
                    close_freehand(pe, mode);
                }
                Some(pe) if pe.tool == PixelTool::PenMask && pe.pen.len() > 2 => {
                    let mode = pe.opts.sel_mode;
                    close_pen(pe, mode);
                }
                Some(pe) if pe.warp.is_some() => pe.apply_warp(),
                Some(pe) if pe.tool == PixelTool::Move && !pe.placement.is_identity() => {
                    pe.apply_placement(None)
                }
                _ => self.finish_pixel_edit(),
            }
            return;
        }
        // Copy, cut and paste: the windowing layer gives Ctrl+C, Ctrl+X
        // and Ctrl+V as copy, cut and paste events (a paste event only
        // while the system clipboard holds text), not key presses.
        let (copy_ev, cut_ev, paste_text) =
            ctx.input(|i| crate::interaction::clipboard_events(&i.events));
        let seen = &mut self.paste_seen;
        let paste_bare =
            ctx.input(|i| crate::interaction::paste_without_text(&i.events, seen).is_some());
        if pressed(Key::C, cmd) || copy_ev {
            self.pixel_copy(false);
            return;
        }
        if pressed(Key::X, cmd) || cut_ev {
            self.pixel_copy(true);
            return;
        }
        if pressed(Key::V, cmd) || paste_text.is_some() || paste_bare {
            self.pixel_paste();
            return;
        }
        // The zoom of the image window.
        use crate::ui::pixel_cmds::PixelCmd;
        if pressed(Key::R, cmd | Modifiers::SHIFT) {
            PixelCmd::TurnLeft.run(self);
            return;
        }
        if pressed(Key::R, cmd) {
            PixelCmd::TurnRight.run(self);
            return;
        }
        // The adjustments with keys of their own.
        for (k, m, id) in crate::ui::pixel_mode::ADJUST_KEYS {
            let m = if m { cmd | Modifiers::SHIFT } else { cmd };
            if pressed(k, m) {
                crate::ui::pixel_mode::open_effect(self, id);
                return;
            }
        }
        for (keys, c) in [
            (&[Key::Plus, Key::Equals][..], PixelCmd::ZoomIn),
            (&[Key::Minus][..], PixelCmd::ZoomOut),
            (&[Key::Num0][..], PixelCmd::ZoomFit),
            (&[Key::Num1][..], PixelCmd::Zoom100),
        ] {
            if keys.iter().any(|k| pressed(*k, cmd)) {
                c.run(self);
                return;
            }
        }
        let Some(pe) = self.pixel_edit.as_mut() else {
            return;
        };
        if pressed(Key::Escape, Modifiers::NONE) {
            // Esc leaves a warp off first, then drops the selection, then
            // ends the editing (an outline being clicked goes first).
            if !pe.lasso.is_empty() || !pe.pen.is_empty() {
                pe.lasso.clear();
                pe.pen.clear();
                pe.pen_drag = false;
            } else if pe.tool == PixelTool::Move && !pe.placement.is_identity() {
                pe.placement = Default::default();
                pe.move_drag = None;
                pe.sync_place_preview();
            } else if pe.warp.is_some() {
                pe.cancel_warp();
            } else if pe.pi.sel.is_some() {
                pe.set_selection(None);
            } else {
                self.finish_pixel_edit();
            }
            return;
        }
        if pressed(Key::Z, cmd | Modifiers::SHIFT) || pressed(Key::Y, cmd) {
            pe.redo();
        } else if pressed(Key::Z, cmd) {
            pe.undo();
        } else if pressed(Key::A, cmd) {
            pe.set_selection(None);
            let (w, h) = pe.pi.img.dimensions();
            pe.pi.sel = Some(image::GrayImage::from_pixel(w, h, image::Luma([255])));
        } else if pressed(Key::D, cmd) {
            pe.set_selection(None);
        } else if pressed(Key::I, cmd | Modifiers::SHIFT) {
            let (w, h) = pe.pi.img.dimensions();
            let inv = pixel_edit::invert(pe.pi.sel.as_ref(), w, h);
            pe.set_selection(Some(inv));
        } else if !pe.pen.is_empty() && pressed(Key::Backspace, Modifiers::NONE) {
            // Backspace takes the pen outline's last point back.
            pe.pen.pop();
            pe.pen_drag = false;
        } else if pressed(Key::Delete, Modifiers::NONE) || pressed(Key::Backspace, Modifiers::NONE)
        {
            pe.apply(false, pixel_edit::clear_selection);
        } else if pressed(Key::Q, Modifiers::SHIFT) {
            let on = pe.target != MaskTarget::QuickMask;
            pe.set_quick_mask(on);
        } else if pressed(Key::OpenBracket, Modifiers::NONE) {
            pe.opts.size = (pe.opts.size / 1.2).max(1.0);
        } else if pressed(Key::CloseBracket, Modifiers::NONE) {
            pe.opts.size = (pe.opts.size * 1.2).min(2000.0);
        } else {
            for (t, k, shift) in crate::ui::pixel_mode::SHORTCUTS {
                let m = if shift { Modifiers::SHIFT } else { Modifiers::NONE };
                if pressed(k, m) {
                    crate::ui::pixel_mode::choose_tool(pe, t);
                    break;
                }
            }
        }
    }
}

/// The Move tool's box round what it moves: its outline, the square
/// handles that scale and the round one above that turns (#4261).
fn draw_move_box(pe: &PixelEdit, painter: &egui::Painter, map: &dyn Fn(f32, f32) -> egui::Pos2) {
    use crate::pixel_ops;
    let b = pe.move_drag.map(|d| d.b).unwrap_or_else(|| pe.move_box());
    let p = pe.placement;
    let spp = map(0.0, 0.0).distance(map(1.0, 0.0)).max(1e-6);
    let to_screen = |q: (f32, f32)| map(q.0, q.1);
    let blue = egui::Color32::from_rgb(0x1E, 0x7C, 0xE8);
    let corners = pixel_ops::placed_corners(b, &p).map(to_screen);
    let mut ring = corners.to_vec();
    ring.push(corners[0]);
    painter.add(egui::Shape::line(ring.clone(), egui::Stroke::new(3.0, egui::Color32::from_white_alpha(160))));
    painter.add(egui::Shape::line(ring, egui::Stroke::new(1.0, blue)));
    let (knob, top) = pixel_ops::rotate_handle(b, &p, BOX_KNOB / spp);
    let (knob, top) = (to_screen(knob), to_screen(top));
    painter.line_segment([top, knob], egui::Stroke::new(1.0, blue));
    painter.circle(knob, 4.5, egui::Color32::WHITE, egui::Stroke::new(1.2, blue));
    for (_, q) in pixel_ops::scale_handles(b, &p) {
        painter.rect(
            egui::Rect::from_center_size(to_screen(q), egui::vec2(7.0, 7.0)),
            0.0,
            egui::Color32::WHITE,
            egui::Stroke::new(1.2, blue),
            egui::StrokeKind::Middle,
        );
    }
}

/// The Move tool: a box with handles round the selected pixels (the
/// layer's pixels that are not clear with nothing selected). A drag inside
/// it, or anywhere away from its handles, moves them; the corners and the
/// middles of the sides scale (a corner keeps the width to the height
/// while Keep ratio is on, Shift the other way); the round handle above
/// it, or just outside a corner, turns (Shift in steps of 15 degrees).
/// The pixels follow the drag and go into the layer on release, one undo
/// step (#4261, #4263).
fn move_input(
    pe: &mut PixelEdit,
    response: &Response,
    x: f32,
    y: f32,
    mods: Modifiers,
    screen_per_px: f32,
) {
    use crate::pixel_ops::{self, BoxHandle};
    let spp = screen_per_px.max(1e-6);
    let (grab, knob) = (BOX_GRAB / spp, BOX_KNOB / spp);
    let pressed =
        response.contains_pointer() && response.ctx.input(|i| i.pointer.primary_pressed());
    let dragged = response.dragged_by(PointerButton::Primary);
    let released = response.drag_stopped_by(PointerButton::Primary)
        || response.clicked_by(PointerButton::Primary)
        || (pe.move_drag.is_some() && !response.ctx.input(|i| i.pointer.primary_down()));
    let b = pe.move_drag.map(|d| d.b).unwrap_or_else(|| pe.move_box());
    let hover = pe
        .move_drag
        .map(|d| d.handle)
        .unwrap_or_else(|| pixel_ops::box_hit(b, &pe.placement, x, y, grab, knob));
    response.ctx.set_cursor_icon(match hover {
        BoxHandle::Move => egui::CursorIcon::Move,
        BoxHandle::Rotate => egui::CursorIcon::Alias,
        BoxHandle::Scale(ix, iy) => {
            // The handle's direction once the box is turned.
            let a = (iy as f32).atan2(ix as f32) + pe.placement.angle.to_radians();
            let k = ((a.to_degrees().rem_euclid(180.0) + 22.5) / 45.0) as i32 % 4;
            match k {
                0 => egui::CursorIcon::ResizeHorizontal,
                1 => egui::CursorIcon::ResizeNwSe,
                2 => egui::CursorIcon::ResizeVertical,
                _ => egui::CursorIcon::ResizeNeSw,
            }
        }
    });
    if pressed && pe.move_drag.is_none() {
        pe.move_drag = Some(MoveDrag {
            handle: hover,
            from: (x, y),
            start: pe.placement,
            b,
        });
    } else if dragged {
        if let Some(d) = pe.move_drag {
            let lock = response
                .ctx
                .data(|m| m.get_temp::<bool>(egui::Id::new("pixel_place_lock")))
                .unwrap_or(true);
            pe.placement = pixel_ops::drag_box(
                d.b,
                &d.start,
                d.handle,
                d.from,
                (x, y),
                lock != mods.shift,
                mods.shift,
            );
        }
    }
    if released {
        if let Some(d) = pe.move_drag.take() {
            if pe.placement != d.start {
                pe.apply_placement(Some(d.b));
                return;
            }
        }
    }
    pe.sync_place_preview();
}

/// How a new selection joins the one there is: Shift adds, Ctrl takes
/// away, Shift and Ctrl keep where only one is; else the mode chosen on
/// the property bar.
pub fn select_mode(chosen: SelectMode, mods: Modifiers) -> SelectMode {
    let ctrl = mods.command || mods.ctrl;
    match (mods.shift, ctrl) {
        (true, true) => SelectMode::Xor,
        (true, false) => SelectMode::Add,
        (false, true) => SelectMode::Subtract,
        _ => chosen,
    }
}

/// Close the outline of clicked corners into a selection.
pub fn close_freehand(pe: &mut PixelEdit, mode: SelectMode) {
    let pts = std::mem::take(&mut pe.lasso);
    let (w, h) = pe.pi.img.dimensions();
    if let Some(m) = pixel_edit::lasso_mask(w, h, &pts) {
        pe.select_new(m, mode);
    }
}

/// Close the pen outline into a selection.
pub fn close_pen(pe: &mut PixelEdit, mode: SelectMode) {
    let nodes = std::mem::take(&mut pe.pen);
    pe.pen_drag = false;
    let (w, h) = pe.pi.img.dimensions();
    if let Some(m) = pixel_edit::lasso_mask(w, h, &pixel_edit::pen_polygon(&nodes, true)) {
        pe.select_new(m, mode);
    }
}

/// The end of a drag of a selection tool or the gradient.
fn pixel_release(pe: &mut PixelEdit, mods: Modifiers) {
    let mode = select_mode(pe.opts.sel_mode, mods);
    let (w, h) = pe.pi.img.dimensions();
    let (from, to) = (pe.drag_from.take(), pe.drag_to.take());
    let lasso = std::mem::take(&mut pe.lasso);
    match pe.tool {
        PixelTool::RectSelect | PixelTool::EllipseSelect => {
            let (Some(a), Some(b)) = (from, to) else {
                return;
            };
            if (a.0 - b.0).abs() < 1.0 || (a.1 - b.1).abs() < 1.0 {
                return;
            }
            let m = if pe.tool == PixelTool::RectSelect {
                pixel_edit::rect_mask(w, h, a, b)
            } else {
                pixel_edit::ellipse_mask(w, h, a, b)
            };
            if let Some(m) = m {
                pe.select_new(m, mode);
            }
        }
        PixelTool::Lasso => {
            if let Some(m) = pixel_edit::lasso_mask(w, h, &lasso) {
                pe.select_new(m, mode);
            }
        }
        PixelTool::Gradient => {
            let (Some(a), Some(b)) = (from, to) else {
                return;
            };
            if (a.0 - b.0).abs() + (a.1 - b.1).abs() < 1.0 {
                return;
            }
            let o = pe.opts;
            pe.apply(false, |pi| pixel_edit::gradient(pi, a, b, &o));
        }
        PixelTool::LayerFade => {
            let (Some(a), Some(b)) = (from, to) else {
                return;
            };
            let o = pe.opts;
            pe.apply(false, |pi| crate::pixel_ops::layer_fade(pi, a, b, &o));
        }
        PixelTool::BandMask => {
            let (Some(a), Some(b)) = (from, to) else {
                return;
            };
            if let Some(m) = crate::pixel_ops::band_mask(w, h, a, b, pe.opts.band_soft) {
                pe.select_new(m, mode);
            }
        }
        PixelTool::Shape => {
            let (Some(a), Some(b)) = (from, to) else {
                return;
            };
            if (a.0 - b.0).abs() + (a.1 - b.1).abs() < 1.0 {
                return;
            }
            let o = pe.opts;
            pe.apply(false, |pi| crate::pixel_paint::draw_shape(pi, a, b, &o));
        }
        _ => {}
    }
}

/// Draw the picture being edited over the page: its pixels as they are
/// (or the effect preview), the selection outline, the drag in progress
/// and the brush under the pointer.
pub fn draw(app: &App, painter: &egui::Painter) {
    let Some(pe) = app.pixel_edit.as_ref() else {
        return;
    };
    let map = |x: f32, y: f32| app.view.to_screen(pe.to_page(x, y));
    draw_mapped(pe, painter, &map, app.pointer_page.map(|p| pe.to_px(p)));
}

/// Draw the picture being edited with `map` taking its pixels to the
/// screen, and the brush at `pointer` (in its pixels).
pub fn draw_mapped(
    pe: &PixelEdit,
    painter: &egui::Painter,
    map: &dyn Fn(f32, f32) -> egui::Pos2,
    pointer: Option<(f32, f32)>,
) {
    let ctx = painter.ctx();
    let shown = pe.preview.as_ref().unwrap_or(&pe.pi.img);
    let (w, h) = shown.dimensions();
    if w == 0 || h == 0 {
        return;
    }
    let mut v = pe.view.borrow_mut();
    let preview = pe.preview.is_some();
    let size_changed = v
        .tex
        .as_ref()
        .is_some_and(|t| t.size() != [w as usize, h as usize]);
    let opts = egui::TextureOptions::NEAREST;
    // Layers: what is seen is put together from them (only the changed
    // box while painting).
    let layered = pe.pi.is_layered();
    // For the mask view, before the picture's upload takes them.
    let (dirty_now, full_now, sel_now) = (v.dirty, v.full, v.sel_changed);
    if v.tex.is_none() || v.full || size_changed || preview || v.showed_preview {
        let composed = layered
            .then(|| pe.pi.composite_region([0, 0, w, h], pe.preview.as_ref()))
            .flatten();
        let px = composed.as_ref().unwrap_or(shown);
        let img = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], px.as_raw());
        match v.tex.as_mut() {
            Some(t) if !size_changed => t.set(img, opts),
            _ => v.tex = Some(ctx.load_texture("pixel_edit", img, opts)),
        }
        v.full = false;
        v.dirty = None;
        v.thumbs.stale = true;
    } else if let Some(b) = v.dirty.take() {
        v.thumbs.stale = true;
        let (x0, y0) = (b[0].min(w), b[1].min(h));
        let (x1, y1) = (b[2].min(w), b[3].min(h));
        if x1 > x0 && y1 > y0 {
            let mut part = Vec::with_capacity(((x1 - x0) * (y1 - y0) * 4) as usize);
            if layered {
                if let Some(m) = pe.pi.composite_region([x0, y0, x1, y1], None) {
                    part = m.into_raw();
                }
            } else {
                for y in y0..y1 {
                    let row = (y * w + x0) as usize * 4;
                    let end = (y * w + x1) as usize * 4;
                    if let Some(px) = shown.as_raw().get(row..end) {
                        part.extend_from_slice(px);
                    }
                }
            }
            if part.len() == ((x1 - x0) * (y1 - y0) * 4) as usize {
                let img = egui::ColorImage::from_rgba_unmultiplied(
                    [(x1 - x0) as usize, (y1 - y0) as usize],
                    &part,
                );
                if let Some(t) = v.tex.as_mut() {
                    t.set_partial([x0 as usize, y0 as usize], img, opts);
                }
            }
        }
    }
    v.showed_preview = preview;
    if v.sel_changed {
        v.edges = pe
            .pi
            .sel
            .as_ref()
            .map(pixel_edit::selection_edges)
            .unwrap_or_default();
        v.sel_changed = false;
    }
    let corners = [
        (0.0, 0.0),
        (w as f32, 0.0),
        (w as f32, h as f32),
        (0.0, h as f32),
    ]
    .map(|(x, y)| map(x, y));
    let quad = |tex: egui::TextureId| {
        let mut mesh = egui::Mesh::with_texture(tex);
        let uvs = [
            egui::pos2(0.0, 0.0),
            egui::pos2(1.0, 0.0),
            egui::pos2(1.0, 1.0),
            egui::pos2(0.0, 1.0),
        ];
        for (c, uv) in corners.iter().zip(uvs) {
            mesh.vertices.push(egui::epaint::Vertex {
                pos: *c,
                uv,
                color: egui::Color32::WHITE,
            });
        }
        mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
        egui::Shape::mesh(mesh)
    };
    if let Some(t) = &v.tex {
        painter.add(quad(t.id()));
    }
    // The mask being painted, in red where it hides or leaves out.
    match pe.target_mask() {
        Some(m) if pe.target != MaskTarget::Pixels || pe.remove.is_some() => {
            let size = [m.width() as usize, m.height() as usize];
            let whole = full_now || sel_now || v.overlay.as_ref().is_none_or(|t| t.size() != size);
            if whole {
                let o = pixel_edit::mask_overlay(m);
                let img = egui::ColorImage::from_rgba_unmultiplied(size, o.as_raw());
                match v.overlay.as_mut() {
                    Some(t) if t.size() == size => t.set(img, opts),
                    _ => v.overlay = Some(ctx.load_texture("pixel_edit_mask", img, opts)),
                }
            } else if let Some(b) = dirty_now {
                let (x0, y0) = (b[0].min(m.width()), b[1].min(m.height()));
                let (x1, y1) = (b[2].min(m.width()), b[3].min(m.height()));
                if x1 > x0 && y1 > y0 {
                    let part = image::imageops::crop_imm(m, x0, y0, x1 - x0, y1 - y0).to_image();
                    let o = pixel_edit::mask_overlay(&part);
                    let img = egui::ColorImage::from_rgba_unmultiplied(
                        [(x1 - x0) as usize, (y1 - y0) as usize],
                        o.as_raw(),
                    );
                    if let Some(t) = v.overlay.as_mut() {
                        t.set_partial([x0 as usize, y0 as usize], img, opts);
                    }
                }
            }
        }
        _ => v.overlay = None,
    }
    if let Some(t) = &v.overlay {
        painter.add(quad(t.id()));
    }
    // While the Move tool's pixels follow a drag, the box shows where
    // they go in place of the selection's outline.
    if pe.target != MaskTarget::QuickMask && !v.edges.is_empty() && pe.placed_for.is_none() {
        marching_ants(painter, &v.edges, map);
    }
    // The picture's edge.
    let edge = egui::Stroke::new(1.0, egui::Color32::from_rgb(0x1E, 0x7C, 0xE8));
    for i in 0..4 {
        painter.line_segment([corners[i], corners[(i + 1) % 4]], edge);
    }
    drop(v);
    let to_screen = |p: (f32, f32)| map(p.0, p.1);
    let ink = egui::Stroke::new(1.0, egui::Color32::from_rgb(0x1E, 0x7C, 0xE8));
    if let (Some(a), Some(b)) = (pe.drag_from, pe.drag_to) {
        match pe.tool {
            PixelTool::Gradient | PixelTool::LayerFade => {
                painter.line_segment([to_screen(a), to_screen(b)], ink);
                painter.circle_filled(to_screen(a), 3.5, egui::Color32::WHITE);
                painter.circle_filled(to_screen(b), 3.5, egui::Color32::BLACK);
            }
            PixelTool::BandMask => {
                // The band's two edges, square to the drag, across the
                // picture.
                let (dx, dy) = (b.0 - a.0, b.1 - a.1);
                let len = dx.hypot(dy).max(1e-3);
                let (nx, ny) = (-dy / len, dx / len);
                let far = (w + h) as f32;
                for p in [a, b] {
                    painter.line_segment(
                        [
                            to_screen((p.0 - nx * far, p.1 - ny * far)),
                            to_screen((p.0 + nx * far, p.1 + ny * far)),
                        ],
                        ink,
                    );
                }
                painter.line_segment([to_screen(a), to_screen(b)], ink);
            }
            PixelTool::RectSelect => {
                let pts = [a, (b.0, a.1), b, (a.0, b.1), a].map(to_screen);
                painter.add(egui::Shape::line(pts.to_vec(), ink));
            }
            PixelTool::Shape => {
                use crate::pixel_paint::PixelShape;
                let pts: Vec<(f32, f32)> = match pe.opts.shape {
                    PixelShape::Rect => vec![a, (b.0, a.1), b, (a.0, b.1), a],
                    PixelShape::Line => vec![a, b],
                    PixelShape::Polygon => {
                        let mut p = crate::pixel_paint::polygon_points(a, b, pe.opts.sides);
                        if let Some(f) = p.first().copied() {
                            p.push(f);
                        }
                        p
                    }
                    PixelShape::Ellipse => {
                        let (cx, cy) = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
                        let (rx, ry) = ((b.0 - a.0).abs() / 2.0, (b.1 - a.1).abs() / 2.0);
                        (0..=48)
                            .map(|i| {
                                let t = i as f32 / 48.0 * std::f32::consts::TAU;
                                (cx + rx * t.cos(), cy + ry * t.sin())
                            })
                            .collect()
                    }
                };
                painter.add(egui::Shape::line(
                    pts.into_iter().map(to_screen).collect(),
                    ink,
                ));
            }
            PixelTool::EllipseSelect => {
                let (cx, cy) = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
                let (rx, ry) = ((b.0 - a.0).abs() / 2.0, (b.1 - a.1).abs() / 2.0);
                let pts: Vec<_> = (0..=48)
                    .map(|i| {
                        let t = i as f32 / 48.0 * std::f32::consts::TAU;
                        to_screen((cx + rx * t.cos(), cy + ry * t.sin()))
                    })
                    .collect();
                painter.add(egui::Shape::line(pts, ink));
            }
            _ => {}
        }
    }
    // The warp grid: its lines through the smooth surface and its points.
    if let Some(wp) = &pe.warp {
        let line = egui::Stroke::new(1.0, egui::Color32::from_rgb(0x1E, 0x7C, 0xE8));
        let steps = 16;
        for r in 0..wp.rows {
            let pts: Vec<_> = (0..=steps * (wp.cols - 1))
                .map(|i| to_screen(wp.eval(i as f32 / steps as f32, r as f32)))
                .collect();
            painter.add(egui::Shape::line(pts, line));
        }
        for c in 0..wp.cols {
            let pts: Vec<_> = (0..=steps * (wp.rows - 1))
                .map(|j| to_screen(wp.eval(c as f32, j as f32 / steps as f32)))
                .collect();
            painter.add(egui::Shape::line(pts, line));
        }
        // The point the pointer would take shows larger and filled.
        let hot = wp.drag.or_else(|| {
            let ptr = to_screen(pointer?);
            wp.pts
                .iter()
                .enumerate()
                .map(|(k, p)| (k, to_screen(*p).distance(ptr)))
                .filter(|(_, d)| *d <= crate::pixel_warp::GRAB_RADIUS)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(k, _)| k)
        });
        for (k, p) in wp.pts.iter().enumerate() {
            let c = to_screen(*p);
            if hot == Some(k) {
                painter.circle(c, 6.0, line.color, egui::Stroke::new(1.5, egui::Color32::WHITE));
            } else {
                painter.circle(c, 4.0, egui::Color32::WHITE, line);
            }
        }
    }
    if pe.tool == PixelTool::Move && pe.warp.is_none() {
        draw_move_box(pe, painter, map);
    }
    if pe.lasso.len() > 1 {
        let pts: Vec<_> = pe.lasso.iter().map(|p| to_screen(*p)).collect();
        painter.add(egui::Shape::line(pts, ink));
    }
    // The outline of clicked corners: its corners and the line on to the
    // pointer.
    if pe.tool == PixelTool::FreehandMask && !pe.lasso.is_empty() {
        if let (Some(l), Some(p)) = (pe.lasso.last(), pointer) {
            painter.line_segment([to_screen(*l), to_screen(p)], ink);
        }
        if let Some(f) = pe.lasso.first() {
            painter.circle(to_screen(*f), 4.0, egui::Color32::WHITE, ink);
        }
    }
    // The pen outline: its curve so far, on to the pointer, its points,
    // the first one round, and the handles of the last.
    if pe.tool == PixelTool::PenMask && !pe.pen.is_empty() {
        let line: Vec<_> = pixel_edit::pen_polygon(&pe.pen, false)
            .into_iter()
            .map(to_screen)
            .collect();
        if line.len() > 1 {
            painter.add(egui::Shape::line(line, ink));
        }
        if let (Some(l), Some(p), false) = (pe.pen.last(), pointer, pe.pen_drag) {
            let next = [*l, pixel_edit::PenNode::corner(p)];
            let pts: Vec<_> = pixel_edit::pen_polygon(&next, false)
                .into_iter()
                .map(to_screen)
                .collect();
            painter.add(egui::Shape::dashed_line(&pts, ink, 4.0, 3.0));
        }
        for n in &pe.pen {
            let c = to_screen(n.p);
            painter.rect(
                egui::Rect::from_center_size(c, egui::vec2(6.0, 6.0)),
                0.0,
                egui::Color32::WHITE,
                ink,
                egui::StrokeKind::Middle,
            );
        }
        if let Some(f) = pe.pen.first() {
            painter.circle(to_screen(f.p), 4.0, egui::Color32::WHITE, ink);
        }
        if let Some(l) = pe.pen.last() {
            for h in [l.inn, l.out] {
                if h != (0.0, 0.0) {
                    let e = to_screen((l.p.0 + h.0, l.p.1 + h.1));
                    painter.line_segment([to_screen(l.p), e], ink);
                    painter.circle_filled(e, 3.0, ink.color);
                }
            }
        }
    }
    // The brush's size under the pointer, and the clone source.
    if pe.tool.is_brush()
        || pe.tool.is_liquify()
        || matches!(
            pe.tool,
            PixelTool::Light | PixelTool::MaskBrush | PixelTool::Sprayer | PixelTool::Remove
        )
    {
        if let Some((x, y)) = pointer {
            let r = pe.opts.size / 2.0;
            let pts: Vec<_> = (0..=40)
                .map(|i| {
                    let t = i as f32 / 40.0 * std::f32::consts::TAU;
                    to_screen((x + r * t.cos(), y + r * t.sin()))
                })
                .collect();
            painter.add(egui::Shape::line(
                pts.clone(),
                egui::Stroke::new(2.0, egui::Color32::from_black_alpha(140)),
            ));
            painter.add(egui::Shape::line(
                pts,
                egui::Stroke::new(1.0, egui::Color32::WHITE),
            ));
        }
        if matches!(pe.tool, PixelTool::Clone | PixelTool::Heal) {
            let src = match (pe.clone_offset, pointer, pe.stroke.is_some()) {
                (Some(o), Some((x, y)), _) => Some((x + o.0, y + o.1)),
                _ => pe.clone_src,
            };
            if let Some(s) = src {
                let c = to_screen(s);
                let st = egui::Stroke::new(1.5, egui::Color32::from_rgb(0xE8, 0x3A, 0x1E));
                painter.line_segment([c - egui::vec2(6.0, 0.0), c + egui::vec2(6.0, 0.0)], st);
                painter.line_segment([c - egui::vec2(0.0, 6.0), c + egui::vec2(0.0, 6.0)], st);
            }
        }
    }
}

/// Dashes and gaps of the selection outline, in screen points.
const ANT: f32 = 4.0;
/// How fast the dashes march round the selection, points a second.
const ANT_SPEED: f64 = 12.0;
/// More screen points of outline than this and the dashes stand still
/// (a selection of thousands of specks), so the screen keeps up.
const ANT_MOST: f32 = 400_000.0;

/// The selection outline as marching ants: a black line with white
/// dashes on it that move round the selection, clockwise, a line one
/// screen point wide at every zoom. Only the runs on the screen are
/// drawn.
fn marching_ants(
    painter: &egui::Painter,
    edges: &[[(f32, f32); 2]],
    map: &dyn Fn(f32, f32) -> egui::Pos2,
) {
    let clip = painter.clip_rect().expand(2.0);
    let shown: Vec<(egui::Pos2, egui::Pos2)> = edges
        .iter()
        .map(|e| (map(e[0].0, e[0].1), map(e[1].0, e[1].1)))
        .filter(|(a, b)| clip.intersects(egui::Rect::from_two_pos(*a, *b)))
        .collect();
    if shown.is_empty() {
        return;
    }
    let black = egui::Stroke::new(1.0, egui::Color32::BLACK);
    let white = egui::Stroke::new(1.0, egui::Color32::WHITE);
    let mut shapes: Vec<egui::Shape> =
        shown.iter().map(|(a, b)| egui::Shape::line_segment([*a, *b], black)).collect();
    let length: f32 = shown.iter().map(|(a, b)| (*b - *a).length()).sum();
    let moving = length <= ANT_MOST;
    let ctx = painter.ctx();
    let phase = if moving {
        (ctx.input(|i| i.time) * ANT_SPEED).rem_euclid(2.0 * ANT as f64) as f32
    } else {
        0.0
    };
    for (a, b) in shown {
        ant_dashes(a, b, phase, white, &mut shapes);
    }
    painter.extend(shapes);
    if moving {
        ctx.request_repaint_after(std::time::Duration::from_millis(50));
    }
}

/// The white dashes on the line from `a` to `b`: laid on the line's
/// own distance along its direction less `phase`, so runs in a line
/// share one pattern and a larger phase moves them on towards `b`.
pub(crate) fn ant_dashes(
    a: egui::Pos2,
    b: egui::Pos2,
    phase: f32,
    stroke: egui::Stroke,
    out: &mut Vec<egui::Shape>,
) {
    let v = b - a;
    let len = v.length();
    if len <= 0.0 {
        return;
    }
    let u = v / len;
    let along = a.x * u.x + a.y * u.y - phase;
    let mut t = -along.rem_euclid(2.0 * ANT);
    while t < len {
        let (d0, d1) = (t.max(0.0), (t + ANT).min(len));
        if d1 > d0 {
            out.push(egui::Shape::line_segment([a + u * d0, a + u * d1], stroke));
        }
        t += 2.0 * ANT;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};
    use crate::ui::pixel_cmds::PixelCmd;

    #[test]
    fn the_pen_closes_its_outline_into_a_selection_and_curves_through_handles() {
        let (mut app, id) = app_with_picture();
        app.edit_bitmap_inside(id);
        let pe = app.pixel_edit.as_mut().unwrap();
        pe.tool = PixelTool::PenMask;
        let c = pixel_edit::PenNode::corner;
        pe.pen = vec![c((1.0, 1.0)), c((10.0, 1.0)), c((10.0, 9.0)), c((1.0, 9.0))];
        close_pen(pe, SelectMode::Replace);
        let sel = pe.pi.sel.clone().unwrap();
        assert_eq!(sel.get_pixel(5, 5)[0], 255);
        assert_eq!(sel.get_pixel(15, 5)[0], 0);
        assert!(pe.pen.is_empty() && !pe.pen_drag);
        // The right side pulled out into a curve reaches further right.
        let mut bulge = c((10.0, 5.0));
        bulge.inn = (6.0, -3.0);
        bulge.out = (-6.0, 3.0);
        pe.pen = vec![c((1.0, 1.0)), c((10.0, 1.0)), bulge, c((10.0, 9.0)), c((1.0, 9.0))];
        close_pen(pe, SelectMode::Replace);
        let sel = pe.pi.sel.clone().unwrap();
        assert!(sel.get_pixel(11, 3)[0] >= 128, "the curve bulges past the corners");
        assert_eq!(sel.get_pixel(15, 3)[0], 0);
    }

    #[test]
    fn the_ants_dashes_march_along_the_line() {
        let dashes = |phase: f32| {
            let mut out = Vec::new();
            let st = egui::Stroke::new(1.0, egui::Color32::WHITE);
            ant_dashes(egui::pos2(0.0, 0.0), egui::pos2(16.0, 0.0), phase, st, &mut out);
            out.iter()
                .filter_map(|s| match s {
                    egui::Shape::LineSegment { points, .. } => Some((points[0].x, points[1].x)),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(dashes(0.0), vec![(0.0, 4.0), (8.0, 12.0)]);
        // Later on they have moved on along the line.
        assert_eq!(dashes(2.0), vec![(2.0, 6.0), (10.0, 14.0)]);
        assert_eq!(dashes(6.0), vec![(0.0, 2.0), (6.0, 10.0), (14.0, 16.0)]);
    }

    /// An app with one 20 x 10 pixel picture, 40 x 20 mm at (10, 10).
    fn app_with_picture() -> (App, ShapeId) {
        let mut app = App::headless();
        let img = RgbaImage::from_pixel(20, 10, Rgba([255, 255, 255, 255]));
        let png = crate::bitmap_fx::encode(&img).unwrap();
        let id = app
            .new_shape(ShapeKind::Bitmap {
                rect: Rect::new(10.0, 10.0, 50.0, 30.0),
                width_px: 20,
                height_px: 10,
                png: png.into(),
                fx: None,
            })
            .expect("a layer");
        (app, id)
    }

    fn pixels(app: &App, id: ShapeId) -> RgbaImage {
        match &app.doc().find_shape(id).unwrap().kind {
            ShapeKind::Bitmap { png, .. } => crate::bitmap_fx::decode(png).unwrap(),
            _ => panic!("not a picture"),
        }
    }

    /// #4376: Apply in the Effects panel runs the effect once; a second
    /// click does nothing until a setting changes or another effect is
    /// picked, and the preview stops showing the effect run once more.
    #[test]
    fn apply_in_the_effects_panel_runs_the_effect_once() {
        let (mut app, id) = app_with_picture();
        app.edit_bitmap_inside(id);
        let pe = app.pixel_edit.as_mut().unwrap();
        let before = pe.pi.img.clone();
        pe.effect = Some(crate::fx::new_effect("invert_colors"));
        pe.refresh_preview();
        assert!(pe.preview.is_some());
        crate::ui::pixel_bar::apply_panel_effect(pe);
        let once = pe.pi.img.clone();
        assert_ne!(once, before, "applied");
        assert!(pe.effect_applied && pe.preview.is_none());
        crate::ui::pixel_bar::apply_panel_effect(pe);
        assert_eq!(pe.pi.img, once, "a second Apply does nothing");
        // A change of a setting lets Apply work again.
        pe.effect_applied = false;
        crate::ui::pixel_bar::apply_panel_effect(pe);
        assert_eq!(pe.pi.img, before, "inverted back");
    }

    /// #4374: a layer moved partly past the picture's edge and moved back
    /// comes back whole; a move of its own pixels in between does not
    /// lose what went past the edge.
    #[test]
    fn a_layer_moved_past_the_edge_and_back_comes_back_whole() {
        let (mut app, id) = app_with_picture();
        app.edit_bitmap_inside(id);
        let pe = app.pixel_edit.as_mut().unwrap();
        pe.tool = PixelTool::Move;
        let (w, h) = pe.pi.img.dimensions();
        // Paint a mark in the top left corner.
        for y in 0..3 {
            for x in 0..3 {
                pe.pi.img.put_pixel(x, y, image::Rgba([255, 0, 0, 255]));
            }
        }
        pe.bump();
        let before = pe.pi.img.clone();
        let full = [0, 0, w, h];
        let mv = |dx: f32, dy: f32| crate::pixel_ops::Placement {
            dx,
            dy,
            ..Default::default()
        };
        pe.placement = mv(-5.0, -5.0);
        pe.apply_placement(Some(full));
        assert_eq!(pe.pi.img.get_pixel(0, 0)[3], before.get_pixel(5, 5)[3]);
        pe.placement = mv(2.0, 0.0);
        pe.apply_placement(Some(full));
        pe.placement = mv(3.0, 5.0);
        pe.apply_placement(Some(full));
        assert_eq!(pe.pi.img, before, "the mark came back from past the edge");
        // Undo goes back one move at a time.
        pe.undo();
        assert_eq!(*pe.pi.img.get_pixel(0, 0), *before.get_pixel(5, 5));
    }

    #[test]
    fn page_points_map_to_the_pictures_pixels() {
        let (mut app, id) = app_with_picture();
        assert!(app.edit_bitmap_inside(id));
        let pe = app.pixel_edit.as_ref().unwrap();
        // The top left corner of the picture is (10, 30) on the page.
        let (x, y) = pe.to_px(Point::new(10.0, 30.0));
        assert!(x.abs() < 1e-3 && y.abs() < 1e-3, "{x} {y}");
        let (x, y) = pe.to_px(Point::new(50.0, 10.0));
        assert!((x - 20.0).abs() < 1e-3 && (y - 10.0).abs() < 1e-3);
        let back = pe.to_page(5.0, 5.0);
        assert!((back.x - 20.0).abs() < 1e-6 && (back.y - 20.0).abs() < 1e-6);
    }

    #[test]
    fn finish_puts_the_pixels_back_in_one_undo_step() {
        let (mut app, id) = app_with_picture();
        app.edit_bitmap_inside(id);
        let pe = app.pixel_edit.as_mut().unwrap();
        pe.apply(false, |pi| {
            pixel_edit::fill_selection(pi, [255, 0, 0, 255], 1.0)
        });
        app.finish_pixel_edit();
        assert!(app.pixel_edit.is_none());
        assert_eq!(*pixels(&app, id).get_pixel(3, 3), Rgba([255, 0, 0, 255]));
        app.undo();
        assert_eq!(
            *pixels(&app, id).get_pixel(3, 3),
            Rgba([255, 255, 255, 255])
        );
    }

    #[test]
    fn cancel_and_an_unchanged_finish_leave_the_picture() {
        let (mut app, id) = app_with_picture();
        let rev = app.engine.revision();
        app.edit_bitmap_inside(id);
        app.finish_pixel_edit();
        assert_eq!(app.engine.revision(), rev);
        app.edit_bitmap_inside(id);
        let pe = app.pixel_edit.as_mut().unwrap();
        pe.apply(false, |pi| {
            pixel_edit::fill_selection(pi, [0, 0, 0, 255], 1.0)
        });
        app.cancel_pixel_edit();
        assert_eq!(app.engine.revision(), rev);
        assert_eq!(
            *pixels(&app, id).get_pixel(3, 3),
            Rgba([255, 255, 255, 255])
        );
    }

    #[test]
    fn a_crop_keeps_the_pixels_where_they_were_on_the_page() {
        let (mut app, id) = app_with_picture();
        app.edit_bitmap_inside(id);
        let pe = app.pixel_edit.as_mut().unwrap();
        pe.pi.sel = pixel_edit::rect_mask(20, 10, (10.0, 0.0), (20.0, 5.0));
        assert!(pe.apply(true, pixel_edit::crop_to_selection));
        app.finish_pixel_edit();
        match &app.doc().find_shape(id).unwrap().kind {
            ShapeKind::Bitmap {
                rect,
                width_px,
                height_px,
                ..
            } => {
                assert_eq!((*width_px, *height_px), (10, 5));
                // Right half, top half: x 30..50, y 20..30 mm.
                assert!(
                    (rect.x0 - 30.0).abs() < 1e-9 && (rect.x1 - 50.0).abs() < 1e-9,
                    "{rect:?}"
                );
                assert!(
                    (rect.y0 - 20.0).abs() < 1e-9 && (rect.y1 - 30.0).abs() < 1e-9,
                    "{rect:?}"
                );
            }
            _ => panic!(),
        }
    }

    #[test]
    fn the_page_is_drawn_without_the_picture_while_it_is_edited() {
        let (mut app, id) = app_with_picture();
        assert!(app.pixel_edit_doc().is_none());
        app.edit_bitmap_inside(id);
        let doc = app.pixel_edit_doc().unwrap();
        assert!(!doc.find_shape(id).unwrap().visible);
        assert!(app.doc().find_shape(id).unwrap().visible);
    }

    #[test]
    fn a_linked_picture_stands_alone_after_editing() {
        let (mut app, id) = app_with_picture();
        app.run(Command::SetObjectData {
            shape: id,
            data: vec![("bitmap.link".into(), "/tmp/x.png".into())],
        });
        app.edit_bitmap_inside(id);
        let pe = app.pixel_edit.as_mut().unwrap();
        pe.apply(false, |pi| {
            pixel_edit::fill_selection(pi, [0, 0, 0, 255], 1.0)
        });
        app.finish_pixel_edit();
        assert!(!app
            .doc()
            .find_shape(id)
            .unwrap()
            .data
            .iter()
            .any(|(k, _)| k == "bitmap.link"));
    }

    #[test]
    fn undo_inside_the_editing_goes_back_a_step() {
        let (mut app, id) = app_with_picture();
        app.edit_bitmap_inside(id);
        let pe = app.pixel_edit.as_mut().unwrap();
        pe.apply(false, |pi| {
            pixel_edit::fill_selection(pi, [0, 0, 0, 255], 1.0)
        });
        pe.undo();
        assert_eq!(pe.pi.img.get_pixel(0, 0)[0], 255);
        pe.redo();
        assert_eq!(pe.pi.img.get_pixel(0, 0)[0], 0);
    }

    #[test]
    fn finish_puts_the_layers_together_and_a_locked_layer_keeps_its_pixels() {
        let (mut app, id) = app_with_picture();
        app.edit_bitmap_inside(id);
        let pe = app.pixel_edit.as_mut().unwrap();
        pe.apply_all(true, |pi| {
            pi.add_layer("top");
            Some([0, 0, 20, 10])
        });
        pe.apply(false, |pi| {
            pixel_edit::fill_selection(pi, [0, 0, 255, 255], 1.0)
        });
        pe.pi.layers[1].locked = true;
        assert!(!pe.apply(false, |pi| {
            pixel_edit::fill_selection(pi, [0, 255, 0, 255], 1.0)
        }));
        pe.pi.layers[1].opacity = 0.5;
        app.finish_pixel_edit();
        let q = *pixels(&app, id).get_pixel(3, 3);
        assert!(q[2] == 255 && (120..=135).contains(&q[0]), "{q:?}");
        app.undo();
        assert_eq!(
            *pixels(&app, id).get_pixel(3, 3),
            Rgba([255, 255, 255, 255])
        );
    }

    /// A picture opened with a second layer painted blue, then finished.
    fn finish_with_two_layers(app: &mut App, id: ShapeId) {
        assert!(app.edit_bitmap_inside(id));
        let pe = app.pixel_edit.as_mut().unwrap();
        pe.apply_all(true, |pi| {
            pi.add_layer("top");
            Some([0, 0, 20, 10])
        });
        pe.apply(false, |pi| pixel_edit::fill_selection(pi, [0, 0, 255, 255], 1.0));
        app.finish_pixel_edit();
    }

    #[test]
    fn the_layers_open_again_after_finish_until_the_picture_changes() {
        let (mut app, id) = app_with_picture();
        assert!(app.settings.pixel_keep_layers, "on by default");
        finish_with_two_layers(&mut app, id);
        assert!(app.pixel_edit.is_none());
        assert_eq!(pixels(&app, id).get_pixel(3, 3), &Rgba([0, 0, 255, 255]));
        // Edit Inside again: both layers, the top one in use.
        assert!(app.edit_bitmap_inside(id));
        let pe = app.pixel_edit.as_ref().unwrap();
        assert_eq!(pe.pi.layers.len(), 2);
        assert_eq!(pe.pi.active, 1);
        assert_eq!(pe.pi.layer_pixels(0).unwrap().get_pixel(3, 3), &Rgba([255, 255, 255, 255]));
        assert_eq!(pe.pi.img.get_pixel(3, 3), &Rgba([0, 0, 255, 255]));
        app.cancel_pixel_edit();
        // The finish undone: the picture is another, the layers do not fit.
        app.undo();
        assert!(app.edit_bitmap_inside(id));
        assert_eq!(app.pixel_edit.as_ref().unwrap().pi.layers.len(), 1);
        assert!(app.doc().find_shape(id).unwrap().data.is_empty(), "nothing in the file");
    }

    #[test]
    fn layers_saved_in_the_file_open_again_and_flattening_for_good_asks_first() {
        let (mut app, id) = app_with_picture();
        app.settings.pixel_keep_layers = false;
        app.settings.pixel_save_layers = true;
        finish_with_two_layers(&mut app, id);
        assert_eq!(app.pixel_kept.len(), 0);
        let data = app.doc().find_shape(id).unwrap().data.clone();
        assert!(data.iter().any(|(k, _)| k == crate::pixel_keep::DATA_KEY));
        assert!(app.edit_bitmap_inside(id));
        assert_eq!(app.pixel_edit.as_ref().unwrap().pi.layers.len(), 2);
        app.cancel_pixel_edit();
        // Kept nowhere: Finish asks, and nothing changes meanwhile.
        app.settings.pixel_save_layers = false;
        assert!(app.edit_bitmap_inside(id));
        let pe = app.pixel_edit.as_mut().unwrap();
        pe.apply(false, |pi| pixel_edit::fill_selection(pi, [0, 255, 0, 255], 1.0));
        app.finish_pixel_edit();
        assert!(matches!(
            app.dialog,
            crate::ui::dialogs::Dialog::ConfirmPixelFlatten { .. }
        ));
        assert!(app.pixel_edit.is_some());
        app.finish_pixel_edit_now();
        assert_eq!(pixels(&app, id).get_pixel(3, 3), &Rgba([0, 255, 0, 255]));
        assert!(
            !app.doc().find_shape(id).unwrap().data.iter().any(|(k, _)| k == crate::pixel_keep::DATA_KEY),
            "the old layers no longer fit and go"
        );
        // Told not to ask: it finishes at once.
        app.dialog = crate::ui::dialogs::Dialog::None;
        app.settings.warnings.pixel_flatten = false;
        finish_with_two_layers(&mut app, id);
        assert!(app.pixel_edit.is_none());
    }

    #[test]
    fn quick_mask_turns_painting_into_a_selection() {
        let (mut app, id) = app_with_picture();
        app.edit_bitmap_inside(id);
        let pe = app.pixel_edit.as_mut().unwrap();
        pe.set_quick_mask(true);
        assert_eq!(pe.target, MaskTarget::QuickMask);
        assert!(pe.pi.sel.as_ref().unwrap().pixels().all(|v| v[0] == 0));
        pe.opts.hardness = 1.0;
        pe.opts.opacity = 1.0;
        pe.opts.flow = 1.0;
        pe.opts.size = 6.0;
        let m = pe.target_mask().unwrap().clone();
        pe.mask_stroke = Some(MaskStroke::new(pe.opts, &m, 255));
        pe.mask_to(5.0, 5.0);
        assert_eq!(pe.pi.sel.as_ref().unwrap().get_pixel(5, 5)[0], 255);
        assert_eq!(pe.pi.img.get_pixel(5, 5)[0], 255, "pixels untouched");
        pe.set_quick_mask(false);
        assert_eq!(pe.target, MaskTarget::Pixels);
        assert!(pe.pi.sel.is_some());
        pe.set_quick_mask(true);
        pe.set_selection(Some(image::GrayImage::new(20, 10)));
        pe.set_quick_mask(false);
        assert!(pe.pi.sel.is_none(), "nothing painted, no selection");
    }

    #[test]
    fn an_effect_preview_does_not_change_the_picture_until_applied() {
        let (mut app, id) = app_with_picture();
        app.edit_bitmap_inside(id);
        let pe = app.pixel_edit.as_mut().unwrap();
        pe.effect = Some(crate::fx::new_effect("invert_colors"));
        pe.refresh_preview();
        assert_eq!(pe.preview.as_ref().unwrap().get_pixel(0, 0)[0], 0);
        assert_eq!(pe.pi.img.get_pixel(0, 0)[0], 255);
        let e = pe.effect.clone().unwrap();
        pe.apply(true, |pi| pixel_edit::apply_effect(pi, &e));
        assert_eq!(pe.pi.img.get_pixel(0, 0)[0], 0);
        assert!(pe.preview.is_none());
    }

    #[test]
    fn the_effect_preview_comes_back_after_painting_with_it_open() {
        // #4264: a Burn stroke with Brightness/Contrast open took the
        // preview off until a setting changed.
        let (mut app, id) = app_with_picture();
        app.edit_bitmap_inside(id);
        let pe = app.pixel_edit.as_mut().unwrap();
        pe.effect = Some(crate::fx::new_effect("invert_colors"));
        pe.refresh_preview();
        pe.tool = PixelTool::Burn;
        pe.opts.size = 4.0;
        pe.opts.hardness = 1.0;
        pe.step();
        let mut s = Stroke::new(PixelTool::Burn, pe.opts, &pe.pi, (0.0, 0.0));
        let b = s.to(&mut pe.pi, 5.0, 5.0);
        pe.mark(b);
        assert!(pe.preview.is_none(), "off while painting");
        pe.keep_preview_current(true);
        assert!(pe.preview.is_none(), "not while the button is down");
        pe.keep_preview_current(false);
        let burnt = pe.pi.img.get_pixel(5, 5)[0];
        assert!(burnt < 255);
        let shown = pe.preview.as_ref().expect("the preview is back").get_pixel(5, 5)[0];
        assert_eq!(shown, 255 - burnt, "the effect runs on the burnt pixels");
        // Nothing changed since: it is not made again.
        let rev = pe.preview_rev;
        pe.keep_preview_current(false);
        assert_eq!(pe.preview_rev, rev);
    }

    #[test]
    fn copy_paste_puts_the_pixels_back_where_they_were_and_move_takes_them() {
        // #4263: Ctrl+C, Ctrl+V, then the Move tool.
        let (mut app, id) = app_with_picture();
        app.edit_bitmap_inside(id);
        let pe = app.pixel_edit.as_mut().unwrap();
        pe.apply(false, |pi| {
            for y in 2..5 {
                for x in 3..6 {
                    pi.img.put_pixel(x, y, Rgba([200, 0, 0, 255]));
                }
            }
            Some([0, 0, 20, 10])
        });
        pe.set_selection(pixel_edit::rect_mask(20, 10, (3.0, 2.0), (6.0, 5.0)));
        assert!(pe.copy_pixels(false));
        // Pasted from what was kept (the system clipboard may hold
        // anything in a test run).
        let (img, at) = pe.clip.clone().unwrap();
        assert_eq!(at, (3, 2));
        let name = crate::ui::pixel_bar::new_layer_name(pe);
        assert!(pe.apply_all(true, |pi| pixel_edit::paste_layer_at(pi, name, &img, at)));
        assert_eq!(pe.pi.layers.len(), 2);
        assert!(pe.pi.sel.is_none());
        assert_eq!(*pe.pi.img.get_pixel(3, 2), Rgba([200, 0, 0, 255]));
        assert_eq!(pe.pi.img.get_pixel(0, 0)[3], 0);
        // The Move tool's box is the pasted pixels, not the picture.
        pe.tool = PixelTool::Move;
        assert_eq!(pe.move_box(), [3, 2, 6, 5]);
        pe.placement.dx = 10.0;
        pe.sync_place_preview();
        assert!(pe.preview.is_some(), "the pixels follow the drag");
        pe.apply_placement(None);
        assert!(pe.preview.is_none());
        assert_eq!(*pe.pi.img.get_pixel(13, 2), Rgba([200, 0, 0, 255]));
        assert_eq!(pe.pi.img.get_pixel(3, 2)[3], 0);
        assert_eq!(pe.move_box(), [13, 2, 16, 5]);
        pe.undo();
        assert_eq!(*pe.pi.img.get_pixel(3, 2), Rgba([200, 0, 0, 255]));
    }

    #[test]
    fn a_cut_clears_the_selected_pixels() {
        let (mut app, id) = app_with_picture();
        app.edit_bitmap_inside(id);
        let pe = app.pixel_edit.as_mut().unwrap();
        pe.set_selection(pixel_edit::rect_mask(20, 10, (0.0, 0.0), (4.0, 4.0)));
        assert!(pe.copy_pixels(true));
        assert_eq!(pe.pi.img.get_pixel(1, 1)[3], 0);
        assert_eq!(pe.pi.img.get_pixel(8, 8)[3], 255);
        assert_eq!(pe.clip.as_ref().unwrap().0.dimensions(), (4, 4));
    }

    #[test]
    fn the_move_tool_moves_only_the_selected_pixels_and_the_selection() {
        let (mut app, id) = app_with_picture();
        app.edit_bitmap_inside(id);
        let pe = app.pixel_edit.as_mut().unwrap();
        pe.apply(false, |pi| {
            pi.img.put_pixel(1, 1, Rgba([0, 0, 255, 255]));
            Some([0, 0, 20, 10])
        });
        pe.set_selection(pixel_edit::rect_mask(20, 10, (0.0, 0.0), (3.0, 3.0)));
        pe.tool = PixelTool::Move;
        pe.placement.dx = 5.0;
        pe.apply_placement(None);
        assert_eq!(*pe.pi.img.get_pixel(6, 1), Rgba([0, 0, 255, 255]));
        // What was selected left a clear place; the rest stayed.
        assert_eq!(pe.pi.img.get_pixel(1, 1)[3], 0);
        assert_eq!(*pe.pi.img.get_pixel(15, 8), Rgba([255, 255, 255, 255]));
        assert_eq!(pe.pi.sel_bounds(), Some([5, 0, 8, 3]));
    }

    #[test]
    fn a_warp_previews_then_applies_as_one_undo_step() {
        let (mut app, id) = app_with_picture();
        app.edit_bitmap_inside(id);
        let pe = app.pixel_edit.as_mut().unwrap();
        pe.apply(false, |pi| {
            for y in 0..10 {
                pi.img.put_pixel(4, y, Rgba([0, 0, 0, 255]));
            }
            Some([4, 0, 5, 10])
        });
        let before = pe.pi.img.clone();
        pe.warp_grid = 2;
        pe.start_warp();
        let w = pe.warp.as_mut().unwrap();
        assert_eq!(pe.warp_box, [0, 0, 20, 10]);
        // Every point of the grid moved 3 pixels right.
        for p in w.pts.iter_mut() {
            p.0 += 3.0;
        }
        pe.refresh_warp();
        assert_eq!(pe.preview.as_ref().unwrap().get_pixel(7, 5)[0], 0);
        assert_eq!(pe.pi.img, before, "only previewed");
        pe.cancel_warp();
        assert!(pe.preview.is_none() && pe.warp.is_none());
        pe.start_warp();
        for p in pe.warp.as_mut().unwrap().pts.iter_mut() {
            p.0 += 3.0;
        }
        pe.apply_warp();
        assert_eq!(pe.pi.img.get_pixel(7, 5)[0], 0);
        pe.undo();
        assert_eq!(pe.pi.img, before);
    }

    #[test]
    fn a_dragged_shape_is_drawn_as_one_undo_step() {
        let (mut app, id) = app_with_picture();
        app.edit_bitmap_inside(id);
        let pe = app.pixel_edit.as_mut().unwrap();
        pe.tool = PixelTool::Shape;
        pe.opts.color = [255, 0, 0, 255];
        pe.opts.opacity = 1.0;
        pe.drag_from = Some((2.0, 2.0));
        pe.drag_to = Some((12.0, 8.0));
        pixel_release(pe, Modifiers::NONE);
        assert_eq!(*pe.pi.img.get_pixel(6, 5), Rgba([255, 0, 0, 255]));
        assert_eq!(*pe.pi.img.get_pixel(16, 5), Rgba([255, 255, 255, 255]));
        pe.undo();
        assert_eq!(*pe.pi.img.get_pixel(6, 5), Rgba([255, 255, 255, 255]));
    }

    #[test]
    fn the_remove_brush_shows_what_is_painted_then_fills_it_as_one_step() {
        let (mut app, id) = app_with_picture();
        app.edit_bitmap_inside(id);
        let pe = app.pixel_edit.as_mut().unwrap();
        pe.pi.img.put_pixel(10, 5, Rgba([0, 0, 0, 255]));
        pe.opts.size = 4.0;
        pe.opts.hardness = 1.0;
        pe.opts.opacity = 1.0;
        pe.opts.flow = 1.0;
        let m = image::GrayImage::from_pixel(20, 10, image::Luma([255]));
        pe.mask_stroke = Some(MaskStroke::new(pe.opts, &m, 0));
        pe.remove = Some(m);
        pe.mask_to(10.5, 5.5);
        assert_eq!(
            pe.target_mask().unwrap().get_pixel(10, 5)[0],
            0,
            "shown in red"
        );
        let painted = pe.remove.take().unwrap();
        pe.mask_stroke = None;
        let grow_by = pe.opts.remove_grow as u32;
        assert!(pe.apply(false, |pi| pixel_edit::remove_painted(
            pi, &painted, grow_by
        )));
        assert_eq!(*pe.pi.img.get_pixel(10, 5), Rgba([255, 255, 255, 255]));
        assert!(pe.target_mask().is_none());
        pe.undo();
        assert_eq!(*pe.pi.img.get_pixel(10, 5), Rgba([0, 0, 0, 255]));
    }

    #[test]
    fn the_new_selection_tools_select() {
        let (mut app, id) = app_with_picture();
        app.edit_bitmap_inside(id);
        let pe = app.pixel_edit.as_mut().unwrap();
        // Band: the band dragged across.
        pe.tool = PixelTool::BandMask;
        pe.opts.band_soft = 0.0;
        pe.drag_from = Some((5.0, 5.0));
        pe.drag_to = Some((10.0, 5.0));
        pixel_release(pe, Modifiers::NONE);
        let sel = pe.pi.sel.clone().unwrap();
        assert_eq!(sel.get_pixel(7, 2)[0], 255);
        assert_eq!(sel.get_pixel(15, 2)[0], 0);
        // Freehand: clicked corners closed; Shift+Ctrl keeps where only
        // one of the two is.
        pe.tool = PixelTool::FreehandMask;
        pe.lasso = vec![(0.0, 0.0), (8.0, 0.0), (8.0, 10.0), (0.0, 10.0)];
        let m = Modifiers::SHIFT | Modifiers::CTRL;
        close_freehand(pe, select_mode(pe.opts.sel_mode, m));
        let sel = pe.pi.sel.clone().unwrap();
        assert_eq!(sel.get_pixel(2, 5)[0], 255, "only the freehand");
        assert_eq!(sel.get_pixel(7, 5)[0], 0, "both");
        assert_eq!(sel.get_pixel(9, 5)[0], 255, "only the band");
        assert!(pe.lasso.is_empty());
        // The mode on the bar when no key is held.
        pe.opts.sel_mode = SelectMode::Subtract;
        assert_eq!(select_mode(pe.opts.sel_mode, Modifiers::NONE), SelectMode::Subtract);
        assert_eq!(select_mode(pe.opts.sel_mode, Modifiers::SHIFT), SelectMode::Add);
    }

    #[test]
    fn layer_fade_fades_the_layer_in_one_step() {
        let (mut app, id) = app_with_picture();
        app.edit_bitmap_inside(id);
        let pe = app.pixel_edit.as_mut().unwrap();
        pe.tool = PixelTool::LayerFade;
        pe.drag_from = Some((0.0, 5.0));
        pe.drag_to = Some((20.0, 5.0));
        pixel_release(pe, Modifiers::NONE);
        assert!(pe.pi.img.get_pixel(19, 5)[3] < 30);
        pe.undo();
        assert_eq!(pe.pi.img.get_pixel(19, 5)[3], 255);
    }

    #[test]
    fn the_liquify_tools_each_move_the_pixels_their_way() {
        use crate::pixel_liquify::LiquifyMode as M;
        let chosen = M::Bloat;
        assert_eq!(PixelTool::Liquify.liquify_mode(chosen, false), Some(M::Bloat));
        assert_eq!(PixelTool::Smear.liquify_mode(chosen, false), Some(M::Push));
        assert_eq!(PixelTool::Twirl.liquify_mode(chosen, false), Some(M::TwirlRight));
        assert_eq!(PixelTool::Twirl.liquify_mode(chosen, true), Some(M::TwirlLeft));
        assert_eq!(PixelTool::Attract.liquify_mode(chosen, false), Some(M::Pinch));
        assert_eq!(PixelTool::Repel.liquify_mode(chosen, false), Some(M::Bloat));
        assert_eq!(PixelTool::Brush.liquify_mode(chosen, false), None);
    }

    /// #4369: a layer's style shows when the layers are put together,
    /// stays changeable (off again leaves the pixels as they were), is
    /// copied with the layer and becomes pixels on Merge Down.
    #[test]
    fn layer_styles_show_live_and_merge_down_keeps_them() {
        let (mut app, id) = app_with_picture();
        app.edit_bitmap_inside(id);
        let pe = app.pixel_edit.as_mut().unwrap();
        pe.pi.add_layer("top");
        let (w, h) = pe.pi.img.dimensions();
        pe.pi.img = RgbaImage::new(w, h);
        for y in 3..7 {
            for x in 6..12 {
                pe.pi.img.put_pixel(x, y, Rgba([255, 0, 0, 255]));
            }
        }
        let plain = pe.pi.composite();
        let mut st = crate::pixel_style::LayerStyle::default();
        st.stroke.on = true;
        st.stroke.size = 2.0;
        st.stroke.color = [0, 0, 255];
        let i = pe.pi.active;
        pe.pi.layers[i].style = Some(st);
        assert!(pe.pi.is_layered());
        let styled = pe.pi.composite();
        assert_eq!(styled.get_pixel(5, 4), &Rgba([0, 0, 255, 255]), "outline left of the red");
        assert_eq!(styled.get_pixel(8, 4), &Rgba([255, 0, 0, 255]));
        // A part of the picture is styled as the whole is.
        let part = pe.pi.composite_region([4, 2, 8, 6], None).unwrap();
        assert_eq!(part.get_pixel(1, 2), styled.get_pixel(5, 4));
        pe.pi.layers[i].style.as_mut().unwrap().on = false;
        assert_eq!(pe.pi.composite(), plain);
        pe.pi.layers[i].style.as_mut().unwrap().on = true;
        pe.pi.duplicate_layer("copy");
        assert!(pe.pi.layers[pe.pi.active].style.is_some());
        pe.pi.delete_layer();
        assert!(pe.pi.merge_down());
        assert_eq!(pe.pi.img.get_pixel(5, 4), &Rgba([0, 0, 255, 255]));
    }

    /// #4370: the Text tool's layer keeps its text: changing it draws it
    /// again, painting on it makes it pixels, an empty one goes when the
    /// typing ends, and it is saved with the layers.
    #[test]
    fn text_layers_stay_text_until_painted_and_empty_ones_go() {
        tracedraw_text::install();
        let (mut app, id) = app_with_picture();
        app.edit_bitmap_inside(id);
        let pe = app.pixel_edit.as_mut().unwrap();
        let n = pe.pi.layers.len();
        pe.tool = PixelTool::Text;
        pe.start_text(2.0, 8.0);
        assert_eq!(pe.pi.layers.len(), n + 1);
        assert!(pe.text_edit.is_some() && pe.active_text().is_some());
        // Nothing typed: the layer goes when the typing ends.
        pe.end_text();
        assert_eq!(pe.pi.layers.len(), n);
        pe.start_text(2.0, 8.0);
        pe.change_text(|t| {
            t.text = "Hi".into();
            t.size = 8.0;
        });
        pe.end_text();
        assert_eq!(pe.pi.layers.len(), n + 1);
        let i = pe.pi.active;
        assert_eq!(pe.pi.layers[i].name, "Hi");
        let text = pe.active_text().cloned().unwrap();
        let png = crate::bitmap_fx::encode(&pe.pi.composite()).unwrap();
        let saved = crate::pixel_keep::save(&pe.pi, &png).unwrap();
        let (layers, _) = crate::pixel_keep::load(&saved, &png).unwrap();
        assert_eq!(layers[i].text.as_ref(), Some(&text));
        // Moved as a whole by the Move tool: still text, further right.
        pe.placement.dx = 3.0;
        pe.apply_placement(None);
        assert_eq!(pe.active_text().unwrap().at.0, text.at.0 + 3.0);
        // An effect on its pixels makes it plain pixels.
        pe.apply(false, |pi| pixel_edit::fill_selection(pi, [0, 0, 0, 255], 1.0));
        assert!(pe.active_text().is_none());
        pe.undo();
        assert!(pe.active_text().is_some(), "undo brings the text back");
    }

    /// #4394: the clone brush copies its source mirrored when asked, and
    /// from every layer onto a clear one.
    #[test]
    fn the_clone_brush_mirrors_and_copies_from_all_layers() {
        let mut pi = PixelImage::new(RgbaImage::from_fn(40, 10, |x, _| {
            if x < 20 {
                Rgba([x as u8 * 10, 0, 0, 255])
            } else {
                Rgba([0, 0, 0, 255])
            }
        }));
        let mut opts = BrushOpts { size: 9.0, hardness: 1.0, flow: 1.0, ..Default::default() };
        opts.clone_flip_h = true;
        let xform = pixel_edit::CloneXform {
            anchor: (30.5, 5.5),
            src: (10.5, 5.5),
            angle: 0.0,
            scale: 1.0,
            flip_h: true,
            flip_v: false,
        };
        let mut s = Stroke::new(PixelTool::Clone, opts, &pi, (-20.0, 0.0)).with_clone(xform, None);
        s.to(&mut pi, 30.5, 5.5);
        // Right of the anchor comes from left of the source: mirrored.
        assert_eq!(pi.img.get_pixel(32, 5)[0], 80);
        assert_eq!(pi.img.get_pixel(28, 5)[0], 120);
        // From all layers onto a clear layer.
        pi.add_layer("clone");
        let all = std::rc::Rc::new(pi.composite());
        let plain = pixel_edit::CloneXform { flip_h: false, ..xform };
        let mut s = Stroke::new(PixelTool::Clone, opts, &pi, (-20.0, 0.0)).with_clone(plain, Some(all));
        s.to(&mut pi, 30.5, 5.5);
        assert_eq!(pi.img.get_pixel(30, 5), &Rgba([100, 0, 0, 255]));
        assert_eq!(pi.img.get_pixel(5, 5)[3], 0, "the rest stays clear");
        assert_eq!(pi.layers[0].img.get_pixel(5, 5), &Rgba([50, 0, 0, 255]), "beneath unchanged");
    }

    /// #4391 #4393: the selection's commands from the right-click menu:
    /// a stroke along its edge, expand and contract by the pixels asked,
    /// and the tool's feather on new selections.
    #[test]
    fn selection_stroke_expand_contract_and_feather() {
        let (mut app, id) = app_with_picture();
        app.edit_bitmap_inside(id);
        let pe = app.pixel_edit.as_mut().unwrap();
        let (w, h) = pe.pi.img.dimensions();
        let m = pixel_edit::rect_mask(w, h, (6.0, 3.0), (14.0, 7.0)).unwrap();
        pe.set_selection(Some(m.clone()));
        let ask = PxAsk {
            cmd: PixelCmd::StrokeSelection,
            px: 1.0,
            color: [0, 0, 0, 255],
            opacity: 1.0,
            at: crate::pixel_style::StrokeAt::Outside,
            pos: None,
        };
        crate::ui::pixel_canvas::run_asked(&mut app, ask);
        let pe = app.pixel_edit.as_mut().unwrap();
        assert_eq!(pe.pi.img.get_pixel(5, 4), &Rgba([0, 0, 0, 255]));
        assert_eq!(pe.pi.img.get_pixel(8, 4), &Rgba([255, 255, 255, 255]));
        let ask = PxAsk { cmd: PixelCmd::Expand, px: 2.0, ..ask };
        crate::ui::pixel_canvas::run_asked(&mut app, ask);
        let pe = app.pixel_edit.as_mut().unwrap();
        assert_eq!(pe.pi.sel.as_ref().unwrap().get_pixel(4, 4)[0], 255);
        let ask = PxAsk { cmd: PixelCmd::Contract, px: 3.0, ..ask };
        crate::ui::pixel_canvas::run_asked(&mut app, ask);
        let pe = app.pixel_edit.as_mut().unwrap();
        let sel = pe.pi.sel.clone().unwrap();
        assert_eq!(sel.get_pixel(6, 4)[0], 0);
        assert_eq!(sel.get_pixel(10, 5)[0], 255);
        // The tool's feather softens a new selection's edge.
        pe.set_selection(None);
        pe.opts.sel_feather = 2.0;
        pe.select_new(m, SelectMode::Replace);
        let v = pe.pi.sel.as_ref().unwrap().get_pixel(6, 5)[0];
        assert!(v > 0 && v < 255, "{v}");
    }
}
