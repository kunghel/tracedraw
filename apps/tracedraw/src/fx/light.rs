//! Effects > Light: light thrown on a picture. Lighting (an omni light,
//! a spotlight or a light from one direction, with an ambient level),
//! Lens Flare, Light Rays, Bloom and Light Leak. Positions are in per
//! cent of the picture's width and height from its top left corner, so
//! the effect stays in place when the picture is resampled.

use super::util::*;
use super::*;
use image::{Rgba, RgbaImage};

pub const EFFECTS: &[EffectSpec] = &[
    EffectSpec {
        id: "lighting",
        params: &[
            choice("light_type", &["omni", "spot", "directional"], 0),
            range("center_x", 0.0, 100.0, 35.0, 1.0),
            range("center_y", 0.0, 100.0, 30.0, 1.0),
            range("radius", 1.0, 200.0, 70.0, 1.0),
            angle("direction", 315.0),
            range("cone", 5.0, 180.0, 60.0, 1.0),
            range("intensity", 0.0, 200.0, 100.0, 1.0),
            range("ambient", 0.0, 100.0, 35.0, 1.0),
            color("light_color", 0xFFF4E0),
            range("relief", 0.0, 100.0, 0.0, 1.0),
        ],
        apply: lighting,
        reach: no_reach,
    },
    EffectSpec {
        id: "lens_flare",
        params: &[
            range("center_x", 0.0, 100.0, 25.0, 1.0),
            range("center_y", 0.0, 100.0, 25.0, 1.0),
            range("size", 1.0, 100.0, 30.0, 1.0),
            range("intensity", 0.0, 200.0, 100.0, 1.0),
            color("light_color", 0xFFE8C0),
            check("rings", true),
        ],
        apply: lens_flare,
        reach: no_reach,
    },
    EffectSpec {
        id: "light_rays",
        params: &[
            range("center_x", 0.0, 100.0, 50.0, 1.0),
            range("center_y", 0.0, 100.0, 15.0, 1.0),
            range("length", 1.0, 100.0, 60.0, 1.0),
            range("threshold", 0.0, 255.0, 170.0, 1.0),
            range("intensity", 0.0, 200.0, 80.0, 1.0),
            color("light_color", 0xFFF2CC),
        ],
        apply: light_rays,
        reach: no_reach,
    },
    EffectSpec {
        id: "bloom",
        params: &[
            range("threshold", 0.0, 255.0, 190.0, 1.0),
            range("radius", 1.0, 100.0, 12.0, 1.0),
            range("intensity", 0.0, 300.0, 120.0, 1.0),
        ],
        apply: bloom,
        reach: no_reach,
    },
    EffectSpec {
        id: "light_leak",
        params: &[
            choice(
                "corner",
                &["top_left", "top_right", "bottom_left", "bottom_right"],
                0,
            ),
            range("size", 1.0, 100.0, 55.0, 1.0),
            range("intensity", 0.0, 100.0, 60.0, 1.0),
            color("light_color", 0xFF7A2E),
        ],
        apply: light_leak,
        reach: no_reach,
    },
];

fn center(img: &RgbaImage, p: &P) -> (f32, f32) {
    (
        p.f32("center_x") / 100.0 * img.width() as f32,
        p.f32("center_y") / 100.0 * img.height() as f32,
    )
}

fn diag(img: &RgbaImage) -> f32 {
    ((img.width() as f32).powi(2) + (img.height() as f32).powi(2))
        .sqrt()
        .max(1.0)
}

/// Screen blend of `light` (0..1 per channel, already scaled) over `q`.
fn screen(q: &Rgba<u8>, light: [f32; 3]) -> Rgba<u8> {
    let f = |c: u8, l: f32| {
        let c = c as f32 / 255.0;
        clamp8((1.0 - (1.0 - c) * (1.0 - l.clamp(0.0, 1.0))) * 255.0)
    };
    Rgba([
        f(q[0], light[0]),
        f(q[1], light[1]),
        f(q[2], light[2]),
        q[3],
    ])
}

