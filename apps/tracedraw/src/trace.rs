//! Bitmap to vector tracing: outline mode (colour regions become filled
//! curves) and centreline mode (strokes become open curves). See
//! docs/behavior/bitmap-tracing.md for the algorithm and defaults.

use image::{Rgba, RgbaImage};
use tracedraw_core::{
    geometry::{BezPath, Point, Rect},
    Color,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Preset {
    // Centreline
    Technical,
    LineDrawing,
    // Outline
    LineArt,
    Logo,
    DetailedLogo,
    Clipart,
    LowQualityImage,
    HighQualityImage,
}

impl Preset {
    /// Every preset, centreline ones first.
    pub const ALL: [Preset; 8] = [
        Preset::Technical,
        Preset::LineDrawing,
        Preset::LineArt,
        Preset::Logo,
        Preset::DetailedLogo,
        Preset::Clipart,
        Preset::LowQualityImage,
        Preset::HighQualityImage,
    ];

    pub fn is_centerline(self) -> bool {
        matches!(self, Preset::Technical | Preset::LineDrawing)
    }
    pub fn key(self) -> &'static str {
        match self {
            Preset::Technical => "menu.bitmaps.trace_technical",
            Preset::LineDrawing => "menu.bitmaps.trace_line_drawing",
            Preset::LineArt => "menu.bitmaps.trace_line_art",
            Preset::Logo => "menu.bitmaps.trace_logo",
            Preset::DetailedLogo => "menu.bitmaps.trace_detailed_logo",
            Preset::Clipart => "menu.bitmaps.trace_clipart",
            Preset::LowQualityImage => "menu.bitmaps.trace_low_quality",
            Preset::HighQualityImage => "menu.bitmaps.trace_high_quality",
        }
    }
    pub fn settings(self) -> Settings {
        let (detail, smoothing, colors, bw) = match self {
            Preset::Technical => (60.0, 30.0, 2, true),
            Preset::LineDrawing => (50.0, 50.0, 2, true),
            Preset::LineArt => (40.0, 25.0, 2, true),
            Preset::Logo => (50.0, 25.0, 8, false),
            Preset::DetailedLogo => (75.0, 20.0, 24, false),
            Preset::Clipart => (60.0, 25.0, 32, false),
            Preset::LowQualityImage => (35.0, 40.0, 48, false),
            Preset::HighQualityImage => (80.0, 15.0, 96, false),
        };
        Settings {
            detail,
            smoothing,
            corner_smoothness: 0.0,
            colors,
            black_white: bw,
            remove_background: true,
            merge_adjacent: false,
            centerline: self.is_centerline(),
            delete_original: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub detail: f64,
    pub smoothing: f64,
    pub corner_smoothness: f64,
    pub colors: u32,
    pub black_white: bool,
    pub remove_background: bool,
    pub merge_adjacent: bool,
    pub centerline: bool,
    pub delete_original: bool,
}

/// One traced region or stroke.
pub struct Traced {
    pub path: BezPath,
    pub color: Color,
    pub closed: bool,
}

/// Trace `img` into shapes whose coordinates are in pixel space (y down).
/// The caller maps them into the bitmap's rectangle.
pub fn trace(img: &RgbaImage, s: &Settings) -> Vec<Traced> {
    if s.centerline {
        return centerline(img, s);
    }
    outline(img, s)
}

// ----- outline mode -----------------------------------------------------------

fn outline(img: &RgbaImage, s: &Settings) -> Vec<Traced> {
    let (w, h) = (img.width() as usize, img.height() as usize);
    if w == 0 || h == 0 {
        return Vec::new();
    }
    // 1. Smooth a little when detail is low, then quantise.
    let src = if s.detail < 50.0 {
        image::imageops::blur(img, (50.0 - s.detail as f32) / 25.0)
    } else {
        img.clone()
    };
    let palette: Vec<[u8; 3]> = if s.black_white {
        vec![[0, 0, 0], [255, 255, 255]]
    } else {
        median_cut(&src, s.colors.clamp(2, 256) as usize)
    };
    let mut labels = vec![0usize; w * h];
    for (x, y, p) in src.enumerate_pixels() {
        let idx = if p[3] < 128 {
            usize::MAX
        } else if s.black_white {
            if luma(*p) < 128.0 {
                0
            } else {
                1
            }
        } else {
            nearest(&palette, [p[0], p[1], p[2]])
        };
        labels[y as usize * w + x as usize] = idx;
    }
    // Specks: patches of one colour smaller than the detail allows take
    // the colour around them, so stray dots and the fringes of soft edges
    // do not become curves of their own (#611).
    despeckle(&mut labels, w, h, speck_area(s.detail));
    // 2. Background: the most common colour touching the border.
    let mut border_counts = vec![0usize; palette.len()];
    for x in 0..w {
        for y in [0, h - 1] {
            let l = labels[y * w + x];
            if l != usize::MAX {
                border_counts[l] += 1;
            }
        }
    }
    for y in 0..h {
        for x in [0, w - 1] {
            let l = labels[y * w + x];
            if l != usize::MAX {
                border_counts[l] += 1;
            }
        }
    }
    let background = if s.remove_background {
        border_counts
            .iter()
            .enumerate()
            .max_by_key(|(_, c)| **c)
            .map(|(i, _)| i)
    } else {
        None
    };
    // 3. Per colour, darkest first (so lighter regions stack on top),
    // extract the boundary contours of the mask.
    let mut order: Vec<usize> = (0..palette.len()).collect();
    order.sort_by_key(|i| {
        let c = palette[*i];
        (c[0] as u32 * 299 + c[1] as u32 * 587 + c[2] as u32 * 114) / 1000
    });
    let tolerance = 0.3 + (100.0 - s.detail) / 100.0 * 2.5;
    let mut out = Vec::new();
    for ci in order {
        if Some(ci) == background {
            continue;
        }
        let mask: Vec<bool> = labels.iter().map(|l| *l == ci).collect();
        let contours = contours(&mask, w, h);
        let mut path = BezPath::new();
        for ring in contours {
            if ring.len() < 3 {
                continue;
            }
            // Without smoothing the pixels' own edges, steps and all.
            if s.smoothing <= 0.0 {
                let simplified = tracedraw_core::geometry::simplify(&ring, tolerance);
                if simplified.len() >= 3 {
                    path.extend(polyline_closed(&simplified));
                }
                continue;
            }
            path.extend(fit_ring(&ring, s));
        }
        if path.elements().is_empty() {
            continue;
        }
        let c = palette[ci];
        out.push(Traced {
            path,
            color: Color::rgb8(c[0], c[1], c[2]),
            closed: true,
        });
    }
    out
}

/// The smallest patch of one colour that is kept, in pixels: none
/// below detail 100, a few pixels at 80, about 40 at 35.
fn speck_area(detail: f64) -> usize {
    let k = (100.0 - detail.clamp(0.0, 100.0)) / 100.0 * 10.0;
    (k * k).round().max(1.0) as usize
}

/// Give every 4-connected patch of one label smaller than `min_area`
/// pixels the label most of its neighbours outside it have (transparent
/// pixels are left alone and never spread).
fn despeckle(labels: &mut [usize], w: usize, h: usize, min_area: usize) {
    if min_area <= 1 || labels.len() != w * h {
        return;
    }
    let mut seen = vec![false; w * h];
    let mut patch: Vec<usize> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    let mut counts: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for start in 0..w * h {
        if seen[start] || labels[start] == usize::MAX {
            continue;
        }
        let l = labels[start];
        patch.clear();
        counts.clear();
        stack.push(start);
        seen[start] = true;
        while let Some(i) = stack.pop() {
            patch.push(i);
            let (x, y) = (i % w, i / w);
            let mut near = [usize::MAX; 4];
            if x > 0 {
                near[0] = i - 1;
            }
            if x + 1 < w {
                near[1] = i + 1;
            }
            if y > 0 {
                near[2] = i - w;
            }
            if y + 1 < h {
                near[3] = i + w;
            }
            for j in near.into_iter().filter(|j| *j != usize::MAX) {
                if labels[j] == l {
                    if !seen[j] {
                        seen[j] = true;
                        stack.push(j);
                    }
                } else if labels[j] != usize::MAX && patch.len() < min_area {
                    *counts.entry(labels[j]).or_default() += 1;
                }
            }
        }
        if patch.len() < min_area {
            if let Some((&to, _)) = counts.iter().max_by_key(|(k, c)| (**c, usize::MAX - **k)) {
                for &i in &patch {
                    labels[i] = to;
                }
            }
        }
    }
}

/// A pixel-edge ring as smooth curves (#611): the ring through the
/// middles of the pixel edges, so stair steps become slopes; its corners
/// found where it turns sharply (fewer with more corner smoothness);
/// the stretches between them smoothed (more with more smoothing) and
/// fitted with as few cubic Beziers as stay within the detail's
/// tolerance of them. Corners stay sharp, a ring without corners closes
/// smoothly.
fn fit_ring(ring: &[Point], s: &Settings) -> BezPath {
    // One point per pixel edge: the middle of each unit step, with the
    // pixel corner the step starts at.
    let mut pts: Vec<Point> = Vec::new();
    let mut starts: Vec<Point> = Vec::new();
    let n = ring.len();
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        let steps = (b - a).hypot().round().max(1.0) as usize;
        for k in 0..steps {
            starts.push(a.lerp(b, k as f64 / steps as f64));
            pts.push(a.lerp(b, (k as f64 + 0.5) / steps as f64));
        }
    }
    let n = pts.len();
    if n < 4 {
        return polyline_closed(&pts);
    }
    // Corners: where the ring turns by more than the threshold over a
    // few pixels, the sharpest of each run.
    let reach = 3usize.min(n / 4).max(1);
    let threshold = (60.0 + s.corner_smoothness.clamp(0.0, 100.0) * 0.8).to_radians();
    let at = |i: isize| pts[i.rem_euclid(n as isize) as usize];
    let turn: Vec<f64> = (0..n as isize)
        .map(|i| {
            let (a, b, c) = (at(i - reach as isize), at(i), at(i + reach as isize));
            let (u, v) = (b - a, c - b);
            u.cross(v).atan2(u.dot(v)).abs()
        })
        .collect();
    let mut corners: Vec<usize> = (0..n)
        .filter(|&i| {
            turn[i] > threshold
                && (1..=reach as isize).all(|d| {
                    let before = turn[(i as isize - d).rem_euclid(n as isize) as usize];
                    let after = turn[(i as isize + d).rem_euclid(n as isize) as usize];
                    turn[i] >= before && turn[i] > after
                })
        })
        .collect();
    // A corner sits on the pixel corner at one end of its edge: the one
    // standing further out from its neighbours, so a square stays square.
    let mut is_corner = vec![false; n];
    for &c in &corners {
        is_corner[c] = true;
        let mid = pts[(c + n - 1) % n].midpoint(pts[(c + 1) % n]);
        let (a, b) = (starts[c], starts[(c + 1) % n]);
        pts[c] = if (a - mid).hypot() > (b - mid).hypot() {
            a
        } else {
            b
        };
    }
    // Smooth the stretches between corners.
    let passes = (1.0 + s.smoothing.clamp(0.0, 100.0) / 100.0 * 12.0).round() as usize;
    for _ in 0..passes {
        let prev = pts.clone();
        for i in 0..n {
            if is_corner[i] {
                continue;
            }
            let a = prev[(i + n - 1) % n];
            let c = prev[(i + 1) % n];
            pts[i] = Point::new(
                (a.x + 2.0 * prev[i].x + c.x) / 4.0,
                (a.y + 2.0 * prev[i].y + c.y) / 4.0,
            );
        }
    }
    let tolerance = 0.2 + (100.0 - s.detail.clamp(0.0, 100.0)) / 100.0;
    let mut out = BezPath::new();
    if corners.is_empty() {
        // No corner: quarters meeting smoothly (a cubic bends through a
        // quarter turn well, a half turn only roughly).
        let tangent = |i: usize| unit(pts[(i + reach) % n] - pts[(i + n - reach) % n]);
        let cuts: Vec<usize> = (0..4).map(|k| k * n / 4).collect();
        out.move_to(pts[0]);
        for k in 0..4 {
            let (a, b) = (cuts[k], cuts[(k + 1) % 4]);
            let mut piece: Vec<Point> = if b > a {
                pts[a..=b].to_vec()
            } else {
                pts[a..].to_vec()
            };
            if b <= a {
                piece.push(pts[0]);
            }
            fit_cubics(&piece, tangent(a), -tangent(b), tolerance, &mut out);
        }
        out.close_path();
        return out;
    }
    corners.sort_unstable();
    out.move_to(pts[corners[0]]);
    for k in 0..corners.len() {
        let a = corners[k];
        let b = corners[(k + 1) % corners.len()];
        let mut piece: Vec<Point> = Vec::new();
        let mut i = a;
        loop {
            piece.push(pts[i]);
            if i == b && piece.len() > 1 {
                break;
            }
            i = (i + 1) % n;
            if piece.len() > n + 1 {
                break;
            }
        }
        if piece.len() < 2 {
            continue;
        }
        // The directions the sides leave and reach the corners in, over a
        // few pixels so a stair step does not tilt them.
        let m = piece.len();
        let far = TANGENT_REACH.min(m - 1);
        let t1 = unit(piece[far] - piece[0]);
        let t2 = unit(piece[m - 1 - far] - piece[m - 1]);
        fit_cubics(&piece, t1, t2, tolerance, &mut out);
    }
    out.close_path();
    out
}

/// How many points away a fitted curve's end direction is taken from.
const TANGENT_REACH: usize = 4;

fn unit(v: tracedraw_core::geometry::Vec2) -> tracedraw_core::geometry::Vec2 {
    let l = v.hypot();
    if l > 1e-12 && l.is_finite() {
        v / l
    } else {
        tracedraw_core::geometry::Vec2::ZERO
    }
}

/// Fit cubic Beziers to `pts` (from the first point to the last), the
/// first leaving along `t1` and the last arriving back along `t2` (unit
/// vectors pointing into the curve from its ends), no point further than
/// `tolerance` from them, and append them to `out`: least squares on the
/// chord-length parameters, improved by Newton steps, split at the worst
/// point when it still does not fit (P. J. Schneider, "An Algorithm for
/// Automatically Fitting Digitized Curves", Graphics Gems, 1990).
fn fit_cubics(
    pts: &[Point],
    t1: tracedraw_core::geometry::Vec2,
    t2: tracedraw_core::geometry::Vec2,
    tolerance: f64,
    out: &mut BezPath,
) {
    fit_cubics_depth(pts, t1, t2, tolerance, out, 0);
}

fn fit_cubics_depth(
    pts: &[Point],
    t1: tracedraw_core::geometry::Vec2,
    t2: tracedraw_core::geometry::Vec2,
    tolerance: f64,
    out: &mut BezPath,
    depth: usize,
) {
    use tracedraw_core::geometry::{CubicBez, ParamCurve, ParamCurveDeriv};
    let n = pts.len();
    if n < 2 {
        return;
    }
    let (p0, p3) = (pts[0], pts[n - 1]);
    let dist = (p3 - p0).hypot();
    if n == 2 || depth > 24 {
        let h = dist / 3.0;
        out.curve_to(p0 + t1 * h, p3 + t2 * h, p3);
        return;
    }
    // Chord-length parameters.
    let mut u = vec![0.0; n];
    for i in 1..n {
        u[i] = u[i - 1] + (pts[i] - pts[i - 1]).hypot();
    }
    let total = u[n - 1];
    if total < 1e-12 {
        return;
    }
    for v in u.iter_mut() {
        *v /= total;
    }
    let generate = |u: &[f64]| -> CubicBez {
        // The handle lengths minimising the squared distances.
        let (mut c00, mut c01, mut c11, mut x0, mut x1) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for (p, &t) in pts.iter().zip(u) {
            let mt = 1.0 - t;
            let b0 = mt * mt * mt;
            let b1 = 3.0 * t * mt * mt;
            let b2 = 3.0 * t * t * mt;
            let b3 = t * t * t;
            let a1 = t1 * b1;
            let a2 = t2 * b2;
            c00 += a1.dot(a1);
            c01 += a1.dot(a2);
            c11 += a2.dot(a2);
            let tmp = p.to_vec2() - (p0.to_vec2() * (b0 + b1) + p3.to_vec2() * (b2 + b3));
            x0 += a1.dot(tmp);
            x1 += a2.dot(tmp);
        }
        let det = c00 * c11 - c01 * c01;
        let (mut l1, mut l2) = if det.abs() > 1e-12 {
            ((x0 * c11 - x1 * c01) / det, (c00 * x1 - c01 * x0) / det)
        } else {
            (0.0, 0.0)
        };
        let eps = 1e-6 * dist;
        if !(l1.is_finite() && l2.is_finite()) || l1 < eps || l2 < eps {
            l1 = dist / 3.0;
            l2 = dist / 3.0;
        }
        CubicBez::new(p0, p0 + t1 * l1, p3 + t2 * l2, p3)
    };
    let worst = |c: &CubicBez, u: &[f64]| -> (f64, usize) {
        let mut best = (0.0, n / 2);
        for i in 1..n - 1 {
            let d = (c.eval(u[i]) - pts[i]).hypot2();
            if d > best.0 {
                best = (d, i);
            }
        }
        best
    };
    let mut curve = generate(&u);
    let (mut err, mut split) = worst(&curve, &u);
    let tol2 = tolerance * tolerance;
    if err <= tol2 {
        out.curve_to(curve.p1, curve.p2, curve.p3);
        return;
    }
    if err < tol2 * 4.0 {
        for _ in 0..4 {
            // Newton steps towards each point's nearest parameter.
            let d1 = curve.deriv();
            let d2 = d1.deriv();
            for i in 0..n {
                let t = u[i];
                let diff = curve.eval(t) - pts[i];
                let q1 = d1.eval(t).to_vec2();
                let q2 = d2.eval(t).to_vec2();
                let den = q1.dot(q1) + diff.dot(q2);
                if den.abs() > 1e-12 {
                    u[i] = (t - diff.dot(q1) / den).clamp(0.0, 1.0);
                }
            }
            curve = generate(&u);
            (err, split) = worst(&curve, &u);
            if err <= tol2 {
                out.curve_to(curve.p1, curve.p2, curve.p3);
                return;
            }
        }
    }
    let split = split.clamp(1, n - 2);
    let reach = TANGENT_REACH.min(split).min(n - 1 - split).max(1);
    let centre = unit(pts[split - reach] - pts[split + reach]);
    let centre = if centre == tracedraw_core::geometry::Vec2::ZERO {
        unit(pts[split - 1] - pts[split])
    } else {
        centre
    };
    fit_cubics_depth(&pts[..=split], t1, centre, tolerance, out, depth + 1);
    fit_cubics_depth(&pts[split..], -centre, t2, tolerance, out, depth + 1);
}

fn polyline_closed(pts: &[Point]) -> BezPath {
    let mut p = BezPath::new();
    if let Some(f) = pts.first() {
        p.move_to(*f);
        for q in &pts[1..] {
            p.line_to(*q);
        }
        p.close_path();
    }
    p
}

fn luma(p: Rgba<u8>) -> f32 {
    0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32
}

fn nearest(palette: &[[u8; 3]], c: [u8; 3]) -> usize {
    let mut best = (u32::MAX, 0);
    for (i, p) in palette.iter().enumerate() {
        let d = (0..3)
            .map(|k| (p[k] as i32 - c[k] as i32).pow(2) as u32)
            .sum::<u32>();
        if d < best.0 {
            best = (d, i);
        }
    }
    best.1
}

/// Median cut colour quantisation to `n` colours.
pub fn median_cut(img: &RgbaImage, n: usize) -> Vec<[u8; 3]> {
    let mut pixels: Vec<[u8; 3]> = img
        .pixels()
        .filter(|p| p[3] >= 128)
        .map(|p| [p[0], p[1], p[2]])
        .collect();
    if pixels.is_empty() {
        return vec![[0, 0, 0]];
    }
    // Subsample large images.
    if pixels.len() > 200_000 {
        let step = pixels.len() / 200_000 + 1;
        pixels = pixels.into_iter().step_by(step).collect();
    }
    let mut buckets: Vec<Vec<[u8; 3]>> = vec![pixels.clone()];
    while buckets.len() < n {
        // Split the bucket with the largest range.
        let mut best: Option<(usize, usize, i32)> = None; // (bucket, channel, range)
        for (bi, b) in buckets.iter().enumerate() {
            if b.len() < 2 {
                continue;
            }
            for ch in 0..3 {
                let (mn, mx) = b.iter().fold((255i32, 0i32), |(mn, mx), p| {
                    (mn.min(p[ch] as i32), mx.max(p[ch] as i32))
                });
                let r = mx - mn;
                if best.map(|b| r > b.2).unwrap_or(true) {
                    best = Some((bi, ch, r));
                }
            }
        }
        let Some((bi, ch, r)) = best else { break };
        if r == 0 {
            break;
        }
        let mut b = buckets.remove(bi);
        b.sort_by_key(|p| p[ch]);
        // Cut at the median, but never through a run of equal values: a
        // picture of mostly black and white would otherwise put both in
        // one bucket and average them to grey.
        let v = b[b.len() / 2][ch];
        let mut mid = b.partition_point(|p| p[ch] < v);
        if mid == 0 {
            mid = b.partition_point(|p| p[ch] <= v);
        }
        let hi = b.split_off(mid);
        buckets.push(b);
        buckets.push(hi);
    }
    let start: Vec<[u8; 3]> = buckets
        .iter()
        .filter(|b| !b.is_empty())
        .map(|b| {
            let n = b.len() as u32;
            let s = b.iter().fold([0u32; 3], |acc, p| {
                [
                    acc[0] + p[0] as u32,
                    acc[1] + p[1] as u32,
                    acc[2] + p[2] as u32,
                ]
            });
            [(s[0] / n) as u8, (s[1] / n) as u8, (s[2] / n) as u8]
        })
        .collect();
    refine(&pixels, start)
}

/// A few rounds of k-means from the median cut's colours: each colour
/// moves to the mean of the pixels nearest it, so the palette lands on
/// the colours the picture really has.
fn refine(pixels: &[[u8; 3]], mut palette: Vec<[u8; 3]>) -> Vec<[u8; 3]> {
    for _ in 0..6 {
        let mut sums = vec![[0u64; 4]; palette.len()];
        for p in pixels {
            let i = nearest(&palette, *p);
            for k in 0..3 {
                sums[i][k] += p[k] as u64;
            }
            sums[i][3] += 1;
        }
        let mut moved = false;
        for (c, s) in palette.iter_mut().zip(&sums) {
            if s[3] == 0 {
                continue;
            }
            let m = [
                (s[0] / s[3]) as u8,
                (s[1] / s[3]) as u8,
                (s[2] / s[3]) as u8,
            ];
            moved |= m != *c;
            *c = m;
        }
        if !moved {
            break;
        }
    }
    palette.dedup();
    palette
}

/// Boundary rings of a binary mask (pixel-edge following). Outer rings are
/// counter-clockwise and holes clockwise, so even-odd filling is right.
pub fn contours(mask: &[bool], w: usize, h: usize) -> Vec<Vec<Point>> {
    // Build the set of boundary edges between mask and non-mask cells, then
    // chain them into rings. Each edge is a directed unit segment keeping
    // the filled cell on its left.
    use std::collections::HashMap;
    let at = |x: i64, y: i64| -> bool {
        if x < 0 || y < 0 || x >= w as i64 || y >= h as i64 {
            false
        } else {
            mask[y as usize * w + x as usize]
        }
    };
    // Directed edges keyed by start vertex.
    let mut edges: HashMap<(i64, i64), Vec<(i64, i64)>> = HashMap::new();
    let mut push = |a: (i64, i64), b: (i64, i64)| edges.entry(a).or_default().push(b);
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            if !at(x, y) {
                continue;
            }
            // Image y grows down; walk so the filled cell is on the left in
            // image space (clockwise on screen), which flips to CCW in page space.
            if !at(x, y - 1) {
                push((x, y), (x + 1, y)); // top edge, left to right
            }
            if !at(x + 1, y) {
                push((x + 1, y), (x + 1, y + 1)); // right edge, down
            }
            if !at(x, y + 1) {
                push((x + 1, y + 1), (x, y + 1)); // bottom edge, right to left
            }
            if !at(x - 1, y) {
                push((x, y + 1), (x, y)); // left edge, up
            }
        }
    }
    let mut rings = Vec::new();
    let mut keys: Vec<(i64, i64)> = edges.keys().copied().collect();
    keys.sort_unstable();
    for start in keys {
        while let Some(first) = edges.get_mut(&start).and_then(|v| v.pop()) {
            let mut ring = vec![Point::new(start.0 as f64, start.1 as f64)];
            let mut cur = first;
            let mut prev = start;
            let mut guard = 0;
            while cur != start && guard < w * h * 4 {
                ring.push(Point::new(cur.0 as f64, cur.1 as f64));
                let Some(nexts) = edges.get_mut(&cur) else {
                    break;
                };
                if nexts.is_empty() {
                    break;
                }
                // Prefer turning right (keeps separate touching rings apart).
                let dir = (cur.0 - prev.0, cur.1 - prev.1);
                let right = (-dir.1, dir.0);
                let pick = nexts
                    .iter()
                    .position(|n| (n.0 - cur.0, n.1 - cur.1) == right)
                    .or_else(|| nexts.iter().position(|n| (n.0 - cur.0, n.1 - cur.1) == dir))
                    .unwrap_or(0);
                let next = nexts.remove(pick);
                prev = cur;
                cur = next;
                guard += 1;
            }
            // Drop collinear points.
            let mut compact: Vec<Point> = Vec::new();
            for p in ring {
                let n = compact.len();
                if n >= 2 {
                    let a = compact[n - 2];
                    let b = compact[n - 1];
                    if (b.x - a.x) * (p.y - b.y) - (b.y - a.y) * (p.x - b.x) == 0.0 {
                        compact.pop();
                    }
                }
                compact.push(p);
            }
            if compact.len() >= 3 {
                rings.push(compact);
            }
        }
    }
    rings
}

