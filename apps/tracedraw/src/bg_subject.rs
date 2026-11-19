//! The subject of a picture, for Remove Background's Automatic entry:
//! which pixels are the thing photographed or drawn, and which are the
//! backdrop behind it, whatever their colours.
//!
//! 1. A copy at most [`WORK`] pixels across is worked on. The colours of
//!    its border are clustered; the cluster that covers most of the
//!    border (the top and the sides count more than the bottom, where a
//!    subject usually leaves the picture) is surely backdrop, and so is
//!    one covering at least half as much. Border pixels of other colours
//!    are left open: a shoulder or an arm that touches the border is not
//!    backdrop.
//! 2. From the sure border pixels a fill walks through small steps of
//!    colour: the first guess of the backdrop. The rest is the first
//!    guess of the subject.
//! 3. Each side gets a model of its colours and of where they lie (a
//!    mixture of Gaussians over Lab and position), and a minimum cut on
//!    the pixel grid gives the labelling that follows the models best
//!    while cutting along edges of the picture. Models and cut are made
//!    again a few times, each from the last labelling.
//! 4. Backdrop not connected to the border (seen through a gap of the
//!    subject) is told from the backdrop around it.
//! 5. At the picture's size the edge is refined: within a band around
//!    the cut, the opacity follows the picture's own colours (a guided
//!    filter), so hair and anti-aliased edges come out soft and thin
//!    parts the small copy lost come back.
//!
//! See docs/behavior/remove-background.md.

// The matrix arithmetic below reads as its formulas with indices.
#![allow(clippy::needless_range_loop)]

use image::RgbaImage;
use std::collections::VecDeque;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

type Lab = [f32; 3];

/// The longer side of the copy the subject is found on.
const WORK: usize = 256;
/// The longer side of the copy the edge is refined on; a larger picture
/// takes the refined edge's coefficients from it.
const MID: usize = 512;

/// The subject at the picture's size.
pub struct Subject {
    /// Opacity of the subject, 0 where backdrop, one value per pixel.
    pub keep: Vec<f32>,
    /// The backdrop's colour near each pixel (0 to 255), on a copy
    /// `bw` x `bh`.
    back: Vec<[f32; 3]>,
    /// The subject's colour near each pixel, on the same copy.
    fore: Vec<[f32; 3]>,
    /// How near the subject's edge each pixel of that copy is: 1 at the
    /// edge, 0 well inside the subject and away from it.
    edge: Vec<f32>,
    bw: usize,
    bh: usize,
    w: usize,
    h: usize,
}

impl Subject {
    /// The colour of the subject in pixel (x, y), whose colour is `c`
    /// and of which `m` (0 to 1) is subject, the rest backdrop: the
    /// backdrop's share is taken out, and near the edge so is the tint
    /// the backdrop left (more of the backdrop's hue than the subject
    /// has there). None where the colour stays as it is.
    pub fn cleaned(&self, x: usize, y: usize, c: [u8; 3], m: f32) -> Option<[u8; 3]> {
        if self.bw == 0 || self.bh == 0 || self.w == 0 || self.h == 0 {
            return None;
        }
        let bx = (x * self.bw / self.w).min(self.bw - 1);
        let by = (y * self.bh / self.h).min(self.bh - 1);
        let i = by * self.bw + bx;
        let edge = self.edge.get(i).copied().unwrap_or(0.0);
        if m >= 0.999 && edge <= 0.0 {
            return None;
        }
        let back = self.back.get(i).copied().unwrap_or([255.0; 3]);
        let fore = self.fore.get(i).copied().unwrap_or([0.0; 3]);
        let mut out = [c[0] as f32, c[1] as f32, c[2] as f32];
        if m < 0.999 {
            // The pixel was m * subject + (1 - m) * backdrop; where next
            // to nothing of the subject is in it, the subject's colour
            // nearby stands in.
            let k = smoothstep(0.05, 0.35, m);
            for j in 0..3 {
                let solved = ((out[j] - (1.0 - m) * back[j]) / m.max(0.05)).clamp(0.0, 255.0);
                out[j] = fore[j] * (1.0 - k) + solved * k;
            }
        }
        if edge > 0.0 {
            let grey = |c: &[f32; 3]| 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2];
            let hue = |c: &[f32; 3]| {
                let g = grey(c);
                [c[0] - g, c[1] - g, c[2] - g]
            };
            let tint = hue(&back);
            let strength = (tint[0] * tint[0] + tint[1] * tint[1] + tint[2] * tint[2]).sqrt();
            if strength > 25.0 {
                let dir = [tint[0] / strength, tint[1] / strength, tint[2] / strength];
                let along = |c: &[f32; 3]| {
                    let h = hue(c);
                    (h[0] * dir[0] + h[1] * dir[1] + h[2] * dir[2]).max(0.0)
                };
                let extra = (along(&out) - along(&fore)).max(0.0) * edge;
                for j in 0..3 {
                    out[j] -= extra * dir[j];
                }
            }
        }
        Some([
            out[0].round().clamp(0.0, 255.0) as u8,
            out[1].round().clamp(0.0, 255.0) as u8,
            out[2].round().clamp(0.0, 255.0) as u8,
        ])
    }
}

/// What a pixel of the small copy is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    /// Backdrop connected to the border.
    Backdrop,
    /// The subject.
    Subject,
    /// Backdrop seen through a gap of the subject.
    Gap,
    /// A part inside the subject that only has the backdrop's colours:
    /// it goes without Contiguous only.
    Inside,
}

/// The labelling of the small copy.
struct Cut {
    w: usize,
    h: usize,
    part: Vec<Part>,
    /// The backdrop is flat (a drawing, a scan), see [`parts`].
    flat: bool,
}

fn lab_of(c: [f32; 3]) -> Lab {
    let (l, a, b) = tracedraw_core::color::rgb_to_lab(c[0] / 255.0, c[1] / 255.0, c[2] / 255.0);
    [l, a, b]
}

fn dist2(a: &Lab, b: &Lab) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

/// A copy of the picture at most `side` across (box average), as colours
/// 0 to 255 and whether each pixel is opaque.
struct Small {
    w: usize,
    h: usize,
    rgb: Vec<[f32; 3]>,
    opaque: Vec<bool>,
}

fn small_copy(img: &RgbaImage, side: usize) -> Small {
    let (fw, fh) = (img.width() as usize, img.height() as usize);
    let longer = fw.max(fh).max(1);
    let (w, h) = if longer <= side {
        (fw, fh)
    } else {
        (
            (fw * side).div_ceil(longer).max(1),
            (fh * side).div_ceil(longer).max(1),
        )
    };
    let n = w * h;
    let mut sum = vec![[0.0f32; 4]; n];
    let mut count = vec![0.0f32; n];
    let raw = img.as_raw();
    for y in 0..fh {
        let ty = (y * h / fh.max(1)).min(h - 1);
        let row = &raw[y * fw * 4..(y + 1) * fw * 4];
        for (x, p) in row.chunks_exact(4).enumerate() {
            let tx = (x * w / fw.max(1)).min(w - 1);
            let a = p[3] as f32 / 255.0;
            let s = &mut sum[ty * w + tx];
            s[0] += p[0] as f32 * a;
            s[1] += p[1] as f32 * a;
            s[2] += p[2] as f32 * a;
            s[3] += a;
            count[ty * w + tx] += 1.0;
        }
    }
    let mut rgb = vec![[0.0f32; 3]; n];
    let mut opaque = vec![false; n];
    for i in 0..n {
        let s = sum[i];
        if s[3] > 0.0 {
            rgb[i] = [s[0] / s[3], s[1] / s[3], s[2] / s[3]];
        }
        opaque[i] = count[i] > 0.0 && s[3] / count[i] >= 0.5;
    }
    Small { w, h, rgb, opaque }
}

