//! Laser output: G-code for diode and CO2 lasers run by GRBL or Marlin
//! firmware, a frame to show where the job lands, a material test grid
//! and a clean DXF of the cut lines.
//!
//! The objects are sorted into laser layers, by the colour of their
//! outline (the fill's when they have none) or by the document layer
//! they are on. Each laser layer has a mode:
//!
//! - **Cut** and **Score**: the beam follows the outlines (closed ones
//!   and open curves). Score is the same move with its own power and
//!   speed, for lines marked and not cut through.
//! - **Engrave**: the areas the closed outlines enclose (even-odd, so
//!   holes and the counters of letters stay clear) are filled with
//!   horizontal lines `interval` apart, run both ways, each line run past
//!   its ends by the overscan with the beam off so the head is at speed
//!   where it burns.
//! - **Skip**: left out.
//!
//! Engrave layers go first, then Score, then Cut, so a part is cut free
//! only once everything on it is done; within a mode the layers keep the
//! order their objects are stacked in. The outlines of a Cut or Score
//! layer are ordered by [`crate::cut::order::plan`] from where the head
//! is, inner ones first. A layer with several passes runs all its lines
//! once per pass.
//!
//! The program is in millimetres (`G21`, `G90`). Power is a share of the
//! firmware's largest `S` value (1000 for GRBL, 255 for Marlin, both
//! settable). GRBL: `M4 S0` (dynamic power, scaled with the speed so
//! corners do not burn deeper) or `M3 S0` at the start, `G1 .. S.. F..`
//! burns and `G0` moves with the beam off, which GRBL's laser mode does
//! on its own. Marlin: `M3 I S0` turns on inline power, `G1 .. S..`
//! burns and `G0 .. S0` moves. Air assist is `M8`/`M9` per layer. The
//! program ends with `M5` (Marlin `M5 I`) and a move back to X0 Y0. X0
//! Y0 is the bottom-left corner of the job (or a given origin).

use crate::cnc::Zero;
use crate::cut::order::{self, Direction, RouteItem, RouteSpec, StartAt};
use crate::cut::{fin, finite, poly_bounds, polygon_path, polys};
use serde::{Deserialize, Serialize};
use tracedraw_core::geometry::{Affine, BezPath, Point, Rect};
use tracedraw_core::live::Effect;
use tracedraw_core::{Fill, Shape, ShapeKind};

/// The firmware the program is written for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Firmware {
    #[default]
    Grbl,
    Marlin,
}

impl Firmware {
    /// The largest `S` value the firmware takes out of the box.
    pub fn default_max_power(self) -> f64 {
        match self {
            Firmware::Grbl => 1000.0,
            Firmware::Marlin => 255.0,
        }
    }
}

/// What the laser does with a layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Mode {
    #[default]
    Cut,
    Score,
    Engrave,
    Skip,
}

impl Mode {
    pub const ALL: [Mode; 4] = [Mode::Cut, Mode::Score, Mode::Engrave, Mode::Skip];

    /// The order the modes run in: engraving, then scoring, then cutting.
    fn rank(self) -> u8 {
        match self {
            Mode::Engrave => 0,
            Mode::Score => 1,
            Mode::Cut => 2,
            Mode::Skip => 3,
        }
    }
}

/// How the objects are sorted into laser layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum GroupBy {
    /// By outline colour (the fill's when there is no outline).
    #[default]
    Color,
    /// By the document layer the object is on.
    Layer,
}

/// The settings of one laser layer; power in percent, speed in mm a
/// second, interval in millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LayerSpec {
    pub mode: Mode,
    pub power: f64,
    pub speed: f64,
    pub passes: u32,
    /// Engrave: the distance between the lines.
    pub interval: f64,
    /// Air assist on while the layer runs.
    pub air: bool,
}

impl Default for LayerSpec {
    fn default() -> Self {
        LayerSpec::for_mode(Mode::Cut)
    }
}

impl LayerSpec {
    /// Starting values for a layer in `mode`.
    pub fn for_mode(mode: Mode) -> LayerSpec {
        let (power, speed) = match mode {
            Mode::Cut | Mode::Skip => (100.0, 8.0),
            Mode::Score => (30.0, 40.0),
            Mode::Engrave => (40.0, 120.0),
        };
        LayerSpec {
            mode,
            power,
            speed,
            passes: 1,
            interval: 0.1,
            air: mode == Mode::Cut,
        }
    }

    /// The settings with every value finite and in its range.
    pub fn clamped(&self) -> LayerSpec {
        let d = LayerSpec::for_mode(self.mode);
        LayerSpec {
            power: fin(self.power, d.power).clamp(POWER_RANGE.0, POWER_RANGE.1),
            speed: fin(self.speed, d.speed).clamp(SPEED_RANGE.0, SPEED_RANGE.1),
            passes: self.passes.clamp(PASSES_RANGE.0, PASSES_RANGE.1),
            interval: fin(self.interval, d.interval).clamp(INTERVAL_RANGE.0, INTERVAL_RANGE.1),
            ..*self
        }
    }
}

/// A laser layer's settings under its name (`#RRGGBB` or the document
/// layer's name).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LaserLayer {
    pub key: String,
    pub spec: LayerSpec,
}

/// The material test: a grid of squares, power rising along the columns
/// and speed along the rows, each square marked as `mode` says, with
/// the values scored beside them.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TestGrid {
    /// Engrave fills the squares, Score or Cut follows their outlines.
    pub mode: Mode,
    /// Power steps (columns) and speed steps (rows).
    pub columns: u32,
    pub rows: u32,
    pub power_min: f64,
    pub power_max: f64,
    pub speed_min: f64,
    pub speed_max: f64,
    /// The side of each square and the gap between them (mm).
    pub cell: f64,
    pub gap: f64,
    pub interval: f64,
    pub passes: u32,
    /// Score the values beside the grid.
    pub labels: bool,
    pub label_power: f64,
    pub label_speed: f64,
}

