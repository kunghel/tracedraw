//! Editing a picture's pixels in place: the pixel work behind Bitmaps >
//! Edit Inside (a double click on a picture). Everything here works on an
//! RGBA image and an optional selection mask, with no interface: the
//! selection tools (rectangle, ellipse, freehand, magic wand), the
//! brushes (paint, eraser, clone, heal, dodge, burn, sponge, blur,
//! sharpen, smudge, red eye), the fill, the gradient, effects run inside
//! the selection, content-aware fill and the undo steps. See
//! `docs/behavior/edit-inside-picture.md`.

use image::{GrayImage, Luma, Rgba, RgbaImage};

/// The tools of the picture editing bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PixelTool {
    RectSelect,
    EllipseSelect,
    Lasso,
    MagicWand,
    Brush,
    Eraser,
    Clone,
    Heal,
    Fill,
    Gradient,
    Dodge,
    Burn,
    Sponge,
    Blur,
    Sharpen,
    Smudge,
    RedEye,
    Eyedropper,
}

impl PixelTool {
    pub const ALL: [PixelTool; 18] = [
        PixelTool::RectSelect,
        PixelTool::EllipseSelect,
        PixelTool::Lasso,
        PixelTool::MagicWand,
        PixelTool::Brush,
        PixelTool::Eraser,
        PixelTool::Clone,
        PixelTool::Heal,
        PixelTool::Fill,
        PixelTool::Gradient,
        PixelTool::Dodge,
        PixelTool::Burn,
        PixelTool::Sponge,
        PixelTool::Blur,
        PixelTool::Sharpen,
        PixelTool::Smudge,
        PixelTool::RedEye,
        PixelTool::Eyedropper,
    ];

    /// The tool's i18n key (`pixel.tool.<name>`).
    pub fn key(self) -> &'static str {
        match self {
            PixelTool::RectSelect => "pixel.tool.rect_select",
            PixelTool::EllipseSelect => "pixel.tool.ellipse_select",
            PixelTool::Lasso => "pixel.tool.lasso",
            PixelTool::MagicWand => "pixel.tool.magic_wand",
            PixelTool::Brush => "pixel.tool.brush",
            PixelTool::Eraser => "pixel.tool.eraser",
            PixelTool::Clone => "pixel.tool.clone",
            PixelTool::Heal => "pixel.tool.heal",
            PixelTool::Fill => "pixel.tool.fill",
            PixelTool::Gradient => "pixel.tool.gradient",
            PixelTool::Dodge => "pixel.tool.dodge",
            PixelTool::Burn => "pixel.tool.burn",
            PixelTool::Sponge => "pixel.tool.sponge",
            PixelTool::Blur => "pixel.tool.blur",
            PixelTool::Sharpen => "pixel.tool.sharpen",
            PixelTool::Smudge => "pixel.tool.smudge",
            PixelTool::RedEye => "pixel.tool.red_eye",
            PixelTool::Eyedropper => "pixel.tool.eyedropper",
        }
    }

    /// A tool painted with a round brush along a drag.
    pub fn is_brush(self) -> bool {
        matches!(
            self,
            PixelTool::Brush
                | PixelTool::Eraser
                | PixelTool::Clone
                | PixelTool::Heal
                | PixelTool::Dodge
                | PixelTool::Burn
                | PixelTool::Sponge
                | PixelTool::Blur
                | PixelTool::Sharpen
                | PixelTool::Smudge
                | PixelTool::RedEye
        )
    }

    pub fn is_selection(self) -> bool {
        matches!(
            self,
            PixelTool::RectSelect
                | PixelTool::EllipseSelect
                | PixelTool::Lasso
                | PixelTool::MagicWand
        )
    }
}

/// How a new selection combines with the one there is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectMode {
    Replace,
    Add,
    Subtract,
}

/// The settings of the brushes, the fill and the magic wand.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BrushOpts {
    /// Diameter, picture pixels.
    pub size: f32,
    /// 0 (soft edge all the way to the centre) to 1 (hard edge).
    pub hardness: f32,
    /// Most a stroke changes a pixel, 0 to 1.
    pub opacity: f32,
    /// How much each dab adds, 0 to 1.
    pub flow: f32,
    /// The paint colour (straight RGBA).
    pub color: [u8; 4],
    /// Colour distance the wand and the fill take in, 0 to 255.
    pub tolerance: u8,
    /// The wand and the fill spread only to touching pixels.
    pub contiguous: bool,
    /// The sponge takes colour away instead of adding it.
    pub desaturate: bool,
    /// The gradient goes from the colour to see-through (else to white).
    pub gradient_to_clear: bool,
}

impl Default for BrushOpts {
    fn default() -> Self {
        BrushOpts {
            size: 40.0,
            hardness: 0.5,
            opacity: 1.0,
            flow: 0.6,
            color: [0, 0, 0, 255],
            tolerance: 32,
            contiguous: true,
            desaturate: false,
            gradient_to_clear: true,
        }
    }
}

/// The picture being edited and its selection (255 inside, 0 outside,
/// soft in between). No selection means the whole picture.
#[derive(Debug, Clone, PartialEq)]
pub struct PixelImage {
    pub img: RgbaImage,
    pub sel: Option<GrayImage>,
    /// Where the picture's top left pixel lies in the picture as it was
    /// first opened (it moves when the picture is cropped).
    pub origin: (i64, i64),
}

/// A rectangle of changed pixels: x0, y0 inclusive, x1, y1 exclusive.
pub type Dirty = [u32; 4];

pub fn union(a: Option<Dirty>, b: Dirty) -> Dirty {
    match a {
        None => b,
        Some(a) => [
            a[0].min(b[0]),
            a[1].min(b[1]),
            a[2].max(b[2]),
            a[3].max(b[3]),
        ],
    }
}

impl PixelImage {
    pub fn new(img: RgbaImage) -> Self {
        PixelImage {
            img,
            sel: None,
            origin: (0, 0),
        }
    }

    pub fn width(&self) -> u32 {
        self.img.width()
    }

    pub fn height(&self) -> u32 {
        self.img.height()
    }

    /// How much of pixel (x, y) the selection takes in, 0 to 1.
    pub fn sel_at(&self, x: u32, y: u32) -> f32 {
        match &self.sel {
            None => 1.0,
            Some(m) => m
                .get_pixel_checked(x, y)
                .map(|v| v[0] as f32 / 255.0)
                .unwrap_or(0.0),
        }
    }

    /// The selection's bounding box (the whole picture with none).
    pub fn sel_bounds(&self) -> Option<Dirty> {
        let (w, h) = self.img.dimensions();
        let Some(m) = &self.sel else {
            return (w > 0 && h > 0).then_some([0, 0, w, h]);
        };
        let mut b: Option<Dirty> = None;
        for (x, y, v) in m.enumerate_pixels() {
            if v[0] > 0 {
                b = Some(union(b, [x, y, x + 1, y + 1]));
            }
        }
        b
    }
}

// ---------------------------------------------------------------------
// Selections
// ---------------------------------------------------------------------

/// A mask of the picture's size from a filled path (tiny-skia, smooth
/// edges). `None` when the path is empty or the picture has no pixels.
fn mask_from_path(
    w: u32,
    h: u32,
    build: impl FnOnce(&mut tiny_skia::PathBuilder),
) -> Option<GrayImage> {
    let mut pb = tiny_skia::PathBuilder::new();
    build(&mut pb);
    let path = pb.finish()?;
    let mut pix = tiny_skia::Pixmap::new(w.max(1), h.max(1))?;
    let mut paint = tiny_skia::Paint::default();
    paint.set_color_rgba8(255, 255, 255, 255);
    paint.anti_alias = true;
    pix.fill_path(
        &path,
        &paint,
        tiny_skia::FillRule::Winding,
        tiny_skia::Transform::identity(),
        None,
    );
    let data: Vec<u8> = pix.pixels().iter().map(|p| p.alpha()).collect();
    GrayImage::from_raw(w.max(1), h.max(1), data)
}

