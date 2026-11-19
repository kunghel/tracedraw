//! The fine part of the outline trace: where the edge between two colours
//! runs inside the pixels it crosses, and the lines, curves and corners
//! fitted to it. See docs/behavior/bitmap-tracing.md, "Fine edges".
//!
//! The pixels along an edge hold a blend of the two colours beside it,
//! and the share of each says where in the pixel the edge runs. The
//! labels of the picture are made again on a grid `k` times finer from
//! the picture's colours laid between its pixels, and the borders are
//! walked and fitted on that grid: the steps of the pixel grid are gone
//! before any curve is fitted, so no smoothing has to melt them away.

use crate::trace::{cancelled, fit_cubics, Cancel, Settings};
use tracedraw_core::geometry::{BezPath, Point, Vec2};

/// The most pixels the finer grid may have.
const FINE_PIXELS: usize = 6_000_000;

/// How many times finer than the picture the grid of its edges is: 4 for
/// a small picture, less as it grows, 1 (the pixels themselves) when it
/// is large enough for its own pixels to say where the edges run.
pub(crate) fn scale_for(w: usize, h: usize) -> usize {
    let k = ((FINE_PIXELS as f64) / ((w * h).max(1) as f64))
        .sqrt()
        .floor() as usize;
    k.clamp(1, 4)
}

fn dist2(a: [f32; 3], b: [u8; 3]) -> f32 {
    (0..3).map(|c| (a[c] - b[c] as f32).powi(2)).sum()
}

/// The most steps from a pixel up or down to the flat colour or the
/// line's core its edge leads to.
const CLIMB: usize = 6;
/// How far apart (RGB distance) the two ends of a pixel's edge must be.
const EDGE_APART: f32 = 48.0;
/// The least a step of the walk must gain in brightness (thousandths of
/// a level): less is the slope of a gradient or noise, not an edge.
const RISE: i32 = 5000;
/// How many times more than the brightness a single channel must change
/// around a pixel for its edge to be walked by that channel.
const CHANNEL_OVER: f64 = 1.5;

/// For each pixel on an edge, the two pixels the edge runs between and
/// how far from the first to the second the pixel's colour lies.
///
/// Brightness rises steadily across the blurred edge of two colours (or,
/// where the two are as bright as each other, one of red, green and
/// blue does), so
/// walking from a pixel to its brightest neighbour again and again ends
/// in the flat colour on the bright side (or on the core of a bright
/// line thinner than the blur), and to the darkest neighbour in the one
/// on the dark side. A pixel both walks of which end within [`CLIMB`]
/// steps, at colours at least [`EDGE_APART`] apart with its own between
/// them, is a blend of those two and nothing else: no third colour that
/// happens to look like the blend can claim it, and a faint thin line is
/// measured against its own core, not against the full colour it never
/// reaches.
pub(crate) struct Edges {
    /// (dark end, bright end, share of the bright end); `u32::MAX` ends
    /// for a pixel on no edge.
    of: Vec<(u32, u32, f32)>,
}

impl Edges {
    pub(crate) fn none(len: usize) -> Edges {
        Edges {
            of: vec![(u32::MAX, u32::MAX, 0.0); len],
        }
    }

    pub(crate) fn at(&self, i: usize) -> Option<(usize, usize, f32)> {
        let (lo, hi, t) = *self.of.get(i)?;
        (lo != u32::MAX).then_some((lo as usize, hi as usize, t))
    }

    pub(crate) fn find(
        colors: &[[u8; 3]],
        alpha: &[u8],
        w: usize,
        h: usize,
        cancel: Option<&Cancel>,
    ) -> Option<Edges> {
        let n = w * h;
        let mut out = Edges::none(n);
        if colors.len() != n || alpha.len() != n || w < 3 || h < 3 {
            return Some(out);
        }
        // The measure a pixel's walk climbs by: brightness, or the red,
        // green or blue channel alone where that changes much more around
        // the pixel (two colours of one brightness, a purple beside a
        // blue, have an edge brightness does not see), in thousandths of
        // a level.
        let value = |j: usize, by: usize| -> i32 {
            let c = colors[j];
            match by {
                0 => c[0] as i32 * 299 + c[1] as i32 * 587 + c[2] as i32 * 114,
                k => c[k - 1] as i32 * 1000,
            }
        };
        let around = |i: usize| {
            let (x, y) = (i % w, i / w);
            (y.saturating_sub(1)..=(y + 1).min(h - 1))
                .flat_map(move |v| {
                    (x.saturating_sub(1)..=(x + 1).min(w - 1)).map(move |u| v * w + u)
                })
                .filter(|j| alpha[*j] >= 128)
        };
        let measure = |i: usize| -> usize {
            let mut range = [0i32; 4];
            for (by, r) in range.iter_mut().enumerate() {
                let (mut lo, mut hi) = (i32::MAX, i32::MIN);
                for j in around(i) {
                    let v = value(j, by);
                    lo = lo.min(v);
                    hi = hi.max(v);
                }
                *r = hi - lo;
            }
            let best = (1..4).max_by_key(|by| range[*by]).unwrap_or(0);
            if range[best] as f64 > CHANNEL_OVER * range[0] as f64 {
                best
            } else {
                0
            }
        };
        // The end of a pixel's walk up or down by a measure: none when
        // it has not ended within [`CLIMB`] steps.
        let end = |start: usize, by: usize, up: bool| -> Option<usize> {
            let mut i = start;
            for _ in 0..=CLIMB {
                let here = value(i, by);
                let (mut best, mut next) = (here, i);
                for j in around(i) {
                    let v = value(j, by);
                    if up && v > best && v >= here + RISE || !up && v < best && v <= here - RISE {
                        best = v;
                        next = j;
                    }
                }
                if next == i {
                    return Some(i);
                }
                i = next;
            }
            None
        };
        // The edge pixel `i` is on, if any.
        let edge_of = |i: usize| -> Option<(u32, u32, f32)> {
            if alpha[i] < 128 {
                return None;
            }
            let by = measure(i);
            let (hi, lo) = (end(i, by, true)?, end(i, by, false)?);
            if hi == lo {
                return None;
            }
            let f = |c: [u8; 3]| [c[0] as f32, c[1] as f32, c[2] as f32];
            let (a, b, c) = (f(colors[lo]), f(colors[hi]), f(colors[i]));
            let ab: [f32; 3] = std::array::from_fn(|k| b[k] - a[k]);
            let ac: [f32; 3] = std::array::from_fn(|k| c[k] - a[k]);
            let len2: f32 = ab.iter().map(|v| v * v).sum();
            if len2 < EDGE_APART * EDGE_APART {
                return None;
            }
            let t = (ab.iter().zip(&ac).map(|(p, q)| p * q).sum::<f32>() / len2).clamp(0.0, 1.0);
            let off2: f32 = (0..3).map(|k| (ac[k] - t * ab[k]).powi(2)).sum();
            let slack = (0.12 * len2.sqrt()).max(16.0);
            if off2 > slack * slack {
                return None;
            }
            Some((lo as u32, hi as u32, t))
        };
        // Each pixel on its own: bands of rows on the computer's cores.
        crate::trace::par_chunks(&mut out.of, w * 64, |start, part| {
            for (k, at) in part.iter_mut().enumerate() {
                if k % 4096 == 0 && cancelled(cancel) {
                    return;
                }
                if let Some(e) = edge_of(start + k) {
                    *at = e;
                }
            }
        });
        if cancelled(cancel) {
            return None;
        }
        Some(out)
    }
}

/// The share of the borders in the picture drawn without anti-aliasing
/// for the picture to count as drawn so.
const HARD_SHARE: f64 = 0.7;

/// Whether the picture's edges are drawn hard, with no pixels blending
/// the two colours (pixel art, a one-bit scan, a drawing saved without
/// anti-aliasing): of the places two neighbouring pixels have different
/// labels, at least [`HARD_SHARE`] have a pixel of full colour on both
/// sides. Such edges say nothing of where they run inside a pixel: they
/// are stair steps, and only smoothing makes slopes of them.
pub(crate) fn hard_edges(labels: &[usize], edges: &Edges, w: usize, h: usize) -> bool {
    if labels.len() != w * h {
        return false;
    }
    let full = |i: usize| {
        edges
            .at(i)
            .is_none_or(|(_, _, t)| !(0.1..=0.9).contains(&t))
    };
    let (mut borders, mut hard) = (0usize, 0usize);
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            for j in [(x + 1 < w).then(|| i + 1), (y + 1 < h).then(|| i + w)]
                .into_iter()
                .flatten()
            {
                if labels[i] != labels[j] && labels[i] != usize::MAX && labels[j] != usize::MAX {
                    borders += 1;
                    hard += (full(i) && full(j)) as usize;
                }
            }
        }
    }
    borders > 0 && hard as f64 >= HARD_SHARE * borders as f64
}

