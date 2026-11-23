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
    pub outline: Option<egui::TextureHandle>,
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
}

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
    /// Move: the layer's pixels when the drag started, and how far the
    /// drag has moved them.
    pub move_base: Option<image::RgbaImage>,
    pub move_by: (i64, i64),
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
        }
    }

    fn mark_all(&self) {
        let mut v = self.view.borrow_mut();
        v.full = true;
        v.sel_changed = true;
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
    }

    pub fn set_selection(&mut self, sel: Option<image::GrayImage>) {
        self.step();
        self.pi.sel = sel;
        self.view.borrow_mut().sel_changed = true;
    }

    pub fn undo(&mut self) {
        // Live lights undo their strokes one by one first.
        if self.lights.is_live() && self.undo_light() {
            return;
        }
        if self.history.undo(&mut self.pi) {
            self.mark_all();
            self.preview = None;
        }
    }

    pub fn redo(&mut self) {
        self.lights.bake();
        if self.history.redo(&mut self.pi) {
            self.mark_all();
            self.preview = None;
        }
    }

    /// Run an operation on the pixels of the layer in use as one undo
    /// step; `full` when it may change the size or every pixel. Nothing
    /// happens on a locked layer.
    pub fn apply(&mut self, full: bool, f: impl FnOnce(&mut PixelImage) -> Option<Dirty>) -> bool {
        if self.pi.active_locked() {
            return false;
        }
        self.apply_all(full, f)
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

    /// The effect being set up, run on the picture inside the selection,
    /// for the preview.
    pub fn refresh_preview(&mut self) {
        self.preview = None;
        if !self.preview_on {
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

impl App {
    /// Open the picture `id` for editing in place (Bitmaps > Edit Inside,
    /// a double click on it). False when it is not a picture or its
    /// pixels cannot be read.
    pub fn edit_bitmap_inside(&mut self, id: ShapeId) -> bool {
        if self.pixel_edit.is_some() {
            self.finish_pixel_edit();
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
        static SESSIONS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let session = SESSIONS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.pixel_edit = Some(Box::new(PixelEdit {
            session,
            shape: id,
            img_to_page,
            rect: *rect,
            size_px,
            original: std::rc::Rc::new(img.clone()),
            liquify: None,
            spray: None,
            placement: Default::default(),
            smart_size: (100.0, 100.0),
            depth: None,
            remove: None,
            adjust_drag: false,
            pi: PixelImage::new(img),
            history: History::default(),
            tool: PixelTool::Brush,
            opts: self.pixel_opts,
            stroke: None,
            drag_from: None,
            drag_to: None,
            lasso: Vec::new(),
            move_base: None,
            move_by: (0, 0),
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
        }));
        // No selection handles over the pixels being edited.
        self.selection.clear();
        self.status = tr("pixel.status.editing");
        crate::ui::pixel_mode::enter(self);
        true
    }

    /// Finish editing the picture: its pixels go back into it in one undo
    /// step (nothing changes when nothing was changed).
    pub fn finish_pixel_edit(&mut self) {
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
        // edit: it stands alone now.
        if s.data.iter().any(|(k, _)| k == "bitmap.link") {
            let mut data = s.data.clone();
            data.retain(|(k, _)| k != "bitmap.link");
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
        let Some((x, y)) = px else {
            if response.drag_stopped() {
                pe.lights.end_stroke();
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
        if paints && pe.pi.active_locked() {
            if started || clicked {
                self.status = tr("pixel.status.layer_locked");
            }
            return;
        }
        if tool == PixelTool::Warp {
            if pe.warp.is_none() && (started || clicked) {
                pe.start_warp();
            }
            // Points within 12 screen pixels of the press are taken.
            let grab = 12.0 / screen_per_px.max(1e-6);
            if started {
                let pick = pe.warp.as_ref().and_then(|w| {
                    let near = w
                        .pts
                        .iter()
                        .enumerate()
                        .map(|(k, p)| (k, (p.0 - x).hypot(p.1 - y)))
                        .min_by(|a, b| a.1.total_cmp(&b.1))?;
                    (near.1 <= grab).then_some(near.0)
                });
                if let Some(w) = pe.warp.as_mut() {
                    w.drag = pick;
                }
            } else if dragged {
                if let Some(w) = pe.warp.as_mut() {
                    if let Some(k) = w.drag {
                        if let Some(p) = w.pts.get_mut(k) {
                            *p = (x, y);
                        }
                    }
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
            if started {
                pe.step();
                pe.move_base = Some(pe.pi.img.clone());
                pe.move_by = (0, 0);
                pe.drag_from = Some((x, y));
            } else if dragged {
                if let (Some(base), Some(from)) = (pe.move_base.as_ref(), pe.drag_from) {
                    let by = ((x - from.0).round() as i64, (y - from.1).round() as i64);
                    if by != pe.move_by {
                        pe.move_by = by;
                        pe.pi.img = pixel_edit::offset_image(base, by.0, by.1);
                        pe.view.borrow_mut().full = true;
                    }
                }
            }
            if stopped {
                pe.move_base = None;
                pe.drag_from = None;
            }
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
                    let sel = pixel_edit::combine(pe.pi.sel.as_ref(), m, mode);
                    pe.set_selection(sel);
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
                Some(pe) if pe.warp.is_some() => pe.apply_warp(),
                _ => self.finish_pixel_edit(),
            }
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
            if !pe.lasso.is_empty() {
                pe.lasso.clear();
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
        let sel = pixel_edit::combine(pe.pi.sel.as_ref(), m, mode);
        pe.set_selection(sel);
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
                let sel = pixel_edit::combine(pe.pi.sel.as_ref(), m, mode);
                pe.set_selection(sel);
            }
        }
        PixelTool::Lasso => {
            if let Some(m) = pixel_edit::lasso_mask(w, h, &lasso) {
                let sel = pixel_edit::combine(pe.pi.sel.as_ref(), m, mode);
                pe.set_selection(sel);
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
                let sel = pixel_edit::combine(pe.pi.sel.as_ref(), m, mode);
                pe.set_selection(sel);
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
    } else if let Some(b) = v.dirty.take() {
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
        v.outline = pe.pi.sel.as_ref().map(|m| {
            let o = pixel_edit::selection_outline(m);
            let img = egui::ColorImage::from_rgba_unmultiplied(
                [o.width() as usize, o.height() as usize],
                o.as_raw(),
            );
            ctx.load_texture("pixel_edit_outline", img, opts)
        });
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
    if pe.target != MaskTarget::QuickMask {
        if let Some(t) = &v.outline {
            painter.add(quad(t.id()));
        }
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
        for p in &wp.pts {
            let c = to_screen(*p);
            painter.circle(c, 4.0, egui::Color32::WHITE, line);
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

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
}
