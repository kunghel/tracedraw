//! Painted looks in Art Strokes: Paper Layers, Bristle Paint, Mural,
//! Luminous Wash, Inked Poster, Diagonal Smear, Blotted Color and Primed
//! Canvas, plus the field helpers (brightness, edges, smears, surface
//! relief) other hand-made looks share. All keep alpha.

use super::util::*;
use super::*;
use image::{Rgba, RgbaImage};

/// Paper Layers, listed in Art Strokes.
pub const PAPER_LAYERS: EffectSpec = EffectSpec {
    id: "paper_layers",
    params: &[
        range("levels", 2.0, 8.0, 4.0, 1.0),
        range("smoothness", 0.0, 10.0, 4.0, 1.0),
        range("detail", 1.0, 10.0, 5.0, 1.0),
    ],
    apply: paper_layers,
    reach: no_reach,
};

/// Bristle Paint, listed in Art Strokes.
pub const BRISTLE_PAINT: EffectSpec = EffectSpec {
    id: "bristle_paint",
    params: &[
        range("brush_size", 1.0, 10.0, 2.0, 1.0),
        range("detail", 0.0, 10.0, 8.0, 1.0),
        range("texture", 0.0, 100.0, 50.0, 1.0),
        range("seed", 0.0, 9999.0, 1.0, 1.0),
    ],
    apply: bristle_paint,
    reach: no_reach,
};

/// Mural, listed in Art Strokes.
pub const MURAL_PAINT: EffectSpec = EffectSpec {
    id: "mural_paint",
    params: &[
        range("brush_size", 1.0, 10.0, 2.0, 1.0),
        range("detail", 0.0, 10.0, 8.0, 1.0),
        range("texture", 0.0, 100.0, 50.0, 1.0),
        range("seed", 0.0, 9999.0, 1.0, 1.0),
    ],
    apply: mural_paint,
    reach: no_reach,
};

/// Luminous Wash, listed in Art Strokes.
pub const LUMINOUS_WASH: EffectSpec = EffectSpec {
    id: "luminous_wash",
    params: &[
        range("radius", 0.0, 24.0, 5.0, 1.0),
        range("brightness", 0.0, 50.0, 15.0, 1.0),
        color("light_color", 0x2BC8FF),
    ],
    apply: luminous_wash,
    reach: no_reach,
};

/// Inked Poster, listed in Art Strokes.
pub const INKED_POSTER: EffectSpec = EffectSpec {
    id: "inked_poster",
    params: &[
        range("edge", 0.0, 10.0, 2.0, 1.0),
        range("intensity", 0.0, 10.0, 1.0, 1.0),
        range("levels", 2.0, 8.0, 3.0, 1.0),
    ],
    apply: inked_poster,
    reach: no_reach,
};

/// Diagonal Smear, listed in Art Strokes.
pub const DIAGONAL_SMEAR: EffectSpec = EffectSpec {
    id: "diagonal_smear",
    params: &[
        range("length", 0.0, 10.0, 4.0, 1.0),
        range("highlights", 0.0, 20.0, 0.0, 1.0),
        range("intensity", 0.0, 10.0, 10.0, 1.0),
    ],
    apply: diagonal_smear,
    reach: no_reach,
};

/// Blotted Color, listed in Art Strokes.
pub const BLOTTED_COLOR: EffectSpec = EffectSpec {
    id: "blotted_color",
    params: &[
        range("brush_size", 0.0, 10.0, 2.0, 1.0),
        range("contrast", 0.0, 25.0, 12.0, 1.0),
        range("smoothness", 1.0, 15.0, 5.0, 1.0),
        range("seed", 0.0, 9999.0, 1.0, 1.0),
    ],
    apply: blotted_color,
    reach: no_reach,
};

/// Primed Canvas, listed in Art Strokes.
pub const PRIMED_CANVAS: EffectSpec = EffectSpec {
    id: "primed_canvas",
    params: &[
        range("brush_size", 0.0, 40.0, 6.0, 1.0),
        range("density", 0.0, 40.0, 16.0, 1.0),
        range("relief", 0.0, 50.0, 4.0, 1.0),
        angle("angle", 135.0),
    ],
    apply: primed_canvas,
    reach: no_reach,
};

// ---------------------------------------------------------------- helpers

/// The pixel with new colour channels (0..255 floats), alpha kept.
pub(super) fn px(p: &Rgba<u8>, c: [f32; 3]) -> Rgba<u8> {
    Rgba([clamp8(c[0]), clamp8(c[1]), clamp8(c[2]), p[3]])
}

