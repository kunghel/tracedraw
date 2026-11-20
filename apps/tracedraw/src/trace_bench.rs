//! Trace Bitmap quality benchmark (#2938): pictures made the way logos
//! reach people (JPEG noise, anti-aliased thin strokes and small
//! capitals, a dark backdrop that belongs to the design, a soft grey
//! gradient behind the logo, smooth gradients, a drop shadow) go through
//! the outline presets, and each trace is measured against the clean
//! picture it was made from, not against its noisy copy.
//!
//! Every fixture is drawn here from shapes (no files with someone else's
//! rights): the clean picture is the shapes drawn with 4 x 4 samples a
//! pixel, the input is that picture degraded (JPEG at a low quality, or
//! as it is). Measured per fixture and preset:
//!
//! - curves, nodes and colours of the trace;
//! - wrong: the share (percent) of the pixels drawn wrong against the
//!   clean picture ([`crate::trace::wrong_share`], an edge a pixel off is
//!   not wrong);
//! - halos: tiny shapes (under 12 square pixels) of the trace, the
//!   fringes a JPEG or an anti-aliased edge leaves;
//! - text: wrong share inside the small capitals' box, where there are
//!   some;
//! - the background decision: the logo on a soft grey gradient is
//!   measured against the logo on white, so a backdrop left in is wrong;
//! - shift: how far (pixels) the trace's ink sits from the clean ink.
//!
//! The test fails when a measure gets clearly worse than the baseline
//! kept below ([`BASELINE`]). `TRACE_BENCH_OUT=<folder>` writes the
//! report (`report.md`) and, per run, the input, the clean picture and
//! the trace side by side. See docs/trace-benchmark.md.

use crate::trace::{render_traced, stats, trace, wrong_share, Preset, Traced};
use image::{Rgba, RgbaImage};

/// The side of every fixture.
const SIDE: u32 = 300;

/// A fixture: its name, the clean picture, the input traced, and the box
/// of its small capitals (x0, y0, x1, y1) when it has some.
struct Fixture {
    name: &'static str,
    clean: RgbaImage,
    input: RgbaImage,
    text: Option<[u32; 4]>,
}

/// A colour as floats.
type Rgb = [f32; 3];

const WHITE: Rgb = [255.0, 255.0, 255.0];
const BLACK: Rgb = [20.0, 20.0, 22.0];

/// The picture of `at` (the colour at a point) with 4 x 4 samples a pixel.
fn draw(at: impl Fn(f32, f32) -> Rgb) -> RgbaImage {
    RgbaImage::from_fn(SIDE, SIDE, |x, y| {
        let mut c = [0.0f32; 3];
        for sy in 0..4 {
            for sx in 0..4 {
                let s = at(
                    x as f32 + (sx as f32 + 0.5) / 4.0,
                    y as f32 + (sy as f32 + 0.5) / 4.0,
                );
                for k in 0..3 {
                    c[k] += s[k] / 16.0;
                }
            }
        }
        Rgba([c[0] as u8, c[1] as u8, c[2] as u8, 255])
    })
}

/// `img` through a JPEG of `quality` and back.
fn jpeg(img: &RgbaImage, quality: u8) -> RgbaImage {
    let rgb = image::DynamicImage::ImageRgba8(img.clone()).to_rgb8();
    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, quality)
        .encode_image(&rgb)
        .expect("jpeg");
    image::load_from_memory(&bytes).expect("decoded").to_rgba8()
}

fn dist(x: f32, y: f32, cx: f32, cy: f32) -> f32 {
    ((x - cx).powi(2) + (y - cy).powi(2)).sqrt()
}

/// Whether (x, y) is on the segment a-b drawn `w` wide.
fn on_line(x: f32, y: f32, a: (f32, f32), b: (f32, f32), w: f32) -> bool {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = (((x - a.0) * dx + (y - a.1) * dy) / len2).clamp(0.0, 1.0);
    dist(x, y, a.0 + t * dx, a.1 + t * dy) <= w / 2.0
}

