//! Liquify brushes and Smart Resize inside a picture being edited
//! (Bitmaps > Edit Inside). Liquify moves pixels with the brush (push,
//! twirl either way, pinch, bloat); Smart Resize takes away or adds the
//! least noticed seams of pixels so the main subject keeps its shape. See
//! `docs/behavior/edit-inside-picture.md`.

use crate::fx::util::bilinear;
use crate::pixel_edit::{dab_weight, union, BrushOpts, Dirty, PixelImage};
use image::{GrayImage, RgbaImage};

/// What the Liquify brush does to the pixels under it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiquifyMode {
    /// Along with the brush.
    Push,
    /// Round the centre, clockwise.
    TwirlRight,
    /// Round the centre, counter-clockwise.
    TwirlLeft,
    /// Towards the centre.
    Pinch,
    /// Away from the centre.
    Bloat,
}

impl LiquifyMode {
    pub const ALL: [LiquifyMode; 5] = [
        LiquifyMode::Push,
        LiquifyMode::TwirlRight,
        LiquifyMode::TwirlLeft,
        LiquifyMode::Pinch,
        LiquifyMode::Bloat,
    ];

    pub fn key(self) -> &'static str {
        match self {
            LiquifyMode::Push => "pixel.liquify.push",
            LiquifyMode::TwirlRight => "pixel.liquify.twirl_right",
            LiquifyMode::TwirlLeft => "pixel.liquify.twirl_left",
            LiquifyMode::Pinch => "pixel.liquify.pinch",
            LiquifyMode::Bloat => "pixel.liquify.bloat",
        }
    }
}

/// One Liquify stroke: where each pixel takes its colour from (an offset
/// into the layer as it was at the press), changed by every dab.
pub struct Liquify {
    pub mode: LiquifyMode,
    opts: BrushOpts,
    base: RgbaImage,
    off: Vec<(f32, f32)>,
    last: Option<(f32, f32)>,
}

impl Liquify {
    pub fn new(mode: LiquifyMode, opts: BrushOpts, pi: &PixelImage) -> Self {
        let n = (pi.width() as usize) * (pi.height() as usize);
        Liquify {
            mode,
            opts,
            base: pi.img.clone(),
            off: vec![(0.0, 0.0); n],
            last: None,
        }
    }

    /// The brush taken to (x, y): dabs a tenth of the brush apart.
    pub fn to(&mut self, pi: &mut PixelImage, x: f32, y: f32) -> Option<Dirty> {
        let step = (self.opts.size * 0.1).max(1.0);
        let Some((lx, ly)) = self.last else {
            self.last = Some((x, y));
            // Push needs a direction; the others work where pressed.
            return if self.mode == LiquifyMode::Push {
                None
            } else {
                self.dab(pi, x, y, (0.0, 0.0))
            };
        };
        let d = (x - lx).hypot(y - ly);
        if d < step && self.mode == LiquifyMode::Push {
            return None;
        }
        let n = ((d / step).floor() as usize).max(1);
        let mut changed: Option<Dirty> = None;
        let mv = ((x - lx) / n as f32, (y - ly) / n as f32);
        for i in 1..=n {
            let t = i as f32 / n as f32;
            if let Some(b) = self.dab(pi, lx + (x - lx) * t, ly + (y - ly) * t, mv) {
                changed = Some(union(changed, b));
            }
        }
        self.last = Some((x, y));
        changed
    }