/// The picture lit by one light: dark where it does not reach (down to
/// the ambient level), brighter and tinted where it does. Relief lights
/// the picture's brightness as a surface, so edges catch the light.
pub fn lighting(img: &RgbaImage, p: &P) -> RgbaImage {
    let kind = p.i("light_type");
    let (cx, cy) = center(img, p);
    let radius = p.f32("radius") / 100.0 * diag(img);
    let dir = p.f32("direction").to_radians();
    let (dx, dy) = (dir.cos(), -dir.sin());
    let cone = (p.f32("cone") / 2.0).to_radians().max(0.01);
    let gain = p.f32("intensity") / 100.0;
    let ambient = p.f32("ambient") / 100.0;
    let lc = p.rgb("light_color");
    let relief = p.f32("relief") / 100.0;
    let shading = (relief > 0.0).then(|| {
        let h = heights(img);
        shade(
            &h,
            img.width(),
            img.height(),
            p.f32("direction"),
            2.0 + relief * 6.0,
        )
    });
    let w = img.width();
    map_px(img, |x, y, q| {
        let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
        let reach = match kind {
            // From one side: brighter towards the light's direction.
            2 => {
                let span = diag(img);
                let t = ((fx - img.width() as f32 / 2.0) * dx
                    + (fy - img.height() as f32 / 2.0) * dy)
                    / span
                    + 0.5;
                t.clamp(0.0, 1.0)
            }
            _ => {
                let (vx, vy) = (fx - cx, fy - cy);
                let d = (vx * vx + vy * vy).sqrt();
                let fall = (1.0 - d / radius.max(1.0)).clamp(0.0, 1.0);
                let fall = fall * fall * (3.0 - 2.0 * fall);
                if kind == 1 && d > 0.5 {
                    // A spotlight pointing along `direction`: full inside
                    // the cone, fading over its last fifth.
                    let a = ((vx * dx + vy * dy) / d).clamp(-1.0, 1.0).acos();
                    let edge = (1.0 - (a - cone * 0.8) / (cone * 0.2)).clamp(0.0, 1.0);
                    fall * if a <= cone * 0.8 { 1.0 } else { edge }
                } else {
                    fall
                }
            }
        };
        let bump = shading
            .as_ref()
            .and_then(|s| s.get((y * w + x) as usize))
            .map(|s| 1.0 + (s - 0.5) * relief)
            .unwrap_or(1.0);
        let level = (ambient + reach * gain) * bump;
        let ch = |c: u8, l: u8| {
            let lit = c as f32 * level * (l as f32 / 255.0 * reach + (1.0 - reach));
            clamp8(lit)
        };
        Rgba([ch(q[0], lc[0]), ch(q[1], lc[1]), ch(q[2], lc[2]), q[3]])
    })
}

/// A bright source with a halo and, on the line through the picture's
/// centre, a few soft rings, screened over the picture.
pub fn lens_flare(img: &RgbaImage, p: &P) -> RgbaImage {
    let (cx, cy) = center(img, p);
    let size = p.f32("size") / 100.0 * diag(img) * 0.5;
    let gain = p.f32("intensity") / 100.0;
    let lc = p.rgb("light_color");
    let rings = p.b("rings");
    let (mx, my) = (img.width() as f32 / 2.0, img.height() as f32 / 2.0);
    // Rings along the line from the source through the centre: (position
    // along it, radius, strength).
    let ghosts: &[(f32, f32, f32)] = if rings {
        &[
            (0.5, 0.10, 0.25),
            (1.2, 0.18, 0.18),
            (1.6, 0.06, 0.35),
            (2.1, 0.28, 0.12),
        ]
    } else {
        &[]
    };
    map_px(img, |x, y, q| {
        let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
        let d = ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt();
        let core = (1.0 - d / (size * 0.12).max(1.0)).clamp(0.0, 1.0);
        let halo = (-(d / size.max(1.0)).powi(2) * 3.0).exp() * 0.55;
        let mut l = core + halo;
        for (t, r, s) in ghosts {
            let gx = cx + (mx - cx) * t;
            let gy = cy + (my - cy) * t;
            let gd = ((fx - gx).powi(2) + (fy - gy).powi(2)).sqrt();
            let rr = r * size.max(1.0);
            let ring = (1.0 - ((gd - rr * 0.8).abs() / (rr * 0.25).max(1.0))).clamp(0.0, 1.0);
            let disc = (1.0 - gd / rr).clamp(0.0, 1.0) * 0.4;
            l += (ring + disc) * s;
        }
        let l = l * gain;
        screen(
            q,
            [
                l * lc[0] as f32 / 255.0,
                l * lc[1] as f32 / 255.0,
                l * lc[2] as f32 / 255.0,
            ],
        )
    })
}