// ----- centreline mode --------------------------------------------------------

fn centerline(img: &RgbaImage, s: &Settings) -> Vec<Traced> {
    let (w, h) = (img.width() as usize, img.height() as usize);
    if w == 0 || h == 0 {
        return Vec::new();
    }
    let mut mask: Vec<bool> = img
        .pixels()
        .map(|p| p[3] >= 128 && luma(*p) < 128.0)
        .collect();
    // Mean stroke thickness before thinning: area / skeleton length later.
    let area = mask.iter().filter(|m| **m).count() as f64;
    thin(&mut mask, w, h);
    let skeleton = mask.iter().filter(|m| **m).count().max(1) as f64;
    let width = (area / skeleton).clamp(1.0, 50.0);
    // Trace skeleton pixels into polylines by walking neighbours.
    let mut visited = vec![false; w * h];
    let idx = |x: usize, y: usize| y * w + x;
    let neighbours = |x: usize, y: usize, mask: &[bool]| -> Vec<(usize, usize)> {
        let mut v = Vec::new();
        for dy in -1i64..=1 {
            for dx in -1i64..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                if nx >= 0
                    && ny >= 0
                    && (nx as usize) < w
                    && (ny as usize) < h
                    && mask[ny as usize * w + nx as usize]
                {
                    v.push((nx as usize, ny as usize));
                }
            }
        }
        v
    };
    let mut lines: Vec<Vec<Point>> = Vec::new();
    // Start from endpoints first (one neighbour), then anything left.
    let mut starts: Vec<(usize, usize)> = Vec::new();
    for y in 0..h {
        for x in 0..w {
            if mask[idx(x, y)] && neighbours(x, y, &mask).len() == 1 {
                starts.push((x, y));
            }
        }
    }
    for y in 0..h {
        for x in 0..w {
            if mask[idx(x, y)] {
                starts.push((x, y));
            }
        }
    }
    for (sx, sy) in starts {
        if visited[idx(sx, sy)] {
            continue;
        }
        let mut line = vec![Point::new(sx as f64 + 0.5, sy as f64 + 0.5)];
        visited[idx(sx, sy)] = true;
        let (mut x, mut y) = (sx, sy);
        loop {
            let next = neighbours(x, y, &mask)
                .into_iter()
                .find(|(nx, ny)| !visited[idx(*nx, *ny)]);
            let Some((nx, ny)) = next else { break };
            visited[idx(nx, ny)] = true;
            line.push(Point::new(nx as f64 + 0.5, ny as f64 + 0.5));
            x = nx;
            y = ny;
        }
        if line.len() >= 2 {
            lines.push(line);
        }
    }
    let tolerance = 0.5 + (100.0 - s.detail) / 100.0 * 2.0;
    let mut out = Vec::new();
    for l in lines {
        let simplified = tracedraw_core::geometry::simplify(&l, tolerance);
        if simplified.len() < 2 {
            continue;
        }
        let path = if s.smoothing > 0.0 && simplified.len() > 2 {
            tracedraw_core::geometry::smooth_path(&simplified, false)
        } else {
            tracedraw_core::geometry::polyline_path(&simplified, false)
        };
        out.push(Traced {
            path,
            color: Color::BLACK,
            closed: false,
        });
    }
    // Encode the stroke width in the first result's colour alpha? No: the
    // caller reads `stroke_width` through the returned tuple below.
    STROKE_WIDTH.with(|c| c.set(width));
    out
}