/// The colours of the border that are surely backdrop: the centres of
/// the sure clusters and, per pixel, whether it is a sure border pixel.
/// None when the border is mostly transparent already.
fn sure_border(lab: &[Lab], opaque: &[bool], w: usize, h: usize) -> Option<(Vec<Lab>, Vec<bool>)> {
    let depth = if w.min(h) >= 24 { 2 } else { 1 };
    // Border pixels with their weight: the top and the upper sides say
    // most about the backdrop, the bottom least.
    let mut ring: Vec<(usize, f32)> = Vec::new();
    let mut total = 0usize;
    for y in 0..h {
        for x in 0..w {
            let edge = x < depth || y < depth || x + depth >= w || y + depth >= h;
            if !edge {
                continue;
            }
            total += 1;
            let i = y * w + x;
            if !opaque[i] {
                continue;
            }
            let down = if h > 1 {
                y as f32 / (h - 1) as f32
            } else {
                0.0
            };
            let weight = if y < depth {
                1.0
            } else if y + depth >= h {
                0.35
            } else {
                1.0 - 0.5 * down
            };
            ring.push((i, weight));
        }
    }
    if ring.len() * 3 < total || ring.is_empty() {
        return None;
    }
    // Farthest-point seeds, then k-means.
    let mut centres: Vec<Lab> = vec![lab[ring[0].0]];
    while centres.len() < 8 {
        let mut far = (centres[0], -1.0f32);
        for (i, _) in &ring {
            let d = centres
                .iter()
                .map(|c| dist2(&lab[*i], c))
                .fold(f32::MAX, f32::min);
            if d > far.1 {
                far = (lab[*i], d);
            }
        }
        if far.1 < 10.0f32.powi(2) {
            break;
        }
        centres.push(far.0);
    }
    let nearest = |centres: &[Lab], l: &Lab| {
        centres
            .iter()
            .enumerate()
            .map(|(k, c)| (k, dist2(l, c)))
            .fold((0, f32::MAX), |a, b| if b.1 < a.1 { b } else { a })
    };
    for _ in 0..10 {
        let mut sum = vec![[0.0f64; 3]; centres.len()];
        let mut cnt = vec![0usize; centres.len()];
        for (i, _) in &ring {
            let k = nearest(&centres, &lab[*i]).0;
            cnt[k] += 1;
            for j in 0..3 {
                sum[k][j] += lab[*i][j] as f64;
            }
        }
        for (k, c) in centres.iter_mut().enumerate() {
            if cnt[k] > 0 {
                for j in 0..3 {
                    c[j] = (sum[k][j] / cnt[k] as f64) as f32;
                }
            }
        }
    }
    let mut cover = vec![0.0f32; centres.len()];
    for (i, wgt) in &ring {
        cover[nearest(&centres, &lab[*i]).0] += wgt;
    }
    let most = cover.iter().copied().fold(0.0f32, f32::max);
    let sure: Vec<bool> = cover.iter().map(|c| *c >= 0.5 * most && *c > 0.0).collect();
    let mut seeds = vec![false; w * h];
    for (i, _) in &ring {
        let (k, d) = nearest(&centres, &lab[*i]);
        if sure[k] && d <= 10.0f32.powi(2) {
            seeds[*i] = true;
        }
    }
    let kept: Vec<Lab> = centres
        .iter()
        .zip(&sure)
        .filter(|(_, s)| **s)
        .map(|(c, _)| *c)
        .collect();
    Some((kept, seeds))
}

/// The first guess of the backdrop: what a fill from the sure border
/// pixels reaches through small steps of colour, staying within reach of
/// the sure colours.
fn first_backdrop(lab: &[Lab], seeds: &[bool], sure: &[Lab], w: usize, h: usize) -> Vec<bool> {
    const STEP2: f32 = 3.5 * 3.5;
    const REACH2: f32 = 45.0 * 45.0;
    let mut back = seeds.to_vec();
    let mut queue: VecDeque<usize> = (0..w * h).filter(|i| seeds[*i]).collect();
    while let Some(i) = queue.pop_front() {
        let (x, y) = (i % w, i / w);
        let mut visit = |j: usize| {
            if back[j] || dist2(&lab[i], &lab[j]) > STEP2 {
                return;
            }
            if sure.iter().all(|c| dist2(c, &lab[j]) > REACH2) {
                return;
            }
            back[j] = true;
            queue.push_back(j);
        };
        if x > 0 {
            visit(i - 1);
        }
        if x + 1 < w {
            visit(i + 1);
        }
        if y > 0 {
            visit(i - w);
        }
        if y + 1 < h {
            visit(i + w);
        }
    }
    back
}

/// Colour and position of a pixel: Lab, then x and y scaled so the
/// picture's longer side is [`SPREAD`].
const DIM: usize = 5;
type Feat = [f32; DIM];
const SPREAD: f32 = 40.0;

/// One Gaussian of a mixture.
struct Gauss {
    mean: Feat,
    /// The inverse of the covariance.
    prec: [[f32; DIM]; DIM],
    /// ln(weight) - ln(normalisation).
    ln_scale: f32,
}

/// A mixture of Gaussians over colour and position.
struct Mixture {
    parts: Vec<Gauss>,
}

impl Mixture {
    /// ln of the density at `f`.
    fn ln_density(&self, f: &Feat) -> f32 {
        let mut best = f32::MIN;
        let mut terms = [0.0f32; 8];
        for (k, g) in self.parts.iter().enumerate() {
            let mut d = [0.0f32; DIM];
            for j in 0..DIM {
                d[j] = f[j] - g.mean[j];
            }
            let mut m = 0.0f32;
            for a in 0..DIM {
                let mut row = 0.0f32;
                for b in 0..DIM {
                    row += g.prec[a][b] * d[b];
                }
                m += row * d[a];
            }
            let t = g.ln_scale - 0.5 * m;
            if let Some(slot) = terms.get_mut(k) {
                *slot = t;
            }
            best = best.max(t);
        }
        if self.parts.is_empty() {
            return -1.0e9;
        }
        // ln of a sum of exponentials.
        let sum: f32 = terms
            .iter()
            .take(self.parts.len())
            .map(|t| (t - best).exp())
            .sum();
        best + sum.ln()
    }