pub(super) fn chans(p: &Rgba<u8>) -> [f32; 3] {
    [p[0] as f32, p[1] as f32, p[2] as f32]
}

/// Brightness of every pixel, 0..1.
pub(super) fn gray(img: &RgbaImage) -> Vec<f32> {
    img.pixels().map(|p| luma(p) / 255.0).collect()
}

/// `v` (0..255) cut into `levels` even steps.
pub(super) fn quant(v: f32, levels: u32) -> f32 {
    let n = levels.max(2) as f32 - 1.0;
    ((v / 255.0 * n).round() / n * 255.0).clamp(0.0, 255.0)
}

fn at(v: &[f32], w: u32, h: u32, x: i64, y: i64) -> f32 {
    let x = x.clamp(0, w as i64 - 1) as u32;
    let y = y.clamp(0, h as i64 - 1) as u32;
    v[(y * w + x) as usize]
}

/// Edge strength of a field (Sobel), about 0..1 for 0..1 fields.
pub(super) fn sobel(v: &[f32], w: u32, h: u32) -> Vec<f32> {
    let mut out = vec![0.0; v.len()];
    if w == 0 || h == 0 {
        return out;
    }
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            let a = |dx: i64, dy: i64| at(v, w, h, x + dx, y + dy);
            let gx = a(1, -1) + 2.0 * a(1, 0) + a(1, 1) - a(-1, -1) - 2.0 * a(-1, 0) - a(-1, 1);
            let gy = a(-1, 1) + 2.0 * a(0, 1) + a(1, 1) - a(-1, -1) - 2.0 * a(0, -1) - a(1, -1);
            out[(y as u32 * w + x as u32) as usize] = ((gx * gx + gy * gy).sqrt() / 4.0).min(1.0);
        }
    }
    out
}

/// One running-sum box pass of radius `r` along rows or columns.
fn box_field_pass(v: &[f32], w: u32, h: u32, r: i64, horizontal: bool) -> Vec<f32> {
    let mut out = vec![0.0; v.len()];
    let (len, lines) = if horizontal {
        (w as i64, h)
    } else {
        (h as i64, w)
    };
    let n = (2 * r + 1) as f32;
    for line in 0..lines {
        let get = |i: i64| -> f32 {
            let i = i.clamp(0, len - 1) as u32;
            if horizontal {
                v[(line * w + i) as usize]
            } else {
                v[(i * w + line) as usize]
            }
        };
        let mut acc: f32 = (-r..=r).map(get).sum();
        for i in 0..len {
            let o = if horizontal {
                (line * w + i as u32) as usize
            } else {
                (i as u32 * w + line) as usize
            };
            out[o] = acc / n;
            acc += get(i + r + 1) - get(i - r);
        }
    }
    out
}

/// A field blurred by about `sigma` pixels (three box passes).
pub(super) fn blur_field(v: &[f32], w: u32, h: u32, sigma: f32) -> Vec<f32> {
    if sigma < 0.3 || w == 0 || h == 0 {
        return v.to_vec();
    }
    let r = ((sigma * sigma + 1.0).sqrt().round() as i64).max(1);
    let mut b = v.to_vec();
    for _ in 0..3 {
        b = box_field_pass(&b, w, h, r, true);
        b = box_field_pass(&b, w, h, r, false);
    }
    b
}

/// The largest value of a field within `r` pixels (a square).
pub(super) fn dilate_field(v: &[f32], w: u32, h: u32, r: u32) -> Vec<f32> {
    if r == 0 || w == 0 || h == 0 {
        return v.to_vec();
    }
    let r = r as i64;
    let mut tmp = vec![0.0; v.len()];
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            let m = (-r..=r)
                .map(|d| at(v, w, h, x + d, y))
                .fold(0.0f32, f32::max);
            tmp[(y as u32 * w + x as u32) as usize] = m;
        }
    }
    let mut out = vec![0.0; v.len()];
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            let m = (-r..=r)
                .map(|d| at(&tmp, w, h, x, y + d))
                .fold(0.0f32, f32::max);
            out[(y as u32 * w + x as u32) as usize] = m;
        }
    }
    out
}

