//! Mockup (Effects > Mockup): the selected design laid on a product photo
//! (a T-shirt, a mug, a bottle, a bag, a cushion, a poster, a shop sign,
//! a van, business cards, a flyer, a flag, a phone case, a box) or on a
//! photo of the user's own, to show a client how the print will look.
//!
//! The design is grouped and given three live effects, all of them
//! editable afterwards like any other: a Mesh Envelope that bends it onto
//! the product's surface (a plane in perspective, a cylinder, a puffed
//! cushion, a waving flag), Displace with the photo's own lightness, so
//! the print follows the cloth's folds and takes its shadows and
//! highlights, and, on light products, uniform transparency with the
//! Multiply merge mode so the texture shows through the ink. The photo
//! and the design make one group, one undo step. The photos are drawn
//! by `mockup_photos`, in the program itself: nothing to license, no
//! brand. [`App::mockup_jpeg`] renders the result as a JPEG to send for
//! approval, with an optional caption.

use crate::app::App;
use crate::mockup_photos as ph;
use tracedraw_core::geometry::{Affine, Point, Rect, Shape as _};
use tracedraw_core::live::{Effect, MergeMode, SurfaceMap, SurfaceShade};
use tracedraw_core::{Color, Command, Fill, Shape, ShapeId, ShapeKind};

/// The data key a mockup group carries, with its template's id.
pub const DATA_KEY: &str = "mockup";

/// The photo's width in pixels when placed.
pub const PHOTO_PX: u32 = 1600;

/// The grid of the mesh that bends the design.
const MESH: u8 = 8;

/// How the design lies on the product's surface, in photo grid units
/// (see [`ph::REF_W`]); `u` runs left to right and `v` top to bottom
/// across the print area.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Surface {
    /// A flat print area in perspective: its corners top left, top
    /// right, bottom right, bottom left; `bulge` (a fraction) brings the
    /// middle forward, as on a cushion or a chest.
    Plane { corners: [(f32, f32); 4], bulge: f32 },
    /// Round a cylinder standing up: its axis at `cx`, its radius `r`,
    /// the area from `top` to `bottom` (on the axis), the ellipse of a
    /// level line `ry` deep, `span` degrees of it facing the eye.
    Cylinder {
        cx: f32,
        top: f32,
        bottom: f32,
        r: f32,
        ry: f32,
        span: f32,
    },
    /// The waving cloth of the flag photo.
    Flag { corners: [(f32, f32); 4] },
}

impl Surface {
    /// The photo point at `(u, v)` of the print area.
    pub fn at(&self, u: f32, v: f32) -> (f32, f32) {
        match *self {
            Surface::Plane { corners, bulge } => {
                let (x, y) = square_to_quad(&corners, u, v);
                if bulge == 0.0 {
                    return (x, y);
                }
                let (cx, cy) = square_to_quad(&corners, 0.5, 0.5);
                let k = 1.0 + bulge * (std::f32::consts::PI * u).sin() * (std::f32::consts::PI * v).sin();
                (cx + (x - cx) * k, cy + (y - cy) * k)
            }
            Surface::Cylinder {
                cx,
                top,
                bottom,
                r,
                ry,
                span,
            } => {
                let t = (u - 0.5) * span.to_radians();
                (cx + r * t.sin(), top + (bottom - top) * v + ry * t.cos())
            }
            Surface::Flag { corners } => {
                let (x, y) = square_to_quad(&corners, u, v);
                let along = ((x - corners[0].0) / (corners[1].0 - corners[0].0).max(1.0)).clamp(0.0, 1.0);
                (x, y + ph::flag_wave(along))
            }
        }
    }
}

