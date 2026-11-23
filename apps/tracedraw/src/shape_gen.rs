//! The Shape Generator (#3412, #3413): patterns and line art drawn from a
//! few settings as ordinary curves. Every generator is a pure function of
//! its settings and seed, so the panel's preview is the object made by
//! Create, and a generated group keeps its settings to be changed again.
//!
//! Generators:
//! - Dot grid: a grid of cells whose size and place change with their
//!   distance from the middle (lens or ripple, circular or elliptical
//!   falloff), cells round, square or diamond, turned and rounded.
//! - Truchet: square tiles of quarter arcs (one or more bands), filled
//!   quarter discs or diagonals, each turned at random by the seed; any
//!   layout repeats with no seam.
//! - Tessellation: triangles, hexagons, scales, chevrons, with a gap and
//!   a rotation.
//! - Line waves: horizontal lines bent by a smooth noise inside a bump
//!   (its width, sharpness, amplitude, spread and fade).
//! - Rings: closed rings between two radii, each wobbled by a noise that
//!   loops round the circle.
//! - Field lines: evenly spaced lines following the field of a few
//!   charged poles in a flow, with an optional swirl.
//! - Iso blocks (2.5D): stacks of cubes on an isometric grid from a
//!   height layout, three shaded faces drawn back to front.
//! - Cone template: the flat pattern of a cone or tapered cylinder
//!   (ring segment, rectangle or sector) with a glue tab and bleed, in
//!   one or more pieces; [`wrap_point`] bends a flat strip onto it.
//!
//! Model space: millimetres, y up. See docs/behavior/shape-generator.md.

use tracedraw_core::geometry::{Affine, BezPath, Point, Rect, Vec2};

/// The most curves one generation may make; past it the result is cut.
pub const MAX_ITEMS: usize = 40_000;

/// Which generator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GenKind {
    #[default]
    DotGrid,
    Truchet,
    Tessellation,
    LineWaves,
    Rings,
    FieldLines,
    IsoBlocks,
    ConeTemplate,
    Voronoi,
    Stipple,
    Halftone,
    Spirograph,
    Guilloche,
    Fractal,
    Tiling,
    Hatch,
    Contour,
    Maze,
    SpaceCurve,
    SliceGrid,
    Growth,
    LayerWaves,
    Blobs,
    SoftGradient,
}

