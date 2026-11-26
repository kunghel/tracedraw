//! TraceCut's Cut tools: plotter output (with the test cut), CNC
//! toolpaths, weeding, bridges, cut order, CutContour and registration
//! marks. The geometry
//! is `tracedraw_tracecut::cut`; this module keeps the tools' settings,
//! works their results out for the preview and turns them into commands.
//! See docs/behavior/tracecut.md.
//!
//! Adapted from the Sign & Cut cutting tools of #170 by LaMka21.

use crate::app::App;
use crate::i18n::{tr, trf};
use crate::tracecut_send::Destination;
use serde::{Deserialize, Serialize};
use tracedraw_core::geometry::{Affine, BezPath, PathEl, Point, Rect, Shape as _};
use tracedraw_core::{Color, Command, Fill, LayerId, Shape, ShapeId, ShapeKind, Stroke};
use tracedraw_tracecut::cnc::{self, CncSpec, Program};
use tracedraw_tracecut::laser::{self, LaserItem, LaserSpec};
use tracedraw_tracecut::cut::bridges::{self, BridgeCount, BridgeSpec};
use tracedraw_tracecut::cut::hpgl::{self, Plot, Profile};
use tracedraw_tracecut::cut::marks::{self, RegSpec};
use tracedraw_tracecut::cut::order::{self, Direction, RouteItem, RouteSpec, StartAt};
use tracedraw_tracecut::cut::weed::{self, WeedSpec};

/// The name of the spot ink that marks cut lines.
pub const CUT_CONTOUR: &str = "CutContour";

/// The process stand-in of the CutContour ink: 100 % magenta, so the
/// lines show on screen and stand out in a proof.
pub const CUT_CONTOUR_CMYK: [f32; 4] = [0.0, 1.0, 0.0, 0.0];

/// Plotter Output: the cutter's profile and where the job goes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlotterSettings {
    pub profile: Profile,
    /// Cut in the planned order (nearest next, inner outlines first)
    /// instead of the stacking order.
    pub optimize: bool,
    pub inner_first: bool,
    pub destination: Destination,
    /// The port path: `/dev/ttyUSB0`, `COM3`.
    pub port: String,
    /// The cutter on the network: `host` or `host:port`.
    pub host: String,
    /// The side of the test cut's square (mm).
    pub test_size: f64,
}

impl Default for PlotterSettings {
    fn default() -> Self {
        PlotterSettings {
            profile: Profile::default(),
            optimize: true,
            inner_first: true,
            destination: Destination::File,
            port: crate::tracecut_send::default_port(),
            host: String::new(),
            test_size: 20.0,
        }
    }
}

impl PlotterSettings {
    /// The port path or the host the job goes to.
    pub fn target(&self) -> String {
        match self.destination.usable() {
            Destination::Port => self.port.trim().to_string(),
            Destination::Network => self.host.trim().to_string(),
            Destination::File => String::new(),
        }
    }
}

/// Laser Output: the laser layers' settings, the frame, the material
/// test and where the program goes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LaserSettings {
    pub spec: LaserSpec,
    pub destination: Destination,
    /// The port path: `/dev/ttyUSB0`, `COM3`.
    pub port: String,
    /// The laser on the network: `host` or `host:port`.
    pub host: String,
    /// Send one line at a time and wait for the controller's `ok`, as
    /// GRBL and Marlin need (else the program goes out in one piece).
    pub wait_ok: bool,
}

impl Default for LaserSettings {
    fn default() -> Self {
        LaserSettings {
            spec: LaserSpec::default(),
            destination: Destination::File,
            port: crate::tracecut_send::default_port(),
            host: String::new(),
            wait_ok: true,
        }
    }
}

impl LaserSettings {
    /// The port path or the host the program goes to.
    pub fn target(&self) -> String {
        match self.destination.usable() {
            Destination::Port => self.port.trim().to_string(),
            Destination::Network => self.host.trim().to_string(),
            Destination::File => String::new(),
        }
    }
}

/// Bridges, as the window shows them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BridgeSettings {
    /// Length of each bridge (mm).
    pub length: f64,
    /// A count per outline (else one every `every` mm).
    pub per_outline: bool,
    pub count: u32,
    pub every: f64,
    /// Only outlines longer than this get bridges (mm).
    pub min_outline: f64,
}

impl Default for BridgeSettings {
    fn default() -> Self {
        BridgeSettings {
            length: 2.0,
            per_outline: true,
            count: 2,
            every: 100.0,
            min_outline: 0.0,
        }
    }
}

impl BridgeSettings {
    pub fn spec(&self) -> BridgeSpec {
        BridgeSpec {
            length: self.length,
            count: if self.per_outline {
                BridgeCount::PerOutline(self.count)
            } else {
                BridgeCount::Every(self.every)
            },
            min_outline: self.min_outline,
        }
    }
}

/// Cut Order: start points, directions and inner outlines first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OrderSettings {
    pub direction: Direction,
    pub start: StartAt,
    pub inner_first: bool,
}

impl Default for OrderSettings {
    fn default() -> Self {
        OrderSettings {
            direction: Direction::Keep,
            start: StartAt::Nearest,
            inner_first: true,
        }
    }
}

/// CutContour: the outline width (0: a hairline) and whether the fill
/// goes; a bleed around each object in its own colour, and cut lines
/// that go round each object's (or group's) outer edge as one, its
/// overlaps merged (#2098).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CutContourSettings {
    pub width: f64,
    pub remove_fill: bool,
    /// How far the bleed reaches outside each object, mm (0: none).
    pub bleed: f64,
    /// The cut line is a new curve round each object's area, its
    /// overlapping parts (letters, pieces of a design) merged into one
    /// outline; a group always gets one shared line.
    pub merge: bool,
}

/// The bleed CutContour adds reaches up to 20 mm (#2098).
pub const BLEED_RANGE: (f64, f64) = (0.0, 20.0);

/// Registration marks: their look, and whether they go inside the page
/// (else around the selection).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MarksSettings {
    pub spec: RegSpec,
    pub page: bool,
}

/// A change of the drawing worked out by a Cut tool.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    /// A new group of `paths` named by the i18n key `name`, drawn as
    /// `style` says, on the active layer.
    AddGroup {
        name: &'static str,
        paths: Vec<BezPath>,
        style: AddStyle,
    },
    /// A new group named by the i18n key `name` holding one group for
    /// each part: its i18n name, its paths and how they are drawn.
    AddNested {
        name: &'static str,
        parts: Vec<(&'static str, Vec<BezPath>, AddStyle)>,
    },
    /// A command as it is.
    Cmd(Box<Command>),
    /// A new curve `path` (page space) just behind (or, `above`, just in
    /// front of) the object `near`, on its layer, named by the i18n key
    /// `name`, filled and outlined as given (#2098).
    AddNear {
        near: ShapeId,
        above: bool,
        name: &'static str,
        path: BezPath,
        fill: Fill,
        stroke: Option<Stroke>,
    },
}

/// How new objects look.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AddStyle {
    /// A hairline, no fill (weeding lines).
    Hairline,
    /// Filled black, no outline (registration marks).
    Filled,
    /// A hairline of this colour, no fill (one pass of a cut).
    Line(Color),
}

