//! Clean up a pencil sketch before it is traced (the trace dialog's
//! "Clean up a pencil sketch", off at first): the pencil lines are found
//! against the paper's own shade, the hatching and the doubled outlines
//! of each letter close into one solid shape, the construction lines and
//! specks too thin to be part of a shape go, and the edges are smoothed;
//! what is left is traced black on white. No machine learning: only
//! distances and blurs on the pixels. See docs/behavior/trace-sketch.md.

use crate::trace::{ColorMode, Remove, Settings};
use image::{Rgba, RgbaImage};

/// The settings a cleaned sketch is traced with: black on white, the
/// white left out, one curve for the black.
pub fn sketch_settings(s: &Settings) -> Settings {
    Settings {
        clean_sketch: false,
        mode: ColorMode::BlackWhite,
        colors: 2,
        remove: Remove::Background,
        background: Some([255, 255, 255]),
        merge_adjacent: true,
        merged: Vec::new(),
        recolored: Vec::new(),
        ..s.clone()
    }
}

/// The lightness of each pixel, 0..255, over white where it is see-through.
fn lightness(img: &RgbaImage) -> Vec<f32> {
    img.pixels()
        .map(|p| {
            let a = p[3] as f32 / 255.0;
            let l = 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32;
            l * a + 255.0 * (1.0 - a)
        })
        .collect()
}

/// `v` blurred by a box `r` pixels each side, three times (close to a
/// Gaussian), rows then columns.
fn blur(v: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    if r == 0 || w == 0 || h == 0 {
        return v.to_vec();
    }
    let mut a = v.to_vec();
    let mut b = vec![0.0f32; v.len()];
    let pass = |src: &[f32],
                dst: &mut [f32],
                n: usize,
                lines: usize,
                at: &dyn Fn(usize, usize) -> usize| {
        for line in 0..lines {
            // A running sum over the window, the ends repeated.
            let get = |i: isize| src[at(line, i.clamp(0, n as isize - 1) as usize)];
            let mut sum = 0.0f32;
            for i in -(r as isize)..=(r as isize) {
                sum += get(i);
            }
            let k = (2 * r + 1) as f32;
            for i in 0..n {
                dst[at(line, i)] = sum / k;
                sum += get(i as isize + r as isize + 1) - get(i as isize - r as isize);
            }
        }
    };
    for _ in 0..3 {
        pass(&a, &mut b, w, h, &|y, x| y * w + x);
        pass(&b, &mut a, h, w, &|x, y| y * w + x);
    }
    a
}

/// The squared distance of each pixel to the nearest pixel where `set`
/// is true (exact Euclidean, two passes of the lower envelope of
/// parabolas); very large where there is none.
fn distance2(set: &[bool], w: usize, h: usize) -> Vec<f32> {
    const FAR: f32 = 1e20;
    let mut d: Vec<f32> = set.iter().map(|s| if *s { 0.0 } else { FAR }).collect();
    let mut f = vec![0.0f32; w.max(h)];
    let mut out = vec![0.0f32; w.max(h)];
    let mut v = vec![0usize; w.max(h)];
    let mut z = vec![0.0f32; w.max(h) + 1];
    let mut one = |f: &[f32], n: usize, out: &mut [f32]| {
        let mut k = 0usize;
        v[0] = 0;
        z[0] = f32::NEG_INFINITY;
        z[1] = f32::INFINITY;
        for q in 1..n {
            loop {
                let p = v[k];
                let s =
                    ((f[q] + (q * q) as f32) - (f[p] + (p * p) as f32)) / (2.0 * (q - p) as f32);
                if s <= z[k] && k > 0 {
                    k -= 1;
                    continue;
                }
                if s <= z[k] {
                    // k == 0: the first parabola is replaced.
                    v[0] = q;
                    z[0] = f32::NEG_INFINITY;
                    z[1] = f32::INFINITY;
                    break;
                }
                k += 1;
                v[k] = q;
                z[k] = s;
                z[k + 1] = f32::INFINITY;
                break;
            }
        }
        k = 0;
        for q in 0..n {
            while z[k + 1] < q as f32 {
                k += 1;
            }
            let p = v[k];
            let dq = q as f32 - p as f32;
            out[q] = dq * dq + f[p];
        }
    };
    for x in 0..w {
        for y in 0..h {
            f[y] = d[y * w + x];
        }
        one(&f, h, &mut out);
        for y in 0..h {
            d[y * w + x] = out[y];
        }
    }
    for y in 0..h {
        f[..w].copy_from_slice(&d[y * w..y * w + w]);
        one(&f, w, &mut out);
        d[y * w..y * w + w].copy_from_slice(&out[..w]);
    }
    d
}

