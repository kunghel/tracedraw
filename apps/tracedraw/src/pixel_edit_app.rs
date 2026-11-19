//! Bitmaps > Edit Inside: a double click on a picture opens its pixels
//! for editing in place, as a clip frame's contents are edited. The
//! picture's pixels are worked on in memory (see `pixel_edit.rs`) and
//! drawn over the page where the picture lies; Finish puts them back in
//! the picture in one undo step, Cancel leaves it as it was. See
//! `docs/behavior/edit-inside-picture.md`.

use crate::app::App;
use crate::i18n::tr;
use crate::pixel_edit::{
    self, BrushOpts, Dirty, History, PixelImage, PixelTool, SelectMode, Stroke,
};
use egui::{Modifiers, PointerButton, Response};
use std::cell::RefCell;
use tracedraw_core::geometry::{Affine, Point, Rect};
use tracedraw_core::{BitmapEffect, Command, Document, ShapeId, ShapeKind};

/// The undo label of Finish.
const LABEL: &str = "Edit Picture";

/// What the screen holds of the picture being edited.
#[derive(Default)]
pub struct PixelView {
    pub tex: Option<egui::TextureHandle>,
    pub outline: Option<egui::TextureHandle>,
    /// Pixels changed since the picture was last sent to the screen.
    pub dirty: Option<Dirty>,
    /// The whole picture is sent again (new size, undo, preview).
    pub full: bool,
    /// The selection outline is made again.
    pub sel_changed: bool,
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
    pub view: RefCell<PixelView>,
    /// The drawing without the picture, for the page under the edit.
    pub hidden: RefCell<Option<(u64, std::rc::Rc<Document>)>>,
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

    /// Remember the picture before a change.
    fn step(&mut self) {
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
        if self.history.undo(&mut self.pi) {
            self.mark_all();
            self.preview = None;
        }
    }

    pub fn redo(&mut self) {
        if self.history.redo(&mut self.pi) {
            self.mark_all();
            self.preview = None;
        }
    }

    /// Run an operation on the picture as one undo step; `full` when it
    /// may change the size or every pixel.
    pub fn apply(&mut self, full: bool, f: impl FnOnce(&mut PixelImage) -> Option<Dirty>) -> bool {
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
        self.pixel_edit = Some(Box::new(PixelEdit {
            shape: id,
            img_to_page,
            rect: *rect,
            size_px,
            pi: PixelImage::new(img),
            history: History::default(),
            tool: PixelTool::Brush,
            opts: self.pixel_opts,
            stroke: None,
            drag_from: None,
            drag_to: None,
            lasso: Vec::new(),
            clone_src: None,
            clone_offset: None,
            changed: false,
            effect: None,
            preview_on: true,
            preview: None,
            feather_px: 4,
            grow_px: 2,
            view: RefCell::new(PixelView {
                full: true,
                sel_changed: true,
                ..Default::default()
            }),
            hidden: RefCell::new(None),
        }));
        // No selection handles over the pixels being edited.
        self.selection.clear();
        self.status = tr("pixel.status.editing");
        true
    }

