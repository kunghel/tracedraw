//! Live (non-destructive) effects. A shape carries a stack of `Effect`s
//! that are evaluated whenever it is drawn or exported; the source
//! geometry is never changed. `evaluate` returns the shapes to draw
//! beneath the object, the object itself (possibly with its geometry
//! warped), and the shapes to draw above it. "Break Effect Apart" turns
//! the result into ordinary objects.

use crate::document::{Shape, ShapeKind};
use crate::geometry::{Affine, BezPath, PathEl, Point, Rect, Shape as _, Vec2};
use crate::style::{lerp_color, Fill};
// The colour blend lives with the fills now (fountain fills use it too,
// #1208); it keeps its place here for the code that knows it as a blend
// option.
pub use crate::style::{blend_color, ColorBlend};
use crate::Color;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "effect", rename_all = "lowercase")]
pub enum Effect {
    /// Concentric copies inside or outside the outline.
    Contour {
        steps: u32,
        offset: f64,
        /// true = outside, false = inside; `to_center` ignores `steps`.
        outside: bool,
        to_center: bool,
        /// Steps on both sides of a closed outline at once, as many
        /// inside as outside, with the same spacing and colours (#3085);
        /// `outside` and `to_center` are then not read. Older files have
        /// none (one side).
        #[serde(default, skip_serializing_if = "is_false")]
        both: bool,
        fill_to: Color,
        outline_to: Option<Color>,
        /// The last step's colour for the end of a fountain fill (#1265):
        /// a fountain-filled object's steps keep its fountain, its first
        /// colour going to `fill_to` and its last to this one. None (older
        /// files): `fill_to` for both.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fill_end_to: Option<Color>,
        /// 0 = direct RGB, 1 = clockwise HSB, 2 = counter-clockwise HSB.
        color_blend: u8,
        /// How the steps turn their corners; older files have none (round).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        corners: Option<crate::shaping::Join>,
        /// Acceleration of the steps' spacing and of their colours, -1 to
        /// 1 (0: even).
        #[serde(default, skip_serializing_if = "is_zero")]
        accel_objects: f64,
        #[serde(default, skip_serializing_if = "is_zero")]
        accel_colors: f64,
    },
    /// Intermediate objects between this shape and `end`.
    Blend {
        end: Box<Shape>,
        steps: u32,
        /// Acceleration -1..1 for object spacing and colours.
        accel_objects: f64,
        accel_colors: f64,
        /// Extra rotation of the intermediates, degrees.
        rotation: f64,
        /// Optional path the blend follows (page space).
        path: Option<BezPath>,
        rotate_on_path: bool,
        /// The end object, when it is an object of the drawing: `end` is
        /// then a copy of it kept up to date (in page space), and the
        /// object draws itself. Older files have none (the end lives only
        /// in the effect and is drawn with the steps).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        end_id: Option<crate::ShapeId>,
        /// Spacing, loop, full path, sizing acceleration and the way the
        /// colours turn; older files have the defaults.
        #[serde(default, skip_serializing_if = "BlendOptions::is_default")]
        options: BlendOptions,
    },
    /// Geometry distortions (push/pull, zipper, twister).
    Distort(crate::effects::Distort),
    /// Envelope: a frame of nodes in local space (see
    /// [`crate::envelope`]) the shape's bounding box is mapped into.
    Envelope {
        nodes: Vec<Point>,
        keep_lines: bool,
        /// The handles of the segment from each node to the next; older
        /// files have none (each side is then the arc through its nodes).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        handles: Vec<[Point; 2]>,
        /// The corner nodes (bottom-left, bottom-right, top-right,
        /// top-left); older files have none (nodes 0, 2, 4 and 6 of 8).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        corners: Vec<usize>,
        #[serde(default)]
        mapping: crate::envelope::Mapping,
    },
    /// Perspective: the four corners of the bounding box moved.
    Perspective { corners: [Point; 4] },
    /// Extrusion with optional vanishing point and shading.
    Extrude {
        depth: Vec2,
        /// Vanishing point in page space; None = parallel extrusion.
        vanishing: Option<Point>,
        /// 0..1 fraction toward the vanishing point for the back face.
        amount: f64,
        shade_from: Option<Color>,
        shade_to: Option<Color>,
        light_angle: f64,
        light_intensity: f64,
        bevel: f64,
        /// How the body runs; older files have none (Back parallel, or
        /// Small back with a vanishing point). See [`crate::extrude`].
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kind: Option<crate::extrude::ExtrudeKind>,
        /// The vanishing point is in the object's own space (it moves with
        /// the object) instead of the page's.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        on_object: bool,
        /// One colour for the whole body.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        solid: Option<Color>,
        /// Turn in space, the first light's elevation and further lights
        /// (see [`crate::extrude3d`]); `bevel` is the width of the near
        /// cap's bevel in mm.
        #[serde(default, skip_serializing_if = "crate::extrude3d::Solid::is_plain")]
        space: crate::extrude3d::Solid,
    },
    /// Bevel: a band of `distance` lit from `light_angle`, inside the
    /// outline (soft edge) or around it (emboss).
    Bevel {
        distance: f64,
        light_angle: f64,
        intensity: f64,
        /// Soft edge (0, the band inside the outline), emboss (1, the
        /// band around it, so the object stands out of the page) or
        /// centre (2, the band half inside and half outside, #1144).
        style: u8,
        shadow_color: Color,
        light_color: Color,
        /// How many steps the band is cut into: 1 is two flat tones, more
        /// fade the light and the shade across the band, strongest at its
        /// outer edge. Older files have 1.
        #[serde(default = "one_level")]
        level: u8,
        /// The colour the band is toned from, in place of the object's
        /// own fill; older files use the fill.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        emboss_color: Option<Color>,
        /// The light falls round the band by direction: each part of
        /// the band as lit as it faces the light, fading from lit to
        /// shaded, instead of two halves cut through the middle (#1144).
        /// Older files have the halves.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        directional: bool,
    },
    /// Block shadow: a solid extrusion behind the object.
    BlockShadow {
        offset: Vec2,
        color: Color,
        /// Keep a gap between the object and the shadow (overprint trick).
        gap: f64,
    },
    /// Inner shadow: a soft shadow cast inside the object, as if the
    /// object were a hole in the page lit from the other side of
    /// `offset` (a shadow falls inside along the edges facing away from
    /// it). Drawn by the renderer over the object, cut to its shape
    /// (#2473).
    InnerShadow {
        offset: Vec2,
        color: Color,
        /// 0..1
        opacity: f64,
        /// Feathering, millimetres.
        blur: f64,
        merge: MergeMode,
    },
    /// Symmetry: mirrored copies of the object across `lines` mirror lines
    /// through `center` (page space), the first at `angle` degrees and the
    /// others spread evenly; the object itself stays where it is. With
    /// `rotate` the copies are turned round the centre instead, `lines`
    /// in all counting the object, and none is a mirror image (#2999).
    Symmetry {
        center: Point,
        angle: f64,
        lines: u8,
        #[serde(default, skip_serializing_if = "is_false")]
        rotate: bool,
    },
    /// Offset Path: the outline moved out (positive `distance`, mm) or in
    /// (negative), its corners turned as `join` says, then smoothed by
    /// `smooth` mm (grown and shrunk back by that much with round
    /// corners, which rounds the hollows where parts meet). On a group
    /// with `merge` the group's objects first become one outline, drawn
    /// with the paint of the bottom object, so shapes near each other
    /// flow into one another as they move (metaballs). The effect can be
    /// stacked; each one works on what the ones before it made.
    #[serde(rename = "offsetpath")]
    OffsetPath {
        distance: f64,
        #[serde(default)]
        smooth: f64,
        #[serde(default)]
        join: crate::shaping::Join,
        #[serde(default, skip_serializing_if = "is_false")]
        merge: bool,
    },
    /// Transform: `copies` copies of the object, each one the one before
    /// it scaled by `scale_x` / `scale_y` percent, mirrored (`reflect_x`
    /// turns it over left to right, `reflect_y` top to bottom), turned by
    /// `angle` degrees about the centre of the object's box (or the point
    /// `pivot_x` / `pivot_y` mm from it, for copies round a centre of
    /// one's own, #3607) and moved by `move_x` / `move_y` mm. With `random`, each step takes a share of
    /// those values picked from `seed`. With no copies the object itself
    /// is drawn transformed. The outline keeps its width unless
    /// `scale_strokes`. The object itself stays as it is, so the result
    /// stays editable; Break Effect Apart makes the copies objects.
    Transform {
        scale_x: f64,
        scale_y: f64,
        #[serde(default)]
        move_x: f64,
        #[serde(default)]
        move_y: f64,
        #[serde(default)]
        angle: f64,
        copies: u32,
        #[serde(default, skip_serializing_if = "is_false")]
        reflect_x: bool,
        #[serde(default, skip_serializing_if = "is_false")]
        reflect_y: bool,
        #[serde(default, skip_serializing_if = "is_false")]
        random: bool,
        #[serde(default)]
        seed: u32,
        #[serde(default, skip_serializing_if = "is_false")]
        scale_strokes: bool,
        #[serde(default, skip_serializing_if = "is_zero")]
        pivot_x: f64,
        #[serde(default, skip_serializing_if = "is_zero")]
        pivot_y: f64,
    },
    /// Grid: the object repeated in `columns` × `rows` cells, to the right
    /// and down from the object, which is the top left cell. Each cell is
    /// the object's box with `gap_x` / `gap_y` mm between cells (less than
    /// nothing overlaps them); every other row moves right by `shift_x`
    /// mm and every other column down by `shift_y` mm, for brick and
    /// honeycomb layouts. The object stays as it is and editable.
    Grid {
        columns: u32,
        rows: u32,
        #[serde(default)]
        gap_x: f64,
        #[serde(default)]
        gap_y: f64,
        #[serde(default, skip_serializing_if = "is_zero")]
        shift_x: f64,
        #[serde(default, skip_serializing_if = "is_zero")]
        shift_y: f64,
    },
    /// Outer shadow (#1141): a soft shadow cast beneath the object by its
    /// shape, moved by `offset`; one of a stack of them, beside the
    /// object's own drop shadow. Drawn by the renderer.
    OuterShadow {
        offset: Vec2,
        color: Color,
        /// 0..1
        opacity: f64,
        /// Feathering, millimetres.
        blur: f64,
        merge: MergeMode,
    },
    /// Glow (#1141): a soft light in `color` round the edge, spreading
    /// `blur` mm outside the object, or inside it (`inner`). Drawn by the
    /// renderer, as an outer shadow or an inner shadow that does not move.
    Glow {
        inner: bool,
        color: Color,
        /// 0..1
        opacity: f64,
        /// Spread, millimetres.
        blur: f64,
        merge: MergeMode,
    },
    /// Colour overlay (#1141): the object's shape in one colour over it,
    /// `opacity` 0..1.
    ColorOverlay { color: Color, opacity: f64 },
    /// Gradient overlay (#1141): the object's shape filled with a linear
    /// gradient from `from` to `to` at `angle` degrees over it.
    GradientOverlay {
        from: Color,
        to: Color,
        angle: f64,
        opacity: f64,
    },
    /// Sprayer (#3435): objects of one of the sprayer's shapes (`preset`:
    /// circles, stars, squares, hearts, leaves) in `color` along the
    /// object's path, about `size` mm across and `spacing` mm apart, drawn
    /// above it as one group. The path stays editable and the objects
    /// follow it; Break Effect Apart leaves the group.
    Spray {
        preset: u8,
        size: f64,
        spacing: f64,
        color: Color,
    },
    /// Tiles (the Brush Strokes tool's Tiles mode): one of the tile
    /// pictures (`preset`: chain, rope, zigzag, dash and dot, waves,
    /// beads) repeated end to end along the object's path and bent to
    /// follow it, `width` mm across, in `color`, drawn above it as one
    /// group (see [`crate::tiles`]).
    Tiles { preset: u8, width: f64, color: Color },
    /// Live paint (the Smart Fill tool with Live ticked), on a group:
    /// each fill a point in the group's own space with its colour; the
    /// area the group's lines close around each point is drawn filled
    /// under its objects, following the lines as they change (see
    /// [`crate::live_paint`]).
    LivePaint { fills: Vec<(f64, f64, Color)> },
    /// Gaussian blur (#1141): the object drawn blurred by `radius` mm.
    /// Drawn by the renderer.
    GaussianBlur { radius: f64 },
    /// Fill opacity (#1141): the object itself drawn at `opacity` 0..1
    /// while what its other effects draw keeps its own.
    FillOpacity { opacity: f64 },
    /// Blend If: the object shows only where its own lightness (`this`)
    /// and the lightness of what lies beneath it (`below`) fall in a
    /// range. Each range is four lightnesses 0..1: where the dark end
    /// starts to show and where it shows fully, where the light end starts
    /// to fade and where it is gone (see [`blend_if_weight`]). Drawn by
    /// the renderer, so a light text on a photo takes the photo's grain.
    BlendIf { this: [f64; 4], below: [f64; 4] },
    /// Displace by a photo (#3595): the outline pushed by the lightness of
    /// a photo beneath it (light up and right, dark down and left, by up
    /// to `horizontal` and `vertical` mm on the page), so a print follows
    /// the folds of a cloth, the grain of a paper or a wall; `shade` then
    /// takes the photo's shadows (Multiply) and highlights (Screen) onto
    /// it, by `strength` 0..1, drawn by the renderer from what lies
    /// beneath. The map is the photo sampled when the effect was set.
    Displace {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        map: Option<SurfaceMap>,
        horizontal: f64,
        vertical: f64,
        #[serde(default)]
        shade: SurfaceShade,
        #[serde(default)]
        strength: f64,
    },
    /// Lens over everything beneath (evaluated by the renderer).
    Lens(Lens),
    /// Non-uniform transparency: the fill's luminance is the opacity.
    Transparency {
        mask: Fill,
        merge: MergeMode,
        /// Apply to fill, outline or both (0, 1, 2).
        target: u8,
    },
}

/// How a transparency (or a drop shadow) mixes with what lies beneath.
/// The names in `.tdraw` files are the variants in lower case
/// (`softlight`, `colordodge`); `docs/behavior/transparency.md` has the
/// formulas the renderer uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum MergeMode {
    #[default]
    Normal,
    Add,
    Subtract,
    Difference,
    Multiply,
    Divide,
    IfLighter,
    IfDarker,
    /// The backdrop darkened by the source's lightness (a grey multiply).
    Texturize,
    Hue,
    Saturation,
    Color,
    /// The source's HSL lightness with the backdrop's hue and saturation.
    Lightness,
    Invert,
    And,
    Or,
    Xor,
    Red,
    Green,
    Blue,
    /// The source shows only where the backdrop is clear.
    Behind,
    Screen,
    Overlay,
    SoftLight,
    HardLight,
    ColorDodge,
    ColorBurn,
    Exclusion,
}

impl MergeMode {
    pub const ALL: [MergeMode; 28] = [
        MergeMode::Normal,
        MergeMode::Add,
        MergeMode::Subtract,
        MergeMode::Difference,
        MergeMode::Multiply,
        MergeMode::Divide,
        MergeMode::IfLighter,
        MergeMode::IfDarker,
        MergeMode::Texturize,
        MergeMode::Hue,
        MergeMode::Saturation,
        MergeMode::Color,
        MergeMode::Lightness,
        MergeMode::Invert,
        MergeMode::And,
        MergeMode::Or,
        MergeMode::Xor,
        MergeMode::Red,
        MergeMode::Green,
        MergeMode::Blue,
        MergeMode::Behind,
        MergeMode::Screen,
        MergeMode::Overlay,
        MergeMode::SoftLight,
        MergeMode::HardLight,
        MergeMode::ColorDodge,
        MergeMode::ColorBurn,
        MergeMode::Exclusion,
    ];
    pub fn name(self) -> &'static str {
        match self {
            MergeMode::Normal => "Normal",
            MergeMode::Add => "Add",
            MergeMode::Subtract => "Subtract",
            MergeMode::Difference => "Difference",
            MergeMode::Multiply => "Multiply",
            MergeMode::Divide => "Divide",
            MergeMode::IfLighter => "If Lighter",
            MergeMode::IfDarker => "If Darker",
            MergeMode::Texturize => "Texturize",
            MergeMode::Hue => "Hue",
            MergeMode::Saturation => "Saturation",
            MergeMode::Color => "Color",
            MergeMode::Lightness => "Lightness",
            MergeMode::Invert => "Invert",
            MergeMode::And => "And",
            MergeMode::Or => "Or",
            MergeMode::Xor => "Xor",
            MergeMode::Red => "Red",
            MergeMode::Green => "Green",
            MergeMode::Blue => "Blue",
            MergeMode::Behind => "Behind",
            MergeMode::Screen => "Screen",
            MergeMode::Overlay => "Overlay",
            MergeMode::SoftLight => "Soft Light",
            MergeMode::HardLight => "Hard Light",
            MergeMode::ColorDodge => "Color Dodge",
            MergeMode::ColorBurn => "Color Burn",
            MergeMode::Exclusion => "Exclusion",
        }
    }
}

/// The lens types.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "lens", rename_all = "lowercase")]
pub enum Lens {
    Brighten {
        rate: f64,
    },
    ColorAdd {
        color: Color,
        rate: f64,
    },
    ColorLimit {
        color: Color,
        rate: f64,
    },
    CustomColorMap {
        from: Color,
        to: Color,
        direction: u8,
    },
    FishEye {
        rate: f64,
    },
    HeatMap {
        rotation: f64,
    },
    Invert,
    Magnify {
        amount: f64,
    },
    TintedGrayscale {
        color: Color,
    },
    Transparency {
        rate: f64,
        color: Color,
    },
    Wireframe {
        outline: Color,
        fill: Color,
    },
    /// Bitmap effects applied to whatever lies beneath, in order (the
    /// application supplies them to the renderer; see
    /// `tracedraw_render::set_pixel_effects`).
    Effects {
        effects: Vec<crate::BitmapEffect>,
    },
    /// Cut: the objects beneath are cut across along the lens's middle
    /// line (its long axis), each side of every cut piece grows a curved
    /// spur along that line, and a shadow fades from the cut over the
    /// objects on one side, as far as the lens reaches. Sizes in mm:
    /// `spur_length` how far a spur reaches out of the object,
    /// `spur_width` its thickness where it leaves the object, `curve`
    /// how hollow its sides are (1 straight, higher thinner), `gap` the
    /// width of the cut; `shadow` 0 to 100 how dark the shadow starts,
    /// on the lens's lower side, or its upper side with `shadow_up`.
    Cut {
        spur_length: f64,
        spur_width: f64,
        #[serde(default = "two")]
        curve: f64,
        #[serde(default)]
        gap: f64,
        shadow: f64,
        shadow_color: Color,
        #[serde(default, skip_serializing_if = "is_false")]
        shadow_up: bool,
    },
}

fn two() -> f64 {
    2.0
}

impl Lens {
    /// The Cut lens as the Lens panel starts it.
    pub fn default_cut() -> Lens {
        Lens::Cut {
            spur_length: 3.0,
            spur_width: 3.0,
            curve: 2.0,
            gap: 0.3,
            shadow: 60.0,
            shadow_color: Color::BLACK,
            shadow_up: false,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Lens::Brighten { .. } => "Brighten",
            Lens::ColorAdd { .. } => "Color Add",
            Lens::ColorLimit { .. } => "Color Limit",
            Lens::CustomColorMap { .. } => "Custom Color Map",
            Lens::FishEye { .. } => "Fish Eye",
            Lens::HeatMap { .. } => "Heat Map",
            Lens::Invert => "Invert",
            Lens::Magnify { .. } => "Magnify",
            Lens::TintedGrayscale { .. } => "Tinted Grayscale",
            Lens::Transparency { .. } => "Transparency",
            Lens::Wireframe { .. } => "Wireframe",
            Lens::Effects { .. } => "Bitmap Effects",
            Lens::Cut { .. } => "Cut",
        }
    }

    /// Per-pixel colour function for the lenses that are pure colour maps
    /// (everything but fish eye, magnify and wireframe). Input and output
    /// are 0..1 RGB.
    pub fn map_color(&self, c: [f32; 3]) -> [f32; 3] {
        fn k(c: Color) -> [f32; 3] {
            let [r, g, b] = c.to_rgb8();
            [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0]
        }
        match self {
            Lens::Brighten { rate } => {
                let r = (*rate as f32 / 100.0).clamp(-1.0, 1.0);
                let f = |v: f32| {
                    if r >= 0.0 {
                        v + r * (1.0 - v)
                    } else {
                        v * (1.0 + r)
                    }
                };
                [f(c[0]), f(c[1]), f(c[2])]
            }
            Lens::ColorAdd { color, rate } => {
                let k = k(*color);
                let r = *rate as f32 / 100.0;
                [
                    (c[0] + r * k[0]).min(1.0),
                    (c[1] + r * k[1]).min(1.0),
                    (c[2] + r * k[2]).min(1.0),
                ]
            }
            Lens::ColorLimit { color, rate } => {
                let k = k(*color);
                let r = *rate as f32 / 100.0;
                [
                    c[0] * (1.0 - r) + r * c[0] * k[0],
                    c[1] * (1.0 - r) + r * c[1] * k[1],
                    c[2] * (1.0 - r) + r * c[2] * k[2],
                ]
            }
            Lens::CustomColorMap { from, to, .. } => {
                let y = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
                let a = k(*from);
                let b = k(*to);
                [
                    a[0] + (b[0] - a[0]) * y,
                    a[1] + (b[1] - a[1]) * y,
                    a[2] + (b[2] - a[2]) * y,
                ]
            }
            Lens::HeatMap { rotation } => {
                let y = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
                let t = (y + *rotation as f32 / 100.0).fract();
                // Blue, cyan, green, yellow, red, white.
                let stops: [[f32; 3]; 6] = [
                    [0.0, 0.0, 0.6],
                    [0.0, 0.8, 1.0],
                    [0.0, 0.8, 0.0],
                    [1.0, 1.0, 0.0],
                    [1.0, 0.0, 0.0],
                    [1.0, 1.0, 1.0],
                ];
                let f = t * 5.0;
                let i = (f.floor() as usize).min(4);
                let u = f - i as f32;
                let a = stops[i];
                let b = stops[i + 1];
                [
                    a[0] + (b[0] - a[0]) * u,
                    a[1] + (b[1] - a[1]) * u,
                    a[2] + (b[2] - a[2]) * u,
                ]
            }
            Lens::Invert => [1.0 - c[0], 1.0 - c[1], 1.0 - c[2]],
            Lens::TintedGrayscale { color } => {
                let y = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
                // The grey as a tint of the lens colour (#2800): black
                // becomes the colour, white stays white, so a black lens
                // gives plain grays. A white lens (what the panel gave
                // before) would whiten everything; it means plain grays.
                let k = if *color == Color::WHITE {
                    [0.0; 3]
                } else {
                    k(*color)
                };
                [
                    k[0] + (1.0 - k[0]) * y,
                    k[1] + (1.0 - k[1]) * y,
                    k[2] + (1.0 - k[2]) * y,
                ]
            }
            Lens::Transparency { rate, color } => {
                let k = k(*color);
                let r = *rate as f32 / 100.0;
                [
                    c[0] * (1.0 - r) + k[0] * r,
                    c[1] * (1.0 - r) + k[1] * r,
                    c[2] * (1.0 - r) + k[2] * r,
                ]
            }
            Lens::FishEye { .. }
            | Lens::Magnify { .. }
            | Lens::Wireframe { .. }
            | Lens::Effects { .. }
            | Lens::Cut { .. } => c,
        }
    }
}

impl Effect {
    /// Only reshapes the outline (Distort, Envelope, Perspective): Convert
    /// to Curves and Break Apart make it part of the curves (#647).
    pub fn warps_outline(&self) -> bool {
        matches!(
            self,
            Effect::Envelope { .. }
                | Effect::Perspective { .. }
                | Effect::Distort(_)
                | Effect::OffsetPath { .. }
                | Effect::Displace { .. }
        )
    }

    /// Draws objects made from the outline as it is at its place in the
    /// stack (contours, blends, extrusions, shadows, symmetry...): a warp
    /// after it changes nothing of what it drew, so it stays live. (An
    /// extrusion's body is built from the outline as the warps after it
    /// leave it, #2214; a bake still leaves those warps live.)
    fn draws_from_outline(&self) -> bool {
        !self.warps_outline()
            && !matches!(
                self,
                Effect::Lens(_)
                    | Effect::Transparency { .. }
                    | Effect::InnerShadow { .. }
                    | Effect::OuterShadow { .. }
                    | Effect::Glow { .. }
                    | Effect::GaussianBlur { .. }
                    | Effect::FillOpacity { .. }
                    | Effect::BlendIf { .. }
            )
    }

    /// Drawn by the renderer from the object as it is drawn, adding no
    /// objects of its own (#1141).
    pub fn drawn_by_renderer(&self) -> bool {
        matches!(
            self,
            Effect::Lens(_)
                | Effect::Transparency { .. }
                | Effect::InnerShadow { .. }
                | Effect::OuterShadow { .. }
                | Effect::Glow { .. }
                | Effect::GaussianBlur { .. }
                | Effect::BlendIf { .. }
        )
    }
}

/// How Displace takes the photo's light onto the object (#3595).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SurfaceShade {
    /// The object keeps its colours.
    #[default]
    None,
    /// Darkened where the photo is darker than its middle grey.
    Multiply,
    /// Lightened where the photo is lighter than its middle grey.
    Screen,
    /// Both.
    Both,
}

impl SurfaceShade {
    pub const ALL: [SurfaceShade; 4] = [
        SurfaceShade::None,
        SurfaceShade::Multiply,
        SurfaceShade::Screen,
        SurfaceShade::Both,
    ];
}

/// A photo's lightness sampled on a grid over a box of an object's own
/// space, for Displace (#3595): `light` holds `cols * rows` values 0..255,
/// row by row from the box's `y0` side, each the photo's lightness at
/// the middle of its cell; `mean` is the photo's average lightness 0..1
/// there (the grey that pushes nothing); `to_local` takes a page
/// direction to the object's own space (the inverse of its transform's
/// linear part when the photo was sampled).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceMap {
    pub rect: Rect,
    pub cols: u32,
    pub rows: u32,
    pub light: Vec<u8>,
    pub mean: f64,
    pub to_local: [f64; 4],
}

impl SurfaceMap {
    /// The lightness 0..1 at `p` (the object's own space), bilinear
    /// between the cells' middles and kept at the edge value outside.
    pub fn lightness(&self, p: Point) -> f64 {
        let (c, r) = (self.cols as usize, self.rows as usize);
        if c == 0 || r == 0 || self.light.len() < c * r {
            return self.mean;
        }
        let (w, h) = (self.rect.width(), self.rect.height());
        if !(w > 0.0 && h > 0.0) {
            return self.mean;
        }
        let fx = ((p.x - self.rect.x0) / w * c as f64 - 0.5).clamp(0.0, (c - 1) as f64);
        let fy = ((p.y - self.rect.y0) / h * r as f64 - 0.5).clamp(0.0, (r - 1) as f64);
        if !fx.is_finite() || !fy.is_finite() {
            return self.mean;
        }
        let (i, j) = (fx.floor() as usize, fy.floor() as usize);
        let (i1, j1) = ((i + 1).min(c - 1), (j + 1).min(r - 1));
        let (tx, ty) = (fx - i as f64, fy - j as f64);
        let at = |x: usize, y: usize| f64::from(self.light[y * c + x]) / 255.0;
        let top = at(i, j) * (1.0 - tx) + at(i1, j) * tx;
        let bottom = at(i, j1) * (1.0 - tx) + at(i1, j1) * tx;
        top * (1.0 - ty) + bottom * ty
    }

    /// Where Displace takes `p` (the object's own space): pushed along
    /// the page's right and up by `horizontal` and `vertical` mm times
    /// how far the photo there is from its middle grey (twice the
    /// difference, so full white over a mid grey photo pushes the whole
    /// amount).
    pub fn displace(&self, p: Point, horizontal: f64, vertical: f64) -> Point {
        let d = ((self.lightness(p) - self.mean) * 2.0).clamp(-1.0, 1.0);
        let (dx, dy) = (horizontal * d, vertical * d);
        let [a, b, c, e] = self.to_local;
        let v = Vec2::new(a * dx + c * dy, b * dx + e * dy);
        if v.x.is_finite() && v.y.is_finite() {
            p + v
        } else {
            p
        }
    }
}

/// The factor Displace's shading scales a colour channel by (Multiply)
/// and the amount it screens it with (Screen), for a photo lightness `l`
/// against its `mean`, at `strength` 0..1 (#3595): the shadows darken by
/// how far below the middle grey they are, the highlights lighten by how
/// far above it.
pub fn surface_shade(shade: SurfaceShade, strength: f64, mean: f64, l: f64) -> (f64, f64) {
    let s = if strength.is_finite() { strength.clamp(0.0, 1.0) } else { 0.0 };
    let m = if mean.is_finite() { mean.clamp(0.01, 0.99) } else { 0.5 };
    let l = if l.is_finite() { l.clamp(0.0, 1.0) } else { m };
    let dark = matches!(shade, SurfaceShade::Multiply | SurfaceShade::Both);
    let light = matches!(shade, SurfaceShade::Screen | SurfaceShade::Both);
    let mul = if dark && l < m { 1.0 - s * (1.0 - l / m) } else { 1.0 };
    let scr = if light && l > m { s * (l - m) / (1.0 - m) } else { 0.0 };
    (mul, scr)
}

/// Blend If's full range: every lightness shows.
pub const BLEND_IF_ALL: [f64; 4] = [0.0, 0.0, 1.0, 1.0];

/// How much a pixel of lightness `l` (0..1) shows under a Blend If range
/// `[dark_from, dark_full, light_full, light_to]`: nothing below
/// `dark_from`, rising to fully at `dark_full`, fully up to `light_full`,
/// falling to nothing at `light_to`. A split end (`from` short of
/// `full`) fades in smoothly; a joined one cuts sharply. The values are
/// put in order first, so a range dragged over itself stays a range.
pub fn blend_if_weight(range: &[f64; 4], l: f64) -> f64 {
    let mut r = range.map(|v| if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 });
    r[1] = r[1].max(r[0]);
    r[2] = r[2].max(r[1]);
    r[3] = r[3].max(r[2]);
    let l = if l.is_finite() { l } else { 0.0 };
    let rise = if l < r[0] {
        0.0
    } else if l >= r[1] {
        1.0
    } else {
        (l - r[0]) / (r[1] - r[0])
    };
    let fall = if l > r[3] {
        0.0
    } else if l <= r[2] {
        1.0
    } else {
        (r[3] - l) / (r[3] - r[2])
    };
    rise.min(fall)
}

/// The lightness Blend If reads from a colour, 0..1 (Rec. 601 weights,
/// the grey a picture's channels mix to).
pub fn blend_if_lightness(r: f64, g: f64, b: f64) -> f64 {
    (0.299 * r + 0.587 * g + 0.114 * b).clamp(0.0, 1.0)
}

/// Which entries of an effect stack a bake takes: the warps switched on
/// with nothing before them drawing from the outline.
fn baked_entries(stack: &[(Effect, bool)]) -> Vec<bool> {
    let mut drawn_from = false;
    stack
        .iter()
        .map(|(e, on)| {
            if !*on {
                return false;
            }
            if e.draws_from_outline() {
                drawn_from = true;
            }
            !drawn_from && e.warps_outline()
        })
        .collect()
}

/// A picture anywhere in `shape` (it cannot become a curve).
fn holds_bitmap(shape: &Shape, depth: usize) -> bool {
    if depth > 64 {
        return true;
    }
    match &shape.kind {
        ShapeKind::Bitmap { .. } => true,
        ShapeKind::Group { children } => children.iter().any(|c| holds_bitmap(c, depth + 1)),
        ShapeKind::ClipFrame { frame, contents } => {
            holds_bitmap(frame, depth + 1) || contents.iter().any(|c| holds_bitmap(c, depth + 1))
        }
        _ => false,
    }
}

/// `shape` with its outline warps (envelope, perspective, distortion; those
/// no contour, blend or other drawn effect comes before) made part of its
/// outline, as Convert to Curves does (#647): a curve, or a group whose
/// objects are curves, drawn as before; the other effects stay, those
/// switched off too. `None` when there is nothing to bake, or a picture
/// inside would be lost.
pub fn bake_outline_warps(shape: &Shape) -> Option<Shape> {
    if !shape.has_outline_warp() || holds_bitmap(shape, 0) {
        return None;
    }
    let stack = shape.effect_stack();
    let mask = baked_entries(&stack);
    if !mask.contains(&true) {
        return None;
    }
    let mut probe = shape.clone();
    probe.effects = stack
        .iter()
        .zip(&mask)
        .filter(|(_, m)| **m)
        .map(|((e, _), _)| e.clone())
        .collect();
    probe.parked_effects.clear();
    let main = evaluate(&probe).main;
    let rest: Vec<(Effect, bool)> = stack
        .into_iter()
        .zip(mask)
        .filter(|(_, m)| !m)
        .map(|(e, _)| e)
        .collect();
    let (effects, parked) = Shape::split_stack(rest);
    let mut out = shape.clone();
    let tolerance = refit_tolerance(&main.page_path());
    out.kind = refit_kind(main.kind, tolerance);
    out.effects = effects;
    out.parked_effects = parked;
    out.node_types.clear();
    out.spline = None;
    out.spline_fixed.clear();
    Some(out)
}

/// The outline warps a bake would take, each with the box it maps (for
/// the letters of a text broken apart). Not for groups and clip frames.
pub struct OutlineWarp(Vec<(Effect, Rect)>);