/// What a Cut tool worked out: the changes for OK in one undo step
/// named `label`, what the preview adds, and the status line.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Changes {
    pub label: &'static str,
    pub changes: Vec<Change>,
    /// Outlines to draw over the objects in the preview.
    pub preview: Vec<BezPath>,
    /// Lines grooved or scored rather than cut through, drawn in the
    /// preview in a colour of their own.
    pub scored: Vec<BezPath>,
    /// Places to ring in the preview.
    pub points: Vec<Point>,
    /// Moves with the knife up, for the preview.
    pub moves: Vec<(Point, Point)>,
    /// Eyelets for the preview: centre and hole diameter, drawn as
    /// grommet rings (#2759).
    pub rings: Vec<(Point, f64)>,
    /// Guide lines drawn dashed in the preview (where the eyelets go).
    pub guides: Vec<BezPath>,
    pub summary: String,
    /// The preview shows the result in place of the objects (nesting).
    pub hide_objects: bool,
}

/// A plotter job worked out for the preview and for OK.
#[derive(Debug, Clone, PartialEq)]
pub struct Job {
    pub plot: Plot,
    /// The outlines in cut order (page space).
    pub cuts: Vec<BezPath>,
    pub moves: Vec<(Point, Point)>,
}

/// The inverse of `t`, when it has one.
fn invert(t: Affine) -> Option<Affine> {
    let det = t.determinant();
    (det.is_finite() && det.abs() > 1e-12).then(|| t.inverse())
}

/// True when every subpath of `path` is closed.
fn all_closed(path: &BezPath) -> bool {
    let els = path.elements();
    let starts = els
        .iter()
        .filter(|e| matches!(e, PathEl::MoveTo(_)))
        .count();
    let closes = els
        .iter()
        .filter(|e| matches!(e, PathEl::ClosePath))
        .count();
    starts > 0 && starts == closes
}

/// Each subpath of `path` as a path of its own.
fn split_subpaths(path: &BezPath) -> Vec<BezPath> {
    let mut out: Vec<BezPath> = Vec::new();
    for el in path.elements() {
        if matches!(el, PathEl::MoveTo(_)) || out.is_empty() {
            out.push(BezPath::new());
        }
        if let Some(last) = out.last_mut() {
            last.push(*el);
        }
    }
    out.retain(|p| p.elements().len() > 1);
    out
}

/// The moves with the knife up between outlines cut in this order, from
/// `from`.
fn moves_between(cuts: &[BezPath], from: Point) -> Vec<(Point, Point)> {
    let mut at = from;
    let mut out = Vec::new();
    for c in cuts {
        let els = c.elements();
        let (Some(start), Some(end)) = (
            els.first().and_then(|e| e.end_point()),
            els.iter().rev().find_map(|e| e.end_point()),
        ) else {
            continue;
        };
        let closed = matches!(els.last(), Some(PathEl::ClosePath));
        if (start - at).hypot() > 1e-9 {
            out.push((at, start));
        }
        at = if closed { start } else { end };
    }
    out
}

/// The box of the finite, non-empty paths.
fn bounds_of(paths: &[BezPath]) -> Option<Rect> {
    paths
        .iter()
        .filter(|p| !p.elements().is_empty())
        .map(|p| p.bounding_box())
        .filter(|b| b.x0.is_finite() && b.y0.is_finite() && b.x1.is_finite() && b.y1.is_finite())
        .reduce(|a, b| a.union(b))
}

/// A length for the status line, in the document's units.
pub fn fmt_len(u: crate::app::Units, mm: f64) -> String {
    format!("{:.1} {}", u.world_from_mm(mm), u.short())
}

impl App {
    /// The objects among `ids` whose outline a Cut tool may rewrite:
    /// shapes and curves, groups opened, unlocked, without live effects.
    pub fn tracecut_leaves(&self, ids: &[ShapeId]) -> Vec<ShapeId> {
        fn walk(s: &Shape, out: &mut Vec<ShapeId>) {
            if s.locked {
                return;
            }
            match &s.kind {
                ShapeKind::Group { children } => {
                    for c in children {
                        walk(c, out);
                    }
                }
                ShapeKind::Rect { .. }
                | ShapeKind::Ellipse { .. }
                | ShapeKind::Polygon { .. }
                | ShapeKind::Path { .. }
                    if s.effects.is_empty() =>
                {
                    out.push(s.id);
                }
                _ => {}
            }
        }
        let doc = self.doc();
        let mut out = Vec::new();
        for id in ids {
            if let Some(s) = doc.find_shape(*id) {
                walk(s, &mut out);
            }
        }
        out
    }

    /// The plotter job of the selection: its outlines in stacking order,
    /// or planned (nearest next, inner outlines first) from the job's
    /// bottom-left corner.
    pub fn tracecut_job(&self, s: &PlotterSettings) -> Job {
        let ids = self.tracecut_targets();
        let outlines = self.tracecut_outlines(&ids);
        let all: Vec<BezPath> = outlines.iter().flatten().cloned().collect();
        let corner = bounds_of(&all)
            .map(|b| Point::new(b.x0, b.y0))
            .unwrap_or(Point::ZERO);
        let cuts: Vec<BezPath> = if s.optimize {
            let items: Vec<RouteItem> = outlines
                .iter()
                .map(|paths| RouteItem {
                    paths: paths.clone(),
                    editable: true,
                })
                .collect();
            let spec = RouteSpec {
                direction: Direction::Keep,
                start: StartAt::Nearest,
                inner_first: s.inner_first,
                from: corner,
            };
            let route = order::plan(&items, &spec);
            route
                .order
                .iter()
                .filter_map(|i| route.cuts.get(*i))
                .flatten()
                .cloned()
                .collect()
        } else {
            all.iter().flat_map(split_subpaths).collect()
        };
        let plot = hpgl::plot(&cuts, &s.profile);
        let moves = moves_between(&cuts, corner);
        Job { plot, cuts, moves }
    }

    /// The test cut as a job.
    pub fn tracecut_test_job(&self, s: &PlotterSettings) -> Plot {
        hpgl::plot(&hpgl::test_cut(s.test_size), &s.profile)
    }

    /// Send or save `plot`: a `.plt` file where the user says, or the
    /// port or the network on a thread of its own.
    pub fn tracecut_output(
        &mut self,
        s: &PlotterSettings,
        plot: &Plot,
        ctx: Option<egui::Context>,
    ) {
        let bytes = plot.text.clone().into_bytes();
        match s.destination.usable() {
            Destination::File => {
                crate::files::Dialog::new()
                    .add_filter("PLT (HPGL)", &["plt"])
                    .set_file_name("cut.plt")
                    .save_file(self, move |app, path| {
                        app.status = match crate::files::write(&path, bytes) {
                            Ok(()) => {
                                trf("tracecut.saved", &[("path", &path.display().to_string())])
                            }
                            Err(e) => e.to_string(),
                        };
                    });
            }
            dest => {
                let target = s.target();
                if target.is_empty() {
                    self.status = tr("tracecut.no_target");
                    return;
                }
                self.status = trf("tracecut.sending", &[("to", &target)]);
                crate::tracecut_send::start(dest, target, bytes, ctx);
            }
        }
    }

