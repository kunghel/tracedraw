//! Effects > Glitch: colour channels pulled apart, screen lines, sorted
//! runs of pixels, damaged compression blocks and shifted bands. Then the
//! colour maps (a gradient map and dithering, in Color Transform) and
//! polar coordinates (in Distort), which share the helpers here.

use super::util::*;
use super::*;
use image::{Rgba, RgbaImage};
use std::f32::consts::{PI, TAU};

pub const EFFECTS: &[EffectSpec] = &[
    EffectSpec {
        id: "rgb_split",
        params: &[
            choice("split_mode", &["split_linear", "split_radial"], 0),
            range("distance", 0.0, 200.0, 6.0, 1.0),
            angle("angle", 0.0),
        ],
        apply: rgb_split,
        reach: no_reach,
    },
    EffectSpec {
        id: "scanlines",
        params: &[
            range("spacing", 2.0, 100.0, 4.0, 1.0),
            range("width", 1.0, 50.0, 2.0, 1.0),
            range("opacity", 0.0, 100.0, 40.0, 1.0),
            range("curvature", 0.0, 100.0, 0.0, 1.0),
        ],
        apply: scanlines,
        reach: no_reach,
    },
    EffectSpec {
        id: "pixel_sort",
        params: &[
            choice(
                "sort_key",
                &["sort_brightness", "sort_hue", "sort_saturation"],
                0,
            ),
            choice("sort_direction", &["horizontal", "vertical"], 0),
            range("threshold_low", 0.0, 100.0, 25.0, 1.0),
            range("threshold_high", 0.0, 100.0, 85.0, 1.0),
            check("reverse", false),
        ],
        apply: pixel_sort,
        reach: no_reach,
    },
    EffectSpec {
        id: "compression_blocks",
        params: &[
            range("size", 4.0, 64.0, 8.0, 1.0),
            range("amount", 0.0, 100.0, 20.0, 1.0),
            range("distance", 0.0, 200.0, 24.0, 1.0),
            range("artifacts", 0.0, 100.0, 50.0, 1.0),
            range("seed", 0.0, 9999.0, 1.0, 1.0),
        ],
        apply: compression_blocks,
        reach: no_reach,
    },
    EffectSpec {
        id: "band_shift",
        params: &[
            range("height", 1.0, 200.0, 12.0, 1.0),
            range("amount", 0.0, 100.0, 35.0, 1.0),
            range("distance", 0.0, 400.0, 30.0, 1.0),
            check("color_shift", true),
            range("seed", 0.0, 9999.0, 1.0, 1.0),
        ],
        apply: band_shift,
        reach: no_reach,
    },
];

/// Gradient Map, listed in Color Transform.
pub const GRADIENT_MAP: EffectSpec = EffectSpec {
    id: "gradient_map",
    params: &[
        choice(
            "gradient_preset",
            &[
                "map_custom",
                "map_sunset",
                "map_ocean",
                "map_neon",
                "map_fire",
                "map_infrared",
            ],
            1,
        ),
        color("shadow_color", 0x1A1033),
        color("midtone_color", 0xC0306A),
        color("highlight_color", 0xFFE08A),
        range("midpoint", 1.0, 99.0, 50.0, 1.0),
        check("invert", false),
        range("opacity", 0.0, 100.0, 100.0, 1.0),
    ],
    apply: gradient_map,
    reach: no_reach,
};

/// Dither, listed in Color Transform.
pub const DITHER: EffectSpec = EffectSpec {
    id: "dither",
    params: &[
        choice(
            "dither_method",
            &[
                "bayer_2",
                "bayer_4",
                "bayer_8",
                "floyd_steinberg",
                "atkinson",
                "blue_noise",
            ],
            2,
        ),
        range("levels", 2.0, 16.0, 2.0, 1.0),
        check("two_colors", true),
        color("dark_color", 0x111111),
        color("light_color", 0xF2EFE6),
    ],
    apply: dither,
    reach: no_reach,
};

