//! Editing a picture's pixels in place: the pixel work behind Bitmaps >
//! Edit Inside (a double click on a picture). Everything here works on an
//! RGBA image and an optional selection mask, with no interface: the
//! selection tools (rectangle, ellipse, freehand, magic wand), the
//! brushes (paint, eraser, clone, heal, dodge, burn, sponge, blur,
//! sharpen, smudge, red eye), the fill, the gradient, effects run inside
//! the selection, content-aware fill, pixel layers (each with its
//! opacity and merge mode) and the undo steps. See
//! `docs/behavior/edit-inside-picture.md`.

use image::{GrayImage, Luma, Rgba, RgbaImage};
pub use tracedraw_core::live::MergeMode;

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
    /// Drags the pixels of the layer in use.
    Move,
    /// Paints light (see `pixel_light.rs`).
    Light,
    /// Bends the layer in use with a grid of points.
    Warp,
    /// Paints the picture back as it was when the editing began.
    Restore,
    /// Colours the pixels with the brush colour, keeping their light.
    Tint,
    /// Swaps the colour under the first dab for the brush colour.
    ReplaceColor,
    /// Rectangles, ellipses, polygons and lines.
    Shape,
    /// Moves pixels with the brush: push, twirl, pinch, bloat.
    Liquify,
    /// Paints over what is to go; on release the place, a little wider,
    /// is filled from what is around it.
    Remove,
    /// Zooms the view of the picture: a click in, Alt+click out, a drag
    /// to the box dragged (the image editing mode).
    Zoom,
    /// Drags the view of the picture (the image editing mode).
    Pan,
    /// A selection outline of clicked corners and dragged freehand parts,
    /// closed with a double click, Enter or a click on its first point.
    FreehandMask,
    /// A freehand selection whose outline clings to the strongest edge
    /// near the pointer.
    EdgeMask,
    /// Paints the selection with the brush (Alt or Ctrl takes away).
    MaskBrush,
    /// A selection band between two parallel lines, dragged across.
    BandMask,
    /// Liquify's push, its own tool.
    Smear,
    /// Liquify's twirl (Alt turns the other way).
    Twirl,
    /// Liquify's pinch: pixels drawn to the brush's centre.
    Attract,
    /// Liquify's bloat: pixels pushed from the brush's centre.
    Repel,
    /// Sprays scattered dots, stars, bubbles, leaves or confetti.
    Sprayer,
    /// A drag fades the layer from its opacity at the press to its
    /// opacity at the release.
    LayerFade,
    /// A click makes the colour clicked (and those near it) see-through.
    ColorToClear,
    /// Paints see-through into the layer, to the level chosen.
    TransparencyBrush,
}

impl PixelTool {
    pub const ALL: [PixelTool; 41] = [
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
        PixelTool::Move,
        PixelTool::Light,
        PixelTool::Warp,
        PixelTool::Restore,
        PixelTool::Tint,
        PixelTool::ReplaceColor,
        PixelTool::Shape,
        PixelTool::Liquify,
        PixelTool::Remove,
        PixelTool::Zoom,
        PixelTool::Pan,
        PixelTool::FreehandMask,
        PixelTool::EdgeMask,
        PixelTool::MaskBrush,
        PixelTool::BandMask,
        PixelTool::Smear,
        PixelTool::Twirl,
        PixelTool::Attract,
        PixelTool::Repel,
        PixelTool::Sprayer,
        PixelTool::LayerFade,
        PixelTool::ColorToClear,
        PixelTool::TransparencyBrush,
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
            PixelTool::Move => "pixel.tool.move",
            PixelTool::Light => "pixel.tool.light",
            PixelTool::Warp => "pixel.tool.warp",
            PixelTool::Restore => "pixel.tool.restore",
            PixelTool::Tint => "pixel.tool.tint",
            PixelTool::ReplaceColor => "pixel.tool.replace_color",
            PixelTool::Shape => "pixel.tool.shape",
            PixelTool::Liquify => "pixel.tool.liquify",
            PixelTool::Remove => "pixel.tool.remove",
            PixelTool::Zoom => "pixel.tool.zoom",
            PixelTool::Pan => "pixel.tool.pan",
            PixelTool::FreehandMask => "pixel.tool.freehand_mask",
            PixelTool::EdgeMask => "pixel.tool.edge_mask",
            PixelTool::MaskBrush => "pixel.tool.mask_brush",
            PixelTool::BandMask => "pixel.tool.band_mask",
            PixelTool::Smear => "pixel.tool.smear",
            PixelTool::Twirl => "pixel.tool.twirl",
            PixelTool::Attract => "pixel.tool.attract",
            PixelTool::Repel => "pixel.tool.repel",
            PixelTool::Sprayer => "pixel.tool.sprayer",
            PixelTool::LayerFade => "pixel.tool.layer_fade",
            PixelTool::ColorToClear => "pixel.tool.color_to_clear",
            PixelTool::TransparencyBrush => "pixel.tool.transparency_brush",
        }
    }

    /// What a Liquify tool does with the pixels (`Liquify` itself takes
    /// the mode chosen; Alt turns a twirl the other way).
    pub fn liquify_mode(
        self,
        chosen: crate::pixel_liquify::LiquifyMode,
        alt: bool,
    ) -> Option<crate::pixel_liquify::LiquifyMode> {
        use crate::pixel_liquify::LiquifyMode as M;
        Some(match self {
            PixelTool::Liquify => chosen,
            PixelTool::Smear => M::Push,
            PixelTool::Twirl if alt => M::TwirlLeft,
            PixelTool::Twirl => M::TwirlRight,
            PixelTool::Attract => M::Pinch,
            PixelTool::Repel => M::Bloat,
            _ => return None,
        })
    }

    /// A tool that moves pixels with the brush (Liquify and its kin).
    pub fn is_liquify(self) -> bool {
        self.liquify_mode(crate::pixel_liquify::LiquifyMode::Push, false)
            .is_some()
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
                | PixelTool::Restore
                | PixelTool::Tint
                | PixelTool::ReplaceColor
                | PixelTool::TransparencyBrush
        )
    }

    pub fn is_selection(self) -> bool {
        matches!(
            self,
            PixelTool::RectSelect
                | PixelTool::EllipseSelect
                | PixelTool::Lasso
                | PixelTool::MagicWand
                | PixelTool::FreehandMask
                | PixelTool::EdgeMask
                | PixelTool::MaskBrush
                | PixelTool::BandMask
        )
    }
}

/// How a new selection combines with the one there is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectMode {
    Replace,
    Add,
    Subtract,
    /// Kept where only one of the two is.
    Xor,
}

impl SelectMode {
    pub const ALL: [SelectMode; 4] = [
        SelectMode::Replace,
        SelectMode::Add,
        SelectMode::Subtract,
        SelectMode::Xor,
    ];