    /// The laser items of the selection, each part of a group by its own
    /// colour or by the layer its object is on.
    pub fn tracecut_laser_items(&self, by: laser::GroupBy) -> Vec<LaserItem> {
        let doc = self.doc();
        let layers = self.page_layers();
        let layer_of = |id: ShapeId| -> String {
            fn holds(s: &Shape, id: ShapeId) -> bool {
                s.id == id
                    || match &s.kind {
                        ShapeKind::Group { children } => children.iter().any(|c| holds(c, id)),
                        ShapeKind::ClipFrame { frame, .. } => holds(frame, id),
                        _ => false,
                    }
            }
            layers
                .iter()
                .find(|l| l.shapes.iter().any(|s| holds(s, id)))
                .map(|l| l.name.clone())
                .unwrap_or_default()
        };
        let mut out = Vec::new();
        for id in self.tracecut_targets() {
            if let Some(s) = doc.shape_on_page(id) {
                out.extend(laser::items(&s, &layer_of(id), by));
            }
        }
        out
    }

    /// Laser Output: the G-code program of the selection.
    pub fn tracecut_laser(&self, s: &LaserSettings) -> laser::Program {
        let items = self.tracecut_laser_items(s.spec.group_by);
        laser::program(&items, &s.spec, self.ruler_origin())
    }

    /// The laser program's line for the status strip.
    pub fn tracecut_laser_summary(&self, p: &laser::Program) -> String {
        if p.layers == 0 {
            return tr("tracecut.nothing_to_cut");
        }
        let (w, h) = p.bounds.map(|b| (b.width(), b.height())).unwrap_or_default();
        trf(
            "tracecut.laser_found",
            &[
                ("layers", &p.layers.to_string()),
                ("burn", &fmt_len(self.units, p.burn_length)),
                ("w", &fmt_len(self.units, w)),
                ("h", &fmt_len(self.units, h)),
                ("min", &format!("{:.1}", p.minutes)),
            ],
        )
    }

    /// Send or save a laser program: a G-code file where the user says,
    /// or the port or the network on a thread of its own.
    pub fn tracecut_laser_output(
        &mut self,
        s: &LaserSettings,
        gcode: String,
        file_name: &str,
        ctx: Option<egui::Context>,
    ) {
        match s.destination.usable() {
            Destination::File => {
                let bytes = gcode.into_bytes();
                crate::files::Dialog::new()
                    .add_filter("G-code", &["gcode", "nc", "ngc", "gc"])
                    .set_file_name(file_name)
                    .save_file(self, move |app, path| {
                        app.status = match crate::files::write(&path, bytes) {
                            Ok(()) => {
                                trf("tracecut.saved", &[("path", &path.display().to_string())])
                            }
                            Err(e) => e.to_string(),
                        };
                    });
            }
            dest => {
                let target = s.target();
                if target.is_empty() {
                    self.status = tr("tracecut.no_target");
                    return;
                }
                self.status = trf("tracecut.sending", &[("to", &target)]);
                if s.wait_ok {
                    crate::tracecut_send::start_gcode(dest, target, gcode, ctx);
                } else {
                    crate::tracecut_send::start(dest, target, gcode.into_bytes(), ctx);
                }
            }
        }
    }

    /// Run the frame of the selection's laser job.
    pub fn tracecut_laser_frame(&mut self, s: &LaserSettings, ctx: Option<egui::Context>) {
        let items = self.tracecut_laser_items(s.spec.group_by);
        match laser::frame(&items, &s.spec, self.ruler_origin()) {
            Some(g) => self.tracecut_laser_output(s, g, "frame.gcode", ctx),
            None => self.status = tr("tracecut.nothing_to_cut"),
        }
    }

    /// Run the material test grid.
    pub fn tracecut_laser_test(&mut self, s: &LaserSettings, ctx: Option<egui::Context>) {
        let (items, spec) = laser::test_job(&s.spec);
        let p = laser::program(&items, &spec, Point::ZERO);
        self.tracecut_laser_output(s, p.gcode, "material-test.gcode", ctx);
    }

    /// Save the selection's laser lines as a clean DXF where the user says.
    pub fn tracecut_laser_dxf(&mut self, s: &LaserSettings) {
        let items = self.tracecut_laser_items(s.spec.group_by);
        if items.is_empty() {
            self.status = tr("tracecut.nothing_to_cut");
            return;
        }
        let bytes = laser::dxf(&items, &s.spec, self.ruler_origin()).into_bytes();
        crate::files::Dialog::new()
            .add_filter("DXF", &["dxf"])
            .set_file_name("laser.dxf")
            .save_file(self, move |app, path| {
                app.status = match crate::files::write(&path, bytes) {
                    Ok(()) => trf("tracecut.saved", &[("path", &path.display().to_string())]),
                    Err(e) => e.to_string(),
                };
            });
    }

    /// CNC Toolpath: the G-code program of the selection, X0 Y0 at the
    /// bottom-left corner of its box or at the ruler origin.
    pub fn tracecut_cnc(&self, spec: &CncSpec) -> Program {
        let outlines = self.tracecut_outlines(&self.tracecut_targets());
        cnc::program(&outlines, spec, self.ruler_origin())
    }

    /// The program's line for the status strip.
    pub fn tracecut_cnc_summary(&self, p: &Program) -> String {
        let u = self.units;
        if p.toolpaths.is_empty() {
            return if p.skipped > 0 {
                trf("tracecut.cnc_skipped", &[("n", &p.skipped.to_string())])
            } else {
                tr("tracecut.nothing_to_cut")
            };
        }
        let mut line = trf(
            "tracecut.cnc_found",
            &[
                ("n", &p.toolpaths.len().to_string()),
                ("passes", &p.passes.to_string()),
                ("cut", &fmt_len(u, p.cut_length)),
                ("min", &format!("{:.1}", p.minutes)),
            ],
        );
        if p.skipped > 0 {
            line.push(' ');
            line.push_str(&trf(
                "tracecut.cnc_skipped",
                &[("n", &p.skipped.to_string())],
            ));
        }
        line
    }

    /// Save `p` as a G-code file where the user says.
    pub fn tracecut_save_gcode(&mut self, p: &Program) {
        let bytes = p.gcode.clone().into_bytes();
        crate::files::Dialog::new()
            .add_filter("G-code", &["nc", "gcode", "ngc", "tap"])
            .set_file_name("toolpath.nc")
            .save_file(self, move |app, path| {
                app.status = match crate::files::write(&path, bytes) {
                    Ok(()) => trf("tracecut.saved", &[("path", &path.display().to_string())]),
                    Err(e) => e.to_string(),
                };
            });
    }

    /// The job's line for the status strip.
    pub fn tracecut_job_summary(&self, job: &Job) -> String {
        let u = self.units;
        if job.plot.outlines == 0 {
            return tr("tracecut.nothing_to_cut");
        }
        trf(
            "tracecut.job_summary",
            &[
                ("n", &job.plot.outlines.to_string()),
                ("cut", &fmt_len(u, job.plot.cut_length)),
                ("travel", &fmt_len(u, job.plot.travel)),
                ("w", &fmt_len(u, job.plot.width)),
                ("h", &fmt_len(u, job.plot.height)),
            ],
        )
    }

    /// Weeding lines, boxes and frame around the selection.
    pub fn tracecut_weeding(&self, spec: &WeedSpec) -> Changes {
        let outlines = self.tracecut_outlines(&self.tracecut_targets());
        let paths = weed::weeding(&outlines, spec);
        let summary = if paths.is_empty() {
            tr("tracecut.nothing_to_cut")
        } else {
            trf("tracecut.weeding_found", &[("n", &paths.len().to_string())])
        };
        Changes {
            label: "Weeding",
            preview: paths.clone(),
            changes: if paths.is_empty() {
                Vec::new()
            } else {
                vec![Change::AddGroup {
                    name: "tracecut.weeding_group",
                    paths,
                    style: AddStyle::Hairline,
                }]
            },
            summary,
            ..Default::default()
        }
    }

