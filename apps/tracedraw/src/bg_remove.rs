//! Image > Remove Background: makes the background of a picture
//! transparent on this computer, without an AI service.
//!
//! 1. What to remove is a list: each entry is Automatic (everything but
//!    the subject of the picture, found by `bg_subject`, whatever its
//!    colours) or a colour the user picked, with its own tolerance.
//! 2. For the picked colours, every pixel gets its colour distance
//!    (CIE76 Delta E) to the one it is closest to within that colour's
//!    tolerance; it is background when it is within the tolerance of any
//!    of them.
//! 3. With Contiguous on, such background is what a flood fill from the
//!    border (from the points where colours were picked, and from what
//!    Automatic took) reaches through such pixels; it may cross a few
//!    edge pixels (the anti-aliased rim) but never the subject, so
//!    background-coloured parts inside the subject stay. Off, every
//!    pixel near a picked colour goes.
//! 4. Opacity ramps over a band past the tolerance (soft, anti-aliased
//!    edges), small loose specks are removed, and Edge softness feathers
//!    the edge.
//! 5. Clean edge colours takes the background's colour out of the
//!    half-transparent edge pixels, so no halo of the old backdrop shows
//!    on a new one.
//!
//! 6. The Erase and Restore brushes (#1723, #1915) then take away, or
//!    bring back at full opacity, what was painted over, whatever the
//!    colours said.
//! 7. A backdrop colour or gradient, when chosen, fills behind the
//!    result, so the picture is ready to export on its new background.
//!
//! See docs/behavior/remove-background.md.

use image::RgbaImage;
use std::collections::VecDeque;

/// The dialog's settings.
#[derive(Debug, Clone, PartialEq)]
pub struct BgSettings {
    /// How far (Delta E) a colour may be from the background's and still
    /// be background: the tolerance of every listed colour that has none
    /// of its own.
    pub tolerance: f32,
    /// The feather of the edge, in pixels of the picture.
    pub softness: f32,
    /// Only the background connected to the picture's border goes.
    pub contiguous: bool,
    /// Loose specks left in the background go too.
    pub despeckle: bool,
    /// The backdrop's colour is taken out of half-transparent edge pixels.
    pub clean_edges: bool,
    /// The colours to remove; empty is one Automatic entry.
    pub colors: Vec<BgPick>,
    /// Brush strokes painted on the preview, in order: Erase takes the
    /// painted pixels away, Restore brings them back (#1723, #1915).
    pub strokes: Vec<BgStroke>,
    /// A colour or gradient put behind the result; None leaves the
    /// background transparent (#1915).
    pub backdrop: Option<BgBackdrop>,
}

/// One brush stroke on the preview: the pixels within `radius` of the
/// line through `points` are taken away (Erase) or kept whole (Restore).
#[derive(Debug, Clone, PartialEq)]
pub struct BgStroke {
    /// Restore brush; false: Erase brush.
    pub restore: bool,
    /// The brush's radius, as a fraction of the picture's larger side,
    /// so it covers the same part of the picture on any copy.
    pub radius: f32,
    /// The points the brush went through, as fractions of the picture's
    /// width and height.
    pub points: Vec<[f32; 2]>,
}

/// What fills behind the picture once its background is gone: a colour
/// at the top fading to one at the bottom (the same colour twice is a
/// plain fill).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BgBackdrop {
    pub top: [u8; 3],
    pub bottom: [u8; 3],
}

impl BgBackdrop {
    /// A plain white backdrop.
    pub const WHITE: BgBackdrop = BgBackdrop {
        top: [255, 255, 255],
        bottom: [255, 255, 255],
    };

    /// The backdrop's colour on row `y` of `h`.
    fn at(&self, y: u32, h: u32) -> [f32; 3] {
        let t = if h > 1 {
            y as f32 / (h - 1) as f32
        } else {
            0.0
        };
        let mut c = [0.0; 3];
        for (k, v) in c.iter_mut().enumerate() {
            *v = self.top[k] as f32 + (self.bottom[k] as f32 - self.top[k] as f32) * t;
        }
        c
    }
}

/// One colour to remove: Automatic (learnt from the border) or picked,
/// with its own tolerance or the dialog's.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BgPick {
    /// The picked colour; None: Automatic.
    pub color: Option<[u8; 3]>,
    /// Its tolerance (Delta E); None: the dialog's Tolerance.
    pub tolerance: Option<f32>,
    /// Where it was picked, as fractions of the picture's width and
    /// height: with Contiguous on the fill starts there too, so a colour
    /// picked inside the subject (a hole) goes.
    pub seed: Option<[f32; 2]>,
}

impl BgPick {
    /// The entry that learns the background from the border.
    pub const AUTO: BgPick = BgPick {
        color: None,
        tolerance: None,
        seed: None,
    };

    /// A picked colour with the dialog's tolerance and no seed.
    pub fn color(c: [u8; 3]) -> Self {
        BgPick {
            color: Some(c),
            ..BgPick::AUTO
        }
    }

    /// This entry's tolerance, `global` when it has none of its own.
    pub fn tolerance_or(&self, global: f32) -> f32 {
        self.tolerance.unwrap_or(global).max(0.0)
    }
}

impl BgSettings {
    /// The colours to remove, never empty: an empty list is Automatic.
    pub fn entries(&self) -> Vec<BgPick> {
        if self.colors.is_empty() {
            vec![BgPick::AUTO]
        } else {
            self.colors.clone()
        }
    }

    /// Add a picked colour (with the seed where it was picked) unless
    /// the list has it already; true when added.
    pub fn add_color(&mut self, c: [u8; 3], seed: Option<[f32; 2]>) -> bool {
        if self.colors.is_empty() {
            self.colors.push(BgPick::AUTO);
        }
        if self.colors.iter().any(|p| p.color == Some(c)) {
            return false;
        }
        self.colors.push(BgPick {
            seed,
            ..BgPick::color(c)
        });
        true
    }

    /// Remove the listed entry nearest in colour to `c` (Automatic
    /// entries by the nearest of `learnt`, the colours learnt from the
    /// picture; on a tie the picked entry); the list never ends up empty: the last entry gone,
    /// Automatic is left. The index removed comes back.
    pub fn remove_nearest(&mut self, c: [u8; 3], learnt: &[BgColor]) -> Option<usize> {
        let l = lab(c);
        let entries = self.entries();
        let best = entries
            .iter()
            .enumerate()
            .filter_map(|(i, p)| {
                // On a tie a picked entry goes before Automatic (whose
                // learnt colours may hold the picked one too).
                let d = match p.color {
                    Some(pc) => (dist2(&lab(pc), &l), 0u8),
                    None => (
                        learnt.iter().map(|b| dist2(&b.lab, &l)).reduce(f32::min)?,
                        1u8,
                    ),
                };
                Some((i, d))
            })
            .min_by(|a, b| a.1 .0.total_cmp(&b.1 .0).then(a.1 .1.cmp(&b.1 .1)))?
            .0;
        self.remove(best);
        Some(best)
    }

    /// Remove entry `i`; the last one gone, Automatic is left.
    pub fn remove(&mut self, i: usize) {
        if self.colors.is_empty() {
            self.colors.push(BgPick::AUTO);
        }
        if i < self.colors.len() {
            self.colors.remove(i);
        }
        if self.colors.is_empty() {
            self.colors.push(BgPick::AUTO);
        }
    }
}

