//! Tools > Workflow: the window where a workflow's steps are boxes joined
//! by wires (`flow.rs` holds the model and the run). Steps are added from
//! the Add list and dragged by their boxes; a wire is drawn from a step's
//! output dot (right) to another step's input dot (left), and taken off
//! by right-clicking the input dot. The selected step's settings show at
//! the right; a Compare step shows the results wired into it side by
//! side. Preview runs the workflow and shows every step's result in its
//! box; Run does the same and then carries out the Export steps (onto
//! the page as one undo step, or into files). Run on a Picture File runs
//! it once on a picture file instead of the Input steps' own pictures.
//! On the desktop the run is off the interface thread and can be
//! cancelled. See docs/behavior/workflow.md.

use crate::app::App;
use crate::flow::{self, Flow, NodeId, Outcome, Source, Step, Target, Value};
use crate::i18n::{tr, trf};
use crate::trace::Cancel;
use crate::ui::tips::{keyed, RichTip};
use egui::{Color32, Context, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use tracedraw_core::geometry::{Rect as MmRect, Shape as _, Size};
use tracedraw_core::{Color, Command, Fill, Shape, ShapeKind};

/// A step's box on the workflow canvas, points.
const NODE_W: f32 = 150.0;
const NODE_H: f32 = 128.0;
const TITLE_H: f32 = 22.0;
const PORT_R: f32 = 6.0;
/// The side of a step's result picture kept for its box and the Compare
/// view, pixels.
const THUMB: u32 = 320;
/// The longest side of a selection drawn into a picture for an Input
/// step, pixels.
const MAX_INPUT_SIDE: f64 = 2400.0;
/// The settings column's width.
const PANEL_W: f32 = 270.0;

/// What a drag on the canvas moves.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Drag {
    Node { id: NodeId, grab: Vec2 },
    Wire { from: NodeId },
    Pan,
}

/// A run in progress.
struct Job {
    cancel: Arc<Cancel>,
    rx: std::sync::mpsc::Receiver<Option<BTreeMap<NodeId, Outcome>>>,
    export: bool,
    anchor: Option<MmRect>,
}

/// The window's state, kept on the App while it is closed.
#[derive(Default)]
pub struct WorkflowState {
    pub flow: Flow,
    pub selected: Option<NodeId>,
    pub pan: Vec2,
    drag: Option<Drag>,
    pub results: BTreeMap<NodeId, Outcome>,
    thumbs: HashMap<NodeId, egui::TextureHandle>,
    /// The results' thumbnails are made again.
    stale_thumbs: bool,
    job: Option<Job>,
    pub status: String,
    /// The file the workflow was opened from or saved to.
    pub path: Option<PathBuf>,
}

/// A file dialog or other step the window asks for, carried out once
/// the state is back on the App.
enum Ask {
    New,
    Open,
    Save,
    Run { export: bool, file: Option<PathBuf> },
    RunOnFile,
    InputFile(NodeId),
    ExportFile(NodeId),
}

impl App {
    /// Tools > Workflow.
    pub fn open_workflow(&mut self) {
        self.dialog = crate::ui::dialogs::Dialog::Workflow;
    }

    /// The pictures of the workflow's Input steps: the selection (a lone
    /// picture's own pixels, anything else drawn into a picture) or each
    /// step's file; with `file`, that picture for every Input step. Also
    /// the place on the page results go (the selection's box, else a box
    /// of the picture's size at the page's centre).
    pub fn workflow_inputs(
        &self,
        flow: &Flow,
        file: Option<&std::path::Path>,
    ) -> (BTreeMap<NodeId, Value>, Option<MmRect>) {
        let mut given = BTreeMap::new();
        let mut anchor = None;
        let mut selection: Option<Option<Value>> = None;
        for n in flow.input_nodes() {
            let Step::Input { source } = &n.step else {
                continue;
            };
            let path = match (file, source) {
                (Some(f), _) => Some(f.to_path_buf()),
                (None, Source::File { path }) if !path.is_empty() => Some(PathBuf::from(path)),
                _ => None,
            };
            let v = match path {
                Some(p) => load_picture(&p),
                None => selection
                    .get_or_insert_with(|| self.selection_picture())
                    .clone(),
            };
            if let Some(v) = v {
                if anchor.is_none() {
                    anchor = if file.is_none() && matches!(source, Source::Selection) {
                        self.selection_bounds()
                    } else {
                        None
                    }
                    .or_else(|| self.centred_box(v.size()));
                }
                given.insert(n.id, v);
            }
        }
        (given, anchor)
    }

    /// The selection as a picture: a lone picture's pixels, else the
    /// selection drawn at up to 300 dpi, its longest side at most
    /// [`MAX_INPUT_SIDE`] pixels.
    fn selection_picture(&self) -> Option<Value> {
        let shapes = self.selected_shapes();
        if let [one] = shapes.as_slice() {
            if let ShapeKind::Bitmap { png, .. } = &one.kind {
                return crate::bitmap_fx::decode(png).map(|i| Value::Picture(Arc::new(i)));
            }
        }
        let b = self.selection_bounds()?;
        let side_mm = b.width().max(b.height()).max(0.1);
        let dpi = (MAX_INPUT_SIDE * 25.4 / side_mm).min(300.0);
        let (png, _, _, _) = self.render_selection_png(dpi, true)?;
        crate::bitmap_fx::decode(&png).map(|i| Value::Picture(Arc::new(i)))
    }

    /// A box of a picture `size` pixels big at the default resolution,
    /// centred on the page.
    fn centred_box(&self, size: (u32, u32)) -> Option<MmRect> {
        let dpi = crate::image_dpi::DEFAULT_DPI;
        let (w, h) = (size.0 as f64 * 25.4 / dpi, size.1 as f64 * 25.4 / dpi);
        let c = self.page_rect().center();
        Some(MmRect::new(
            c.x - w / 2.0,
            c.y - h / 2.0,
            c.x + w / 2.0,
            c.y + h / 2.0,
        ))
    }

