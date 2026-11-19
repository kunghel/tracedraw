//! Printed looks in Color Transform: Riso Print (two inks screened as
//! dots, slightly out of register, with a grainy ink laydown on tinted
//! paper) and Newsprint (black or four-colour dots on newsprint, with dot
//! gain and paper fibres). Both keep alpha.

use super::util::*;
use super::*;
use image::{Rgba, RgbaImage};
use std::f32::consts::PI;

/// Riso Print, listed in Color Transform.
pub const RISO: EffectSpec = EffectSpec {
    id: "riso_print",
    params: &[
        choice(
            "riso_inks",
            &[
                "riso_pink_blue",
                "riso_orange_teal",
                "riso_red_black",
                "riso_yellow_purple",
                "riso_custom",
            ],
            0,
        ),
        color("first_ink", 0xFF48B0),
        color("second_ink", 0x0078BF),
        range("size", 2.0, 40.0, 6.0, 1.0),
        angle("angle", 15.0),
        range("distance", 0.0, 40.0, 3.0, 1.0),
        range("grain", 0.0, 100.0, 40.0, 1.0),
        color("paper_color", 0xF5F0E4),
        range("seed", 0.0, 9999.0, 1.0, 1.0),
    ],
    apply: riso_print,
    reach: no_reach,
};

/// Newsprint, listed in Color Transform.
pub const NEWSPRINT: EffectSpec = EffectSpec {
    id: "newsprint",
    params: &[
        choice("news_inks", &["news_black", "news_color"], 0),
        range("size", 2.0, 40.0, 5.0, 1.0),
        angle("angle", 45.0),
        range("dot_gain", 0.0, 100.0, 30.0, 1.0),
        range("distance", 0.0, 20.0, 1.0, 1.0),
        range("grain", 0.0, 100.0, 35.0, 1.0),
        color("paper_color", 0xE8E2D2),
        range("seed", 0.0, 9999.0, 1.0, 1.0),
    ],
    apply: newsprint,
    reach: no_reach,
};

/// The two inks of a Riso Print preset (0xRRGGBB), or the chosen ones.
fn riso_inks(p: &P) -> [[u8; 3]; 2] {
    let pair = match p.i("riso_inks") {
        0 => [0xFF48B0, 0x0078BF],
        1 => [0xFF6C2F, 0x00838A],
        2 => [0xFF665E, 0x1C1C1C],
        3 => [0xFFE800, 0x765BA7],
        _ => return [p.rgb("first_ink"), p.rgb("second_ink")],
    };
    pair.map(rgb)
}

/// How much light an ink lets through per channel, 0..1 (never quite 0,
/// so its density stays finite).
fn through(c: [u8; 3]) -> [f32; 3] {
    c.map(|v| (v as f32 / 255.0).max(0.02))
}

/// How much ink of a screen of round dots lies on the pixel at (x, y):
/// the cell (side `cell` pixels, turned by `angle` radians) holds a dot
/// that grows from its centre with `cov` (0..1) until the dots meet and
/// fill it. Smoothed over about one pixel.
pub fn screen(x: f32, y: f32, cell: f32, angle: f32, cov: f32) -> f32 {
    if cov <= 0.0 {
        return 0.0;
    }
    if cov >= 1.0 {
        return 1.0;
    }
    let (s, c) = angle.sin_cos();
    let u = (x * c + y * s) / cell;
    let v = (-x * s + y * c) / cell;
    // -1..1 across the cell.
    let cu = (u - u.floor()) * 2.0 - 1.0;
    let cv = (v - v.floor()) * 2.0 - 1.0;
    // The share of the cell a round dot reaching this far covers (0 at
    // the centre, 1 in the corners), so the ink's area is `cov`; and how
    // fast it grows outward, to smooth the edge.
    let r = (cu * cu + cv * cv).sqrt();
    let (thr, slope) = if r <= 1.0 {
        (PI * r * r / 4.0, PI * r / 2.0)
    } else {
        let a = (1.0 / r).acos();
        (
            (PI * r * r - 4.0 * r * r * a + 4.0 * (r * r - 1.0).sqrt()) / 4.0,
            r * (PI / 2.0 - 2.0 * a),
        )
    };
    let fw = (slope * 2.0 / cell).max(1e-3);
    ((cov - thr) / fw + 0.5).clamp(0.0, 1.0)
}