/// `m` grown by `r` pixels (a disc).
fn dilate(m: &[bool], w: usize, h: usize, r: f32) -> Vec<bool> {
    distance2(m, w, h).iter().map(|d| *d <= r * r).collect()
}

/// `m` shrunk by `r` pixels (a disc).
fn erode(m: &[bool], w: usize, h: usize, r: f32) -> Vec<bool> {
    let inv: Vec<bool> = m.iter().map(|b| !b).collect();
    distance2(&inv, w, h).iter().map(|d| *d > r * r).collect()
}

/// The 4-connected patches of `m` where it is `on`: for each pixel its
/// patch number (or `usize::MAX`), and each patch's size and whether it
/// touches the picture's edge.
fn patches(m: &[bool], w: usize, h: usize, on: bool) -> (Vec<usize>, Vec<(usize, bool)>) {
    let mut label = vec![usize::MAX; m.len()];
    let mut info = Vec::new();
    let mut stack = Vec::new();
    for start in 0..m.len() {
        if m[start] != on || label[start] != usize::MAX {
            continue;
        }
        let id = info.len();
        let (mut size, mut edge) = (0usize, false);
        label[start] = id;
        stack.push(start);
        while let Some(i) = stack.pop() {
            size += 1;
            let (x, y) = (i % w, i / w);
            if x == 0 || y == 0 || x + 1 == w || y + 1 == h {
                edge = true;
            }
            let mut push = |j: usize| {
                if m[j] == on && label[j] == usize::MAX {
                    label[j] = id;
                    stack.push(j);
                }
            };
            if x > 0 {
                push(i - 1);
            }
            if x + 1 < w {
                push(i + 1);
            }
            if y > 0 {
                push(i - w);
            }
            if y + 1 < h {
                push(i + w);
            }
        }
        info.push((size, edge));
    }
    (label, info)
}