/// Block capitals T, R, A, C, E (strokes of a 5 x 7 cell), `h` pixels
/// tall from (x0, y0), `w` the stroke: whether (x, y) is ink.
fn capitals(x: f32, y: f32, x0: f32, y0: f32, h: f32, w: f32) -> bool {
    let u = h / 7.0;
    let letters: [&[((f32, f32), (f32, f32))]; 5] = [
        // T
        &[((0.0, 0.0), (5.0, 0.0)), ((2.5, 0.0), (2.5, 7.0))],
        // R
        &[
            ((0.0, 0.0), (0.0, 7.0)),
            ((0.0, 0.0), (4.0, 0.0)),
            ((4.0, 0.0), (4.5, 1.8)),
            ((4.5, 1.8), (4.0, 3.5)),
            ((4.0, 3.5), (0.0, 3.5)),
            ((1.5, 3.5), (4.5, 7.0)),
        ],
        // A
        &[
            ((0.0, 7.0), (2.5, 0.0)),
            ((2.5, 0.0), (5.0, 7.0)),
            ((1.0, 4.5), (4.0, 4.5)),
        ],
        // C
        &[
            ((5.0, 0.0), (0.0, 0.0)),
            ((0.0, 0.0), (0.0, 7.0)),
            ((0.0, 7.0), (5.0, 7.0)),
        ],
        // E
        &[
            ((0.0, 0.0), (0.0, 7.0)),
            ((0.0, 0.0), (5.0, 0.0)),
            ((0.0, 3.5), (4.0, 3.5)),
            ((0.0, 7.0), (5.0, 7.0)),
        ],
    ];
    letters.iter().enumerate().any(|(i, strokes)| {
        // Letters 5 units wide, 3 units apart (spaced capitals).
        let lx = x0 + i as f32 * 8.0 * u;
        strokes.iter().any(|(a, b)| {
            on_line(
                x,
                y,
                (lx + a.0 * u, y0 + a.1 * u),
                (lx + b.0 * u, y0 + b.1 * u),
                w,
            )
        })
    })
}

/// The box of [`capitals`] from (x0, y0) at height `h`, with a margin.
fn capitals_box(x0: f32, y0: f32, h: f32) -> [u32; 4] {
    let u = h / 7.0;
    [
        (x0 - 3.0) as u32,
        (y0 - 3.0) as u32,
        (x0 + 37.0 * u + 3.0) as u32,
        (y0 + h + 3.0) as u32,
    ]
}

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    std::array::from_fn(|k| a[k] + (b[k] - a[k]) * t)
}