    /// The mixture of `pts`: k-means from farthest-point seeds, one
    /// Gaussian per cluster that holds enough points.
    fn fit(pts: &[Feat]) -> Mixture {
        const K: usize = 6;
        if pts.is_empty() {
            return Mixture { parts: Vec::new() };
        }
        // Seeds on a sample, so one stray pixel is rarely a seed.
        let step = (pts.len() / 4000).max(1);
        let d2 = |a: &Feat, b: &Feat| {
            let mut s = 0.0f32;
            for j in 0..DIM {
                s += (a[j] - b[j]).powi(2);
            }
            s
        };
        let mut centres: Vec<Feat> = vec![pts[pts.len() / 2]];
        while centres.len() < K {
            let mut far = (centres[0], -1.0f32);
            for p in pts.iter().step_by(step) {
                let d = centres.iter().map(|c| d2(p, c)).fold(f32::MAX, f32::min);
                if d > far.1 {
                    far = (*p, d);
                }
            }
            if far.1 < 6.0f32.powi(2) {
                break;
            }
            centres.push(far.0);
        }
        let nearest = |centres: &[Feat], p: &Feat| {
            centres
                .iter()
                .enumerate()
                .map(|(k, c)| (k, d2(p, c)))
                .fold((0, f32::MAX), |a, b| if b.1 < a.1 { b } else { a })
                .0
        };
        // The centres settle on the sample; every point then joins the
        // nearest.
        for _ in 0..7 {
            let mut sum = vec![[0.0f64; DIM]; centres.len()];
            let mut cnt = vec![0usize; centres.len()];
            for p in pts.iter().step_by(step) {
                let k = nearest(&centres, p);
                cnt[k] += 1;
                for j in 0..DIM {
                    sum[k][j] += p[j] as f64;
                }
            }
            for (k, c) in centres.iter_mut().enumerate() {
                if cnt[k] > 0 {
                    for j in 0..DIM {
                        c[j] = (sum[k][j] / cnt[k] as f64) as f32;
                    }
                }
            }
        }
        let member: Vec<u8> = pts.iter().map(|p| nearest(&centres, p) as u8).collect();
        let mut parts = Vec::new();
        for (k, _) in centres.iter().enumerate() {
            let mine: Vec<&Feat> = pts
                .iter()
                .zip(&member)
                .filter(|(_, m)| **m as usize == k)
                .map(|(p, _)| p)
                .collect();
            if mine.is_empty() {
                continue;
            }
            let n = mine.len() as f64;
            let mut mean = [0.0f64; DIM];
            for p in &mine {
                for j in 0..DIM {
                    mean[j] += p[j] as f64;
                }
            }
            for m in &mut mean {
                *m /= n;
            }
            let mut cov = [[0.0f64; DIM]; DIM];
            for p in &mine {
                let mut d = [0.0f64; DIM];
                for j in 0..DIM {
                    d[j] = p[j] as f64 - mean[j];
                }
                for a in 0..DIM {
                    for b in 0..DIM {
                        cov[a][b] += d[a] * d[b];
                    }
                }
            }
            for (a, row) in cov.iter_mut().enumerate() {
                for v in row.iter_mut() {
                    *v /= n;
                }
                // A floor on the spread: a flat colour is still a few
                // Delta E wide, a spot a few pixels.
                row[a] += if a < 3 { 4.0 } else { 2.0 };
            }
            let Some((prec, ln_det)) = invert(&cov) else {
                continue;
            };
            let weight = n / pts.len() as f64;
            let ln_scale =
                weight.ln() - 0.5 * ln_det - 0.5 * DIM as f64 * (2.0 * std::f64::consts::PI).ln();
            let mut g = Gauss {
                mean: [0.0; DIM],
                prec: [[0.0; DIM]; DIM],
                ln_scale: ln_scale as f32,
            };
            for j in 0..DIM {
                g.mean[j] = mean[j] as f32;
                for b in 0..DIM {
                    g.prec[j][b] = prec[j][b] as f32;
                }
            }
            parts.push(g);
        }
        Mixture { parts }
    }
}

/// The inverse and ln of the determinant of a symmetric positive
/// definite matrix (Cholesky); None when it is not.
fn invert(m: &[[f64; DIM]; DIM]) -> Option<([[f64; DIM]; DIM], f64)> {
    let mut l = [[0.0f64; DIM]; DIM];
    for i in 0..DIM {
        for j in 0..=i {
            let mut s = m[i][j];
            for k in 0..j {
                s -= l[i][k] * l[j][k];
            }
            if i == j {
                if s <= 1.0e-9 {
                    return None;
                }
                l[i][j] = s.sqrt();
            } else {
                l[i][j] = s / l[j][j];
            }
        }
    }
    let ln_det: f64 = (0..DIM).map(|i| 2.0 * l[i][i].ln()).sum();
    // The inverse of L, then of the matrix.
    let mut li = [[0.0f64; DIM]; DIM];
    for i in 0..DIM {
        li[i][i] = 1.0 / l[i][i];
        for j in 0..i {
            let mut s = 0.0;
            for k in j..i {
                s -= l[i][k] * li[k][j];
            }
            li[i][j] = s / l[i][i];
        }
    }
    let mut inv = [[0.0f64; DIM]; DIM];
    for a in 0..DIM {
        for b in 0..DIM {
            let mut s = 0.0;
            for k in a.max(b)..DIM {
                s += li[k][a] * li[k][b];
            }
            inv[a][b] = s;
        }
    }
    Some((inv, ln_det))
}

/// The eight neighbours of a grid pixel: direction `d` and `d ^ 1` are
/// opposite.
const DIRS: [(isize, isize); 8] = [
    (1, 0),
    (-1, 0),
    (0, 1),
    (0, -1),
    (1, 1),
    (-1, -1),
    (-1, 1),
    (1, -1),
];

const FREE: u8 = 0;
const SOURCE: u8 = 1;
const SINK: u8 = 2;
/// A node's parent: a direction 0 to 7, the terminal, or none (orphan).
const TERMINAL: u8 = 8;
const NO_PARENT: u8 = 9;

/// A minimum cut on a pixel grid with eight neighbours (augmenting paths
/// found by two search trees that are kept between paths). The grid has
/// a margin of one node all round that nothing flows through, so a
/// neighbour always exists.
struct Flow {
    off: [isize; 8],
    /// Residual capacity of the arc from node `i` in direction `d`, at
    /// `i * 8 + d`.
    cap: Vec<i32>,
    /// Residual capacity from the source (positive) or to the sink
    /// (negative).
    tcap: Vec<i32>,
    tree: Vec<u8>,
    parent: Vec<u8>,
    stamp: Vec<u32>,
    dist: Vec<u32>,
    active: VecDeque<u32>,
    is_active: Vec<bool>,
    orphans: VecDeque<u32>,
    time: u32,
}

impl Flow {
    fn new(w: usize, h: usize) -> Flow {
        let pw = w + 2;
        let n = pw * (h + 2);
        let mut off = [0isize; 8];
        for (d, (dx, dy)) in DIRS.iter().enumerate() {
            off[d] = dy * pw as isize + dx;
        }
        Flow {
            off,
            cap: vec![0; n * 8],
            tcap: vec![0; n],
            tree: vec![FREE; n],
            parent: vec![NO_PARENT; n],
            stamp: vec![0; n],
            dist: vec![0; n],
            active: VecDeque::new(),
            is_active: vec![false; n],
            orphans: VecDeque::new(),
            time: 1,
        }
    }

    fn at(&self, i: usize, d: usize) -> usize {
        (i as isize + self.off[d]) as usize
    }

    fn activate(&mut self, i: usize) {
        if !self.is_active[i] {
            self.is_active[i] = true;
            self.active.push_back(i as u32);
        }
    }

