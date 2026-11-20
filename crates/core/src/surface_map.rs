//! The photo map of Displace (#3595): the lightness of a photo beneath an
//! object, sampled on a grid over the object's own box, so its outline
//! can follow the folds of a cloth, a paper or a wall in the photo.

use crate::document::{Document, Shape, ShapeKind};
use crate::id::ShapeId;
use crate::geometry::{Affine, Point, Rect, Shape as _};
use crate::live::SurfaceMap;

/// At most this many cells along the map's longer side.
pub const MAX_CELLS: u32 = 160;
/// The size of a cell on the page the map aims for, in mm.
pub const CELL_MM: f64 = 0.5;

/// A photo of the drawing with where its pixels lie on the page.
struct Photo<'a> {
    rect: Rect,
    png: &'a [u8],
    /// The photo's own space to the page.
    to_page: Affine,
}

/// The pictures in `shape` (it or inside its groups), drawn under
/// `parent`, bottom to top.
fn photos_in<'a>(shape: &'a Shape, parent: Affine, out: &mut Vec<Photo<'a>>) {
    let t = parent * shape.transform;
    match &shape.kind {
        ShapeKind::Bitmap { rect, png, .. } => out.push(Photo {
            rect: *rect,
            png,
            to_page: t,
        }),
        ShapeKind::Group { children } => {
            for c in children {
                photos_in(c, t, out);
            }
        }
        _ => {}
    }
}

/// The pictures drawn beneath the object `id` on its page (the layers
/// under its layer and the objects under it on its own), bottom to top.
fn photos_beneath(doc: &Document, id: ShapeId) -> Vec<Photo<'_>> {
    let mut out = Vec::new();
    for page in &doc.pages {
        let mut below = Vec::new();
        for layer in &page.layers {
            for s in &layer.shapes {
                if s.id == id || crate::document::holds(s, id) {
                    if !layer.visible {
                        return Vec::new();
                    }
                    out.extend(below);
                    return out;
                }
                if layer.visible {
                    photos_in(s, Affine::IDENTITY, &mut below);
                }
            }
        }
    }
    out
}

/// A photo's pixels decoded (straight RGBA, row 0 at its rect's top).
fn decode(png: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    if crate::jpeg_source::is_jpeg(png) {
        crate::jpeg_source::decode_rgba(png)
    } else {
        crate::bitmap_warp::decode_rgba(png)
    }
}