    pub fn key(self) -> &'static str {
        match self {
            SelectMode::Replace => "pixel.sel_mode.normal",
            SelectMode::Add => "pixel.sel_mode.add",
            SelectMode::Subtract => "pixel.sel_mode.subtract",
            SelectMode::Xor => "pixel.sel_mode.xor",
        }
    }
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
    /// How the gradient spreads.
    pub gradient: crate::pixel_paint::GradientKind,
    /// Brush tip: the distance between dabs as a share of the size, how
    /// far dabs stray from the stroke (share of the size), the tip's
    /// angle in degrees and its roundness (1 round, less flatter).
    pub spacing: f32,
    pub scatter: f32,
    pub angle: f32,
    pub roundness: f32,
    /// The Shape tool: which shape, filled, outlined, the outline's width
    /// (and the line's), its colour and the polygon's sides.
    pub shape: crate::pixel_paint::PixelShape,
    pub shape_fill: bool,
    pub shape_outline: bool,
    pub line_width: f32,
    pub outline_color: [u8; 4],
    pub sides: u32,
    /// The brush and the stamp follow the surface of the photo: bent by
    /// up to `surface_depth` pixels along its relief and lit by its light
    /// (`surface_light`, 0 to 1).
    pub surface: bool,
    pub surface_depth: f32,
    pub surface_light: f32,
    /// Remove: how many pixels the painted place is widened by, to take
    /// in the edge and the shadow.
    pub remove_grow: f32,
    /// What the Liquify brush does.
    pub liquify: crate::pixel_liquify::LiquifyMode,
    /// How a new selection joins the one there is (Shift adds, Ctrl
    /// takes away, whatever this is).
    pub sel_mode: SelectMode,
    /// The rectangle and ellipse selections: a click makes one of this
    /// width and height (pixels) when on.
    pub sel_fixed: bool,
    pub sel_size: (f32, f32),
    /// The soft edge of the band selection, pixels.
    pub band_soft: f32,
    /// How far the edge mask looks for an edge, pixels.
    pub magnet_reach: f32,
    /// The transparency brush: how see-through it paints, 0 to 1.
    pub transparency: f32,
    /// Layer fade: the layer's opacity where the drag starts and
    /// where it ends, 0 to 1.
    pub fade_from: f32,
    pub fade_to: f32,
    /// The sprayer.
    pub spray: crate::pixel_spray::SprayOpts,
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
            gradient: crate::pixel_paint::GradientKind::Linear,
            spacing: 0.25,
            scatter: 0.0,
            angle: 0.0,
            roundness: 1.0,
            shape: crate::pixel_paint::PixelShape::Rect,
            shape_fill: true,
            shape_outline: false,
            line_width: 4.0,
            outline_color: [0, 0, 0, 255],
            sides: 5,
            surface: false,
            surface_depth: 6.0,
            surface_light: 0.8,
            remove_grow: 4.0,
            liquify: crate::pixel_liquify::LiquifyMode::Push,
            sel_mode: SelectMode::Replace,
            sel_fixed: false,
            sel_size: (100.0, 100.0),
            band_soft: 20.0,
            magnet_reach: 6.0,
            transparency: 0.5,
            fade_from: 1.0,
            fade_to: 0.0,
            spray: crate::pixel_spray::SprayOpts::default(),
        }
    }
}

/// One layer of pixels of the picture being edited. All layers have the
/// picture's size.
#[derive(Debug, Clone, PartialEq)]
pub struct PixelLayer {
    pub name: String,
    /// The layer's pixels; empty (0 by 0) for the layer in use, whose
    /// pixels are `PixelImage::img`.
    pub img: RgbaImage,
    /// 0 to 1.
    pub opacity: f32,
    pub merge: MergeMode,
    pub visible: bool,
    /// No tool changes a locked layer's pixels.
    pub locked: bool,
    /// How much of each pixel shows (255 all, 0 none): hides pixels
    /// without wiping them. The picture's size.
    pub mask: Option<GrayImage>,
    /// Shows only where the layer beneath it (the nearest one not
    /// clipped) has pixels.
    pub clip: bool,
    /// An adjustment layer: no pixels of its own, the effect run on
    /// everything beneath it (through its mask and opacity), changeable
    /// at any time.
    pub adjust: Option<tracedraw_core::BitmapEffect>,
}

impl PixelLayer {
    pub fn new(name: impl Into<String>) -> Self {
        PixelLayer {
            name: name.into(),
            img: RgbaImage::new(0, 0),
            opacity: 1.0,
            merge: MergeMode::Normal,
            visible: true,
            locked: false,
            mask: None,
            clip: false,
            adjust: None,
        }
    }
}