thread_local! {
    static STROKE_WIDTH: std::cell::Cell<f64> = const { std::cell::Cell::new(1.0) };
}

/// Mean stroke width (pixels) measured by the last centreline trace.
pub fn last_stroke_width_px() -> f64 {
    STROKE_WIDTH.with(|c| c.get())
}

/// Zhang-Suen thinning.
pub fn thin(mask: &mut [bool], w: usize, h: usize) {
    let at = |m: &[bool], x: i64, y: i64| -> bool {
        x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h && m[y as usize * w + x as usize]
    };
    loop {
        let mut changed = false;
        for pass in 0..2 {
            let mut remove = Vec::new();
            for y in 0..h as i64 {
                for x in 0..w as i64 {
                    if !at(mask, x, y) {
                        continue;
                    }
                    let p2 = at(mask, x, y - 1);
                    let p3 = at(mask, x + 1, y - 1);
                    let p4 = at(mask, x + 1, y);
                    let p5 = at(mask, x + 1, y + 1);
                    let p6 = at(mask, x, y + 1);
                    let p7 = at(mask, x - 1, y + 1);
                    let p8 = at(mask, x - 1, y);
                    let p9 = at(mask, x - 1, y - 1);
                    let ring = [p2, p3, p4, p5, p6, p7, p8, p9];
                    let b = ring.iter().filter(|v| **v).count();
                    if !(2..=6).contains(&b) {
                        continue;
                    }
                    let a = (0..8).filter(|i| !ring[*i] && ring[(i + 1) % 8]).count();
                    if a != 1 {
                        continue;
                    }
                    let (c1, c2) = if pass == 0 {
                        (p2 && p4 && p6, p4 && p6 && p8)
                    } else {
                        (p2 && p4 && p8, p2 && p6 && p8)
                    };
                    if c1 || c2 {
                        continue;
                    }
                    remove.push((x as usize, y as usize));
                }
            }
            if !remove.is_empty() {
                changed = true;
            }
            for (x, y) in remove {
                mask[y * w + x] = false;
            }
        }
        if !changed {
            break;
        }
    }
}