/// Polar Coordinates, listed in Distort.
pub const POLAR: EffectSpec = EffectSpec {
    id: "polar_coordinates",
    params: &[choice("polar_mode", &["rect_to_polar", "polar_to_rect"], 0)],
    apply: polar_coordinates,
    reach: no_reach,
};

/// The red channel from one place, green from the pixel itself and blue
/// from the other side: along a direction, or out from the middle with
/// the distance growing toward the edges.
pub fn rgb_split(img: &RgbaImage, p: &P) -> RgbaImage {
    let d = p.f32("distance");
    if d <= 0.0 {
        return img.clone();
    }
    let radial = p.i("split_mode") == 1;
    let a = p.f32("angle").to_radians();
    let (ux, uy) = (a.cos(), -a.sin());
    let (cx, cy) = (img.width() as f32 / 2.0, img.height() as f32 / 2.0);
    let reach = (cx * cx + cy * cy).sqrt().max(1.0);
    map_px(img, |x, y, q| {
        let (fx, fy) = (x as f32, y as f32);
        let (ox, oy) = if radial {
            let (dx, dy) = (fx - cx, fy - cy);
            (dx / reach * d, dy / reach * d)
        } else {
            (ux * d, uy * d)
        };
        // Past the edges a channel repeats the edge rather than bringing
        // in a coloured fringe.
        let r = bilinear(img, fx - ox, fy - oy, false);
        let b = bilinear(img, fx + ox, fy + oy, false);
        // Where a shifted channel brings colour over transparency, the
        // pixel takes its alpha.
        let alpha = q[3].max(r[3]).max(b[3]);
        Rgba([r[0], q[1], b[2], alpha])
    })
}

/// Dark lines across the picture, `width` pixels of every `spacing`,
/// with a screen's bulge (`curvature`) bending the picture and the lines
/// and darkening the corners.
pub fn scanlines(img: &RgbaImage, p: &P) -> RgbaImage {
    let spacing = p.f32("spacing").max(2.0);
    let width = p.f32("width").clamp(1.0, spacing);
    let opacity = p.f32("opacity") / 100.0;
    let k = p.f32("curvature") / 100.0 * 0.35;
    let (w, h) = (img.width() as f32, img.height() as f32);
    let (cx, cy) = (w / 2.0, h / 2.0);
    let bent = |x: f32, y: f32| -> (f32, f32) {
        if k <= 0.0 {
            return (x, y);
        }
        let (nx, ny) = ((x - cx) / cx.max(1.0), (y - cy) / cy.max(1.0));
        let f = 1.0 + k * (nx * nx + ny * ny);
        (cx + nx * f * cx, cy + ny * f * cy)
    };
    let mut out = RgbaImage::new(img.width(), img.height());
    for y in 0..img.height() {
        for x in 0..img.width() {
            let (sx, sy) = bent(x as f32, y as f32);
            let mut px = if k > 0.0 {
                bilinear(img, sx, sy, true)
            } else {
                *img.get_pixel(x, y)
            };
            // The share of the pixel the lines cover, from four rows across
            // it, so bent lines stay smooth instead of beating into rings.
            let (_, sy_next) = bent(x as f32, y as f32 + 1.0);
            let dy = sy_next - sy;
            let cover = (0..4)
                .filter(|i| (sy + dy * (*i as f32 + 0.5) / 4.0).rem_euclid(spacing) < width)
                .count() as f32
                / 4.0;
            let line = cover * opacity;
            let corner = if k > 0.0 {
                let (nx, ny) = ((sx - cx) / cx.max(1.0), (sy - cy) / cy.max(1.0));
                (1.0 - 0.6 * k * (nx * nx * ny * ny).sqrt() * 4.0).clamp(0.0, 1.0)
            } else {
                1.0
            };
            let f = (1.0 - line) * corner;
            for c in 0..3 {
                px[c] = clamp8(px[c] as f32 * f);
            }
            out.put_pixel(x, y, px);
        }
    }
    out
}