/// The unit square mapped onto a quadrilateral in perspective (corners
/// top left, top right, bottom right, bottom left).
pub fn square_to_quad(q: &[(f32, f32); 4], u: f32, v: f32) -> (f32, f32) {
    let [(x0, y0), (x1, y1), (x2, y2), (x3, y3)] = q.map(|(x, y)| (f64::from(x), f64::from(y)));
    let (u, v) = (f64::from(u), f64::from(v));
    let (sx, sy) = (x0 - x1 + x2 - x3, y0 - y1 + y2 - y3);
    let (a, b, c, d, e, f, g, h);
    if sx.abs() < 1e-9 && sy.abs() < 1e-9 {
        (a, b, c, d, e, f, g, h) = (x1 - x0, x3 - x0, x0, y1 - y0, y3 - y0, y0, 0.0, 0.0);
    } else {
        let (dx1, dx2, dy1, dy2) = (x1 - x2, x3 - x2, y1 - y2, y3 - y2);
        let den = dx1 * dy2 - dx2 * dy1;
        if den.abs() < 1e-12 {
            return ((x0 + (x1 - x0) * u) as f32, (y0 + (y3 - y0) * v) as f32);
        }
        g = (sx * dy2 - dx2 * sy) / den;
        h = (dx1 * sy - sx * dy1) / den;
        a = x1 - x0 + g * x1;
        b = x3 - x0 + h * x3;
        c = x0;
        d = y1 - y0 + g * y1;
        e = y3 - y0 + h * y3;
        f = y0;
    }
    let w = g * u + h * v + 1.0;
    if w.abs() < 1e-12 {
        return (x0 as f32, y0 as f32);
    }
    (((a * u + b * v + c) / w) as f32, ((d * u + e * v + f) / w) as f32)
}

/// A ready product photo with its print area.
pub struct Template {
    /// Kept in the mockup's data and the settings.
    pub id: &'static str,
    /// i18n key of its name.
    pub key: &'static str,
    pub draw: fn(&mut ph::Canvas),
    pub surface: Surface,
    /// The print area's width over its height, as printed.
    pub aspect: f32,
    /// Where the design sits up and down in the area when it is less
    /// tall (0 at the top, 1 at the bottom).
    pub anchor: f32,
    /// A light product: the ink multiplies with it.
    pub multiply: bool,
    /// How strongly the photo's light and shade go onto the print, 0..1.
    pub shade: f64,
    /// How far the photo's lightness pushes the print, in photo units.
    pub push: f64,
}

fn shirt_white(c: &mut ph::Canvas) {
    ph::shirt(c, ph::rgb(246, 244, 240));
}
fn shirt_black(c: &mut ph::Canvas) {
    ph::shirt(c, ph::rgb(34, 34, 36));
}
fn shirt_grey(c: &mut ph::Canvas) {
    ph::shirt(c, ph::rgb(170, 172, 176));
}
fn mug_white(c: &mut ph::Canvas) {
    ph::mug(c, ph::rgb(246, 246, 244));
}
fn mug_black(c: &mut ph::Canvas) {
    ph::mug(c, ph::rgb(40, 40, 44));
}
fn bottle(c: &mut ph::Canvas) {
    ph::bottle(c, ph::rgb(236, 238, 240));
}

const CHEST: Surface = Surface::Plane {
    corners: ph::SHIRT_CHEST,
    bulge: 0.04,
};
const MUG_SURFACE: Surface = Surface::Cylinder {
    cx: ph::MUG.0,
    top: ph::MUG.1,
    bottom: ph::MUG.2,
    r: ph::MUG.3,
    ry: ph::MUG.4,
    span: 140.0,
};
const BOTTLE_SURFACE: Surface = Surface::Cylinder {
    cx: ph::BOTTLE.0,
    top: ph::BOTTLE.1,
    bottom: ph::BOTTLE.2,
    r: ph::BOTTLE.3,
    ry: ph::BOTTLE.4,
    span: 130.0,
};
const fn plane(corners: [(f32, f32); 4]) -> Surface {
    Surface::Plane { corners, bulge: 0.0 }
}

