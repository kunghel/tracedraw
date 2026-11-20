//! Paper, cast and print looks: Embossed Paper, Gypsum Cast, Film
//! Clumping, Cracked Surface and Quilt Squares (Texture), Copier Toner,
//! Rubber Print and Wet Fiber Paper (Art Strokes) and Broadcast Safe
//! Colors (Transform). All keep alpha.

use super::paint::{blur_field, chans, gray, px, relief};
use super::util::*;
use super::*;
use image::{Rgba, RgbaImage};

/// Embossed Paper, listed in Texture.
pub const EMBOSSED_PAPER: EffectSpec = EffectSpec {
    id: "embossed_paper",
    params: &[
        range("threshold", 0.0, 100.0, 50.0, 1.0),
        range("grain", 0.0, 20.0, 10.0, 1.0),
        range("relief", 0.0, 25.0, 11.0, 1.0),
        color("paper_color", 0xEDEAE0),
    ],
    apply: embossed_paper,
    reach: no_reach,
};

/// Gypsum Cast, listed in Texture.
pub const GYPSUM_CAST: EffectSpec = EffectSpec {
    id: "gypsum_cast",
    params: &[
        range("threshold", 0.0, 100.0, 40.0, 1.0),
        range("smoothness", 1.0, 15.0, 2.0, 1.0),
        angle("angle", 135.0),
        color("paper_color", 0xF2F0EA),
    ],
    apply: gypsum_cast,
    reach: no_reach,
};

/// Film Clumping, listed in Texture.
pub const FILM_CLUMPING: EffectSpec = EffectSpec {
    id: "film_clumping",
    params: &[
        range("density", 0.0, 50.0, 12.0, 1.0),
        range("black_point", 0.0, 50.0, 40.0, 1.0),
        range("white_point", 0.0, 50.0, 5.0, 1.0),
        range("seed", 0.0, 9999.0, 1.0, 1.0),
    ],
    apply: film_clumping,
    reach: no_reach,
};

/// Cracked Surface, listed in Texture.
pub const CRACKED_SURFACE: EffectSpec = EffectSpec {
    id: "cracked_surface",
    params: &[
        range("spacing", 2.0, 100.0, 15.0, 1.0),
        range("depth", 0.0, 10.0, 6.0, 1.0),
        range("brightness", 0.0, 10.0, 9.0, 1.0),
        range("seed", 0.0, 9999.0, 1.0, 1.0),
    ],
    apply: cracked_surface,
    reach: no_reach,
};

/// Quilt Squares, listed in Texture.
pub const QUILT_SQUARES: EffectSpec = EffectSpec {
    id: "quilt_squares",
    params: &[
        range("size", 2.0, 20.0, 8.0, 1.0),
        range("relief", 0.0, 25.0, 8.0, 1.0),
        range("seed", 0.0, 9999.0, 1.0, 1.0),
    ],
    apply: quilt_squares,
    reach: no_reach,
};

/// Copier Toner, listed in Art Strokes.
pub const COPIER_TONER: EffectSpec = EffectSpec {
    id: "copier_toner",
    params: &[
        range("detail", 1.0, 24.0, 7.0, 1.0),
        range("intensity", 1.0, 50.0, 8.0, 1.0),
    ],
    apply: copier_toner,
    reach: no_reach,
};

/// Rubber Print, listed in Art Strokes.
pub const RUBBER_PRINT: EffectSpec = EffectSpec {
    id: "rubber_print",
    params: &[
        range("threshold", 0.0, 100.0, 50.0, 1.0),
        range("smoothness", 1.0, 50.0, 5.0, 1.0),
        color("dark_color", 0x000000),
        color("paper_color", 0xFFFFFF),
        range("seed", 0.0, 9999.0, 1.0, 1.0),
    ],
    apply: rubber_print,
    reach: no_reach,
};

/// Wet Fiber Paper, listed in Art Strokes.
pub const WET_FIBER_PAPER: EffectSpec = EffectSpec {
    id: "wet_fiber_paper",
    params: &[
        range("length", 3.0, 50.0, 8.0, 1.0),
        range("brightness", 0.0, 100.0, 60.0, 1.0),
        range("contrast", 0.0, 100.0, 80.0, 1.0),
        range("seed", 0.0, 9999.0, 1.0, 1.0),
    ],
    apply: wet_fiber_paper,
    reach: no_reach,
};

/// Broadcast Safe Colors, listed in Transform.
pub const BROADCAST_SAFE: EffectSpec = EffectSpec {
    id: "broadcast_safe",
    params: &[choice("tv_standard", &["tv_ntsc", "tv_pal"], 0)],
    apply: broadcast_safe,
    reach: no_reach,
};