/// How much of the way from the colour `pa` to the colour `pb` a faint
/// end of an edge counts for at least (and its other end at most one
/// less this).
const FAINT: f32 = 0.75;
/// How near (RGB distance) an end must be to its label's colour to be of
/// that colour in full.
const FULL: f32 = 56.0;

/// How far from the colour `pa` (of the label at an edge's `lo` end) to
/// the colour `pb` (of the label at its `hi` end) a pixel on that edge
/// lies, `t` being its place between the colours of the two ends
/// themselves. The same, when both ends are of their labels' colours. An
/// end far from its label's colour is the core of a stroke thinner than
/// the blur, which never reaches its colour: it counts for as much of the
/// way as it lies, [`FAINT`] at least, so the stroke is about as wide as
/// the ink in it and a faint hairline is still drawn.
pub(crate) fn share(lo: [u8; 3], hi: [u8; 3], pa: [u8; 3], pb: [u8; 3], t: f32) -> f32 {
    let ab: [f32; 3] = std::array::from_fn(|k| pb[k] as f32 - pa[k] as f32);
    let len2: f32 = ab.iter().map(|v| v * v).sum();
    if len2 < 1.0 {
        return t;
    }
    let place = |x: [u8; 3]| -> f32 {
        (0..3)
            .map(|k| (x[k] as f32 - pa[k] as f32) * ab[k])
            .sum::<f32>()
            / len2
    };
    let from = if dist2(pa.map(|v| v as f32), lo) <= FULL * FULL {
        0.0
    } else {
        place(lo).clamp(0.0, 1.0 - FAINT)
    };
    let to = if dist2(pb.map(|v| v as f32), hi) <= FULL * FULL {
        1.0
    } else {
        place(hi).clamp(FAINT, 1.0)
    };
    (from + t * (to - from)).clamp(0.0, 1.0)
}

/// Give each pixel on an edge ([`Edges`]) the label of the end of the
/// edge its colour is nearer ([`share`]): `label_of` names the label of
/// the pixel at an end, given that pixel and the one at the other end.
pub(crate) fn settle_on_ends(
    labels: &mut [usize],
    edges: &Edges,
    colors: &[[u8; 3]],
    palette: &[[u8; 3]],
    label_of: impl Fn(usize, usize) -> usize,
) {
    for (i, l) in labels.iter_mut().enumerate() {
        if *l == usize::MAX {
            continue;
        }
        if let Some((lo, hi, t)) = edges.at(i) {
            let (la, lb) = (label_of(lo, hi), label_of(hi, lo));
            let t = match (palette.get(la), palette.get(lb)) {
                (Some(pa), Some(pb)) => share(colors[lo], colors[hi], *pa, *pb, t),
                _ => t,
            };
            *l = if t >= 0.5 { lb } else { la };
        }
    }
}

/// How many pixels blending two colours the picture has for each place
/// two neighbouring pixels have different labels: under one for edges
/// drawn with plain anti-aliasing (one blended pixel across the edge),
/// two and more for a picture blurred or scaled down softly.
pub(crate) fn softness(labels: &[usize], edges: &Edges, w: usize, h: usize) -> f64 {
    if labels.len() != w * h {
        return 0.0;
    }
    let (mut borders, mut mixed) = (0usize, 0usize);
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if labels[i] == usize::MAX {
                continue;
            }
            if edges
                .at(i)
                .is_some_and(|(_, _, t)| (0.1..=0.9).contains(&t))
            {
                mixed += 1;
            }
            for j in [(x + 1 < w).then(|| i + 1), (y + 1 < h).then(|| i + w)]
                .into_iter()
                .flatten()
            {
                borders += (labels[i] != labels[j] && labels[j] != usize::MAX) as usize;
            }
        }
    }
    mixed as f64 / borders.max(1) as f64
}

/// The colour each pixel stands for when the labels are made finer. On
/// an edge whose two ends have different labels, one of them the
/// pixel's: the blend of the two labels' colours by the pixel's share
/// ([`Edges`]), kept on its own label's side, so the fine edge runs where
/// the blend is half and half and a faint thin line comes out as wide as
/// it is. Else its own colour where its label is the one its colour is
/// nearest among the labels around it, and its label's colour, just on
/// its side of the edge, where the cleaning gave it another.
fn stand_ins(
    labels: &[usize],
    colors: &[[u8; 3]],
    edges: &Edges,
    palette: &[[u8; 3]],
    w: usize,
    h: usize,
) -> Vec<[f32; 3]> {
    let own = |i: usize| -> [f32; 3] { std::array::from_fn(|c| colors[i][c] as f32) };
    let mut out: Vec<[f32; 3]> = (0..w * h).map(own).collect();
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let l = labels[i];
            let around = (y.saturating_sub(1)..=(y + 1).min(h - 1))
                .flat_map(|v| (x.saturating_sub(1)..=(x + 1).min(w - 1)).map(move |u| v * w + u));
            if l == usize::MAX {
                // No colour of its own: the mean of the opaque pixels
                // round it, so it does not tint the edge beside it.
                let (mut sum, mut n) = ([0f32; 3], 0f32);
                for j in around {
                    if labels[j] != usize::MAX {
                        for c in 0..3 {
                            sum[c] += colors[j][c] as f32;
                        }
                        n += 1.0;
                    }
                }
                if n > 0.0 {
                    out[i] = sum.map(|v| v / n);
                }
                continue;
            }
            let Some(&mine) = palette.get(l) else {
                continue;
            };
            let full: [f32; 3] = std::array::from_fn(|c| mine[c] as f32);
            if let Some((lo, hi, t)) = edges.at(i) {
                let (la, lb) = (labels[lo], labels[hi]);
                if la != lb && (l == la || l == lb) {
                    if let (Some(pa), Some(pb)) = (palette.get(la), palette.get(lb)) {
                        let t = share(colors[lo], colors[hi], *pa, *pb, t);
                        let t = if l == lb { t.max(0.52) } else { t.min(0.48) };
                        out[i] =
                            std::array::from_fn(|c| pa[c] as f32 * (1.0 - t) + pb[c] as f32 * t);
                        continue;
                    }
                }
            }
            let c = own(i);
            let mut best = (dist2(c, mine), l);
            for j in around {
                let m = labels[j];
                if m == l || m == usize::MAX {
                    continue;
                }
                if let Some(p) = palette.get(m) {
                    let d = dist2(c, *p);
                    if d < best.0 {
                        best = (d, m);
                    }
                }
            }
            if best.1 == l {
                continue;
            }
            let other = palette[best.1];
            out[i] = std::array::from_fn(|c| 0.6 * full[c] + 0.4 * other[c] as f32);
        }
    }
    out
}

/// The softness ([`softness`]) of a picture with plain anti-aliasing,
/// which is not sharpened, and of one sharpened in full; and how much
/// that is.
const SOFT_FROM: f64 = 0.8;
const SOFT_FULL: f64 = 1.25;
const SHARPEN_MOST: f32 = 2.2;
/// The weights of a pixel, a neighbour beside it and one at its corner
/// in the blur the sharpening undoes (a deviation of three quarters of a
/// pixel).
const BLUR: [f32; 3] = [0.3012, 0.1238, 0.0509];

/// How soft a picture of that softness ([`softness`]) is, from 0 (plain
/// anti-aliasing or none) to 1.
pub(crate) fn soft_share(soft: f64) -> f64 {
    ((soft - SOFT_FROM) / (SOFT_FULL - SOFT_FROM)).clamp(0.0, 1.0)
}

/// Sharpen the colours the labels are made finer from: each moves away
/// from the blur of the colours around it by `amount` times its distance
/// from it. A picture scaled down softly has its thin strokes, the gaps
/// between them and its corners melted into each other; this gives them
/// back their contrast before the edges are placed, so the bowl of a
/// small letter is open and the arm of another stands clear of its
/// stem.
fn sharpen(stand: &mut [[f32; 3]], w: usize, h: usize, amount: f32) {
    if amount <= 0.0 || stand.len() != w * h || w < 3 || h < 3 {
        return;
    }
    let src = stand.to_vec();
    for y in 0..h {
        for x in 0..w {
            let mut blur = [0f32; 3];
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let u = (x as i32 + dx).clamp(0, w as i32 - 1) as usize;
                    let v = (y as i32 + dy).clamp(0, h as i32 - 1) as usize;
                    let weight = BLUR[(dx.abs() + dy.abs()) as usize];
                    for (b, c) in blur.iter_mut().zip(src[v * w + u]) {
                        *b += weight * c;
                    }
                }
            }
            let i = y * w + x;
            for k in 0..3 {
                stand[i][k] = src[i][k] + amount * (src[i][k] - blur[k]);
            }
        }
    }
}

