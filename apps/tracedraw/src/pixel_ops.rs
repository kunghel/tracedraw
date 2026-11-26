//! More pixel work for a picture being edited (Bitmaps > Edit Inside):
//! the band and edge masks, the transparency tools, the
//! Image menu's turns, canvas size, grey and black and white, and the
//! Pick tool's numeric move, scale and turn. No interface here. See
//! `docs/behavior/edit-inside-picture.md`.

use crate::fx::util::{bilinear, luma};
use crate::pixel_edit::{BrushOpts, Dirty, PixelImage};
use image::{GrayImage, Luma, Rgba, RgbaImage};

/// The band between the line through `a` and the line through `b`, both
/// square to the drag from `a` to `b`; `soft` pixels of soft edge on
/// each side.
pub fn band_mask(w: u32, h: u32, a: (f32, f32), b: (f32, f32), soft: f32) -> Option<GrayImage> {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = dx.hypot(dy);
    if len < 1.0 || w == 0 || h == 0 {
        return None;
    }
    let (ux, uy) = (dx / len, dy / len);
    let soft = soft.max(0.0);
    let mut out = GrayImage::new(w, h);
    for (x, y, q) in out.enumerate_pixels_mut() {
        let t = (x as f32 + 0.5 - a.0) * ux + (y as f32 + 0.5 - a.1) * uy;
        // Distance outside the band (negative inside).
        let out_by = (-t).max(t - len);
        let v = if out_by <= 0.0 {
            1.0
        } else if soft > 0.0 {
            let k = (1.0 - out_by / soft).clamp(0.0, 1.0);
            k * k * (3.0 - 2.0 * k)
        } else {
            0.0
        };
        q[0] = (v * 255.0).round() as u8;
    }
    Some(out)
}

/// How sharp the picture's light changes at (x, y): the size of its
/// slope.
fn edge_at(img: &RgbaImage, x: i64, y: i64) -> f32 {
    let (w, h) = (img.width() as i64, img.height() as i64);
    let l = |x: i64, y: i64| luma(img.get_pixel(x.clamp(0, w - 1) as u32, y.clamp(0, h - 1) as u32));
    let gx = l(x + 1, y - 1) + 2.0 * l(x + 1, y) + l(x + 1, y + 1)
        - l(x - 1, y - 1)
        - 2.0 * l(x - 1, y)
        - l(x - 1, y + 1);
    let gy = l(x - 1, y + 1) + 2.0 * l(x, y + 1) + l(x + 1, y + 1)
        - l(x - 1, y - 1)
        - 2.0 * l(x, y - 1)
        - l(x + 1, y - 1);
    gx.hypot(gy)
}

/// The point near (x, y), within `reach` pixels, on the strongest edge;
/// (x, y) itself where the picture is flat.
pub fn snap_to_edge(img: &RgbaImage, x: f32, y: f32, reach: f32) -> (f32, f32) {
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return (x, y);
    }
    let r = reach.max(0.0).round() as i64;
    let (cx, cy) = (x.floor() as i64, y.floor() as i64);
    let mut best = (edge_at(img, cx, cy), cx, cy);
    for oy in -r..=r {
        for ox in -r..=r {
            if ox * ox + oy * oy > r * r {
                continue;
            }
            let (px, py) = (cx + ox, cy + oy);
            if px < 0 || py < 0 || px >= w as i64 || py >= h as i64 {
                continue;
            }
            // A little pull towards the pointer, so a flat stretch keeps
            // to the hand.
            let e = edge_at(img, px, py) - 0.5 * ((ox * ox + oy * oy) as f32).sqrt();
            if e > best.0 + 4.0 {
                best = (e, px, py);
            }
        }
    }
    (best.1 as f32 + 0.5, best.2 as f32 + 0.5)
}

/// Colour to clear: the pixels within the tolerance of the colour at
/// (x, y) (touching it, or anywhere) made see-through, softly towards the
/// tolerance's edge; inside the selection. Returns the changed box.
pub fn color_to_clear(pi: &mut PixelImage, x: u32, y: u32, o: &BrushOpts) -> Option<Dirty> {
    let seed = *pi.img.get_pixel_checked(x, y)?;
    let m = crate::pixel_edit::similar_mask(&pi.img, x, y, o.tolerance, o.contiguous)?;
    let tol = o.tolerance.max(1) as f32;
    let (w, h) = pi.img.dimensions();
    let mut b: Option<Dirty> = None;
    for py in 0..h {
        for px in 0..w {
            if m.get_pixel(px, py)[0] == 0 {
                continue;
            }
            let s = pi.sel_at(px, py);
            if s <= 0.0 {
                continue;
            }
            let q = *pi.img.get_pixel(px, py);
            let d = (0..3)
                .map(|i| (q[i] as f32 - seed[i] as f32).abs())
                .fold(0.0, f32::max);
            // Fully clear at the colour, half way at the tolerance.
            let k = (1.0 - 0.5 * d / tol).clamp(0.0, 1.0) * s * o.opacity.clamp(0.0, 1.0);
            let a = (q[3] as f32 * (1.0 - k)).round() as u8;
            if a != q[3] {
                pi.img.put_pixel(px, py, Rgba([q[0], q[1], q[2], a]));
                b = Some(crate::pixel_edit::union(b, [px, py, px + 1, py + 1]));
            }
        }
    }
    b
}

