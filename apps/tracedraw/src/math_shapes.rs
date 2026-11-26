//! Math Shapes (Window > Panels > Math Shapes): shapes drawn from
//! mathematics. A Cartesian plane with axes, grid, ticks and numbers, with
//! any number of curves on it (functions y = f(x), parametric curves,
//! polar curves, equations in x and y, and points), and geometric figures
//! built from their measures (circle, triangle, regular polygon or star,
//! ellipse, arc and sector).
//!
//! Everything here is a pure function of [`MathSettings`]: [`draw`] gives
//! the items (curves with their paint) and labels in millimetres, y up;
//! the panel previews them and `App::math_create` turns them into one
//! group of plain curves and texts. See `docs/behavior/math-shapes.md`.

use crate::math_expr::{self, Expr, ExprError, Vars};
use kurbo::{BezPath, Point, Rect, Vec2};
use std::f64::consts::{PI, TAU};

/// The object data key a Math Shapes group keeps its settings under.
pub const DATA_KEY: &str = "tracedraw.math_shapes";

/// Most curves (plots) in one plane.
pub const MAX_PLOTS: usize = 12;
/// Most points one curve is drawn with (after refining).
const MAX_POINTS: usize = 200_000;
/// Most grid or tick lines in one direction.
const MAX_LINES: usize = 1000;

pub type Rgb = [u8; 3];

/// What the panel makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// A Cartesian plane and curves.
    #[default]
    Graph,
    /// A geometric figure from its measures.
    Figure,
}

/// The kind of a curve on the plane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlotKind {
    /// y = f(x).
    #[default]
    Function,
    /// x = f(t), y = g(t).
    Parametric,
    /// r = f(θ).
    Polar,
    /// An equation in x and y, such as x² + y² = 9.
    Implicit,
    /// Points typed as A(2, 3); B(-1, 4).
    Points,
}

impl PlotKind {
    pub const ALL: [PlotKind; 5] = [
        PlotKind::Function,
        PlotKind::Parametric,
        PlotKind::Polar,
        PlotKind::Implicit,
        PlotKind::Points,
    ];
    pub fn key(self) -> &'static str {
        match self {
            PlotKind::Function => "math.kind_function",
            PlotKind::Parametric => "math.kind_parametric",
            PlotKind::Polar => "math.kind_polar",
            PlotKind::Implicit => "math.kind_implicit",
            PlotKind::Points => "math.kind_points",
        }
    }
}

/// How a line is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dash {
    #[default]
    Solid,
    Dashed,
    Dotted,
}

impl Dash {
    pub const ALL: [Dash; 3] = [Dash::Solid, Dash::Dashed, Dash::Dotted];
    pub fn key(self) -> &'static str {
        match self {
            Dash::Solid => "math.dash_solid",
            Dash::Dashed => "math.dash_dashed",
            Dash::Dotted => "math.dash_dotted",
        }
    }
    /// On/off lengths in multiples of the line width (empty: solid).
    pub fn pattern(self) -> Vec<f64> {
        match self {
            Dash::Solid => Vec::new(),
            Dash::Dashed => vec![6.0, 4.0],
            Dash::Dotted => vec![1.0, 2.5],
        }
    }
}

/// The Cartesian plane.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Plane {
    /// Draw the plane at all (off: the curves alone, for decorative
    /// shapes; they are not cut at the plane's edges then).
    pub show: bool,
    pub x_min: f64,
    pub x_max: f64,
    pub y_min: f64,
    pub y_max: f64,
    /// Millimetres per unit across and up.
    pub unit_x: f64,
    pub unit_y: f64,
    pub axes: bool,
    pub arrows: bool,
    pub axis_names: bool,
    pub axis_color: Rgb,
    pub axis_width: f64,
    pub grid: bool,
    /// Grid spacing in units.
    pub grid_x: f64,
    pub grid_y: f64,
    pub grid_color: Rgb,
    pub grid_width: f64,
    pub minor: bool,
    /// Minor lines per grid square.
    pub minor_div: u32,
    pub minor_color: Rgb,
    pub minor_width: f64,
    pub ticks: bool,
    /// Tick and number spacing in units.
    pub tick_x: f64,
    pub tick_y: f64,
    /// Tick length in mm.
    pub tick_len: f64,
    pub numbers: bool,
    pub number_pt: f64,
    pub number_color: Rgb,
    /// Write the numbers across as multiples of π.
    pub pi_x: bool,
    /// A 0 at the origin.
    pub origin: bool,
    pub border: bool,
    pub font: String,
}

impl Default for Plane {
    fn default() -> Self {
        Self {
            show: true,
            x_min: -6.0,
            x_max: 6.0,
            y_min: -4.0,
            y_max: 4.0,
            unit_x: 10.0,
            unit_y: 10.0,
            axes: true,
            arrows: true,
            axis_names: true,
            axis_color: [0x1A, 0x1A, 0x1A],
            axis_width: 0.35,
            grid: true,
            grid_x: 1.0,
            grid_y: 1.0,
            grid_color: [0xB4, 0xBE, 0xC8],
            grid_width: 0.15,
            minor: false,
            minor_div: 5,
            minor_color: [0xDF, 0xE5, 0xEA],
            minor_width: 0.08,
            ticks: true,
            tick_x: 1.0,
            tick_y: 1.0,
            tick_len: 1.6,
            numbers: true,
            number_pt: 7.0,
            number_color: [0x33, 0x33, 0x33],
            pi_x: false,
            origin: true,
            border: false,
            font: "Arial".into(),
        }
    }
}

/// One curve on the plane.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Plot {
    pub kind: PlotKind,
    /// y = f(x); x(t); r(θ); the equation; the points.
    pub f: String,
    /// y(t) of a parametric curve.
    pub g: String,
    /// The range of t or θ (parametric and polar).
    pub t_min: f64,
    pub t_max: f64,
    /// Keep a function to [d_min, d_max] (its domain).
    pub domain: bool,
    pub d_min: f64,
    pub d_max: f64,
    /// Samples across the range (refined where the curve bends).
    pub samples: u32,
    pub visible: bool,
    pub color: Rgb,
    pub width: f64,
    pub dash: Dash,
    /// Fill under a function (to the x axis), or inside a closed curve.
    pub fill: bool,
    pub fill_color: Rgb,
    /// Write the formula next to the curve (the points' names).
    pub label: bool,
    /// Mark a function's zeros, y intercept and turning points; for
    /// points, dashed lines to the axes.
    pub marks: bool,
}

impl Default for Plot {
    fn default() -> Self {
        Self {
            kind: PlotKind::Function,
            f: "x^2 - 2x - 3".into(),
            g: String::new(),
            t_min: 0.0,
            t_max: TAU,
            domain: false,
            d_min: -1.0,
            d_max: 1.0,
            samples: 400,
            visible: true,
            color: PLOT_COLORS[0],
            width: 0.5,
            dash: Dash::Solid,
            fill: false,
            fill_color: [0xC9, 0xDD, 0xF6],
            label: false,
            marks: false,
        }
    }
}

/// Colours given to new curves in turn.
pub const PLOT_COLORS: [Rgb; 8] = [
    [0x1F, 0x6F, 0xD1],
    [0xD1, 0x2F, 0x2F],
    [0x2E, 0x9B, 0x45],
    [0x8A, 0x3F, 0xD1],
    [0xE0, 0x7B, 0x00],
    [0x00, 0x96, 0xA6],
    [0xC2, 0x18, 0x7E],
    [0x55, 0x55, 0x55],
];

/// Lighter fills to go with [`PLOT_COLORS`].
pub fn light(c: Rgb) -> Rgb {
    let m = |v: u8| (v as f64 + (255.0 - v as f64) * 0.75).round() as u8;
    [m(c[0]), m(c[1]), m(c[2])]
}

impl Plot {
    /// A new curve of the `i`th colour.
    pub fn nth(i: usize) -> Plot {
        let color = PLOT_COLORS[i % PLOT_COLORS.len()];
        Plot {
            color,
            fill_color: light(color),
            f: match i % 4 {
                0 => "x^2 - 2x - 3",
                1 => "sin(x)",
                2 => "2x + 1",
                _ => "sqrt(x)",
            }
            .into(),
            ..Default::default()
        }
    }
}

/// The geometric figures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FigureKind {
    #[default]
    Circle,
    Triangle,
    Polygon,
    Ellipse,
    Arc,
}

impl FigureKind {
    pub const ALL: [FigureKind; 5] = [
        FigureKind::Circle,
        FigureKind::Triangle,
        FigureKind::Polygon,
        FigureKind::Ellipse,
        FigureKind::Arc,
    ];
    pub fn key(self) -> &'static str {
        match self {
            FigureKind::Circle => "math.fig_circle",
            FigureKind::Triangle => "math.fig_triangle",
            FigureKind::Polygon => "math.fig_polygon",
            FigureKind::Ellipse => "math.fig_ellipse",
            FigureKind::Arc => "math.fig_arc",
        }
    }
}

/// What a circle is given by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CircleBy {
    #[default]
    Radius,
    Diameter,
    Circumference,
    Area,
}

impl CircleBy {
    pub const ALL: [CircleBy; 4] = [
        CircleBy::Radius,
        CircleBy::Diameter,
        CircleBy::Circumference,
        CircleBy::Area,
    ];
    pub fn key(self) -> &'static str {
        match self {
            CircleBy::Radius => "math.by_radius",
            CircleBy::Diameter => "math.by_diameter",
            CircleBy::Circumference => "math.by_circumference",
            CircleBy::Area => "math.by_area",
        }
    }
}

/// What a triangle is given by. Sides a, b, c face the vertices A, B, C.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriBy {
    /// Three sides.
    #[default]
    Sss,
    /// Sides b and c and the angle A between them.
    Sas,
    /// Angles A and B and the side c between them.
    Asa,
    /// The legs a and b; the right angle at C.
    Right,
    /// The base c and the two equal sides a = b.
    Isosceles,
    /// One side for all three.
    Equilateral,
}

impl TriBy {
    pub const ALL: [TriBy; 6] = [
        TriBy::Sss,
        TriBy::Sas,
        TriBy::Asa,
        TriBy::Right,
        TriBy::Isosceles,
        TriBy::Equilateral,
    ];
    pub fn key(self) -> &'static str {
        match self {
            TriBy::Sss => "math.tri_sss",
            TriBy::Sas => "math.tri_sas",
            TriBy::Asa => "math.tri_asa",
            TriBy::Right => "math.tri_right",
            TriBy::Isosceles => "math.tri_isosceles",
            TriBy::Equilateral => "math.tri_equilateral",
        }
    }
}

/// Arc, sector (a pie slice) or segment (cut by the chord).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArcKind {
    Arc,
    #[default]
    Sector,
    Segment,
}

impl ArcKind {
    pub const ALL: [ArcKind; 3] = [ArcKind::Arc, ArcKind::Sector, ArcKind::Segment];
    pub fn key(self) -> &'static str {
        match self {
            ArcKind::Arc => "math.arc_arc",
            ArcKind::Sector => "math.arc_sector",
            ArcKind::Segment => "math.arc_segment",
        }
    }
}

/// The unit measures are written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeasureUnit {
    #[default]
    Mm,
    Cm,
    /// Numbers only.
    None,
}

impl MeasureUnit {
    pub const ALL: [MeasureUnit; 3] = [MeasureUnit::Mm, MeasureUnit::Cm, MeasureUnit::None];
    pub fn key(self) -> &'static str {
        match self {
            MeasureUnit::Mm => "math.unit_mm",
            MeasureUnit::Cm => "math.unit_cm",
            MeasureUnit::None => "math.unit_none",
        }
    }
}

/// A figure from its measures (lengths in mm, angles in degrees).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Figure {
    pub kind: FigureKind,
    pub circle_by: CircleBy,
    /// The circle's radius, diameter, circumference or area.
    pub circle_value: f64,
    pub tri_by: TriBy,
    pub side_a: f64,
    pub side_b: f64,
    pub side_c: f64,
    pub angle_a: f64,
    pub angle_b: f64,
    pub sides: u32,
    /// Regular polygon: by its side (else by its circumradius).
    pub by_side: bool,
    pub poly_value: f64,
    /// Star: join every `star`th corner (1 = a plain polygon).
    pub star: u32,
    pub rotation: f64,
    pub rx: f64,
    pub ry: f64,
    pub arc_kind: ArcKind,
    pub arc_radius: f64,
    pub arc_start: f64,
    pub arc_sweep: f64,
    // What is shown.
    pub names: bool,
    pub measures: bool,
    pub angles: bool,
    pub center: bool,
    pub radius_line: bool,
    pub diameter_line: bool,
    pub heights: bool,
    pub medians: bool,
    pub bisectors: bool,
    pub incircle: bool,
    pub circumcircle: bool,
    pub diagonals: bool,
    pub foci: bool,
    pub unit: MeasureUnit,
    pub decimals: u32,
    pub color: Rgb,
    pub width: f64,
    pub fill: bool,
    pub fill_color: Rgb,
    pub aid_color: Rgb,
    pub label_pt: f64,
    pub font: String,
}