/// Light streaming from a point: the bright parts of the picture are
/// smeared outwards from the centre and screened over it.
pub fn light_rays(img: &RgbaImage, p: &P) -> RgbaImage {
    let (cx, cy) = center(img, p);
    let length = p.f32("length") / 100.0;
    let threshold = p.f32("threshold");
    let gain = p.f32("intensity") / 100.0;
    let lc = p.rgb("light_color");
    let (w, h) = img.dimensions();
    // What gives off light: the parts brighter than the threshold.
    let bright: Vec<f32> = img
        .pixels()
        .map(|q| {
            ((luma(q) - threshold) / (255.0 - threshold).max(1.0)).clamp(0.0, 1.0)
                * (q[3] as f32 / 255.0)
        })
        .collect();
    let at = |x: f32, y: f32| -> f32 {
        if x < 0.0 || y < 0.0 || x >= w as f32 || y >= h as f32 {
            return 0.0;
        }
        bright
            .get((y as u32 * w + x as u32) as usize)
            .copied()
            .unwrap_or(0.0)
    };
    const STEPS: usize = 24;
    map_px(img, |x, y, q| {
        let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
        let mut acc = 0.0;
        let mut wsum = 0.0;
        for i in 0..STEPS {
            let t = i as f32 / STEPS as f32 * length;
            let wt = 1.0 - i as f32 / STEPS as f32;
            acc += at(fx + (cx - fx) * t, fy + (cy - fy) * t) * wt;
            wsum += wt;
        }
        let l = acc / wsum.max(1e-6) * gain;
        screen(
            q,
            [
                l * lc[0] as f32 / 255.0,
                l * lc[1] as f32 / 255.0,
                l * lc[2] as f32 / 255.0,
            ],
        )
    })
}

/// The bright parts glowing: they are kept above the threshold, blurred
/// and screened back over the picture.
pub fn bloom(img: &RgbaImage, p: &P) -> RgbaImage {
    let threshold = p.f32("threshold");
    let gain = p.f32("intensity") / 100.0;
    let radius = p.f32("radius");
    let bright = map_px(img, |_, _, q| {
        let k = ((luma(q) - threshold) / (255.0 - threshold).max(1.0)).clamp(0.0, 1.0);
        let k = k * q[3] as f32 / 255.0;
        Rgba([
            clamp8(q[0] as f32 * k),
            clamp8(q[1] as f32 * k),
            clamp8(q[2] as f32 * k),
            255,
        ])
    });
    let glow = gaussian(&bright, radius / 2.0);
    map_px(img, |x, y, q| {
        let g = glow.get_pixel(x, y);
        screen(
            q,
            [
                g[0] as f32 / 255.0 * gain,
                g[1] as f32 / 255.0 * gain,
                g[2] as f32 / 255.0 * gain,
            ],
        )
    })
}

