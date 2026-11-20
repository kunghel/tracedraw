//! Stroke looks: Chalked Edges, Diagonal Hatching, Pencil Weave, Light
//! and Shadow Strokes, Spray Streaks and Ink Wash (Art Strokes), Grit
//! Screen (Color Transform) and Neon Outline (Contour). All keep alpha.

use super::paint::{blur_field, chans, dilate_field, gray, px, smear, sobel};
use super::util::*;
use super::*;
use image::{Rgba, RgbaImage};

/// Chalked Edges, listed in Art Strokes.
pub const CHALKED_EDGES: EffectSpec = EffectSpec {
    id: "chalked_edges",
    params: &[
        range("width", 1.0, 14.0, 2.0, 1.0),
        range("brightness", 0.0, 50.0, 38.0, 1.0),
        range("smoothness", 1.0, 15.0, 5.0, 1.0),
    ],
    apply: chalked_edges,
    reach: no_reach,
};

/// Diagonal Hatching, listed in Art Strokes.
pub const DIAGONAL_HATCHING: EffectSpec = EffectSpec {
    id: "diagonal_hatching",
    params: &[
        range("midpoint", 0.0, 100.0, 50.0, 1.0),
        range("length", 3.0, 50.0, 15.0, 1.0),
        range("contrast", 0.0, 10.0, 3.0, 1.0),
    ],
    apply: diagonal_hatching,
    reach: no_reach,
};

/// Pencil Weave, listed in Art Strokes.
pub const PENCIL_WEAVE: EffectSpec = EffectSpec {
    id: "pencil_weave",
    params: &[
        range("length", 3.0, 50.0, 9.0, 1.0),
        range("contrast", 0.0, 20.0, 6.0, 1.0),
        range("intensity", 1.0, 3.0, 1.0, 1.0),
        range("seed", 0.0, 9999.0, 1.0, 1.0),
    ],
    apply: pencil_weave,
    reach: no_reach,
};

/// Light and Shadow Strokes, listed in Art Strokes.
pub const LIGHT_SHADOW_STROKES: EffectSpec = EffectSpec {
    id: "light_shadow_strokes",
    params: &[
        range("midpoint", 0.0, 10.0, 5.0, 1.0),
        range("highlights", 0.0, 10.0, 2.0, 1.0),
        range("shadows", 0.0, 10.0, 6.0, 1.0),
    ],
    apply: light_shadow_strokes,
    reach: no_reach,
};

/// Spray Streaks, listed in Art Strokes.
pub const SPRAY_STREAKS: EffectSpec = EffectSpec {
    id: "spray_streaks",
    params: &[
        range("length", 0.0, 20.0, 12.0, 1.0),
        range("radius", 0.0, 25.0, 7.0, 1.0),
        angle("angle", 45.0),
        range("seed", 0.0, 9999.0, 1.0, 1.0),
    ],
    apply: spray_streaks,
    reach: no_reach,
};

/// Ink Wash, listed in Art Strokes.
pub const INK_WASH: EffectSpec = EffectSpec {
    id: "ink_wash",
    params: &[
        range("brush_size", 3.0, 15.0, 10.0, 1.0),
        range("pressure", 0.0, 15.0, 2.0, 1.0),
        range("contrast", 0.0, 40.0, 16.0, 1.0),
        color("paper_color", 0xF4EFE2),
    ],
    apply: ink_wash,
    reach: no_reach,
};

/// Grit Screen, listed in Color Transform.
pub const GRIT_SCREEN: EffectSpec = EffectSpec {
    id: "grit_screen",
    params: &[
        choice(
            "screen_type",
            &[
                "grit_fine_dots",
                "grit_coarse_dots",
                "grit_grain",
                "grit_short_lines",
                "grit_long_lines",
                "grit_strokes",
            ],
            0,
        ),
        range("seed", 0.0, 9999.0, 1.0, 1.0),
    ],
    apply: grit_screen,
    reach: no_reach,
};