/// Every pixel the average of `len` pixels along a line at `angle`
/// degrees (counter-clockwise from the right) centred on it.
pub(super) fn smear(img: &RgbaImage, angle: f32, len: u32) -> RgbaImage {
    if len < 2 {
        return img.clone();
    }
    let (dx, dy) = (angle.to_radians().cos(), -angle.to_radians().sin());
    let half = len as f32 / 2.0;
    map_px(img, |x, y, p| {
        let mut acc = [0f32; 4];
        let mut n = 0.0;
        let mut t = -half;
        while t <= half {
            add_premultiplied(&mut acc, &sample(img, x as f32 + dx * t, y as f32 + dy * t));
            n += 1.0;
            t += 1.0;
        }
        let mut q = unpremultiply(acc, n);
        q[3] = p[3];
        q
    })
}

/// The image lit as a raised surface: `heights` (0..1) shaded from
/// `angle` degrees, `amount` 0..1 how strongly light and shade show.
pub(super) fn relief(img: &RgbaImage, heights: &[f32], angle: f32, amount: f32) -> RgbaImage {
    let (w, h) = img.dimensions();
    if amount <= 0.0 || w == 0 || h == 0 {
        return img.clone();
    }
    let s = shade(heights, w, h, angle, 4.0);
    map_px(img, |x, y, p| {
        let k = 1.0 + (s[(y * w + x) as usize] - 0.5) * 2.0 * amount;
        px(p, chans(p).map(|c| c * k))
    })
}

/// A woven canvas surface, 0..1, threads about `size` pixels apart.
pub(super) fn canvas_weave(w: u32, h: u32, size: f32, seed: u32) -> Vec<f32> {
    let s = size.max(2.0);
    let mut out = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = (x as f32 / s, y as f32 / s);
            let warp = ((fx * std::f32::consts::PI).sin() * 0.5 + 0.5)
                * ((fy * std::f32::consts::PI).cos() * 0.5 + 0.5);
            let n = value_noise(x as f32, y as f32, s * 0.7, seed);
            out.push((warp * 0.75 + n * 0.25).clamp(0.0, 1.0));
        }
    }
    out
}

// ---------------------------------------------------------------- effects

/// Flat shapes of a few colours, as if cut from layers of coloured paper.
fn paper_layers(img: &RgbaImage, p: &P) -> RgbaImage {
    let soft = gaussian(img, p.f32("smoothness") * 0.6);
    let r = (11 - p.u("detail").min(10)) / 2;
    let flat = rank(&soft, r, 0.5);
    let levels = p.u("levels");
    // Colours cut by brightness and hue together keep shapes whole.
    map_px(&flat, |x, y, q| {
        let c = chans(q).map(|v| quant(v, levels));
        px(img.get_pixel(x, y), c)
    })
}

/// Dabs of a dry brush: flat colour with bristle streaks.
fn bristle_paint(img: &RgbaImage, p: &P) -> RgbaImage {
    let size = p.u("brush_size");
    let flat = rank(img, size, 0.5);
    let levels = 4 + p.u("detail") * 3;
    let tex = p.f32("texture") / 100.0;
    let seed = p.u("seed");
    let s = size as f32 * 2.0 + 2.0;
    map_px(&flat, |x, y, q| {
        // Noise stretched along the stroke: bristle lines.
        let n = value_noise(x as f32 * 0.25, y as f32 * 1.5, s, seed) - 0.5;
        let k = 1.0 + n * tex * 0.9;
        let c = chans(q).map(|v| quant(v, levels) * k);
        px(img.get_pixel(x, y), c)
    })
}

/// Short thick round strokes over a rough wall: strong colour, dark edges.
fn mural_paint(img: &RgbaImage, p: &P) -> RgbaImage {
    let size = p.u("brush_size");
    let flat = rank(&gaussian(img, size as f32 * 0.5), size, 0.5);
    let dark = rank(&flat, 1 + size / 3, 0.0);
    let (w, h) = img.dimensions();
    let edges = sobel(&gray(&flat), w, h);
    let detail = p.f32("detail") / 10.0;
    let tex = p.f32("texture") / 100.0;
    let seed = p.u("seed");
    let s = size as f32 * 3.0 + 3.0;
    map_px(&flat, |x, y, q| {
        let e = (edges[(y * w + x) as usize] * 4.0 * (1.2 - detail * 0.6)).min(1.0);
        let d = dark.get_pixel(x, y);
        let blot = fractal(x as f32, y as f32, s, seed) - 0.5;
        let c = [0, 1, 2].map(|i| {
            let v = q[i] as f32 * (1.0 - e) + d[i] as f32 * e;
            ((v - 128.0) * 1.25 + 128.0) * (1.0 + blot * tex * 0.6)
        });
        px(img.get_pixel(x, y), c)
    })
}