    /// Grow the tree of `p` by one ring; a path from source to sink when
    /// the trees meet: (node of the source tree, node of the sink tree,
    /// direction from the first to the second).
    fn grow(&mut self, p: usize) -> Option<(usize, usize, usize)> {
        let mine = self.tree[p];
        for d in 0..8 {
            let q = self.at(p, d);
            let room = if mine == SOURCE {
                self.cap[p * 8 + d]
            } else {
                self.cap[q * 8 + (d ^ 1)]
            };
            if room <= 0 {
                continue;
            }
            let other = self.tree[q];
            if other == FREE {
                self.tree[q] = mine;
                self.parent[q] = (d ^ 1) as u8;
                self.stamp[q] = self.stamp[p];
                self.dist[q] = self.dist[p] + 1;
                self.activate(q);
            } else if other != mine {
                return Some(if mine == SOURCE {
                    (p, q, d)
                } else {
                    (q, p, d ^ 1)
                });
            }
        }
        None
    }

    fn orphan(&mut self, i: usize) {
        self.parent[i] = NO_PARENT;
        self.orphans.push_back(i as u32);
    }

    /// Push as much as fits along the path through the arc `s` to `t`.
    fn augment(&mut self, s: usize, t: usize, d: usize) {
        let mut f = self.cap[s * 8 + d];
        let mut u = s;
        loop {
            let pd = self.parent[u] as usize;
            if pd >= 8 {
                f = f.min(self.tcap[u]);
                break;
            }
            let v = self.at(u, pd);
            f = f.min(self.cap[v * 8 + (pd ^ 1)]);
            u = v;
        }
        u = t;
        loop {
            let pd = self.parent[u] as usize;
            if pd >= 8 {
                f = f.min(-self.tcap[u]);
                break;
            }
            let v = self.at(u, pd);
            f = f.min(self.cap[u * 8 + pd]);
            u = v;
        }
        if f <= 0 {
            // Cannot happen on a sound tree; drop the arc so the search
            // goes on.
            self.cap[s * 8 + d] = 0;
            return;
        }
        self.cap[s * 8 + d] -= f;
        self.cap[t * 8 + (d ^ 1)] += f;
        u = s;
        loop {
            let pd = self.parent[u] as usize;
            if pd >= 8 {
                self.tcap[u] -= f;
                if self.tcap[u] == 0 {
                    self.orphan(u);
                }
                break;
            }
            let v = self.at(u, pd);
            self.cap[v * 8 + (pd ^ 1)] -= f;
            self.cap[u * 8 + pd] += f;
            if self.cap[v * 8 + (pd ^ 1)] == 0 {
                self.orphan(u);
            }
            u = v;
        }
        u = t;
        loop {
            let pd = self.parent[u] as usize;
            if pd >= 8 {
                self.tcap[u] += f;
                if self.tcap[u] == 0 {
                    self.orphan(u);
                }
                break;
            }
            let v = self.at(u, pd);
            self.cap[u * 8 + pd] -= f;
            self.cap[v * 8 + (pd ^ 1)] += f;
            if self.cap[u * 8 + pd] == 0 {
                self.orphan(u);
            }
            u = v;
        }
    }

    /// How far `q` is from its terminal through its parents; None when
    /// the way ends at an orphan.
    fn origin(&mut self, q: usize) -> Option<u32> {
        let mut u = q;
        let mut steps = 0u32;
        let total = loop {
            if self.stamp[u] == self.time {
                break self.dist[u] + steps;
            }
            let pd = self.parent[u] as usize;
            if pd == NO_PARENT as usize {
                return None;
            }
            if pd == TERMINAL as usize {
                self.stamp[u] = self.time;
                self.dist[u] = 1;
                break 1 + steps;
            }
            u = self.at(u, pd);
            steps += 1;
        };
        // Remember the way for the next walks of this round.
        let mut u = q;
        let mut left = total;
        while self.stamp[u] != self.time {
            self.stamp[u] = self.time;
            self.dist[u] = left;
            left = left.saturating_sub(1);
            let pd = self.parent[u] as usize;
            if pd >= 8 {
                break;
            }
            u = self.at(u, pd);
        }
        Some(total)
    }

    /// Find a new parent for the orphan `p`, or free it and make its
    /// children orphans.
    fn adopt(&mut self, p: usize) {
        let mine = self.tree[p];
        if mine == FREE {
            return;
        }
        let mut best: Option<(usize, u32)> = None;
        for d in 0..8 {
            let q = self.at(p, d);
            if self.tree[q] != mine {
                continue;
            }
            let room = if mine == SOURCE {
                self.cap[q * 8 + (d ^ 1)]
            } else {
                self.cap[p * 8 + d]
            };
            if room <= 0 {
                continue;
            }
            if let Some(far) = self.origin(q) {
                if best.is_none_or(|b| far < b.1) {
                    best = Some((d, far));
                }
            }
        }
        if let Some((d, far)) = best {
            self.parent[p] = d as u8;
            self.stamp[p] = self.time;
            self.dist[p] = far + 1;
            return;
        }
        for d in 0..8 {
            let q = self.at(p, d);
            if self.tree[q] != mine {
                continue;
            }
            let room = if mine == SOURCE {
                self.cap[q * 8 + (d ^ 1)]
            } else {
                self.cap[p * 8 + d]
            };
            if room > 0 {
                self.activate(q);
            }
            let pd = self.parent[q] as usize;
            if pd < 8 && self.at(q, pd) == p {
                self.orphan(q);
            }
        }
        self.tree[p] = FREE;
    }

    /// Run to the maximum flow; true for the nodes left on the sink's
    /// side of the minimum cut.
    fn run(&mut self) -> Vec<bool> {
        for i in 0..self.tcap.len() {
            if self.tcap[i] != 0 {
                self.tree[i] = if self.tcap[i] > 0 { SOURCE } else { SINK };
                self.parent[i] = TERMINAL;
                self.dist[i] = 1;
                self.activate(i);
            }
        }
        while let Some(&p) = self.active.front() {
            let p = p as usize;
            if self.tree[p] == FREE {
                self.active.pop_front();
                self.is_active[p] = false;
                continue;
            }
            match self.grow(p) {
                None => {
                    self.active.pop_front();
                    self.is_active[p] = false;
                }
                Some((s, t, d)) => {
                    self.time += 1;
                    self.augment(s, t, d);
                    while let Some(o) = self.orphans.pop_front() {
                        self.adopt(o as usize);
                    }
                }
            }
        }
        self.tree.iter().map(|t| *t == SINK).collect()
    }
}

