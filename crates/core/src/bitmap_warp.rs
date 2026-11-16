//! A picture put in perspective (#2691): its pixels resampled through the
//! perspective's projective map into a new picture covering the box the
//! warped picture takes, transparent outside it. The outline of a curve
//! is warped node by node (`live::perspective_in`); a picture has no
//! outline to warp, and it became an empty frame, so the picture went.
//!
//! Each pixel of the new picture is mapped back through the inverse map
//! into the picture and takes the colour there, mixed from the four
//! pixels around it (bilinear, on premultiplied colours, so a pixel next
//! to a transparent one does not darken). The results are kept for the
//! latest pictures and corners, since a frame asks for its objects many
//! times.

use crate::geometry::{Point, Rect};
use crate::SharedBytes;
use kurbo::Affine;

/// The most pixels a warped picture gets: four times its own, and never
/// more than this.
const MAX_PIXELS: u64 = 25_000_000;
/// Warped pictures kept.
const MEMORY: usize = 8;

/// A picture: its place (local space), size in pixels and PNG.
#[derive(Clone)]
pub struct Picture {
    pub rect: Rect,
    pub width_px: u32,
    pub height_px: u32,
    pub png: SharedBytes,
}

type Key = (usize, usize, [u64; 4], [u64; 6], [u64; 4], [u64; 8]);

static KEPT: std::sync::Mutex<Vec<(Key, SharedBytes, Option<Picture>)>> =
    std::sync::Mutex::new(Vec::new());

/// The picture `pic` (in a space `m` maps its local space to) seen
/// through the perspective that takes box `b` of that space onto
/// `corners`: a new picture in that space. `None` when the picture does
/// not decode or the map is not one.
pub fn perspective(pic: &Picture, m: Affine, b: Rect, corners: &[Point; 4]) -> Option<Picture> {
    let bits = |v: &[f64]| -> Vec<u64> { v.iter().map(|x| x.to_bits()).collect() };
    let r = pic.rect;
    let key: Key = (
        pic.png.addr(),
        pic.png.len(),
        bits(&[r.x0, r.y0, r.x1, r.y1]).try_into().ok()?,
        bits(&m.as_coeffs()).try_into().ok()?,
        bits(&[b.x0, b.y0, b.x1, b.y1]).try_into().ok()?,
        bits(&corners.iter().flat_map(|p| [p.x, p.y]).collect::<Vec<_>>())
            .try_into()
            .ok()?,
    );
    if let Ok(kept) = KEPT.lock() {
        if let Some((_, _, out)) = kept.iter().find(|(k, _, _)| *k == key) {
            return out.clone();
        }
    }
    let out = warp(pic, m, b, corners);
    if let Ok(mut kept) = KEPT.lock() {
        if kept.len() >= MEMORY {
            kept.remove(0);
        }
        // The picture's bytes are held with the key, so its address
        // names no other picture while the result is kept.
        kept.push((key, pic.png.clone(), out.clone()));
    }
    out
}