impl OutlineWarp {
    /// `path` (the object's own space) warped with the boxes of the whole
    /// outline. A distortion also measures its reach on the path it is
    /// given, so parts go through [`OutlineWarp::apply_parts`].
    pub fn apply(&self, path: &BezPath) -> BezPath {
        self.0.iter().fold(path.clone(), |p, (e, b)| {
            warp_outline(e, &p, *b).unwrap_or(p)
        })
    }

    /// `parts`, which together make the whole outline, warped as the whole
    /// is drawn: warped together, then split back by their subpaths (a
    /// twister turns each part by its distance across the whole, not
    /// across the part alone). Each part on its own if the warp changed
    /// the number of subpaths.
    pub fn apply_parts(&self, parts: &[BezPath]) -> Vec<BezPath> {
        warp_parts(parts, &|p| self.apply(p))
    }
}

/// `parts`, which together make one outline, warped by `warp` as the
/// whole is: warped together, then split back by their subpaths. Each
/// part on its own if the warp changed the number of subpaths.
fn warp_parts(parts: &[BezPath], warp: &dyn Fn(&BezPath) -> BezPath) -> Vec<BezPath> {
    let starts = |p: &BezPath| {
        p.elements()
            .iter()
            .filter(|e| matches!(e, PathEl::MoveTo(_)))
            .count()
    };
    let mut whole = BezPath::new();
    for p in parts {
        whole.extend(p.elements().iter().copied());
    }
    let warped = warp(&whole);
    let counts: Vec<usize> = parts.iter().map(starts).collect();
    if counts.iter().sum::<usize>() != starts(&warped) {
        return parts.iter().map(warp).collect();
    }
    let mut out = Vec::with_capacity(parts.len());
    let mut els = warped.elements().iter().copied().peekable();
    for n in counts {
        let mut part = BezPath::new();
        let mut seen = 0;
        while let Some(e) = els.peek() {
            if matches!(e, PathEl::MoveTo(_)) {
                if seen == n {
                    break;
                }
                seen += 1;
            }
            part.push(*e);
            els.next();
        }
        out.push(part);
    }
    out
}

/// The outline warps of `shape` a bake would take (see [`OutlineWarp`]).
pub fn outline_warp(shape: &Shape) -> Option<OutlineWarp> {
    if matches!(
        shape.kind,
        ShapeKind::Group { .. } | ShapeKind::ClipFrame { .. } | ShapeKind::Bitmap { .. }
    ) {
        return None;
    }
    let stack = shape.effect_stack();
    let mask = baked_entries(&stack);
    let mut p = shape.local_path();
    let mut steps = Vec::new();
    for ((e, _), m) in stack.iter().zip(mask) {
        if !m {
            continue;
        }
        let b = p.bounding_box();
        steps.push((e.clone(), b));
        p = warp_outline(e, &p, b).unwrap_or(p);
    }
    (!steps.is_empty()).then_some(OutlineWarp(steps))
}

/// One outline warp of `p` with the box `b` (as [`evaluate`] does).
fn warp_outline(e: &Effect, p: &BezPath, b: Rect) -> Option<BezPath> {
    match e {
        Effect::OffsetPath {
            distance,
            smooth,
            join,
            ..
        } => Some(crate::shaping::offset_smooth(
            p,
            *distance,
            *smooth,
            *join,
            crate::shaping::FillRule::EvenOdd,
        )),
        Effect::Distort(how) => Some(crate::effects::distort_in(p, b, *how)),
        Effect::Envelope {
            nodes,
            keep_lines,
            handles,
            corners,
            mapping,
        } => {
            let frame = crate::envelope::Frame::from_parts(nodes, handles, corners)?;
            Some(crate::envelope::apply_in(
                p,
                b,
                &frame,
                *keep_lines,
                *mapping,
            ))
        }
        Effect::Perspective { corners } => Some(perspective_in(p, b, corners)),
        Effect::Displace {
            map: Some(map),
            horizontal,
            vertical,
            ..
        } => Some(warp(p, b, |q| map.displace(q, *horizontal, *vertical))),
        _ => None,
    }
}

/// The letters of a warped text, each its own curve warped as the whole
/// text is (Break Apart, #647): one per character (a mark stays with its
/// letter), the underlines and other marks as one more. `None` for other
/// shapes, without a text engine or without a warp to bake.
pub fn warped_letters(shape: &Shape) -> Option<Vec<BezPath>> {
    if !matches!(shape.kind, ShapeKind::Text { .. }) {
        return None;
    }
    let warp = outline_warp(shape)?;
    let layout = crate::document::text_glyph_layout(&shape.kind)?;
    let mut letters: Vec<BezPath> = Vec::new();
    for g in &layout.glyphs {
        match letters.last_mut() {
            Some(last) if g.text.is_empty() => last.extend(g.path.elements().iter().copied()),
            _ => letters.push(g.path.clone()),
        }
    }
    if !layout.rest.elements().is_empty() {
        letters.push(layout.rest.clone());
    }
    // Curves fill with the even-odd rule, text with nonzero (#800).
    let letters: Vec<BezPath> = letters
        .iter()
        .filter(|p| !p.elements().is_empty())
        .map(crate::shaping::nonzero_as_even_odd)
        .collect();
    let warped = warp.apply_parts(&letters);
    let mut all = BezPath::new();
    warped
        .iter()
        .for_each(|p| all.extend(p.elements().iter().copied()));
    let tolerance = refit_tolerance(&all);
    Some(warped.iter().map(|p| refit_path(p, tolerance)).collect())
}

/// How closely a baked warp's outline is fitted with curves: a ten
/// thousandth of the outline's size, between 0.005 and 0.05 mm. The warp
/// is drawn as short lines; fitting them gives smooth curves with about
/// as many nodes as the outline had before (#647).
fn refit_tolerance(path: &BezPath) -> f64 {
    let b = path.bounding_box();
    let size = b.width().hypot(b.height());
    if size.is_finite() {
        (size * 1e-4).clamp(0.005, 0.05)
    } else {
        0.01
    }
}

/// A baked warp's outline (short lines, and curves flattened first) as
/// curves within `tolerance`, its corners kept sharp (#647, #840).
fn refit_path(path: &BezPath, tolerance: f64) -> BezPath {
    let mut lines = BezPath::new();
    crate::geometry::flatten(path, tolerance * 0.25, &mut |el| lines.push(el));
    crate::shaping::fit_polygon_curves(&lines, tolerance)
}

/// Whether outlines `a` and `b` run within `tol` of each other: every
/// point of either flattened lies that near the other (for tests of a
/// baked warp, which is fitted with curves).
#[cfg(test)]
pub(crate) fn same_outline(a: &BezPath, b: &BezPath, tol: f64) -> bool {
    fn segments(p: &BezPath) -> Vec<(Point, Point)> {
        let mut out = Vec::new();
        let (mut at, mut start) = (Point::ZERO, Point::ZERO);
        crate::geometry::flatten(p, 0.001, &mut |el| match el {
            PathEl::MoveTo(q) => {
                at = q;
                start = q;
            }
            PathEl::LineTo(q) => {
                out.push((at, q));
                at = q;
            }
            PathEl::ClosePath => {
                out.push((at, start));
                at = start;
            }
            _ => {}
        });
        out
    }
    fn dist(p: Point, (a, b): (Point, Point)) -> f64 {
        let d = b - a;
        let len = d.hypot2();
        let t = if len > 0.0 {
            ((p - a).dot(d) / len).clamp(0.0, 1.0)
        } else {
            0.0
        };
        (p - (a + d * t)).hypot()
    }
    let (sa, sb) = (segments(a), segments(b));
    let near = |from: &[(Point, Point)], to: &[(Point, Point)]| {
        from.iter().all(|s| {
            [s.0, s.1]
                .iter()
                .all(|p| to.iter().map(|t| dist(*p, *t)).fold(f64::MAX, f64::min) <= tol)
        })
    };
    !sa.is_empty() && near(&sa, &sb) && near(&sb, &sa)
}

/// `kind` with its curves (a group's too) fitted within `tolerance`.
fn refit_kind(kind: ShapeKind, tolerance: f64) -> ShapeKind {
    match kind {
        ShapeKind::Path { path, closed } => ShapeKind::Path {
            path: refit_path(&path, tolerance),
            closed,
        },
        ShapeKind::Group { children } => ShapeKind::Group {
            children: children
                .into_iter()
                .map(|mut c| {
                    c.kind = refit_kind(c.kind, tolerance);
                    c
                })
                .collect(),
        },
        other => other,
    }
}

impl Shape {
    /// An outline warp a bake would take (no evaluation: menus ask every
    /// frame). Pictures keep their warps.
    pub fn has_outline_warp(&self) -> bool {
        self.effects
            .iter()
            .take_while(|e| !e.draws_from_outline())
            .any(Effect::warps_outline)
            && !holds_bitmap(self, 0)
    }

    /// Effects of this object that stay live and read its outline after a
    /// bake (a contour, shadow, blend..., or a warp kept for a picture
    /// inside): the warps of the objects inside it then stay live too,
    /// or what those effects draw would change.
    pub fn outline_stays_live(&self) -> bool {
        self.effects.iter().any(Effect::draws_from_outline)
            || (self.effects.iter().any(Effect::warps_outline) && holds_bitmap(self, 0))
    }
}

impl Shape {
    /// The object, or an object in its groups, has live effects.
    pub fn has_live_effects(&self) -> bool {
        !self.effects.is_empty()
            || matches!(&self.kind, ShapeKind::Group { children } if children.iter().any(Shape::has_live_effects))
    }

    /// The bounds of what the object draws: the outline its live effects
    /// warp it to and the objects they add (contours, blends, extrusions)
    /// included. The same as [`Shape::bounds`] without live effects.
    pub fn visual_bounds(&self) -> Rect {
        if !self.has_live_effects() {
            return self.bounds();
        }
        if self.effects.is_empty() {
            if let ShapeKind::Group { children } = &self.kind {
                return children
                    .iter()
                    .filter(|c| !c.is_empty_path())
                    .map(|c| {
                        let mut c = c.clone();
                        c.transform_by(self.transform);
                        c.visual_bounds()
                    })
                    .reduce(|a, b| a.union(b))
                    .unwrap_or_else(|| self.bounds());
            }
        }
        let ev = evaluate(self);
        // A body that draws the object among its faces leaves the object
        // itself an empty path, whose box is a point at the page's origin:
        // taken in, the box of extruded text reached the page's corner
        // (#3227).
        std::iter::once(&ev.main)
            .chain(ev.below.iter())
            .chain(ev.above.iter())
            .filter(|s| !s.is_empty_path())
            .map(Shape::bounds)
            .filter(|b| {
                b.x0.is_finite() && b.y0.is_finite() && b.x1.is_finite() && b.y1.is_finite()
            })
            .reduce(|a, b| a.union(b))
            .unwrap_or_else(|| self.bounds())
    }

    /// The object starts a blend whose end is an object of the drawing
    /// (see [`Shape::own_visual_bounds`]).
    pub fn starts_linked_blend(&self) -> bool {
        self.effects.iter().any(|e| {
            matches!(
                e,
                Effect::Blend {
                    end_id: Some(_),
                    ..
                }
            )
        })
    }

    /// The object's own box, the one it is selected and transformed by:
    /// [`Shape::visual_bounds`] without the steps of a blend it starts
    /// whose end is an object of the drawing. Those steps lie between the
    /// two objects, each of which has a box of its own (#1540); a group
    /// holding the blend still spans them, and a blend whose end lives
    /// only in the effect (older files) is one object, steps included.
    pub fn own_visual_bounds(&self) -> Rect {
        if !self.starts_linked_blend() {
            return self.visual_bounds();
        }
        let mut own = self.clone();
        own.effects.retain(|e| {
            !matches!(
                e,
                Effect::Blend {
                    end_id: Some(_),
                    ..
                }
            )
        });
        own.visual_bounds()
    }
}

impl Effect {
    /// A fresh envelope around `b` (Putty mapping): the object stays as it
    /// is until the frame changes.
    pub fn envelope_around(b: Rect, keep_lines: bool) -> Effect {
        Effect::from_envelope(
            &crate::envelope::Frame::around(b),
            keep_lines,
            crate::envelope::Mapping::default(),
        )
    }

    /// An envelope effect holding `frame`.
    pub fn from_envelope(
        frame: &crate::envelope::Frame,
        keep_lines: bool,
        mapping: crate::envelope::Mapping,
    ) -> Effect {
        Effect::Envelope {
            nodes: frame.nodes.clone(),
            keep_lines,
            handles: frame.handles.clone(),
            corners: frame.corners.to_vec(),
            mapping,
        }
    }

    /// The frame of an envelope effect (None for other effects and for an
    /// envelope with too few nodes).
    pub fn envelope_frame(&self) -> Option<crate::envelope::Frame> {
        match self {
            Effect::Envelope {
                nodes,
                handles,
                corners,
                ..
            } => crate::envelope::Frame::from_parts(nodes, handles, corners),
            _ => None,
        }
    }
}

/// A colour or gradient overlay (#1141): the object's shape over it,
/// filled with `fill` and its outline in `line`, at `opacity` of the
/// object's own; nothing at no opacity.
fn overlay(main: &Shape, fill: Fill, line: Color, opacity: f64) -> Option<Shape> {
    let opacity = if opacity.is_finite() {
        opacity.clamp(0.0, 1.0)
    } else {
        0.0
    };
    if opacity <= 0.0 {
        return None;
    }
    let mut s = main.clone();
    s.effects.clear();
    s.parked_effects.clear();
    s.shadow = None;
    if !matches!(s.fill, Fill::None) {
        s.fill = fill;
    }
    if let Some(st) = s.stroke.as_mut() {
        st.color = line;
        st.painterly = None;
    }
    s.opacity *= opacity;
    Some(s)
}

/// The sprayer's object number `k` of shape `preset` centred on `c`,
/// `size` across (#3435).
pub fn spray_object(preset: u8, c: Point, size: f64, k: usize) -> BezPath {
    let r = size / 2.0;
    let rect = Rect::new(c.x - r, c.y - r, c.x + r, c.y + r);
    match preset {
        0 => crate::geometry::ellipse_path(rect),
        1 => crate::geometry::polygon_path(rect, 5, 0.5),
        2 => crate::geometry::rect_path(rect, 0.0),
        3 => {
            // Heart: two circles and a triangle, welded.
            let top = crate::geometry::ellipse_path(Rect::new(c.x - r, c.y, c.x, c.y + r));
            let top2 = crate::geometry::ellipse_path(Rect::new(c.x, c.y, c.x + r, c.y + r));
            let tri = crate::effects::polygon(&[
                Point::new(c.x - r, c.y + r * 0.5),
                Point::new(c.x + r, c.y + r * 0.5),
                Point::new(c.x, c.y - r),
            ]);
            let a = crate::shaping::overlay(&top, &top2, crate::shaping::Op::Weld);
            crate::shaping::overlay(&a, &tri, crate::shaping::Op::Weld)
        }
        _ => {
            // Leaf: a lens shape turned with the index.
            let mut p = BezPath::new();
            p.move_to((c.x - r, c.y));
            p.curve_to(
                (c.x - r * 0.3, c.y + r),
                (c.x + r * 0.3, c.y + r),
                (c.x + r, c.y),
            );
            p.curve_to(
                (c.x + r * 0.3, c.y - r),
                (c.x - r * 0.3, c.y - r),
                (c.x - r, c.y),
            );
            p.close_path();
            Affine::rotate_about((k as f64 * 0.7).sin() * 0.8, c) * p
        }
    }
}

/// The sprayer's objects along `path` (its own space): one every
/// `spacing` along each part, from its start, the size varying a little
/// from one to the next (#3435).
pub fn spray_paths(path: &BezPath, preset: u8, size: f64, spacing: f64) -> Vec<BezPath> {
    let spacing = if spacing.is_finite() {
        spacing.max(0.5)
    } else {
        1.0
    };
    let size = if size.is_finite() {
        size.max(0.01)
    } else {
        1.0
    };
    let mut parts: Vec<Vec<Point>> = Vec::new();
    crate::geometry::flatten(path, 0.05, &mut |el| match el {
        PathEl::MoveTo(p) => parts.push(vec![p]),
        PathEl::LineTo(p) => {
            if let Some(part) = parts.last_mut() {
                part.push(p);
            }
        }
        PathEl::ClosePath => {
            if let Some(part) = parts.last_mut() {
                if let Some(&first) = part.first() {
                    part.push(first);
                }
            }
        }
        _ => {}
    });
    let mut out = Vec::new();
    let mut k = 0usize;
    for pts in parts.iter().filter(|p| p.len() >= 2) {
        let mut cum = vec![0.0];
        for w in pts.windows(2) {
            cum.push(cum.last().copied().unwrap_or(0.0) + (w[1] - w[0]).hypot());
        }
        let total = cum.last().copied().unwrap_or(0.0);
        let mut s = 0.0;
        while s <= total + 1e-9 && out.len() < 20_000 {
            let i = cum
                .partition_point(|c| *c <= s)
                .saturating_sub(1)
                .min(pts.len() - 2);
            let seg = pts[i + 1] - pts[i];
            let len = seg.hypot().max(1e-9);
            let u = ((s - cum[i]) / len).clamp(0.0, 1.0);
            let c = pts[i] + seg * u;
            let d = size * (0.7 + 0.3 * ((k as f64 * 1.3).sin().abs()));
            let p = spray_object(preset, c, d, k);
            if !p.elements().is_empty() {
                out.push(p);
            }
            s += spacing;
            k += 1;
        }
    }
    out
}

/// The group of objects a sprayer effect draws along `main` (#3435).
fn spray_group(main: &Shape, preset: u8, size: f64, spacing: f64, color: Color) -> Option<Shape> {
    let path = match &main.kind {
        ShapeKind::Group { .. }
        | ShapeKind::ClipFrame { .. }
        | ShapeKind::Table(_)
        | ShapeKind::SymbolInstance { .. }
        | ShapeKind::Bitmap { .. } => return None,
        _ => main.local_path(),
    };
    let children: Vec<Shape> = spray_paths(&path, preset, size, spacing)
        .into_iter()
        .map(|p| {
            let mut c = Shape::new(
                main.id,
                ShapeKind::Path {
                    path: p,
                    closed: true,
                },
            );
            c.fill = Fill::Solid(color);
            c.stroke = None;
            c.name = Some("Sprayed object".into());
            c
        })
        .collect();
    if children.is_empty() {
        return None;
    }
    let mut g = Shape::new(main.id, ShapeKind::Group { children });
    g.transform = main.transform;
    g.name = Some("Sprayed objects".into());
    Some(g)
}

/// Result of evaluating a shape's effect stack.
#[derive(Clone)]
pub struct Evaluated {
    pub below: Vec<Shape>,
    pub main: Shape,
    pub above: Vec<Shape>,
}

/// How many evaluations [`evaluate`] remembers.
const EVALUATION_MEMORY: usize = 8;

/// The latest evaluations of shapes that draw objects from their outline,
/// newest last, each with the shape it was made from. Shared by all
/// threads: the rasteriser works on its own.
static EVALUATED: std::sync::Mutex<Vec<(Shape, Evaluated)>> = std::sync::Mutex::new(Vec::new());

/// Evaluate every effect of `shape` in order. Geometry effects replace the
/// main shape's kind with a path; object effects add shapes.
///
/// A contour, blend or extrusion costs milliseconds, and a frame asks for
/// it to draw, to measure and to pick: the evaluations of the latest
/// shapes are kept, so the same shape is worked out once (#897).
pub fn evaluate(shape: &Shape) -> Evaluated {
    if !shape.effects.iter().any(Effect::draws_from_outline) {
        return evaluate_now(shape);
    }
    if let Ok(mut kept) = EVALUATED.lock() {
        if let Some(i) = kept.iter().position(|(from, _)| from == shape) {
            let hit = kept.remove(i);
            let ev = hit.1.clone();
            kept.push(hit);
            return ev;
        }
    }
    let ev = evaluate_now(shape);
    if let Ok(mut kept) = EVALUATED.lock() {
        if kept.len() >= EVALUATION_MEMORY {
            kept.remove(0);
        }
        kept.push((shape.clone(), ev.clone()));
    }
    ev
}

/// The box of the object's outline where the effect at `at` of its list
/// takes it: after the warps before it. The other effects leave the
/// outline alone, but for an extrusion turned in space or bevelled, whose
/// object is drawn among its faces: a warp after an extrusion is put on
/// the outline before the body is built (#2214), so the box is the
/// outline's there too.
pub fn warp_box(shape: &Shape, at: usize) -> Rect {
    let mut s = shape.clone();
    s.effects.truncate(at);
    s.effects.retain(Effect::warps_outline);
    evaluate(&s).main.local_path().bounding_box()
}

/// One outline warp (envelope, perspective, distortion) applied to
/// `main` in the box of its outline; other effects change nothing.
fn warp_shape(main: &mut Shape, effect: &Effect) {
    match effect {
        Effect::OffsetPath {
            distance,
            smooth,
            join,
            merge,
        } => {
            let (distance, smooth, join) = (*distance, *smooth, *join);
            if *merge && merge_group(main) {
                // One outline now, its parts adding up under the nonzero rule.
                reshape(main, &|p, _| {
                    crate::shaping::offset_smooth(
                        p,
                        distance,
                        smooth,
                        join,
                        crate::shaping::FillRule::NonZero,
                    )
                });
                main.fill_rule = crate::shaping::FillRule::EvenOdd;
            } else {
                let fill = if matches!(main.kind, ShapeKind::Text { .. }) {
                    crate::shaping::FillRule::NonZero
                } else {
                    main.fill_rule
                };
                reshape(main, &|p, _| {
                    crate::shaping::offset_smooth(p, distance, smooth, join, fill)
                });
                if !matches!(main.kind, ShapeKind::Group { .. } | ShapeKind::ClipFrame { .. }) {
                    main.fill_rule = crate::shaping::FillRule::EvenOdd;
                }
            }
        }
        Effect::Distort(how) => {
            reshape(main, &|p, b| crate::effects::distort_in(p, b, *how));
        }
        Effect::Envelope {
            nodes,
            keep_lines,
            handles,
            corners,
            mapping,
        } => {
            if let Some(frame) = crate::envelope::Frame::from_parts(nodes, handles, corners) {
                // Pictures bend through the same map, their pixels
                // resampled (#3608); the outlines of the rest bend after.
                let b = main.local_path().bounding_box();
                if let Some(map) = crate::envelope::point_map(b, &frame, *mapping) {
                    let key = {
                        use std::hash::{Hash, Hasher};
                        let mut h = std::collections::hash_map::DefaultHasher::new();
                        for p in nodes.iter().chain(handles.iter().flatten()) {
                            (p.x.to_bits(), p.y.to_bits()).hash(&mut h);
                        }
                        corners.hash(&mut h);
                        mapping.id().hash(&mut h);
                        [b.x0, b.y0, b.x1, b.y1].map(f64::to_bits).hash(&mut h);
                        h.finish()
                    };
                    warp_pictures(main, None, &|pic, m| {
                        crate::bitmap_warp::bend(pic, m, key, &map)
                    });
                }
                reshape(main, &|p, b| {
                    crate::envelope::apply_in(p, b, &frame, *keep_lines, *mapping)
                });
            }
        }
        Effect::Perspective { corners } => {
            // Pictures first, through the same map: their pixels are
            // resampled (#2691); the outlines of the rest bend.
            let b = main.local_path().bounding_box();
            warp_pictures(main, None, &|pic, m| {
                crate::bitmap_warp::perspective(pic, m, b, corners)
            });
            reshape(main, &|p, b| perspective_in(p, b, corners));
        }
        Effect::Displace {
            map: Some(map),
            horizontal,
            vertical,
            ..
        } => {
            let (h, v) = (*horizontal, *vertical);
            if h != 0.0 || v != 0.0 {
                reshape(main, &|p, b| warp(p, b, |q| map.displace(q, h, v)));
            }
        }
        _ => {}
    }
}

/// The pictures of `shape` (it, or the ones inside it), in the space
/// `parent` maps them into (`None`: the shape's own space, the top of the
/// warp), put through the warp `f` of that space: a perspective (#2691)
/// or an envelope (#3608). A warped picture covers the box the warp
/// takes it to, transparent outside, in the top shape's space.
fn warp_pictures(
    shape: &mut Shape,
    parent: Option<Affine>,
    f: &dyn Fn(&crate::bitmap_warp::Picture, Affine) -> Option<crate::bitmap_warp::Picture>,
) {
    let t = parent.map(|p| p * shape.transform);
    let m = t.unwrap_or(Affine::IDENTITY);
    match &mut shape.kind {
        ShapeKind::Group { children } => {
            for c in children.iter_mut() {
                warp_pictures(c, Some(m), f);
            }
        }
        ShapeKind::ClipFrame { frame, contents } => {
            warp_pictures(frame, Some(m), f);
            for c in contents.iter_mut() {
                warp_pictures(c, Some(m), f);
            }
        }
        ShapeKind::Bitmap {
            rect,
            width_px,
            height_px,
            png,
            ..
        } => {
            let pic = crate::bitmap_warp::Picture {
                rect: *rect,
                width_px: *width_px,
                height_px: *height_px,
                png: png.clone(),
            };
            if let Some(out) = f(&pic, m) {
                shape.kind = ShapeKind::Bitmap {
                    rect: out.rect,
                    width_px: out.width_px,
                    height_px: out.height_px,
                    png: out.png,
                    fx: None,
                };
                // Inside a group the warped picture is in the group's
                // space: its own transform takes the parents' back.
                if let Some(p) = parent {
                    if p.determinant().abs() > 1e-12 {
                        shape.transform = p.inverse();
                    }
                }
            }
        }
        _ => {}
    }
}

fn evaluate_now(shape: &Shape) -> Evaluated {
    let mut main = shape.clone();
    let mut below: Vec<Shape> = Vec::new();
    let mut above: Vec<Shape> = Vec::new();
    // The warps left in the list were applied ahead, for an extrusion.
    let mut warped_ahead = false;
    for (at, effect) in shape.effects.iter().enumerate() {
        match effect {
            Effect::Distort(_)
            | Effect::Envelope { .. }
            | Effect::Perspective { .. }
            | Effect::Displace { .. }
            | Effect::OffsetPath { .. } => {
                // An extrusion before it already took it into its outline.
                if !warped_ahead {
                    warp_shape(&mut main, effect);
                }
            }
            Effect::Contour {
                steps,
                offset,
                outside,
                to_center,
                both,
                fill_to,
                outline_to,
                fill_end_to,
                color_blend,
                corners,
                accel_objects,
                accel_colors,
            } => {
                let params = |outside: bool, to_center: bool| ContourParams {
                    steps: *steps,
                    offset: *offset,
                    outside,
                    to_center,
                    fill_to: *fill_to,
                    outline_to: *outline_to,
                    fill_end_to: *fill_end_to,
                    color_blend: *color_blend,
                    corners: corners.unwrap_or(crate::shaping::Join::Round),
                    accel_objects: *accel_objects,
                    accel_colors: *accel_colors,
                };
                // Both sides: the outside steps behind the object and the
                // inside ones in front, each as a one-sided contour draws
                // them (#3085).
                let sides: &[bool] = if *both {
                    &[true, false]
                } else {
                    std::slice::from_ref(outside)
                };
                for &out in sides {
                    let shapes =
                        contour(&main, area_rule(shape), &params(out, *to_center && !*both));
                    if out {
                        // Outermost first so the object stays on top.
                        below.extend(shapes.into_iter().rev());
                    } else {
                        above.extend(shapes);
                    }
                }
            }
            Effect::Blend {
                end,
                steps,
                accel_objects,
                accel_colors,
                rotation,
                path,
                rotate_on_path,
                end_id,
                options,
            } => {
                let shapes = blend(
                    &main,
                    end,
                    *steps,
                    (*accel_objects, *accel_colors),
                    *rotation,
                    path.as_ref(),
                    *rotate_on_path,
                    options,
                );
                if options.steps_below {
                    // Bottom to top: the end (when the effect draws it),
                    // then the steps from the end's side to the start.
                    if end_id.is_none() {
                        below.push((**end).clone());
                    }
                    below.extend(shapes.into_iter().rev());
                } else {
                    above.extend(shapes);
                    // An end object of the drawing draws itself.
                    if end_id.is_none() {
                        above.push((**end).clone());
                    }
                }
            }
            Effect::Extrude {
                depth,
                vanishing,
                amount,
                shade_from,
                shade_to,
                light_angle,
                light_intensity,
                kind,
                on_object,
                solid,
                bevel,
                space,
            } => {
                if let Some(mut body) = extrude_body(
                    &main,
                    *depth,
                    *vanishing,
                    *amount,
                    (*shade_from, *shade_to, *solid),
                    (*light_angle, *light_intensity),
                    *kind,
                    *on_object,
                ) {
                    body.bevel = *bevel;
                    body.solid = space.clone();
                    // The body follows the outline as the warps after it
                    // in the list leave it (#2214): an envelope put on an
                    // extruded object bends the object, and the sides and
                    // the far face built from the outline before it stayed
                    // behind, poking out around the bent object.
                    let rest = shape.effects.get(at + 1..).unwrap_or(&[]);
                    let ahead =
                        (!warped_ahead && rest.iter().any(Effect::warps_outline)).then(|| {
                            let mut bent = main.clone();
                            for e in rest {
                                warp_shape(&mut bent, e);
                            }
                            bent
                        });
                    let faces = body.faces(ahead.as_ref().unwrap_or(&main));
                    below.extend(faces.below);
                    above.extend(faces.above);
                    // The bent outline is the object's from here on, unless
                    // an effect between the extrusion and a warp draws from
                    // the outline as it is at its own place (a contour, a
                    // blend): the warps then come at their places.
                    let plain_rest = !rest.iter().any(Effect::draws_from_outline);
                    match (faces.main, ahead) {
                        (Some(m), bent) => {
                            warped_ahead |= bent.is_some();
                            main = m;
                        }
                        (None, Some(bent)) if plain_rest => {
                            warped_ahead = true;
                            main = bent;
                        }
                        _ => {}
                    }
                }
            }
            e @ Effect::Bevel { .. } => {
                above.extend(bevel(&main, e));
            }
            Effect::BlockShadow { offset, color, gap } => {
                // Cast by all the object draws, and beneath all of it
                // (#2218).
                let cast = cast_area(&main, area_rule(shape), below.iter().chain(&above));
                let shadow = block_shadow(&main, cast, *offset, *color, *gap);
                below.splice(0..0, shadow);
            }
            // Mirrored last, whatever its place in the list: the copies
            // carry every other effect of the object (#2121).
            Effect::Symmetry { .. } => {}
            // Copied after the others too, so each copy carries them.
            Effect::Transform { .. } => {}
            Effect::Grid { .. } => {}
            Effect::ColorOverlay { color, opacity } => {
                above.extend(overlay(&main, Fill::Solid(*color), *color, *opacity));
            }
            Effect::GradientOverlay {
                from,
                to,
                angle,
                opacity,
            } => {
                let f = crate::style::Fountain::two(
                    crate::style::FountainKind::Linear,
                    *from,
                    *to,
                    *angle,
                );
                above.extend(overlay(&main, Fill::Fountain(f), *to, *opacity));
            }
            Effect::Spray {
                preset,
                size,
                spacing,
                color,
            } => {
                above.extend(spray_group(&main, *preset, *size, *spacing, *color));
            }
            Effect::Tiles {
                preset,
                width,
                color,
            } => {
                above.extend(crate::tiles::tiles_group(&main, *preset, *width, *color));
            }
            Effect::LivePaint { fills } => {
                below.extend(crate::live_paint::paint_shapes(&main, fills));
            }
            Effect::FillOpacity { .. } | Effect::BlendIf { .. } => {}
            Effect::Lens(_)
            | Effect::Transparency { .. }
            | Effect::InnerShadow { .. }
            | Effect::OuterShadow { .. }
            | Effect::Glow { .. }
            | Effect::GaussianBlur { .. } => {}
        }
    }
    // Fill opacity: the object alone, after the overlays and the other
    // effects took it as it was (#1141).
    for effect in &shape.effects {
        if let Effect::FillOpacity { opacity } = effect {
            let o = if opacity.is_finite() {
                opacity.clamp(0.0, 1.0)
            } else {
                1.0
            };
            main.opacity *= o;
        }
    }
    for effect in &shape.effects {
        if let Effect::Transform { .. } = effect {
            transform_copies(effect, &mut below, &mut main, &mut above);
        }
    }
    for effect in &shape.effects {
        if let Effect::Grid { .. } = effect {
            // The cell is all the object draws: what its other effects
            // (and a Transform's copies) add too.
            let cell = below
                .iter()
                .chain(&above)
                .fold(main.bounds(), |r, s| r.union(s.bounds()));
            let maps = grid_maps(effect, cell);
            copy_under(&maps, false, &mut below, &main, &mut above);
        }
    }
    for effect in &shape.effects {
        if let Effect::Symmetry {
            center,
            angle,
            lines,
            rotate,
        } = effect
        {
            let mut original = main.clone();
            // The copies keep the effects the renderer draws on the object
            // itself (transparency, lens); the objects the other effects
            // add are mirrored with it.
            original
                .effects
                .retain(|e| matches!(e, Effect::Lens(_) | Effect::Transparency { .. }));
            let mirrors = symmetry_mirrors(*center, *angle, *lines, *rotate);
            let (mut under, mut over) = (Vec::new(), Vec::new());
            for m in &mirrors {
                // A fountain fill or a gradient transparency is placed on
                // the object's page box, so it is mirrored with the copy:
                // without this the copy's gradient ran the same way as
                // the object's instead of as its mirror image (#2121).
                let flip = |s: &Shape| {
                    let mut s = s.clone();
                    s.carry_fills(*m, Affine::IDENTITY);
                    s.transform = *m * s.transform;
                    s
                };
                under.extend(below.iter().map(flip));
                over.push(flip(&original));
                over.extend(above.iter().map(flip));
            }
            below.extend(under);
            above.extend(over);
        }
    }
    Evaluated { below, main, above }
}