impl Default for Figure {
    fn default() -> Self {
        Self {
            kind: FigureKind::Triangle,
            circle_by: CircleBy::Radius,
            circle_value: 30.0,
            tri_by: TriBy::Sss,
            side_a: 50.0,
            side_b: 40.0,
            side_c: 60.0,
            angle_a: 60.0,
            angle_b: 50.0,
            sides: 6,
            by_side: true,
            poly_value: 30.0,
            star: 1,
            rotation: 0.0,
            rx: 40.0,
            ry: 25.0,
            arc_kind: ArcKind::Sector,
            arc_radius: 30.0,
            arc_start: 0.0,
            arc_sweep: 120.0,
            names: true,
            measures: true,
            angles: true,
            center: true,
            radius_line: true,
            diameter_line: false,
            heights: false,
            medians: false,
            bisectors: false,
            incircle: false,
            circumcircle: false,
            diagonals: false,
            foci: false,
            unit: MeasureUnit::Mm,
            decimals: 1,
            color: [0x1A, 0x1A, 0x1A],
            width: 0.4,
            fill: false,
            fill_color: [0xC9, 0xDD, 0xF6],
            aid_color: [0xD1, 0x2F, 0x2F],
            label_pt: 8.0,
            font: "Arial".into(),
        }
    }
}

/// Everything the panel keeps.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct MathSettings {
    pub mode: Mode,
    pub plane: Plane,
    pub plots: Vec<Plot>,
    /// The parameters a, b, c, d and k the formulas may use.
    pub params: [f64; 5],
    /// Trigonometry (and θ ranges) in degrees.
    pub degrees: bool,
    /// Turn sampled curves into smooth Bezier curves with few nodes.
    pub smooth: bool,
    pub figure: Figure,
}

impl Default for MathSettings {
    fn default() -> Self {
        let mut s = Self {
            mode: Mode::Graph,
            plane: Plane::default(),
            plots: Vec::new(),
            params: [1.0, 1.0, 1.0, 1.0, 3.0],
            degrees: false,
            smooth: true,
            figure: Figure::default(),
        };
        apply_preset(&mut s, Preset::Parabola);
        s
    }
}

fn finite_or(v: f64, d: f64) -> f64 {
    if v.is_finite() {
        v
    } else {
        d
    }
}

impl MathSettings {
    /// Every value in range (from a group's data or the preferences).
    pub fn sanitized(&self) -> MathSettings {
        let mut s = self.clone();
        let d = Plane::default();
        let p = &mut s.plane;
        p.x_min = finite_or(p.x_min, d.x_min).clamp(-1e6, 1e6);
        p.x_max = finite_or(p.x_max, d.x_max).clamp(-1e6, 1e6);
        p.y_min = finite_or(p.y_min, d.y_min).clamp(-1e6, 1e6);
        p.y_max = finite_or(p.y_max, d.y_max).clamp(-1e6, 1e6);
        if p.x_max - p.x_min < 1e-9 {
            p.x_max = p.x_min + 1.0;
        }
        if p.y_max - p.y_min < 1e-9 {
            p.y_max = p.y_min + 1.0;
        }
        p.unit_x = finite_or(p.unit_x, 10.0).clamp(1e-4, 1e4);
        p.unit_y = finite_or(p.unit_y, 10.0).clamp(1e-4, 1e4);
        // The plane itself at most 5 m across.
        let w = (p.x_max - p.x_min) * p.unit_x;
        if w > 5000.0 {
            p.unit_x *= 5000.0 / w;
        }
        let h = (p.y_max - p.y_min) * p.unit_y;
        if h > 5000.0 {
            p.unit_y *= 5000.0 / h;
        }
        for v in [
            &mut p.axis_width,
            &mut p.grid_width,
            &mut p.minor_width,
            &mut p.tick_len,
        ] {
            *v = finite_or(*v, 0.2).clamp(0.0, 20.0);
        }
        for v in [&mut p.grid_x, &mut p.grid_y, &mut p.tick_x, &mut p.tick_y] {
            *v = finite_or(*v, 1.0).abs().max(1e-9);
        }
        p.minor_div = p.minor_div.clamp(2, 20);
        p.number_pt = finite_or(p.number_pt, 7.0).clamp(2.0, 72.0);
        if p.font.trim().is_empty() {
            p.font = d.font.clone();
        }
        s.plots.truncate(MAX_PLOTS);
        for pl in &mut s.plots {
            pl.t_min = finite_or(pl.t_min, 0.0).clamp(-1e6, 1e6);
            pl.t_max = finite_or(pl.t_max, TAU).clamp(-1e6, 1e6);
            pl.d_min = finite_or(pl.d_min, -1.0).clamp(-1e6, 1e6);
            pl.d_max = finite_or(pl.d_max, 1.0).clamp(-1e6, 1e6);
            pl.samples = pl.samples.clamp(20, 5000);
            pl.width = finite_or(pl.width, 0.5).clamp(0.0, 20.0);
            pl.f.truncate(2000);
            pl.g.truncate(2000);
        }
        for v in &mut s.params {
            *v = finite_or(*v, 1.0).clamp(-1e6, 1e6);
        }
        let f = &mut s.figure;
        let df = Figure::default();
        for (v, dv) in [
            (&mut f.circle_value, df.circle_value),
            (&mut f.side_a, df.side_a),
            (&mut f.side_b, df.side_b),
            (&mut f.side_c, df.side_c),
            (&mut f.poly_value, df.poly_value),
            (&mut f.rx, df.rx),
            (&mut f.ry, df.ry),
            (&mut f.arc_radius, df.arc_radius),
        ] {
            *v = finite_or(*v, dv).clamp(0.01, 1e5);
        }
        f.angle_a = finite_or(f.angle_a, 60.0).clamp(0.01, 179.99);
        f.angle_b = finite_or(f.angle_b, 50.0).clamp(0.01, 179.99);
        f.rotation = finite_or(f.rotation, 0.0).clamp(-3600.0, 3600.0);
        f.arc_start = finite_or(f.arc_start, 0.0).clamp(-3600.0, 3600.0);
        f.arc_sweep = finite_or(f.arc_sweep, 120.0).clamp(0.1, 360.0);
        f.sides = f.sides.clamp(3, 200);
        f.star = f.star.clamp(1, f.sides.saturating_sub(1).max(1) / 2 + 1);
        f.decimals = f.decimals.min(4);
        f.width = finite_or(f.width, 0.4).clamp(0.0, 20.0);
        f.label_pt = finite_or(f.label_pt, 8.0).clamp(2.0, 72.0);
        if f.font.trim().is_empty() {
            f.font = df.font;
        }
        s
    }

    /// The variables with the parameters set.
    pub fn vars(&self) -> Vars {
        let mut v = [0.0; math_expr::SLOTS];
        v[math_expr::A] = self.params[0];
        v[math_expr::B] = self.params[1];
        v[math_expr::C] = self.params[2];
        v[math_expr::D] = self.params[3];
        v[math_expr::K] = self.params[4];
        v
    }
}

// ----- output -------------------------------------------------------------

/// Which part of the drawing an item belongs to (each becomes a group of
/// its own, so that the grid or a curve is easy to pick).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Grid,
    Axes,
    Numbers,
    Plot(usize),
    Figure,
    Aids,
    Labels,
}

/// The outline of an item.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub color: Rgb,
    pub width: f64,
    pub dash: Dash,
}

/// One curve with its paint.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub part: Part,
    pub path: BezPath,
    pub closed: bool,
    pub line: Option<Line>,
    pub fill: Option<Rgb>,
}

/// Where a label's anchor sits on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    /// The anchor at the middle of the left edge.
    Left,
    Center,
    Right,
}

/// A text: its anchor point is at the middle of the letters' height.
#[derive(Debug, Clone, PartialEq)]
pub struct Label {
    pub part: Part,
    pub text: String,
    pub at: Point,
    pub anchor: Anchor,
    pub size_pt: f64,
    pub color: Rgb,
    pub font: String,
}

/// The drawing.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Drawn {
    pub items: Vec<Item>,
    pub labels: Vec<Label>,
    /// The plane's rectangle (graph mode with the plane shown).
    pub frame: Option<Rect>,
    /// Formulas that could not be read: (curve, error).
    pub errors: Vec<(usize, ExprError)>,
    /// The figure could not be built (a triangle that cannot close).
    pub impossible: bool,
}

/// Height of a capital in mm for a size in points (about 0.7 em).
pub fn em(pt: f64) -> f64 {
    pt * 25.4 / 72.0
}

/// About how wide a label is (0.55 em a character).
pub fn label_width(text: &str, pt: f64) -> f64 {
    text.chars().count() as f64 * 0.55 * em(pt)
}

impl Drawn {
    /// The bounds of everything drawn, labels included.
    pub fn bounds(&self) -> Option<Rect> {
        let mut b: Option<Rect> = self.frame;
        let mut add = |r: Rect| {
            b = Some(match b {
                Some(o) => o.union(r),
                None => r,
            });
        };
        for it in &self.items {
            if it.path.elements().is_empty() {
                continue;
            }
            let pad = it.line.as_ref().map_or(0.0, |l| l.width / 2.0);
            add(kurbo::Shape::bounding_box(&it.path).inflate(pad, pad));
        }
        for l in &self.labels {
            let w = label_width(&l.text, l.size_pt);
            let h = em(l.size_pt) * 0.7;
            let x0 = match l.anchor {
                Anchor::Left => l.at.x,
                Anchor::Center => l.at.x - w / 2.0,
                Anchor::Right => l.at.x - w,
            };
            add(Rect::new(x0, l.at.y - h / 2.0, x0 + w, l.at.y + h / 2.0));
        }
        b
    }
}

/// The drawing of `s`.
pub fn draw(s: &MathSettings) -> Drawn {
    let s = s.sanitized();
    let mut out = Drawn::default();
    match s.mode {
        Mode::Graph => draw_graph(&s, &mut out),
        Mode::Figure => draw_figure(&s.figure, &mut out),
    }
    out
}

/// Where a closed sampled curve first comes back to its start: the index
/// of that point, so that a curve drawn round twice (a rose of an odd
/// number of petals over a full turn) fills once; the last index when it
/// goes round once, `None` when it never closes. A point counts as back
/// when it is nearer the start than the longer of 0.05 mm and one and a
/// half of the step that reached it, and the rest of the curve has to run
/// over the first round (a figure eight crossing its start keeps both
/// loops).
pub(crate) fn first_return(run: &[Point]) -> Option<usize> {
    let n = run.len();
    if n < 4 {
        return None;
    }
    let start = run[0];
    let near = |j: usize| {
        let step = run[j].distance(run[j - 1]);
        (step, run[j].distance(start) <= (1.5 * step).max(0.05))
    };
    if !near(n - 1).1 {
        return None;
    }
    let mut away = 0.0f64;
    for j in 1..n - 1 {
        let (step, back) = near(j);
        if back && away > 4.0 * step.max(0.05) {
            let rest = &run[j..];
            let tol = run[..=j]
                .windows(2)
                .map(|w| w[0].distance(w[1]))
                .fold(0.05f64, f64::max)
                * 2.0;
            let probes = (1..16).map(|q| run[q * j / 16]);
            let retraced = probes
                .into_iter()
                .all(|p| rest.iter().any(|r| r.distance(p) <= tol));
            if retraced {
                return Some(j);
            }
            return Some(n - 1);
        }
        away = away.max(run[j].distance(start));
    }
    Some(n - 1)
}

fn polyline(pts: &[Point], closed: bool) -> BezPath {
    let mut p = BezPath::new();
    if let Some(first) = pts.first() {
        p.move_to(*first);
        for q in &pts[1..] {
            p.line_to(*q);
        }
        if closed {
            p.close_path();
        }
    }
    p
}

fn segment(a: Point, b: Point) -> BezPath {
    polyline(&[a, b], false)
}

fn circle_path(c: Point, r: f64) -> BezPath {
    kurbo::Shape::to_path(&kurbo::Circle::new(c, r), 0.01)
}

/// A circular arc from angle `a0` turning `sweep` (radians), as curves.
fn arc_path(c: Point, r: f64, a0: f64, sweep: f64) -> BezPath {
    let arc = kurbo::Arc::new(c, (r, r), a0, sweep, 0.0);
    let mut p = BezPath::new();
    p.move_to(c + Vec2::new(r * a0.cos(), r * a0.sin()));
    arc.to_cubic_beziers(0.01, |p1, p2, p3| p.curve_to(p1, p2, p3));
    p
}

fn stroke(color: Rgb, width: f64, dash: Dash) -> Option<Line> {
    Some(Line { color, width, dash })
}

/// A number written short: at most `dec` decimals, no trailing zeros, no
/// "-0".
pub fn fmt_num(v: f64, dec: u32) -> String {
    if !v.is_finite() {
        return "?".into();
    }
    let mut s = format!("{:.*}", dec as usize, v);
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    if s == "-0" {
        s = "0".into();
    }
    s
}

/// `v` as a multiple of π with a small denominator ("π/2", "-3π/4",
/// "2π"), or the plain number when it is not one.
pub fn fmt_pi(v: f64) -> String {
    let q = v / PI;
    for den in [1i64, 2, 3, 4, 6, 8, 12] {
        let num = q * den as f64;
        if (num - num.round()).abs() < 1e-6 {
            let n = num.round() as i64;
            if n == 0 {
                return "0".into();
            }
            let g = gcd(n.unsigned_abs(), den as u64) as i64;
            let (n, d) = (n / g, den / g);
            let sign = if n < 0 { "-" } else { "" };
            let top = match n.abs() {
                1 => "π".to_string(),
                m => format!("{m}π"),
            };
            return if d == 1 {
                format!("{sign}{top}")
            } else {
                format!("{sign}{top}/{d}")
            };
        }
    }
    fmt_num(v, 2)
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 {
        a.max(1)
    } else {
        gcd(b, a % b)
    }
}