/// The map of the topmost photo beneath the object `id` that its box
/// overlaps, over its own box grown by `margin` mm (how far Displace may
/// push it), or `None` without such a photo.
pub fn sample(doc: &Document, id: ShapeId, margin: f64) -> Option<SurfaceMap> {
    let shape = doc.find_shape(id)?;
    let parent = doc.parent_transform(id).unwrap_or(Affine::IDENTITY);
    let t = parent * shape.transform;
    let det = t.determinant();
    if !det.is_finite() || det.abs() < 1e-12 {
        return None;
    }
    let inv = t.inverse();
    let [a, b, c, d, _, _] = inv.as_coeffs();
    let to_local = [a, b, c, d];
    let local = shape.local_path().bounding_box();
    // The margin in the object's own units, along each of its axes.
    let margin = if margin.is_finite() { margin.abs() } else { 0.0 };
    let (gx, gy) = (margin * a.hypot(b), margin * c.hypot(d));
    let rect = local.inflate(gx.max(gy), gx.max(gy));
    if !(rect.width() > 0.0 && rect.height() > 0.0) {
        return None;
    }
    let page_box = t.transform_rect_bbox(rect);
    let photos = photos_beneath(doc, id);
    let photo = photos.iter().rev().find(|p| {
        let r = p.to_page.transform_rect_bbox(p.rect);
        r.intersect(page_box).area() > 0.0
    })?;
    let (w, h, rgba) = decode(photo.png)?;
    if w == 0 || h == 0 || rgba.len() < (w * h * 4) as usize {
        return None;
    }
    let pdet = photo.to_page.determinant();
    if !pdet.is_finite() || pdet.abs() < 1e-12 {
        return None;
    }
    let to_photo = photo.to_page.inverse();
    // The grid: about half a millimetre a cell on the page.
    let (pw, ph) = (
        (t * Point::new(rect.x1, rect.y0) - t * Point::new(rect.x0, rect.y0)).hypot(),
        (t * Point::new(rect.x0, rect.y1) - t * Point::new(rect.x0, rect.y0)).hypot(),
    );
    let mut cols = (pw / CELL_MM).ceil().clamp(2.0, f64::from(MAX_CELLS));
    let mut rows = (ph / CELL_MM).ceil().clamp(2.0, f64::from(MAX_CELLS));
    let longer = cols.max(rows);
    if longer >= f64::from(MAX_CELLS) {
        let k = f64::from(MAX_CELLS) / longer;
        cols = (cols * k).round().max(2.0);
        rows = (rows * k).round().max(2.0);
    }
    let (cols, rows) = (cols as u32, rows as u32);
    let cell = (rect.width() / f64::from(cols), rect.height() / f64::from(rows));
    let lightness = |q: Point| -> Option<f64> {
        let l = to_photo * (t * q);
        let u = (l.x - photo.rect.x0) / photo.rect.width() * f64::from(w);
        let v = (photo.rect.y1 - l.y) / photo.rect.height() * f64::from(h);
        if !(u >= 0.0 && v >= 0.0 && u < f64::from(w) && v < f64::from(h)) {
            return None;
        }
        let i = ((v as u32).min(h - 1) * w + (u as u32).min(w - 1)) as usize * 4;
        let px = &rgba[i..i + 4];
        let al = f64::from(px[3]) / 255.0;
        let ch = |k: usize| f64::from(px[k]) / 255.0 * al + (1.0 - al);
        Some(crate::live::blend_if_lightness(ch(0), ch(1), ch(2)))
    };
    // How many samples across a cell: as many as photo pixels it covers.
    let footprint = {
        let pixel = (photo.to_page * Point::new(photo.rect.width() / f64::from(w), 0.0)
            - photo.to_page * Point::ZERO)
            .hypot();
        let across = (pw / f64::from(cols)).max(ph / f64::from(rows));
        if pixel > 0.0 && pixel.is_finite() {
            ((across / pixel).ceil() as usize).clamp(1, 4)
        } else {
            1
        }
    };
    let mut values: Vec<Option<f64>> = Vec::with_capacity((cols * rows) as usize);
    for j in 0..rows {
        for i in 0..cols {
            let mut sum = 0.0;
            let mut n = 0.0;
            for sj in 0..footprint {
                for si in 0..footprint {
                    let q = Point::new(
                        rect.x0 + (f64::from(i) + (si as f64 + 0.5) / footprint as f64) * cell.0,
                        rect.y0 + (f64::from(j) + (sj as f64 + 0.5) / footprint as f64) * cell.1,
                    );
                    if let Some(l) = lightness(q) {
                        sum += l;
                        n += 1.0;
                    }
                }
            }
            values.push((n > 0.0).then(|| sum / n));
        }
    }
    let seen: Vec<f64> = values.iter().flatten().copied().collect();
    if seen.is_empty() {
        return None;
    }
    let mean = seen.iter().sum::<f64>() / seen.len() as f64;
    let light = values
        .iter()
        .map(|v| (v.unwrap_or(mean) * 255.0).round().clamp(0.0, 255.0) as u8)
        .collect();
    Some(SurfaceMap {
        rect,
        cols,
        rows,
        light,
        mean,
        to_local,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::document::{Shape, ShapeKind};
    use crate::live::{Effect, SurfaceShade};

    /// A photo 40 by 20 mm, its left half black and its right half
    /// white, under a 20 by 10 mm rectangle across the middle.
    fn scene() -> (crate::Engine, ShapeId) {
        let mut e = crate::Engine::default();
        let layer = e.document().pages[0].layers[0].id;
        let (w, h) = (8u32, 4u32);
        let mut px = Vec::new();
        for _ in 0..h {
            for x in 0..w {
                let v = if x < w / 2 { 0 } else { 255 };
                px.extend_from_slice(&[v, v, v, 255]);
            }
        }
        let png = crate::bitmap_warp::encode_rgba(w, h, &px).expect("png");
        let photo = Shape::new(
            e.new_shape_id(),
            ShapeKind::Bitmap {
                rect: Rect::new(0.0, 0.0, 40.0, 20.0),
                width_px: w,
                height_px: h,
                png: png.into(),
                fx: None,
            },
        );
        e.run(&Command::AddShape { layer, shape: photo }).expect("photo");
        let id = e.new_shape_id();
        let rect = Shape::new(
            id,
            ShapeKind::Rect {
                rect: Rect::new(10.0, 5.0, 30.0, 15.0),
                radius: 0.0,
                corners: None,
            },
        );
        e.run(&Command::AddShape { layer, shape: rect }).expect("rect");
        (e, id)
    }

    /// The map of the photo beneath: dark on the left of the object's
    /// box, light on its right, the photo's mean grey in between; an
    /// object with no photo beneath it gets none, and the map follows the
    /// object's turn (`to_local`).
    #[test]
    fn the_map_reads_the_photo_beneath_the_object() {
        let (mut e, id) = scene();
        let map = sample(e.document(), id, 2.0).expect("map");
        assert!(map.cols >= 2 && map.rows >= 2);
        assert!((map.rect.x0 - 8.0).abs() < 1e-9 && (map.rect.x1 - 32.0).abs() < 1e-9);
        assert!(map.lightness(Point::new(11.0, 10.0)) < 0.05);
        assert!(map.lightness(Point::new(29.0, 10.0)) > 0.95);
        assert!((map.mean - 0.5).abs() < 0.1, "{}", map.mean);
        assert_eq!(map.to_local, [1.0, 0.0, 0.0, 1.0]);
        // Displaced: the dark side down and left, the light side up and
        // right, by the whole amount over pure black and white.
        let p = map.displace(Point::new(11.0, 10.0), 1.0, 2.0);
        assert!((p.x - 10.0).abs() < 0.15 && (p.y - 8.0).abs() < 0.3, "{p:?}");
        let p = map.displace(Point::new(29.0, 10.0), 1.0, 2.0);
        assert!((p.x - 30.0).abs() < 0.15 && (p.y - 12.0).abs() < 0.3, "{p:?}");
        // Moved off the photo: nothing beneath.
        e.run(&Command::TransformShapes {
            shapes: vec![id],
            transform: Affine::translate((100.0, 0.0)),
        })
        .expect("moved");
        assert!(sample(e.document(), id, 2.0).is_none());
        // Turned a quarter: page right is the object's own down.
        e.run(&Command::TransformShapes {
            shapes: vec![id],
            transform: Affine::translate((-100.0, 0.0)),
        })
        .expect("back");
        e.run(&Command::TransformShapes {
            shapes: vec![id],
            transform: Affine::rotate_about(std::f64::consts::FRAC_PI_2, Point::new(20.0, 10.0)),
        })
        .expect("turned");
        let map = sample(e.document(), id, 0.0).expect("map");
        let [a, b, c, d] = map.to_local;
        assert!(a.abs() < 1e-9 && (b + 1.0).abs() < 1e-9 && (c - 1.0).abs() < 1e-9 && d.abs() < 1e-9);
    }

    /// With Displace the object's outline bends with the photo: its left
    /// side goes down, its right side up, so the top edge slants; with
    /// no map it stays as it was.
    #[test]
    fn displace_bends_the_outline_with_the_photo() {
        let (e, id) = scene();
        let doc = e.document();
        let mut shape = doc.find_shape(id).expect("shape").clone();
        shape.effects.push(Effect::Displace {
            map: sample(doc, id, 3.0),
            horizontal: 0.0,
            vertical: 3.0,
            shade: SurfaceShade::Multiply,
            strength: 0.5,
        });
        let out = crate::live::evaluate(&shape);
        let path = out.main.local_path();
        let b = path.bounding_box();
        assert!(b.y0 < 5.0 - 2.5 && b.y1 > 15.0 + 2.5, "{b:?}");
        // The left top corner went down, the right top corner up.
        let top_left = path
            .elements()
            .iter()
            .filter_map(|el| el.end_point())
            .filter(|p| (p.x - 10.0).abs() < 1e-6)
            .map(|p| p.y)
            .fold(f64::MIN, f64::max);
        assert!((top_left - 12.0).abs() < 0.3, "{top_left}");
        let json = serde_json::to_string(&shape.effects[0]).expect("json");
        let back: Effect = serde_json::from_str(&json).expect("back");
        assert_eq!(back, shape.effects[0]);
        shape.effects[0] = Effect::Displace {
            map: None,
            horizontal: 3.0,
            vertical: 3.0,
            shade: SurfaceShade::None,
            strength: 0.0,
        };
        let out = crate::live::evaluate(&shape);
        let b = out.main.local_path().bounding_box();
        assert!((b.y0 - 5.0).abs() < 1e-9 && (b.y1 - 15.0).abs() < 1e-9);
    }

    /// The shading: Multiply darkens only below the mean, Screen lightens
    /// only above it, by the strength; None does neither.
    #[test]
    fn the_shading_takes_the_shadows_and_highlights() {
        use crate::live::surface_shade;
        assert_eq!(surface_shade(SurfaceShade::None, 1.0, 0.5, 0.0), (1.0, 0.0));
        let (m, s) = surface_shade(SurfaceShade::Multiply, 1.0, 0.5, 0.25);
        assert!((m - 0.5).abs() < 1e-9 && s == 0.0);
        assert_eq!(surface_shade(SurfaceShade::Multiply, 1.0, 0.5, 0.9), (1.0, 0.0));
        let (m, s) = surface_shade(SurfaceShade::Screen, 0.5, 0.5, 1.0);
        assert!(m == 1.0 && (s - 0.5).abs() < 1e-9);
        let (m, s) = surface_shade(SurfaceShade::Both, 1.0, 0.5, 0.0);
        assert!(m.abs() < 1e-9 && s == 0.0);
    }
}
