//! Painting inside a picture being edited (Bitmaps > Edit Inside): the
//! gradient kinds and the pixel shapes (rectangle, ellipse, polygon and
//! line, filled and outlined). See `docs/behavior/edit-inside-picture.md`.

use crate::pixel_edit::{over, BrushOpts, Dirty, PixelImage};
use image::GrayImage;

/// How a gradient spreads from its start point to its end point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GradientKind {
    /// Along the line from start to end.
    Linear,
    /// Along the line, both ways from the start.
    Reflected,
    /// In circles around the start.
    Radial,
    /// Around the start, a full turn from the line to itself.
    Conical,
    /// In squares around the start, turned with the line.
    Square,
}

impl GradientKind {
    pub const ALL: [GradientKind; 5] = [
        GradientKind::Linear,
        GradientKind::Reflected,
        GradientKind::Radial,
        GradientKind::Conical,
        GradientKind::Square,
    ];

    pub fn key(self) -> &'static str {
        match self {
            GradientKind::Linear => "pixel.gradient.linear",
            GradientKind::Reflected => "pixel.gradient.reflected",
            GradientKind::Radial => "pixel.gradient.radial",
            GradientKind::Conical => "pixel.gradient.conical",
            GradientKind::Square => "pixel.gradient.square",
        }
    }
}

/// Where (x, y) falls in a gradient from `a` to `b`: 0 at the start, 1
/// at the end and past it.
pub fn gradient_t(kind: GradientKind, a: (f32, f32), b: (f32, f32), x: f32, y: f32) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = (dx * dx + dy * dy).sqrt().max(1e-3);
    let (px, py) = (x - a.0, y - a.1);
    // Along the line and across it, in lengths of the line.
    let u = (px * dx + py * dy) / (len * len);
    let v = (py * dx - px * dy) / (len * len);
    let t = match kind {
        GradientKind::Linear => u,
        GradientKind::Reflected => u.abs(),
        GradientKind::Radial => (u * u + v * v).sqrt(),
        GradientKind::Conical => {
            let ang = v.atan2(u);
            ang.rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU
        }
        GradientKind::Square => u.abs().max(v.abs()),
    };
    t.clamp(0.0, 1.0)
}

/// The shapes the Shape tool draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelShape {
    Rect,
    Ellipse,
    /// A regular polygon inside the dragged box.
    Polygon,
    Line,
}

impl PixelShape {
    pub const ALL: [PixelShape; 4] = [
        PixelShape::Rect,
        PixelShape::Ellipse,
        PixelShape::Polygon,
        PixelShape::Line,
    ];

    pub fn key(self) -> &'static str {
        match self {
            PixelShape::Rect => "pixel.shape.rect",
            PixelShape::Ellipse => "pixel.shape.ellipse",
            PixelShape::Polygon => "pixel.shape.polygon",
            PixelShape::Line => "pixel.shape.line",
        }
    }
}

/// The corners of a regular polygon of `sides` inside the box from `a`
/// to `b`, its first corner at the top.
pub fn polygon_points(a: (f32, f32), b: (f32, f32), sides: u32) -> Vec<(f32, f32)> {
    let n = sides.clamp(3, 64);
    let (cx, cy) = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
    let (rx, ry) = ((b.0 - a.0).abs() / 2.0, (b.1 - a.1).abs() / 2.0);
    (0..n)
        .map(|i| {
            let t = i as f32 / n as f32 * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
            (cx + rx * t.cos(), cy + ry * t.sin())
        })
        .collect()
}

/// The outline of a shape from `a` to `b`, picture pixels.
fn shape_path(
    shape: PixelShape,
    a: (f32, f32),
    b: (f32, f32),
    sides: u32,
) -> Option<tiny_skia::Path> {
    let mut pb = tiny_skia::PathBuilder::new();
    let (x0, y0) = (a.0.min(b.0), a.1.min(b.1));
    let (x1, y1) = (a.0.max(b.0), a.1.max(b.1));
    match shape {
        PixelShape::Rect => {
            pb.push_rect(tiny_skia::Rect::from_ltrb(x0, y0, x1, y1)?);
        }
        PixelShape::Ellipse => {
            pb.push_oval(tiny_skia::Rect::from_ltrb(x0, y0, x1, y1)?);
        }
        PixelShape::Polygon => {
            let pts = polygon_points(a, b, sides);
            let first = pts.first()?;
            pb.move_to(first.0, first.1);
            for p in pts.iter().skip(1) {
                pb.line_to(p.0, p.1);
            }
            pb.close();
        }
        PixelShape::Line => {
            pb.move_to(a.0, a.1);
            pb.line_to(b.0, b.1);
        }
    }
    pb.finish()
}

/// How much of each pixel a shape covers: its inside (`fill`) and its
/// outline `width` pixels wide (centred on the edge), 0 to 255.
#[allow(clippy::too_many_arguments)]
pub fn shape_coverage(
    w: u32,
    h: u32,
    shape: PixelShape,
    a: (f32, f32),
    b: (f32, f32),
    sides: u32,
    fill: bool,
    width: Option<f32>,
) -> (Option<GrayImage>, Option<GrayImage>) {
    let Some(path) = shape_path(shape, a, b, sides) else {
        return (None, None);
    };
    let paint = {
        let mut p = tiny_skia::Paint::default();
        p.set_color_rgba8(255, 255, 255, 255);
        p.anti_alias = true;
        p
    };
    let to_gray = |pm: &tiny_skia::Pixmap| {
        let data: Vec<u8> = pm.pixels().iter().map(|p| p.alpha()).collect();
        GrayImage::from_raw(w, h, data)
    };
    let inside = if fill && shape != PixelShape::Line {
        tiny_skia::Pixmap::new(w, h).and_then(|mut pm| {
            pm.fill_path(
                &path,
                &paint,
                tiny_skia::FillRule::Winding,
                tiny_skia::Transform::identity(),
                None,
            );
            to_gray(&pm)
        })
    } else {
        None
    };
    let edge = width.filter(|v| *v > 0.0).and_then(|lw| {
        let mut pm = tiny_skia::Pixmap::new(w, h)?;
        let stroke = tiny_skia::Stroke {
            width: lw,
            line_join: tiny_skia::LineJoin::Miter,
            ..Default::default()
        };
        pm.stroke_path(
            &path,
            &paint,
            &stroke,
            tiny_skia::Transform::identity(),
            None,
        );
        to_gray(&pm)
    });
    (inside, edge)
}