/// The multiples of `step` from `lo` to `hi` (with a little slack), at
/// most `MAX_LINES` (the step grows when there would be more).
fn steps(lo: f64, hi: f64, step: f64) -> Vec<f64> {
    let mut step = step.abs().max(1e-12);
    while (hi - lo) / step > MAX_LINES as f64 {
        step *= 2.0;
    }
    let eps = step * 1e-6;
    let first = ((lo - eps) / step).ceil() as i64;
    let last = ((hi + eps) / step).floor() as i64;
    (first..=last)
        .map(|k| {
            let v = k as f64 * step;
            // Snap away float noise: 0.30000000000000004 -> 0.3.
            if v.abs() < eps {
                0.0
            } else {
                (v * 1e9).round() / 1e9
            }
        })
        .collect()
}

/// Maps plane units to millimetres.
#[derive(Debug, Clone, Copy)]
pub struct Map {
    pub x_min: f64,
    pub y_min: f64,
    pub ux: f64,
    pub uy: f64,
}

impl Map {
    pub fn of(p: &Plane) -> Map {
        Map {
            x_min: p.x_min,
            y_min: p.y_min,
            ux: p.unit_x,
            uy: p.unit_y,
        }
    }
    pub fn pt(&self, x: f64, y: f64) -> Point {
        Point::new((x - self.x_min) * self.ux, (y - self.y_min) * self.uy)
    }
    pub fn x(&self, x: f64) -> f64 {
        (x - self.x_min) * self.ux
    }
    pub fn y(&self, y: f64) -> f64 {
        (y - self.y_min) * self.uy
    }
}

/// The plane's rectangle in mm.
pub fn plane_rect(p: &Plane) -> Rect {
    let m = Map::of(p);
    Rect::new(0.0, 0.0, m.x(p.x_max), m.y(p.y_max))
}

/// Where the axes cross: 0 when it is in range, else the nearer edge.
fn axis_at(lo: f64, hi: f64) -> f64 {
    if lo <= 0.0 && hi >= 0.0 {
        0.0
    } else if lo > 0.0 {
        lo
    } else {
        hi
    }
}

fn draw_plane(p: &Plane, out: &mut Drawn) {
    let m = Map::of(p);
    let r = plane_rect(p);
    out.frame = Some(r);
    if p.grid {
        if p.minor {
            let mut path = BezPath::new();
            let (sx, sy) = (p.grid_x / p.minor_div as f64, p.grid_y / p.minor_div as f64);
            let on_major = |v: f64, step: f64| {
                let k = v / step;
                (k - k.round()).abs() < 1e-6
            };
            for x in steps(p.x_min, p.x_max, sx) {
                if !on_major(x, p.grid_x) {
                    path.extend(segment(m.pt(x, p.y_min), m.pt(x, p.y_max)));
                }
            }
            for y in steps(p.y_min, p.y_max, sy) {
                if !on_major(y, p.grid_y) {
                    path.extend(segment(m.pt(p.x_min, y), m.pt(p.x_max, y)));
                }
            }
            if !path.elements().is_empty() {
                out.items.push(Item {
                    part: Part::Grid,
                    path,
                    closed: false,
                    line: stroke(p.minor_color, p.minor_width, Dash::Solid),
                    fill: None,
                });
            }
        }
        let mut path = BezPath::new();
        for x in steps(p.x_min, p.x_max, p.grid_x) {
            path.extend(segment(m.pt(x, p.y_min), m.pt(x, p.y_max)));
        }
        for y in steps(p.y_min, p.y_max, p.grid_y) {
            path.extend(segment(m.pt(p.x_min, y), m.pt(p.x_max, y)));
        }
        if !path.elements().is_empty() {
            out.items.push(Item {
                part: Part::Grid,
                path,
                closed: false,
                line: stroke(p.grid_color, p.grid_width, Dash::Solid),
                fill: None,
            });
        }
    }
    if p.border {
        out.items.push(Item {
            part: Part::Grid,
            path: polyline(
                &[
                    Point::new(r.x0, r.y0),
                    Point::new(r.x1, r.y0),
                    Point::new(r.x1, r.y1),
                    Point::new(r.x0, r.y1),
                ],
                true,
            ),
            closed: true,
            line: stroke(p.axis_color, p.axis_width, Dash::Solid),
            fill: None,
        });
    }
    let ax = axis_at(p.x_min, p.x_max);
    let ay = axis_at(p.y_min, p.y_max);
    let num_em = em(p.number_pt);
    let head = (p.axis_width * 6.0).clamp(1.8, 6.0);
    let over = if p.arrows { head * 1.2 } else { 0.0 };
    if p.axes {
        let line = stroke(p.axis_color, p.axis_width, Dash::Solid);
        let y0 = m.y(ay);
        let x0 = m.x(ax);
        out.items.push(Item {
            part: Part::Axes,
            path: segment(Point::new(r.x0, y0), Point::new(r.x1 + over - head * 0.8, y0)),
            closed: false,
            line: line.clone(),
            fill: None,
        });
        out.items.push(Item {
            part: Part::Axes,
            path: segment(Point::new(x0, r.y0), Point::new(x0, r.y1 + over - head * 0.8)),
            closed: false,
            line: line.clone(),
            fill: None,
        });
        if p.arrows {
            let w = head * 0.42;
            let tip_x = Point::new(r.x1 + over, y0);
            let tip_y = Point::new(x0, r.y1 + over);
            for pts in [
                [
                    tip_x,
                    Point::new(tip_x.x - head, y0 + w),
                    Point::new(tip_x.x - head * 0.8, y0),
                    Point::new(tip_x.x - head, y0 - w),
                ],
                [
                    tip_y,
                    Point::new(x0 - w, tip_y.y - head),
                    Point::new(x0, tip_y.y - head * 0.8),
                    Point::new(x0 + w, tip_y.y - head),
                ],
            ] {
                out.items.push(Item {
                    part: Part::Axes,
                    path: polyline(&pts, true),
                    closed: true,
                    line: None,
                    fill: Some(p.axis_color),
                });
            }
        }
        if p.ticks && p.tick_len > 0.0 {
            let mut path = BezPath::new();
            let t = p.tick_len / 2.0;
            for x in steps(p.x_min, p.x_max, p.tick_x) {
                if (x - ax).abs() > 1e-9 {
                    let px = m.x(x);
                    path.extend(segment(Point::new(px, y0 - t), Point::new(px, y0 + t)));
                }
            }
            for y in steps(p.y_min, p.y_max, p.tick_y) {
                if (y - ay).abs() > 1e-9 {
                    let py = m.y(y);
                    path.extend(segment(Point::new(x0 - t, py), Point::new(x0 + t, py)));
                }
            }
            if !path.elements().is_empty() {
                out.items.push(Item {
                    part: Part::Axes,
                    path,
                    closed: false,
                    line: line.clone(),
                    fill: None,
                });
            }
        }
        if p.axis_names {
            let pt = p.number_pt * 1.25;
            out.labels.push(Label {
                part: Part::Axes,
                text: "x".into(),
                at: Point::new(r.x1 + over + em(pt) * 0.25, y0 - em(pt) * 0.55),
                anchor: Anchor::Left,
                size_pt: pt,
                color: p.axis_color,
                font: p.font.clone(),
            });
            out.labels.push(Label {
                part: Part::Axes,
                text: "y".into(),
                at: Point::new(x0 - em(pt) * 0.5, r.y1 + over + em(pt) * 0.1),
                anchor: Anchor::Right,
                size_pt: pt,
                color: p.axis_color,
                font: p.font.clone(),
            });
        }
    }
    if p.numbers {
        let y0 = m.y(ay);
        let x0 = m.x(ax);
        let gap = p.tick_len / 2.0 + num_em * 0.25;
        let decimals = |step: f64| -> u32 {
            let mut d = 0;
            let mut s = step.abs();
            while d < 4 && (s - s.round()).abs() > 1e-6 {
                s *= 10.0;
                d += 1;
            }
            d
        };
        let (dx, dy) = (decimals(p.tick_x), decimals(p.tick_y));
        for x in steps(p.x_min, p.x_max, p.tick_x) {
            if (x - ax).abs() < 1e-9 || (p.arrows && p.axes && (x - p.x_max).abs() < 1e-9) {
                continue;
            }
            let text = if p.pi_x { fmt_pi(x) } else { fmt_num(x, dx) };
            out.labels.push(Label {
                part: Part::Numbers,
                text,
                at: Point::new(m.x(x), y0 - gap - num_em * 0.45),
                anchor: Anchor::Center,
                size_pt: p.number_pt,
                color: p.number_color,
                font: p.font.clone(),
            });
        }
        for y in steps(p.y_min, p.y_max, p.tick_y) {
            if (y - ay).abs() < 1e-9 || (p.arrows && p.axes && (y - p.y_max).abs() < 1e-9) {
                continue;
            }
            out.labels.push(Label {
                part: Part::Numbers,
                text: fmt_num(y, dy),
                at: Point::new(x0 - gap - num_em * 0.1, m.y(y)),
                anchor: Anchor::Right,
                size_pt: p.number_pt,
                color: p.number_color,
                font: p.font.clone(),
            });
        }
        if p.origin && ax == 0.0 && ay == 0.0 {
            out.labels.push(Label {
                part: Part::Numbers,
                text: "0".into(),
                at: Point::new(x0 - gap - num_em * 0.1, y0 - gap - num_em * 0.45),
                anchor: Anchor::Right,
                size_pt: p.number_pt,
                color: p.number_color,
                font: p.font.clone(),
            });
        }
    }
}

// ----- sampling -----------------------------------------------------------

/// Sample a curve `f(t)` (in mm; None where it is not defined) over
/// [t0, t1]: `n` even steps, each split in two again (up to ten times)
/// where the curve strays more than `tol` mm from the chord. The curve is
/// broken where it is not defined and where a step jumps further than
/// `jump` mm however fine it is cut (an asymptote).
pub fn sample(
    f: &dyn Fn(f64) -> Option<Point>,
    t0: f64,
    t1: f64,
    n: u32,
    tol: f64,
    jump: f64,
) -> Vec<Vec<Point>> {
    struct S<'a> {
        f: &'a dyn Fn(f64) -> Option<Point>,
        tol: f64,
        jump: f64,
        runs: Vec<Vec<Point>>,
        count: usize,
    }
    impl S<'_> {
        fn push(&mut self, p: Point) {
            if self.count >= MAX_POINTS {
                return;
            }
            self.count += 1;
            match self.runs.last_mut() {
                Some(r) => r.push(p),
                None => self.runs.push(vec![p]),
            }
        }
        fn brk(&mut self) {
            if self.runs.last().is_some_and(|r| !r.is_empty()) {
                self.runs.push(Vec::new());
            }
        }
        fn refine(&mut self, t0: f64, p0: Option<Point>, t1: f64, p1: Option<Point>, depth: u32) {
            if self.count >= MAX_POINTS {
                return;
            }
            let tm = (t0 + t1) / 2.0;
            let deep = depth >= 10;
            match (p0, p1) {
                (Some(a), Some(b)) => {
                    if deep {
                        if a.distance(b) > self.jump {
                            self.brk();
                        }
                        self.push(b);
                        return;
                    }
                    let pm = (self.f)(tm);
                    let bend = match pm {
                        Some(m) => {
                            let mid = a.midpoint(b);
                            m.distance(mid) > self.tol
                        }
                        None => true,
                    };
                    if bend || a.distance(b) > self.jump {
                        self.refine(t0, p0, tm, pm, depth + 1);
                        self.refine(tm, pm, t1, p1, depth + 1);
                    } else {
                        self.push(b);
                    }
                }
                (None, None) => {
                    if depth < 3 {
                        let pm = (self.f)(tm);
                        if pm.is_some() {
                            self.refine(t0, p0, tm, pm, depth + 1);
                            self.refine(tm, pm, t1, p1, depth + 1);
                        }
                    }
                }
                (Some(_), None) | (None, Some(_)) => {
                    if deep {
                        if let Some(b) = p1 {
                            self.brk();
                            self.push(b);
                        } else {
                            self.brk();
                        }
                        return;
                    }
                    let pm = (self.f)(tm);
                    self.refine(t0, p0, tm, pm, depth + 1);
                    self.refine(tm, pm, t1, p1, depth + 1);
                }
            }
        }
    }
    let mut s = S {
        f,
        tol: tol.max(1e-4),
        jump: jump.max(tol),
        runs: Vec::new(),
        count: 0,
    };
    if !(t0.is_finite() && t1.is_finite()) || t0 == t1 {
        return Vec::new();
    }
    let n = n.max(2);
    let mut prev_t = t0;
    let mut prev = f(t0);
    if let Some(p) = prev {
        s.push(p);
    }
    for i in 1..=n {
        let t = t0 + (t1 - t0) * i as f64 / n as f64;
        let cur = f(t);
        if prev.is_none() && cur.is_some() && i == 1 {
            // Started undefined: refine finds the start.
        }
        s.refine(prev_t, prev, t, cur, 0);
        prev_t = t;
        prev = cur;
    }
    s.runs.into_iter().filter(|r| r.len() >= 2).collect()
}