/// The ready templates, in the order the docker shows them.
pub const TEMPLATES: [Template; 16] = [
    Template { id: "shirt-white", key: "mockup.shirt_white", draw: shirt_white, surface: CHEST, aspect: 0.77, anchor: 0.1, multiply: true, shade: 0.55, push: 3.0 },
    Template { id: "shirt-black", key: "mockup.shirt_black", draw: shirt_black, surface: CHEST, aspect: 0.77, anchor: 0.1, multiply: false, shade: 0.8, push: 3.0 },
    Template { id: "shirt-grey", key: "mockup.shirt_grey", draw: shirt_grey, surface: CHEST, aspect: 0.77, anchor: 0.1, multiply: true, shade: 0.6, push: 3.0 },
    Template { id: "mug-white", key: "mockup.mug_white", draw: mug_white, surface: MUG_SURFACE, aspect: 1.3, anchor: 0.5, multiply: true, shade: 0.5, push: 0.0 },
    Template { id: "mug-black", key: "mockup.mug_black", draw: mug_black, surface: MUG_SURFACE, aspect: 1.3, anchor: 0.5, multiply: false, shade: 0.7, push: 0.0 },
    Template { id: "bottle", key: "mockup.bottle", draw: bottle, surface: BOTTLE_SURFACE, aspect: 0.75, anchor: 0.4, multiply: true, shade: 0.6, push: 0.0 },
    Template { id: "tote", key: "mockup.tote", draw: ph::tote, surface: plane(ph::TOTE), aspect: 0.96, anchor: 0.3, multiply: true, shade: 0.6, push: 2.5 },
    Template { id: "cushion", key: "mockup.cushion", draw: ph::cushion, surface: Surface::Plane { corners: ph::CUSHION, bulge: 0.06 }, aspect: 1.0, anchor: 0.5, multiply: true, shade: 0.6, push: 2.0 },
    Template { id: "poster", key: "mockup.poster", draw: ph::poster, surface: plane(ph::POSTER), aspect: 0.75, anchor: 0.5, multiply: true, shade: 0.3, push: 0.0 },
    Template { id: "sign", key: "mockup.sign", draw: ph::storefront, surface: plane(ph::SIGN), aspect: 5.4, anchor: 0.5, multiply: true, shade: 0.4, push: 0.0 },
    Template { id: "van", key: "mockup.van", draw: ph::van, surface: plane(ph::VAN), aspect: 1.95, anchor: 0.5, multiply: true, shade: 0.5, push: 0.0 },
    Template { id: "cards", key: "mockup.cards", draw: ph::cards, surface: plane(ph::CARD), aspect: 1.55, anchor: 0.5, multiply: true, shade: 0.3, push: 0.0 },
    Template { id: "flyer", key: "mockup.flyer", draw: ph::flyer, surface: plane(ph::FLYER), aspect: 0.707, anchor: 0.5, multiply: true, shade: 0.4, push: 0.0 },
    Template { id: "flag", key: "mockup.flag", draw: ph::flag, surface: Surface::Flag { corners: ph::FLAG }, aspect: 1.5, anchor: 0.5, multiply: true, shade: 0.8, push: 2.0 },
    Template { id: "phone", key: "mockup.phone", draw: ph::phone, surface: plane(ph::PHONE), aspect: 0.64, anchor: 0.5, multiply: true, shade: 0.3, push: 0.0 },
    Template { id: "box", key: "mockup.box", draw: ph::product_box, surface: plane(ph::BOX_FRONT), aspect: 1.17, anchor: 0.5, multiply: true, shade: 0.4, push: 0.0 },
];

/// The template named `id`.
pub fn template(id: &str) -> Option<&'static Template> {
    TEMPLATES.iter().find(|t| t.id == id)
}

/// A template's photo, `w` pixels wide.
pub fn photo(t: &Template, w: u32) -> image::RgbaImage {
    let mut c = ph::Canvas::new(w);
    (t.draw)(&mut c);
    c.image()
}

/// How the ink takes the product's colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Blend {
    /// As the template says: Multiply on light products.
    #[default]
    Auto,
    Multiply,
    Normal,
}

impl Blend {
    pub const ALL: [Blend; 3] = [Blend::Auto, Blend::Multiply, Blend::Normal];
    pub fn key(self) -> &'static str {
        match self {
            Blend::Auto => "docker.mockup_blend_auto",
            Blend::Multiply => "docker.mockup_blend_multiply",
            Blend::Normal => "docker.mockup_blend_normal",
        }
    }
}

