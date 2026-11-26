//! Pictures on colour separation plates (#1659).
//!
//! A plate shows the coverage of one ink in grey (black = full ink). A
//! picture is split into its inks once per picture: a picture that keeps
//! its ink channels (`Shape::picture_inks`, a `.cdr` CMYK image) gives
//! them as they are; any other picture (RGB, greyscale, CMYK converted
//! from RGB) has its pixels converted to CMYK with the drawing's colour
//! management (`Color::convert_to`, the conversion vector colours take),
//! each distinct colour once. The pixels' alpha stays on every plate, so
//! transparency, masks and clipping print as they show. The split and
//! the plates made from it are cached by picture, so the plates of a
//! print job, and Print Preview redrawing a plate, do the work once.

use crate::export::Plate;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tracedraw_core::color::IccEngine;
use tracedraw_core::document::ShapeKind;
use tracedraw_core::picture_inks::{PictureInk, PictureInks};
use tracedraw_core::{Color, Shape, SharedBytes};

/// A picture split into its inks.
struct Separated {
    width: u32,
    height: u32,
    /// The pixels' alpha.
    alpha: Vec<u8>,
    /// Cyan, magenta, yellow and black of each pixel (255 = full ink).
    cmyk: Vec<[u8; 4]>,
    /// The spot inks the picture holds: the ink's id and its coverage.
    spots: Vec<(u32, Vec<u8>)>,
}

impl Separated {
    fn bytes(&self) -> usize {
        self.alpha.len() * 5 + self.spots.iter().map(|s| s.1.len()).sum::<usize>()
    }
}

/// One cached picture: what it was split from and the plates made of it.
struct Entry {
    png: SharedBytes,
    inks: Option<SharedBytes>,
    engine: Option<Arc<IccEngine>>,
    gray_to_black: bool,
    separated: Arc<Separated>,
    made: Vec<(String, SharedBytes)>,
}

/// The cache, the most recent picture last; kept under this many bytes
/// of split pixels (the last picture always stays).
static CACHE: Mutex<Vec<Entry>> = Mutex::new(Vec::new());
const CACHE_BYTES: usize = 256 << 20;

/// The picture `s` shows (a bitmap's pixels) and the ink channels that
/// fit them.
fn picture(s: &Shape) -> Option<(&SharedBytes, Option<&PictureInks>)> {
    let ShapeKind::Bitmap { png, .. } = &s.kind else {
        return None;
    };
    let inks = s.picture_inks.as_deref().filter(|i| i.fits(png.as_slice()));
    Some((png, inks))
}

/// The picture of `s` as made by `make` from its inks (`key` names what
/// `make` makes), from the cache when it was made before.
fn made(
    s: &Shape,
    key: &str,
    make: &dyn Fn(&Separated) -> Option<SharedBytes>,
) -> Option<SharedBytes> {
    let (png, inks) = picture(s)?;
    let engine = tracedraw_core::color::engine::icc();
    let gray_to_black = tracedraw_core::color::engine::map_gray_to_black();
    let same = |e: &Entry| {
        SharedBytes::ptr_eq(&e.png, png)
            && match (&e.inks, inks) {
                (Some(a), Some(b)) => SharedBytes::ptr_eq(a, &b.png),
                (None, None) => true,
                _ => false,
            }
            && match (&e.engine, &engine) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
            && e.gray_to_black == gray_to_black
    };
    let cached = {
        let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        let at = cache.iter().position(same);
        at.map(|i| {
            // Most recently used last.
            let e = cache.remove(i);
            let found = e
                .made
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, b)| b.clone());
            let sep = e.separated.clone();
            cache.push(e);
            (sep, found)
        })
    };
    let separated = match cached {
        Some((_, Some(done))) => return Some(done),
        Some((sep, None)) => sep,
        None => Arc::new(separate(png, inks)?),
    };
    let out = make(&separated)?;
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(e) = cache.iter_mut().find(|e| same(e)) {
        e.made.push((key.to_string(), out.clone()));
    } else {
        cache.push(Entry {
            png: png.clone(),
            inks: inks.map(|i| i.png.clone()),
            engine,
            gray_to_black,
            separated,
            made: vec![(key.to_string(), out.clone())],
        });
        let mut total: usize = cache.iter().map(|e| e.separated.bytes()).sum();
        while cache.len() > 1 && total > CACHE_BYTES {
            total -= cache.remove(0).separated.bytes();
        }
    }
    Some(out)
}