/// The weights of the four samples round a place `t` of the way from the
/// second to the third (Catmull-Rom: the curve passes through the samples,
/// so the middle of a pixel keeps the pixel's own value).
fn weights(t: f32) -> [f32; 4] {
    let (t2, t3) = (t * t, t * t * t);
    [
        0.5 * (-t3 + 2.0 * t2 - t),
        0.5 * (3.0 * t3 - 5.0 * t2 + 2.0),
        0.5 * (-3.0 * t3 + 4.0 * t2 + t),
        0.5 * (t3 - t2),
    ]
}

/// The labels of a picture (`w` x `h`) on a grid `k` times finer: each
/// fine pixel takes, of the labels of the four pixels round it, the one
/// whose colour is nearest the picture's colour there (the pixels' colours
/// laid smoothly between them), so the edge between two colours runs
/// through the pixels blending them where the blend is half and half. A
/// pixel's middle keeps the pixel's label. `alpha` (the picture's, one
/// value a pixel) decides the same way where the picture ends. A soft
/// picture (`soft`, [`soft_share`]) is sharpened first ([`sharpen`]).
/// `None` once `cancel` is set.
#[allow(clippy::too_many_arguments)]
pub(crate) fn finer_labels(
    labels: &[usize],
    colors: &[[u8; 3]],
    alpha: &[u8],
    edges: &Edges,
    palette: &[[u8; 3]],
    w: usize,
    h: usize,
    k: usize,
    soft: f64,
    cancel: Option<&Cancel>,
) -> Option<Vec<usize>> {
    if k <= 1 || labels.len() != w * h || colors.len() != w * h || alpha.len() != w * h {
        return Some(labels.to_vec());
    }
    let mut stand = stand_ins(labels, colors, edges, palette, w, h);
    sharpen(&mut stand, w, h, soft as f32 * SHARPEN_MOST);
    let (fw, fh) = (w * k, h * k);
    let mut out = vec![usize::MAX; fw * fh];
    // Where each fine column and row lies between the pixels: the pixel
    // before it and how far on towards the next.
    let place = |q: usize, n: usize| -> (usize, usize, f32) {
        let u = (q as f32 + 0.5) / k as f32 - 0.5;
        let f = u.floor();
        let a = (f.max(0.0) as usize).min(n - 1);
        let b = ((f + 1.0).max(0.0) as usize).min(n - 1);
        (a, b, u - f)
    };
    let cols: Vec<(usize, usize, f32)> = (0..fw).map(|q| place(q, w)).collect();
    let clampi = |v: isize, n: usize| v.clamp(0, n as isize - 1) as usize;
    for fy in 0..fh {
        if fy % 64 == 0 && cancelled(cancel) {
            return None;
        }
        let (y0, y1, ty) = place(fy, h);
        let wy = weights(ty);
        let rows: [usize; 4] = std::array::from_fn(|n| clampi(y0 as isize + n as isize - 1, h));
        // At the picture's edge the place before the first row is the
        // first row itself: `ty` then counts from it.
        let rows = if (fy as f32 + 0.5) / (k as f32) < 0.5 {
            [0, 0, 0, 1.min(h - 1)]
        } else {
            rows
        };
        for (fx, &(x0, x1, tx)) in cols.iter().enumerate().take(fw) {
            let cell = [
                labels[y0 * w + x0],
                labels[y0 * w + x1],
                labels[y1 * w + x0],
                labels[y1 * w + x1],
            ];
            let o = fy * fw + fx;
            if cell[0] == cell[1] && cell[0] == cell[2] && cell[0] == cell[3] {
                out[o] = cell[0];
                continue;
            }
            if cell.contains(&usize::MAX) {
                let a = |x: usize, y: usize| alpha[y * w + x] as f32;
                let top = a(x0, y0) * (1.0 - tx) + a(x1, y0) * tx;
                let bottom = a(x0, y1) * (1.0 - tx) + a(x1, y1) * tx;
                if top * (1.0 - ty) + bottom * ty < 128.0 {
                    continue;
                }
            }
            let wx = weights(tx);
            let xs: [usize; 4] = if (fx as f32 + 0.5) / (k as f32) < 0.5 {
                [0, 0, 0, 1.min(w - 1)]
            } else {
                std::array::from_fn(|n| clampi(x0 as isize + n as isize - 1, w))
            };
            let mut c = [0f32; 3];
            for (r, wr) in rows.iter().zip(wy) {
                for (x, wc) in xs.iter().zip(wx) {
                    let s = stand[r * w + x];
                    for ch in 0..3 {
                        c[ch] += s[ch] * wr * wc;
                    }
                }
            }
            let mut best = (f32::MAX, usize::MAX);
            for l in cell {
                if let Some(p) = palette.get(l) {
                    let d = dist2(c, *p);
                    if d < best.0 || (d == best.0 && l < best.1) {
                        best = (d, l);
                    }
                }
            }
            out[o] = best.1;
        }
    }
    Some(out)
}

// ----- fitting -------------------------------------------------------------------

/// How a border of the finer grid is fitted. Lengths are in pixels of
/// the picture.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Fit {
    /// Fine pixels per pixel of the picture.
    pub k: f64,
    /// How much the border is smoothed (the deviation of the blur along
    /// it).
    pub sigma: f64,
    /// The turn that makes a corner, radians.
    pub corner: f64,
    /// How far the fitted curve may stray from the border.
    pub tolerance: f64,
    /// How much the border is smoothed before its corners are looked
    /// for.
    pub light: f64,
    /// The longest side of the polygon its corners are looked for on
    /// that is part of a rounded corner.
    pub rounded: f64,
    /// The widest radius of a corner the picture's blur rounded.
    pub round_most: f64,
    /// How far (fine pixels) the place two sides meet may lie from the
    /// rounded corner.
    pub reach: f64,
    /// How far from the border the two corners of a stroke's square end
    /// may lie, as a share of the end's width: infinite when any end
    /// found is taken.
    pub cap_near: f64,
}

/// The least smoothing of a picture drawn with hard edges
/// ([`hard_edges`]), for the stretches between corners and for finding
/// the corners: its borders are stair steps a pixel high.
const HARD_SIGMA: f64 = 1.4;
const HARD_LIGHT: f64 = 0.9;

/// How far (pixels) the border may stray from the polygon its corners
/// are looked for on.
const POLYGON: f64 = 0.15;
/// The longest side of that polygon (pixels) that is part of a corner
/// the picture's blur rounded off: two or three short sides in a row
/// are one corner.
const ROUNDED: f64 = 0.9;
/// The same in a soft picture, whose blur rounds corners further.
const ROUNDED_SOFT: f64 = 1.5;
/// How much a corner must stand out from an arc: its turn (radians)
/// times the longer side beside it (pixels). The sides of the polygon of
/// an arc are no longer than its radius allows, and this product is
/// never more than eight times [`POLYGON`] for them.
const CORNER_STANDS_OUT: f64 = 2.0;
/// A turn of more than this in a run of short sides is a round blob, no
/// corner.
const CORNER_MOST: f64 = 2.97;
/// The square end of a thin stroke, which the picture's blur made a half
/// circle: a turn between [`CAP_TURN`] and [`CAP_MOST`] over at most
/// [`CAP_SIDES`] sides no longer than [`CAP_SIDE`] (pixels), between two
/// sides each at least [`CAP_FLANK`] times the longest of those and a
/// pixel, running opposite ways no more than [`CAP_WIDE`] apart.
const CAP_TURN: f64 = 2.3;
const CAP_MOST: f64 = 3.6;
const CAP_SIDES: isize = 5;
const CAP_SIDE: f64 = 1.5;
const CAP_FLANK: f64 = 1.3;
const CAP_WIDE: f64 = 2.6;
/// A side beside a stroke's end shorter than this (pixels) and much
/// shorter than the other takes the other's direction.
const CAP_BRIEF: f64 = 2.5;
/// The two sides of a stroke's end run opposite ways: the cosine of
/// the angle between them is no more than this. Sides closing on each
/// other more than that are a wedge, sharp when they meet within
/// [`WEDGE_REACH`] pixels of its tip.
const CAP_ALIKE: f64 = -0.94;
const WEDGE_REACH: f64 = 2.2;
/// Both sides of a wedge are straight for this many pixels at least:
/// shorter ones, of a curved stroke's end, tell too little of where
/// they would meet.
const WEDGE_SIDE: f64 = 2.2;
/// The least turn (radians) of a vertex of the polygon in the core of a
/// corner.
const CORE_TURN: f64 = 0.44;
/// A run turning more than this is two corners, apart at its longest
/// side when that is [`SPLIT_STANDS_OUT`] times the next longest.
const SPLIT_TURN: f64 = 1.92;
const SPLIT_STANDS_OUT: f64 = 1.4;
/// The widest radius (pixels) the blur of a crisp picture and of a soft
/// one rounds a corner to.
const ROUND_MOST: f64 = 0.7;
const ROUND_MOST_SOFT: f64 = 1.2;
/// Between two level sides of [`ROUND_LONG_SIDE`] pixels or more the
/// rounding may be this many times wider.
const ROUND_LONG: f64 = 1.4;
const ROUND_LONG_SIDE: f64 = 2.0;
const STRAIGHT_SIDE: f64 = 4.0;
const STRAIGHT_ROUND: f64 = 1.6;
/// Two level sides are one when their lines are no further apart.
const SAME_LINE: f64 = 0.25;
/// The turn (radians) at the far end of a side for the side to be part
/// of an arc, and the most that turn times the side's length (pixels)
/// is for one.
const ARC_TURN: f64 = 0.26;
const ARC_MOST: f64 = 0.8;
const ARC_PRODUCT: f64 = 1.3;
/// The most a vertex of a bowl turns (radians), the longest side
/// between two of them (pixels) and the least a bowl turns in all.
const BOWL_TURN: f64 = 1.05;
const BOWL_SIDE: f64 = 1.5;
const BOWL_LEAST: f64 = 2.6;
/// The fewest vertices of a round ring, and how many times the side a
/// quarter of the way up from its shortest its longest may be.
const ROUND_RING: usize = 8;
const ROUND_RING_SIDES: f64 = 2.5;
/// The longest ring (pixels round) that has no corner when only one
/// was found on it, or in a picture with soft edges.
const LONE_CORNER_RING: f64 = 10.0;
/// How far (pixels) the place two sides meet may lie from the rounded
/// corner.
const CORNER_REACH: f64 = 1.6;
/// A side this near level or upright (pixels its far end is off by, and
/// radians) is taken for level or upright.
const AXIS_OFF: f64 = 0.15;
const AXIS_TURN: f64 = 0.14;
/// A side shorter than [`AXIS_SHORT`] pixels is level or upright from
/// further off ([`AXIS_SHORT_TURN`], radians): too short to say which
/// way it runs to a few degrees, and the sides of small print are level
/// and upright far more often than not.
const AXIS_SHORT: f64 = 4.0;
const AXIS_SHORT_TURN: f64 = 0.21;
/// A side shorter than this is made neither level nor upright.
const AXIS_LEAST: f64 = 0.9;
/// How near its side's line (pixels) the border must be for the
/// rounding of a corner to be over.
const JOINS: f64 = 0.08;
/// How much of a smoothing pass the pass after it undoes.
const PUSH_BACK: f64 = 1.06;
/// The most a ring without corners is grown to get its area back.
const GROW_MOST: f64 = 1.25;