    /// Carry out the Export steps of a finished run: onto the page over
    /// `anchor` (one undo step, the new objects selected) or into their
    /// files. How many results went out, and the messages of the ones
    /// that could not.
    pub fn workflow_export(
        &mut self,
        flow: &Flow,
        results: &BTreeMap<NodeId, Outcome>,
        anchor: Option<MmRect>,
    ) -> (usize, Vec<String>) {
        let mut done = 0;
        let mut errors = Vec::new();
        let mut cmds = Vec::new();
        let mut placed = Vec::new();
        let layer = self.active_layer();
        for id in flow.order() {
            let Some(Step::Export { target }) = flow.node(id).map(|n| &n.step) else {
                continue;
            };
            let Some(Ok(v)) = results.get(&id) else {
                continue;
            };
            match target {
                Target::Page => {
                    let (Some(layer), Some(rect)) =
                        (layer, anchor.or_else(|| self.centred_box(v.size())))
                    else {
                        continue;
                    };
                    let shape = self.flow_shape(v, rect);
                    placed.push(shape.id);
                    cmds.push(Command::AddShape { layer, shape });
                    done += 1;
                }
                Target::File { path } if path.is_empty() => {
                    errors.push(tr("flow.no_target_file"));
                }
                Target::File { path } => match self.flow_file_bytes(v, path) {
                    Some(bytes) => match crate::files::write(path, bytes) {
                        Ok(()) => done += 1,
                        Err(e) => errors.push(trf("flow.save_failed", &[("e", &e.to_string())])),
                    },
                    None => errors.push(trf("flow.save_failed", &[("e", path)])),
                },
            }
        }
        if !cmds.is_empty() {
            match self.engine.run_batch("Workflow", &cmds) {
                Ok(()) => self.selection = placed,
                Err(e) => errors.push(e.to_string()),
            }
        }
        (done, errors)
    }

    /// A value as an object filling `rect`: curves as a group (filled
    /// regions and outlined strokes, as a trace places them), a picture
    /// as a bitmap.
    fn flow_shape(&mut self, v: &Value, rect: MmRect) -> Shape {
        match v {
            Value::Picture(p) => {
                let id = self.engine.new_shape_id();
                let png = crate::bitmap_fx::encode(p).unwrap_or_default();
                let mut s = Shape::new(
                    id,
                    ShapeKind::Bitmap {
                        rect,
                        width_px: p.width(),
                        height_px: p.height(),
                        png: png.into(),
                        fx: None,
                    },
                );
                s.stroke = None;
                s.name = Some(tr("flow.result_name"));
                s
            }
            Value::Art(a) => {
                let children = art_shapes(a, rect, &mut || self.engine.new_shape_id());
                let id = self.engine.new_shape_id();
                let mut g = Shape::new(id, ShapeKind::Group { children });
                g.name = Some(tr("flow.result_name"));
                g
            }
        }
    }

    /// The bytes of a value saved to `path`: SVG for curves when the name
    /// ends in `.svg`, else PNG.
    fn flow_file_bytes(&mut self, v: &Value, path: &str) -> Option<Vec<u8>> {
        let svg = path.to_ascii_lowercase().ends_with(".svg");
        match v {
            Value::Art(a) if svg => {
                let dpi = crate::image_dpi::DEFAULT_DPI;
                let (w, h) = (a.w as f64 * 25.4 / dpi, a.h as f64 * 25.4 / dpi);
                let mut doc = tracedraw_core::Document::new("workflow", Size::new(w, h));
                let rect = MmRect::new(0.0, 0.0, w, h);
                let shapes = art_shapes(a, rect, &mut || self.engine.new_shape_id());
                let layer = doc.pages.first()?.layers.first()?.id;
                doc.layer_mut(layer).ok()?.shapes.extend(shapes);
                Some(tracedraw_io::svg::page_to_svg(&doc, 0).into_bytes())
            }
            _ => crate::bitmap_fx::encode(&v.picture()),
        }
    }
}

/// The curves of `a` as objects filling `rect` (page space).
fn art_shapes(
    a: &flow::Art,
    rect: MmRect,
    new_id: &mut dyn FnMut() -> tracedraw_core::ShapeId,
) -> Vec<Shape> {
    let mut out = Vec::new();
    for t in &a.traced {
        let path = crate::trace::to_rect(&t.path, a.w, a.h, rect);
        let path_box = path.bounding_box();
        let mut s = Shape::new(
            new_id(),
            ShapeKind::Path {
                path,
                closed: t.closed,
            },
        );
        if t.closed {
            s.fill = match &t.blend {
                Some(b) => crate::trace::blend_fill(b, &path_box, a.w, a.h, rect),
                None => Fill::Solid(t.color),
            };
            s.stroke = None;
        } else {
            s.fill = Fill::None;
            let mm = a.stroke_px * rect.width() / a.w.max(1) as f64;
            s.stroke = Some(tracedraw_core::Stroke::new(t.color, mm.max(0.1)));
        }
        out.push(s);
    }
    out
}

/// A picture file's pixels.
fn load_picture(path: &std::path::Path) -> Option<Value> {
    let bytes = crate::files::read(path).ok()?;
    let img = image::load_from_memory(&bytes).ok()?.to_rgba8();
    Some(Value::Picture(Arc::new(img)))
}

/// A step's message: an i18n key translated, or an extension's own text.
fn message(m: &str) -> String {
    if crate::i18n::has_english(m) {
        tr(m)
    } else {
        m.to_string()
    }
}

/// Start a run of `flow` on `given`: on another thread on the desktop,
/// in the frame in a browser.
fn start(flow: Flow, given: BTreeMap<NodeId, Value>, export: bool, anchor: Option<MmRect>) -> Job {
    let cancel = Arc::new(Cancel::new(false));
    let (tx, rx) = std::sync::mpsc::channel();
    #[cfg(not(target_arch = "wasm32"))]
    {
        let c = cancel.clone();
        std::thread::spawn(move || {
            let _ = tx.send(flow::run(&flow, &given, Some(&c)));
        });
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = tx.send(flow::run(&flow, &given, Some(&cancel)));
    }
    Job {
        cancel,
        rx,
        export,
        anchor,
    }
}

impl WorkflowState {
    pub fn running(&self) -> bool {
        self.job.is_some()
    }