/// Ink coverage at `i` of a map `w` wide, `h` high, read at a pixel
/// moved by (dx, dy) and clamped to the image.
fn at(map: &[f32], w: u32, h: u32, x: u32, y: u32, dx: f32, dy: f32) -> f32 {
    let xi = (x as f32 - dx).round().clamp(0.0, w as f32 - 1.0) as usize;
    let yi = (y as f32 - dy).round().clamp(0.0, h as f32 - 1.0) as usize;
    map[yi * w as usize + xi]
}

/// Paper with ink on it: each ink takes away the light it does not let
/// through, in proportion to how much of it lies there.
fn print(paper: [u8; 3], inks: &[([f32; 3], f32)], fibre: f32) -> [u8; 3] {
    std::array::from_fn(|ch| {
        let mut v = paper[ch] as f32 / 255.0 * fibre;
        for (t, amount) in inks {
            v *= 1.0 - amount * (1.0 - t[ch]);
        }
        clamp8(v * 255.0)
    })
}

/// Riso Print: the picture split into two inks (by their densities, so
/// each colour goes to the ink that makes it best), each screened as
/// round dots at its own angle, the second ink out of register by
/// `distance` pixels, the ink laid down unevenly and speckled by `grain`
/// over the paper colour.
pub fn riso_print(img: &RgbaImage, p: &P) -> RgbaImage {
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return img.clone();
    }
    let inks = riso_inks(p);
    let t = inks.map(through);
    let dens = t.map(|t| t.map(|v| -v.ln()));
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let (aa, ab, bb) = (
        dot(dens[0], dens[0]),
        dot(dens[0], dens[1]),
        dot(dens[1], dens[1]),
    );
    let det = aa * bb - ab * ab;
    // Coverage of each ink: the least-squares fit of the pixel's density
    // as so much of the first ink plus so much of the second.
    let mut cov = [vec![0f32; (w * h) as usize], vec![0f32; (w * h) as usize]];
    for (x, y, q) in img.enumerate_pixels() {
        let d: [f32; 3] = std::array::from_fn(|ch| -((q[ch] as f32 / 255.0).max(1.0 / 255.0)).ln());
        let (da, db) = (dot(d, dens[0]), dot(d, dens[1]));
        let (a, b) = if det.abs() < 1e-4 * aa.max(bb).max(1e-6) {
            (da / aa.max(1e-6), 0.0)
        } else {
            ((bb * da - ab * db) / det, (aa * db - ab * da) / det)
        };
        let i = (y * w + x) as usize;
        cov[0][i] = a.clamp(0.0, 1.0);
        cov[1][i] = b.clamp(0.0, 1.0);
    }
    let cell = p.f32("size").max(2.0);
    let angle = p.f32("angle").to_radians();
    let shift = p.f32("distance");
    let (dx, dy) = (shift * 0.8, shift * 0.6);
    let grain = p.f32("grain") / 100.0;
    let seed = p.u("seed");
    let paper = p.rgb("paper_color");
    map_px(img, |x, y, q| {
        let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
        let a = at(&cov[0], w, h, x, y, 0.0, 0.0);
        let b = at(&cov[1], w, h, x, y, dx, dy);
        let ia = screen(fx, fy, cell, angle, a);
        let ib = screen(fx - dx, fy - dy, cell, angle + 30f32.to_radians(), b);
        // Uneven laydown: soft blotches and fine speckles of missing ink.
        let lay = |s: u32| {
            let blotch = value_noise(fx, fy, cell * 1.5, seed.wrapping_add(s));
            let speck = hash01(x, y, seed.wrapping_add(s + 7));
            1.0 - grain * (0.35 * blotch + if speck < 0.12 * grain { 0.6 } else { 0.0 })
        };
        let fibre = 1.0 - grain * 0.05 * value_noise(fx, fy, 2.0, seed.wrapping_add(31));
        let o = print(paper, &[(t[0], ia * lay(0)), (t[1], ib * lay(11))], fibre);
        Rgba([o[0], o[1], o[2], q[3]])
    })
}

