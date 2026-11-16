//! Gradients in the outline trace: the bands a smooth blend of colours
//! is cut into by the colour reduction are one region again, filled with
//! the blend. See docs/behavior/bitmap-tracing.md, "Gradients".

use crate::trace::{cancelled, Cancel};
use crate::trace_fine::Edges;
use tracedraw_core::geometry::Point;

/// A blend of two colours along a line, in the picture's pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Blend {
    pub from: Point,
    pub to: Point,
    pub start: [u8; 3],
    pub end: [u8; 3],
}

/// How far apart (RGB distance) the colours two pixels either side of
/// the border of two patches may be, at the middle border place, for the
/// border to be no edge of the art but a cut through a blend.
const BAND_STEP: f64 = 18.0;
/// How much (RGB levels) a patch's colour must change from one pixel
/// to the next to be part of a blend: a steep blend is cut into bands a
/// few pixels wide, each changing little from side to side.
const SLOPE: f64 = 0.25;
/// A patch of fewer pixels, or one a pixel wide, joins a blend beside
/// it whatever its own slope: too small to show one.
const SMALL: f64 = 12.0;
/// How far apart the two ends of a blend must be to be drawn as one,
/// and how far (RGB distance, root mean square) the pixels may lie from
/// it.
const BLEND_APART: f64 = 24.0;
const BLEND_FIT: f64 = 18.0;
/// The most of a lone patch's colour variation a blend may leave
/// unexplained for the patch to be drawn as one.
const ALONE_LEFT: f64 = 0.35;

/// The sums a plane is fitted to a patch's colours from; they add up
/// when patches join.
#[derive(Clone, Copy, Default)]
struct Sums {
    n: f64,
    x: f64,
    y: f64,
    xx: f64,
    xy: f64,
    yy: f64,
    c: [f64; 3],
    xc: [f64; 3],
    yc: [f64; 3],
    cc: [f64; 3],
    /// The box of the pixels.
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
}

impl Sums {
    fn new() -> Sums {
        Sums {
            x0: f64::MAX,
            y0: f64::MAX,
            x1: f64::MIN,
            y1: f64::MIN,
            ..Default::default()
        }
    }
    fn add(&mut self, x: f64, y: f64, c: [u8; 3]) {
        self.n += 1.0;
        self.x += x;
        self.y += y;
        self.xx += x * x;
        self.xy += x * y;
        self.yy += y * y;
        for (k, v) in c.iter().enumerate() {
            let v = *v as f64;
            self.c[k] += v;
            self.xc[k] += x * v;
            self.yc[k] += y * v;
            self.cc[k] += v * v;
        }
        self.reach(x, y);
    }
    fn reach(&mut self, x: f64, y: f64) {
        self.x0 = self.x0.min(x);
        self.y0 = self.y0.min(y);
        self.x1 = self.x1.max(x);
        self.y1 = self.y1.max(y);
    }
    fn join(&mut self, o: &Sums) {
        self.n += o.n;
        self.x += o.x;
        self.y += o.y;
        self.xx += o.xx;
        self.xy += o.xy;
        self.yy += o.yy;
        for k in 0..3 {
            self.c[k] += o.c[k];
            self.xc[k] += o.xc[k];
            self.yc[k] += o.yc[k];
            self.cc[k] += o.cc[k];
        }
        self.x0 = self.x0.min(o.x0);
        self.y0 = self.y0.min(o.y0);
        self.x1 = self.x1.max(o.x1);
        self.y1 = self.y1.max(o.y1);
    }
    /// The slope of each channel across and down (levels a pixel), about
    /// the pixels' middle; none for pixels in a line or too few.
    fn slopes(&self) -> Option<[(f64, f64); 3]> {
        if self.n < 6.0 {
            return None;
        }
        let (mx, my) = (self.x / self.n, self.y / self.n);
        let sxx = self.xx - self.n * mx * mx;
        let sxy = self.xy - self.n * mx * my;
        let syy = self.yy - self.n * my * my;
        let det = sxx * syy - sxy * sxy;
        if det.abs() < 1e-6 * (sxx * syy).abs().max(1.0) {
            return None;
        }
        Some(std::array::from_fn(|k| {
            let sxc = self.xc[k] - mx * self.c[k];
            let syc = self.yc[k] - my * self.c[k];
            ((sxc * syy - syc * sxy) / det, (syc * sxx - sxc * sxy) / det)
        }))
    }
    /// Whether the pixels are part of a blend: see [`SLOPE`].
    fn sloped(&self) -> bool {
        self.slopes()
            .is_some_and(|s| s.iter().any(|(gx, gy)| gx.hypot(*gy) >= SLOPE))
    }
    fn mean(&self) -> [u8; 3] {
        std::array::from_fn(|k| (self.c[k] / self.n.max(1.0)).round().clamp(0.0, 255.0) as u8)
    }
}