fn sort_key(p: &Rgba<u8>, key: usize) -> f32 {
    match key {
        1 | 2 => {
            let (h, s, _) = rgb_to_hsl(
                p[0] as f32 / 255.0,
                p[1] as f32 / 255.0,
                p[2] as f32 / 255.0,
            );
            if key == 1 {
                h / 360.0
            } else {
                s
            }
        }
        _ => luma(p) / 255.0,
    }
}

/// In every row (or column), the runs of pixels whose key lies between
/// the two thresholds sorted by that key; the others stay where they are.
pub fn pixel_sort(img: &RgbaImage, p: &P) -> RgbaImage {
    let key = p.i("sort_key");
    let vertical = p.i("sort_direction") == 1;
    let (mut lo, mut hi) = (
        p.f32("threshold_low") / 100.0,
        p.f32("threshold_high") / 100.0,
    );
    if lo > hi {
        std::mem::swap(&mut lo, &mut hi);
    }
    let reverse = p.b("reverse");
    let (w, h) = (img.width(), img.height());
    let mut out = img.clone();
    let (lines, len) = if vertical { (w, h) } else { (h, w) };
    let at = |line: u32, i: u32| if vertical { (line, i) } else { (i, line) };
    let mut run: Vec<(f32, Rgba<u8>)> = Vec::new();
    for line in 0..lines {
        let mut start = 0u32;
        run.clear();
        for i in 0..=len {
            let inside = i < len && {
                let (x, y) = at(line, i);
                let q = img.get_pixel(x, y);
                let v = sort_key(q, key);
                q[3] > 0 && v >= lo && v <= hi
            };
            if inside {
                if run.is_empty() {
                    start = i;
                }
                let (x, y) = at(line, i);
                let q = *img.get_pixel(x, y);
                run.push((sort_key(&q, key), q));
                continue;
            }
            if run.len() > 1 {
                run.sort_by(|a, b| a.0.total_cmp(&b.0));
                if reverse {
                    run.reverse();
                }
                for (k, (_, q)) in run.iter().enumerate() {
                    let (x, y) = at(line, start + k as u32);
                    out.put_pixel(x, y, *q);
                }
            }
            run.clear();
        }
    }
    out
}

/// Square blocks of `size` pixels: `amount` percent of them, picked by
/// `seed`, are copied from a block up to `distance` away or repeat the
/// row above them, and every block loses detail toward its average by
/// `artifacts`, with its colour stepped as a heavily compressed picture.
pub fn compression_blocks(img: &RgbaImage, p: &P) -> RgbaImage {
    let size = p.u("size").max(4);
    let amount = p.f32("amount") / 100.0;
    let dist = p.f32("distance");
    let artifacts = p.f32("artifacts") / 100.0;
    let seed = p.u("seed");
    let (w, h) = (img.width(), img.height());
    let mut out = img.clone();
    let (bw, bh) = (w.div_ceil(size), h.div_ceil(size));
    for by in 0..bh {
        for bx in 0..bw {
            let (x0, y0) = (bx * size, by * size);
            let (x1, y1) = ((x0 + size).min(w), (y0 + size).min(h));
            let r = hash01(bx, by, seed);
            let glitched = r < amount;
            // Where this block's pixels come from.
            let (sx, sy, repeat) = if glitched {
                let a = hash01(bx, by, seed.wrapping_add(11)) * TAU;
                let d = hash01(bx, by, seed.wrapping_add(23)) * dist;
                let repeat = hash01(bx, by, seed.wrapping_add(37)) < 0.35;
                (
                    (a.cos() * d).round() as i64,
                    (a.sin() * d).round() as i64,
                    repeat,
                )
            } else {
                (0, 0, false)
            };
            let src = |x: u32, y: u32| -> Rgba<u8> {
                if repeat {
                    // The row just above the block, smeared down.
                    let yy = y0.saturating_sub(1);
                    *img.get_pixel(x, yy)
                } else {
                    let xx = (x as i64 + sx).clamp(0, w as i64 - 1) as u32;
                    let yy = (y as i64 + sy).clamp(0, h as i64 - 1) as u32;
                    *img.get_pixel(xx, yy)
                }
            };
            let mut acc = [0f32; 3];
            let mut n = 0f32;
            for y in y0..y1 {
                for x in x0..x1 {
                    let q = src(x, y);
                    for c in 0..3 {
                        acc[c] += q[c] as f32;
                    }
                    n += 1.0;
                }
            }
            let mean = acc.map(|v| v / n.max(1.0));
            // Coarser colour steps the more artifacts there are.
            let step = 1.0 + artifacts * 40.0;
            for y in y0..y1 {
                for x in x0..x1 {
                    let mut q = src(x, y);
                    for c in 0..3 {
                        let v = q[c] as f32 * (1.0 - artifacts * 0.7) + mean[c] * artifacts * 0.7;
                        q[c] = clamp8((v / step).round() * step);
                    }
                    q[3] = img.get_pixel(x, y)[3];
                    out.put_pixel(x, y, q);
                }
            }
        }
    }
    out
}