impl Default for BgSettings {
    fn default() -> Self {
        BgSettings {
            tolerance: 14.0,
            softness: 1.0,
            contiguous: true,
            despeckle: true,
            clean_edges: true,
            colors: vec![BgPick::AUTO],
            strokes: Vec::new(),
            backdrop: None,
        }
    }
}

type Lab = [f32; 3];

fn lab(c: [u8; 3]) -> Lab {
    let (l, a, b) = tracedraw_core::color::rgb_to_lab(
        c[0] as f32 / 255.0,
        c[1] as f32 / 255.0,
        c[2] as f32 / 255.0,
    );
    [l, a, b]
}

fn dist2(a: &Lab, b: &Lab) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

/// A background colour: in Lab for distances, in RGB for cleaning edges.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BgColor {
    pub lab: Lab,
    pub rgb: [f32; 3],
}

impl BgColor {
    fn of(c: [u8; 3]) -> Self {
        BgColor {
            lab: lab(c),
            rgb: [c[0] as f32, c[1] as f32, c[2] as f32],
        }
    }
}

/// The pixels of the picture's border, two pixels deep (opaque ones).
fn border(img: &RgbaImage) -> Vec<[u8; 3]> {
    let (w, h) = img.dimensions();
    let depth = 2.min(w).min(h);
    let mut out = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let edge = x < depth || y < depth || x + depth >= w || y + depth >= h;
            if !edge {
                continue;
            }
            let p = img.get_pixel(x, y).0;
            if p[3] >= 128 {
                out.push([p[0], p[1], p[2]]);
            }
        }
    }
    out
}

/// The colours of the picture's border: up to six k-means clusters in
/// Lab, each kept when it covers at least a fifteenth of the border.
/// None when the border is mostly transparent already. Automatic does
/// not remove by them (it finds the subject, `bg_subject`); they say
/// which listed entry a colour clicked with Alt is nearest to.
pub fn learn_background(img: &RgbaImage) -> Vec<BgColor> {
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return Vec::new();
    }
    let px = border(img);
    let ring = (2 * (w as usize + h as usize)).max(1);
    if px.len() * 3 < ring {
        return Vec::new();
    }
    // A sample of at most 4096 pixels, evenly spread.
    let step = (px.len() / 4096).max(1);
    let pts: Vec<([u8; 3], Lab)> = px.iter().step_by(step).map(|c| (*c, lab(*c))).collect();
    // Farthest-point seeds: the first point, then the one farthest from
    // every centre so far, while it is far enough to be another colour.
    let mut centres: Vec<Lab> = vec![pts[0].1];
    while centres.len() < 6 {
        let (far, d) = pts
            .iter()
            .map(|(_, l)| {
                let d = centres.iter().map(|c| dist2(l, c)).fold(f32::MAX, f32::min);
                (*l, d)
            })
            .fold(([0.0; 3], -1.0), |a, b| if b.1 > a.1 { b } else { a });
        if d < 10.0f32.powi(2) {
            break;
        }
        centres.push(far);
    }
    let mut members = vec![0usize; pts.len()];
    for _ in 0..10 {
        let mut sum = vec![[0.0f64; 6]; centres.len()];
        let mut n = vec![0usize; centres.len()];
        for (i, (rgb, l)) in pts.iter().enumerate() {
            let k = nearest(&centres, l).0;
            members[i] = k;
            n[k] += 1;
            for j in 0..3 {
                sum[k][j] += l[j] as f64;
                sum[k][3 + j] += rgb[j] as f64;
            }
        }
        for (k, c) in centres.iter_mut().enumerate() {
            if n[k] > 0 {
                for j in 0..3 {
                    c[j] = (sum[k][j] / n[k] as f64) as f32;
                }
            }
        }
    }
    let mut out = Vec::new();
    for (k, centre) in centres.iter().enumerate() {
        let mine: Vec<&[u8; 3]> = pts
            .iter()
            .zip(&members)
            .filter(|(_, m)| **m == k)
            .map(|((rgb, _), _)| rgb)
            .collect();
        if mine.len() * 15 < pts.len() {
            continue;
        }
        let mut rgb = [0.0f32; 3];
        for c in &mine {
            for j in 0..3 {
                rgb[j] += c[j] as f32;
            }
        }
        for v in &mut rgb {
            *v /= mine.len() as f32;
        }
        out.push(BgColor { lab: *centre, rgb });
    }
    out
}

fn nearest(centres: &[Lab], l: &Lab) -> (usize, f32) {
    centres
        .iter()
        .enumerate()
        .map(|(i, c)| (i, dist2(l, c)))
        .fold((0, f32::MAX), |a, b| if b.1 < a.1 { b } else { a })
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The picture with its background transparent. `scale` is the size of
/// `img` against the bitmap (previews are smaller), so the softness keeps
/// its look.
pub fn remove_background(img: &RgbaImage, s: &BgSettings, scale: f32) -> RgbaImage {
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return img.clone();
    }
    let mut out = remove_by_colour(img, s, scale);
    if let Some(b) = s.backdrop {
        put_backdrop(&mut out, &b);
    }
    out
}