/// Cut polyline `run` to `r`: the parts inside, with points on the edge
/// where it crosses (Liang-Barsky on each segment).
pub fn clip(run: &[Point], r: Rect) -> Vec<Vec<Point>> {
    let mut out: Vec<Vec<Point>> = Vec::new();
    let mut cur: Vec<Point> = Vec::new();
    let inside = |p: Point| p.x >= r.x0 && p.x <= r.x1 && p.y >= r.y0 && p.y <= r.y1;
    for w in run.windows(2) {
        let (a, b) = (w[0], w[1]);
        let d = b - a;
        let mut t0 = 0.0f64;
        let mut t1 = 1.0f64;
        let mut ok = true;
        for (p, q) in [
            (-d.x, a.x - r.x0),
            (d.x, r.x1 - a.x),
            (-d.y, a.y - r.y0),
            (d.y, r.y1 - a.y),
        ] {
            if p == 0.0 {
                if q < 0.0 {
                    ok = false;
                    break;
                }
            } else {
                let t = q / p;
                if p < 0.0 {
                    t0 = t0.max(t);
                } else {
                    t1 = t1.min(t);
                }
            }
        }
        if !ok || t0 > t1 {
            if cur.len() >= 2 {
                out.push(std::mem::take(&mut cur));
            }
            cur.clear();
            continue;
        }
        let pa = a + d * t0;
        let pb = a + d * t1;
        if cur.is_empty() || t0 > 0.0 {
            if cur.len() >= 2 {
                out.push(std::mem::take(&mut cur));
            }
            cur.clear();
            cur.push(pa);
        }
        cur.push(pb);
        if t1 < 1.0 || !inside(b) {
            if cur.len() >= 2 {
                out.push(std::mem::take(&mut cur));
            }
            cur.clear();
        }
    }
    if cur.len() >= 2 {
        out.push(cur);
    }
    out
}

/// The zero lines of `f(x, y)` over the plane by marching squares, as
/// joined polylines in mm. Sign changes across a pole (where |f| grows
/// towards the middle of the cell edge) are skipped.
pub fn contour(
    f: &dyn Fn(f64, f64) -> f64,
    p: &Plane,
    cells: usize,
) -> Vec<Vec<Point>> {
    let m = Map::of(p);
    let w = (p.x_max - p.x_min) * p.unit_x;
    let h = (p.y_max - p.y_min) * p.unit_y;
    let (nx, ny) = if w >= h {
        (cells, ((cells as f64 * h / w).round() as usize).max(8))
    } else {
        (((cells as f64 * w / h).round() as usize).max(8), cells)
    };
    let xs: Vec<f64> = (0..=nx)
        .map(|i| p.x_min + (p.x_max - p.x_min) * i as f64 / nx as f64)
        .collect();
    let ys: Vec<f64> = (0..=ny)
        .map(|j| p.y_min + (p.y_max - p.y_min) * j as f64 / ny as f64)
        .collect();
    let mut v = vec![0.0f64; (nx + 1) * (ny + 1)];
    for j in 0..=ny {
        for i in 0..=nx {
            v[j * (nx + 1) + i] = f(xs[i], ys[j]);
        }
    }
    let at = |i: usize, j: usize| v[j * (nx + 1) + i];
    // Edge ids: horizontal edge (i, j)-(i+1, j) = 2 (j (nx+1) + i),
    // vertical edge (i, j)-(i, j+1) = 2 (j (nx+1) + i) + 1.
    let hid = |i: usize, j: usize| 2 * (j * (nx + 1) + i);
    let vid = |i: usize, j: usize| 2 * (j * (nx + 1) + i) + 1;
    let mut point_of: std::collections::HashMap<usize, Point> = std::collections::HashMap::new();
    let crossing = |a: f64, b: f64| a.is_finite() && b.is_finite() && ((a < 0.0) != (b < 0.0));
    let mut edge_point = |id: usize, x0: f64, y0: f64, x1: f64, y1: f64, a: f64, b: f64| -> Option<usize> {
        if !crossing(a, b) {
            return None;
        }
        if let std::collections::hash_map::Entry::Vacant(e) = point_of.entry(id) {
            let t = (a / (a - b)).clamp(0.0, 1.0);
            let (x, y) = (x0 + (x1 - x0) * t, y0 + (y1 - y0) * t);
            let mid = f((x0 + x1) / 2.0, (y0 + y1) / 2.0);
            if !mid.is_finite() || mid.abs() > a.abs().max(b.abs()) * 1.5 {
                return None;
            }
            e.insert(m.pt(x, y));
        }
        Some(id)
    };
    let mut segs: Vec<(usize, usize)> = Vec::new();
    for j in 0..ny {
        for i in 0..nx {
            let (v00, v10, v11, v01) = (at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1));
            let bottom = edge_point(hid(i, j), xs[i], ys[j], xs[i + 1], ys[j], v00, v10);
            let right = edge_point(vid(i + 1, j), xs[i + 1], ys[j], xs[i + 1], ys[j + 1], v10, v11);
            let top = edge_point(hid(i, j + 1), xs[i], ys[j + 1], xs[i + 1], ys[j + 1], v01, v11);
            let left = edge_point(vid(i, j), xs[i], ys[j], xs[i], ys[j + 1], v00, v01);
            let es: Vec<usize> = [bottom, right, top, left].into_iter().flatten().collect();
            match es.len() {
                2 => segs.push((es[0], es[1])),
                4 => {
                    // A saddle: the centre's sign picks the pairing.
                    let c = f((xs[i] + xs[i + 1]) / 2.0, (ys[j] + ys[j + 1]) / 2.0);
                    let (b, r, t, l) = (es[0], es[1], es[2], es[3]);
                    if (c < 0.0) == (v00 < 0.0) {
                        segs.push((b, r));
                        segs.push((t, l));
                    } else {
                        segs.push((b, l));
                        segs.push((r, t));
                    }
                }
                _ => {}
            }
        }
    }
    // Join the segments into polylines.
    let mut by_edge: std::collections::HashMap<usize, Vec<usize>> = std::collections::HashMap::new();
    for (k, &(a, b)) in segs.iter().enumerate() {
        by_edge.entry(a).or_default().push(k);
        by_edge.entry(b).or_default().push(k);
    }
    let mut used = vec![false; segs.len()];
    let mut runs = Vec::new();
    let next_from = |e: usize, used: &Vec<bool>| -> Option<usize> {
        by_edge.get(&e)?.iter().copied().find(|&k| !used[k])
    };
    for start in 0..segs.len() {
        if used[start] {
            continue;
        }
        used[start] = true;
        let (a, b) = segs[start];
        let mut chain: std::collections::VecDeque<usize> = std::collections::VecDeque::from(vec![a, b]);
        // Forward from b.
        let mut end = b;
        while let Some(k) = next_from(end, &used) {
            used[k] = true;
            let (p, q) = segs[k];
            end = if p == end { q } else { p };
            chain.push_back(end);
        }
        let mut end = a;
        while let Some(k) = next_from(end, &used) {
            used[k] = true;
            let (p, q) = segs[k];
            end = if p == end { q } else { p };
            chain.push_front(end);
        }
        let pts: Vec<Point> = chain.iter().filter_map(|e| point_of.get(e).copied()).collect();
        if pts.len() >= 2 {
            runs.push(pts);
        }
    }
    runs
}

/// Points typed as `A(2, 3); B(-1; 4)`, `(1, 2) (3, 4)` or `2 3; 4 5`:
/// (name, x, y). Formulas work in the coordinates (`(pi, sqrt(2))`).
pub fn parse_points(src: &str, vars: &Vars, degrees: bool) -> Vec<(String, f64, f64)> {
    let mut out = Vec::new();
    let mut rest = src;
    let eval = |t: &str| -> Option<f64> {
        let v = Expr::parse(t).ok()?.eval(vars, degrees);
        v.is_finite().then_some(v)
    };
    while let Some(open) = rest.find('(') {
        let name: String = rest[..open]
            .trim()
            .trim_start_matches([';', ','])
            .trim()
            .chars()
            .rev()
            .take_while(|c| c.is_alphanumeric() || *c == '\'' || *c == '_')
            .collect::<Vec<char>>()
            .into_iter()
            .rev()
            .collect();
        let Some(close_rel) = rest[open..].find(')') else {
            break;
        };
        // Nested brackets inside a coordinate: find the matching one.
        let mut depth = 0i32;
        let mut close = open + close_rel;
        for (k, c) in rest[open..].char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        close = open + k;
                        break;
                    }
                }
                _ => {}
            }
        }
        let inner = &rest[open + 1..close];
        // Split at the top-level comma or semicolon.
        let mut d = 0i32;
        let mut cut = None;
        for (k, c) in inner.char_indices() {
            match c {
                '(' => d += 1,
                ')' => d -= 1,
                ',' | ';' if d == 0 => {
                    cut = Some(k);
                    break;
                }
                _ => {}
            }
        }
        if let Some(k) = cut {
            if let (Some(x), Some(y)) = (eval(&inner[..k]), eval(&inner[k + 1..])) {
                out.push((name, x, y));
            }
        }
        rest = &rest[close + 1..];
        if out.len() >= 500 {
            break;
        }
    }
    out
}

/// The zeros, y intercept and turning points of `f` on [x0, x1].
pub fn function_marks(f: &dyn Fn(f64) -> f64, x0: f64, x1: f64) -> Vec<(f64, f64)> {
    let n = 2000;
    let mut out: Vec<(f64, f64)> = Vec::new();
    let xs: Vec<f64> = (0..=n).map(|i| x0 + (x1 - x0) * i as f64 / n as f64).collect();
    let ys: Vec<f64> = xs.iter().map(|&x| f(x)).collect();
    let scale = ys
        .iter()
        .filter(|v| v.is_finite())
        .fold(0.0f64, |a, v| a.max(v.abs()))
        .max(1.0);
    let push = |out: &mut Vec<(f64, f64)>, x: f64, y: f64| {
        let tol = (x1 - x0) * 1e-6;
        if x.is_finite() && y.is_finite() && !out.iter().any(|&(px, py)| (px - x).abs() <= tol && (py - y).abs() <= scale * 1e-9 + 1e-12) {
            out.push((x, y));
        }
    };
    // Zeros by bisection where the sign changes (not across a pole).
    for i in 0..n {
        let (a, b) = (ys[i], ys[i + 1]);
        if !(a.is_finite() && b.is_finite()) {
            continue;
        }
        if a == 0.0 {
            push(&mut out, xs[i], 0.0);
            continue;
        }
        if (a < 0.0) != (b < 0.0) && b != 0.0 {
            let (mut lo, mut hi, mut flo) = (xs[i], xs[i + 1], a);
            for _ in 0..60 {
                let mid = (lo + hi) / 2.0;
                let fm = f(mid);
                if !fm.is_finite() {
                    break;
                }
                if (fm < 0.0) == (flo < 0.0) {
                    lo = mid;
                    flo = fm;
                } else {
                    hi = mid;
                }
            }
            let x = (lo + hi) / 2.0;
            if f(x).abs() < scale * 1e-6 {
                push(&mut out, x, 0.0);
            }
        }
    }
    if ys[n] == 0.0 {
        push(&mut out, xs[n], 0.0);
    }
    if x0 <= 0.0 && x1 >= 0.0 {
        let y = f(0.0);
        push(&mut out, 0.0, y);
    }
    // Turning points: the slope changes sign; golden-section search.
    for i in 1..n {
        let (a, b, c) = (ys[i - 1], ys[i], ys[i + 1]);
        if !(a.is_finite() && b.is_finite() && c.is_finite()) {
            continue;
        }
        let is_max = b > a && b >= c;
        let is_min = b < a && b <= c;
        if !(is_max || is_min) {
            continue;
        }
        let (mut lo, mut hi) = (xs[i - 1], xs[i + 1]);
        let g = 0.618_033_988_749_895;
        for _ in 0..80 {
            let m1 = hi - g * (hi - lo);
            let m2 = lo + g * (hi - lo);
            let (f1, f2) = (f(m1), f(m2));
            if (f1 > f2) == is_max {
                hi = m2;
            } else {
                lo = m1;
            }
        }
        let x = (lo + hi) / 2.0;
        push(&mut out, x, f(x));
    }
    out.truncate(60);
    out
}