/// The picture in bands about `height` pixels high: `amount` percent of
/// them slide sideways by up to `distance`, some with their red and blue
/// pulled apart (`color_shift`).
pub fn band_shift(img: &RgbaImage, p: &P) -> RgbaImage {
    let band = p.f32("height").max(1.0);
    let amount = p.f32("amount") / 100.0;
    let dist = p.f32("distance");
    let color = p.b("color_shift");
    let seed = p.u("seed");
    let (w, h) = (img.width(), img.height());
    let mut out = img.clone();
    let mut y = 0u32;
    let mut k = 0u32;
    while y < h {
        // Bands of varying height around the asked one.
        let bh = (band * (0.4 + 1.2 * hash01(k, 0, seed.wrapping_add(5)))).max(1.0) as u32;
        let y1 = (y + bh).min(h);
        if hash01(k, 1, seed) < amount {
            let s = (hash01(k, 2, seed) * 2.0 - 1.0) * dist;
            let split = if color && hash01(k, 3, seed) < 0.5 {
                (hash01(k, 4, seed) * 0.25 * dist).max(1.0)
            } else {
                0.0
            };
            for yy in y..y1 {
                for x in 0..w {
                    let fx = x as f32 - s;
                    let g = sample_wrap(img, fx, yy);
                    let r = sample_wrap(img, fx - split, yy);
                    let b = sample_wrap(img, fx + split, yy);
                    out.put_pixel(x, yy, Rgba([r[0], g[1], b[2], g[3].max(r[3]).max(b[3])]));
                }
            }
        }
        y = y1;
        k += 1;
    }
    out
}

/// The pixel at column `x` (wrapping round the row) of row `y`.
fn sample_wrap(img: &RgbaImage, x: f32, y: u32) -> Rgba<u8> {
    let w = img.width() as i64;
    let xi = (x.round() as i64).rem_euclid(w.max(1)) as u32;
    *img.get_pixel(xi, y)
}

/// The colours of a gradient map preset, dark to light.
fn preset_colors(p: &P) -> [[f32; 3]; 3] {
    let c = |v: u32| {
        let [r, g, b] = rgb(v);
        [r as f32, g as f32, b as f32]
    };
    match p.i("gradient_preset") {
        1 => [c(0x1A1033), c(0xC0306A), c(0xFFE08A)],
        2 => [c(0x04132B), c(0x1679A8), c(0xBFF4F0)],
        3 => [c(0x12003A), c(0xFF1F9E), c(0x3DF5FF)],
        4 => [c(0x140000), c(0xE0400A), c(0xFFF2A8)],
        5 => [c(0x0B0B3B), c(0xE81E62), c(0xF7F7D6)],
        _ => [
            c(p.u("shadow_color")),
            c(p.u("midtone_color")),
            c(p.u("highlight_color")),
        ],
    }
}