/// A PNG decoded without the decoder's usual memory cap: a picture
/// already in the drawing may be larger than it (#4201: 11834 x 25340
/// pixels).
fn decode_any(png: &[u8], format: image::ImageFormat) -> Option<image::DynamicImage> {
    let mut r = image::ImageReader::with_format(std::io::Cursor::new(png), format);
    r.no_limits();
    r.decode().ok()
}

/// Make the pictures in `s` no larger than an output at `dpi` shows
/// (twice that, for the resampling to keep the detail), their ink
/// channels with them (#4201: the inks of a picture too large to split
/// were left out of a CMYK export, which came out grey). Smaller
/// pictures stay as they are.
pub fn fit_pictures_to(s: &mut Shape, dpi: f64) {
    fit_pictures(s, dpi, 16_000_000.0);
}

/// [`fit_pictures_to`], for pictures of at least `least` pixels.
fn fit_pictures(s: &mut Shape, dpi: f64, least: f64) {
    match &mut s.kind {
        ShapeKind::Group { children } => {
            children.iter_mut().for_each(|c| fit_pictures(c, dpi, least));
            return;
        }
        ShapeKind::ClipFrame { frame, contents } => {
            fit_pictures(frame, dpi, least);
            contents.iter_mut().for_each(|c| fit_pictures(c, dpi, least));
            return;
        }
        _ => {}
    }
    let c = s.transform.as_coeffs();
    let (sx, sy) = (c[0].hypot(c[1]), c[2].hypot(c[3]));
    let ShapeKind::Bitmap {
        rect,
        width_px,
        height_px,
        png,
        ..
    } = &mut s.kind
    else {
        return;
    };
    let px = |mm: f64| (mm / 25.4 * dpi * 2.0).ceil().max(1.0);
    let (want_w, want_h) = (px(rect.width().abs() * sx), px(rect.height().abs() * sy));
    let (w, h) = (f64::from(*width_px), f64::from(*height_px));
    // Small pictures, and those already near the output's size, stay.
    if w * h < least || (w <= want_w * 1.5 && h <= want_h * 1.5) {
        return;
    }
    let (nw, nh) = (want_w.min(w) as u32, want_h.min(h) as u32);
    let inks = s
        .picture_inks
        .as_deref()
        .filter(|i| i.fits(png.as_slice()))
        .cloned();
    let Some(img) = decode_any(png.as_slice(), image::ImageFormat::Png) else {
        return;
    };
    let (ow, oh) = (img.width(), img.height());
    let small = image::imageops::resize(&img.to_rgba8(), nw, nh, image::imageops::FilterType::Triangle);
    drop(img);
    let Some(new_png) = crate::bitmap_fx::encode(&small) else {
        return;
    };
    let new_png = SharedBytes::from(new_png);
    let new_inks = inks.and_then(|i| {
        let planes = decode_any(i.png.as_slice(), image::ImageFormat::Png)?.to_luma8();
        let n = i.inks.len() as u32;
        if n == 0 || planes.width() != ow || planes.height() != oh * n {
            return None;
        }
        let mut out = image::GrayImage::new(nw, nh * n);
        for k in 0..n {
            let plane = image::imageops::crop_imm(&planes, 0, k * oh, ow, oh).to_image();
            let shrunk =
                image::imageops::resize(&plane, nw, nh, image::imageops::FilterType::Triangle);
            image::imageops::replace(&mut out, &shrunk, 0, i64::from(k * nh));
        }
        let mut buf = Vec::new();
        out.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
            .ok()?;
        Some(PictureInks {
            inks: i.inks.clone(),
            png: SharedBytes::from(buf),
            of: tracedraw_core::picture_inks::fingerprint(new_png.as_slice()),
        })
    });
    *png = new_png;
    *width_px = nw;
    *height_px = nh;
    s.picture_inks = new_inks.map(Box::new);
}