/// The labels with the bands of each blend joined into one region, the
/// palette with a colour more for each such region (its mean), and each
/// palette colour's blend, if its region is drawn as one.
///
/// A patch (the touching pixels of one label) is a band of a blend when
/// its colour changes steadily across it; two bands side by side with no
/// edge of the art between them (the colours two pixels either side of
/// their border are close, [`BAND_STEP`]) are one region. Flat colours
/// are never joined, so the soft edge between two of them, bands and
/// all, never makes them one. A patch whose own colour changes steadily
/// from end to end (a gentle blend the colour reduction left whole) is a
/// region by itself. The region is drawn as a blend along the
/// direction its colour changes most, from the colour at one end to the
/// colour at the other, when its pixels lie near that blend; else in its
/// mean colour. `colors` are the picture's own; pixels on an edge
/// (`edges`) hold a mix and are left out of the sums.
pub(crate) fn join_blends(
    labels: &mut [usize],
    w: usize,
    h: usize,
    mut palette: Vec<[u8; 3]>,
    colors: &[[u8; 3]],
    edges: &Edges,
    cancel: Option<&Cancel>,
) -> Option<(Vec<[u8; 3]>, Vec<Option<Blend>>)> {
    let none = |p: &Vec<[u8; 3]>| vec![None; p.len()];
    if labels.len() != w * h || colors.len() != w * h || w < 3 || h < 3 {
        let blends = none(&palette);
        return Some((palette, blends));
    }
    // 1. The patches.
    let mut of = vec![u32::MAX; w * h];
    let mut sums: Vec<Sums> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    for start in 0..w * h {
        if start % 65536 == 0 && cancelled(cancel) {
            return None;
        }
        if of[start] != u32::MAX || labels[start] == usize::MAX {
            continue;
        }
        let (l, id) = (labels[start], sums.len() as u32);
        let mut s = Sums::new();
        of[start] = id;
        stack.push(start);
        while let Some(i) = stack.pop() {
            let (x, y) = (i % w, i / w);
            if edges.at(i).is_none() {
                s.add(x as f64, y as f64, colors[i]);
            } else {
                s.reach(x as f64, y as f64);
            }
            for j in [
                (x > 0).then(|| i - 1),
                (x + 1 < w).then(|| i + 1),
                (y > 0).then(|| i - w),
                (y + 1 < h).then(|| i + w),
            ]
            .into_iter()
            .flatten()
            {
                if labels[j] == l && of[j] == u32::MAX {
                    of[j] = id;
                    stack.push(j);
                }
            }
        }
        sums.push(s);
    }
    let count = sums.len();
    // 2. The step across each border, two pixels either side.
    let mut steps: std::collections::HashMap<(u32, u32), Vec<u16>> =
        std::collections::HashMap::new();
    let dist = |a: [u8; 3], b: [u8; 3]| -> f64 {
        (0..3)
            .map(|k| (a[k] as f64 - b[k] as f64).powi(2))
            .sum::<f64>()
            .sqrt()
    };
    for y in 0..h {
        if y % 64 == 0 && cancelled(cancel) {
            return None;
        }
        for x in 0..w {
            let i = y * w + x;
            let a = of[i];
            if a == u32::MAX {
                continue;
            }
            for (dx, dy) in [(1usize, 0usize), (0, 1)] {
                if x + dx >= w || y + dy >= h {
                    continue;
                }
                let j = i + dy * w + dx;
                let b = of[j];
                if b == u32::MAX || b == a {
                    continue;
                }
                // A pixel further in on each side, when still the patch's.
                let back = (x >= dx && y >= dy).then(|| i - dy * w - dx);
                let on = (x + 2 * dx < w && y + 2 * dy < h).then(|| j + dy * w + dx);
                let p = back.filter(|p| of[*p] == a).unwrap_or(i);
                let q = on.filter(|q| of[*q] == b).unwrap_or(j);
                let d = dist(colors[p], colors[q]).min(65535.0) as u16;
                steps.entry((a.min(b), a.max(b))).or_default().push(d);
            }
        }
    }
    // 3. Bands with no edge between them join, the faintest borders first.
    let sloped: Vec<bool> = sums.iter().map(Sums::sloped).collect();
    let small: Vec<bool> = sums
        .iter()
        .map(|s| s.n < SMALL || s.slopes().is_none())
        .collect();
    let mut pairs: Vec<(u16, u32, u32)> = Vec::new();
    for ((a, b), mut v) in steps {
        if v.len() < 3 {
            continue;
        }
        v.sort_unstable();
        let mid = v[v.len() / 2];
        if (mid as f64) < BAND_STEP {
            pairs.push((mid, a, b));
        }
    }
    pairs.sort_unstable();
    let mut root: Vec<u32> = (0..count as u32).collect();
    fn find(root: &mut [u32], mut i: u32) -> u32 {
        while root[i as usize] != i {
            root[i as usize] = root[root[i as usize] as usize];
            i = root[i as usize];
        }
        i
    }
    for (_, a, b) in pairs {
        let (sa, sb) = (sloped[a as usize], sloped[b as usize]);
        let (ta, tb) = (small[a as usize], small[b as usize]);
        // Two patches too small to show a slope join as well (two bands
        // a pixel wide side by side); their region counts only with a
        // sloped patch in it, below.
        if !((sa && sb) || (sa && tb) || (sb && ta) || (ta && tb)) {
            continue;
        }
        let (ra, rb) = (find(&mut root, a), find(&mut root, b));
        if ra != rb {
            root[rb as usize] = ra;
        }
    }
    let mut blends = none(&palette);
    // 4. Each joined region: its sums, a new label, its blend.
    let mut group: Vec<Sums> = vec![Sums::new(); count];
    let mut members = vec![0u32; count];
    let mut blended = vec![false; count];
    for i in 0..count as u32 {
        let r = find(&mut root, i) as usize;
        let s = sums[i as usize];
        group[r].join(&s);
        members[r] += 1;
        blended[r] |= sloped[i as usize];
    }
    // The direction each region's colour changes most along, and its
    // reach along it.
    let mut axis: Vec<Option<(f64, f64)>> = vec![None; count];
    for r in 0..count {
        if group[r].n < 1.0 || !blended[r] {
            continue;
        }
        if let Some(s) = group[r].slopes() {
            let (mut a, mut b, mut c) = (0.0, 0.0, 0.0);
            for (gx, gy) in s {
                a += gx * gx;
                b += gx * gy;
                c += gy * gy;
            }
            let angle = 0.5 * (2.0 * b).atan2(a - c);
            axis[r] = Some((angle.cos(), angle.sin()));
        }
    }
    let mut span = vec![(f64::MAX, f64::MIN); count];
    for i in 0..w * h {
        if of[i] == u32::MAX {
            continue;
        }
        let r = find(&mut root, of[i]) as usize;
        if let Some((dx, dy)) = axis[r] {
            let s = (i % w) as f64 * dx + (i / w) as f64 * dy;
            span[r] = (span[r].0.min(s), span[r].1.max(s));
        }
    }
    let mut blend_of: Vec<Option<Blend>> = vec![None; count];
    for r in 0..count {
        let Some((dx, dy)) = axis[r] else {
            continue;
        };
        let g = &group[r];
        let (s0, s1) = span[r];
        if s0 >= s1 {
            continue;
        }
        // Each channel against the place along the direction.
        let n = g.n;
        let s_sum = dx * g.x + dy * g.y;
        let ss = dx * dx * g.xx + 2.0 * dx * dy * g.xy + dy * dy * g.yy;
        let var = ss - s_sum * s_sum / n;
        if var < 1e-6 {
            continue;
        }
        let (mut start, mut end, mut rss, mut all) = ([0u8; 3], [0u8; 3], 0.0, 0.0);
        for k in 0..3 {
            let sc = dx * g.xc[k] + dy * g.yc[k];
            let slope = (sc - s_sum * g.c[k] / n) / var;
            let at0 = g.c[k] / n - slope * s_sum / n;
            start[k] = (at0 + slope * s0).round().clamp(0.0, 255.0) as u8;
            end[k] = (at0 + slope * s1).round().clamp(0.0, 255.0) as u8;
            let spread = g.cc[k] - g.c[k] * g.c[k] / n;
            all += spread;
            rss += spread - slope * slope * var;
        }
        let fit = (rss.max(0.0) / n).sqrt();
        if dist(start, end) < BLEND_APART || fit > BLEND_FIT {
            continue;
        }
        // A patch on its own is a blend only when the blend accounts for
        // most of what its colour varies by: a flat colour with a stain
        // or a soft shadow in it stays flat.
        if members[r] < 2 && rss > ALONE_LEFT * all {
            continue;
        }
        // Through the region's middle, pixel middles at half pixels.
        let (mx, my) = (g.x / n + 0.5, g.y / n + 0.5);
        let mid = dx * (mx - 0.5) + dy * (my - 0.5);
        let at = |s: f64| Point::new(mx + dx * (s - mid), my + dy * (s - mid));
        blend_of[r] = Some(Blend {
            from: at(s0),
            to: at(s1 + 1.0),
            start,
            end,
        });
    }
    // 5. A label of its own for each region of several bands and each
    // patch that is a blend by itself.
    let mut label_of = vec![usize::MAX; count];
    for r in 0..count {
        if group[r].n < 1.0 || !blended[r] || (members[r] < 2 && blend_of[r].is_none()) {
            continue;
        }
        label_of[r] = palette.len();
        palette.push(group[r].mean());
        blends.push(blend_of[r]);
    }
    for i in 0..w * h {
        if of[i] == u32::MAX {
            continue;
        }
        let r = find(&mut root, of[i]) as usize;
        if label_of[r] != usize::MAX {
            labels[i] = label_of[r];
        }
    }
    Some((palette, blends))
}