    /// A finished run's results taken in (and its exports carried out).
    fn poll(&mut self, app: &mut App) {
        let Some(job) = &self.job else {
            return;
        };
        let got = match job.rx.try_recv() {
            Ok(r) => r,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => None,
        };
        let Some(job) = self.job.take() else {
            return;
        };
        let Some(results) = got else {
            self.status = tr("flow.cancelled");
            return;
        };
        self.results = results;
        self.stale_thumbs = true;
        if job.export {
            let (n, errors) = app.workflow_export(&self.flow, &self.results, job.anchor);
            self.status = trf("flow.done", &[("n", &n.to_string())]);
            if let Some(e) = errors.first() {
                self.status = format!("{} {e}", self.status);
            }
        } else {
            self.status = tr("flow.previewed");
        }
    }

    /// The results' thumbnails made again after a run.
    fn refresh_thumbs(&mut self, ctx: &Context) {
        if !self.stale_thumbs {
            return;
        }
        self.stale_thumbs = false;
        self.thumbs.clear();
        for (id, r) in &self.results {
            let Ok(v) = r else {
                continue;
            };
            let img = v.picture();
            // Fitted into the square, keeping the picture's proportions.
            let s = (THUMB as f32 / img.width().max(img.height()).max(1) as f32).min(1.0);
            let small = if s < 1.0 {
                let tw = ((img.width() as f32 * s) as u32).max(1);
                let th = ((img.height() as f32 * s) as u32).max(1);
                image::imageops::thumbnail(img.as_ref(), tw, th)
            } else {
                img.as_ref().clone()
            };
            let (w, h) = small.dimensions();
            let ci =
                egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], small.as_raw());
            let tex =
                ctx.load_texture(format!("flow_thumb_{id}"), ci, egui::TextureOptions::LINEAR);
            self.thumbs.insert(*id, tex);
        }
    }
}

/// The step's box on the canvas whose origin is `origin`.
fn node_rect(n: &flow::Node, origin: Pos2) -> Rect {
    Rect::from_min_size(
        origin + Vec2::new(n.pos[0], n.pos[1]),
        Vec2::new(NODE_W, NODE_H),
    )
}

fn input_port(r: Rect, port: usize, count: usize) -> Pos2 {
    let top = r.top() + TITLE_H;
    let room = r.height() - TITLE_H;
    Pos2::new(
        r.left(),
        top + room * (port as f32 + 0.5) / count.max(1) as f32,
    )
}

fn output_port(r: Rect) -> Pos2 {
    Pos2::new(r.right(), r.top() + TITLE_H + (r.height() - TITLE_H) / 2.0)
}

/// A wire drawn as a curve of short straight pieces.
fn wire(painter: &egui::Painter, a: Pos2, b: Pos2, color: Color32) {
    let dx = ((b.x - a.x).abs() / 2.0).max(30.0);
    let (c1, c2) = (a + Vec2::new(dx, 0.0), b - Vec2::new(dx, 0.0));
    let pts: Vec<Pos2> = (0..=24)
        .map(|i| {
            let t = i as f32 / 24.0;
            let u = 1.0 - t;
            let p = a.to_vec2() * (u * u * u)
                + c1.to_vec2() * (3.0 * u * u * t)
                + c2.to_vec2() * (3.0 * u * t * t)
                + b.to_vec2() * (t * t * t);
            p.to_pos2()
        })
        .collect();
    painter.add(egui::Shape::line(pts, Stroke::new(2.0, color)));
}

/// What lies under `p` on the canvas: a step's output, one of its inputs,
/// or its box.
enum Hit {
    Output(NodeId),
    Input(NodeId, usize),
    Node(NodeId),
}

fn hit(flow: &Flow, origin: Pos2, p: Pos2) -> Option<Hit> {
    // The box drawn last (on top) first.
    for n in flow.nodes.iter().rev() {
        let r = node_rect(n, origin);
        if n.step.has_output() && output_port(r).distance(p) <= PORT_R + 3.0 {
            return Some(Hit::Output(n.id));
        }
        let count = n.step.inputs();
        for port in 0..count {
            if input_port(r, port, count).distance(p) <= PORT_R + 3.0 {
                return Some(Hit::Input(n.id, port));
            }
        }
        if r.contains(p) {
            return Some(Hit::Node(n.id));
        }
    }
    None
}