/// The picture as glowing tinted light over darkness.
fn luminous_wash(img: &RgbaImage, p: &P) -> RgbaImage {
    let (w, h) = img.dimensions();
    let g = gray(img);
    let glow = blur_field(
        &g.iter().map(|v| v * v).collect::<Vec<_>>(),
        w,
        h,
        p.f32("radius"),
    );
    let tint = p.rgb("light_color").map(|v| v as f32);
    let bright = p.f32("brightness") / 50.0;
    map_px(img, |x, y, q| {
        let i = (y * w + x) as usize;
        let base = chans(q).map(|v| v * 0.55);
        let l = (glow[i] * (0.6 + bright * 1.4)).min(1.0);
        // Screen the coloured glow over the dimmed picture.
        let c = [0, 1, 2].map(|k| 255.0 - (255.0 - base[k]) * (1.0 - tint[k] / 255.0 * l));
        px(q, c)
    })
}

/// Fewer colours with the edges drawn in black ink.
fn inked_poster(img: &RgbaImage, p: &P) -> RgbaImage {
    let (w, h) = img.dimensions();
    let levels = p.u("levels");
    let soft = gaussian(img, 0.8);
    let e = sobel(&gray(&soft), w, h);
    let e = dilate_field(&e, w, h, p.u("edge") / 3);
    let k = 2.0 + p.f32("intensity") * 1.2;
    map_px(&soft, |x, y, q| {
        let ink = ((e[(y * w + x) as usize] * k - 0.15) * 2.5).clamp(0.0, 1.0);
        let c = chans(q).map(|v| quant(v, levels) * (1.0 - ink));
        px(img.get_pixel(x, y), c)
    })
}

/// Dark areas smeared along a diagonal, highlights lifted.
fn diagonal_smear(img: &RgbaImage, p: &P) -> RgbaImage {
    let len = p.u("length") * 2 + 1;
    let sm = smear(img, 45.0, len);
    let hi = p.f32("highlights") / 20.0;
    let inten = p.f32("intensity") / 10.0;
    map_px(img, |x, y, q| {
        let s = sm.get_pixel(x, y);
        let l = luma(q) / 255.0;
        let t = ((1.0 - l) * 1.4).min(1.0) * inten;
        let c = [0, 1, 2].map(|i| {
            let v = q[i] as f32 * (1.0 - t) + s[i] as f32 * t;
            v + (255.0 - v) * hi * l * l
        });
        px(q, c)
    })
}

/// Soft dabs of colour as if blotted on with a sponge.
fn blotted_color(img: &RgbaImage, p: &P) -> RgbaImage {
    let (w, h) = img.dimensions();
    let size = p.u("brush_size");
    let base = rank(img, size / 2, 0.5);
    let seed = p.u("seed");
    let s = size as f32 * 1.5 + 2.0;
    let mask: Vec<f32> = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .map(|(x, y)| {
            let n = value_noise(x as f32, y as f32, s, seed) * 0.7
                + hash01(x / (size + 1), y / (size + 1), seed ^ 0x5A5A) * 0.3;
            if n > 0.5 {
                1.0
            } else {
                0.0
            }
        })
        .collect();
    let mask = blur_field(&mask, w, h, p.f32("smoothness") * 0.4);
    let k = p.f32("contrast") / 25.0;
    map_px(&base, |x, y, q| {
        let m = mask[(y * w + x) as usize] - 0.5;
        let c = chans(q).map(|v| v * (1.0 + m * k * 0.8));
        px(img.get_pixel(x, y), c)
    })
}

