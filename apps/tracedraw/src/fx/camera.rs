//! Effects > Camera: photographic filters and old photo looks.

use super::util::*;
use super::*;
use image::{Rgba, RgbaImage};

pub const EFFECTS: &[EffectSpec] = &[
    EffectSpec {
        id: "colorize",
        params: &[
            range("hue", 0.0, 360.0, 30.0, 1.0),
            range("saturation", 0.0, 100.0, 50.0, 1.0),
        ],
        apply: colorize,
        reach: no_reach,
    },
    EffectSpec {
        id: "diffuse",
        params: &[range("level", 1.0, 100.0, 20.0, 1.0)],
        apply: diffuse,
        reach: no_reach,
    },
    EffectSpec {
        id: "lens_correction",
        params: &[
            range("distortion", -100.0, 100.0, 0.0, 1.0),
            range("fringe", -100.0, 100.0, 0.0, 1.0),
            range("corner_light", -100.0, 100.0, 0.0, 1.0),
            range("scale", 50.0, 150.0, 100.0, 1.0),
        ],
        apply: lens_correction,
        reach: no_reach,
    },
    EffectSpec {
        id: "photo_filter",
        params: &[
            color("filter_color", 0xEC8A00),
            range("density", 1.0, 100.0, 25.0, 1.0),
            check("preserve_luminosity", true),
        ],
        apply: photo_filter,
        reach: no_reach,
    },
    EffectSpec {
        id: "tilt_shift",
        params: &[
            range("focus_position", 0.0, 100.0, 50.0, 1.0),
            range("focus_width", 0.0, 100.0, 20.0, 1.0),
            range("blur", 1.0, 50.0, 8.0, 1.0),
            range("angle", 0.0, 180.0, 0.0, 1.0),
        ],
        apply: tilt_shift,
        reach: no_reach,
    },
    EffectSpec {
        id: "sepia_toning",
        params: &[range("level", 0.0, 100.0, 50.0, 1.0)],
        apply: sepia,
        reach: no_reach,
    },
    EffectSpec {
        id: "vintage_photo",
        params: &[
            choice(
                "style",
                &[
                    "daguerreotype",
                    "albumen_print",
                    "sepia",
                    "black_white",
                    "cross_process",
                    "faded_print",
                    "warm_color",
                ],
                1,
            ),
            range("intensity", 0.0, 100.0, 70.0, 1.0),
        ],
        apply: vintage_photo,
        reach: no_reach,
    },
];

/// Lens faults put right: barrel (Distortion below 0 pulls the edges in,
/// above 0 pushes them out to undo a barrel), coloured fringes (red and
/// blue scaled apart), darker or lighter corners, and a scale to fill
/// the frame.
pub fn lens_correction(img: &RgbaImage, p: &P) -> RgbaImage {
    let (w, h) = img.dimensions();
    let k = p.f32("distortion") / 100.0 * 0.5;
    let fringe = p.f32("fringe") / 100.0 * 0.01;
    let light = p.f32("corner_light") / 100.0;
    let scale = (p.f32("scale") / 100.0).max(0.01);
    let (cx, cy) = (w as f32 / 2.0 - 0.5, h as f32 / 2.0 - 0.5);
    let half = ((w * w + h * h) as f32).sqrt().max(1.0) / 2.0;
    map_px(img, |x, y, _| {
        let (dx, dy) = (x as f32 - cx, y as f32 - cy);
        let r2 = (dx * dx + dy * dy) / (half * half);
        let f = (1.0 + k * r2) / scale;
        let at = |g: f32| bilinear(img, cx + dx * f * g, cy + dy * f * g, false);
        let (r, g, b) = (at(1.0 + fringe), at(1.0), at(1.0 - fringe));
        let m = (1.0 + light * r2).max(0.0);
        Rgba([
            clamp8(r[0] as f32 * m),
            clamp8(g[1] as f32 * m),
            clamp8(b[2] as f32 * m),
            g[3],
        ])
    })
}