/// Paper in two tones, the dark parts pressed up from the sheet.
fn embossed_paper(img: &RgbaImage, p: &P) -> RgbaImage {
    let (w, h) = img.dimensions();
    let t = p.f32("threshold") / 100.0;
    let g = blur_field(&gray(img), w, h, 1.0);
    let grain = p.f32("grain") / 20.0;
    let mask: Vec<f32> = g.iter().map(|v| if *v < t { 1.0 } else { 0.0 }).collect();
    let soft = blur_field(&mask, w, h, 1.2);
    let heights: Vec<f32> = soft
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let (x, y) = ((i as u32 % w) as f32, (i as u32 / w) as f32);
            m * 0.8 + fractal(x, y, 3.0, 5) * grain * 0.3
        })
        .collect();
    let paper = p.rgb("paper_color").map(|v| v as f32);
    let flat = map_px(img, |x, y, q| {
        let m = soft[(y * w + x) as usize];
        px(q, paper.map(|v| v * (1.0 - m * 0.18)))
    });
    relief(&flat, &heights, 135.0, p.f32("relief") / 25.0)
}

/// The picture cast in plaster: dark parts raised and lit from Angle.
fn gypsum_cast(img: &RgbaImage, p: &P) -> RgbaImage {
    let (w, h) = img.dimensions();
    let t = p.f32("threshold") / 100.0;
    let g = gray(img);
    let mask: Vec<f32> = g
        .iter()
        .map(|v| ((t - v) * 8.0 + 0.5).clamp(0.0, 1.0))
        .collect();
    let heights = blur_field(&mask, w, h, 1.0 + p.f32("smoothness") * 1.2);
    let paper = p.rgb("paper_color").map(|v| v as f32);
    let flat = map_px(img, |x, y, q| {
        let m = heights[(y * w + x) as usize];
        px(q, paper.map(|v| v * (0.92 - m * 0.12)))
    });
    relief(&flat, &heights, p.f32("angle"), 0.6)
}

/// Grain that clumps in the shadows and thins out in the highlights.
fn film_clumping(img: &RgbaImage, p: &P) -> RgbaImage {
    let seed = p.u("seed");
    let dens = p.f32("density") / 50.0;
    let lo = p.f32("black_point") / 50.0 * 0.35;
    let hi = 1.0 - p.f32("white_point") / 50.0 * 0.35;
    map_px(img, |x, y, q| {
        let l = luma(q) / 255.0;
        let n =
            value_noise(x as f32, y as f32, 1.8, seed) * 0.6 + hash01(x, y, seed ^ 0xBEEF) * 0.4;
        let clump = ((n - 0.5) * (1.0 + dens * 6.0) + 0.5).clamp(0.0, 1.0);
        let v = if l > clump * 0.8 + 0.1 { 1.0 } else { 0.0 } * 0.6 + l * 0.4;
        let v = (lo + v * (hi - lo)) * 255.0;
        px(q, [v, v, v])
    })
}

/// Distance to the nearest and second nearest jittered cell centre.
fn cells(x: f32, y: f32, s: f32, seed: u32) -> (f32, f32) {
    let (cx, cy) = ((x / s).floor() as i64, (y / s).floor() as i64);
    let (mut d1, mut d2) = (f32::MAX, f32::MAX);
    for j in cy - 1..=cy + 1 {
        for i in cx - 1..=cx + 1 {
            let px = (i as f32 + hash01(i as u32, j as u32, seed)) * s;
            let py = (j as f32 + hash01(i as u32, j as u32, seed ^ 0x77)) * s;
            let d = ((x - px).powi(2) + (y - py).powi(2)).sqrt();
            if d < d1 {
                d2 = d1;
                d1 = d;
            } else if d < d2 {
                d2 = d;
            }
        }
    }
    (d1, d2)
}

/// Fine cracks over a raised surface, as in old varnish.
fn cracked_surface(img: &RgbaImage, p: &P) -> RgbaImage {
    let (w, h) = img.dimensions();
    let s = p.f32("spacing");
    let seed = p.u("seed");
    let depth = p.f32("depth") / 10.0;
    let heights: Vec<f32> = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .map(|(x, y)| {
            let (d1, d2) = cells(x as f32, y as f32, s, seed);
            ((d2 - d1) / 1.5).min(1.0)
        })
        .collect();
    let bright = p.f32("brightness") / 10.0;
    let lit = map_px(img, |x, y, q| {
        let crack = 1.0 - heights[(y * w + x) as usize];
        let k = 0.85 + bright * 0.25;
        px(q, chans(q).map(|v| v * k * (1.0 - crack * depth * 0.7)))
    });
    relief(&lit, &heights, 135.0, depth * 0.5)
}