/// Map traced pixel-space paths into a rectangle (page mm, Y up).
/// Draw traced shapes (pixel space, y down) into a transparent image of
/// `w` x `h`: closed shapes filled with their colour, centrelines stroked
/// `stroke_px` wide. The trace dialog previews a trace this way.
pub fn render_traced(traced: &[Traced], w: u32, h: u32, stroke_px: f64) -> RgbaImage {
    let Some(mut pm) = tiny_skia::Pixmap::new(w.max(1), h.max(1)) else {
        return RgbaImage::new(w.max(1), h.max(1));
    };
    for t in traced {
        let mut pb = tiny_skia::PathBuilder::new();
        for el in t.path.elements() {
            match *el {
                tracedraw_core::geometry::PathEl::MoveTo(p) => pb.move_to(p.x as f32, p.y as f32),
                tracedraw_core::geometry::PathEl::LineTo(p) => pb.line_to(p.x as f32, p.y as f32),
                tracedraw_core::geometry::PathEl::QuadTo(a, p) => {
                    pb.quad_to(a.x as f32, a.y as f32, p.x as f32, p.y as f32)
                }
                tracedraw_core::geometry::PathEl::CurveTo(a, b, p) => pb.cubic_to(
                    a.x as f32, a.y as f32, b.x as f32, b.y as f32, p.x as f32, p.y as f32,
                ),
                tracedraw_core::geometry::PathEl::ClosePath => pb.close(),
            }
        }
        let Some(path) = pb.finish() else {
            continue;
        };
        let [r, g, b] = t.color.to_rgb8();
        let mut paint = tiny_skia::Paint::default();
        paint.set_color_rgba8(r, g, b, 255);
        paint.anti_alias = true;
        if t.closed {
            pm.fill_path(
                &path,
                &paint,
                tiny_skia::FillRule::EvenOdd,
                tiny_skia::Transform::identity(),
                None,
            );
        } else {
            let stroke = tiny_skia::Stroke {
                width: stroke_px.max(0.5) as f32,
                line_cap: tiny_skia::LineCap::Round,
                line_join: tiny_skia::LineJoin::Round,
                ..Default::default()
            };
            pm.stroke_path(
                &path,
                &paint,
                &stroke,
                tiny_skia::Transform::identity(),
                None,
            );
        }
    }
    let mut out = RgbaImage::new(pm.width(), pm.height());
    for (px, p) in out.pixels_mut().zip(pm.pixels()) {
        let c = p.demultiply();
        *px = image::Rgba([c.red(), c.green(), c.blue(), c.alpha()]);
    }
    out
}