impl Default for TestGrid {
    fn default() -> Self {
        TestGrid {
            mode: Mode::Engrave,
            columns: 5,
            rows: 5,
            power_min: 20.0,
            power_max: 100.0,
            speed_min: 20.0,
            speed_max: 100.0,
            cell: 8.0,
            gap: 2.0,
            interval: 0.1,
            passes: 1,
            labels: true,
            label_power: 30.0,
            label_speed: 40.0,
        }
    }
}

/// Settings of the laser output; lengths in millimetres.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LaserSpec {
    pub firmware: Firmware,
    pub group_by: GroupBy,
    /// The `S` value of full power.
    pub max_power: f64,
    /// GRBL: power scaled with the speed (`M4`) instead of constant (`M3`).
    pub dynamic: bool,
    /// Engrave: how far each line runs past its ends with the beam off.
    pub overscan: f64,
    /// How far the line segments may stray from curves.
    pub tolerance: f64,
    pub zero: Zero,
    /// Outlines inside others are cut first.
    pub inner_first: bool,
    /// Frame: power in percent (0 runs it with the beam off) and speed in
    /// mm a second.
    pub frame_power: f64,
    pub frame_speed: f64,
    /// The settings of the laser layers met so far.
    pub layers: Vec<LaserLayer>,
    pub test: TestGrid,
}

impl Default for LaserSpec {
    fn default() -> Self {
        LaserSpec {
            firmware: Firmware::Grbl,
            group_by: GroupBy::Color,
            max_power: Firmware::Grbl.default_max_power(),
            dynamic: true,
            overscan: 2.0,
            tolerance: 0.02,
            zero: Zero::JobCorner,
            inner_first: true,
            frame_power: 0.0,
            frame_speed: 50.0,
            layers: Vec::new(),
            test: TestGrid::default(),
        }
    }
}

/// The ranges the settings are kept in.
pub const POWER_RANGE: (f64, f64) = (0.0, 100.0);
pub const SPEED_RANGE: (f64, f64) = (0.1, 2000.0);
pub const PASSES_RANGE: (u32, u32) = (1, 100);
pub const INTERVAL_RANGE: (f64, f64) = (0.02, 10.0);
pub const MAX_POWER_RANGE: (f64, f64) = (1.0, 100_000.0);
pub const OVERSCAN_RANGE: (f64, f64) = (0.0, 50.0);
pub const TOLERANCE_RANGE: (f64, f64) = (0.001, 1.0);
pub const CELL_RANGE: (f64, f64) = (1.0, 100.0);
pub const GAP_RANGE: (f64, f64) = (0.0, 50.0);
pub const STEPS_RANGE: (u32, u32) = (1, 20);

/// The most engraving lines one layer gets; past it the interval grows.
pub const MAX_SCAN_LINES: usize = 100_000;

/// The speed assumed for moves with the beam off, for the time (mm a
/// minute).
const TRAVEL_RATE: f64 = 6000.0;

impl LaserSpec {
    /// The settings of the laser layer `key`: the kept ones, or Cut's
    /// starting values.
    pub fn layer(&self, key: &str) -> LayerSpec {
        self.layers
            .iter()
            .find(|l| l.key == key)
            .map(|l| l.spec)
            .unwrap_or_default()
    }

    /// The kept settings of `key` to change, added with Cut's starting
    /// values when it has none yet.
    pub fn layer_mut(&mut self, key: &str) -> &mut LayerSpec {
        let i = match self.layers.iter().position(|l| l.key == key) {
            Some(i) => i,
            None => {
                self.layers.push(LaserLayer {
                    key: key.to_string(),
                    spec: LayerSpec::default(),
                });
                self.layers.len() - 1
            }
        };
        &mut self.layers[i].spec
    }

    fn max_power(&self) -> f64 {
        fin(self.max_power, self.firmware.default_max_power())
            .clamp(MAX_POWER_RANGE.0, MAX_POWER_RANGE.1)
    }

    fn tolerance(&self) -> f64 {
        fin(self.tolerance, 0.02).clamp(TOLERANCE_RANGE.0, TOLERANCE_RANGE.1)
    }

    fn overscan(&self) -> f64 {
        fin(self.overscan, 2.0).clamp(OVERSCAN_RANGE.0, OVERSCAN_RANGE.1)
    }
}

/// The outlines of one object (or one part of a group) and the laser
/// layer they go to.
#[derive(Debug, Clone, PartialEq)]
pub struct LaserItem {
    /// `#RRGGBB` or the document layer's name.
    pub key: String,
    /// The colour shown for the layer.
    pub color: [u8; 3],
    /// Page space.
    pub paths: Vec<BezPath>,
}

/// The name of a colour as a layer key.
pub fn color_key(c: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}

/// The colour that sorts `shape` into a layer: its outline's, else its
/// solid fill's, else black.
fn shape_color(shape: &Shape) -> [u8; 3] {
    if let Some(s) = &shape.stroke {
        return s.color.to_rgb8();
    }
    match &shape.fill {
        Fill::Solid(c) => c.to_rgb8(),
        _ => [0, 0, 0],
    }
}

/// The laser items of `shape` (with its page transform, see
/// `Document::shape_on_page`) on the document layer `layer`: groups are
/// opened so each part goes by its own colour, and outlines are the ones
/// drawn, as for the cutter (see [`crate::outlines`]).
pub fn items(shape: &Shape, layer: &str, by: GroupBy) -> Vec<LaserItem> {
    let mut out = Vec::new();
    collect(shape, Affine::IDENTITY, 0, layer, by, &mut out);
    out
}