/// A rectangle selection between two corners (picture pixels).
pub fn rect_mask(w: u32, h: u32, a: (f32, f32), b: (f32, f32)) -> Option<GrayImage> {
    let r = tiny_skia::Rect::from_ltrb(a.0.min(b.0), a.1.min(b.1), a.0.max(b.0), a.1.max(b.1))?;
    mask_from_path(w, h, |pb| pb.push_rect(r))
}

/// An ellipse selection in the box between two corners.
pub fn ellipse_mask(w: u32, h: u32, a: (f32, f32), b: (f32, f32)) -> Option<GrayImage> {
    let r = tiny_skia::Rect::from_ltrb(a.0.min(b.0), a.1.min(b.1), a.0.max(b.0), a.1.max(b.1))?;
    mask_from_path(w, h, |pb| pb.push_oval(r))
}

/// A freehand selection: the outline the points make, closed.
pub fn lasso_mask(w: u32, h: u32, pts: &[(f32, f32)]) -> Option<GrayImage> {
    if pts.len() < 3 {
        return None;
    }
    mask_from_path(w, h, |pb| {
        pb.move_to(pts[0].0, pts[0].1);
        for p in &pts[1..] {
            pb.line_to(p.0, p.1);
        }
        pb.close();
    })
}

fn color_distance(a: &Rgba<u8>, b: &Rgba<u8>) -> u8 {
    (0..4)
        .map(|i| (a[i] as i16 - b[i] as i16).unsigned_abs() as u8)
        .max()
        .unwrap_or(0)
}

/// The pixels like the one at (x, y): within `tolerance` of its colour,
/// touching it (`contiguous`) or anywhere. 255 for those, 0 for the rest.
pub fn similar_mask(
    img: &RgbaImage,
    x: u32,
    y: u32,
    tolerance: u8,
    contiguous: bool,
) -> Option<GrayImage> {
    let (w, h) = img.dimensions();
    let seed = *img.get_pixel_checked(x, y)?;
    let mut out = GrayImage::new(w, h);
    if !contiguous {
        for (px, py, q) in img.enumerate_pixels() {
            if color_distance(q, &seed) <= tolerance {
                out.put_pixel(px, py, Luma([255]));
            }
        }
        return Some(out);
    }
    let mut stack = vec![(x, y)];
    while let Some((px, py)) = stack.pop() {
        if out.get_pixel(px, py)[0] != 0 || color_distance(img.get_pixel(px, py), &seed) > tolerance
        {
            continue;
        }
        out.put_pixel(px, py, Luma([255]));
        if px > 0 {
            stack.push((px - 1, py));
        }
        if py > 0 {
            stack.push((px, py - 1));
        }
        if px + 1 < w {
            stack.push((px + 1, py));
        }
        if py + 1 < h {
            stack.push((px, py + 1));
        }
    }
    Some(out)
}

/// The selection `new` combined with the one there is.
pub fn combine(old: Option<&GrayImage>, new: GrayImage, mode: SelectMode) -> Option<GrayImage> {
    match (mode, old) {
        (SelectMode::Replace, _) | (SelectMode::Add, None) => Some(new),
        // Taking away from "everything".
        (SelectMode::Subtract, None) => {
            let mut m = new;
            m.pixels_mut().for_each(|v| v[0] = 255 - v[0]);
            Some(m)
        }
        (SelectMode::Add, Some(o)) => {
            let mut m = o.clone();
            for (a, b) in m.pixels_mut().zip(new.pixels()) {
                a[0] = a[0].max(b[0]);
            }
            Some(m)
        }
        (SelectMode::Subtract, Some(o)) => {
            let mut m = o.clone();
            for (a, b) in m.pixels_mut().zip(new.pixels()) {
                a[0] = ((a[0] as u16 * (255 - b[0]) as u16) / 255) as u8;
            }
            Some(m)
        }
    }
}

/// Everything outside the selection (nothing selected: nothing).
pub fn invert(sel: Option<&GrayImage>, w: u32, h: u32) -> GrayImage {
    match sel {
        None => GrayImage::new(w, h),
        Some(m) => {
            let mut m = m.clone();
            m.pixels_mut().for_each(|v| v[0] = 255 - v[0]);
            m
        }
    }
}

/// Blur of a mask with three box passes of radius `r` (about a Gaussian).
pub fn feather(m: &GrayImage, r: u32) -> GrayImage {
    if r == 0 {
        return m.clone();
    }
    let (w, h) = m.dimensions();
    let mut buf: Vec<f32> = m.pixels().map(|v| v[0] as f32).collect();
    let r = r.max(1) as i64;
    for _ in 0..3 {
        buf = box_pass(&buf, w as usize, h as usize, r, true);
        buf = box_pass(&buf, w as usize, h as usize, r, false);
    }
    GrayImage::from_fn(w, h, |x, y| {
        Luma([buf
            .get(y as usize * w as usize + x as usize)
            .map(|v| v.round().clamp(0.0, 255.0) as u8)
            .unwrap_or(0)])
    })
}

fn box_pass(src: &[f32], w: usize, h: usize, r: i64, horizontal: bool) -> Vec<f32> {
    let mut out = vec![0.0; src.len()];
    let (lines, len) = if horizontal { (h, w) } else { (w, h) };
    let idx = |line: usize, i: usize| {
        if horizontal {
            line * w + i
        } else {
            i * w + line
        }
    };
    for line in 0..lines {
        let mut acc = 0.0;
        let at = |i: i64| -> f32 {
            let i = i.clamp(0, len as i64 - 1) as usize;
            src.get(idx(line, i)).copied().unwrap_or(0.0)
        };
        for i in -r..=r {
            acc += at(i);
        }
        let n = (2 * r + 1) as f32;
        for i in 0..len {
            if let Some(o) = out.get_mut(idx(line, i)) {
                *o = acc / n;
            }
            acc += at(i as i64 + r + 1) - at(i as i64 - r);
        }
    }
    out
}

/// The selection grown (`by` > 0) or shrunk (`by` < 0) by whole pixels.
pub fn grow(m: &GrayImage, by: i32) -> GrayImage {
    let (w, h) = m.dimensions();
    let r = by.unsigned_abs() as i64;
    if r == 0 {
        return m.clone();
    }
    let take = |a: u8, b: u8| if by > 0 { a.max(b) } else { a.min(b) };
    let pass = |src: &GrayImage, horizontal: bool| {
        GrayImage::from_fn(w, h, |x, y| {
            let mut v = src.get_pixel(x, y)[0];
            for d in -r..=r {
                let (sx, sy) = if horizontal {
                    (x as i64 + d, y as i64)
                } else {
                    (x as i64, y as i64 + d)
                };
                let q = if sx < 0 || sy < 0 || sx >= w as i64 || sy >= h as i64 {
                    // Shrinking eats in from the picture's edges too.
                    if by > 0 {
                        0
                    } else {
                        v
                    }
                } else {
                    src.get_pixel(sx as u32, sy as u32)[0]
                };
                v = take(v, q);
            }
            Luma([v])
        })
    };
    pass(&pass(m, true), false)
}