    /// Bridges in the closed outlines of the selected shapes and curves.
    pub fn tracecut_bridges(&self, s: &BridgeSettings) -> Changes {
        let spec = s.spec();
        let doc = self.doc();
        let mut out = Changes {
            label: "Bridges",
            ..Default::default()
        };
        let mut n = 0;
        for id in self.tracecut_leaves(&self.tracecut_targets()) {
            let Some(shape) = doc.shape_on_page(id) else {
                continue;
            };
            let page = shape.page_path();
            let Some(b) = bridges::bridge_path(&page, &spec) else {
                continue;
            };
            let Some(inv) = invert(shape.transform) else {
                continue;
            };
            n += b.bridges;
            out.points.extend(b.centres.iter().copied());
            out.preview.push(b.path.clone());
            out.changes
                .push(Change::Cmd(Box::new(Command::SetShapeKind {
                    shape: id,
                    kind: ShapeKind::Path {
                        path: inv * b.path,
                        closed: false,
                    },
                })));
        }
        out.summary = if n == 0 {
            tr("tracecut.no_bridges")
        } else {
            trf("tracecut.bridges_found", &[("n", &n.to_string())])
        };
        out
    }

    /// The cut order of the selection, layer by layer: the objects
    /// reordered in their layers, the subpaths of shapes and curves given
    /// their new order, start and direction.
    pub fn tracecut_cut_order(&self, s: &OrderSettings) -> Changes {
        let targets = self.tracecut_targets();
        let doc = self.doc();
        let mut out = Changes {
            label: "Cut Order",
            ..Default::default()
        };
        let mut from = Point::ZERO;
        let (mut before, mut after) = (0.0, 0.0);
        for layer in self.page_layers() {
            if layer.locked {
                continue;
            }
            let ids: Vec<ShapeId> = layer
                .shapes
                .iter()
                .filter(|sh| !sh.locked && targets.contains(&sh.id))
                .map(|sh| sh.id)
                .collect();
            if ids.is_empty() {
                continue;
            }
            let items: Vec<RouteItem> = ids
                .iter()
                .map(|id| {
                    let sh = doc.shape_on_page(*id);
                    let editable = sh.as_ref().is_some_and(|sh| {
                        sh.effects.is_empty()
                            && matches!(
                                sh.kind,
                                ShapeKind::Rect { .. }
                                    | ShapeKind::Ellipse { .. }
                                    | ShapeKind::Polygon { .. }
                                    | ShapeKind::Path { .. }
                            )
                    });
                    RouteItem {
                        paths: sh
                            .map(|sh| tracedraw_tracecut::outlines(&sh))
                            .unwrap_or_default(),
                        editable,
                    }
                })
                .collect();
            let spec = RouteSpec {
                direction: s.direction,
                start: s.start,
                inner_first: s.inner_first,
                from,
            };
            let route = order::plan(&items, &spec);
            from = route.end;
            before += route.travel_before;
            after += route.travel_after;
            out.moves.extend(route.moves.iter().copied());
            // The layer's list with the targets' places taken in route
            // order, then one move per object out of place.
            let wanted: Vec<ShapeId> = route
                .order
                .iter()
                .filter_map(|i| ids.get(*i))
                .copied()
                .collect();
            let mut next = wanted.iter();
            let final_order: Vec<ShapeId> = layer
                .shapes
                .iter()
                .map(|sh| {
                    if ids.contains(&sh.id) {
                        next.next().copied().unwrap_or(sh.id)
                    } else {
                        sh.id
                    }
                })
                .collect();
            let mut now: Vec<ShapeId> = layer.shapes.iter().map(|sh| sh.id).collect();
            for (i, id) in final_order.iter().enumerate() {
                if now.get(i) == Some(id) {
                    continue;
                }
                if let Some(from_i) = now.iter().position(|x| x == id) {
                    let moved = now.remove(from_i);
                    now.insert(i, moved);
                    out.changes.push(Change::Cmd(Box::new(Command::Reorder {
                        shape: *id,
                        layer: layer.id,
                        index: i,
                    })));
                }
            }
            for (i, changed) in route.changed.iter().enumerate() {
                let (Some(id), Some(cuts)) = (ids.get(i), route.cuts.get(i)) else {
                    continue;
                };
                out.preview.extend(cuts.iter().cloned());
                if !changed {
                    continue;
                }
                let Some(inv) = doc.shape_on_page(*id).and_then(|sh| invert(sh.transform)) else {
                    continue;
                };
                let mut path = BezPath::new();
                for c in cuts {
                    path.extend(c.elements().iter().copied());
                }
                let closed = all_closed(&path);
                out.changes
                    .push(Change::Cmd(Box::new(Command::SetShapeKind {
                        shape: *id,
                        kind: ShapeKind::Path {
                            path: inv * path,
                            closed,
                        },
                    })));
            }
        }
        let u = self.units;
        out.summary = if out.preview.is_empty() {
            tr("tracecut.nothing_to_cut")
        } else {
            trf(
                "tracecut.order_found",
                &[
                    ("before", &fmt_len(u, before)),
                    ("after", &fmt_len(u, after)),
                ],
            )
        };
        out
    }

    /// The CutContour ink of the drawing, and the command that adds it
    /// when the drawing has none.
    pub fn tracecut_contour_ink(&self) -> (Color, Option<Command>) {
        let inks = &self.doc().inks;
        if let Some(i) = inks.iter().find(|i| i.name == CUT_CONTOUR) {
            return (i.color(1.0), None);
        }
        let id = inks.iter().map(|i| i.id).max().unwrap_or(0) + 1;
        let ink = tracedraw_core::document::Ink {
            id,
            name: CUT_CONTOUR.into(),
            cmyk: CUT_CONTOUR_CMYK,
        };
        let color = ink.color(1.0);
        let mut all = inks.clone();
        all.push(ink);
        (color, Some(Command::SetInks { inks: all }))
    }