/// The node canvas: boxes, wires, and the drags that move and join them.
fn canvas(ui: &mut Ui, st: &mut WorkflowState) {
    let size = ui.available_size().max(Vec2::splat(100.0));
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let resp = resp.rich_tip(keyed("flow.canvas"));
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals().clone();
    painter.rect_filled(rect, 0.0, visuals.extreme_bg_color);
    // A light grid that moves with the canvas.
    let step = 24.0;
    let grid = visuals.weak_text_color().gamma_multiply(0.15);
    let off = Vec2::new(st.pan.x.rem_euclid(step), st.pan.y.rem_euclid(step));
    let mut x = rect.left() + off.x;
    while x < rect.right() {
        painter.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            Stroke::new(1.0, grid),
        );
        x += step;
    }
    let mut y = rect.top() + off.y;
    while y < rect.bottom() {
        painter.line_segment(
            [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
            Stroke::new(1.0, grid),
        );
        y += step;
    }
    let origin = rect.min + st.pan;
    let pointer = resp.interact_pointer_pos().or(resp.hover_pos());

    // Drags.
    if resp.drag_started() {
        // Where the button went down: a drag starts only once the pointer
        // has moved a little from there.
        let press = ui.input(|i| i.pointer.press_origin());
        if let Some(p) = press.or(resp.interact_pointer_pos()) {
            st.drag = match hit(&st.flow, origin, p) {
                Some(Hit::Output(id)) => Some(Drag::Wire { from: id }),
                Some(Hit::Input(id, port)) => match st.flow.input_of(id, port) {
                    // An input's wire picked up again from its end.
                    Some(from) => {
                        st.flow.disconnect(id, port);
                        Some(Drag::Wire { from })
                    }
                    None => None,
                },
                Some(Hit::Node(id)) => {
                    st.selected = Some(id);
                    let r = st.flow.node(id).map(|n| node_rect(n, origin));
                    r.map(|r| Drag::Node {
                        id,
                        grab: p - r.min,
                    })
                }
                None => Some(Drag::Pan),
            };
        }
    }
    if resp.dragged() {
        match st.drag {
            Some(Drag::Node { id, grab }) => {
                if let (Some(p), Some(n)) = (resp.interact_pointer_pos(), st.flow.node_mut(id)) {
                    let at = p - grab - origin;
                    n.pos = [at.x, at.y];
                }
            }
            Some(Drag::Pan) => st.pan += resp.drag_delta(),
            _ => {}
        }
    }
    if resp.drag_stopped() {
        if let (Some(Drag::Wire { from }), Some(p)) = (st.drag, pointer) {
            let target = match hit(&st.flow, origin, p) {
                Some(Hit::Input(id, port)) => Some((id, port)),
                // Dropped on a box: its first free input, else its first.
                Some(Hit::Node(id)) => st.flow.node(id).and_then(|n| {
                    let count = n.step.inputs();
                    (0..count)
                        .find(|&port| st.flow.input_of(id, port).is_none())
                        .or((count > 0).then_some(0))
                        .map(|port| (id, port))
                }),
                _ => None,
            };
            if let Some((id, port)) = target {
                if !st.flow.connect(from, id, port) {
                    st.status = tr("flow.cannot_connect");
                }
            }
        }
        st.drag = None;
    }
    if resp.clicked() {
        st.selected = match resp
            .interact_pointer_pos()
            .and_then(|p| hit(&st.flow, origin, p))
        {
            Some(Hit::Node(id) | Hit::Output(id) | Hit::Input(id, _)) => Some(id),
            None => None,
        };
    }
    if resp.secondary_clicked() {
        if let Some(Hit::Input(id, port)) = resp
            .interact_pointer_pos()
            .and_then(|p| hit(&st.flow, origin, p))
        {
            st.flow.disconnect(id, port);
        }
    }

    // Wires.
    let accent = visuals.selection.bg_fill;
    let wire_color = visuals.text_color().gamma_multiply(0.7);
    for w in &st.flow.wires {
        let (Some(a), Some(b)) = (st.flow.node(w.from), st.flow.node(w.to)) else {
            continue;
        };
        let count = b.step.inputs();
        wire(
            &painter,
            output_port(node_rect(a, origin)),
            input_port(node_rect(b, origin), w.port, count),
            wire_color,
        );
    }
    if let (Some(Drag::Wire { from }), Some(p)) = (st.drag, pointer) {
        if let Some(a) = st.flow.node(from) {
            wire(&painter, output_port(node_rect(a, origin)), p, accent);
        }
    }

    // Boxes.
    for n in &st.flow.nodes {
        let r = node_rect(n, origin);
        if !r.intersects(rect) {
            continue;
        }
        let selected = st.selected == Some(n.id);
        painter.rect_filled(r, 4.0, visuals.window_fill);
        let title = Rect::from_min_size(r.min, Vec2::new(r.width(), TITLE_H));
        painter.rect_filled(title, 4.0, visuals.faint_bg_color);
        painter.rect_stroke(
            r,
            4.0,
            Stroke::new(
                if selected { 2.0 } else { 1.0 },
                if selected {
                    accent
                } else {
                    visuals.widgets.noninteractive.bg_stroke.color
                },
            ),
            egui::StrokeKind::Inside,
        );
        painter.text(
            title.left_center() + Vec2::new(8.0, 0.0),
            egui::Align2::LEFT_CENTER,
            tr(&n.step.name_key()),
            egui::FontId::proportional(12.0),
            visuals.strong_text_color(),
        );
        let body = Rect::from_min_max(
            Pos2::new(r.left() + 10.0, title.bottom() + 4.0),
            Pos2::new(r.right() - 10.0, r.bottom() - 4.0),
        );
        match (st.results.get(&n.id), st.thumbs.get(&n.id)) {
            (Some(Ok(_)), Some(tex)) => {
                let s = tex.size_vec2();
                let k = (body.width() / s.x).min(body.height() / s.y).min(1.0);
                let pic = Rect::from_center_size(body.center(), s * k);
                checker(&painter, pic);
                painter.image(
                    tex.id(),
                    pic,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            (Some(Err(m)), _) => {
                let galley = painter.layout(
                    message(m),
                    egui::FontId::proportional(11.0),
                    visuals.error_fg_color,
                    body.width(),
                );
                painter.galley(body.min, galley, visuals.error_fg_color);
            }
            _ => {
                painter.text(
                    body.center(),
                    egui::Align2::CENTER_CENTER,
                    summary(&n.step),
                    egui::FontId::proportional(11.0),
                    visuals.weak_text_color(),
                );
            }
        }
        // Dots: inputs left, output right.
        let count = n.step.inputs();
        for port in 0..count {
            let c = input_port(r, port, count);
            let wired = st.flow.input_of(n.id, port).is_some();
            painter.circle(
                c,
                PORT_R,
                if wired {
                    wire_color
                } else {
                    visuals.window_fill
                },
                Stroke::new(1.5, wire_color),
            );
        }
        if n.step.has_output() {
            painter.circle(output_port(r), PORT_R, accent, Stroke::new(1.5, wire_color));
        }
    }
    if st.flow.nodes.is_empty() {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            tr("flow.empty"),
            egui::FontId::proportional(13.0),
            visuals.weak_text_color(),
        );
    }
}

/// Where a new step's box goes: the first place of a grid three boxes
/// wide, row by row, that no box covers.
fn free_spot(flow: &Flow) -> [f32; 2] {
    let (dx, dy) = (NODE_W + 50.0, NODE_H + 30.0);
    for i in 0..400 {
        let at = [20.0 + dx * (i % 3) as f32, 60.0 + dy * (i / 3) as f32];
        let r = Rect::from_min_size(Pos2::new(at[0], at[1]), Vec2::new(NODE_W, NODE_H));
        let taken = flow.nodes.iter().any(|n| {
            Rect::from_min_size(Pos2::new(n.pos[0], n.pos[1]), Vec2::new(NODE_W, NODE_H))
                .expand(8.0)
                .intersects(r)
        });
        if !taken {
            return at;
        }
    }
    [20.0, 60.0]
}

/// A grey and white checkerboard under a picture, so its clear parts show.
fn checker(painter: &egui::Painter, r: Rect) {
    painter.rect_filled(r, 0.0, Color32::WHITE);
    let s = 8.0;
    let mut y = r.top();
    let mut row = 0;
    while y < r.bottom() {
        let mut x = r.left() + if row % 2 == 0 { 0.0 } else { s };
        while x < r.right() {
            let cell = Rect::from_min_max(
                Pos2::new(x, y),
                Pos2::new((x + s).min(r.right()), (y + s).min(r.bottom())),
            );
            painter.rect_filled(cell, 0.0, Color32::from_gray(225));
            x += 2.0 * s;
        }
        y += s;
        row += 1;
    }
}

/// The short line a step's box shows before a run.
fn summary(step: &Step) -> String {
    match step {
        Step::Input {
            source: Source::Selection,
        } => tr("flow.source_selection"),
        Step::Input {
            source: Source::File { path },
        }
        | Step::Export {
            target: Target::File { path },
        } => {
            if path.is_empty() {
                tr("flow.no_file")
            } else {
                file_name(path)
            }
        }
        Step::Vectorize { preset, .. } => preset_name(*preset),
        Step::Effect { effect } => tr(&format!("fx.{}", effect.id)),
        Step::Export {
            target: Target::Page,
        } => tr("flow.target_page"),
        _ => String::new(),
    }
}

/// A trace preset's name, without the dots its menu item carries.
fn preset_name(p: crate::trace::Preset) -> String {
    tr(p.key())
        .trim_end_matches("...")
        .trim_end_matches('\u{2026}')
        .trim()
        .to_string()
}

fn file_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

/// The picture of one result, fitted into `size`.
fn result_picture(ui: &mut Ui, st: &WorkflowState, id: NodeId, size: Vec2) {
    match (st.results.get(&id), st.thumbs.get(&id)) {
        (Some(Ok(v)), Some(tex)) => {
            let s = tex.size_vec2();
            let k = (size.x / s.x).min(size.y / s.y).min(2.0);
            let (r, _) = ui.allocate_exact_size(s * k, Sense::hover());
            checker(ui.painter(), r);
            ui.painter().image(
                tex.id(),
                r,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
            ui.label(describe(v));
        }
        (Some(Err(m)), _) => {
            ui.colored_label(ui.visuals().error_fg_color, message(m));
        }
        _ => {
            ui.weak(tr("flow.not_run"));
        }
    }
}

/// What a result is: a picture's size or a count of curves.
fn describe(v: &Value) -> String {
    match v {
        Value::Picture(p) => trf(
            "flow.picture_size",
            &[
                ("w", &p.width().to_string()),
                ("h", &p.height().to_string()),
            ],
        ),
        Value::Art(a) => trf("flow.curves", &[("n", &a.traced.len().to_string())]),
    }
}

/// The selected step's settings.
fn settings(ui: &mut Ui, st: &mut WorkflowState, ask: &mut Option<Ask>) {
    let Some(id) = st.selected.filter(|id| st.flow.node(*id).is_some()) else {
        ui.weak(tr("flow.select_step"));
        return;
    };
    let name = st
        .flow
        .node(id)
        .map(|n| tr(&n.step.name_key()))
        .unwrap_or_default();
    ui.strong(name);
    ui.add_space(4.0);
    let compare: Vec<NodeId> = (0..flow::MAX_INPUTS)
        .filter_map(|p| st.flow.input_of(id, p))
        .collect();
    let Some(node) = st.flow.node_mut(id) else {
        return;
    };
    match &mut node.step {
        Step::Input { source } => {
            let mut file = matches!(source, Source::File { .. });
            ui.radio_value(&mut file, false, tr("flow.source_selection"))
                .rich_tip(keyed("flow.source_selection"));
            ui.radio_value(&mut file, true, tr("flow.source_file"))
                .rich_tip(keyed("flow.source_file"));
            if file != matches!(source, Source::File { .. }) {
                *source = if file {
                    Source::File {
                        path: String::new(),
                    }
                } else {
                    Source::Selection
                };
            }
            if let Source::File { path } = source {
                ui.horizontal(|ui| {
                    if ui
                        .button(tr("flow.choose"))
                        .rich_tip(keyed("flow.choose"))
                        .clicked()
                    {
                        *ask = Some(Ask::InputFile(id));
                    }
                    ui.label(if path.is_empty() {
                        tr("flow.no_file")
                    } else {
                        file_name(path)
                    });
                });
            }
        }
        Step::Vectorize {
            preset,
            colors,
            remove,
        } => {
            ui.label(tr("flow.preset"));
            egui::ComboBox::from_id_salt("flow_preset")
                .selected_text(preset_name(*preset))
                .width(PANEL_W - 30.0)
                .show_ui(ui, |ui| {
                    for p in crate::trace::Preset::ALL {
                        ui.selectable_value(preset, p, preset_name(p));
                    }
                })
                .response
                .rich_tip(keyed("flow.preset"));
            ui.label(tr("flow.colors"));
            ui.add(egui::DragValue::new(colors).range(0..=256))
                .rich_tip(keyed("flow.colors"));
            ui.label(tr("flow.remove"));
            egui::ComboBox::from_id_salt("flow_remove")
                .selected_text(tr(remove.key()))
                .width(PANEL_W - 30.0)
                .show_ui(ui, |ui| {
                    for r in crate::trace::Remove::ALL {
                        ui.selectable_value(remove, r, tr(r.key()));
                    }
                })
                .response
                .rich_tip(keyed("flow.remove"));
        }
        Step::Recolor { recolor, invert } => {
            let mut tint = recolor.tint.is_some();
            ui.horizontal(|ui| {
                ui.checkbox(&mut tint, tr("recolor.tint"))
                    .rich_tip(keyed("flow.tint"));
                let mut c = recolor.tint.unwrap_or(Color::rgb8(30, 110, 220));
                if crate::ui::color_picker::color_button(ui, &mut c) {
                    tint = true;
                }
                recolor.tint = tint.then_some(c);
            });
            for (key, v, range) in [
                ("recolor.hue", &mut recolor.hue, -180.0..=180.0),
                (
                    "recolor.saturation",
                    &mut recolor.saturation,
                    -100.0..=100.0,
                ),
                ("recolor.lightness", &mut recolor.lightness, -100.0..=100.0),
            ] {
                ui.label(tr(key));
                ui.spacing_mut().slider_width = PANEL_W - 90.0;
                ui.add(egui::Slider::new(v, range).step_by(1.0))
                    .rich_tip(keyed(&format!(
                        "flow.{}",
                        key.trim_start_matches("recolor.")
                    )));
            }
            ui.checkbox(invert, tr("flow.invert"))
                .rich_tip(keyed("flow.invert"));
        }
        Step::Effect { effect } => {
            ui.label(tr("flow.effect"));
            let mut chosen = None;
            ui.menu_button(tr(&format!("fx.{}", effect.id)), |ui| {
                for (group, specs) in crate::fx::groups() {
                    ui.menu_button(tr(group), |ui| {
                        for s in specs {
                            if ui.button(tr(&format!("fx.{}", s.id))).clicked() {
                                chosen = Some(s.id);
                                ui.close();
                            }
                        }
                    });
                }
            })
            .response
            .rich_tip(keyed("flow.effect"));
            if let Some(c) = chosen {
                *effect = crate::fx::new_effect(c);
            }
            if let Some(spec) = crate::fx::spec(&effect.id) {
                for p in spec.params {
                    crate::ui::effect_dialog::param_stacked(ui, p, &mut effect.params, None);
                }
            }
        }
        Step::Compare => {
            if compare.is_empty() {
                ui.weak(tr("flow.compare_empty"));
            }
        }
        Step::Export { target } => {
            let mut file = matches!(target, Target::File { .. });
            ui.radio_value(&mut file, false, tr("flow.target_page"))
                .rich_tip(keyed("flow.target_page"));
            ui.radio_value(&mut file, true, tr("flow.target_file"))
                .rich_tip(keyed("flow.target_file"));
            if file != matches!(target, Target::File { .. }) {
                *target = if file {
                    Target::File {
                        path: String::new(),
                    }
                } else {
                    Target::Page
                };
            }
            if let Target::File { path } = target {
                ui.horizontal(|ui| {
                    if ui
                        .button(tr("flow.choose"))
                        .rich_tip(keyed("flow.choose_export"))
                        .clicked()
                    {
                        *ask = Some(Ask::ExportFile(id));
                    }
                    ui.label(if path.is_empty() {
                        tr("flow.no_file")
                    } else {
                        file_name(path)
                    });
                });
            }
        }
        Step::Extension { kind, params } => {
            if let Some(e) = flow::extension(kind) {
                for (key, _) in e.params {
                    ui.label(tr(key));
                    let v = params.entry(key.to_string()).or_default();
                    ui.text_edit_singleline(v).rich_tip(tr(key));
                }
            }
        }
    }
    ui.separator();
    if matches!(st.flow.node(id).map(|n| &n.step), Some(Step::Compare)) {
        // The results wired in, side by side.
        if !compare.is_empty() {
            let w = (ui.available_width() - 8.0) / 2.0;
            egui::Grid::new("flow_compare")
                .num_columns(2)
                .show(ui, |ui| {
                    for (i, from) in compare.iter().enumerate() {
                        ui.vertical(|ui| {
                            let name = st
                                .flow
                                .node(*from)
                                .map(|n| tr(&n.step.name_key()))
                                .unwrap_or_default();
                            ui.label(name);
                            result_picture(ui, st, *from, Vec2::new(w, w));
                        });
                        if i % 2 == 1 {
                            ui.end_row();
                        }
                    }
                });
        }
    } else {
        result_picture(ui, st, id, Vec2::new(ui.available_width(), 200.0));
    }
}

/// The window.
pub fn dialog(app: &mut App, ctx: &Context, close: &mut bool) {
    let mut st = std::mem::take(&mut app.workflow);
    st.poll(app);
    st.refresh_thumbs(ctx);
    if st.running() {
        ctx.request_repaint_after(std::time::Duration::from_millis(50));
    }
    let mut ask = None;
    let mut help = false;
    crate::ui::dialogs::window(ctx, tr("flow.title"))
        .id_salt("workflow")
        .resizable(Vec2::new(1000.0, 640.0), Vec2::new(720.0, 460.0))
        .show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                let idle = !st.running();
                if ui
                    .add_enabled(idle, egui::Button::new(tr("flow.new")))
                    .rich_tip(keyed("flow.new"))
                    .clicked()
                {
                    ask = Some(Ask::New);
                }
                if ui
                    .add_enabled(idle, egui::Button::new(tr("flow.open")))
                    .rich_tip(keyed("flow.open"))
                    .clicked()
                {
                    ask = Some(Ask::Open);
                }
                if ui
                    .button(tr("flow.save"))
                    .rich_tip(keyed("flow.save"))
                    .clicked()
                {
                    ask = Some(Ask::Save);
                }
                ui.separator();
                ui.add_enabled_ui(idle, |ui| {
                    ui.menu_button(tr("flow.add"), |ui| {
                        let at = free_spot(&st.flow);
                        for step in Step::builtin() {
                            if ui.button(tr(&step.name_key())).clicked() {
                                st.selected = Some(st.flow.add(step, at));
                                ui.close();
                            }
                        }
                        // Steps other parts of the program add.
                        let more = flow::extensions();
                        if !more.is_empty() {
                            ui.separator();
                        }
                        for e in more {
                            if ui
                                .add_enabled((e.available)(), egui::Button::new(tr(e.name_key)))
                                .clicked()
                            {
                                st.selected = Some(st.flow.add(Step::extension_step(e), at));
                                ui.close();
                            }
                        }
                    })
                    .response
                    .rich_tip(keyed("flow.add"));
                    let can_delete = st.selected.is_some();
                    if ui
                        .add_enabled(can_delete, egui::Button::new(tr("flow.delete")))
                        .rich_tip(keyed("flow.delete"))
                        .clicked()
                    {
                        if let Some(id) = st.selected.take() {
                            st.flow.remove(id);
                            st.results.remove(&id);
                        }
                    }
                });
                ui.separator();
                if ui
                    .add_enabled(idle, egui::Button::new(tr("flow.preview")))
                    .rich_tip(keyed("flow.preview"))
                    .clicked()
                {
                    ask = Some(Ask::Run {
                        export: false,
                        file: None,
                    });
                }
                if ui
                    .add_enabled(idle, egui::Button::new(tr("flow.run")))
                    .rich_tip(keyed("flow.run"))
                    .clicked()
                {
                    ask = Some(Ask::Run {
                        export: true,
                        file: None,
                    });
                }
                if ui
                    .add_enabled(idle, egui::Button::new(tr("flow.run_file")))
                    .rich_tip(keyed("flow.run_file"))
                    .clicked()
                {
                    ask = Some(Ask::RunOnFile);
                }
                if !idle {
                    ui.spinner();
                    if ui
                        .button(tr("dialog.cancel"))
                        .rich_tip(keyed("flow.cancel"))
                        .clicked()
                    {
                        if let Some(j) = &st.job {
                            j.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                        }
                    }
                }
                if ui
                    .button(tr("dialog.help"))
                    .rich_tip(keyed("flow.help"))
                    .clicked()
                {
                    help = true;
                }
            });
            ui.label(
                egui::RichText::new(if st.status.is_empty() {
                    tr("flow.hint")
                } else {
                    st.status.clone()
                })
                .weak(),
            );
            ui.separator();
            let room = ui.available_size();
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    Vec2::new((room.x - PANEL_W - 12.0).max(200.0), room.y),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        canvas(ui, &mut st);
                    },
                );
                ui.separator();
                ui.allocate_ui_with_layout(
                    Vec2::new(PANEL_W, room.y),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(PANEL_W);
                        egui::ScrollArea::vertical()
                            .id_salt("flow_settings")
                            .auto_shrink([false, false])
                            .show(ui, |ui| settings(ui, &mut st, &mut ask));
                    },
                );
            });
        });
    // Delete takes the selected step out while no field is being typed in.
    if !st.running()
        && st.selected.is_some()
        && ctx.memory(|m| m.focused().is_none())
        && ctx.input(|i| i.key_pressed(egui::Key::Delete))
    {
        if let Some(id) = st.selected.take() {
            st.flow.remove(id);
            st.results.remove(&id);
        }
    }
    app.workflow = st;
    if help {
        app.open_dialog_help();
    }
    if let Some(a) = ask {
        app.workflow_ask(a);
    }
    // A run in progress is cancelled when the window closes.
    if *close {
        if let Some(j) = &app.workflow.job {
            j.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        app.workflow.job = None;
    }
}