/// Raised squares, each one colour, sewn together with dark seams.
fn quilt_squares(img: &RgbaImage, p: &P) -> RgbaImage {
    let s = p.u("size").max(2);
    let seed = p.u("seed");
    let (w, h) = img.dimensions();
    let heights: Vec<f32> = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .map(|(x, y)| {
            let (fx, fy) = ((x % s) as f32 + 0.5, (y % s) as f32 + 0.5);
            let edge = fx.min(fy).min(s as f32 - fx).min(s as f32 - fy) / (s as f32 * 0.5);
            let lift = 0.5 + hash01(x / s, y / s, seed) * 0.5;
            (edge * 3.0).min(1.0) * lift
        })
        .collect();
    let flat = map_px(img, |x, y, q| {
        let c = sample(
            img,
            ((x / s) * s + s / 2) as f32,
            ((y / s) * s + s / 2) as f32,
        );
        px(q, chans(&c))
    });
    relief(&flat, &heights, 135.0, p.f32("relief") / 25.0)
}

/// Dark toner where the picture has detail, white elsewhere.
fn copier_toner(img: &RgbaImage, p: &P) -> RgbaImage {
    let (w, h) = img.dimensions();
    let g = gray(img);
    let b = blur_field(&g, w, h, p.f32("detail") / 2.0);
    let k = p.f32("intensity") / 2.0;
    map_px(img, |x, y, q| {
        let i = (y * w + x) as usize;
        let d = b[i] - g[i];
        let toner = (d * k * 3.0 + (0.18 - g[i]).max(0.0) * 4.0).clamp(0.0, 1.0);
        let v = 255.0 * (1.0 - toner);
        px(q, [v, v, v])
    })
}

/// Two flat colours, as printed with an inked rubber block.
fn rubber_print(img: &RgbaImage, p: &P) -> RgbaImage {
    let (w, h) = img.dimensions();
    let g = blur_field(&gray(img), w, h, p.f32("smoothness") / 5.0);
    let t = p.f32("threshold") / 100.0;
    let ink = p.rgb("dark_color").map(|v| v as f32);
    let paper = p.rgb("paper_color").map(|v| v as f32);
    let seed = p.u("seed");
    map_px(img, |x, y, q| {
        let v = g[(y * w + x) as usize];
        let mut a = ((t - v) * 12.0 + 0.5).clamp(0.0, 1.0);
        // Specks where the block took no ink.
        if hash01(x, y, seed) < 0.04 + 0.1 * value_noise(x as f32, y as f32, 6.0, seed ^ 3) {
            a *= 0.3;
        }
        px(q, [0, 1, 2].map(|c| paper[c] + (ink[c] - paper[c]) * a))
    })
}

/// Colours running along the fibres of wet paper.
fn wet_fiber_paper(img: &RgbaImage, p: &P) -> RgbaImage {
    let len = p.f32("length");
    let seed = p.u("seed");
    let soft = gaussian(img, 1.2);
    let b = (p.f32("brightness") - 50.0) / 50.0 * 60.0;
    let k = p.f32("contrast") / 80.0;
    map_px(img, |x, y, q| {
        // Each spot runs along its own fibre direction.
        let a = value_noise(x as f32, y as f32, 24.0, seed) * std::f32::consts::TAU * 2.0;
        let (dx, dy) = (a.cos(), a.sin());
        let mut acc = [0f32; 4];
        let mut n = 0.0;
        let mut t = 0.0;
        while t <= len {
            add_premultiplied(
                &mut acc,
                &sample(&soft, x as f32 + dx * t, y as f32 + dy * t),
            );
            n += 1.0;
            t += 1.5;
        }
        let s = unpremultiply(acc, n);
        let fibre = value_noise(x as f32 * 0.2, y as f32 * 3.0, 2.0, seed ^ 9) * 30.0;
        px(
            q,
            chans(&s).map(|v| (v - 128.0) * k + 128.0 + b + fibre - 15.0),
        )
    })
}