/// The Mockup docker's settings.
#[derive(Debug, Clone, PartialEq)]
pub struct MockupSettings {
    /// A template's id, or [`OWN_PHOTO`].
    pub template: String,
    /// The design's size, a fraction of the print area (0.1..1).
    pub size: f64,
    /// Moves the design across and down the print area, fractions of it
    /// (-0.5..0.5).
    pub offset: (f64, f64),
    /// The photo's shading on the print, a factor of the template's (0..2).
    pub shading: f64,
    /// The photo's texture pushing the print, a factor of the template's (0..3).
    pub texture: f64,
    pub blend: Blend,
    /// The caption at the foot of the approval JPEG; empty: none.
    pub caption: String,
    /// The approval JPEG's width in pixels.
    pub jpeg_width: u32,
}

/// The template id that uses the selected picture as the photo.
pub const OWN_PHOTO: &str = "own-photo";

impl Default for MockupSettings {
    fn default() -> Self {
        MockupSettings {
            template: TEMPLATES[0].id.into(),
            size: 0.85,
            offset: (0.0, 0.0),
            shading: 1.0,
            texture: 1.0,
            blend: Blend::Auto,
            caption: String::new(),
            jpeg_width: PHOTO_PX,
        }
    }
}

/// The photo of a mockup on the page: its picture and where its grid
/// lands (grid unit to page mm, y up).
struct Placed {
    img: image::RgbaImage,
    rect: Rect,
}

impl Placed {
    /// The page point of a photo grid point.
    fn page(&self, (x, y): (f32, f32)) -> Point {
        Point::new(
            self.rect.x0 + f64::from(x) / f64::from(ph::REF_W) * self.rect.width(),
            self.rect.y1 - f64::from(y) / f64::from(ph::REF_H) * self.rect.height(),
        )
    }

    /// Page mm per photo grid unit.
    fn unit(&self) -> f64 {
        self.rect.width() / f64::from(ph::REF_W)
    }

    /// The photo's lightness 0..1 at a page point (`None` off the photo).
    fn lightness(&self, p: Point) -> Option<f64> {
        let (w, h) = self.img.dimensions();
        let u = (p.x - self.rect.x0) / self.rect.width() * f64::from(w);
        let v = (self.rect.y1 - p.y) / self.rect.height() * f64::from(h);
        if !(u >= 0.0 && v >= 0.0 && u < f64::from(w) && v < f64::from(h)) {
            return None;
        }
        let px = self.img.get_pixel(u as u32, v as u32);
        let ch = |k: usize| f64::from(px[k]) / 255.0;
        Some(tracedraw_core::live::blend_if_lightness(ch(0), ch(1), ch(2)))
    }

    /// The Displace map of the photo over the page box `b` (the design
    /// sits on the page, its own space the page's).
    fn surface_map(&self, b: Rect) -> Option<SurfaceMap> {
        if !(b.width() > 0.0 && b.height() > 0.0) {
            return None;
        }
        let cell = tracedraw_core::surface_map::CELL_MM;
        let max = f64::from(tracedraw_core::surface_map::MAX_CELLS);
        let k = (max / (b.width().max(b.height()) / cell)).min(1.0);
        let cols = ((b.width() / cell * k).ceil()).clamp(2.0, max) as u32;
        let rows = ((b.height() / cell * k).ceil()).clamp(2.0, max) as u32;
        let mut light = Vec::with_capacity((cols * rows) as usize);
        let (mut sum, mut n) = (0.0, 0.0);
        for r in 0..rows {
            for c in 0..cols {
                let p = Point::new(
                    b.x0 + (f64::from(c) + 0.5) / f64::from(cols) * b.width(),
                    b.y0 + (f64::from(r) + 0.5) / f64::from(rows) * b.height(),
                );
                let l = self.lightness(p).unwrap_or(0.5);
                sum += l;
                n += 1.0;
                light.push((l * 255.0).round().clamp(0.0, 255.0) as u8);
            }
        }
        Some(SurfaceMap {
            rect: b,
            cols,
            rows,
            light,
            mean: if n > 0.0 { sum / n } else { 0.5 },
            to_local: [1.0, 0.0, 0.0, 1.0],
        })
    }
}

/// What a mockup is made of before it goes into the document.
pub struct Built {
    pub group: Shape,
    /// The shapes it replaces.
    pub replaced: Vec<ShapeId>,
}