/// A shape drawn from `a` to `b` on the layer in use, inside the
/// selection: filled with the colour, outlined with the outline colour,
/// a line in the colour.
pub fn draw_shape(
    pi: &mut PixelImage,
    a: (f32, f32),
    b: (f32, f32),
    o: &BrushOpts,
) -> Option<Dirty> {
    let (w, h) = pi.img.dimensions();
    if w == 0 || h == 0 {
        return None;
    }
    let line = o.shape == PixelShape::Line;
    let width = (line || o.shape_outline).then_some(o.line_width.max(0.5));
    let (inside, edge) = shape_coverage(w, h, o.shape, a, b, o.sides, o.shape_fill && !line, width);
    let edge_color = if line { o.color } else { o.outline_color };
    let mut changed: Option<Dirty> = None;
    for (m, color) in [(inside, o.color), (edge, edge_color)] {
        let Some(m) = m else {
            continue;
        };
        for (x, y, v) in m.enumerate_pixels() {
            if v[0] == 0 {
                continue;
            }
            let t = v[0] as f32 / 255.0 * pi.sel_at(x, y) * o.opacity;
            if t <= 0.0 {
                continue;
            }
            let q = *pi.img.get_pixel(x, y);
            pi.img.put_pixel(x, y, over(&q, color, t));
            changed = Some(crate::pixel_edit::union(changed, [x, y, x + 1, y + 1]));
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    #[test]
    fn each_gradient_kind_spreads_its_own_way() {
        let (a, b) = ((0.0, 0.0), (10.0, 0.0));
        let at = |k, x, y| gradient_t(k, a, b, x, y);
        assert!((at(GradientKind::Linear, 5.0, 3.0) - 0.5).abs() < 1e-4);
        assert_eq!(at(GradientKind::Linear, -5.0, 0.0), 0.0);
        assert!((at(GradientKind::Reflected, -5.0, 0.0) - 0.5).abs() < 1e-4);
        assert!((at(GradientKind::Radial, 0.0, 5.0) - 0.5).abs() < 1e-4);
        assert!((at(GradientKind::Conical, 0.0, 5.0) - 0.25).abs() < 1e-4);
        assert!((at(GradientKind::Square, 4.0, 5.0) - 0.5).abs() < 1e-4);
    }

    #[test]
    fn a_shape_is_filled_and_outlined_inside_the_selection() {
        let mut pi = PixelImage::new(RgbaImage::from_pixel(20, 20, Rgba([255, 255, 255, 255])));
        let o = BrushOpts {
            color: [255, 0, 0, 255],
            outline_color: [0, 0, 255, 255],
            shape: PixelShape::Rect,
            shape_fill: true,
            shape_outline: true,
            line_width: 2.0,
            ..BrushOpts::default()
        };
        pi.sel = crate::pixel_edit::rect_mask(20, 20, (0.0, 0.0), (10.0, 20.0));
        draw_shape(&mut pi, (2.0, 2.0), (18.0, 18.0), &o).unwrap();
        assert_eq!(*pi.img.get_pixel(6, 10), Rgba([255, 0, 0, 255]), "filled");
        assert_eq!(*pi.img.get_pixel(2, 10), Rgba([0, 0, 255, 255]), "outline");
        assert_eq!(
            *pi.img.get_pixel(14, 10),
            Rgba([255, 255, 255, 255]),
            "outside the selection"
        );
        assert_eq!(*pi.img.get_pixel(0, 0), Rgba([255, 255, 255, 255]));
    }

    #[test]
    fn a_line_and_a_polygon_are_drawn() {
        let mut pi = PixelImage::new(RgbaImage::new(20, 20));
        let mut o = BrushOpts {
            color: [0, 0, 0, 255],
            shape: PixelShape::Line,
            line_width: 3.0,
            ..BrushOpts::default()
        };
        draw_shape(&mut pi, (0.0, 10.0), (20.0, 10.0), &o).unwrap();
        assert_eq!(pi.img.get_pixel(5, 10)[3], 255);
        assert_eq!(pi.img.get_pixel(5, 3)[3], 0);
        let pts = polygon_points((0.0, 0.0), (20.0, 20.0), 3);
        assert_eq!(pts.len(), 3);
        assert!((pts[0].0 - 10.0).abs() < 1e-4 && pts[0].1.abs() < 1e-4);
        o.shape = PixelShape::Polygon;
        o.sides = 6;
        let mut pi = PixelImage::new(RgbaImage::new(20, 20));
        draw_shape(&mut pi, (0.0, 0.0), (20.0, 20.0), &o).unwrap();
        assert_eq!(pi.img.get_pixel(10, 10)[3], 255);
        assert_eq!(pi.img.get_pixel(0, 0)[3], 0);
    }
}