/// Find the subject on the small copy.
fn cut(small: &Small) -> Option<Cut> {
    let (w, h) = (small.w, small.h);
    let n = w * h;
    if n == 0 {
        return None;
    }
    let lab: Vec<Lab> = small.rgb.iter().map(|c| lab_of(*c)).collect();
    let (sure, seeds) = sure_border(&lab, &small.opaque, w, h)?;
    let mut first = first_backdrop(&lab, &seeds, &sure, w, h);
    // The backdrop seen through a gap of the subject is not reached by
    // the fill: what has the colour of the backdrop near it joins the
    // first guess too.
    let reached: Vec<f32> = (0..n)
        .map(|i| {
            if first[i] && small.opaque[i] {
                1.0
            } else {
                0.0
            }
        })
        .collect();
    let local = local_mean(small, &reached, (w.max(h) / 10).max(2));
    let mut like = vec![false; n];
    for i in 0..n {
        if first[i] || !small.opaque[i] {
            continue;
        }
        // A coloured backdrop in the subject's shadow keeps its hue: on
        // one, darker counts for less, and the colour may be further.
        let near = lab_of(local[i]);
        let chroma = ((near[1] * near[1] + near[2] * near[2]).sqrt() / 25.0).min(1.0);
        let mut dl = lab[i][0] - near[0];
        let mut limit = 6.0;
        if dl < 0.0 {
            dl *= 1.0 - 0.75 * chroma;
            limit += 4.0 * chroma;
        }
        let d2 = dl * dl + (lab[i][1] - near[1]).powi(2) + (lab[i][2] - near[2]).powi(2);
        like[i] = d2 <= limit * limit;
    }
    // Such a part next to what the fill reached is not a gap: the fill
    // stopped at an edge there, so it is something lying on the backdrop
    // (a white petal on a cloud).
    let mut seen = vec![false; n];
    let mut stack = Vec::new();
    let mut region = Vec::new();
    for start in 0..n {
        if !like[start] || seen[start] {
            continue;
        }
        region.clear();
        let mut touches = false;
        seen[start] = true;
        stack.push(start);
        while let Some(i) = stack.pop() {
            region.push(i);
            let (x, y) = (i % w, i / w);
            let mut go = |j: usize, stack: &mut Vec<usize>| {
                if first[j] {
                    touches = true;
                } else if like[j] && !seen[j] {
                    seen[j] = true;
                    stack.push(j);
                }
            };
            if x > 0 {
                go(i - 1, &mut stack);
            }
            if x + 1 < w {
                go(i + 1, &mut stack);
            }
            if y > 0 {
                go(i - w, &mut stack);
            }
            if y + 1 < h {
                go(i + w, &mut stack);
            }
        }
        if touches {
            for &i in &region {
                like[i] = false;
            }
        }
    }
    for i in 0..n {
        first[i] |= like[i];
    }
    let gap_like = like;
    let scale = SPREAD / w.max(h) as f32;
    let feat: Vec<Feat> = (0..n)
        .map(|i| {
            [
                lab[i][0],
                lab[i][1],
                lab[i][2],
                (i % w) as f32 * scale,
                (i / w) as f32 * scale,
            ]
        })
        .collect();
    // True: backdrop.
    let mut back: Vec<bool> = (0..n).map(|i| first[i] || !small.opaque[i]).collect();
    // The pull between neighbours: strong where the colours agree, next
    // to nothing across an edge.
    let mut sum = 0.0f64;
    let mut pairs = 0usize;
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            for (dx, dy) in [(1isize, 0isize), (0, 1), (1, 1), (-1, 1)] {
                let (qx, qy) = (x as isize + dx, y as isize + dy);
                if qx < 0 || qx >= w as isize || qy >= h as isize {
                    continue;
                }
                sum += dist2(&lab[i], &lab[qy as usize * w + qx as usize]) as f64;
                pairs += 1;
            }
        }
    }
    let beta = if sum > 0.0 {
        (pairs as f64 / (2.0 * sum)) as f32
    } else {
        0.0
    };
    const UNIT: f32 = 64.0;
    const PULL: f32 = 24.0;
    const HARD: i32 = 1 << 26;
    let pw = w + 2;
    let node = |x: usize, y: usize| (y + 1) * pw + x + 1;
    let mut pull = vec![0i32; n * 4];
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            for (k, (dx, dy)) in [(1isize, 0isize), (0, 1), (1, 1), (-1, 1)]
                .into_iter()
                .enumerate()
            {
                let (qx, qy) = (x as isize + dx, y as isize + dy);
                if qx < 0 || qx >= w as isize || qy >= h as isize {
                    continue;
                }
                let j = qy as usize * w + qx as usize;
                let far = if dx != 0 && dy != 0 {
                    std::f32::consts::SQRT_2
                } else {
                    1.0
                };
                let v = PULL * (-beta * dist2(&lab[i], &lab[j])).exp() / far;
                pull[i * 4 + k] = (v * UNIT).round() as i32;
            }
        }
    }
    // The directions of the four pairs above, as indices of DIRS.
    const PAIR_DIR: [usize; 4] = [0, 2, 4, 6];
    for _ in 0..5 {
        let fg_pts: Vec<Feat> = (0..n).filter(|i| !back[*i]).map(|i| feat[i]).collect();
        let bg_pts: Vec<Feat> = (0..n)
            .filter(|i| back[*i] && small.opaque[*i])
            .map(|i| feat[i])
            .collect();
        if fg_pts.len() < 4 || bg_pts.is_empty() {
            break;
        }
        let fg = Mixture::fit(&fg_pts);
        let bg = Mixture::fit(&bg_pts);
        if fg.parts.is_empty() || bg.parts.is_empty() {
            break;
        }
        let mut flow = Flow::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                let nd = node(x, y);
                flow.tcap[nd] = if seeds[i] || !small.opaque[i] {
                    -HARD
                } else {
                    let v = (fg.ln_density(&feat[i]) - bg.ln_density(&feat[i])).clamp(-20.0, 20.0);
                    // What looked like the backdrop through a gap stays
                    // backdrop unless its neighbours hold it.
                    let lean = if gap_like[i] { -20.0 } else { 0.0 };
                    ((v + lean) * UNIT).round() as i32
                };
                for (k, d) in PAIR_DIR.into_iter().enumerate() {
                    let c = pull[i * 4 + k];
                    if c > 0 {
                        let q = flow.at(nd, d);
                        flow.cap[nd * 8 + d] = c;
                        flow.cap[q * 8 + (d ^ 1)] = c;
                    }
                }
            }
        }
        let sink = flow.run();
        let mut changed = 0usize;
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                let now = sink[node(x, y)] || !small.opaque[i];
                if now != back[i] {
                    changed += 1;
                    back[i] = now;
                }
            }
        }
        if changed * 1000 < n {
            break;
        }
    }
    let (part, flat) = parts(&back, &lab, &sure, small, w, h);
    Some(Cut { w, h, part, flat })
}

/// Tell the backdrop connected to the border from the backdrop enclosed
/// by the subject, and among the enclosed parts those the backdrop shows
/// through from those that only share its colours.
fn parts(
    back: &[bool],
    lab: &[Lab],
    sure: &[Lab],
    small: &Small,
    w: usize,
    h: usize,
) -> (Vec<Part>, bool) {
    let n = w * h;
    let mut part = vec![Part::Subject; n];
    // Outside: backdrop reached from the border.
    let mut stack: Vec<usize> = Vec::new();
    let push = |i: usize, part: &mut Vec<Part>, stack: &mut Vec<usize>| {
        if back[i] && part[i] == Part::Subject {
            part[i] = Part::Backdrop;
            stack.push(i);
        }
    };
    for x in 0..w {
        push(x, &mut part, &mut stack);
        push((h - 1) * w + x, &mut part, &mut stack);
    }
    for y in 0..h {
        push(y * w, &mut part, &mut stack);
        push(y * w + w - 1, &mut part, &mut stack);
    }
    // Transparent pixels are outside wherever they are.
    for i in 0..n {
        if !small.opaque[i] {
            push(i, &mut part, &mut stack);
        }
    }
    while let Some(i) = stack.pop() {
        let (x, y) = (i % w, i / w);
        if x > 0 {
            push(i - 1, &mut part, &mut stack);
        }
        if x + 1 < w {
            push(i + 1, &mut part, &mut stack);
        }
        if y > 0 {
            push(i - w, &mut part, &mut stack);
        }
        if y + 1 < h {
            push(i + w, &mut part, &mut stack);
        }
    }
    // A flat backdrop (a drawing, a scan, a blown-out studio white) says
    // nothing about a part of the same colour inside the subject: it may
    // as well be drawn that way. A backdrop with its own shades (a photo)
    // seen again inside the subject is the backdrop showing through.
    let mut outside = 0usize;
    let mut off = 0usize;
    for i in 0..n {
        if part[i] == Part::Backdrop && small.opaque[i] {
            outside += 1;
            let d = sure
                .iter()
                .map(|c| dist2(c, &lab[i]))
                .fold(f32::MAX, f32::min);
            if d > 2.5f32.powi(2) {
                off += 1;
            }
        }
    }
    let flat = off * 33 <= outside;
    let limit = (n / 4000).max(6);
    let mut seen = vec![false; n];
    let mut region = Vec::new();
    for start in 0..n {
        if seen[start] || !back[start] || part[start] != Part::Subject {
            continue;
        }
        region.clear();
        stack.push(start);
        seen[start] = true;
        while let Some(i) = stack.pop() {
            region.push(i);
            let (x, y) = (i % w, i / w);
            let mut go = |j: usize, stack: &mut Vec<usize>| {
                if !seen[j] && back[j] && part[j] == Part::Subject {
                    seen[j] = true;
                    stack.push(j);
                }
            };
            if x > 0 {
                go(i - 1, &mut stack);
            }
            if x + 1 < w {
                go(i + 1, &mut stack);
            }
            if y > 0 {
                go(i - w, &mut stack);
            }
            if y + 1 < h {
                go(i + w, &mut stack);
            }
        }
        let what = if !flat && region.len() >= limit {
            Part::Gap
        } else {
            Part::Inside
        };
        for &i in &region {
            part[i] = what;
        }
    }
    (part, flat)
}