/// Each pixel's brightness looked up on a three-colour gradient (the
/// middle colour at `midpoint`).
pub fn gradient_map(img: &RgbaImage, p: &P) -> RgbaImage {
    let cols = preset_colors(p);
    let mid = p.f32("midpoint") / 100.0;
    let invert = p.b("invert");
    let opacity = p.f32("opacity") / 100.0;
    let mut lut = [[0u8; 3]; 256];
    for (i, e) in lut.iter_mut().enumerate() {
        let mut t = i as f32 / 255.0;
        if invert {
            t = 1.0 - t;
        }
        let (a, b, u) = if t <= mid {
            (cols[0], cols[1], t / mid.max(1e-3))
        } else {
            (cols[1], cols[2], (t - mid) / (1.0 - mid).max(1e-3))
        };
        for c in 0..3 {
            e[c] = clamp8(a[c] + (b[c] - a[c]) * u);
        }
    }
    map_px(img, |_, _, q| {
        let m = lut[clamp8(luma(q)) as usize];
        let mut o = *q;
        for c in 0..3 {
            o[c] = clamp8(q[c] as f32 * (1.0 - opacity) + m[c] as f32 * opacity);
        }
        o
    })
}

/// The Bayer threshold matrix of side 2, 4 or 8, values 0..1.
fn bayer(n: u32, x: u32, y: u32) -> f32 {
    let mut v = 0u32;
    let mut size = n;
    let (mut xx, mut yy) = (x % n, y % n);
    let mut weight = 1;
    while size > 1 {
        size /= 2;
        let (qx, qy) = (xx / size, yy / size);
        let q = match (qx, qy) {
            (0, 0) => 0,
            (1, 1) => 1,
            (1, 0) => 2,
            _ => 3,
        };
        v += q * weight;
        weight *= 4;
        xx %= size;
        yy %= size;
    }
    // Reverse the digit order so the coarse quadrant weighs most.
    let digits = (n * n).trailing_zeros() / 2;
    let mut r = 0u32;
    let mut t = v;
    for _ in 0..digits {
        r = r * 4 + t % 4;
        t /= 4;
    }
    (r as f32 + 0.5) / (n * n) as f32
}

/// A threshold 0..1 for a pixel of the blue-noise-like pattern
/// (interleaved gradient noise: neighbours never share a value).
fn noise_threshold(x: u32, y: u32) -> f32 {
    let v = 52.982_918 * (0.067_110_56 * x as f32 + 0.005_837_15 * y as f32).fract();
    v.fract()
}

/// The picture reduced to `levels` steps per channel (or to two colours,
/// by brightness), the steps mixed by an ordered pattern or by spreading
/// each pixel's error to its neighbours.
pub fn dither(img: &RgbaImage, p: &P) -> RgbaImage {
    let method = p.i("dither_method");
    let levels = p.u("levels").clamp(2, 16) as f32;
    let two = p.b("two_colors");
    let dark = rgb(p.u("dark_color")).map(f32::from);
    let light = rgb(p.u("light_color")).map(f32::from);
    let (w, h) = (img.width() as usize, img.height() as usize);
    let chans = if two { 1 } else { 3 };
    let mut v: Vec<f32> = Vec::with_capacity(w * h * chans);
    for q in img.pixels() {
        if two {
            v.push(luma(q) / 255.0);
        } else {
            v.extend([q[0], q[1], q[2]].map(|c| c as f32 / 255.0));
        }
    }
    let steps = levels - 1.0;
    let quant = |x: f32| (x * steps).round().clamp(0.0, steps) / steps;
    match method {
        0..=2 | 5 => {
            let n = [2, 4, 8][method.min(2)];
            for y in 0..h {
                for x in 0..w {
                    let t = if method == 5 {
                        noise_threshold(x as u32, y as u32)
                    } else {
                        bayer(n, x as u32, y as u32)
                    } - 0.5;
                    for c in 0..chans {
                        let i = (y * w + x) * chans + c;
                        v[i] = quant(v[i] + t / steps);
                    }
                }
            }
        }
        _ => {
            // Floyd-Steinberg (7 3 5 1 sixteenths) or Atkinson (an eighth
            // to six neighbours, a quarter of the error dropped).
            let spread: &[(i64, i64, f32)] = if method == 3 {
                &[
                    (1, 0, 7.0 / 16.0),
                    (-1, 1, 3.0 / 16.0),
                    (0, 1, 5.0 / 16.0),
                    (1, 1, 1.0 / 16.0),
                ]
            } else {
                &[
                    (1, 0, 0.125),
                    (2, 0, 0.125),
                    (-1, 1, 0.125),
                    (0, 1, 0.125),
                    (1, 1, 0.125),
                    (0, 2, 0.125),
                ]
            };
            for y in 0..h {
                for x in 0..w {
                    for c in 0..chans {
                        let i = (y * w + x) * chans + c;
                        let old = v[i];
                        let new = quant(old);
                        v[i] = new;
                        let err = old - new;
                        for (dx, dy, f) in spread {
                            let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                            if nx >= 0 && (nx as usize) < w && (ny as usize) < h {
                                let j = (ny as usize * w + nx as usize) * chans + c;
                                v[j] += err * f;
                            }
                        }
                    }
                }
            }
        }
    }
    let mut out = img.clone();
    for (k, q) in out.pixels_mut().enumerate() {
        if two {
            let t = v[k].clamp(0.0, 1.0);
            for c in 0..3 {
                q[c] = clamp8(dark[c] + (light[c] - dark[c]) * t);
            }
        } else {
            for c in 0..3 {
                q[c] = clamp8(v[k * 3 + c] * 255.0);
            }
        }
    }
    out
}