impl App {
    fn workflow_ask(&mut self, a: Ask) {
        let filter = format!("{} (*.{})", tr("flow.title"), flow::EXTENSION);
        match a {
            Ask::New => {
                let st = &mut self.workflow;
                st.flow = Flow::default();
                st.results.clear();
                st.thumbs.clear();
                st.selected = None;
                st.pan = Vec2::ZERO;
                st.path = None;
                st.status.clear();
            }
            Ask::Open => crate::files::Dialog::new()
                .add_filter(filter, &[flow::EXTENSION])
                .pick_file(self, |app, path| {
                    let read = crate::files::read_to_string(&path)
                        .map_err(|e| e.to_string())
                        .and_then(|t| Flow::from_json(&t));
                    let st = &mut app.workflow;
                    match read {
                        Ok(f) => {
                            st.flow = f;
                            st.results.clear();
                            st.thumbs.clear();
                            st.selected = None;
                            st.pan = Vec2::ZERO;
                            st.status = file_name(&path.to_string_lossy());
                            st.path = Some(path);
                        }
                        Err(e) => st.status = trf("flow.open_failed", &[("e", &message(&e))]),
                    }
                }),
            Ask::Save => {
                let name = self
                    .workflow
                    .path
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| format!("{}.{}", tr("flow.title"), flow::EXTENSION));
                crate::files::Dialog::new()
                    .add_filter(filter, &[flow::EXTENSION])
                    .set_file_name(name)
                    .save_file(self, |app, path| {
                        let st = &mut app.workflow;
                        match crate::files::write(&path, st.flow.to_json()) {
                            Ok(()) => {
                                st.status = trf(
                                    "flow.saved",
                                    &[("name", &file_name(&path.to_string_lossy()))],
                                );
                                st.path = Some(path);
                            }
                            Err(e) => st.status = trf("flow.save_failed", &[("e", &e.to_string())]),
                        }
                    })
            }
            Ask::Run { export, file } => {
                let flow = self.workflow.flow.clone();
                let (given, anchor) = self.workflow_inputs(&flow, file.as_deref());
                let st = &mut self.workflow;
                st.status = tr("flow.running");
                st.job = Some(start(flow, given, export, anchor));
            }
            Ask::RunOnFile => crate::files::Dialog::new()
                .add_filter(tr("flow.pictures"), PICTURES)
                .pick_file(self, |app, path| {
                    app.workflow_ask(Ask::Run {
                        export: true,
                        file: Some(path),
                    })
                }),
            Ask::InputFile(id) => crate::files::Dialog::new()
                .add_filter(tr("flow.pictures"), PICTURES)
                .pick_file(self, move |app, path| {
                    if let Some(n) = app.workflow.flow.node_mut(id) {
                        n.step = Step::Input {
                            source: Source::File {
                                path: path.to_string_lossy().into_owned(),
                            },
                        };
                    }
                }),
            Ask::ExportFile(id) => crate::files::Dialog::new()
                .add_filter("PNG (*.png)", &["png"])
                .add_filter("SVG (*.svg)", &["svg"])
                .set_file_name(format!("{}.png", tr("flow.result_name")))
                .save_file(self, move |app, path| {
                    if let Some(n) = app.workflow.flow.node_mut(id) {
                        n.step = Step::Export {
                            target: Target::File {
                                path: path.to_string_lossy().into_owned(),
                            },
                        };
                    }
                }),
        }
    }
}