/// The fixtures, as described in the module's comment.
fn fixtures() -> Vec<Fixture> {
    let red = [220.0, 30.0, 40.0];
    let blue = [20.0, 60.0, 150.0];
    let gold = [235.0, 175.0, 30.0];
    let green = [30.0, 140.0, 70.0];
    // A colour logo: a red disc, a white bar, a blue triangle.
    let logo = move |x: f32, y: f32| -> Rgb {
        let d = dist(x, y, 150.0, 150.0);
        let tri = y > 40.0 && y < 125.0 && (x - 150.0).abs() < (y - 40.0) * 0.6;
        if tri {
            blue
        } else if (70.0..230.0).contains(&x) && (130.0..190.0).contains(&y) {
            WHITE
        } else if d < 120.0 {
            red
        } else {
            WHITE
        }
    };
    let logo_clean = draw(logo);
    // Thin black art: rings, a cross of 2 px lines and spaced capitals.
    let (tx, ty, th) = (60.0, 230.0, 14.0);
    let art = move |x: f32, y: f32| -> Rgb {
        let d = dist(x, y, 150.0, 110.0);
        let ring = (d - 80.0).abs() < 1.5 || (d - 60.0).abs() < 1.0;
        let cross = on_line(x, y, (90.0, 110.0), (210.0, 110.0), 2.0)
            || on_line(x, y, (150.0, 50.0), (150.0, 170.0), 2.0);
        if ring || cross || capitals(x, y, tx, ty, th, 2.0) {
            BLACK
        } else {
            WHITE
        }
    };
    let art_clean = draw(art);
    // A flat colour logo with curves: a gold star in a green ring.
    let star = move |x: f32, y: f32| -> Rgb {
        let (dx, dy) = (x - 150.0, y - 150.0);
        let r = (dx * dx + dy * dy).sqrt();
        let a = dy.atan2(dx);
        let edge = 55.0 + 30.0 * (5.0 * a).cos();
        if r < edge {
            gold
        } else if (110.0..135.0).contains(&r) {
            green
        } else {
            WHITE
        }
    };
    // An emblem on a dark backdrop that is part of the design.
    let navy = [18.0, 24.0, 60.0];
    let emblem = move |x: f32, y: f32| -> Rgb {
        let d = dist(x, y, 150.0, 140.0);
        if !(20.0..280.0).contains(&x) || !(20.0..280.0).contains(&y) {
            WHITE
        } else if d < 70.0 && d > 55.0 {
            gold
        } else if d < 40.0 {
            WHITE
        } else if capitals(x, y, 95.0, 235.0, 16.0, 3.0) {
            gold
        } else {
            navy
        }
    };
    // A logo on a soft light grey gradient.
    let on_grey = move |x: f32, y: f32| -> Rgb {
        let l = logo(x, y);
        if l == WHITE && dist(x, y, 150.0, 150.0) >= 120.0 {
            let g = 238.0 - 30.0 * y / SIDE as f32;
            [g, g, g + 2.0]
        } else {
            l
        }
    };
    // Smooth gradients: a radial disc and a linear bar.
    let gradients = move |x: f32, y: f32| -> Rgb {
        let d = dist(x, y, 150.0, 120.0);
        if d < 80.0 {
            mix(gold, red, d / 80.0)
        } else if (40.0..260.0).contains(&x) && (220.0..260.0).contains(&y) {
            mix(blue, green, (x - 40.0) / 220.0)
        } else {
            WHITE
        }
    };
    // A drop shadow: a blue rounded square over a soft grey shadow.
    let shadow = move |x: f32, y: f32| -> Rgb {
        let inside = |x: f32, y: f32, grow: f32| {
            let (qx, qy) = ((x - 140.0).abs() - 70.0, (y - 140.0).abs() - 70.0);
            let out = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
            out + qx.max(qy).min(0.0) - 15.0 < grow
        };
        if inside(x, y, 0.0) {
            blue
        } else {
            // The shadow, 12 px down and right, fading over 10 px.
            let (sx, sy) = (x - 12.0, y - 12.0);
            let mut k = 0.0;
            for g in 0..10 {
                if inside(sx, sy, -(g as f32)) {
                    k += 0.1;
                }
            }
            mix(WHITE, [120.0, 120.0, 125.0], k)
        }
    };
    // Small capitals only, JPEG'd.
    let small = move |x: f32, y: f32| -> Rgb {
        if capitals(x, y, 40.0, 120.0, 10.0, 1.6) || capitals(x, y, 40.0, 160.0, 20.0, 3.0) {
            BLACK
        } else {
            WHITE
        }
    };
    let small_clean = draw(small);
    let star_clean = draw(star);
    let emblem_clean = draw(emblem);
    let grey_clean = draw(on_grey);
    let gradients_clean = draw(gradients);
    let shadow_clean = draw(shadow);
    vec![
        Fixture {
            name: "jpeg-logo-q35",
            input: jpeg(&logo_clean, 35),
            clean: logo_clean.clone(),
            text: None,
        },
        Fixture {
            name: "thin-black-art",
            input: jpeg(&art_clean, 60),
            clean: art_clean,
            text: Some(capitals_box(tx, ty, th)),
        },
        Fixture {
            name: "flat-colour-aa",
            input: star_clean.clone(),
            clean: star_clean,
            text: None,
        },
        Fixture {
            name: "dark-emblem",
            input: jpeg(&emblem_clean, 70),
            clean: emblem_clean,
            text: Some(capitals_box(95.0, 235.0, 16.0)),
        },
        Fixture {
            name: "grey-gradient-backdrop",
            // The soft grey behind the logo is a backdrop, not part of
            // the design: the trace should leave it out.
            input: jpeg(&grey_clean, 60),
            clean: logo_clean.clone(),
            text: None,
        },
        Fixture {
            name: "smooth-gradients",
            input: gradients_clean.clone(),
            clean: gradients_clean,
            text: None,
        },
        Fixture {
            name: "drop-shadow",
            input: shadow_clean.clone(),
            clean: shadow_clean,
            text: None,
        },
        Fixture {
            name: "small-capitals-q50",
            input: jpeg(&small_clean, 50),
            clean: small_clean,
            text: Some(capitals_box(40.0, 120.0, 10.0)),
        },
    ]
}

/// The presets measured: the outline ones a logo is traced with.
const PRESETS: [Preset; 4] = [
    Preset::Logo,
    Preset::DetailedLogo,
    Preset::Clipart,
    Preset::Optimized,
];