/// The parts of `shape` a transparency with `target` (0 the fill, 1 the
/// outline, 2 the whole object) draws, in drawing order, each with
/// whether the transparency applies to it. The fill and the outline of
/// an object that has both are drawn apart, the outline first when it is
/// set behind the fill; a part the object lacks leaves the other drawn as
/// it is. Groups, clip frames, tables and symbols, whose members have
/// fills and outlines of their own, and pictures are faded whole.
pub fn transparency_parts(shape: &Shape, target: u8) -> Vec<(Shape, bool)> {
    let container = matches!(
        shape.kind,
        ShapeKind::Group { .. }
            | ShapeKind::ClipFrame { .. }
            | ShapeKind::Table(_)
            | ShapeKind::SymbolInstance { .. }
            | ShapeKind::Bitmap { .. }
    );
    let has_fill = !matches!(shape.fill, Fill::None);
    let has_outline = shape.stroke.is_some();
    if target >= 2 || container {
        return vec![(shape.clone(), true)];
    }
    let fill_faded = target == 0;
    match (has_fill, has_outline) {
        (true, true) => {
            let mut fill = shape.clone();
            fill.stroke = None;
            let mut outline = shape.clone();
            outline.fill = Fill::None;
            // The shadow goes once, beneath the part drawn first.
            let behind = shape.stroke.as_ref().is_some_and(|s| s.behind_fill);
            if let Some(st) = outline.stroke.as_mut() {
                st.behind_fill = false;
            }
            if behind {
                fill.shadow = None;
                vec![(outline, !fill_faded), (fill, fill_faded)]
            } else {
                outline.shadow = None;
                vec![(fill, fill_faded), (outline, !fill_faded)]
            }
        }
        (true, false) => vec![(shape.clone(), fill_faded)],
        (false, true) => vec![(shape.clone(), !fill_faded)],
        (false, false) => vec![(shape.clone(), true)],
    }
}

/// A group (Offset Path with merge) as one curve: the outlines of all its
/// objects, in the group's space, joined into one outline that fills
/// with the nonzero rule, painted as the bottom object that has a fill
/// (its fill, extra fills and outlines). False, and nothing changes, for
/// anything but a group with an outline to join.
fn merge_group(main: &mut Shape) -> bool {
    fn collect(
        s: &Shape,
        t: Affine,
        out: &mut BezPath,
        paint: &mut Option<Shape>,
    ) {
        let t = t * s.transform;
        match &s.kind {
            ShapeKind::Group { children } => {
                for c in children {
                    collect(c, t, out, paint);
                }
            }
            ShapeKind::ClipFrame { frame, .. } => collect(frame, t, out, paint),
            ShapeKind::Bitmap { .. } => {}
            _ => {
                if !s.visible {
                    return;
                }
                // Each part as the area it fills, turned outer contours
                // one way and holes the other, so the parts add up under
                // the nonzero rule.
                let rule = if matches!(s.kind, ShapeKind::Text { .. }) {
                    crate::shaping::FillRule::NonZero
                } else {
                    s.fill_rule
                };
                let area = crate::shaping::oriented_area(&(t * s.local_path()), rule);
                if area.elements().is_empty() {
                    return;
                }
                out.extend(area.elements().iter().copied());
                if paint.is_none() && !matches!(s.fill, crate::style::Fill::None) {
                    *paint = Some(s.clone());
                }
            }
        }
    }
    if !matches!(main.kind, ShapeKind::Group { .. }) {
        return false;
    }
    let mut path = BezPath::new();
    let mut paint = None;
    if let ShapeKind::Group { children } = &main.kind {
        for c in children {
            collect(c, Affine::IDENTITY, &mut path, &mut paint);
        }
    }
    if path.elements().is_empty() {
        return false;
    }
    if let Some(p) = paint {
        main.fill = p.fill.clone();
        main.extra_fills = p.extra_fills.clone();
        main.stroke = p.stroke.clone();
        main.extra_strokes = p.extra_strokes.clone();
    }
    main.fill_rule = crate::shaping::FillRule::NonZero;
    set_path(main, path);
    true
}

/// Reshape `shape` by `f`, which works in the shape's own space. A group
/// (or a clip frame) is reshaped object by object, each keeping its own
/// fill and outline, as the reference program does with an envelope on a
/// group; the whole would otherwise become one path with the group's
/// empty fill and vanish.
/// `f` takes a path and the box mapped by the effect: the shape's own
/// bounding box, for a group the box of all its objects.
fn reshape(shape: &mut Shape, f: &dyn Fn(&BezPath, Rect) -> BezPath) {
    let b = shape.local_path().bounding_box();
    // Text with characters of their own colours: a group of one curve a
    // look (as the text is drawn in parts), warped together in the box of
    // the whole text, so the colours stay (#912).
    if let Some(parts) = shape.text_parts() {
        let paths: Vec<BezPath> = parts.iter().map(Shape::local_path).collect();
        let warped = warp_parts(&paths, &|p| f(p, b));
        let children = parts
            .into_iter()
            .zip(warped)
            .map(|(mut part, path)| {
                part.transform = Affine::IDENTITY;
                part.effects.clear();
                part.parked_effects.clear();
                part.shadow = None;
                set_path(&mut part, path);
                part
            })
            .collect();
        shape.kind = ShapeKind::Group { children };
        shape.spline = None;
        shape.spline_fixed.clear();
        shape.node_types.clear();
        return;
    }
    match &mut shape.kind {
        ShapeKind::Group { children } => {
            for c in children.iter_mut() {
                reshape_inside(c, Affine::IDENTITY, b, f);
            }
        }
        ShapeKind::ClipFrame { frame, contents } => {
            reshape_inside(frame, Affine::IDENTITY, b, f);
            for c in contents.iter_mut() {
                reshape_inside(c, Affine::IDENTITY, b, f);
            }
        }
        // A picture has no outline to bend: a perspective or an envelope
        // resamples it ahead (`warp_pictures`); the other warps leave it
        // as it is. It became an empty frame, so the picture went (#2691).
        ShapeKind::Bitmap { .. } => {}
        _ => {
            let p = f(&shape.local_path(), b);
            set_path(shape, p);
        }
    }
}

/// [`reshape`] for an object inside a group whose space is `parent` times
/// the object's transform away from the object's own.
fn reshape_inside(
    shape: &mut Shape,
    parent: Affine,
    b: Rect,
    f: &dyn Fn(&BezPath, Rect) -> BezPath,
) {
    let t = parent * shape.transform;
    match &mut shape.kind {
        ShapeKind::Group { children } => {
            for c in children.iter_mut() {
                reshape_inside(c, t, b, f);
            }
        }
        ShapeKind::ClipFrame { frame, contents } => {
            reshape_inside(frame, t, b, f);
            for c in contents.iter_mut() {
                reshape_inside(c, t, b, f);
            }
        }
        ShapeKind::Bitmap { .. } => {}
        _ => {
            if t.determinant().abs() < 1e-12 {
                return;
            }
            let warped = f(&(t * shape.local_path()), b);
            set_path(shape, t.inverse() * warped);
        }
    }
    // A blend on the object mixes it with its end copy, in the same space
    // as the object, along a path in that space: both bend with it, or
    // the steps between them ignore the warp (#2120).
    for effect in shape.effects.iter_mut() {
        if let Effect::Blend { end, path, .. } = effect {
            reshape_inside(end, parent, b, f);
            if let Some(p) = path.as_mut() {
                if parent.determinant().abs() > 1e-12 {
                    *p = parent.inverse() * f(&(parent * p.clone()), b);
                }
            }
        }
    }
}

fn set_path(shape: &mut Shape, path: BezPath) {
    let closed = path
        .elements()
        .iter()
        .any(|e| matches!(e, PathEl::ClosePath));
    // The letters of a text overlap (strokes of one letter, letters set
    // tight): their outline fills with the nonzero rule, as the text did,
    // also once it is a curve written to a file or broken apart (#2733).
    if matches!(shape.kind, ShapeKind::Text { .. }) {
        shape.fill_rule = crate::shaping::FillRule::NonZero;
    }
    shape.kind = ShapeKind::Path { path, closed };
    // A warped outline is no longer the B-spline of its control points,
    // and its nodes are new (#647).
    shape.spline = None;
    shape.spline_fixed.clear();
    shape.node_types.clear();
}

/// Break the effect stack into ordinary shapes (below, main, above), with
/// the given ids assigned in order.
pub fn break_apart(shape: &Shape, mut next_id: impl FnMut() -> crate::ShapeId) -> Vec<Shape> {
    let ev = evaluate(shape);
    let mut out = Vec::new();
    for mut s in ev.below {
        s.id = next_id();
        s.effects.clear();
        out.push(s);
    }
    let mut m = ev.main;
    m.effects.clear();
    out.push(m);
    for mut s in ev.above {
        s.id = next_id();
        s.effects.clear();
        out.push(s);
    }
    out
}

fn mix(a: Color, b: Color, t: f64, mode: u8) -> Color {
    if mode == 0 {
        return lerp_color(a, b, t as f32);
    }
    let (h1, s1, v1) = a.to_hsb();
    let (h2, s2, v2) = b.to_hsb();
    let mut dh = h2 - h1;
    if mode == 1 && dh < 0.0 {
        dh += 360.0;
    }
    if mode == 2 && dh > 0.0 {
        dh -= 360.0;
    }
    Color::from_hsb(
        (h1 + dh * t).rem_euclid(360.0),
        s1 + (s2 - s1) * t,
        v1 + (v2 - v1) * t,
    )
}

/// The most copies a Transform effect draws.
pub const MAX_TRANSFORM_COPIES: u32 = 500;

/// One step of a Transform effect about `center` (page space), with each
/// value taken at `share` of the way from none to the effect's own (1:
/// the values as set).
fn transform_step(e: &Effect, center: Point, share: [f64; 4]) -> Affine {
    let Effect::Transform {
        scale_x,
        scale_y,
        move_x,
        move_y,
        angle,
        reflect_x,
        reflect_y,
        ..
    } = *e
    else {
        return Affine::IDENTITY;
    };
    let fin = |v: f64, or: f64| if v.is_finite() { v } else { or };
    let part = |full: f64, from: f64, k: f64| from + (full - from) * k;
    let sign = |m: bool| if m { -1.0 } else { 1.0 };
    let sx = part(fin(scale_x, 100.0), 100.0, share[0]) / 100.0 * sign(reflect_x);
    let sy = part(fin(scale_y, 100.0), 100.0, share[1]) / 100.0 * sign(reflect_y);
    let turn = fin(angle, 0.0) * share[2];
    let d = Vec2::new(fin(move_x, 0.0), fin(move_y, 0.0)) * share[3];
    Affine::translate(center.to_vec2() + d)
        * Affine::rotate(turn.to_radians())
        * Affine::scale_non_uniform(sx, sy)
        * Affine::translate(-center.to_vec2())
}

/// The page maps of a Transform effect's copies (or, with no copies, the
/// one map of the object itself), for an object whose box is centred on
/// `center`: each the one before it with one more step.
pub fn transform_maps(e: &Effect, center: Point) -> Vec<Affine> {
    let Effect::Transform {
        copies,
        random,
        seed,
        ..
    } = *e
    else {
        return Vec::new();
    };
    let n = copies.clamp(1, MAX_TRANSFORM_COPIES) as usize;
    let mut state = u64::from(seed) ^ 0x9E37_79B9_7F4A_7C15;
    let mut next = || {
        // splitmix64
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (z >> 11) as f64 / (1u64 << 53) as f64
    };
    let mut at = Affine::IDENTITY;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let share = if random {
            [next(), next(), next(), next()]
        } else {
            [1.0; 4]
        };
        let step = transform_step(e, center, share);
        // Every step about the object's own centre, so a move with a
        // turn walks the copies round a circle.
        at = step * at;
        out.push(at);
    }
    out
}

/// The outline widths of `s` (its members' too) kept as they are under a
/// scale by `f`, or scaled with it (`scale`), whatever each outline's own
/// "scale with object" says.
fn keep_stroke_widths(s: &mut Shape, f: f64, scale: bool) {
    if let Some(st) = s.stroke.as_mut() {
        if scale && !st.scale_with_object {
            st.width *= f;
        } else if !scale && st.scale_with_object && f > 1e-9 {
            st.width /= f;
        }
    }
    if let ShapeKind::Group { children } = &mut s.kind {
        for c in children {
            keep_stroke_widths(c, f, scale);
        }
    }
}

/// The copies of a Transform effect: the object (with what its other
/// effects drew) copied under itself, the first copy nearest it; with no
/// copies the object and its effects' shapes are moved instead.
fn transform_copies(e: &Effect, below: &mut Vec<Shape>, main: &mut Shape, above: &mut [Shape]) {
    let Effect::Transform {
        copies,
        scale_strokes,
        pivot_x,
        pivot_y,
        ..
    } = *e
    else {
        return;
    };
    // The steps go about the box's centre, or the point set from it.
    let fin = |v: f64| if v.is_finite() { v } else { 0.0 };
    let center = main.bounds().center() + Vec2::new(fin(pivot_x), fin(pivot_y));
    if !(center.x.is_finite() && center.y.is_finite()) {
        return;
    }
    let maps = transform_maps(e, center);
    let place = |s: &Shape, m: Affine| place_copy(s, m, scale_strokes);
    if copies == 0 {
        let Some(m) = maps.first().copied() else {
            return;
        };
        *main = place(main, m);
        for s in below.iter_mut().chain(above.iter_mut()) {
            *s = place(s, m);
        }
        return;
    }
    copy_under(&maps, scale_strokes, below, main, above);
}

/// Copies of the object (with what its other effects drew) under itself,
/// one for each of `maps`, the first nearest it.
fn copy_under(
    maps: &[Affine],
    scale_strokes: bool,
    below: &mut Vec<Shape>,
    main: &Shape,
    above: &mut [Shape],
) {
    if maps.is_empty() {
        return;
    }
    let place = |s: &Shape, m: Affine| place_copy(s, m, scale_strokes);
    let mut original = main.clone();
    // As the symmetry's copies: the effects the renderer draws on the
    // object itself stay on each copy.
    original
        .effects
        .retain(|e| matches!(e, Effect::Lens(_) | Effect::Transparency { .. }));
    let mut under = Vec::new();
    // The last copy at the bottom.
    for m in maps.iter().rev() {
        under.extend(below.iter().map(|s| place(s, *m)));
        under.push(place(&original, *m));
        under.extend(above.iter().map(|s| place(s, *m)));
    }
    under.append(below);
    *below = under;
}

/// `s` moved by `m`, its fills going with it and its outline keeping its
/// width (or scaling with it).
fn place_copy(s: &Shape, m: Affine, scale_strokes: bool) -> Shape {
    let mut s = s.clone();
    s.carry_fills(m, Affine::IDENTITY);
    s.transform = m * s.transform;
    let [a, b, c, d, _, _] = m.as_coeffs();
    let f = (a * d - b * c).abs().sqrt();
    if f.is_finite() && (f - 1.0).abs() > 1e-12 {
        keep_stroke_widths(&mut s, f, scale_strokes);
    }
    s
}

/// The most cells a Grid effect has on a side.
pub const MAX_GRID_SIDE: u32 = 100;
/// The most cells a Grid effect has in all.
pub const MAX_GRID_CELLS: u32 = 2500;

/// The columns and rows a Grid effect draws, within its limits.
pub fn grid_size(e: &Effect) -> (u32, u32) {
    let Effect::Grid { columns, rows, .. } = *e else {
        return (1, 1);
    };
    let c = columns.clamp(1, MAX_GRID_SIDE);
    let r = rows
        .clamp(1, MAX_GRID_SIDE)
        .min((MAX_GRID_CELLS / c).max(1));
    (c, r)
}

/// Where cell (`col`, `row`) of a Grid effect is moved from the object's
/// own place, for an object whose box is `cell` (page space, y up: rows
/// go down).
pub fn grid_offset(e: &Effect, cell: Rect, col: u32, row: u32) -> Vec2 {
    let Effect::Grid {
        gap_x,
        gap_y,
        shift_x,
        shift_y,
        ..
    } = *e
    else {
        return Vec2::ZERO;
    };
    let fin = |v: f64| if v.is_finite() { v } else { 0.0 };
    let step_x = cell.width() + fin(gap_x);
    let step_y = cell.height() + fin(gap_y);
    let dx = f64::from(col) * step_x + if row % 2 == 1 { fin(shift_x) } else { 0.0 };
    let dy = f64::from(row) * step_y + if col % 2 == 1 { fin(shift_y) } else { 0.0 };
    Vec2::new(dx, -dy)
}

/// The box a Grid effect's cells take for `shape`: all it draws without
/// its grid and symmetry.
pub fn grid_cell(shape: &Shape) -> Option<Rect> {
    let mut own = shape.clone();
    own.effects
        .retain(|e| !matches!(e, Effect::Grid { .. } | Effect::Symmetry { .. }));
    let ev = evaluate(&own);
    let r = ev
        .below
        .iter()
        .chain(&ev.above)
        .fold(ev.main.bounds(), |r, s| r.union(s.bounds()));
    [r.x0, r.y0, r.x1, r.y1]
        .iter()
        .all(|v| v.is_finite())
        .then_some(r)
}

/// The page moves of a Grid effect's copies (every cell but the object's
/// own), row by row, for an object whose box is `cell`.
pub fn grid_maps(e: &Effect, cell: Rect) -> Vec<Affine> {
    let ok = [cell.x0, cell.y0, cell.x1, cell.y1]
        .iter()
        .all(|v| v.is_finite());
    if !ok {
        return Vec::new();
    }
    let (cols, rows) = grid_size(e);
    let mut out = Vec::with_capacity((cols * rows) as usize);
    for r in 0..rows {
        for c in 0..cols {
            if r == 0 && c == 0 {
                continue;
            }
            out.push(Affine::translate(grid_offset(e, cell, c, r)));
        }
    }
    out
}

fn is_false(v: &bool) -> bool {
    !*v
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

/// What a contour is drawn from.
pub struct ContourParams {
    pub steps: u32,
    pub offset: f64,
    pub outside: bool,
    pub to_center: bool,
    pub fill_to: Color,
    pub outline_to: Option<Color>,
    /// The last step's colour for the end of a fountain fill.
    pub fill_end_to: Option<Color>,
    pub color_blend: u8,
    pub corners: crate::shaping::Join,
    pub accel_objects: f64,
    pub accel_colors: f64,
}

/// The factor of a blend's acceleration at its ends (-1 and 1): `1.3 ^
/// 25`, as the `.cdr` blend records hold it (#1192: every acceleration
/// of the reporter's files is `1.3 ^ (s / 4)` for a whole `s` from -100
/// to 100, and the extremes are exactly this and its inverse).
pub const BLEND_ACCEL_FACTOR: f64 = 705.641_001_486_682_3;

/// The factor `k` of acceleration `a` (-1 to 1): `BLEND_ACCEL_FACTOR ^
/// -a`, so a positive acceleration gives a factor below 1. Out of range
/// values are held to it, a value that is not a number reads as 0.
pub fn blend_accel_factor(a: f64) -> f64 {
    let a = if a.is_finite() {
        a.clamp(-1.0, 1.0)
    } else {
        0.0
    };
    BLEND_ACCEL_FACTOR.powf(-a)
}

/// Where a blend step `t` of the way from the start (0 to 1) lies with
/// acceleration `a` (-1 to 1), as a share of the way: `ln(1 + (k - 1) t)
/// / ln k` with `k` = [`blend_accel_factor`]`(a)`, so the steps' places
/// form an even run of `k ^ t'` (#1192: the step places of the
/// reporter's files, measured on the pictures they carry, follow this
/// curve within a pixel). A positive acceleration packs the steps toward
/// the start, a negative one toward the end, the same distance either
/// way.
pub fn blend_place(t: f64, a: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    let ln_k = blend_accel_factor(a).ln();
    if ln_k.abs() < 1e-9 {
        return t;
    }
    // ln(1 + (k - 1) t) without losing digits for small (k - 1) t.
    ((ln_k.exp_m1() * t).ln_1p() / ln_k).clamp(0.0, 1.0)
}

/// Where the middle step of a blend with acceleration `a` (-1 to 1)
/// lands, as a share of the way from the start: [`blend_place`]`(0.5,
/// a)`, about 10.5 % from the start at +1 and as far from the end at -1
/// (#1261 wanted the same distance either way; #1192 the curve of the
/// `.cdr` files).
pub fn blend_middle(a: f64) -> f64 {
    blend_place(0.5, a)
}

/// `t` (0 to 1) bent by an acceleration `a` (-1 to 1): `t ^ 4^a`, so a
/// positive acceleration packs the steps toward the start and a negative
/// one toward the end (contours).
pub fn accelerate(t: f64, a: f64) -> f64 {
    let a = if a.is_finite() {
        a.clamp(-1.0, 1.0)
    } else {
        0.0
    };
    t.clamp(0.0, 1.0).powf(4f64.powf(a))
}

/// The most steps a contour draws.
const MAX_CONTOUR_STEPS: u32 = 999;

/// The most steps a blend draws (the range the Blend tool offers). A
/// blend asking for more, from a file or a script, draws this many, so
/// one value can never fill the memory with steps (#980).
pub const MAX_BLEND_STEPS: u32 = 999;

/// Some subpath of `path` ends away from where it starts.
fn has_loose_end(path: &BezPath) -> bool {
    let mut start: Option<Point> = None;
    let mut last: Option<Point> = None;
    let mut loose = false;
    for e in path.elements() {
        if let PathEl::MoveTo(p) = e {
            if let (Some(a), Some(b)) = (start, last) {
                loose |= (a - b).hypot() > 1e-6;
            }
            start = Some(*p);
            last = None;
        } else if let Some(p) = e.end_point() {
            last = Some(p);
        }
    }
    if let (Some(a), Some(b)) = (start, last) {
        loose |= (a - b).hypot() > 1e-6;
    }
    loose
}

/// The contour steps of `shape`: step `i` of `n` is its outline offset by
/// `n * offset * accelerate(i / n, accel_objects)` (to the centre: by
/// `i * offset` until nothing is left), coloured `accelerate(i / n,
/// accel_colors)` of the way to the end colours.
///
/// The offset is measured on the page: the outline is offset with the
/// object's transformation applied and brought back to its local
/// coordinates, so a scaled or stretched object keeps the distance the
/// user typed (#754).
/// The object of a group whose fill and outline its contour steps take
/// (#3245): the first one inside it with a fill, else the first with an
/// outline; a group has neither of its own.
fn group_style(shape: &Shape) -> Option<&Shape> {
    fn leaves<'a>(s: &'a Shape, out: &mut Vec<&'a Shape>) {
        match &s.kind {
            ShapeKind::Group { children } => children.iter().for_each(|c| leaves(c, out)),
            ShapeKind::ClipFrame { frame, .. } => leaves(frame, out),
            _ => out.push(s),
        }
    }
    let mut all = Vec::new();
    leaves(shape, &mut all);
    all.iter()
        .find(|s| !matches!(s.fill, Fill::None))
        .or_else(|| all.iter().find(|s| s.stroke.is_some()))
        .copied()
}

/// The area a group covers, in the group's space: the union of what its
/// objects cover, so where they overlap the contour goes round them all
/// as one (#3245).
fn group_area(shape: &Shape) -> BezPath {
    let ShapeKind::Group { children } = &shape.kind else {
        return shape.local_path();
    };
    let mut acc = BezPath::new();
    for c in children {
        let part = crate::shaping::overlay_with(
            &c.page_path(),
            &BezPath::new(),
            crate::shaping::Op::Weld,
            area_rule(c),
        );
        acc = crate::shaping::overlay(&acc, &part, crate::shaping::Op::Weld);
    }
    acc
}