impl Fit {
    pub(crate) fn of(s: &Settings, k: f64, hard: bool, soft: f64) -> Fit {
        let detail = s.detail.clamp(0.0, 100.0);
        let sigma = 0.3 + s.smoothing.clamp(0.0, 100.0) / 100.0 * 1.5;
        Fit {
            k,
            sigma: if hard { sigma.max(HARD_SIGMA) } else { sigma },
            light: if hard { HARD_LIGHT } else { 0.45 },
            corner: (35.0 + s.corner_smoothness.clamp(0.0, 100.0) * 1.3).to_radians(),
            tolerance: 0.35 + ((100.0 - detail) / 100.0).powf(1.5) * 0.6,
            rounded: ROUNDED + soft.clamp(0.0, 1.0) * (ROUNDED_SOFT - ROUNDED),
            round_most: ROUND_MOST + soft.clamp(0.0, 1.0) * (ROUND_MOST_SOFT - ROUND_MOST),
            reach: CORNER_REACH * k,
            cap_near: f64::INFINITY,
        }
    }
}

fn unit(v: Vec2) -> Vec2 {
    let l = v.hypot();
    if l > 1e-12 && l.is_finite() {
        v / l
    } else {
        Vec2::ZERO
    }
}

/// `pts` blurred along the border by `passes` rounds of quarter, half,
/// quarter; an open border keeps its two ends.
fn smooth(pts: &[Point], passes: usize, closed: bool) -> Vec<Point> {
    let n = pts.len();
    let mut cur = pts.to_vec();
    if n < 3 {
        return cur;
    }
    let mut prev = cur.clone();
    for _ in 0..passes {
        std::mem::swap(&mut cur, &mut prev);
        for i in 0..n {
            let (a, c) = if closed {
                (prev[(i + n - 1) % n], prev[(i + 1) % n])
            } else if i == 0 || i == n - 1 {
                cur[i] = prev[i];
                continue;
            } else {
                (prev[i - 1], prev[i + 1])
            };
            cur[i] = Point::new(
                (a.x + 2.0 * prev[i].x + c.x) / 4.0,
                (a.y + 2.0 * prev[i].y + c.y) / 4.0,
            );
        }
    }
    cur
}

/// Smoothed without drawing the shape in: each pass of [`smooth`] is
/// followed by one that pushes every point back out by a little more
/// than half of what a pass pulls in, so a round shape keeps its size
/// while its ripples go.
fn smooth_kept(pts: &[Point], passes: usize, closed: bool) -> Vec<Point> {
    let n = pts.len();
    let mut cur = pts.to_vec();
    if n < 3 {
        return cur;
    }
    for _ in 0..passes {
        let pulled = smooth(&cur, 1, closed);
        let again = smooth(&pulled, 1, closed);
        for i in 0..n {
            cur[i] = pulled[i] + (pulled[i] - again[i]) * PUSH_BACK;
        }
    }
    cur
}

/// The area a closed line of points goes round, signed.
fn area_of(pts: &[Point]) -> f64 {
    let n = pts.len();
    (0..n)
        .map(|i| pts[i].to_vec2().cross(pts[(i + 1) % n].to_vec2()))
        .sum::<f64>()
        / 2.0
}

/// `ring` grown about its middle to the area of `like`, by a quarter of
/// its size at most; a ring that lost nothing is left as it is.
fn same_area(mut ring: Vec<Point>, like: &[Point]) -> Vec<Point> {
    let (now, want) = (area_of(&ring).abs(), area_of(like).abs());
    if now < 1e-9 || want <= now {
        return ring;
    }
    let by = (want / now).sqrt().min(GROW_MOST);
    let n = ring.len() as f64;
    let mid = ring.iter().fold(Vec2::ZERO, |sum, p| sum + p.to_vec2()) / n;
    for p in &mut ring {
        *p = (mid + (p.to_vec2() - mid) * by).to_point();
    }
    ring
}

fn passes_for(sigma_fine: f64) -> usize {
    (2.0 * sigma_fine * sigma_fine).round().max(1.0) as usize
}

/// A corner found on a border: the point of the border it sits at,
/// where its two sides meet, and how many points of the border before
/// and after it are the rounding (left out of the stretches beside it).
struct Corner {
    at: usize,
    point: Point,
    back: usize,
    fwd: usize,
}

/// The points of `p` (indices, rising) of a polygon no point of the
/// border strays from by more than `eps` (Douglas and Peucker). A closed
/// border's polygon starts at its first point and does not repeat it.
fn polygon(p: &[Point], closed: bool, eps: f64) -> Vec<usize> {
    let n = p.len();
    if n < 3 {
        return (0..n).collect();
    }
    let at = |i: usize| p[i % n];
    // The point of a..b (indices may run past the end, round a closed
    // border) furthest from the chord, if further than `eps`.
    let furthest = |a: usize, b: usize| -> Option<usize> {
        let (pa, pb) = (at(a), at(b));
        let chord = pb - pa;
        let len = chord.hypot();
        let mut best = (eps, None);
        for j in a + 1..b {
            let d = at(j) - pa;
            let off = if len > 1e-9 {
                let t = (d.dot(chord) / (len * len)).clamp(0.0, 1.0);
                (d - chord * t).hypot()
            } else {
                d.hypot()
            };
            if off > best.0 {
                best = (off, Some(j));
            }
        }
        best.1
    };
    let mut keep = vec![false; n + 1];
    let mut stack: Vec<(usize, usize)> = Vec::new();
    let seed = if closed {
        // The point furthest from the first.
        let far = (1..n)
            .max_by(|a, b| (p[*a] - p[0]).hypot2().total_cmp(&(p[*b] - p[0]).hypot2()))
            .unwrap_or(n / 2);
        keep[0] = true;
        keep[far] = true;
        stack.push((0, far));
        stack.push((far, n));
        Some(far)
    } else {
        keep[0] = true;
        keep[n - 1] = true;
        stack.push((0, n - 1));
        None
    };
    while let Some((a, b)) = stack.pop() {
        if let Some(j) = furthest(a, b) {
            keep[j % n] = true;
            stack.push((a, j));
            stack.push((j, b));
        }
    }
    let mut out: Vec<usize> = (0..n).filter(|i| keep[*i]).collect();
    // The two points the polygon was started from are no corners of it
    // when the border runs straight through them.
    if let Some(far) = seed {
        for start in [far, 0] {
            let m = out.len();
            if m < 4 {
                break;
            }
            let Some(k) = out.iter().position(|i| *i == start) else {
                continue;
            };
            let (prev, next) = (out[(k + m - 1) % m], out[(k + 1) % m]);
            let (a, b) = (prev, if next > prev { next } else { next + n });
            if furthest(a, b).is_none() {
                out.remove(k);
            }
        }
    }
    out
}