/// The measures of one trace.
#[derive(Debug, Clone, Copy)]
struct Measures {
    curves: usize,
    nodes: usize,
    colors: usize,
    wrong: f64,
    halos: usize,
    text: Option<f64>,
    shift: f64,
}

/// `img` cut to `b` (x0, y0, x1, y1).
fn crop(img: &RgbaImage, b: [u32; 4]) -> RgbaImage {
    image::imageops::crop_imm(img, b[0], b[1], b[2] - b[0], b[3] - b[1]).to_image()
}

/// The centre of the ink (pixels darker or more coloured than near
/// white) of `img`, weighted by how far each is from white.
fn ink_centre(img: &RgbaImage) -> (f64, f64) {
    let (mut sx, mut sy, mut sw) = (0.0, 0.0, 0.0);
    for (x, y, p) in img.enumerate_pixels() {
        let a = p[3] as f64 / 255.0;
        let ink = (0..3).map(|k| 255.0 - p[k] as f64).sum::<f64>() * a / 765.0;
        if ink > 0.1 {
            sx += x as f64 * ink;
            sy += y as f64 * ink;
            sw += ink;
        }
    }
    if sw == 0.0 {
        (0.0, 0.0)
    } else {
        (sx / sw, sy / sw)
    }
}

/// Tiny closed shapes: under 12 square pixels of bounding box.
fn halos(traced: &[Traced]) -> usize {
    use kurbo::Shape;
    traced
        .iter()
        .filter(|t| t.closed && t.path.bounding_box().area() < 12.0)
        .count()
}

fn measure(f: &Fixture, preset: Preset) -> (Measures, RgbaImage) {
    let s = preset.settings();
    let t = trace(&f.input, &s);
    let st = stats(&t);
    let out = render_traced(&t, SIDE, SIDE, 1.0);
    let wrong = wrong_share(&f.clean, &out, None).unwrap_or(100.0);
    let text = f
        .text
        .map(|b| wrong_share(&crop(&f.clean, b), &crop(&out, b), None).unwrap_or(100.0));
    let (a, b) = (ink_centre(&f.clean), ink_centre(&out));
    let shift = ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt();
    (
        Measures {
            curves: st.curves,
            nodes: st.nodes,
            colors: st.colors,
            wrong,
            halos: halos(&t),
            text,
            shift,
        },
        out,
    )
}

/// The measures kept as the baseline: fixture, preset, nodes, wrong
/// share (percent), halos, text wrong share (percent, or -1 without
/// text). A run fails when it gets clearly worse (see [`worse`]).
#[rustfmt::skip]
const BASELINE: &[(&str, Preset, usize, f64, usize, f64)] = &[
    ("jpeg-logo-q35", Preset::Logo, 121, 0.12, 1, -1.00),
    ("jpeg-logo-q35", Preset::DetailedLogo, 179, 0.14, 0, -1.00),
    ("jpeg-logo-q35", Preset::Clipart, 86, 0.18, 0, -1.00),
    ("jpeg-logo-q35", Preset::Optimized, 98, 0.12, 0, -1.00),
    ("thin-black-art", Preset::Logo, 100, 0.26, 0, 2.12),
    ("thin-black-art", Preset::DetailedLogo, 162, 0.22, 0, 2.25),
    ("thin-black-art", Preset::Clipart, 97, 0.32, 0, 2.00),
    ("thin-black-art", Preset::Optimized, 164, 0.21, 0, 2.19),
    ("flat-colour-aa", Preset::Logo, 63, 0.10, 0, -1.00),
    ("flat-colour-aa", Preset::DetailedLogo, 68, 0.13, 0, -1.00),
    ("flat-colour-aa", Preset::Clipart, 60, 0.10, 0, -1.00),
    ("flat-colour-aa", Preset::Optimized, 77, 0.13, 0, -1.00),
    ("dark-emblem", Preset::Logo, 104, 0.15, 0, 4.24),
    ("dark-emblem", Preset::DetailedLogo, 208, 0.15, 1, 3.18),
    ("dark-emblem", Preset::Clipart, 73, 0.17, 0, 4.44),
    ("dark-emblem", Preset::Optimized, 82, 0.15, 0, 4.19),
    ("grey-gradient-backdrop", Preset::Logo, 737, 0.39, 0, -1.00),
    ("grey-gradient-backdrop", Preset::DetailedLogo, 826, 0.06, 0, -1.00),
    ("grey-gradient-backdrop", Preset::Clipart, 409, 0.06, 1, -1.00),
    ("grey-gradient-backdrop", Preset::Optimized, 375, 0.07, 1, -1.00),
    ("smooth-gradients", Preset::Logo, 124, 0.09, 0, -1.00),
    ("smooth-gradients", Preset::DetailedLogo, 357, 0.03, 0, -1.00),
    ("smooth-gradients", Preset::Clipart, 72, 0.10, 0, -1.00),
    ("smooth-gradients", Preset::Optimized, 72, 0.10, 0, -1.00),
    ("drop-shadow", Preset::Logo, 41, 1.49, 0, -1.00),
    ("drop-shadow", Preset::DetailedLogo, 43, 1.47, 0, -1.00),
    ("drop-shadow", Preset::Clipart, 40, 1.50, 0, -1.00),
    ("drop-shadow", Preset::Optimized, 41, 1.48, 0, -1.00),
    ("small-capitals-q50", Preset::Logo, 116, 0.21, 0, 8.62),
    ("small-capitals-q50", Preset::DetailedLogo, 131, 0.19, 0, 7.00),
    ("small-capitals-q50", Preset::Clipart, 105, 0.26, 0, 10.45),
    ("small-capitals-q50", Preset::Optimized, 228, 0.25, 0, 10.02),
];