    fn dab(&mut self, pi: &mut PixelImage, cx: f32, cy: f32, mv: (f32, f32)) -> Option<Dirty> {
        let (w, h) = pi.img.dimensions();
        let r = (self.opts.size / 2.0).max(1.0);
        let x0 = (cx - r).floor().max(0.0) as u32;
        let y0 = (cy - r).floor().max(0.0) as u32;
        let x1 = ((cx + r).ceil().max(0.0) as u32).min(w);
        let y1 = ((cy + r).ceil().max(0.0) as u32).min(h);
        if x0 >= x1 || y0 >= y1 {
            return None;
        }
        let k = self.opts.flow.clamp(0.01, 1.0) * self.opts.opacity.clamp(0.0, 1.0);
        for y in y0..y1 {
            for x in x0..x1 {
                let (vx, vy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                let wgt = dab_weight(vx.hypot(vy), self.opts.size, self.opts.hardness)
                    * pi.sel_at(x, y)
                    * k;
                if wgt <= 0.0 {
                    continue;
                }
                let i = (y * w + x) as usize;
                let Some(o) = self.off.get_mut(i) else {
                    continue;
                };
                // The pixel takes its colour from further back along the
                // move (push), from a turned, nearer or farther place.
                let (dx, dy) = match self.mode {
                    LiquifyMode::Push => (-mv.0 * wgt, -mv.1 * wgt),
                    LiquifyMode::TwirlRight | LiquifyMode::TwirlLeft => {
                        let a = if self.mode == LiquifyMode::TwirlRight {
                            -0.08
                        } else {
                            0.08
                        } * wgt;
                        let (s, c) = a.sin_cos();
                        (vx * c - vy * s - vx, vx * s + vy * c - vy)
                    }
                    LiquifyMode::Pinch => (vx * 0.06 * wgt, vy * 0.06 * wgt),
                    LiquifyMode::Bloat => (-vx * 0.06 * wgt, -vy * 0.06 * wgt),
                };
                o.0 += dx;
                o.1 += dy;
                let q = bilinear(&self.base, x as f32 + o.0, y as f32 + o.1, true);
                pi.img.put_pixel(x, y, q);
            }
        }
        Some([x0, y0, x1, y1])
    }
}

// ---------------------------------------------------------------------
// Smart Resize
// ---------------------------------------------------------------------

/// How much each pixel stands out from its neighbours (the size of the
/// colour change across it), raised where `protect` is set.
fn energy(img: &RgbaImage, protect: Option<&GrayImage>) -> Vec<f32> {
    let (w, h) = img.dimensions();
    let (wi, hi) = (w as i64, h as i64);
    let lum = |x: i64, y: i64| {
        let q = img.get_pixel(x.clamp(0, wi - 1) as u32, y.clamp(0, hi - 1) as u32);
        let a = q[3] as f32 / 255.0;
        [
            q[0] as f32 * a,
            q[1] as f32 * a,
            q[2] as f32 * a,
            q[3] as f32,
        ]
    };
    let mut e = vec![0.0; (w * h) as usize];
    for y in 0..hi {
        for x in 0..wi {
            let here = lum(x, y);
            let mut v = 0.0;
            // The change to each of the four neighbours, so a line one
            // pixel wide stands out too.
            for n in [lum(x - 1, y), lum(x + 1, y), lum(x, y - 1), lum(x, y + 1)] {
                for c in 0..4 {
                    v += (here[c] - n[c]).abs();
                }
            }
            if let Some(m) = protect {
                v += m.get_pixel(x as u32, y as u32)[0] as f32 * 100.0;
            }
            e[(y * wi + x) as usize] = v;
        }
    }
    e
}

/// The top-to-bottom seam (one x for each row, neighbours at most one
/// apart) crossing the least energy.
fn vertical_seam(e: &[f32], w: usize, h: usize) -> Vec<usize> {
    let mut cost = e.to_vec();
    for y in 1..h {
        for x in 0..w {
            let up = |dx: isize| {
                let xx = x as isize + dx;
                if xx < 0 || xx >= w as isize {
                    f32::MAX
                } else {
                    cost[(y - 1) * w + xx as usize]
                }
            };
            let best = up(-1).min(up(0)).min(up(1));
            cost[y * w + x] += best;
        }
    }
    let mut seam = vec![0; h];
    let last = &cost[(h - 1) * w..h * w];
    seam[h - 1] = last
        .iter()
        .enumerate()
        .min_by(|a, b| a.1.total_cmp(b.1))
        .map(|(i, _)| i)
        .unwrap_or(0);
    for y in (0..h - 1).rev() {
        let x = seam[y + 1];
        let mut bx = x;
        for dx in [-1isize, 1] {
            let xx = x as isize + dx;
            if xx >= 0 && (xx as usize) < w && cost[y * w + xx as usize] < cost[y * w + bx] {
                bx = xx as usize;
            }
        }
        seam[y] = bx;
    }
    seam
}

fn transpose(img: &RgbaImage) -> RgbaImage {
    let (w, h) = img.dimensions();
    RgbaImage::from_fn(h, w, |x, y| *img.get_pixel(y, x))
}

fn transpose_gray(m: &GrayImage) -> GrayImage {
    let (w, h) = m.dimensions();
    GrayImage::from_fn(h, w, |x, y| *m.get_pixel(y, x))
}

/// The images (all the same size) made `new_w` wide by taking away (or
/// doubling) the seams of least energy in `guide`, the first of them, the
/// `protect` mask raising the energy where it is set.
fn carve_width(
    imgs: &mut [RgbaImage],
    masks: &mut [GrayImage],
    protect: Option<&GrayImage>,
    new_w: u32,
) {
    let Some(first) = imgs.first() else {
        return;
    };
    let (w, h) = first.dimensions();
    if new_w == w || new_w == 0 || h == 0 {
        return;
    }
    let mut prot = protect.cloned();
    if new_w < w {
        for _ in 0..(w - new_w) {
            let Some(guide) = imgs.first() else {
                return;
            };
            let cw = guide.width() as usize;
            let e = energy(guide, prot.as_ref());
            let seam = vertical_seam(&e, cw, h as usize);
            let drop_rgba = |img: &RgbaImage| {
                RgbaImage::from_fn(cw as u32 - 1, h, |x, y| {
                    let sx = if (x as usize) < seam[y as usize] {
                        x
                    } else {
                        x + 1
                    };
                    *img.get_pixel(sx, y)
                })
            };
            let drop_gray = |m: &GrayImage| {
                GrayImage::from_fn(cw as u32 - 1, h, |x, y| {
                    let sx = if (x as usize) < seam[y as usize] {
                        x
                    } else {
                        x + 1
                    };
                    *m.get_pixel(sx, y)
                })
            };
            for img in imgs.iter_mut() {
                *img = drop_rgba(img);
            }
            for m in masks.iter_mut() {
                *m = drop_gray(m);
            }
            if let Some(p) = prot.as_mut() {
                *p = drop_gray(p);
            }
        }
    } else {
        // Wider: the cheapest seams found on a shrinking copy, each
        // doubled (with the average of it and its right neighbour).
        let add = (new_w - w) as usize;
        let cw = w as usize;
        let mut index: Vec<Vec<usize>> = (0..h).map(|_| (0..cw).collect()).collect();
        let mut work = imgs[0].clone();
        let mut wprot = prot.clone();
        let mut chosen: Vec<Vec<usize>> = Vec::new();
        for _ in 0..add.min(cw.saturating_sub(1)) {
            let ww = work.width() as usize;
            let e = energy(&work, wprot.as_ref());
            let seam = vertical_seam(&e, ww, h as usize);
            chosen.push(seam.iter().enumerate().map(|(y, &x)| index[y][x]).collect());
            for (y, row) in index.iter_mut().enumerate() {
                row.remove(seam[y]);
            }
            work = RgbaImage::from_fn(ww as u32 - 1, h, |x, y| {
                let sx = if (x as usize) < seam[y as usize] {
                    x
                } else {
                    x + 1
                };
                *work.get_pixel(sx, y)
            });
            if let Some(p) = wprot.as_mut() {
                *p = GrayImage::from_fn(ww as u32 - 1, h, |x, y| {
                    let sx = if (x as usize) < seam[y as usize] {
                        x
                    } else {
                        x + 1
                    };
                    *p.get_pixel(sx, y)
                });
            }
        }
        // How many copies each original pixel gets in each row.
        let mut times = vec![vec![1usize; cw]; h as usize];
        for s in chosen.iter().take(add) {
            for (y, &x) in s.iter().enumerate() {
                times[y][x] += 1;
            }
        }
        let extra = add.saturating_sub(chosen.len());
        // More than the picture's width: the rest spread evenly.
        if extra > 0 {
            for row in times.iter_mut() {
                for i in 0..extra {
                    row[i * cw / extra] += 1;
                }
            }
        }
        let widen = |img: &RgbaImage| {
            let mut out = RgbaImage::new(new_w, h);
            for y in 0..h {
                let mut ox = 0u32;
                for x in 0..cw {
                    let q = *img.get_pixel(x as u32, y);
                    for _ in 0..times[y as usize][x] {
                        if ox < new_w {
                            out.put_pixel(ox, y, q);
                        }
                        ox += 1;
                    }
                }
            }
            out
        };
        let widen_gray = |m: &GrayImage| {
            let mut out = GrayImage::new(new_w, h);
            for y in 0..h {
                let mut ox = 0u32;
                for x in 0..cw {
                    let q = *m.get_pixel(x as u32, y);
                    for _ in 0..times[y as usize][x] {
                        if ox < new_w {
                            out.put_pixel(ox, y, q);
                        }
                        ox += 1;
                    }
                }
            }
            out
        };
        for img in imgs.iter_mut() {
            *img = widen(img);
        }
        for m in masks.iter_mut() {
            *m = widen_gray(m);
        }
    }
}

/// The picture resized to `new_w` by `new_h` pixels by taking away or
/// adding seams of the least noticed pixels, every layer and mask alike
/// (the seams found on what is seen); the selection keeps its pixels
/// (they are left out of the seams while others remain) and is then
/// dropped.
pub fn smart_resize(pi: &mut PixelImage, new_w: u32, new_h: u32) -> Option<Dirty> {
    let (w, h) = pi.img.dimensions();
    if new_w == 0 || new_h == 0 || (new_w == w && new_h == h) || w < 2 || h < 2 {
        return None;
    }
    let protect = pi.sel.take();
    let guide = pi.composite();
    // Layer pixels and masks, in a fixed order.
    let active = pi.active;
    let mut imgs = vec![guide, std::mem::take(&mut pi.img)];
    let mut slots = Vec::new();
    for (i, l) in pi.layers.iter_mut().enumerate() {
        if i != active && l.img.width() > 0 {
            imgs.push(std::mem::take(&mut l.img));
            slots.push(i);
        }
    }
    let mut masks: Vec<GrayImage> = Vec::new();
    let mut mask_slots = Vec::new();
    for (i, l) in pi.layers.iter_mut().enumerate() {
        if let Some(m) = l.mask.take() {
            masks.push(m);
            mask_slots.push(i);
        }
    }
    let mut prot = protect;
    carve_width(&mut imgs, &mut masks, prot.as_ref(), new_w);
    if let Some(p) = prot.as_mut() {
        if p.width() != new_w {
            *p = image::imageops::resize(p, new_w, h, image::imageops::FilterType::Nearest);
        }
    }
    if new_h != h {
        let mut t: Vec<RgbaImage> = imgs.iter().map(transpose).collect();
        let mut tm: Vec<GrayImage> = masks.iter().map(transpose_gray).collect();
        let tp = prot.as_ref().map(transpose_gray);
        carve_width(&mut t, &mut tm, tp.as_ref(), new_h);
        imgs = t.iter().map(transpose).collect();
        masks = tm.iter().map(transpose_gray).collect();
    }
    let mut it = imgs.into_iter();
    let _guide = it.next();
    pi.img = it.next().unwrap_or_else(|| RgbaImage::new(new_w, new_h));
    for (slot, img) in slots.into_iter().zip(it) {
        if let Some(l) = pi.layers.get_mut(slot) {
            l.img = img;
        }
    }
    for (slot, m) in mask_slots.into_iter().zip(masks) {
        if let Some(l) = pi.layers.get_mut(slot) {
            l.mask = Some(m);
        }
    }
    Some([0, 0, new_w, new_h])
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn solid(w: u32, h: u32, c: [u8; 4]) -> PixelImage {
        PixelImage::new(RgbaImage::from_pixel(w, h, Rgba(c)))
    }

    #[test]
    fn push_moves_pixels_along_with_the_brush() {
        let mut pi = solid(30, 10, [255, 255, 255, 255]);
        for y in 0..10 {
            pi.img.put_pixel(10, y, Rgba([0, 0, 0, 255]));
        }
        let o = BrushOpts {
            size: 12.0,
            hardness: 1.0,
            flow: 1.0,
            opacity: 1.0,
            ..BrushOpts::default()
        };
        let mut lq = Liquify::new(LiquifyMode::Push, o, &pi);
        lq.to(&mut pi, 10.5, 5.0);
        lq.to(&mut pi, 14.5, 5.0);
        assert_eq!(pi.img.get_pixel(10, 5)[0], 255, "the line left its place");
        assert!(
            (11..=16).any(|x| pi.img.get_pixel(x, 5)[0] < 128),
            "the line went right"
        );
        assert_eq!(pi.img.get_pixel(25, 5)[0], 255, "far away untouched");
    }

    #[test]
    fn pinch_and_bloat_pull_and_push_from_the_centre() {
        let o = BrushOpts {
            size: 20.0,
            hardness: 1.0,
            flow: 1.0,
            opacity: 1.0,
            ..BrushOpts::default()
        };
        let ring = || {
            let mut pi = solid(30, 30, [255, 255, 255, 255]);
            for y in 0..30 {
                pi.img.put_pixel(19, y, Rgba([0, 0, 0, 255]));
            }
            pi
        };
        let mut pi = ring();
        let mut lq = Liquify::new(LiquifyMode::Pinch, o, &pi);
        for _ in 0..10 {
            lq.dab(&mut pi, 15.0, 15.0, (0.0, 0.0));
        }
        let dark = |pi: &PixelImage| (0..30).find(|x| pi.img.get_pixel(*x, 15)[0] < 128);
        assert!(dark(&pi).unwrap() < 19, "pinched in: {:?}", dark(&pi));
        let mut pi = ring();
        let mut lq = Liquify::new(LiquifyMode::Bloat, o, &pi);
        for _ in 0..5 {
            lq.dab(&mut pi, 15.0, 15.0, (0.0, 0.0));
        }
        assert!(dark(&pi).map_or(true, |x| x > 19), "bloated out");
        let mut pi = ring();
        let mut lq = Liquify::new(LiquifyMode::TwirlRight, o, &pi);
        lq.dab(&mut pi, 15.0, 15.0, (0.0, 0.0));
        assert_ne!(pi.img, ring().img, "twirled");
    }

    #[test]
    fn smart_resize_takes_away_the_plain_columns_first() {
        // A flat picture with two dark bars: narrowing keeps the bars.
        let mut pi = solid(20, 6, [200, 200, 200, 255]);
        for y in 0..6 {
            for x in [4, 15] {
                pi.img.put_pixel(x, y, Rgba([0, 0, 0, 255]));
            }
        }
        smart_resize(&mut pi, 12, 6).unwrap();
        assert_eq!(pi.img.dimensions(), (12, 6));
        let bars = (0..12).filter(|x| pi.img.get_pixel(*x, 3)[0] == 0).count();
        assert_eq!(bars, 2, "both bars kept");
        smart_resize(&mut pi, 16, 9).unwrap();
        assert_eq!(pi.img.dimensions(), (16, 9));
        let bars = (0..16).filter(|x| pi.img.get_pixel(*x, 4)[0] == 0).count();
        assert_eq!(bars, 2, "widened around the bars");
    }

    #[test]
    fn smart_resize_reaches_every_layer_and_keeps_the_selection() {
        let mut pi = solid(10, 10, [200, 200, 200, 255]);
        pi.add_layer("top");
        for y in 0..10 {
            pi.img.put_pixel(2, y, Rgba([255, 0, 0, 255]));
        }
        pi.sel = crate::pixel_edit::rect_mask(10, 10, (1.0, 0.0), (4.0, 10.0));
        smart_resize(&mut pi, 6, 10).unwrap();
        assert_eq!(pi.img.dimensions(), (6, 10));
        assert_eq!(pi.layer_pixels(0).unwrap().dimensions(), (6, 10));
        assert!((0..6).any(|x| pi.img.get_pixel(x, 5)[0] == 255));
        assert!(pi.sel.is_none());
    }
}