/// The line through the points of the border from `a` to `b` (indices
/// may run past the end of a closed border): a point of it and its
/// direction of travel, by least squares on the points away from its two
/// ends, level or upright when it is nearly so ([`AXIS_OFF`]).
fn side_line(p: &[Point], a: usize, b: usize, k: f64) -> Option<(Point, Vec2)> {
    let n = p.len();
    let at = |i: usize| p[i % n];
    let chord = at(b) - at(a);
    let len = chord.hypot();
    if b <= a || len < 1e-9 {
        return None;
    }
    let trim = (b - a) / 5;
    let (lo, hi) = (a + trim, b - trim);
    let count = (hi - lo + 1) as f64;
    let (mut cx, mut cy) = (0.0, 0.0);
    for j in lo..=hi {
        cx += at(j).x;
        cy += at(j).y;
    }
    let c = Point::new(cx / count, cy / count);
    let (mut xx, mut xy, mut yy) = (0.0, 0.0, 0.0);
    for j in lo..=hi {
        let d = at(j) - c;
        xx += d.x * d.x;
        xy += d.x * d.y;
        yy += d.y * d.y;
    }
    let mut dir = if hi - lo >= 2 {
        let angle = 0.5 * (2.0 * xy).atan2(xx - yy);
        Vec2::new(angle.cos(), angle.sin())
    } else {
        chord / len
    };
    if dir.dot(chord) < 0.0 {
        dir = -dir;
    }
    let most = if len < AXIS_SHORT * k {
        AXIS_SHORT_TURN
    } else {
        AXIS_TURN
    };
    let near = |off: f64| off.abs() * len <= AXIS_OFF * k || off.abs() <= most.sin();
    if len < AXIS_LEAST * k {
        // Too short to say which way it runs.
    } else if near(dir.y) {
        dir = Vec2::new(dir.x.signum(), 0.0);
    } else if near(dir.x) {
        dir = Vec2::new(0.0, dir.y.signum());
    }
    Some((c, dir))
}

/// Where the lines `a` and `b` (a point and a direction each) cross.
fn crossing(a: (Point, Vec2), b: (Point, Vec2)) -> Option<Point> {
    let det = a.1.cross(b.1);
    if det.abs() < 0.2 {
        return None;
    }
    let s = (b.0 - a.0).cross(b.1) / det;
    let x = a.0 + a.1 * s;
    (x.x.is_finite() && x.y.is_finite()).then_some(x)
}