// ---------------------------------------------------------------------
// Whole-selection operations
// ---------------------------------------------------------------------

/// Straight-alpha "source over": `top` with coverage `t` over `base`.
pub fn over(base: &Rgba<u8>, top: [u8; 4], t: f32) -> Rgba<u8> {
    let ta = top[3] as f32 / 255.0 * t.clamp(0.0, 1.0);
    let ba = base[3] as f32 / 255.0;
    let oa = ta + ba * (1.0 - ta);
    if oa <= 0.0 {
        return Rgba([0, 0, 0, 0]);
    }
    let ch = |i: usize| {
        let c = (top[i] as f32 * ta + base[i] as f32 * ba * (1.0 - ta)) / oa;
        c.round().clamp(0.0, 255.0) as u8
    };
    Rgba([ch(0), ch(1), ch(2), (oa * 255.0).round() as u8])
}

/// Mix of two pixels, `t` = 0 is `a`, premultiplied so clear pixels
/// lend no colour.
pub fn mix(a: &Rgba<u8>, b: &Rgba<u8>, t: f32) -> Rgba<u8> {
    crate::fx::util::lerp_premultiplied(a, b, t.clamp(0.0, 1.0))
}

/// The selected pixels made clear (Delete).
pub fn clear_selection(pi: &mut PixelImage) -> Option<Dirty> {
    let b = pi.sel_bounds()?;
    for y in b[1]..b[3] {
        for x in b[0]..b[2] {
            let s = pi.sel_at(x, y);
            if s > 0.0 {
                let q = pi.img.get_pixel_mut(x, y);
                q[3] = (q[3] as f32 * (1.0 - s)).round() as u8;
            }
        }
    }
    Some(b)
}

/// The selection filled with a colour.
pub fn fill_selection(pi: &mut PixelImage, color: [u8; 4], opacity: f32) -> Option<Dirty> {
    let b = pi.sel_bounds()?;
    for y in b[1]..b[3] {
        for x in b[0]..b[2] {
            let s = pi.sel_at(x, y) * opacity;
            if s > 0.0 {
                let q = *pi.img.get_pixel(x, y);
                pi.img.put_pixel(x, y, over(&q, color, s));
            }
        }
    }
    Some(b)
}

/// The fill (paint bucket): the pixels like the clicked one, in the
/// selection, filled with the colour.
pub fn bucket_fill(pi: &mut PixelImage, x: u32, y: u32, o: &BrushOpts) -> Option<Dirty> {
    let m = similar_mask(&pi.img, x, y, o.tolerance, o.contiguous)?;
    let mut b: Option<Dirty> = None;
    for (px, py, v) in m.enumerate_pixels() {
        if v[0] == 0 {
            continue;
        }
        let s = pi.sel_at(px, py) * o.opacity;
        if s > 0.0 {
            let q = *pi.img.get_pixel(px, py);
            pi.img.put_pixel(px, py, over(&q, o.color, s));
            b = Some(union(b, [px, py, px + 1, py + 1]));
        }
    }
    b
}

/// A straight gradient from `a` to `b` (picture pixels) in the
/// selection: the colour at `a`, fading to clear (or white) at `b`.
pub fn gradient(pi: &mut PixelImage, a: (f32, f32), b: (f32, f32), o: &BrushOpts) -> Option<Dirty> {
    let bounds = pi.sel_bounds()?;
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = (dx * dx + dy * dy).max(1e-6);
    let end = if o.gradient_to_clear {
        [o.color[0], o.color[1], o.color[2], 0]
    } else {
        [255, 255, 255, 255]
    };
    for y in bounds[1]..bounds[3] {
        for x in bounds[0]..bounds[2] {
            let s = pi.sel_at(x, y) * o.opacity;
            if s <= 0.0 {
                continue;
            }
            let t = (((x as f32 + 0.5 - a.0) * dx + (y as f32 + 0.5 - a.1) * dy) / len2)
                .clamp(0.0, 1.0);
            let c = [0, 1, 2, 3].map(|i| {
                (o.color[i] as f32 + (end[i] as f32 - o.color[i] as f32) * t).round() as u8
            });
            let q = *pi.img.get_pixel(x, y);
            pi.img.put_pixel(x, y, over(&q, c, s));
        }
    }
    Some(bounds)
}

/// `edited` (the whole picture run through an effect) kept only inside
/// the selection: the effect applied to the selected part.
pub fn blend_through_selection(pi: &mut PixelImage, edited: &RgbaImage) -> Option<Dirty> {
    let b = pi.sel_bounds()?;
    if edited.dimensions() != pi.img.dimensions() {
        return None;
    }
    for y in b[1]..b[3] {
        for x in b[0]..b[2] {
            let s = pi.sel_at(x, y);
            if s >= 1.0 {
                pi.img.put_pixel(x, y, *edited.get_pixel(x, y));
            } else if s > 0.0 {
                let q = mix(pi.img.get_pixel(x, y), edited.get_pixel(x, y), s);
                pi.img.put_pixel(x, y, q);
            }
        }
    }
    Some(b)
}

/// A bitmap effect run on the picture and kept inside the selection.
/// An effect that changes the picture's size is fitted back to it.
pub fn apply_effect(pi: &mut PixelImage, e: &tracedraw_core::BitmapEffect) -> Option<Dirty> {
    let mut out = crate::fx::apply_effect(&pi.img, e);
    if out.dimensions() != pi.img.dimensions() {
        out = crate::bitmap_fx::resample(&out, pi.img.width(), pi.img.height());
    }
    blend_through_selection(pi, &out)
}

/// Red taken out of the eyes: in the selection, pixels where red stands
/// well above green and blue get the red of their green and blue.
pub fn red_eye_selection(pi: &mut PixelImage) -> Option<Dirty> {
    let b = pi.sel_bounds()?;
    for y in b[1]..b[3] {
        for x in b[0]..b[2] {
            let s = pi.sel_at(x, y);
            if s > 0.0 {
                let q = *pi.img.get_pixel(x, y);
                pi.img.put_pixel(x, y, red_eye_px(&q, s));
            }
        }
    }
    Some(b)
}

fn red_eye_px(q: &Rgba<u8>, t: f32) -> Rgba<u8> {
    let (r, g, b) = (q[0] as f32, q[1] as f32, q[2] as f32);
    let gb = (g + b) / 2.0;
    if r > 60.0 && r > gb * 1.4 {
        let nr = r + (gb - r) * t;
        Rgba([nr.round() as u8, q[1], q[2], q[3]])
    } else {
        *q
    }
}

/// The picture flipped left to right (or top to bottom), selection too.
pub fn flip(pi: &mut PixelImage, horizontal: bool) {
    if horizontal {
        image::imageops::flip_horizontal_in_place(&mut pi.img);
        if let Some(m) = &mut pi.sel {
            image::imageops::flip_horizontal_in_place(m);
        }
    } else {
        image::imageops::flip_vertical_in_place(&mut pi.img);
        if let Some(m) = &mut pi.sel {
            image::imageops::flip_vertical_in_place(m);
        }
    }
}