/// Rectangular to polar: the picture's top edge gathered to the middle and
/// its left and right edges joined into a ring. Polar to rectangular
/// unrolls a ring back into a rectangle.
pub fn polar_coordinates(img: &RgbaImage, p: &P) -> RgbaImage {
    let to_polar = p.i("polar_mode") == 0;
    let (w, h) = (img.width() as f32, img.height() as f32);
    let (cx, cy) = (w / 2.0, h / 2.0);
    let rmax = cx.min(cy).max(1.0);
    remap(img, true, |x, y| {
        if to_polar {
            let (dx, dy) = (x - cx, y - cy);
            let r = (dx * dx + dy * dy).sqrt() / rmax;
            if r > 1.0 {
                return (-10.0, -10.0);
            }
            let a = (dx.atan2(-dy) + PI) / TAU;
            (a * w, r * h)
        } else {
            let a = x / w * TAU - PI;
            let r = y / h * rmax;
            (cx + r * a.sin(), cy - r * a.cos())
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A left-to-right grey ramp over a red-blue stripe.
    fn ramp() -> RgbaImage {
        RgbaImage::from_fn(64, 48, |x, y| {
            let g = (x * 4) as u8;
            if y < 8 {
                Rgba([255, 0, 0, 255])
            } else {
                Rgba([g, g, g, 255])
            }
        })
    }

    fn run(src: &RgbaImage, id: &str, set: &[(&str, f64)]) -> RgbaImage {
        let mut e = new_effect(id);
        for (k, v) in set {
            e.params.insert(k.to_string(), *v);
        }
        apply_effect(src, &e)
    }

    #[test]
    fn rgb_split_moves_red_and_blue_apart() {
        let src = ramp();
        let out = run(&src, "rgb_split", &[("distance", 4.0)]);
        // Red comes from 4 px to the left, blue from 4 px to the right.
        let (x, y) = (30, 20);
        assert_eq!(out.get_pixel(x, y)[0], src.get_pixel(x - 4, y)[0]);
        assert_eq!(out.get_pixel(x, y)[2], src.get_pixel(x + 4, y)[2]);
        assert_eq!(out.get_pixel(x, y)[1], src.get_pixel(x, y)[1]);
        assert_eq!(run(&src, "rgb_split", &[("distance", 0.0)]), src);
    }

    #[test]
    fn scanlines_darken_every_other_line() {
        let src = RgbaImage::from_pixel(16, 16, Rgba([200, 200, 200, 255]));
        let out = run(
            &src,
            "scanlines",
            &[("spacing", 4.0), ("width", 2.0), ("opacity", 50.0)],
        );
        assert_eq!(out.get_pixel(5, 0)[0], 100);
        assert_eq!(out.get_pixel(5, 2)[0], 200);
        assert_eq!(out.get_pixel(5, 4)[0], 100);
    }

    #[test]
    fn pixel_sort_orders_runs_inside_the_thresholds() {
        let src = RgbaImage::from_fn(8, 1, |x, _| {
            let v = [200u8, 100, 150, 120, 10, 250, 90, 130][x as usize];
            Rgba([v, v, v, 255])
        });
        let out = run(
            &src,
            "pixel_sort",
            &[("threshold_low", 30.0), ("threshold_high", 90.0)],
        );
        let row: Vec<u8> = out.pixels().map(|p| p[0]).collect();
        // 10 and 250 are outside and stay; the runs on each side sort.
        assert_eq!(row, vec![100, 120, 150, 200, 10, 250, 90, 130]);
    }

    #[test]
    fn blocks_and_bands_change_pixels_and_follow_the_seed() {
        let src = ramp();
        for id in ["compression_blocks", "band_shift"] {
            let a = run(&src, id, &[("amount", 60.0), ("seed", 3.0)]);
            let b = run(&src, id, &[("amount", 60.0), ("seed", 3.0)]);
            let c = run(&src, id, &[("amount", 60.0), ("seed", 4.0)]);
            assert_eq!(a, b, "{id}");
            assert_ne!(a, src, "{id}");
            assert_ne!(a, c, "{id}");
        }
        // Without damage the bands stay where they are.
        assert_eq!(run(&src, "band_shift", &[("amount", 0.0)]), src);
    }

    #[test]
    fn gradient_map_follows_brightness() {
        let src = ramp();
        let out = run(
            &src,
            "gradient_map",
            &[
                ("gradient_preset", 0.0),
                ("shadow_color", 0x000080 as f64),
                ("midtone_color", 0x808080 as f64),
                ("highlight_color", 0xFFFF00 as f64),
            ],
        );
        let dark = out.get_pixel(0, 30);
        let light = out.get_pixel(63, 30);
        assert_eq!((dark[0], dark[2]), (0, 128));
        assert!(light[0] > 240 && light[2] < 20, "{light:?}");
    }

    #[test]
    fn dithering_keeps_only_the_levels_and_the_tone() {
        let src = ramp();
        for m in 0..6 {
            let out = run(
                &src,
                "dither",
                &[
                    ("dither_method", m as f64),
                    ("dark_color", 0.0),
                    ("light_color", 0xFFFFFF as f64),
                ],
            );
            // Two colours only.
            assert!(out.pixels().all(|p| p[0] == 0 || p[0] == 255), "{m}");
            // The share of white follows the ramp's mean brightness.
            let rows = 8..48;
            let white = rows
                .clone()
                .flat_map(|y| (0..64).map(move |x| (x, y)))
                .filter(|(x, y)| out.get_pixel(*x, *y)[0] == 255)
                .count() as f32
                / (40.0 * 64.0);
            assert!((white - 0.49).abs() < 0.08, "{m} {white}");
        }
        // Several levels per channel in colour.
        let out = run(&src, "dither", &[("two_colors", 0.0), ("levels", 4.0)]);
        assert!(out.pixels().all(|p| [0, 85, 170, 255].contains(&p[0])));
    }

    #[test]
    fn bayer_matrices_hold_each_threshold_once() {
        for n in [2, 4, 8] {
            let mut seen: Vec<f32> = (0..n)
                .flat_map(|y| (0..n).map(move |x| bayer(n, x, y)))
                .collect();
            seen.sort_by(f32::total_cmp);
            seen.dedup();
            assert_eq!(seen.len() as u32, n * n);
        }
    }

    #[test]
    fn polar_and_back_keeps_the_middle() {
        let src = ramp();
        let polar = run(&src, "polar_coordinates", &[]);
        // Outside the ring is clear.
        assert_eq!(polar.get_pixel(0, 0)[3], 0);
        let back = run(&polar, "polar_coordinates", &[("polar_mode", 1.0)]);
        // The bottom half of the unrolled picture is the original's grey
        // ramp far from the middle, opaque.
        assert_eq!(back.get_pixel(32, 40)[3], 255);
    }
}
