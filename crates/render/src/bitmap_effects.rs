//! Prepare editable Gaussian bitmap effects for callers without the app's
//! effect engine. Pictures are cached in the existing RasterFx model, so
//! PNG, PDF and SVG use the same appearance without flattening the page.

use tiny_skia::Pixmap;
use tracedraw_core::{document::RasterFx, Affine, Document, Rect, Shape, ShapeKind, Size, Symbol};

const MAX_PIXELS: f64 = 4_000_000.0;

/// A finite Gaussian using the displayed radius as sigma, with support
/// at two sigma. Original straight RGBA pixels in, straight RGBA out.
/// Shared by imported effect previews and headless preparation.
pub fn finite_gaussian_rgba(rgba: &[u8], width: u32, height: u32, radius: f64) -> Option<Vec<u8>> {
    let count = u64::from(width).checked_mul(u64::from(height))?;
    if count == 0
        || count > 16_000_000
        || count.checked_mul(4)? != rgba.len() as u64
        || !radius.is_finite()
        || !(0.0..=250.0).contains(&radius)
    {
        return None;
    }
    let mut pixels = Pixmap::new(width, height)?;
    for (out, input) in pixels
        .data_mut()
        .chunks_exact_mut(4)
        .zip(rgba.chunks_exact(4))
    {
        for c in 0..3 {
            out[c] = ((u16::from(input[c]) * u16::from(input[3]) + 127) / 255) as u8;
        }
        out[3] = input[3];
    }
    gaussian(&mut pixels, radius, true);
    let mut out = Vec::with_capacity(rgba.len());
    for pixel in pixels.pixels() {
        let pixel = pixel.demultiply();
        out.extend_from_slice(&[pixel.red(), pixel.green(), pixel.blue(), pixel.alpha()]);
    }
    Some(out)
}

/// Make missing or stale Gaussian pictures. Other filters are left to the
/// application's effect engine. Returns the number of pictures made.
pub fn prepare(doc: &mut Document) -> usize {
    fn walk(s: &mut Shape, symbols: &[Symbol], dpi: f64) -> usize {
        let mut n = 0;
        match &mut s.kind {
            ShapeKind::Group { children } => {
                for child in children {
                    n += walk(child, symbols, dpi);
                }
            }
            ShapeKind::ClipFrame { frame, contents } => {
                n += walk(frame, symbols, dpi);
                for child in contents {
                    n += walk(child, symbols, dpi);
                }
            }
            _ => {}
        }
        if let Some(fx) = &s.raster_fx {
            if fx.png.is_empty() || fx.source != s.raster_source() {
                if let Some(picture) = make(s, symbols, dpi) {
                    s.raster_fx = Some(Box::new(picture));
                    n += 1;
                }
            }
        }
        n
    }
    let dpi = doc.metadata.resolution_dpi;
    let symbols = doc.symbols.clone();
    let mut n = 0;
    for layer in doc.all_layers_mut() {
        for shape in &mut layer.shapes {
            n += walk(shape, &symbols, dpi);
        }
    }
    n
}