/// The picture cut to the selection's box; the pixels outside the
/// selection inside the box become clear. Returns the box (in the
/// picture before the cut).
pub fn crop_to_selection(pi: &mut PixelImage) -> Option<Dirty> {
    let b = pi.sel_bounds()?;
    let (w, h) = (b[2] - b[0], b[3] - b[1]);
    if w == 0 || h == 0 {
        return None;
    }
    let mut out = image::imageops::crop_imm(&pi.img, b[0], b[1], w, h).to_image();
    if pi.sel.is_some() {
        for (x, y, q) in out.enumerate_pixels_mut() {
            let s = pi.sel_at(x + b[0], y + b[1]);
            q[3] = (q[3] as f32 * s).round() as u8;
        }
    }
    pi.img = out;
    pi.sel = None;
    pi.origin = (pi.origin.0 + b[0] as i64, pi.origin.1 + b[1] as i64);
    Some(b)
}

// ---------------------------------------------------------------------
// Brush strokes
// ---------------------------------------------------------------------

/// The weight of a round dab of diameter `size` at distance `d` from its
/// centre: 1 inside the hard core, easing to 0 at the edge.
pub fn dab_weight(d: f32, size: f32, hardness: f32) -> f32 {
    let r = (size / 2.0).max(0.5);
    if d >= r {
        return 0.0;
    }
    let inner = r * hardness.clamp(0.0, 1.0);
    if d <= inner {
        return 1.0;
    }
    let t = 1.0 - (d - inner) / (r - inner).max(1e-3);
    t * t * (3.0 - 2.0 * t)
}

/// One stroke of a brush, from the press to the release. The pixels are
/// worked out from the picture as it was at the press and how much the
/// stroke covers each one, so going over a place again within a stroke
/// never goes past the brush's opacity.
pub struct Stroke {
    pub tool: PixelTool,
    pub opts: BrushOpts,
    base: RgbaImage,
    cover: Vec<f32>,
    last: Option<(f32, f32)>,
    /// Clone and heal: where the source is, as an offset from the brush.
    pub offset: (f32, f32),
    /// Smudge: the colours carried along.
    carried: Vec<[f32; 4]>,
    pub dirty: Option<Dirty>,
}

impl Stroke {
    pub fn new(tool: PixelTool, opts: BrushOpts, pi: &PixelImage, offset: (f32, f32)) -> Self {
        let n = (pi.width() as usize) * (pi.height() as usize);
        Stroke {
            tool,
            opts,
            base: pi.img.clone(),
            cover: vec![0.0; n],
            last: None,
            offset,
            carried: Vec::new(),
            dirty: None,
        }
    }

    /// The brush taken to (x, y): dabs along the way from the last
    /// point, a quarter of the brush apart.
    pub fn to(&mut self, pi: &mut PixelImage, x: f32, y: f32) -> Option<Dirty> {
        let step = (self.opts.size * 0.25).max(1.0);
        let mut changed: Option<Dirty> = None;
        match self.last {
            None => {
                changed = self.dab(pi, x, y);
            }
            Some((lx, ly)) => {
                let d = ((x - lx).powi(2) + (y - ly).powi(2)).sqrt();
                if d < step {
                    return None;
                }
                let n = (d / step).floor() as usize;
                for i in 1..=n {
                    let t = i as f32 * step / d;
                    let (px, py) = (lx + (x - lx) * t, ly + (y - ly) * t);
                    if let Some(b) = self.dab(pi, px, py) {
                        changed = Some(union(changed, b));
                    }
                }
                let t = n as f32 * step / d;
                self.last = Some((lx + (x - lx) * t, ly + (y - ly) * t));
                if let Some(b) = changed {
                    self.dirty = Some(union(self.dirty, b));
                }
                return changed;
            }
        }
        self.last = Some((x, y));
        if let Some(b) = changed {
            self.dirty = Some(union(self.dirty, b));
        }
        changed
    }

    fn dab(&mut self, pi: &mut PixelImage, cx: f32, cy: f32) -> Option<Dirty> {
        let (w, h) = pi.img.dimensions();
        let r = (self.opts.size / 2.0).max(0.5);
        let x0 = (cx - r).floor().max(0.0) as u32;
        let y0 = (cy - r).floor().max(0.0) as u32;
        let x1 = ((cx + r).ceil().max(0.0) as u32).min(w);
        let y1 = ((cy + r).ceil().max(0.0) as u32).min(h);
        if x0 >= x1 || y0 >= y1 {
            return None;
        }
        let b = [x0, y0, x1, y1];
        if self.tool == PixelTool::Smudge {
            self.smudge(pi, cx, cy, b);
            return Some(b);
        }
        // Filtered versions of the picture near the dab, for the tools
        // that need its neighbourhood.
        let local = match self.tool {
            PixelTool::Blur | PixelTool::Sharpen | PixelTool::Heal => {
                let pad = (r * 0.5).ceil().max(2.0) as u32;
                Some(LocalBlur::new(
                    &self.base,
                    b,
                    pad,
                    (r * 0.25).max(1.0) as u32,
                ))
            }
            _ => None,
        };
        let flow = self.opts.flow.clamp(0.01, 1.0);
        let opacity = self.opts.opacity.clamp(0.0, 1.0);
        for y in y0..y1 {
            for x in x0..x1 {
                let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
                let wgt = dab_weight(d, self.opts.size, self.opts.hardness) * pi.sel_at(x, y);
                if wgt <= 0.0 {
                    continue;
                }
                let i = (y * w + x) as usize;
                let Some(c) = self.cover.get_mut(i) else {
                    continue;
                };
                *c = (*c + (opacity - *c).max(0.0) * wgt * flow).min(opacity);
                let cov = *c;
                let base = *self.base.get_pixel(x, y);
                let out = self.shade(&base, x, y, cov, local.as_ref());
                pi.img.put_pixel(x, y, out);
            }
        }
        Some(b)
    }

    /// The pixel the tool makes of `base` at (x, y) with coverage `t`.
    fn shade(
        &self,
        base: &Rgba<u8>,
        x: u32,
        y: u32,
        t: f32,
        local: Option<&LocalBlur>,
    ) -> Rgba<u8> {
        match self.tool {
            PixelTool::Brush => over(base, self.opts.color, t),
            PixelTool::Eraser => Rgba([
                base[0],
                base[1],
                base[2],
                (base[3] as f32 * (1.0 - t)).round() as u8,
            ]),
            PixelTool::Clone => match self.source(x, y) {
                Some(src) => mix(base, &src, t),
                None => *base,
            },
            PixelTool::Heal => {
                // The source's detail on the destination's colours: the
                // source minus its own local average plus the local
                // average here.
                let (Some(src), Some(lb)) = (self.source(x, y), local) else {
                    return *base;
                };
                let sx = (x as f32 + self.offset.0).round() as i64;
                let sy = (y as f32 + self.offset.1).round() as i64;
                let here = lb.at(x as i64, y as i64);
                let there = blur_at(&self.base, sx, sy, (self.opts.size * 0.25).max(1.0) as i64);
                let ch = |i: usize| {
                    (src[i] as f32 - there[i] + here[i])
                        .round()
                        .clamp(0.0, 255.0) as u8
                };
                let healed = Rgba([ch(0), ch(1), ch(2), base[3]]);
                mix(base, &healed, t)
            }
            PixelTool::Dodge | PixelTool::Burn => {
                let k = if self.tool == PixelTool::Dodge {
                    1.0 + 0.6 * t
                } else {
                    1.0 - 0.6 * t
                };
                let ch = |c: u8| {
                    if self.tool == PixelTool::Dodge {
                        // Lightens the darks less than the mid tones.
                        let v = c as f32 / 255.0;
                        (255.0 * (v * k).min(1.0 - (1.0 - v) / k))
                            .round()
                            .clamp(0.0, 255.0) as u8
                    } else {
                        (c as f32 * k).round().clamp(0.0, 255.0) as u8
                    }
                };
                Rgba([ch(base[0]), ch(base[1]), ch(base[2]), base[3]])
            }
            PixelTool::Sponge => {
                let l = crate::fx::util::luma(base);
                let s = if self.opts.desaturate {
                    1.0 - t
                } else {
                    1.0 + t
                };
                let ch = |c: u8| (l + (c as f32 - l) * s).round().clamp(0.0, 255.0) as u8;
                Rgba([ch(base[0]), ch(base[1]), ch(base[2]), base[3]])
            }
            PixelTool::Blur => match local {
                Some(lb) => {
                    let v = lb.at(x as i64, y as i64);
                    let blurred = Rgba(v.map(|c| c.round().clamp(0.0, 255.0) as u8));
                    mix(base, &blurred, t)
                }
                None => *base,
            },
            PixelTool::Sharpen => match local {
                Some(lb) => {
                    let v = lb.at(x as i64, y as i64);
                    let ch = |i: usize| {
                        (base[i] as f32 + (base[i] as f32 - v[i]) * 1.5 * t)
                            .round()
                            .clamp(0.0, 255.0) as u8
                    };
                    Rgba([ch(0), ch(1), ch(2), base[3]])
                }
                None => *base,
            },
            PixelTool::RedEye => red_eye_px(base, t),
            _ => *base,
        }
    }