fn contour(shape: &Shape, rule: crate::shaping::FillRule, p: &ContourParams) -> Vec<Shape> {
    let to_page = shape.transform;
    let to_local = if to_page.determinant().abs() > 1e-12 {
        to_page.inverse()
    } else {
        return Vec::new();
    };
    // A group: its objects' area as one, read even-odd as the union is,
    // and the look of the first object in it (#3245).
    let group = matches!(shape.kind, ShapeKind::Group { .. });
    let (base, rule) = if group {
        (to_page * group_area(shape), crate::shaping::FillRule::EvenOdd)
    } else {
        (to_page * shape.local_path(), rule)
    };
    let style = if group {
        group_style(shape).unwrap_or(shape)
    } else {
        shape
    };
    let from_fill = style.fill.preview_color().unwrap_or(Color::WHITE);
    let from_outline = style.stroke.as_ref().map(|s| s.color);
    let sign = if p.outside && !p.to_center { 1.0 } else { -1.0 };
    let offset = if p.offset.is_finite() {
        p.offset.abs().max(1e-3)
    } else {
        return Vec::new();
    };
    // An open curve: each step is a closed band round both sides of it,
    // the two offsets joined across its ends (#2577). Read as a filled
    // area, the curve was closed by a straight line from end to end and
    // the contour ran round that line too.
    // A line whose ends meet (as some files give a closed shape) is read
    // as the area it closes, as before.
    let open = matches!(shape.kind, ShapeKind::Path { closed: false, .. })
        && !base
            .elements()
            .iter()
            .any(|e| matches!(e, PathEl::ClosePath))
        && has_loose_end(&base);
    let step_path = |d: f64| {
        if open {
            crate::shaping::open_offset_band(&base, d, p.corners)
        } else {
            crate::shaping::offset_filled(&base, sign * d, p.corners, rule)
        }
    };
    let mut paths = Vec::new();
    if p.to_center && !open {
        for i in 1..=MAX_CONTOUR_STEPS {
            let q = crate::shaping::offset_filled(&base, sign * offset * i as f64, p.corners, rule);
            if q.elements().is_empty() {
                break;
            }
            paths.push(to_local * q);
        }
    } else {
        let n = p.steps.clamp(1, MAX_CONTOUR_STEPS);
        let total = offset * n as f64;
        for i in 1..=n {
            let d = total * accelerate(i as f64 / n as f64, p.accel_objects);
            let q = step_path(d);
            if q.elements().is_empty() {
                break;
            }
            paths.push(to_local * q);
        }
    }
    let n = paths.len().max(1) as f64;
    paths
        .into_iter()
        .enumerate()
        .map(|(i, path)| {
            let t = accelerate((i + 1) as f64 / n, p.accel_colors);
            let mut s = shape.clone();
            s.effects.clear();
            s.shadow = None;
            s.kind = ShapeKind::Path { path, closed: true };
            if group {
                s.fill = style.fill.clone();
                s.stroke = style.stroke.clone();
            }
            match &style.fill {
                Fill::None => {}
                // A fountain stays a fountain (#1265): each stop goes
                // toward the last step's colour at its place between the
                // first (fill) and the last (last fill) colours.
                Fill::Fountain(f) => {
                    let end = p.fill_end_to.unwrap_or(p.fill_to);
                    let mut g = f.clone();
                    for st in &mut g.stops {
                        let to = mix(p.fill_to, end, st.pos.clamp(0.0, 1.0), 0);
                        st.color = mix(st.color, to, t, p.color_blend);
                    }
                    s.fill = Fill::Fountain(g);
                }
                _ => s.fill = Fill::Solid(mix(from_fill, p.fill_to, t, p.color_blend)),
            }
            if let (Some(st), Some(from)) = (&mut s.stroke, from_outline) {
                st.color = mix(from, p.outline_to.unwrap_or(p.fill_to), t, p.color_blend);
            }
            s
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn blend(
    start: &Shape,
    end: &Shape,
    steps: u32,
    (accel_objects, accel_colors): (f64, f64),
    rotation: f64,
    path: Option<&BezPath>,
    rotate_on_path: bool,
    options: &BlendOptions,
) -> Vec<Shape> {
    let a = start.page_path();
    let b = end.page_path();
    let path_samples = path.map(|p| {
        let mut pts = Vec::new();
        kurbo::flatten(p.elements().iter().copied(), 0.05, |el| match el {
            PathEl::MoveTo(q) | PathEl::LineTo(q) => pts.push(q),
            _ => {}
        });
        let mut cum = vec![0.0];
        for w in pts.windows(2) {
            cum.push(cum.last().copied().unwrap_or(0.0) + (w[1] - w[0]).hypot());
        }
        (pts, cum)
    });
    // The part of the path the blend runs along: from where its start
    // sits to where its end sits (#2946).
    let (r0, r1) = options.path_ends();
    let path_len = path_samples
        .as_ref()
        .and_then(|(_, cum)| cum.last().copied())
        .unwrap_or(0.0)
        * (r1 - r0).abs();
    // Fixed spacing along a path sets the number of steps.
    let n = match options.spacing {
        Some(d) if d > 1e-6 && path_len > 0.0 => ((path_len / d).floor() as usize)
            .saturating_sub(1)
            .clamp(1, 999),
        _ => steps.clamp(1, MAX_BLEND_STEPS) as usize,
    };
    // Paired from the given nodes (in page space now) or by shape.
    let nodes = options.map_nodes.map(|m| {
        let (ts, te) = (start.transform, end.transform);
        [
            ts * m.start[0],
            ts * m.start[1],
            te * m.end[0],
            te * m.end[1],
        ]
    });
    // Each step's place between the two objects: along the whole path
    // the steps reach its ends; otherwise they sit between the objects.
    let even: Vec<f64> = (0..n)
        .map(|i| {
            if options.full_path && path_samples.is_some() && n > 1 {
                i as f64 / (n - 1) as f64
            } else {
                (i + 1) as f64 / (n + 1) as f64
            }
        })
        .collect();
    // The outline each step takes: the even morph, or with Apply to
    // sizing the morph at the step's accelerated place, so that its shape
    // and its size both follow the object acceleration, as the previews
    // of the .cdr files with that setting show (#1576).
    let looped = rotation != 0.0 && options.loop_rotation && path_samples.is_none();
    let morph_at: Vec<f64> = even
        .iter()
        .map(|t| {
            if options.accel_sizing {
                blend_place(*t, accel_objects)
            } else {
                *t
            }
        })
        .collect();
    // A loop turns the whole blend about the middle of the two objects:
    // each step is a step of the straight blend from the start to the end
    // object turned back about that middle by the whole rotation, then
    // turned forward by its share, so the last steps reach the end
    // object as it is (#1576).
    let mid = a
        .bounding_box()
        .center()
        .midpoint(b.bounding_box().center());
    let back = Affine::rotate_about(-rotation.to_radians(), mid);
    let cb_end = b.bounding_box().center();
    let b = if looped { back * b } else { b };
    let nodes = if looped {
        nodes.map(|[s0, s1, e0, e1]| [s0, s1, back * e0, back * e1])
    } else {
        nodes
    };
    let paths = crate::effects::blend_from_nodes_at(&a, &b, &morph_at, 120, nodes);
    // The same outline at both ends, only turned, scaled or moved (a copy
    // rotated): the steps take the transformation part way, so each turns
    // by an even share and keeps its shape. Mixed point by point, as two
    // different outlines are, the steps shrank toward the middle and
    // turned unevenly, the first and last most of all (#1260).
    let local = start.local_path();
    let turned =
        (nodes.is_none() && local == end.local_path()).then(|| local.bounding_box().center());
    let fa = start.fill.preview_color();
    let fb = end.fill.preview_color();
    // An outline on one object only: the steps take it, so a blend from
    // an object without an outline to an outlined one draws every step
    // outlined (they drew none when the start had none).
    let stroke = start.stroke.clone().or_else(|| end.stroke.clone());
    let ca = start
        .stroke
        .as_ref()
        .or(end.stroke.as_ref())
        .map(|s| s.color);
    let cb = end
        .stroke
        .as_ref()
        .or(start.stroke.as_ref())
        .map(|s| s.color);
    let wa = start
        .stroke
        .as_ref()
        .or(end.stroke.as_ref())
        .map(|s| s.width)
        .unwrap_or(0.0);
    let wb = end
        .stroke
        .as_ref()
        .or(start.stroke.as_ref())
        .map(|s| s.width)
        .unwrap_or(0.0);
    let ba = a.bounding_box();
    let bb = b.bounding_box();
    let ca_center = ba.center();
    let cb_center = bb.center();
    let mut out = Vec::new();
    for (i, p) in paths.into_iter().enumerate() {
        let t = even.get(i).copied().unwrap_or(0.5);
        let m = morph_at.get(i).copied().unwrap_or(t);
        let to = blend_place(t, accel_objects);
        let tc = blend_place(t, accel_colors);
        let mut s = start.clone();
        s.effects.clear();
        s.shadow = None;
        s.transform = Affine::IDENTITY;
        s.stroke = stroke.clone();
        let mut geo = match turned {
            Some(pivot) => {
                let g = lerp_transform(start.transform, end.transform, pivot, m) * local.clone();
                if looped {
                    // The copy turned back by its share of the whole
                    // rotation and moved onto the straight way to the end
                    // turned back. Mixing toward the end turned back took
                    // the shortest turn, wrong past half a turn.
                    let c = g.bounding_box().center();
                    Affine::translate((cb_center - cb_end) * m)
                        * Affine::rotate_about(-rotation.to_radians() * m, c)
                        * g
                } else {
                    g
                }
            }
            None => p,
        };
        let center = geo.bounding_box().center();
        let mut extra = Affine::IDENTITY;
        if looped {
            // A loop: the step of the straight blend to the end turned
            // back, moved to its accelerated place on that straight way
            // and turned by its share about the middle of the two objects
            // (see above). The steps leave the start and reach the end on
            // the curve of `loop_place`, and the outlines between a
            // square and a circle twist the way the files show (#1576).
            // Turning each step about its own centre and moving it onto
            // that curve gave smooth bands where the files show the steps
            // folding into the end object, and with an object
            // acceleration the steps packed at the start hardly turned.
            extra = Affine::rotate_about(rotation.to_radians() * to, mid)
                * Affine::translate((cb_center - ca_center) * (to - m));
        } else {
            // Each step turns about its own centre by its share of the
            // rotation.
            if rotation != 0.0 {
                extra = Affine::rotate_about(rotation.to_radians() * to, center);
            }
            if path_samples.is_none() && to != m {
                // Off a path the object acceleration moves the steps toward
                // one end: each goes where its accelerated place lies between
                // the two objects' centres (the steps stay evenly morphed and
                // sized unless Apply to sizing is on, which morphed them there
                // already). It moved only sizes and turns, so the slider
                // seemed to do nothing (#910).
                extra = Affine::translate((cb_center - ca_center) * (to - m)) * extra;
            }
        }
        if let Some((pts, cum)) = &path_samples {
            if pts.len() >= 2 {
                let total = *cum.last().unwrap_or(&0.0);
                let s_at = (r0 + (r1 - r0) * to) * total;
                let k = cum
                    .partition_point(|c| *c <= s_at)
                    .saturating_sub(1)
                    .min(pts.len() - 2);
                let seg = pts[k + 1] - pts[k];
                let len = seg.hypot().max(1e-9);
                let u = ((s_at - cum[k]) / len).clamp(0.0, 1.0);
                let pos = pts[k] + seg * u;
                // The step's own centre goes onto the path.
                extra = Affine::translate(pos - center) * extra;
                if rotate_on_path {
                    let ang = seg.y.atan2(seg.x);
                    extra = Affine::translate(pos.to_vec2())
                        * Affine::rotate(ang)
                        * Affine::translate(-pos.to_vec2())
                        * extra;
                }
            }
        }
        geo = extra * geo;
        // The steps of two open lines are open lines (#1024).
        let closed = geo
            .elements()
            .iter()
            .any(|e| matches!(e, PathEl::ClosePath));
        s.kind = ShapeKind::Path { path: geo, closed };
        if let (Some(x), Some(y)) = (fa, fb) {
            s.fill = Fill::Solid(blend_color(x, y, tc as f32, options.color_blend));
        }
        if let (Some(st), Some(x), Some(y)) = (&mut s.stroke, ca, cb) {
            st.color = blend_color(x, y, tc as f32, options.color_blend);
            st.width = wa + (wb - wa) * to;
        }
        // The uniform transparency goes from the start's to the end's
        // with the colours: every step took the start's, so a blend to a
        // faded end never faded.
        s.opacity = clean_opacity(start.opacity)
            + (clean_opacity(end.opacity) - clean_opacity(start.opacity)) * tc;
        out.push(s);
    }
    out
}

/// An object's opacity held to 0..1 (1 for a value that is no number).
fn clean_opacity(o: f64) -> f64 {
    if o.is_finite() {
        o.clamp(0.0, 1.0)
    } else {
        1.0
    }
}

/// Where the centre of a looped blend's step `t` of the way (0 to 1)
/// lies, for a blend from centre `a` to centre `b` turning by
/// `rotation` degrees: `a` turned by `rotation * t` about the middle of
/// the two and `b` turned back by `rotation * (1 - t)` about it, mixed
/// `t` of the way. The steps leave `a` and reach `b` along a curve that
/// bends to the right of the way for a positive (counterclockwise)
/// turn: a half circle about the middle at 180 degrees, a curl through
/// the middle at 360 (#1192: the pictures of the reporter's files at
/// -360 to 360 degrees show these curves; turning about the middle alone
/// left the last steps far from `b`).
#[cfg(test)]
fn loop_place(a: Point, b: Point, rotation: f64, t: f64) -> Point {
    let m = a.midpoint(b);
    let turn = rotation.to_radians();
    let from_a = Affine::rotate_about(turn * t, m) * a;
    let from_b = Affine::rotate_about(turn * (t - 1.0), m) * b;
    from_a.lerp(from_b, t)
}

/// The transformation `t` of the way from `a` to `b` (0 to 1): the point
/// `pivot` (before either) on the straight line between where each puts
/// it, and the turn, scale and slant in between, the turn the shorter way
/// round. Mixing the matrices' entries instead shrinks a turned copy.
fn lerp_transform(a: Affine, b: Affine, pivot: Point, t: f64) -> Affine {
    // The linear part as a turn by `angle` after an upper triangular
    // [[sx, k], [0, sy]] (a negative sy for a mirror).
    let parts = |m: Affine| {
        let [a, b, c, d, _, _] = m.as_coeffs();
        let angle = b.atan2(a);
        let sx = a.hypot(b);
        let (sin, cos) = angle.sin_cos();
        (angle, sx, cos * c + sin * d, -sin * c + cos * d)
    };
    let (aa, asx, ak, asy) = parts(a);
    let (ba, bsx, bk, bsy) = parts(b);
    let tau = std::f64::consts::TAU;
    let mut turn = (ba - aa).rem_euclid(tau);
    if turn > std::f64::consts::PI {
        turn -= tau;
    }
    let lerp = |x: f64, y: f64| x + (y - x) * t;
    let angle = aa + turn * t;
    let (sx, k, sy) = (lerp(asx, bsx), lerp(ak, bk), lerp(asy, bsy));
    let (sin, cos) = angle.sin_cos();
    let linear = Affine::new([
        cos * sx,
        sin * sx,
        cos * k - sin * sy,
        sin * k + cos * sy,
        0.0,
        0.0,
    ]);
    let at = (a * pivot).lerp(b * pivot, t);
    let m = Affine::translate(at.to_vec2()) * linear * Affine::translate(-pivot.to_vec2());
    if m.as_coeffs().iter().all(|v| v.is_finite()) {
        m
    } else {
        a
    }
}

/// A blend's further options.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BlendOptions {
    /// Along a path: the distance between steps, mm (the step count then
    /// follows from the path's length). `None`: the step count.
    pub spacing: Option<f64>,
    /// The steps' centres leave the straight way between the two
    /// objects along a curve the rotation bends ([`loop_place`]); without
    /// it each step only turns about its own centre.
    pub loop_rotation: bool,
    /// Along a path: the steps spread over the whole path, its ends
    /// included.
    pub full_path: bool,
    /// The object acceleration changes the steps' sizes too.
    pub accel_sizing: bool,
    pub color_blend: ColorBlend,
    /// The end object lies below the start object: the steps are drawn
    /// below the start, from the end's side up, so they sit between the
    /// two (kept up to date with the stacking order by the editor).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub steps_below: bool,
    /// The nodes the morph starts from: on each object, in its own
    /// coordinates, the node the steps pair with the other object's, then
    /// a point the outline passes next (the way it runs). The outlines are
    /// then paired from these nodes as they are, twists included, as a
    /// `.cdr` blend draws (#597). `None`: they are matched by shape, so
    /// the steps do not twist.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_nodes: Option<BlendNodes>,
    /// Along a path: where the start and the end of the blend sit on it,
    /// as shares of its length (0 its start, 1 its end), so the two
    /// objects can stand anywhere along it (#2946). `None`: its two ends.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_range: Option<[f64; 2]>,
}

impl BlendOptions {
    /// [`BlendOptions::path_range`] made safe: two shares from 0 to 1.
    pub fn path_ends(&self) -> (f64, f64) {
        let clean = |v: f64, or: f64| if v.is_finite() { v.clamp(0.0, 1.0) } else { or };
        match self.path_range {
            Some([a, b]) => (clean(a, 0.0), clean(b, 1.0)),
            None => (0.0, 1.0),
        }
    }
}

/// The point `share` of the way along `path` (0 its start, 1 its end), by
/// length, and the direction it runs there; `None` for a path of no
/// length (#2946).
pub fn point_along(path: &BezPath, share: f64) -> Option<(Point, Vec2)> {
    let mut pts = Vec::new();
    kurbo::flatten(path.elements().iter().copied(), 0.05, |el| match el {
        PathEl::MoveTo(q) | PathEl::LineTo(q) => pts.push(q),
        _ => {}
    });
    let mut cum = vec![0.0];
    for w in pts.windows(2) {
        cum.push(cum.last().copied().unwrap_or(0.0) + (w[1] - w[0]).hypot());
    }
    let total = cum.last().copied().unwrap_or(0.0);
    if pts.len() < 2 || total <= 1e-9 || !share.is_finite() {
        return None;
    }
    let s_at = share.clamp(0.0, 1.0) * total;
    let k = cum
        .partition_point(|c| *c <= s_at)
        .saturating_sub(1)
        .min(pts.len() - 2);
    let seg = pts[k + 1] - pts[k];
    let len = seg.hypot().max(1e-9);
    let u = ((s_at - cum[k]) / len).clamp(0.0, 1.0);
    Some((pts[k] + seg * u, seg))
}

/// See [`BlendOptions::map_nodes`]: each pair is a first node and a point
/// after it along the outline, in that object's own coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BlendNodes {
    pub start: [Point; 2],
    pub end: [Point; 2],
}

impl BlendOptions {
    pub fn is_default(&self) -> bool {
        *self == BlendOptions::default()
    }
}

/// The body an extrude effect on `main` describes (older files: Back
/// parallel, or Small back with a vanishing point). `None` for a body
/// that needs a vanishing point and has none.
#[allow(clippy::too_many_arguments)]
pub fn extrude_body(
    main: &Shape,
    depth: Vec2,
    vanishing: Option<Point>,
    amount: f64,
    (shade_from, shade_to, solid): (Option<Color>, Option<Color>, Option<Color>),
    (light_angle, light_intensity): (f64, f64),
    kind: Option<crate::extrude::ExtrudeKind>,
    on_object: bool,
) -> Option<crate::extrude::Body> {
    use crate::extrude::{Body, Coloring, ExtrudeKind};
    let kind = kind.unwrap_or(if vanishing.is_some() {
        ExtrudeKind::SmallBack
    } else {
        ExtrudeKind::BackParallel
    });
    let vp = match (kind.parallel(), vanishing) {
        (true, _) => Point::ZERO,
        (false, Some(v)) if on_object => main.transform * v,
        (false, Some(v)) => v,
        (false, None) => return None,
    };
    let base = main.fill.preview_color().unwrap_or(Color::WHITE);
    let coloring = match (solid, shade_from) {
        (Some(c), _) => Coloring::Solid(c),
        (None, Some(f)) => Coloring::Shade(
            f,
            shade_to.unwrap_or_else(|| lerp_color(base, Color::BLACK, 0.5)),
        ),
        (None, None) => Coloring::Object,
    };
    Some(Body {
        kind,
        vector: depth,
        vanishing: vp,
        depth: if amount.is_finite() {
            amount.clamp(0.0, 0.99)
        } else {
            0.0
        },
        coloring,
        light_angle,
        light_intensity,
        bevel: 0.0,
        solid: Default::default(),
    })
}

/// The most steps a bevel band is cut into.
pub const MAX_BEVEL_LEVEL: u8 = 8;

/// The wedges a directional bevel's light is toned in, round the object.
const BEVEL_WEDGES: usize = 24;

fn one_level() -> u8 {
    1
}

/// The shapes a [`Effect::Bevel`] draws over `shape`: the band cut into
/// `level` steps, each split into a lit and a shaded half by the light
/// direction. Soft edge puts the band inside the outline, emboss around it.
fn bevel(shape: &Shape, effect: &Effect) -> Vec<Shape> {
    let Effect::Bevel {
        distance,
        light_angle,
        intensity,
        style,
        shadow_color,
        light_color,
        level,
        emboss_color,
        directional,
    } = effect
    else {
        return Vec::new();
    };
    let path = shape.local_path();
    let width = if distance.is_finite() {
        distance.abs()
    } else {
        return Vec::new();
    };
    let steps = (*level).clamp(1, MAX_BEVEL_LEVEL) as usize;
    // Offsets of the step boundaries, from the band's outer edge in.
    let reach = |j: usize| {
        let t = j as f64 / steps as f64;
        match style {
            1 => width * (1.0 - t),
            2 => width * (0.5 - t),
            _ => -width * t,
        }
    };
    let edge = |j: usize| {
        let d = reach(j);
        if d == 0.0 {
            path.clone()
        } else {
            crate::shaping::offset(&path, d)
        }
    };
    let base = emboss_color
        .or_else(|| shape.fill.preview_color())
        .unwrap_or(Color::WHITE);
    let k = (intensity / 100.0).clamp(0.0, 1.0);
    let ldir = Vec2::new(
        light_angle.to_radians().cos(),
        light_angle.to_radians().sin(),
    );
    let mut outer = edge(0);
    let b = outer.bounding_box();
    let c = b.center();
    // Lit half: the part of the band on the light side of the centre line.
    let big = b.inflate(b.width(), b.height());
    let half = crate::effects::polygon(&[
        c + Vec2::new(-ldir.y, ldir.x) * big.width() * 2.0,
        c + Vec2::new(ldir.y, -ldir.x) * big.width() * 2.0,
        c + Vec2::new(ldir.y, -ldir.x) * big.width() * 2.0 + ldir * big.width() * 2.0,
        c + Vec2::new(-ldir.y, ldir.x) * big.width() * 2.0 + ldir * big.width() * 2.0,
    ]);
    // Directional light: wedges round the centre, each toned by how
    // squarely it faces the light (1 facing it, -1 facing away).
    let wedges: Vec<(BezPath, f64)> = if *directional {
        let reach = big.width().max(big.height()) * 2.0;
        (0..BEVEL_WEDGES)
            .map(|w| {
                let step = std::f64::consts::TAU / BEVEL_WEDGES as f64;
                let a0 = step * w as f64;
                let a1 = a0 + step;
                let at = |a: f64| c + Vec2::new(a.cos(), a.sin()) * reach;
                let mid = a0 + step / 2.0;
                let facing = mid.cos() * ldir.x + mid.sin() * ldir.y;
                (crate::effects::polygon(&[c, at(a0), at(a1)]), facing)
            })
            .collect()
    } else {
        Vec::new()
    };
    let mut out = Vec::new();
    for i in 0..steps {
        let inner = edge(i + 1);
        if outer.elements().is_empty() || inner.elements().is_empty() {
            break;
        }
        let band = crate::shaping::overlay(&outer, &inner, crate::shaping::Op::Trim);
        // The light fades from the band's outer edge inwards.
        let strength = (k * (steps - i) as f64 / steps as f64) as f32;
        let parts: Vec<(BezPath, Color)> = if *directional {
            wedges
                .iter()
                .map(|(wedge, facing)| {
                    let p = crate::shaping::overlay(&band, wedge, crate::shaping::Op::Intersect);
                    let f = (*facing as f32 * strength).clamp(-1.0, 1.0);
                    let col = if f >= 0.0 {
                        lerp_color(base, *light_color, f)
                    } else {
                        lerp_color(base, *shadow_color, -f)
                    };
                    (p, col)
                })
                .collect()
        } else {
            let lit = crate::shaping::overlay(&band, &half, crate::shaping::Op::Intersect);
            let dark = crate::shaping::overlay(&band, &half, crate::shaping::Op::Trim);
            vec![
                (lit, lerp_color(base, *light_color, strength)),
                (dark, lerp_color(base, *shadow_color, strength)),
            ]
        };
        for (p, col) in parts {
            if p.elements().is_empty() {
                continue;
            }
            let mut s = shape.clone();
            s.effects.clear();
            s.shadow = None;
            s.stroke = None;
            s.kind = ShapeKind::Path {
                path: p,
                closed: true,
            };
            s.fill = Fill::Solid(col);
            out.push(s);
        }
        outer = inner;
    }
    out
}

/// Carry the places an object's live effects hold with the object when
/// `t` is applied to it (in the space its transform maps to): a
/// symmetry's centre and the angle of its first mirror line, so its
/// copies go with it. Left behind, the copies mirrored a move about the
/// old centre and a scale or a turn rearranged them, as if the symmetry
/// were still being edited. A move, a turn, a uniform scale and a mirror
/// carry it exactly. A stretch or a slant cannot keep the copies mirror
/// images of the changed whole; the centre goes with it and the lines
/// keep their angle (mirrored when `t` mirrors), so the copies stay mirror
/// images of the changed object.
pub fn carry_effects<'a>(effects: impl IntoIterator<Item = &'a mut Effect>, t: Affine) {
    let [a, b, c, d, _, _] = t.as_coeffs();
    let scale = a.hypot(b).max(c.hypot(d)).max(1e-12);
    let near = |x: f64, y: f64| (x - y).abs() <= 1e-9 * scale;
    // A turn and an even scale, or that and a mirror.
    let similar = (near(a, d) && near(b, -c)) || (near(a, -d) && near(b, c));
    let mirrors = a * d - b * c < 0.0;
    for e in effects {
        if let Effect::Symmetry { center, angle, .. } = e {
            let p = t * *center;
            if !(p.x.is_finite() && p.y.is_finite()) {
                continue;
            }
            if similar {
                let r = angle.to_radians();
                let dir = t * (*center + Vec2::new(r.cos(), r.sin())) - p;
                if dir.hypot() > 1e-12 && dir.hypot().is_finite() {
                    *angle = dir.y.atan2(dir.x).to_degrees();
                }
            } else if mirrors {
                *angle = -*angle;
            }
            // A line is the same turned half a turn.
            *angle = angle.rem_euclid(180.0);
            *center = p;
        }
    }
}

/// Mirror copies for the Symmetry effect. With `n` lines through `center`
/// the result is the dihedral orbit of the shape: reflections across each
/// line and the rotations they generate (2n - 1 copies).
/// The mirror maps of a symmetry, page to page: one reflection a mirror
/// line, then the rotations between them (the identity left out).
/// The transformation that takes the point `p`, in one of a symmetry's
/// copies, back to the original's side: the slice between two mirror
/// lines that holds `origin` (the original's middle). The identity when
/// `p` is on that side already (#2998: an area filled in a copy goes on
/// the original, so its mirrors fill the other copies).
pub fn symmetry_back_to(center: Point, angle: f64, lines: u8, origin: Point, p: Point) -> Affine {
    symmetry_back_to_turned(center, angle, lines, false, origin, p)
}

/// [`symmetry_back_to`] for a symmetry that turns its copies when
/// `rotate` is set (#2999): the slices are then whole turns shared out.
pub fn symmetry_back_to_turned(
    center: Point,
    angle: f64,
    lines: u8,
    rotate: bool,
    origin: Point,
    p: Point,
) -> Affine {
    let n = lines.clamp(1, 12) as usize;
    let slice = std::f64::consts::PI / n as f64 * if rotate { 2.0 } else { 1.0 };
    let wedges = if rotate { n } else { 2 * n } as i64;
    let a0 = angle.to_radians();
    let wedge = |q: Point| -> Option<i64> {
        let d = q - center;
        if d.hypot() < 1e-9 {
            return None;
        }
        let a = (d.y.atan2(d.x) - a0).rem_euclid(std::f64::consts::TAU);
        Some(((a / slice).floor() as i64).rem_euclid(wedges))
    };
    let Some(target) = wedge(origin) else {
        return Affine::IDENTITY;
    };
    std::iter::once(Affine::IDENTITY)
        .chain(symmetry_mirrors(center, angle, lines, rotate))
        .map(|m| m.inverse())
        .find(|back| wedge(*back * p) == Some(target))
        .unwrap_or(Affine::IDENTITY)
}

fn symmetry_mirrors(center: Point, angle: f64, lines: u8, rotate: bool) -> Vec<Affine> {
    let n = lines.clamp(1, 12) as usize;
    let mut out = Vec::with_capacity(2 * n);
    let to_center = Affine::translate(center.to_vec2());
    let from_center = Affine::translate(-center.to_vec2());
    if rotate {
        // Turned copies only: `n` in all, the object the first (#2999).
        return (1..n)
            .map(|k| {
                let rot = Affine::rotate(std::f64::consts::TAU * k as f64 / n as f64);
                to_center * rot * from_center
            })
            .collect();
    }
    let reflect = |theta: f64| -> Affine {
        // Reflection across a line through the origin at angle theta.
        let (s, c) = (2.0 * theta).sin_cos();
        Affine::new([c, s, s, -c, 0.0, 0.0])
    };
    let a0 = angle.to_radians();
    for k in 0..n {
        let theta = a0 + std::f64::consts::PI * k as f64 / n as f64;
        out.push(to_center * reflect(theta) * from_center);
    }
    // Rotations (products of two reflections), skipping the identity.
    for k in 1..n {
        let rot = Affine::rotate(2.0 * std::f64::consts::PI * k as f64 / n as f64);
        out.push(to_center * rot * from_center);
    }
    out
}

/// How the area of `shape`'s outline is read: text (warped text too,
/// whose outline is a curve by the time an effect reads it) and objects
/// set to the nonzero rule have contours that overlap without making
/// holes (#2156).
fn area_rule(shape: &Shape) -> crate::shaping::FillRule {
    if matches!(shape.kind, ShapeKind::Text { .. }) {
        crate::shaping::FillRule::NonZero
    } else {
        shape.fill_rule
    }
}

/// The outlines `shape` covers the page with, each with the rule it is
/// read by: its own (`rule` when given), or those of the objects of a
/// group, `parent` taking them to the page.
fn cast_parts(
    shape: &Shape,
    parent: Affine,
    rule: Option<crate::shaping::FillRule>,
    depth: usize,
    out: &mut Vec<(BezPath, crate::shaping::FillRule)>,
) {
    if !shape.visible || depth > 64 {
        return;
    }
    if let ShapeKind::Group { children } = &shape.kind {
        for c in children {
            cast_parts(c, parent * shape.transform, None, depth + 1, out);
        }
        return;
    }
    let path = parent * shape.page_path();
    if !path.elements().is_empty() {
        out.push((path, rule.unwrap_or_else(|| area_rule(shape))));
    }
}

/// The area [`cast_area`] joined last, with the object, its rule and the
/// objects it was joined from. A drag of the shadow changes the offset in
/// every frame and nothing of the area, and joining it costs as much as
/// sweeping it.
type CastKept = (Shape, crate::shaping::FillRule, Vec<Shape>, BezPath);
static CAST: std::sync::Mutex<Option<CastKept>> = std::sync::Mutex::new(None);

/// What a block shadow is cast from, with the rule to read it by: the
/// object's own outline, or, for a group and for an object whose effects
/// have drawn objects of their own (`drawn`: the body of an extrusion,
/// the steps of a contour), the area all of them cover together. Each
/// outline is made clean contours first, so that together they fill,
/// with the nonzero rule, what any of them covers: read as one outline
/// with the even-odd rule, two objects of a group were a hole where they
/// overlap, and the body of an extruded object cast nothing (#2218).
fn cast_area<'a>(
    shape: &Shape,
    rule: crate::shaping::FillRule,
    drawn: impl Iterator<Item = &'a Shape>,
) -> (BezPath, crate::shaping::FillRule) {
    use crate::shaping::{overlay_with, FillRule, Op};
    let drawn: Vec<Shape> = drawn.filter(|d| d.fill != Fill::None).cloned().collect();
    let mut parts = Vec::new();
    cast_parts(shape, Affine::IDENTITY, Some(rule), 0, &mut parts);
    for d in &drawn {
        cast_parts(d, Affine::IDENTITY, None, 0, &mut parts);
    }
    if parts.len() == 1 {
        if let Some(only) = parts.pop() {
            return only;
        }
    }
    // The shadow's own settings are not part of what casts it.
    let mut from = shape.clone();
    from.effects.clear();
    if let Ok(kept) = CAST.lock() {
        if let Some((s, r, d, area)) = kept.as_ref() {
            if *s == from && *r == rule && *d == drawn {
                return (area.clone(), FillRule::NonZero);
            }
        }
    }
    let empty = BezPath::new();
    let mut all = BezPath::new();
    for (path, rule) in &parts {
        all.extend(overlay_with(path, &empty, Op::Weld, *rule));
    }
    if let Ok(mut kept) = CAST.lock() {
        *kept = Some((from, rule, drawn, all.clone()));
    }
    (all, FillRule::NonZero)
}

/// The solid the area `front` (read by `rule`) makes slid along `offset`
/// (page space), each of its contours swept within itself, its own area
/// included, less the area grown by `gap`. `shape` gives the rest.
fn block_shadow(
    shape: &Shape,
    (front, rule): (BezPath, crate::shaping::FillRule),
    offset: Vec2,
    color: Color,
    gap: f64,
) -> Vec<Shape> {
    let mut solid = crate::shaping::sweep(&front, offset, rule);
    if solid.elements().is_empty() {
        return Vec::new();
    }
    if gap > 0.0 {
        let grown = crate::shaping::offset_filled(&front, gap, crate::shaping::Join::Round, rule);
        solid = crate::shaping::overlay(&solid, &grown, crate::shaping::Op::Trim);
    }
    let mut s = shape.clone();
    s.effects.clear();
    s.shadow = None;
    s.transform = Affine::IDENTITY;
    s.stroke = None;
    s.kind = ShapeKind::Path {
        path: solid,
        closed: true,
    };
    s.fill = Fill::Solid(color);
    vec![s]
}

/// Flatten, subdivide long segments and map every point through `map`, so
/// straight edges bend with the warp. Where the warp bends hard a piece
/// of 1/64 of the object's size is halved again, so a tight bend is not
/// drawn in visible facets (#881); elsewhere the points are as many as
/// before and so are the calls to `map`.
pub(crate) fn warp(path: &BezPath, b: Rect, map: impl Fn(Point) -> Point) -> BezPath {
    let size = b.width().max(b.height());
    let step = (size / 64.0).max(size * 1e-4).max(1e-9);
    // Curves are flattened finely for the object's own size: a fixed
    // tolerance made letters, whose own space is small and scaled up by
    // their transform, come out in facets (#455).
    let tolerance = (size * 2e-5).clamp(1e-7, 0.05);
    // How far where a piece's middle maps to may lie off its mapped
    // chord.
    let deviation = (size * 1e-5).clamp(1e-8, 0.01);
    let mut out = BezPath::new();
    // The last point, where it maps to, and the subpath's start.
    let mut last: Option<(Point, Point)> = None;
    let mut start: Option<(Point, Point)> = None;
    let mut line = WarpLine::default();
    kurbo::flatten(path.elements().iter().copied(), tolerance, |el| match el {
        PathEl::MoveTo(p) => {
            let m = map(p);
            out.move_to(m);
            last = Some((p, m));
            start = last;
        }
        PathEl::LineTo(p) => match last {
            Some(from) => {
                line.run(&map, from, p, step, deviation);
                line.out.iter().for_each(|q| out.line_to(*q));
                last = Some((p, line.end));
            }
            None => {
                let m = map(p);
                out.line_to(m);
                last = Some((p, m));
            }
        },
        PathEl::ClosePath => {
            if let (Some(from), Some(s)) = (last, start) {
                if (s.0 - from.0).hypot() > 0.0 {
                    line.run(&map, from, s.0, step, deviation);
                    // The last point is the start, which the close reaches.
                    line.out.pop();
                    line.out.iter().for_each(|q| out.line_to(*q));
                }
            }
            out.close_path();
            last = start;
        }
        _ => {}
    });
    out
}

/// How many times a piece of a warped line may be halved: up to 64
/// pieces for each 1/64 of the object's size.
const WARP_HALVINGS: u32 = 6;

/// One line of a warped outline, divided and mapped; its buffers are
/// kept from line to line.
#[derive(Default)]
struct WarpLine {
    src: Vec<Point>,
    mapped: Vec<Point>,
    /// How far each mapped point lies off its neighbours' chord (0 at
    /// the ends).
    off: Vec<f64>,
    /// The mapped points after the line's start, its end included.
    out: Vec<Point>,
    /// Where the line's end maps to.
    end: Point,
}

impl WarpLine {
    /// Map the line from `from` (a point and where it maps to) to `to`
    /// in pieces of at most `step`. A piece is halved where the warp
    /// bends it more than `deviation` off its chord: judged without more
    /// calls to `map` from how far each point lies off the chord of its
    /// neighbours (on a straight line only the warp bends them; a piece
    /// strays from its chord about a quarter of that), then measured at
    /// its middle. A line of one piece (a curve's flattening) is measured
    /// only when longer than a quarter of `step`: a shorter one strays
    /// sixteen times less than the pieces of `step` drawn before.
    fn run(
        &mut self,
        map: &impl Fn(Point) -> Point,
        from: (Point, Point),
        to: Point,
        step: f64,
        deviation: f64,
    ) {
        let (l, ml) = from;
        let len = (to - l).hypot();
        let n = (len / step).ceil().clamp(1.0, 1e6) as usize;
        self.src.clear();
        self.mapped.clear();
        self.out.clear();
        self.src.push(l);
        self.mapped.push(ml);
        for i in 1..=n {
            let at = l + (to - l) * (i as f64 / n as f64);
            self.src.push(at);
            self.mapped.push(map(at));
        }
        self.end = self.mapped.last().copied().unwrap_or(ml);
        // How far each inner point lies off its neighbours' chord.
        self.off.clear();
        if n > 1 {
            self.off.push(0.0);
            for w in self.mapped.windows(3) {
                self.off.push(off_chord(w[1], w[0], w[2]));
            }
        }
        let off = &self.off;
        for i in 0..n {
            let bent = if n == 1 {
                len > step * 0.25
            } else {
                // The bend at either end of the piece, or at its inner
                // neighbour's for the first and the last piece.
                let near = [i, i + 1, i.max(1), (i + 1).min(n - 1)];
                near.iter()
                    .any(|k| off.get(*k).copied().unwrap_or(0.0) * 0.25 > deviation)
            };
            let a = (self.src[i], self.mapped[i]);
            let c = (self.src[i + 1], self.mapped[i + 1]);
            if bent {
                refine(map, a, c, deviation, WARP_HALVINGS, &mut self.out);
            } else {
                self.out.push(c.1);
            }
        }
    }
}

/// How far `p` lies off the line through `a` and `b` (off `a` when they
/// meet).
fn off_chord(p: Point, a: Point, b: Point) -> f64 {
    let chord = b - a;
    let len = chord.hypot();
    if len > 1e-12 {
        (p - a).cross(chord).abs() / len
    } else {
        (p - a).hypot()
    }
}

/// The mapped points after `from` up to `to` (each a point and where it
/// maps to), the piece halved while where its middle maps to lies more
/// than `deviation` off the mapped chord.
fn refine(
    map: &impl Fn(Point) -> Point,
    from: (Point, Point),
    to: (Point, Point),
    deviation: f64,
    halvings: u32,
    out: &mut Vec<Point>,
) {
    if halvings > 0 {
        let mid = from.0.midpoint(to.0);
        let m = map(mid);
        // Off the mapped chord, not along it: a warp that only stretches
        // a straight piece needs no more points there.
        let off = off_chord(m, from.1, to.1);
        if off.is_finite() && off > deviation {
            refine(map, from, (mid, m), deviation, halvings - 1, out);
            refine(map, (mid, m), to, deviation, halvings - 1, out);
            return;
        }
    }
    out.push(to.1);
}

/// Projective map of the bounding box onto `corners` (BL, BR, TR, TL).
/// A point through a homography from [`homography`].
pub fn map_homography(h: &[f64; 8], p: Point) -> Point {
    let w = h[6] * p.x + h[7] * p.y + 1.0;
    let w = if w.abs() < 1e-12 { 1e-12 } else { w };
    Point::new(
        (h[0] * p.x + h[1] * p.y + h[2]) / w,
        (h[3] * p.x + h[4] * p.y + h[5]) / w,
    )
}

pub fn perspective(path: &BezPath, corners: &[Point; 4]) -> BezPath {
    perspective_in(path, path.bounding_box(), corners)
}

/// As [`perspective`], with the box `b` mapped onto the corners instead
/// of the path's own bounding box.
///
/// The nodes and the control points of the outline are mapped, so a
/// segment stays one segment: a straight side stays straight (a projective
/// map keeps lines), and a curve bends as its handles do, so a side of the
/// box pulled to a point brings the handles along it to that point too
/// and the outline comes to a tip there (an ellipse in a triangle is a
/// drop, #879). Only where some of the outline lies past the horizon (a
/// frame folded over itself) is the outline flattened and mapped point by
/// point instead.
pub fn perspective_in(path: &BezPath, b: Rect, corners: &[Point; 4]) -> BezPath {
    if b.width() < 1e-9 || b.height() < 1e-9 {
        return path.clone();
    }
    let src = [
        Point::new(b.x0, b.y0),
        Point::new(b.x1, b.y0),
        Point::new(b.x1, b.y1),
        Point::new(b.x0, b.y1),
    ];
    let corners = corners_apart(corners);
    let Some(h) = homography(&src, &corners) else {
        return path.clone();
    };
    let w = |p: Point| h[6] * p.x + h[7] * p.y + 1.0;
    // The side of the horizon the box's middle is on; a point closer to
    // the horizon than a billionth of that counts as past it.
    let wc = w(b.center());
    let near = wc.abs() * 1e-9;
    let map = |p: Point| map_homography(&h, p);
    let before_horizon = path.elements().iter().all(|e| {
        let pts: &[Point] = match e {
            PathEl::MoveTo(p) | PathEl::LineTo(p) => std::slice::from_ref(p),
            PathEl::QuadTo(a, p) => &[*a, *p][..],
            PathEl::CurveTo(a, c, p) => &[*a, *c, *p][..],
            PathEl::ClosePath => &[],
        };
        pts.iter().all(|p| w(*p) * wc.signum() > near)
    });
    if !before_horizon {
        return warp(path, b, map);
    }
    let mut out = BezPath::new();
    for e in path.elements() {
        match *e {
            PathEl::MoveTo(p) => out.move_to(map(p)),
            PathEl::LineTo(p) => out.line_to(map(p)),
            PathEl::QuadTo(a, p) => out.quad_to(map(a), map(p)),
            PathEl::CurveTo(a, c, p) => out.curve_to(map(a), map(c), map(p)),
            PathEl::ClosePath => out.close_path(),
        }
    }
    out
}