/// The picture with its background (and the brushes' strokes) made
/// transparent: an Automatic entry takes everything but the subject
/// (`bg_subject`), a picked colour what is near it.
fn remove_by_colour(img: &RgbaImage, s: &BgSettings, scale: f32) -> RgbaImage {
    let (w, h) = img.dimensions();
    let entries = s.entries();
    // The picked colours with their tolerances, and Automatic's.
    let mut bg: Vec<BgColor> = Vec::new();
    let mut tols: Vec<f32> = Vec::new();
    let mut auto: Option<f32> = None;
    for p in &entries {
        let t = p.tolerance_or(s.tolerance);
        match p.color {
            Some(c) => {
                bg.push(BgColor::of(c));
                tols.push(t);
            }
            None => auto = Some(auto.map_or(t, |a: f32| a.max(t))),
        }
    }
    let subject = auto.and_then(|t| crate::bg_subject::subject(img, t, s.contiguous));
    let (wu, hu) = (w as usize, h as usize);
    let n = wu * hu;
    if bg.is_empty() && subject.is_none() {
        // Nothing to take (a picture already transparent all round):
        // only the brushes change it.
        let mut out = img.clone();
        if !s.strokes.is_empty() {
            let mut keep = vec![1.0f32; n];
            paint_strokes(&mut keep, wu, hu, &s.strokes);
            for (p, a) in out.pixels_mut().zip(&keep) {
                p.0[3] = (p.0[3] as f32 * a).round().clamp(0.0, 255.0) as u8;
            }
        }
        return out;
    }
    // Opacity of the subject, 1 where it stays.
    let mut keep = match &subject {
        Some(sub) => sub.keep.clone(),
        None => vec![1.0f32; n],
    };
    // The picked colour that took each pixel; none: Automatic did.
    const AUTO: u16 = u16::MAX;
    let mut which = vec![AUTO; if bg.is_empty() { 0 } else { n }];
    if !bg.is_empty() {
        by_picked_colours(img, s, &entries, &bg, &tols, &mut keep, &mut which);
    }
    if s.despeckle {
        despeckle(&mut keep, wu, hu);
    }
    // The brushes have the last word, before the edge is feathered.
    paint_strokes(&mut keep, wu, hu, &s.strokes);
    let radius = s.softness.max(0.0) * scale;
    // The opacity before the feather: what Automatic's edge colours are
    // cleaned by (how much of each pixel is backdrop).
    let mut matte: Option<Vec<f32>> = None;
    if radius >= 0.5 {
        if s.clean_edges && subject.is_some() {
            matte = Some(keep.clone());
        }
        let r = radius.round() as usize;
        // Two box passes approach a Gaussian; only the edge changes,
        // since flat areas stay flat.
        box_blur(&mut keep, wu, hu, r);
        box_blur(&mut keep, wu, hu, r);
    }
    let mut out = img.clone();
    for (i, p) in out.pixels_mut().enumerate() {
        let a = keep[i];
        if a <= 0.0 {
            p.0[3] = 0;
            continue;
        }
        if s.clean_edges {
            match (which.get(i).and_then(|k| bg.get(*k as usize)), &subject) {
                (Some(c), _) => {
                    if a > 0.05 && a < 0.999 {
                        // The pixel was a * subject + (1 - a) * backdrop:
                        // solve for the subject's colour.
                        for (ch, back) in p.0.iter_mut().zip(c.rgb) {
                            let v = *ch as f32;
                            *ch = ((v - (1.0 - a) * back) / a).round().clamp(0.0, 255.0) as u8;
                        }
                    }
                }
                (None, Some(sub)) => {
                    // The same, by the opacity before the feather.
                    let m = matte.as_ref().map_or(a, |v| v[i]);
                    let c = [p.0[0], p.0[1], p.0[2]];
                    if let Some(c) = sub.cleaned(i % wu, i / wu, c, m) {
                        p.0[..3].copy_from_slice(&c);
                    }
                }
                (None, None) => {}
            }
        }
        if a < 0.999 {
            p.0[3] = (p.0[3] as f32 * a).round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

/// `img` shrunk to `w` x `h` with its colours weighted by their opacity,
/// so the colour a removed pixel kept does not bleed into the edge (the
/// preview of the result, see `bitmap_dialogs::bg_preview_result`).
pub fn shrink_result(img: &RgbaImage, w: u32, h: u32) -> RgbaImage {
    let (w, h) = (w.max(1), h.max(1));
    if img.dimensions() == (w, h) {
        return img.clone();
    }
    // (The resize keeps float channels within 0 to 1.)
    let mut pre = image::Rgba32FImage::new(img.width(), img.height());
    for (to, from) in pre.pixels_mut().zip(img.pixels()) {
        let a = from.0[3] as f32 / 255.0;
        to.0 = [
            from.0[0] as f32 / 255.0 * a,
            from.0[1] as f32 / 255.0 * a,
            from.0[2] as f32 / 255.0 * a,
            a,
        ];
    }
    let small = image::imageops::resize(&pre, w, h, image::imageops::FilterType::Triangle);
    let mut out = RgbaImage::new(w, h);
    for (to, from) in out.pixels_mut().zip(small.pixels()) {
        let a = from.0[3].clamp(0.0, 1.0);
        if a <= 1.0 / 512.0 {
            to.0 = [0, 0, 0, 0];
            continue;
        }
        let ch = |v: f32| (v / a * 255.0).round().clamp(0.0, 255.0) as u8;
        to.0 = [
            ch(from.0[0]),
            ch(from.0[1]),
            ch(from.0[2]),
            (a * 255.0).round() as u8,
        ];
    }
    out
}

/// Take what is near a picked colour out of `keep` (which holds what
/// Automatic left, or 1 everywhere): `which` gets the colour that took
/// each pixel. What Automatic took counts as background here, so with
/// Contiguous a picked colour next to it goes too.
fn by_picked_colours(
    img: &RgbaImage,
    s: &BgSettings,
    entries: &[BgPick],
    bg: &[BgColor],
    tols: &[f32],
    keep: &mut [f32],
    which: &mut [u16],
) {
    let (w, h) = img.dimensions();
    let (wu, hu) = (w as usize, h as usize);
    let n = wu * hu;
    // Each pixel in Lab, its distance to the background colour it is
    // closest to within that colour's tolerance, which one, and that
    // colour's tolerance: a pixel is background when it is within the
    // tolerance of any listed colour.
    let mut labs = vec![[0.0f32; 3]; n];
    let mut dist = vec![0.0f32; n];
    let mut near = vec![0u16; n];
    let mut tolp = vec![tols[0]; n];
    // Taken by Automatic already.
    let taken: Vec<bool> = keep.iter().map(|k| *k < 0.5).collect();
    for (i, p) in img.pixels().enumerate() {
        if p.0[3] == 0 {
            continue;
        }
        let l = lab([p.0[0], p.0[1], p.0[2]]);
        let (k, d) = if bg.len() == 1 {
            (0, dist2(&bg[0].lab, &l).sqrt())
        } else {
            // The colour it is deepest inside (least distance past, or
            // most within, that colour's tolerance).
            let mut best = (0, f32::MAX, f32::MAX);
            for (k, (b, t)) in bg.iter().zip(tols).enumerate() {
                let d = dist2(&b.lab, &l).sqrt();
                if d - t < best.2 {
                    best = (k, d, d - t);
                }
            }
            (best.0, best.1)
        };
        labs[i] = l;
        dist[i] = d;
        near[i] = k as u16;
        tolp[i] = tols[k];
    }
    // The ramp past the tolerance where edge pixels are partly opaque.
    let band = |tol: f32| (tol * 0.6).max(4.0);
    let ramp = |d: f32, tol: f32| smoothstep(tol, tol + band(tol), d);
    let mut take = |i: usize, a: f32| {
        if a < keep[i] {
            keep[i] = a;
            which[i] = near[i];
        }
    };
    if s.contiguous {
        // Flood fill from the border (and the points where colours were
        // picked, and what Automatic took): through background pixels
        // freely, and at most `rim` pixels into the ramp, so the fill
        // takes the anti-aliased rim and stops at the subject. It also
        // walks on from a background pixel to a neighbour of nearly the
        // same colour (a step under `step`), while that neighbour is
        // within `reach` of a background colour: so a backdrop that
        // drifts (texture, a gradient the border did not show, a soft
        // shadow) goes whole, while the subject's edge, a sharp step,
        // stops it.
        let rim = 2 + s.softness.max(0.0).ceil() as u8;
        let step2 = |tol: f32| (tol / 3.0).max(3.0).powi(2);
        let reach = |tol: f32| (tol * 3.0).max(tol + band(tol));
        let mut depth = vec![u8::MAX; n];
        // Reached by such steps: background, whatever its distance.
        let mut drifted = vec![false; n];
        let mut queue = VecDeque::new();
        let seed = |i: usize, depth: &mut Vec<u8>, queue: &mut VecDeque<usize>| {
            let t = tolp[i];
            if depth[i] == u8::MAX && dist[i] < t + band(t) {
                depth[i] = if dist[i] <= t { 0 } else { 1 };
                if depth[i] <= rim {
                    queue.push_back(i);
                }
            }
        };
        for x in 0..wu {
            seed(x, &mut depth, &mut queue);
            seed((hu - 1) * wu + x, &mut depth, &mut queue);
        }
        for y in 0..hu {
            seed(y * wu, &mut depth, &mut queue);
            seed(y * wu + wu - 1, &mut depth, &mut queue);
        }
        // A picked point and the pixels around it (the point was taken
        // on a copy of another size).
        for at in entries.iter().filter_map(|p| p.seed) {
            if !(at[0].is_finite() && at[1].is_finite()) {
                continue;
            }
            let cx = ((at[0] * w as f32) as i64).clamp(0, wu as i64 - 1);
            let cy = ((at[1] * h as f32) as i64).clamp(0, hu as i64 - 1);
            for y in (cy - 1).max(0)..=(cy + 1).min(hu as i64 - 1) {
                for x in (cx - 1).max(0)..=(cx + 1).min(wu as i64 - 1) {
                    seed(y as usize * wu + x as usize, &mut depth, &mut queue);
                }
            }
        }
        // What Automatic took is background the fill passes through.
        for i in 0..n {
            if taken[i] && depth[i] == u8::MAX {
                depth[i] = 0;
                queue.push_back(i);
            }
        }
        while let Some(i) = queue.pop_front() {
            let (x, y) = (i % wu, i / wu);
            let here = depth[i];
            let step_i = step2(tolp[i]);
            let mut visit = |j: usize| {
                if depth[j] != u8::MAX {
                    return;
                }
                let t = tolp[j];
                let near_j = dist[j] < t + band(t);
                // A step from solid background of a picked colour only,
                // never from the rim or from what Automatic took.
                let drift = here == 0
                    && !taken[i]
                    && dist[j] < reach(t)
                    && img.get_pixel((j % wu) as u32, (j / wu) as u32).0[3] > 0
                    && dist2(&labs[i], &labs[j]) <= step_i;
                if !near_j && !drift {
                    return;
                }
                let d = if dist[j] <= t || drift { 0 } else { here + 1 };
                if d > rim {
                    return;
                }
                depth[j] = d;
                drifted[j] = drift && dist[j] > t;
                queue.push_back(j);
            };
            if x > 0 {
                visit(i - 1);
            }
            if x + 1 < wu {
                visit(i + 1);
            }
            if y > 0 {
                visit(i - wu);
            }
            if y + 1 < hu {
                visit(i + wu);
            }
        }
        for i in 0..n {
            if taken[i] {
                continue;
            }
            if drifted[i] {
                take(i, 0.0);
            } else if depth[i] != u8::MAX {
                take(i, ramp(dist[i], tolp[i]));
            }
        }
    } else {
        for i in 0..n {
            take(i, ramp(dist[i], tolp[i]));
        }
    }
}

/// The brushes' strokes on the opacity map: within a stroke's radius of
/// its line, Erase sets 0 and Restore 1, blending over the last pixel of
/// the rim so the brush's edge is smooth.
fn paint_strokes(keep: &mut [f32], w: usize, h: usize, strokes: &[BgStroke]) {
    if w == 0 || h == 0 {
        return;
    }
    let side = w.max(h) as f32;
    for st in strokes {
        let r = (st.radius * side).max(0.5);
        if !r.is_finite() {
            continue;
        }
        let target = if st.restore { 1.0 } else { 0.0 };
        let pts: Vec<(f32, f32)> = st
            .points
            .iter()
            .filter(|p| p[0].is_finite() && p[1].is_finite())
            .map(|p| (p[0] * w as f32, p[1] * h as f32))
            .collect();
        let Some(&first) = pts.first() else {
            continue;
        };
        // A lone dab is a segment from the point to itself.
        let segs: Vec<((f32, f32), (f32, f32))> = if pts.len() == 1 {
            vec![(first, first)]
        } else {
            pts.windows(2).map(|p| (p[0], p[1])).collect()
        };
        for (a, b) in segs {
            let x0 = ((a.0.min(b.0) - r - 1.0).floor().max(0.0)) as usize;
            let y0 = ((a.1.min(b.1) - r - 1.0).floor().max(0.0)) as usize;
            let x1 = ((a.0.max(b.0) + r + 1.0).ceil().max(0.0) as usize).min(w);
            let y1 = ((a.1.max(b.1) + r + 1.0).ceil().max(0.0) as usize).min(h);
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let len2 = dx * dx + dy * dy;
            for y in y0..y1 {
                for x in x0..x1 {
                    let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                    let t = if len2 > 0.0 {
                        (((px - a.0) * dx + (py - a.1) * dy) / len2).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    let (qx, qy) = (a.0 + dx * t - px, a.1 + dy * t - py);
                    let d = (qx * qx + qy * qy).sqrt();
                    let v = (r - d + 0.5).clamp(0.0, 1.0);
                    if v > 0.0 {
                        let k = &mut keep[y * w + x];
                        *k = *k * (1.0 - v) + target * v;
                    }
                }
            }
        }
    }
}

/// Fill behind the picture with the backdrop: every pixel ends opaque.
fn put_backdrop(img: &mut RgbaImage, b: &BgBackdrop) {
    let h = img.height();
    for (_, y, p) in img.enumerate_pixels_mut() {
        let back = b.at(y, h);
        let a = p.0[3] as f32 / 255.0;
        for (ch, bk) in p.0.iter_mut().zip(back) {
            *ch = (*ch as f32 * a + bk * (1.0 - a)).round().clamp(0.0, 255.0) as u8;
        }
        p.0[3] = 255;
    }
}

/// Loose specks: kept parts not touching anything larger, smaller than
/// a two-thousandth of the picture (at least 12 pixels), go.
fn despeckle(keep: &mut [f32], w: usize, h: usize) {
    let n = w * h;
    let limit = (n / 2000).max(12);
    let mut seen = vec![false; n];
    let mut part = Vec::new();
    let mut stack = Vec::new();
    for start in 0..n {
        if seen[start] || keep[start] < 0.5 {
            continue;
        }
        part.clear();
        stack.push(start);
        seen[start] = true;
        while let Some(i) = stack.pop() {
            part.push(i);
            let (x, y) = (i % w, i / w);
            let mut go = |j: usize| {
                if !seen[j] && keep[j] >= 0.5 {
                    seen[j] = true;
                    stack.push(j);
                }
            };
            if x > 0 {
                go(i - 1);
            }
            if x + 1 < w {
                go(i + 1);
            }
            if y > 0 {
                go(i - w);
            }
            if y + 1 < h {
                go(i + w);
            }
        }
        if part.len() < limit && part.len() * 4 < n {
            for &i in &part {
                keep[i] = 0.0;
            }
        }
    }
}

/// A box blur of radius `r` across rows and then columns (edges
/// repeated), by running sums.
pub(crate) fn box_blur(v: &mut [f32], w: usize, h: usize, r: usize) {
    if r == 0 || w == 0 || h == 0 || v.len() < w * h {
        return;
    }
    let inv = 1.0 / (2 * r + 1) as f32;
    // Rows.
    let mut line = vec![0.0f32; w];
    for row in v.chunks_exact_mut(w).take(h) {
        line.copy_from_slice(row);
        let mut sum = line[0] * (r + 1) as f32;
        for i in 1..=r {
            sum += line[i.min(w - 1)];
        }
        for x in 0..w {
            row[x] = sum * inv;
            sum += line[(x + r + 1).min(w - 1)] - line[x.saturating_sub(r)];
        }
    }
    // Columns: one running sum per column, row after row.
    let src = v[..w * h].to_vec();
    let at = |y: usize| &src[y * w..(y + 1) * w];
    let mut sums: Vec<f32> = at(0).iter().map(|a| a * (r + 1) as f32).collect();
    for i in 1..=r {
        for (s, a) in sums.iter_mut().zip(at(i.min(h - 1))) {
            *s += a;
        }
    }
    for y in 0..h {
        let (add, sub) = (at((y + r + 1).min(h - 1)), at(y.saturating_sub(r)));
        let row = &mut v[y * w..(y + 1) * w];
        for x in 0..w {
            row[x] = sums[x] * inv;
            sums[x] += add[x] - sub[x];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    /// A red disc on a backdrop that goes from light grey to white, with
    /// a white dot inside the disc and one dark speck in the backdrop.
    fn picture() -> RgbaImage {
        RgbaImage::from_fn(80, 60, |x, y| {
            let (dx, dy) = (x as f32 - 40.0, y as f32 - 30.0);
            let r = (dx * dx + dy * dy).sqrt();
            if (38..40).contains(&x) && (28..30).contains(&y) {
                Rgba([255, 255, 255, 255])
            } else if r < 18.0 {
                Rgba([200, 30, 30, 255])
            } else if r < 19.0 {
                // The anti-aliased rim: half disc, half backdrop.
                Rgba([226, 140, 140, 255])
            } else if (5..7).contains(&x) && (5..7).contains(&y) {
                Rgba([40, 40, 40, 255])
            } else {
                let g = 225 + (x * 30 / 80) as u8;
                Rgba([g, g, g, 255])
            }
        })
    }

    #[test]
    fn the_backdrop_goes_and_the_subject_stays() {
        let img = picture();
        let out = remove_background(&img, &BgSettings::default(), 1.0);
        // Backdrop at both ends of the gradient: gone.
        assert_eq!(out.get_pixel(2, 50).0[3], 0);
        assert_eq!(out.get_pixel(77, 2).0[3], 0);
        // The subject: opaque and its colour unchanged.
        assert_eq!(out.get_pixel(40, 40).0, [200, 30, 30, 255]);
        // The white dot inside the subject is not reached by the fill.
        assert_eq!(out.get_pixel(38, 28).0[3], 255);
        // The loose dark speck in the backdrop is removed.
        assert!(out.get_pixel(5, 5).0[3] < 40, "{:?}", out.get_pixel(5, 5));
    }

    #[test]
    fn everywhere_mode_takes_background_coloured_parts_inside_too() {
        let img = picture();
        // No feather, which would blur the tiny hole partly back.
        let s = BgSettings {
            contiguous: false,
            softness: 0.0,
            ..BgSettings::default()
        };
        let out = remove_background(&img, &s, 1.0);
        assert_eq!(out.get_pixel(38, 28).0[3], 0);
        assert_eq!(out.get_pixel(40, 40).0[3], 255);
    }

    #[test]
    fn edge_pixels_lose_the_backdrops_colour() {
        let img = picture();
        let s = BgSettings {
            softness: 0.0,
            ..BgSettings::default()
        };
        let out = remove_background(&img, &s, 1.0);
        // A rim pixel: partly transparent, its colour pulled toward the
        // disc's red rather than the backdrop's pink.
        let rim = (0..80)
            .map(|x| *out.get_pixel(x, 30))
            .find(|p| p.0[3] > 10 && p.0[3] < 245);
        if let Some(p) = rim {
            assert!(p.0[0] > p.0[1] + 60, "{p:?}");
        }
        let plain = remove_background(
            &img,
            &BgSettings {
                clean_edges: false,
                ..s
            },
            1.0,
        );
        // Without cleaning, colours stay as they were.
        for (a, b) in img.pixels().zip(plain.pixels()) {
            assert_eq!(a.0[..3], b.0[..3]);
        }
    }

    #[test]
    fn learnt_colours_cover_a_two_tone_backdrop() {
        let img = RgbaImage::from_fn(40, 40, |x, y| {
            if (12..28).contains(&x) && (12..28).contains(&y) {
                Rgba([20, 160, 40, 255])
            } else if x < 20 {
                Rgba([250, 250, 250, 255])
            } else {
                Rgba([30, 60, 200, 255])
            }
        });
        assert_eq!(learn_background(&img).len(), 2);
        let out = remove_background(&img, &BgSettings::default(), 1.0);
        assert_eq!(out.get_pixel(1, 20).0[3], 0);
        assert_eq!(out.get_pixel(38, 20).0[3], 0);
        assert_eq!(out.get_pixel(20, 20).0[3], 255);
    }

    #[test]
    fn picked_colours_and_tolerance() {
        let img = picture();
        // A picked colour far from the backdrop: nothing goes.
        let s = BgSettings {
            colors: vec![BgPick::color([0, 0, 255])],
            ..BgSettings::default()
        };
        let out = remove_background(&img, &s, 1.0);
        assert!(out.pixels().all(|p| p.0[3] == 255));
        // Tolerance 0 on a gradient keeps most of it: a picked colour
        // takes what is within its tolerance.
        let grey = |tolerance: f32| BgSettings {
            tolerance,
            despeckle: false,
            softness: 0.0,
            colors: vec![BgPick::color([240, 240, 240])],
            ..BgSettings::default()
        };
        let count = |s: &BgSettings| {
            remove_background(&img, s, 1.0)
                .pixels()
                .filter(|p| p.0[3] == 0)
                .count()
        };
        let (gone, loose) = (count(&grey(0.0)), count(&grey(14.0)));
        assert!(gone < loose, "{gone} {loose}");
        // Automatic does not go by colour: at any tolerance the whole
        // backdrop goes and the subject stays.
        for tolerance in [0.0, 14.0, 45.0] {
            let s = BgSettings {
                tolerance,
                softness: 0.0,
                ..BgSettings::default()
            };
            let out = remove_background(&img, &s, 1.0);
            assert_eq!(out.get_pixel(2, 50).0[3], 0, "{tolerance}");
            assert_eq!(out.get_pixel(77, 2).0[3], 0, "{tolerance}");
            assert_eq!(out.get_pixel(40, 40).0, [200, 30, 30, 255], "{tolerance}");
        }
    }

    #[test]
    fn a_transparent_picture_is_left_alone() {
        let img = RgbaImage::from_fn(20, 20, |x, y| {
            if (5..15).contains(&x) && (5..15).contains(&y) {
                Rgba([10, 10, 10, 255])
            } else {
                Rgba([0, 0, 0, 0])
            }
        });
        assert_eq!(remove_background(&img, &BgSettings::default(), 1.0), img);
        let empty = RgbaImage::new(0, 0);
        assert_eq!(
            remove_background(&empty, &BgSettings::default(), 1.0),
            empty
        );
    }

    #[test]
    fn feathering_softens_only_the_edge() {
        let img = picture();
        let s = BgSettings {
            softness: 3.0,
            ..BgSettings::default()
        };
        let out = remove_background(&img, &s, 1.0);
        assert_eq!(out.get_pixel(40, 40).0[3], 255);
        assert_eq!(out.get_pixel(2, 50).0[3], 0);
        let partial = (0..80)
            .filter(|x| {
                let a = out.get_pixel(*x, 30).0[3];
                a > 0 && a < 255
            })
            .count();
        assert!(partial >= 4, "{partial}");
    }

    /// A backdrop that drifts away from the colours its border shows (a
    /// soft shadow under the subject, a texture) goes whole: the fill
    /// walks on through small steps of colour, while the subject's edge,
    /// a sharp step, stops it (#231).
    #[test]
    fn a_drifting_backdrop_goes_and_the_subject_stays() {
        // A white backdrop with a grey shadow fading in over 30 px under
        // a red disc, and a faint texture all over.
        let img = RgbaImage::from_fn(120, 90, |x, y| {
            let (dx, dy) = (x as f32 - 60.0, y as f32 - 40.0);
            if (dx * dx + dy * dy).sqrt() < 20.0 {
                return Rgba([200, 30, 30, 255]);
            }
            let (sx, sy) = (x as f32 - 60.0, y as f32 - 68.0);
            let shade = (1.0 - ((sx / 40.0).powi(2) + (sy / 10.0).powi(2)).sqrt()).max(0.0);
            let noise = ((x * 7 + y * 13) % 5) as i32 - 2;
            let v = (250.0 - 90.0 * shade) as i32 + noise;
            let v = v.clamp(0, 255) as u8;
            Rgba([v, v, v.saturating_sub(4), 255])
        });
        let out = remove_background(&img, &BgSettings::default(), 1.0);
        // The darkest of the shadow is 36 Delta E from white, past the
        // tolerance and its band, yet gone.
        assert_eq!(out.get_pixel(60, 68).0[3], 0, "{:?}", out.get_pixel(60, 68));
        assert_eq!(out.get_pixel(3, 3).0[3], 0);
        // The subject keeps its colour, to its edge.
        assert_eq!(out.get_pixel(60, 40).0, [200, 30, 30, 255]);
        assert_eq!(out.get_pixel(60, 24).0[3], 255);
        // (Counted at half opacity: the feather softens the rim.)
        let kept = out.pixels().filter(|p| p.0[3] >= 128).count();
        let disc = (std::f32::consts::PI * 20.0 * 20.0) as usize;
        assert!(
            kept >= disc * 9 / 10 && kept <= disc * 11 / 10,
            "{kept} vs {disc}"
        );
        // A picked colour does the same with Contiguous: the fill walks
        // on through the shadow.
        let white = BgSettings {
            colors: vec![BgPick::color([250, 250, 246])],
            ..BgSettings::default()
        };
        let out = remove_background(&img, &white, 1.0);
        assert_eq!(out.get_pixel(60, 68).0[3], 0);
        assert_eq!(out.get_pixel(60, 40).0, [200, 30, 30, 255]);
        // Everywhere mode (no fill) goes by the picked colour alone: the
        // shadow's dark middle stays.
        let s = BgSettings {
            contiguous: false,
            ..white
        };
        assert_eq!(remove_background(&img, &s, 1.0).get_pixel(60, 68).0[3], 255);
    }

    /// A photo of a person, drawn: a backdrop that shades from one blue
    /// to another with noise, a head with dark hair whose rim is hair
    /// and backdrop by turns, shoulders that leave the picture at the
    /// bottom and at both sides, a shirt, and an arm raised in a loop
    /// the backdrop shows through.
    fn portrait() -> RgbaImage {
        RgbaImage::from_fn(160, 200, |x, y| {
            let n = ((x * 31 + y * 17) % 7) as i32 - 3;
            let px = |c: [i32; 3]| {
                Rgba([
                    (c[0] + n).clamp(0, 255) as u8,
                    (c[1] + n).clamp(0, 255) as u8,
                    (c[2] + n).clamp(0, 255) as u8,
                    255,
                ])
            };
            let (fx, fy) = (x as f32, y as f32);
            let back = [80 + x as i32 / 6, 185 + y as i32 / 12, 232 - y as i32 / 10];
            let (skin, hair, shirt) = ([150, 90, 60], [25, 18, 15], [235, 190, 190]);
            let face = ((fx - 80.0) / 34.0).powi(2) + ((fy - 80.0) / 44.0).powi(2);
            let hairs = ((fx - 80.0) / 40.0).powi(2) + ((fy - 62.0) / 36.0).powi(2);
            let arm = ((fx - 38.0).powi(2) + (fy - 70.0).powi(2)).sqrt();
            let torso = fy >= 125.0 && (fx - 80.0).abs() <= 40.0 + (fy - 125.0) * 0.9;
            if face < 1.0 && fy > 58.0 {
                px(skin)
            } else if hairs < 1.0 {
                px(hair)
            } else if hairs < 1.25 && fy < 62.0 {
                // The rim: hair, and hair with the backdrop through it.
                if (x + y) % 2 == 0 {
                    px(hair)
                } else {
                    px([
                        (hair[0] + back[0]) / 2,
                        (hair[1] + back[1]) / 2,
                        (hair[2] + back[2]) / 2,
                    ])
                }
            } else if torso {
                if fy > 170.0 && (fx - 80.0).abs() < 30.0 {
                    px(shirt)
                } else {
                    px(skin)
                }
            } else if (12.0..22.0).contains(&arm) {
                px(skin)
            } else {
                px(back)
            }
        })
    }

    /// Automatic keeps the subject whole where it leaves the picture
    /// (its colours on the border are not backdrop), takes the whole
    /// backdrop, the part seen through a gap too, and leaves no tint of
    /// the backdrop in the hair (#3422).
    #[test]
    fn automatic_keeps_a_subject_that_leaves_the_picture() {
        let img = portrait();
        for softness in [0.0, 1.0] {
            let s = BgSettings {
                softness,
                ..BgSettings::default()
            };
            let out = remove_background(&img, &s, 1.0);
            // The backdrop, at every side.
            for (x, y) in [(5, 5), (155, 5), (5, 110), (155, 110), (80, 8)] {
                assert_eq!(out.get_pixel(x, y).0[3], 0, "backdrop at {x},{y}");
            }
            // Shoulders at the bottom corners, the shirt on the bottom
            // edge, the face, the hair, the arm: all there, unchanged.
            for (x, y) in [
                (3, 196),
                (156, 196),
                (80, 197),
                (80, 100),
                (80, 40),
                (38, 87),
                (21, 70),
            ] {
                assert_eq!(
                    out.get_pixel(x, y),
                    img.get_pixel(x, y),
                    "subject at {x},{y}"
                );
            }
            // The backdrop inside the arm's loop.
            assert_eq!(out.get_pixel(38, 70).0[3], 0, "the gap");
            // What is left of the hair's rim is hair-coloured, not blue.
            for y in 20..62 {
                for x in 30..130 {
                    let p = out.get_pixel(x, y).0;
                    assert!(
                        p[3] < 100 || p[2] as i32 <= p[0] as i32 + 40,
                        "tint at {x},{y}: {p:?}"
                    );
                }
            }
        }
        // Fill background fills what was removed only.
        let filled = BgSettings {
            backdrop: Some(BgBackdrop::WHITE),
            ..BgSettings::default()
        };
        let out = remove_background(&img, &filled, 1.0);
        assert_eq!(out.get_pixel(5, 5).0, [255, 255, 255, 255]);
        assert_eq!(out.get_pixel(38, 70).0, [255, 255, 255, 255]);
        assert_eq!(out.get_pixel(3, 196), img.get_pixel(3, 196));
        assert_eq!(out.get_pixel(80, 100), img.get_pixel(80, 100));
    }

    /// On a flat backdrop (a drawing), a part of the backdrop's colour
    /// inside the subject may be drawn that way: it stays with
    /// Contiguous and goes without.
    #[test]
    fn a_flat_part_inside_the_subject_stays_with_contiguous() {
        let img = RgbaImage::from_fn(100, 80, |x, y| {
            let (dx, dy) = (x as f32 - 50.0, y as f32 - 40.0);
            let r = (dx * dx + dy * dy).sqrt();
            if (8.0..24.0).contains(&r) {
                Rgba([200, 30, 30, 255])
            } else {
                Rgba([250, 250, 250, 255])
            }
        });
        let out = remove_background(&img, &sharp(vec![BgPick::AUTO]), 1.0);
        assert_eq!(out.get_pixel(2, 2).0[3], 0);
        assert_eq!(out.get_pixel(50, 40).0[3], 255, "inside stays");
        // Whole, to its rim, though it has the backdrop's colour there.
        assert_eq!(out.get_pixel(56, 40).0, [250, 250, 250, 255]);
        assert_eq!(out.get_pixel(50, 46).0, [250, 250, 250, 255]);
        assert_eq!(out.get_pixel(50, 24).0, [200, 30, 30, 255]);
        let everywhere = BgSettings {
            contiguous: false,
            ..sharp(vec![BgPick::AUTO])
        };
        let out = remove_background(&img, &everywhere, 1.0);
        assert_eq!(out.get_pixel(50, 40).0[3], 0, "inside goes");
        assert_eq!(out.get_pixel(50, 24).0, [200, 30, 30, 255]);
    }

    /// A white backdrop with a red disc; a green patch on the backdrop
    /// (touching it, not the border) and a blue hole inside the disc.
    fn three_colours() -> RgbaImage {
        RgbaImage::from_fn(100, 80, |x, y| {
            let (dx, dy) = (x as f32 - 60.0, y as f32 - 40.0);
            let r = (dx * dx + dy * dy).sqrt();
            if r < 5.0 {
                Rgba([30, 60, 220, 255])
            } else if r < 20.0 {
                Rgba([200, 30, 30, 255])
            } else if (10..30).contains(&x) && (20..60).contains(&y) {
                Rgba([40, 180, 60, 255])
            } else {
                Rgba([250, 250, 250, 255])
            }
        })
    }

    fn sharp(colors: Vec<BgPick>) -> BgSettings {
        BgSettings {
            softness: 0.0,
            colors,
            ..BgSettings::default()
        }
    }

    /// With Contiguous, the fill goes through pixels matching any listed
    /// colour: Automatic alone stops at the green patch, Automatic and
    /// green take both, and the subject stays.
    #[test]
    fn the_fill_goes_through_every_listed_colour() {
        let img = three_colours();
        let auto = remove_background(&img, &sharp(vec![BgPick::AUTO]), 1.0);
        assert_eq!(auto.get_pixel(2, 2).0[3], 0);
        assert_eq!(auto.get_pixel(20, 40).0[3], 255, "green stays");
        let both = sharp(vec![BgPick::AUTO, BgPick::color([40, 180, 60])]);
        let out = remove_background(&img, &both, 1.0);
        assert_eq!(out.get_pixel(2, 2).0[3], 0);
        assert_eq!(out.get_pixel(20, 40).0[3], 0, "green goes too");
        assert_eq!(out.get_pixel(50, 40).0, [200, 30, 30, 255]);
        // The blue hole is not reached from the border.
        assert_eq!(out.get_pixel(60, 40).0[3], 255);
        // Two picked colours, no Automatic: the same.
        let picked = sharp(vec![
            BgPick::color([250, 250, 250]),
            BgPick::color([40, 180, 60]),
        ]);
        let out2 = remove_background(&img, &picked, 1.0);
        assert_eq!(out2.get_pixel(20, 40).0[3], 0);
        assert_eq!(out2.get_pixel(2, 2).0[3], 0);
        assert_eq!(out2.get_pixel(50, 40).0[3], 255);
    }

    /// A third colour picked inside the subject goes from the point it
    /// was picked at (Contiguous), everywhere without Contiguous; Remove
    /// specks, Edge softness and Clean edge colors leave the subject.
    #[test]
    fn a_colour_picked_inside_goes_from_its_seed() {
        let img = three_colours();
        let mut s = sharp(vec![BgPick::AUTO, BgPick::color([40, 180, 60])]);
        s.colors.push(BgPick::color([30, 60, 220]));
        let unseeded = remove_background(&img, &s, 1.0);
        assert_eq!(unseeded.get_pixel(60, 40).0[3], 255, "not reached");
        s.colors[2].seed = Some([0.6, 0.5]);
        let out = remove_background(&img, &s, 1.0);
        assert_eq!(out.get_pixel(60, 40).0[3], 0, "reached from its seed");
        assert_eq!(out.get_pixel(20, 40).0[3], 0);
        assert_eq!(out.get_pixel(50, 40).0, [200, 30, 30, 255]);
        // Without Contiguous (and no seed), the blue goes everywhere.
        s.colors[2].seed = None;
        let everywhere = BgSettings {
            contiguous: false,
            ..s.clone()
        };
        let out = remove_background(&img, &everywhere, 1.0);
        assert_eq!(out.get_pixel(60, 40).0[3], 0);
        assert_eq!(out.get_pixel(50, 40).0[3], 255);
        // Feathered, the union's edge softens and the subject's middle
        // stays.
        let soft = BgSettings { softness: 2.0, ..s };
        let out = remove_background(&img, &soft, 1.0);
        assert_eq!(out.get_pixel(50, 40).0[3], 255);
        assert_eq!(out.get_pixel(20, 40).0[3], 0);
    }

    /// Each colour has its own tolerance; one left unset follows the
    /// dialog's.
    #[test]
    fn each_colour_has_its_own_tolerance() {
        // A grey backdrop and a darker grey patch 10 Delta E or so away.
        let img = RgbaImage::from_fn(60, 40, |x, y| {
            if (20..40).contains(&x) && (10..30).contains(&y) {
                Rgba([200, 30, 30, 255])
            } else if x < 10 {
                Rgba([150, 150, 150, 255])
            } else {
                Rgba([180, 180, 180, 255])
            }
        });
        let tight = BgPick {
            tolerance: Some(1.0),
            ..BgPick::color([170, 170, 170])
        };
        let s = BgSettings {
            tolerance: 30.0,
            softness: 0.0,
            despeckle: false,
            colors: vec![tight],
            ..BgSettings::default()
        };
        // Its own tolerance of 1: neither grey goes.
        let out = remove_background(&img, &s, 1.0);
        assert_eq!(out.get_pixel(5, 5).0[3], 255);
        assert_eq!(out.get_pixel(50, 5).0[3], 255);
        // Following the dialog's 30: both go, the subject stays.
        let loose = BgSettings {
            colors: vec![BgPick::color([170, 170, 170])],
            ..s.clone()
        };
        let out = remove_background(&img, &loose, 1.0);
        assert_eq!(out.get_pixel(5, 5).0[3], 0);
        assert_eq!(out.get_pixel(50, 5).0[3], 0);
        assert_eq!(out.get_pixel(30, 20).0[3], 255);
        // Two colours, each tight: each takes its own grey.
        let two = BgSettings {
            colors: vec![
                BgPick {
                    tolerance: Some(2.0),
                    ..BgPick::color([150, 150, 150])
                },
                BgPick {
                    tolerance: Some(2.0),
                    ..BgPick::color([180, 180, 180])
                },
            ],
            ..s
        };
        let out = remove_background(&img, &two, 1.0);
        assert_eq!(out.get_pixel(5, 5).0[3], 0);
        assert_eq!(out.get_pixel(50, 5).0[3], 0);
        assert_eq!(out.get_pixel(30, 20).0[3], 255);
    }

    /// The list: adding skips a colour listed already, removing the
    /// nearest takes the closest entry and never leaves the list empty.
    #[test]
    fn the_colour_list_adds_and_removes() {
        let mut s = BgSettings {
            colors: Vec::new(),
            ..BgSettings::default()
        };
        assert_eq!(s.entries(), vec![BgPick::AUTO]);
        assert!(s.add_color([0, 200, 0], Some([0.1, 0.2])));
        assert!(!s.add_color([0, 200, 0], None));
        assert!(s.add_color([0, 0, 200], None));
        assert_eq!(s.colors.len(), 3);
        assert_eq!(s.colors[0], BgPick::AUTO);
        assert_eq!(s.colors[1].seed, Some([0.1, 0.2]));
        let learnt = [BgColor::of([255, 255, 255])];
        // Near blue: the blue entry goes.
        assert_eq!(s.remove_nearest([10, 10, 190], &learnt), Some(2));
        // Near white: Automatic (its learnt white) goes; green is first.
        assert_eq!(s.remove_nearest([250, 250, 250], &learnt), Some(0));
        assert_eq!(
            s.colors,
            vec![BgPick {
                seed: Some([0.1, 0.2]),
                ..BgPick::color([0, 200, 0])
            }]
        );
        assert_eq!(s.remove_nearest([0, 0, 0], &learnt), Some(0));
        assert_eq!(s.colors, vec![BgPick::AUTO]);
        // Automatic with nothing learnt cannot be the nearest.
        assert_eq!(s.remove_nearest([0, 0, 0], &[]), None);
        s.remove(7);
        assert_eq!(s.colors, vec![BgPick::AUTO]);
        // A tie between Automatic's learnt white and a picked white: the
        // picked one goes.
        let mut t = BgSettings {
            colors: vec![BgPick::AUTO, BgPick::color([255, 255, 255])],
            ..BgSettings::default()
        };
        assert_eq!(t.remove_nearest([255, 255, 255], &learnt), Some(1));
    }
    /// The Erase brush takes away part of the subject and the Restore
    /// brush brings back a piece of the backdrop, each where it was
    /// painted only (#1723, #1915).
    #[test]
    fn the_brushes_erase_and_restore_where_painted() {
        let img = picture();
        let s = BgSettings {
            softness: 0.0,
            strokes: vec![
                // Across the disc's middle, left to right.
                BgStroke {
                    restore: false,
                    radius: 2.0 / 80.0,
                    points: vec![[30.0 / 80.0, 0.5], [50.0 / 80.0, 0.5]],
                },
                // A dab on the backdrop, top left.
                BgStroke {
                    restore: true,
                    radius: 3.0 / 80.0,
                    points: vec![[15.0 / 80.0, 15.0 / 60.0]],
                },
            ],
            ..BgSettings::default()
        };
        let out = remove_background(&img, &s, 1.0);
        // Erased along the stroke, the rest of the disc stays.
        assert_eq!(out.get_pixel(35, 30).0[3], 0);
        assert_eq!(out.get_pixel(45, 30).0[3], 0);
        assert_eq!(out.get_pixel(40, 40).0, [200, 30, 30, 255]);
        // Restored at full opacity and in its own colour; beside it the
        // backdrop is still gone.
        assert_eq!(out.get_pixel(15, 15).0, img.get_pixel(15, 15).0);
        assert_eq!(out.get_pixel(15, 25).0[3], 0);
        // A later stroke wins over an earlier one.
        let mut s2 = s.clone();
        s2.strokes.push(BgStroke {
            restore: true,
            radius: 2.0 / 80.0,
            points: vec![[35.0 / 80.0, 0.5]],
        });
        let out2 = remove_background(&img, &s2, 1.0);
        assert_eq!(out2.get_pixel(35, 30).0[3], 255);
        assert_eq!(out2.get_pixel(45, 30).0[3], 0);
    }

    /// Strokes are kept as fractions of the picture, so a smaller copy
    /// (the preview) is painted in the same places.
    #[test]
    fn strokes_follow_the_picture_at_any_size() {
        let big = picture();
        let small = image::imageops::resize(&big, 40, 30, image::imageops::FilterType::Nearest);
        let s = BgSettings {
            softness: 0.0,
            strokes: vec![BgStroke {
                restore: false,
                radius: 4.0 / 80.0,
                points: vec![[0.5, 0.5]],
            }],
            ..BgSettings::default()
        };
        assert_eq!(remove_background(&big, &s, 1.0).get_pixel(40, 30).0[3], 0);
        assert_eq!(remove_background(&small, &s, 0.5).get_pixel(20, 15).0[3], 0);
    }

    /// With a backdrop, the removed background shows its colour (a
    /// gradient from top to bottom) and every pixel is opaque; the
    /// subject keeps its colour (#1915).
    #[test]
    fn a_backdrop_fills_behind_the_subject() {
        let img = picture();
        let s = BgSettings {
            softness: 0.0,
            backdrop: Some(BgBackdrop {
                top: [0, 0, 255],
                bottom: [0, 255, 0],
            }),
            ..BgSettings::default()
        };
        let out = remove_background(&img, &s, 1.0);
        assert!(out.pixels().all(|p| p.0[3] == 255));
        assert_eq!(out.get_pixel(2, 0).0, [0, 0, 255, 255]);
        assert_eq!(out.get_pixel(2, 59).0, [0, 255, 0, 255]);
        assert_eq!(out.get_pixel(40, 40).0, [200, 30, 30, 255]);
    }

    /// A picture with nothing to take by colour still takes the brushes.
    #[test]
    fn the_brushes_work_on_a_transparent_picture() {
        let img = RgbaImage::from_fn(20, 20, |x, y| {
            if (5..15).contains(&x) && (5..15).contains(&y) {
                Rgba([10, 20, 30, 255])
            } else {
                Rgba([0, 0, 0, 0])
            }
        });
        let s = BgSettings {
            strokes: vec![BgStroke {
                restore: false,
                radius: 2.0 / 20.0,
                points: vec![[0.5, 0.5]],
            }],
            ..BgSettings::default()
        };
        let out = remove_background(&img, &s, 1.0);
        assert_eq!(out.get_pixel(10, 10).0[3], 0);
        assert_eq!(out.get_pixel(6, 6).0[3], 255);
    }

    /// A result shrunk for the preview: a removed pixel keeps its old
    /// colour with no opacity, and that colour does not tint the edge;
    /// an opaque area keeps its colour and a removed one stays clear.
    #[test]
    fn a_shrunk_result_keeps_removed_colours_out() {
        let img = RgbaImage::from_fn(40, 20, |x, _| {
            if x < 20 {
                Rgba([0, 255, 0, 0])
            } else {
                Rgba([200, 30, 30, 255])
            }
        });
        let small = shrink_result(&img, 20, 10);
        assert_eq!(small.dimensions(), (20, 10));
        assert_eq!(small.get_pixel(2, 5).0[3], 0);
        assert_eq!(small.get_pixel(17, 5).0, [200, 30, 30, 255]);
        for p in small.pixels().filter(|p| p.0[3] > 0) {
            assert!(p.0[1] <= 31, "green bled into the edge: {:?}", p.0);
        }
        assert_eq!(shrink_result(&img, 40, 20), img);
    }
}