    /// Finish editing the picture: its pixels go back into it in one undo
    /// step (nothing changes when nothing was changed).
    pub fn finish_pixel_edit(&mut self) {
        let Some(pe) = self.pixel_edit.take() else {
            return;
        };
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
        let Some(png) = crate::bitmap_fx::encode(&pe.pi.img) else {
            self.status = tr("pixel.status.unreadable");
            return;
        };
        let (w, h) = pe.pi.img.dimensions();
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
        response.ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
        let Some(pe) = self.pixel_edit.as_mut() else {
            return;
        };
        let Some(p) = pointer else {
            if response.drag_stopped() {
                pixel_release(pe, mods);
            }
            return;
        };
        let (x, y) = pe.to_px(p);
        let started = response.drag_started_by(PointerButton::Primary);
        let dragged = response.dragged_by(PointerButton::Primary);
        let stopped = response.drag_stopped_by(PointerButton::Primary);
        let clicked = response.clicked_by(PointerButton::Primary);
        let (w, h) = pe.pi.img.dimensions();
        let inside = x >= 0.0 && y >= 0.0 && x < w as f32 && y < h as f32;
        let mode = if mods.shift {
            SelectMode::Add
        } else if mods.command || mods.ctrl {
            SelectMode::Subtract
        } else {
            SelectMode::Replace
        };
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
            PixelTool::RectSelect | PixelTool::EllipseSelect | PixelTool::Gradient => {
                if started {
                    pe.drag_from = Some((x, y));
                    pe.drag_to = Some((x, y));
                } else if dragged {
                    pe.drag_to = Some((x, y));
                } else if clicked && tool != PixelTool::Gradient && mode == SelectMode::Replace {
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
                if let Some(q) = pe.pi.img.get_pixel_checked(x as u32, y as u32) {
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
            self.finish_pixel_edit();
            return;
        }
        let Some(pe) = self.pixel_edit.as_mut() else {
            return;
        };
        if pressed(Key::Escape, Modifiers::NONE) {
            // Esc drops the selection first, then ends the editing.
            if pe.pi.sel.is_some() {
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
        } else if pressed(Key::OpenBracket, Modifiers::NONE) {
            pe.opts.size = (pe.opts.size / 1.2).max(1.0);
        } else if pressed(Key::CloseBracket, Modifiers::NONE) {
            pe.opts.size = (pe.opts.size * 1.2).min(2000.0);
        } else {
            for (k, t) in [
                (Key::B, PixelTool::Brush),
                (Key::E, PixelTool::Eraser),
                (Key::S, PixelTool::Clone),
                (Key::J, PixelTool::Heal),
                (Key::M, PixelTool::RectSelect),
                (Key::L, PixelTool::Lasso),
                (Key::W, PixelTool::MagicWand),
                (Key::G, PixelTool::Fill),
                (Key::O, PixelTool::Dodge),
                (Key::I, PixelTool::Eyedropper),
                (Key::R, PixelTool::Blur),
            ] {
                if pressed(k, Modifiers::NONE) {
                    pe.tool = t;
                }
            }
        }
    }
}

/// The end of a drag of a selection tool or the gradient.
fn pixel_release(pe: &mut PixelEdit, mods: Modifiers) {
    let mode = if mods.shift {
        SelectMode::Add
    } else if mods.command || mods.ctrl {
        SelectMode::Subtract
    } else {
        SelectMode::Replace
    };
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
    if v.tex.is_none() || v.full || size_changed || preview || v.showed_preview {
        let img =
            egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], shown.as_raw());
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
            for y in y0..y1 {
                let row = (y * w + x0) as usize * 4;
                let end = (y * w + x1) as usize * 4;
                if let Some(px) = shown.as_raw().get(row..end) {
                    part.extend_from_slice(px);
                }
            }
            let img = egui::ColorImage::from_rgba_unmultiplied(
                [(x1 - x0) as usize, (y1 - y0) as usize],
                &part,
            );
            if let Some(t) = v.tex.as_mut() {
                t.set_partial([x0 as usize, y0 as usize], img, opts);
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
    .map(|(x, y)| app.view.to_screen(pe.to_page(x, y)));
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
    if let Some(t) = &v.outline {
        painter.add(quad(t.id()));
    }
    // The picture's edge.
    let edge = egui::Stroke::new(1.0, egui::Color32::from_rgb(0x1E, 0x7C, 0xE8));
    for i in 0..4 {
        painter.line_segment([corners[i], corners[(i + 1) % 4]], edge);
    }
    drop(v);
    let to_screen = |p: (f32, f32)| app.view.to_screen(pe.to_page(p.0, p.1));
    let ink = egui::Stroke::new(1.0, egui::Color32::from_rgb(0x1E, 0x7C, 0xE8));
    if let (Some(a), Some(b)) = (pe.drag_from, pe.drag_to) {
        match pe.tool {
            PixelTool::RectSelect | PixelTool::Gradient if pe.tool == PixelTool::Gradient => {
                painter.line_segment([to_screen(a), to_screen(b)], ink);
            }
            PixelTool::RectSelect => {
                let pts = [a, (b.0, a.1), b, (a.0, b.1), a].map(to_screen);
                painter.add(egui::Shape::line(pts.to_vec(), ink));
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
    if pe.lasso.len() > 1 {
        let pts: Vec<_> = pe.lasso.iter().map(|p| to_screen(*p)).collect();
        painter.add(egui::Shape::line(pts, ink));
    }
    // The brush's size under the pointer, and the clone source.
    if pe.tool.is_brush() {
        if let Some(pp) = app.pointer_page {
            let (x, y) = pe.to_px(pp);
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
            let src = match (pe.clone_offset, app.pointer_page, pe.stroke.is_some()) {
                (Some(o), Some(pp), _) => {
                    let (x, y) = pe.to_px(pp);
                    Some((x + o.0, y + o.1))
                }
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
}