/// Every shape of `ids` that lies in `doc`, top-level copies with their
/// page placement (a shape inside a group takes the group's transform),
/// in drawing order bottom to top.
fn take_shapes(doc: &tracedraw_core::Document, ids: &[ShapeId]) -> Vec<Shape> {
    fn walk(s: &Shape, ids: &[ShapeId], out: &mut Vec<ShapeId>) {
        if ids.contains(&s.id) {
            out.push(s.id);
            return;
        }
        if let ShapeKind::Group { children } = &s.kind {
            for c in children {
                walk(c, ids, out);
            }
        }
    }
    let mut order = Vec::new();
    for page in &doc.pages {
        for layer in &page.layers {
            for s in &layer.shapes {
                walk(s, ids, &mut order);
            }
        }
    }
    order
        .into_iter()
        .filter_map(|id| {
            let s = doc.find_shape(id)?;
            let parent = doc.parent_transform(id).unwrap_or(Affine::IDENTITY);
            let mut c = s.clone();
            c.transform = parent * s.transform;
            Some(c)
        })
        .collect()
}

/// The design and the photo of an existing mockup `m` (a group with
/// [`DATA_KEY`]): the design's objects with their page placement, and
/// the photo's place on the page. `None` when it is not one.
fn unpack(doc: &tracedraw_core::Document, m: &Shape) -> Option<(Vec<Shape>, Rect, Option<Shape>)> {
    m.data.iter().find(|(k, _)| k == DATA_KEY)?;
    let ShapeKind::Group { children } = &m.kind else {
        return None;
    };
    let parent = doc.parent_transform(m.id).unwrap_or(Affine::IDENTITY) * m.transform;
    let photo = children.iter().find(|c| matches!(c.kind, ShapeKind::Bitmap { .. }))?;
    let design = children.iter().find(|c| matches!(c.kind, ShapeKind::Group { .. }))?;
    let ShapeKind::Group { children: parts } = &design.kind else {
        return None;
    };
    let t = parent * design.transform;
    let parts = parts
        .iter()
        .map(|p| {
            let mut c = p.clone();
            c.transform = t * p.transform;
            c
        })
        .collect();
    let mut own = photo.clone();
    own.transform = parent * photo.transform;
    let rect = (parent * photo.transform).transform_rect_bbox(photo.bounds_local());
    Some((parts, rect, Some(own)))
}

trait LocalBounds {
    fn bounds_local(&self) -> Rect;
}

impl LocalBounds for Shape {
    fn bounds_local(&self) -> Rect {
        match &self.kind {
            ShapeKind::Bitmap { rect, .. } => *rect,
            _ => self.local_path().bounding_box(),
        }
    }
}