    /// Mark the selection's outlines as cut lines: a CutContour outline
    /// (the ink added to the drawing when it has none), the fill removed
    /// when asked.
    pub fn tracecut_cut_contour(&self, s: &CutContourSettings) -> Changes {
        let ids: Vec<ShapeId> = self
            .tracecut_targets()
            .into_iter()
            .filter(|id| self.doc().find_shape(*id).is_some_and(|sh| !sh.locked))
            .collect();
        let mut out = Changes {
            label: "CutContour",
            ..Default::default()
        };
        if ids.is_empty() {
            out.summary = tr("tracecut.select_first");
            return out;
        }
        let (color, add) = self.tracecut_contour_ink();
        if let Some(c) = add {
            out.changes.push(Change::Cmd(Box::new(c)));
        }
        // Its swatch joins the document palette in the same step, as a
        // colour applied from the Properties docker does (#1927).
        if self.settings.palette.auto_update_document && !self.doc().palette.contains(&color) {
            let mut colors = self.doc().palette.clone();
            colors.push(color);
            out.changes
                .push(Change::Cmd(Box::new(Command::SetDocumentPalette {
                    colors,
                })));
        }
        let width = if s.width.is_finite() && s.width > Stroke::HAIRLINE {
            s.width.min(10.0)
        } else {
            Stroke::HAIRLINE
        };
        let mut stroke = Stroke::hairline(color);
        stroke.width = width;
        let bleed = if s.bleed.is_finite() {
            s.bleed.clamp(BLEED_RANGE.0, BLEED_RANGE.1)
        } else {
            0.0
        };
        // Objects whose cut line is their own outline, and the areas of
        // those (groups, or all with Merge overlaps) that get a new one.
        let mut own = Vec::new();
        for id in &ids {
            let Some(sh) = self.doc().shape_on_page(*id) else {
                continue;
            };
            // A group, and a picture (its outline can't be stroked): a
            // new cut line round what they cover (#3245).
            let group = matches!(sh.kind, ShapeKind::Group { .. } | ShapeKind::Bitmap { .. });
            let area = (s.merge || group || bleed > 0.0).then(|| outer_area(&sh));
            if area.as_ref().is_some_and(|a| a.elements().is_empty()) {
                // Nothing closed to go round: the outline as it is.
                own.push(*id);
                continue;
            }
            if bleed > 0.0 {
                if let Some(area) = &area {
                    let band = tracedraw_core::shaping::offset(area, bleed);
                    if !band.elements().is_empty() {
                        out.preview.push(band.clone());
                        out.changes.push(Change::AddNear {
                            near: *id,
                            above: false,
                            name: "tracecut.bleed_name",
                            path: band,
                            fill: Fill::Solid(object_color(&sh)),
                            stroke: None,
                        });
                    }
                }
            }
            match area {
                Some(area) if s.merge || group => {
                    out.preview.push(area.clone());
                    out.changes.push(Change::AddNear {
                        near: *id,
                        above: true,
                        name: "tracecut.cut_line_name",
                        path: area,
                        fill: Fill::None,
                        stroke: Some(stroke.clone()),
                    });
                }
                _ => own.push(*id),
            }
        }
        if !own.is_empty() {
            out.changes.push(Change::Cmd(Box::new(Command::SetStroke {
                shapes: own.clone(),
                stroke: Some(stroke),
            })));
        }
        if s.remove_fill {
            out.changes.push(Change::Cmd(Box::new(Command::SetFill {
                shapes: ids.clone(),
                fill: Fill::None,
            })));
        }
        let outlines = self.tracecut_outlines(&own);
        out.preview.extend(outlines.into_iter().flatten());
        out.summary = trf("tracecut.contour_found", &[("n", &ids.len().to_string())]);
        out
    }

    /// Registration marks around the selection, or inside the page.
    pub fn tracecut_marks(&self, s: &MarksSettings) -> Changes {
        let area = if s.page {
            Some(self.page_rect())
        } else {
            bounds_of(
                &self
                    .tracecut_outlines(&self.tracecut_targets())
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>(),
            )
        };
        let spec = RegSpec {
            inside: s.page,
            ..s.spec
        };
        let paths = area
            .map(|a| marks::registration_marks(a, &spec))
            .unwrap_or_default();
        Changes {
            label: "Registration Marks",
            preview: paths.clone(),
            summary: if paths.is_empty() {
                tr("tracecut.select_first")
            } else {
                trf("tracecut.marks_found", &[("n", &paths.len().to_string())])
            },
            changes: if paths.is_empty() {
                Vec::new()
            } else {
                vec![Change::AddGroup {
                    name: "tracecut.marks_group",
                    paths,
                    style: AddStyle::Filled,
                }]
            },
            ..Default::default()
        }
    }

    /// Apply `c` in one undo step. Returns whether anything changed; the
    /// status line says what.
    pub fn tracecut_apply_changes(&mut self, c: &Changes) -> bool {
        if c.changes.is_empty() {
            self.status = c.summary.clone();
            return false;
        }
        let mut cmds = Vec::new();
        let mut added = Vec::new();
        // Objects put in each layer so far, below the ones still to come
        // (the targets go bottom to top), and in front of each target.
        let mut put: std::collections::HashMap<LayerId, usize> = std::collections::HashMap::new();
        let mut in_front: std::collections::HashMap<ShapeId, usize> =
            std::collections::HashMap::new();
        for ch in &c.changes {
            match ch {
                Change::Cmd(cmd) => cmds.push((**cmd).clone()),
                Change::AddNear {
                    near,
                    above,
                    name,
                    path,
                    fill,
                    stroke,
                } => {
                    let id = self.engine.new_shape_id();
                    let mut sh = Shape::new(
                        id,
                        ShapeKind::Path {
                            path: path.clone(),
                            closed: all_closed(path),
                        },
                    );
                    sh.fill = fill.clone();
                    sh.stroke = stroke.clone();
                    sh.name = Some(tr(name));
                    // Beside a top-level object; one inside a group gets
                    // it on top of the active layer.
                    match self.doc().locate(*near) {
                        Ok((layer, index)) => {
                            let shift = put.get(&layer).copied().unwrap_or(0);
                            let behind = index + shift;
                            let at = if *above {
                                behind + 1 + in_front.get(near).copied().unwrap_or(0)
                            } else {
                                behind
                            };
                            cmds.push(Command::AddShape { layer, shape: sh });
                            cmds.push(Command::Reorder {
                                shape: id,
                                layer,
                                index: at,
                            });
                            *put.entry(layer).or_default() += 1;
                            if *above {
                                *in_front.entry(*near).or_default() += 1;
                            }
                        }
                        Err(_) => {
                            let Some(layer) = self.active_layer() else {
                                continue;
                            };
                            cmds.push(Command::AddShape { layer, shape: sh });
                        }
                    }
                }
                Change::AddGroup { name, paths, style } => {
                    let Some(layer) = self.active_layer() else {
                        continue;
                    };
                    let g = self.tracecut_group(layer, name, paths, *style);
                    added.push(g.id);
                    cmds.push(Command::AddShape { layer, shape: g });
                }
                Change::AddNested { name, parts } => {
                    let Some(layer) = self.active_layer() else {
                        continue;
                    };
                    let children = parts
                        .iter()
                        .map(|(n, paths, style)| self.tracecut_group(layer, n, paths, *style))
                        .collect();
                    let g = self.tracecut_group_of(name, children);
                    added.push(g.id);
                    cmds.push(Command::AddShape { layer, shape: g });
                }
            }
        }
        if cmds.is_empty() {
            self.status = tr("tracecut.no_layer");
            return false;
        }
        if let Err(e) = self.engine.run_batch(c.label, &cmds) {
            self.status = e.to_string();
            return false;
        }
        if !added.is_empty() {
            self.select(added);
        }
        self.status = c.summary.clone();
        true
    }

    /// A group of new objects drawn from `paths`.
    fn tracecut_group(
        &mut self,
        _layer: LayerId,
        name: &str,
        paths: &[BezPath],
        style: AddStyle,
    ) -> Shape {
        let children: Vec<Shape> = paths
            .iter()
            .map(|p| {
                let id = self.engine.new_shape_id();
                let closed = all_closed(p);
                let mut s = Shape::new(
                    id,
                    ShapeKind::Path {
                        path: p.clone(),
                        closed,
                    },
                );
                match style {
                    AddStyle::Hairline => {
                        s.fill = Fill::None;
                        s.stroke = Some(Stroke::hairline(Color::BLACK));
                    }
                    AddStyle::Filled => {
                        s.fill = Fill::Solid(Color::BLACK);
                        s.stroke = None;
                    }
                    AddStyle::Line(c) => {
                        s.fill = Fill::None;
                        s.stroke = Some(Stroke::hairline(c));
                    }
                }
                s
            })
            .collect();
        self.tracecut_group_of(name, children)
    }