/// The picture files an Input step and Run on a Picture File take.
const PICTURES: &[&str] = &["png", "jpg", "jpeg", "bmp", "gif", "webp", "tif", "tiff"];

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::Rect as R;

    fn wait(app: &mut App) {
        for _ in 0..24000 {
            let mut st = std::mem::take(&mut app.workflow);
            st.poll(app);
            let busy = st.running();
            app.workflow = st;
            if !busy {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("the run did not end");
    }

    /// A red square drawn on the page, selected.
    fn app_with_square() -> App {
        let mut app = {
            let mut a = App::headless();
            a.show_welcome = false;
            a
        };
        let id = app
            .new_shape(ShapeKind::Rect {
                rect: R::new(10.0, 10.0, 40.0, 40.0),
                radius: 0.0,
                corners: None,
            })
            .expect("rect");
        app.run(Command::SetFill {
            shapes: vec![id],
            fill: Fill::Solid(Color::rgb8(200, 30, 30)),
        });
        app.select(vec![id]);
        app
    }

    #[test]
    fn the_default_workflow_traces_the_selection_onto_the_page_in_one_step() {
        let mut app = app_with_square();
        let before = app.doc().pages[0]
            .layers
            .iter()
            .map(|l| l.shapes.len())
            .sum::<usize>();
        app.workflow_ask(Ask::Run {
            export: true,
            file: None,
        });
        wait(&mut app);
        let st = &app.workflow;
        assert_eq!(st.results.len(), 3, "{:?}", st.status);
        assert!(
            st.results.values().all(|r| r.is_ok()),
            "{:?}",
            st.results
                .values()
                .filter(|r| r.is_err())
                .collect::<Vec<_>>()
        );
        let after = app.doc().pages[0]
            .layers
            .iter()
            .map(|l| l.shapes.len())
            .sum::<usize>();
        assert_eq!(after, before + 1);
        // The group of curves sits over the square, selected.
        let g = app.selected_shapes();
        assert_eq!(g.len(), 1);
        assert!(matches!(g[0].kind, ShapeKind::Group { .. }));
        let b = app.selection_bounds().expect("bounds");
        assert!(
            (b.x0 - 10.0).abs() < 2.0 && (b.x1 - 40.0).abs() < 2.0,
            "{b:?}"
        );
        // One undo step takes it back.
        app.undo();
        let undone = app.doc().pages[0]
            .layers
            .iter()
            .map(|l| l.shapes.len())
            .sum::<usize>();
        assert_eq!(undone, before);
    }

    #[test]
    fn preview_shows_results_without_changing_the_drawing() {
        let mut app = app_with_square();
        let before = app.doc().clone();
        app.workflow_ask(Ask::Run {
            export: false,
            file: None,
        });
        wait(&mut app);
        assert_eq!(app.workflow.results.len(), 3);
        assert_eq!(app.doc().pages, before.pages);
    }

    #[test]
    fn an_export_step_saves_a_picture_or_curves_to_its_file() {
        let dir = std::env::temp_dir().join(format!("td_flow_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let png = dir.join("out.png");
        let svg = dir.join("out.svg");
        let mut app = app_with_square();
        let mut f = Flow::empty();
        let a = f.add(
            Step::Input {
                source: Source::Selection,
            },
            [0.0; 2],
        );
        let t = f.add(
            Step::Vectorize {
                preset: crate::trace::Preset::Logo,
                colors: 0,
                remove: crate::trace::Remove::Background,
            },
            [0.0; 2],
        );
        let p = f.add(
            Step::Export {
                target: Target::File {
                    path: png.to_string_lossy().into_owned(),
                },
            },
            [0.0; 2],
        );
        let s = f.add(
            Step::Export {
                target: Target::File {
                    path: svg.to_string_lossy().into_owned(),
                },
            },
            [0.0; 2],
        );
        f.connect(a, p, 0);
        f.connect(a, t, 0);
        f.connect(t, s, 0);
        app.workflow.flow = f;
        app.workflow_ask(Ask::Run {
            export: true,
            file: None,
        });
        wait(&mut app);
        let bytes = std::fs::read(&png).expect("png written");
        assert!(image::load_from_memory(&bytes).is_ok());
        let text = std::fs::read_to_string(&svg).expect("svg written");
        assert!(text.contains("<svg") && text.contains("<path"), "{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_picture_file_runs_through_in_place_of_the_selection() {
        let dir = std::env::temp_dir().join(format!("td_flow_in_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("in.png");
        let img = image::RgbaImage::from_pixel(30, 20, image::Rgba([10, 200, 10, 255]));
        img.save(&path).expect("saved");
        let mut app = {
            let mut a = App::headless();
            a.show_welcome = false;
            a
        };
        let mut f = Flow::empty();
        let a = f.add(
            Step::Input {
                source: Source::Selection,
            },
            [0.0; 2],
        );
        let inv = f.add(
            Step::Recolor {
                recolor: Default::default(),
                invert: true,
            },
            [0.0; 2],
        );
        let out = f.add(
            Step::Export {
                target: Target::Page,
            },
            [0.0; 2],
        );
        f.connect(a, inv, 0);
        f.connect(inv, out, 0);
        app.workflow.flow = f;
        app.workflow_ask(Ask::Run {
            export: true,
            file: Some(path.clone()),
        });
        wait(&mut app);
        let placed = app.selected_shapes();
        assert_eq!(placed.len(), 1);
        let ShapeKind::Bitmap {
            width_px,
            height_px,
            png,
            ..
        } = &placed[0].kind
        else {
            panic!("a bitmap");
        };
        assert_eq!((*width_px, *height_px), (30, 20));
        let px = crate::bitmap_fx::decode(png).expect("decodes");
        assert_eq!(px.get_pixel(0, 0).0, [245, 55, 245, 255]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_window_draws_results_and_keeps_its_workflow_when_closed() {
        let ctx = crate::theme::ui_context();
        let mut app = app_with_square();
        let frame = |app: &mut App| {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1400.0, 900.0))),
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| crate::ui::dialogs::show(app, ui.ctx()));
            out.textures_delta.clear();
        };
        app.open_workflow();
        let id = app.workflow.flow.add(Step::Compare, [600.0, 60.0]);
        let input = app.workflow.flow.input_nodes()[0].id;
        assert!(app.workflow.flow.connect(input, id, 0));
        app.workflow.selected = Some(id);
        frame(&mut app);
        app.workflow_ask(Ask::Run {
            export: false,
            file: None,
        });
        wait(&mut app);
        frame(&mut app);
        frame(&mut app);
        // Every step's result has its picture.
        assert_eq!(app.workflow.thumbs.len(), 4);
        assert!(matches!(app.dialog, crate::ui::dialogs::Dialog::Workflow));
        app.dialog = crate::ui::dialogs::Dialog::None;
        frame(&mut app);
        app.open_workflow();
        frame(&mut app);
        assert!(app.workflow.flow.node(id).is_some());
        assert_eq!(app.workflow.flow.nodes.len(), 4);
    }
}