/// The corners of the border `p` (lightly smoothed points, a fine pixel
/// or less apart), in order.
///
/// The border is first a polygon ([`polygon`]). A corner of the art is a
/// corner of the polygon, or a few of them a short side apart (the
/// picture's blur rounds a corner, and the polygon cuts the rounding in
/// two or three), turning by `f.corner` or more in all, with a side
/// beside it long enough to tell it from an arc ([`CORNER_STANDS_OUT`]):
/// an arc's polygon has sides the shorter the more they turn. The corner
/// is put where the lines of the two sides beside it cross, so it is
/// sharp again. A half turn with only short sides in it is the end of a
/// thin stroke: two corners, on the line across its tip.
fn corners_of(p: &[Point], closed: bool, f: &Fit) -> Vec<Corner> {
    let n = p.len();
    let mut out = Vec::new();
    if n < 8 {
        return out;
    }
    let poly = polygon(p, closed, POLYGON * f.k);
    let m = poly.len();
    if m < 3 {
        return out;
    }
    // Vertex `v` of the polygon as an index into `p` that keeps rising
    // round a closed border.
    let index = |v: isize| -> usize {
        if closed {
            let turns = v.div_euclid(m as isize);
            (poly[v.rem_euclid(m as isize) as usize] as isize + turns * n as isize + n as isize)
                as usize
        } else {
            poly[v.clamp(0, m as isize - 1) as usize]
        }
    };
    let pt = |v: isize| p[index(v) % n];
    let side = |v: isize| (pt(v + 1) - pt(v)).hypot();
    let turn = |v: isize| -> f64 {
        let (a, b) = (pt(v) - pt(v - 1), pt(v + 1) - pt(v));
        a.cross(b).atan2(a.dot(b))
    };
    let short = f.rounded * f.k;
    let mean_gap = {
        let mut total = 0.0;
        for i in 0..n - 1 {
            total += (p[i + 1] - p[i]).hypot();
        }
        (total / (n - 1) as f64).max(1e-6)
    };
    let margin = ((0.3 * f.k / mean_gap).round() as usize).max(1);
    let reach = f.reach;
    // How many points of a side, from its end at a corner on, are the
    // rounding: until the border comes within a little of the side's
    // line, at least `room` and at most six tenths of the side. `from`
    // is the corner's end of the side, `to` its other end.
    let rounding = |line: (Point, Vec2), from: usize, to: usize| -> usize {
        let len = from.abs_diff(to);
        let (least, most) = (margin.min(len / 3), len * 6 / 10);
        let near = |j: usize| ((p[j % n] - line.0).cross(line.1)).abs() <= JOINS * f.k;
        let mut count = 0;
        while count < most {
            let j = if to > from {
                from + count + 1
            } else {
                from - count - 1
            };
            if count >= least && near(j) {
                break;
            }
            count += 1;
        }
        count
    };
    let (first, last) = if closed {
        (0, m as isize - 1)
    } else {
        (1, m as isize - 2)
    };
    let slot = |v: isize| v.rem_euclid(m as isize) as usize;
    // 1. The ends of thin strokes: a half turn in a few sides, between
    // two longer sides running opposite ways no more than [`CAP_WIDE`]
    // apart. Two corners each, on the line across the tip.
    let mut used = vec![false; m];
    // The lines the ends of strokes gave their sides.
    let mut lines: std::collections::HashMap<usize, (Point, Vec2)> =
        std::collections::HashMap::new();
    let mut caps: Vec<(f64, isize, isize)> = Vec::new();
    for a in first..=last {
        for b in a + 1..=(a + CAP_SIDES).min(if closed { a + m as isize - 2 } else { last }) {
            let inner = (a..b).map(side).fold(0.0, f64::max);
            if inner > CAP_SIDE * f.k {
                break;
            }
            let total: f64 = (a..=b).map(turn).sum();
            if !(CAP_TURN..=CAP_MOST).contains(&total.abs()) {
                continue;
            }
            let (before, after) = (side(a - 1), side(b));
            if before.max(after) < (CAP_FLANK * inner).max(f.k) || before.min(after) < 0.9 * f.k {
                continue;
            }
            caps.push(((total.abs() - std::f64::consts::PI).abs(), a, b));
        }
    }
    caps.sort_by(|x, y| x.0.total_cmp(&y.0));
    for (_, a, b) in caps {
        if (a..=b).any(|v| used[slot(v)]) {
            continue;
        }
        let (ia, ib) = (index(a), index(b));
        // A side that goes on nearly straight into the next one is
        // taken with it.
        let on_before = turn(a - 1).abs() < ARC_TURN && (closed || a - 2 >= 0);
        let on_after = turn(b + 1).abs() < ARC_TURN && (closed || b + 2 < m as isize);
        let (Some(la), Some(lb)) = (
            side_line(p, index(if on_before { a - 2 } else { a - 1 }), ia, f.k),
            side_line(p, ib, index(if on_after { b + 2 } else { b + 1 }), f.k),
        ) else {
            continue;
        };
        // Opposite ways, a stroke's width apart. A short side between
        // the tip and the next turn the other way is the slope the blur
        // made of two corners, and says only where the stroke's edge
        // passes, not which way it runs: the stroke's two edges are
        // taken to run alike, as the longer one does.
        if la.1.dot(lb.1) > -0.8 {
            continue;
        }
        let before = side(a - 1) + if on_before { side(a - 2) } else { 0.0 };
        let after = side(b) + if on_after { side(b + 1) } else { 0.0 };
        let brief = CAP_BRIEF * f.k;
        let level = |d: Vec2| d.x == 0.0 || d.y == 0.0;
        let (la, lb) = if level(lb.1) && !level(la.1) && before < brief {
            ((la.0, -lb.1), lb)
        } else if level(la.1) && !level(lb.1) && after < brief {
            (la, (lb.0, -la.1))
        } else if !level(la.1) && before < brief && after >= 1.5 * before {
            ((la.0, -lb.1), lb)
        } else if !level(lb.1) && after < brief && before >= 1.5 * after {
            (la, (lb.0, -la.1))
        } else {
            (la, lb)
        };
        let mut along = unit(la.1 - lb.1);
        let wedge = la.1.dot(lb.1) > CAP_ALIKE && before.min(after) >= WEDGE_SIDE * f.k;
        if wedge {
            // Two sides closing on each other: the sharp end of a
            // wedge when they meet near by, one corner. Else a blunt
            // end, square to the side that is level or upright.
            let mid = p[((ia + ib) / 2) % n];
            let meet = crossing(la, lb).filter(|x| (*x - mid).hypot() <= WEDGE_REACH * f.k);
            if let Some(point) = meet {
                for v in a..=b {
                    used[slot(v)] = true;
                }
                let at = (ia + ib) / 2;
                out.push(Corner {
                    at: at % n,
                    point,
                    back: at - ia + rounding(la, ia, index(a - 1)),
                    fwd: ib - at + rounding(lb, ib, index(b + 1)),
                });
                continue;
            }
            along = match (level(la.1), level(lb.1)) {
                (true, false) => la.1,
                (false, true) => -lb.1,
                _ => continue,
            };
        }
        lines.insert(slot(a - 1), la);
        lines.insert(slot(b), lb);
        let across = Vec2::new(-along.y, along.x);
        if along == Vec2::ZERO || (!wedge && ((lb.0 - la.0).dot(across)).abs() > CAP_WIDE * f.k) {
            continue;
        }
        let Some(tip) = (ia..=ib)
            .map(|j| p[j % n])
            .max_by(|x, y| x.to_vec2().dot(along).total_cmp(&y.to_vec2().dot(along)))
        else {
            continue;
        };
        let (Some(c1), Some(c2)) = (crossing(la, (tip, across)), crossing(lb, (tip, across)))
        else {
            continue;
        };
        if (c1 - tip).hypot() > CAP_WIDE * f.k || (c2 - tip).hypot() > CAP_WIDE * f.k {
            continue;
        }
        // The corners of a square end lie no further from the border
        // than the rounding of such an end leaves them: further, the
        // border turns round there (the bend of an S).
        if f.cap_near.is_finite() {
            let wide = (c2 - c1).hypot();
            let span = (ib - ia).min(n / 4);
            let off = |c: Point| {
                (ia + n - span..=ib + n + span)
                    .map(|j| (p[j % n] - c).hypot())
                    .fold(f64::MAX, f64::min)
            };
            if off(c1).max(off(c2)) > f.cap_near * wide {
                continue;
            }
        }
        for v in a..=b {
            used[slot(v)] = true;
        }
        out.push(Corner {
            at: ia % n,
            point: c1,
            back: rounding(la, ia, index(a - 1)),
            fwd: ib - ia,
        });
        out.push(Corner {
            at: ib % n,
            point: c2,
            back: ib - ia,
            fwd: rounding(lb, ib, index(b + 1)),
        });
    }
    // A round ring: every vertex turns the same way no more than a
    // bowl's do, eight of them at least, with no side much longer than
    // the short ones (a square with rounded corners has four). It has no
    // corner.
    if closed && m >= ROUND_RING && used.iter().all(|u| !*u) {
        let way = (0..m as isize).map(turn).sum::<f64>().signum();
        let mut sides: Vec<f64> = (0..m as isize).map(side).collect();
        sides.sort_by(|x, y| x.total_cmp(y));
        if (0..m as isize).all(|v| (0.0..=BOWL_TURN).contains(&(turn(v) * way)))
            && sides[m - 1] <= ROUND_RING_SIDES * sides[m / 4]
        {
            return out;
        }
    }
    // A bowl: four vertices or more in a row turning the same way, each
    // like an arc's ([`ARC_TURN`] to [`BOWL_TURN`]), half a turn in all
    // nearly, with sides alike between them. No corner is in it.
    {
        let fits = |v: isize| {
            (closed || (first..=last).contains(&v)) && !used[slot(v)] && {
                let t = turn(v).abs();
                (ARC_TURN..=BOWL_TURN).contains(&t)
            }
        };
        let mut bowls: Vec<(isize, isize)> = Vec::new();
        let mut v = first;
        while v <= last {
            if !fits(v) {
                v += 1;
                continue;
            }
            let mut e = v;
            while e - v < m as isize - 1
                && (closed || e < last)
                && fits(e + 1)
                && turn(e) * turn(e + 1) > 0.0
                && side(e) <= BOWL_SIDE * f.k
            {
                e += 1;
            }
            let total: f64 = (v..=e).map(turn).sum();
            let mut sides: Vec<f64> = (v..e).map(side).collect();
            sides.sort_by(|x, y| x.total_cmp(y));
            if e - v >= 3
                && total.abs() >= BOWL_LEAST
                && sides[sides.len() - 1] <= SPLIT_STANDS_OUT * sides[sides.len() / 2]
            {
                bowls.push((v, e));
            }
            v = e + 1;
        }
        for (v, e) in bowls {
            for u in v..=e {
                used[slot(u)] = true;
            }
        }
    }
    // 2. The other corners. The core of one is a run of vertices each
    // turning the same way by [`CORE_TURN`] or more, a short side apart.
    let core = |v: isize| -> bool {
        (closed || (first..=last).contains(&v)) && !used[slot(v)] && turn(v).abs() >= CORE_TURN
    };
    let joined = |v: isize| -> bool {
        core(v) && core(v + 1) && turn(v) * turn(v + 1) > 0.0 && side(v) <= short
    };
    let start = if closed {
        match (0..m as isize).find(|v| !joined(*v - 1)) {
            Some(v) => v,
            // Turning all the way round a short side at a time: a blob.
            None => {
                out.sort_by_key(|c| c.at);
                return out;
            }
        }
    } else {
        first
    };
    let end = if closed { start + m as isize - 1 } else { last };
    // (first vertex, last vertex, the side given as the one before, the
    // side given as the one after).
    let mut runs: Vec<(isize, isize, Option<isize>, Option<isize>)> = Vec::new();
    let mut v = start;
    while v <= end {
        if !core(v) {
            v += 1;
            continue;
        }
        let mut e = v;
        while e < end && joined(e) {
            e += 1;
        }
        let (a, b) = (v, e);
        v = e + 1;
        let total: f64 = (a..=b).map(turn).sum();
        if total.abs() <= SPLIT_TURN || a == b {
            runs.push((a, b, None, None));
            continue;
        }
        // Two corners a short side apart (the two ends of a notch, of a
        // thick stroke's end): apart at the longest side between them,
        // when it stands out from the others; else an arc, no corner.
        let mut inner: Vec<(f64, isize)> = (a..b).map(|v| (side(v), v)).collect();
        inner.sort_by(|x, y| y.0.total_cmp(&x.0));
        let (longest, at) = inner[0];
        let next = inner.get(1).map_or(0.0, |x| x.0);
        if longest >= 0.5 * f.k && longest >= SPLIT_STANDS_OUT * next {
            runs.push((a, at, None, Some(at)));
            runs.push((at + 1, b, Some(at), None));
        }
    }
    let level = |d: Vec2| d.x == 0.0 || d.y == 0.0;
    for (a, b, given_before, given_after) in runs {
        let line_of = |v: isize| -> Option<(Point, Vec2)> {
            lines
                .get(&slot(v))
                .copied()
                .or_else(|| side_line(p, index(v), index(v + 1), f.k))
        };
        // The side beside the run its line is taken from: the first of
        // the two next to it that is level or upright (the first is
        // often the slope the blur left between the corner and the true
        // side), else the first long enough to aim by, else the first.
        let pick = |near: isize,
                    step: isize,
                    ok: &dyn Fn(isize) -> bool|
         -> Option<(isize, (Point, Vec2))> {
            let mut all = vec![near];
            for i in 1..=2 {
                // The vertex between this side and the one before it.
                let between = if step > 0 { near + i } else { near - i + 1 };
                if !ok(between) {
                    break;
                }
                all.push(near + step * i);
            }
            let found: Vec<(isize, (Point, Vec2))> = all
                .into_iter()
                .filter_map(|v| line_of(v).map(|l| (v, l)))
                .collect();
            let at = found
                .iter()
                .position(|(_, l)| level(l.1))
                .or_else(|| found.iter().position(|(v, _)| side(*v) >= 0.9 * f.k))
                .or(if found.is_empty() { None } else { Some(0) })?;
            // A short side before a long straight one running another
            // way is the corner's rounding, not its side.
            let (v, l) = found[at];
            if side(v) < ROUND_LONG_SIDE * f.k {
                let far = found[at + 1..].iter().find(|(w, m)| {
                    side(*w) >= STRAIGHT_SIDE * f.k && !(level(l.1) && m.1.dot(l.1) >= 0.99)
                });
                if let Some(far) = far {
                    return Some(*far);
                }
            }
            Some((v, l))
        };
        let free = |v: isize| (closed || (first..=last).contains(&v)) && !used[slot(v)] && !core(v);
        let before = match given_before {
            Some(v) => line_of(v).map(|l| (v, l)),
            None => pick(a - 1, -1, &free),
        };
        let after = match given_after {
            Some(v) => line_of(v).map(|l| (v, l)),
            None => pick(b, 1, &free),
        };
        let (Some((va, la)), Some((vb, lb))) = (before, after) else {
            continue;
        };
        let total = la.1.cross(lb.1).atan2(la.1.dot(lb.1));
        // A level side goes on through the sides after it that lie on
        // the same line: the blur leaves a long stem a little wavy.
        let whole = |v: isize, l: (Point, Vec2), step: isize| -> (f64, (Point, Vec2)) {
            let mut len = side(v);
            // The line is the longest side's: the others are the ends
            // the blur bent a little.
            let (mut longest, mut line) = (len, l);
            let mut w = v;
            for _ in 0..3 {
                let between = if step > 0 { w + 1 } else { w };
                w += step;
                let Some(next) = line_of(w) else { break };
                // Level sides on one line, or slanted ones going on
                // nearly straight.
                let same = if level(l.1) {
                    next.1.dot(l.1) >= 0.99 && ((next.0 - l.0).cross(l.1)).abs() <= SAME_LINE * f.k
                } else {
                    !level(next.1) && turn(between).abs() < AXIS_TURN
                };
                if !free(between) || !same {
                    break;
                }
                len += side(w);
                if side(w) > longest {
                    (longest, line) = (side(w), next);
                }
            }
            (len, line)
        };
        let ((len_a, la), (len_b, lb)) = (whole(va, la, -1), whole(vb, lb, 1));
        if total.abs() < f.corner || total.abs() > CORNER_MOST {
            continue;
        }
        let square = level(la.1) && level(lb.1);
        let needed = if square {
            0.9 * f.k
        } else {
            CORNER_STANDS_OUT * f.k / total.abs()
        };
        if len_a.max(len_b) < needed {
            continue;
        }
        // A side that is itself part of an arc (it turns on at its far
        // end the same way, as little as an arc's sides do) is no side
        // of a corner: a straight stroke running into a bowl.
        let arc = |l: (Point, Vec2), len: f64, far: f64| {
            !level(l.1)
                && far * total > 0.0
                && (ARC_TURN..=ARC_MOST).contains(&far.abs())
                && far.abs() * len <= ARC_PRODUCT * f.k
        };
        if (!lines.contains_key(&slot(va)) && arc(la, len_a, turn(va)))
            || (!lines.contains_key(&slot(vb)) && arc(lb, len_b, turn(vb + 1)))
        {
            continue;
        }
        // Rounded no more than the picture's blur rounds a corner: a
        // wider curve is a rounded corner of the art.
        // The rounding is all there is between the two sides taken:
        // from where the border leaves the line of the one to where it
        // comes onto the line of the other.
        let on = |l: (Point, Vec2), j: usize| ((p[j % n] - l.0).cross(l.1)).abs() <= POLYGON * f.k;
        let (mut from, mut to) = (index(va + 1), index(vb));
        while from < to && on(la, from + 1) {
            from += 1;
        }
        while to > from && on(lb, to - 1) {
            to -= 1;
        }
        let chord = (p[to % n] - p[from % n]).hypot();
        // Two long level sides stay the sides of a corner a little
        // further apart: a bowl has no such sides.
        let bends = |len: f64, v: isize| {
            let far = turn(v);
            !used[slot(v)]
                && far * total > 0.0
                && (ARC_TURN..=ARC_MOST).contains(&far.abs())
                && far.abs() * len <= ARC_PRODUCT * f.k
        };
        let (bends_a, bends_b) = (bends(len_a, va), bends(len_b, vb + 1));
        let stem = |len: f64, bent: bool| len >= STRAIGHT_SIDE * f.k && !bent;
        let most = if square
            && len_a.min(len_b) >= ROUND_LONG_SIDE * f.k
            && ((!bends_a && !bends_b) || stem(len_a, bends_a) || stem(len_b, bends_b))
        {
            ROUND_LONG * f.round_most
        } else {
            f.round_most
        };
        // Between two sides straight for [`STRAIGHT_SIDE`] pixels, level
        // or not, a rounding as wide as a soft picture's is still the
        // blur's: the art rounds a corner between such sides wider.
        let most = if len_a.min(len_b) >= STRAIGHT_SIDE * f.k
            && !bends(len_a, va)
            && !bends(len_b, vb + 1)
        {
            most.max(STRAIGHT_ROUND)
        } else {
            most
        };
        if chord > 2.0 * (total.abs() / 2.0).sin() * most * f.k {
            continue;
        }
        let (ia, ib) = (index(a), index(b));
        let (back, fwd) = (rounding(la, ia, index(va)), rounding(lb, ib, index(vb + 1)));
        let mid = p[((ia + ib) / 2) % n];
        let point = match crossing(la, lb) {
            Some(x) => {
                let d = x - mid;
                let l = d.hypot();
                if l <= reach {
                    x
                } else {
                    mid + d * (reach / l)
                }
            }
            None => mid,
        };
        let at = (ia + ib) / 2;
        out.push(Corner {
            at: at % n,
            point,
            back: at - ia + back,
            fwd: ib - at + fwd,
        });
    }
    out.sort_by_key(|c| c.at);
    out.dedup_by_key(|c| c.at);
    // A small ring (the gap in a small letter) is smooth when the blur
    // left only one of its corners, and in a picture with soft edges
    // whatever was found: its corners cannot be told from the blur.
    if closed
        && mean_gap * n as f64 <= LONE_CORNER_RING * f.k
        && (out.len() == 1 || f.light < HARD_LIGHT)
    {
        out.clear();
    }
    out
}