/// Layer fade: the layer faded along the drag from `a` (its
/// opacity times `o.fade_from`) to `b` (times `o.fade_to`), straight or
/// round as the gradient kind says; inside the selection.
pub fn layer_fade(
    pi: &mut PixelImage,
    a: (f32, f32),
    b: (f32, f32),
    o: &BrushOpts,
) -> Option<Dirty> {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    if len2 < 1.0 {
        return None;
    }
    let len = len2.sqrt();
    let radial = o.gradient != crate::pixel_paint::GradientKind::Linear;
    let (w, h) = pi.img.dimensions();
    for y in 0..h {
        for x in 0..w {
            let s = pi.sel_at(x, y);
            if s <= 0.0 {
                continue;
            }
            let (px, py) = (x as f32 + 0.5 - a.0, y as f32 + 0.5 - a.1);
            let t = if radial {
                px.hypot(py) / len
            } else {
                (px * dx + py * dy) / len2
            }
            .clamp(0.0, 1.0);
            let k = o.fade_from + (o.fade_to - o.fade_from) * t;
            let k = 1.0 + (k.clamp(0.0, 1.0) - 1.0) * s;
            let q = pi.img.get_pixel_mut(x, y);
            q[3] = (q[3] as f32 * k).round() as u8;
        }
    }
    Some([0, 0, w, h])
}

fn turn_rgba(img: &RgbaImage, quarter: u8) -> RgbaImage {
    match quarter % 4 {
        1 => image::imageops::rotate90(img),
        2 => image::imageops::rotate180(img),
        3 => image::imageops::rotate270(img),
        _ => img.clone(),
    }
}

fn turn_gray(img: &GrayImage, quarter: u8) -> GrayImage {
    match quarter % 4 {
        1 => image::imageops::rotate90(img),
        2 => image::imageops::rotate180(img),
        3 => image::imageops::rotate270(img),
        _ => img.clone(),
    }
}

/// The whole picture turned by `quarter` quarter turns clockwise: every
/// layer, mask and the selection; it keeps its middle where it was.
pub fn turn(pi: &mut PixelImage, quarter: u8) -> Option<Dirty> {
    if quarter.is_multiple_of(4) {
        return None;
    }
    let (w, h) = pi.img.dimensions();
    pi.img = turn_rgba(&pi.img, quarter);
    for l in &mut pi.layers {
        if l.img.width() > 0 {
            l.img = turn_rgba(&l.img, quarter);
        }
        if let Some(m) = &l.mask {
            l.mask = Some(turn_gray(m, quarter));
        }
    }
    if let Some(m) = &pi.sel {
        pi.sel = Some(turn_gray(m, quarter));
    }
    let (nw, nh) = pi.img.dimensions();
    let half = |a: u32, b: u32| (a as i64 - b as i64) / 2;
    pi.origin = (pi.origin.0 + half(w, nw), pi.origin.1 + half(h, nh));
    Some([0, 0, nw, nh])
}

/// The picture's canvas made `nw` by `nh` pixels, the pixels kept at the
/// anchor (0, 0.5 or 1 across and down: left, middle or right); the new
/// room clear.
pub fn canvas_size(pi: &mut PixelImage, nw: u32, nh: u32, anchor: (f32, f32)) -> Option<Dirty> {
    let (w, h) = pi.img.dimensions();
    if nw == 0 || nh == 0 || (nw, nh) == (w, h) {
        return None;
    }
    let ox = ((nw as f32 - w as f32) * anchor.0).round() as i64;
    let oy = ((nh as f32 - h as f32) * anchor.1).round() as i64;
    let place = |img: &RgbaImage| {
        let mut out = RgbaImage::new(nw, nh);
        image::imageops::overlay(&mut out, img, ox, oy);
        out
    };
    let place_gray = |img: &GrayImage, fill: u8| {
        let mut out = GrayImage::from_pixel(nw, nh, Luma([fill]));
        image::imageops::replace(&mut out, img, ox, oy);
        out
    };
    pi.img = place(&pi.img);
    for l in &mut pi.layers {
        if l.img.width() > 0 {
            l.img = place(&l.img);
        }
        if let Some(m) = &l.mask {
            l.mask = Some(place_gray(m, 255));
        }
    }
    if let Some(m) = &pi.sel {
        pi.sel = Some(place_gray(m, 0));
    }
    pi.origin = (pi.origin.0 - ox, pi.origin.1 - oy);
    Some([0, 0, nw, nh])
}