/// Neon Outline, listed in Contour.
pub const NEON_OUTLINE: EffectSpec = EffectSpec {
    id: "neon_outline",
    params: &[
        range("width", 1.0, 14.0, 2.0, 1.0),
        range("brightness", 0.0, 20.0, 6.0, 1.0),
        range("smoothness", 1.0, 15.0, 5.0, 1.0),
    ],
    apply: neon_outline,
    reach: no_reach,
};

/// The edges whitened like chalk (bright) or darkened like ink (dim).
fn chalked_edges(img: &RgbaImage, p: &P) -> RgbaImage {
    let (w, h) = img.dimensions();
    let soft = gaussian(img, p.f32("smoothness") / 4.0);
    let e = sobel(&gray(&soft), w, h);
    let e = blur_field(&dilate_field(&e, w, h, p.u("width") / 2), w, h, 0.8);
    let b = (p.f32("brightness") - 25.0) / 25.0;
    map_px(&soft, |x, y, q| {
        let k = (e[(y * w + x) as usize] * 5.0).min(1.0);
        let c = chans(q).map(|v| {
            if b >= 0.0 {
                v + (255.0 - v) * k * b
            } else {
                v * (1.0 - k * -b)
            }
        });
        px(img.get_pixel(x, y), c)
    })
}

/// Light areas stroked one way, dark areas the other, then sharpened.
fn diagonal_hatching(img: &RgbaImage, p: &P) -> RgbaImage {
    let len = p.u("length");
    let a = smear(img, 45.0, len);
    let b = smear(img, -45.0, len);
    let mid = p.f32("midpoint") / 100.0 * 255.0;
    let k = p.f32("contrast") / 10.0;
    let mixed = map_px(img, |x, y, q| {
        let t = ((luma(q) - mid) / 40.0 + 0.5).clamp(0.0, 1.0);
        let (pa, pb) = (a.get_pixel(x, y), b.get_pixel(x, y));
        let c = [0, 1, 2].map(|i| pa[i] as f32 * t + pb[i] as f32 * (1.0 - t));
        px(q, c)
    });
    let soft = gaussian(&mixed, 1.5);
    map_px(&mixed, |x, y, q| {
        let s = soft.get_pixel(x, y);
        let c = [0, 1, 2].map(|i| q[i] as f32 + (q[i] as f32 - s[i] as f32) * k * 2.0);
        px(q, c)
    })
}

/// Pencil hatching in two directions over the picture, denser where dark.
fn pencil_weave(img: &RgbaImage, p: &P) -> RgbaImage {
    let (w, h) = img.dimensions();
    let len = p.u("length");
    let seed = p.u("seed");
    let noise = RgbaImage::from_fn(w, h, |x, y| {
        let v = (hash01(x, y, seed) * 255.0) as u8;
        let u = (hash01(x, y, seed ^ 0x9E37) * 255.0) as u8;
        Rgba([v, u, 0, 255])
    });
    let ha = smear(&noise, 45.0, len);
    let hb = smear(&noise, -45.0, len);
    let k = p.f32("contrast") / 20.0;
    let inten = p.f32("intensity");
    map_px(img, |x, y, q| {
        let d = 1.0 - luma(q) / 255.0;
        // Smeared noise: centred, stretched so the strokes show.
        let sa = (ha.get_pixel(x, y)[0] as f32 / 255.0 - 0.5) * 6.0;
        let sb = (hb.get_pixel(x, y)[1] as f32 / 255.0 - 0.5) * 6.0;
        let mut dark = sa.max(0.0) * d;
        if d > 0.45 {
            dark += sb.max(0.0) * (d - 0.45) * 1.8;
        }
        let ink = (dark * 0.35 * inten).min(1.0);
        let c = chans(q).map(|v| {
            let v = (v - 128.0) * (1.0 + k) + 128.0;
            v * (1.0 - ink)
        });
        px(q, c)
    })
}