/// Split a picture into its inks: from the channels it keeps, else its
/// pixels through the colour management.
fn separate(png: &[u8], inks: Option<&PictureInks>) -> Option<Separated> {
    let img = decode_any(png, image::ImageFormat::Png)?.to_rgba8();
    let (width, height) = img.dimensions();
    let n = width as usize * height as usize;
    let alpha: Vec<u8> = img.pixels().map(|p| p[3]).collect();
    if let Some(sep) = inks.and_then(|i| from_channels(i, width, height, &alpha)) {
        return Some(sep);
    }
    let mut known: HashMap<[u8; 3], [u8; 4]> = HashMap::new();
    let mut cmyk = Vec::with_capacity(n);
    for p in img.pixels() {
        if p[3] == 0 {
            cmyk.push([0; 4]);
            continue;
        }
        let rgb = [p[0], p[1], p[2]];
        let ink = *known.entry(rgb).or_insert_with(|| {
            match Color::rgb8(rgb[0], rgb[1], rgb[2]).convert_to("CMYK") {
                Color::Cmyk { c, m, y, k } => [byte(c), byte(m), byte(y), byte(k)],
                _ => [0; 4],
            }
        });
        cmyk.push(ink);
    }
    Some(Separated {
        width,
        height,
        alpha,
        cmyk,
        spots: Vec::new(),
    })
}

/// The inks a picture keeps, when they decode to planes of its size.
fn from_channels(inks: &PictureInks, width: u32, height: u32, alpha: &[u8]) -> Option<Separated> {
    let planes = decode_any(&inks.png, image::ImageFormat::Png)?.to_luma8();
    let n = width as usize * height as usize;
    if planes.width() != width
        || planes.height() as usize != height as usize * inks.inks.len()
        || inks.inks.is_empty()
    {
        return None;
    }
    let raw = planes.as_raw();
    let mut cmyk = vec![[0u8; 4]; n];
    let mut spots = Vec::new();
    for (i, ink) in inks.inks.iter().enumerate() {
        let plane = raw.get(i * n..(i + 1) * n)?;
        match *ink {
            PictureInk::Process(k) if k < 4 => {
                for (px, v) in cmyk.iter_mut().zip(plane) {
                    px[k as usize] = *v;
                }
            }
            PictureInk::Process(_) => {}
            PictureInk::Spot(id) => spots.push((id, plane.to_vec())),
        }
    }
    Some(Separated {
        width,
        height,
        alpha: alpha.to_vec(),
        cmyk,
        spots,
    })
}