/// The picture being edited and its selection (255 inside, 0 outside,
/// soft in between). No selection means the whole picture.
#[derive(Debug, Clone, PartialEq)]
pub struct PixelImage {
    /// The pixels of the layer in use: every tool works on these.
    pub img: RgbaImage,
    pub sel: Option<GrayImage>,
    /// Where the picture's top left pixel lies in the picture as it was
    /// first opened (it moves when the picture is cropped).
    pub origin: (i64, i64),
    /// The layers, bottom first; at least one.
    pub layers: Vec<PixelLayer>,
    /// The layer in use.
    pub active: usize,
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
            layers: vec![PixelLayer::new("")],
            active: 0,
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
        // With nothing selected, "everything" stands for the old one.
        (SelectMode::Xor, None) => {
            let mut m = new;
            m.pixels_mut().for_each(|v| v[0] = 255 - v[0]);
            Some(m)
        }
        (SelectMode::Xor, Some(o)) => {
            let mut m = o.clone();
            for (a, b) in m.pixels_mut().zip(new.pixels()) {
                a[0] = (a[0] as i16 - b[0] as i16).unsigned_abs() as u8;
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

/// A gradient from `a` to `b` (picture pixels) in the selection, spread
/// as `o.gradient` says: the colour at `a`, fading to clear (or white) at
/// `b`.
pub fn gradient(pi: &mut PixelImage, a: (f32, f32), b: (f32, f32), o: &BrushOpts) -> Option<Dirty> {
    let bounds = pi.sel_bounds()?;
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
            let t =
                crate::pixel_paint::gradient_t(o.gradient, a, b, x as f32 + 0.5, y as f32 + 0.5);
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

/// The picture flipped left to right (or top to bottom): every layer and
/// the selection.
pub fn flip(pi: &mut PixelImage, horizontal: bool) {
    let f = |m: &mut RgbaImage| {
        if horizontal {
            image::imageops::flip_horizontal_in_place(m);
        } else {
            image::imageops::flip_vertical_in_place(m);
        }
    };
    f(&mut pi.img);
    for l in &mut pi.layers {
        f(&mut l.img);
        if let Some(m) = &mut l.mask {
            if horizontal {
                image::imageops::flip_horizontal_in_place(m);
            } else {
                image::imageops::flip_vertical_in_place(m);
            }
        }
    }
    if let Some(m) = &mut pi.sel {
        if horizontal {
            image::imageops::flip_horizontal_in_place(m);
        } else {
            image::imageops::flip_vertical_in_place(m);
        }
    }
}

/// The picture cut to the selection's box, every layer; the pixels
/// outside the selection inside the box become clear. Returns the box (in
/// the picture before the cut).
pub fn crop_to_selection(pi: &mut PixelImage) -> Option<Dirty> {
    let b = pi.sel_bounds()?;
    let (w, h) = (b[2] - b[0], b[3] - b[1]);
    if w == 0 || h == 0 {
        return None;
    }
    let sel = pi.sel.take();
    let cut = |img: &RgbaImage| {
        let mut out = image::imageops::crop_imm(img, b[0], b[1], w, h).to_image();
        if let Some(m) = &sel {
            for (x, y, q) in out.enumerate_pixels_mut() {
                let s = m
                    .get_pixel_checked(x + b[0], y + b[1])
                    .map(|v| v[0] as f32 / 255.0)
                    .unwrap_or(0.0);
                q[3] = (q[3] as f32 * s).round() as u8;
            }
        }
        out
    };
    pi.img = cut(&pi.img);
    for l in &mut pi.layers {
        if l.img.width() > 0 {
            l.img = cut(&l.img);
        }
        if let Some(m) = &l.mask {
            l.mask = Some(image::imageops::crop_imm(m, b[0], b[1], w, h).to_image());
        }
    }
    pi.origin = (pi.origin.0 + b[0] as i64, pi.origin.1 + b[1] as i64);
    Some(b)
}

// ---------------------------------------------------------------------
// Layers
// ---------------------------------------------------------------------

impl PixelImage {
    /// More than one layer, or one that does not show its pixels as they
    /// are: what is seen must be put together from the layers.
    pub fn is_layered(&self) -> bool {
        self.layers.len() > 1
            || self.layers.first().is_some_and(|l| {
                !l.visible || l.opacity < 1.0 || l.merge != MergeMode::Normal || l.mask.is_some()
            })
    }

    /// The layer in use is locked.
    pub fn active_locked(&self) -> bool {
        self.layers
            .get(self.active)
            .is_some_and(|l| l.locked || l.adjust.is_some())
    }

    /// The layer in use is locked by its lock box (its mask may still be
    /// painted when it is an adjustment layer).
    pub fn active_lock_box(&self) -> bool {
        self.layers.get(self.active).is_some_and(|l| l.locked)
    }

    /// An adjustment layer running `effect` on what is beneath, put above
    /// the layer in use and used.
    pub fn add_adjustment_layer(
        &mut self,
        name: impl Into<String>,
        effect: tracedraw_core::BitmapEffect,
    ) {
        let (w, h) = self.img.dimensions();
        self.insert_layer(name, RgbaImage::new(w, h));
        if let Some(l) = self.layers.get_mut(self.active) {
            l.adjust = Some(effect);
        }
    }

    /// Make layer `i` the one in use.
    pub fn set_active(&mut self, i: usize) {
        if i >= self.layers.len() || i == self.active {
            return;
        }
        if let Some(cur) = self.layers.get_mut(self.active) {
            std::mem::swap(&mut cur.img, &mut self.img);
        }
        if let Some(next) = self.layers.get_mut(i) {
            std::mem::swap(&mut next.img, &mut self.img);
        }
        self.active = i;
    }

    /// The pixels of layer `i`.
    pub fn layer_pixels(&self, i: usize) -> Option<&RgbaImage> {
        if i == self.active {
            Some(&self.img)
        } else {
            self.layers.get(i).map(|l| &l.img)
        }
    }

    /// A layer of `pixels` (the picture's size) put above the one in use,
    /// and used.
    pub fn insert_layer(&mut self, name: impl Into<String>, pixels: RgbaImage) {
        let mut l = PixelLayer::new(name);
        l.img = pixels;
        let at = (self.active + 1).min(self.layers.len());
        self.layers.insert(at, l);
        self.set_active(at);
    }

    /// A new clear layer above the one in use.
    pub fn add_layer(&mut self, name: impl Into<String>) {
        let (w, h) = self.img.dimensions();
        self.insert_layer(name, RgbaImage::new(w, h));
    }

    /// A copy of the layer in use above it, with its settings.
    pub fn duplicate_layer(&mut self, name: impl Into<String>) {
        let from = self.layers.get(self.active).cloned();
        self.insert_layer(name, self.img.clone());
        if let (Some(f), Some(n)) = (from, self.layers.get_mut(self.active)) {
            n.opacity = f.opacity;
            n.merge = f.merge;
            n.visible = f.visible;
            n.mask = f.mask;
            n.clip = f.clip;
        }
    }

    /// The layer in use taken away (never the last one); the one below
    /// it is used next.
    pub fn delete_layer(&mut self) -> bool {
        if self.layers.len() < 2 {
            return false;
        }
        let gone = self.active;
        let next = if gone > 0 { gone - 1 } else { 1 };
        self.set_active(next);
        self.layers.remove(gone);
        if next > gone {
            self.active = next - 1;
        }
        true
    }

    /// The layer in use moved one place up (towards the top) or down.
    pub fn move_layer(&mut self, up: bool) -> bool {
        let i = self.active;
        let j = if up {
            i + 1
        } else {
            match i.checked_sub(1) {
                Some(j) => j,
                None => return false,
            }
        };
        if j >= self.layers.len() {
            return false;
        }
        self.layers.swap(i, j);
        self.active = j;
        true
    }

    /// The layer in use merged into the one below it, with its opacity
    /// and merge mode; the one below keeps its own settings.
    pub fn merge_down(&mut self) -> bool {
        let i = self.active;
        if i == 0 || i >= self.layers.len() {
            return false;
        }
        let top = self.layers.get(i).cloned();
        let Some(top) = top else {
            return false;
        };
        let mut top_pixels = self.img.clone();
        if let Some(m) = &top.mask {
            apply_mask_to(&mut top_pixels, m);
        }
        if top.clip {
            if let Some(below) = self.layers.get(i - 1) {
                for (q, b) in top_pixels.pixels_mut().zip(below.img.pixels()) {
                    q[3] = ((q[3] as u16 * b[3] as u16 + 127) / 255) as u8;
                }
            }
        }
        self.set_active(i - 1);
        // An adjustment layer: its effect run on the layer beneath.
        if let Some(e) = &top.adjust {
            top_pixels = crate::fx::apply_effect(&self.img, e);
            if let Some(m) = &top.mask {
                apply_mask_to(&mut top_pixels, m);
            }
        }
        if top.visible {
            let (w, h) = self.img.dimensions();
            let mut base = rgba_to_pixmap(&self.img, 1.0);
            let layer = rgba_to_pixmap(&top_pixels, top.opacity);
            if let (Some(b), Some(layer)) = (base.as_mut(), layer) {
                tracedraw_render::composite_layer(b, &layer, (0, 0), top.merge);
            }
            if let Some(b) = base {
                if let Some(out) = pixmap_to_rgba(&b, w, h) {
                    self.img = out;
                }
            }
        }
        self.layers.remove(i);
        true
    }

    /// All layers put together into one, as they are seen.
    pub fn flatten(&mut self) -> bool {
        if !self.is_layered() {
            return false;
        }
        let all = self.composite();
        self.layers = vec![PixelLayer::new(
            self.layers
                .first()
                .map(|l| l.name.clone())
                .unwrap_or_default(),
        )];
        self.active = 0;
        self.img = all;
        true
    }

    /// The selected pixels of the layer in use (soft edges kept) on a new
    /// layer above it; `cut` clears them from the layer in use.
    pub fn selection_to_layer(&mut self, name: impl Into<String>, cut: bool) -> bool {
        let (w, h) = self.img.dimensions();
        if w == 0 || h == 0 {
            return false;
        }
        let mut out = RgbaImage::new(w, h);
        for (x, y, q) in out.enumerate_pixels_mut() {
            let s = self.sel_at(x, y);
            if s > 0.0 {
                let p = *self.img.get_pixel(x, y);
                *q = Rgba([p[0], p[1], p[2], (p[3] as f32 * s).round() as u8]);
            }
        }
        if cut {
            if self.active_locked() {
                return false;
            }
            clear_selection(self);
        }
        self.insert_layer(name, out);
        true
    }

    /// The pixels of the layer in use moved by (dx, dy); what leaves the
    /// picture is lost, what comes in is clear.
    pub fn offset_layer(&mut self, dx: i64, dy: i64) {
        self.img = offset_image(&self.img, dx, dy);
    }

    /// Everything seen: the layers put together, bottom first.
    pub fn composite(&self) -> RgbaImage {
        let (w, h) = self.img.dimensions();
        self.composite_region([0, 0, w, h], None)
            .unwrap_or_else(|| RgbaImage::new(w, h))
    }

    /// The pixels seen in box `b`, the layer in use shown as `active` when
    /// given (an effect's preview). The result has the box's size.
    pub fn composite_region(&self, b: Dirty, active: Option<&RgbaImage>) -> Option<RgbaImage> {
        let (w, h) = self.img.dimensions();
        let (x0, y0) = (b[0].min(w), b[1].min(h));
        let (x1, y1) = (b[2].min(w), b[3].min(h));
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        let (bw, bh) = (x1 - x0, y1 - y0);
        let mut target = tiny_skia::Pixmap::new(bw, bh)?;
        // The coverage of the last layer not clipped, for the clipped
        // ones above it.
        let n = (bw * bh) as usize;
        let mut base: Option<Vec<u8>> = None;
        for (i, l) in self.layers.iter().enumerate() {
            let clipped = l.clip && base.is_some();
            let px = if i == self.active {
                active.unwrap_or(&self.img)
            } else {
                &l.img
            };
            if !l.visible || l.opacity <= 0.0 || px.dimensions() != (w, h) {
                if !clipped && l.adjust.is_none() {
                    base = Some(vec![0; n]);
                }
                continue;
            }
            // An adjustment layer: the effect run on what is seen so far.
            let adjusted = l.adjust.as_ref().and_then(|e| {
                Some(crate::fx::apply_effect(
                    &pixmap_to_rgba(&target, bw, bh)?,
                    e,
                ))
            });
            let mut part = match adjusted {
                Some(a) => a,
                None => image::imageops::crop_imm(px, x0, y0, bw, bh).to_image(),
            };
            if let Some(m) = &l.mask {
                let mp = image::imageops::crop_imm(m, x0, y0, bw, bh).to_image();
                apply_mask_to(&mut part, &mp);
            }
            if clipped {
                if let Some(b) = &base {
                    for (q, a) in part.pixels_mut().zip(b.iter()) {
                        q[3] = ((q[3] as u16 * *a as u16 + 127) / 255) as u8;
                    }
                }
            } else if l.adjust.is_none() {
                base = Some(part.pixels().map(|q| q[3]).collect());
            }
            if let Some(layer) = rgba_to_pixmap(&part, l.opacity) {
                tracedraw_render::composite_layer(&mut target, &layer, (0, 0), l.merge);
            }
        }
        pixmap_to_rgba(&target, bw, bh)
    }

    /// The colour seen at (x, y).
    pub fn seen_at(&self, x: u32, y: u32) -> Option<Rgba<u8>> {
        if !self.is_layered() {
            return self.img.get_pixel_checked(x, y).copied();
        }
        self.composite_region([x, y, x + 1, y + 1], None)
            .map(|m| *m.get_pixel(0, 0))
    }

    /// Memory the picture takes, bytes (every layer and the selection).
    pub fn bytes(&self) -> usize {
        self.img.as_raw().len()
            + self
                .layers
                .iter()
                .map(|l| {
                    l.img.as_raw().len() + l.mask.as_ref().map(|m| m.as_raw().len()).unwrap_or(0)
                })
                .sum::<usize>()
            + self.sel.as_ref().map(|m| m.as_raw().len()).unwrap_or(0)
    }
}

/// A picture pasted as a new layer above the one in use, its top left at
/// the selection's (or the picture's) top left; what passes the picture's
/// edge is cut off.
pub fn paste_as_layer(pi: &mut PixelImage, name: String, img: &RgbaImage) -> Option<Dirty> {
    let (w, h) = pi.img.dimensions();
    if w == 0 || h == 0 || img.width() == 0 || img.height() == 0 {
        return None;
    }
    let at = pi.sel_bounds().map(|b| (b[0], b[1])).unwrap_or((0, 0));
    let mut layer = RgbaImage::new(w, h);
    image::imageops::replace(&mut layer, img, at.0 as i64, at.1 as i64);
    pi.insert_layer(name, layer);
    Some([0, 0, w, h])
}

/// The alpha of `img` multiplied by `mask` (same size).
pub fn apply_mask_to(img: &mut RgbaImage, mask: &GrayImage) {
    for (q, m) in img.pixels_mut().zip(mask.pixels()) {
        q[3] = ((q[3] as u16 * m[0] as u16 + 127) / 255) as u8;
    }
}

impl PixelImage {
    /// A mask on the layer in use: the selection's shape (all shown with
    /// no selection).
    pub fn add_layer_mask(&mut self) -> bool {
        let (w, h) = self.img.dimensions();
        let Some(l) = self.layers.get_mut(self.active) else {
            return false;
        };
        if l.mask.is_some() {
            return false;
        }
        l.mask = Some(match &self.sel {
            Some(m) => m.clone(),
            None => GrayImage::from_pixel(w, h, Luma([255])),
        });
        true
    }

    /// The mask of the layer in use baked into its pixels' alpha.
    pub fn apply_layer_mask(&mut self) -> bool {
        let Some(m) = self.layers.get_mut(self.active).and_then(|l| l.mask.take()) else {
            return false;
        };
        apply_mask_to(&mut self.img, &m);
        true
    }

    /// The mask of the layer in use taken away (its pixels all show).
    pub fn delete_layer_mask(&mut self) -> bool {
        self.layers
            .get_mut(self.active)
            .and_then(|l| l.mask.take())
            .is_some()
    }

    /// The mask of the layer in use turned inside out.
    pub fn invert_layer_mask(&mut self) -> bool {
        match self
            .layers
            .get_mut(self.active)
            .and_then(|l| l.mask.as_mut())
        {
            Some(m) => {
                image::imageops::invert(m);
                true
            }
            None => false,
        }
    }
}

// ---------------------------------------------------------------------
// More selections
// ---------------------------------------------------------------------

/// The tones a selection by lightness takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tones {
    Shadows,
    Midtones,
    Highlights,
}

/// The pixels by lightness: full in the middle of the range, fading out
/// over its edges so the selection blends in.
pub fn tone_mask(img: &RgbaImage, tones: Tones) -> GrayImage {
    let (w, h) = img.dimensions();
    GrayImage::from_fn(w, h, |x, y| {
        let l = crate::fx::util::luma(img.get_pixel(x, y)) / 255.0;
        let v = match tones {
            Tones::Shadows => 1.0 - smooth01((l - 0.25) / 0.25),
            Tones::Highlights => smooth01((l - 0.5) / 0.25),
            Tones::Midtones => 1.0 - smooth01(((l - 0.5).abs() - 0.15) / 0.2),
        };
        Luma([(v.clamp(0.0, 1.0) * 255.0).round() as u8])
    })
}

fn smooth01(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// One channel of the picture (0 red, 1 green, 2 blue, 3 alpha) as a
/// selection: brighter in that channel is more selected.
pub fn channel_mask(img: &RgbaImage, ch: usize) -> GrayImage {
    let (w, h) = img.dimensions();
    let ch = ch.min(3);
    GrayImage::from_fn(w, h, |x, y| Luma([img.get_pixel(x, y)[ch]]))
}

/// A band `r` pixels wide each side of the selection's edge.
pub fn border(m: &GrayImage, r: u32) -> GrayImage {
    let r = r.max(1) as i32;
    let out = grow(m, r);
    let inner = grow(m, -r);
    let (w, h) = m.dimensions();
    GrayImage::from_fn(w, h, |x, y| {
        Luma([out.get_pixel(x, y)[0].saturating_sub(inner.get_pixel(x, y)[0])])
    })
}

/// The selection's edge smoothed: corners rounded and specks gone, a
/// hard edge kept.
pub fn smooth_selection(m: &GrayImage, r: u32) -> GrayImage {
    let f = feather(m, r.max(1));
    let (w, h) = m.dimensions();
    GrayImage::from_fn(w, h, |x, y| {
        Luma([if f.get_pixel(x, y)[0] >= 128 { 255 } else { 0 }])
    })
}

// ---------------------------------------------------------------------
// Painting a mask
// ---------------------------------------------------------------------

/// One stroke painted on a mask (the selection in quick mask, or a
/// layer's mask) with a round brush: every pixel goes towards `value` as
/// much as the stroke covers it, never past the brush's opacity.
pub struct MaskStroke {
    pub opts: BrushOpts,
    value: u8,
    base: GrayImage,
    cover: Vec<f32>,
    last: Option<(f32, f32)>,
    pub dirty: Option<Dirty>,
}

impl MaskStroke {
    pub fn new(opts: BrushOpts, mask: &GrayImage, value: u8) -> Self {
        let n = (mask.width() as usize) * (mask.height() as usize);
        MaskStroke {
            opts,
            value,
            base: mask.clone(),
            cover: vec![0.0; n],
            last: None,
            dirty: None,
        }
    }

    /// The brush taken to (x, y); `limit` (the selection, when a layer's
    /// mask is painted) keeps the paint inside it.
    pub fn to(
        &mut self,
        mask: &mut GrayImage,
        limit: Option<&GrayImage>,
        x: f32,
        y: f32,
    ) -> Option<Dirty> {
        let step = (self.opts.size * 0.25).max(1.0);
        let pts: Vec<(f32, f32)> = match self.last {
            None => vec![(x, y)],
            Some((lx, ly)) => {
                let d = ((x - lx).powi(2) + (y - ly).powi(2)).sqrt();
                if d < step {
                    return None;
                }
                let n = (d / step).floor() as usize;
                (1..=n)
                    .map(|i| {
                        let t = i as f32 * step / d;
                        (lx + (x - lx) * t, ly + (y - ly) * t)
                    })
                    .collect()
            }
        };
        let mut changed: Option<Dirty> = None;
        for &(px, py) in &pts {
            if let Some(b) = self.dab(mask, limit, px, py) {
                changed = Some(union(changed, b));
            }
        }
        self.last = pts.last().copied();
        if let Some(b) = changed {
            self.dirty = Some(union(self.dirty, b));
        }
        changed
    }

    fn dab(
        &mut self,
        mask: &mut GrayImage,
        limit: Option<&GrayImage>,
        cx: f32,
        cy: f32,
    ) -> Option<Dirty> {
        let (w, h) = mask.dimensions();
        let r = (self.opts.size / 2.0).max(0.5);
        let x0 = (cx - r).floor().max(0.0) as u32;
        let y0 = (cy - r).floor().max(0.0) as u32;
        let x1 = ((cx + r).ceil().max(0.0) as u32).min(w);
        let y1 = ((cy + r).ceil().max(0.0) as u32).min(h);
        if x0 >= x1 || y0 >= y1 {
            return None;
        }
        let flow = self.opts.flow.clamp(0.01, 1.0);
        let opacity = self.opts.opacity.clamp(0.0, 1.0);
        for y in y0..y1 {
            for x in x0..x1 {
                let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
                let lim = limit
                    .map(|m| {
                        m.get_pixel_checked(x, y)
                            .map(|v| v[0] as f32 / 255.0)
                            .unwrap_or(0.0)
                    })
                    .unwrap_or(1.0);
                let wgt = dab_weight(d, self.opts.size, self.opts.hardness) * lim;
                if wgt <= 0.0 {
                    continue;
                }
                let Some(c) = self.cover.get_mut((y * w + x) as usize) else {
                    continue;
                };
                *c = (*c + (opacity - *c).max(0.0) * wgt * flow).min(opacity);
                let b = self.base.get_pixel(x, y)[0] as f32;
                let v = b + (self.value as f32 - b) * *c;
                mask.put_pixel(x, y, Luma([v.round().clamp(0.0, 255.0) as u8]));
            }
        }
        Some([x0, y0, x1, y1])
    }
}

/// A mask shown over the picture: red where it hides (or leaves out),
/// clear where it shows.
pub fn mask_overlay(m: &GrayImage) -> RgbaImage {
    let (w, h) = m.dimensions();
    RgbaImage::from_fn(w, h, |x, y| {
        let v = m.get_pixel(x, y)[0];
        Rgba([255, 0, 0, ((255 - v) as u16 * 128 / 255) as u8])
    })
}

/// `img` moved by (dx, dy), the same size.
pub fn offset_image(img: &RgbaImage, dx: i64, dy: i64) -> RgbaImage {
    let (w, h) = img.dimensions();
    let mut out = RgbaImage::new(w, h);
    image::imageops::replace(&mut out, img, dx, dy);
    out
}

/// A straight-alpha picture as a premultiplied pixmap, its alpha scaled
/// by `opacity`.
fn rgba_to_pixmap(img: &RgbaImage, opacity: f32) -> Option<tiny_skia::Pixmap> {
    let (w, h) = img.dimensions();
    let mut pm = tiny_skia::Pixmap::new(w, h)?;
    let k = opacity.clamp(0.0, 1.0);
    for (d, s) in pm.pixels_mut().iter_mut().zip(img.pixels()) {
        let a = (s[3] as f32 * k).round() as u8;
        *d = tiny_skia::ColorU8::from_rgba(s[0], s[1], s[2], a).premultiply();
    }
    Some(pm)
}

/// A premultiplied pixmap back to straight alpha.
fn pixmap_to_rgba(pm: &tiny_skia::Pixmap, w: u32, h: u32) -> Option<RgbaImage> {
    let data: Vec<u8> = pm
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect();
    RgbaImage::from_raw(w, h, data)
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
    /// Restore: the picture as it was when the editing began, and where
    /// the picture's top left pixel lies in it.
    original: Option<std::rc::Rc<RgbaImage>>,
    origin: (i64, i64),
    /// Replace colour: the colour under the first dab.
    target: Option<Rgba<u8>>,
    /// Scatter: the state of the dabs' random offsets.
    seed: u32,
    /// Follow surface: the relief of the photo.
    surface: Option<std::rc::Rc<crate::pixel_surface::SurfaceMap>>,
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
            original: None,
            origin: pi.origin,
            target: None,
            seed: 0x9E37_79B9,
            surface: None,
        }
    }

    /// The relief the brush and the stamp follow, when the option is on.
    pub fn with_surface(mut self, map: std::rc::Rc<crate::pixel_surface::SurfaceMap>) -> Self {
        if self.opts.surface && matches!(self.tool, PixelTool::Brush | PixelTool::Clone) {
            self.surface = Some(map);
        }
        self
    }

    /// The picture as it was when the editing began, for Restore.
    pub fn with_original(mut self, img: std::rc::Rc<RgbaImage>) -> Self {
        self.original = Some(img);
        self
    }

    /// A random number from -1 to 1 (the same each stroke).
    fn jitter(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        (self.seed as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    /// A dab at (x, y), strayed by the scatter.
    fn scattered_dab(&mut self, pi: &mut PixelImage, x: f32, y: f32) -> Option<Dirty> {
        let s = self.opts.scatter.max(0.0) * self.opts.size * 0.5;
        if s > 0.0 {
            let (jx, jy) = (self.jitter(), self.jitter());
            self.dab(pi, x + jx * s, y + jy * s)
        } else {
            self.dab(pi, x, y)
        }
    }

    /// The brush taken to (x, y): dabs along the way from the last
    /// point, the spacing apart (a quarter of the brush by default).
    pub fn to(&mut self, pi: &mut PixelImage, x: f32, y: f32) -> Option<Dirty> {
        let step = (self.opts.size * self.opts.spacing.clamp(0.01, 5.0)).max(1.0);
        let mut changed: Option<Dirty> = None;
        match self.last {
            None => {
                changed = self.scattered_dab(pi, x, y);
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
                    if let Some(b) = self.scattered_dab(pi, px, py) {
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
        if self.tool == PixelTool::ReplaceColor && self.target.is_none() {
            let (tx, ty) = (cx.floor().max(0.0) as u32, cy.floor().max(0.0) as u32);
            self.target = self.base.get_pixel_checked(tx, ty).copied();
        }
        let flow = self.opts.flow.clamp(0.01, 1.0);
        let opacity = self.opts.opacity.clamp(0.0, 1.0);
        // The tip turned by its angle and flattened by its roundness.
        let (sin, cos) = self.opts.angle.to_radians().sin_cos();
        let round = self.opts.roundness.clamp(0.05, 1.0);
        for y in y0..y1 {
            for x in x0..x1 {
                let (mut px, mut py) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                if let Some(s) = &self.surface {
                    // The dab seen through the relief: bent along the folds.
                    let (ox, oy) =
                        s.offset(x as f32 + 0.5, y as f32 + 0.5, self.opts.surface_depth);
                    px += ox;
                    py += oy;
                }
                let d = if round < 1.0 {
                    let u = px * cos + py * sin;
                    let v = (py * cos - px * sin) / round;
                    (u * u + v * v).sqrt()
                } else {
                    (px * px + py * py).sqrt()
                };
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
            PixelTool::Brush => match &self.surface {
                Some(s) => over(
                    base,
                    s.shade(self.opts.color, x, y, self.opts.surface_light),
                    t,
                ),
                None => over(base, self.opts.color, t),
            },
            PixelTool::Eraser => Rgba([
                base[0],
                base[1],
                base[2],
                (base[3] as f32 * (1.0 - t)).round() as u8,
            ]),
            PixelTool::TransparencyBrush => {
                let k = self.opts.transparency.clamp(0.0, 1.0);
                Rgba([
                    base[0],
                    base[1],
                    base[2],
                    (base[3] as f32 * (1.0 - k * t)).round() as u8,
                ])
            }
            PixelTool::Clone => match self.source(x, y) {
                Some(src) => match &self.surface {
                    Some(s) => mix(
                        base,
                        &Rgba(s.shade(src.0, x, y, self.opts.surface_light)),
                        t,
                    ),
                    None => mix(base, &src, t),
                },
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
            PixelTool::Restore => {
                let ox = x as i64 + self.origin.0;
                let oy = y as i64 + self.origin.1;
                let was = self
                    .original
                    .as_ref()
                    .filter(|_| ox >= 0 && oy >= 0)
                    .and_then(|o| o.get_pixel_checked(ox as u32, oy as u32).copied())
                    .unwrap_or(Rgba([0, 0, 0, 0]));
                mix(base, &was, t)
            }
            PixelTool::Tint => {
                // The brush colour moved to the pixel's lightness.
                let c = Rgba(self.opts.color);
                let shift = crate::fx::util::luma(base) - crate::fx::util::luma(&c);
                let ch = |i: usize| (c[i] as f32 + shift).round().clamp(0.0, 255.0) as u8;
                mix(base, &Rgba([ch(0), ch(1), ch(2), base[3]]), t)
            }
            PixelTool::ReplaceColor => {
                // Pixels near the first dab's colour take the brush colour,
                // keeping how far they were from it.
                let Some(tg) = self.target else {
                    return *base;
                };
                let tol = self.opts.tolerance.max(1) as f32;
                let near = 1.0 - (color_distance(base, &tg) as f32 / tol).min(1.0);
                if near <= 0.0 {
                    return *base;
                }
                let ch = |i: usize| {
                    (self.opts.color[i] as f32 + base[i] as f32 - tg[i] as f32)
                        .round()
                        .clamp(0.0, 255.0) as u8
                };
                mix(base, &Rgba([ch(0), ch(1), ch(2), base[3]]), t * near)
            }
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
/// The place painted with the Remove tool (0 in `painted`) widened by
/// `grow_by` pixels and filled from what is around it, inside the
/// selection when there is one.
pub fn remove_painted(pi: &mut PixelImage, painted: &GrayImage, grow_by: u32) -> Option<Dirty> {
    let (w, h) = pi.img.dimensions();
    if painted.dimensions() != (w, h) {
        return None;
    }
    let mut m = grow(&invert(Some(painted), w, h), grow_by as i32);
    if let Some(s) = &pi.sel {
        for (p, s) in m.pixels_mut().zip(s.pixels()) {
            p[0] = p[0].min(s[0]);
        }
    }
    for p in m.pixels_mut() {
        p[0] = if p[0] >= 64 { 255 } else { 0 };
    }
    if m.pixels().all(|p| p[0] == 0) {
        return None;
    }
    let keep = pi.sel.replace(m);
    let done = content_fill(pi);
    pi.sel = keep;
    done
}

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
        let per = state.bytes().max(1);
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
    fn a_new_layer_is_clear_and_painting_changes_only_it() {
        let mut pi = solid(10, 10, [255, 255, 255, 255]);
        assert!(!pi.is_layered());
        pi.add_layer("Layer 1");
        assert!(pi.is_layered());
        assert_eq!((pi.layers.len(), pi.active), (2, 1));
        assert_eq!(pi.img.get_pixel(3, 3)[3], 0);
        fill_selection(&mut pi, [255, 0, 0, 255], 1.0).unwrap();
        assert_eq!(pi.layer_pixels(0).unwrap().get_pixel(3, 3)[1], 255);
        assert_eq!(*pi.composite().get_pixel(3, 3), Rgba([255, 0, 0, 255]));
        pi.set_active(0);
        assert_eq!(pi.img.get_pixel(3, 3)[1], 255, "background untouched");
        assert_eq!(pi.layer_pixels(1).unwrap().get_pixel(3, 3)[0], 255);
    }

    #[test]
    fn layer_opacity_visibility_and_merge_mode_change_what_is_seen() {
        let mut pi = solid(4, 4, [200, 200, 200, 255]);
        pi.add_layer("top");
        fill_selection(&mut pi, [0, 0, 0, 255], 1.0).unwrap();
        pi.layers[1].opacity = 0.5;
        let v = pi.composite().get_pixel(1, 1)[0];
        assert!((95..=105).contains(&v), "{v}");
        pi.layers[1].visible = false;
        assert_eq!(pi.composite().get_pixel(1, 1)[0], 200);
        pi.layers[1].visible = true;
        pi.layers[1].opacity = 1.0;
        pi.img = RgbaImage::from_pixel(4, 4, Rgba([255, 255, 255, 255]));
        pi.layers[1].merge = MergeMode::Multiply;
        assert_eq!(
            pi.composite().get_pixel(1, 1)[0],
            200,
            "white multiplies to the backdrop"
        );
        assert_eq!(pi.seen_at(1, 1).unwrap()[0], 200);
    }

    #[test]
    fn merge_down_flatten_delete_and_order() {
        let mut pi = solid(4, 4, [255, 255, 255, 255]);
        pi.add_layer("a");
        fill_selection(&mut pi, [0, 0, 255, 255], 1.0).unwrap();
        pi.layers[1].opacity = 0.5;
        let seen = pi.composite();
        assert!(pi.merge_down());
        assert_eq!((pi.layers.len(), pi.active), (1, 0));
        assert_eq!(pi.img, seen);
        pi.add_layer("b");
        pi.add_layer("c");
        assert_eq!(pi.active, 2);
        assert!(pi.move_layer(false));
        assert_eq!(pi.active, 1);
        assert_eq!(pi.layers[1].name, "c");
        assert!(pi.delete_layer());
        assert_eq!(pi.layers.len(), 2);
        assert_eq!(pi.layers[pi.active].name, "");
        let all = pi.composite();
        assert!(pi.flatten());
        assert_eq!(pi.layers.len(), 1);
        assert_eq!(pi.img, all);
        assert!(!pi.delete_layer(), "the last layer stays");
    }

    #[test]
    fn copy_and_cut_to_layer_and_move_a_layer() {
        let mut pi = solid(10, 10, [9, 9, 9, 255]);
        pi.sel = rect_mask(10, 10, (0.0, 0.0), (5.0, 10.0));
        assert!(pi.selection_to_layer("copy", false));
        assert_eq!(pi.img.get_pixel(1, 1)[3], 255);
        assert_eq!(pi.img.get_pixel(8, 1)[3], 0);
        pi.set_active(0);
        assert!(pi.selection_to_layer("cut", true));
        assert_eq!(
            pi.layer_pixels(0).unwrap().get_pixel(1, 1)[3],
            0,
            "cut away"
        );
        pi.offset_layer(5, 0);
        assert_eq!(pi.img.get_pixel(1, 1)[3], 0);
        assert_eq!(pi.img.get_pixel(6, 1)[3], 255);
        let img = RgbaImage::from_pixel(2, 2, Rgba([1, 2, 3, 255]));
        pi.sel = rect_mask(10, 10, (4.0, 4.0), (8.0, 8.0));
        paste_as_layer(&mut pi, "p".into(), &img).unwrap();
        assert_eq!(*pi.img.get_pixel(4, 4), Rgba([1, 2, 3, 255]));
        assert_eq!(pi.img.get_pixel(7, 7)[3], 0);
    }

    #[test]
    fn restore_tint_and_replace_colour_brushes() {
        let hard = BrushOpts {
            size: 6.0,
            hardness: 1.0,
            opacity: 1.0,
            flow: 1.0,
            ..BrushOpts::default()
        };
        // Restore paints back the picture as first opened, crop included.
        let orig = std::rc::Rc::new(RgbaImage::from_pixel(10, 10, Rgba([9, 9, 9, 255])));
        let mut pi = solid(10, 10, [200, 0, 0, 255]);
        pi.origin = (2, 0);
        let mut s = Stroke::new(PixelTool::Restore, hard, &pi, (0.0, 0.0)).with_original(orig);
        s.to(&mut pi, 3.0, 5.0);
        assert_eq!(*pi.img.get_pixel(3, 5), Rgba([9, 9, 9, 255]));
        assert_eq!(pi.img.get_pixel(9, 5)[0], 200);
        // Outside the original picture it paints clear.
        let mut s = Stroke::new(PixelTool::Restore, hard, &pi, (0.0, 0.0))
            .with_original(std::rc::Rc::new(RgbaImage::new(4, 4)));
        s.to(&mut pi, 7.0, 5.0);
        assert_eq!(pi.img.get_pixel(7, 5)[3], 0);
        // Tint: a grey pixel takes the hue and keeps its lightness.
        let mut pi = solid(10, 10, [128, 128, 128, 255]);
        let o = BrushOpts {
            color: [255, 0, 0, 255],
            ..hard
        };
        Stroke::new(PixelTool::Tint, o, &pi, (0.0, 0.0)).to(&mut pi, 5.0, 5.0);
        let q = *pi.img.get_pixel(5, 5);
        assert!(q[0] > q[1] && q[1] == q[2], "{q:?}");
        let l = crate::fx::util::luma(&q);
        assert!((l - 128.0).abs() < 25.0, "{l}");
        // Replace colour: only pixels like the first dab's change.
        let mut pi = solid(10, 10, [0, 200, 0, 255]);
        for y in 0..10 {
            pi.img.put_pixel(6, y, Rgba([0, 0, 200, 255]));
        }
        let o = BrushOpts {
            color: [200, 0, 0, 255],
            tolerance: 40,
            size: 10.0,
            ..hard
        };
        Stroke::new(PixelTool::ReplaceColor, o, &pi, (0.0, 0.0)).to(&mut pi, 4.5, 5.0);
        assert_eq!(*pi.img.get_pixel(4, 5), Rgba([200, 0, 0, 255]));
        assert_eq!(*pi.img.get_pixel(6, 5), Rgba([0, 0, 200, 255]));
    }

    #[test]
    fn the_remove_brush_fills_the_painted_place_and_its_edge() {
        // A grey wall with a dark spot and its soft shadow.
        let mut img = RgbaImage::from_pixel(40, 40, Rgba([120, 120, 120, 255]));
        for y in 15..25 {
            for x in 15..25 {
                img.put_pixel(x, y, Rgba([10, 10, 10, 255]));
            }
        }
        for x in 15..26 {
            img.put_pixel(x, 25, Rgba([70, 70, 70, 255]));
        }
        let mut pi = PixelImage::new(img);
        // Painted over the spot only, not its shadow.
        let painted = GrayImage::from_fn(40, 40, |x, y| {
            Luma([if (15..25).contains(&x) && (15..25).contains(&y) {
                0
            } else {
                255
            }])
        });
        remove_painted(&mut pi, &painted, 2).unwrap();
        assert!(pi.img.get_pixel(20, 20)[0] > 100, "spot gone");
        assert!(
            pi.img.get_pixel(20, 25)[0] > 100,
            "the shadow on the edge went too"
        );
        assert_eq!(*pi.img.get_pixel(3, 3), Rgba([120, 120, 120, 255]));
        assert!(pi.sel.is_none(), "the selection is left as it was");
        let nothing = GrayImage::from_pixel(40, 40, Luma([255]));
        assert!(remove_painted(&mut pi, &nothing, 2).is_none());
    }

    #[test]
    fn a_brush_following_the_surface_is_shaded_by_the_photo() {
        // Light cloth with a dark fold down the middle.
        let img = RgbaImage::from_fn(60, 20, |x, _| {
            let v = if (28..32).contains(&x) { 60 } else { 200 };
            Rgba([v, v, v, 255])
        });
        let mut pi = PixelImage::new(img);
        pi.insert_layer("paint", RgbaImage::new(60, 20));
        let opts = BrushOpts {
            color: [200, 0, 0, 255],
            size: 80.0,
            hardness: 1.0,
            opacity: 1.0,
            flow: 1.0,
            surface: true,
            surface_light: 1.0,
            ..BrushOpts::default()
        };
        let map =
            crate::pixel_surface::SurfaceMap::new(&crate::pixel_surface::seen_beneath(&pi), None);
        let mut s = Stroke::new(PixelTool::Brush, opts, &pi, (0.0, 0.0))
            .with_surface(std::rc::Rc::new(map));
        s.to(&mut pi, 30.0, 10.0);
        let fold = pi.img.get_pixel(30, 10)[0];
        let flat = pi.img.get_pixel(10, 10)[0];
        assert!(fold < flat, "{fold} {flat}");
        // Without the option the colour is flat.
        let mut pi2 = PixelImage::new(RgbaImage::new(60, 20));
        let mut s2 = Stroke::new(
            PixelTool::Brush,
            BrushOpts {
                surface: false,
                ..opts
            },
            &pi2,
            (0.0, 0.0),
        );
        s2.to(&mut pi2, 30.0, 10.0);
        assert_eq!(pi2.img.get_pixel(30, 10)[0], pi2.img.get_pixel(10, 10)[0]);
    }

    #[test]
    fn the_tip_spacing_and_roundness_shape_the_stroke() {
        let o = BrushOpts {
            size: 10.0,
            hardness: 1.0,
            opacity: 1.0,
            flow: 1.0,
            color: [0, 0, 0, 255],
            roundness: 0.3,
            angle: 0.0,
            ..BrushOpts::default()
        };
        let mut pi = solid(20, 20, [255, 255, 255, 255]);
        Stroke::new(PixelTool::Brush, o, &pi, (0.0, 0.0)).to(&mut pi, 10.0, 10.0);
        assert_eq!(pi.img.get_pixel(14, 10)[0], 0, "wide across");
        assert_eq!(pi.img.get_pixel(10, 14)[0], 255, "flat up and down");
        let o = BrushOpts {
            size: 2.0,
            spacing: 3.0,
            roundness: 1.0,
            ..o
        };
        let mut pi = solid(20, 4, [255, 255, 255, 255]);
        let mut s = Stroke::new(PixelTool::Brush, o, &pi, (0.0, 0.0));
        s.to(&mut pi, 1.0, 2.0);
        s.to(&mut pi, 19.0, 2.0);
        let painted = (0..20).filter(|x| pi.img.get_pixel(*x, 2)[0] == 0).count();
        assert!(
            (3..=10).contains(&painted),
            "dabs six pixels apart: {painted}"
        );
    }

    #[test]
    fn crop_and_flip_reach_every_layer() {
        let mut pi = solid(10, 10, [9, 9, 9, 255]);
        pi.add_layer("top");
        pi.img.put_pixel(0, 5, Rgba([255, 0, 0, 255]));
        flip(&mut pi, true);
        assert_eq!(pi.img.get_pixel(9, 5)[0], 255);
        pi.sel = rect_mask(10, 10, (2.0, 2.0), (6.0, 8.0));
        crop_to_selection(&mut pi).unwrap();
        assert_eq!(pi.img.dimensions(), (4, 6));
        assert_eq!(pi.layer_pixels(0).unwrap().dimensions(), (4, 6));
        assert_eq!(pi.composite().dimensions(), (4, 6));
    }

    #[test]
    fn a_layer_mask_hides_without_erasing_and_applies() {
        let mut pi = solid(10, 10, [255, 255, 255, 255]);
        pi.add_layer("top");
        fill_selection(&mut pi, [0, 0, 0, 255], 1.0).unwrap();
        pi.sel = rect_mask(10, 10, (0.0, 0.0), (5.0, 10.0));
        assert!(pi.add_layer_mask());
        pi.sel = None;
        let seen = pi.composite();
        assert_eq!(seen.get_pixel(1, 1)[0], 0, "inside the mask shows");
        assert_eq!(seen.get_pixel(8, 1)[0], 255, "outside the mask hidden");
        assert_eq!(pi.img.get_pixel(8, 1)[3], 255, "pixels kept");
        assert!(pi.invert_layer_mask());
        assert_eq!(pi.composite().get_pixel(1, 1)[0], 255);
        assert!(pi.apply_layer_mask());
        assert_eq!(pi.img.get_pixel(1, 1)[3], 0);
        assert!(pi.layers[1].mask.is_none());
        assert!(!pi.delete_layer_mask());
    }

    #[test]
    fn a_clipped_layer_shows_only_over_the_layer_beneath() {
        let mut pi = PixelImage::new(RgbaImage::new(10, 10));
        pi.sel = rect_mask(10, 10, (0.0, 0.0), (5.0, 10.0));
        fill_selection(&mut pi, [255, 255, 255, 255], 1.0).unwrap();
        pi.sel = None;
        pi.add_layer("red");
        fill_selection(&mut pi, [255, 0, 0, 255], 1.0).unwrap();
        pi.layers[1].clip = true;
        let seen = pi.composite();
        assert_eq!(*seen.get_pixel(1, 1), Rgba([255, 0, 0, 255]));
        assert_eq!(seen.get_pixel(8, 1)[3], 0, "nothing beneath, nothing shows");
        assert!(pi.merge_down());
        assert_eq!(pi.img.get_pixel(8, 1)[3], 0);
        assert_eq!(*pi.img.get_pixel(1, 1), Rgba([255, 0, 0, 255]));
    }

    #[test]
    fn a_mask_stroke_moves_towards_its_value_up_to_the_opacity() {
        let mut m = GrayImage::new(20, 20);
        let opts = BrushOpts {
            size: 8.0,
            hardness: 1.0,
            opacity: 0.5,
            flow: 1.0,
            ..BrushOpts::default()
        };
        let mut s = MaskStroke::new(opts, &m, 255);
        for i in 0..10 {
            s.to(&mut m, None, 10.0 + (i % 2) as f32 * 3.0, 10.0);
        }
        let v = m.get_pixel(10, 10)[0];
        assert!((120..=135).contains(&v), "{v}");
        assert_eq!(m.get_pixel(0, 0)[0], 0);
        let limit = rect_mask(20, 20, (0.0, 0.0), (10.0, 20.0)).unwrap();
        let mut m2 = GrayImage::new(20, 20);
        let mut s2 = MaskStroke::new(
            BrushOpts {
                opacity: 1.0,
                ..opts
            },
            &m2,
            255,
        );
        s2.to(&mut m2, Some(&limit), 10.0, 10.0);
        assert_eq!(m2.get_pixel(12, 10)[0], 0, "outside the selection");
        assert_eq!(m2.get_pixel(8, 10)[0], 255);
    }

    #[test]
    fn tone_channel_border_and_smooth_selections() {
        let grey = RgbaImage::from_fn(30, 1, |x, _| {
            let v = (x * 255 / 29) as u8;
            Rgba([v, v, v, 255])
        });
        let sh = tone_mask(&grey, Tones::Shadows);
        let hi = tone_mask(&grey, Tones::Highlights);
        let mid = tone_mask(&grey, Tones::Midtones);
        assert!(sh.get_pixel(0, 0)[0] > 200 && sh.get_pixel(29, 0)[0] < 20);
        assert!(hi.get_pixel(29, 0)[0] > 200 && hi.get_pixel(0, 0)[0] < 20);
        assert!(mid.get_pixel(15, 0)[0] > 200 && mid.get_pixel(0, 0)[0] < 20);
        let img = RgbaImage::from_fn(30, 1, |x, _| {
            let v = (x * 255 / 29) as u8;
            Rgba([v, 0, 255 - v, 255])
        });
        let red = channel_mask(&img, 0);
        assert_eq!(red.get_pixel(29, 0)[0], 255);
        let m = rect_mask(30, 30, (10.0, 10.0), (20.0, 20.0)).unwrap();
        let b = border(&m, 2);
        assert_eq!(b.get_pixel(15, 15)[0], 0);
        assert_eq!(b.get_pixel(10, 15)[0], 255);
        let mut specks = m.clone();
        specks.put_pixel(2, 2, Luma([255]));
        let sm = smooth_selection(&specks, 2);
        assert_eq!(sm.get_pixel(2, 2)[0], 0, "speck gone");
        assert_eq!(sm.get_pixel(15, 15)[0], 255);
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