fn make(shape: &Shape, symbols: &[Symbol], dpi: f64) -> Option<RasterFx> {
    let fx = shape.raster_fx.as_ref()?;
    if fx.effects.is_empty()
        || fx.effects.len() > 64
        || !dpi.is_finite()
        || !(1.0..=9600.0).contains(&dpi)
    {
        return None;
    }
    // Never cache a partial result for a stack the headless engine cannot
    // execute. This also prevents an invalid radius from allocating.
    let radii: Vec<(f64, bool)> = fx
        .effects
        .iter()
        .map(|e| {
            let radius = e.params.get("radius").copied()?;
            let finite = match e.params.get("kernel_extent") {
                None => false,
                Some(extent) if *extent == 2.0 => true,
                _ => return None,
            };
            (e.id == "gaussian_blur" && radius.is_finite() && (0.0..=250.0).contains(&radius))
                .then_some((if e.visible { radius } else { 0.0 }, finite))
        })
        .collect::<Option<_>>()?;
    let (sx, sy) = tracedraw_core::document::transform_scale(shape.transform);
    if !sx.is_finite() || !sy.is_finite() || sx <= 1e-9 || sy <= 1e-9 {
        return None;
    }
    let mut bare = shape.clone();
    bare.raster_fx = None;
    bare.effects
        .retain(|e| !tracedraw_core::document::beside_picture(e));
    // Fountain fills lie on the page box: turn them back with the picture
    // (#3983).
    let local = Affine::scale_non_uniform(sx, sy);
    bare.carry_fills(local * shape.transform.inverse(), Affine::IDENTITY);
    bare.transform = local;
    let bounds = bare.bounds();
    if ![bounds.x0, bounds.y0, bounds.x1, bounds.y1]
        .iter()
        .all(|v| v.is_finite())
        || bounds.width() <= 0.0
        || bounds.height() <= 0.0
    {
        return None;
    }
    let padding = 2.0
        + shape.stroke.as_ref().map_or(0.0, |s| s.width.abs())
        + radii.iter().map(|r| r.0).sum::<f64>() * 2.0 * 25.4 / dpi;
    let bounds = bounds.inflate(padding, padding);
    let area = bounds.width() * bounds.height();
    if !area.is_finite() || area <= 0.0 {
        return None;
    }
    let zoom = (dpi / 25.4).min((MAX_PIXELS / area).sqrt());
    let width = (bounds.width() * zoom).ceil().max(1.0) as u32;
    let height = (bounds.height() * zoom).ceil().max(1.0) as u32;
    let mut doc = Document::new(
        "Gaussian effect",
        Size::new(bounds.width(), bounds.height()),
    );
    doc.symbols = symbols.to_vec();
    bare.transform = Affine::translate((-bounds.x0, -bounds.y0)) * bare.transform;
    let page = doc.pages.first_mut()?;
    page.layers.first_mut()?.shapes.push(bare);
    let page_id = page.id;
    let mut pixels = crate::render_page(
        &doc,
        page_id,
        &crate::RenderOptions {
            width,
            height,
            view: crate::ViewTransform {
                zoom,
                origin_x: 0.0,
                origin_y: f64::from(height),
                stretch_y: 1.0,
            },
            ..Default::default()
        },
    )?;
    for (radius, finite) in radii {
        gaussian(
            &mut pixels,
            radius * if finite { 1.0 } else { 0.5 } * zoom * 25.4 / dpi,
            finite,
        );
    }
    Some(RasterFx {
        effects: fx.effects.clone(),
        rect: Rect::new(
            bounds.x0 / sx,
            bounds.y0 / sy,
            bounds.x1 / sx,
            bounds.y1 / sy,
        ),
        width_px: width,
        height_px: height,
        png: pixels.encode_png().ok()?.into(),
        source: shape.raster_source(),
    })
}