/// Long light strokes in the highlights, short dark ones in the shadows.
fn light_shadow_strokes(img: &RgbaImage, p: &P) -> RgbaImage {
    let long = smear(img, -45.0, 13);
    let short = smear(img, 45.0, 5);
    let mid = p.f32("midpoint") / 10.0 * 255.0;
    let hi = p.f32("highlights") / 10.0;
    let sh = p.f32("shadows") / 10.0;
    map_px(img, |x, y, q| {
        if luma(q) >= mid {
            let s = long.get_pixel(x, y);
            px(q, chans(s).map(|v| v + (255.0 - v) * hi * 0.8))
        } else {
            let s = short.get_pixel(x, y);
            px(q, chans(s).map(|v| v * (1.0 - sh * 0.8)))
        }
    })
}

/// Colours sprayed out by Radius and streaked along Angle.
fn spray_streaks(img: &RgbaImage, p: &P) -> RgbaImage {
    let r = p.f32("radius");
    let seed = p.u("seed");
    let sprayed = map_px(img, |x, y, q| {
        let dx = (hash01(x, y, seed) - 0.5) * 2.0 * r;
        let dy = (hash01(x, y, seed ^ 0x51ED) - 0.5) * 2.0 * r;
        let mut s = sample(img, x as f32 + dx, y as f32 + dy);
        s[3] = q[3];
        s
    });
    smear(&sprayed, p.f32("angle"), p.u("length"))
}

/// Black ink washed onto paper with a wet brush.
fn ink_wash(img: &RgbaImage, p: &P) -> RgbaImage {
    let (w, h) = img.dimensions();
    let g = blur_field(&gray(img), w, h, p.f32("brush_size") / 5.0);
    let k = 1.0 + p.f32("contrast") / 8.0;
    let ink: Vec<f32> = g
        .iter()
        .map(|v| (((1.0 - v) - 0.5) * k + 0.5).clamp(0.0, 1.0))
        .collect();
    // Pressure lets the ink bleed into the paper.
    let ink = dilate_field(&ink, w, h, p.u("pressure") / 4);
    let ink = blur_field(&ink, w, h, 0.6 + p.f32("pressure") / 6.0);
    let paper = p.rgb("paper_color").map(|v| v as f32);
    map_px(img, |x, y, q| {
        let fibre = value_noise(x as f32 * 0.3, y as f32 * 2.0, 3.0, 11) * 0.12;
        let i = (ink[(y * w + x) as usize] * (1.0 - fibre)).min(1.0);
        px(q, paper.map(|v| v * (1.0 - i * 0.94)))
    })
}

/// Every channel as pure ink or none, against a random screen of dots,
/// lines or strokes.
fn grit_screen(img: &RgbaImage, p: &P) -> RgbaImage {
    let seed = p.u("seed");
    let kind = p.i("screen_type");
    let screen = |x: u32, y: u32, c: u32| -> f32 {
        let s = seed.wrapping_add(c * 7919);
        let (xf, yf) = (x as f32, y as f32);
        match kind {
            0 => hash01(x, y, s),
            1 => hash01(x / 2, y / 2, s) * 0.6 + hash01(x, y, s ^ 1) * 0.4,
            2 => value_noise(xf, yf, 1.6, s) * 0.5 + hash01(x, y, s) * 0.5,
            3 => value_noise(xf * 0.25, yf, 1.0, s) * 0.7 + hash01(x, y, s) * 0.3,
            4 => value_noise(xf * 0.08, yf, 1.0, s) * 0.8 + hash01(x, y, s) * 0.2,
            _ => value_noise((xf + yf) * 0.15, xf - yf, 1.0, s) * 0.75 + hash01(x, y, s) * 0.25,
        }
    };
    map_px(img, |x, y, q| {
        let c = [0u32, 1, 2].map(|i| {
            // Noise of the smoother screens spreads over 0.2..0.8.
            let t = screen(x, y, i);
            let t = if kind >= 2 {
                ((t - 0.5) * 1.8 + 0.5).clamp(0.0, 1.0)
            } else {
                t
            };
            if q[i as usize] as f32 / 255.0 > t {
                255.0
            } else {
                0.0
            }
        });
        px(q, c)
    })
}