/// The picture cut to the box of its pixels that are not clear (or, when
/// all are solid, not the colour of its top left corner). Every layer.
pub fn auto_crop(pi: &mut PixelImage) -> Option<Dirty> {
    let seen = if pi.is_layered() {
        pi.composite()
    } else {
        pi.img.clone()
    };
    let (w, h) = seen.dimensions();
    let corner = *seen.get_pixel_checked(0, 0)?;
    let solid = seen.pixels().all(|q| q[3] == 255);
    let keep = |q: &Rgba<u8>| {
        if solid {
            (0..3).any(|i| (q[i] as i16 - corner[i] as i16).abs() > 8)
        } else {
            q[3] > 0
        }
    };
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for (x, y, q) in seen.enumerate_pixels() {
        if keep(q) {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x + 1);
            y1 = y1.max(y + 1);
        }
    }
    if x0 >= x1 || y0 >= y1 || (x0, y0, x1, y1) == (0, 0, w, h) {
        return None;
    }
    let old = pi.sel.take();
    pi.sel = Some(crate::pixel_edit::rect_mask(w, h, (x0 as f32, y0 as f32), (x1 as f32, y1 as f32))?);
    let r = crate::pixel_edit::crop_to_selection(pi);
    if r.is_none() {
        pi.sel = old;
    }
    r
}

/// Every layer in greys (its light kept).
pub fn grayscale(pi: &mut PixelImage) -> Option<Dirty> {
    let grey = |img: &mut RgbaImage| {
        for q in img.pixels_mut() {
            let l = luma(q).round().clamp(0.0, 255.0) as u8;
            *q = Rgba([l, l, l, q[3]]);
        }
    };
    each_layer(pi, grey)
}

/// Every layer in black and white, split at `threshold` (0 to 255).
pub fn black_and_white(pi: &mut PixelImage, threshold: u8) -> Option<Dirty> {
    let bw = |img: &mut RgbaImage| {
        for q in img.pixels_mut() {
            let v = if luma(q) >= threshold as f32 { 255 } else { 0 };
            *q = Rgba([v, v, v, q[3]]);
        }
    };
    each_layer(pi, bw)
}

fn each_layer(pi: &mut PixelImage, f: impl Fn(&mut RgbaImage)) -> Option<Dirty> {
    f(&mut pi.img);
    for l in &mut pi.layers {
        if l.img.width() > 0 {
            f(&mut l.img);
        }
    }
    Some([0, 0, pi.img.width(), pi.img.height()])
}

/// The Pick tool's numbers: a move in pixels, a scale across and down
/// (1 is as it is), a turn in degrees clockwise and flips.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub dx: f32,
    pub dy: f32,
    pub sx: f32,
    pub sy: f32,
    pub angle: f32,
    pub flip_h: bool,
    pub flip_v: bool,
}

impl Default for Placement {
    fn default() -> Self {
        Placement {
            dx: 0.0,
            dy: 0.0,
            sx: 1.0,
            sy: 1.0,
            angle: 0.0,
            flip_h: false,
            flip_v: false,
        }
    }
}

impl Placement {
    pub fn is_identity(&self) -> bool {
        *self == Placement::default()
    }
}

/// The box the Pick tool's numbers work on: the selection's, else the
/// layer's pixels that are not clear, else the whole picture. (With no
/// selection the box was always the whole picture, so a pasted piece had
/// its handles at the picture's corners, #4261.)
pub fn pick_box(pi: &PixelImage) -> Dirty {
    let (w, h) = pi.img.dimensions();
    if pi.sel.is_some() {
        if let Some(b) = pi.sel_bounds() {
            return b;
        }
    }
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for (x, y, q) in pi.img.enumerate_pixels() {
        if q[3] > 0 {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x + 1);
            y1 = y1.max(y + 1);
        }
    }
    if x0 >= x1 {
        [0, 0, w, h]
    } else {
        [x0, y0, x1, y1]
    }
}

/// The selected pixels of the layer in use (all of them with nothing
/// selected) moved, scaled, turned and flipped about the middle of box
/// `b` (see `pick_box`); the selection goes with them.
pub fn place_in(pi: &mut PixelImage, b: Dirty, p: &Placement) -> Option<Dirty> {
    let (img, sel) = place_image(&pi.img, pi.sel.as_ref(), b, p)?;
    let (w, h) = img.dimensions();
    pi.img = img;
    if sel.is_some() {
        pi.sel = sel;
    }
    Some([0, 0, w, h])
}