/// Newsprint: black dots (or cyan, magenta, yellow and black, each at its
/// angle around `angle`, cyan and magenta slightly out of register) on
/// newsprint, the dots swollen by `dot_gain` as ink spreads into the
/// paper, and the paper's fibres showing with `grain`.
pub fn newsprint(img: &RgbaImage, p: &P) -> RgbaImage {
    let colour = p.i("news_inks") == 1;
    let cell = p.f32("size").max(2.0);
    let base = p.f32("angle");
    let gain = p.f32("dot_gain") / 100.0;
    let shift = p.f32("distance");
    let grain = p.f32("grain") / 100.0;
    let seed = p.u("seed");
    let paper = p.rgb("paper_color");
    let swell = |c: f32| (c + gain * 0.6 * c * (1.0 - c)).clamp(0.0, 1.0);
    let black = through(rgb(0x262422));
    let cmy = [
        through(rgb(0x2BA6DE)),
        through(rgb(0xE0357F)),
        through(rgb(0xF6E03C)),
    ];
    map_px(img, |x, y, q| {
        let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
        let fibre = 1.0
            - grain * 0.10 * (fractal(fx, fy, 6.0, seed) - 0.5).abs() * 2.0
            - grain * 0.04 * hash01(x, y, seed.wrapping_add(3));
        let blot = 1.0 - grain * 0.2 * value_noise(fx, fy, cell * 2.0, seed.wrapping_add(9));
        let s = [q[0], q[1], q[2]].map(|v| v as f32 / 255.0);
        let o = if colour {
            let k = 1.0 - s[0].max(s[1]).max(s[2]);
            let den = (1.0 - k).max(1e-6);
            let ink = |ch: usize| ((1.0 - s[ch] - k) / den).clamp(0.0, 1.0);
            // Cyan and magenta off in opposite directions.
            let off = [(shift, 0.0), (-shift * 0.5, shift * 0.7), (0.0, 0.0)];
            let angles = [base - 30.0, base + 30.0, base - 45.0];
            let mut layers: Vec<([f32; 3], f32)> = (0..3)
                .map(|i| {
                    let (ox, oy) = off[i];
                    let src = sample(img, fx - 0.5 - ox, fy - 0.5 - oy);
                    let sc = [src[0], src[1], src[2]].map(|v| v as f32 / 255.0);
                    let sk = 1.0 - sc[0].max(sc[1]).max(sc[2]);
                    let c = ((1.0 - sc[i] - sk) / (1.0 - sk).max(1e-6)).clamp(0.0, 1.0);
                    let c = if ox == 0.0 && oy == 0.0 { ink(i) } else { c };
                    let d = screen(fx - ox, fy - oy, cell, angles[i].to_radians(), swell(c));
                    (cmy[i], d * blot)
                })
                .collect();
            layers.push((
                black,
                screen(fx, fy, cell, base.to_radians(), swell(k)) * blot,
            ));
            print(paper, &layers, fibre)
        } else {
            let c = 1.0 - luma(q) / 255.0;
            let d = screen(fx, fy, cell, base.to_radians(), swell(c));
            print(paper, &[(black, d * blot)], fibre)
        };
        Rgba([o[0], o[1], o[2], q[3]])
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

    fn mean(img: &RgbaImage, ch: usize) -> f32 {
        img.pixels().map(|p| p[ch] as f32).sum::<f32>() / (img.width() * img.height()) as f32
    }

    fn flat(c: u32) -> RgbaImage {
        let [r, g, b] = rgb(c);
        RgbaImage::from_pixel(48, 48, Rgba([r, g, b, 255]))
    }

    #[test]
    fn riso_leaves_white_as_paper_and_splits_colours_by_ink() {
        let clean = [("grain", 0.0), ("distance", 0.0)];
        let white = run(&flat(0xFFFFFF), "riso_print", &clean);
        assert!(white.pixels().all(|p| p.0 == [0xF5, 0xF0, 0xE4, 255]));
        // The pink ink's own colour is printed with the pink ink alone:
        // red stays high, no blue ink pulls it down.
        let pink = run(&flat(0xFF48B0), "riso_print", &clean);
        assert!(mean(&pink, 0) > 220.0, "{}", mean(&pink, 0));
        assert!(mean(&pink, 1) < 150.0);
        // Blue goes to the blue ink: red falls, blue stays.
        let blue = run(&flat(0x0078BF), "riso_print", &clean);
        assert!(mean(&blue, 0) < 80.0, "{}", mean(&blue, 0));
        assert!(mean(&blue, 2) > 150.0);
        // Black takes both inks, so it is far darker than either.
        let dark = run(&flat(0x000000), "riso_print", &clean);
        assert!(mean(&dark, 1) < 40.0);
    }

    #[test]
    fn riso_mid_tones_are_dots_and_follow_the_seed() {
        let grey = flat(0x9A9A9A);
        let out = run(&grey, "riso_print", &[("grain", 0.0)]);
        // Screened: some pixels are bare paper, some fully inked.
        assert!(out.pixels().any(|p| p[0] >= 0xF0));
        assert!(out.pixels().any(|p| p[0] < 0x90));
        let a = run(&grey, "riso_print", &[("seed", 5.0)]);
        assert_eq!(a, run(&grey, "riso_print", &[("seed", 5.0)]));
        assert_ne!(a, run(&grey, "riso_print", &[("seed", 6.0)]));
        // Custom inks are used as chosen.
        let green = run(
            &flat(0x000000),
            "riso_print",
            &[
                ("riso_inks", 4.0),
                ("first_ink", 0x00A000 as f64),
                ("second_ink", 0x00A000 as f64),
                ("grain", 0.0),
            ],
        );
        let p = green.get_pixel(20, 20);
        assert!(p[1] > p[0] && p[1] > p[2], "{p:?}");
    }

    #[test]
    fn newsprint_black_follows_the_tone_with_dot_gain() {
        let ramp = RgbaImage::from_fn(96, 32, |x, _| {
            let v = (x * 255 / 95) as u8;
            Rgba([v, v, v, 255])
        });
        let out = run(&ramp, "newsprint", &[("grain", 0.0), ("dot_gain", 0.0)]);
        // Grey only (paper tint aside): no colour inks.
        let left: f32 = (0..32).map(|y| out.get_pixel(4, y)[0] as f32).sum::<f32>() / 32.0;
        let right: f32 = (0..32).map(|y| out.get_pixel(92, y)[0] as f32).sum::<f32>() / 32.0;
        assert!(left < 80.0 && right > 200.0, "{left} {right}");
        let mid = flat(0x808080);
        let thin = mean(
            &run(&mid, "newsprint", &[("grain", 0.0), ("dot_gain", 0.0)]),
            0,
        );
        let fat = mean(
            &run(&mid, "newsprint", &[("grain", 0.0), ("dot_gain", 100.0)]),
            0,
        );
        assert!(fat < thin - 10.0, "{thin} {fat}");
    }

    #[test]
    fn newsprint_colour_prints_red_with_magenta_and_yellow() {
        let out = run(
            &flat(0xE02020),
            "newsprint",
            &[("news_inks", 1.0), ("grain", 0.0), ("distance", 0.0)],
        );
        assert!(mean(&out, 0) > mean(&out, 1) + 60.0);
        assert!(mean(&out, 0) > mean(&out, 2) + 60.0);
        // The paper shows through the white.
        let white = run(
            &flat(0xFFFFFF),
            "newsprint",
            &[("news_inks", 1.0), ("grain", 0.0)],
        );
        assert!(white.pixels().all(|p| p.0 == [0xE8, 0xE2, 0xD2, 255]));
    }

    #[test]
    fn a_screen_fills_its_cell_with_the_coverage() {
        for cov in [0.1f32, 0.3, 0.5, 0.7, 0.9] {
            let mut sum = 0.0;
            for y in 0..64 {
                for x in 0..64 {
                    sum += screen(x as f32 + 0.5, y as f32 + 0.5, 8.0, 0.4, cov);
                }
            }
            let got = sum / 4096.0;
            assert!((got - cov).abs() < 0.03, "{cov} {got}");
        }
    }
}
