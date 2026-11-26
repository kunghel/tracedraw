//! While a contour, a drop shadow or a block shadow is dragged, the page's
//! picture is drawn again on its own thread and can take a while with a
//! soft shadow or many contour steps: until it comes, the object is drawn
//! alone at a low resolution over the page, with its effect as it stands,
//! so the effect follows the pointer at once (D#4226).

use crate::app::{App, Drag};
use egui::{Color32, Painter, Pos2, Rect as ERect, TextureHandle};
use tracedraw_core::document::{Document, Shape};
use tracedraw_core::geometry::Rect;
use tracedraw_core::{ShapeId, Size};

/// The longest side of the low-resolution picture, in pixels.
const PREVIEW_PX: f64 = 480.0;

/// The low-resolution picture last drawn: for which drawing, object and
/// pixel size, and the part of the page (mm) it covers.
pub struct EffectPreview {
    key: (u64, ShapeId, u64),
    texture: TextureHandle,
    area: Rect,
}

impl App {
    /// The object whose contour or shadow a drag is changing.
    pub fn effect_drag_shape(&self) -> Option<ShapeId> {
        match &self.drag {
            Drag::ContourDrag { .. } => self.contour_drag.map(|d| d.id),
            Drag::Shadow { shape, .. } => Some(*shape),
            Drag::ShadowHandle { .. } | Drag::ShadowLine { .. } | Drag::OffsetShadow { .. } => {
                self.selection.first().copied()
            }
            _ => None,
        }
    }
}

/// What `shape` draws on the page: itself, its live effects and its
/// drop shadow with the shadow's feathered edge.
pub fn drawn_area(doc: &Document, shape: &Shape) -> Rect {
    let b = shape.visual_bounds_with(&doc.symbols);
    match shape.shadow.filter(|sh| sh.opacity > 0.0) {
        Some(sh) => b.union(sh.bounds(shape.shadow_box())),
        None => b,
    }
}

/// `shape` (on the page) drawn alone over `area` at `zoom` pixels per mm:
/// premultiplied RGBA and its size.
pub fn render_alone(doc: &Document, shape: &Shape, area: Rect, zoom: f64) -> Option<(Vec<u8>, u32, u32)> {
    let w = (area.width() * zoom).ceil().clamp(1.0, 4096.0) as u32;
    let h = (area.height() * zoom).ceil().clamp(1.0, 4096.0) as u32;
    let size = doc.pages.first().map(|p| p.size).unwrap_or(Size::new(210.0, 297.0));
    let mut tmp = Document::new("effect preview", size);
    tmp.inks = doc.inks.clone();
    tmp.fonts = doc.fonts.clone();
    tmp.symbols = doc.symbols.clone();
    tmp.metadata.fill_open_curves = doc.metadata.fill_open_curves;
    let page = tmp.pages.first()?.id;
    let layer = tmp.pages.first()?.layers.first()?.id;
    tmp.layer_mut(layer).ok()?.shapes.push(shape.clone());
    let pm = tracedraw_render::render_page(
        &tmp,
        page,
        &tracedraw_render::RenderOptions {
            width: w,
            height: h,
            view: tracedraw_render::ViewTransform {
                zoom,
                origin_x: -area.x0 * zoom,
                origin_y: area.y1 * zoom,
                stretch_y: 1.0,
            },
            draft: true,
            ..tracedraw_render::RenderOptions::default()
        },
    )?;
    Some((pm.data().to_vec(), w, h))
}

/// Draw the low-resolution picture of the object whose effect is being
/// dragged, while the page's picture (of drawing `shown`, if any) is not
/// yet of the drawing as it is now (`revision`).
pub fn paint(app: &App, painter: &Painter, revision: u64, shown: Option<u64>) {
    let Some(id) = app.effect_drag_shape() else {
        if app.effect_preview.borrow().is_some() {
            *app.effect_preview.borrow_mut() = None;
        }
        return;
    };
    if shown == Some(revision) {
        return;
    }
    let Some(shape) = app.doc().shape_on_page(id) else {
        return;
    };
    let mut area = drawn_area(app.doc(), &shape);
    let longest = area.width().max(area.height());
    if !(longest.is_finite() && longest > 0.0) {
        return;
    }
    let pad = longest * 0.02;
    area = area.inflate(pad, pad);
    let longest = area.width().max(area.height());
    let ppp = f64::from(painter.ctx().pixels_per_point());
    let screen_zoom = f64::from(app.view.zoom) * ppp;
    let zoom = screen_zoom.min(PREVIEW_PX / longest).max(1e-3);
    let key = (revision, id, zoom.to_bits());
    let mut cached = app.effect_preview.borrow_mut();
    if cached.as_ref().is_none_or(|c| c.key != key) {
        let Some((rgba, w, h)) = render_alone(app.doc(), &shape, area, zoom) else {
            return;
        };
        let image = egui::ColorImage::from_rgba_premultiplied([w as usize, h as usize], &rgba);
        let texture = match cached.take() {
            Some(mut c) => {
                c.texture.set(image, egui::TextureOptions::LINEAR);
                c.texture
            }
            None => painter
                .ctx()
                .load_texture("effect_drag_preview", image, egui::TextureOptions::LINEAR),
        };
        *cached = Some(EffectPreview { key, texture, area });
    }
    let Some(c) = cached.as_ref() else {
        return;
    };
    let at = app.view.rect_to_screen(c.area);
    let full = ERect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
    painter.image(c.texture.id(), at, full, Color32::WHITE);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::Tool;
    use tracedraw_core::geometry::{Point, Vec2};
    use tracedraw_core::{Color, Fill, ShapeKind};

    /// A drop shadow dragged: the object drawn alone shows the shadow
    /// where it is now, beside the object (D#4226).
    #[test]
    fn a_dragged_shadow_shows_in_the_low_resolution_picture() {
        let mut app = App::headless();
        let id = app
            .new_shape(ShapeKind::Rect {
                rect: Rect::new(10.0, 10.0, 30.0, 30.0),
                radius: 0.0,
                corners: None,
            })
            .expect("a square");
        app.run(tracedraw_core::Command::SetFill {
            shapes: vec![id],
            fill: Fill::Solid(Color::rgb8(255, 0, 0)),
        });
        app.select(vec![id]);
        app.tool = Tool::DropShadow;
        let start = Point::new(20.0, 20.0);
        app.drag = Drag::Shadow { shape: id, start };
        app.drag_new_shadow(id, start, start + Vec2::new(15.0, -15.0), true);
        assert_eq!(app.effect_drag_shape(), Some(id));
        let shape = app.doc().shape_on_page(id).expect("the square");
        assert!(shape.shadow.is_some());
        let area = drawn_area(app.doc(), &shape);
        assert!(area.width() > 25.0, "the shadow widens it: {area:?}");
        let zoom = 4.0;
        let (rgba, w, h) = render_alone(app.doc(), &shape, area, zoom).expect("a picture");
        let painted = rgba.chunks_exact(4).filter(|p| p[3] > 0).count();
        assert!(painted > 0, "{w}x{h}");
        let dark = rgba
            .chunks_exact(4)
            .filter(|p| p[3] > 40 && p[0] < 120 && p[1] < 120 && p[2] < 120)
            .count();
        assert!(dark > 20, "the shadow is drawn: {dark}");
        app.drag = Drag::None;
        assert_eq!(app.effect_drag_shape(), None);
    }
}