/// The furthest a straight line may stray from the border it stands for.
const LINE_MOST: f64 = 0.2;

/// Append the stretch of border `pts` (first and last points fixed) to
/// `out`: a straight line when no point strays from one by more than the
/// tolerance, else cubics.
fn fit_piece(pts: &[Point], tolerance: f64, k: f64, out: &mut BezPath) {
    let n = pts.len();
    if n < 2 {
        return;
    }
    let (a, b) = (pts[0], pts[n - 1]);
    let chord = b - a;
    let len = chord.hypot();
    if n == 2 || len < 1e-9 && n < 4 {
        out.line_to(b);
        return;
    }
    if len > 1e-9 {
        let straight = pts[1..n - 1].iter().all(|q| {
            let d = *q - a;
            let t = d.dot(chord) / (len * len);
            (d.cross(chord) / len).abs() <= tolerance.min(LINE_MOST * k)
                && (-0.05..=1.05).contains(&t)
        });
        if straight {
            out.line_to(b);
            return;
        }
    }
    let far = ((n - 1) / 3).clamp(1, 6);
    let mut t1 = unit(pts[far] - a);
    let mut t2 = unit(pts[n - 1 - far] - b);
    if t1 == Vec2::ZERO {
        t1 = unit(pts[1] - a);
    }
    if t2 == Vec2::ZERO {
        t2 = unit(pts[n - 2] - b);
    }
    fit_cubics(pts, t1, t2, tolerance, out);
}