/// Perspective corners (BL, BR, TR, TL) with two neighbours that meet (a
/// side pulled to a point), or that were dragged a little past each other
/// on the way there, set a hair apart along the side across from them, in
/// its direction: the frame stays a quadrilateral a projective map can take
/// the box to, and the outline comes to a tip there (#879). Corners that
/// crossed by more than a fiftieth of the frame (a frame folded on
/// purpose) and all others stay as given.
pub(crate) fn corners_apart(corners: &[Point; 4]) -> [Point; 4] {
    let mut c = *corners;
    let span = c
        .iter()
        .flat_map(|p| c.iter().map(move |q| (*p - *q).hypot()))
        .fold(0.0f64, f64::max);
    if !(span.is_finite() && span > 0.0) {
        return c;
    }
    let eps = span * 1e-4;
    for i in 0..4 {
        let j = (i + 1) % 4;
        let gap = c[j] - c[i];
        if gap.hypot() > span * 0.02 {
            continue;
        }
        // The side across, in the same direction (TL to TR across from
        // BL to BR).
        let along = c[(i + 2) % 4] - c[(i + 3) % 4];
        let len = along.hypot();
        let d = if len > 1e-12 {
            along / len
        } else {
            Vec2::new(1.0, 0.0)
        };
        let crossed = gap.dot(d) < 0.0;
        if gap.hypot() > eps * 0.5 && !crossed {
            continue;
        }
        let mid = c[i].midpoint(c[j]);
        c[i] = mid - d * eps;
        c[j] = mid + d * eps;
    }
    c
}

/// Give a group's `member` the group's perspectives as it is ungrouped
/// (#674): the group (outline `group_path`, effects `group`) warps the
/// member's outline before the member's own effects run, so the member
/// takes one Perspective at the front of its stack, the group's
/// projections seen in its own space (a homography restricted to the
/// member is still one). A Perspective the member had first already is
/// folded into it, so it keeps one set of corners. Drawn as it was in the
/// group; other group effects are not carried.
pub fn take_group_perspectives(member: &mut Shape, group_path: &BezPath, group: &[Effect]) {
    // Nothing to take, and nothing to copy: the group's outline was
    // copied for each of its objects, which made Ungroup of a group of
    // thousands take seconds.
    if !group
        .iter()
        .any(|e| matches!(e, Effect::Perspective { .. }))
    {
        return;
    }
    let t = member.transform;
    if t.determinant().abs() < 1e-12 {
        return;
    }
    let inv = t.inverse();
    let raw = member.local_path();
    let mb = raw.bounding_box();
    if mb.width() < 1e-9 || mb.height() < 1e-9 {
        return;
    }
    let rect = |b: Rect| {
        [
            Point::new(b.x0, b.y0),
            Point::new(b.x1, b.y0),
            Point::new(b.x1, b.y1),
            Point::new(b.x0, b.y1),
        ]
    };
    // The member's box corners through each group perspective in turn,
    // each on the box of the group's outline as the one before left it.
    let mut corners = rect(mb);
    let mut path = group_path.clone();
    let mut any = false;
    for e in group {
        let Effect::Perspective { corners: to } = e else {
            continue;
        };
        let b = path.bounding_box();
        if b.width() < 1e-9 || b.height() < 1e-9 {
            continue;
        }
        let Some(h) = homography(&rect(b), to) else {
            continue;
        };
        corners = corners.map(|q| inv * map_homography(&h, t * q));
        path = perspective_in(&path, b, to);
        any = true;
    }
    if !any || !corners.iter().all(|p| p.x.is_finite() && p.y.is_finite()) {
        return;
    }
    let mut stack = member.effect_stack();
    // The member's own first perspective ran on the warped outline's box.
    let folded = match stack.first() {
        Some((Effect::Perspective { corners: own }, true)) => {
            let b = perspective_in(&raw, mb, &corners).bounding_box();
            homography(&rect(b), own).map(|h| corners.map(|g| map_homography(&h, g)))
        }
        _ => None,
    };
    match folded {
        Some(c) if c.iter().all(|p| p.x.is_finite() && p.y.is_finite()) => {
            stack[0].0 = Effect::Perspective { corners: c };
        }
        _ => stack.insert(0, (Effect::Perspective { corners }, true)),
    }
    let (effects, parked) = Shape::split_stack(stack);
    member.effects = effects;
    member.parked_effects = parked;
}