/// Why `m` is clearly worse than its baseline, if it is: wrong share up
/// by more than 1 point and a quarter, text wrong share up by more than 2
/// points and a quarter, nodes up by more than 30 % and 20, halos up by
/// more than 50 % and 5, or the ink shifted more than 1.5 pixels.
fn worse(m: &Measures, nodes: usize, wrong: f64, halos: usize, text: f64) -> Option<String> {
    let mut why = Vec::new();
    if m.wrong > wrong * 1.25 + 1.0 {
        why.push(format!("wrong {:.2}% (was {wrong:.2}%)", m.wrong));
    }
    if let Some(t) = m.text.filter(|_| text >= 0.0) {
        if t > text * 1.25 + 2.0 {
            why.push(format!("text wrong {t:.2}% (was {text:.2}%)"));
        }
    }
    if m.nodes as f64 > nodes as f64 * 1.3 + 20.0 {
        why.push(format!("{} nodes (was {nodes})", m.nodes));
    }
    if m.halos as f64 > halos as f64 * 1.5 + 5.0 {
        why.push(format!("{} halos (was {halos})", m.halos));
    }
    if m.shift > 1.5 {
        why.push(format!("ink shifted {:.2} px", m.shift));
    }
    (!why.is_empty()).then(|| why.join(", "))
}

/// The input, the clean picture and the trace side by side.
fn side_by_side(f: &Fixture, out: &RgbaImage) -> RgbaImage {
    let mut all = RgbaImage::from_pixel(SIDE * 3 + 20, SIDE, Rgba([200, 200, 200, 255]));
    let mut flat = RgbaImage::from_pixel(SIDE, SIDE, Rgba([255, 255, 255, 255]));
    image::imageops::overlay(&mut flat, out, 0, 0);
    image::imageops::overlay(&mut all, &f.input, 0, 0);
    image::imageops::overlay(&mut all, &f.clean, (SIDE + 10) as i64, 0);
    image::imageops::overlay(&mut all, &flat, (2 * SIDE + 20) as i64, 0);
    all
}