/// The border through `corners` (the pixel corners of the finer grid it
/// passes, a unit step apart; a closed one without its first point
/// repeated) as lines and curves, in the finer grid's coordinates: the
/// middles of its steps, smoothed; its corners found and made sharp
/// ([`corners_of`]); each stretch between two corners a straight line or
/// as few cubics as stay within the tolerance. An open border keeps its
/// two ends, where the borders of the other regions meeting there end
/// too.
pub(crate) fn fit(corners: &[Point], closed: bool, f: &Fit) -> BezPath {
    let mut out = BezPath::new();
    let n = corners.len();
    if n == 0 {
        return out;
    }
    let mut mids: Vec<Point> = Vec::with_capacity(n + 2);
    if closed {
        for i in 0..n {
            mids.push(corners[i].midpoint(corners[(i + 1) % n]));
        }
    } else {
        mids.push(corners[0]);
        for e in corners.windows(2) {
            mids.push(e[0].midpoint(e[1]));
        }
        mids.push(corners[n - 1]);
    }
    let n = mids.len();
    if closed && n < 4 {
        return out;
    }
    if !closed && n < 6 {
        out.move_to(mids[0]);
        out.line_to(mids[n - 1]);
        return out;
    }
    // Lightly smoothed: the steps of the finer grid go, corners stay
    // within a fraction of a pixel.
    let light = smooth(&mids, passes_for((f.light * f.k).max(1.0)), closed);
    let found = corners_of(&light, closed, f);
    let sigma = (f.sigma * f.k).max(1.0);
    let tolerance = f.tolerance * f.k;
    let more = if f.sigma > f.light {
        passes_for(sigma).saturating_sub(passes_for((f.light * f.k).max(1.0)))
    } else {
        0
    };
    if closed && found.is_empty() {
        // No corner: smoothed as the settings ask, quarters meeting
        // smoothly.
        let soft = if f.sigma <= f.light {
            light
        } else {
            smooth_kept(&light, more, true)
        };
        // Smoothing draws a small ring in (a dot of three pixels loses a
        // quarter of its area): it is given the area of its border back.
        let soft = same_area(soft, &mids);
        let reach = (n / 8).clamp(1, 6);
        let tangent = |i: usize| unit(soft[(i + reach) % n] - soft[(i + n - reach) % n]);
        out.move_to(soft[0]);
        for q in 0..4 {
            let (a, b) = (q * n / 4, ((q + 1) * n / 4) % n);
            let mut piece: Vec<Point> = if b > a {
                soft[a..=b].to_vec()
            } else {
                soft[a..].to_vec()
            };
            if b <= a {
                piece.push(soft[0]);
            }
            fit_cubics(&piece, tangent(a), -tangent(b), tolerance, &mut out);
        }
        out.close_path();
        return out;
    }
    // The fixed points: corners, and the two ends of an open border,
    // each with the points before and after it left out.
    let mut stops: Vec<(usize, Point, usize, usize)> = Vec::new();
    if !closed {
        stops.push((0, mids[0], 1, 1));
    }
    for c in &found {
        stops.push((c.at, c.point, c.back.max(1), c.fwd.max(1)));
    }
    if !closed {
        stops.push((n - 1, mids[n - 1], 1, 1));
    }
    let count = stops.len();
    out.move_to(stops[0].1);
    let pieces = if closed { count } else { count - 1 };
    // How much more each stretch is smoothed than the border was for
    // finding its corners, its two ends fixed.
    let mut piece: Vec<Point> = Vec::new();
    for s in 0..pieces {
        let (a, pa, _, from) = stops[s];
        let (b, pb, to, _) = stops[(s + 1) % count];
        // The points strictly between the two stops.
        let len = if closed {
            let d = (b + n - a) % n;
            if d == 0 {
                n
            } else {
                d
            }
        } else {
            b - a
        };
        piece.clear();
        piece.push(pa);
        if len > from + to {
            for j in from..=len - to {
                piece.push(light[(a + j) % n]);
            }
        }
        piece.push(pb);
        if more > 0 && piece.len() > 3 {
            let soft = smooth_kept(&piece, more, false);
            fit_piece(&soft, tolerance, f.k, &mut out);
        } else {
            fit_piece(&piece, tolerance, f.k, &mut out);
        }
    }
    if closed {
        out.close_path();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_grid_grows_finer_for_small_pictures_only() {
        assert_eq!(scale_for(300, 300), 4);
        assert_eq!(scale_for(1024, 1024), 2);
        assert_eq!(scale_for(4000, 3000), 1);
    }

    /// Sharpening starts at a softness a crisp picture never reaches and
    /// is full for a blurred one; it leaves flat colour as it is and
    /// steepens a soft edge.
    #[test]
    fn only_a_soft_picture_is_sharpened() {
        assert_eq!(soft_share(0.7), 0.0);
        assert_eq!(soft_share(1.3), 1.0);
        assert!(soft_share(1.0) > 0.3 && soft_share(1.0) < 0.6);
        let (w, h) = (8, 5);
        let ramp = [0.0f32, 0.0, 30.0, 100.0, 170.0, 240.0, 255.0, 255.0];
        let mut stand: Vec<[f32; 3]> = (0..w * h).map(|i| [ramp[i % w]; 3]).collect();
        sharpen(&mut stand, w, h, SHARPEN_MOST);
        let row = &stand[2 * w..3 * w];
        assert!(
            row[0][0].abs() < 0.5 && (row[7][0] - 255.0).abs() < 0.5,
            "{row:?}"
        );
        assert!(row[2][0] < 30.0 && row[5][0] > 240.0, "{row:?}");
        assert!(row[5][0] - row[2][0] > 240.0, "{row:?}");
    }

    fn soft_fit() -> Fit {
        Fit::of(&crate::trace::Preset::Logo.settings(), 3.5, false, 1.0)
    }

    /// Points a pixel apart along the lines joining `at`, closed.
    fn border(at: &[Point]) -> Vec<Point> {
        let mut out = Vec::new();
        for (i, a) in at.iter().enumerate() {
            let b = at[(i + 1) % at.len()];
            let steps = ((b - *a).hypot()).ceil().max(1.0) as usize;
            for s in 0..steps {
                out.push(*a + (b - *a) * (s as f64 / steps as f64));
            }
        }
        out
    }

    /// The round bottom inside a small U (half a turn eleven pixels
    /// wide between two straight sides, its polygon as an enlarged
    /// picture gave it) has no corner; the two square corners at its
    /// top stay.
    #[test]
    fn a_bowl_between_two_straight_sides_has_no_corner() {
        let at = [
            Point::new(7.9, 4.6),
            Point::new(8.3, 20.2),
            Point::new(10.0, 23.5),
            Point::new(12.6, 24.9),
            Point::new(16.6, 24.1),
            Point::new(18.7, 21.2),
            Point::new(19.4, 4.6),
        ];
        let fit = Fit::of(&crate::trace::Preset::Logo.settings(), 3.0, false, 0.575);
        let found = corners_of(&border(&at), true, &fit);
        let points: Vec<Point> = found.iter().map(|c| c.point).collect();
        assert_eq!(points.len(), 2, "{points:?}");
        assert!(points.iter().all(|p| (p.y - 4.6).abs() < 0.5), "{points:?}");
    }

    /// The bends of a small S (its polygon as an enlarged picture gave
    /// it) are no square ends of a stroke: where every end found is
    /// taken, the corners of one lie three pixels inside the letter; with
    /// the corners of an end held near the border, none strays from it.
    #[test]
    fn the_bend_of_a_small_s_is_not_squared_off() {
        let at = [
            Point::new(19.8, 15.3),
            Point::new(26.5, 15.0),
            Point::new(30.5, 16.0),
            Point::new(33.0, 17.5),
            Point::new(35.7, 20.8),
            Point::new(35.6, 23.1),
            Point::new(34.4, 23.8),
            Point::new(31.8, 23.7),
            Point::new(28.0, 21.5),
            Point::new(24.5, 21.0),
            Point::new(20.9, 21.4),
            Point::new(19.4, 23.7),
            Point::new(21.1, 26.4),
            Point::new(32.7, 30.8),
            Point::new(36.7, 34.8),
            Point::new(36.9, 38.4),
            Point::new(35.8, 41.3),
            Point::new(32.5, 45.0),
            Point::new(29.2, 46.7),
            Point::new(20.0, 46.5),
            Point::new(15.5, 44.0),
            Point::new(12.3, 40.2),
            Point::new(12.3, 37.8),
            Point::new(13.7, 36.4),
            Point::new(23.6, 40.9),
            Point::new(27.2, 40.7),
            Point::new(29.7, 38.2),
            Point::new(29.4, 36.1),
            Point::new(27.9, 34.6),
            Point::new(16.7, 30.8),
            Point::new(12.5, 26.0),
            Point::new(12.3, 23.7),
            Point::new(14.6, 19.1),
            Point::new(17.1, 16.6),
        ];
        let ring = border(&at);
        let strays = |fit: &Fit| {
            corners_of(&ring, true, fit)
                .iter()
                .map(|c| {
                    ring.iter()
                        .map(|q| (*q - c.point).hypot())
                        .fold(f64::MAX, f64::min)
                })
                .fold(0.0, f64::max)
        };
        let mut fit = Fit::of(&crate::trace::Preset::Logo.settings(), 3.5, false, 0.0);
        assert!(strays(&fit) > 2.5, "{}", strays(&fit));
        fit.cap_near = 0.25;
        assert!(strays(&fit) < 1.8, "{}", strays(&fit));
    }

    /// A small oval ring (the inside of an O) has no corner, and neither
    /// has a ring too small to tell its corners from the blur.
    #[test]
    fn a_round_ring_and_a_tiny_one_have_no_corner() {
        let oval: Vec<Point> = (0..24)
            .map(|i| {
                let a = std::f64::consts::TAU * i as f64 / 24.0;
                Point::new(20.0 + 8.0 * a.cos(), 20.0 + 10.0 * a.sin())
            })
            .collect();
        assert!(corners_of(&border(&oval), true, &soft_fit()).is_empty());
        let slot = [
            Point::new(0.0, 0.0),
            Point::new(6.0, 0.0),
            Point::new(6.0, 10.0),
            Point::new(0.0, 10.0),
        ];
        assert!(corners_of(&border(&slot), true, &soft_fit()).is_empty());
        let window = [
            Point::new(0.0, 0.0),
            Point::new(30.0, 0.0),
            Point::new(30.0, 40.0),
            Point::new(0.0, 40.0),
        ];
        assert_eq!(corners_of(&border(&window), true, &soft_fit()).len(), 4);
    }
}