/// Whether the placement only moves by whole pixels.
fn whole_pixel_move(p: &Placement) -> bool {
    p.sx == 1.0
        && p.sy == 1.0
        && p.angle == 0.0
        && !p.flip_h
        && !p.flip_v
        && p.dx.fract() == 0.0
        && p.dy.fract() == 0.0
}

/// The pixels of `img` that `sel` takes (all with no selection) placed
/// about the middle of box `b`, and the selection moved with them.
pub fn place_image(
    img: &RgbaImage,
    sel: Option<&GrayImage>,
    b: Dirty,
    p: &Placement,
) -> Option<(RgbaImage, Option<GrayImage>)> {
    if p.is_identity() || p.sx.abs() < 1e-3 || p.sy.abs() < 1e-3 {
        return None;
    }
    let (w, h) = img.dimensions();
    // A move by whole pixels of the whole layer: the pixels as they are,
    // shifted (quick enough to follow the pointer on a large picture).
    if sel.is_none() && whole_pixel_move(p) {
        return Some((
            crate::pixel_edit::offset_image(img, p.dx as i64, p.dy as i64),
            None,
        ));
    }
    let c = ((b[0] + b[2]) as f32 / 2.0, (b[1] + b[3]) as f32 / 2.0);
    let (sin, cos) = p.angle.to_radians().sin_cos();
    let sx = if p.flip_h { -p.sx } else { p.sx };
    let sy = if p.flip_v { -p.sy } else { p.sy };
    // Where each new pixel comes from: undo the move, the turn, then the
    // scale.
    let back = |x: f32, y: f32| {
        let (u, v) = (x - c.0 - p.dx, y - c.1 - p.dy);
        let (u, v) = (u * cos + v * sin, -u * sin + v * cos);
        (c.0 + u / sx, c.1 + v / sy)
    };
    let weight = |x: u32, y: u32| -> f32 {
        sel.map(|m| m.get_pixel(x, y)[0] as f32 / 255.0)
            .unwrap_or(1.0)
    };
    // The pixels that move (lifted out of the layer) and what stays.
    let mut lifted = img.clone();
    let mut stays = img.clone();
    for y in 0..h {
        for x in 0..w {
            let k = weight(x, y);
            let q = *img.get_pixel(x, y);
            lifted.put_pixel(x, y, Rgba([q[0], q[1], q[2], (q[3] as f32 * k).round() as u8]));
            stays.put_pixel(
                x,
                y,
                Rgba([q[0], q[1], q[2], (q[3] as f32 * (1.0 - k)).round() as u8]),
            );
        }
    }
    let whole = whole_pixel_move(p);
    let mut out = stays;
    let mut new_sel = sel.map(|_| GrayImage::new(w, h));
    for y in 0..h {
        for x in 0..w {
            let (u, v) = back(x as f32 + 0.5, y as f32 + 0.5);
            let q = if whole {
                // A move by whole pixels takes them as they are.
                let (mx, my) = (u - 0.5, v - 0.5);
                if mx >= 0.0 && my >= 0.0 && (mx as u32) < w && (my as u32) < h {
                    *lifted.get_pixel(mx as u32, my as u32)
                } else {
                    Rgba([0, 0, 0, 0])
                }
            } else {
                bilinear(&lifted, u - 0.5, v - 0.5, true)
            };
            if q[3] > 0 {
                let under = *out.get_pixel(x, y);
                out.put_pixel(x, y, crate::pixel_edit::over(&under, [q[0], q[1], q[2], q[3]], 1.0));
            }
            if let (Some(ns), Some(m)) = (new_sel.as_mut(), sel) {
                let (mx, my) = ((u - 0.5).round(), (v - 0.5).round());
                if mx >= 0.0 && my >= 0.0 && (mx as u32) < w && (my as u32) < h {
                    ns.put_pixel(x, y, *m.get_pixel(mx as u32, my as u32));
                }
            }
        }
    }
    Some((out, new_sel))
}

/// Where the pixel at (`x`, `y`) after placement `p` about box `b` came
/// from (see [`place_image`]).
fn placed_from(b: Dirty, p: &Placement, x: f32, y: f32) -> (f32, f32) {
    let c = ((b[0] + b[2]) as f32 / 2.0, (b[1] + b[3]) as f32 / 2.0);
    let (sin, cos) = p.angle.to_radians().sin_cos();
    let sx = if p.flip_h { -p.sx } else { p.sx };
    let sy = if p.flip_v { -p.sy } else { p.sy };
    let (u, v) = (x - c.0 - p.dx, y - c.1 - p.dy);
    let (u, v) = (u * cos + v * sin, -u * sin + v * cos);
    (c.0 + u / sx, c.1 + v / sy)
}