/// Every fixture through every preset, measured; the report written to
/// `TRACE_BENCH_OUT` when it is set, and no measure clearly worse than
/// its baseline.
#[test]
fn trace_quality_benchmark() {
    let out_dir = std::env::var_os("TRACE_BENCH_OUT").map(std::path::PathBuf::from);
    let fixtures = fixtures();
    let runs: Vec<(usize, Preset)> = (0..fixtures.len())
        .flat_map(|i| PRESETS.iter().map(move |p| (i, *p)))
        .collect();
    // The runs share out over the processors.
    let threads = std::thread::available_parallelism().map_or(2, |n| n.get()).min(8);
    let results: Vec<(usize, Preset, Measures, RgbaImage)> = std::thread::scope(|scope| {
        let chunks: Vec<Vec<(usize, Preset)>> = (0..threads)
            .map(|k| runs.iter().copied().skip(k).step_by(threads).collect())
            .collect();
        let fixtures = &fixtures;
        let handles: Vec<_> = chunks
            .into_iter()
            .map(|chunk| {
                scope.spawn(move || {
                    chunk
                        .into_iter()
                        .map(|(i, p)| {
                            let (m, img) = measure(&fixtures[i], p);
                            (i, p, m, img)
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        let mut all: Vec<_> = handles
            .into_iter()
            .flat_map(|h| h.join().expect("a run"))
            .collect();
        all.sort_by_key(|(i, p, _, _)| (*i, PRESETS.iter().position(|q| q == p)));
        all
    });
    let mut report = String::from(
        "| fixture | preset | curves | nodes | colours | wrong % | halos | text wrong % | shift px |\n|---|---|---|---|---|---|---|---|---|\n",
    );
    let mut baseline = String::new();
    let mut failures = Vec::new();
    for (i, p, m, img) in &results {
        let f = &fixtures[*i];
        let text = m.text.map_or("-".to_string(), |t| format!("{t:.2}"));
        report.push_str(&format!(
            "| {} | {:?} | {} | {} | {} | {:.2} | {} | {} | {:.2} |\n",
            f.name, p, m.curves, m.nodes, m.colors, m.wrong, m.halos, text, m.shift
        ));
        baseline.push_str(&format!(
            "    (\"{}\", Preset::{:?}, {}, {:.2}, {}, {:.2}),\n",
            f.name,
            p,
            m.nodes,
            m.wrong,
            m.halos,
            m.text.unwrap_or(-1.0)
        ));
        if let Some(b) = BASELINE.iter().find(|b| b.0 == f.name && b.1 == *p) {
            if let Some(why) = worse(m, b.2, b.3, b.4, b.5) {
                failures.push(format!("{} / {p:?}: {why}", f.name));
            }
        }
        if let Some(dir) = &out_dir {
            std::fs::create_dir_all(dir).expect("report folder");
            side_by_side(f, img)
                .save(dir.join(format!("{}-{p:?}.png", f.name)))
                .expect("render");
        }
    }
    if let Some(dir) = &out_dir {
        std::fs::write(dir.join("report.md"), &report).expect("report");
        std::fs::write(dir.join("baseline.txt"), &baseline).expect("baseline");
    }
    println!("{report}");
    // Every fixture and preset has its baseline.
    let missing: Vec<String> = results
        .iter()
        .filter(|(i, p, _, _)| !BASELINE.iter().any(|b| b.0 == fixtures[*i].name && b.1 == *p))
        .map(|(i, p, _, _)| format!("{} / {p:?}", fixtures[*i].name))
        .collect();
    assert!(missing.is_empty(), "no baseline for {missing:?}:\n{baseline}");
    assert!(failures.is_empty(), "worse than the baseline:\n{}", failures.join("\n"));
}

/// The measures themselves: a perfect trace of the clean picture is not
/// wrong and does not shift; a lost shape is wrong; a tiny shape is a
/// halo.
#[test]
fn the_benchmark_measures_what_it_says() {
    use kurbo::Shape;
    let f = &fixtures()[0];
    assert_eq!(wrong_share(&f.clean, &f.clean, None), Some(0.0));
    let (a, b) = (ink_centre(&f.clean), ink_centre(&f.clean));
    assert_eq!(a, b);
    let blank = RgbaImage::from_pixel(SIDE, SIDE, Rgba([255, 255, 255, 255]));
    assert!(wrong_share(&f.clean, &blank, None).unwrap() > 20.0);
    let dot = Traced {
        path: kurbo::Rect::new(10.0, 10.0, 12.0, 12.0).to_path(0.1),
        color: tracedraw_core::Color::rgb8(0, 0, 0),
        closed: true,
        blend: None,
    };
    assert_eq!(halos(&[dot]), 1);
    // A clean measure is never worse than itself.
    let m = Measures {
        curves: 3,
        nodes: 100,
        colors: 3,
        wrong: 1.0,
        halos: 2,
        text: Some(3.0),
        shift: 0.2,
    };
    assert!(worse(&m, 100, 1.0, 2, 3.0).is_none());
    assert!(worse(&Measures { wrong: 5.0, ..m }, 100, 1.0, 2, 3.0).is_some());
    assert!(worse(&Measures { nodes: 200, ..m }, 100, 1.0, 2, 3.0).is_some());
}