/// Solve the 8 parameters of a homography from 4 point pairs.
pub fn homography(src: &[Point; 4], dst: &[Point; 4]) -> Option<[f64; 8]> {
    let mut m = [[0.0f64; 9]; 8];
    for i in 0..4 {
        let (x, y) = (src[i].x, src[i].y);
        let (u, v) = (dst[i].x, dst[i].y);
        m[2 * i] = [x, y, 1.0, 0.0, 0.0, 0.0, -u * x, -u * y, u];
        m[2 * i + 1] = [0.0, 0.0, 0.0, x, y, 1.0, -v * x, -v * y, v];
    }
    // Gaussian elimination.
    for col in 0..8 {
        let mut pivot = col;
        for r in col + 1..8 {
            if m[r][col].abs() > m[pivot][col].abs() {
                pivot = r;
            }
        }
        if m[pivot][col].abs() < 1e-12 {
            return None;
        }
        m.swap(col, pivot);
        let p = m[col][col];
        for v in m[col].iter_mut() {
            *v /= p;
        }
        let pivot_row = m[col];
        for (r, row) in m.iter_mut().enumerate() {
            if r != col {
                let f = row[col];
                for (v, pv) in row.iter_mut().zip(pivot_row.iter()) {
                    *v -= f * pv;
                }
            }
        }
    }
    let mut h = [0.0; 8];
    for i in 0..8 {
        h[i] = m[i][8];
    }
    Some(h)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn transform_fx(copies: u32, angle: f64, scale: f64, random: bool) -> Effect {
        Effect::Transform {
            scale_x: scale,
            scale_y: scale,
            move_x: 0.0,
            move_y: 0.0,
            angle,
            copies,
            reflect_x: false,
            reflect_y: false,
            random,
            seed: 7,
            scale_strokes: false,
            pivot_x: 0.0,
            pivot_y: 0.0,
        }
    }

    /// Transform about a point of one's own (#3607): copies turned about
    /// a centre away from the object lie round it, as the hours of a
    /// clock, each the same size and at the same distance.
    #[test]
    fn transform_copies_turn_round_a_centre_of_their_own() {
        let mut s = Shape::new(
            crate::ShapeId(1),
            ShapeKind::Rect {
                rect: Rect::new(-1.0, -41.0, 1.0, -39.0),
                radius: 0.0,
                corners: None,
            },
        );
        let mut fx = transform_fx(11, 30.0, 100.0, false);
        if let Effect::Transform {
            pivot_x, pivot_y, ..
        } = &mut fx
        {
            // The clock's centre, 40 mm under the object's.
            *pivot_x = 0.0;
            *pivot_y = 40.0;
        }
        s.effects = vec![fx.clone()];
        let ev = evaluate(&s);
        assert_eq!(ev.below.len(), 11);
        for c in &ev.below {
            let b = c.bounds();
            assert!((b.center().to_vec2().hypot() - 40.0).abs() < 1e-6, "{b:?}");
            assert!((b.width() - b.height()).abs() < 1e-6);
        }
        // The sixth copy is opposite the object.
        let six = ev.below[11 - 6].bounds().center();
        assert!((six.y - 40.0).abs() < 1e-6 && six.x.abs() < 1e-6, "{six:?}");
        // Saved and read back; without a pivot the field is left out.
        let json = serde_json::to_string(&fx).expect("save");
        assert!(json.contains("pivot_y"), "{json}");
        let back: Effect = serde_json::from_str(&json).expect("read");
        assert_eq!(back, fx);
        let plain = serde_json::to_string(&transform_fx(1, 0.0, 50.0, false)).unwrap();
        assert!(!plain.contains("pivot"), "{plain}");
    }

    /// Grid: copies right and down from the object in cells of its box
    /// with the gaps between; every other row shifted; the object stays
    /// the top left cell; the cell count is kept within its limits.
    #[test]
    fn grid_effect_repeats_in_rows_and_columns() {
        let mut s = Shape::new(
            crate::ShapeId(1),
            ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 10.0, 5.0),
                radius: 0.0,
                corners: None,
            },
        );
        s.effects = vec![Effect::Grid {
            columns: 3,
            rows: 2,
            gap_x: 2.0,
            gap_y: -1.0,
            shift_x: 4.0,
            shift_y: 0.0,
        }];
        let ev = evaluate(&s);
        assert_eq!(ev.below.len(), 5);
        assert_eq!(ev.main.bounds(), s.bounds());
        let mut boxes: Vec<(f64, f64)> = ev
            .below
            .iter()
            .map(|c| (c.bounds().x0, c.bounds().y0))
            .collect();
        boxes.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
        let mut want = vec![
            (12.0, 0.0),
            (24.0, 0.0),
            (4.0, -4.0),
            (16.0, -4.0),
            (28.0, -4.0),
        ];
        want.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
        for (g, w) in boxes.iter().zip(&want) {
            assert!(
                (g.0 - w.0).abs() < 1e-9 && (g.1 - w.1).abs() < 1e-9,
                "{g:?} {w:?}"
            );
        }
        assert_eq!(grid_cell(&s), Some(s.bounds()));
        let big = Effect::Grid {
            columns: 1000,
            rows: 1000,
            gap_x: 0.0,
            gap_y: 0.0,
            shift_x: 0.0,
            shift_y: 0.0,
        };
        let (c, r) = grid_size(&big);
        assert!(c * r <= MAX_GRID_CELLS && c == MAX_GRID_SIDE);
    }

    /// Transform: copies under the object, each the one before it scaled
    /// and turned about the object's centre, the outline keeping its
    /// width; with no copies the object itself is moved; random steps
    /// are the same from the same seed; the object stays as it is.
    #[test]
    fn transform_effect_copies_scale_and_turn() {
        let mut s = Shape::new(
            crate::ShapeId(1),
            ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                radius: 0.0,
                corners: None,
            },
        );
        let mut st = crate::Stroke::new(Color::BLACK, 1.0);
        st.scale_with_object = true;
        s.stroke = Some(st);
        s.effects = vec![transform_fx(3, 90.0, 50.0, false)];
        let ev = evaluate(&s);
        assert_eq!(ev.below.len(), 3);
        assert!(ev.above.is_empty());
        assert_eq!(ev.main.transform, Affine::IDENTITY);
        assert_eq!(ev.main.bounds(), s.bounds());
        // The first copy (nearest the object, last below) is half size
        // about the centre; the last a eighth.
        let first = &ev.below[2];
        let b = first.bounds();
        assert!((b.width() - 5.0).abs() < 1e-6, "{b:?}");
        assert!((b.center().x - 5.0).abs() < 1e-6 && (b.center().y - 5.0).abs() < 1e-6);
        assert!((ev.below[0].bounds().width() - 1.25).abs() < 1e-6);
        // The outline keeps its width on the page.
        let w = |sh: &Shape| {
            let c = sh.transform.as_coeffs();
            sh.stroke.as_ref().expect("outline").width * (c[0] * c[3] - c[1] * c[2]).abs().sqrt()
        };
        assert!((w(first) - 1.0).abs() < 1e-9);
        assert!(ev.below.iter().all(|c| c.effects.is_empty()));
        // With the outlines scaling too.
        let mut fx = transform_fx(1, 0.0, 50.0, false);
        if let Effect::Transform { scale_strokes, .. } = &mut fx {
            *scale_strokes = true;
        }
        s.effects = vec![fx];
        assert!((w(&evaluate(&s).below[0]) - 0.5).abs() < 1e-9);
        // No copies: the object itself is transformed.
        s.effects = vec![transform_fx(0, 0.0, 200.0, false)];
        let ev = evaluate(&s);
        assert!(ev.below.is_empty());
        assert!((ev.main.bounds().width() - 20.0).abs() < 1e-6);
        // Random: same seed, same copies; within the set values.
        s.effects = vec![transform_fx(5, 30.0, 80.0, true)];
        let a = evaluate(&s);
        let b = evaluate(&s);
        assert_eq!(a.below.len(), 5);
        for (x, y) in a.below.iter().zip(&b.below) {
            assert_eq!(x.transform, y.transform);
        }
        let maps = transform_maps(&s.effects[0], Point::new(5.0, 5.0));
        let [a0, b0, ..] = maps[0].as_coeffs();
        let k = a0.hypot(b0);
        assert!((0.8 - 1e-9..=1.0 + 1e-9).contains(&k), "{k}");
        // Saved and read back.
        let json = serde_json::to_string(&s.effects[0]).expect("save");
        assert!(json.contains("\"effect\":\"transform\""), "{json}");
        let back: Effect = serde_json::from_str(&json).expect("read");
        assert_eq!(back, s.effects[0]);
    }

    /// Blend If's weight: nothing outside the range, all inside it, a
    /// smooth rise and fall over split ends, and a range dragged over
    /// itself kept in order.
    #[test]
    fn blend_if_weight_rises_and_falls_over_split_ends() {
        let all = BLEND_IF_ALL;
        for l in [0.0, 0.3, 1.0] {
            assert_eq!(blend_if_weight(&all, l), 1.0);
        }
        // Dark pixels hidden below 0.25, fading in up to 0.5.
        let r = [0.25, 0.5, 1.0, 1.0];
        assert_eq!(blend_if_weight(&r, 0.1), 0.0);
        assert!((blend_if_weight(&r, 0.375) - 0.5).abs() < 1e-9);
        assert_eq!(blend_if_weight(&r, 0.7), 1.0);
        // Joined ends cut sharply.
        let r = [0.4, 0.4, 0.6, 0.6];
        assert_eq!(blend_if_weight(&r, 0.39), 0.0);
        assert_eq!(blend_if_weight(&r, 0.5), 1.0);
        assert_eq!(blend_if_weight(&r, 0.61), 0.0);
        // Out of order and out of range values.
        let r = [0.8, 0.2, f64::NAN, 3.0];
        assert_eq!(blend_if_weight(&r, 0.5), 0.0);
        assert_eq!(blend_if_weight(&r, 0.8), 1.0);
        assert!((blend_if_lightness(1.0, 1.0, 1.0) - 1.0).abs() < 1e-9);
        assert_eq!(blend_if_lightness(0.0, 0.0, 0.0), 0.0);
        let e = Effect::BlendIf {
            this: all,
            below: [0.3, 0.6, 1.0, 1.0],
        };
        assert!(e.drawn_by_renderer());
        let json = serde_json::to_string(&e).expect("json");
        let back: Effect = serde_json::from_str(&json).expect("back");
        assert_eq!(back, e);
    }

    /// Colour and gradient overlays draw the object's shape over it at
    /// their opacity; fill opacity fades the object alone; the shadows,
    /// glows and blur add no objects (the renderer draws them) (#1141).
    #[test]
    fn layer_effect_overlays_and_fill_opacity() {
        let mut s = Shape::new(
            crate::ShapeId(1),
            ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                radius: 0.0,
                corners: None,
            },
        );
        s.fill = Fill::Solid(Color::BLACK);
        s.stroke = Some(crate::Stroke::new(Color::BLACK, 1.0));
        s.effects = vec![
            Effect::ColorOverlay {
                color: Color::rgb8(255, 0, 0),
                opacity: 0.5,
            },
            Effect::GradientOverlay {
                from: Color::WHITE,
                to: Color::BLACK,
                angle: 0.0,
                opacity: 1.0,
            },
            Effect::FillOpacity { opacity: 0.25 },
            Effect::Glow {
                inner: false,
                color: Color::WHITE,
                opacity: 1.0,
                blur: 2.0,
                merge: MergeMode::Screen,
            },
            Effect::GaussianBlur { radius: 1.0 },
        ];
        let ev = evaluate(&s);
        assert!(ev.below.is_empty());
        assert_eq!(ev.above.len(), 2);
        assert_eq!(ev.above[0].fill, Fill::Solid(Color::rgb8(255, 0, 0)));
        assert!((ev.above[0].opacity - 0.5).abs() < 1e-9);
        assert_eq!(
            ev.above[0].stroke.as_ref().map(|st| st.color),
            Some(Color::rgb8(255, 0, 0))
        );
        assert!(matches!(ev.above[1].fill, Fill::Fountain(_)));
        assert!((ev.above[1].opacity - 1.0).abs() < 1e-9);
        assert!((ev.main.opacity - 0.25).abs() < 1e-9);
        // Nothing at no opacity.
        s.effects = vec![Effect::ColorOverlay {
            color: Color::WHITE,
            opacity: 0.0,
        }];
        assert!(evaluate(&s).above.is_empty());
        // Saved and read back.
        let e = Effect::OuterShadow {
            offset: Vec2::new(1.0, -1.0),
            color: Color::BLACK,
            opacity: 0.5,
            blur: 1.0,
            merge: MergeMode::Multiply,
        };
        let back: Effect = serde_json::from_str(&serde_json::to_string(&e).unwrap()).unwrap();
        assert_eq!(back, e);
    }

    /// The parts a transparency's target fades (live test of 0.41: the
    /// target made no difference): the fill and the outline apart, the
    /// outline first when it is behind the fill, an object lacking the
    /// part named drawn as it is, a group faded whole.
    #[test]
    fn transparency_parts_follow_the_target() {
        let mut s = Shape::new(
            crate::ShapeId(1),
            ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                radius: 2.0,
                corners: None,
            },
        );
        s.fill = Fill::Solid(Color::rgb8(255, 0, 0));
        s.stroke = Some(crate::Stroke::new(Color::BLACK, 1.0));
        let look = |parts: &[(Shape, bool)]| {
            parts
                .iter()
                .map(|(p, faded)| (!matches!(p.fill, Fill::None), p.stroke.is_some(), *faded))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            look(&transparency_parts(&s, 0)),
            [(true, false, true), (false, true, false)]
        );
        assert_eq!(
            look(&transparency_parts(&s, 1)),
            [(true, false, false), (false, true, true)]
        );
        assert_eq!(look(&transparency_parts(&s, 2)), [(true, true, true)]);
        if let Some(st) = s.stroke.as_mut() {
            st.behind_fill = true;
        }
        assert_eq!(
            look(&transparency_parts(&s, 1)),
            [(false, true, true), (true, false, false)]
        );
        // No outline: the outline target fades nothing, the fill target all.
        s.stroke = None;
        assert_eq!(look(&transparency_parts(&s, 1)), [(true, false, false)]);
        assert_eq!(look(&transparency_parts(&s, 0)), [(true, false, true)]);
        let group = Shape::new(crate::ShapeId(2), ShapeKind::Group { children: vec![s] });
        let parts = transparency_parts(&group, 0);
        assert!(parts.len() == 1 && parts[0].1);
    }

    /// A symmetry goes with its object: the centre and the first line are
    /// carried by a move, a turn and a mirror, so the copies the object
    /// makes are those of the transformed whole.
    #[test]
    fn a_symmetry_is_carried_with_its_object() {
        let sym = |center: Point, angle: f64| Effect::Symmetry {
            center,
            angle,
            lines: 2,
            rotate: false,
        };
        let angle_of = |e: &Effect| match e {
            Effect::Symmetry { center, angle, .. } => (*center, *angle),
            _ => unreachable!(),
        };
        let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
        // A move: the centre moves, the line keeps its angle.
        let mut fx = vec![sym(Point::new(10.0, 5.0), 30.0)];
        carry_effects(&mut fx, Affine::translate((4.0, -2.0)));
        let (c, a) = angle_of(&fx[0]);
        assert!(
            near(c.x, 14.0) && near(c.y, 3.0) && near(a, 30.0),
            "{c:?} {a}"
        );
        // A quarter turn about the origin: the line turns with it.
        carry_effects(&mut fx, Affine::rotate(std::f64::consts::FRAC_PI_2));
        let (c, a) = angle_of(&fx[0]);
        assert!(
            near(c.x, -3.0) && near(c.y, 14.0) && near(a, 120.0),
            "{c:?} {a}"
        );
        // A mirror across the upright axis: 120 degrees becomes 60.
        carry_effects(&mut fx, Affine::scale_non_uniform(-1.0, 1.0));
        let (c, a) = angle_of(&fx[0]);
        assert!(
            near(c.x, 3.0) && near(c.y, 14.0) && near(a, 60.0),
            "{c:?} {a}"
        );
        // The copies of the carried symmetry are the transformed copies.
        let mut s = Shape::new(
            crate::ShapeId(1),
            ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 4.0, 2.0),
                radius: 0.0,
                corners: None,
            },
        );
        s.effects = vec![Effect::Symmetry {
            center: Point::new(4.0, 1.0),
            angle: 90.0,
            lines: 1,
            rotate: false,
        }];
        let t = Affine::translate((10.0, 3.0)) * Affine::rotate(0.7);
        let before: Vec<Rect> = evaluate(&s)
            .above
            .iter()
            .map(|c| (t * c.page_path()).bounding_box())
            .collect();
        let mut moved = s.clone();
        moved.transform_by(t);
        let after: Vec<Rect> = evaluate(&moved)
            .above
            .iter()
            .map(|c| c.page_path().bounding_box())
            .collect();
        assert_eq!(before.len(), after.len());
        for (b, a) in before.iter().zip(&after) {
            assert!(
                near(b.x0, a.x0) && near(b.y0, a.y0) && near(b.x1, a.x1) && near(b.y1, a.y1),
                "{b:?} {a:?}"
            );
        }
        // Switched off, a symmetry still goes with the object, and an
        // object taken out of a group takes the group's transform with
        // its symmetry.
        let mut off = s.clone();
        off.parked_effects = vec![crate::document::ParkedEffect {
            index: 0,
            effect: off.effects.remove(0),
        }];
        off.transform_by(Affine::translate((7.0, 0.0)));
        let Effect::Symmetry { center, .. } = off.parked_effects[0].effect else {
            unreachable!()
        };
        assert!(near(center.x, 11.0), "{center:?}");
        let mut out = s.clone();
        out.absorb(Affine::translate((0.0, 5.0)));
        let Effect::Symmetry { center, .. } = out.effects[0] else {
            unreachable!()
        };
        assert!(near(center.y, 6.0), "{center:?}");
    }
    use crate::geometry::Rect;

    /// Offset Path: on one object it grows or shrinks the outline live;
    /// on a group with merge the objects become one outline in the bottom
    /// object's paint, and the smoothing joins two circles near each
    /// other with a rounded neck (metaballs) while circles far apart stay
    /// two; Convert to Curves bakes it.
    #[test]
    fn offset_path_grows_and_merges_a_group_into_metaballs() {
        use crate::geometry::Shape as _;
        let mut s = rect_shape();
        s.effects.push(Effect::OffsetPath {
            distance: 1.0,
            smooth: 0.0,
            join: crate::shaping::Join::Miter,
            merge: false,
        });
        let b = evaluate(&s).main.local_path().bounding_box();
        assert!((b.width() - 22.0).abs() < 0.05, "{b:?}");
        let circle = |id: u64, cx: f64, color: Color| {
            let mut c = Shape::new(
                crate::ShapeId(id),
                ShapeKind::Ellipse {
                    rect: Rect::new(cx - 10.0, -10.0, cx + 10.0, 10.0),
                    arc: None,
                },
            );
            c.fill = Fill::Solid(color);
            c
        };
        let group = |gap: f64| {
            let mut g = Shape::new(
                crate::ShapeId(9),
                ShapeKind::Group {
                    children: vec![
                        circle(2, 0.0, Color::rgb8(0, 160, 0)),
                        circle(3, 20.0 + gap, Color::rgb8(0, 0, 200)),
                    ],
                },
            );
            g.effects.push(Effect::OffsetPath {
                distance: 0.0,
                smooth: 6.0,
                join: crate::shaping::Join::Round,
                merge: true,
            });
            g
        };
        let contours = |p: &BezPath| {
            p.elements()
                .iter()
                .filter(|e| matches!(e, PathEl::MoveTo(_)))
                .count()
        };
        // 4 mm apart: one shape, the neck filled in.
        let near = evaluate(&group(4.0)).main;
        assert!(matches!(near.kind, ShapeKind::Path { .. }));
        assert_eq!(near.fill, Fill::Solid(Color::rgb8(0, 160, 0)));
        let path = near.local_path();
        assert_eq!(contours(&path), 1);
        let two_circles = 2.0 * std::f64::consts::PI * 100.0;
        assert!(path.area().abs() > two_circles + 20.0, "{}", path.area());
        // The middle of the gap is inside, on the line between centres.
        assert!(path.winding(Point::new(22.0, 0.0)) != 0);
        // 30 mm apart: still two.
        assert_eq!(contours(&evaluate(&group(30.0)).main.local_path()), 2);
        // Saved and read back; baked by Convert to Curves.
        let g = group(4.0);
        let json = serde_json::to_string(&g.effects).expect("json");
        assert!(json.contains("\"offsetpath\""), "{json}");
        let back: Vec<Effect> = serde_json::from_str(&json).expect("read");
        assert_eq!(back, g.effects);
        let min: Effect =
            serde_json::from_str(r#"{"effect":"offsetpath","distance":2}"#).expect("min");
        assert!(matches!(min, Effect::OffsetPath { smooth, merge: false, .. } if smooth == 0.0));
        let baked = bake_outline_warps(&g).expect("bake");
        assert!(baked.effects.is_empty());
    }

    fn rect_shape() -> Shape {
        let mut s = Shape::new(
            crate::ShapeId(1),
            ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 20.0, 20.0),
                radius: 0.0,
                corners: None,
            },
        );
        s.fill = Fill::Solid(Color::rgb8(255, 0, 0));
        s
    }

    fn bevelled(style: u8) -> Evaluated {
        let mut s = rect_shape();
        s.effects.push(Effect::Bevel {
            distance: 2.0,
            light_angle: 135.0,
            intensity: 50.0,
            style,
            shadow_color: Color::BLACK,
            light_color: Color::WHITE,
            level: 1,
            emboss_color: None,
            directional: false,
        });
        evaluate(&s)
    }

    /// #1144: the centre style puts the band half inside and half
    /// outside the outline; directional light tones it round the object,
    /// brightest where it faces the light and darkest opposite.
    #[test]
    fn centre_bevel_straddles_the_outline_and_directional_light_fades_round_it() {
        let ev = bevelled(2);
        let all = ev
            .above
            .iter()
            .map(|p| p.bounds())
            .reduce(|a, b| a.union(b))
            .expect("a band");
        assert!(
            (all.x0 + 1.0).abs() < 1e-3 && (all.x1 - 21.0).abs() < 1e-3,
            "{all:?}"
        );
        let mut s = rect_shape();
        s.effects.push(Effect::Bevel {
            distance: 2.0,
            light_angle: 0.0,
            intensity: 100.0,
            style: 0,
            shadow_color: Color::BLACK,
            light_color: Color::WHITE,
            level: 1,
            emboss_color: None,
            directional: true,
        });
        let ev = evaluate(&s);
        assert!(ev.above.len() > 8, "toned in wedges: {}", ev.above.len());
        let lum = |p: &Shape| match p.fill {
            Fill::Solid(c) => {
                let [r, g, b] = c.to_rgb8();
                r as u32 + g as u32 + b as u32
            }
            _ => 0,
        };
        // The light comes from the right (+x): the right edge is the
        // brightest part, the left edge the darkest, the top in between.
        let at = |x: f64, y: f64| {
            ev.above
                .iter()
                .find(|p| {
                    p.bounds().contains(Point::new(x, y)) && {
                        let path = p.local_path();
                        path.winding(Point::new(x, y)) != 0
                    }
                })
                .map(lum)
                .unwrap_or_else(|| {
                    panic!(
                        "no wedge at {x},{y}: {:?}",
                        ev.above.iter().map(|p| p.bounds()).collect::<Vec<_>>()
                    )
                })
        };
        let (right, top, left) = (at(19.5, 11.0), at(11.0, 19.5), at(0.5, 11.0));
        assert!(right > top && top > left, "{right} {top} {left}");
    }

    #[test]
    fn soft_edge_bevel_stays_inside_the_outline() {
        let ev = bevelled(0);
        assert!(!ev.above.is_empty());
        for part in &ev.above {
            let b = part.bounds();
            assert!(
                b.x0 >= -1e-6 && b.x1 <= 20.0 + 1e-6 && b.y0 >= -1e-6 && b.y1 <= 20.0 + 1e-6,
                "{b:?}"
            );
        }
    }

    #[test]
    fn emboss_bevel_surrounds_the_outline() {
        let soft = bevelled(0);
        let emboss = bevelled(1);
        assert!(!emboss.above.is_empty());
        let mut reach = Rect::new(0.0, 0.0, 20.0, 20.0);
        for part in &emboss.above {
            reach = reach.union(part.bounds());
        }
        // The band stands out of the object by the distance.
        assert!(
            (reach.x0 + 2.0).abs() < 0.1 && (reach.x1 - 22.0).abs() < 0.1,
            "{reach:?}"
        );
        // And it differs from the soft edge.
        assert_ne!(
            emboss.above.iter().map(|p| p.bounds()).collect::<Vec<_>>(),
            soft.above.iter().map(|p| p.bounds()).collect::<Vec<_>>()
        );
    }

    fn bevel_with(level: u8, emboss_color: Option<Color>) -> Evaluated {
        let mut s = rect_shape();
        s.effects.push(Effect::Bevel {
            distance: 3.0,
            light_angle: 135.0,
            intensity: 80.0,
            style: 0,
            shadow_color: Color::BLACK,
            light_color: Color::WHITE,
            level,
            emboss_color,
            directional: false,
        });
        evaluate(&s)
    }

    fn colours(ev: &Evaluated) -> Vec<Color> {
        ev.above
            .iter()
            .filter_map(|p| p.fill.preview_color())
            .collect()
    }

    #[test]
    fn bevel_levels_fade_the_light_across_the_band() {
        let flat = colours(&bevel_with(1, None));
        let stepped = colours(&bevel_with(4, None));
        assert_eq!(flat.len(), 2, "one lit and one shaded tone");
        assert!(stepped.len() > flat.len());
        let mut distinct = stepped.clone();
        distinct.dedup();
        assert!(distinct.len() > 2, "{stepped:?}");
        // The steps stay inside the first level's band.
        for part in &bevel_with(4, None).above {
            let b = part.bounds();
            assert!(b.x0 >= -1e-6 && b.x1 <= 20.0 + 1e-6, "{b:?}");
        }
    }

    #[test]
    fn bevel_level_is_clamped() {
        let many = bevel_with(200, None);
        assert!(many.above.len() <= 2 * usize::from(MAX_BEVEL_LEVEL));
        assert_eq!(
            bevel_with(0, None).above.len(),
            bevel_with(1, None).above.len()
        );
    }

    #[test]
    fn the_emboss_colour_replaces_the_fill_as_the_base_of_the_band() {
        let own = colours(&bevel_with(1, None));
        let blue = Color::rgb8(0, 0, 255);
        let set = colours(&bevel_with(1, Some(blue)));
        assert_eq!(own.len(), set.len());
        assert_ne!(own, set);
    }

    #[test]
    fn older_bevels_load_as_one_level_on_the_fill() {
        let json = serde_json::to_string(&Effect::Bevel {
            distance: 2.0,
            light_angle: 135.0,
            intensity: 50.0,
            style: 0,
            shadow_color: Color::BLACK,
            light_color: Color::WHITE,
            level: 1,
            emboss_color: None,
            directional: false,
        })
        .unwrap_or_default();
        // The new fields are left out while they hold their defaults ...
        assert!(!json.contains("emboss_color"), "{json}");
        // ... and a bevel saved without them reads back with them.
        match serde_json::from_str::<Effect>(&json.replace(",\"level\":1", "")) {
            Ok(Effect::Bevel {
                level,
                emboss_color,
                ..
            }) => assert_eq!((level, emboss_color), (1, None)),
            other => panic!("{other:?}"),
        }
    }

    /// #2218: a block shadow is cast by the whole object. A group casts
    /// it from what its objects cover together (read as one outline, two
    /// of them were a hole where they overlap), and an object with an
    /// extrusion from itself and its body, beneath the body (it was the
    /// object's alone, drawn over the body's faces).
    #[test]
    fn block_shadow_is_cast_by_the_whole_object() {
        let shadow = Effect::BlockShadow {
            offset: Vec2::new(4.0, -4.0),
            color: Color::BLACK,
            gap: 0.0,
        };
        let square = |id: u64, x: f64| {
            let mut s = Shape::new(
                crate::ShapeId(id),
                ShapeKind::Rect {
                    rect: Rect::new(x, 0.0, x + 10.0, 10.0),
                    radius: 0.0,
                    corners: None,
                },
            );
            s.fill = Fill::Solid(Color::WHITE);
            s
        };
        // Two squares 6 mm apart, overlapping by 4 mm.
        let mut group = Shape::new(
            crate::ShapeId(1),
            ShapeKind::Group {
                children: vec![square(2, 0.0), square(3, 6.0)],
            },
        );
        group.transform = Affine::translate((100.0, 50.0));
        group.effects.push(shadow.clone());
        let ev = evaluate(&group);
        assert_eq!(ev.below.len(), 1);
        let cast = ev.below[0].page_path();
        // The 16 x 10 box they cover, slid by (4, -4).
        let expected = 160.0 + 16.0 * 4.0 + 10.0 * 4.0;
        assert!(
            (cast.area().abs() - expected).abs() < 1e-3,
            "{}",
            cast.area()
        );
        assert!(cast.contains(Point::new(108.0, 55.0)), "where they overlap");
        // A square with a body 10 mm up and to the right.
        let mut s = rect_shape();
        s.effects = vec![
            Effect::Extrude {
                depth: Vec2::new(10.0, 10.0),
                vanishing: None,
                amount: 0.0,
                shade_from: None,
                shade_to: None,
                light_angle: 135.0,
                light_intensity: 50.0,
                bevel: 0.0,
                kind: None,
                on_object: false,
                solid: None,
                space: Default::default(),
            },
            shadow,
        ];
        let ev = evaluate(&s);
        // Beneath the far face and the sides.
        assert_eq!(ev.below.len(), 4);
        assert_eq!(ev.below[0].fill, Fill::Solid(Color::BLACK));
        let cast = ev.below[0].page_path();
        // The body's far corner, slid: past the square's own shadow.
        assert!(cast.contains(Point::new(33.0, 25.0)), "the body's shadow");
        assert!(cast.contains(Point::new(23.0, -3.0)), "the object's shadow");
        assert!(!cast.contains(Point::new(35.0, 2.0)));
        // The square (400), its body (a hexagon of 800 with it) and
        // their slide: 800 + 4 * (30 + 30).
        assert!((cast.area().abs() - 1040.0).abs() < 1e-3, "{}", cast.area());
    }

    #[test]
    fn block_shadow_keeps_corners_sharp() {
        let mut s = rect_shape();
        s.effects.push(Effect::BlockShadow {
            offset: Vec2::new(5.0, -5.0),
            color: Color::BLACK,
            gap: 0.0,
        });
        let ev = evaluate(&s);
        assert_eq!(ev.below.len(), 1);
        let b = ev.below[0].bounds();
        assert!(
            (b.x0 - 0.0).abs() < 1e-6 && (b.x1 - 25.0).abs() < 1e-6,
            "{b:?}"
        );
        assert!(
            (b.y0 + 5.0).abs() < 1e-6 && (b.y1 - 20.0).abs() < 1e-6,
            "{b:?}"
        );
        // The shadow's far corner is a true corner: a point 0.2 mm inside
        // it is covered (a rounded corner would miss it).
        let path = ev.below[0].page_path();
        let corner = Point::new(25.0 - 0.2, -5.0 + 0.2);
        let inside = crate::shaping::overlay(
            &path,
            &crate::effects::polygon(&[
                corner,
                corner + Vec2::new(0.1, 0.0),
                corner + Vec2::new(0.1, 0.1),
                corner + Vec2::new(0.0, 0.1),
            ]),
            crate::shaping::Op::Intersect,
        );
        assert!(!inside.elements().is_empty());
    }

    /// An outline of several parts, as the letters of a word: the block
    /// shadow of each part follows that part, and nothing joins the end of
    /// one to the start of the next (the shards of #2156).
    #[test]
    fn block_shadow_follows_each_part_of_the_outline() {
        let square = |x: f64| Rect::new(x, 0.0, x + 10.0, 10.0).to_path(0.01);
        let mut path = square(0.0);
        path.extend(square(30.0).elements().iter().copied());
        // A hole in the second part, wider than the shadow's depth.
        path.extend(Rect::new(32.0, 2.0, 38.0, 8.0).to_path(0.01));
        let mut s = Shape::new(crate::ShapeId(1), ShapeKind::Path { path, closed: true });
        let offset = Vec2::new(4.0, -0.5);
        s.effects.push(Effect::BlockShadow {
            offset,
            color: Color::BLACK,
            gap: 0.0,
        });
        let ev = evaluate(&s);
        assert_eq!(ev.below.len(), 1);
        let shadow = ev.below[0].page_path();
        let solid = 100.0 + 10.0 * 4.0 + 10.0 * 0.5;
        // The hole, 6 x 6, slid by (4, -0.5): 2 x 5.5 of it stays open.
        let expected = 2.0 * solid - 11.0;
        assert!(
            (shadow.area().abs() - expected).abs() < 1e-3,
            "{}",
            shadow.area()
        );
        let parts = shadow
            .elements()
            .iter()
            .filter(|e| matches!(e, PathEl::MoveTo(_)))
            .count();
        assert_eq!(parts, 3);
        // Nothing between the two parts.
        let between = crate::shaping::overlay(
            &shadow,
            &Rect::new(14.5, -1.0, 29.5, 11.0).to_path(0.01),
            crate::shaping::Op::Intersect,
        );
        assert!(between.elements().is_empty(), "{between:?}");
        // The gap still trims the object grown by it away.
        if let Some(Effect::BlockShadow { gap, .. }) = s.effects.first_mut() {
            *gap = 1.0;
        }
        let trimmed = evaluate(&s).below[0].page_path();
        assert!(trimmed.area().abs() < shadow.area().abs() - 1.0);
    }

    /// A warped text's outline keeps the nonzero rule the text is drawn
    /// with: saved to `.cdr` it went out as an even-odd curve, and the
    /// program that opened it showed holes where its letters overlapped
    /// (#2733).
    #[test]
    fn a_warped_texts_outline_fills_with_the_nonzero_rule() {
        use crate::shaping::FillRule;
        let mut text = Shape::new(
            crate::ShapeId(1),
            ShapeKind::Text {
                spans: Vec::new(),
                origin: Point::ZERO,
                frame: None,
                align: Default::default(),
                para: Default::default(),
                on_path: None,
            },
        );
        text.effects.push(Effect::Perspective {
            corners: [
                Point::new(0.0, 0.0),
                Point::new(10.0, 0.0),
                Point::new(8.0, 5.0),
                Point::new(2.0, 5.0),
            ],
        });
        let main = evaluate(&text).main;
        assert!(
            matches!(main.kind, ShapeKind::Path { .. }),
            "{:?}",
            main.kind
        );
        assert_eq!(main.fill_rule, FillRule::NonZero);
        // A curve keeps its own rule.
        let mut curve = rect_shape();
        curve.effects = text.effects.clone();
        assert_eq!(evaluate(&curve).main.fill_rule, FillRule::EvenOdd);
    }

    /// Text and objects set to the nonzero rule are read with it, so the
    /// overlapping strokes of a letter make no holes in its contour or
    /// block shadow (#2156).
    #[test]
    fn effects_read_text_with_the_nonzero_rule() {
        use crate::shaping::FillRule;
        let text = Shape::new(
            crate::ShapeId(1),
            ShapeKind::Text {
                spans: Vec::new(),
                origin: Point::ZERO,
                frame: None,
                align: Default::default(),
                para: Default::default(),
                on_path: None,
            },
        );
        assert_eq!(area_rule(&text), FillRule::NonZero);
        let mut curve = rect_shape();
        assert_eq!(area_rule(&curve), FillRule::EvenOdd);
        curve.fill_rule = FillRule::NonZero;
        assert_eq!(area_rule(&curve), FillRule::NonZero);
        // Two crossing bars turning the same way.
        let mut path = Rect::new(0.0, 5.0, 20.0, 15.0).to_path(0.01);
        path.extend(Rect::new(5.0, 0.0, 15.0, 20.0).to_path(0.01));
        let mut s = Shape::new(crate::ShapeId(2), ShapeKind::Path { path, closed: true });
        s.fill = Fill::Solid(Color::WHITE);
        s.fill_rule = FillRule::NonZero;
        s.effects.push(Effect::Contour {
            steps: 1,
            offset: 1.0,
            outside: false,
            to_center: false,
            both: false,
            fill_to: Color::BLACK,
            outline_to: None,
            fill_end_to: None,
            color_blend: 0,
            corners: Some(crate::shaping::Join::Miter),
            accel_objects: 0.0,
            accel_colors: 0.0,
        });
        let inside = evaluate(&s).above;
        assert_eq!(inside.len(), 1);
        let parts = |p: &BezPath| {
            p.elements()
                .iter()
                .filter(|e| matches!(e, PathEl::MoveTo(_)))
                .count()
        };
        assert_eq!(parts(&inside[0].page_path()), 1);
        s.effects = vec![Effect::BlockShadow {
            offset: Vec2::new(1.0, 0.0),
            color: Color::BLACK,
            gap: 0.0,
        }];
        let shadow = evaluate(&s).below[0].page_path();
        assert_eq!(parts(&shadow), 1);
        assert!(
            (shadow.area().abs() - 320.0).abs() < 1e-3,
            "{}",
            shadow.area()
        );
    }

    /// The contour offset is the distance on the page, whatever the
    /// object's scale (#754): an 80 mm inside step of a polygon scaled
    /// down lay 47 mm from its edge.
    #[test]
    fn contour_offset_is_measured_on_the_page() {
        let mut s = Shape::new(
            crate::ShapeId(1),
            ShapeKind::Polygon {
                rect: Rect::new(0.0, 0.0, 100.0, 100.0),
                points: 8,
                sharpness: 0.0,
                complex: None,
                twist: 0.0,
                bend: 0.0,
            },
        );
        s.fill = Fill::Solid(Color::rgb8(255, 0, 0));
        s.transform = Affine::translate((30.0, 40.0))
            * Affine::scale_non_uniform(1.7, 1.7)
            * Affine::rotate(22.5f64.to_radians());
        s.effects.push(Effect::Contour {
            steps: 1,
            offset: 20.0,
            outside: false,
            to_center: false,
            both: false,
            fill_to: Color::WHITE,
            outline_to: None,
            fill_end_to: None,
            color_blend: 0,
            corners: Some(crate::shaping::Join::Miter),
            accel_objects: 0.0,
            accel_colors: 0.0,
        });
        let page = |s: &Shape| (s.transform * s.local_path()).bounding_box();
        let outer = page(&s);
        let ev = evaluate(&s);
        let inner = ev
            .above
            .iter()
            .chain(&ev.below)
            .map(page)
            .next()
            .expect("one step");
        for gap in [
            inner.x0 - outer.x0,
            outer.x1 - inner.x1,
            inner.y0 - outer.y0,
            outer.y1 - inner.y1,
        ] {
            assert!((gap - 20.0).abs() < 0.2, "{gap} {outer:?} {inner:?}");
        }
        // Stretched: the same distance across and down.
        let effects = s.effects.clone();
        s = rect_shape();
        s.effects = effects;
        s.transform = Affine::scale_non_uniform(3.0, 1.0);
        if let Some(Effect::Contour { offset, .. }) = s.effects.last_mut() {
            *offset = 4.0;
        }
        let outer = page(&s);
        let ev = evaluate(&s);
        let inner = ev
            .above
            .iter()
            .chain(&ev.below)
            .map(page)
            .next()
            .expect("step");
        assert!(
            (inner.x0 - outer.x0 - 4.0).abs() < 0.05,
            "{outer:?} {inner:?}"
        );
        assert!(
            (inner.y0 - outer.y0 - 4.0).abs() < 0.05,
            "{outer:?} {inner:?}"
        );
    }

    /// An evaluation kept for the same shape is the one made then, and a
    /// change of the shape or of its effect is never answered with it
    /// (#897).
    #[test]
    fn a_kept_evaluation_follows_the_shape() {
        let contour = |offset: f64| Effect::Contour {
            steps: 2,
            offset,
            outside: true,
            to_center: false,
            both: false,
            fill_to: Color::WHITE,
            outline_to: None,
            fill_end_to: None,
            color_blend: 0,
            corners: Some(crate::shaping::Join::Miter),
            accel_objects: 0.0,
            accel_colors: 0.0,
        };
        let mut s = rect_shape();
        s.effects.push(contour(2.0));
        let first = evaluate(&s);
        let again = evaluate(&s);
        assert_eq!(first.below.len(), again.below.len());
        assert_eq!(first.below[0].bounds(), again.below[0].bounds());
        // Another offset, the same object.
        s.effects = vec![contour(3.0)];
        let wider = evaluate(&s).below[0].bounds().width();
        assert!(wider > first.below[0].bounds().width() + 1.0, "{wider}");
        // Another fill, the same effect.
        let mut red = s.clone();
        red.fill = Fill::Solid(Color::rgb8(200, 0, 0));
        assert_ne!(evaluate(&red).main.fill, evaluate(&s).main.fill);
        // Moved: the steps move with it.
        let mut moved = s.clone();
        moved.transform = Affine::translate((10.0, 0.0)) * moved.transform;
        let dx = evaluate(&moved).below[0].bounds().x0 - evaluate(&s).below[0].bounds().x0;
        assert!((dx - 10.0).abs() < 1e-6, "{dx}");
    }

    /// #2577: the contour of an open curve is a closed band round both
    /// sides of it, joined across its ends, not the outline of the area
    /// between the curve and the straight line from end to end.
    #[test]
    fn an_open_curve_contours_into_a_closed_band() {
        use crate::geometry::Shape as _;
        let mut path = BezPath::new();
        path.move_to((0.0, 0.0));
        path.curve_to((10.0, 20.0), (30.0, 20.0), (40.0, 0.0));
        let mut s = Shape::new(
            crate::ShapeId(1),
            ShapeKind::Path {
                path,
                closed: false,
            },
        );
        s.fill = Fill::None;
        s.effects.push(Effect::Contour {
            steps: 2,
            offset: 2.0,
            outside: true,
            to_center: false,
            both: false,
            fill_to: Color::WHITE,
            outline_to: None,
            fill_end_to: None,
            color_blend: 0,
            corners: Some(crate::shaping::Join::Round),
            accel_objects: 0.0,
            accel_colors: 0.0,
        });
        let ev = evaluate(&s);
        assert_eq!(ev.below.len(), 2);
        // The outer step: 4 mm to each side of the curve, closed.
        let outer = ev.below[0].local_path();
        assert!(matches!(
            ev.below[0].kind,
            ShapeKind::Path { closed: true, .. }
        ));
        let top = Point::new(20.0, 15.0);
        let winds = |q: Point| outer.winding(q) != 0;
        assert!(winds(top), "on the curve");
        assert!(winds(top + Vec2::new(0.0, 3.5)), "above the curve");
        assert!(winds(top - Vec2::new(0.0, 3.5)), "below the curve");
        assert!(!winds(top + Vec2::new(0.0, 4.5)));
        assert!(!winds(top - Vec2::new(0.0, 4.5)));
        // Not round the chord: its middle stays outside.
        assert!(!winds(Point::new(20.0, 0.0)));
        // Cut straight at the ends: nothing past the start along the
        // curve's way out of it.
        assert!(!winds(Point::new(-1.5, -3.0)));
    }

    /// A fountain-filled object's contour steps keep the fountain (#1265):
    /// its first colour goes to the fill colour and its last to the last
    /// fill colour, the outermost step reaching both.
    #[test]
    fn contour_steps_keep_a_fountain_toward_both_end_colours() {
        let mut s = rect_shape();
        s.fill = Fill::linear(Color::rgb8(255, 0, 0), Color::rgb8(0, 0, 255), 0.0);
        let effect = |end: Option<Color>| Effect::Contour {
            steps: 2,
            offset: 2.0,
            outside: true,
            to_center: false,
            both: false,
            fill_to: Color::WHITE,
            outline_to: None,
            fill_end_to: end,
            color_blend: 0,
            corners: None,
            accel_objects: 0.0,
            accel_colors: 0.0,
        };
        s.effects.push(effect(Some(Color::BLACK)));
        let ev = evaluate(&s);
        assert_eq!(ev.below.len(), 2);
        let stops = |f: &Fill| match f {
            Fill::Fountain(g) => g
                .stops
                .iter()
                .map(|st| st.color.to_rgb8())
                .collect::<Vec<_>>(),
            other => panic!("a fountain, not {other:?}"),
        };
        // Outermost first: the last step, all the way to white and black.
        let outer = stops(&ev.below[0].fill);
        assert_eq!(outer.first(), Some(&Color::WHITE.to_rgb8()));
        assert_eq!(outer.last(), Some(&Color::BLACK.to_rgb8()));
        // The step nearer the object is half way from red and blue.
        let near = stops(&ev.below[1].fill);
        let close = |a: [u8; 3], b: [u8; 3]| (0..3).all(|k| a[k].abs_diff(b[k]) <= 4);
        assert!(close(near[0], [255, 128, 128]), "{near:?}");
        assert!(close(near[near.len() - 1], [0, 0, 128]), "{near:?}");
        // Without a last fill colour (older files) both ends go to the
        // fill colour.
        s.effects[0] = effect(None);
        let outer = stops(&evaluate(&s).below[0].fill);
        assert!(outer.iter().all(|c| *c == Color::WHITE.to_rgb8()), "{outer:?}");
    }

    /// #3085: a contour on both sides draws its steps inside and outside
    /// at once, the outside ones behind the object and the inside ones in
    /// front, as the one-sided contours draw them; it is saved and read
    /// back, and a contour of an older file (no `both`) stays one-sided.
    #[test]
    fn a_group_takes_a_contour_round_all_its_objects() {
        // #3245: two overlapping squares in a group, outlined round both
        // as one area, in the colour of the first; before, a group took
        // no contour.
        let mut a = rect_shape();
        a.id = crate::ShapeId(2);
        let mut b = rect_shape();
        b.id = crate::ShapeId(3);
        b.fill = Fill::Solid(Color::rgb8(0, 0, 255));
        b.transform = Affine::translate((10.0, 10.0));
        let mut g = Shape::new(
            crate::ShapeId(4),
            ShapeKind::Group {
                children: vec![a, b],
            },
        );
        g.effects.push(Effect::Contour {
            steps: 2,
            offset: 1.0,
            outside: true,
            to_center: false,
            both: false,
            fill_to: Color::WHITE,
            outline_to: None,
            fill_end_to: None,
            color_blend: 0,
            corners: Some(crate::shaping::Join::Miter),
            accel_objects: 0.0,
            accel_colors: 0.0,
        });
        let ev = evaluate(&g);
        assert_eq!(ev.below.len(), 2, "two steps");
        // The outer step goes round both squares: from -2 to 32.
        let outer = ev.below[0].page_path().bounding_box();
        assert!((outer.x0 + 2.0).abs() < 1e-6 && (outer.x1 - 32.0).abs() < 1e-6, "{outer:?}");
        assert!((outer.y0 + 2.0).abs() < 1e-6 && (outer.y1 - 32.0).abs() < 1e-6, "{outer:?}");
        // One ring round the union: the corner where the squares meet
        // inside (15, 15) is covered, the empty corner (25, 5) of the box
        // only by the band near the edge, not its middle.
        let ring = ev.below[0].page_path();
        assert!(ring.contains(Point::new(15.0, 15.0)));
        assert!(!ring.contains(Point::new(26.0, 4.0)), "it follows the L shape");
        // The steps take the first square's fill towards white.
        match &ev.below[1].fill {
            Fill::Solid(c) => {
                let [r, _, b] = c.to_rgb8();
                assert!(r > b, "reddish: {c:?}");
            }
            f => panic!("{f:?}"),
        }
    }

    #[test]
    fn a_contour_can_go_inside_and_outside_at_once() {
        let effect = |outside: bool, both: bool| Effect::Contour {
            steps: 3,
            offset: 2.0,
            outside,
            to_center: false,
            both,
            fill_to: Color::WHITE,
            outline_to: None,
            fill_end_to: None,
            color_blend: 0,
            corners: Some(crate::shaping::Join::Miter),
            accel_objects: 0.0,
            accel_colors: 0.0,
        };
        let with = |e: Effect| {
            let mut s = rect_shape();
            s.effects.push(e);
            evaluate(&s)
        };
        let out = with(effect(true, false));
        let inn = with(effect(false, false));
        let both = with(effect(true, true));
        assert_eq!(both.below.len(), 3);
        assert_eq!(both.above.len(), 3);
        let paths = |v: &[Shape]| v.iter().map(Shape::local_path).collect::<Vec<_>>();
        assert_eq!(paths(&both.below), paths(&out.below));
        assert_eq!(paths(&both.above), paths(&inn.above));
        // `outside` is not read with both sides on.
        let both_in = with(effect(false, true));
        assert_eq!(paths(&both_in.below), paths(&out.below));
        assert_eq!(paths(&both_in.above), paths(&inn.above));
        // Saved with the flag, read back the same.
        let json = serde_json::to_string(&effect(true, true)).expect("json");
        assert!(json.contains("\"both\":true"), "{json}");
        let back: Effect = serde_json::from_str(&json).expect("back");
        assert_eq!(back, effect(true, true));
        // One side: no flag written, so older versions read it as before;
        // an older file without it reads as one side.
        let json = serde_json::to_string(&effect(true, false)).expect("json");
        assert!(!json.contains("both"), "{json}");
        let back: Effect = serde_json::from_str(&json).expect("back");
        assert_eq!(back, effect(true, false));
        // A whole document through the .tdraw format.
        let mut doc = crate::Document::default();
        let layer = doc.pages[0].layers[0].id;
        let mut s = rect_shape();
        s.effects.push(effect(true, true));
        doc.layer_mut(layer).expect("layer").shapes.push(s);
        let mut packed = Vec::new();
        doc.write_tdraw_version(&mut packed, crate::document::TDRAW_FORMAT_VERSION)
            .expect("write");
        let back = crate::Document::from_tdraw_bytes(&packed).expect("read");
        assert_eq!(
            back.pages[0].layers[0].shapes[0].effects,
            vec![effect(true, true)]
        );
    }

    #[test]
    fn contour_outside_three_steps() {
        let mut s = rect_shape();
        s.effects.push(Effect::Contour {
            steps: 3,
            offset: 2.0,
            outside: true,
            to_center: false,
            both: false,
            fill_to: Color::WHITE,
            outline_to: None,
            fill_end_to: None,
            color_blend: 0,
            corners: Some(crate::shaping::Join::Miter),
            accel_objects: 0.0,
            accel_colors: 0.0,
        });
        let ev = evaluate(&s);
        assert_eq!(ev.below.len(), 3);
        let outer = ev.below[0].bounds();
        assert!((outer.width() - 32.0).abs() < 0.5, "{outer:?}");
        assert_eq!(ev.below[0].fill, Fill::Solid(Color::WHITE));
        // Mitred: the outermost step is a full square.
        let area = ev.below[0].local_path().area().abs();
        assert!((area - 32.0 * 32.0).abs() < 1.0, "{area}");
        // Accelerated: the first step lies nearer the object.
        if let Some(Effect::Contour { accel_objects, .. }) = s.effects.last_mut() {
            *accel_objects = 1.0;
        }
        let ev = evaluate(&s);
        let first = ev.below[2].bounds().width();
        assert!(first < 22.0 && first > 20.0, "{first}");
        // To the centre: steps until nothing is left, the last in the
        // end colour.
        let mut c = rect_shape();
        c.fill = Fill::Solid(Color::BLACK);
        c.effects.push(Effect::Contour {
            steps: 1,
            offset: 2.0,
            outside: false,
            to_center: true,
            both: false,
            fill_to: Color::WHITE,
            outline_to: None,
            fill_end_to: None,
            color_blend: 0,
            corners: None,
            accel_objects: 0.0,
            accel_colors: 0.0,
        });
        let ev = evaluate(&c);
        assert!(ev.above.len() >= 4, "{}", ev.above.len());
        assert_eq!(
            ev.above.last().map(|s| s.fill.clone()),
            Some(Fill::Solid(Color::WHITE))
        );
        assert!((accelerate(0.5, 0.0) - 0.5).abs() < 1e-12);
        assert!(accelerate(0.5, 1.0) < 0.1 && accelerate(0.5, -1.0) > 0.8);
    }

    /// A blend's acceleration moves its middle step as far toward the
    /// start at +1 as toward the end at -1 (#1261).
    #[test]
    fn blend_acceleration_goes_as_far_either_way() {
        assert!((blend_middle(0.0) - 0.5).abs() < 1e-12);
        let at = blend_middle;
        let far = 0.105_461;
        assert!((at(1.0) - far).abs() < 1e-5, "{}", at(1.0));
        assert!((at(-1.0) - (1.0 - far)).abs() < 1e-5);
        assert!((at(0.5) + at(-0.5) - 1.0).abs() < 1e-9);
        assert!((blend_middle(f64::NAN) - 0.5).abs() < 1e-12);
        assert!((at(7.0) - far).abs() < 1e-5, "held to the range");
        // Two squares 100 mm apart blended with one step: the step's
        // centre lands where the middle step should, either way.
        let square = |id: u64, x: f64| {
            let mut s = Shape::new(
                crate::ShapeId(id),
                ShapeKind::Rect {
                    rect: Rect::new(x, 0.0, x + 10.0, 10.0),
                    radius: 0.0,
                    corners: None,
                },
            );
            s.fill = Fill::Solid(Color::BLACK);
            s
        };
        let steps_at = |accel: f64| -> f64 {
            let start = square(1, 0.0);
            let end = square(2, 100.0);
            let steps = blend(
                &start,
                &end,
                1,
                (accel, 0.0),
                0.0,
                None,
                false,
                &BlendOptions::default(),
            );
            assert_eq!(steps.len(), 1);
            steps[0].bounds().center().x
        };
        assert!((steps_at(0.0) - 55.0).abs() < 1e-6, "{}", steps_at(0.0));
        let near = 5.0 + 100.0 * far;
        assert!((steps_at(1.0) - near).abs() < 1e-3, "{}", steps_at(1.0));
        assert!(
            (steps_at(-1.0) - (110.0 - near)).abs() < 1e-3,
            "{}",
            steps_at(-1.0)
        );
    }

    /// A looped blend's steps leave the start and reach the end along
    /// the curves the `.cdr` files' pictures show (#1192): a half circle
    /// about the middle at 180 degrees, bending right of the way for a
    /// positive turn, and a curl through the middle at 360.
    #[test]
    fn a_looped_blend_curves_from_one_end_to_the_other() {
        let (a, b) = (Point::new(0.0, 0.0), Point::new(100.0, 0.0));
        for r in [90.0, -90.0, 180.0, 360.0, -320.0] {
            assert!((loop_place(a, b, r, 0.0) - a).hypot() < 1e-9);
            assert!((loop_place(a, b, r, 1.0) - b).hypot() < 1e-9);
        }
        let half = loop_place(a, b, 180.0, 0.5);
        assert!((half - Point::new(50.0, -50.0)).hypot() < 1e-9, "{half:?}");
        let quarter = loop_place(a, b, 180.0, 0.25);
        assert!(((quarter - Point::new(50.0, 0.0)).hypot() - 50.0).abs() < 1e-9);
        assert!(loop_place(a, b, 90.0, 0.5).y < -1.0);
        assert!(loop_place(a, b, -90.0, 0.5).y > 1.0);
        assert!((loop_place(a, b, 360.0, 0.5) - Point::new(50.0, 0.0)).hypot() < 1e-9);
    }

    /// A looped blend turns the whole straight blend about the middle of
    /// the two objects (#1576): each step is the step of the blend to the
    /// end object turned back by the whole rotation, moved to its
    /// accelerated place and turned by its share about that middle. Two
    /// copies of a square: every step is the square turned by its share
    /// of the rotation, its centre on the loop's curve at its accelerated
    /// place, and with an object acceleration the steps packed at the
    /// start turn by their accelerated share, not their even one.
    #[test]
    fn a_looped_blend_turns_the_straight_blend_about_the_middle() {
        let a = rect_shape();
        let mut b = rect_shape();
        b.transform = Affine::translate((100.0, 0.0));
        let (ca, cb) = (a.bounds().center(), b.bounds().center());
        for (rotation, accel) in [(90.0, 0.0), (90.0, 0.8), (-45.0, -0.6), (240.0, 0.4)] {
            let mut s = a.clone();
            s.effects.push(Effect::Blend {
                end: Box::new(b.clone()),
                steps: 5,
                accel_objects: accel,
                accel_colors: 0.0,
                rotation,
                path: None,
                rotate_on_path: false,
                end_id: Some(b.id),
                options: BlendOptions {
                    loop_rotation: true,
                    ..Default::default()
                },
            });
            let steps = evaluate(&s).above;
            assert_eq!(steps.len(), 5);
            for (i, st) in steps.iter().enumerate() {
                let to = blend_place((i + 1) as f64 / 6.0, accel);
                let ShapeKind::Path { path, .. } = &st.kind else {
                    panic!("{:?}", st.kind);
                };
                let c = path.bounding_box().center();
                let want = loop_place(ca, cb, rotation, to);
                assert!(
                    (c - want).hypot() < 1e-6,
                    "{rotation} {accel} {i}: {c:?} {want:?}"
                );
                // The copy turned back by its share of the even way and
                // forward by its share of the accelerated one.
                let m = (i + 1) as f64 / 6.0;
                let pts: Vec<Point> = path
                    .elements()
                    .iter()
                    .filter_map(|e| match e {
                        PathEl::MoveTo(p) | PathEl::LineTo(p) => Some(*p),
                        _ => None,
                    })
                    .collect();
                let d = pts[1] - pts[0];
                let off = (d.y.atan2(d.x).to_degrees() - rotation * (to - m)).rem_euclid(90.0);
                assert!(off.min(90.0 - off) < 1e-6, "{rotation} {accel} {i}: {d:?}");
            }
        }
    }

    /// The steps of an accelerated blend lie where the `.cdr` files'
    /// blends put them (#1192): `t' = ln(1 + (k - 1) t) / ln k` with
    /// `k = 1.3 ^ (-25 a)`, measured on the pictures of the reporter's
    /// files (a = 0.37: the first of 50 steps at 7.6 % of the way, the
    /// tenth at 45.5 %, read here from the end the file counts from).
    #[test]
    fn blend_steps_follow_the_file_s_acceleration_curve() {
        assert!((blend_accel_factor(-1.0) - BLEND_ACCEL_FACTOR).abs() < 1e-9);
        assert!((BLEND_ACCEL_FACTOR - 1.3f64.powi(25)).abs() < 1e-9);
        // The factor 1.3 ^ (37 / 4) of the file, its steps measured from
        // the lower object (`1 - t'` of the upper one's).
        let a = 0.37;
        let from_lower = |k: usize| 1.0 - blend_place(1.0 - k as f64 / 51.0, a);
        assert!((from_lower(1) - 0.0759).abs() < 0.002, "{}", from_lower(1));
        assert!((from_lower(10) - 0.455).abs() < 0.003, "{}", from_lower(10));
        // Even at 0, monotonic, ends held.
        assert!((blend_place(0.3, 0.0) - 0.3).abs() < 1e-12);
        assert_eq!(blend_place(0.0, 1.0), 0.0);
        assert!((blend_place(1.0, -1.0) - 1.0).abs() < 1e-12);
        let mut last = 0.0;
        for k in 1..100 {
            let p = blend_place(k as f64 / 100.0, -0.8);
            assert!(p > last);
            last = p;
        }
        assert!((blend_place(0.5, 1e-12) - 0.5).abs() < 1e-9);
    }

    /// With Apply to sizing each step is the morph at its accelerated
    /// place, outline and size both: a square blended into a bigger
    /// circle with the steps packed toward the circle has round steps
    /// there, as the previews of the .cdr files with that setting show.
    /// It scaled the evenly morphed step instead, so the steps by the
    /// circle stayed square (#1576). Without the setting the steps keep
    /// the even morph and only move.
    #[test]
    fn apply_to_sizing_morphs_each_step_at_its_accelerated_place() {
        let start = rect_shape();
        let mut end = Shape::new(
            crate::ShapeId(2),
            ShapeKind::Ellipse {
                rect: Rect::new(100.0, -10.0, 140.0, 30.0),
                arc: None,
            },
        );
        end.fill = Fill::Solid(Color::rgb8(0, 0, 255));
        let (a, b) = (start.page_path(), end.page_path());
        let accel = -0.6;
        let steps = |accel_sizing: bool| {
            let options = BlendOptions {
                accel_sizing,
                ..Default::default()
            };
            blend(&start, &end, 4, (accel, 0.0), 0.0, None, false, &options)
        };
        let path = |s: &Shape| match &s.kind {
            ShapeKind::Path { path, .. } => s.transform * path.clone(),
            other => panic!("{other:?}"),
        };
        let near = |p: &BezPath, q: &BezPath| {
            p.elements().len() == q.elements().len()
                && p.elements().iter().zip(q.elements()).all(|(x, y)| {
                    let (xs, ys) = (x.end_point(), y.end_point());
                    matches!((xs, ys), (Some(u), Some(v)) if (u - v).hypot() < 1e-6)
                        || (xs.is_none() && ys.is_none())
                })
        };
        for (k, (sized, plain)) in steps(true).iter().zip(&steps(false)).enumerate() {
            let t = (k + 1) as f64 / 5.0;
            let to = blend_place(t, accel);
            assert!(to > t + 0.05, "{k}: packed toward the circle");
            let at = crate::effects::blend_from_nodes_at(&a, &b, &[to], 120, None);
            assert!(near(&path(sized), &at[0]), "{k}: the morph at {to}");
            // Unsized: the even morph, moved to the accelerated place.
            let even = crate::effects::blend_from_nodes_at(&a, &b, &[t], 120, None);
            let moved = Affine::translate(
                (b.bounding_box().center() - a.bounding_box().center()) * (to - t),
            ) * even[0].clone();
            assert!(near(&path(plain), &moved), "{k}: the even morph moved");
        }
    }

    #[test]
    fn visual_bounds_follow_the_live_effects() {
        let mut s = rect_shape();
        let plain = s.bounds();
        assert_eq!(s.visual_bounds(), plain);
        let mut nodes = crate::envelope::default_nodes(plain);
        nodes[5].y += 10.0;
        let frame = crate::envelope::Frame::from_parts(&nodes, &[], &[]).expect("frame");
        s.effects.push(Effect::from_envelope(
            &frame,
            false,
            crate::envelope::Mapping::Putty,
        ));
        assert!(s.has_live_effects());
        // The arch rises above the rectangle; plain bounds do not see it.
        assert!((s.visual_bounds().y1 - (plain.y1 + 10.0)).abs() < 0.5);
        assert_eq!(s.bounds(), plain);
        // In a group too.
        let g = Shape::new(
            crate::ShapeId(99),
            ShapeKind::Group {
                children: vec![s.clone()],
            },
        );
        assert!(g.has_live_effects());
        assert!((g.visual_bounds().y1 - (plain.y1 + 10.0)).abs() < 0.5);
    }

    #[test]
    fn identity_envelope_keeps_geometry() {
        let mut s = rect_shape();
        s.effects.push(Effect::envelope_around(s.bounds(), false));
        let pb = evaluate(&s).main.bounds();
        assert!((pb.x1 - 20.0).abs() < 1e-6 && (pb.y1 - 20.0).abs() < 1e-6);
    }

    #[test]
    fn envelope_moves_top_centre() {
        let mut s = rect_shape();
        let mut nodes = crate::envelope::default_nodes(s.bounds());
        nodes[5].y += 10.0;
        // As older files store it: the nodes alone.
        let json = serde_json::json!({
            "effect": "envelope",
            "nodes": nodes,
            "keep_lines": false,
        });
        let e: Effect = serde_json::from_value(json).expect("older envelope");
        s.effects.push(e.clone());
        assert!((evaluate(&s).main.bounds().y1 - 30.0).abs() < 0.5);
        // Written back as it was read.
        let back = serde_json::to_value(&e).expect("json");
        assert!(back.get("handles").is_none() && back.get("corners").is_none());
        assert_eq!(back.get("mapping").and_then(|m| m.as_str()), Some("putty"));
    }

    /// An envelope bends a picture too, alone or in a group: its top
    /// rises with the top of the frame, where it used to stay as it was
    /// while the outlines around it bent (#3608).
    #[test]
    fn envelope_bends_pictures_alone_and_in_groups() {
        let px: Vec<u8> = std::iter::repeat([200u8, 40, 40, 255])
            .take(20 * 20)
            .flatten()
            .collect();
        let png = crate::bitmap_warp::encode_rgba(20, 20, &px).expect("png");
        let picture = Shape::new(
            crate::ShapeId(7),
            ShapeKind::Bitmap {
                rect: Rect::new(0.0, 0.0, 20.0, 20.0),
                width_px: 20,
                height_px: 20,
                png: png.into(),
                fx: None,
            },
        );
        let group = Shape::new(
            crate::ShapeId(8),
            ShapeKind::Group {
                children: vec![picture.clone()],
            },
        );
        for mut s in [picture, group] {
            let mut nodes = crate::envelope::default_nodes(s.bounds());
            nodes[5].y += 10.0;
            s.effects.push(Effect::Envelope {
                nodes,
                keep_lines: false,
                handles: Vec::new(),
                corners: Vec::new(),
                mapping: crate::envelope::Mapping::Putty,
            });
            let main = evaluate(&s).main;
            let pic = match &main.kind {
                ShapeKind::Group { children } => children[0].clone(),
                _ => main.clone(),
            };
            let ShapeKind::Bitmap { rect, .. } = pic.kind else {
                panic!("still a picture: {:?}", pic.kind);
            };
            assert!((rect.y1 - 30.0).abs() < 0.5, "{rect:?}");
            assert!(
                rect.x0.abs() < 1e-6 && (rect.x1 - 20.0).abs() < 1e-6,
                "{rect:?}"
            );
        }
    }

    /// An envelope on a group reshapes each object from the box of them
    /// all, each keeping its fill; the group used to become one path with
    /// the group's empty fill and vanish.
    #[test]
    fn an_envelope_on_a_group_keeps_each_object() {
        let mut left = rect_shape();
        left.id = crate::ShapeId(2);
        let mut right = rect_shape();
        right.id = crate::ShapeId(3);
        right.transform = Affine::translate((30.0, 0.0));
        right.fill = Fill::Solid(Color::rgb8(0, 0, 255));
        let mut g = Shape::new(
            crate::ShapeId(4),
            ShapeKind::Group {
                children: vec![left, right],
            },
        );
        let b = Rect::new(0.0, 0.0, 50.0, 20.0);
        let mut frame = crate::envelope::Frame::around(b);
        // The top middle raised: an arch over both.
        frame.move_node(5, Vec2::new(0.0, 10.0), crate::envelope::Mode::SingleArc);
        g.effects
            .push(Effect::from_envelope(&frame, false, Default::default()));
        let ev = evaluate(&g);
        let ShapeKind::Group { children } = &ev.main.kind else {
            panic!("still a group: {:?}", ev.main.kind);
        };
        assert_eq!(children.len(), 2);
        assert_eq!(children[1].fill, Fill::Solid(Color::rgb8(0, 0, 255)));
        let page = |s: &Shape| (s.transform * s.local_path()).bounding_box();
        let (l, r) = (page(&children[0]), page(&children[1]));
        // Each stays on its side of the box, and the arch lifts the inner
        // edges more than the outer ones.
        assert!(l.x1 <= 25.0 && r.x0 >= 25.0, "{l:?} {r:?}");
        assert!(l.y1 > 20.0 && r.y1 > 20.0, "{l:?} {r:?}");
        assert!(l.y1 < 30.0 + 1e-6, "{l:?}");
        // Perspective on a group too (#458): each object stays, drawn.
        g.effects = vec![Effect::Perspective {
            corners: [
                Point::new(0.0, 0.0),
                Point::new(50.0, 0.0),
                Point::new(40.0, 20.0),
                Point::new(10.0, 20.0),
            ],
        }];
        let ev = evaluate(&g);
        let ShapeKind::Group { children } = &ev.main.kind else {
            panic!("still a group: {:?}", ev.main.kind);
        };
        assert_eq!(children.len(), 2);
        assert!(children
            .iter()
            .all(|c| !c.local_path().elements().is_empty()));
        assert_eq!(children[1].fill, Fill::Solid(Color::rgb8(0, 0, 255)));
    }

    /// The arch on [`rect_shape`]: its top middle raised 10 mm.
    fn arch() -> Effect {
        let mut nodes = crate::envelope::default_nodes(rect_shape().bounds());
        nodes[5].y += 10.0;
        let frame = crate::envelope::Frame::from_parts(&nodes, &[], &[]).expect("frame");
        Effect::from_envelope(&frame, false, crate::envelope::Mapping::Putty)
    }

    fn see_through() -> Effect {
        Effect::Transparency {
            mask: Fill::Solid(Color::WHITE),
            merge: MergeMode::Normal,
            target: 2,
        }
    }

    fn shadow() -> Effect {
        Effect::BlockShadow {
            offset: Vec2::new(2.0, -2.0),
            color: Color::BLACK,
            gap: 0.0,
        }
    }

    fn contour() -> Effect {
        Effect::Contour {
            steps: 1,
            offset: 2.0,
            outside: true,
            to_center: false,
            both: false,
            fill_to: Color::WHITE,
            outline_to: None,
            fill_end_to: None,
            color_blend: 0,
            corners: None,
            accel_objects: 0.0,
            accel_colors: 0.0,
        }
    }

    /// Convert to Curves makes an envelope part of the curve (#647): a
    /// path drawn as the enveloped object was, the other effects kept in
    /// their order, a switched-off one switched off still.
    #[test]
    fn bake_outline_warps_makes_the_envelope_the_outline() {
        let mut s = rect_shape();
        s.effects = vec![arch(), see_through()];
        s.parked_effects = vec![crate::document::ParkedEffect {
            index: 2,
            effect: shadow(),
        }];
        assert!(s.has_outline_warp());
        let baked = bake_outline_warps(&s).expect("baked");
        assert!(matches!(baked.kind, ShapeKind::Path { closed: true, .. }));
        assert_eq!(baked.effects, vec![see_through()]);
        assert_eq!(baked.parked_effects.len(), 1);
        assert_eq!(baked.parked_effects[0].index, 1);
        assert_eq!(baked.parked_effects[0].effect, shadow());
        assert_eq!(baked.fill, s.fill);
        assert!(!baked.has_outline_warp());
        assert!(bake_outline_warps(&baked).is_none());
        // Drawn as before, fitted with curves.
        assert!(same_outline(
            &baked.local_path(),
            &evaluate(&s).main.local_path(),
            0.02
        ));
        assert!(
            (baked.bounds().y1 - 30.0).abs() < 0.5,
            "{:?}",
            baked.bounds()
        );
        // Nothing to bake on a plain object.
        assert!(bake_outline_warps(&rect_shape()).is_none());
    }

    /// A warp after a contour, blend or other object drawn from the outline
    /// stays live: baking it would change what that effect draws.
    #[test]
    fn only_warps_before_a_contour_are_baked() {
        let mut s = rect_shape();
        s.effects = vec![contour(), arch()];
        assert!(!s.has_outline_warp());
        assert!(bake_outline_warps(&s).is_none());
        let twist = Effect::Distort(crate::effects::Distort::Twister {
            angle_deg: 30.0,
            center: None,
        });
        s.effects = vec![arch(), contour(), twist.clone()];
        let baked = bake_outline_warps(&s).expect("baked");
        assert_eq!(baked.effects, vec![contour(), twist]);
        // The contour is drawn around the same arch.
        assert!(same_outline(
            &evaluate(&baked).below[0].local_path(),
            &evaluate(&s).below[0].local_path(),
            0.05
        ));
        // A transparency does not draw from the outline.
        s.effects = vec![see_through(), arch()];
        let baked = bake_outline_warps(&s).expect("baked");
        assert_eq!(baked.effects, vec![see_through()]);
    }

    /// Perspective and distortion bake too, one after the other.
    #[test]
    fn perspective_and_distortion_bake_too() {
        let mut s = rect_shape();
        s.effects = vec![
            Effect::Perspective {
                corners: [
                    Point::new(0.0, 0.0),
                    Point::new(20.0, 0.0),
                    Point::new(15.0, 20.0),
                    Point::new(5.0, 20.0),
                ],
            },
            Effect::Distort(crate::effects::Distort::PushPull {
                amount: 30.0,
                center: None,
            }),
        ];
        let baked = bake_outline_warps(&s).expect("baked");
        assert!(baked.effects.is_empty() && baked.parked_effects.is_empty());
        assert!(same_outline(
            &baked.local_path(),
            &evaluate(&s).main.local_path(),
            0.02
        ));
    }

    /// An envelope on a group bakes into each object, which keeps its own
    /// look: the group stays a group of the same objects.
    #[test]
    fn an_envelope_on_a_group_bakes_into_each_object() {
        let mut left = rect_shape();
        left.id = crate::ShapeId(2);
        let mut right = rect_shape();
        right.id = crate::ShapeId(3);
        right.transform = Affine::translate((30.0, 0.0));
        right.fill = Fill::Solid(Color::rgb8(0, 0, 255));
        let mut g = Shape::new(
            crate::ShapeId(4),
            ShapeKind::Group {
                children: vec![left, right],
            },
        );
        let mut frame = crate::envelope::Frame::around(Rect::new(0.0, 0.0, 50.0, 20.0));
        frame.move_node(5, Vec2::new(0.0, 10.0), crate::envelope::Mode::SingleArc);
        g.effects
            .push(Effect::from_envelope(&frame, false, Default::default()));
        assert!(g.has_outline_warp());
        let baked = bake_outline_warps(&g).expect("baked");
        assert!(baked.effects.is_empty());
        let ShapeKind::Group { children } = &baked.kind else {
            panic!("still a group: {:?}", baked.kind);
        };
        let ids: Vec<_> = children.iter().map(|c| c.id).collect();
        assert_eq!(ids, vec![crate::ShapeId(2), crate::ShapeId(3)]);
        assert!(children
            .iter()
            .all(|c| matches!(c.kind, ShapeKind::Path { .. })));
        assert_eq!(children[1].fill, Fill::Solid(Color::rgb8(0, 0, 255)));
        assert!(
            baked.visual_bounds().y1 > 25.0,
            "{:?}",
            baked.visual_bounds()
        );
    }

    /// A picture cannot become a curve: it keeps its envelope, on its own
    /// or in a group.
    #[test]
    fn a_bitmap_keeps_its_envelope() {
        let mut b = Shape::new(
            crate::ShapeId(7),
            ShapeKind::Bitmap {
                rect: Rect::new(0.0, 0.0, 20.0, 20.0),
                width_px: 1,
                height_px: 1,
                png: Default::default(),
                fx: None,
            },
        );
        b.effects = vec![arch()];
        assert!(!b.has_outline_warp());
        assert!(bake_outline_warps(&b).is_none());
        let mut g = Shape::new(
            crate::ShapeId(8),
            ShapeKind::Group {
                children: vec![rect_shape(), {
                    b.effects.clear();
                    b
                }],
            },
        );
        g.effects = vec![arch()];
        assert!(!g.has_outline_warp());
        assert!(bake_outline_warps(&g).is_none());
    }

    /// The warp of the whole applied to each part gives the parts of the
    /// warped whole (the letters of a text broken apart).
    #[test]
    fn outline_warp_warps_parts_as_the_whole() {
        let square = |x: f64| {
            let mut p = BezPath::new();
            p.move_to((x, 0.0));
            p.line_to((x + 8.0, 0.0));
            p.line_to((x + 8.0, 20.0));
            p.line_to((x, 20.0));
            p.close_path();
            p
        };
        let (a, b) = (square(0.0), square(12.0));
        let mut whole = a.clone();
        whole.extend(b.elements().iter().copied());
        let mut s = Shape::new(
            crate::ShapeId(9),
            ShapeKind::Path {
                path: whole,
                closed: true,
            },
        );
        s.effects = vec![arch()];
        let warp = outline_warp(&s).expect("warp");
        let mut parts = warp.apply(&a);
        parts.extend(warp.apply(&b).elements().iter().copied());
        assert_eq!(parts.elements(), evaluate(&s).main.local_path().elements());
        // Nothing for a group, or with no warp to bake.
        s.effects = vec![contour(), arch()];
        assert!(outline_warp(&s).is_none());
        // A twister and a local zipper measure their reach on the whole:
        // three parts warped together give the parts of the warped whole
        // (each part on its own turned far more).
        let parts = [square(0.0), square(12.0), square(24.0)];
        let mut whole = BezPath::new();
        for p in &parts {
            whole.extend(p.elements().iter().copied());
        }
        s.kind = ShapeKind::Path {
            path: whole,
            closed: true,
        };
        for how in [
            crate::effects::Distort::Twister {
                angle_deg: 90.0,
                center: None,
            },
            crate::effects::Distort::Zipper {
                amplitude: 2.0,
                frequency: 4,
                center: None,
                random: false,
                smooth: false,
                local: true,
                tilt_deg: 0.0,
                shifted_tips: false,
            },
        ] {
            s.effects = vec![Effect::Distort(how)];
            let warp = outline_warp(&s).expect("warp");
            let got = warp.apply_parts(&parts);
            assert_eq!(got.len(), 3);
            let mut joined = BezPath::new();
            for p in &got {
                joined.extend(p.elements().iter().copied());
            }
            assert_eq!(
                joined.elements(),
                evaluate(&s).main.local_path().elements(),
                "{how:?}"
            );
        }
    }

    /// A warp baked into a group's objects drops their B-spline control
    /// points and node types: the Shape tool would rebuild the old outline
    /// from them.
    #[test]
    fn a_baked_group_object_loses_its_spline() {
        let mut child = rect_shape();
        child.id = crate::ShapeId(2);
        child.kind = ShapeKind::Path {
            path: child.local_path(),
            closed: true,
        };
        child.spline = Some(vec![Point::new(0.0, 0.0), Point::new(20.0, 20.0)]);
        child.node_types.insert(1, crate::nodes::NodeType::Smooth);
        let mut g = Shape::new(
            crate::ShapeId(4),
            ShapeKind::Group {
                children: vec![child],
            },
        );
        g.effects = vec![arch()];
        let baked = bake_outline_warps(&g).expect("baked");
        let ShapeKind::Group { children } = &baked.kind else {
            panic!("a group");
        };
        assert!(children[0].spline.is_none());
        assert!(children[0].node_types.is_empty());
    }

    /// A curve whose own space is small and scaled up by its transform
    /// (letters are) stays smooth under an envelope: it was flattened
    /// with a fixed tolerance and came out in facets (#455).
    #[test]
    fn an_envelope_keeps_small_curves_smooth() {
        let mut s = Shape::new(
            crate::ShapeId(5),
            ShapeKind::Ellipse {
                rect: Rect::new(-0.5, -0.5, 0.5, 0.5),
                arc: None,
            },
        );
        s.transform = Affine::scale(80.0);
        let b = s.local_path().bounding_box();
        s.effects.push(Effect::from_envelope(
            &crate::envelope::Frame::around(b),
            false,
            Default::default(),
        ));
        let ev = evaluate(&s);
        let page = s.transform * ev.main.local_path();
        // Every point of the warped outline lies on the 40 mm circle, and
        // the chords between them sag less than a few hundredths of a mm
        // (the Bézier circle itself is off by about one).
        let mut worst: f64 = 0.0;
        let mut prev: Option<Point> = None;
        for el in page.elements() {
            let p = match el {
                PathEl::MoveTo(p) | PathEl::LineTo(p) => *p,
                _ => continue,
            };
            if let Some(q) = prev {
                let mid = q.midpoint(p);
                worst = worst.max((40.0 - mid.to_vec2().hypot()).abs());
            }
            worst = worst.max((40.0 - p.to_vec2().hypot()).abs());
            prev = Some(p);
        }
        assert!(worst < 0.03, "{worst}");
    }

    #[test]
    fn perspective_maps_corners() {
        let s = rect_shape();
        let corners = [
            Point::new(0.0, 0.0),
            Point::new(20.0, 0.0),
            Point::new(15.0, 20.0),
            Point::new(5.0, 20.0),
        ];
        let p = perspective(&s.local_path(), &corners);
        let b = p.bounding_box();
        assert!((b.x0 - 0.0).abs() < 1e-6 && (b.x1 - 20.0).abs() < 1e-6);
        assert!((b.y1 - 20.0).abs() < 1e-6);
    }

    /// A perspective whose top corners meet (the frame a triangle) still
    /// warps the outline, and an ellipse comes to a tip there, a drop:
    /// its top node and the handles beside it all go to the point where
    /// the corners meet (#879). It was left as it was, since no projective
    /// map takes a box to a triangle; corners a hair apart, or a hair past
    /// each other, give the same.
    #[test]
    fn a_perspective_with_two_corners_together_makes_a_tip() {
        let ellipse = Shape::new(
            crate::ShapeId(3),
            ShapeKind::Ellipse {
                rect: Rect::new(0.0, 0.0, 20.0, 40.0),
                arc: None,
            },
        )
        .local_path();
        let apex = Point::new(10.0, 60.0);
        let corners = [Point::new(-20.0, 0.0), Point::new(40.0, 0.0), apex, apex];
        let p = perspective(&ellipse, &corners);
        assert_ne!(p, ellipse);
        assert_eq!(p.elements().len(), ellipse.elements().len());
        let b = p.bounding_box();
        assert!((b.y1 - 60.0).abs() < 0.05, "{b:?}");
        assert!(b.y0.abs() < 0.05, "{b:?}");
        // The top node and both handles beside it at the tip.
        let els = p.elements();
        let k = els
            .iter()
            .position(|e| matches!(e, PathEl::CurveTo(_, _, q) if (*q - apex).hypot() < 0.05))
            .expect("a node at the tip");
        let PathEl::CurveTo(_, c2, _) = els[k] else {
            unreachable!()
        };
        let next = els[k + 1..]
            .iter()
            .chain(els.iter())
            .find_map(|e| match e {
                PathEl::CurveTo(c1, _, _) => Some(*c1),
                _ => None,
            })
            .expect("the next curve");
        assert!((c2 - apex).hypot() < 0.05, "{c2:?}");
        assert!((next - apex).hypot() < 0.05, "{next:?}");
        // Corners nearly together give nearly the same drop.
        let close = [
            corners[0],
            corners[1],
            apex + Vec2::new(0.001, 0.0),
            apex - Vec2::new(0.001, 0.0),
        ];
        let q = perspective(&ellipse, &close).bounding_box();
        for (a, b) in [(q.x0, b.x0), (q.y0, b.y0), (q.x1, b.x1), (q.y1, b.y1)] {
            assert!((a - b).abs() < 0.05, "{q:?} {b:?}");
        }
        // Dragged together by hand, the two corners end a little past
        // each other (as in the drawing of the report): the same drop, not
        // a sliver running off past the frame.
        let crossed = [
            corners[0],
            corners[1],
            apex - Vec2::new(0.05, 0.0),
            apex + Vec2::new(0.05, 0.0),
        ];
        let q = perspective(&ellipse, &crossed).bounding_box();
        for (a, b) in [(q.x0, b.x0), (q.y0, b.y0), (q.x1, b.x1), (q.y1, b.y1)] {
            assert!((a - b).abs() < 0.05, "{q:?} {b:?}");
        }
    }

    /// A frame folded over itself puts part of the outline past the
    /// horizon: it is mapped point by point there and stays finite.
    #[test]
    fn a_folded_perspective_stays_finite() {
        let ellipse = Shape::new(
            crate::ShapeId(3),
            ShapeKind::Ellipse {
                rect: Rect::new(0.0, 0.0, 20.0, 20.0),
                arc: None,
            },
        )
        .local_path();
        let corners = [
            Point::new(0.0, 0.0),
            Point::new(20.0, 0.0),
            Point::new(-30.0, 5.0),
            Point::new(0.0, 20.0),
        ];
        let p = perspective(&ellipse, &corners);
        assert!(p.elements().iter().all(|e| e
            .end_point()
            .is_none_or(|q| q.x.is_finite() && q.y.is_finite())));
    }

    /// Convert to Curves of a square in perspective gives its four
    /// corners joined by straight sides, and of a circle a few curves, not
    /// a row of points (#840, #647).
    #[test]
    fn a_baked_perspective_is_a_few_nodes() {
        let corners = [
            Point::new(0.0, 0.0),
            Point::new(20.0, 0.0),
            Point::new(15.0, 20.0),
            Point::new(5.0, 20.0),
        ];
        let pieces = |p: &BezPath| {
            p.elements()
                .iter()
                .filter(|e| !matches!(e, PathEl::MoveTo(_) | PathEl::ClosePath))
                .count()
        };
        let mut square = rect_shape();
        square.effects = vec![Effect::Perspective { corners }];
        let baked = bake_outline_warps(&square).expect("baked");
        let p = baked.local_path();
        assert!(pieces(&p) <= 4, "{:?}", p.elements());
        assert!(p
            .elements()
            .iter()
            .all(|e| !matches!(e, PathEl::CurveTo(..))));
        assert!(same_outline(&p, &evaluate(&square).main.local_path(), 0.02));
        let mut circle = Shape::new(
            crate::ShapeId(2),
            ShapeKind::Ellipse {
                rect: Rect::new(0.0, 0.0, 20.0, 20.0),
                arc: None,
            },
        );
        circle.effects = vec![Effect::Perspective { corners }];
        let baked = bake_outline_warps(&circle).expect("baked");
        let p = baked.local_path();
        assert!(pieces(&p) <= 12, "{} pieces", pieces(&p));
        assert!(same_outline(&p, &evaluate(&circle).main.local_path(), 0.05));
    }

    /// A blend inside a group the group's envelope bends: its end copy
    /// bends too, so the middle steps follow the warp (#2120). They were
    /// mixed toward the unwarped end and stayed where they were.
    #[test]
    fn a_grouped_blend_bends_with_the_group() {
        let a = rect_shape();
        let mut b = rect_shape();
        b.id = crate::ShapeId(2);
        b.transform = Affine::translate((100.0, 0.0));
        let mut start = a.clone();
        start.effects.push(Effect::Blend {
            end: Box::new(b.clone()),
            steps: 3,
            accel_objects: 0.0,
            accel_colors: 0.0,
            rotation: 0.0,
            path: None,
            rotate_on_path: false,
            end_id: Some(b.id),
            options: BlendOptions::default(),
        });
        let mut group = Shape::new(
            crate::ShapeId(3),
            ShapeKind::Group {
                children: vec![start, b],
            },
        );
        // The group moved: its objects' space is not the page.
        group.transform = Affine::translate((5.0, 7.0));
        // A warp raising everything by half its distance from the left.
        reshape(&mut group, &|p, _| {
            let mut q = p.clone();
            q.apply_affine(Affine::new([1.0, 0.5, 0.0, 1.0, 0.0, 0.0]));
            q
        });
        let ShapeKind::Group { children } = &group.kind else {
            panic!("still a group");
        };
        let ev = evaluate(&children[0]);
        let end = children[1].bounds().center();
        let first = children[0].bounds().center();
        let mid = ev.above[1].bounds().center();
        let want = first.midpoint(end);
        assert!((mid - want).hypot() < 0.5, "{mid:?} not {want:?}");
    }

    /// A blend asking for more steps than the Blend tool offers draws
    /// that most, so a misread or edited value cannot fill the memory
    /// (#980).
    #[test]
    fn a_blend_draws_at_most_its_maximum_steps() {
        let a = rect_shape();
        let mut b = rect_shape();
        b.transform = Affine::translate((100.0, 0.0));
        let mut s = a.clone();
        s.effects.push(Effect::Blend {
            end: Box::new(b.clone()),
            steps: u32::MAX,
            accel_objects: 0.0,
            accel_colors: 0.0,
            rotation: 0.0,
            path: None,
            rotate_on_path: false,
            end_id: Some(b.id),
            options: BlendOptions::default(),
        });
        assert_eq!(evaluate(&s).above.len(), MAX_BLEND_STEPS as usize);
    }

    /// The box of a blend's start object is the object itself: the steps
    /// lie between it and the end object, which has a box of its own
    /// (#1540). A blend from an older file, whose end lives only in the
    /// effect, is one object and keeps the whole blend in its box.
    #[test]
    fn a_blend_start_s_own_box_leaves_the_steps_out() {
        let a = rect_shape();
        let mut b = rect_shape();
        b.transform = Affine::translate((100.0, 0.0));
        let mut s = a.clone();
        s.effects.push(Effect::Blend {
            end: Box::new(b.clone()),
            steps: 5,
            accel_objects: 0.0,
            accel_colors: 0.0,
            rotation: 0.0,
            path: None,
            rotate_on_path: false,
            end_id: Some(b.id),
            options: BlendOptions::default(),
        });
        assert!(s.starts_linked_blend());
        let own = s.own_visual_bounds();
        assert!((own.x0 - a.bounds().x0).abs() < 1e-9, "{own:?}");
        assert!((own.x1 - a.bounds().x1).abs() < 1e-9, "{own:?}");
        // What it draws still reaches the steps next to the end.
        assert!(s.visual_bounds().x1 > a.bounds().x1 + 50.0);
        // Other live effects of the start object stay in its box.
        let mut nodes = crate::envelope::default_nodes(a.bounds());
        nodes[5].y += 10.0;
        let frame = crate::envelope::Frame::from_parts(&nodes, &[], &[]).expect("frame");
        let mut warped = s.clone();
        warped.effects.insert(
            0,
            Effect::from_envelope(&frame, false, crate::envelope::Mapping::Putty),
        );
        let own = warped.own_visual_bounds();
        assert!((own.y1 - (a.bounds().y1 + 10.0)).abs() < 0.5, "{own:?}");
        assert!(own.x1 < a.bounds().x1 + 1.0, "{own:?}");
        // The end only in the effect: the whole blend.
        let mut old = s.clone();
        if let Some(Effect::Blend { end_id, .. }) = old.effects.last_mut() {
            *end_id = None;
        }
        assert!(!old.starts_linked_blend());
        assert_eq!(old.own_visual_bounds(), old.visual_bounds());
        // Without a blend, the visual bounds.
        assert_eq!(a.own_visual_bounds(), a.visual_bounds());
    }

    #[test]
    fn blend_midpoint_is_halfway() {
        let a = rect_shape();
        let mut b = rect_shape();
        b.transform = Affine::translate((100.0, 0.0));
        b.fill = Fill::Solid(Color::rgb8(0, 0, 255));
        let mut s = a.clone();
        s.effects.push(Effect::Blend {
            end: Box::new(b.clone()),
            steps: 9,
            accel_objects: 0.0,
            accel_colors: 0.0,
            rotation: 0.0,
            path: None,
            rotate_on_path: false,
            end_id: None,
            options: BlendOptions::default(),
        });
        let ev = evaluate(&s);
        assert_eq!(ev.above.len(), 10);
        // With an end object of the drawing, the steps only.
        let mut linked = s.clone();
        if let Some(Effect::Blend { end_id, .. }) = linked.effects.last_mut() {
            *end_id = Some(b.id);
        }
        assert_eq!(evaluate(&linked).above.len(), 9);
        let mid = ev.above[4].bounds().center();
        assert!((mid.x - 60.0).abs() < 1.0, "{mid:?}");
    }

    /// The object acceleration moves the steps toward one end of a blend
    /// off a path; it only changed their sizes and turns, so the slider
    /// seemed to do nothing (#910).
    #[test]
    fn object_acceleration_moves_the_steps() {
        let a = rect_shape();
        let mut b = rect_shape();
        b.transform = Affine::translate((100.0, 0.0));
        let centres = |accel: f64| -> Vec<f64> {
            let mut s = a.clone();
            s.effects.push(Effect::Blend {
                end: Box::new(b.clone()),
                steps: 3,
                accel_objects: accel,
                accel_colors: 0.0,
                rotation: 0.0,
                path: None,
                rotate_on_path: false,
                end_id: Some(b.id),
                options: BlendOptions::default(),
            });
            evaluate(&s)
                .above
                .iter()
                .map(|x| x.bounds().center().x)
                .collect()
        };
        let even = centres(0.0);
        let x0 = a.bounds().center().x;
        for (k, x) in even.iter().enumerate() {
            let want = x0 + 100.0 * (k + 1) as f64 / 4.0;
            assert!((x - want).abs() < 1.0, "even step {k}: {x} not {want}");
        }
        // Toward the start object, then toward the end one.
        let to_start = centres(0.8);
        let to_end = centres(-0.8);
        for k in 0..3 {
            assert!(
                to_start[k] < even[k] - 2.0,
                "step {k}: {to_start:?} {even:?}"
            );
            assert!(to_end[k] > even[k] + 2.0, "step {k}: {to_end:?} {even:?}");
        }
        // Still in order between the two objects.
        assert!(to_start.windows(2).all(|w| w[0] < w[1]) && to_start[0] > x0);
        assert!(to_end.windows(2).all(|w| w[0] < w[1]) && to_end[2] < x0 + 100.0);
    }

    /// A blend to a turned copy of the object: each step turned by an
    /// even share of the way and the same size, none squashed (#1260).
    #[test]
    fn a_blend_to_a_turned_copy_turns_its_steps_evenly() {
        use crate::geometry::Shape as _;
        let mut a = rect_shape();
        a.kind = ShapeKind::Rect {
            rect: Rect::new(0.0, 0.0, 40.0, 10.0),
            radius: 0.0,
            corners: None,
        };
        let mut b = a.clone();
        b.id = crate::ShapeId(2);
        // Turned a quarter about its centre and moved 100 mm right.
        b.transform = Affine::translate((100.0, 0.0))
            * Affine::rotate_about(std::f64::consts::FRAC_PI_2, Point::new(20.0, 5.0));
        let mut s = a.clone();
        s.effects.push(Effect::Blend {
            end: Box::new(b.clone()),
            steps: 3,
            accel_objects: 0.0,
            accel_colors: 0.0,
            rotation: 0.0,
            path: None,
            rotate_on_path: false,
            end_id: Some(b.id),
            options: BlendOptions::default(),
        });
        let steps = evaluate(&s).above;
        assert_eq!(steps.len(), 3);
        for (i, st) in steps.iter().enumerate() {
            let path = st.page_path();
            // The same area as the object: none shrinks.
            assert!(
                (path.area().abs() - 400.0).abs() < 1e-6,
                "{i}: {}",
                path.area()
            );
            // Its first edge turned by a quarter of the quarter turn per step.
            let pts: Vec<Point> = path
                .elements()
                .iter()
                .filter_map(|e| match e {
                    PathEl::MoveTo(p) | PathEl::LineTo(p) => Some(*p),
                    _ => None,
                })
                .collect();
            let d = pts[1] - pts[0];
            let want = 22.5 * (i + 1) as f64;
            // (Whichever edge comes first: the turn counts within a quarter.)
            let off = (d.y.atan2(d.x).to_degrees() - want).rem_euclid(90.0);
            assert!(off.min(90.0 - off) < 1e-6, "{i}: {d:?}");
            // Its centre evenly along the line between the two.
            let c = path.bounding_box().center();
            let x = 20.0 + 100.0 * (i + 1) as f64 / 4.0;
            assert!(
                (c.x - x).abs() < 1e-6 && (c.y - 5.0).abs() < 1e-6,
                "{i}: {c:?}"
            );
        }
    }

    /// A point in any copy of a symmetry goes back to the original's side
    /// by the one mirror or turn that made that copy (#2998).
    #[test]
    fn a_point_in_a_symmetry_copy_goes_back_to_the_original() {
        let c = Point::new(50.0, 50.0);
        // One vertical line: the right half mirrors back to the left.
        let back = symmetry_back_to(c, 90.0, 1, Point::new(20.0, 50.0), Point::new(70.0, 40.0));
        let q = back * Point::new(70.0, 40.0);
        assert!((q.x - 30.0).abs() < 1e-9 && (q.y - 40.0).abs() < 1e-9, "{q:?}");
        // On the original's side: as it is.
        let same = symmetry_back_to(c, 90.0, 1, Point::new(20.0, 50.0), Point::new(30.0, 60.0));
        assert_eq!(same, Affine::IDENTITY);
        // Three lines (six slices): every point lands in the original's
        // slice, at the same distance from the centre.
        let origin = c + Vec2::new(20.0, 5.0);
        let slice = |q: Point| {
            let a = ((q - c).y.atan2((q - c).x)).rem_euclid(std::f64::consts::TAU);
            (a / (std::f64::consts::PI / 3.0)).floor() as i64
        };
        for k in 0..12 {
            let a = k as f64 * 0.5 + 0.1;
            let p = c + Vec2::new(a.cos(), a.sin()) * 15.0;
            let q = symmetry_back_to(c, 0.0, 3, origin, p) * p;
            assert_eq!(slice(q), slice(origin), "{p:?} -> {q:?}");
            assert!(((q - c).hypot() - 15.0).abs() < 1e-9);
        }
    }

    /// Two open curves with a yellow outline, the end one faded, blended
    /// in 146 steps along an S curve: every step is an open curve on the
    /// path with the outline, the transparency fades from end to end, and
    /// it takes well under a second (#2946).
    #[test]
    fn a_ribbon_of_many_faded_steps_along_an_s_curve() {
        let curve = |dx: f64| {
            let mut p = BezPath::new();
            p.move_to((0.0 + dx, 0.0));
            p.curve_to((20.0 + dx, -30.0), (60.0 + dx, 30.0), (80.0 + dx, 0.0));
            p
        };
        let yellow = Color::rgb8(240, 200, 30);
        let line = |dx: f64, opacity: f64| {
            let mut s = rect_shape();
            s.kind = ShapeKind::Path {
                path: curve(dx),
                closed: false,
            };
            s.fill = Fill::None;
            s.stroke = Some(crate::style::Stroke {
                color: yellow,
                width: 0.5,
                ..Default::default()
            });
            s.opacity = opacity;
            s
        };
        let b = line(0.0, 0.1);
        let mut a = line(0.0, 1.0);
        let mut s_curve = BezPath::new();
        s_curve.move_to((40.0, 0.0));
        s_curve.curve_to((140.0, 60.0), (-60.0, 140.0), (40.0, 200.0));
        a.effects.push(Effect::Blend {
            end: Box::new(b.clone()),
            steps: 146,
            accel_objects: 0.0,
            accel_colors: 0.0,
            rotation: 0.0,
            path: Some(s_curve.clone()),
            rotate_on_path: false,
            end_id: Some(b.id),
            options: BlendOptions::default(),
        });
        let t0 = std::time::Instant::now();
        let ev = evaluate(&a);
        let took = t0.elapsed();
        assert!(took.as_secs_f64() < 5.0, "{took:?}");
        assert_eq!(ev.above.len(), 146);
        let mut last = 1.0 + 1e-9;
        for st in &ev.above {
            assert!(matches!(st.kind, ShapeKind::Path { closed: false, .. }));
            assert_eq!(st.stroke.as_ref().map(|s| s.color), Some(yellow));
            assert!(st.opacity <= last && st.opacity >= 0.1 - 1e-9, "{}", st.opacity);
            last = st.opacity;
        }
        assert!(ev.above[0].opacity > 0.95 && ev.above[145].opacity < 0.15);
        // Each step's centre lies on the S curve.
        let near = |p: Point| {
            let mut pts = Vec::new();
            kurbo::flatten(s_curve.elements().iter().copied(), 0.05, |el| {
                if let PathEl::LineTo(q) | PathEl::MoveTo(q) = el {
                    pts.push(q);
                }
            });
            pts.windows(2)
                .map(|w| {
                    use kurbo::ParamCurveNearest;
                    kurbo::Line::new(w[0], w[1]).nearest(p, 1e-6).distance_sq.sqrt()
                })
                .fold(f64::MAX, f64::min)
        };
        for st in &ev.above {
            let c = st.page_path().bounding_box().center();
            assert!(near(c) < 0.5, "{c:?} {}", near(c));
        }
    }

    /// Along a path, the steps run between where the start and the end
    /// sit on it, anywhere along it, either way round (#2946).
    #[test]
    fn a_blend_runs_along_the_part_of_its_path_between_its_ends() {
        let a = rect_shape();
        let mut b = rect_shape();
        b.transform = Affine::translate((100.0, 0.0));
        let mut line = BezPath::new();
        line.move_to((0.0, 50.0));
        line.line_to((100.0, 50.0));
        let centres = |range: Option<[f64; 2]>| {
            let mut s = a.clone();
            s.effects.push(Effect::Blend {
                end: Box::new(b.clone()),
                steps: 3,
                accel_objects: 0.0,
                accel_colors: 0.0,
                rotation: 0.0,
                path: Some(line.clone()),
                rotate_on_path: false,
                end_id: Some(b.id),
                options: BlendOptions {
                    path_range: range,
                    ..Default::default()
                },
            });
            evaluate(&s)
                .above
                .iter()
                .map(|c| c.bounds().center().x)
                .collect::<Vec<_>>()
        };
        let near = |xs: Vec<f64>, want: [f64; 3]| {
            for (x, w) in xs.iter().zip(want) {
                assert!((x - w).abs() < 0.5, "{xs:?} {want:?}");
            }
        };
        // The whole path: at a quarter, half and three quarters.
        near(centres(None), [25.0, 50.0, 75.0]);
        // From 20 % to 60 %: between 20 and 60 mm.
        near(centres(Some([0.2, 0.6])), [30.0, 40.0, 50.0]);
        // The other way round: from 60 mm back to 20 mm.
        near(centres(Some([0.6, 0.2])), [50.0, 40.0, 30.0]);
        // A share out of range is held to the path.
        near(centres(Some([-1.0, f64::NAN])), [25.0, 50.0, 75.0]);
        let (p, d) = point_along(&line, 0.25).expect("a point");
        assert!((p.x - 25.0).abs() < 1e-6 && d.x > 0.0, "{p:?}");
        assert!(point_along(&BezPath::new(), 0.5).is_none());
    }

    /// A rectangle without an outline blended into an outlined letter
    /// (text converted to curves): every step is outlined, with the
    /// letter's outline, and the steps follow when the letter moves or
    /// the rectangle's corners are rounded afterwards.
    #[test]
    fn a_blend_into_an_outlined_letter_outlines_every_step_and_follows_edits() {
        let mut a = rect_shape();
        a.kind = ShapeKind::Rect {
            rect: Rect::new(-100.0, -100.0, 200.0, 150.0),
            radius: 0.0,
            corners: None,
        };
        a.stroke = None;
        let mut m = BezPath::new();
        for (i, (x, y)) in [
            (0.0, 0.0),
            (0.0, 40.0),
            (10.0, 40.0),
            (20.0, 20.0),
            (30.0, 40.0),
            (40.0, 40.0),
            (40.0, 0.0),
            (30.0, 0.0),
            (30.0, 25.0),
            (20.0, 8.0),
            (10.0, 25.0),
            (10.0, 0.0),
        ]
        .into_iter()
        .enumerate()
        {
            if i == 0 {
                m.move_to((x, y));
            } else {
                m.line_to((x, y));
            }
        }
        m.close_path();
        let mut b = Shape::new(
            crate::ShapeId(2),
            ShapeKind::Path {
                path: m,
                closed: true,
            },
        );
        b.fill = Fill::Solid(Color::rgb8(255, 255, 255));
        b.stroke = Some(crate::style::Stroke {
            width: 0.35,
            ..Default::default()
        });
        let with_end = |a: &Shape, b: &Shape| {
            let mut s = a.clone();
            s.effects.push(Effect::Blend {
                end: Box::new(b.clone()),
                steps: 6,
                accel_objects: 0.0,
                accel_colors: 0.0,
                rotation: 0.0,
                path: None,
                rotate_on_path: false,
                end_id: Some(b.id),
                options: BlendOptions::default(),
            });
            evaluate(&s).above
        };
        let steps = with_end(&a, &b);
        assert_eq!(steps.len(), 6);
        for s in &steps {
            let st = s.stroke.as_ref().expect("every step outlined");
            assert!((st.width - 0.35).abs() < 1e-9, "{}", st.width);
        }
        // The letter moved: the step next to it moves with it.
        let near = |steps: &[Shape]| steps[5].page_path().bounding_box();
        let before = near(&steps);
        let mut moved = b.clone();
        moved.transform = Affine::translate((30.0, 20.0));
        let after = near(&with_end(&a, &moved));
        let shift = after.center() - before.center();
        assert!(
            (shift.x - 30.0 * 6.0 / 7.0).abs() < 0.5 && (shift.y - 20.0 * 6.0 / 7.0).abs() < 0.5,
            "{shift:?}"
        );
        // The rectangle's corners rounded: the step next to it rounds
        // too (its corner no longer reaches the box's corner).
        let first = |steps: &[Shape]| steps[0].page_path();
        let sharp = first(&steps);
        let corner = sharp.bounding_box().origin();
        assert!(sharp.winding(corner + Vec2::new(3.0, 3.0)) != 0);
        let mut round = a.clone();
        round.kind = ShapeKind::Rect {
            rect: Rect::new(-100.0, -100.0, 200.0, 150.0),
            radius: 30.0,
            corners: None,
        };
        let rounded = first(&with_end(&round, &b));
        let corner = rounded.bounding_box().origin();
        assert_eq!(
            rounded.winding(corner + Vec2::new(3.0, 3.0)),
            0,
            "{rounded:?}"
        );
    }

    #[test]
    fn blend_options_change_the_steps() {
        let a = rect_shape();
        let mut b = rect_shape();
        b.transform = Affine::translate((100.0, 0.0));
        b.fill = Fill::Solid(Color::rgb8(0, 0, 255));
        let blend = |options: BlendOptions, path: Option<BezPath>, rotation: f64| {
            let mut s = a.clone();
            s.fill = Fill::Solid(Color::rgb8(255, 0, 0));
            s.effects.push(Effect::Blend {
                end: Box::new(b.clone()),
                steps: 3,
                accel_objects: 0.5,
                accel_colors: 0.0,
                rotation,
                path,
                rotate_on_path: false,
                end_id: Some(b.id),
                options,
            });
            evaluate(&s).above
        };
        // A 100 mm path at 10 mm spacing: nine steps whatever the count.
        let mut line = BezPath::new();
        line.move_to((0.0, 50.0));
        line.line_to((100.0, 50.0));
        let spaced = BlendOptions {
            spacing: Some(10.0),
            ..Default::default()
        };
        assert_eq!(blend(spaced, Some(line.clone()), 0.0).len(), 9);
        // Without a path the spacing is ignored.
        assert_eq!(blend(spaced, None, 0.0).len(), 3);
        // Along the full path the first and last steps sit on its ends.
        let full = BlendOptions {
            full_path: true,
            ..Default::default()
        };
        let steps = blend(full, Some(line), 0.0);
        let first = steps[0].bounds().center();
        assert!((first.x - 0.0).abs() < 1.0, "{first:?}");
        // Loop: the steps' centres leave the straight line between the
        // objects.
        let looped = BlendOptions {
            loop_rotation: true,
            ..Default::default()
        };
        let plain = blend(BlendOptions::default(), None, 90.0);
        let turned = blend(looped, None, 90.0);
        let (p0, t0) = (plain[0].bounds().center(), turned[0].bounds().center());
        // (At acceleration 0.5 the first step turns by a sixth of the
        // way, so its centre moves about 3 mm.)
        assert!((p0.y - t0.y).abs() > 2.0, "{p0:?} {t0:?}");
        // Sizing: toward a bigger end object, acceleration 0.5 keeps the
        // steps smaller than the even blend makes them.
        let mut big = b.clone();
        big.transform = Affine::translate((100.0, 0.0)) * Affine::scale(3.0);
        let sized_width = |accel_sizing: bool| {
            let mut s = a.clone();
            s.effects.push(Effect::Blend {
                end: Box::new(big.clone()),
                steps: 3,
                accel_objects: 0.5,
                accel_colors: 0.0,
                rotation: 0.0,
                path: None,
                rotate_on_path: false,
                end_id: Some(big.id),
                options: BlendOptions {
                    accel_sizing,
                    ..Default::default()
                },
            });
            evaluate(&s).above[1].bounds().width()
        };
        assert!(sized_width(true) < sized_width(false) - 1e-6);
        // Around the wheel the middle colour is neither the straight mix
        // nor the same each way.
        let mid = |how: ColorBlend| {
            let o = BlendOptions {
                color_blend: how,
                ..Default::default()
            };
            blend(o, None, 0.0)[1].fill.preview_color().expect("colour")
        };
        let (d, cw, ccw) = (
            mid(ColorBlend::Direct),
            mid(ColorBlend::Clockwise),
            mid(ColorBlend::CounterClockwise),
        );
        assert_ne!(d.to_rgb8(), cw.to_rgb8());
        assert_ne!(cw.to_rgb8(), ccw.to_rgb8());
        // Red to blue clockwise (hue up) passes green; the other way magenta.
        let [r, g, _] = cw.to_rgb8();
        assert!(g > r, "{:?}", cw.to_rgb8());
        let [r, g, bl] = ccw.to_rgb8();
        assert!(r > g && bl > g, "{:?}", ccw.to_rgb8());
        // Old files read with the defaults.
        let old: Effect = serde_json::from_str(
            &serde_json::to_string(&Effect::Blend {
                end: Box::new(b.clone()),
                steps: 3,
                accel_objects: 0.0,
                accel_colors: 0.0,
                rotation: 0.0,
                path: None,
                rotate_on_path: false,
                end_id: None,
                options: BlendOptions::default(),
            })
            .expect("json"),
        )
        .expect("reads");
        assert!(matches!(old, Effect::Blend { options, .. } if options.is_default()));
        // Nodes given in each object's own coordinates (the end's moved by
        // its transform): paired from the next corner, the steps turn and
        // differ from the plain blend; the nodes are saved (#597).
        let local = a.local_path().bounding_box();
        let (p0, p1) = (
            Point::new(local.x0, local.y0),
            Point::new(local.x1, local.y0),
        );
        let p2 = Point::new(local.x1, local.y1);
        let mapped = BlendOptions {
            map_nodes: Some(BlendNodes {
                start: [p0, p1],
                end: [p1, p2],
            }),
            ..Default::default()
        };
        let plain = blend(BlendOptions::default(), None, 0.0);
        let turned = blend(mapped, None, 0.0);
        assert_eq!(plain.len(), turned.len());
        assert_ne!(plain[1].page_path(), turned[1].page_path());
        let json = serde_json::to_string(&mapped).expect("json");
        let back: BlendOptions = serde_json::from_str(&json).expect("reads");
        assert_eq!(back, mapped);
    }

    /// A bevelled extrusion draws the object among its faces and leaves
    /// the object itself empty: its box is the faces' box, not one that
    /// reaches the page's origin (#3227).
    #[test]
    fn bevelled_extrusion_box_stays_round_the_object() {
        let mut s = rect_shape();
        s.transform_by(Affine::translate((100.0, 80.0)));
        s.effects.push(Effect::Extrude {
            depth: Vec2::new(6.0, 6.0),
            vanishing: Some(Point::new(150.0, 150.0)),
            amount: 0.02,
            shade_from: None,
            shade_to: None,
            light_angle: 135.0,
            light_intensity: 50.0,
            bevel: 1.0,
            kind: Some(crate::extrude::ExtrudeKind::SmallBack),
            on_object: true,
            solid: None,
            space: Default::default(),
        });
        assert!(evaluate(&s).main.is_empty_path());
        let b = s.visual_bounds();
        assert!(
            b.x0 > 99.0 && b.y0 > 79.0 && b.x1 < 125.0 && b.y1 < 105.0,
            "{b:?}"
        );
    }

    #[test]
    fn extrude_parallel_has_back_and_sides() {
        let mut s = rect_shape();
        s.effects.push(Effect::Extrude {
            depth: Vec2::new(10.0, -10.0),
            vanishing: None,
            amount: 0.0,
            shade_from: None,
            shade_to: None,
            light_angle: 135.0,
            light_intensity: 50.0,
            bevel: 0.0,
            kind: None,
            on_object: false,
            solid: None,
            space: Default::default(),
        });
        // The far face, hidden behind the filled object, and the two
        // faces that show (toward the vector), each one outlined object
        // (#1281).
        let ev = evaluate(&s);
        assert_eq!(ev.below.len(), 3);
        assert!(ev.below.iter().all(|f| f.stroke.is_some()));
        // Without a fill it shows through, first, in a wire frame of all
        // four sides.
        s.fill = Fill::None;
        let ev = evaluate(&s);
        assert_eq!(ev.below.len(), 5);
        let back = ev.below[0].bounds();
        assert!((back.x0 - 10.0).abs() < 0.5);
    }
}