fn collect(
    shape: &Shape,
    parent: Affine,
    depth: usize,
    layer: &str,
    by: GroupBy,
    out: &mut Vec<LaserItem>,
) {
    if depth > 64 {
        return;
    }
    if shape.effects.iter().any(Effect::warps_outline) {
        let mut drawn = tracedraw_core::live::evaluate(shape).main;
        drawn.effects.clear();
        collect(&drawn, parent, depth + 1, layer, by, out);
        return;
    }
    let t = parent * shape.transform;
    match &shape.kind {
        ShapeKind::Group { children } => {
            for c in children {
                collect(c, t, depth + 1, layer, by, out);
            }
        }
        ShapeKind::ClipFrame { frame, .. } => collect(frame, t, depth + 1, layer, by, out),
        _ => {
            let p = t * shape.local_path();
            if p.elements().is_empty() {
                return;
            }
            let color = shape_color(shape);
            let key = match by {
                GroupBy::Color => color_key(color),
                GroupBy::Layer => layer.to_string(),
            };
            out.push(LaserItem {
                key,
                color,
                paths: vec![p],
            });
        }
    }
}

/// The laser layers of `items` in the order they appear, each with the
/// colour of its first item.
pub fn layer_keys(items: &[LaserItem]) -> Vec<(String, [u8; 3])> {
    let mut keys: Vec<(String, [u8; 3])> = Vec::new();
    for i in items {
        if !keys.iter().any(|(k, _)| *k == i.key) {
            keys.push((i.key.clone(), i.color));
        }
    }
    keys
}

/// A laser program and what it does.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Program {
    pub gcode: String,
    /// The outlines cut, page space, for the preview.
    pub cuts: Vec<BezPath>,
    /// Scored outlines and (some of) the engraving lines, for the preview.
    pub marked: Vec<BezPath>,
    /// Moves with the beam off between outlines, page space.
    pub moves: Vec<(Point, Point)>,
    /// The layers that run.
    pub layers: usize,
    /// Length burnt, every pass (mm).
    pub burn_length: f64,
    /// About how long the job takes (minutes).
    pub minutes: f64,
    /// The box of everything that burns, page space.
    pub bounds: Option<Rect>,
    /// The page point that is X0 Y0.
    pub zero: Point,
}

/// A number for the program: up to three decimals, trailing zeros cut,
/// never `-0`.
fn num(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.').to_string();
    if s == "-0" || s.is_empty() {
        "0".into()
    } else {
        s
    }
}

/// Writes the moves and keeps count of what they cost.
struct Writer {
    out: String,
    firmware: Firmware,
    zero: Point,
    at: Point,
    /// The modal `S` and `F` last written with a `G1`.
    s: Option<f64>,
    f: Option<f64>,
    burn_length: f64,
    minutes: f64,
    moves: Vec<(Point, Point)>,
}

impl Writer {
    fn line(&mut self, s: &str) {
        self.out.push_str(s);
        self.out.push('\n');
    }

    fn xy(&self, p: Point) -> String {
        format!("X{} Y{}", num(p.x - self.zero.x), num(p.y - self.zero.y))
    }

    /// A move with the beam off at the machine's travel rate.
    fn travel(&mut self, p: Point) {
        let d = (p - self.at).hypot();
        if d <= 1e-9 {
            return;
        }
        let xy = self.xy(p);
        let s = match self.firmware {
            Firmware::Grbl => format!("G0 {xy}"),
            Firmware::Marlin => {
                // G0 S0 sets the inline power to nothing.
                self.s = Some(0.0);
                format!("G0 {xy} S0")
            }
        };
        self.line(&s);
        self.moves.push((self.at, p));
        self.minutes += d / TRAVEL_RATE;
        self.at = p;
    }

    /// A `G1` to `p` at `s` (the firmware's units) and `f` mm a minute.
    fn burn(&mut self, p: Point, s: f64, f: f64) {
        let d = (p - self.at).hypot();
        if d <= 1e-9 {
            return;
        }
        let mut cmd = format!("G1 {}", self.xy(p));
        if self.s != Some(s) {
            cmd.push_str(&format!(" S{}", num(s)));
            self.s = Some(s);
        }
        if self.f != Some(f) {
            cmd.push_str(&format!(" F{}", num(f)));
            self.f = Some(f);
        }
        self.line(&cmd);
        if s > 0.0 {
            self.burn_length += d;
        }
        self.minutes += d / f;
        self.at = p;
    }
}

/// One laser layer ready to run.
struct Run {
    key: String,
    spec: LayerSpec,
    polys: Vec<(Vec<Point>, bool)>,
}

/// The layers of `items` that run, in running order.
fn runs(items: &[LaserItem], spec: &LaserSpec) -> Vec<Run> {
    let tol = spec.tolerance();
    let mut out: Vec<Run> = Vec::new();
    for item in items {
        let ls = spec.layer(&item.key).clamped();
        if ls.mode == Mode::Skip {
            continue;
        }
        let i = match out.iter().position(|r| r.key == item.key) {
            Some(i) => i,
            None => {
                out.push(Run {
                    key: item.key.clone(),
                    spec: ls,
                    polys: Vec::new(),
                });
                out.len() - 1
            }
        };
        for p in &item.paths {
            for poly in polys(p, tol) {
                let closed = poly.closed && poly.pts.len() >= 3;
                out[i].polys.push((poly.pts, closed));
            }
        }
    }
    out.retain(|r| !r.polys.is_empty());
    // A stable sort keeps the stacking order within a mode.
    out.sort_by_key(|r| r.spec.mode.rank());
    out
}