/// A whole layer `src` with the Move tool's placements `steps` applied
/// one after another, each pixel read once from `src`: what one step
/// took past the picture's edge comes back when a later step brings it
/// in again (#4374), instead of the pixels cut at every step.
pub fn place_chain(src: &RgbaImage, steps: &[(Dirty, Placement)]) -> Option<RgbaImage> {
    if steps
        .iter()
        .any(|(_, p)| p.sx.abs() < 1e-3 || p.sy.abs() < 1e-3)
    {
        return None;
    }
    let (w, h) = src.dimensions();
    let whole = steps.iter().all(|(_, p)| whole_pixel_move(p));
    let mut out = RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let (mut u, mut v) = (x as f32 + 0.5, y as f32 + 0.5);
            for (b, p) in steps.iter().rev() {
                (u, v) = placed_from(*b, p, u, v);
            }
            let q = if whole {
                let (mx, my) = ((u - 0.5).round(), (v - 0.5).round());
                if mx >= 0.0 && my >= 0.0 && (mx as u32) < w && (my as u32) < h {
                    *src.get_pixel(mx as u32, my as u32)
                } else {
                    Rgba([0, 0, 0, 0])
                }
            } else {
                bilinear(src, u - 0.5, v - 0.5, true)
            };
            out.put_pixel(x, y, q);
        }
    }
    Some(out)
}

/// A handle of the box the Move tool shows round what it moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoxHandle {
    /// Inside the box, or anywhere away from the handles: moves.
    Move,
    /// The round handle above the box, or just outside a corner: turns.
    Rotate,
    /// A corner or the middle of a side: -1, 0 or 1 across and down.
    Scale(i8, i8),
}

/// The box's turn and its middle once placed.
fn frame(b: Dirty, p: &Placement) -> ((f32, f32), f32, f32) {
    let c = ((b[0] + b[2]) as f32 / 2.0 + p.dx, (b[1] + b[3]) as f32 / 2.0 + p.dy);
    let (sin, cos) = p.angle.to_radians().sin_cos();
    (c, sin, cos)
}

/// The point (u, v) of the box's own frame (from its middle, scaled) on
/// the picture.
fn to_picture(b: Dirty, p: &Placement, u: f32, v: f32) -> (f32, f32) {
    let (c, sin, cos) = frame(b, p);
    (c.0 + u * cos - v * sin, c.1 + u * sin + v * cos)
}

/// The point (x, y) of the picture in the box's own frame.
fn to_frame(b: Dirty, p: &Placement, x: f32, y: f32) -> (f32, f32) {
    let (c, sin, cos) = frame(b, p);
    let (u, v) = (x - c.0, y - c.1);
    (u * cos + v * sin, -u * sin + v * cos)
}

/// Half the box's width and height once scaled.
fn half(b: Dirty, p: &Placement) -> (f32, f32) {
    (
        (b[2] - b[0]) as f32 / 2.0 * p.sx.abs(),
        (b[3] - b[1]) as f32 / 2.0 * p.sy.abs(),
    )
}

/// The placed box's corners: top left, top right, bottom right, bottom
/// left.
pub fn placed_corners(b: Dirty, p: &Placement) -> [(f32, f32); 4] {
    let (hx, hy) = half(b, p);
    [(-hx, -hy), (hx, -hy), (hx, hy), (-hx, hy)].map(|(u, v)| to_picture(b, p, u, v))
}

/// The scale handles: the corners and the middles of the sides.
pub fn scale_handles(b: Dirty, p: &Placement) -> Vec<(BoxHandle, (f32, f32))> {
    let (hx, hy) = half(b, p);
    let mut out = Vec::with_capacity(8);
    for iy in -1i8..=1 {
        for ix in -1i8..=1 {
            if ix != 0 || iy != 0 {
                let at = to_picture(b, p, ix as f32 * hx, iy as f32 * hy);
                out.push((BoxHandle::Scale(ix, iy), at));
            }
        }
    }
    out
}

/// The turning handle: `above` pixels over the middle of the top side,
/// and that middle.
pub fn rotate_handle(b: Dirty, p: &Placement, above: f32) -> ((f32, f32), (f32, f32)) {
    let (_, hy) = half(b, p);
    (to_picture(b, p, 0.0, -hy - above), to_picture(b, p, 0.0, -hy))
}

/// Which handle a press at (x, y) takes: `grab` is how near (in pixels)
/// a handle must be, `above` how far the turning handle stands over the
/// box.
pub fn box_hit(b: Dirty, p: &Placement, x: f32, y: f32, grab: f32, above: f32) -> BoxHandle {
    let near = |q: (f32, f32)| (q.0 - x).hypot(q.1 - y) <= grab;
    if near(rotate_handle(b, p, above).0) {
        return BoxHandle::Rotate;
    }
    if let Some((h, _)) = scale_handles(b, p)
        .into_iter()
        .filter(|(_, q)| near(*q))
        .min_by(|a, c| {
            let d = |q: (f32, f32)| (q.0 - x).hypot(q.1 - y);
            d(a.1).total_cmp(&d(c.1))
        })
    {
        return h;
    }
    // Just outside a corner turns.
    let (hx, hy) = half(b, p);
    let (u, v) = to_frame(b, p, x, y);
    let out_x = u.abs() > hx;
    let out_y = v.abs() > hy;
    if out_x && out_y && (u.abs() - hx).max(v.abs() - hy) <= grab * 3.0 {
        return BoxHandle::Rotate;
    }
    BoxHandle::Move
}