/// A warm glow coming in from a corner, as light reaching the film.
pub fn light_leak(img: &RgbaImage, p: &P) -> RgbaImage {
    let (w, h) = (img.width() as f32, img.height() as f32);
    let (cx, cy) = match p.i("corner") {
        1 => (w, 0.0),
        2 => (0.0, h),
        3 => (w, h),
        _ => (0.0, 0.0),
    };
    let size = p.f32("size") / 100.0 * diag(img);
    let gain = p.f32("intensity") / 100.0;
    let lc = p.rgb("light_color");
    map_px(img, |x, y, q| {
        let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
        let t = (1.0 - d / size.max(1.0)).clamp(0.0, 1.0);
        let l = t * t * gain;
        screen(
            q,
            [
                l * lc[0] as f32 / 255.0,
                l * lc[1] as f32 / 255.0,
                l * lc[2] as f32 / 255.0,
            ],
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn run(id: &str, img: &RgbaImage, set: &[(&str, f64)]) -> RgbaImage {
        let mut e = new_effect(id);
        let mut params = BTreeMap::new();
        for (k, v) in set {
            params.insert(k.to_string(), *v);
        }
        e.params = params;
        apply_effect(img, &e)
    }

    fn gray(v: u8) -> RgbaImage {
        RgbaImage::from_pixel(40, 30, Rgba([v, v, v, 255]))
    }

    #[test]
    fn lighting_brightens_near_the_light_and_darkens_far_away() {
        let img = gray(128);
        let out = run(
            "lighting",
            &img,
            &[
                ("center_x", 10.0),
                ("center_y", 10.0),
                ("light_color", 16_777_215.0),
            ],
        );
        let near = out.get_pixel(4, 3)[0];
        let far = out.get_pixel(39, 29)[0];
        assert!(near > 128, "near {near}");
        assert!(far < 128, "far {far}");
        assert_eq!(out.dimensions(), img.dimensions());
    }

    #[test]
    fn a_spotlight_lights_only_its_cone() {
        let img = gray(128);
        // Pointing right (0 degrees) from the left middle.
        let out = run(
            "lighting",
            &img,
            &[
                ("light_type", 1.0),
                ("center_x", 5.0),
                ("center_y", 50.0),
                ("direction", 0.0),
                ("cone", 30.0),
                ("radius", 150.0),
                ("ambient", 0.0),
            ],
        );
        let ahead = out.get_pixel(30, 15)[0];
        let behind = out.get_pixel(0, 0)[0];
        assert!(ahead > behind + 40, "ahead {ahead} behind {behind}");
    }

    #[test]
    fn lens_flare_only_adds_light() {
        let img = gray(60);
        let out = run("lens_flare", &img, &[]);
        assert!(out.pixels().zip(img.pixels()).all(|(a, b)| a[0] >= b[0]));
        let (cx, cy) = (10u32, 7u32);
        assert!(out.get_pixel(cx, cy)[0] > 200);
    }

    #[test]
    fn bloom_glows_around_bright_parts_only() {
        let mut img = gray(30);
        for y in 12..18 {
            for x in 17..23 {
                img.put_pixel(x, y, Rgba([255, 255, 255, 255]));
            }
        }
        let out = run("bloom", &img, &[("radius", 8.0)]);
        assert!(out.get_pixel(15, 15)[0] > 30, "next to the bright square");
        assert_eq!(out.get_pixel(0, 0)[0], 30, "far corner untouched");
    }

    #[test]
    fn light_rays_and_leak_keep_size_and_alpha() {
        let mut img = gray(200);
        img.put_pixel(0, 0, Rgba([200, 200, 200, 0]));
        for id in ["light_rays", "light_leak"] {
            let out = run(id, &img, &[]);
            assert_eq!(out.dimensions(), img.dimensions());
            assert_eq!(out.get_pixel(0, 0)[3], 0, "{id}");
        }
        let leak = run("light_leak", &gray(50), &[]);
        assert!(leak.get_pixel(0, 0)[0] > leak.get_pixel(39, 29)[0]);
    }
}