/// The horizontal lines that fill the closed `polys` (even-odd), `step`
/// apart, each as its segments from left to right.
fn scan_lines(polys: &[(Vec<Point>, bool)], step: f64) -> Vec<(f64, Vec<(f64, f64)>)> {
    let closed: Vec<&Vec<Point>> = polys.iter().filter(|(_, c)| *c).map(|(p, _)| p).collect();
    let Some(b) = closed
        .iter()
        .filter_map(|p| poly_bounds(p))
        .reduce(|a, b| a.union(b))
    else {
        return Vec::new();
    };
    let mut step = step;
    while (b.height() / step) as usize > MAX_SCAN_LINES {
        step *= 2.0;
    }
    let mut out = Vec::new();
    let mut y = b.y0 + step / 2.0;
    let mut xs: Vec<f64> = Vec::new();
    while y < b.y1 {
        xs.clear();
        for p in &closed {
            let n = p.len();
            for k in 0..n {
                let a = p[k];
                let c = p[(k + 1) % n];
                if (a.y <= y) != (c.y <= y) {
                    xs.push(a.x + (y - a.y) * (c.x - a.x) / (c.y - a.y));
                }
            }
        }
        xs.retain(|x| x.is_finite());
        xs.sort_by(f64::total_cmp);
        let segs: Vec<(f64, f64)> = xs
            .chunks_exact(2)
            .map(|w| (w[0], w[1]))
            .filter(|(a, b)| b - a > 1e-6)
            .collect();
        if !segs.is_empty() {
            out.push((y, segs));
        }
        y += step;
    }
    out
}

/// The program that burns `items` as `spec` says; `origin` is where X0
/// Y0 goes with [`Zero::Origin`].
pub fn program(items: &[LaserItem], spec: &LaserSpec, origin: Point) -> Program {
    let runs = runs(items, spec);
    let all: Vec<BezPath> = runs
        .iter()
        .flat_map(|r| r.polys.iter().map(|(p, c)| poly_path(p, *c)))
        .collect();
    let bounds = crate::cut::bounds(&all);
    let corner = bounds.map(|b| Point::new(b.x0, b.y0)).unwrap_or(Point::ZERO);
    let zero = match spec.zero {
        Zero::JobCorner => corner,
        Zero::Origin if finite(origin) => origin,
        Zero::Origin => Point::ZERO,
    };
    let max_s = spec.max_power();
    let mut w = Writer {
        out: String::new(),
        firmware: spec.firmware,
        zero,
        at: zero,
        s: None,
        f: None,
        burn_length: 0.0,
        minutes: 0.0,
        moves: Vec::new(),
    };
    let mut prog = Program {
        layers: runs.len(),
        bounds,
        zero,
        ..Program::default()
    };
    w.line("; TraceDraw laser job");
    let fw = match spec.firmware {
        Firmware::Grbl => "GRBL",
        Firmware::Marlin => "Marlin",
    };
    let head = format!(
        "; {fw}, units mm, full power S{}, {} layers",
        num(max_s),
        runs.len()
    );
    w.line(&head);
    w.line("; X0 Y0 at the origin chosen");
    w.line("G21");
    w.line("G90");
    match spec.firmware {
        Firmware::Grbl if spec.dynamic => w.line("M4 S0"),
        Firmware::Grbl => w.line("M3 S0"),
        Firmware::Marlin => w.line("M3 I S0"),
    }
    w.s = Some(0.0);
    let overscan = spec.overscan();
    let mut air = false;
    for run in &runs {
        let ls = run.spec;
        let s = max_s * ls.power / 100.0;
        let f = ls.speed * 60.0;
        let name = match ls.mode {
            Mode::Cut => "cut",
            Mode::Score => "score",
            Mode::Engrave => "engrave",
            Mode::Skip => "skip",
        };
        let note = format!(
            "; Layer {}: {name}, power {}%, speed {} mm/s, {} passes",
            run.key,
            num(ls.power),
            num(ls.speed),
            ls.passes
        );
        w.line(&note);
        if ls.air != air {
            w.line(if ls.air { "M8" } else { "M9" });
            air = ls.air;
        }
        match ls.mode {
            Mode::Engrave => {
                let lines = scan_lines(&run.polys, ls.interval);
                // The preview shows at most this many lines a layer.
                let every = (lines.len() / 300).max(1);
                for (i, (y, segs)) in lines.iter().enumerate() {
                    if i % every == 0 {
                        for (a, b) in segs {
                            prog.marked
                                .push(crate::cut::line_path(Point::new(*a, *y), Point::new(*b, *y)));
                        }
                    }
                }
                for _ in 0..ls.passes {
                    for (i, (y, segs)) in lines.iter().enumerate() {
                        let y = *y;
                        let forward = i % 2 == 0;
                        let ordered: Vec<(f64, f64)> = if forward {
                            segs.clone()
                        } else {
                            segs.iter().rev().map(|(a, b)| (*b, *a)).collect()
                        };
                        let dir = if forward { 1.0 } else { -1.0 };
                        let (Some(first), Some(last)) = (ordered.first(), ordered.last()) else {
                            continue;
                        };
                        w.travel(Point::new(first.0 - dir * overscan, y));
                        for (a, b) in &ordered {
                            w.burn(Point::new(*a, y), 0.0, f);
                            w.burn(Point::new(*b, y), s, f);
                        }
                        w.burn(Point::new(last.1 + dir * overscan, y), 0.0, f);
                    }
                }
            }
            Mode::Cut | Mode::Score => {
                let route_items: Vec<RouteItem> = run
                    .polys
                    .iter()
                    .map(|(p, c)| RouteItem {
                        paths: vec![poly_path(p, *c)],
                        editable: true,
                    })
                    .collect();
                let route = order::plan(
                    &route_items,
                    &RouteSpec {
                        direction: Direction::Keep,
                        start: StartAt::Nearest,
                        inner_first: spec.inner_first,
                        from: w.at,
                    },
                );
                let tol = spec.tolerance();
                let mut ordered: Vec<(Vec<Point>, bool)> = Vec::new();
                for i in &route.order {
                    let Some((pts, closed)) = run.polys.get(*i) else {
                        continue;
                    };
                    let pts = route
                        .cuts
                        .get(*i)
                        .and_then(|c| c.first())
                        .and_then(|c| polys(c, tol).into_iter().next())
                        .map(|p| p.pts)
                        .filter(|p| p.len() >= 2)
                        .unwrap_or_else(|| pts.clone());
                    ordered.push((pts, *closed));
                }
                for (pts, closed) in &ordered {
                    let path = poly_path(pts, *closed);
                    if ls.mode == Mode::Cut {
                        prog.cuts.push(path);
                    } else {
                        prog.marked.push(path);
                    }
                }
                for _ in 0..ls.passes {
                    for (pts, closed) in &ordered {
                        let Some(&start) = pts.first() else { continue };
                        w.travel(start);
                        for p in pts.iter().skip(1) {
                            w.burn(*p, s, f);
                        }
                        if *closed {
                            w.burn(start, s, f);
                        }
                    }
                }
            }
            Mode::Skip => {}
        }
    }
    if air {
        w.line("M9");
    }
    match spec.firmware {
        Firmware::Grbl => w.line("M5"),
        Firmware::Marlin => w.line("M5 I"),
    }
    w.travel(zero);
    prog.burn_length = w.burn_length;
    prog.minutes = w.minutes;
    prog.moves = std::mem::take(&mut w.moves);
    prog.gcode = w.out;
    prog
}