fn byte(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// PNG bytes of RGBA pixels.
fn encode(width: u32, height: u32, rgba: Vec<u8>) -> Option<SharedBytes> {
    let img = image::RgbaImage::from_raw(width, height, rgba)?;
    crate::bitmap_fx::encode(&img).map(SharedBytes::from)
}

/// The plate of a picture: the coverage of the plate's ink in grey,
/// with the picture's alpha.
pub fn plate_picture(s: &Shape, plate: Plate) -> Option<SharedBytes> {
    let key = format!("{plate:?}");
    made(s, &key, &|sep| {
        let none = Vec::new();
        let spot = match plate {
            Plate::Spot(id) => sep
                .spots
                .iter()
                .find(|(i, _)| *i == id)
                .map(|(_, p)| p)
                .unwrap_or(&none),
            Plate::Process(_) => &none,
        };
        let mut rgba = Vec::with_capacity(sep.alpha.len() * 4);
        for (i, a) in sep.alpha.iter().enumerate() {
            let v = match plate {
                Plate::Process(k) => sep.cmyk.get(i).and_then(|c| c.get(k)).copied().unwrap_or(0),
                Plate::Spot(_) => spot.get(i).copied().unwrap_or(0),
            };
            let g = 255 - v;
            rgba.extend_from_slice(&[g, g, g, *a]);
        }
        encode(sep.width, sep.height, rgba)
    })
}

/// A picture as printed with only some inks: the process inks of
/// `process` and the spot inks of `spots` (each with its colour as
/// RGB); the others are paper.
pub fn inks_picture(
    s: &Shape,
    process: [bool; 4],
    spots: &[(u32, [u8; 3])],
) -> Option<SharedBytes> {
    let key = format!("{process:?}{spots:?}");
    made(s, &key, &|sep| {
        let mut shown: HashMap<[u8; 4], [u8; 3]> = HashMap::new();
        let on: Vec<(&[u8], [u8; 3])> = sep
            .spots
            .iter()
            .filter_map(|(id, plane)| {
                spots
                    .iter()
                    .find(|(i, _)| i == id)
                    .map(|(_, rgb)| (plane.as_slice(), *rgb))
            })
            .collect();
        let mut rgba = Vec::with_capacity(sep.alpha.len() * 4);
        for (i, a) in sep.alpha.iter().enumerate() {
            let mut ink = sep.cmyk.get(i).copied().unwrap_or([0; 4]);
            for (v, keep) in ink.iter_mut().zip(process) {
                if !keep {
                    *v = 0;
                }
            }
            let rgb = *shown.entry(ink).or_insert_with(|| {
                let f = |v: u8| v as f32 / 255.0;
                Color::Cmyk {
                    c: f(ink[0]),
                    m: f(ink[1]),
                    y: f(ink[2]),
                    k: f(ink[3]),
                }
                .to_rgb8()
            });
            // Spot inks print over the process inks, as duotone inks do.
            let mut c = rgb.map(|v| v as f32 / 255.0);
            for (plane, spot) in &on {
                let cover = plane.get(i).copied().unwrap_or(0) as f32 / 255.0;
                for (v, s) in c.iter_mut().zip(spot) {
                    *v *= 1.0 - cover * (1.0 - *s as f32 / 255.0);
                }
            }
            rgba.extend_from_slice(&[byte(c[0]), byte(c[1]), byte(c[2]), *a]);
        }
        encode(sep.width, sep.height, rgba)
    })
}

/// A picture as CMYK ink coverages, for a CMYK file of it (Save Bitmap
/// As, Edit Bitmap, #1834): cyan, magenta, yellow and black of each
/// pixel, row by row, 255 = full ink, with its size. The inks it keeps
/// give them as they are, other pictures their pixels through the colour
/// management, as on the separation plates. A spot ink it keeps prints
/// as its process mix over the process inks (`spots`: each ink's id
/// and its C, M, Y and K from 0 to 1); a see-through pixel holds as much
/// less ink as it lets the paper show. With `alpha` each pixel keeps
/// its full inks and a fifth byte, its opacity (unassociated alpha,
/// #1925), for a file that keeps transparency.
pub fn picture_cmyk(
    s: &Shape,
    spots: &[(u32, [f32; 4])],
    alpha: bool,
) -> Option<(u32, u32, Vec<u8>)> {
    let (png, inks) = picture(s)?;
    let sep = separate(png, inks)?;
    let mut out = Vec::with_capacity(sep.alpha.len() * if alpha { 5 } else { 4 });
    for (i, a) in sep.alpha.iter().enumerate() {
        let ink = sep.cmyk.get(i).copied().unwrap_or([0; 4]);
        let mut v = ink.map(|x| x as f32 / 255.0);
        for (id, plane) in &sep.spots {
            let Some((_, mix)) = spots.iter().find(|(s, _)| s == id) else {
                continue;
            };
            let cover = plane.get(i).copied().unwrap_or(0) as f32 / 255.0;
            for (x, m) in v.iter_mut().zip(mix) {
                *x = 1.0 - (1.0 - *x) * (1.0 - cover * m.clamp(0.0, 1.0));
            }
        }
        if alpha {
            out.extend(v.map(byte));
            out.push(*a);
            continue;
        }
        let a = *a as f32 / 255.0;
        out.extend(v.map(|x| byte(x * a)));
    }
    Some((sep.width, sep.height, out))
}

/// Put `picture`'s result in place of every picture in `s` and the
/// objects it holds (groups, clip frames); a picture it gives nothing
/// for stays.
pub fn swap_pictures(s: &mut Shape, picture: &dyn Fn(&Shape) -> Option<SharedBytes>) {
    if matches!(s.kind, ShapeKind::Bitmap { .. }) {
        if let Some(new) = picture(s) {
            if let ShapeKind::Bitmap { png, fx, .. } = &mut s.kind {
                *png = new;
                *fx = None;
            }
            s.cmyk_bitmap = false;
            s.picture_inks = None;
        }
        return;
    }
    match &mut s.kind {
        ShapeKind::Group { children } => {
            children.iter_mut().for_each(|c| swap_pictures(c, picture))
        }
        ShapeKind::ClipFrame { frame, contents } => {
            swap_pictures(frame, picture);
            contents.iter_mut().for_each(|c| swap_pictures(c, picture));
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::Rect;

    /// A picture far larger than the output keeps its inks when it is
    /// made smaller for it (#4201): cyan on the left half, red (magenta
    /// and yellow) on the right, on every plate as before.
    #[test]
    fn a_large_picture_made_smaller_keeps_its_inks() {
        let (w, h) = (400u32, 200u32);
        let img = image::RgbaImage::from_fn(w, h, |x, _| {
            image::Rgba(if x < w / 2 { [0, 160, 230, 255] } else { [230, 30, 40, 255] })
        });
        let png = crate::bitmap_fx::encode(&img).expect("png");
        let planes = image::GrayImage::from_fn(w, h * 4, |x, y| {
            let ink = y / h;
            image::Luma([match (ink, x < w / 2) {
                (0, true) | (1, false) | (2, false) => 255,
                _ => 0,
            }])
        });
        let mut buf = std::io::Cursor::new(Vec::new());
        planes
            .write_to(&mut buf, image::ImageFormat::Png)
            .expect("planes");
        let mut s = Shape::new(
            tracedraw_core::ShapeId(1),
            ShapeKind::Bitmap {
                rect: Rect::new(0.0, 0.0, 40.0, 20.0),
                width_px: w,
                height_px: h,
                png: png.clone().into(),
                fx: None,
            },
        );
        s.cmyk_bitmap = true;
        s.picture_inks = Some(Box::new(PictureInks::cmyk(&png, buf.into_inner().into())));
        // 25.4 dpi: 40 x 20 pixels shown, so 80 x 40 kept.
        fit_pictures(&mut s, 25.4, 0.0);
        let ShapeKind::Bitmap {
            width_px,
            height_px,
            png,
            ..
        } = &s.kind
        else {
            panic!("bitmap");
        };
        assert_eq!((*width_px, *height_px), (80, 40));
        let inks = s.picture_inks.as_deref().expect("inks kept");
        assert!(inks.fits(png.as_slice()));
        let at = |plate: Plate, x: u32| {
            let p = plate_picture(&s, plate).expect("plate");
            let img = crate::bitmap_fx::decode(&p).expect("png");
            assert_eq!(img.dimensions(), (80, 40));
            255 - img.get_pixel(x, 20).0[0]
        };
        assert!(at(Plate::Process(0), 10) > 250);
        assert!(at(Plate::Process(0), 70) < 5);
        assert!(at(Plate::Process(1), 70) > 250);
        assert!(at(Plate::Process(2), 70) > 250);
        assert!(at(Plate::Process(3), 10) < 5);
        // Pictures near the output's size stay as they are.
        let before = s.clone();
        fit_pictures(&mut s, 25.4, 0.0);
        assert_eq!(s, before);
    }
}