fn draw_graph(s: &MathSettings, out: &mut Drawn) {
    let p = &s.plane;
    if p.show {
        draw_plane(p, out);
    }
    let m = Map::of(p);
    let r = plane_rect(p);
    let diag = (r.width().powi(2) + r.height().powi(2)).sqrt();
    let base = s.vars();
    let deg = s.degrees;
    let ang = if deg { PI / 180.0 } else { 1.0 };
    for (i, pl) in s.plots.iter().enumerate() {
        if !pl.visible {
            continue;
        }
        let line = stroke(pl.color, pl.width, pl.dash);
        let mut runs: Vec<Vec<Point>>;
        let mut label_at: Option<Point> = None;
        let label_text: String;
        let tol = 0.02;
        match pl.kind {
            PlotKind::Function => {
                let e = match Expr::parse(&pl.f) {
                    Ok(e) => e,
                    Err(err) => {
                        out.errors.push((i, err));
                        continue;
                    }
                };
                let (mut x0, mut x1) = (p.x_min, p.x_max);
                if pl.domain {
                    x0 = x0.max(pl.d_min.min(pl.d_max));
                    x1 = x1.min(pl.d_max.max(pl.d_min));
                    if !p.show {
                        x0 = pl.d_min.min(pl.d_max);
                        x1 = pl.d_max.max(pl.d_min);
                    }
                }
                if x1 <= x0 {
                    continue;
                }
                let fy = |x: f64| {
                    let mut v = base;
                    v[math_expr::X] = x;
                    v[math_expr::T] = x;
                    e.eval(&v, deg)
                };
                let fp = |x: f64| {
                    let y = fy(x);
                    y.is_finite().then(|| m.pt(x, y))
                };
                runs = sample(&fp, x0, x1, pl.samples, tol, diag.max(1.0) * 2.0);
                if p.show {
                    runs = runs.iter().flat_map(|run| clip(run, r)).collect();
                }
                if pl.fill {
                    let y0 = if p.show {
                        m.y(0.0_f64.clamp(p.y_min, p.y_max))
                    } else {
                        m.y(0.0)
                    };
                    for run in &runs {
                        let (Some(a), Some(b)) = (run.first(), run.last()) else {
                            continue;
                        };
                        let mut pts = run.clone();
                        pts.push(Point::new(b.x, y0));
                        pts.push(Point::new(a.x, y0));
                        out.items.push(Item {
                            part: Part::Plot(i),
                            path: polyline(&pts, true),
                            closed: true,
                            line: None,
                            fill: Some(pl.fill_color),
                        });
                    }
                }
                if pl.marks {
                    for (x, y) in function_marks(&fy, x0, x1) {
                        let q = m.pt(x, y);
                        if p.show && !r.inflate(1e-6, 1e-6).contains(q) {
                            continue;
                        }
                        let rad = (pl.width * 1.6).max(0.7);
                        out.items.push(Item {
                            part: Part::Plot(i),
                            path: circle_path(q, rad),
                            closed: true,
                            line: None,
                            fill: Some(pl.color),
                        });
                        let pt = p.number_pt;
                        out.labels.push(Label {
                            part: Part::Plot(i),
                            text: format!("({}; {})", fmt_num(x, 2), fmt_num(y, 2)),
                            at: Point::new(q.x + rad + em(pt) * 0.3, q.y + rad + em(pt) * 0.4),
                            anchor: Anchor::Left,
                            size_pt: pt,
                            color: pl.color,
                            font: p.font.clone(),
                        });
                    }
                }
                label_text = format!("y = {}", pl.f.trim());
            }
            PlotKind::Parametric | PlotKind::Polar => {
                let ex = Expr::parse(&pl.f);
                let ey = if pl.kind == PlotKind::Parametric {
                    Some(Expr::parse(&pl.g))
                } else {
                    None
                };
                let ex = match ex {
                    Ok(e) => e,
                    Err(err) => {
                        out.errors.push((i, err));
                        continue;
                    }
                };
                let ey = match ey {
                    Some(Ok(e)) => Some(e),
                    Some(Err(err)) => {
                        out.errors.push((i, err));
                        continue;
                    }
                    None => None,
                };
                let polar = pl.kind == PlotKind::Polar;
                let fp = |t: f64| {
                    let mut v = base;
                    v[math_expr::T] = t;
                    let (x, y) = if polar {
                        let rr = ex.eval(&v, deg);
                        let th = t * ang;
                        (rr * th.cos(), rr * th.sin())
                    } else {
                        let x = ex.eval(&v, deg);
                        v[math_expr::X] = x;
                        let y = ey.as_ref().map_or(f64::NAN, |e| e.eval(&v, deg));
                        (x, y)
                    };
                    (x.is_finite() && y.is_finite()).then(|| m.pt(x, y))
                };
                runs = sample(&fp, pl.t_min, pl.t_max, pl.samples, tol, diag.max(1.0) * 2.0);
                if p.show {
                    runs = runs.iter().flat_map(|run| clip(run, r)).collect();
                }
                label_text = if polar {
                    format!("r = {}", pl.f.trim())
                } else {
                    format!("({}; {})", pl.f.trim(), pl.g.trim())
                };
            }
            PlotKind::Implicit => {
                let e = match Expr::parse(&pl.f) {
                    Ok(e) => e,
                    Err(err) => {
                        out.errors.push((i, err));
                        continue;
                    }
                };
                let fxy = |x: f64, y: f64| {
                    let mut v = base;
                    v[math_expr::X] = x;
                    v[math_expr::Y] = y;
                    e.eval(&v, deg)
                };
                let cells = (pl.samples as usize / 2).clamp(40, 600);
                runs = contour(&fxy, p, cells);
                label_text = pl.f.trim().to_string();
            }
            PlotKind::Points => {
                let pts = parse_points(&pl.f, &base, deg);
                let rad = (pl.width * 2.0).max(0.8);
                let x_axis = m.y(axis_at(p.y_min, p.y_max));
                let y_axis = m.x(axis_at(p.x_min, p.x_max));
                for (name, x, y) in pts {
                    let q = m.pt(x, y);
                    if p.show && !r.inflate(1e-6, 1e-6).contains(q) {
                        continue;
                    }
                    if pl.marks {
                        let mut g = BezPath::new();
                        g.extend(segment(q, Point::new(q.x, x_axis)));
                        g.extend(segment(q, Point::new(y_axis, q.y)));
                        out.items.push(Item {
                            part: Part::Plot(i),
                            path: g,
                            closed: false,
                            line: stroke(pl.color, (pl.width * 0.5).max(0.1), Dash::Dashed),
                            fill: None,
                        });
                    }
                    out.items.push(Item {
                        part: Part::Plot(i),
                        path: circle_path(q, rad),
                        closed: true,
                        line: None,
                        fill: Some(pl.color),
                    });
                    if pl.label {
                        let pt = p.number_pt * 1.15;
                        let text = if name.is_empty() {
                            format!("({}; {})", fmt_num(x, 2), fmt_num(y, 2))
                        } else {
                            format!("{name}({}; {})", fmt_num(x, 2), fmt_num(y, 2))
                        };
                        out.labels.push(Label {
                            part: Part::Plot(i),
                            text,
                            at: Point::new(q.x + rad + em(pt) * 0.25, q.y + rad + em(pt) * 0.35),
                            anchor: Anchor::Left,
                            size_pt: pt,
                            color: pl.color,
                            font: p.font.clone(),
                        });
                    }
                }
                continue;
            }
        }
        // Closed curves (a polar rose, an implicit circle) fill inside.
        if pl.fill && pl.kind != PlotKind::Function {
            for run in &runs {
                if let Some(end) = first_return(run) {
                    out.items.push(Item {
                        part: Part::Plot(i),
                        path: polyline(&run[..=end], true),
                        closed: true,
                        line: None,
                        fill: Some(pl.fill_color),
                    });
                }
            }
        }
        for run in &runs {
            let closed = run.len() > 3
                && run
                    .first()
                    .zip(run.last())
                    .is_some_and(|(a, b)| a.distance(*b) < 0.05);
            let path = if closed {
                polyline(&run[..run.len() - 1], true)
            } else {
                polyline(run, false)
            };
            out.items.push(Item {
                part: Part::Plot(i),
                path,
                closed,
                line: line.clone(),
                fill: None,
            });
        }
        if pl.label && !label_text.is_empty() {
            // At the end of the longest run, turned outwards.
            if let Some(run) = runs.iter().max_by_key(|r| r.len()) {
                if let Some(q) = run.last() {
                    label_at = Some(*q);
                }
            }
            if let Some(q) = label_at {
                let pt = p.number_pt * 1.15;
                let w = label_width(&label_text, pt);
                let (x, anchor) = if p.show && q.x + w + 1.0 > r.x1 {
                    (q.x - 1.0, Anchor::Right)
                } else {
                    (q.x + 1.0, Anchor::Left)
                };
                let mut y = q.y + em(pt) * 0.6;
                if p.show && y > r.y1 - em(pt) * 0.4 {
                    y = q.y - em(pt) * 0.6;
                }
                out.labels.push(Label {
                    part: Part::Plot(i),
                    text: label_text,
                    at: Point::new(x, y),
                    anchor,
                    size_pt: pt,
                    color: pl.color,
                    font: p.font.clone(),
                });
            }
        }
    }
}

// ----- figures ------------------------------------------------------------

/// A solved triangle: the vertices A, B, C (C above AB), sides a, b, c
/// and angles in degrees.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Triangle {
    pub a: Point,
    pub b: Point,
    pub c: Point,
    pub sides: [f64; 3],
    pub angles: [f64; 3],
}

impl Triangle {
    pub fn area(&self) -> f64 {
        let [a, b, c] = self.sides;
        let s = (a + b + c) / 2.0;
        (s * (s - a) * (s - b) * (s - c)).max(0.0).sqrt()
    }
    pub fn perimeter(&self) -> f64 {
        self.sides.iter().sum()
    }
}

/// The triangle from the figure's measures; None when they cannot close
/// a triangle.
pub fn solve_triangle(f: &Figure) -> Option<Triangle> {
    let deg = PI / 180.0;
    let (a, b, c) = match f.tri_by {
        TriBy::Sss => (f.side_a, f.side_b, f.side_c),
        TriBy::Sas => {
            let (b, c, aa) = (f.side_b, f.side_c, f.angle_a * deg);
            ((b * b + c * c - 2.0 * b * c * aa.cos()).max(0.0).sqrt(), b, c)
        }
        TriBy::Asa => {
            let (aa, bb) = (f.angle_a * deg, f.angle_b * deg);
            let cc = PI - aa - bb;
            if cc <= 1e-9 {
                return None;
            }
            let c = f.side_c;
            (c * aa.sin() / cc.sin(), c * bb.sin() / cc.sin(), c)
        }
        TriBy::Right => (f.side_a, f.side_b, f.side_a.hypot(f.side_b)),
        TriBy::Isosceles => (f.side_a, f.side_a, f.side_c),
        TriBy::Equilateral => (f.side_a, f.side_a, f.side_a),
    };
    if !(a > 0.0 && b > 0.0 && c > 0.0) {
        return None;
    }
    let eps = 1e-9 * (a + b + c);
    if a + b <= c + eps || a + c <= b + eps || b + c <= a + eps {
        return None;
    }
    // A at the origin, B along x, C above.
    let cx = (b * b + c * c - a * a) / (2.0 * c);
    let cy = (b * b - cx * cx).max(0.0).sqrt();
    let pa = Point::ORIGIN;
    let pb = Point::new(c, 0.0);
    let pc = Point::new(cx, cy);
    let ang = |opp: f64, s1: f64, s2: f64| {
        ((s1 * s1 + s2 * s2 - opp * opp) / (2.0 * s1 * s2))
            .clamp(-1.0, 1.0)
            .acos()
            / deg
    };
    Some(Triangle {
        a: pa,
        b: pb,
        c: pc,
        sides: [a, b, c],
        angles: [ang(a, b, c), ang(b, a, c), ang(c, a, b)],
    })
}

/// The circle's radius from what it was given by.
pub fn circle_radius(f: &Figure) -> f64 {
    let v = f.circle_value;
    match f.circle_by {
        CircleBy::Radius => v,
        CircleBy::Diameter => v / 2.0,
        CircleBy::Circumference => v / TAU,
        CircleBy::Area => (v / PI).sqrt(),
    }
}

/// A regular polygon's circumradius.
pub fn polygon_radius(f: &Figure) -> f64 {
    if f.by_side {
        f.poly_value / (2.0 * (PI / f.sides as f64).sin())
    } else {
        f.poly_value
    }
}

/// The facts the panel lists under a figure: (language key, value, kind)
/// where kind 1 is a length, 2 an area, 0 an angle in degrees.
pub fn figure_facts(f: &Figure) -> Vec<(&'static str, f64, u8)> {
    let f = &MathSettings {
        figure: f.clone(),
        ..Default::default()
    }
    .sanitized()
    .figure;
    match f.kind {
        FigureKind::Circle => {
            let r = circle_radius(f);
            vec![
                ("math.fact_radius", r, 1),
                ("math.fact_diameter", 2.0 * r, 1),
                ("math.fact_circumference", TAU * r, 1),
                ("math.fact_area", PI * r * r, 2),
            ]
        }
        FigureKind::Triangle => match solve_triangle(f) {
            Some(t) => vec![
                ("math.fact_side_a", t.sides[0], 1),
                ("math.fact_side_b", t.sides[1], 1),
                ("math.fact_side_c", t.sides[2], 1),
                ("math.fact_angle_a", t.angles[0], 0),
                ("math.fact_angle_b", t.angles[1], 0),
                ("math.fact_angle_c", t.angles[2], 0),
                ("math.fact_perimeter", t.perimeter(), 1),
                ("math.fact_area", t.area(), 2),
            ],
            None => Vec::new(),
        },
        FigureKind::Polygon => {
            let n = f.sides as f64;
            let rr = polygon_radius(f);
            let side = 2.0 * rr * (PI / n).sin();
            let apothem = rr * (PI / n).cos();
            vec![
                ("math.fact_side", side, 1),
                ("math.fact_circumradius", rr, 1),
                ("math.fact_apothem", apothem, 1),
                ("math.fact_interior", 180.0 * (n - 2.0) / n, 0),
                ("math.fact_perimeter", side * n, 1),
                ("math.fact_area", n * side * apothem / 2.0, 2),
            ]
        }
        FigureKind::Ellipse => {
            let (a, b) = (f.rx.max(f.ry), f.rx.min(f.ry));
            let h = ((a - b) / (a + b)).powi(2);
            // Ramanujan's second approximation.
            let per = PI * (a + b) * (1.0 + 3.0 * h / (10.0 + (4.0 - 3.0 * h).sqrt()));
            vec![
                ("math.fact_focal", (a * a - b * b).sqrt(), 1),
                ("math.fact_eccentricity", (1.0 - (b / a).powi(2)).sqrt(), 3),
                ("math.fact_perimeter", per, 1),
                ("math.fact_area", PI * a * b, 2),
            ]
        }
        FigureKind::Arc => {
            let r = f.arc_radius;
            let th = f.arc_sweep * PI / 180.0;
            let chord = 2.0 * r * (th / 2.0).sin();
            let area = match f.arc_kind {
                ArcKind::Arc | ArcKind::Sector => r * r * th / 2.0,
                ArcKind::Segment => r * r * (th - th.sin()) / 2.0,
            };
            vec![
                ("math.fact_arc_length", r * th, 1),
                ("math.fact_chord", chord, 1),
                ("math.fact_area", area, 2),
            ]
        }
    }
}