fn poly_path(pts: &[Point], closed: bool) -> BezPath {
    if closed {
        polygon_path(pts)
    } else {
        tracedraw_core::geometry::polyline_path(pts, false)
    }
}

/// The frame: the head goes round the box of everything that burns, so
/// the material can be placed under it. With a frame power above 0 the
/// beam is on at that power (`M3`, constant), else off.
pub fn frame(items: &[LaserItem], spec: &LaserSpec, origin: Point) -> Option<String> {
    let p = program(items, spec, origin);
    let b = p.bounds?;
    let zero = p.zero;
    let speed = fin(spec.frame_speed, 50.0).clamp(SPEED_RANGE.0, SPEED_RANGE.1);
    let power = fin(spec.frame_power, 0.0).clamp(POWER_RANGE.0, POWER_RANGE.1);
    let f = num(speed * 60.0);
    let xy = |x: f64, y: f64| format!("X{} Y{}", num(x - zero.x), num(y - zero.y));
    let mut out = String::new();
    out.push_str("; TraceDraw laser frame\nG21\nG90\n");
    if power > 0.0 {
        out.push_str(&format!("M3 S{}\n", num(spec.max_power() * power / 100.0)));
    } else {
        out.push_str("M5\n");
    }
    out.push_str(&format!("G0 {}\n", xy(b.x0, b.y0)));
    for (x, y) in [(b.x1, b.y0), (b.x1, b.y1), (b.x0, b.y1), (b.x0, b.y0)] {
        out.push_str(&format!("G1 {} F{f}\n", xy(x, y)));
    }
    out.push_str("M5\n");
    Some(out)
}

/// The lines of a digit in a box `w` wide and `h` high from `at`
/// (bottom left), drawn as seven straight segments.
fn digit_lines(d: char, at: Point, w: f64, h: f64) -> Vec<BezPath> {
    // Segments: top, top right, bottom right, bottom, bottom left, top
    // left, middle.
    let on: [bool; 7] = match d {
        '0' => [true, true, true, true, true, true, false],
        '1' => [false, true, true, false, false, false, false],
        '2' => [true, true, false, true, true, false, true],
        '3' => [true, true, true, true, false, false, true],
        '4' => [false, true, true, false, false, true, true],
        '5' => [true, false, true, true, false, true, true],
        '6' => [true, false, true, true, true, true, true],
        '7' => [true, true, true, false, false, false, false],
        '8' => [true; 7],
        '9' => [true, true, true, true, false, true, true],
        _ => [false; 7],
    };
    let p = |x: f64, y: f64| Point::new(at.x + x * w, at.y + y * h);
    let ends = [
        (p(0.0, 1.0), p(1.0, 1.0)),
        (p(1.0, 1.0), p(1.0, 0.5)),
        (p(1.0, 0.5), p(1.0, 0.0)),
        (p(0.0, 0.0), p(1.0, 0.0)),
        (p(0.0, 0.0), p(0.0, 0.5)),
        (p(0.0, 0.5), p(0.0, 1.0)),
        (p(0.0, 0.5), p(1.0, 0.5)),
    ];
    on.iter()
        .zip(ends)
        .filter(|(o, _)| **o)
        .map(|(_, (a, b))| crate::cut::line_path(a, b))
        .collect()
}

/// `text` (digits) as lines, `h` high, starting at `at`.
fn number_lines(text: &str, at: Point, h: f64) -> Vec<BezPath> {
    let w = h * 0.55;
    let mut out = Vec::new();
    for (i, c) in text.chars().enumerate() {
        out.extend(digit_lines(c, Point::new(at.x + i as f64 * w * 1.6, at.y), w, h));
    }
    out
}

/// The width of `text` drawn by [`number_lines`] at height `h`.
fn number_width(text: &str, h: f64) -> f64 {
    let n = text.chars().count() as f64;
    let w = h * 0.55;
    if n == 0.0 {
        0.0
    } else {
        (n - 1.0) * w * 1.6 + w
    }
}