impl GenKind {
    pub const ALL: [GenKind; 24] = [
        GenKind::DotGrid,
        GenKind::Truchet,
        GenKind::Tessellation,
        GenKind::LineWaves,
        GenKind::Rings,
        GenKind::FieldLines,
        GenKind::IsoBlocks,
        GenKind::ConeTemplate,
        GenKind::Voronoi,
        GenKind::Stipple,
        GenKind::Halftone,
        GenKind::Spirograph,
        GenKind::Guilloche,
        GenKind::Fractal,
        GenKind::Tiling,
        GenKind::Hatch,
        GenKind::Contour,
        GenKind::Maze,
        GenKind::SpaceCurve,
        GenKind::SliceGrid,
        GenKind::Growth,
        GenKind::LayerWaves,
        GenKind::Blobs,
        GenKind::SoftGradient,
    ];
    pub fn key(self) -> &'static str {
        match self {
            GenKind::DotGrid => "shapegen.kind_dot_grid",
            GenKind::Truchet => "shapegen.kind_truchet",
            GenKind::Tessellation => "shapegen.kind_tessellation",
            GenKind::LineWaves => "shapegen.kind_line_waves",
            GenKind::Rings => "shapegen.kind_rings",
            GenKind::FieldLines => "shapegen.kind_field_lines",
            GenKind::IsoBlocks => "shapegen.kind_iso_blocks",
            GenKind::ConeTemplate => "shapegen.kind_cone",
            GenKind::Voronoi => "shapegen.kind_voronoi",
            GenKind::Stipple => "shapegen.kind_stipple",
            GenKind::Halftone => "shapegen.kind_halftone",
            GenKind::Spirograph => "shapegen.kind_spirograph",
            GenKind::Guilloche => "shapegen.kind_guilloche",
            GenKind::Fractal => "shapegen.kind_fractal",
            GenKind::Tiling => "shapegen.kind_tiling",
            GenKind::Hatch => "shapegen.kind_hatch",
            GenKind::Contour => "shapegen.kind_contour",
            GenKind::Maze => "shapegen.kind_maze",
            GenKind::SpaceCurve => "shapegen.kind_space_curve",
            GenKind::SliceGrid => "shapegen.kind_slice",
            GenKind::Growth => "shapegen.kind_growth",
            GenKind::LayerWaves => "shapegen.kind_layer_waves",
            GenKind::Blobs => "shapegen.kind_blobs",
            GenKind::SoftGradient => "shapegen.kind_soft_gradient",
        }
    }
    /// Whether the generator works from the panel's picked selection (an
    /// outline to fill, or a picture's tones).
    pub fn uses_source(self) -> bool {
        matches!(
            self,
            GenKind::Voronoi
                | GenKind::Stipple
                | GenKind::Halftone
                | GenKind::Hatch
                | GenKind::Contour
                | GenKind::Maze
                | GenKind::SpaceCurve
                | GenKind::Growth
        )
    }
    /// Whether it reads the picked picture's tones.
    pub fn uses_tone(self) -> bool {
        matches!(
            self,
            GenKind::Stipple | GenKind::Halftone | GenKind::Hatch | GenKind::Contour
        )
    }
    /// Whether the generator draws inside the Width x Height box (the
    /// cone template takes its size from its own measures).
    pub fn uses_box(self) -> bool {
        !matches!(self, GenKind::ConeTemplate)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CellShape {
    #[default]
    Circle,
    Square,
    Diamond,
}

impl CellShape {
    pub const ALL: [CellShape; 3] = [CellShape::Circle, CellShape::Square, CellShape::Diamond];
    pub fn key(self) -> &'static str {
        match self {
            CellShape::Circle => "shapegen.cell_circle",
            CellShape::Square => "shapegen.cell_square",
            CellShape::Diamond => "shapegen.cell_diamond",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Warp {
    None,
    /// Cells pushed out from the middle (positive) or drawn in (negative).
    #[default]
    Lens,
    /// Rings of cells moved in and out like ripples.
    Ripple,
}

impl Warp {
    pub const ALL: [Warp; 3] = [Warp::None, Warp::Lens, Warp::Ripple];
    pub fn key(self) -> &'static str {
        match self {
            Warp::None => "shapegen.warp_none",
            Warp::Lens => "shapegen.warp_lens",
            Warp::Ripple => "shapegen.warp_ripple",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TruchetStyle {
    #[default]
    Arcs,
    Filled,
    Diagonals,
}

impl TruchetStyle {
    pub const ALL: [TruchetStyle; 3] = [
        TruchetStyle::Arcs,
        TruchetStyle::Filled,
        TruchetStyle::Diagonals,
    ];
    pub fn key(self) -> &'static str {
        match self {
            TruchetStyle::Arcs => "shapegen.truchet_arcs",
            TruchetStyle::Filled => "shapegen.truchet_filled",
            TruchetStyle::Diagonals => "shapegen.truchet_diagonals",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tess {
    Triangles,
    #[default]
    Hexagons,
    Scales,
    Chevrons,
}

impl Tess {
    pub const ALL: [Tess; 4] = [
        Tess::Triangles,
        Tess::Hexagons,
        Tess::Scales,
        Tess::Chevrons,
    ];
    pub fn key(self) -> &'static str {
        match self {
            Tess::Triangles => "shapegen.tess_triangles",
            Tess::Hexagons => "shapegen.tess_hexagons",
            Tess::Scales => "shapegen.tess_scales",
            Tess::Chevrons => "shapegen.tess_chevrons",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IsoLayout {
    Flat,
    #[default]
    Random,
    Pyramid,
    Wave,
}

impl IsoLayout {
    pub const ALL: [IsoLayout; 4] = [
        IsoLayout::Flat,
        IsoLayout::Random,
        IsoLayout::Pyramid,
        IsoLayout::Wave,
    ];
    pub fn key(self) -> &'static str {
        match self {
            IsoLayout::Flat => "shapegen.iso_flat",
            IsoLayout::Random => "shapegen.iso_random",
            IsoLayout::Pyramid => "shapegen.iso_pyramid",
            IsoLayout::Wave => "shapegen.iso_wave",
        }
    }
}

/// Every generator's settings; each uses its own fields. Kept between
/// sessions and in a generated group's object data.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ShapeGenSettings {
    pub kind: GenKind,
    /// The box the pattern fills, mm.
    pub width: f64,
    pub height: f64,
    pub seed: u32,
    /// Outline width of lines, mm; 0 uses the default outline.
    pub line_width: f64,
    /// Separate curves (grouped), or one curve per colour.
    pub combine: bool,

    // Dot grid.
    /// Cells across the longer side, 2 to 200.
    pub cells: u32,
    /// Cell size as a share of its pitch, 5 to 100 %.
    pub cell_size: f64,
    /// Edge cells' size against the middle ones, 0 to 400 %.
    pub scale_ratio: f64,
    pub cell_shape: CellShape,
    /// Square cells' corner roundness, 0 to 100 %.
    pub roundness: f64,
    /// Degrees.
    pub cell_rotation: f64,
    pub warp: Warp,
    /// -100 to 100.
    pub warp_strength: f64,
    /// Where the falloff ends, a share of the half diagonal, 10 to 200 %.
    pub falloff: f64,
    pub elliptical: bool,
    pub filled: bool,

    // Truchet and tessellation.
    /// Tile size, mm.
    pub tile: f64,
    pub truchet_style: TruchetStyle,
    /// Arc bands per corner, 1 to 12.
    pub bands: u32,
    pub tess: Tess,
    /// Gap between tiles, mm.
    pub gap: f64,
    /// Degrees.
    pub rotation: f64,

    // Line waves.
    pub lines: u32,
    /// The bump's width, a share of the box width, 5 to 200 %.
    pub wave_width: f64,
    /// mm.
    pub amplitude: f64,
    /// Noise waves across the box, 0.1 to 20.
    pub noise_x: f64,
    /// How much the noise changes from one line to the next, 0 to 1.
    pub noise_y: f64,
    /// How far lines fan apart inside the bump, mm.
    pub spread: f64,
    /// 0.5 to 12: higher makes the bump's sides steeper.
    pub sharpness: f64,
    /// 0 to 1: lines away from the middle move less.
    pub fade: f64,
    /// 0 to 100: fewer, rounder bends.
    pub softness: f64,

    // Rings.
    pub rings: u32,
    /// mm.
    pub min_radius: f64,
    pub max_radius: f64,
    /// mm.
    pub ring_amplitude: f64,
    /// Wobbles round a ring, 0.5 to 12.
    pub ring_detail: f64,
    /// 0 to 1: how much one ring's wobble differs from the next.
    pub ring_drift: f64,

    // Field lines.
    pub poles: u32,
    /// Spacing between lines, mm.
    pub line_spacing: f64,
    /// Degrees the field is turned round each pole.
    pub swirl: f64,
    /// Strength of the even flow across the box, 0 to 100.
    pub flow: f64,
    pub show_poles: bool,

    // Iso blocks.
    pub iso_cols: u32,
    pub iso_rows: u32,
    pub iso_height: u32,
    pub iso_layout: IsoLayout,
    /// Cube edge, mm.
    pub iso_size: f64,
    /// Space between cubes, 0 to 80 %.
    pub iso_gap: f64,

    // Cone template.
    /// mm.
    pub cone_top: f64,
    pub cone_bottom: f64,
    pub cone_height: f64,
    /// Glue tab width, mm; 0 for none.
    pub cone_tab: f64,
    /// Bleed, mm; 0 for none.
    pub cone_bleed: f64,
    /// Pieces the template is cut into, 1 to 8.
    pub cone_pieces: u32,
    /// How many times the wrapped strip goes round, 1 to 24.
    pub cone_repeat: u32,

    // Picked selection (Voronoi, stippling, halftone).
    /// Where Create puts the result: the middle of the selection picked,
    /// page space.
    pub source_center: Option<[f64; 2]>,
    /// The picked selection's outline, in the box (its bottom left at the
    /// origin): Voronoi fills it, marks stay inside it.
    pub boundary: Option<BezPath>,
    /// The picked selection drawn to a PNG covering the box, base64.
    pub tone: Option<String>,
    /// Light places take the marks instead of dark ones.
    pub invert: bool,

    // Voronoi.
    pub vor_mode: crate::shape_gen_cells::PointMode,
    /// Distance between sites, mm.
    pub vor_spacing: f64,
    /// Lattice sites moved at random, 0 to 100 % of the spacing.
    pub vor_jitter: f64,
    /// Relaxation steps applied to generated sites, 0 to 50.
    pub vor_relax: u32,
    pub vor_gradient: crate::shape_gen_cells::Gradient,
    /// 0 to 100 %.
    pub vor_grad_strength: f64,
    /// Direction of a linear gradient, degrees.
    pub vor_grad_angle: f64,
    /// Small cells at the edge (or the far end) instead of the middle.
    pub vor_grad_edge: bool,
    /// Space cut between cells, mm.
    pub vor_inset: f64,
    /// Corner radius, mm.
    pub vor_round: f64,
    pub vor_cells: bool,
    pub vor_show_sites: bool,
    pub vor_delaunay: bool,
    /// Sites placed in the panel, 0 to 1 across the box; when any, they
    /// replace the generated ones.
    pub vor_points: Vec<[f64; 2]>,

    // Stippling.
    pub st_dots: u32,
    /// Relaxation steps, 0 to 100.
    pub st_iter: u32,
    /// Dot diameters, mm.
    pub st_min: f64,
    pub st_max: f64,
    /// Dots sized by tone, or all at the largest size.
    pub st_by_tone: bool,

    // Halftone.
    /// Screen cell, mm.
    pub ht_cell: f64,
    /// Screen angle, degrees.
    pub ht_angle: f64,
    pub ht_shape: crate::shape_gen_cells::HtShape,
    /// Mark size at no ink and at full ink, % of the cell.
    pub ht_min: f64,
    pub ht_max: f64,
    /// Four screens, one per ink.
    pub ht_cmyk: bool,

    // Spirograph.
    pub spiro_type: crate::shape_gen_curves::SpiroType,
    /// Fixed and rolling circle radii and the pen's distance from the
    /// rolling circle's middle, mm (the drawing is then fitted).
    pub spiro_fixed: f64,
    pub spiro_rolling: f64,
    pub spiro_pen: f64,
    /// Lissajous frequencies, 1 to 30, and phase, degrees.
    pub liss_a: u32,
    pub liss_b: u32,
    pub liss_phase: f64,
    /// Rose petals n / d, 1 to 30 each.
    pub rose_n: u32,
    pub rose_d: u32,
    /// Mandala symmetry, 3 to 64, and rings, 1 to 12.
    pub mandala_fold: u32,
    pub mandala_rings: u32,

    // Guilloche.
    pub guil_lines: u32,
    /// mm.
    pub guil_inner: f64,
    pub guil_outer: f64,
    /// Waves round a ring, 0 to 200, and their height, mm.
    pub guil_waves: u32,
    pub guil_amp: f64,
    pub guil_waves2: u32,
    pub guil_amp2: f64,
    /// Degrees each ring's waves are turned from the last.
    pub guil_twist: f64,

    // Fractal.
    pub frac_type: crate::shape_gen_curves::FractalType,
    /// Levels, 0 to 14.
    pub frac_depth: u32,
    /// Turn, degrees.
    pub frac_angle: f64,
    /// Tree: each branch's length against its parent's, 0.3 to 0.95.
    pub frac_ratio: f64,
    /// Tree: random change of angle and length, 0 to 100 %.
    pub frac_jitter: f64,
    /// Custom L-system: axiom and rules (`F=F+F;X=...`).
    pub frac_axiom: String,
    pub frac_rules: String,

    // Tiling.
    pub tile_type: crate::shape_gen_curves::TilingType,
    /// Edge of a rhomb or cell, mm.
    pub tile_size: f64,
    /// Star patterns: angle between a ray and its edge, degrees.
    pub tile_angle: f64,
    /// Star patterns: grid rotation, degrees.
    pub tile_rotation: f64,
    /// Penrose: thin and thick rhombs filled light and dark.
    pub tile_fill: bool,

    // Hatching.
    pub hatch_mode: crate::shape_gen_fill::HatchMode,
    /// Distance between lines, mm; angle and the crossing lines' turn
    /// from it, degrees.
    pub hatch_spacing: f64,
    pub hatch_angle: f64,
    pub hatch_cross: f64,
    /// By tone: layers of lines, 1 to 8.
    pub hatch_layers: u32,

    // Contour lines.
    pub contour_mode: crate::shape_gen_fill::ContourMode,
    /// Offset rings: distance between rings, mm, and the most rings.
    pub contour_spacing: f64,
    pub contour_rings: u32,
    /// Tone lines: how many levels, 1 to 40.
    pub contour_levels: u32,

    // Maze.
    /// Cell size, mm.
    pub maze_cell: f64,
    pub maze_style: crate::shape_gen_fill::MazeStyle,
    /// An entrance at the bottom and an exit at the top.
    pub maze_openings: bool,

    // Space-filling curve.
    pub curve_type: crate::shape_gen_fill::SpaceCurve,
    pub curve_order: u32,
    pub curve_round: bool,

    // Slice grid.
    pub slice_layout: crate::shape_gen_slice::SliceLayout,
    /// Columns and rows, 1 to 200.
    pub slice_cols: u32,
    pub slice_rows: u32,
    pub slice_pattern: crate::shape_gen_slice::SlicePattern,
    /// The largest move sideways and up or down, mm.
    pub slice_dx: f64,
    pub slice_dy: f64,
    /// Waves across the strips, for the wave pattern.
    pub slice_waves: f64,
    /// Space between pieces, mm.
    pub slice_gap: f64,
    /// Leave the selection in place.
    pub slice_keep: bool,

    // Differential growth.
    pub growth_start: crate::shape_gen_growth::GrowthStart,
    /// Circles for the several-circles start, 1 to 50.
    pub growth_seed_count: u32,
    /// The starting circle's or line's size, % of the box.
    pub growth_size: f64,
    /// Edge length between points, mm.
    pub growth_spacing: f64,
    /// How far points push each other, in edge lengths.
    pub growth_repel: f64,
    /// Pull toward the neighbours' middle, 0 to 1.
    pub growth_align: f64,
    /// Random splits per step.
    pub growth_rate: u32,
    /// Steps grown; the live run sets it.
    pub growth_steps: u32,
    /// The point limit.
    pub growth_max_nodes: u32,

    // Deform and split (any generator).
    /// Pins: anchor (share of Width and Height) and pull (share of the
    /// longer side).
    pub warp_pins: Vec<[f64; 4]>,
    /// How far a pin's pull reaches, % of the longer side.
    pub warp_reach: f64,
    pub split_mode: crate::shape_gen_warp::SplitMode,
    /// The picked outline, centred on the origin, mm.
    pub split_shape: Option<BezPath>,
    /// Where the picked outline's middle is on the page.
    pub split_center: Option<[f64; 2]>,
    /// The lines inside: colour and width (0 keeps the line width), mm.
    pub split_color: [u8; 3],
    pub split_width: f64,

    // Backgrounds (layered waves, blobs, soft gradient).
    /// The palette, 1 to 6 colours.
    pub bg_colors: Vec<[u8; 3]>,
    /// Layered waves: bands, how high the waves go (% of Height), waves
    /// across, sine waves added per band, and the side they rise from.
    pub bg_layers: u32,
    pub bg_amplitude: f64,
    pub bg_frequency: f64,
    pub bg_complexity: u32,
    pub bg_side: crate::shape_gen_bg::WaveSide,
    /// Blobs: how many, points round each, how much they vary (%) and
    /// their size (% of the box's shorter side).
    pub bg_blob_count: u32,
    pub bg_blob_points: u32,
    pub bg_randomness: f64,
    pub bg_blob_size: f64,
    /// Soft gradient: mesh cells per side and how far inner nodes wander
    /// (% of a cell).
    pub bg_mesh_size: u32,
    pub bg_jitter: f64,
}

impl Default for ShapeGenSettings {
    fn default() -> Self {
        ShapeGenSettings {
            kind: GenKind::DotGrid,
            width: 100.0,
            height: 100.0,
            seed: 1,
            line_width: 0.0,
            combine: false,
            cells: 25,
            cell_size: 60.0,
            scale_ratio: 30.0,
            cell_shape: CellShape::Circle,
            roundness: 30.0,
            cell_rotation: 0.0,
            warp: Warp::Lens,
            warp_strength: 50.0,
            falloff: 100.0,
            elliptical: false,
            filled: false,
            tile: 10.0,
            truchet_style: TruchetStyle::Arcs,
            bands: 3,
            tess: Tess::Hexagons,
            gap: 1.0,
            rotation: 0.0,
            lines: 60,
            wave_width: 60.0,
            amplitude: 12.0,
            noise_x: 3.0,
            noise_y: 0.05,
            spread: 25.0,
            sharpness: 3.0,
            fade: 0.5,
            softness: 50.0,
            rings: 30,
            min_radius: 25.0,
            max_radius: 48.0,
            ring_amplitude: 5.0,
            ring_detail: 3.0,
            ring_drift: 0.2,
            poles: 5,
            line_spacing: 2.0,
            swirl: 0.0,
            flow: 30.0,
            show_poles: true,
            iso_cols: 6,
            iso_rows: 6,
            iso_height: 4,
            iso_layout: IsoLayout::Random,
            iso_size: 8.0,
            iso_gap: 0.0,
            cone_top: 70.0,
            cone_bottom: 55.0,
            cone_height: 95.0,
            cone_tab: 8.0,
            cone_bleed: 0.0,
            cone_pieces: 1,
            cone_repeat: 1,
            source_center: None,
            boundary: None,
            tone: None,
            invert: false,
            vor_mode: crate::shape_gen_cells::PointMode::Random,
            vor_spacing: 8.0,
            vor_jitter: 50.0,
            vor_relax: 3,
            vor_gradient: crate::shape_gen_cells::Gradient::None,
            vor_grad_strength: 70.0,
            vor_grad_angle: 0.0,
            vor_grad_edge: false,
            vor_inset: 0.0,
            vor_round: 0.0,
            vor_cells: true,
            vor_show_sites: false,
            vor_delaunay: false,
            vor_points: Vec::new(),
            st_dots: 2000,
            st_iter: 20,
            st_min: 0.3,
            st_max: 1.2,
            st_by_tone: true,
            ht_cell: 3.0,
            ht_angle: 45.0,
            ht_shape: crate::shape_gen_cells::HtShape::Circle,
            ht_min: 0.0,
            ht_max: 110.0,
            ht_cmyk: false,
            spiro_type: crate::shape_gen_curves::SpiroType::Hypotrochoid,
            spiro_fixed: 40.0,
            spiro_rolling: 15.0,
            spiro_pen: 12.0,
            liss_a: 3,
            liss_b: 2,
            liss_phase: 90.0,
            rose_n: 5,
            rose_d: 1,
            mandala_fold: 12,
            mandala_rings: 4,
            guil_lines: 40,
            guil_inner: 20.0,
            guil_outer: 46.0,
            guil_waves: 18,
            guil_amp: 3.0,
            guil_waves2: 5,
            guil_amp2: 2.0,
            guil_twist: 4.0,
            frac_type: crate::shape_gen_curves::FractalType::Tree,
            frac_depth: 6,
            frac_angle: 25.0,
            frac_ratio: 0.72,
            frac_jitter: 0.0,
            frac_axiom: "F".to_string(),
            frac_rules: "F=F[+F]F[-F]F".to_string(),
            tile_type: crate::shape_gen_curves::TilingType::Penrose,
            tile_size: 8.0,
            tile_angle: 67.5,
            tile_rotation: 0.0,
            tile_fill: false,
            hatch_mode: crate::shape_gen_fill::HatchMode::Parallel,
            hatch_spacing: 2.0,
            hatch_angle: 45.0,
            hatch_cross: 90.0,
            hatch_layers: 4,
            contour_mode: crate::shape_gen_fill::ContourMode::Offset,
            contour_spacing: 3.0,
            contour_rings: 100,
            contour_levels: 8,
            maze_cell: 5.0,
            maze_style: crate::shape_gen_fill::MazeStyle::Walls,
            maze_openings: true,
            curve_type: crate::shape_gen_fill::SpaceCurve::Hilbert,
            curve_order: 5,
            curve_round: false,
            slice_layout: crate::shape_gen_slice::SliceLayout::Rows,
            slice_cols: 8,
            slice_rows: 12,
            slice_pattern: crate::shape_gen_slice::SlicePattern::Random,
            slice_dx: 6.0,
            slice_dy: 0.0,
            slice_waves: 1.0,
            slice_gap: 0.0,
            slice_keep: false,
            growth_start: crate::shape_gen_growth::GrowthStart::Circle,
            growth_seed_count: 5,
            growth_size: 20.0,
            growth_spacing: 1.5,
            growth_repel: 2.5,
            growth_align: 0.4,
            growth_rate: 3,
            growth_steps: 200,
            growth_max_nodes: 3000,
            warp_pins: Vec::new(),
            warp_reach: 20.0,
            split_mode: crate::shape_gen_warp::SplitMode::Off,
            split_shape: None,
            split_center: None,
            split_color: [255, 205, 220],
            split_width: 0.0,
            bg_colors: crate::shape_gen_bg::default_palette(),
            bg_layers: 5,
            bg_amplitude: 10.0,
            bg_frequency: 1.0,
            bg_complexity: 3,
            bg_side: crate::shape_gen_bg::WaveSide::Bottom,
            bg_blob_count: 1,
            bg_blob_points: 7,
            bg_randomness: 35.0,
            bg_blob_size: 80.0,
            bg_mesh_size: 3,
            bg_jitter: 40.0,
        }
    }
}

/// How a generated curve is painted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Paint {
    /// Outline only.
    Line,
    /// Filled, no outline.
    Solid,
    /// A shaded cube face (0 top, 1 left, 2 right): filled grey with a
    /// thin outline.
    Face(u8),
    /// A halftone screen's ink (0 cyan, 1 magenta, 2 yellow, 3 black):
    /// filled with it, each ink grouped on its own.
    Ink(u8),
    /// Lines inside the split shape: the second colour and width.
    Accent,
    /// Filled with the colour this share (0 to 255) along
    /// [`Generated::palette`], no outline.
    Tone(u8),
    /// Filled with [`Generated::mesh`], no outline.
    Mesh,
}

#[derive(Debug, Clone)]
pub struct GenItem {
    pub path: BezPath,
    pub closed: bool,
    pub paint: Paint,
}

/// What a generation made, in model space with its box's bottom left at
/// the origin.
#[derive(Debug, Clone, Default)]
pub struct Generated {
    pub items: Vec<GenItem>,
    /// True when [`MAX_ITEMS`] cut the result.
    pub cut: bool,
    /// The box the drawing belongs to when it is not its curves' bounds
    /// (cells, marks): the preview and placement use it.
    pub frame: Option<Rect>,
    /// Voronoi sites, for editing them in the panel.
    pub sites: Vec<Point>,
    /// The palette [`Paint::Tone`] picks from.
    pub palette: Vec<[u8; 3]>,
    /// The mesh fill of [`Paint::Mesh`] curves, in the drawing's space.
    pub mesh: Option<tracedraw_core::Mesh>,
}

impl Generated {
    pub(crate) fn push(&mut self, path: BezPath, closed: bool, paint: Paint) {
        if self.items.len() >= MAX_ITEMS {
            self.cut = true;
            return;
        }
        self.items.push(GenItem {
            path,
            closed,
            paint,
        });
    }
    pub(crate) fn full(&self) -> bool {
        self.items.len() >= MAX_ITEMS
    }
    /// The bounding box of every curve.
    pub fn bounds(&self) -> Option<Rect> {
        use tracedraw_core::geometry::Shape as _;
        self.items
            .iter()
            .filter(|i| !i.path.elements().is_empty())
            .map(|i| i.path.bounding_box())
            .reduce(|a, b| a.union(b))
    }
}

/// A finite number clamped to a range, or the fallback.
fn clampf(v: f64, lo: f64, hi: f64, fallback: f64) -> f64 {
    if v.is_finite() {
        v.clamp(lo, hi)
    } else {
        fallback
    }
}

impl ShapeGenSettings {
    /// The settings with every value inside its range (values read from
    /// a file or typed may be anything).
    pub fn sanitized(&self) -> ShapeGenSettings {
        let d = ShapeGenSettings::default();
        let mut s = self.clone();
        s.width = clampf(s.width, 1.0, 5000.0, d.width);
        s.height = clampf(s.height, 1.0, 5000.0, d.height);
        s.line_width = clampf(s.line_width, 0.0, 50.0, 0.0);
        s.cells = s.cells.clamp(2, 200);
        s.cell_size = clampf(s.cell_size, 5.0, 100.0, d.cell_size);
        s.scale_ratio = clampf(s.scale_ratio, 0.0, 400.0, d.scale_ratio);
        s.roundness = clampf(s.roundness, 0.0, 100.0, d.roundness);
        s.cell_rotation = clampf(s.cell_rotation, -360.0, 360.0, 0.0);
        s.warp_strength = clampf(s.warp_strength, -100.0, 100.0, d.warp_strength);
        s.falloff = clampf(s.falloff, 10.0, 200.0, d.falloff);
        s.tile = clampf(s.tile, 0.5, 1000.0, d.tile);
        s.bands = s.bands.clamp(1, 12);
        s.gap = clampf(s.gap, 0.0, s.tile * 0.9, 0.0);
        s.rotation = clampf(s.rotation, -360.0, 360.0, 0.0);
        s.lines = s.lines.clamp(2, 1000);
        s.wave_width = clampf(s.wave_width, 5.0, 200.0, d.wave_width);
        s.amplitude = clampf(s.amplitude, 0.0, 1000.0, d.amplitude);
        s.noise_x = clampf(s.noise_x, 0.1, 20.0, d.noise_x);
        s.noise_y = clampf(s.noise_y, 0.0, 1.0, d.noise_y);
        s.spread = clampf(s.spread, -1000.0, 1000.0, d.spread);
        s.sharpness = clampf(s.sharpness, 0.5, 12.0, d.sharpness);
        s.fade = clampf(s.fade, 0.0, 1.0, d.fade);
        s.softness = clampf(s.softness, 0.0, 100.0, d.softness);
        s.rings = s.rings.clamp(1, 500);
        s.min_radius = clampf(s.min_radius, 0.1, 2500.0, d.min_radius);
        s.max_radius = clampf(s.max_radius, 0.1, 2500.0, d.max_radius);
        s.ring_amplitude = clampf(s.ring_amplitude, 0.0, 1000.0, d.ring_amplitude);
        s.ring_detail = clampf(s.ring_detail, 0.5, 12.0, d.ring_detail);
        s.ring_drift = clampf(s.ring_drift, 0.0, 1.0, d.ring_drift);
        s.poles = s.poles.clamp(1, 20);
        s.line_spacing = clampf(
            s.line_spacing,
            (s.width.max(s.height) / 400.0).max(0.2),
            500.0,
            d.line_spacing,
        );
        s.swirl = clampf(s.swirl, -90.0, 90.0, 0.0);
        s.flow = clampf(s.flow, 0.0, 100.0, d.flow);
        s.iso_cols = s.iso_cols.clamp(1, 60);
        s.iso_rows = s.iso_rows.clamp(1, 60);
        s.iso_height = s.iso_height.clamp(1, 30);
        s.iso_size = clampf(s.iso_size, 0.5, 500.0, d.iso_size);
        s.iso_gap = clampf(s.iso_gap, 0.0, 80.0, 0.0);
        s.cone_top = clampf(s.cone_top, 0.0, 5000.0, d.cone_top);
        s.cone_bottom = clampf(s.cone_bottom, 0.0, 5000.0, d.cone_bottom);
        if s.cone_top <= 0.0 && s.cone_bottom <= 0.0 {
            s.cone_bottom = d.cone_bottom;
        }
        s.cone_height = clampf(s.cone_height, 0.5, 5000.0, d.cone_height);
        s.cone_tab = clampf(s.cone_tab, 0.0, 500.0, 0.0);
        s.cone_bleed = clampf(s.cone_bleed, 0.0, 100.0, 0.0);
        s.cone_pieces = s.cone_pieces.clamp(1, 8);
        s.cone_repeat = s.cone_repeat.clamp(1, 24);
        s.source_center = s
            .source_center
            .filter(|c| c[0].is_finite() && c[1].is_finite());
        s.boundary = s.boundary.take().filter(|b| {
            use tracedraw_core::geometry::Shape as _;
            !b.elements().is_empty() && {
                let r = b.bounding_box();
                r.x0.is_finite() && r.y0.is_finite() && r.x1.is_finite() && r.y1.is_finite()
            }
        });
        s.tone = s
            .tone
            .take()
            .filter(|t| !t.is_empty() && t.len() < 8_000_000);
        let side = s.width.max(s.height);
        s.vor_spacing = clampf(
            s.vor_spacing,
            (side / 150.0).max(0.1),
            1000.0,
            d.vor_spacing,
        )
        .max((side / 150.0).max(0.1));
        s.vor_jitter = clampf(s.vor_jitter, 0.0, 100.0, d.vor_jitter);
        s.vor_relax = s.vor_relax.min(50);
        s.vor_grad_strength = clampf(s.vor_grad_strength, 0.0, 100.0, d.vor_grad_strength);
        s.vor_grad_angle = clampf(s.vor_grad_angle, -360.0, 360.0, 0.0);
        s.vor_inset = clampf(s.vor_inset, 0.0, 100.0, 0.0);
        s.vor_round = clampf(s.vor_round, 0.0, 100.0, 0.0);
        s.vor_points
            .retain(|p| p[0].is_finite() && p[1].is_finite());
        for p in &mut s.vor_points {
            p[0] = p[0].clamp(0.0, 1.0);
            p[1] = p[1].clamp(0.0, 1.0);
        }
        s.vor_points.truncate(crate::shape_gen_cells::MAX_SITES);
        s.st_dots = s
            .st_dots
            .clamp(10, crate::shape_gen_cells::MAX_SITES as u32);
        s.st_iter = s.st_iter.min(100);
        s.st_min = clampf(s.st_min, 0.0, 50.0, d.st_min);
        s.st_max = clampf(s.st_max, 0.01, 50.0, d.st_max).max(s.st_min);
        s.ht_cell = clampf(s.ht_cell, (side / 300.0).max(0.1), 500.0, d.ht_cell)
            .max((side / 300.0).max(0.1));
        s.ht_angle = clampf(s.ht_angle, -360.0, 360.0, d.ht_angle);
        s.ht_min = clampf(s.ht_min, 0.0, 200.0, 0.0);
        s.ht_max = clampf(s.ht_max, 0.0, 200.0, d.ht_max).max(s.ht_min);
        s.spiro_fixed = clampf(s.spiro_fixed, 1.0, 1000.0, d.spiro_fixed);
        s.spiro_rolling = clampf(s.spiro_rolling, 0.5, 1000.0, d.spiro_rolling);
        s.spiro_pen = clampf(s.spiro_pen, 0.0, 1000.0, d.spiro_pen);
        s.liss_a = s.liss_a.clamp(1, 30);
        s.liss_b = s.liss_b.clamp(1, 30);
        s.liss_phase = clampf(s.liss_phase, -360.0, 360.0, d.liss_phase);
        s.rose_n = s.rose_n.clamp(1, 30);
        s.rose_d = s.rose_d.clamp(1, 30);
        s.mandala_fold = s.mandala_fold.clamp(3, 64);
        s.mandala_rings = s.mandala_rings.clamp(1, 12);
        s.guil_lines = s.guil_lines.clamp(1, 500);
        s.guil_inner = clampf(s.guil_inner, 0.0, 2500.0, d.guil_inner);
        s.guil_outer = clampf(s.guil_outer, 0.0, 2500.0, d.guil_outer);
        s.guil_waves = s.guil_waves.min(200);
        s.guil_amp = clampf(s.guil_amp, 0.0, 500.0, d.guil_amp);
        s.guil_waves2 = s.guil_waves2.min(200);
        s.guil_amp2 = clampf(s.guil_amp2, 0.0, 500.0, d.guil_amp2);
        s.guil_twist = clampf(s.guil_twist, -90.0, 90.0, d.guil_twist);
        s.frac_depth = s.frac_depth.min(14);
        s.frac_angle = clampf(s.frac_angle, -180.0, 180.0, d.frac_angle);
        s.frac_ratio = clampf(s.frac_ratio, 0.3, 0.95, d.frac_ratio);
        s.frac_jitter = clampf(s.frac_jitter, 0.0, 100.0, 0.0);
        if s.frac_type == crate::shape_gen_curves::FractalType::Tree {
            // Two branches per level: 2^15 segments at most.
            s.frac_depth = s.frac_depth.min(14);
        }
        s.frac_axiom = s.frac_axiom.chars().take(200).collect();
        s.frac_rules = s.frac_rules.chars().take(1000).collect();
        s.tile_size = clampf(s.tile_size, (side / 200.0).max(0.2), 1000.0, d.tile_size)
            .max((side / 200.0).max(0.2));
        s.tile_angle = clampf(s.tile_angle, 20.0, 85.0, d.tile_angle);
        s.tile_rotation = clampf(s.tile_rotation, -360.0, 360.0, 0.0);
        let fine = (side / 1000.0).max(0.05);
        s.hatch_spacing = clampf(s.hatch_spacing, fine, 1000.0, d.hatch_spacing).max(fine);
        s.hatch_angle = clampf(s.hatch_angle, -360.0, 360.0, d.hatch_angle);
        s.hatch_cross = clampf(s.hatch_cross, -180.0, 180.0, d.hatch_cross);
        s.hatch_layers = s.hatch_layers.clamp(1, 8);
        let ring = (side / 500.0).max(0.05);
        s.contour_spacing = clampf(s.contour_spacing, ring, 1000.0, d.contour_spacing).max(ring);
        s.contour_rings = s.contour_rings.clamp(1, 500);
        s.contour_levels = s.contour_levels.clamp(1, 40);
        let cell = (side / 300.0).max(0.1);
        s.maze_cell = clampf(s.maze_cell, cell, 1000.0, d.maze_cell).max(cell);
        s.curve_order = s.curve_order.clamp(1, s.curve_type.max_order());
        s.slice_cols = s.slice_cols.clamp(1, 200);
        s.slice_rows = s.slice_rows.clamp(1, 200);
        s.slice_dx = clampf(s.slice_dx, -1000.0, 1000.0, d.slice_dx);
        s.slice_dy = clampf(s.slice_dy, -1000.0, 1000.0, 0.0);
        s.slice_waves = clampf(s.slice_waves, 0.1, 50.0, d.slice_waves);
        s.slice_gap = clampf(s.slice_gap, 0.0, 1000.0, 0.0);
        s.growth_seed_count = s.growth_seed_count.clamp(1, 50);
        s.growth_size = clampf(s.growth_size, 5.0, 100.0, d.growth_size);
        let step = (side / 400.0).max(0.05);
        s.growth_spacing = clampf(s.growth_spacing, step, 50.0, d.growth_spacing).max(step);
        s.growth_repel = clampf(s.growth_repel, 1.0, 5.0, d.growth_repel);
        s.growth_align = clampf(s.growth_align, 0.0, 1.0, d.growth_align);
        s.growth_rate = s.growth_rate.min(100);
        s.growth_steps = s.growth_steps.min(crate::shape_gen_growth::MAX_STEPS);
        s.growth_max_nodes = s.growth_max_nodes.clamp(100, 20_000);
        s.warp_pins.retain(|p| p.iter().all(|v| v.is_finite()));
        s.warp_pins.truncate(crate::shape_gen_warp::MAX_PINS);
        for p in &mut s.warp_pins {
            p[0] = p[0].clamp(-1.0, 2.0);
            p[1] = p[1].clamp(-1.0, 2.0);
            p[2] = p[2].clamp(-2.0, 2.0);
            p[3] = p[3].clamp(-2.0, 2.0);
        }
        s.warp_reach = clampf(s.warp_reach, 1.0, 100.0, d.warp_reach);
        s.split_width = clampf(s.split_width, 0.0, 50.0, 0.0);
        s.bg_colors.truncate(crate::shape_gen_bg::MAX_COLORS);
        if s.bg_colors.is_empty() {
            s.bg_colors = d.bg_colors.clone();
        }
        s.bg_layers = s.bg_layers.clamp(1, 40);
        s.bg_amplitude = clampf(s.bg_amplitude, 0.0, 100.0, d.bg_amplitude);
        s.bg_frequency = clampf(s.bg_frequency, 0.1, 20.0, d.bg_frequency);
        s.bg_complexity = s.bg_complexity.clamp(1, 6);
        s.bg_blob_count = s.bg_blob_count.clamp(1, 200);
        s.bg_blob_points = s.bg_blob_points.clamp(3, 24);
        s.bg_randomness = clampf(s.bg_randomness, 0.0, 90.0, d.bg_randomness);
        s.bg_blob_size = clampf(s.bg_blob_size, 1.0, 300.0, d.bg_blob_size);
        s.bg_mesh_size = s.bg_mesh_size.clamp(1, 8);
        s.bg_jitter = clampf(s.bg_jitter, 0.0, 100.0, d.bg_jitter);
        s.split_shape = s
            .split_shape
            .take()
            .filter(|p| !p.elements().is_empty() && p.elements().len() < 2_000_000);
        s
    }
}

/// Draw what the settings describe.
pub fn generate(settings: &ShapeGenSettings) -> Generated {
    let s = settings.sanitized();
    let mut out = Generated::default();
    match s.kind {
        GenKind::DotGrid => dot_grid(&s, &mut out),
        GenKind::Truchet => truchet(&s, &mut out),
        GenKind::Tessellation => tessellation(&s, &mut out),
        GenKind::LineWaves => line_waves(&s, &mut out),
        GenKind::Rings => rings(&s, &mut out),
        GenKind::FieldLines => field_lines(&s, &mut out),
        GenKind::IsoBlocks => iso_blocks(&s, &mut out),
        GenKind::ConeTemplate => cone_template(&s, &mut out),
        GenKind::Voronoi => crate::shape_gen_cells::voronoi(&s, &mut out),
        GenKind::Stipple => crate::shape_gen_cells::stipple(&s, &mut out),
        GenKind::Halftone => crate::shape_gen_cells::halftone(&s, &mut out),
        GenKind::Spirograph => crate::shape_gen_curves::spirograph(&s, &mut out),
        GenKind::Guilloche => crate::shape_gen_curves::guilloche(&s, &mut out),
        GenKind::Fractal => crate::shape_gen_curves::fractal(&s, &mut out),
        GenKind::Tiling => crate::shape_gen_curves::tiling(&s, &mut out),
        GenKind::Hatch => crate::shape_gen_fill::hatch(&s, &mut out),
        GenKind::Contour => crate::shape_gen_fill::contour(&s, &mut out),
        GenKind::Maze => crate::shape_gen_fill::maze(&s, &mut out),
        GenKind::SpaceCurve => crate::shape_gen_fill::space_curve(&s, &mut out),
        GenKind::SliceGrid => crate::shape_gen_slice::slice_grid(&s, &mut out),
        GenKind::Growth => crate::shape_gen_growth::growth(&s, &mut out),
        GenKind::LayerWaves => crate::shape_gen_bg::waves(&s, &mut out),
        GenKind::Blobs => crate::shape_gen_bg::blobs(&s, &mut out),
        GenKind::SoftGradient => crate::shape_gen_bg::soft_gradient(&s, &mut out),
    }
    crate::shape_gen_warp::post(&s, &mut out);
    out
}

// ----- random numbers and noise -----------------------------------------

/// A small, fast generator (SplitMix64): the same seed always deals the
/// same numbers on every platform.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u32, salt: u64) -> Rng {
        Rng((seed as u64) << 32 ^ salt ^ 0x9E37_79B9_7F4A_7C15)
    }
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    /// 0 to 1.
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
    pub fn below(&mut self, n: u32) -> u32 {
        if n == 0 {
            0
        } else {
            (self.next_u64() % n as u64) as u32
        }
    }
}

fn hash3(seed: u32, x: i64, y: i64, z: i64) -> u64 {
    let mut h = (seed as u64).wrapping_mul(0x2545_F491_4F6C_DD1D);
    for v in [x, y, z] {
        h ^= (v as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        h = (h ^ (h >> 29)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    }
    h ^ (h >> 32)
}

/// A gradient at a lattice corner: a unit-ish vector from the hash.
fn grad(seed: u32, x: i64, y: i64, z: i64) -> [f64; 3] {
    let h = hash3(seed, x, y, z);
    let a = (h & 0xFFFF) as f64 / 65535.0 * std::f64::consts::TAU;
    let b = ((h >> 16) & 0xFFFF) as f64 / 65535.0 * 2.0 - 1.0;
    let r = (1.0 - b * b).max(0.0).sqrt();
    [r * a.cos(), r * a.sin(), b]
}

fn fade(t: f64) -> f64 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

pub(crate) fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// Smooth gradient noise in three dimensions, about -1 to 1.
pub fn noise3(seed: u32, x: f64, y: f64, z: f64) -> f64 {
    if !(x.is_finite() && y.is_finite() && z.is_finite()) {
        return 0.0;
    }
    let (xf, yf, zf) = (x.floor(), y.floor(), z.floor());
    let (xi, yi, zi) = (xf as i64, yf as i64, zf as i64);
    let (dx, dy, dz) = (x - xf, y - yf, z - zf);
    let corner = |i: i64, j: i64, k: i64| {
        let g = grad(seed, xi + i, yi + j, zi + k);
        g[0] * (dx - i as f64) + g[1] * (dy - j as f64) + g[2] * (dz - k as f64)
    };
    let (u, v, w) = (fade(dx), fade(dy), fade(dz));
    let x00 = lerp(corner(0, 0, 0), corner(1, 0, 0), u);
    let x10 = lerp(corner(0, 1, 0), corner(1, 1, 0), u);
    let x01 = lerp(corner(0, 0, 1), corner(1, 0, 1), u);
    let x11 = lerp(corner(0, 1, 1), corner(1, 1, 1), u);
    let y0 = lerp(x00, x10, v);
    let y1 = lerp(x01, x11, v);
    (lerp(y0, y1, w) * 1.4).clamp(-1.0, 1.0)
}

/// Two octaves of [`noise3`].
fn fbm(seed: u32, x: f64, y: f64, z: f64) -> f64 {
    (noise3(seed, x, y, z) + 0.5 * noise3(seed ^ 0x5bd1, x * 2.0, y * 2.0, z * 2.0)) / 1.5
}

// ----- curve helpers -----------------------------------------------------

/// A smooth curve through `pts` (Catmull-Rom as cubic Beziers); `closed`
/// joins the last point to the first smoothly.
pub fn smooth_through(pts: &[Point], closed: bool) -> BezPath {
    let mut path = BezPath::new();
    let n = pts.len();
    if n == 0 {
        return path;
    }
    path.move_to(pts[0]);
    if n == 1 {
        return path;
    }
    if n == 2 {
        path.line_to(pts[1]);
        if closed {
            path.close_path();
        }
        return path;
    }
    let get = |i: isize| -> Point {
        if closed {
            pts[i.rem_euclid(n as isize) as usize]
        } else {
            pts[i.clamp(0, n as isize - 1) as usize]
        }
    };
    let segs = if closed { n } else { n - 1 };
    for i in 0..segs as isize {
        let (p0, p1, p2, p3) = (get(i - 1), get(i), get(i + 1), get(i + 2));
        let c1 = p1 + (p2 - p0) / 6.0;
        let c2 = p2 - (p3 - p1) / 6.0;
        path.curve_to(c1, c2, p2);
    }
    if closed {
        path.close_path();
    }
    path
}

pub(crate) fn polygon(pts: &[Point]) -> BezPath {
    let mut path = BezPath::new();
    if let Some((first, rest)) = pts.split_first() {
        path.move_to(*first);
        for p in rest {
            path.line_to(*p);
        }
        path.close_path();
    }
    path
}

/// A circular arc from angle `a0` to `a1` (radians) as cubic Beziers of
/// at most a quarter turn each; `start` adds the move to its first point.
fn arc_into(path: &mut BezPath, c: Point, r: f64, a0: f64, a1: f64, start: bool) {
    let sweep = a1 - a0;
    let n = (sweep.abs() / std::f64::consts::FRAC_PI_2).ceil().max(1.0) as usize;
    let step = sweep / n as f64;
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    let at = |a: f64| Point::new(c.x + r * a.cos(), c.y + r * a.sin());
    if start {
        path.move_to(at(a0));
    } else {
        path.line_to(at(a0));
    }
    for i in 0..n {
        let (s, e) = (a0 + step * i as f64, a0 + step * (i + 1) as f64);
        let (ps, pe) = (at(s), at(e));
        let ts = Vec2::new(-s.sin(), s.cos()) * (r * k);
        let te = Vec2::new(-e.sin(), e.cos()) * (r * k);
        path.curve_to(ps + ts, pe - te, pe);
    }
}

// ----- dot grid -----------------------------------------------------------

fn dot_grid(s: &ShapeGenSettings, out: &mut Generated) {
    let (w, h) = (s.width, s.height);
    let pitch = w.max(h) / s.cells as f64;
    let cols = ((w / pitch).round() as i64).max(1);
    let rows = ((h / pitch).round() as i64).max(1);
    let c = Point::new(w / 2.0, h / 2.0);
    let half_diag = (w * w + h * h).sqrt() / 2.0;
    let reach = half_diag * s.falloff / 100.0;
    let base = pitch * s.cell_size / 100.0;
    let paint = if s.filled { Paint::Solid } else { Paint::Line };
    let rot = Affine::rotate(s.cell_rotation.to_radians());
    for j in 0..rows {
        for i in 0..cols {
            if out.full() {
                out.cut = true;
                return;
            }
            let p = Point::new(
                (i as f64 + 0.5) * w / cols as f64,
                (j as f64 + 0.5) * h / rows as f64,
            );
            let mut d = p - c;
            // Distance on the falloff's scale: circular, or the box's oval.
            let dn = if s.elliptical {
                Vec2::new(d.x / (w / 2.0) * half_diag, d.y / (h / 2.0) * half_diag).hypot() / reach
            } else {
                d.hypot() / reach
            };
            let t = smoothstep(dn.min(1.0));
            // Warp of the place.
            let k = s.warp_strength / 100.0;
            match s.warp {
                Warp::None => {}
                Warp::Lens => {
                    if dn < 1.0 && dn > 0.0 {
                        let e = (1.0 - 0.75 * k).max(0.1);
                        let dn2 = dn.powf(e);
                        d *= dn2 / dn;
                    }
                }
                Warp::Ripple => {
                    let r = d.hypot();
                    if r > 1e-9 {
                        let waves = 4.0;
                        let shift =
                            k * pitch * 0.8 * (r / reach * waves * std::f64::consts::TAU).sin();
                        d *= (r + shift).max(0.0) / r;
                    }
                }
            }
            let q = c + d;
            let size = base * lerp(1.0, s.scale_ratio / 100.0, t);
            if size < 1e-3 {
                continue;
            }
            let half = size / 2.0;
            let path = match s.cell_shape {
                CellShape::Circle => tracedraw_core::geometry::ellipse_path(Rect::new(
                    q.x - half,
                    q.y - half,
                    q.x + half,
                    q.y + half,
                )),
                CellShape::Square | CellShape::Diamond => {
                    let r = Rect::new(-half, -half, half, half);
                    let mut p = tracedraw_core::geometry::rect_path(r, half * s.roundness / 100.0);
                    let turn = if s.cell_shape == CellShape::Diamond {
                        Affine::rotate(std::f64::consts::FRAC_PI_4)
                    } else {
                        Affine::IDENTITY
                    };
                    p.apply_affine(Affine::translate(q.to_vec2()) * rot * turn);
                    p
                }
            };
            out.push(path, true, paint);
        }
    }
}

fn smoothstep(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

// ----- truchet ------------------------------------------------------------

/// The arc radii of a tile of side `t` with `bands` bands: symmetric about
/// half the side so that every edge meets its neighbour's, and below the
/// radius where arcs from opposite corners would cross.
pub fn truchet_radii(t: f64, bands: u32) -> Vec<f64> {
    let n = bands.max(1);
    if n == 1 {
        return vec![t / 2.0];
    }
    let span = 0.4 * t;
    let step = span / (n - 1) as f64;
    (0..n)
        .map(|k| t / 2.0 - span / 2.0 + step * k as f64)
        .collect()
}

fn truchet(s: &ShapeGenSettings, out: &mut Generated) {
    let t = s.tile;
    let cols = ((s.width / t).round() as i64).max(1);
    let rows = ((s.height / t).round() as i64).max(1);
    let mut rng = Rng::new(s.seed, 0x7a);
    let radii = truchet_radii(t, s.bands);
    use std::f64::consts::{FRAC_PI_2, PI};
    for j in 0..rows {
        for i in 0..cols {
            if out.full() {
                out.cut = true;
                return;
            }
            let o = Point::new(i as f64 * t, j as f64 * t);
            let flip = rng.below(2) == 1;
            // The two opposite corners the arcs turn round, and the
            // angle each quarter starts at.
            let (c1, a1, c2, a2) = if flip {
                (
                    o + Vec2::new(t, 0.0),
                    FRAC_PI_2,
                    o + Vec2::new(0.0, t),
                    -FRAC_PI_2,
                )
            } else {
                (o, 0.0, o + Vec2::new(t, t), PI)
            };
            match s.truchet_style {
                TruchetStyle::Arcs => {
                    for &r in &radii {
                        for (c, a) in [(c1, a1), (c2, a2)] {
                            let mut p = BezPath::new();
                            arc_into(&mut p, c, r, a, a + FRAC_PI_2, true);
                            out.push(p, false, Paint::Line);
                        }
                    }
                }
                TruchetStyle::Filled => {
                    for (c, a) in [(c1, a1), (c2, a2)] {
                        let mut p = BezPath::new();
                        p.move_to(c);
                        arc_into(&mut p, c, t / 2.0, a, a + FRAC_PI_2, false);
                        p.close_path();
                        out.push(p, true, Paint::Solid);
                    }
                }
                TruchetStyle::Diagonals => {
                    let mut p = BezPath::new();
                    if flip {
                        p.move_to(o);
                        p.line_to(o + Vec2::new(t, t));
                    } else {
                        p.move_to(o + Vec2::new(t, 0.0));
                        p.line_to(o + Vec2::new(0.0, t));
                    }
                    out.push(p, false, Paint::Line);
                }
            }
        }
    }
}

// ----- tessellation -------------------------------------------------------

/// `pts` drawn in towards their middle by `gap / 2` (an approximation
/// that keeps convex tiles' sides parallel).
fn shrink(pts: &[Point], gap: f64) -> Vec<Point> {
    if gap <= 0.0 || pts.is_empty() {
        return pts.to_vec();
    }
    let n = pts.len() as f64;
    let c = pts.iter().fold(Vec2::ZERO, |a, p| a + p.to_vec2()) / n;
    // Inradius estimate: the least distance from the middle to a side.
    let mut inr = f64::MAX;
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
        let ab = b - a;
        let len = ab.hypot();
        if len > 1e-12 {
            let dist = ((c.to_point() - a).cross(ab) / len).abs();
            inr = inr.min(dist);
        }
    }
    if !inr.is_finite() || inr <= 1e-9 {
        return pts.to_vec();
    }
    let f = ((inr - gap / 2.0) / inr).max(0.0);
    pts.iter()
        .map(|p| c.to_point() + (*p - c.to_point()) * f)
        .collect()
}

fn tessellation(s: &ShapeGenSettings, out: &mut Generated) {
    let (w, h) = (s.width, s.height);
    let t = s.tile;
    let c = Point::new(w / 2.0, h / 2.0);
    let turn = Affine::rotate_about(s.rotation.to_radians(), c);
    // Tiles are laid over a box large enough to cover the area once
    // turned, and kept when their middle falls inside it.
    let reach = (w * w + h * h).sqrt() / 2.0 + 2.0 * t;
    let inside = |p: Point| p.x >= -1e-9 && p.x <= w + 1e-9 && p.y >= -1e-9 && p.y <= h + 1e-9;
    let emit = |pts: Vec<Point>, out: &mut Generated| {
        let pts: Vec<Point> = pts.into_iter().map(|p| turn * p).collect();
        let n = pts.len().max(1) as f64;
        let mid = pts.iter().fold(Vec2::ZERO, |a, p| a + p.to_vec2()) / n;
        if inside(mid.to_point()) {
            out.push(polygon(&shrink(&pts, s.gap)), true, Paint::Solid);
        }
    };
    match s.tess {
        Tess::Triangles => {
            let th = t * 3f64.sqrt() / 2.0;
            let nj = (reach / th).ceil() as i64;
            let ni = (reach / t).ceil() as i64 + 1;
            for j in -nj..nj {
                for i in -ni..ni {
                    if out.full() {
                        out.cut = true;
                        return;
                    }
                    let y0 = c.y + j as f64 * th;
                    let shift = if j.rem_euclid(2) == 0 { 0.0 } else { t / 2.0 };
                    let x0 = c.x + i as f64 * t + shift;
                    let up = vec![
                        Point::new(x0, y0),
                        Point::new(x0 + t, y0),
                        Point::new(x0 + t / 2.0, y0 + th),
                    ];
                    let down = vec![
                        Point::new(x0 + t / 2.0, y0 + th),
                        Point::new(x0 + t, y0),
                        Point::new(x0 + 1.5 * t, y0 + th),
                    ];
                    emit(up, out);
                    emit(down, out);
                }
            }
        }
        Tess::Hexagons => {
            // Pointy-top hexagons of circumradius t / 2.
            let r = t / 2.0;
            let dx = 3f64.sqrt() * r;
            let dy = 1.5 * r;
            let nj = (reach / dy).ceil() as i64;
            let ni = (reach / dx).ceil() as i64 + 1;
            for j in -nj..=nj {
                for i in -ni..=ni {
                    if out.full() {
                        out.cut = true;
                        return;
                    }
                    let shift = if j.rem_euclid(2) == 0 { 0.0 } else { dx / 2.0 };
                    let m = Point::new(c.x + i as f64 * dx + shift, c.y + j as f64 * dy);
                    let pts = (0..6)
                        .map(|k| {
                            let a = std::f64::consts::FRAC_PI_6
                                + k as f64 * std::f64::consts::FRAC_PI_3;
                            Point::new(m.x + r * a.cos(), m.y + r * a.sin())
                        })
                        .collect();
                    emit(pts, out);
                }
            }
        }
        Tess::Chevrons => {
            // Bands of V shapes, each a hexagon with a notch.
            let bh = t / 2.0;
            let nj = (reach / bh).ceil() as i64;
            let ni = (reach / t).ceil() as i64 + 1;
            for j in -nj..nj {
                for i in -ni..ni {
                    if out.full() {
                        out.cut = true;
                        return;
                    }
                    let x0 = c.x + i as f64 * t;
                    let y0 = c.y + j as f64 * bh;
                    let pts = vec![
                        Point::new(x0, y0),
                        Point::new(x0 + t / 2.0, y0 + t / 2.0),
                        Point::new(x0 + t, y0),
                        Point::new(x0 + t, y0 + bh),
                        Point::new(x0 + t / 2.0, y0 + t / 2.0 + bh),
                        Point::new(x0, y0 + bh),
                    ];
                    let pts = shrink_band(&pts, s.gap);
                    let pts: Vec<Point> = pts.into_iter().map(|p| turn * p).collect();
                    let mid = turn * Point::new(x0 + t / 2.0, y0 + t / 4.0 + bh / 2.0);
                    if inside(mid) {
                        out.push(polygon(&pts), true, Paint::Solid);
                    }
                }
            }
        }
        Tess::Scales => {
            // Overlapping half discs in rows offset by half a tile, drawn
            // from the top row down so each row covers the one above.
            let r = t / 2.0;
            let dy = r;
            let nj = (reach / dy).ceil() as i64;
            let ni = (reach / t).ceil() as i64 + 1;
            for j in (-nj..=nj).rev() {
                for i in -ni..=ni {
                    if out.full() {
                        out.cut = true;
                        return;
                    }
                    let shift = if j.rem_euclid(2) == 0 { 0.0 } else { r };
                    let m = Point::new(c.x + i as f64 * t + shift, c.y + j as f64 * dy);
                    let rr = (r - s.gap / 2.0).max(r * 0.05);
                    // The scale: a half disc hanging under its top line,
                    // with straight sides up to the line.
                    let mut p = BezPath::new();
                    p.move_to(turn * Point::new(m.x - rr, m.y + r));
                    let mut arc = BezPath::new();
                    arc_into(
                        &mut arc,
                        m,
                        rr,
                        std::f64::consts::PI,
                        std::f64::consts::TAU,
                        true,
                    );
                    for el in arc.elements().iter().skip(1) {
                        p.push(transform_el(*el, turn));
                    }
                    p.line_to(turn * Point::new(m.x + rr, m.y + r));
                    p.close_path();
                    if inside(turn * m) {
                        out.push(p, true, Paint::Solid);
                    }
                }
            }
        }
    }
}

fn transform_el(
    el: tracedraw_core::geometry::PathEl,
    m: Affine,
) -> tracedraw_core::geometry::PathEl {
    use tracedraw_core::geometry::PathEl;
    match el {
        PathEl::MoveTo(p) => PathEl::MoveTo(m * p),
        PathEl::LineTo(p) => PathEl::LineTo(m * p),
        PathEl::QuadTo(a, b) => PathEl::QuadTo(m * a, m * b),
        PathEl::CurveTo(a, b, c) => PathEl::CurveTo(m * a, m * b, m * c),
        PathEl::ClosePath => PathEl::ClosePath,
    }
}

/// A chevron band thinned by the gap: its top edge moved down and its
/// bottom edge up by half the gap each.
fn shrink_band(pts: &[Point], gap: f64) -> Vec<Point> {
    if gap <= 0.0 || pts.len() != 6 {
        return pts.to_vec();
    }
    let g = gap / 2.0;
    let mut v = pts.to_vec();
    for p in v.iter_mut().take(3) {
        p.y += g;
    }
    for p in v.iter_mut().skip(3) {
        p.y -= g;
    }
    v
}

// ----- line waves ---------------------------------------------------------

fn line_waves(s: &ShapeGenSettings, out: &mut Generated) {
    let (w, h) = (s.width, s.height);
    let n = s.lines.max(2);
    let samples = (lerp(240.0, 40.0, s.softness / 100.0) as usize).max(8);
    let half_bump = (s.wave_width / 100.0 * w / 2.0).max(1e-6);
    for i in 0..n {
        if out.full() {
            out.cut = true;
            return;
        }
        let t = i as f64 / (n - 1) as f64;
        let base = t * h;
        let fade_k = 1.0 - s.fade * (2.0 * t - 1.0).abs();
        let pts: Vec<Point> = (0..=samples)
            .map(|k| {
                let u = k as f64 / samples as f64;
                let x = u * w;
                let e = (-((x - w / 2.0).abs() / half_bump).powf(s.sharpness)).exp();
                let nz = fbm(s.seed, u * s.noise_x, i as f64 * s.noise_y, 0.37);
                let y = base + (t - 0.5) * s.spread * e + s.amplitude * e * fade_k * nz;
                Point::new(x, y)
            })
            .collect();
        out.push(smooth_through(&pts, false), false, Paint::Line);
    }
}

// ----- rings --------------------------------------------------------------

fn rings(s: &ShapeGenSettings, out: &mut Generated) {
    let c = Point::new(s.width / 2.0, s.height / 2.0);
    let (r0, r1) = if s.min_radius <= s.max_radius {
        (s.min_radius, s.max_radius)
    } else {
        (s.max_radius, s.min_radius)
    };
    let n = s.rings.max(1);
    let samples = ((s.ring_detail * 24.0) as usize).clamp(24, 360);
    for k in 0..n {
        if out.full() {
            out.cut = true;
            return;
        }
        let t = if n == 1 {
            0.0
        } else {
            k as f64 / (n - 1) as f64
        };
        let rb = lerp(r0, r1, t);
        let z = k as f64 * s.ring_drift;
        let pts: Vec<Point> = (0..samples)
            .map(|j| {
                let a = j as f64 / samples as f64 * std::f64::consts::TAU;
                let f = s.ring_detail / 2.0;
                let nz = fbm(s.seed, a.cos() * f + 10.0, a.sin() * f + 10.0, z);
                let r = (rb + s.ring_amplitude * nz).max(0.0);
                Point::new(c.x + r * a.cos(), c.y + r * a.sin())
            })
            .collect();
        out.push(smooth_through(&pts, true), true, Paint::Line);
    }
}

// ----- field lines --------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct Pole {
    at: Point,
    charge: f64,
}

fn field_at(p: Point, poles: &[Pole], flow: Vec2, swirl: f64) -> Vec2 {
    let mut v = flow;
    let (sn, cs) = swirl.sin_cos();
    for q in poles {
        let d = p - q.at;
        let r2 = d.hypot2().max(1e-6);
        let e = d * (q.charge / r2);
        // Turned round the pole by the swirl.
        v += Vec2::new(e.x * cs - e.y * sn, e.x * sn + e.y * cs);
    }
    v
}

fn field_lines(s: &ShapeGenSettings, out: &mut Generated) {
    let (w, h) = (s.width, s.height);
    let mut rng = Rng::new(s.seed, 0xf1e1d);
    let scale = w.max(h);
    let poles: Vec<Pole> = (0..s.poles)
        .map(|_| Pole {
            at: Point::new(w * (0.15 + 0.7 * rng.unit()), h * (0.15 + 0.7 * rng.unit())),
            charge: if rng.unit() < 0.5 { -1.0 } else { 1.0 } * scale * (0.3 + rng.unit()),
        })
        .collect();
    let fa = rng.unit() * std::f64::consts::TAU;
    let flow = Vec2::new(fa.cos(), fa.sin()) * (s.flow / 100.0 * 4.0);
    let swirl = s.swirl.to_radians();
    let sep = s.line_spacing;
    let test = sep * 0.5;
    let step = sep * 0.25;
    let near_pole = sep * 0.6;
    // Occupancy grid: points of the lines drawn so far, by cell.
    let cell = test.max(1e-3);
    let gw = (w / cell).ceil() as i64 + 1;
    let gh = (h / cell).ceil() as i64 + 1;
    let mut grid: Vec<Vec<Point>> = vec![Vec::new(); (gw * gh).max(1) as usize];
    let key = |p: Point| -> Option<usize> {
        let (x, y) = ((p.x / cell).floor() as i64, (p.y / cell).floor() as i64);
        (x >= 0 && y >= 0 && x < gw && y < gh).then_some((y * gw + x) as usize)
    };
    let free = |grid: &Vec<Vec<Point>>, p: Point, d: f64| -> bool {
        let (x, y) = ((p.x / cell).floor() as i64, (p.y / cell).floor() as i64);
        let reach = (d / cell).ceil() as i64;
        for yy in (y - reach)..=(y + reach) {
            for xx in (x - reach)..=(x + reach) {
                if xx < 0 || yy < 0 || xx >= gw || yy >= gh {
                    continue;
                }
                if let Some(c) = grid.get((yy * gw + xx) as usize) {
                    if c.iter().any(|q| (*q - p).hypot() < d) {
                        return false;
                    }
                }
            }
        }
        true
    };
    let inside = |p: Point| p.x >= 0.0 && p.y >= 0.0 && p.x <= w && p.y <= h;
    let max_steps = ((w + h) * 4.0 / step) as usize;
    let trace = |grid: &Vec<Vec<Point>>, seed: Point, dir: f64| -> Vec<Point> {
        let mut pts = Vec::new();
        let mut p = seed;
        for _ in 0..max_steps {
            let v1 = field_at(p, &poles, flow, swirl);
            let l1 = v1.hypot();
            if l1 < 1e-9 {
                break;
            }
            let mid = p + v1 / l1 * (step * 0.5 * dir);
            let v2 = field_at(mid, &poles, flow, swirl);
            let l2 = v2.hypot();
            if l2 < 1e-9 {
                break;
            }
            let next = p + v2 / l2 * (step * dir);
            if !inside(next)
                || !free(grid, next, test)
                || poles.iter().any(|q| (next - q.at).hypot() < near_pole)
            {
                break;
            }
            pts.push(next);
            p = next;
        }
        pts
    };
    // Seeds: a jittered grid at the line spacing.
    let mut seeds: Vec<Point> = Vec::new();
    let nx = (w / sep).ceil() as i64;
    let ny = (h / sep).ceil() as i64;
    for j in 0..ny {
        for i in 0..nx {
            seeds.push(Point::new(
                (i as f64 + 0.2 + 0.6 * rng.unit()) * sep,
                (j as f64 + 0.2 + 0.6 * rng.unit()) * sep,
            ));
        }
    }
    // Shuffle so lines start all over the box, not in rows.
    for k in (1..seeds.len()).rev() {
        let j = rng.below((k + 1) as u32) as usize;
        seeds.swap(k, j);
    }
    for seed in seeds {
        if out.full() {
            out.cut = true;
            return;
        }
        if !inside(seed) || !free(&grid, seed, sep) {
            continue;
        }
        let mut back = trace(&grid, seed, -1.0);
        back.reverse();
        back.push(seed);
        let fwd = trace(&grid, seed, 1.0);
        back.extend(fwd);
        if back.len() < 6 {
            continue;
        }
        // Thinned to every other point: still smooth, half the nodes.
        let thin: Vec<Point> = back
            .iter()
            .enumerate()
            .filter(|(k, _)| k % 2 == 0 || *k == back.len() - 1)
            .map(|(_, p)| *p)
            .collect();
        for p in &back {
            if let Some(k) = key(*p) {
                if let Some(c) = grid.get_mut(k) {
                    c.push(*p);
                }
            }
        }
        out.push(smooth_through(&thin, false), false, Paint::Line);
    }
    if s.show_poles {
        for q in &poles {
            let r = sep * 0.5;
            out.push(
                tracedraw_core::geometry::ellipse_path(Rect::new(
                    q.at.x - r,
                    q.at.y - r,
                    q.at.x + r,
                    q.at.y + r,
                )),
                true,
                Paint::Line,
            );
        }
    }
}

// ----- iso blocks ---------------------------------------------------------

/// The height of each column of the iso grid, in cubes.
pub fn iso_heights(s: &ShapeGenSettings) -> Vec<Vec<u32>> {
    let (cols, rows, hmax) = (s.iso_cols.max(1), s.iso_rows.max(1), s.iso_height.max(1));
    let mut rng = Rng::new(s.seed, 0x150);
    let (ci, cj) = ((cols - 1) as f64 / 2.0, (rows - 1) as f64 / 2.0);
    (0..rows)
        .map(|j| {
            (0..cols)
                .map(|i| match s.iso_layout {
                    IsoLayout::Flat => 1,
                    IsoLayout::Random => rng.below(hmax + 1),
                    IsoLayout::Pyramid => {
                        let d = (i as f64 - ci).abs().max((j as f64 - cj).abs());
                        (hmax as f64 - d).round().max(0.0) as u32
                    }
                    IsoLayout::Wave => {
                        let v = ((i as f64 / cols as f64 * std::f64::consts::TAU).sin()
                            + (j as f64 / rows as f64 * std::f64::consts::TAU).cos())
                            / 4.0
                            + 0.5;
                        ((v * hmax as f64).round() as u32).clamp(1, hmax)
                    }
                })
                .collect()
        })
        .collect()
}

fn iso_blocks(s: &ShapeGenSettings, out: &mut Generated) {
    let a = s.iso_size;
    let (cx, cy) = (a * 3f64.sqrt() / 2.0, a / 2.0);
    let heights = iso_heights(s);
    let rows = heights.len() as i64;
    let cols = heights.first().map_or(0, |r| r.len()) as i64;
    let k = 1.0 - s.iso_gap / 100.0;
    // Screen place of grid point (i, j, z): i runs down to the right, j
    // down to the left, z up.
    let at = |i: f64, j: f64, z: f64| Point::new((i - j) * cx, -(i + j) * cy + z * a);
    // Back to front: the far corner (small i + j) first, then upward.
    for diag in 0..(rows + cols - 1) {
        for j in 0..rows {
            let i = diag - j;
            if i < 0 || i >= cols {
                continue;
            }
            let hgt = heights
                .get(j as usize)
                .and_then(|r| r.get(i as usize))
                .copied()
                .unwrap_or(0);
            for z in 0..hgt {
                if out.items.len() + 3 > MAX_ITEMS {
                    out.cut = true;
                    return;
                }
                // The cube in its cell, shrunk about its middle by the gap.
                let (i0, j0, z0) = (
                    i as f64 + (1.0 - k) / 2.0,
                    j as f64 + (1.0 - k) / 2.0,
                    z as f64 + (1.0 - k) / 2.0,
                );
                let (i1, j1, z1) = (i0 + k, j0 + k, z0 + k);
                let top = vec![
                    at(i0, j0, z1),
                    at(i1, j0, z1),
                    at(i1, j1, z1),
                    at(i0, j1, z1),
                ];
                let left = vec![
                    at(i0, j1, z1),
                    at(i1, j1, z1),
                    at(i1, j1, z0),
                    at(i0, j1, z0),
                ];
                let right = vec![
                    at(i1, j0, z1),
                    at(i1, j1, z1),
                    at(i1, j1, z0),
                    at(i1, j0, z0),
                ];
                out.push(polygon(&top), true, Paint::Face(0));
                out.push(polygon(&left), true, Paint::Face(1));
                out.push(polygon(&right), true, Paint::Face(2));
            }
        }
    }
    // Moved so the drawing's bottom left is at the origin.
    if let Some(b) = out.bounds() {
        let m = Affine::translate(Vec2::new(-b.x0, -b.y0));
        for it in &mut out.items {
            it.path.apply_affine(m);
        }
    }
}

// ----- cone template --------------------------------------------------------

/// The measures of a cone's flat template.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConeLayout {
    /// Slant height (the side's length), mm.
    pub slant: f64,
    /// None for a cylinder (a rectangle).
    pub radii: Option<ConeRadii>,
    /// Width of the rectangle of a cylinder (its circumference), mm.
    pub circumference_top: f64,
    pub circumference_bottom: f64,
    /// Angle of the whole template, degrees (0 for a cylinder).
    pub angle: f64,
    /// Pieces it is cut into.
    pub pieces: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConeRadii {
    /// Radius of the arc of the top edge and of the bottom edge, mm.
    pub top: f64,
    pub bottom: f64,
}

pub fn cone_layout(s: &ShapeGenSettings) -> ConeLayout {
    let s = s.sanitized();
    let (dt, db, hgt) = (s.cone_top, s.cone_bottom, s.cone_height);
    let dr = (db - dt).abs() / 2.0;
    let slant = (hgt * hgt + dr * dr).sqrt();
    let pi = std::f64::consts::PI;
    let (ct, cb) = (pi * dt, pi * db);
    if (db - dt).abs() < 1e-6 {
        return ConeLayout {
            slant,
            radii: None,
            circumference_top: ct,
            circumference_bottom: cb,
            angle: 0.0,
            pieces: s.cone_pieces,
        };
    }
    // Radii from the apex: proportional to the diameters.
    let k = slant / (db - dt).abs();
    let (rt, rb) = (dt * k, db * k);
    let angle = (pi * (db - dt).abs() / slant).to_degrees();
    ConeLayout {
        slant,
        radii: Some(ConeRadii {
            top: rt,
            bottom: rb,
        }),
        circumference_top: ct,
        circumference_bottom: cb,
        angle,
        pieces: s.cone_pieces,
    }
}

/// Where the flat strip's point `(u, v)` lands on piece `piece` of the
/// template: `u` 0 to 1 along the piece's arc (left to right), `v` 0 at the
/// object's top edge to 1 at its bottom edge. Template coordinates, before
/// the move that puts the drawing's bottom left at the origin.
pub fn wrap_point(layout: &ConeLayout, piece: u32, u: f64, v: f64) -> Point {
    let pieces = layout.pieces.max(1) as f64;
    match layout.radii {
        None => {
            let w = layout.circumference_top / pieces;
            let gap = 10.0;
            Point::new(piece as f64 * (w + gap) + u * w, layout.slant * (1.0 - v))
        }
        Some(r) => {
            let sweep = layout.angle.to_radians() / pieces;
            let apex = piece_apex(layout, piece);
            let a = -std::f64::consts::FRAC_PI_2 - sweep / 2.0 + u * sweep;
            let rad = lerp(r.top, r.bottom, v);
            Point::new(apex.x + rad * a.cos(), apex.y + rad * a.sin())
        }
    }
}

/// The apex of piece `piece`: the pieces hang under their apexes side by
/// side, each opening downward, 10 mm apart.
fn piece_apex(layout: &ConeLayout, piece: u32) -> Point {
    let Some(r) = layout.radii else {
        return Point::ORIGIN;
    };
    let sweep = layout.angle.to_radians() / layout.pieces.max(1) as f64;
    let r_out = r.top.max(r.bottom);
    let width = if sweep >= std::f64::consts::PI {
        2.0 * r_out
    } else {
        2.0 * r_out * (sweep / 2.0).sin()
    };
    Point::new(piece as f64 * (width + 10.0), 0.0)
}

fn cone_template(s: &ShapeGenSettings, out: &mut Generated) {
    let l = cone_layout(s);
    let pieces = l.pieces.max(1);
    for k in 0..pieces {
        let outline = cone_piece_outline(&l, k, s.cone_tab, 0.0);
        out.push(outline, true, Paint::Line);
        if s.cone_bleed > 0.0 {
            out.push(
                cone_piece_outline(&l, k, 0.0, s.cone_bleed),
                true,
                Paint::Line,
            );
        }
    }
    if let Some(b) = out.bounds() {
        let m = Affine::translate(Vec2::new(-b.x0, -b.y0));
        for it in &mut out.items {
            it.path.apply_affine(m);
        }
    }
}

/// The offset that [`cone_template`] moves the template by (its bottom left
/// to the origin), for wrapping a strip onto the same place.
pub fn cone_origin_shift(s: &ShapeGenSettings) -> Vec2 {
    let l = cone_layout(s);
    let mut g = Generated::default();
    for k in 0..l.pieces.max(1) {
        g.push(
            cone_piece_outline(&l, k, s.sanitized().cone_tab, 0.0),
            true,
            Paint::Line,
        );
        if s.sanitized().cone_bleed > 0.0 {
            g.push(
                cone_piece_outline(&l, k, 0.0, s.sanitized().cone_bleed),
                true,
                Paint::Line,
            );
        }
    }
    g.bounds().map_or(Vec2::ZERO, |b| Vec2::new(-b.x0, -b.y0))
}

/// One piece's outline: top edge, the right side with the glue tab, the
/// bottom edge back and the left side; `bleed` grows it all round.
fn cone_piece_outline(l: &ConeLayout, piece: u32, tab: f64, bleed: f64) -> BezPath {
    let mut p = BezPath::new();
    let pieces = l.pieces.max(1) as f64;
    match l.radii {
        None => {
            let a = wrap_point(l, piece, 0.0, 1.0);
            let w = l.circumference_top / pieces;
            let r = Rect::new(
                a.x - bleed,
                a.y - bleed,
                a.x + w + bleed,
                a.y + l.slant + bleed,
            );
            p.move_to(Point::new(r.x0, r.y1));
            p.line_to(Point::new(r.x1, r.y1));
            if tab > 0.0 {
                let t = tab.min(l.slant / 2.0);
                p.line_to(Point::new(r.x1 + tab, r.y1 - t));
                p.line_to(Point::new(r.x1 + tab, r.y0 + t));
            }
            p.line_to(Point::new(r.x1, r.y0));
            p.line_to(Point::new(r.x0, r.y0));
            p.close_path();
        }
        Some(rad) => {
            let sweep = l.angle.to_radians() / pieces;
            let apex = piece_apex(l, piece);
            let (r_in, r_out) = (rad.top.min(rad.bottom), rad.top.max(rad.bottom));
            let (r_in, r_out) = ((r_in - bleed).max(0.0), r_out + bleed);
            let grow = if r_out > 0.0 { bleed / r_out } else { 0.0 };
            let a0 = -std::f64::consts::FRAC_PI_2 - sweep / 2.0 - grow;
            let a1 = -std::f64::consts::FRAC_PI_2 + sweep / 2.0 + grow;
            // Outer arc left to right.
            arc_into(&mut p, apex, r_out, a0, a1, true);
            // The right side, outer to inner, with the tab beside it.
            let at = |r: f64, a: f64| Point::new(apex.x + r * a.cos(), apex.y + r * a.sin());
            let (po, pi) = (at(r_out, a1), at(r_in, a1));
            if tab > 0.0 {
                let side = pi - po;
                let len = side.hypot();
                if len > 1e-9 {
                    let dir = side / len;
                    // Outward normal of the right side (away from the piece).
                    let n = Vec2::new(-dir.y, dir.x);
                    let n = if n.dot(Vec2::new(a1.cos() - a0.cos(), a1.sin() - a0.sin())) < 0.0 {
                        -n
                    } else {
                        n
                    };
                    let t = tab.min(len / 2.0);
                    p.line_to(po + n * tab + dir * t);
                    p.line_to(pi + n * tab - dir * t);
                }
            }
            p.line_to(pi);
            // Inner arc right to left (a point for a full cone).
            if r_in > 1e-9 {
                let mut inner = BezPath::new();
                arc_into(&mut inner, apex, r_in, a1, a0, true);
                for el in inner.elements().iter().skip(1) {
                    p.push(*el);
                }
            }
            p.close_path();
        }
    }
    p
}

/// Bend a flat strip (the paths inside `strip`) onto the template's
/// pieces: across the strip runs round the object `repeat` times, down
/// the strip runs from the object's top edge to its bottom edge. Curves
/// are flattened to fine lines first so they bend smoothly.
pub fn wrap_paths(
    s: &ShapeGenSettings,
    strip: Rect,
    paths: &[BezPath],
    tolerance: f64,
) -> Vec<Vec<BezPath>> {
    let s = s.sanitized();
    let l = cone_layout(&s);
    let shift = cone_origin_shift(&s);
    let pieces = l.pieces.max(1);
    let repeat = s.cone_repeat.max(1);
    let (sw, sh) = (strip.width().max(1e-9), strip.height().max(1e-9));
    let mut out = Vec::new();
    for path in paths {
        let mut per_path = Vec::new();
        // The strip copies round the object: copy c of `repeat` covers
        // u from c / repeat to (c + 1) / repeat of the whole way round.
        let total = repeat as f64;
        for c in 0..repeat {
            for piece in 0..pieces {
                let mut bent = BezPath::new();
                let mut any = false;
                let mut last: Option<Point> = None;
                // Straight pieces are cut short so they bend with the arcs.
                let max_seg = (sw / (64.0 * pieces as f64)).max(tolerance);
                tracedraw_core::geometry::flatten(path, tolerance, &mut |el| {
                    use tracedraw_core::geometry::PathEl;
                    let map = |p: Point| -> Option<Point> {
                        let fu = (p.x - strip.x0) / sw;
                        let fv = (strip.y1 - p.y) / sh;
                        // Position round the whole object, 0 to 1.
                        let g = (c as f64 + fu) / total;
                        let local = g * pieces as f64 - piece as f64;
                        if !(-1e-9..=1.0 + 1e-9).contains(&local) {
                            return None;
                        }
                        Some(wrap_point(&l, piece, local, fv) + shift)
                    };
                    let to = |p: Point, bent: &mut BezPath, any: &mut bool| {
                        match map(p) {
                            Some(q) if *any => bent.line_to(q),
                            Some(q) => {
                                bent.move_to(q);
                                *any = true;
                            }
                            // Off this piece: the curve starts again where
                            // it comes back.
                            None => *any = false,
                        }
                    };
                    match el {
                        PathEl::MoveTo(p) => {
                            any = false;
                            to(p, &mut bent, &mut any);
                            last = Some(p);
                        }
                        PathEl::LineTo(p) => {
                            let from = last.unwrap_or(p);
                            let n =
                                ((p - from).hypot() / max_seg).ceil().clamp(1.0, 4096.0) as usize;
                            for k in 1..=n {
                                to(from.lerp(p, k as f64 / n as f64), &mut bent, &mut any);
                            }
                            last = Some(p);
                        }
                        PathEl::ClosePath if any => bent.close_path(),
                        _ => {}
                    }
                });
                if !bent.elements().is_empty() {
                    per_path.push(bent);
                }
            }
        }
        out.push(per_path);
    }
    out
}

// ----- in the drawing -------------------------------------------------------

/// The object data key a generated group keeps its settings under.
pub const DATA_KEY: &str = "tracedraw.shape_generator";

/// Grey of a cube face: top light, left mid, right dark.
fn face_color(k: u8) -> tracedraw_core::Color {
    match k {
        0 => tracedraw_core::Color::rgb8(0xE8, 0xE8, 0xE8),
        1 => tracedraw_core::Color::rgb8(0x9C, 0x9C, 0x9C),
        _ => tracedraw_core::Color::rgb8(0x4E, 0x4E, 0x4E),
    }
}

impl crate::app::App {
    /// Where new generated objects go: the middle of the window, or of
    /// the page when the window has no size yet.
    fn shape_gen_place(&self) -> Point {
        let c = self.view_center();
        if c.x.is_finite() && c.y.is_finite() {
            c
        } else {
            self.page_rect().center()
        }
    }

    /// The colour generated lines and solids take: the default outline's,
    /// else black.
    fn shape_gen_color(&self) -> tracedraw_core::Color {
        self.default_stroke()
            .map(|s| s.color)
            .unwrap_or(tracedraw_core::Color::BLACK)
    }

    fn shape_gen_line_stroke(&self, s: &ShapeGenSettings) -> tracedraw_core::Stroke {
        let base = self
            .default_stroke()
            .unwrap_or_else(|| tracedraw_core::Stroke::hairline(tracedraw_core::Color::BLACK));
        if s.line_width > 0.0 {
            tracedraw_core::Stroke {
                width: s.line_width,
                ..base
            }
        } else {
            base
        }
    }

    /// The generated objects as one group whose middle is at `center`,
    /// carrying its settings; None when nothing was drawn.
    fn shape_gen_group(
        &mut self,
        s: &ShapeGenSettings,
        g: &Generated,
        center: Point,
    ) -> Option<tracedraw_core::Shape> {
        use tracedraw_core::{Fill, Shape, ShapeKind};
        let bounds = g.bounds()?;
        let anchor = g.frame.unwrap_or(bounds);
        let shift = Affine::translate(center - anchor.center());
        let color = self.shape_gen_color();
        let line = self.shape_gen_line_stroke(s);
        let accent = {
            let [r, g, b] = s.split_color;
            let mut st = line.clone();
            st.color = tracedraw_core::Color::rgb8(r, g, b);
            if s.split_width > 0.0 {
                st.width = s.split_width;
            }
            st
        };
        let palette = g.palette.clone();
        let mesh = g.mesh.as_ref().map(|m| m.transformed(shift));
        let mut children: Vec<Shape> = Vec::new();
        let make = |app: &mut Self, path: BezPath, closed: bool, paint: Paint| {
            let id = app.engine.new_shape_id();
            let mut shape = Shape::new(id, ShapeKind::Path { path, closed });
            match paint {
                Paint::Line => {
                    shape.fill = Fill::None;
                    shape.stroke = Some(line.clone());
                }
                Paint::Solid => {
                    shape.fill = Fill::Solid(color);
                    shape.stroke = None;
                }
                Paint::Face(k) => {
                    shape.fill = Fill::Solid(face_color(k));
                    shape.stroke = Some(tracedraw_core::Stroke::hairline(
                        tracedraw_core::Color::BLACK,
                    ));
                }
                Paint::Ink(k) => {
                    shape.fill = Fill::Solid(crate::shape_gen_cells::ink_color(k));
                    shape.stroke = None;
                }
                Paint::Accent => {
                    shape.fill = Fill::None;
                    shape.stroke = Some(accent.clone());
                }
                Paint::Tone(t) => {
                    let [r, g, b] = crate::shape_gen_bg::palette_color(&palette, t as f64 / 255.0);
                    shape.fill = Fill::Solid(tracedraw_core::Color::rgb8(r, g, b));
                    shape.stroke = None;
                }
                Paint::Mesh => {
                    shape.fill = match &mesh {
                        Some(m) => Fill::Mesh(m.clone()),
                        None => Fill::Solid(color),
                    };
                    shape.stroke = None;
                }
            }
            shape
        };
        let combine = s.combine && !matches!(s.kind, GenKind::IsoBlocks);
        if combine {
            // One curve per kind of paint, in the order they come.
            let mut parts: Vec<(Paint, BezPath, bool)> = Vec::new();
            for it in &g.items {
                let mut p = it.path.clone();
                p.apply_affine(shift);
                match parts.iter_mut().find(|(k, _, _)| *k == it.paint) {
                    Some((_, acc, closed)) => {
                        acc.extend(p);
                        *closed &= it.closed;
                    }
                    None => parts.push((it.paint, p, it.closed)),
                }
            }
            for (paint, path, closed) in parts {
                children.push(make(self, path, closed, paint));
            }
        } else {
            for it in &g.items {
                let mut p = it.path.clone();
                p.apply_affine(shift);
                children.push(make(self, p, it.closed, it.paint));
            }
        }
        if children.is_empty() {
            return None;
        }
        // Each ink's marks in a group of their own, named after it.
        let has_ink = g.items.iter().any(|i| matches!(i.paint, Paint::Ink(_)));
        if has_ink {
            let ink_of = |c: &Shape| -> Option<u8> {
                (0..4u8).find(|k| c.fill == Fill::Solid(crate::shape_gen_cells::ink_color(*k)))
            };
            let mut by_ink: Vec<Vec<Shape>> = vec![Vec::new(); 4];
            let mut rest = Vec::new();
            for c in children {
                match ink_of(&c) {
                    Some(k) => by_ink[k as usize].push(c),
                    None => rest.push(c),
                }
            }
            for (k, kids) in by_ink.into_iter().enumerate() {
                if kids.is_empty() {
                    continue;
                }
                let id = self.engine.new_shape_id();
                let mut sub = Shape::new(id, ShapeKind::Group { children: kids });
                sub.fill = Fill::None;
                sub.stroke = None;
                sub.name = Some(crate::i18n::tr(crate::shape_gen_cells::ink_key(k as u8)));
                rest.push(sub);
            }
            children = rest;
        }
        let gid = self.engine.new_shape_id();
        let mut group = Shape::new(gid, ShapeKind::Group { children });
        group.fill = Fill::None;
        group.stroke = None;
        group.name = Some(crate::i18n::tr(s.kind.key()));
        if let Ok(json) = serde_json::to_string(s) {
            group.data = vec![(DATA_KEY.to_string(), json)];
        }
        Some(group)
    }

    /// The selected generated group and the settings it was made with.
    pub fn shape_gen_selected(&self) -> Option<(tracedraw_core::ShapeId, ShapeGenSettings)> {
        if self.selection.len() != 1 {
            return None;
        }
        let shape = self.selected_shapes().into_iter().next()?;
        let json = shape
            .data
            .iter()
            .find(|(k, _)| k == DATA_KEY)
            .map(|(_, v)| v.clone())?;
        let s: ShapeGenSettings = serde_json::from_str(&json).ok()?;
        Some((shape.id, s.sanitized()))
    }

    /// Create: the panel's pattern as a group in the middle of the window
    /// (one undo step), selected.
    pub fn shape_gen_create(&mut self) -> Option<tracedraw_core::ShapeId> {
        let s = self.shape_gen.sanitized();
        let g = self
            .shape_gen_growth_cached(&s)
            .unwrap_or_else(|| generate(&s));
        let split = s.split_center.filter(|_| {
            s.split_mode != crate::shape_gen_warp::SplitMode::Off
                && s.split_shape.is_some()
                && crate::shape_gen_warp::applies(s.kind)
        });
        let center = match (s.source_center, split) {
            (Some(c), _) if s.kind.uses_source() => Point::new(c[0], c[1]),
            // Split by a shape: the drawing centred on it.
            (_, Some(c)) => Point::new(c[0], c[1]),
            _ => self.shape_gen_place(),
        };
        self.shape_gen_add(&s, &g, center, None, "Shape Generator")
    }

    /// Update: the selected generated group made again with the panel's
    /// settings, in the same place (one undo step).
    pub fn shape_gen_update(&mut self) -> Option<tracedraw_core::ShapeId> {
        let (old, _) = self.shape_gen_selected()?;
        let s = self.shape_gen.sanitized();
        let center = match s.source_center {
            Some(c) if s.kind.uses_source() => Point::new(c[0], c[1]),
            _ => self.selection_bounds()?.center(),
        };
        let g = self
            .shape_gen_growth_cached(&s)
            .unwrap_or_else(|| generate(&s));
        self.shape_gen_add(&s, &g, center, Some(old), "Shape Generator")
    }

    pub(crate) fn shape_gen_add(
        &mut self,
        s: &ShapeGenSettings,
        g: &Generated,
        center: Point,
        replace: Option<tracedraw_core::ShapeId>,
        label: &'static str,
    ) -> Option<tracedraw_core::ShapeId> {
        let layer = self.active_layer()?;
        let Some(group) = self.shape_gen_group(s, g, center) else {
            self.status = crate::i18n::tr("status.shape_generator_empty");
            return None;
        };
        let gid = group.id;
        let mut cmds = Vec::new();
        // The replaced group's layer, so an update stays where it was.
        let layer = match replace.and_then(|id| self.doc().locate(id).ok().map(|(l, _)| l)) {
            Some(l) => l,
            None => layer,
        };
        if let Some(old) = replace {
            cmds.push(tracedraw_core::Command::DeleteShapes { shapes: vec![old] });
        }
        cmds.push(tracedraw_core::Command::AddShape {
            layer,
            shape: group,
        });
        if let Err(e) = self.engine.run_batch(label, &cmds) {
            log::warn!("shape generator: {e}");
            return None;
        }
        self.select(vec![gid]);
        self.status = if g.cut {
            crate::i18n::trf(
                "status.shape_generator_cut",
                &[("max", &MAX_ITEMS.to_string())],
            )
        } else {
            crate::i18n::trf(
                "status.shape_generator_made",
                &[("n", &g.items.len().to_string())],
            )
        };
        Some(gid)
    }

    /// The selection's box and its closed outlines welded into one
    /// (holes kept; text as its letters), in page space; None with
    /// nothing selected or a flat selection.
    pub(crate) fn shape_gen_selection_outline(&self) -> Option<(Rect, Option<BezPath>)> {
        use tracedraw_core::{Shape, ShapeKind};
        let bounds = self.selection_bounds()?;
        if bounds.width() <= 1e-6 || bounds.height() <= 1e-6 {
            return None;
        }
        fn walk(shape: &Shape, t: Affine, out: &mut Vec<BezPath>) {
            match &shape.kind {
                ShapeKind::Group { children } => {
                    for c in children {
                        walk(c, t * shape.transform, out);
                    }
                }
                ShapeKind::Path { closed: false, .. }
                | ShapeKind::Bitmap { .. }
                | ShapeKind::ClipFrame { .. }
                | ShapeKind::Table(_)
                | ShapeKind::SymbolInstance { .. } => {}
                _ => out.push(t * shape.page_path()),
            }
        }
        let mut paths = Vec::new();
        for shape in self.selected_shapes() {
            walk(&shape, Affine::IDENTITY, &mut paths);
        }
        let mut outline: Option<BezPath> = None;
        for p in paths {
            outline = Some(match outline {
                None => tracedraw_core::shaping::overlay(
                    &p,
                    &BezPath::new(),
                    tracedraw_core::shaping::Op::Weld,
                ),
                Some(o) => {
                    tracedraw_core::shaping::overlay(&o, &p, tracedraw_core::shaping::Op::Weld)
                }
            });
        }
        Some((bounds, outline.filter(|o| !o.elements().is_empty())))
    }

    /// Use Selection: the selection becomes the panel's source. Its box
    /// sets Width and Height and where Create puts the result, its closed
    /// curves (welded, holes kept) the outline Voronoi fills and marks
    /// stay in, and for stippling and halftone it is drawn to a picture
    /// whose tones size the marks. False with nothing selected.
    pub fn shape_gen_use_selection(&mut self) -> bool {
        let Some((bounds, outline)) = self.shape_gen_selection_outline() else {
            return false;
        };
        let to_box = Affine::translate(Vec2::new(-bounds.x0, -bounds.y0));
        let outline = outline.map(|o| to_box * o);
        let s = &mut self.shape_gen;
        s.width = bounds.width();
        s.height = bounds.height();
        s.source_center = Some([bounds.center().x, bounds.center().y]);
        s.boundary = outline;
        s.vor_points.clear();
        s.tone = None;
        if self.shape_gen.kind.uses_tone() {
            // About 240 pixels along the longer side.
            let dpi = 240.0 / (bounds.width().max(bounds.height()) / 25.4);
            if let Some((png, _, _, _)) = self.render_selection_png(dpi.clamp(1.0, 2400.0), true) {
                use base64::Engine as _;
                self.shape_gen.tone = Some(base64::engine::general_purpose::STANDARD.encode(png));
            }
        }
        true
    }

    /// Clear the picked selection: back to the box and the stand-in ramp.
    pub fn shape_gen_clear_source(&mut self) {
        let s = &mut self.shape_gen;
        s.source_center = None;
        s.boundary = None;
        s.tone = None;
        s.vor_points.clear();
    }

    /// Cone template: the template made in the middle of the window and
    /// the selection bent onto it (#3413): the selection's box is the flat
    /// strip, its width going round the object `cone_repeat` times, its
    /// top along the object's top edge. The bent copies keep each
    /// object's fill and outline; the originals stay. One undo step.
    pub fn shape_gen_wrap_selection(&mut self) -> Option<tracedraw_core::ShapeId> {
        use tracedraw_core::{Shape, ShapeKind};
        let mut s = self.shape_gen.sanitized();
        s.kind = GenKind::ConeTemplate;
        let strip = self.selection_bounds()?;
        // The selection's curves with their paint, groups opened.
        fn walk(shape: &Shape, t: Affine, out: &mut Vec<(BezPath, Shape)>) {
            match &shape.kind {
                ShapeKind::Group { children } => {
                    for c in children {
                        walk(c, t * shape.transform, out);
                    }
                }
                ShapeKind::Text { .. }
                | ShapeKind::Bitmap { .. }
                | ShapeKind::ClipFrame { .. }
                | ShapeKind::Table(_)
                | ShapeKind::SymbolInstance { .. } => {}
                _ => out.push((t * shape.page_path(), shape.clone())),
            }
        }
        let mut parts = Vec::new();
        for shape in self.selected_shapes() {
            walk(&shape, Affine::IDENTITY, &mut parts);
        }
        if parts.is_empty() {
            self.status = crate::i18n::tr("status.shape_generator_wrap_none");
            return None;
        }
        let template = generate(&s);
        let tb = template.bounds()?;
        let center = self.shape_gen_place();
        let to_page = Affine::translate(center - tb.center());
        let paths: Vec<BezPath> = parts.iter().map(|(p, _)| p.clone()).collect();
        let tol = (strip.width().max(strip.height()) / 2000.0).max(0.005);
        let bent = wrap_paths(&s, strip, &paths, tol);
        let mut group = self.shape_gen_group(&s, &template, center)?;
        if let ShapeKind::Group { children } = &mut group.kind {
            for ((_, src), copies) in parts.iter().zip(bent) {
                for mut path in copies {
                    path.apply_affine(to_page);
                    let id = self.engine.new_shape_id();
                    let closed = matches!(src.kind, ShapeKind::Path { closed: true, .. })
                        || !matches!(src.kind, ShapeKind::Path { .. });
                    let mut c = Shape::new(id, ShapeKind::Path { path, closed });
                    c.fill = src.fill.clone();
                    c.stroke = src.stroke.clone();
                    c.opacity = src.opacity;
                    children.push(c);
                }
            }
        }
        let layer = self.active_layer()?;
        let gid = group.id;
        if let Err(e) = self.engine.run_batch(
            "Wrap onto Cone",
            &[tracedraw_core::Command::AddShape {
                layer,
                shape: group,
            }],
        ) {
            log::warn!("shape generator wrap: {e}");
            return None;
        }
        self.select(vec![gid]);
        self.status = crate::i18n::tr("status.shape_generator_wrapped");
        Some(gid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(kind: GenKind) -> ShapeGenSettings {
        ShapeGenSettings {
            kind,
            ..ShapeGenSettings::default()
        }
    }

    #[test]
    fn every_generator_draws_inside_its_box_and_repeats_itself() {
        for kind in GenKind::ALL {
            let s = with(kind);
            let a = generate(&s);
            let b = generate(&s);
            assert!(!a.items.is_empty(), "{kind:?} drew nothing");
            assert_eq!(a.items.len(), b.items.len(), "{kind:?} not repeatable");
            for (x, y) in a.items.iter().zip(&b.items) {
                assert_eq!(x.path, y.path, "{kind:?} not repeatable");
            }
            if kind.uses_box() && kind != GenKind::IsoBlocks {
                let bb = a.bounds().expect("bounds");
                let slack = s.width * 0.25;
                assert!(bb.x0 > -slack && bb.y0 > -slack, "{kind:?} {bb:?}");
                assert!(
                    bb.x1 < s.width + slack && bb.y1 < s.height + slack,
                    "{kind:?} {bb:?}"
                );
            }
        }
    }

    #[test]
    fn another_seed_deals_another_pattern() {
        for kind in [
            GenKind::Truchet,
            GenKind::LineWaves,
            GenKind::FieldLines,
            GenKind::IsoBlocks,
        ] {
            let a = generate(&with(kind));
            let b = generate(&ShapeGenSettings {
                seed: 99,
                ..with(kind)
            });
            let same = a.items.len() == b.items.len()
                && a.items.iter().zip(&b.items).all(|(x, y)| x.path == y.path);
            assert!(!same, "{kind:?}");
        }
    }

    #[test]
    fn bad_values_never_panic() {
        let mut s = ShapeGenSettings {
            width: f64::NAN,
            height: -5.0,
            cells: 0,
            tile: 0.0,
            lines: 0,
            rings: 0,
            line_spacing: 0.0,
            iso_cols: 0,
            cone_top: f64::INFINITY,
            cone_bottom: 0.0,
            cone_height: 0.0,
            ..ShapeGenSettings::default()
        };
        for kind in GenKind::ALL {
            s.kind = kind;
            let _ = generate(&s);
        }
    }

    #[test]
    fn dot_grid_cells_shrink_toward_the_edge() {
        let s = ShapeGenSettings {
            warp: Warp::None,
            scale_ratio: 20.0,
            ..with(GenKind::DotGrid)
        };
        let g = generate(&s);
        use tracedraw_core::geometry::Shape as _;
        let sizes: Vec<(f64, f64)> = g
            .items
            .iter()
            .map(|i| {
                let b = i.path.bounding_box();
                ((b.center() - Point::new(50.0, 50.0)).hypot(), b.width())
            })
            .collect();
        let middle = sizes
            .iter()
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .expect("a cell");
        let edge = sizes
            .iter()
            .max_by(|a, b| a.0.total_cmp(&b.0))
            .expect("a cell");
        assert!(middle.1 > edge.1 * 2.0, "{middle:?} {edge:?}");
        assert_eq!(g.items.len(), 25 * 25);
    }

    #[test]
    fn truchet_radii_meet_their_neighbours() {
        for bands in 1..=8 {
            let t = 10.0;
            let r = truchet_radii(t, bands);
            assert_eq!(r.len(), bands as usize);
            // Symmetric about half the side: an edge's ends match whichever
            // way the next tile is turned.
            let mut a: Vec<f64> = r.clone();
            let mut b: Vec<f64> = r.iter().map(|x| t - x).collect();
            a.sort_by(f64::total_cmp);
            b.sort_by(f64::total_cmp);
            for (x, y) in a.iter().zip(&b) {
                assert!((x - y).abs() < 1e-9, "{bands}: {a:?} {b:?}");
            }
            // Arcs from opposite corners never cross.
            assert!(r.iter().all(|x| 2.0 * x < t * 2f64.sqrt()));
        }
    }

    #[test]
    fn hexagons_cover_the_box_without_overlap() {
        let s = ShapeGenSettings {
            gap: 0.0,
            tile: 10.0,
            ..with(GenKind::Tessellation)
        };
        let g = generate(&s);
        use tracedraw_core::geometry::Shape as _;
        let area: f64 = g.items.iter().map(|i| i.path.area().abs()).sum();
        // Tiles whose middle is inside: about the box's area.
        assert!((area / (100.0 * 100.0) - 1.0).abs() < 0.15, "{area}");
    }

    #[test]
    fn line_waves_make_one_curve_per_line_and_bend_in_the_bump() {
        let s = with(GenKind::LineWaves);
        let g = generate(&s);
        assert_eq!(g.items.len(), s.lines as usize);
        use tracedraw_core::geometry::Shape as _;
        let mid = &g.items[s.lines as usize / 2].path;
        let b = mid.bounding_box();
        assert!(b.height() > 0.5, "{b:?}");
        // The ends lie on the line's place: no bump there.
        let first = mid
            .elements()
            .first()
            .and_then(|e| e.end_point())
            .expect("start");
        let base = (s.lines as usize / 2) as f64 / (s.lines - 1) as f64 * s.height;
        assert!((first.y - base).abs() < 0.5, "{first:?} {base}");
    }

    #[test]
    fn rings_stay_between_their_radii() {
        let s = ShapeGenSettings {
            ring_amplitude: 0.0,
            ..with(GenKind::Rings)
        };
        let g = generate(&s);
        assert_eq!(g.items.len(), s.rings as usize);
        use tracedraw_core::geometry::Shape as _;
        let first = g.items[0].path.bounding_box();
        let last = g.items[g.items.len() - 1].path.bounding_box();
        assert!(
            (first.width() / 2.0 - s.min_radius).abs() < 0.2,
            "{first:?}"
        );
        assert!((last.width() / 2.0 - s.max_radius).abs() < 0.2, "{last:?}");
    }

    #[test]
    fn field_lines_keep_their_spacing() {
        let s = with(GenKind::FieldLines);
        let g = generate(&s);
        assert!(g.items.len() > 20, "{}", g.items.len());
        assert!(!g.cut);
    }

    #[test]
    fn iso_blocks_draw_three_faces_per_cube_back_to_front() {
        let s = ShapeGenSettings {
            iso_layout: IsoLayout::Flat,
            iso_cols: 3,
            iso_rows: 2,
            ..with(GenKind::IsoBlocks)
        };
        let g = generate(&s);
        assert_eq!(g.items.len(), 3 * 2 * 3);
        assert_eq!(g.items[0].paint, Paint::Face(0));
        assert_eq!(g.items[1].paint, Paint::Face(1));
        assert_eq!(g.items[2].paint, Paint::Face(2));
        // A pyramid is highest in the middle.
        let p = ShapeGenSettings {
            iso_layout: IsoLayout::Pyramid,
            iso_cols: 5,
            iso_rows: 5,
            iso_height: 3,
            ..with(GenKind::IsoBlocks)
        };
        let h = iso_heights(&p);
        assert_eq!(h[2][2], 3);
        assert!(h[0][0] < h[2][2]);
    }

    #[test]
    fn cone_template_measures_roll_back_into_the_object() {
        let s = ShapeGenSettings {
            cone_top: 80.0,
            cone_bottom: 60.0,
            cone_height: 100.0,
            ..with(GenKind::ConeTemplate)
        };
        let l = cone_layout(&s);
        let r = l.radii.expect("a cone");
        // The arcs are as long as the object's rims.
        let sweep = l.angle.to_radians();
        assert!((r.top * sweep - std::f64::consts::PI * 80.0).abs() < 1e-6);
        assert!((r.bottom * sweep - std::f64::consts::PI * 60.0).abs() < 1e-6);
        assert!(((r.top - r.bottom) - l.slant).abs() < 1e-9);
        assert!((l.slant - (100f64 * 100.0 + 10.0 * 10.0).sqrt()).abs() < 1e-9);
        // A cylinder is a rectangle as wide as its rim.
        let c = cone_layout(&ShapeGenSettings {
            cone_top: 50.0,
            cone_bottom: 50.0,
            cone_height: 30.0,
            ..s.clone()
        });
        assert!(c.radii.is_none());
        assert!((c.circumference_top - std::f64::consts::PI * 50.0).abs() < 1e-9);
        // A full cone: a sector (no inner arc).
        let f = cone_layout(&ShapeGenSettings {
            cone_top: 0.0,
            ..s.clone()
        });
        assert!(f.radii.expect("cone").top.abs() < 1e-9);
        let g = generate(&ShapeGenSettings {
            cone_top: 0.0,
            ..s.clone()
        });
        assert_eq!(g.items.len(), 1);
    }

    #[test]
    fn cone_pieces_and_bleed_add_outlines() {
        let s = ShapeGenSettings {
            cone_pieces: 3,
            cone_bleed: 3.0,
            ..with(GenKind::ConeTemplate)
        };
        let g = generate(&s);
        assert_eq!(g.items.len(), 6);
    }

    #[test]
    fn a_wrapped_strip_lands_on_the_template() {
        let s = with(GenKind::ConeTemplate);
        let l = cone_layout(&s);
        let r = l.radii.expect("cone");
        let shift = cone_origin_shift(&s);
        // A horizontal line across the strip's top lands on the top arc.
        let strip = Rect::new(0.0, 0.0, 100.0, 20.0);
        let mut line = BezPath::new();
        line.move_to(Point::new(0.0, 20.0));
        line.line_to(Point::new(100.0, 20.0));
        let bent = wrap_paths(&s, strip, &[line], 0.05);
        assert_eq!(bent.len(), 1);
        let p = &bent[0][0];
        let apex = wrap_point(&l, 0, 0.5, 0.0) + shift;
        let apex = Point::new(apex.x, apex.y + r.top);
        let pts: Vec<Point> = p.elements().iter().filter_map(|e| e.end_point()).collect();
        assert!(pts.len() > 10, "a straight line is bent into many points");
        for q in pts {
            assert!(((q - apex).hypot() - r.top).abs() < 1e-6, "{q:?}");
        }
    }

    fn shape_count(app: &crate::app::App) -> usize {
        app.doc().all_layers().map(|l| l.shapes.len()).sum()
    }

    #[test]
    fn create_adds_one_group_in_one_undo_step_and_update_replaces_it() {
        let mut app = crate::app::App::headless();
        let before = shape_count(&app);
        app.shape_gen = ShapeGenSettings {
            kind: GenKind::Truchet,
            ..ShapeGenSettings::default()
        };
        let id = app.shape_gen_create().expect("a group");
        assert_eq!(shape_count(&app), before + 1);
        assert_eq!(app.selection, vec![id]);
        // The group keeps its settings.
        let (sel, saved) = app.shape_gen_selected().expect("generated");
        assert_eq!(sel, id);
        assert_eq!(saved.kind, GenKind::Truchet);
        // Update: other settings, the same place, still one object.
        let middle = app.selection_bounds().expect("bounds").center();
        app.shape_gen.seed = 77;
        app.shape_gen.bands = 1;
        let id2 = app.shape_gen_update().expect("updated");
        assert_ne!(id, id2);
        assert_eq!(shape_count(&app), before + 1);
        let m2 = app.selection_bounds().expect("bounds").center();
        assert!((m2 - middle).hypot() < 1e-6, "{m2:?} {middle:?}");
        assert_eq!(app.shape_gen_selected().expect("generated").1.seed, 77);
        // Undo brings back the first group; again removes it.
        app.undo();
        assert_eq!(shape_count(&app), before + 1);
        app.undo();
        assert_eq!(shape_count(&app), before);
    }

    #[test]
    fn combine_makes_one_curve_per_paint() {
        let mut app = crate::app::App::headless();
        app.shape_gen = ShapeGenSettings {
            kind: GenKind::Rings,
            combine: true,
            ..ShapeGenSettings::default()
        };
        app.shape_gen_create().expect("a group");
        let g = app.selected_shapes().into_iter().next().expect("group");
        let tracedraw_core::ShapeKind::Group { children } = &g.kind else {
            panic!("a group");
        };
        assert_eq!(children.len(), 1);
    }

    #[test]
    fn wrap_selection_bends_a_strip_onto_the_template() {
        let mut app = crate::app::App::headless();
        let strip = app
            .new_shape(tracedraw_core::ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 120.0, 20.0),
                radius: 0.0,
                corners: None,
            })
            .expect("strip");
        app.select(vec![strip]);
        let before = shape_count(&app);
        app.shape_gen = ShapeGenSettings {
            kind: GenKind::ConeTemplate,
            cone_repeat: 2,
            ..ShapeGenSettings::default()
        };
        let id = app.shape_gen_wrap_selection().expect("wrapped");
        assert_eq!(shape_count(&app), before + 1, "the strip stays");
        let g = app.selected_shapes().into_iter().next().expect("group");
        assert_eq!(g.id, id);
        let tracedraw_core::ShapeKind::Group { children } = &g.kind else {
            panic!("a group");
        };
        // The template and the strip twice round.
        assert_eq!(children.len(), 3);
    }

    #[test]
    fn settings_survive_a_restart_and_bad_ones_are_brought_into_range() {
        let mut app = crate::app::App::headless();
        app.shape_gen.kind = GenKind::FieldLines;
        app.shape_gen.poles = 9;
        let prefs = app.tool_prefs();
        let mut next = crate::app::App::headless();
        next.apply_tool_prefs(&prefs);
        assert_eq!(next.shape_gen, app.shape_gen.sanitized());
        let mut odd = prefs.clone();
        odd.shape_gen.poles = 0;
        odd.shape_gen.width = f64::NAN;
        next.apply_tool_prefs(&odd);
        assert_eq!(next.shape_gen.poles, 1);
        assert_eq!(next.shape_gen.width, ShapeGenSettings::default().width);
    }

    fn black_disc(app: &mut crate::app::App) -> tracedraw_core::ShapeId {
        let id = app
            .new_shape(tracedraw_core::ShapeKind::Ellipse {
                rect: Rect::new(10.0, 10.0, 70.0, 70.0),
                arc: None,
            })
            .expect("disc");
        app.run(tracedraw_core::Command::SetFill {
            shapes: vec![id],
            fill: tracedraw_core::Fill::Solid(tracedraw_core::Color::BLACK),
        });
        app.select(vec![id]);
        id
    }

    #[test]
    fn halftone_of_a_picked_disc_lands_on_it_inside_its_outline() {
        let mut app = crate::app::App::headless();
        black_disc(&mut app);
        app.shape_gen = ShapeGenSettings {
            kind: GenKind::Halftone,
            ht_cell: 3.0,
            ..ShapeGenSettings::default()
        };
        assert!(app.shape_gen_use_selection());
        assert!(app.shape_gen.tone.is_some());
        assert!(app.shape_gen.boundary.is_some());
        assert!((app.shape_gen.width - 60.0).abs() < 1e-6);
        app.shape_gen_create().expect("made");
        let g = app.selected_shapes().into_iter().next().expect("group");
        let tracedraw_core::ShapeKind::Group { children } = &g.kind else {
            panic!("a group");
        };
        // Black everywhere inside: full dots over the disc's area.
        assert!(children.len() > 250, "{}", children.len());
        let mid = Point::new(40.0, 40.0);
        for c in children {
            use tracedraw_core::geometry::Shape as _;
            let b = c.page_path().bounding_box();
            assert!((b.center() - mid).hypot() < 31.0, "{b:?}");
        }
    }

    #[test]
    fn cmyk_marks_are_grouped_per_ink() {
        let mut app = crate::app::App::headless();
        black_disc(&mut app);
        app.shape_gen = ShapeGenSettings {
            kind: GenKind::Halftone,
            ht_cell: 4.0,
            ht_cmyk: true,
            ..ShapeGenSettings::default()
        };
        app.shape_gen_use_selection();
        app.shape_gen_create().expect("made");
        let g = app.selected_shapes().into_iter().next().expect("group");
        let tracedraw_core::ShapeKind::Group { children } = &g.kind else {
            panic!("a group");
        };
        // Black only: one ink group.
        assert_eq!(children.len(), 1);
        assert!(matches!(
            children[0].kind,
            tracedraw_core::ShapeKind::Group { .. }
        ));
    }

    #[test]
    fn voronoi_fills_a_picked_shape_and_clear_forgets_it() {
        let mut app = crate::app::App::headless();
        black_disc(&mut app);
        app.shape_gen = ShapeGenSettings {
            kind: GenKind::Voronoi,
            vor_spacing: 6.0,
            ..ShapeGenSettings::default()
        };
        app.shape_gen_use_selection();
        assert!(app.shape_gen.tone.is_none(), "cells need no picture");
        let g = generate(&app.shape_gen);
        use tracedraw_core::geometry::Shape as _;
        let area: f64 = g.items.iter().map(|i| i.path.area().abs()).sum();
        let disc = std::f64::consts::PI * 30.0 * 30.0;
        assert!((area - disc).abs() < disc * 0.03, "{area} {disc}");
        app.shape_gen_clear_source();
        assert!(app.shape_gen.boundary.is_none() && app.shape_gen.source_center.is_none());
    }
}