#[cfg(test)]
mod warped_extrusion_tests {
    use super::*;
    use crate::geometry::Rect;

    /// A letter-like outline: a "D" with its hole, curves on the right.
    fn letter() -> Shape {
        let mut p = BezPath::new();
        p.move_to((0.0, 0.0));
        p.line_to((0.0, 30.0));
        p.line_to((10.0, 30.0));
        p.curve_to((30.0, 30.0), (30.0, 0.0), (10.0, 0.0));
        p.close_path();
        p.move_to((4.0, 5.0));
        p.line_to((8.0, 5.0));
        p.curve_to((20.0, 5.0), (20.0, 25.0), (8.0, 25.0));
        p.line_to((4.0, 25.0));
        p.close_path();
        let mut s = Shape::new(
            crate::ShapeId(1),
            ShapeKind::Path {
                path: p,
                closed: true,
            },
        );
        s.fill = Fill::Solid(Color::rgb8(160, 20, 40));
        s
    }

    fn extrusion(depth: Vec2) -> Effect {
        Effect::Extrude {
            depth,
            vanishing: None,
            amount: 0.0,
            shade_from: None,
            shade_to: None,
            light_angle: 135.0,
            light_intensity: 50.0,
            bevel: 0.0,
            kind: None,
            on_object: false,
            solid: None,
            space: Default::default(),
        }
    }