/// A band in focus across the picture (its middle at Focus position, a
/// share of the way across, turned by Angle), the rest blurred more the
/// farther from it: the look of a miniature.
pub fn tilt_shift(img: &RgbaImage, p: &P) -> RgbaImage {
    let (w, h) = img.dimensions();
    let blurred = gaussian(img, p.f32("blur"));
    let (s, c) = p.f32("angle").to_radians().sin_cos();
    // Across the band: 0 at the picture's one side, 1 at the other.
    let proj = |u: f32, v: f32| -u * s + v * c;
    let corners = [
        proj(0.0, 0.0),
        proj(w as f32, 0.0),
        proj(0.0, h as f32),
        proj(w as f32, h as f32),
    ];
    let lo = corners.iter().copied().fold(f32::MAX, f32::min);
    let hi = corners.iter().copied().fold(f32::MIN, f32::max);
    let span = (hi - lo).max(1.0);
    let pos = p.f32("focus_position") / 100.0;
    let half = p.f32("focus_width") / 200.0;
    let fade = 0.15f32;
    map_px(img, |x, y, q| {
        let along = (proj(x as f32 + 0.5, y as f32 + 0.5) - lo) / span;
        let d = (along.clamp(0.0, 1.0) - pos).abs();
        let t = ((d - half) / fade).clamp(0.0, 1.0);
        let t = t * t * (3.0 - 2.0 * t);
        let b = blurred.get_pixel(x, y);
        Rgba(std::array::from_fn(|i| {
            clamp8(q[i] as f32 + (b[i] as f32 - q[i] as f32) * t)
        }))
    })
}

pub fn colorize(img: &RgbaImage, p: &P) -> RgbaImage {
    let h = p.f32("hue");
    let s = p.f32("saturation") / 100.0;
    map_px(img, |_, _, q| {
        let l = luma(q) / 255.0;
        let (r, g, b) = hsl_to_rgb(h, s, l);
        Rgba([
            clamp8(r * 255.0),
            clamp8(g * 255.0),
            clamp8(b * 255.0),
            q[3],
        ])
    })
}

/// Pixels swapped with near neighbours, then softened: a diffusion filter.
pub fn diffuse(img: &RgbaImage, p: &P) -> RgbaImage {
    let r = 1.0 + p.f32("level") / 100.0 * 4.0;
    let scattered = map_px(img, |x, y, _| {
        let dx = (hash01(x, y, 201) - 0.5) * 2.0 * r;
        let dy = (hash01(x, y, 202) - 0.5) * 2.0 * r;
        sample(img, x as f32 + dx, y as f32 + dy)
    });
    mix(&scattered, &gaussian(&scattered, r * 0.5), 0.5)
}

pub fn photo_filter(img: &RgbaImage, p: &P) -> RgbaImage {
    let f = p.rgb("filter_color").map(|v| v as f32 / 255.0);
    let d = p.f32("density") / 100.0;
    let keep = p.b("preserve_luminosity");
    map_px(img, |_, _, q| {
        let c = [
            q[0] as f32 / 255.0,
            q[1] as f32 / 255.0,
            q[2] as f32 / 255.0,
        ];
        let l0 = 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2];
        let mut o: [f32; 3] = std::array::from_fn(|k| c[k] * (1.0 - d) + c[k] * f[k] * d * 2.0);
        if keep {
            let l1 = 0.299 * o[0] + 0.587 * o[1] + 0.114 * o[2];
            if l1 > 1e-6 {
                for v in o.iter_mut() {
                    *v *= l0 / l1;
                }
            }
        }
        Rgba([
            clamp8(o[0] * 255.0),
            clamp8(o[1] * 255.0),
            clamp8(o[2] * 255.0),
            q[3],
        ])
    })
}

pub fn sepia(img: &RgbaImage, p: &P) -> RgbaImage {
    let t = p.f32("level") / 100.0;
    map_px(img, |_, _, q| {
        let l = luma(q);
        let s = [l * 1.07 + 20.0 * t, l * 0.87 + 8.0 * t, l * 0.6];
        let g = [l, l, l];
        Rgba([
            clamp8(g[0] + (s[0] - g[0]) * t),
            clamp8(g[1] + (s[1] - g[1]) * t),
            clamp8(g[2] + (s[2] - g[2]) * t),
            q[3],
        ])
    })
}