    fn source(&self, x: u32, y: u32) -> Option<Rgba<u8>> {
        let sx = (x as f32 + self.offset.0).round();
        let sy = (y as f32 + self.offset.1).round();
        if sx < 0.0 || sy < 0.0 {
            return None;
        }
        self.base.get_pixel_checked(sx as u32, sy as u32).copied()
    }

    /// Smudge: the colours carried from the last dab mixed into the
    /// picture under the brush, and picked up again from it.
    fn smudge(&mut self, pi: &mut PixelImage, cx: f32, cy: f32, b: Dirty) {
        let side = self.opts.size.ceil().max(1.0) as i64;
        let n = (side * side) as usize;
        let strength = (self.opts.opacity * 0.9).clamp(0.0, 0.95);
        let first = self.carried.len() != n;
        if first {
            self.carried = vec![[0.0; 4]; n];
        }
        let half = side as f32 / 2.0;
        for y in b[1]..b[3] {
            for x in b[0]..b[2] {
                let lx = (x as f32 + 0.5 - cx + half).floor() as i64;
                let ly = (y as f32 + 0.5 - cy + half).floor() as i64;
                if lx < 0 || ly < 0 || lx >= side || ly >= side {
                    continue;
                }
                let k = (ly * side + lx) as usize;
                let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
                let wgt = dab_weight(d, self.opts.size, self.opts.hardness) * pi.sel_at(x, y);
                let cur = *pi.img.get_pixel(x, y);
                let curf = [cur[0] as f32, cur[1] as f32, cur[2] as f32, cur[3] as f32];
                let Some(car) = self.carried.get_mut(k) else {
                    continue;
                };
                if first {
                    *car = curf;
                    continue;
                }
                if wgt > 0.0 {
                    let t = strength * wgt;
                    let out = [0, 1, 2, 3].map(|i| curf[i] + (car[i] - curf[i]) * t);
                    pi.img
                        .put_pixel(x, y, Rgba(out.map(|v| v.round().clamp(0.0, 255.0) as u8)));
                    *car = [0, 1, 2, 3].map(|i| car[i] * strength + out[i] * (1.0 - strength));
                }
            }
        }
    }
}

/// A box blur of a picture over a region and a margin around it.
struct LocalBlur {
    x0: i64,
    y0: i64,
    w: i64,
    h: i64,
    data: Vec<[f32; 4]>,
}

impl LocalBlur {
    fn new(img: &RgbaImage, b: Dirty, pad: u32, r: u32) -> Self {
        let x0 = b[0] as i64;
        let y0 = b[1] as i64;
        let w = (b[2] - b[0]) as i64;
        let h = (b[3] - b[1]) as i64;
        let _ = pad;
        let r = r as i64;
        let mut data = Vec::with_capacity((w * h) as usize);
        for y in 0..h {
            for x in 0..w {
                data.push(blur_at(img, x0 + x, y0 + y, r));
            }
        }
        LocalBlur { x0, y0, w, h, data }
    }

    fn at(&self, x: i64, y: i64) -> [f32; 4] {
        let (lx, ly) = (x - self.x0, y - self.y0);
        if lx < 0 || ly < 0 || lx >= self.w || ly >= self.h {
            return [0.0; 4];
        }
        self.data
            .get((ly * self.w + lx) as usize)
            .copied()
            .unwrap_or([0.0; 4])
    }
}

/// The average colour of the square of radius `r` around (x, y), the
/// picture's edges repeated beyond it.
fn blur_at(img: &RgbaImage, x: i64, y: i64, r: i64) -> [f32; 4] {
    let (w, h) = (img.width() as i64, img.height() as i64);
    if w == 0 || h == 0 {
        return [0.0; 4];
    }
    let mut acc = [0.0f32; 4];
    let mut n: f32 = 0.0;
    // A sparse grid of samples keeps a large brush quick.
    let step = (r / 3).max(1);
    let mut dy = -r;
    while dy <= r {
        let mut dx = -r;
        while dx <= r {
            let q = img.get_pixel(
                (x + dx).clamp(0, w - 1) as u32,
                (y + dy).clamp(0, h - 1) as u32,
            );
            for i in 0..4 {
                acc[i] += q[i] as f32;
            }
            n += 1.0;
            dx += step;
        }
        dy += step;
    }
    acc.map(|v| v / n.max(1.0))
}

// ---------------------------------------------------------------------
// Content-aware fill
// ---------------------------------------------------------------------