/// A length written in the figure's unit: "40 mm", "4 cm", "40".
pub fn fmt_len(v: f64, f: &Figure) -> String {
    match f.unit {
        MeasureUnit::Mm => format!("{} mm", fmt_num(v, f.decimals)),
        MeasureUnit::Cm => format!("{} cm", fmt_num(v / 10.0, f.decimals.max(1))),
        MeasureUnit::None => fmt_num(v, f.decimals),
    }
}

/// An area in the figure's unit.
pub fn fmt_area(v: f64, f: &Figure) -> String {
    match f.unit {
        MeasureUnit::Mm => format!("{} mm²", fmt_num(v, f.decimals)),
        MeasureUnit::Cm => format!("{} cm²", fmt_num(v / 100.0, f.decimals.max(1))),
        MeasureUnit::None => fmt_num(v, f.decimals),
    }
}

fn fig_label(f: &Figure, part: Part, text: String, at: Point, anchor: Anchor, color: Rgb) -> Label {
    Label {
        part,
        text,
        at,
        anchor,
        size_pt: f.label_pt,
        color,
        font: f.font.clone(),
    }
}

/// A label placed outside a polygon at `p`, pushed along `dir`.
fn label_out(f: &Figure, part: Part, text: String, p: Point, dir: Vec2, gap: f64, color: Rgb) -> Label {
    let d = if dir.hypot() > 1e-12 {
        dir / dir.hypot()
    } else {
        Vec2::new(0.0, 1.0)
    };
    let w = label_width(&text, f.label_pt);
    let h = em(f.label_pt) * 0.7;
    // Far enough that the text's box clears the point in that direction.
    let reach = gap + (d.x.abs() * w / 2.0).max(d.y.abs() * h / 2.0);
    let at = p + d * reach;
    fig_label(f, part, text, at, Anchor::Center, color)
}

fn draw_figure(f: &Figure, out: &mut Drawn) {
    let shape_line = stroke(f.color, f.width, Dash::Solid);
    let aid = stroke(f.aid_color, (f.width * 0.6).max(0.1), Dash::Dashed);
    let aid_solid = stroke(f.aid_color, (f.width * 0.6).max(0.1), Dash::Solid);
    let gap = em(f.label_pt) * 0.35;
    let dot = (f.width * 1.6).max(0.6);
    let fill = f.fill.then_some(f.fill_color);
    let push = |out: &mut Drawn, part: Part, path: BezPath, closed: bool, line: Option<Line>, fill: Option<Rgb>| {
        out.items.push(Item {
            part,
            path,
            closed,
            line,
            fill,
        })
    };
    match f.kind {
        FigureKind::Circle => {
            let r = circle_radius(f);
            let c = Point::ORIGIN;
            push(out, Part::Figure, circle_path(c, r), true, shape_line.clone(), fill);
            if f.center {
                push(out, Part::Aids, circle_path(c, dot), true, None, Some(f.color));
                if f.names {
                    out.labels.push(label_out(f, Part::Labels, "O".into(), c, Vec2::new(-1.0, -1.0), gap, f.color));
                }
            }
            if f.radius_line {
                let e = Point::new(r * (PI / 6.0).cos(), r * (PI / 6.0).sin());
                push(out, Part::Aids, segment(c, e), false, aid_solid.clone(), None);
                if f.measures {
                    let mid = c.midpoint(e);
                    out.labels.push(label_out(
                        f,
                        Part::Labels,
                        format!("r = {}", fmt_len(r, f)),
                        mid,
                        Vec2::new(-0.5, 0.866),
                        gap,
                        f.aid_color,
                    ));
                }
            }
            if f.diameter_line {
                let (a, b) = (Point::new(-r, 0.0), Point::new(r, 0.0));
                push(out, Part::Aids, segment(a, b), false, aid.clone(), None);
                if f.measures {
                    out.labels.push(label_out(
                        f,
                        Part::Labels,
                        format!("d = {}", fmt_len(2.0 * r, f)),
                        Point::new(-r * 0.5, 0.0),
                        Vec2::new(0.0, -1.0),
                        gap,
                        f.aid_color,
                    ));
                }
            }
            if f.measures && !f.radius_line && !f.diameter_line {
                out.labels.push(fig_label(
                    f,
                    Part::Labels,
                    format!("C = {}", fmt_len(TAU * r, f)),
                    Point::new(0.0, -r - gap - em(f.label_pt) * 0.5),
                    Anchor::Center,
                    f.color,
                ));
            }
        }
        FigureKind::Triangle => {
            let Some(t) = solve_triangle(f) else {
                out.impossible = true;
                return;
            };
            // Centred on its centroid.
            let g = Point::new(
                (t.a.x + t.b.x + t.c.x) / 3.0,
                (t.a.y + t.b.y + t.c.y) / 3.0,
            );
            let sh = |p: Point| p - g.to_vec2();
            let (pa, pb, pc) = (sh(t.a), sh(t.b), sh(t.c));
            push(out, Part::Figure, polyline(&[pa, pb, pc], true), true, shape_line.clone(), fill);
            let verts = [pa, pb, pc];
            let names = ["A", "B", "C"];
            let side_names = ["a", "b", "c"];
            let centre = Point::ORIGIN;
            let [sa, sb, sc] = t.sides;
            let s = (sa + sb + sc) / 2.0;
            let area = t.area();
            if f.incircle {
                let ic = Point::new(
                    (sa * pa.x + sb * pb.x + sc * pc.x) / (2.0 * s),
                    (sa * pa.y + sb * pb.y + sc * pc.y) / (2.0 * s),
                );
                push(out, Part::Aids, circle_path(ic, area / s), true, aid.clone(), None);
                push(out, Part::Aids, circle_path(ic, dot * 0.8), true, None, Some(f.aid_color));
            }
            if f.circumcircle {
                let d = 2.0 * (pa.x * (pb.y - pc.y) + pb.x * (pc.y - pa.y) + pc.x * (pa.y - pb.y));
                if d.abs() > 1e-12 {
                    let n = |p: Point| p.x * p.x + p.y * p.y;
                    let ux = (n(pa) * (pb.y - pc.y) + n(pb) * (pc.y - pa.y) + n(pc) * (pa.y - pb.y)) / d;
                    let uy = (n(pa) * (pc.x - pb.x) + n(pb) * (pa.x - pc.x) + n(pc) * (pb.x - pa.x)) / d;
                    let cc = Point::new(ux, uy);
                    push(out, Part::Aids, circle_path(cc, cc.distance(pa)), true, aid.clone(), None);
                    push(out, Part::Aids, circle_path(cc, dot * 0.8), true, None, Some(f.aid_color));
                }
            }
            for k in 0..3 {
                let v = verts[k];
                let (p1, p2) = (verts[(k + 1) % 3], verts[(k + 2) % 3]);
                if f.heights {
                    let d = p2 - p1;
                    let tt = (v - p1).dot(d) / d.hypot2().max(1e-12);
                    let foot = p1 + d * tt;
                    push(out, Part::Aids, segment(v, foot), false, aid.clone(), None);
                    // Extend the side when the foot falls outside it.
                    if !(0.0..=1.0).contains(&tt) {
                        let near = if tt < 0.0 { p1 } else { p2 };
                        push(out, Part::Aids, segment(near, foot), false, aid.clone(), None);
                    }
                }
                if f.medians {
                    push(out, Part::Aids, segment(v, p1.midpoint(p2)), false, aid.clone(), None);
                }
                if f.bisectors {
                    let (l1, l2) = ((p1 - v).hypot(), (p2 - v).hypot());
                    let foot = p1 + (p2 - p1) * (l1 / (l1 + l2));
                    push(out, Part::Aids, segment(v, foot), false, aid.clone(), None);
                }
                if f.names {
                    out.labels.push(label_out(f, Part::Labels, names[k].into(), v, v - centre, gap, f.color));
                }
                if f.measures {
                    let mid = p1.midpoint(p2);
                    let edge = p2 - p1;
                    let mut normal = Vec2::new(edge.y, -edge.x);
                    if normal.dot(mid - centre) < 0.0 {
                        normal = -normal;
                    }
                    out.labels.push(label_out(
                        f,
                        Part::Labels,
                        format!("{} = {}", side_names[k], fmt_len(t.sides[k], f)),
                        mid,
                        normal,
                        gap,
                        f.color,
                    ));
                }
                if f.angles {
                    let (u1, u2) = ((p1 - v).normalize(), (p2 - v).normalize());
                    let shortest = t.sides.iter().cloned().fold(f64::INFINITY, f64::min);
                    let rad = (shortest * 0.18).clamp(2.0, 12.0);
                    let angle = t.angles[k];
                    if (angle - 90.0).abs() < 1e-6 {
                        let q = rad * 0.7;
                        let sq = [v + u1 * q, v + (u1 + u2) * q, v + u2 * q];
                        push(out, Part::Aids, polyline(&sq, false), false, aid_solid.clone(), None);
                    } else {
                        let a1 = u1.y.atan2(u1.x);
                        let mut sweep = u2.y.atan2(u2.x) - a1;
                        while sweep > PI {
                            sweep -= TAU;
                        }
                        while sweep < -PI {
                            sweep += TAU;
                        }
                        push(out, Part::Aids, arc_path(v, rad, a1, sweep), false, aid_solid.clone(), None);
                    }
                    let bis = (u1 + u2).normalize();
                    let at = v + bis * (rad + em(f.label_pt) * 1.4);
                    out.labels.push(fig_label(
                        f,
                        Part::Labels,
                        format!("{}°", fmt_num(angle, f.decimals)),
                        at,
                        Anchor::Center,
                        f.aid_color,
                    ));
                }
            }
        }
        FigureKind::Polygon => {
            let n = f.sides as usize;
            let rr = polygon_radius(f);
            let rot = f.rotation * PI / 180.0 + PI / 2.0 - if n.is_multiple_of(2) { PI / n as f64 } else { 0.0 };
            let verts: Vec<Point> = (0..n)
                .map(|k| {
                    let a = rot + TAU * k as f64 / n as f64;
                    Point::new(rr * a.cos(), rr * a.sin())
                })
                .collect();
            let step = f.star.max(1) as usize;
            if step > 1 && n > 4 {
                // A star polygon {n/step}: one or more closed outlines.
                let loops = gcd(n as u64, step as u64) as usize;
                let mut path = BezPath::new();
                for l in 0..loops {
                    let mut pts = Vec::new();
                    let mut k = l;
                    for _ in 0..n / loops {
                        pts.push(verts[k % n]);
                        k += step;
                    }
                    path.extend(polyline(&pts, true));
                }
                push(out, Part::Figure, path, true, shape_line.clone(), fill);
            } else {
                push(out, Part::Figure, polyline(&verts, true), true, shape_line.clone(), fill);
            }
            if f.diagonals && n <= 40 {
                let mut path = BezPath::new();
                for i in 0..n {
                    for j in i + 2..n {
                        if i == 0 && j == n - 1 {
                            continue;
                        }
                        path.extend(segment(verts[i], verts[j]));
                    }
                }
                push(out, Part::Aids, path, false, aid.clone(), None);
            }
            if f.circumcircle {
                push(out, Part::Aids, circle_path(Point::ORIGIN, rr), true, aid.clone(), None);
            }
            let apothem = rr * (PI / n as f64).cos();
            if f.incircle {
                push(out, Part::Aids, circle_path(Point::ORIGIN, apothem), true, aid.clone(), None);
            }
            if f.center {
                push(out, Part::Aids, circle_path(Point::ORIGIN, dot), true, None, Some(f.color));
            }
            if f.radius_line {
                let mid = verts[0].midpoint(verts[1 % n]);
                push(out, Part::Aids, segment(Point::ORIGIN, mid), false, aid_solid.clone(), None);
                if f.measures {
                    out.labels.push(label_out(
                        f,
                        Part::Labels,
                        format!("ap = {}", fmt_len(apothem, f)),
                        Point::ORIGIN.midpoint(mid),
                        Vec2::new(-(mid.y), mid.x),
                        gap,
                        f.aid_color,
                    ));
                }
            }
            if f.names && n <= 26 {
                for (k, v) in verts.iter().enumerate() {
                    let name = ((b'A' + k as u8) as char).to_string();
                    out.labels.push(label_out(f, Part::Labels, name, *v, v.to_vec2(), gap, f.color));
                }
            }
            if f.measures && n >= 2 {
                let side = verts[0].distance(verts[1]);
                let (p1, p2) = (verts[n - 1], verts[0]);
                let mid = p1.midpoint(p2);
                out.labels.push(label_out(
                    f,
                    Part::Labels,
                    fmt_len(side, f),
                    mid,
                    mid.to_vec2(),
                    gap,
                    f.color,
                ));
            }
            if f.angles && n <= 60 {
                let v = verts[0];
                let (u1, u2) = ((verts[1] - v).normalize(), (verts[n - 1] - v).normalize());
                let side = verts[0].distance(verts[1]);
                let rad = (side * 0.2).clamp(2.0, 12.0);
                let a1 = u1.y.atan2(u1.x);
                let mut sweep = u2.y.atan2(u2.x) - a1;
                while sweep > PI {
                    sweep -= TAU;
                }
                while sweep < -PI {
                    sweep += TAU;
                }
                push(out, Part::Aids, arc_path(v, rad, a1, sweep), false, aid_solid.clone(), None);
                let interior = 180.0 * (n as f64 - 2.0) / n as f64;
                out.labels.push(fig_label(
                    f,
                    Part::Labels,
                    format!("{}°", fmt_num(interior, f.decimals)),
                    v + (u1 + u2).normalize() * (rad + em(f.label_pt) * 1.4),
                    Anchor::Center,
                    f.aid_color,
                ));
            }
        }
        FigureKind::Ellipse => {
            let (rx, ry) = (f.rx, f.ry);
            let rot = f.rotation * PI / 180.0;
            let el = kurbo::Ellipse::new(Point::ORIGIN, (rx, ry), rot);
            push(out, Part::Figure, kurbo::Shape::to_path(&el, 0.01), true, shape_line.clone(), fill);
            let ax = Vec2::new(rot.cos(), rot.sin());
            let ay = Vec2::new(-rot.sin(), rot.cos());
            if f.radius_line {
                let mut path = BezPath::new();
                path.extend(segment(Point::ORIGIN - ax * rx, Point::ORIGIN + ax * rx));
                path.extend(segment(Point::ORIGIN - ay * ry, Point::ORIGIN + ay * ry));
                push(out, Part::Aids, path, false, aid.clone(), None);
                if f.measures {
                    out.labels.push(label_out(f, Part::Labels, format!("a = {}", fmt_len(rx, f)), Point::ORIGIN + ax * (rx / 2.0), ay, gap, f.aid_color));
                    out.labels.push(label_out(f, Part::Labels, format!("b = {}", fmt_len(ry, f)), Point::ORIGIN + ay * (ry / 2.0), ax, gap, f.aid_color));
                }
            }
            if f.center {
                push(out, Part::Aids, circle_path(Point::ORIGIN, dot), true, None, Some(f.color));
            }
            if f.foci {
                let (major, dir) = if rx >= ry { (rx, ax) } else { (ry, ay) };
                let minor = rx.min(ry);
                let c = (major * major - minor * minor).sqrt();
                for (k, sgn) in [-1.0, 1.0].into_iter().enumerate() {
                    let p = Point::ORIGIN + dir * (c * sgn);
                    push(out, Part::Aids, circle_path(p, dot), true, None, Some(f.aid_color));
                    if f.names {
                        let name = if k == 0 { "F₁" } else { "F₂" };
                        out.labels.push(label_out(f, Part::Labels, name.into(), p, if rx >= ry { ay } else { ax }, gap, f.aid_color));
                    }
                }
            }
        }
        FigureKind::Arc => {
            let r = f.arc_radius;
            let a0 = f.arc_start * PI / 180.0;
            let sw = f.arc_sweep * PI / 180.0;
            let c = Point::ORIGIN;
            let arc = arc_path(c, r, a0, sw);
            let start = c + Vec2::new(r * a0.cos(), r * a0.sin());
            let end = c + Vec2::new(r * (a0 + sw).cos(), r * (a0 + sw).sin());
            match f.arc_kind {
                ArcKind::Arc => push(out, Part::Figure, arc, false, shape_line.clone(), None),
                ArcKind::Sector => {
                    let mut p = BezPath::new();
                    p.move_to(c);
                    p.line_to(start);
                    for el in arc.elements().iter().skip(1) {
                        p.push(*el);
                    }
                    p.close_path();
                    push(out, Part::Figure, p, true, shape_line.clone(), fill);
                }
                ArcKind::Segment => {
                    let mut p = arc.clone();
                    p.close_path();
                    push(out, Part::Figure, p, true, shape_line.clone(), fill);
                }
            }
            if f.center {
                push(out, Part::Aids, circle_path(c, dot), true, None, Some(f.color));
            }
            if f.radius_line && f.arc_kind != ArcKind::Sector {
                let mut p = BezPath::new();
                p.extend(segment(c, start));
                p.extend(segment(c, end));
                push(out, Part::Aids, p, false, aid.clone(), None);
            }
            if f.angles {
                let rad = (r * 0.22).clamp(2.0, 12.0);
                push(out, Part::Aids, arc_path(c, rad, a0, sw), false, aid_solid.clone(), None);
                let mid = a0 + sw / 2.0;
                let dir = Vec2::new(mid.cos(), mid.sin());
                out.labels.push(fig_label(
                    f,
                    Part::Labels,
                    format!("{}°", fmt_num(f.arc_sweep, f.decimals)),
                    c + dir * (rad + em(f.label_pt) * 1.4),
                    Anchor::Center,
                    f.aid_color,
                ));
            }
            if f.measures {
                let mid = a0 + sw / 2.0;
                let dir = Vec2::new(mid.cos(), mid.sin());
                out.labels.push(label_out(
                    f,
                    Part::Labels,
                    fmt_len(r * sw, f),
                    c + dir * r,
                    dir,
                    gap,
                    f.color,
                ));
                let rdir = Vec2::new(a0.cos(), a0.sin());
                out.labels.push(label_out(
                    f,
                    Part::Labels,
                    format!("r = {}", fmt_len(r, f)),
                    c + rdir * (r / 2.0),
                    Vec2::new(rdir.y, -rdir.x),
                    gap,
                    f.color,
                ));
            }
        }
    }
}