    /// A group of `children` named by the i18n key `name`.
    fn tracecut_group_of(&mut self, name: &str, children: Vec<Shape>) -> Shape {
        let id = self.engine.new_shape_id();
        let mut g = Shape::new(id, ShapeKind::Group { children });
        g.fill = Fill::None;
        g.stroke = None;
        g.name = Some(tr(name));
        g
    }
}

/// The area `shape` covers (page space; its outlines welded into one,
/// holes kept), for a cut line round it or a bleed (#2098).
fn outer_area(shape: &Shape) -> BezPath {
    let mut area = BezPath::new();
    for p in tracedraw_tracecut::outlines(shape) {
        if !all_closed(&p) || p.elements().is_empty() {
            continue;
        }
        area = if area.elements().is_empty() {
            tracedraw_core::shaping::simplify(&p)
        } else {
            tracedraw_core::shaping::overlay(&area, &p, tracedraw_core::shaping::Op::Weld)
        };
    }
    area
}

/// The colour a bleed takes: the object's fill (a gradient's first
/// colour), the first of a group's members that has one, else its
/// outline's, else black.
fn object_color(shape: &Shape) -> Color {
    fn fill_color(s: &Shape) -> Option<Color> {
        match &s.fill {
            Fill::Solid(c) => return Some(*c),
            Fill::Fountain(f) => {
                if let Some(st) = f.stops.first() {
                    return Some(st.color);
                }
            }
            _ => {}
        }
        if let ShapeKind::Group { children } = &s.kind {
            return children.iter().find_map(fill_color);
        }
        None
    }
    fn line_color(s: &Shape) -> Option<Color> {
        if let Some(st) = &s.stroke {
            return Some(st.color);
        }
        if let ShapeKind::Group { children } = &s.kind {
            return children.iter().find_map(line_color);
        }
        None
    }
    fill_color(shape)
        .or_else(|| line_color(shape))
        .unwrap_or(Color::BLACK)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(app: &mut App, x: f64, y: f64, w: f64, h: f64) -> ShapeId {
        app.new_shape(ShapeKind::Rect {
            rect: Rect::new(x, y, x + w, y + h),
            radius: 0.0,
            corners: None,
        })
        .expect("rect")
    }

    fn undo_len(app: &App) -> usize {
        app.engine.history_labels().0.len()
    }

    #[test]
    fn the_job_cuts_inner_outlines_first_and_goes_to_hpgl() {
        let mut app = App::headless();
        let outer = rect(&mut app, 0.0, 0.0, 100.0, 100.0);
        let inner = rect(&mut app, 40.0, 40.0, 20.0, 20.0);
        app.select(vec![outer, inner]);
        let s = PlotterSettings {
            profile: Profile {
                knife_offset: 0.0,
                overcut: 0.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let job = app.tracecut_job(&s);
        assert_eq!(job.cuts.len(), 2);
        // The inner square first.
        assert!(job.cuts[0].bounding_box().width() < 30.0);
        assert!(job.plot.text.starts_with("IN;"));
        assert_eq!(job.plot.outlines, 2);
        assert!((job.plot.width - 100.0).abs() < 1e-6);
        // In stacking order the outer square comes first.
        let plain = app.tracecut_job(&PlotterSettings {
            optimize: false,
            ..s.clone()
        });
        assert!(plain.cuts[0].bounding_box().width() > 90.0);
        assert!(app.tracecut_job_summary(&job).contains('2'));
        // Nothing selected: nothing to cut.
        app.select(Vec::new());
        assert_eq!(app.tracecut_job(&s).plot.outlines, 0);
    }

    #[test]
    fn the_cnc_program_mills_the_selection_holes_first() {
        let mut app = App::headless();
        let plate = rect(&mut app, 10.0, 10.0, 100.0, 60.0);
        let hole = rect(&mut app, 40.0, 30.0, 20.0, 20.0);
        app.select(vec![plate, hole]);
        let spec = CncSpec {
            tool_diameter: 4.0,
            ..Default::default()
        };
        let p = app.tracecut_cnc(&spec);
        assert_eq!(p.toolpaths.len(), 2);
        assert_eq!(p.toolpaths[0].side, cnc::Side::Inside);
        assert_eq!(p.toolpaths[1].side, cnc::Side::Outside);
        assert!(p.gcode.contains("G21") && p.gcode.trim_end().ends_with("M30"));
        // X0 Y0 at the bottom-left corner of the selection.
        assert_eq!(p.zero, Point::new(10.0, 10.0));
        let summary = app.tracecut_cnc_summary(&p);
        assert!(summary.contains('2'), "{summary}");
        // Working it out and showing it change nothing.
        let rev = app.engine.revision();
        let mut settings = crate::tracecut::ToolSettings::default();
        settings.cnc = spec;
        let w = app.tracecut_work_out(crate::tracecut::ToolId::CncToolpath, &settings);
        let scene = app.tracecut_scene(Some(&w));
        assert_eq!(scene.marked.len(), 2);
        assert!(!scene.moves.is_empty());
        assert_eq!(app.engine.revision(), rev);
        // A hole too small for the tool is said to be left out.
        app.select(Vec::new());
        assert_eq!(
            app.tracecut_cnc_summary(&app.tracecut_cnc(&spec)),
            tr("tracecut.nothing_to_cut")
        );
        let tiny = rect(&mut app, 200.0, 200.0, 2.0, 2.0);
        app.select(vec![tiny]);
        let inside = CncSpec {
            side: cnc::Side::Inside,
            ..spec
        };
        let p = app.tracecut_cnc(&inside);
        assert!(p.toolpaths.is_empty() && p.skipped == 1);
        assert_eq!(
            app.tracecut_cnc_summary(&p),
            trf("tracecut.cnc_skipped", &[("n", "1")])
        );
    }

    #[test]
    fn the_laser_program_goes_by_colour_and_cuts_last() {
        let mut app = App::headless();
        let plate = rect(&mut app, 10.0, 10.0, 100.0, 60.0);
        let logo = rect(&mut app, 40.0, 30.0, 20.0, 20.0);
        let blue = Stroke {
            color: Color::rgb8(0, 0, 255),
            ..Stroke::default()
        };
        app.engine
            .run(&Command::SetStroke {
                shapes: vec![logo],
                stroke: Some(blue),
            })
            .expect("stroke");
        app.select(vec![plate, logo]);
        let items = app.tracecut_laser_items(laser::GroupBy::Color);
        let keys: Vec<String> = laser::layer_keys(&items).into_iter().map(|(k, _)| k).collect();
        // The plate keeps the default outline colour, the logo is blue.
        assert_eq!(keys.len(), 2);
        assert_eq!(keys[1], "#0000FF");
        let plate_key = keys[0].clone();
        let by_layer = app.tracecut_laser_items(laser::GroupBy::Layer);
        assert_eq!(laser::layer_keys(&by_layer).len(), 1);
        let mut s = LaserSettings::default();
        *s.spec.layer_mut("#0000FF") = laser::LayerSpec::for_mode(laser::Mode::Engrave);
        let p = app.tracecut_laser(&s);
        assert_eq!(p.layers, 2);
        let eng = p.gcode.find("; Layer #0000FF").expect("engrave layer");
        let cut = p
            .gcode
            .find(&format!("; Layer {plate_key}"))
            .expect("cut layer");
        assert!(eng < cut, "engraving runs before cutting");
        assert_eq!(p.zero, Point::new(10.0, 10.0));
        assert!(app.tracecut_laser_summary(&p).contains('2'));
        // Working it out and showing it change nothing.
        let rev = app.engine.revision();
        let mut settings = crate::tracecut::ToolSettings::default();
        settings.laser = s.clone();
        let w = app.tracecut_work_out(crate::tracecut::ToolId::LaserOutput, &settings);
        let scene = app.tracecut_scene(Some(&w));
        assert_eq!(scene.marked.len(), 1, "the cut outline");
        assert!(!scene.scored.is_empty(), "the engraving lines");
        assert_eq!(app.engine.revision(), rev);
        // The layer settings are kept with the tool's presets.
        let json = settings.to_json(crate::tracecut::ToolId::LaserOutput);
        let mut back = crate::tracecut::ToolSettings::default();
        assert!(back.load_json(crate::tracecut::ToolId::LaserOutput, &json));
        assert_eq!(back.laser.spec.layer("#0000FF").mode, laser::Mode::Engrave);
        app.select(Vec::new());
        assert_eq!(
            app.tracecut_laser_summary(&app.tracecut_laser(&s)),
            tr("tracecut.nothing_to_cut")
        );
    }

    #[test]
    fn the_test_cut_is_a_small_job() {
        let app = App::headless();
        let p = app.tracecut_test_job(&PlotterSettings::default());
        assert_eq!(p.outlines, 2);
        assert!((p.width - 20.0).abs() < 1e-9);
    }

    #[test]
    fn weeding_adds_one_group_of_hairlines_in_one_step() {
        let mut app = App::headless();
        let a = rect(&mut app, 0.0, 0.0, 20.0, 20.0);
        let b = rect(&mut app, 40.0, 0.0, 20.0, 20.0);
        app.select(vec![a, b]);
        let c = app.tracecut_weeding(&WeedSpec::default());
        assert!(!c.preview.is_empty());
        let before = undo_len(&app);
        assert!(app.tracecut_apply_changes(&c));
        assert_eq!(undo_len(&app), before + 1);
        let g = app
            .doc()
            .find_shape(app.selection[0])
            .expect("group")
            .clone();
        let ShapeKind::Group { children } = &g.kind else {
            panic!("a group");
        };
        assert_eq!(children.len(), c.preview.len());
        assert!(children
            .iter()
            .all(|s| s.fill == Fill::None && s.stroke.is_some()));
        app.undo();
        assert!(app.doc().find_shape(g.id).is_none());
    }

    #[test]
    fn bridges_open_the_outlines_in_one_step() {
        let mut app = App::headless();
        let a = rect(&mut app, 0.0, 0.0, 50.0, 50.0);
        app.select(vec![a]);
        let c = app.tracecut_bridges(&BridgeSettings::default());
        assert_eq!(c.points.len(), 2);
        let before = undo_len(&app);
        assert!(app.tracecut_apply_changes(&c));
        assert_eq!(undo_len(&app), before + 1);
        let s = app.doc().find_shape(a).expect("shape");
        let ShapeKind::Path { path, closed } = &s.kind else {
            panic!("a curve");
        };
        assert!(!closed);
        assert_eq!(
            path.elements()
                .iter()
                .filter(|e| matches!(e, PathEl::MoveTo(_)))
                .count(),
            2
        );
        // The page stays where it was: the cut pieces lie on the square.
        let b = s.page_path().bounding_box();
        assert!((b.x0 - 0.0).abs() < 1e-6 && (b.x1 - 50.0).abs() < 1e-6);
    }

    #[test]
    fn cut_order_reorders_the_layer_in_one_step() {
        let mut app = App::headless();
        let far = rect(&mut app, 200.0, 0.0, 10.0, 10.0);
        let near = rect(&mut app, 0.0, 0.0, 10.0, 10.0);
        let mid = rect(&mut app, 100.0, 0.0, 10.0, 10.0);
        app.select(vec![far, near, mid]);
        let c = app.tracecut_cut_order(&OrderSettings::default());
        let before = undo_len(&app);
        assert!(app.tracecut_apply_changes(&c));
        assert_eq!(undo_len(&app), before + 1);
        let layer = app.active_layer().expect("layer");
        let order: Vec<ShapeId> = app
            .doc()
            .layer(layer)
            .expect("layer")
            .shapes
            .iter()
            .map(|s| s.id)
            .collect();
        assert_eq!(order, vec![near, mid, far]);
    }

    #[test]
    fn cut_contour_adds_the_ink_once_and_marks_the_outlines() {
        let mut app = App::headless();
        let a = rect(&mut app, 0.0, 0.0, 20.0, 20.0);
        app.select(vec![a]);
        let s = CutContourSettings {
            width: 0.0,
            remove_fill: true,
            ..Default::default()
        };
        let c = app.tracecut_cut_contour(&s);
        let before = undo_len(&app);
        assert!(app.tracecut_apply_changes(&c));
        assert_eq!(undo_len(&app), before + 1);
        let inks = app.doc().inks.clone();
        assert_eq!(inks.iter().filter(|i| i.name == CUT_CONTOUR).count(), 1);
        let sh = app.doc().find_shape(a).expect("shape");
        assert_eq!(sh.fill, Fill::None);
        let stroke = sh.stroke.clone().expect("outline");
        assert!(matches!(stroke.color, Color::Spot { .. }));
        assert!((stroke.width - Stroke::HAIRLINE).abs() < 1e-12);
        // Its swatch is in the document palette, once, and the undo step
        // takes it out again with the ink (#1927).
        assert_eq!(app.doc().palette, vec![stroke.color]);
        // Again: the same ink, no second one, no second swatch.
        let (_, add) = app.tracecut_contour_ink();
        assert!(add.is_none());
        let again = app.tracecut_cut_contour(&s);
        assert!(app.tracecut_apply_changes(&again));
        assert_eq!(app.doc().palette, vec![stroke.color]);
        app.undo();
        app.undo();
        assert!(app.doc().palette.is_empty());
        assert!(app.doc().inks.iter().all(|i| i.name != CUT_CONTOUR));
        // With the document palette not following the colours used, it
        // is left as it is.
        let mut app = App::headless();
        app.settings.palette.auto_update_document = false;
        let a = rect(&mut app, 0.0, 0.0, 20.0, 20.0);
        app.select(vec![a]);
        let c = app.tracecut_cut_contour(&s);
        assert!(app.tracecut_apply_changes(&c));
        assert!(app.doc().palette.is_empty());
        // Kept by name in PDF, EPS and SVG.
        let pdf = tracedraw_io::pdf::document_to_pdf(app.doc());
        assert!(String::from_utf8_lossy(&pdf).contains("/Separation /CutContour"));
        let eps = tracedraw_io::eps::page_to_eps(app.doc(), 0);
        assert!(eps.contains("(CutContour) cvn"), "{eps}");
        assert!(tracedraw_io::svg::page_to_svg(app.doc(), 0)
            .contains("data-spot-stroke=\"CutContour\""));
    }

    /// #2186: the swatch CutContour adds goes by the name CutContour
    /// wherever it shows (the document palette, the Palette Manager, the
    /// status bar, a palette it is added to), never by its ink number,
    /// and the PDF's separation is named CutContour exactly.
    #[test]
    fn the_cut_contour_swatch_is_named_cut_contour() {
        let mut app = App::headless();
        let a = rect(&mut app, 0.0, 0.0, 20.0, 20.0);
        app.select(vec![a]);
        let c = app.tracecut_cut_contour(&CutContourSettings::default());
        assert!(app.tracecut_apply_changes(&c));
        let swatch = app.doc().palette.first().copied().expect("swatch");
        assert!(matches!(swatch, Color::Spot { .. }));
        assert_eq!(crate::ui::palette::name_of(&app, swatch), CUT_CONTOUR);
        let tip =
            crate::ui::palette::swatch_tip(&crate::ui::palette::name_of(&app, swatch), swatch);
        assert!(tip.starts_with("CutContour\n"), "{tip}");
        assert!(!tip.contains("Spot"), "{tip}");
        assert_eq!(
            crate::app::color_description_in(app.doc(), swatch),
            "CutContour 100%"
        );
        let stroke = app.doc().find_shape(a).and_then(|s| s.stroke.clone());
        let fill = Fill::Solid(swatch);
        assert_eq!(
            crate::app::fill_description_in(app.doc(), &fill),
            "CutContour 100%"
        );
        assert_eq!(stroke.map(|s| s.color), Some(swatch));
        // Added to one of the user's palettes, it keeps the name.
        let i = app.new_palette("Cut").expect("palette");
        assert!(app.add_colors_to_palette(i, &[swatch]));
        assert_eq!(
            app.palettes[i].colors,
            vec![(CUT_CONTOUR.to_string(), swatch)]
        );
        // The separation of the exported PDF.
        let pdf = tracedraw_io::pdf::document_to_pdf(app.doc());
        let pdf = String::from_utf8_lossy(&pdf);
        assert!(pdf.contains("/Separation /CutContour /DeviceCMYK"), "{pdf}");
        assert!(!pdf.contains("/Separation /Spot"));
    }

    #[test]
    fn registration_marks_go_around_the_selection_or_in_the_page() {
        let mut app = App::headless();
        let a = rect(&mut app, 50.0, 50.0, 40.0, 30.0);
        app.select(vec![a]);
        let c = app.tracecut_marks(&MarksSettings::default());
        assert_eq!(c.preview.len(), 4);
        let around = bounds_of(&c.preview).expect("marks");
        assert!(around.x0 < 50.0 && around.x1 > 90.0);
        let inside = app.tracecut_marks(&MarksSettings {
            page: true,
            ..Default::default()
        });
        let page = app.page_rect();
        let b = bounds_of(&inside.preview).expect("marks");
        assert!(b.x0 >= page.x0 && b.x1 <= page.x1 && b.y0 >= page.y0 && b.y1 <= page.y1);
        assert!(app.tracecut_apply_changes(&c));
        let g = app.doc().find_shape(app.selection[0]).expect("group");
        let ShapeKind::Group { children } = &g.kind else {
            panic!("a group");
        };
        assert!(children
            .iter()
            .all(|s| s.fill == Fill::Solid(Color::BLACK) && s.stroke.is_none()));
    }

    /// #2098: with a bleed and Merge, a group of two overlapping
    /// rectangles gets one Bleed behind it, grown by the bleed, in its
    /// own colour, and one Cut line above it round both, in one step.
    #[test]
    fn bleed_and_merged_cut_line_go_round_the_outer_shape() {
        let mut app = App::headless();
        let a = rect(&mut app, 0.0, 0.0, 20.0, 20.0);
        let b = rect(&mut app, 10.0, 0.0, 20.0, 20.0);
        let red = Color::Rgb {
            r: 1.0,
            g: 0.0,
            b: 0.0,
        };
        for id in [a, b] {
            app.engine
                .run(&Command::SetFill {
                    shapes: vec![id],
                    fill: Fill::Solid(red),
                })
                .expect("fill");
        }
        app.select(vec![a, b]);
        app.group_selection();
        let g = app.selection[0];
        let s = CutContourSettings {
            bleed: 2.0,
            merge: true,
            ..Default::default()
        };
        let c = app.tracecut_cut_contour(&s);
        let before = undo_len(&app);
        assert!(app.tracecut_apply_changes(&c));
        assert_eq!(undo_len(&app), before + 1);
        let shapes = &app.doc().pages[0].layers[0].shapes;
        assert_eq!(shapes.len(), 3);
        let (bleed, group, line) = (&shapes[0], &shapes[1], &shapes[2]);
        assert_eq!(group.id, g);
        assert_eq!(
            bleed.name.as_deref(),
            Some(tr("tracecut.bleed_name").as_str())
        );
        assert_eq!(bleed.fill, Fill::Solid(red));
        assert!(bleed.stroke.is_none());
        let bb = bleed.bounds();
        assert!(
            (bb.x0 + 2.0).abs() < 0.05 && (bb.x1 - 32.0).abs() < 0.05,
            "{bb:?}"
        );
        assert_eq!(
            line.name.as_deref(),
            Some(tr("tracecut.cut_line_name").as_str())
        );
        assert_eq!(line.fill, Fill::None);
        assert!(matches!(
            line.stroke.as_ref().map(|s| s.color),
            Some(Color::Spot { .. })
        ));
        let lb = line.bounds();
        assert!(lb.x0.abs() < 1e-6 && (lb.x1 - 30.0).abs() < 1e-6, "{lb:?}");
        app.undo();
        assert_eq!(app.doc().pages[0].layers[0].shapes.len(), 1);
    }

    /// A picture with see-through pixels gets its cut line round what
    /// shows of it, not round its box (#3245).
    #[test]
    fn a_picture_is_cut_round_its_opaque_pixels() {
        let mut app = App::headless();
        let img = image::RgbaImage::from_fn(100, 100, |x, y| {
            let (dx, dy) = (x as f64 - 49.5, y as f64 - 49.5);
            let a = if dx * dx + dy * dy <= 30.0 * 30.0 { 255 } else { 0 };
            image::Rgba([20, 120, 200, a])
        });
        let mut png = Vec::new();
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .expect("png");
        let id = app
            .new_shape(ShapeKind::Bitmap {
                rect: Rect::new(0.0, 0.0, 100.0, 100.0),
                width_px: 100,
                height_px: 100,
                png: png.into(),
                fx: None,
            })
            .expect("picture");
        app.select(vec![id]);
        let c = app.tracecut_cut_contour(&CutContourSettings::default());
        assert!(app.tracecut_apply_changes(&c));
        let shapes = &app.doc().pages[0].layers[0].shapes;
        assert_eq!(shapes.len(), 2, "the picture and its cut line");
        let line = &shapes[1];
        assert_eq!(line.fill, Fill::None);
        let lb = line.bounds();
        // The disc spans 20 to 80.
        assert!((lb.x0 - 20.0).abs() < 1.5 && (lb.x1 - 80.0).abs() < 1.5, "{lb:?}");
        assert!(!line.page_path().contains(Point::new(3.0, 3.0)));
    }

    #[test]
    fn a_send_without_a_target_says_so() {
        let mut app = App::headless();
        let a = rect(&mut app, 0.0, 0.0, 10.0, 10.0);
        app.select(vec![a]);
        let s = PlotterSettings::default();
        let job = app.tracecut_job(&s);
        let net = PlotterSettings {
            destination: Destination::Network,
            host: String::new(),
            ..s
        };
        app.tracecut_output(&net, &job.plot, None);
        if crate::tracecut_send::CAN_SEND {
            assert_eq!(app.status, tr("tracecut.no_target"));
        }
    }
}