fn lerp(a: f64, b: f64, i: u32, n: u32) -> f64 {
    if n <= 1 {
        a
    } else {
        a + (b - a) * f64::from(i) / f64::from(n - 1)
    }
}

/// The material test as laser items and the settings that run it: a
/// square per power and speed, power rising to the right, speed rising
/// downwards, the values scored above and left of the grid. The
/// firmware, power scale and overscan are `spec`'s.
pub fn test_job(spec: &LaserSpec) -> (Vec<LaserItem>, LaserSpec) {
    let t = spec.test;
    let cols = t.columns.clamp(STEPS_RANGE.0, STEPS_RANGE.1);
    let rows = t.rows.clamp(STEPS_RANGE.0, STEPS_RANGE.1);
    let cell = fin(t.cell, 8.0).clamp(CELL_RANGE.0, CELL_RANGE.1);
    let gap = fin(t.gap, 2.0).clamp(GAP_RANGE.0, GAP_RANGE.1);
    let mode = if t.mode == Mode::Skip { Mode::Engrave } else { t.mode };
    let mut run = LaserSpec {
        layers: Vec::new(),
        zero: Zero::JobCorner,
        inner_first: true,
        ..spec.clone()
    };
    let mut items = Vec::new();
    let pitch = cell + gap;
    let top = f64::from(rows) * pitch - gap;
    let text_h = (cell * 0.4).clamp(1.5, 6.0);
    for r in 0..rows {
        let speed = lerp(t.speed_min, t.speed_max, r, rows);
        for c in 0..cols {
            let power = lerp(t.power_min, t.power_max, c, cols);
            let key = format!("test {} {}", r + 1, c + 1);
            run.layers.push(LaserLayer {
                key: key.clone(),
                spec: LayerSpec {
                    mode,
                    power,
                    speed,
                    passes: t.passes,
                    interval: t.interval,
                    air: mode == Mode::Cut,
                }
                .clamped(),
            });
            let x = f64::from(c) * pitch;
            let y = top - f64::from(r) * pitch - cell;
            items.push(LaserItem {
                key,
                color: [0, 0, 0],
                paths: vec![crate::cut::rect_path(Rect::new(x, y, x + cell, y + cell))],
            });
        }
    }
    if t.labels {
        let mut lines = Vec::new();
        for c in 0..cols {
            let text = format!("{:.0}", lerp(t.power_min, t.power_max, c, cols).max(0.0));
            let x = f64::from(c) * pitch + (cell - number_width(&text, text_h)) / 2.0;
            lines.extend(number_lines(&text, Point::new(x, top + gap), text_h));
        }
        for r in 0..rows {
            let text = format!("{:.0}", lerp(t.speed_min, t.speed_max, r, rows).max(0.0));
            let y = top - f64::from(r) * pitch - cell / 2.0 - text_h / 2.0;
            let x = -gap - number_width(&text, text_h);
            lines.extend(number_lines(&text, Point::new(x, y), text_h));
        }
        run.layers.push(LaserLayer {
            key: "labels".into(),
            spec: LayerSpec {
                mode: Mode::Score,
                power: t.label_power,
                speed: t.label_speed,
                passes: 1,
                interval: t.interval,
                air: false,
            }
            .clamped(),
        });
        items.push(LaserItem {
            key: "labels".into(),
            color: [0, 0, 0],
            paths: lines,
        });
    }
    (items, run)
}

/// A clean DXF (R12, millimetres) of what burns: one polyline per
/// outline, curves as short straight lines, outlines drawn twice kept
/// once, Skip layers left out, a DXF layer per laser layer. Engrave
/// layers give their outlines.
pub fn dxf(items: &[LaserItem], spec: &LaserSpec, origin: Point) -> String {
    let runs = runs(items, spec);
    let zero = program(items, spec, origin).zero;
    let names: Vec<String> = runs.iter().map(|r| dxf_name(&r.key)).collect();
    let colors: Vec<u8> = runs
        .iter()
        .map(|r| {
            items
                .iter()
                .find(|i| i.key == r.key)
                .map(|i| aci(i.color))
                .unwrap_or(7)
        })
        .collect();
    let mut out = String::new();
    let mut pair = |code: u32, value: &str| {
        out.push_str(&format!("{code}\n{value}\n"));
    };
    pair(0, "SECTION");
    pair(2, "HEADER");
    pair(9, "$ACADVER");
    pair(1, "AC1009");
    pair(9, "$INSUNITS");
    pair(70, "4");
    pair(9, "$MEASUREMENT");
    pair(70, "1");
    pair(0, "ENDSEC");
    pair(0, "SECTION");
    pair(2, "TABLES");
    pair(0, "TABLE");
    pair(2, "LAYER");
    pair(70, &names.len().to_string());
    for (n, c) in names.iter().zip(&colors) {
        pair(0, "LAYER");
        pair(2, n);
        pair(70, "0");
        pair(62, &c.to_string());
        pair(6, "CONTINUOUS");
    }
    pair(0, "ENDTAB");
    pair(0, "ENDSEC");
    pair(0, "SECTION");
    pair(2, "ENTITIES");
    let mut seen: Vec<Vec<(i64, i64)>> = Vec::new();
    for (run, name) in runs.iter().zip(&names) {
        for (pts, closed) in &run.polys {
            let key: Vec<(i64, i64)> = pts
                .iter()
                .map(|p| ((p.x * 1000.0).round() as i64, (p.y * 1000.0).round() as i64))
                .collect();
            if same_outline(&seen, &key) {
                continue;
            }
            seen.push(key);
            pair(0, "POLYLINE");
            pair(8, name);
            pair(66, "1");
            pair(10, "0");
            pair(20, "0");
            pair(30, "0");
            pair(70, if *closed { "1" } else { "0" });
            for p in pts {
                pair(0, "VERTEX");
                pair(8, name);
                pair(10, &num(p.x - zero.x));
                pair(20, &num(p.y - zero.y));
                pair(30, "0");
            }
            pair(0, "SEQEND");
            pair(8, name);
        }
    }
    pair(0, "ENDSEC");
    pair(0, "EOF");
    out
}