// ----- presets ------------------------------------------------------------

/// Ready-made drawings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    EmptyPlane,
    Parabola,
    Lines,
    SineCosine,
    Tangent,
    ExpLog,
    Gaussian,
    Damped,
    Fourier,
    Piecewise,
    Conics,
    Lemniscate,
    Rose,
    Spiral,
    GoldenSpiral,
    Cardioid,
    Lissajous,
    Heart,
    Butterfly,
    Superformula,
    Spirograph,
    Astroid,
    Points,
}

impl Preset {
    pub const ALL: [Preset; 23] = [
        Preset::EmptyPlane,
        Preset::Parabola,
        Preset::Lines,
        Preset::SineCosine,
        Preset::Tangent,
        Preset::ExpLog,
        Preset::Gaussian,
        Preset::Damped,
        Preset::Fourier,
        Preset::Piecewise,
        Preset::Conics,
        Preset::Lemniscate,
        Preset::Rose,
        Preset::Spiral,
        Preset::GoldenSpiral,
        Preset::Cardioid,
        Preset::Lissajous,
        Preset::Heart,
        Preset::Butterfly,
        Preset::Superformula,
        Preset::Spirograph,
        Preset::Astroid,
        Preset::Points,
    ];
    pub fn key(self) -> &'static str {
        match self {
            Preset::EmptyPlane => "math.preset_empty",
            Preset::Parabola => "math.preset_parabola",
            Preset::Lines => "math.preset_lines",
            Preset::SineCosine => "math.preset_sine",
            Preset::Tangent => "math.preset_tangent",
            Preset::ExpLog => "math.preset_exp_log",
            Preset::Gaussian => "math.preset_gaussian",
            Preset::Damped => "math.preset_damped",
            Preset::Fourier => "math.preset_fourier",
            Preset::Piecewise => "math.preset_piecewise",
            Preset::Conics => "math.preset_conics",
            Preset::Lemniscate => "math.preset_lemniscate",
            Preset::Rose => "math.preset_rose",
            Preset::Spiral => "math.preset_spiral",
            Preset::GoldenSpiral => "math.preset_golden",
            Preset::Cardioid => "math.preset_cardioid",
            Preset::Lissajous => "math.preset_lissajous",
            Preset::Heart => "math.preset_heart",
            Preset::Butterfly => "math.preset_butterfly",
            Preset::Superformula => "math.preset_superformula",
            Preset::Spirograph => "math.preset_spirograph",
            Preset::Astroid => "math.preset_astroid",
            Preset::Points => "math.preset_points",
        }
    }
}

fn func(i: usize, f: &str) -> Plot {
    Plot {
        f: f.into(),
        ..Plot::nth(i)
    }
}

fn param(i: usize, x: &str, y: &str, t0: f64, t1: f64) -> Plot {
    Plot {
        kind: PlotKind::Parametric,
        f: x.into(),
        g: y.into(),
        t_min: t0,
        t_max: t1,
        samples: 800,
        ..Plot::nth(i)
    }
}

fn polar(i: usize, r: &str, t0: f64, t1: f64) -> Plot {
    Plot {
        kind: PlotKind::Polar,
        f: r.into(),
        t_min: t0,
        t_max: t1,
        samples: 800,
        ..Plot::nth(i)
    }
}

fn implicit(i: usize, f: &str) -> Plot {
    Plot {
        kind: PlotKind::Implicit,
        f: f.into(),
        samples: 500,
        ..Plot::nth(i)
    }
}

fn range(s: &mut MathSettings, x0: f64, x1: f64, y0: f64, y1: f64, ux: f64, uy: f64) {
    let p = &mut s.plane;
    p.x_min = x0;
    p.x_max = x1;
    p.y_min = y0;
    p.y_max = y1;
    p.unit_x = ux;
    p.unit_y = uy;
}