/// The selected part filled from the rest of the picture: from the edge
/// inwards, each pixel takes the centre of the patch outside the
/// selection that best matches what is known around it. Candidates come
/// from the neighbours' sources moved by one pixel (so texture runs on)
/// and from a fixed spread of random places.
pub fn content_fill(pi: &mut PixelImage) -> Option<Dirty> {
    let sel = pi.sel.clone()?;
    let (w, h) = pi.img.dimensions();
    let b = pi.sel_bounds()?;
    let mut known: Vec<bool> = sel.pixels().map(|v| v[0] < 128).collect();
    if !known.iter().any(|k| *k) {
        return None;
    }
    let idx = |x: i64, y: i64| (y * w as i64 + x) as usize;
    let inside = |x: i64, y: i64| x >= 0 && y >= 0 && x < w as i64 && y < h as i64;
    // Places to copy from: known pixels with a whole known patch round them.
    const R: i64 = 3;
    let mut pool: Vec<(i64, i64)> = Vec::new();
    let mut seed = 0x9E37_79B9u32;
    let mut rnd = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed
    };
    let full_patch = |known: &Vec<bool>, x: i64, y: i64| {
        (-R..=R).all(|dy| {
            (-R..=R).all(|dx| {
                inside(x + dx, y + dy) && known.get(idx(x + dx, y + dy)).copied().unwrap_or(false)
            })
        })
    };
    for _ in 0..4000 {
        let x = (rnd() % w.max(1)) as i64;
        let y = (rnd() % h.max(1)) as i64;
        if full_patch(&known, x, y) {
            pool.push((x, y));
        }
        if pool.len() >= 600 {
            break;
        }
    }
    if pool.is_empty() {
        // Small pictures: every known pixel.
        for y in 0..h as i64 {
            for x in 0..w as i64 {
                if known.get(idx(x, y)).copied().unwrap_or(false) {
                    pool.push((x, y));
                }
            }
        }
    }
    let mut source: Vec<Option<(i64, i64)>> = vec![None; (w * h) as usize];
    let mut img = pi.img.clone();
    let mut remaining: usize = known.iter().filter(|k| !**k).count();
    let mut guard = 0;
    while remaining > 0 && guard < 10_000 {
        guard += 1;
        // The front: unknown pixels touching known ones.
        let mut front = Vec::new();
        for y in b[1] as i64..b[3] as i64 {
            for x in b[0] as i64..b[2] as i64 {
                if known.get(idx(x, y)).copied().unwrap_or(true) {
                    continue;
                }
                let touches = [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(dx, dy)| {
                    inside(x + dx, y + dy)
                        && known.get(idx(x + dx, y + dy)).copied().unwrap_or(false)
                });
                if touches {
                    front.push((x, y));
                }
            }
        }
        if front.is_empty() {
            break;
        }
        for &(x, y) in &front {
            let mut cands: Vec<(i64, i64)> = Vec::new();
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if let Some(Some((sx, sy))) = inside(x + dx, y + dy)
                        .then(|| source.get(idx(x + dx, y + dy)).copied().flatten())
                    {
                        cands.push((sx - dx, sy - dy));
                    }
                }
            }
            for _ in 0..24 {
                if let Some(p) = pool.get(rnd() as usize % pool.len().max(1)) {
                    cands.push(*p);
                }
            }
            let mut best: Option<((i64, i64), f32)> = None;
            for (sx, sy) in cands {
                if !inside(sx, sy)
                    || !known.get(idx(sx, sy)).copied().unwrap_or(false)
                    || !sel_outside(&sel, sx, sy)
                {
                    continue;
                }
                let mut cost = 0.0;
                let mut n = 0.0;
                for dy in -R..=R {
                    for dx in -R..=R {
                        let (tx, ty) = (x + dx, y + dy);
                        let (ux, uy) = (sx + dx, sy + dy);
                        if !inside(tx, ty)
                            || !inside(ux, uy)
                            || !known.get(idx(tx, ty)).copied().unwrap_or(false)
                        {
                            continue;
                        }
                        let a = img.get_pixel(tx as u32, ty as u32);
                        let c = img.get_pixel(ux as u32, uy as u32);
                        cost += (0..3)
                            .map(|i| (a[i] as f32 - c[i] as f32).powi(2))
                            .sum::<f32>();
                        n += 1.0;
                    }
                }
                if n == 0.0 {
                    continue;
                }
                let cost = cost / n;
                if best.is_none_or(|(_, bc)| cost < bc) {
                    best = Some(((sx, sy), cost));
                }
            }
            if let Some(((sx, sy), _)) = best {
                let q = *img.get_pixel(sx as u32, sy as u32);
                img.put_pixel(x as u32, y as u32, q);
                if let Some(s) = source.get_mut(idx(x, y)) {
                    *s = Some((sx, sy));
                }
            }
        }
        for &(x, y) in &front {
            if let Some(k) = known.get_mut(idx(x, y)) {
                if !*k {
                    *k = true;
                    remaining = remaining.saturating_sub(1);
                }
            }
        }
    }
    // Soft selection edges: the filled picture mixed in by the mask.
    blend_through_selection(pi, &img)
}

fn sel_outside(sel: &GrayImage, x: i64, y: i64) -> bool {
    x >= 0
        && y >= 0
        && sel
            .get_pixel_checked(x as u32, y as u32)
            .is_some_and(|v| v[0] < 128)
}

// ---------------------------------------------------------------------
// Undo steps
// ---------------------------------------------------------------------

/// The picture's undo and redo steps, kept within a memory budget.
#[derive(Default)]
pub struct History {
    undo: Vec<PixelImage>,
    redo: Vec<PixelImage>,
}

/// About this much memory for the steps.
const HISTORY_BYTES: usize = 600 * 1024 * 1024;

impl History {
    /// Remember `state` before it is changed.
    pub fn push(&mut self, state: &PixelImage) {
        self.undo.push(state.clone());
        self.redo.clear();
        let per = (state.img.as_raw().len()
            + state.sel.as_ref().map(|m| m.as_raw().len()).unwrap_or(0))
        .max(1);
        let most = (HISTORY_BYTES / per).clamp(3, 100);
        while self.undo.len() > most {
            self.undo.remove(0);
        }
    }

    pub fn undo(&mut self, now: &mut PixelImage) -> bool {
        match self.undo.pop() {
            Some(prev) => {
                self.redo.push(std::mem::replace(now, prev));
                true
            }
            None => false,
        }
    }