/// The mesh, Displace and transparency that lay a design whose page box
/// is `b` on `surface` of the photo `placed`.
fn design_effects(
    b: Rect,
    placed: &Placed,
    surface: &Surface,
    aspect: f64,
    anchor: f64,
    s: &MockupSettings,
    shade: f64,
    push: f64,
    multiply: bool,
) -> Vec<Effect> {
    let size = s.size.clamp(0.05, 1.0);
    let d = if b.height() > 1e-9 { b.width() / b.height() } else { 1.0 };
    let (uw, vh) = if d > aspect {
        (size, size * aspect / d)
    } else {
        (size * d / aspect, size)
    };
    let (ox, oy) = (s.offset.0.clamp(-0.5, 0.5), s.offset.1.clamp(-0.5, 0.5));
    let u0 = ((1.0 - uw) / 2.0 + ox).clamp(0.0, 1.0 - uw);
    let v0 = ((1.0 - vh) * anchor.clamp(0.0, 1.0) + oy).clamp(0.0, 1.0 - vh);
    let n = MESH;
    let mut points = Vec::with_capacity(usize::from(n + 1).pow(2));
    for r in 0..=n {
        for c in 0..=n {
            let u = u0 + uw * f64::from(c) / f64::from(n);
            // Row 0 is the box's bottom (y up), the area's v grows down.
            let v = v0 + vh * (1.0 - f64::from(r) / f64::from(n));
            points.push(placed.page(surface.at(u as f32, v as f32)));
        }
    }
    let mut out = vec![Effect::MeshEnvelope {
        cols: n,
        rows: n,
        points: points.clone(),
    }];
    let shade = (shade * s.shading).clamp(0.0, 1.0);
    let push = push * s.texture.clamp(0.0, 3.0) * placed.unit();
    let mut area = Rect::new(f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in &points {
        area = area.union_pt(*p);
    }
    if shade > 0.0 || push > 0.0 {
        if let Some(map) = placed.surface_map(area.inflate(push + 1.0, push + 1.0)) {
            out.push(Effect::Displace {
                map: Some(map),
                horizontal: push,
                vertical: push,
                shade: if shade > 0.0 {
                    SurfaceShade::Both
                } else {
                    SurfaceShade::None
                },
                strength: shade,
            });
        }
    }
    if multiply {
        out.push(Effect::Transparency {
            mask: crate::mixed::uniform_mask(0.0),
            merge: MergeMode::Multiply,
            target: 2,
        });
    }
    out
}

/// The photo's encoded bytes, as JPEG kept beside the PNG every part of
/// the program reads, so a saved drawing stays small.
fn encode_photo(img: &image::RgbaImage) -> Option<(tracedraw_core::SharedBytes, image::RgbaImage)> {
    let rgb = image::DynamicImage::ImageRgba8(img.clone()).to_rgb8();
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90)
        .encode_image(&rgb)
        .ok()?;
    let back = image::load_from_memory_with_format(&jpeg, image::ImageFormat::Jpeg)
        .ok()?
        .to_rgba8();
    let png: tracedraw_core::SharedBytes = crate::bitmap_fx::encode(&back)?.into();
    tracedraw_core::jpeg_source::remember(&png, jpeg.as_slice().into());
    Some((png, back))
}

impl App {
    /// The selected mockup group, if one is selected alone.
    pub fn selected_mockup(&self) -> Option<ShapeId> {
        let [id] = self.selection[..] else {
            return None;
        };
        let s = self.doc().find_shape(id)?;
        s.data.iter().any(|(k, _)| k == DATA_KEY).then_some(id)
    }