pub fn to_rect(path: &BezPath, img_w: u32, img_h: u32, rect: Rect) -> BezPath {
    let sx = rect.width() / img_w.max(1) as f64;
    let sy = rect.height() / img_h.max(1) as f64;
    tracedraw_core::geometry::Affine::new([sx, 0.0, 0.0, -sy, rect.x0, rect.y1]) * path.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::ImageBuffer;
    use tracedraw_core::geometry::Shape as _;

    fn circle_image() -> RgbaImage {
        ImageBuffer::from_fn(100, 100, |x, y| {
            let d = ((x as f32 - 50.0).powi(2) + (y as f32 - 50.0).powi(2)).sqrt();
            if d < 30.0 {
                Rgba([0, 0, 0, 255])
            } else {
                Rgba([255, 255, 255, 255])
            }
        })
    }

    #[test]
    fn black_and_white_logos_keep_their_colours() {
        // A white disc on black with a black square inside and a soft
        // edge, mostly black pixels: the disc stays white, not grey.
        let img: RgbaImage = ImageBuffer::from_fn(120, 120, |x, y| {
            let d = ((x as f32 - 60.0).powi(2) + (y as f32 - 60.0).powi(2)).sqrt();
            let inner = (45..75).contains(&x) && (45..75).contains(&y);
            let v = if inner {
                0.0
            } else {
                (255.0 * (40.0 - d).clamp(0.0, 1.0)).round()
            };
            let v = v as u8;
            Rgba([v, v, v, 255])
        });
        for colors in [2, 8] {
            let mut s = Preset::Logo.settings();
            s.colors = colors;
            let t = trace(&img, &s);
            let biggest = t
                .iter()
                .max_by(|a, b| a.path.area().abs().total_cmp(&b.path.area().abs()))
                .expect("a region");
            let [r, g, b] = biggest.color.to_rgb8();
            assert!(
                r > 240 && g > 240 && b > 240,
                "{colors} colours: {r},{g},{b}"
            );
        }
        let pal = median_cut(&img, 2);
        assert!(
            pal.contains(&[0, 0, 0]) && pal.iter().any(|c| c[0] > 245),
            "{pal:?}"
        );
    }

    #[test]
    fn outline_traces_a_circle_with_right_area() {
        let img = circle_image();
        let t = trace(&img, &Preset::LineArt.settings());
        assert_eq!(t.len(), 1, "background removed, one region");
        let area = t[0].path.area().abs();
        let expect = std::f64::consts::PI * 30.0 * 30.0;
        assert!(
            (area - expect).abs() / expect < 0.05,
            "area {area} vs {expect}"
        );
        assert_eq!(t[0].color.to_rgb8(), [0, 0, 0]);
    }

    #[test]
    fn centerline_traces_a_line() {
        let img = ImageBuffer::from_fn(100, 40, |x, y| {
            if (18..=22).contains(&y) && x > 5 && x < 95 {
                Rgba([0, 0, 0, 255])
            } else {
                Rgba([255, 255, 255, 255])
            }
        });
        let t = trace(&img, &Preset::Technical.settings());
        assert!(!t.is_empty());
        let b = t[0].path.bounding_box();
        assert!(b.width() > 80.0 && b.height() < 3.0, "{b:?}");
        assert!((last_stroke_width_px() - 5.0).abs() < 1.5);
    }

    /// Smooth outlines from pixels (#611): a circle becomes a few curves,
    /// no stair steps, at the right size; a square keeps four sharp
    /// corners on its pixel corners; stray specks make no curves.
    #[test]
    fn traces_are_smooth_curves_with_sharp_corners_and_no_specks() {
        use tracedraw_core::geometry::PathEl;
        let s = Preset::LineArt.settings();
        let t = trace(&circle_image(), &s);
        assert_eq!(t.len(), 1);
        let els = t[0].path.elements();
        let curves = els
            .iter()
            .filter(|e| matches!(e, PathEl::CurveTo(..)))
            .count();
        assert!(
            els.iter().all(|e| !matches!(e, PathEl::LineTo(..))),
            "no steps"
        );
        assert!((2..=12).contains(&curves), "{curves} curves");
        let area = t[0].path.area().abs();
        let expect = std::f64::consts::PI * 30.0 * 30.0;
        assert!((area - expect).abs() / expect < 0.02, "{area} vs {expect}");
        // No point of the curves strays from the circle by more than the
        // fitting tolerance (1.2 px at detail 40).
        for (p, _) in t[0].path.segments().flat_map(|seg| {
            (0..=8).map(move |k| {
                (
                    tracedraw_core::geometry::ParamCurve::eval(&seg, k as f64 / 8.0),
                    (),
                )
            })
        }) {
            let r = (p - Point::new(50.5, 50.5)).hypot();
            assert!((r - 30.0).abs() < 1.25, "{r}");
        }

        // A square with specks around it.
        let img: RgbaImage = ImageBuffer::from_fn(100, 100, |x, y| {
            let square = (20..80).contains(&x) && (20..80).contains(&y);
            let speck = (x % 13 == 3 && y % 17 == 5) && !square;
            if square || speck {
                Rgba([0, 0, 0, 255])
            } else {
                Rgba([255, 255, 255, 255])
            }
        });
        let t = trace(&img, &s);
        assert_eq!(t.len(), 1, "the specks are gone");
        let path = &t[0].path;
        let subpaths = path
            .elements()
            .iter()
            .filter(|e| matches!(e, PathEl::MoveTo(_)))
            .count();
        assert_eq!(subpaths, 1, "the square alone");
        let b = path.bounding_box();
        assert!(
            (b.x0 - 20.0).abs() < 0.01 && (b.x1 - 80.0).abs() < 0.01,
            "sharp corners on the pixel corners: {b:?}"
        );
        assert!((path.area().abs() - 3600.0).abs() < 1.0, "{}", path.area());
        // With all the detail, a speck of one pixel stays.
        let mut keep = s.clone();
        keep.detail = 100.0;
        assert!(
            trace(&img, &keep)[0]
                .path
                .elements()
                .iter()
                .filter(|e| matches!(e, PathEl::MoveTo(_)))
                .count()
                > 1
        );
    }

    /// Patches smaller than the detail allows take the colour around them.
    #[test]
    fn despeckle_fills_small_patches_with_their_surroundings() {
        let (w, h) = (6, 5);
        let mut labels = vec![0usize; w * h];
        labels[2 * w + 2] = 1; // a one-pixel speck
        for x in 0..w {
            labels[4 * w + x] = 2; // a row of six
        }
        labels[0] = usize::MAX; // transparent stays
        despeckle(&mut labels, w, h, 3);
        assert_eq!(labels[2 * w + 2], 0);
        assert!(labels[4 * w..].iter().all(|l| *l == 2));
        assert_eq!(labels[0], usize::MAX);
        assert_eq!(speck_area(100.0), 1);
        assert!(speck_area(35.0) > speck_area(80.0));
    }

    #[test]
    fn contours_of_a_square_with_hole() {
        let w = 10;
        let mut mask = vec![false; 100];
        for y in 2..8 {
            for x in 2..8 {
                mask[y * w + x] = true;
            }
        }
        for y in 4..6 {
            for x in 4..6 {
                mask[y * w + x] = false;
            }
        }
        let rings = contours(&mask, w, 10);
        assert_eq!(rings.len(), 2);
    }
}