    pub fn redo(&mut self, now: &mut PixelImage) -> bool {
        match self.redo.pop() {
            Some(next) => {
                self.undo.push(std::mem::replace(now, next));
                true
            }
            None => false,
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

/// The outline of a selection for the screen: an RGBA picture of its
/// size, clear except a dashed black and white line where the mask
/// crosses half way.
pub fn selection_outline(m: &GrayImage) -> RgbaImage {
    let (w, h) = m.dimensions();
    let on = |x: i64, y: i64| {
        x >= 0
            && y >= 0
            && x < w as i64
            && y < h as i64
            && m.get_pixel(x as u32, y as u32)[0] >= 128
    };
    RgbaImage::from_fn(w, h, |x, y| {
        let (xi, yi) = (x as i64, y as i64);
        let me = on(xi, yi);
        let edge = me && (!on(xi - 1, yi) || !on(xi + 1, yi) || !on(xi, yi - 1) || !on(xi, yi + 1));
        if edge {
            if ((x + y) / 4) % 2 == 0 {
                Rgba([0, 0, 0, 255])
            } else {
                Rgba([255, 255, 255, 255])
            }
        } else {
            Rgba([0, 0, 0, 0])
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, c: [u8; 4]) -> PixelImage {
        PixelImage::new(RgbaImage::from_pixel(w, h, Rgba(c)))
    }

    #[test]
    fn rectangle_and_ellipse_selections_cover_their_box() {
        let m = rect_mask(20, 20, (2.0, 3.0), (10.0, 12.0)).unwrap();
        assert_eq!(m.get_pixel(5, 5)[0], 255);
        assert_eq!(m.get_pixel(15, 15)[0], 0);
        let e = ellipse_mask(20, 20, (0.0, 0.0), (20.0, 20.0)).unwrap();
        assert_eq!(e.get_pixel(10, 10)[0], 255);
        assert_eq!(e.get_pixel(0, 0)[0], 0);
        let l = lasso_mask(20, 20, &[(0.0, 0.0), (20.0, 0.0), (0.0, 20.0)]).unwrap();
        assert_eq!(l.get_pixel(2, 2)[0], 255);
        assert_eq!(l.get_pixel(18, 18)[0], 0);
        assert!(lasso_mask(20, 20, &[(0.0, 0.0), (1.0, 1.0)]).is_none());
    }

    #[test]
    fn the_magic_wand_takes_touching_pixels_of_like_colour() {
        let mut pi = solid(10, 10, [255, 255, 255, 255]);
        // A black wall down the middle and a white island beyond it.
        for y in 0..10 {
            pi.img.put_pixel(5, y, Rgba([0, 0, 0, 255]));
        }
        let m = similar_mask(&pi.img, 1, 1, 10, true).unwrap();
        assert_eq!(m.get_pixel(4, 4)[0], 255);
        assert_eq!(m.get_pixel(5, 4)[0], 0);
        assert_eq!(m.get_pixel(8, 4)[0], 0, "beyond the wall");
        let all = similar_mask(&pi.img, 1, 1, 10, false).unwrap();
        assert_eq!(all.get_pixel(8, 4)[0], 255);
        assert!(similar_mask(&pi.img, 50, 1, 10, true).is_none());
    }

    #[test]
    fn selections_add_subtract_and_invert() {
        let a = rect_mask(10, 10, (0.0, 0.0), (5.0, 10.0)).unwrap();
        let b = rect_mask(10, 10, (5.0, 0.0), (10.0, 10.0)).unwrap();
        let both = combine(Some(&a), b.clone(), SelectMode::Add).unwrap();
        assert!(both.pixels().all(|v| v[0] == 255));
        let back = combine(Some(&both), b.clone(), SelectMode::Subtract).unwrap();
        assert_eq!(back, a);
        let inv = invert(Some(&a), 10, 10);
        assert_eq!(inv, b);
        assert!(invert(None, 3, 3).pixels().all(|v| v[0] == 0));
    }

    #[test]
    fn feather_softens_and_grow_moves_the_edge() {
        let big = rect_mask(50, 50, (10.0, 10.0), (40.0, 40.0)).unwrap();
        let f = feather(&big, 3);
        let edge = f.get_pixel(10, 25)[0];
        assert!(edge > 30 && edge < 230, "{edge}");
        assert_eq!(f.get_pixel(25, 25)[0], 255);
        let m = rect_mask(30, 30, (10.0, 10.0), (20.0, 20.0)).unwrap();
        let g = grow(&m, 2);
        assert_eq!(g.get_pixel(8, 15)[0], 255);
        let s = grow(&m, -2);
        assert_eq!(s.get_pixel(11, 15)[0], 0);
        assert_eq!(s.get_pixel(15, 15)[0], 255);
    }

    #[test]
    fn a_brush_stroke_paints_inside_the_selection_only() {
        let mut pi = solid(40, 20, [255, 255, 255, 255]);
        pi.sel = rect_mask(40, 20, (0.0, 0.0), (20.0, 20.0));
        let opts = BrushOpts {
            size: 10.0,
            hardness: 1.0,
            opacity: 1.0,
            flow: 1.0,
            color: [255, 0, 0, 255],
            ..BrushOpts::default()
        };
        let mut s = Stroke::new(PixelTool::Brush, opts, &pi, (0.0, 0.0));
        s.to(&mut pi, 5.0, 10.0);
        s.to(&mut pi, 35.0, 10.0);
        assert_eq!(*pi.img.get_pixel(10, 10), Rgba([255, 0, 0, 255]));
        assert_eq!(
            *pi.img.get_pixel(30, 10),
            Rgba([255, 255, 255, 255]),
            "outside the selection"
        );
        assert!(s.dirty.is_some());
    }

    #[test]
    fn a_stroke_never_goes_past_its_opacity() {
        let mut pi = solid(20, 20, [255, 255, 255, 255]);
        let opts = BrushOpts {
            size: 8.0,
            hardness: 1.0,
            opacity: 0.5,
            flow: 1.0,
            color: [0, 0, 0, 255],
            ..BrushOpts::default()
        };
        let mut s = Stroke::new(PixelTool::Brush, opts, &pi, (0.0, 0.0));
        for i in 0..20 {
            s.to(&mut pi, 10.0 + (i % 2) as f32 * 3.0, 10.0);
        }
        let v = pi.img.get_pixel(10, 10)[0];
        assert!((120..=135).contains(&v), "{v}");
    }

    #[test]
    fn the_soft_eraser_fades_to_clear_towards_the_centre() {
        let mut pi = solid(40, 40, [10, 20, 30, 255]);
        let opts = BrushOpts {
            size: 30.0,
            hardness: 0.0,
            opacity: 1.0,
            flow: 1.0,
            ..BrushOpts::default()
        };
        let mut s = Stroke::new(PixelTool::Eraser, opts, &pi, (0.0, 0.0));
        s.to(&mut pi, 20.0, 20.0);
        let centre = pi.img.get_pixel(20, 20)[3];
        let side = pi.img.get_pixel(31, 20)[3];
        let out = pi.img.get_pixel(0, 0)[3];
        assert!(centre < 10, "{centre}");
        assert!(side > centre && side < 255, "{side}");
        assert_eq!(out, 255);
        assert_eq!(pi.img.get_pixel(20, 20)[0], 10, "colour kept");
    }

    #[test]
    fn clone_copies_from_the_source_offset() {
        let mut pi = solid(40, 20, [255, 255, 255, 255]);
        for y in 0..20 {
            for x in 0..10 {
                pi.img.put_pixel(x, y, Rgba([0, 0, 255, 255]));
            }
        }
        let opts = BrushOpts {
            size: 6.0,
            hardness: 1.0,
            opacity: 1.0,
            flow: 1.0,
            ..BrushOpts::default()
        };
        let mut s = Stroke::new(PixelTool::Clone, opts, &pi, (-25.0, 0.0));
        s.to(&mut pi, 30.0, 10.0);
        assert_eq!(*pi.img.get_pixel(30, 10), Rgba([0, 0, 255, 255]));
    }

    #[test]
    fn heal_keeps_the_destination_colour_with_the_source_detail() {
        // A red left half with a dark speck; a green right half.
        let mut pi = solid(60, 30, [0, 160, 0, 255]);
        for y in 0..30 {
            for x in 0..30 {
                pi.img.put_pixel(x, y, Rgba([200, 30, 30, 255]));
            }
        }
        let opts = BrushOpts {
            size: 8.0,
            hardness: 1.0,
            opacity: 1.0,
            flow: 1.0,
            ..BrushOpts::default()
        };
        let mut s = Stroke::new(PixelTool::Heal, opts, &pi, (-30.0, 0.0));
        s.to(&mut pi, 45.0, 15.0);
        let q = pi.img.get_pixel(45, 15);
        assert!(q[1] > 120 && q[0] < 60, "stays green: {q:?}");
    }

    #[test]
    fn dodge_lightens_burn_darkens_and_sponge_changes_colour() {
        for (tool, check) in [
            (
                PixelTool::Dodge,
                Box::new(|v: u8| v > 100) as Box<dyn Fn(u8) -> bool>,
            ),
            (PixelTool::Burn, Box::new(|v: u8| v < 100)),
        ] {
            let mut pi = solid(20, 20, [100, 100, 100, 255]);
            let opts = BrushOpts {
                size: 10.0,
                hardness: 1.0,
                opacity: 1.0,
                flow: 1.0,
                ..BrushOpts::default()
            };
            let mut s = Stroke::new(tool, opts, &pi, (0.0, 0.0));
            s.to(&mut pi, 10.0, 10.0);
            assert!(check(pi.img.get_pixel(10, 10)[0]), "{tool:?}");
        }
        let mut pi = solid(20, 20, [200, 100, 50, 255]);
        let opts = BrushOpts {
            size: 10.0,
            hardness: 1.0,
            opacity: 1.0,
            flow: 1.0,
            desaturate: true,
            ..BrushOpts::default()
        };
        let mut s = Stroke::new(PixelTool::Sponge, opts, &pi, (0.0, 0.0));
        s.to(&mut pi, 10.0, 10.0);
        let q = pi.img.get_pixel(10, 10);
        assert!(q[0] == q[1] && q[1] == q[2], "grey {q:?}");
    }

    #[test]
    fn blur_and_sharpen_brushes_change_edges() {
        let mut pi = solid(30, 30, [0, 0, 0, 255]);
        for y in 0..30 {
            for x in 15..30 {
                pi.img.put_pixel(x, y, Rgba([255, 255, 255, 255]));
            }
        }
        let start = pi.clone();
        let opts = BrushOpts {
            size: 12.0,
            hardness: 1.0,
            opacity: 1.0,
            flow: 1.0,
            ..BrushOpts::default()
        };
        let mut s = Stroke::new(PixelTool::Blur, opts, &pi, (0.0, 0.0));
        s.to(&mut pi, 15.0, 15.0);
        let v = pi.img.get_pixel(14, 15)[0];
        assert!(v > 0 && v < 255, "{v}");
        let mut pj = start;
        let mut s = Stroke::new(PixelTool::Sharpen, opts, &pj, (0.0, 0.0));
        s.to(&mut pj, 15.0, 15.0);
        assert_eq!(pj.img.get_pixel(14, 15)[0], 0);
    }

    #[test]
    fn smudge_drags_colour_along() {
        let mut pi = solid(40, 20, [255, 255, 255, 255]);
        for y in 0..20 {
            for x in 0..12 {
                pi.img.put_pixel(x, y, Rgba([0, 0, 0, 255]));
            }
        }
        let opts = BrushOpts {
            size: 10.0,
            hardness: 1.0,
            opacity: 1.0,
            flow: 1.0,
            ..BrushOpts::default()
        };
        let mut s = Stroke::new(PixelTool::Smudge, opts, &pi, (0.0, 0.0));
        s.to(&mut pi, 6.0, 10.0);
        s.to(&mut pi, 20.0, 10.0);
        assert!(
            pi.img.get_pixel(17, 10)[0] < 250,
            "{:?}",
            pi.img.get_pixel(17, 10)
        );
    }

    #[test]
    fn red_eye_takes_the_red_out() {
        let mut pi = solid(10, 10, [220, 40, 40, 255]);
        let opts = BrushOpts {
            size: 20.0,
            hardness: 1.0,
            opacity: 1.0,
            flow: 1.0,
            ..BrushOpts::default()
        };
        let mut s = Stroke::new(PixelTool::RedEye, opts, &pi, (0.0, 0.0));
        s.to(&mut pi, 5.0, 5.0);
        assert_eq!(pi.img.get_pixel(5, 5)[0], 40);
        let mut pj = solid(4, 4, [220, 40, 40, 255]);
        red_eye_selection(&mut pj);
        assert_eq!(pj.img.get_pixel(1, 1)[0], 40);
    }

    #[test]
    fn fill_bucket_and_gradient() {
        let mut pi = solid(10, 10, [255, 255, 255, 255]);
        for y in 0..10 {
            pi.img.put_pixel(5, y, Rgba([0, 0, 0, 255]));
        }
        let o = BrushOpts {
            color: [0, 255, 0, 255],
            ..BrushOpts::default()
        };
        bucket_fill(&mut pi, 1, 1, &o).unwrap();
        assert_eq!(*pi.img.get_pixel(3, 3), Rgba([0, 255, 0, 255]));
        assert_eq!(*pi.img.get_pixel(8, 3), Rgba([255, 255, 255, 255]));
        let mut g = solid(20, 4, [255, 255, 255, 255]);
        let o = BrushOpts {
            color: [0, 0, 0, 255],
            gradient_to_clear: false,
            ..BrushOpts::default()
        };
        gradient(&mut g, (0.0, 2.0), (20.0, 2.0), &o).unwrap();
        assert!(g.img.get_pixel(0, 1)[0] < 20);
        assert!(g.img.get_pixel(19, 1)[0] > 235);
    }

    #[test]
    fn delete_fill_crop_and_flip() {
        let mut pi = solid(10, 10, [9, 9, 9, 255]);
        pi.sel = rect_mask(10, 10, (2.0, 2.0), (6.0, 8.0));
        clear_selection(&mut pi).unwrap();
        assert_eq!(pi.img.get_pixel(3, 3)[3], 0);
        assert_eq!(pi.img.get_pixel(8, 8)[3], 255);
        fill_selection(&mut pi, [1, 2, 3, 255], 1.0).unwrap();
        assert_eq!(*pi.img.get_pixel(3, 3), Rgba([1, 2, 3, 255]));
        let b = crop_to_selection(&mut pi).unwrap();
        assert_eq!(b, [2, 2, 6, 8]);
        assert_eq!(pi.img.dimensions(), (4, 6));
        assert!(pi.sel.is_none());
        assert_eq!(pi.origin, (2, 2));
        pi.img.put_pixel(0, 0, Rgba([255, 0, 0, 255]));
        flip(&mut pi, true);
        assert_eq!(pi.img.get_pixel(3, 0)[0], 255);
    }

    #[test]
    fn an_effect_runs_inside_the_selection() {
        let mut pi = solid(10, 10, [200, 100, 50, 255]);
        pi.sel = rect_mask(10, 10, (0.0, 0.0), (5.0, 10.0));
        let e = crate::fx::new_effect("invert_colors");
        apply_effect(&mut pi, &e).unwrap();
        assert_eq!(*pi.img.get_pixel(1, 1), Rgba([55, 155, 205, 255]));
        assert_eq!(*pi.img.get_pixel(8, 1), Rgba([200, 100, 50, 255]));
    }

    #[test]
    fn content_fill_continues_the_surroundings() {
        // Stripes: rows alternate red and blue; a hole in the middle.
        let mut pi = PixelImage::new(RgbaImage::from_fn(40, 40, |_, y| {
            if (y / 2) % 2 == 0 {
                Rgba([255, 0, 0, 255])
            } else {
                Rgba([0, 0, 255, 255])
            }
        }));
        let truth = pi.img.clone();
        for y in 16..24 {
            for x in 16..24 {
                pi.img.put_pixel(x, y, Rgba([0, 255, 0, 255]));
            }
        }
        pi.sel = rect_mask(40, 40, (16.0, 16.0), (24.0, 24.0));
        content_fill(&mut pi).unwrap();
        let mut right = 0;
        for y in 16..24 {
            for x in 16..24 {
                let q = pi.img.get_pixel(x, y);
                assert!(q[1] < 50, "no green left at {x},{y}: {q:?}");
                if q == truth.get_pixel(x, y) {
                    right += 1;
                }
            }
        }
        assert!(right >= 40, "stripes carried on: {right} of 64");
        let mut none = solid(4, 4, [0, 0, 0, 255]);
        assert!(content_fill(&mut none).is_none(), "needs a selection");
    }

    #[test]
    fn history_undoes_and_redoes() {
        let mut h = History::default();
        let mut pi = solid(4, 4, [0, 0, 0, 255]);
        h.push(&pi);
        pi.img.put_pixel(0, 0, Rgba([9, 9, 9, 255]));
        assert!(h.undo(&mut pi));
        assert_eq!(pi.img.get_pixel(0, 0)[0], 0);
        assert!(h.redo(&mut pi));
        assert_eq!(pi.img.get_pixel(0, 0)[0], 9);
        assert!(!h.redo(&mut pi));
    }

    #[test]
    fn the_selection_outline_is_on_the_edge() {
        let m = rect_mask(10, 10, (2.0, 2.0), (8.0, 8.0)).unwrap();
        let o = selection_outline(&m);
        assert_eq!(o.get_pixel(2, 5)[3], 255);
        assert_eq!(o.get_pixel(5, 5)[3], 0);
        assert_eq!(o.get_pixel(0, 0)[3], 0);
    }
}