/// The colour edges glowing on black.
fn neon_outline(img: &RgbaImage, p: &P) -> RgbaImage {
    let (w, h) = img.dimensions();
    let soft = gaussian(img, p.f32("smoothness") / 4.0);
    let r = p.u("width") / 2;
    let k = 1.0 + p.f32("brightness") / 2.0;
    let edges: Vec<Vec<f32>> = (0..3)
        .map(|c| {
            let f: Vec<f32> = soft.pixels().map(|q| q[c] as f32 / 255.0).collect();
            dilate_field(&sobel(&f, w, h), w, h, r)
        })
        .collect();
    map_px(img, |x, y, q| {
        let i = (y * w + x) as usize;
        px(q, [0, 1, 2].map(|c| edges[c][i] * k * 255.0))
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
    fn every_stroke_look_changes_the_picture_and_keeps_alpha() {
        let src = photo();
        for id in [
            "chalked_edges",
            "diagonal_hatching",
            "pencil_weave",
            "light_shadow_strokes",
            "spray_streaks",
            "ink_wash",
            "grit_screen",
            "neon_outline",
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
    fn grit_screen_is_pure_ink_and_follows_the_tone() {
        let src = photo();
        for kind in 0..6 {
            let out = run(&src, "grit_screen", &[("screen_type", kind as f64)]);
            assert!(out
                .pixels()
                .all(|p| (0..3).all(|c| p[c] == 0 || p[c] == 255)));
        }
        let light = RgbaImage::from_pixel(32, 32, Rgba([220, 220, 220, 255]));
        let dark = RgbaImage::from_pixel(32, 32, Rgba([40, 40, 40, 255]));
        let on = |i: &RgbaImage| i.pixels().filter(|p| p[0] == 255).count();
        assert!(on(&run(&light, "grit_screen", &[])) > on(&run(&dark, "grit_screen", &[])) * 3);
    }

    #[test]
    fn neon_outline_lights_edges_on_black() {
        let src = photo();
        let out = run(&src, "neon_outline", &[]);
        // Inside the flat-blue disc it is dark; on its rim it glows.
        assert!(out.get_pixel(20, 16)[2] < 40);
        assert!(out.get_pixel(20, 6)[2] > 120, "{:?}", out.get_pixel(20, 6));
    }

    #[test]
    fn chalk_and_ink_edges_and_ink_wash() {
        let src = photo();
        let chalk = run(&src, "chalked_edges", &[("brightness", 50.0)]);
        let ink = run(&src, "chalked_edges", &[("brightness", 0.0)]);
        assert!(luma(chalk.get_pixel(20, 6)) > luma(ink.get_pixel(20, 6)) + 40.0);
        // Ink wash: white stays near the paper, black becomes ink.
        let white = RgbaImage::from_pixel(24, 24, Rgba([255, 255, 255, 255]));
        let black = RgbaImage::from_pixel(24, 24, Rgba([0, 0, 0, 255]));
        assert!(luma(run(&white, "ink_wash", &[]).get_pixel(12, 12)) > 200.0);
        assert!(luma(run(&black, "ink_wash", &[]).get_pixel(12, 12)) < 60.0);
    }

    #[test]
    fn strokes_darken_shadows_and_light_highlights() {
        let grad = RgbaImage::from_fn(40, 20, |x, _| {
            let v = (x * 6) as u8;
            Rgba([v, v, v, 255])
        });
        let out = run(&grad, "light_shadow_strokes", &[]);
        assert!(out.get_pixel(3, 10)[0] < grad.get_pixel(3, 10)[0] + 1);
        assert!(out.get_pixel(36, 10)[0] >= grad.get_pixel(36, 10)[0]);
        let a = run(&grad, "pencil_weave", &[("seed", 2.0)]);
        assert_eq!(a, run(&grad, "pencil_weave", &[("seed", 2.0)]));
    }
}