    /// Build the mockup of the selection with `s` (see the module notes):
    /// a new one from the selected objects, the same design again on
    /// another template or with other settings when a mockup is
    /// selected, or, with [`OWN_PHOTO`], the other selected objects on
    /// the selected picture. `Err` holds the i18n key of what is missing.
    pub fn build_mockup(&mut self, s: &MockupSettings) -> Result<Built, &'static str> {
        let doc = self.doc();
        let mut replaced = self.selection.clone();
        let existing = self
            .selected_mockup()
            .and_then(|id| doc.find_shape(id))
            .and_then(|m| unpack(doc, m));
        let (parts, old_rect, old_photo) = match existing {
            Some((parts, rect, photo)) => (parts, Some(rect), photo),
            None => (take_shapes(doc, &self.selection), None, None),
        };
        let own = s.template == OWN_PHOTO;
        // With the own photo: the topmost picture is the photo.
        let (parts, own_photo) = if own {
            let mut parts = parts;
            let at = parts.iter().rposition(|p| matches!(p.kind, ShapeKind::Bitmap { .. }));
            let photo = match (at, old_photo.clone()) {
                (Some(i), _) => Some(parts.remove(i)),
                (None, Some(p)) => Some(p),
                _ => None,
            };
            (parts, photo)
        } else {
            (parts, None)
        };
        if parts.is_empty() {
            return Err("status.mockup_select_design");
        }
        if own && own_photo.is_none() {
            return Err("status.mockup_select_photo");
        }
        let page = self.page_rect();
        let template = template(&s.template);
        // The photo: drawn, or the user's own picture.
        let (photo_shape, placed) = if let Some(p) = own_photo {
            let ShapeKind::Bitmap { png, .. } = &p.kind else {
                return Err("status.mockup_select_photo");
            };
            let img = image::load_from_memory(png)
                .map_err(|_| "status.mockup_select_photo")?
                .to_rgba8();
            let rect = p.transform.transform_rect_bbox(p.bounds_local());
            (p, Placed { img, rect })
        } else {
            let t = template.unwrap_or(&TEMPLATES[0]);
            let img = photo(t, PHOTO_PX);
            let (png, img) = encode_photo(&img).ok_or("status.mockup_select_design")?;
            let rect = old_rect.unwrap_or_else(|| {
                let k = (page.width() * 0.9 / f64::from(ph::REF_W))
                    .min(page.height() * 0.9 / f64::from(ph::REF_H));
                let (w, h) = (f64::from(ph::REF_W) * k, f64::from(ph::REF_H) * k);
                let c = page.center();
                Rect::new(c.x - w / 2.0, c.y - h / 2.0, c.x + w / 2.0, c.y + h / 2.0)
            });
            let mut shape = Shape::new(
                self.engine.new_shape_id(),
                ShapeKind::Bitmap {
                    rect,
                    width_px: img.width(),
                    height_px: img.height(),
                    png,
                    fx: None,
                },
            );
            shape.stroke = None;
            shape.name = Some(crate::i18n::tr(t.key));
            (shape, Placed { img, rect })
        };
        // The print area, the shading and the blend.
        let (surface, aspect, anchor, multiply_default, shade, push, name) = match template {
            Some(t) if !own => (
                t.surface,
                f64::from(t.aspect),
                f64::from(t.anchor),
                t.multiply,
                t.shade,
                t.push,
                crate::i18n::tr(t.key),
            ),
            _ => {
                let i = 0.2;
                let c = [(i, i), (1.0 - i, i), (1.0 - i, 1.0 - i), (i, 1.0 - i)]
                    .map(|(x, y)| (x * ph::REF_W, y * ph::REF_H));
                let (w, h) = (placed.rect.width(), placed.rect.height());
                (
                    Surface::Plane { corners: c, bulge: 0.0 },
                    if h > 0.0 { w / h } else { 1.0 },
                    0.5,
                    false,
                    0.6,
                    1.5,
                    crate::i18n::tr("mockup.own_photo"),
                )
            }
        };
        let multiply = match s.blend {
            Blend::Auto => multiply_default,
            Blend::Multiply => true,
            Blend::Normal => false,
        };
        let mut design = Shape::new(self.engine.new_shape_id(), ShapeKind::Group { children: parts });
        design.fill = Fill::None;
        design.stroke = None;
        design.name = Some(crate::i18n::tr("mockup.design"));
        let b = design.local_path().bounding_box();
        if !(b.width() > 0.0 || b.height() > 0.0) {
            return Err("status.mockup_select_design");
        }
        design.effects = design_effects(b, &placed, &surface, aspect, anchor, s, shade, push, multiply);
        let mut group = Shape::new(
            self.engine.new_shape_id(),
            ShapeKind::Group {
                children: vec![photo_shape, design],
            },
        );
        group.fill = Fill::None;
        group.stroke = None;
        group.name = Some(crate::i18n::trf("mockup.group_name", &[("name", &name)]));
        group.data = vec![(DATA_KEY.into(), s.template.clone())];
        replaced.retain(|id| self.doc().find_shape(*id).is_some());
        Ok(Built { group, replaced })
    }

    /// Effects > Mockup's Apply: the mockup in place of the selection,
    /// one undo step, and selected.
    pub fn apply_mockup(&mut self, s: &MockupSettings) {
        let built = match self.build_mockup(s) {
            Ok(b) => b,
            Err(key) => {
                self.status = crate::i18n::tr(key);
                return;
            }
        };
        let Some(layer) = self.active_layer() else {
            return;
        };
        let id = built.group.id;
        let cmds = [
            Command::DeleteShapes {
                shapes: built.replaced,
            },
            Command::AddShape {
                layer,
                shape: built.group,
            },
        ];
        if let Err(e) = self.engine.run_batch("Mockup", &cmds) {
            self.status = e.to_string();
            return;
        }
        self.select(vec![id]);
    }

    /// The selected mockup (or, with none, the selection's box) drawn as
    /// a JPEG `width` pixels wide for a client's approval, with
    /// `caption` in a band at its foot when there is one.
    pub fn mockup_jpeg(&self, width: u32, caption: &str) -> Option<Vec<u8>> {
        let doc = self.doc();
        let area = match self.selected_mockup().and_then(|id| doc.find_shape(id)) {
            Some(m) => {
                let parent = doc.parent_transform(m.id).unwrap_or(Affine::IDENTITY) * m.transform;
                let ShapeKind::Group { children } = &m.kind else {
                    return None;
                };
                let photo = children.iter().find(|c| matches!(c.kind, ShapeKind::Bitmap { .. }))?;
                (parent * photo.transform).transform_rect_bbox(photo.bounds_local())
            }
            None => {
                let mut r: Option<Rect> = None;
                for id in &self.selection {
                    if let Some(b) = doc.find_shape(*id).map(|s| s.bounds()) {
                        r = Some(r.map_or(b, |a| a.union(b)));
                    }
                }
                r?
            }
        };
        if !(area.width() > 0.0 && area.height() > 0.0) {
            return None;
        }
        let width = width.clamp(200, 6000);
        let zoom = f64::from(width) / area.width();
        let photo_h = (area.height() * zoom).round().max(1.0) as u32;
        let band = if caption.trim().is_empty() {
            0
        } else {
            (f64::from(photo_h) * 0.08).round().max(24.0) as u32
        };
        let mut scratch = doc.clone();
        let page = scratch.pages.iter().find(|p| p.id == self.page)?.id;
        let full = Rect::new(area.x0, area.y0 - f64::from(band) / zoom, area.x1, area.y1);
        if band > 0 {
            let bh = f64::from(band) / zoom;
            let strip = Rect::new(area.x0, area.y0 - bh, area.x1, area.y0);
            let mut bar = Shape::new(scratch.ids_mut().shape(), ShapeKind::Rect { rect: strip, radius: 0.0, corners: None });
            bar.fill = Fill::Solid(Color::rgb8(255, 255, 255));
            bar.stroke = None;
            // Text sized to the band: about half its height, in points.
            let size_pt = bh * 0.45 / 0.3528;
            let mut text = Shape::new(
                scratch.ids_mut().shape(),
                ShapeKind::Text {
                    spans: vec![tracedraw_core::TextSpan::new(caption.trim(), self.text_font.clone(), size_pt)],
                    origin: Point::new(strip.x0 + bh * 0.4, strip.y0 + bh * 0.33),
                    frame: None,
                    align: Default::default(),
                    para: Default::default(),
                    on_path: None,
                },
            );
            text.fill = Fill::Solid(Color::rgb8(40, 40, 40));
            text.stroke = None;
            let p = scratch.pages.iter_mut().find(|p| p.id == page)?;
            let layer = p.layers.last_mut()?;
            layer.visible = true;
            layer.shapes.push(bar);
            layer.shapes.push(text);
        }
        let (w, h) = (width, photo_h + band);
        let opts = tracedraw_render::RenderOptions {
            width: w,
            height: h,
            view: tracedraw_render::ViewTransform {
                zoom,
                origin_x: -full.x0 * zoom,
                origin_y: full.y1 * zoom,
                stretch_y: 1.0,
            },
            ..Default::default()
        };
        let px = tracedraw_render::render_page(&scratch, page, &opts)?;
        let mut img = image::RgbImage::new(w, h);
        for (o, p) in img.pixels_mut().zip(px.pixels()) {
            let c = p.demultiply();
            let a = f64::from(c.alpha()) / 255.0;
            // Over white paper.
            let ch = |v: u8| (f64::from(v) * a + 255.0 * (1.0 - a)).round() as u8;
            *o = image::Rgb([ch(c.red()), ch(c.green()), ch(c.blue())]);
        }
        let mut out = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 92)
            .encode_image(&img)
            .ok()?;
        Some(out)
    }

    /// Effects > Mockup's Export JPG: the approval picture saved where
    /// the user says.
    pub fn export_mockup_jpeg(&mut self, s: &MockupSettings) {
        let Some(bytes) = self.mockup_jpeg(s.jpeg_width, &s.caption) else {
            self.status = crate::i18n::tr("status.mockup_select_design");
            return;
        };
        crate::files::Dialog::new()
            .add_filter("JPEG", &["jpg", "jpeg"])
            .set_file_name("mockup.jpg")
            .save_bytes(self, bytes);
    }
}