/// Colours kept inside what a broadcast signal carries: luma between
/// 16 and 235 and chroma limited so luma plus chroma stays in range.
fn broadcast_safe(img: &RgbaImage, p: &P) -> RgbaImage {
    let pal = p.i("tv_standard") == 1;
    map_px(img, |_, _, q| {
        let [r, g, b] = chans(q).map(|v| v / 255.0);
        let y = 0.299 * r + 0.587 * g + 0.114 * b;
        let (u, v) = if pal {
            (0.492 * (b - y), 0.877 * (r - y))
        } else {
            (
                0.596 * r - 0.274 * g - 0.322 * b,
                0.211 * r - 0.523 * g + 0.312 * b,
            )
        };
        let y2 = y.clamp(16.0 / 255.0, 235.0 / 255.0);
        let c = (u * u + v * v).sqrt();
        // The signal (luma plus or minus chroma) stays within 7.5..100 IRE.
        let room = (1.0 - y2).min(y2 - 0.075).max(0.0) * 0.9;
        let s = if c > room && c > 0.0 { room / c } else { 1.0 };
        let (u, v) = (u * s, v * s);
        let rgb = if pal {
            [y2 + v / 0.877, y2 - 0.395 * u - 0.581 * v, y2 + u / 0.492]
        } else {
            [
                y2 + 0.956 * u + 0.621 * v,
                y2 - 0.272 * u - 0.647 * v,
                y2 - 1.106 * u + 1.703 * v,
            ]
        };
        px(q, rgb.map(|c| c * 255.0))
    })
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
            let inside = (x as i32 - 20).pow(2) + (y as i32 - 16).pow(2) < 100;
            Rgba([
                (x * 6) as u8,
                (y * 7) as u8,
                if inside { 240 } else { 40 },
                if x == 0 { 0 } else { 255 },
            ])
        })
    }

    #[test]
    fn every_paper_look_changes_the_picture_and_keeps_alpha() {
        let src = photo();
        for id in [
            "embossed_paper",
            "gypsum_cast",
            "film_clumping",
            "cracked_surface",
            "quilt_squares",
            "copier_toner",
            "rubber_print",
            "wet_fiber_paper",
            "broadcast_safe",
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
    fn broadcast_safe_tames_hot_colours_and_keeps_greys() {
        for std in [0.0, 1.0] {
            let hot = RgbaImage::from_pixel(4, 4, Rgba([255, 0, 0, 255]));
            let out = run(&hot, "broadcast_safe", &[("tv_standard", std)]);
            let p = out.get_pixel(1, 1);
            assert!(p[0] < 250 && p[1] > 0, "{std} {p:?}");
            let grey = RgbaImage::from_pixel(4, 4, Rgba([128, 128, 128, 255]));
            let g = run(&grey, "broadcast_safe", &[("tv_standard", std)]);
            let q = g.get_pixel(1, 1);
            assert!((0..3).all(|c| q[c].abs_diff(128) <= 1), "{q:?}");
            // White and black fall inside the legal range.
            let white = RgbaImage::from_pixel(4, 4, Rgba([255, 255, 255, 255]));
            assert!(
                run(&white, "broadcast_safe", &[("tv_standard", std)]).get_pixel(0, 0)[0] <= 236
            );
        }
    }

    #[test]
    fn rubber_print_uses_its_two_colours() {
        let src = photo();
        let out = run(&src, "rubber_print", &[("dark_color", 0xFF0000 as f64)]);
        // Mostly the ink or the paper.
        let pure = out
            .pixels()
            .filter(|p| p.0[..3] == [255, 0, 0] || p.0[..3] == [255, 255, 255])
            .count();
        assert!(pure * 10 > out.pixels().count() * 6, "{pure}");
    }

    #[test]
    fn copier_keeps_flat_light_areas_white_and_inks_edges() {
        let src = RgbaImage::from_fn(40, 40, |x, _| {
            if x < 20 {
                Rgba([230, 230, 230, 255])
            } else {
                Rgba([90, 90, 90, 255])
            }
        });
        let out = run(&src, "copier_toner", &[]);
        assert!(out.get_pixel(5, 20)[0] > 240);
        assert!(out.get_pixel(20, 20)[0] <= out.get_pixel(35, 20)[0]);
    }

    #[test]
    fn cracks_and_squares_follow_their_size() {
        let grey = RgbaImage::from_pixel(48, 48, Rgba([150, 150, 150, 255]));
        let a = run(&grey, "cracked_surface", &[("seed", 1.0)]);
        assert_ne!(a, run(&grey, "cracked_surface", &[("seed", 2.0)]));
        // Quilt squares: every square has one colour before its relief,
        // so with no relief a striped picture turns into blocks.
        let stripes = RgbaImage::from_fn(32, 32, |x, _| Rgba([(x * 8) as u8, 0, 0, 255]));
        let q = run(&stripes, "quilt_squares", &[("size", 8.0), ("relief", 0.0)]);
        assert_eq!(q.get_pixel(0, 0), q.get_pixel(7, 7));
        assert_ne!(q.get_pixel(7, 0), q.get_pixel(8, 0));
    }
}