fn warp(pic: &Picture, m: Affine, b: Rect, corners: &[Point; 4]) -> Option<Picture> {
    let (sw, sh, src) = decode_rgba(&pic.png)?;
    if sw == 0 || sh == 0 || b.width() < 1e-9 || b.height() < 1e-9 {
        return None;
    }
    let r = pic.rect;
    if r.width() < 1e-12 || r.height() < 1e-12 || m.determinant().abs() < 1e-18 {
        return None;
    }
    let from = [
        Point::new(b.x0, b.y0),
        Point::new(b.x1, b.y0),
        Point::new(b.x1, b.y1),
        Point::new(b.x0, b.y1),
    ];
    let corners = crate::live::corners_apart(corners);
    let h = crate::live::homography(&from, &corners)?;
    let back = crate::live::homography(&corners, &from)?;
    let w_of = |p: Point| h[6] * p.x + h[7] * p.y + 1.0;
    let side = w_of(b.center()).signum();
    // The picture's corners where they go.
    let pts = [
        Point::new(r.x0, r.y0),
        Point::new(r.x1, r.y0),
        Point::new(r.x1, r.y1),
        Point::new(r.x0, r.y1),
    ];
    let mut out = Rect::new(f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in &pts {
        let q = m * *p;
        if w_of(q).signum() != side {
            // A corner past the horizon: no picture to make.
            return None;
        }
        let w = crate::live::map_homography(&h, q);
        out = Rect::new(
            out.x0.min(w.x),
            out.y0.min(w.y),
            out.x1.max(w.x),
            out.y1.max(w.y),
        );
    }
    if !(out.width() > 1e-9 && out.height() > 1e-9) || !out.width().is_finite() {
        return None;
    }
    // As many pixels to the unit as the picture has where it is placed.
    let area = {
        let q: Vec<Point> = pts.iter().map(|p| m * *p).collect();
        let mut a = 0.0;
        for i in 0..4 {
            let (p, n) = (q[i], q[(i + 1) % 4]);
            a += p.x * n.y - n.x * p.y;
        }
        (a / 2.0).abs()
    };
    let density = ((sw as f64 * sh as f64) / area.max(1e-12)).sqrt();
    let mut ow = (out.width() * density).ceil().max(1.0);
    let mut oh = (out.height() * density).ceil().max(1.0);
    let cap = (4 * u64::from(sw) * u64::from(sh)).clamp(1, MAX_PIXELS) as f64;
    if ow * oh > cap {
        let k = (cap / (ow * oh)).sqrt();
        ow = (ow * k).floor().max(1.0);
        oh = (oh * k).floor().max(1.0);
    }
    let (ow, oh) = (ow as usize, oh as usize);
    let inv = m.inverse();
    let mut px = vec![0u8; ow * oh * 4];
    let (fw, fh) = (sw as f64, sh as f64);
    for j in 0..oh {
        // Rows from the top (y up in local space).
        let y = out.y1 - (j as f64 + 0.5) / oh as f64 * out.height();
        for i in 0..ow {
            let x = out.x0 + (i as f64 + 0.5) / ow as f64 * out.width();
            let q = crate::live::map_homography(&back, Point::new(x, y));
            if w_of(q).signum() != side {
                continue;
            }
            let p = inv * q;
            // Pixel coordinates in the picture (rows from the top).
            let u = (p.x - r.x0) / r.width() * fw - 0.5;
            let v = (r.y1 - p.y) / r.height() * fh - 0.5;
            if !(u > -1.0 && v > -1.0 && u < fw && v < fh) {
                continue;
            }
            let c = sample(&src, sw as usize, sh as usize, u, v);
            let at = (j * ow + i) * 4;
            px[at..at + 4].copy_from_slice(&c);
        }
    }
    let png = encode_rgba(ow as u32, oh as u32, &px)?;
    Some(Picture {
        rect: out,
        width_px: ow as u32,
        height_px: oh as u32,
        png: png.into(),
    })
}

/// The colour at pixel position (`u`, `v`) (pixel centres at whole
/// numbers), mixed from the four pixels around it on premultiplied
/// colours; outside the picture is transparent, so its edge fades over
/// half a pixel.
fn sample(src: &[u8], w: usize, h: usize, u: f64, v: f64) -> [u8; 4] {
    let (x0, y0) = (u.floor(), v.floor());
    let (fx, fy) = (u - x0, v - y0);
    let mut acc = [0.0f64; 4];
    for (dx, dy, k) in [
        (0, 0, (1.0 - fx) * (1.0 - fy)),
        (1, 0, fx * (1.0 - fy)),
        (0, 1, (1.0 - fx) * fy),
        (1, 1, fx * fy),
    ] {
        let (x, y) = (x0 as i64 + dx, y0 as i64 + dy);
        if x < 0 || y < 0 || x >= w as i64 || y >= h as i64 || k == 0.0 {
            continue;
        }
        let at = (y as usize * w + x as usize) * 4;
        let a = f64::from(src[at + 3]) / 255.0;
        acc[0] += f64::from(src[at]) * a * k;
        acc[1] += f64::from(src[at + 1]) * a * k;
        acc[2] += f64::from(src[at + 2]) * a * k;
        acc[3] += a * k;
    }
    if acc[3] <= 1e-9 {
        return [0, 0, 0, 0];
    }
    let a = acc[3];
    [
        (acc[0] / a).round().clamp(0.0, 255.0) as u8,
        (acc[1] / a).round().clamp(0.0, 255.0) as u8,
        (acc[2] / a).round().clamp(0.0, 255.0) as u8,
        (a * 255.0).round().clamp(0.0, 255.0) as u8,
    ]
}

/// The PNG's pixels as 8-bit RGBA, rows from the top.
pub fn decode_rgba(png: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let mut d = png::Decoder::new(std::io::Cursor::new(png));
    d.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = d.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let (w, h) = (info.width, info.height);
    let n = (w as usize) * (h as usize);
    let line = info.line_size;
    let mut out = Vec::with_capacity(n * 4);
    for row in 0..h as usize {
        let data = &buf[row * line..(row + 1) * line];
        match info.color_type {
            png::ColorType::Rgba => out.extend_from_slice(&data[..w as usize * 4]),
            png::ColorType::Rgb => {
                for p in data[..w as usize * 3].chunks_exact(3) {
                    out.extend_from_slice(&[p[0], p[1], p[2], 255]);
                }
            }
            png::ColorType::GrayscaleAlpha => {
                for p in data[..w as usize * 2].chunks_exact(2) {
                    out.extend_from_slice(&[p[0], p[0], p[0], p[1]]);
                }
            }
            png::ColorType::Grayscale => {
                for &g in &data[..w as usize] {
                    out.extend_from_slice(&[g, g, g, 255]);
                }
            }
            png::ColorType::Indexed => return None,
        }
    }
    Some((w, h, out))
}

/// 8-bit RGBA pixels, rows from the top, as a PNG.
pub fn encode_rgba(w: u32, h: u32, px: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut e = png::Encoder::new(&mut out, w, h);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        e.set_compression(png::Compression::Fast);
        let mut wr = e.write_header().ok()?;
        wr.write_image_data(px).ok()?;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picture(w: u32, h: u32, rect: Rect) -> Picture {
        // Left half red, right half blue, opaque.
        let mut px = Vec::new();
        for _ in 0..h {
            for x in 0..w {
                if x < w / 2 {
                    px.extend_from_slice(&[255, 0, 0, 255]);
                } else {
                    px.extend_from_slice(&[0, 0, 255, 255]);
                }
            }
        }
        Picture {
            rect,
            width_px: w,
            height_px: h,
            png: encode_rgba(w, h, &px).expect("png").into(),
        }
    }

    fn at(p: &Picture, x: f64, y: f64) -> [u8; 4] {
        let (w, h, px) = decode_rgba(&p.png).expect("decodes");
        let i = ((x - p.rect.x0) / p.rect.width() * w as f64) as usize;
        let j = ((p.rect.y1 - y) / p.rect.height() * h as f64) as usize;
        let k = (j.min(h as usize - 1) * w as usize + i.min(w as usize - 1)) * 4;
        [px[k], px[k + 1], px[k + 2], px[k + 3]]
    }

    /// The corners where they are: the same picture back.
    #[test]
    fn unmoved_corners_give_the_picture_back() {
        let rect = Rect::new(0.0, 0.0, 40.0, 20.0);
        let pic = picture(40, 20, rect);
        let c = [
            Point::new(0.0, 0.0),
            Point::new(40.0, 0.0),
            Point::new(40.0, 20.0),
            Point::new(0.0, 20.0),
        ];
        let out = perspective(&pic, Affine::IDENTITY, rect, &c).expect("warped");
        assert!((out.rect.x0 - 0.0).abs() < 1e-9 && (out.rect.x1 - 40.0).abs() < 1e-9);
        assert_eq!(at(&out, 5.0, 10.0), [255, 0, 0, 255]);
        assert_eq!(at(&out, 35.0, 10.0), [0, 0, 255, 255]);
    }

    /// The top pulled together: a trapezium, transparent beside its
    /// slanted sides, red on the left and blue on the right.
    #[test]
    fn a_narrowed_top_makes_a_trapezium() {
        let rect = Rect::new(0.0, 0.0, 100.0, 50.0);
        let pic = picture(100, 50, rect);
        let c = [
            Point::new(0.0, 0.0),
            Point::new(100.0, 0.0),
            Point::new(70.0, 50.0),
            Point::new(30.0, 50.0),
        ];
        let out = perspective(&pic, Affine::IDENTITY, rect, &c).expect("warped");
        assert!((out.rect.width() - 100.0).abs() < 1e-9, "{:?}", out.rect);
        // Near the top corners: outside the trapezium.
        assert_eq!(at(&out, 3.0, 48.0)[3], 0);
        assert_eq!(at(&out, 97.0, 48.0)[3], 0);
        // Inside: the picture's halves.
        assert_eq!(at(&out, 40.0, 45.0), [255, 0, 0, 255]);
        assert_eq!(at(&out, 60.0, 45.0), [0, 0, 255, 255]);
        assert_eq!(at(&out, 10.0, 3.0), [255, 0, 0, 255]);
        assert_eq!(at(&out, 90.0, 3.0), [0, 0, 255, 255]);
    }
}