/// The last few cuts, by the small copy they were made of: the settings
/// that follow (tolerance, softness, brushes) do not make them again.
static CUTS: Mutex<Vec<(u64, Option<Arc<Cut>>)>> = Mutex::new(Vec::new());

fn cut_of(small: &Small) -> Option<Arc<Cut>> {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (small.w, small.h).hash(&mut hasher);
    for (c, o) in small.rgb.iter().zip(&small.opaque) {
        (c[0].to_bits(), c[1].to_bits(), c[2].to_bits(), *o).hash(&mut hasher);
    }
    let key = hasher.finish();
    if let Ok(cuts) = CUTS.lock() {
        if let Some((_, c)) = cuts.iter().find(|(k, _)| *k == key) {
            return c.clone();
        }
    }
    let made = cut(small).map(Arc::new);
    if let Ok(mut cuts) = CUTS.lock() {
        if cuts.len() >= 6 {
            cuts.remove(0);
        }
        cuts.push((key, made.clone()));
    }
    made
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// `v` (a plane `w` x `h`) at the point (x, y), between its pixels'
/// centres.
fn sample(v: &[f32], w: usize, h: usize, x: f32, y: f32) -> f32 {
    let x = (x - 0.5).clamp(0.0, (w - 1) as f32);
    let y = (y - 0.5).clamp(0.0, (h - 1) as f32);
    let (x0, y0) = (x.floor() as usize, y.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let top = v[y0 * w + x0] * (1.0 - fx) + v[y0 * w + x1] * fx;
    let bottom = v[y1 * w + x0] * (1.0 - fx) + v[y1 * w + x1] * fx;
    top * (1.0 - fy) + bottom * fy
}

/// The subjects of the last few pictures (previews, by their pixels
/// and the two settings a subject depends on): the brushes, the feather
/// and the backdrop then work on a subject found already.
type Found = (u64, Option<Arc<Subject>>);
static SUBJECTS: Mutex<Vec<Found>> = Mutex::new(Vec::new());
/// The largest picture whose subject is kept.
const KEPT_PIXELS: usize = 1 << 20;

/// The subject of `img`. `tolerance` (0 to 100) says how much of the
/// soft edge counts as backdrop; with `contiguous`, parts inside the
/// subject that only share the backdrop's colours stay. None when the
/// picture's border is transparent already.
pub fn subject(img: &RgbaImage, tolerance: f32, contiguous: bool) -> Option<Arc<Subject>> {
    let n = img.width() as usize * img.height() as usize;
    if n == 0 {
        return None;
    }
    if n > KEPT_PIXELS {
        return find(img, tolerance, contiguous).map(Arc::new);
    }
    // A quick hash of the pixels, eight bytes at a time.
    let mut key = 0x9E37_79B9_7F4A_7C15u64 ^ ((img.width() as u64) << 32 | img.height() as u64);
    let mut mix = |v: u64| {
        key = (key ^ v).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        key ^= key >> 29;
    };
    // (A last pixel alone, when their number is odd, is padded.)
    for c in img.as_raw().chunks(8) {
        let mut b = [0u8; 8];
        for (to, from) in b.iter_mut().zip(c) {
            *to = *from;
        }
        mix(u64::from_le_bytes(b));
    }
    mix(tolerance.to_bits() as u64);
    mix(contiguous as u64);
    if let Ok(found) = SUBJECTS.lock() {
        if let Some((_, s)) = found.iter().find(|(k, _)| *k == key) {
            return s.clone();
        }
    }
    let made = find(img, tolerance, contiguous).map(Arc::new);
    if let Ok(mut found) = SUBJECTS.lock() {
        if found.len() >= 4 {
            found.remove(0);
        }
        found.push((key, made.clone()));
    }
    made
}

fn find(img: &RgbaImage, tolerance: f32, contiguous: bool) -> Option<Subject> {
    let (w, h) = (img.width() as usize, img.height() as usize);
    if w == 0 || h == 0 {
        return None;
    }
    let cut = cut_of(&small_copy(img, WORK))?;
    // The copy the edge is refined on.
    let mid = small_copy(img, MID);
    let (mw, mh) = (mid.w, mid.h);
    let mn = mw * mh;
    let ratio = mw.max(mh) as f32 / cut.w.max(cut.h) as f32;
    // The cut, at this size: 1 where the subject is.
    let gone = |p: Part| match p {
        Part::Backdrop | Part::Gap => true,
        Part::Subject => false,
        Part::Inside => !contiguous,
    };
    let coarse: Vec<f32> = cut
        .part
        .iter()
        .map(|p| if gone(*p) { 0.0 } else { 1.0 })
        .collect();
    let mut p = vec![0.0f32; mn];
    for y in 0..mh {
        for x in 0..mw {
            let i = y * mw + x;
            p[i] = if !mid.opaque[i] {
                0.0
            } else if ratio <= 1.0 {
                coarse[(y.min(cut.h - 1)) * cut.w + x.min(cut.w - 1)]
            } else {
                sample(
                    &coarse,
                    cut.w,
                    cut.h,
                    (x as f32 + 0.5) * cut.w as f32 / mw as f32,
                    (y as f32 + 0.5) * cut.h as f32 / mh as f32,
                )
            };
        }
    }
    // On a flat backdrop, parts inside the subject that stay
    // (Contiguous) stay whole: they have the backdrop's colour, which
    // the edge below would take.
    let inside: Vec<bool> = (0..mn)
        .map(|i| {
            let (x, y) = (i % mw, i / mw);
            let (cx, cy) = (
                (x * cut.w / mw).min(cut.w - 1),
                (y * cut.h / mh).min(cut.h - 1),
            );
            contiguous && cut.flat && cut.part[cy * cut.w + cx] == Part::Inside
        })
        .collect();
    // The band around the cut where the edge is worked out; beyond it
    // the cut stands.
    let band = ((2.0 * ratio).round() as usize).max(2);
    let mut near = p.clone();
    crate::bg_remove::box_blur(&mut near, mw, mh, band);
    // Guided filter: within a window the opacity is a linear function of
    // the colour, fitted to the cut.
    let radius = band + band / 2;
    let eps = 2.0e-4f32;
    let chan = |k: usize| -> Vec<f32> { mid.rgb.iter().map(|c| c[k] / 255.0).collect() };
    let img_c = [chan(0), chan(1), chan(2)];
    let blur = |mut v: Vec<f32>| {
        crate::bg_remove::box_blur(&mut v, mw, mh, radius);
        v
    };
    let mean_i = [
        blur(img_c[0].clone()),
        blur(img_c[1].clone()),
        blur(img_c[2].clone()),
    ];
    let mean_p = blur(p.clone());
    let mut mean_ip = Vec::with_capacity(3);
    for c in &img_c {
        mean_ip.push(blur(c.iter().zip(&p).map(|(a, b)| a * b).collect()));
    }
    // The six products of the colour's channels: rr, rg, rb, gg, gb, bb.
    const PAIRS: [(usize, usize); 6] = [(0, 0), (0, 1), (0, 2), (1, 1), (1, 2), (2, 2)];
    let mut mean_ii = Vec::with_capacity(6);
    for (a, b) in PAIRS {
        mean_ii.push(blur(
            img_c[a].iter().zip(&img_c[b]).map(|(x, y)| x * y).collect(),
        ));
    }
    let mut coef = [vec![0.0f32; mn], vec![0.0f32; mn], vec![0.0f32; mn]];
    let mut shift = vec![0.0f32; mn];
    for i in 0..mn {
        let m = [mean_i[0][i], mean_i[1][i], mean_i[2][i]];
        let var = |k: usize, a: usize, b: usize| mean_ii[k][i] - m[a] * m[b];
        let (rr, rg, rb) = (var(0, 0, 0) + eps, var(1, 0, 1), var(2, 0, 2));
        let (gg, gb, bb) = (var(3, 1, 1) + eps, var(4, 1, 2), var(5, 2, 2) + eps);
        let cov = [
            mean_ip[0][i] - m[0] * mean_p[i],
            mean_ip[1][i] - m[1] * mean_p[i],
            mean_ip[2][i] - m[2] * mean_p[i],
        ];
        // The inverse of the symmetric 3 x 3 matrix, by cofactors.
        let (c00, c01, c02) = (gg * bb - gb * gb, rb * gb - rg * bb, rg * gb - rb * gg);
        let (c11, c12, c22) = (rr * bb - rb * rb, rg * rb - rr * gb, rr * gg - rg * rg);
        let det = rr * c00 + rg * c01 + rb * c02;
        let a = if det.abs() > 1.0e-18 {
            [
                (c00 * cov[0] + c01 * cov[1] + c02 * cov[2]) / det,
                (c01 * cov[0] + c11 * cov[1] + c12 * cov[2]) / det,
                (c02 * cov[0] + c12 * cov[1] + c22 * cov[2]) / det,
            ]
        } else {
            [0.0; 3]
        };
        shift[i] = mean_p[i] - a[0] * m[0] - a[1] * m[1] - a[2] * m[2];
        coef[0][i] = a[0];
        coef[1][i] = a[1];
        coef[2][i] = a[2];
    }
    let [c0, c1, c2] = coef;
    let coef = [blur(c0), blur(c1), blur(c2)];
    let shift = blur(shift);
    // The backdrop's colour near each pixel, and the subject's: the mean
    // of the backdrop pixels around it, and of the pixels well inside
    // the subject.
    let wide = band * 4;
    let mut zone = p.clone();
    crate::bg_remove::box_blur(&mut zone, mw, mh, wide);
    let outside: Vec<f32> = (0..mn)
        .map(|i| if mid.opaque[i] { 1.0 - p[i] } else { 0.0 })
        .collect();
    let back = local_mean(&mid, &outside, band * 3);
    let deep: Vec<f32> = zone
        .iter()
        .map(|z| if *z >= 0.999 { 1.0 } else { 0.0 })
        .collect();
    let fore = local_mean(&mid, &deep, wide);
    // How busy the picture is around each pixel (the spread of its
    // lightness): fine hair against the backdrop is, smooth skin is not.
    let light: Vec<f32> = mid
        .rgb
        .iter()
        .map(|c| 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2])
        .collect();
    let fine = (band * 2 / 3).max(2);
    let mut busy: Vec<f32> = light.iter().map(|v| v * v).collect();
    crate::bg_remove::box_blur(&mut busy, mw, mh, fine);
    let mut mean_light = light;
    crate::bg_remove::box_blur(&mut mean_light, mw, mh, fine);
    for (b, m) in busy.iter_mut().zip(&mean_light) {
        *b = (*b - m * m).max(0.0).sqrt();
    }
    // How much of the soft edge goes: opacities under `low` are backdrop,
    // over `high` subject.
    let low = (tolerance.clamp(0.0, 100.0) / 100.0).min(0.45);
    let high = 1.0 - low * 0.5;
    let level = |a: f32| ((a - low) / (high - low)).clamp(0.0, 1.0);
    let n = w * h;
    let mut keep = vec![0.0f32; n];
    let raw = img.as_raw();
    let same = mw == w && mh == h;
    let (sx, sy) = (mw as f32 / w as f32, mh as f32 / h as f32);
    for y in 0..h {
        let my = (y * mh / h).min(mh - 1);
        for x in 0..w {
            let i = y * w + x;
            let px = &raw[i * 4..i * 4 + 4];
            if px[3] == 0 {
                continue;
            }
            let at = |v: &[f32]| {
                if same {
                    v[i]
                } else {
                    sample(v, mw, mh, (x as f32 + 0.5) * sx, (y as f32 + 0.5) * sy)
                }
            };
            let around = at(&zone);
            if around >= 0.999 || inside[my * mw + (x * mw / w).min(mw - 1)] {
                keep[i] = 1.0;
                continue;
            }
            if around <= 0.001 {
                continue;
            }
            let side = at(&near);
            let mut a = if side >= 0.999 {
                1.0
            } else if side <= 0.001 {
                0.0
            } else {
                (at(&coef[0]) * px[0] as f32 / 255.0
                    + at(&coef[1]) * px[1] as f32 / 255.0
                    + at(&coef[2]) * px[2] as f32 / 255.0
                    + at(&shift))
                .clamp(0.0, 1.0)
            };
            if a > 0.0 {
                // A pixel that is a blend of the subject's colour and the
                // backdrop's (a wisp of hair with the backdrop between)
                // is as opaque as its place between the two.
                let mi = my * mw + (x * mw / w).min(mw - 1);
                let (f, b) = (fore[mi], back[mi]);
                let v = [f[0] - b[0], f[1] - b[1], f[2] - b[2]];
                let c = [
                    px[0] as f32 - b[0],
                    px[1] as f32 - b[1],
                    px[2] as f32 - b[2],
                ];
                let len2 = v[0] * v[0] + v[1] * v[1] + v[2] * v[2];
                if len2 > 30.0 * 30.0 {
                    let t = ((c[0] * v[0] + c[1] * v[1] + c[2] * v[2]) / len2).clamp(0.0, 1.0);
                    let off = [c[0] - t * v[0], c[1] - t * v[1], c[2] - t * v[2]];
                    let miss = (off[0] * off[0] + off[1] * off[1] + off[2] * off[2]).sqrt();
                    let len = len2.sqrt();
                    // Right at the cut, what lies on the backdrop's side
                    // of the two colours is backdrop even off the line
                    // between them (the light fringe a sharpened photo
                    // has along its edges).
                    let lean = if side > 0.001 && side < 0.999 {
                        1.0 - smoothstep(0.0, 0.3, t)
                    } else {
                        0.0
                    };
                    let slack = len * 0.08 + 4.0 + lean * (len * 0.27).min(30.0);
                    // Deeper inside the subject a colour between the
                    // two is its own (a highlight the sky lights) unless
                    // the picture is busy there (hair).
                    let shallow = 1.0 - smoothstep(0.55, 0.8, around);
                    let hairy = smoothstep(14.0, 30.0, busy[mi]);
                    let trust = (1.0 - smoothstep(slack, 2.0 * slack, miss)) * shallow.max(hairy);
                    a = a.min(a + (t - a) * trust);
                }
            }
            keep[i] = level(a);
        }
    }
    let edge: Vec<f32> = zone
        .iter()
        .zip(&inside)
        .map(|(z, kept)| {
            if *kept {
                0.0
            } else {
                smoothstep(0.0, 0.05, *z) * (1.0 - smoothstep(0.8, 0.999, *z))
            }
        })
        .collect();
    Some(Subject {
        keep,
        back,
        fore,
        edge,
        bw: mw,
        bh: mh,
        w,
        h,
    })
}

/// The mean colour of the pixels of weight 1 within `radius` of each
/// pixel; farther from any of them, a wider mean, and at last the mean
/// of them all.
fn local_mean(mid: &Small, weight: &[f32], radius: usize) -> Vec<[f32; 3]> {
    let (w, h) = (mid.w, mid.h);
    let n = w * h;
    let mut all = [0.0f64; 4];
    for i in 0..n {
        for k in 0..3 {
            all[k] += (mid.rgb[i][k] * weight[i]) as f64;
        }
        all[3] += weight[i] as f64;
    }
    let whole = if all[3] > 0.0 {
        [
            (all[0] / all[3]) as f32,
            (all[1] / all[3]) as f32,
            (all[2] / all[3]) as f32,
        ]
    } else {
        [255.0; 3]
    };
    let mut out = vec![whole; n];
    let mut done = vec![false; n];
    for r in [radius.max(1), radius.max(1) * 4] {
        let mut den = weight.to_vec();
        crate::bg_remove::box_blur(&mut den, w, h, r);
        let mut num = Vec::with_capacity(3);
        for k in 0..3 {
            let mut v: Vec<f32> = (0..n).map(|i| mid.rgb[i][k] * weight[i]).collect();
            crate::bg_remove::box_blur(&mut v, w, h, r);
            num.push(v);
        }
        for i in 0..n {
            if !done[i] && den[i] > 0.02 {
                out[i] = [num[0][i] / den[i], num[1][i] / den[i], num[2][i] / den[i]];
                done[i] = true;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The cut found is as cheap as the cheapest of all labellings of a
    /// small grid, tried one by one.
    #[test]
    fn the_minimum_cut_is_the_cheapest_labelling() {
        let (w, h) = (3usize, 3usize);
        let mut seed = 12345u32;
        let mut next = |most: i32| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            ((seed >> 16) as i32).rem_euclid(most)
        };
        for _ in 0..40 {
            let mut flow = Flow::new(w, h);
            let node = |x: usize, y: usize| (y + 1) * (w + 2) + x + 1;
            let mut arcs: Vec<(usize, usize, i32)> = Vec::new();
            for y in 0..h {
                for x in 0..w {
                    let a = node(x, y);
                    flow.tcap[a] = next(41) - 20;
                    for (d, (dx, dy)) in DIRS.iter().enumerate() {
                        let (qx, qy) = (x as isize + dx, y as isize + dy);
                        if qx < 0 || qy < 0 || qx >= w as isize || qy >= h as isize {
                            continue;
                        }
                        let c = next(12);
                        flow.cap[a * 8 + d] = c;
                        arcs.push((a, node(qx as usize, qy as usize), c));
                    }
                }
            }
            let terminal = flow.tcap.clone();
            let cost = |sink: &dyn Fn(usize) -> bool| {
                let mut total = 0i32;
                for y in 0..h {
                    for x in 0..w {
                        let a = node(x, y);
                        if sink(a) {
                            total += terminal[a].max(0);
                        } else {
                            total += (-terminal[a]).max(0);
                        }
                    }
                }
                for (a, b, c) in &arcs {
                    if !sink(*a) && sink(*b) {
                        total += c;
                    }
                }
                total
            };
            let mut least = i32::MAX;
            for bits in 0u32..1 << (w * h) {
                let sink = |a: usize| {
                    let (x, y) = (a % (w + 2) - 1, a / (w + 2) - 1);
                    bits >> (y * w + x) & 1 == 1
                };
                least = least.min(cost(&sink));
            }
            let found = flow.run();
            assert_eq!(cost(&|a| found[a]), least);
        }
    }

    #[test]
    fn a_covariance_is_inverted() {
        let mut m = [[0.0f64; DIM]; DIM];
        for (a, row) in m.iter_mut().enumerate() {
            for (b, v) in row.iter_mut().enumerate() {
                *v = if a == b {
                    4.0 + a as f64
                } else {
                    0.3 / (1.0 + (a as f64 - b as f64).abs())
                };
            }
        }
        let (inv, ln_det) = invert(&m).unwrap();
        for a in 0..DIM {
            for b in 0..DIM {
                let v: f64 = (0..DIM).map(|k| m[a][k] * inv[k][b]).sum();
                let want = if a == b { 1.0 } else { 0.0 };
                assert!((v - want).abs() < 1.0e-9, "{a},{b}: {v}");
            }
        }
        assert!(ln_det > 0.0);
        // Not positive definite: no inverse, no panic.
        m[2][2] = -1.0;
        assert!(invert(&m).is_none());
    }

    /// A subject is found once per picture and pair of settings; odd
    /// pictures (one pixel, one colour, transparent) give no panic.
    #[test]
    fn odd_pictures_are_safe() {
        use image::Rgba;
        for (w, h) in [(1u32, 1u32), (1, 40), (40, 1), (2, 2), (3, 300)] {
            let img = RgbaImage::from_pixel(w, h, Rgba([10, 200, 30, 255]));
            if let Some(s) = subject(&img, 14.0, true) {
                assert!(s.keep.iter().all(|k| *k == 0.0), "{w}x{h}");
            }
        }
        let clear = RgbaImage::from_pixel(30, 30, Rgba([0, 0, 0, 0]));
        assert!(subject(&clear, 14.0, true).is_none());
        let img = RgbaImage::from_fn(64, 64, |x, y| {
            if (20..44).contains(&x) && (20..44).contains(&y) {
                Rgba([200, 30, 30, 255])
            } else {
                Rgba([240, 240, 240, 255])
            }
        });
        let a = subject(&img, 14.0, true).unwrap();
        let b = subject(&img, 14.0, true).unwrap();
        assert!(Arc::ptr_eq(&a, &b));
        assert_eq!(a.keep[32 * 64 + 32], 1.0);
        assert_eq!(a.keep[0], 0.0);
        assert!(!Arc::ptr_eq(&a, &subject(&img, 30.0, true).unwrap()));
    }
}