/// Put preset `p` in `s`: the curves, the plane's range and steps, the
/// parameters. The plane's colours and widths stay as they are.
pub fn apply_preset(s: &mut MathSettings, p: Preset) {
    s.mode = Mode::Graph;
    s.degrees = false;
    s.plane.show = true;
    s.plane.pi_x = false;
    s.plane.grid_x = 1.0;
    s.plane.grid_y = 1.0;
    s.plane.tick_x = 1.0;
    s.plane.tick_y = 1.0;
    s.plane.minor = false;
    s.plane.grid = true;
    s.plane.numbers = true;
    s.plane.axes = true;
    s.params = [1.0, 1.0, 1.0, 1.0, 3.0];
    s.plots.clear();
    match p {
        Preset::EmptyPlane => {
            range(s, -10.0, 10.0, -10.0, 10.0, 5.0, 5.0);
            s.plane.minor = true;
            s.plane.tick_x = 2.0;
            s.plane.tick_y = 2.0;
        }
        Preset::Parabola => {
            range(s, -4.0, 6.0, -5.0, 6.0, 10.0, 10.0);
            s.plots.push(Plot {
                marks: true,
                ..func(0, "x^2 - 2x - 3")
            });
        }
        Preset::Lines => {
            range(s, -5.0, 5.0, -4.0, 6.0, 10.0, 10.0);
            s.plots.push(Plot {
                label: true,
                ..func(0, "2x + 1")
            });
            s.plots.push(Plot {
                label: true,
                ..func(1, "-x/2 + 3")
            });
        }
        Preset::SineCosine => {
            range(s, -TAU, TAU, -1.5, 1.5, 9.0, 20.0);
            s.plane.pi_x = true;
            s.plane.grid_x = PI / 2.0;
            s.plane.tick_x = PI / 2.0;
            s.plane.grid_y = 0.5;
            s.plane.tick_y = 0.5;
            s.plots.push(Plot {
                label: true,
                ..func(0, "sin(x)")
            });
            s.plots.push(Plot {
                label: true,
                ..func(1, "cos(x)")
            });
        }
        Preset::Tangent => {
            range(s, -TAU, TAU, -4.0, 4.0, 9.0, 10.0);
            s.plane.pi_x = true;
            s.plane.grid_x = PI / 2.0;
            s.plane.tick_x = PI / 2.0;
            s.plots.push(func(0, "tan(x)"));
        }
        Preset::ExpLog => {
            range(s, -3.0, 6.0, -3.0, 6.0, 10.0, 10.0);
            s.plots.push(Plot {
                label: true,
                ..func(0, "e^x")
            });
            s.plots.push(Plot {
                label: true,
                ..func(1, "ln(x)")
            });
            s.plots.push(Plot {
                dash: Dash::Dashed,
                width: 0.3,
                ..func(7, "x")
            });
        }
        Preset::Gaussian => {
            range(s, -4.0, 4.0, -0.1, 0.5, 15.0, 120.0);
            s.plane.grid_y = 0.1;
            s.plane.tick_y = 0.1;
            s.plots.push(Plot {
                fill: true,
                ..func(0, "e^(-x^2/2) / sqrt(2pi)")
            });
        }
        Preset::Damped => {
            range(s, 0.0, 12.0, -1.0, 1.0, 10.0, 30.0);
            s.plane.grid_y = 0.25;
            s.plane.tick_y = 0.5;
            s.plots.push(func(0, "e^(-x/4) sin(2x)"));
            s.plots.push(Plot {
                dash: Dash::Dashed,
                width: 0.3,
                ..func(7, "e^(-x/4)")
            });
            s.plots.push(Plot {
                dash: Dash::Dashed,
                width: 0.3,
                ..func(7, "-e^(-x/4)")
            });
        }
        Preset::Fourier => {
            range(s, -TAU, TAU, -1.5, 1.5, 9.0, 20.0);
            s.plane.pi_x = true;
            s.plane.grid_x = PI / 2.0;
            s.plane.tick_x = PI / 2.0;
            s.plane.grid_y = 0.5;
            s.plane.tick_y = 0.5;
            s.plots.push(func(
                0,
                "4/pi (sin(x) + sin(3x)/3 + sin(5x)/5 + sin(7x)/7 + sin(9x)/9)",
            ));
            s.plots.push(Plot {
                dash: Dash::Dashed,
                width: 0.3,
                ..func(7, "sign(sin(x))")
            });
        }
        Preset::Piecewise => {
            range(s, -4.0, 4.0, -1.0, 5.0, 12.0, 12.0);
            s.plots.push(func(0, "if(x < 0, -x, x^2/2)"));
        }
        Preset::Conics => {
            range(s, -8.0, 8.0, -6.0, 6.0, 7.0, 7.0);
            s.plots.push(implicit(0, "x^2 + y^2 = 9"));
            s.plots.push(implicit(1, "x^2/36 + y^2/16 = 1"));
            s.plots.push(implicit(2, "x^2/4 - y^2/9 = 1"));
        }
        Preset::Lemniscate => {
            range(s, -5.0, 5.0, -3.0, 3.0, 12.0, 12.0);
            s.plots.push(implicit(0, "(x^2 + y^2)^2 = 16(x^2 - y^2)"));
        }
        Preset::Rose => {
            range(s, -5.0, 5.0, -5.0, 5.0, 10.0, 10.0);
            s.params[4] = 5.0;
            s.plots.push(Plot {
                fill: true,
                ..polar(0, "4cos(k θ)", 0.0, TAU)
            });
        }
        Preset::Spiral => {
            range(s, -10.0, 10.0, -10.0, 10.0, 5.0, 5.0);
            s.plots.push(polar(0, "0.5θ", 0.0, 6.0 * PI));
        }
        Preset::GoldenSpiral => {
            range(s, -10.0, 10.0, -10.0, 10.0, 5.0, 5.0);
            s.plots.push(polar(0, "0.3 φ^(2θ/π)", 0.0, 4.0 * PI));
        }
        Preset::Cardioid => {
            range(s, -5.0, 2.0, -3.0, 3.0, 12.0, 12.0);
            s.plots.push(Plot {
                fill: true,
                ..polar(0, "2(1 - cos(θ))", 0.0, TAU)
            });
        }
        Preset::Lissajous => {
            range(s, -5.0, 5.0, -5.0, 5.0, 10.0, 10.0);
            s.params[0] = 3.0;
            s.params[1] = 2.0;
            s.plots.push(param(0, "4sin(a t + pi/2)", "4sin(b t)", 0.0, TAU));
        }
        Preset::Heart => {
            range(s, -5.0, 5.0, -5.0, 4.0, 10.0, 10.0);
            s.plots.push(Plot {
                fill: true,
                color: [0xD1, 0x2F, 0x2F],
                fill_color: [0xF4, 0xB4, 0xB4],
                ..param(
                    0,
                    "16sin(t)^3 / 4",
                    "(13cos(t) - 5cos(2t) - 2cos(3t) - cos(4t)) / 4",
                    0.0,
                    TAU,
                )
            });
        }
        Preset::Butterfly => {
            range(s, -4.0, 4.0, -3.0, 5.0, 10.0, 10.0);
            s.plots.push(Plot {
                samples: 3000,
                ..param(
                    0,
                    "sin(t)(e^cos(t) - 2cos(4t) - sin(t/12)^5)",
                    "cos(t)(e^cos(t) - 2cos(4t) - sin(t/12)^5)",
                    0.0,
                    12.0 * PI,
                )
            });
        }
        Preset::Superformula => {
            range(s, -3.0, 3.0, -3.0, 3.0, 15.0, 15.0);
            s.plane.grid_x = 0.5;
            s.plane.grid_y = 0.5;
            s.params = [1.0, 1.0, 0.3, 0.3, 6.0];
            s.plots.push(Plot {
                fill: true,
                samples: 1500,
                ..polar(
                    0,
                    "2(|cos(k θ/4)/a|^d + |sin(k θ/4)/b|^d)^(-1/c)",
                    0.0,
                    TAU,
                )
            });
        }
        Preset::Spirograph => {
            range(s, -10.0, 10.0, -10.0, 10.0, 5.0, 5.0);
            s.params = [7.0, 3.0, 1.0, 5.0, 3.0];
            s.plots.push(Plot {
                samples: 2000,
                ..param(
                    0,
                    "(a - b)cos(t) + d cos((a - b)/b t)",
                    "(a - b)sin(t) - d sin((a - b)/b t)",
                    0.0,
                    6.0 * PI,
                )
            });
        }
        Preset::Astroid => {
            range(s, -5.0, 5.0, -5.0, 5.0, 10.0, 10.0);
            s.plots.push(param(0, "4cos(t)^3", "4sin(t)^3", 0.0, TAU));
        }
        Preset::Points => {
            range(s, -1.0, 7.0, -1.0, 6.0, 12.0, 12.0);
            s.plots.push(Plot {
                kind: PlotKind::Points,
                f: "A(1, 1); B(5, 2); C(3, 5)".into(),
                label: true,
                marks: true,
                ..Plot::nth(0)
            });
            s.plots.push(func(1, "x/4 + 3/4"));
        }
    }
}

/// The plain text of an error, for the panel (language keys).
pub fn error_text(e: &ExprError) -> String {
    use crate::i18n::{tr, trf};
    match e {
        ExprError::Empty => tr("math.err_empty"),
        ExprError::Unknown(n) => trf("math.err_unknown", &[("name", n)]),
        ExprError::Syntax(at) => trf("math.err_syntax", &[("pos", &(at + 1).to_string())]),
        ExprError::Brackets => tr("math.err_brackets"),
        ExprError::Args(n) => trf("math.err_args", &[("name", n)]),
        ExprError::TooLong => tr("math.err_long"),
    }
}

// ----- in the drawing -----------------------------------------------------

fn color(c: Rgb) -> tracedraw_core::Color {
    tracedraw_core::Color::rgb8(c[0], c[1], c[2])
}

/// The part's group name (language key).
fn part_key(part: Part) -> &'static str {
    match part {
        Part::Grid => "math.part_grid",
        Part::Axes => "math.part_axes",
        Part::Numbers => "math.part_numbers",
        Part::Plot(_) => "math.part_curve",
        Part::Figure => "math.part_figure",
        Part::Aids => "math.part_aids",
        Part::Labels => "math.part_labels",
    }
}

/// The drawing as plain objects laid with `shift`: curves (smoothed to
/// few Bezier nodes when `smooth`) and texts, one group per part.
pub fn objects(
    d: &Drawn,
    shift: Vec2,
    smooth: bool,
    next_id: &mut dyn FnMut() -> tracedraw_core::ShapeId,
) -> Vec<tracedraw_core::Shape> {
    use tracedraw_core::{Fill, Shape, ShapeKind, Stroke, TextAlign, TextSpan};
    let mut parts: Vec<(Part, Vec<Shape>)> = Vec::new();
    let mut put = |part: Part, s: Shape| match parts.iter_mut().find(|(p, _)| *p == part) {
        Some((_, v)) => v.push(s),
        None => parts.push((part, vec![s])),
    };
    let t = kurbo::Affine::translate(shift);
    for it in &d.items {
        let mut path = t * it.path.clone();
        // Sampled curves become smooth curves with few nodes; grid lines,
        // arrows and figures are exact already.
        if smooth && matches!(it.part, Part::Plot(_)) && it.line.is_some() {
            let fitted = tracedraw_core::fit::fit_path(&path, 0.01);
            if !fitted.elements().is_empty() {
                path = fitted;
            }
        }
        let mut s = Shape::new(
            next_id(),
            ShapeKind::Path {
                path,
                closed: it.closed,
            },
        );
        s.fill = it.fill.map_or(Fill::None, |c| Fill::Solid(color(c)));
        s.stroke = it.line.as_ref().map(|l| {
            let mut st = Stroke::new(color(l.color), l.width);
            st.dash = l.dash.pattern();
            if l.dash == Dash::Dotted {
                st.cap = tracedraw_core::LineCap::Round;
            }
            st.join = tracedraw_core::LineJoin::Round;
            st
        });
        put(it.part, s);
    }
    for l in &d.labels {
        let align = match l.anchor {
            Anchor::Left => TextAlign::Left,
            Anchor::Center => TextAlign::Center,
            Anchor::Right => TextAlign::Right,
        };
        let at = l.at + shift;
        let mut s = Shape::new(
            next_id(),
            ShapeKind::Text {
                spans: vec![TextSpan::new(&l.text, l.font.clone(), l.size_pt)],
                origin: Point::new(at.x, at.y - 0.35 * em(l.size_pt)),
                frame: None,
                align,
                para: Default::default(),
                on_path: None,
            },
        );
        s.fill = Fill::Solid(color(l.color));
        s.stroke = None;
        put(l.part, s);
    }
    let mut out = Vec::new();
    for (part, kids) in parts {
        if kids.len() == 1 {
            out.extend(kids);
            continue;
        }
        let mut g = Shape::new(next_id(), ShapeKind::Group { children: kids });
        g.fill = Fill::None;
        g.stroke = None;
        g.name = Some(match part {
            Part::Plot(i) => format!("{} {}", crate::i18n::tr(part_key(part)), i + 1),
            _ => crate::i18n::tr(part_key(part)),
        });
        out.push(g);
    }
    out
}

impl crate::app::App {
    /// The selected Math Shapes group and its settings.
    pub fn math_selected(&self) -> Option<(tracedraw_core::ShapeId, MathSettings)> {
        if self.selection.len() != 1 {
            return None;
        }
        let shape = self.selected_shapes().into_iter().next()?;
        let json = shape
            .data
            .iter()
            .find(|(k, _)| k == DATA_KEY)
            .map(|(_, v)| v.clone())?;
        let s: MathSettings = serde_json::from_str(&json).ok()?;
        Some((shape.id, s.sanitized()))
    }

    /// Create: the panel's drawing as one group in the middle of the
    /// window (one undo step), selected.
    pub fn math_create(&mut self) -> Option<tracedraw_core::ShapeId> {
        let c = self.view_center();
        let center = if c.x.is_finite() && c.y.is_finite() {
            c
        } else {
            self.page_rect().center()
        };
        self.math_add(center, None)
    }

    /// Update: the selected Math Shapes group drawn again with the
    /// panel's settings, in the same place (one undo step).
    pub fn math_update(&mut self) -> Option<tracedraw_core::ShapeId> {
        let (old, _) = self.math_selected()?;
        let center = self.selection_bounds()?.center();
        self.math_add(center, Some(old))
    }

    fn math_add(
        &mut self,
        center: Point,
        replace: Option<tracedraw_core::ShapeId>,
    ) -> Option<tracedraw_core::ShapeId> {
        use tracedraw_core::{Command, Fill, Shape, ShapeKind};
        let s = self.math.sanitized();
        let d = draw(&s);
        if d.impossible {
            self.status = crate::i18n::tr("math.impossible");
            return None;
        }
        let Some(b) = d.bounds() else {
            self.status = crate::i18n::tr("math.nothing");
            return None;
        };
        let shift = center - b.center();
        let engine = &mut self.engine;
        let kids = objects(&d, shift, s.smooth, &mut || engine.new_shape_id());
        if kids.is_empty() {
            self.status = crate::i18n::tr("math.nothing");
            return None;
        }
        let gid = self.engine.new_shape_id();
        let mut group = Shape::new(gid, ShapeKind::Group { children: kids });
        group.fill = Fill::None;
        group.stroke = None;
        group.name = Some(crate::i18n::tr(match s.mode {
            Mode::Graph => "math.name_graph",
            Mode::Figure => s.figure.kind.key(),
        }));
        if let Ok(json) = serde_json::to_string(&s) {
            group.data = vec![(DATA_KEY.to_string(), json)];
        }
        let layer = match replace.and_then(|id| self.doc().locate(id).ok().map(|(l, _)| l)) {
            Some(l) => l,
            None => self.active_layer()?,
        };
        let mut cmds = Vec::new();
        if let Some(old) = replace {
            cmds.push(Command::DeleteShapes { shapes: vec![old] });
        }
        cmds.push(Command::AddShape {
            layer,
            shape: group,
        });
        if let Err(e) = self.engine.run_batch("Math Shapes", &cmds) {
            log::warn!("math shapes: {e}");
            return None;
        }
        self.select(vec![gid]);
        self.status = crate::i18n::tr("math.made");
        Some(gid)
    }
}