    /// An envelope around `b` with its top side arched and its top-right
    /// corner pulled up and out.
    fn arch(b: Rect) -> Effect {
        let mut frame = crate::envelope::Frame::around(b);
        frame.move_node(5, Vec2::new(0.0, 12.0), crate::envelope::Mode::SingleArc);
        frame.move_node(4, Vec2::new(6.0, 5.0), crate::envelope::Mode::SingleArc);
        Effect::from_envelope(&frame, false, Default::default())
    }

    /// The lines of `p`, its curves flattened.
    fn lines(p: &BezPath) -> Vec<(Point, Point)> {
        let mut out = Vec::new();
        let (mut at, mut start) = (Point::ZERO, Point::ZERO);
        crate::geometry::flatten(p, 0.01, &mut |el| match el {
            PathEl::MoveTo(q) => {
                at = q;
                start = q;
            }
            PathEl::LineTo(q) => {
                out.push((at, q));
                at = q;
            }
            PathEl::ClosePath => {
                out.push((at, start));
                at = start;
            }
            _ => {}
        });
        out
    }

    /// How far the corners of `a` lie from the lines of `b`, at most.
    fn gap(a: &BezPath, b: &BezPath) -> f64 {
        let to = lines(b);
        let dist = |p: Point, (a, b): (Point, Point)| {
            let d = b - a;
            let len = d.hypot2();
            let t = if len > 0.0 {
                ((p - a).dot(d) / len).clamp(0.0, 1.0)
            } else {
                0.0
            };
            (p - (a + d * t)).hypot()
        };
        lines(a)
            .iter()
            .flat_map(|l| [l.0, l.1])
            .map(|p| to.iter().map(|l| dist(p, *l)).fold(f64::MAX, f64::min))
            .fold(0.0, f64::max)
    }

    /// How far the body of `ev` (a back parallel one, by `depth`) strays
    /// from the object's outline: the far face from the outline moved by
    /// the offset, and each side from the outline and that copy.
    fn body_gap(ev: &Evaluated, depth: Vec2) -> f64 {
        let front = ev.main.page_path();
        let far = Affine::translate(depth) * front.clone();
        let mut both = front;
        both.extend(far.elements().iter().copied());
        assert!(ev.below.len() > 1, "a far face and sides");
        let back = ev.below[0].page_path();
        let mut worst = gap(&back, &far).max(gap(&far, &back));
        for side in &ev.below[1..] {
            worst = worst.max(gap(&side.page_path(), &both));
        }
        worst
    }

    /// An envelope put on an extruded object bends the whole body
    /// (#2214): the sides and the far face are built from the bent
    /// outline, so they meet the object all along it. They were built
    /// from the outline as it was before the envelope and stayed behind,
    /// poking out around the bent object.
    #[test]
    fn an_envelope_on_an_extruded_object_bends_its_body() {
        let depth = Vec2::new(10.0, -10.0);
        let mut s = letter();
        let b = s.local_path().bounding_box();
        s.effects = vec![extrusion(depth), arch(b)];
        let ev = evaluate(&s);
        // The envelope did bend the object.
        let front = ev.main.page_path().bounding_box();
        assert!(front.y1 > 35.0, "{front:?}");
        let worst = body_gap(&ev, depth);
        assert!(worst < 0.1, "the body is {worst} mm off the bent outline");
        // Large letters too: the same outline six times the size.
        s.transform = Affine::scale(6.0);
        let worst = body_gap(&evaluate(&s), depth);
        assert!(worst < 0.1, "the large body is {worst} mm off");
        // It is the body of the object enveloped first, then extruded.
        s.transform = Affine::IDENTITY;
        let mut first = s.clone();
        first.effects.reverse();
        let first = evaluate(&first);
        assert_eq!(ev.main.page_path(), first.main.page_path());
        assert_eq!(ev.below.len(), first.below.len());
        for (a, b) in ev.below.iter().zip(&first.below) {
            assert_eq!(a.page_path(), b.page_path());
        }
    }

    /// Other warps after an extrusion bend its body too, and a front
    /// body's near face (#2214).
    #[test]
    fn every_warp_after_an_extrusion_reaches_its_body() {
        let depth = Vec2::new(8.0, 6.0);
        let mut s = letter();
        let b = s.local_path().bounding_box();
        let lean = Effect::Perspective {
            corners: [
                Point::new(b.x0, b.y0),
                Point::new(b.x1, b.y0 + 4.0),
                Point::new(b.x1, b.y1 - 4.0),
                Point::new(b.x0, b.y1),
            ],
        };
        let mut front = extrusion(depth);
        if let Effect::Extrude { kind, .. } = &mut front {
            *kind = Some(crate::extrude::ExtrudeKind::FrontParallel);
        }
        s.effects = vec![front, lean.clone(), arch(b)];
        let ev = evaluate(&s);
        let mut plain = s.clone();
        plain.effects = vec![lean, arch(b)];
        let bent = evaluate(&plain).main.page_path();
        assert_eq!(ev.main.page_path(), bent);
        // The near face, drawn last, is the bent outline moved.
        let near = ev.above.last().expect("a near face").page_path();
        let moved = Affine::translate(depth) * bent;
        let worst = gap(&near, &moved).max(gap(&moved, &near));
        assert!(worst < 0.1, "the near face is {worst} mm off");
    }

    /// A contour between the extrusion and the envelope is still drawn
    /// from the outline as it is at the contour's place, and the body
    /// follows the envelope all the same (#2214).
    #[test]
    fn a_contour_before_the_envelope_keeps_its_outline() {
        let depth = Vec2::new(10.0, -10.0);
        let mut s = letter();
        let b = s.local_path().bounding_box();
        let contour = Effect::Contour {
            steps: 1,
            offset: 2.0,
            outside: true,
            to_center: false,
            both: false,
            fill_to: Color::WHITE,
            outline_to: None,
            fill_end_to: None,
            color_blend: 0,
            corners: None,
            accel_objects: 0.0,
            accel_colors: 0.0,
        };
        s.effects = vec![extrusion(depth), contour.clone(), arch(b)];
        let ev = evaluate(&s);
        let mut alone = s.clone();
        alone.effects = vec![contour];
        let step = evaluate(&alone).below;
        assert_eq!(step.len(), 1);
        let drawn = ev.below.last().expect("the contour step");
        assert_eq!(drawn.page_path(), step[0].page_path());
        let mut body = ev.clone();
        body.below.pop();
        let worst = body_gap(&body, depth);
        assert!(worst < 0.1, "the body is {worst} mm off the bent outline");
    }

    /// The box an envelope after an extrusion maps is the outline's. For a
    /// body turned in space, whose object is drawn among its faces, it
    /// was empty, and the envelope did nothing.
    #[test]
    fn the_warp_box_is_the_outline_before_the_body() {
        let mut s = letter();
        let b = s.local_path().bounding_box();
        let mut turned = extrusion(Vec2::new(10.0, -10.0));
        if let Effect::Extrude { space, .. } = &mut turned {
            space.rotation = [0.0, 20.0, 0.0];
        }
        s.effects = vec![turned, arch(b)];
        assert_eq!(warp_box(&s, 1), b);
        assert_eq!(warp_box(&s, 0), b);
        let bent = warp_box(&s, 2);
        assert!(bent.y1 > b.y1 + 5.0, "{bent:?}");
        // A fresh frame around that box leaves the body as it was.
        let mut plain = s.clone();
        plain.effects.truncate(1);
        let mut framed = plain.clone();
        framed.effects.push(Effect::envelope_around(b, false));
        let (plain, framed) = (evaluate(&plain), evaluate(&framed));
        // The object is drawn among the faces of a turned body.
        let near = |ev: &Evaluated| ev.above.last().expect("the near face").page_path();
        assert!(same_outline(&near(&plain), &near(&framed), 0.05));
        let reach = |ev: &Evaluated| {
            ev.above
                .iter()
                .map(|f| f.page_path().bounding_box())
                .reduce(|a, c| a.union(c))
                .expect("faces")
        };
        let (a, c) = (reach(&plain), reach(&framed));
        assert!(
            (a.x0 - c.x0).abs() < 0.05
                && (a.y0 - c.y0).abs() < 0.05
                && (a.x1 - c.x1).abs() < 0.05
                && (a.y1 - c.y1).abs() < 0.05,
            "{a:?} {c:?}"
        );
    }

    /// A body turned in space is built from the bent outline as well: the
    /// envelope is not put on the turned face again (#2214).
    #[test]
    fn a_turned_body_is_built_from_the_bent_outline() {
        let depth = Vec2::new(10.0, -10.0);
        let mut s = letter();
        let b = s.local_path().bounding_box();
        let mut turned = extrusion(depth);
        if let Effect::Extrude { space, .. } = &mut turned {
            space.rotation = [0.0, 20.0, 0.0];
        }
        s.effects = vec![turned.clone(), arch(b)];
        let after = evaluate(&s);
        s.effects = vec![arch(b), turned];
        let before = evaluate(&s);
        assert_eq!(after.below.len(), before.below.len());
        assert!(!after.above.is_empty());
        assert_eq!(after.above.len(), before.above.len());
        for (a, b) in after.above.iter().zip(&before.above) {
            assert_eq!(a.page_path(), b.page_path());
        }
    }
}

#[cfg(test)]
mod symmetry_tests {
    use super::*;
    use crate::geometry::Rect;

    #[test]
    fn one_line_mirrors_the_object_across_it() {
        let mut s = Shape::new(
            crate::ShapeId(1),
            ShapeKind::Rect {
                rect: Rect::new(10.0, 0.0, 20.0, 10.0),
                radius: 0.0,
                corners: None,
            },
        );
        s.effects.push(Effect::Symmetry {
            center: Point::new(0.0, 0.0),
            angle: 90.0,
            lines: 1,
            rotate: false,
        });
        let ev = evaluate(&s);
        assert_eq!(ev.above.len(), 1);
        let b = ev.above[0].bounds();
        assert!(
            (b.x0 + 20.0).abs() < 1e-9 && (b.x1 + 10.0).abs() < 1e-9,
            "{b:?}"
        );
        assert!((b.y0).abs() < 1e-9 && (b.y1 - 10.0).abs() < 1e-9);

        // Two lines: three copies in the other quadrants.
        s.effects[0] = Effect::Symmetry {
            center: Point::new(0.0, 0.0),
            angle: 0.0,
            lines: 2,
            rotate: false,
        };
        let ev = evaluate(&s);
        assert_eq!(ev.above.len(), 3);
    }

    /// Rotational symmetry (#2999): three lines make three turned
    /// objects in all, a third of a turn apart, none a mirror image; the
    /// setting is left out of a saved symmetry that mirrors.
    #[test]
    fn rotational_symmetry_turns_the_copies() {
        let mut s = Shape::new(
            crate::ShapeId(1),
            ShapeKind::Rect {
                rect: Rect::new(10.0, -1.0, 20.0, 1.0),
                radius: 0.0,
                corners: None,
            },
        );
        s.effects.push(Effect::Symmetry {
            center: Point::new(0.0, 0.0),
            angle: 0.0,
            lines: 3,
            rotate: true,
        });
        let ev = evaluate(&s);
        assert_eq!(ev.above.len(), 2);
        for (copy, deg) in ev.above.iter().zip([120.0f64, 240.0]) {
            let [a, b, c, d, _, _] = copy.transform.as_coeffs();
            assert!(a * d - b * c > 0.0, "not mirrored");
            let mid = copy.bounds().center();
            let want = Point::new(15.0 * deg.to_radians().cos(), 15.0 * deg.to_radians().sin());
            assert!((mid - want).hypot() < 1e-6, "{mid:?} {want:?}");
        }
        // A point in a copy goes back to the object's slice by a turn.
        let back = symmetry_back_to_turned(
            Point::ZERO,
            0.0,
            3,
            true,
            Point::new(15.0, 0.0),
            Point::new(-7.5, 12.99),
        );
        let q = back * Point::new(-7.5, 12.99);
        assert!((q - Point::new(15.0, 0.0)).hypot() < 0.01, "{q:?}");
        let mirror = Effect::Symmetry {
            center: Point::ZERO,
            angle: 0.0,
            lines: 1,
            rotate: false,
        };
        assert!(!serde_json::to_string(&mirror).unwrap().contains("rotate"));
    }

    /// A mirrored copy's fountain fill and gradient transparency are
    /// mirror images of the object's (#2121): a fill running from red at
    /// the left to blue at the right, towards the mirror line, runs
    /// from red at the right to blue at the left on the copy at the other
    /// side of the line. The copy's gradients ran the object's way.
    #[test]
    fn mirrored_copies_mirror_their_gradients() {
        use crate::color::Color;
        use crate::style::Fill;
        let mut s = Shape::new(
            crate::ShapeId(1),
            ShapeKind::Rect {
                rect: Rect::new(10.0, 0.0, 30.0, 10.0),
                radius: 0.0,
                corners: None,
            },
        );
        let red = Color::rgb8(255, 0, 0);
        let blue = Color::rgb8(0, 0, 255);
        s.fill = Fill::linear(red, blue, 0.0);
        s.effects = vec![
            Effect::Symmetry {
                center: Point::new(40.0, 0.0),
                angle: 90.0,
                lines: 1,
                rotate: false,
            },
            Effect::Transparency {
                mask: Fill::linear(Color::WHITE, Color::BLACK, 0.0),
                merge: MergeMode::Normal,
                target: 2,
            },
        ];
        let ev = evaluate(&s);
        assert_eq!(ev.above.len(), 1);
        let copy = &ev.above[0];
        let b = copy.bounds();
        assert!(
            (b.x0 - 50.0).abs() < 1e-9 && (b.x1 - 70.0).abs() < 1e-9,
            "{b:?}"
        );
        // Where each gradient starts and ends on the page.
        let ends = |f: &Fill, on: Rect| match f {
            Fill::Fountain(f) => {
                let h = f.handles.unwrap_or_else(|| f.default_handles(on));
                (
                    crate::style::Fountain::rel_to_page(on, h.start),
                    crate::style::Fountain::rel_to_page(on, h.end),
                )
            }
            other => panic!("not a fountain: {other:?}"),
        };
        let (from, to) = ends(&ev.main.fill, ev.main.bounds());
        assert!(from.x < to.x, "the object's fill runs to the right");
        let (from, to) = ends(&copy.fill, b);
        assert!(
            (from.x - 70.0).abs() < 1e-6 && (to.x - 50.0).abs() < 1e-6,
            "the copy's fill runs to the left: {from:?} {to:?}"
        );
        let Effect::Transparency { mask, .. } = &copy.effects[0] else {
            panic!("the copy keeps its transparency");
        };
        let (from, to) = ends(mask, b);
        assert!(
            (from.x - 70.0).abs() < 1e-6 && (to.x - 50.0).abs() < 1e-6,
            "the copy's transparency runs to the left: {from:?} {to:?}"
        );
    }

    /// The mirrored copies carry the object's other effects (#2121): its
    /// transparency stays on each copy, and what a contour draws around
    /// the object is mirrored with it, whichever came first in the list.
    #[test]
    fn mirrored_copies_carry_the_other_effects() {
        let mut s = Shape::new(
            crate::ShapeId(1),
            ShapeKind::Rect {
                rect: Rect::new(10.0, 0.0, 20.0, 10.0),
                radius: 0.0,
                corners: None,
            },
        );
        s.effects = vec![
            Effect::Symmetry {
                center: Point::new(0.0, 0.0),
                angle: 90.0,
                lines: 1,
                rotate: false,
            },
            Effect::Transparency {
                mask: crate::style::Fill::Solid(crate::color::Color::rgb8(128, 128, 128)),
                merge: MergeMode::Normal,
                target: 2,
            },
            Effect::Contour {
                steps: 1,
                offset: 2.0,
                outside: true,
                to_center: false,
                both: false,
                fill_to: crate::color::Color::WHITE,
                outline_to: None,
                fill_end_to: None,
                color_blend: 0,
                corners: None,
                accel_objects: 0.0,
                accel_colors: 0.0,
            },
        ];
        let ev = evaluate(&s);
        assert_eq!(ev.above.len(), 1);
        let copy = &ev.above[0];
        assert_eq!(copy.effects.len(), 1);
        assert!(matches!(copy.effects[0], Effect::Transparency { .. }));
        let b = copy.bounds();
        assert!(
            (b.x0 + 20.0).abs() < 1e-9 && (b.x1 + 10.0).abs() < 1e-9,
            "{b:?}"
        );
        // The contour step and its mirror image.
        assert_eq!(ev.below.len(), 2);
        let c = ev.below[1].page_path().bounding_box();
        assert!(
            (c.x0 + 22.0).abs() < 0.1 && (c.x1 + 8.0).abs() < 0.1,
            "{c:?}"
        );
    }
}