/// Whether `key` repeats an outline in `seen`, from any start and either
/// way round.
fn same_outline(seen: &[Vec<(i64, i64)>], key: &[(i64, i64)]) -> bool {
    let n = key.len();
    seen.iter().any(|s| {
        if s.len() != n || n == 0 {
            return false;
        }
        let Some(start) = s.iter().position(|p| *p == key[0]) else {
            return false;
        };
        let fwd = (0..n).all(|i| s[(start + i) % n] == key[i]);
        let back = (0..n).all(|i| s[(start + n - i) % n] == key[i]);
        fwd || back
    })
}

/// A DXF layer name: letters, digits, `-` and `_` only.
fn dxf_name(key: &str) -> String {
    let s: String = key
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let s = s.trim_matches('_').to_string();
    if s.is_empty() {
        "LASER".into()
    } else {
        s
    }
}

/// The nearest of the first seven DXF colours (or 8, grey).
fn aci(c: [u8; 3]) -> u8 {
    const BASIC: [(u8, [u8; 3]); 8] = [
        (1, [255, 0, 0]),
        (2, [255, 255, 0]),
        (3, [0, 255, 0]),
        (4, [0, 255, 255]),
        (5, [0, 0, 255]),
        (6, [255, 0, 255]),
        (7, [0, 0, 0]),
        (8, [128, 128, 128]),
    ];
    let d = |a: [u8; 3]| -> i32 {
        (0..3)
            .map(|i| {
                let x = i32::from(a[i]) - i32::from(c[i]);
                x * x
            })
            .sum()
    };
    BASIC
        .iter()
        .min_by_key(|(_, rgb)| d(*rgb))
        .map(|(i, _)| *i)
        .unwrap_or(7)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x: f64, y: f64, s: f64, key: &str) -> LaserItem {
        LaserItem {
            key: key.into(),
            color: [255, 0, 0],
            paths: vec![crate::cut::rect_path(Rect::new(x, y, x + s, y + s))],
        }
    }

    fn with(key: &str, ls: LayerSpec) -> LaserSpec {
        let mut s = LaserSpec::default();
        *s.layer_mut(key) = ls;
        s
    }

    #[test]
    fn a_cut_square_burns_its_outline_once_per_pass() {
        let spec = with(
            "#FF0000",
            LayerSpec {
                passes: 2,
                ..LayerSpec::for_mode(Mode::Cut)
            },
        );
        let p = program(&[square(10.0, 10.0, 20.0, "#FF0000")], &spec, Point::ZERO);
        assert_eq!(p.layers, 1);
        assert!((p.burn_length - 160.0).abs() < 1e-6, "{}", p.burn_length);
        assert!(p.gcode.contains("G21\nG90\nM4 S0\n"), "{}", p.gcode);
        assert!(p.gcode.contains("S1000 F480"), "{}", p.gcode);
        assert!(p.gcode.contains("M8"), "air assist on for cutting");
        // X0 Y0 is the job's corner, where the square starts and ends.
        assert!(p.gcode.contains("G1 X20 Y0 S1000"), "{}", p.gcode);
        assert!(p.gcode.trim_end().ends_with("M5"), "{}", p.gcode);
        assert!(!p.gcode.contains("-0 "), "{}", p.gcode);
        assert_eq!(p.cuts.len(), 1);
    }

    #[test]
    fn engraving_fills_the_area_but_not_the_hole() {
        let mut ring = BezPath::new();
        ring.extend(crate::cut::rect_path(Rect::new(0.0, 0.0, 10.0, 10.0)));
        ring.extend(crate::cut::rect_path(Rect::new(3.0, 3.0, 7.0, 7.0)));
        let item = LaserItem {
            key: "k".into(),
            color: [0, 0, 0],
            paths: vec![ring],
        };
        let spec = with(
            "k",
            LayerSpec {
                interval: 1.0,
                ..LayerSpec::for_mode(Mode::Engrave)
            },
        );
        let p = program(&[item], &spec, Point::ZERO);
        // 10 lines: 6 full (10 mm), 4 across the hole (6 mm burnt).
        assert!((p.burn_length - (6.0 * 10.0 + 4.0 * 6.0)).abs() < 1e-6, "{}", p.burn_length);
        // Overscan runs past the ends with the beam off.
        assert!(p.gcode.contains("X-2 "), "{}", p.gcode);
        assert!(p.gcode.contains(" S0"), "{}", p.gcode);
    }

    #[test]
    fn engraving_runs_before_scoring_and_cutting_and_skip_is_left_out() {
        let mut spec = LaserSpec::default();
        *spec.layer_mut("cut") = LayerSpec::for_mode(Mode::Cut);
        *spec.layer_mut("eng") = LayerSpec::for_mode(Mode::Engrave);
        *spec.layer_mut("score") = LayerSpec::for_mode(Mode::Score);
        *spec.layer_mut("skip") = LayerSpec::for_mode(Mode::Skip);
        let items = [
            square(0.0, 0.0, 10.0, "cut"),
            square(20.0, 0.0, 10.0, "skip"),
            square(40.0, 0.0, 10.0, "score"),
            square(60.0, 0.0, 10.0, "eng"),
        ];
        let p = program(&items, &spec, Point::ZERO);
        assert_eq!(p.layers, 3);
        let at = |k: &str| p.gcode.find(&format!("; Layer {k}:")).expect(k);
        assert!(at("eng") < at("score") && at("score") < at("cut"));
        assert!(!p.gcode.contains("Layer skip"));
    }

    #[test]
    fn marlin_uses_inline_power_and_turns_it_off_on_moves() {
        let mut spec = with("k", LayerSpec::for_mode(Mode::Cut));
        spec.firmware = Firmware::Marlin;
        spec.max_power = 255.0;
        let p = program(&[square(0.0, 0.0, 10.0, "k")], &spec, Point::ZERO);
        assert!(p.gcode.contains("M3 I S0"), "{}", p.gcode);
        assert!(p.gcode.contains("S255"), "{}", p.gcode);
        assert!(p.gcode.contains("M5 I"), "{}", p.gcode);
        for l in p.gcode.lines().filter(|l| l.starts_with("G0 ")) {
            assert!(l.ends_with(" S0"), "{l}");
        }
    }

    #[test]
    fn grbl_constant_power_is_m3() {
        let mut spec = with("k", LayerSpec::for_mode(Mode::Cut));
        spec.dynamic = false;
        let p = program(&[square(0.0, 0.0, 10.0, "k")], &spec, Point::ZERO);
        assert!(p.gcode.contains("M3 S0") && !p.gcode.contains("M4"));
    }

    #[test]
    fn the_frame_goes_round_the_job() {
        let spec = LaserSpec::default();
        let f = frame(&[square(10.0, 10.0, 20.0, "#FF0000")], &spec, Point::ZERO).expect("frame");
        assert!(f.contains("M5\nG0 X0 Y0\nG1 X20 Y0 F3000\nG1 X20 Y20"), "{f}");
        let lit = LaserSpec {
            frame_power: 1.0,
            ..LaserSpec::default()
        };
        let f = frame(&[square(10.0, 10.0, 20.0, "#FF0000")], &lit, Point::ZERO).expect("frame");
        assert!(f.contains("M3 S10\n"), "{f}");
        assert!(frame(&[], &spec, Point::ZERO).is_none());
    }

    #[test]
    fn the_material_test_steps_power_and_speed() {
        let spec = LaserSpec {
            test: TestGrid {
                columns: 3,
                rows: 2,
                power_min: 10.0,
                power_max: 50.0,
                speed_min: 100.0,
                speed_max: 200.0,
                ..TestGrid::default()
            },
            ..LaserSpec::default()
        };
        let (items, run) = test_job(&spec);
        assert_eq!(items.len(), 7, "six squares and the labels");
        let powers: Vec<f64> = run.layers.iter().take(3).map(|l| l.spec.power).collect();
        assert_eq!(powers, vec![10.0, 30.0, 50.0]);
        assert_eq!(run.layer("test 2 1").speed, 200.0);
        let p = program(&items, &run, Point::ZERO);
        assert_eq!(p.layers, 7);
        assert!(p.gcode.contains("power 30%, speed 100 mm/s"), "{}", p.gcode);
        // Labels are scored after the engraved squares.
        assert!(p.gcode.find("Layer labels").expect("labels") > p.gcode.find("Layer test 2 3").expect("last"));
        // Nothing lands left of or below X0 Y0.
        for l in p.gcode.lines().filter(|l| l.starts_with('G')) {
            assert!(!l.contains("X-") || l.contains(" S0") || l.starts_with("G0"), "{l}");
        }
    }

    #[test]
    fn the_dxf_has_one_polyline_per_outline_and_drops_repeats() {
        let spec = LaserSpec::default();
        let items = [
            square(0.0, 0.0, 10.0, "#FF0000"),
            square(0.0, 0.0, 10.0, "#FF0000"),
            square(20.0, 0.0, 5.0, "#FF0000"),
        ];
        let d = dxf(&items, &spec, Point::ZERO);
        assert_eq!(d.matches("POLYLINE").count(), 2, "{d}");
        assert!(d.contains("AC1009"));
        assert!(d.contains("2\nFF0000\n70\n0\n62\n1\n"), "{d}");
        assert!(d.trim_end().ends_with("EOF"));
        assert!(!d.contains("HATCH") && !d.contains("TEXT"));
    }

    #[test]
    fn bad_numbers_do_not_break_the_program() {
        let spec = with(
            "k",
            LayerSpec {
                power: f64::NAN,
                speed: f64::INFINITY,
                interval: -1.0,
                passes: 0,
                mode: Mode::Engrave,
                air: false,
            },
        );
        let p = program(&[square(0.0, 0.0, 10.0, "k")], &spec, Point::new(f64::NAN, 0.0));
        assert!(!p.gcode.contains("NaN") && !p.gcode.contains("inf"), "{}", p.gcode);
        assert!(p.burn_length > 0.0);
        let empty = program(&[], &LaserSpec::default(), Point::ZERO);
        assert_eq!(empty.layers, 0);
        assert!(empty.gcode.contains("M5"));
    }

    #[test]
    fn items_go_by_colour_or_by_layer() {
        let mut shape = Shape::new(
            tracedraw_core::ShapeId(1),
            ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 5.0, 5.0),
                radius: 0.0,
                corners: None,
            },
        );
        shape.stroke = Some(tracedraw_core::Stroke {
            color: tracedraw_core::Color::rgb8(0, 0, 255),
            ..Default::default()
        });
        let by_color = items(&shape, "Cut lines", GroupBy::Color);
        assert_eq!(by_color.len(), 1);
        assert_eq!(by_color[0].key, "#0000FF");
        let by_layer = items(&shape, "Cut lines", GroupBy::Layer);
        assert_eq!(by_layer[0].key, "Cut lines");
        assert_eq!(layer_keys(&by_layer), vec![("Cut lines".to_string(), [0, 0, 255])]);
    }
}