/// The picture painted over a soft, muted underlayer on a canvas surface.
fn primed_canvas(img: &RgbaImage, p: &P) -> RgbaImage {
    let (w, h) = img.dimensions();
    let size = p.f32("brush_size");
    let under = map_px(&gaussian(img, size), |_, _, q| {
        let l = luma(q);
        px(q, chans(q).map(|v| l + (v - l) * 0.75))
    });
    let cover = 1.0 - p.f32("density") / 40.0 * 0.7;
    let painted = mix(&rank(img, (size / 6.0) as u32, 0.5), &under, 1.0 - cover);
    let painted = map_px(&painted, |x, y, q| px(img.get_pixel(x, y), chans(q)));
    let weave = canvas_weave(w, h, 4.0, 7);
    relief(&painted, &weave, p.f32("angle"), p.f32("relief") / 50.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &RgbaImage, id: &str, params: &[(&str, f64)]) -> RgbaImage {
        let mut e = new_effect(id);
        for (k, v) in params {
            e.params.insert(k.to_string(), *v);
        }
        apply_effect(src, &e)
    }

    fn photo() -> RgbaImage {
        RgbaImage::from_fn(40, 32, |x, y| {
            let a = if (x as i32 - 20).pow(2) + (y as i32 - 16).pow(2) < 100 {
                1
            } else {
                0
            };
            Rgba([
                (x * 6) as u8,
                (y * 7) as u8,
                if a == 1 { 240 } else { 40 },
                if x == 0 { 0 } else { 255 },
            ])
        })
    }

    fn colours(img: &RgbaImage) -> usize {
        img.pixels()
            .map(|p| [p[0], p[1], p[2]])
            .collect::<std::collections::BTreeSet<_>>()
            .len()
    }

    #[test]
    fn every_painted_look_changes_the_picture_and_keeps_alpha() {
        let src = photo();
        for id in [
            "paper_layers",
            "bristle_paint",
            "mural_paint",
            "luminous_wash",
            "inked_poster",
            "diagonal_smear",
            "blotted_color",
            "primed_canvas",
        ] {
            let out = run(&src, id, &[]);
            assert_eq!(out.dimensions(), src.dimensions(), "{id}");
            assert_ne!(out, src, "{id}");
            assert!(
                out.pixels().zip(src.pixels()).all(|(a, b)| a[3] == b[3]),
                "{id} alpha"
            );
        }
    }

    #[test]
    fn paper_layers_and_inked_poster_use_few_colours() {
        let src = photo();
        let two = run(&src, "paper_layers", &[("levels", 2.0)]);
        assert!(two
            .pixels()
            .all(|p| (0..3).all(|c| p[c] == 0 || p[c] == 255)));
        assert!(colours(&run(&src, "paper_layers", &[("levels", 3.0)])) <= 27);
        // Edges get dark ink, flat areas keep their (cut) colour.
        let poster = run(&src, "inked_poster", &[("intensity", 10.0)]);
        let edge = poster.get_pixel(20, 6);
        let flat = poster.get_pixel(20, 16);
        assert!(luma(edge) < luma(flat), "{edge:?} {flat:?}");
    }

    #[test]
    fn luminous_wash_takes_the_light_colour() {
        let white = RgbaImage::from_pixel(16, 16, Rgba([255, 255, 255, 255]));
        let black = RgbaImage::from_pixel(16, 16, Rgba([0, 0, 0, 255]));
        let red = [("light_color", 0xFF0000 as f64)];
        let lit = run(&white, "luminous_wash", &red);
        let p = lit.get_pixel(8, 8);
        assert!(p[0] > p[1] + 40, "{p:?}");
        // Darkness stays dark.
        assert!(luma(run(&black, "luminous_wash", &red).get_pixel(8, 8)) < 10.0);
    }

    #[test]
    fn diagonal_smear_blurs_dark_areas_along_the_diagonal() {
        // A dark picture with a vertical line: the line spreads.
        let src = RgbaImage::from_fn(30, 30, |x, _| {
            if x == 15 {
                Rgba([90, 90, 90, 255])
            } else {
                Rgba([10, 10, 10, 255])
            }
        });
        let out = run(&src, "diagonal_smear", &[("length", 5.0)]);
        assert!(out.get_pixel(15, 15)[0] < 90);
        assert!(out.get_pixel(16, 15)[0] > 10);
    }

    #[test]
    fn primed_canvas_and_bristles_follow_their_settings() {
        let grey = RgbaImage::from_pixel(32, 32, Rgba([128, 128, 128, 255]));
        // A flat picture shows only the canvas, more with more relief.
        let low = run(&grey, "primed_canvas", &[("relief", 0.0)]);
        let high = run(&grey, "primed_canvas", &[("relief", 50.0)]);
        assert!(colours(&high) > colours(&low));
        let a = run(&grey, "bristle_paint", &[("seed", 3.0), ("texture", 100.0)]);
        assert_eq!(
            a,
            run(&grey, "bristle_paint", &[("seed", 3.0), ("texture", 100.0)])
        );
        assert_ne!(
            a,
            run(&grey, "bristle_paint", &[("seed", 4.0), ("texture", 100.0)])
        );
        assert_eq!(
            colours(&run(&grey, "bristle_paint", &[("texture", 0.0)])),
            1
        );
    }
}