/// The placement after a drag of handle `h` from `from` to `to` that
/// began on `start`: moves go by whole pixels; `keep` keeps the width
/// to the height on a corner; `snap` turns in steps of 15 degrees.
pub fn drag_box(
    b: Dirty,
    start: &Placement,
    h: BoxHandle,
    from: (f32, f32),
    to: (f32, f32),
    keep: bool,
    snap: bool,
) -> Placement {
    let mut p = *start;
    match h {
        BoxHandle::Move => {
            p.dx = (start.dx + to.0 - from.0).round();
            p.dy = (start.dy + to.1 - from.1).round();
        }
        BoxHandle::Rotate => {
            let (c, _, _) = frame(b, start);
            let a0 = (from.1 - c.1).atan2(from.0 - c.0);
            let a1 = (to.1 - c.1).atan2(to.0 - c.0);
            let mut a = start.angle + (a1 - a0).to_degrees();
            if snap {
                a = (a / 15.0).round() * 15.0;
            }
            // Within -180..180.
            a = (a + 180.0).rem_euclid(360.0) - 180.0;
            p.angle = if a.abs() < 1e-3 { 0.0 } else { a };
        }
        BoxHandle::Scale(ix, iy) => {
            let (bw, bh) = ((b[2] - b[0]) as f32, (b[3] - b[1]) as f32);
            if bw < 1.0 || bh < 1.0 {
                return p;
            }
            let (hx, hy) = half(b, start);
            let f = to_frame(b, start, from.0, from.1);
            let t = to_frame(b, start, to.0, to.1);
            // The new size along one side, the opposite side staying.
            let side = |i: i8, h: f32, d: f32, len: f32| -> f32 {
                if i == 0 {
                    return 2.0 * h;
                }
                let i = i as f32;
                ((i * h + d) * i + h).max(1.0).min(len * 400.0)
            };
            let mut nw = side(ix, hx, t.0 - f.0, bw);
            let mut nh = side(iy, hy, t.1 - f.1, bh);
            if keep && ix != 0 && iy != 0 {
                let k = (nw / (2.0 * hx).max(1e-3)).max(nh / (2.0 * hy).max(1e-3));
                nw = (2.0 * hx * k).max(1.0);
                nh = (2.0 * hy * k).max(1.0);
            }
            // The middle moves so the opposite side stays where it was.
            let cu = if ix == 0 { 0.0 } else { ix as f32 * (nw / 2.0 - hx) };
            let cv = if iy == 0 { 0.0 } else { iy as f32 * (nh / 2.0 - hy) };
            let (c, sin, cos) = frame(b, start);
            let moved = (c.0 + cu * cos - cv * sin, c.1 + cu * sin + cv * cos);
            p.sx = nw / bw;
            p.sy = nh / bh;
            p.dx = start.dx + moved.0 - c.0;
            p.dy = start.dy + moved.1 - c.1;
        }
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pic(w: u32, h: u32, c: [u8; 4]) -> PixelImage {
        PixelImage::new(RgbaImage::from_pixel(w, h, Rgba(c)))
    }

    #[test]
    fn the_move_box_handles_move_scale_and_turn() {
        // #4261: the box of 10 x 20 pixels at (10, 10).
        let b = [10, 10, 20, 30];
        let p = Placement::default();
        assert_eq!(placed_corners(b, &p)[0], (10.0, 10.0));
        assert_eq!(placed_corners(b, &p)[2], (20.0, 30.0));
        assert_eq!(box_hit(b, &p, 20.0, 30.0, 2.0, 5.0), BoxHandle::Scale(1, 1));
        assert_eq!(box_hit(b, &p, 15.0, 30.0, 2.0, 5.0), BoxHandle::Scale(0, 1));
        assert_eq!(box_hit(b, &p, 15.0, 5.0, 2.0, 5.0), BoxHandle::Rotate);
        assert_eq!(box_hit(b, &p, 22.5, 32.5, 2.0, 5.0), BoxHandle::Rotate);
        assert_eq!(box_hit(b, &p, 15.0, 20.0, 2.0, 5.0), BoxHandle::Move);
        // A move goes by whole pixels.
        let m = drag_box(b, &p, BoxHandle::Move, (15.0, 15.0), (18.4, 13.6), true, false);
        assert_eq!((m.dx, m.dy), (3.0, -1.0));
        // The right side dragged 10 pixels out: twice as wide, the left
        // side where it was.
        let s = drag_box(b, &p, BoxHandle::Scale(1, 0), (20.0, 20.0), (30.0, 20.0), true, false);
        assert!((s.sx - 2.0).abs() < 1e-4 && (s.sy - 1.0).abs() < 1e-4, "{s:?}");
        let c = placed_corners(b, &s);
        assert!((c[0].0 - 10.0).abs() < 1e-3 && (c[1].0 - 30.0).abs() < 1e-3, "{c:?}");
        // A corner keeps the width to the height; Shift (keep off) not.
        let k = drag_box(b, &p, BoxHandle::Scale(1, 1), (20.0, 30.0), (30.0, 30.0), true, false);
        assert!((k.sx - 2.0).abs() < 1e-4 && (k.sy - 2.0).abs() < 1e-4, "{k:?}");
        let f = drag_box(b, &p, BoxHandle::Scale(1, 1), (20.0, 30.0), (30.0, 30.0), false, false);
        assert!((f.sx - 2.0).abs() < 1e-4 && (f.sy - 1.0).abs() < 1e-4, "{f:?}");
        let c = placed_corners(b, &k);
        assert!((c[0].0 - 10.0).abs() < 1e-3 && (c[0].1 - 10.0).abs() < 1e-3, "{c:?}");
        // The top left corner dragged in never turns the box inside out.
        let tiny = drag_box(b, &p, BoxHandle::Scale(-1, -1), (10.0, 10.0), (60.0, 60.0), false, false);
        assert!(tiny.sx > 0.0 && tiny.sy > 0.0);
        // A quarter turn round the middle (15, 20); Shift snaps it.
        let r = drag_box(b, &p, BoxHandle::Rotate, (15.0, 5.0), (30.0, 20.0), true, false);
        assert!((r.angle - 90.0).abs() < 1e-3, "{r:?}");
        let r = drag_box(b, &p, BoxHandle::Rotate, (15.0, 5.0), (30.0, 19.0), true, true);
        assert!((r.angle - 90.0).abs() < 1e-3, "{r:?}");
        // The handles follow the turn.
        assert_eq!(box_hit(b, &r, 25.0, 20.0, 2.0, 5.0), BoxHandle::Scale(0, -1));
    }

    #[test]
    fn a_whole_pixel_move_keeps_the_pixels_sharp() {
        let mut pi = pic(10, 10, [0, 0, 0, 0]);
        pi.img.put_pixel(2, 2, Rgba([10, 20, 30, 255]));
        pi.sel = Some(GrayImage::from_pixel(10, 10, Luma([255])));
        let p = Placement { dx: 3.0, dy: 1.0, ..Placement::default() };
        place_in(&mut pi, [0, 0, 10, 10], &p).unwrap();
        assert_eq!(*pi.img.get_pixel(5, 3), Rgba([10, 20, 30, 255]));
        assert_eq!(pi.img.get_pixel(2, 2)[3], 0);
        // No selection: the box is the pixels that show.
        pi.sel = None;
        assert_eq!(pick_box(&pi), [5, 3, 6, 4]);
    }

    #[test]
    fn the_band_mask_lies_between_the_two_lines() {
        let m = band_mask(40, 10, (10.0, 5.0), (20.0, 5.0), 0.0).unwrap();
        assert_eq!(m.get_pixel(15, 5)[0], 255);
        assert_eq!(m.get_pixel(5, 5)[0], 0);
        assert_eq!(m.get_pixel(25, 5)[0], 0);
        let soft = band_mask(40, 10, (10.0, 5.0), (20.0, 5.0), 6.0).unwrap();
        let v = soft.get_pixel(23, 5)[0];
        assert!(v > 0 && v < 255, "{v}");
    }

    #[test]
    fn the_edge_mask_outline_clings_to_an_edge() {
        let mut img = RgbaImage::from_pixel(30, 30, Rgba([0, 0, 0, 255]));
        for y in 0..30 {
            for x in 15..30 {
                img.put_pixel(x, y, Rgba([255, 255, 255, 255]));
            }
        }
        let (x, _) = snap_to_edge(&img, 11.0, 10.0, 6.0);
        assert!((13.0..=16.5).contains(&x), "{x}");
        // Flat: stays with the pointer.
        let flat = RgbaImage::from_pixel(30, 30, Rgba([9, 9, 9, 255]));
        assert_eq!(snap_to_edge(&flat, 11.2, 10.7, 6.0), (11.5, 10.5));
    }

    #[test]
    fn colour_to_clear_clears_the_colour_clicked() {
        let mut pi = pic(10, 10, [200, 0, 0, 255]);
        pi.img.put_pixel(9, 9, Rgba([0, 0, 200, 255]));
        let o = BrushOpts {
            tolerance: 10,
            contiguous: false,
            ..Default::default()
        };
        assert!(color_to_clear(&mut pi, 0, 0, &o).is_some());
        assert_eq!(pi.img.get_pixel(3, 3)[3], 0);
        assert_eq!(pi.img.get_pixel(9, 9)[3], 255);
    }

    #[test]
    fn layer_fade_fades_along_the_drag() {
        let mut pi = pic(20, 4, [10, 10, 10, 255]);
        let o = BrushOpts::default();
        layer_fade(&mut pi, (0.0, 2.0), (20.0, 2.0), &o);
        assert!(pi.img.get_pixel(0, 2)[3] > 240);
        assert!(pi.img.get_pixel(19, 2)[3] < 20);
        let mid = pi.img.get_pixel(10, 2)[3];
        assert!((100..160).contains(&mid), "{mid}");
    }

    #[test]
    fn a_quarter_turn_swaps_the_sides_and_keeps_the_middle() {
        let mut pi = pic(40, 20, [1, 2, 3, 255]);
        pi.img.put_pixel(0, 0, Rgba([255, 0, 0, 255]));
        turn(&mut pi, 1).unwrap();
        assert_eq!(pi.img.dimensions(), (20, 40));
        assert_eq!(pi.img.get_pixel(19, 0)[0], 255);
        assert_eq!(pi.origin, (10, -10));
    }

    #[test]
    fn the_canvas_grows_round_the_anchor() {
        let mut pi = pic(10, 10, [5, 5, 5, 255]);
        canvas_size(&mut pi, 20, 30, (0.5, 0.5)).unwrap();
        assert_eq!(pi.img.dimensions(), (20, 30));
        assert_eq!(pi.img.get_pixel(0, 0)[3], 0);
        assert_eq!(pi.img.get_pixel(10, 15)[3], 255);
        assert_eq!(pi.origin, (-5, -10));
    }

    #[test]
    fn auto_crop_keeps_the_pixels_that_show() {
        let mut pi = pic(20, 20, [0, 0, 0, 0]);
        for y in 5..9 {
            for x in 3..12 {
                pi.img.put_pixel(x, y, Rgba([9, 9, 9, 255]));
            }
        }
        auto_crop(&mut pi).unwrap();
        assert_eq!(pi.img.dimensions(), (9, 4));
        assert_eq!(pi.origin, (3, 5));
    }

    #[test]
    fn grey_and_black_and_white_change_every_pixel() {
        let mut pi = pic(4, 4, [200, 40, 40, 255]);
        grayscale(&mut pi);
        let q = pi.img.get_pixel(1, 1);
        assert!(q[0] == q[1] && q[1] == q[2]);
        black_and_white(&mut pi, 128);
        assert_eq!(pi.img.get_pixel(1, 1)[0], 0);
    }

    #[test]
    fn the_pick_numbers_move_and_turn_the_selected_pixels() {
        let mut pi = pic(20, 20, [0, 0, 0, 0]);
        pi.img.put_pixel(5, 5, Rgba([255, 0, 0, 255]));
        let p = Placement {
            dx: 4.0,
            ..Default::default()
        };
        let b = pick_box(&pi);
        place_in(&mut pi, b, &p).unwrap();
        assert_eq!(pi.img.get_pixel(9, 5)[3], 255);
        assert_eq!(pi.img.get_pixel(5, 5)[3], 0);
        // A half turn about the middle of the picture's box.
        let mut pi = pic(10, 10, [0, 0, 0, 255]);
        pi.img.put_pixel(1, 1, Rgba([255, 255, 255, 255]));
        let p = Placement {
            angle: 180.0,
            ..Default::default()
        };
        let b = pick_box(&pi);
        place_in(&mut pi, b, &p).unwrap();
        assert_eq!(pi.img.get_pixel(8, 8)[0], 255);
    }

    #[test]
    fn the_transparency_brush_paints_to_the_level_chosen() {
        let mut pi = pic(20, 20, [9, 9, 9, 255]);
        let o = BrushOpts {
            size: 6.0,
            hardness: 1.0,
            opacity: 1.0,
            flow: 1.0,
            transparency: 0.5,
            ..Default::default()
        };
        let mut s = crate::pixel_edit::Stroke::new(
            crate::pixel_edit::PixelTool::TransparencyBrush,
            o,
            &pi,
            (0.0, 0.0),
        );
        s.to(&mut pi, 10.0, 10.0);
        let a = pi.img.get_pixel(10, 10)[3];
        assert!((120..=135).contains(&a), "{a}");
        assert_eq!(pi.img.get_pixel(1, 1)[3], 255);
    }
}