/// The sketch `img` cleaned: solid black shapes on white.
pub fn clean_sketch(img: &RgbaImage) -> RgbaImage {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let mut out = RgbaImage::from_pixel(img.width(), img.height(), Rgba([255, 255, 255, 255]));
    if w < 3 || h < 3 {
        return out;
    }
    // The measure everything is taken in: about a three-hundredth of the
    // picture's longer side, as a pencil line on a scanned sheet.
    let unit = (w.max(h) as f32 / 300.0).max(1.0);
    // 1. The pencil: darker than the paper around it (its own shade,
    //    which changes over a photographed sheet).
    let l = lightness(img);
    let paper = blur(&l, w, h, (unit * 8.0) as usize);
    let ink: Vec<bool> = l
        .iter()
        .zip(&paper)
        .map(|(v, p)| *v < p - 18.0 && *v < p * 0.9)
        .collect();
    // 2. Hatching and doubled outlines close into solid shapes.
    let close = unit * 2.5;
    let mut m = erode(&dilate(&ink, w, h, close), w, h, close);
    // Small holes left inside a shape fill; a letter's own counter, much
    // larger, stays open.
    let (label, info) = patches(&m, w, h, false);
    let hole = (unit * 6.0).powi(2) as usize;
    for (i, px) in m.iter_mut().enumerate() {
        if let Some((size, edge)) = info.get(label[i]) {
            if !*px && !*edge && *size < hole {
                *px = true;
            }
        }
    }
    // 3. Lines too thin to be a shape (construction lines, guides, loose
    //    strokes) go; then specks.
    let open = unit * 1.5;
    m = dilate(&erode(&m, w, h, open), w, h, open);
    let (label, info) = patches(&m, w, h, true);
    let speck = (unit * 5.0).powi(2) as usize;
    for (i, px) in m.iter_mut().enumerate() {
        if let Some((size, _)) = info.get(label[i]) {
            if *px && *size < speck {
                *px = false;
            }
        }
    }
    // 4. Smooth edges: the shapes blurred and cut at half way.
    let soft: Vec<f32> = m.iter().map(|b| if *b { 1.0 } else { 0.0 }).collect();
    let soft = blur(&soft, w, h, (unit * 0.75).round().max(1.0) as usize);
    for (i, v) in soft.iter().enumerate() {
        if *v >= 0.5 {
            out.put_pixel((i % w) as u32, (i / w) as u32, Rgba([0, 0, 0, 255]));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::Shape as _;

    /// A sketch of a thick letter "I" drawn as two outlines with
    /// hatching between them, a thin guide line across the page and a
    /// few specks: cleaned, the letter is one solid black bar and the
    /// guide and the specks are gone.
    #[test]
    fn a_hatched_letter_becomes_solid_and_the_guides_go() {
        let (w, h) = (600u32, 600u32);
        let mut img = RgbaImage::from_pixel(w, h, Rgba([235, 230, 225, 255]));
        let grey = Rgba([90, 90, 90, 255]);
        let mut dot = |x: i64, y: i64| {
            if x >= 0 && y >= 0 && (x as u32) < w && (y as u32) < h {
                img.put_pixel(x as u32, y as u32, grey);
            }
        };
        // The letter: a bar 200..280 across, 100..500 down; its outline
        // drawn twice, 3 pixels apart, 2 wide.
        for off in [0i64, 3] {
            for t in 0..2 {
                let (x0, x1, y0, y1) = (200 + off + t, 280 - off - t, 100 + off + t, 500 - off - t);
                for x in x0..=x1 {
                    dot(x, y0);
                    dot(x, y1);
                }
                for y in y0..=y1 {
                    dot(x0, y);
                    dot(x1, y);
                }
            }
        }
        // Hatching inside: slanted lines 6 pixels apart.
        for k in (-80i64..400).step_by(6) {
            for x in 206..275i64 {
                let y = 106 + k + (x - 206);
                if (106..495).contains(&y) {
                    dot(x, y);
                    dot(x, y + 1);
                }
            }
        }
        // A thin guide line across the page, and specks.
        for x in 0..600i64 {
            dot(x, 50);
        }
        for (x, y) in [(450i64, 300i64), (500, 520), (80, 400)] {
            dot(x, y);
            dot(x + 1, y);
        }
        let clean = clean_sketch(&img);
        let black = |x: u32, y: u32| clean.get_pixel(x, y)[0] == 0;
        // The letter: solid inside, also between hatch lines.
        let mut inside = 0;
        let mut black_inside = 0;
        for y in (120..480).step_by(7) {
            for x in (215..265).step_by(3) {
                inside += 1;
                black_inside += black(x, y) as usize;
            }
        }
        assert!(
            black_inside * 100 / inside >= 97,
            "{black_inside} of {inside}"
        );
        // Outside it: white, the guide and the specks gone.
        for x in (0..600).step_by(10) {
            assert!(!black(x, 50), "guide at {x}");
        }
        for (x, y) in [(450, 300), (500, 520), (80, 400), (100, 300), (350, 300)] {
            assert!(!black(x, y), "speck at {x},{y}");
        }
        // The letter keeps its size.
        assert!(black(205, 300) && black(275, 300));
        assert!(!black(190, 300) && !black(290, 300));
    }

    /// Traced with the option on, a hatched letter is one black curve.
    #[test]
    fn a_cleaned_sketch_traces_as_black_shapes() {
        let mut img = RgbaImage::from_pixel(300, 300, Rgba([240, 238, 235, 255]));
        for y in 60..240u32 {
            for x in 100..160u32 {
                // Outline and hatching only, in grey pencil.
                let edge = x < 102 || x > 157 || y < 62 || y > 237;
                if edge || (x + y) % 5 == 0 {
                    img.put_pixel(x, y, Rgba([100, 100, 100, 255]));
                }
            }
        }
        let mut s = crate::trace::Preset::Logo.settings();
        s.clean_sketch = true;
        let traced = crate::trace::trace(&img, &s);
        assert!(!traced.is_empty());
        assert!(
            traced.iter().all(|t| t.color.to_rgb8() == [0, 0, 0]),
            "{traced:?}"
        );
        let b = traced
            .iter()
            .map(|t| t.path.bounding_box())
            .reduce(|a, b| a.union(b))
            .expect("a box");
        assert!(
            (b.width() - 60.0).abs() < 6.0 && (b.height() - 180.0).abs() < 8.0,
            "{b:?}"
        );
    }

    /// The exact distances: a point's squared distance to a lone set
    /// pixel.
    #[test]
    fn distances_are_exact() {
        let (w, h) = (9, 7);
        let mut set = vec![false; w * h];
        set[3 * w + 4] = true;
        let d = distance2(&set, w, h);
        assert_eq!(d[3 * w + 4], 0.0);
        assert_eq!(d[0], 16.0 + 9.0);
        assert_eq!(d[6 * w + 8], 16.0 + 9.0);
        assert_eq!(d[3 * w], 16.0);
    }
}