/// Same sigma convention as the bitmap filter (radius / 2). Work on
/// premultiplied channels to preserve colour at transparent edges. Small
/// kernels are exact; larger kernels use three boxes with matching variance.
fn gaussian(pixels: &mut Pixmap, sigma: f64, finite: bool) {
    if sigma <= 0.05 {
        return;
    }
    if sigma >= 2.0 {
        // Variance of a normal distribution truncated to [-2 sigma,
        // 2 sigma]: sigma^2 * (1 - 4 phi(2) / erf(sqrt(2))).
        // The three boxes match that variance without work proportional
        // to the radius. This remains an approximation near the cutoff.
        let sigma = sigma * if finite { 0.879625661 } else { 1.0 };
        let low = ((4.0 * sigma * sigma + 1.0).sqrt().floor() as i32) | 1;
        // Choose the odd box width below the ideal, then the one above.
        let low = if f64::from(low) > (4.0 * sigma * sigma + 1.0).sqrt() {
            low - 2
        } else {
            low
        };
        let w = f64::from(low);
        let count = ((12.0 * sigma * sigma - 3.0 * w * w - 12.0 * w - 9.0) / (-4.0 * w - 4.0))
            .round() as i32;
        for i in 0..3 {
            let radius = if i < count {
                (low - 1) / 2
            } else {
                (low + 1) / 2
            };
            if radius > 0 {
                crate::box_blur_passes(pixels, radius, 1);
            }
        }
        return;
    }
    let r = (sigma * if finite { 2.0 } else { 3.0 }).ceil() as i32;
    let mut kernel: Vec<f64> = (-r..=r)
        .map(|i| (-f64::from(i * i) / (2.0 * sigma * sigma)).exp())
        .collect();
    let total: f64 = kernel.iter().sum();
    for k in &mut kernel {
        *k /= total;
    }
    let (width, height) = (pixels.width() as usize, pixels.height() as usize);
    let mut source: Vec<f64> = pixels.data().iter().map(|b| f64::from(*b)).collect();
    let mut target = source.clone();
    for horizontal in [true, false] {
        for y in 0..height {
            for x in 0..width {
                for c in 0..4 {
                    let mut sum = 0.0;
                    for (j, k) in kernel.iter().enumerate() {
                        let delta = j as i32 - r;
                        let xx = if horizontal {
                            (x as i32 + delta).clamp(0, width as i32 - 1) as usize
                        } else {
                            x
                        };
                        let yy = if horizontal {
                            y
                        } else {
                            (y as i32 + delta).clamp(0, height as i32 - 1) as usize
                        };
                        sum += source[(yy * width + xx) * 4 + c] * k;
                    }
                    target[(y * width + x) * 4 + c] = sum;
                }
            }
        }
        std::mem::swap(&mut source, &mut target);
    }
    for (out, value) in pixels.data_mut().iter_mut().zip(source) {
        *out = value.round().clamp(0.0, 255.0) as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::{BitmapEffect, Color, Fill, ShapeId};

    #[test]
    fn gaussian_picture_softens_edges_keeps_source_and_is_cached() {
        let mut doc = Document::new("control", Size::new(40.0, 40.0));
        doc.metadata.resolution_dpi = 254.0;
        let mut s = Shape::new(
            ShapeId(10),
            ShapeKind::rect_with_corners(Rect::new(10., 10., 30., 30.), Default::default()),
        );
        s.fill = Fill::Solid(Color::rgb8(255, 0, 0));
        s.stroke = None;
        s.raster_fx = Some(Box::new(RasterFx {
            effects: vec![BitmapEffect {
                id: "gaussian_blur".into(),
                params: [("radius".into(), 15.0)].into(),
                visible: true,
            }],
            rect: Rect::ZERO,
            width_px: 0,
            height_px: 0,
            png: Default::default(),
            source: 0,
        }));
        doc.pages[0].layers[0].shapes.push(s);
        assert_eq!(prepare(&mut doc), 1);
        assert_eq!(prepare(&mut doc), 0);
        let shape = &doc.pages[0].layers[0].shapes[0];
        assert!(matches!(shape.kind, ShapeKind::Rect { .. }));
        let pic = shape.raster_fx.as_ref().unwrap();
        let pm = Pixmap::decode_png(&pic.png).unwrap();
        assert!(pm.pixels().iter().any(|p| p.alpha() > 0 && p.alpha() < 255));
        for p in pm.pixels().iter().filter(|p| p.alpha() > 10) {
            let p = p.demultiply();
            assert!(p.red() >= 250, "no dark fringe: {p:?}");
            assert_eq!(p.green(), 0);
        }
        let back = Document::from_json(&doc.to_json().unwrap()).unwrap();
        assert_eq!(back.pages[0].layers[0].shapes[0].raster_fx, shape.raster_fx);
    }

    #[test]
    fn finite_gaussian_preserves_alpha_colour_and_measured_spread() {
        let (w, h) = (160, 3);
        let mut source = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 80..w {
                source[(y * w + x) * 4..(y * w + x + 1) * 4].copy_from_slice(&[255, 255, 255, 255]);
            }
        }
        let result = finite_gaussian_rgba(&source, w as u32, h as u32, 15.0).unwrap();
        let alpha = |x: usize| f64::from(result[(w + x) * 4 + 3]);
        let weights: Vec<_> = (1..w)
            .map(|x| (x as f64 - 0.5, (alpha(x) - alpha(x - 1)).max(0.0)))
            .collect();
        let total: f64 = weights.iter().map(|p| p.1).sum();
        let center = weights.iter().map(|p| p.0 * p.1).sum::<f64>() / total;
        let spread = (weights
            .iter()
            .map(|p| (p.0 - center).powi(2) * p.1)
            .sum::<f64>()
            / total)
            .sqrt();
        // An independent 300 dpi reference of the original radius-15
        // drawing has an edge spread of 13.5 px. Allow quantisation and
        // the three-box approximation; native radius/2 would give 7.5.
        assert!((spread - 13.5).abs() < 1.0, "spread {spread}");
        for pixel in result.chunks_exact(4).filter(|p| p[3] > 10) {
            assert!(pixel[0] >= 250, "transparent white must stay white");
        }
        assert!(finite_gaussian_rgba(&source[..3], 160, 3, 15.0).is_none());
        assert!(finite_gaussian_rgba(&source, 160, 3, f64::NAN).is_none());
    }

    /// A mirrored or turned object's fountain fill runs the same way in
    /// its blurred picture as on the page: the fill lies on the page box,
    /// so the picture drawn without the mirror took it unmirrored and the
    /// mirror then flipped it (#3983).
    #[test]
    fn a_mirrored_gradient_keeps_its_direction_when_blurred() {
        use tracedraw_core::{Fountain, FountainKind};
        let brightness = |doc: &Document, y: f64| {
            let pm = crate::render_page_image(doc, doc.pages[0].id, 254.0).unwrap();
            let zoom = 254.0 / 25.4;
            let px = pm
                .pixel((20.0 * zoom) as u32, ((40.0 - y) * zoom) as u32)
                .unwrap();
            px.red() as i32
        };
        for t in [
            Affine::new([1.0, 0.0, 0.0, -1.0, 0.0, 40.0]),
            Affine::new([-1.0, 0.0, 0.0, 1.0, 40.0, 0.0]),
            Affine::new([0.0, 1.0, -1.0, 0.0, 40.0, 0.0]),
        ] {
            let mut doc = Document::new("mirror", Size::new(40.0, 40.0));
            doc.metadata.resolution_dpi = 254.0;
            let mut s = Shape::new(
                ShapeId(10),
                ShapeKind::rect_with_corners(Rect::new(10., 10., 30., 30.), Default::default()),
            );
            // Black at the bottom of the page box, white at the top.
            s.fill = Fill::Fountain(Fountain::two(
                FountainKind::Linear,
                Color::BLACK,
                Color::WHITE,
                90.0,
            ));
            s.stroke = None;
            s.transform = t;
            doc.pages[0].layers[0].shapes.push(s.clone());
            let (low, high) = (brightness(&doc, 12.0), brightness(&doc, 28.0));
            assert!(low + 100 < high, "plain: {low} {high}");
            s.raster_fx = Some(Box::new(RasterFx {
                effects: vec![BitmapEffect {
                    id: "gaussian_blur".into(),
                    params: [("radius".into(), 1.0)].into(),
                    visible: true,
                }],
                rect: Rect::ZERO,
                width_px: 0,
                height_px: 0,
                png: Default::default(),
                source: 0,
            }));
            doc.pages[0].layers[0].shapes[0] = s;
            assert_eq!(prepare(&mut doc), 1);
            let (low, high) = (brightness(&doc, 12.0), brightness(&doc, 28.0));
            assert!(low + 100 < high, "blurred {t:?}: {low} {high}");
        }
    }
}