/// Old photographic processes: tone, contrast, fading, grain and a
/// vignette per style.
pub fn vintage_photo(img: &RgbaImage, p: &P) -> RgbaImage {
    let style = p.i("style");
    let t = p.f32("intensity") / 100.0;
    // (tint, contrast, fade, grain, vignette)
    let (tint, contrast, fade, grain, vig): ([f32; 3], f32, f32, f32, f32) = match style {
        0 => ([1.0, 0.92, 0.75], 1.3, 0.1, 0.25, 0.6),
        1 => ([1.05, 0.9, 0.7], 1.15, 0.15, 0.12, 0.5),
        2 => ([0.95, 0.95, 0.92], 1.25, 0.05, 0.1, 0.35),
        3 => ([0.9, 0.9, 0.9], 1.4, 0.0, 0.15, 0.3),
        4 => ([1.0, 1.0, 1.05], 1.1, 0.1, 0.06, 0.25),
        5 => ([1.08, 0.98, 0.85], 0.9, 0.2, 0.05, 0.2),
        _ => ([1.12, 0.95, 0.8], 0.95, 0.25, 0.04, 0.15),
    };
    let (w, h) = (img.width() as f32, img.height() as f32);
    map_px(img, |x, y, q| {
        let l = luma(q) / 255.0;
        let mut v = ((l - 0.5) * contrast + 0.5) * (1.0 - fade) + fade * 0.85;
        v += (hash01(x, y, 211) - 0.5) * grain;
        let (dx, dy) = ((x as f32 / w - 0.5) * 2.0, (y as f32 / h - 0.5) * 2.0);
        v *= 1.0 - vig * ((dx * dx + dy * dy) / 2.0).powf(1.5);
        let o: [f32; 3] = std::array::from_fn(|k| (v * tint[k]).clamp(0.0, 1.0) * 255.0);
        Rgba([
            clamp8(q[0] as f32 + (o[0] - q[0] as f32) * t),
            clamp8(q[1] as f32 + (o[1] - q[1] as f32) * t),
            clamp8(q[2] as f32 + (o[2] - q[2] as f32) * t),
            q[3],
        ])
    })
}

#[cfg(test)]
mod tests {
    use crate::fx::{apply_effect, new_effect};
    use image::{Rgba, RgbaImage};

    fn with(id: &str, vals: &[(&str, f64)]) -> tracedraw_core::BitmapEffect {
        let mut e = new_effect(id);
        for (k, v) in vals {
            e.params.insert(k.to_string(), *v);
        }
        e
    }

    #[test]
    fn lens_correction_bends_lights_and_leaves_the_centre() {
        // A grid of dark lines on white.
        let src = RgbaImage::from_fn(41, 41, |x, y| {
            if x % 10 == 0 || y % 10 == 0 {
                Rgba([0, 0, 0, 255])
            } else {
                Rgba([255, 255, 255, 255])
            }
        });
        assert_eq!(apply_effect(&src, &new_effect("lens_correction")), src);
        let bent = apply_effect(&src, &with("lens_correction", &[("distortion", 100.0)]));
        assert_eq!(bent.get_pixel(20, 20), src.get_pixel(20, 20), "centre kept");
        assert_ne!(bent, src);
        let lit = apply_effect(&src, &with("lens_correction", &[("corner_light", -100.0)]));
        assert!(lit.get_pixel(1, 1)[0] < 128, "corners darker");
        assert!(lit.get_pixel(21, 21)[0] >= 250, "centre nearly as it was");
    }

    #[test]
    fn tilt_shift_keeps_the_band_sharp_and_blurs_the_rest() {
        let src = RgbaImage::from_fn(40, 40, |x, _| {
            if x % 2 == 0 {
                Rgba([0, 0, 0, 255])
            } else {
                Rgba([255, 255, 255, 255])
            }
        });
        let out = apply_effect(&src, &with("tilt_shift", &[("blur", 4.0)]));
        assert_eq!(out.get_pixel(10, 20), src.get_pixel(10, 20), "in focus");
        let edge = out.get_pixel(10, 1)[0];
        assert!((60..=200).contains(&edge), "blurred: {edge}");
    }
}
