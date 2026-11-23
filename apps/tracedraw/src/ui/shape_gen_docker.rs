//! The Shape Generator docker (Window > Panels, Object > Create, #3412,
//! #3413): the generator list, its settings, a live preview of exactly
//! what Create makes, the curve count, and Create, Update (a generated
//! group selected: made again in its place), Load (its settings back into
//! the panel), Make Pattern Fill and, for the cone template, Wrap
//! Selection. Voronoi sites can be dragged, added and deleted right in the
//! preview; Voronoi, stippling and halftone can pick the selection as
//! their outline or picture.

use crate::app::App;
use crate::i18n::{tr, trf};
use crate::shape_gen::{
    generate, CellShape, GenKind, Generated, IsoLayout, Paint, ShapeGenSettings, Tess,
    TruchetStyle, Warp,
};
use crate::shape_gen_bg::WaveSide;
use crate::shape_gen_cells::{Gradient, HtShape, PointMode};
use crate::shape_gen_curves::{FractalType, SpiroType, TilingType};
use crate::shape_gen_fill::{ContourMode, HatchMode, MazeStyle, SpaceCurve};
use crate::shape_gen_growth::GrowthStart;
use crate::shape_gen_slice::{SliceLayout, SlicePattern};
use crate::shape_gen_warp::SplitMode;
use crate::theme::Tokens;
use crate::ui::docker_form::{indented, row, section, wide_button};
use crate::ui::tips::RichTip;
use egui::{Color32, Pos2, Sense, Stroke, Ui, Vec2};
use std::sync::Arc;
use tracedraw_core::geometry::PathEl;

/// The preview's curves, flattened and fitted into a unit box.
struct PreviewData {
    curves: Vec<(Vec<Pos2>, bool, Paint)>,
    count: usize,
    cut: bool,
    /// Width over height of the drawing.
    aspect: f32,
    /// The box the preview fits (x0, y1 and its longer side), to map
    /// the pointer back to the drawing.
    frame: (f64, f64, f64),
    /// Voronoi sites in the drawing's coordinates.
    sites: Vec<(f64, f64)>,
    /// The palette tone fills pick from.
    palette: Vec<[u8; 3]>,
    /// A mesh fill: the nodes in a row and every node in the preview's
    /// unit box with its colour, row by row.
    mesh: Option<(usize, Vec<(Pos2, Color32)>)>,
}

/// The live growth: advance it a little each frame, Start or Pause and
/// Stop, its steps and points, and its preview. The panel's area is kept
/// so Space over it starts and pauses the growth (and Shift+Space
/// stops it).
fn growth_controls(app: &mut App, ui: &mut Ui) -> Arc<PreviewData> {
    let more = app.shape_gen_growth_tick(std::time::Duration::from_millis(12));
    if more || app.shape_gen_growth_running() {
        ui.ctx().request_repaint();
    }
    let panel = ui.clip_rect();
    let frame = ui.ctx().cumulative_frame_nr();
    ui.ctx().data_mut(|d| {
        d.insert_temp(egui::Id::new(GROWTH_PANEL), (panel, frame));
    });
    let running = app.shape_gen_growth_running();
    let (steps, points, settled) = app
        .shape_gen_growth
        .as_ref()
        .map(|l| (l.sim.steps, l.sim.node_count(), l.sim.settled))
        .unwrap_or((0, 0, false));
    let has_doc = app.active_layer().is_some();
    ui.horizontal(|ui| {
        let (label, key) = if running {
            (tr("shapegen.growth_pause"), "shapegen.growth_pause")
        } else {
            (tr("shapegen.growth_run"), "shapegen.growth_run")
        };
        if ui
            .add_enabled(running || !settled, egui::Button::new(label))
            .rich_tip(crate::ui::tips::keyed(key))
            .clicked()
        {
            app.shape_gen_growth_toggle();
        }
        if ui
            .add_enabled(
                has_doc && points > 0,
                egui::Button::new(tr("shapegen.growth_stop")),
            )
            .rich_tip(crate::ui::tips::keyed("shapegen.growth_stop"))
            .clicked()
        {
            app.shape_gen_growth_stop();
        }
    });
    let state = if settled {
        "shapegen.growth_done"
    } else {
        "shapegen.growth_state"
    };
    ui.label(
        egui::RichText::new(trf(
            state,
            &[
                ("steps", &steps.to_string()),
                ("points", &points.to_string()),
            ],
        ))
        .color(Tokens::text_dim()),
    );
    // The preview, made again only when the growth moved on.
    let id = egui::Id::new("shape_gen_growth_preview");
    let key = (app.shape_gen.sanitized(), steps, points);
    let cached: Option<((ShapeGenSettings, u32, usize), Arc<PreviewData>)> =
        ui.ctx().data(|d| d.get_temp(id));
    if let Some((k, data)) = cached {
        if k == key {
            return data;
        }
    }
    let mut g = Generated::default();
    if let Some(l) = &app.shape_gen_growth {
        l.sim.draw(&mut g);
    }
    let data = Arc::new(build_preview(&g));
    ui.ctx()
        .data_mut(|d| d.insert_temp(id, (key, data.clone())));
    data
}

/// Where the growth controls were last drawn, and on which frame.
pub const GROWTH_PANEL: &str = "shape_gen_growth_panel";

/// Space over the Shape Generator panel showing the growth starts or
/// pauses it, Shift+Space stops it (makes it curves). True when it took
/// the key.
pub fn growth_keys(app: &mut App, ctx: &egui::Context) -> bool {
    if app.shape_gen.kind != GenKind::Growth || ctx.egui_wants_keyboard_input() {
        return false;
    }
    let Some((rect, frame)): Option<(egui::Rect, u64)> =
        ctx.data(|d| d.get_temp(egui::Id::new(GROWTH_PANEL)))
    else {
        return false;
    };
    // Drawn on this frame or the one before: the panel is showing.
    if frame + 1 < ctx.cumulative_frame_nr() {
        return false;
    }
    let over = ctx
        .input(|i| i.pointer.hover_pos())
        .is_some_and(|p| rect.contains(p));
    if !over {
        return false;
    }
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::Space)) {
        app.shape_gen_growth_stop();
        return true;
    }
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Space)) {
        app.shape_gen_growth_toggle();
        return true;
    }
    false
}

/// The most curves the preview draws (Create makes them all).
const PREVIEW_MAX: usize = 6000;

fn build_preview(g: &Generated) -> PreviewData {
    let Some(b) = g.frame.or_else(|| g.bounds()) else {
        return PreviewData {
            curves: Vec::new(),
            count: 0,
            cut: g.cut,
            aspect: 1.0,
            frame: (0.0, 1.0, 1.0),
            sites: Vec::new(),
            palette: Vec::new(),
            mesh: None,
        };
    };
    let side = b.width().max(b.height()).max(1e-9);
    let tol = side / 500.0;
    let mut curves = Vec::new();
    for it in g.items.iter().take(PREVIEW_MAX) {
        let mut cur: Vec<Pos2> = Vec::new();
        let mut closed = false;
        let mut push = |cur: &mut Vec<Pos2>, closed: bool| {
            if cur.len() > 1 {
                curves.push((std::mem::take(cur), closed, it.paint));
            }
            cur.clear();
        };
        tracedraw_core::geometry::flatten(&it.path, tol, &mut |el| match el {
            PathEl::MoveTo(p) => {
                push(&mut cur, closed);
                closed = false;
                // y up in the model, down on screen; 0 to 1 over the
                // longer side.
                cur.push(Pos2::new(
                    ((p.x - b.x0) / side) as f32,
                    ((b.y1 - p.y) / side) as f32,
                ));
            }
            PathEl::LineTo(p) => cur.push(Pos2::new(
                ((p.x - b.x0) / side) as f32,
                ((b.y1 - p.y) / side) as f32,
            )),
            PathEl::ClosePath => closed = true,
            _ => {}
        });
        push(&mut cur, closed);
    }
    PreviewData {
        curves,
        count: g.items.len(),
        cut: g.cut,
        aspect: (b.width() / b.height().max(1e-9)) as f32,
        frame: (b.x0, b.y1, side),
        sites: g.sites.iter().map(|p| (p.x, p.y)).collect(),
        palette: g.palette.clone(),
        mesh: g.mesh.as_ref().map(|m| {
            let nodes = m
                .nodes
                .iter()
                .map(|n| {
                    let [r, gr, bl] = n.color.to_rgb8();
                    (
                        Pos2::new(
                            ((n.pos.x - b.x0) / side) as f32,
                            ((b.y1 - n.pos.y) / side) as f32,
                        ),
                        Color32::from_rgb(r, gr, bl),
                    )
                })
                .collect();
            (m.cols as usize + 1, nodes)
        }),
    }
}

/// The preview for `s`, kept while the settings stay the same.
fn preview_for(ui: &Ui, s: &ShapeGenSettings) -> Arc<PreviewData> {
    let id = egui::Id::new("shape_gen_preview");
    let cached: Option<(ShapeGenSettings, Arc<PreviewData>)> = ui.ctx().data(|d| d.get_temp(id));
    if let Some((key, data)) = cached {
        if &key == s {
            return data;
        }
    }
    let data = Arc::new(build_preview(&generate(s)));
    ui.ctx()
        .data_mut(|d| d.insert_temp(id, (s.clone(), data.clone())));
    data
}

fn convex(pts: &[Pos2]) -> bool {
    if pts.len() < 3 {
        return false;
    }
    let mut sign = 0.0f32;
    for i in 0..pts.len() {
        let (a, b, c) = (pts[i], pts[(i + 1) % pts.len()], pts[(i + 2) % pts.len()]);
        let cross = (b - a).x * (c - b).y - (b - a).y * (c - b).x;
        if cross.abs() > 1e-7 {
            if sign != 0.0 && cross.signum() != sign {
                return false;
            }
            sign = cross.signum();
        }
    }
    true
}

/// Where the preview drew the drawing: its corner and the size of the
/// longer side on screen.
#[derive(Clone, Copy)]
struct PreviewMap {
    o: Pos2,
    side: f32,
}

impl PreviewMap {
    fn to_screen(self, data: &PreviewData, x: f64, y: f64) -> Pos2 {
        let (x0, y1, side) = data.frame;
        self.o
            + Vec2::new(
                ((x - x0) / side) as f32 * self.side,
                ((y1 - y) / side) as f32 * self.side,
            )
    }
    fn to_drawing(self, data: &PreviewData, p: Pos2) -> (f64, f64) {
        let (x0, y1, side) = data.frame;
        let v = (p - self.o) / self.side.max(1e-3);
        (x0 + v.x as f64 * side, y1 - v.y as f64 * side)
    }
}

fn draw_preview(ui: &mut Ui, data: &PreviewData, sense: Sense) -> (egui::Response, PreviewMap) {
    let w = ui.available_width().clamp(120.0, 320.0);
    let (r, resp) = ui.allocate_exact_size(Vec2::new(w, w), sense);
    let painter = ui.painter_at(r);
    painter.rect_filled(r, 0.0, Color32::WHITE);
    painter.rect_stroke(
        r,
        0.0,
        Stroke::new(1.0, Tokens::border()),
        egui::StrokeKind::Inside,
    );
    let inner = r.shrink(8.0);
    let side = inner.width();
    // Centred: the drawing's longer side fills the box.
    let (dw, dh) = if data.aspect >= 1.0 {
        (side, side / data.aspect.max(1e-3))
    } else {
        (side * data.aspect, side)
    };
    let o = inner.center() - Vec2::new(dw / 2.0, dh / 2.0);
    let pm = PreviewMap { o, side };
    let map = |p: Pos2| o + Vec2::new(p.x * side, p.y * side);
    let ink = Color32::from_gray(0x20);
    let line = Stroke::new(1.0, ink);
    for (pts, closed, paint) in &data.curves {
        let pts: Vec<Pos2> = pts.iter().map(|p| map(*p)).collect();
        let fill = match paint {
            Paint::Line | Paint::Accent | Paint::Mesh => None,
            Paint::Tone(t) => {
                let [r, g, b] =
                    crate::shape_gen_bg::palette_color(&data.palette, *t as f64 / 255.0);
                Some(Color32::from_rgb(r, g, b))
            }
            Paint::Solid => Some(ink),
            Paint::Face(0) => Some(Color32::from_gray(0xE8)),
            Paint::Face(1) => Some(Color32::from_gray(0x9C)),
            Paint::Face(_) => Some(Color32::from_gray(0x4E)),
            Paint::Ink(0) => Some(Color32::from_rgba_unmultiplied(0x00, 0xAE, 0xEF, 0xB0)),
            Paint::Ink(1) => Some(Color32::from_rgba_unmultiplied(0xEC, 0x00, 0x8C, 0xB0)),
            Paint::Ink(2) => Some(Color32::from_rgba_unmultiplied(0xFF, 0xE6, 0x00, 0xB0)),
            Paint::Ink(_) => Some(Color32::from_rgba_unmultiplied(0x10, 0x10, 0x10, 0xB0)),
        };
        match fill {
            Some(f) if *closed && convex(&pts) => {
                let edge = if matches!(paint, Paint::Face(_)) {
                    Stroke::new(0.6, Color32::BLACK)
                } else {
                    Stroke::NONE
                };
                painter.add(egui::Shape::convex_polygon(pts, f, edge));
            }
            Some(f) if *closed && matches!(paint, Paint::Tone(_)) => {
                painter.add(egui::Shape::mesh(fill_spans(&pts, f)));
            }
            _ if *paint == Paint::Mesh => {
                if let Some((cols, nodes)) = &data.mesh {
                    painter.add(egui::Shape::mesh(mesh_cells(*cols, nodes, map)));
                }
            }
            _ if *closed => {
                painter.add(egui::Shape::closed_line(pts, line));
            }
            _ if *paint == Paint::Accent => {
                painter.add(egui::Shape::line(pts, Stroke::new(1.4, ACCENT)));
            }
            _ => {
                painter.add(egui::Shape::line(pts, line));
            }
        }
    }
    (resp, pm)
}

/// A filled polygon of any shape, as thin rows of its spans (even-odd).
fn fill_spans(pts: &[Pos2], color: Color32) -> egui::Mesh {
    let mut mesh = egui::Mesh::default();
    let (lo, hi) = pts.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| {
        (lo.min(p.y), hi.max(p.y))
    });
    let step = 1.0f32;
    let mut y = lo + step / 2.0;
    let mut xs: Vec<f32> = Vec::new();
    while y < hi {
        xs.clear();
        for i in 0..pts.len() {
            let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
            if (a.y <= y) != (b.y <= y) {
                xs.push(a.x + (y - a.y) / (b.y - a.y) * (b.x - a.x));
            }
        }
        xs.sort_by(f32::total_cmp);
        for pair in xs.chunks_exact(2) {
            mesh.add_colored_rect(
                egui::Rect::from_min_max(
                    Pos2::new(pair[0], y - step / 2.0),
                    Pos2::new(pair[1], y + step / 2.0),
                ),
                color,
            );
        }
        y += step;
    }
    mesh
}

/// A mesh fill's cells as triangles shaded between their corner colours.
fn mesh_cells(cols: usize, nodes: &[(Pos2, Color32)], map: impl Fn(Pos2) -> Pos2) -> egui::Mesh {
    let mut mesh = egui::Mesh::default();
    for (p, c) in nodes {
        mesh.colored_vertex(map(*p), *c);
    }
    if cols < 2 {
        return mesh;
    }
    let rows = nodes.len() / cols;
    for r in 0..rows.saturating_sub(1) {
        for c in 0..cols - 1 {
            let i = (r * cols + c) as u32;
            let (a, b, d, e) = (i, i + 1, i + cols as u32, i + cols as u32 + 1);
            mesh.add_triangle(a, b, e);
            mesh.add_triangle(a, e, d);
        }
    }
    mesh
}

/// How the preview shows lines inside the split shape and the pins.
const ACCENT: Color32 = Color32::from_rgb(0xD0, 0x30, 0x70);

/// Pins edited in the preview: drag from anywhere to add a pin and pull,
/// drag a pin's end to change its pull or its start to move it, right
/// click one to delete it.
fn edit_pins(app: &mut App, data: &PreviewData, resp: &egui::Response, pm: PreviewMap) {
    use crate::shape_gen_warp::{pins, MAX_PINS};
    let s = &mut app.shape_gen;
    let (w, h) = (s.width.max(1e-9), s.height.max(1e-9));
    let side = w.max(h);
    let painter = resp
        .ctx
        .layer_painter(resp.layer_id)
        .with_clip_rect(resp.rect);
    let at = pins(s);
    let screen: Vec<(Pos2, Pos2)> = at
        .iter()
        .map(|(a, pull)| {
            (
                pm.to_screen(data, a.x, a.y),
                pm.to_screen(data, a.x + pull.x, a.y + pull.y),
            )
        })
        .collect();
    for (a, b) in &screen {
        painter.line_segment([*a, *b], Stroke::new(1.2, ACCENT));
        painter.circle_stroke(*a, 3.0, Stroke::new(1.2, ACCENT));
        painter.circle_filled(*b, 3.5, ACCENT);
    }
    // The end first: a pin with no pull yet is moved by its end.
    let hit = |p: Pos2| -> Option<(bool, usize)> {
        let near = |q: Pos2| q.distance(p) <= 8.0;
        screen
            .iter()
            .position(|(_, b)| near(*b))
            .map(|k| (true, k))
            .or_else(|| {
                screen
                    .iter()
                    .position(|(a, _)| near(*a))
                    .map(|k| (false, k))
            })
    };
    let id = resp.id.with("dragged_pin");
    if resp.drag_started() {
        let target = resp.interact_pointer_pos().and_then(|p| {
            hit(p).or_else(|| {
                (s.warp_pins.len() < MAX_PINS).then(|| {
                    let (x, y) = pm.to_drawing(data, p);
                    s.warp_pins.push([x / w, y / h, 0.0, 0.0]);
                    (true, s.warp_pins.len() - 1)
                })
            })
        });
        resp.ctx.data_mut(|d| d.insert_temp(id, target));
    }
    if resp.dragged() {
        let target: Option<(bool, usize)> = resp.ctx.data(|d| d.get_temp(id)).flatten();
        if let (Some((end, k)), Some(p)) = (target, resp.interact_pointer_pos()) {
            let (x, y) = pm.to_drawing(data, p);
            if let Some(pin) = s.warp_pins.get_mut(k) {
                if end {
                    pin[2] = (x - pin[0] * w) / side;
                    pin[3] = (y - pin[1] * h) / side;
                } else {
                    pin[0] = x / w;
                    pin[1] = y / h;
                }
            }
        }
    }
    if resp.drag_stopped() {
        resp.ctx
            .data_mut(|d| d.insert_temp::<Option<(bool, usize)>>(id, None));
    }
    if resp.secondary_clicked() {
        if let Some((_, k)) = resp.interact_pointer_pos().and_then(hit) {
            if k < s.warp_pins.len() {
                s.warp_pins.remove(k);
            }
        }
    }
}

/// Deform (pins and their reach) and Split by shape (how, the second
/// colour and width, and the picked outline).
fn deform_split(app: &mut App, ui: &mut Ui, units: crate::app::Units, precision: usize) {
    section(ui, None);
    let used = !app.shape_gen.warp_pins.is_empty() || app.shape_gen.split_mode != SplitMode::Off;
    let shown = egui::CollapsingHeader::new(tr("shapegen.deform"))
        .id_salt("shapegen_deform")
        .default_open(used)
        .show(ui, |ui| deform_split_body(app, ui, units, precision));
    shown
        .header_response
        .rich_tip(crate::ui::tips::keyed("shapegen.deform"));
}

fn deform_split_body(app: &mut App, ui: &mut Ui, units: crate::app::Units, precision: usize) {
    {
        let s = &mut app.shape_gen;
        ui.add(
            egui::Label::new(
                egui::RichText::new(tr("shapegen.deform_hint")).color(Tokens::text_dim()),
            )
            .wrap(),
        );
        row(ui, &tr("shapegen.warp_reach"), |ui| {
            slider(
                ui,
                &mut s.warp_reach,
                1.0..=100.0,
                0,
                " %",
                "shapegen.warp_reach",
            )
        });
        if !s.warp_pins.is_empty() {
            indented(ui, |ui| {
                if ui
                    .button(tr("shapegen.warp_clear"))
                    .rich_tip(crate::ui::tips::keyed("shapegen.warp_clear"))
                    .clicked()
                {
                    s.warp_pins.clear();
                }
            });
        }
        row(ui, &tr("shapegen.split_mode"), |ui| {
            combo(
                ui,
                "shapegen_split_mode",
                &mut s.split_mode,
                &SplitMode::ALL,
                SplitMode::key,
                "shapegen.split_mode",
            );
        });
        if s.split_mode == SplitMode::Colors {
            row(ui, &tr("shapegen.split_color"), |ui| {
                ui.color_edit_button_srgb(&mut s.split_color)
                    .rich_tip(crate::ui::tips::keyed("shapegen.split_color"));
            });
            row(ui, &tr("shapegen.split_width"), |ui| {
                length(
                    ui,
                    units,
                    precision,
                    &mut s.split_width,
                    0.0,
                    50.0,
                    "shapegen.split_width",
                )
            });
        }
    }
    if app.shape_gen.split_mode == SplitMode::Off {
        return;
    }
    let picked = app.shape_gen.split_shape.is_some();
    let text = if picked {
        tr("shapegen.split_picked")
    } else {
        tr("shapegen.split_none")
    };
    ui.add(egui::Label::new(egui::RichText::new(text).color(Tokens::text_dim())).wrap());
    let can = !app.selection.is_empty() && app.shape_gen_selected().is_none();
    ui.horizontal(|ui| {
        if ui
            .add_enabled(can, egui::Button::new(tr("shapegen.split_pick")))
            .rich_tip(crate::ui::tips::keyed("shapegen.split_pick"))
            .clicked()
        {
            app.shape_gen_pick_split();
        }
        if picked
            && ui
                .button(tr("shapegen.clear_source"))
                .rich_tip(crate::ui::tips::keyed("shapegen.split_clear"))
                .clicked()
        {
            app.shape_gen_clear_split();
        }
    });
}

/// Voronoi sites edited in the preview: drag one to move it, double
/// click to add one, right click one to delete it. The first edit takes
/// the generated sites as the panel's own points.
fn edit_sites(app: &mut App, data: &PreviewData, resp: &egui::Response, pm: PreviewMap) {
    let s = &mut app.shape_gen;
    let (w, h) = (s.width.max(1e-9), s.height.max(1e-9));
    let painter = resp
        .ctx
        .layer_painter(resp.layer_id)
        .with_clip_rect(resp.rect);
    let hit = |p: Pos2| -> Option<usize> {
        data.sites
            .iter()
            .enumerate()
            .map(|(k, (x, y))| (k, pm.to_screen(data, *x, *y).distance(p)))
            .filter(|(_, d)| *d <= 8.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(k, _)| k)
    };
    let take = |s: &mut ShapeGenSettings| {
        if s.vor_points.is_empty() {
            s.vor_points = data
                .sites
                .iter()
                .map(|(x, y)| [(x / w).clamp(0.0, 1.0), (y / h).clamp(0.0, 1.0)])
                .collect();
        }
    };
    let norm = |p: Pos2| {
        let (x, y) = pm.to_drawing(data, p);
        [(x / w).clamp(0.0, 1.0), (y / h).clamp(0.0, 1.0)]
    };
    let id = resp.id.with("dragged_site");
    if let Some(p) = resp.hover_pos() {
        if let Some(k) = hit(p) {
            let (x, y) = data.sites[k];
            painter.circle_stroke(
                pm.to_screen(data, x, y),
                5.0,
                Stroke::new(1.5, Tokens::selected_row_line()),
            );
        }
    }
    if resp.drag_started() {
        let k = resp.interact_pointer_pos().and_then(hit);
        resp.ctx.data_mut(|d| d.insert_temp(id, k));
        if k.is_some() {
            take(s);
        }
    }
    if resp.dragged() {
        let k: Option<usize> = resp.ctx.data(|d| d.get_temp(id)).flatten();
        if let (Some(k), Some(p)) = (k, resp.interact_pointer_pos()) {
            if let Some(slot) = s.vor_points.get_mut(k) {
                *slot = norm(p);
            }
        }
    }
    if resp.drag_stopped() {
        resp.ctx
            .data_mut(|d| d.insert_temp::<Option<usize>>(id, None));
    }
    if resp.double_clicked() {
        if let Some(p) = resp.interact_pointer_pos() {
            take(s);
            if s.vor_points.len() < crate::shape_gen_cells::MAX_SITES {
                s.vor_points.push(norm(p));
            }
        }
    }
    if resp.secondary_clicked() {
        if let Some(k) = resp.interact_pointer_pos().and_then(hit) {
            take(s);
            if k < s.vor_points.len() && s.vor_points.len() > 1 {
                s.vor_points.remove(k);
            }
        }
    }
}

/// A length field in the document's unit holding millimetres.
fn length(
    ui: &mut Ui,
    app_units: crate::app::Units,
    precision: usize,
    mm: &mut f64,
    lo: f64,
    hi: f64,
    key: &str,
) {
    let mut v = app_units.world_from_mm(*mm);
    let r = ui
        .add(
            crate::ui::field::NumField::new(&mut v)
                .range(app_units.world_from_mm(lo)..=app_units.world_from_mm(hi))
                .length_decimals(precision)
                .unit_mm(app_units.world_to_mm(1.0))
                .suffix(format!(" {}", app_units.short()))
                .width(80.0),
        )
        .rich_tip(crate::ui::tips::keyed(key));
    if r.changed() {
        let x = app_units.world_to_mm(v);
        if x.is_finite() {
            *mm = x.clamp(lo, hi);
        }
    }
}

fn slider(
    ui: &mut Ui,
    v: &mut f64,
    range: std::ops::RangeInclusive<f64>,
    decimals: usize,
    suffix: &str,
    key: &str,
) {
    ui.add(
        crate::ui::Slider::new(v, range)
            .max_decimals(decimals)
            .suffix(suffix),
    )
    .rich_tip(crate::ui::tips::keyed(key));
}

fn count(ui: &mut Ui, v: &mut u32, range: std::ops::RangeInclusive<u32>, key: &str) {
    ui.add(crate::ui::field::NumField::new(v).range(range).width(80.0))
        .rich_tip(crate::ui::tips::keyed(key));
}

fn combo<T: Copy + PartialEq>(
    ui: &mut Ui,
    id: &str,
    v: &mut T,
    all: &[T],
    key_of: fn(T) -> &'static str,
    tip: &str,
) {
    crate::ui::keynav::ComboBox::from_id_salt(id)
        .selected_text(tr(key_of(*v)))
        .width(ui.available_width().clamp(120.0, 190.0))
        .show_ui(ui, |ui| {
            for k in all {
                ui.selectable_value(v, *k, tr(key_of(*k)));
            }
        })
        .response
        .rich_tip(crate::ui::tips::keyed(tip));
}

/// The palette: a colour button for each colour, Add and Remove.
fn palette_row(ui: &mut Ui, s: &mut ShapeGenSettings) {
    row(ui, &tr("shapegen.bg_colors"), |ui| {
        ui.horizontal_wrapped(|ui| {
            for c in s.bg_colors.iter_mut() {
                ui.color_edit_button_srgb(c)
                    .rich_tip(crate::ui::tips::keyed("shapegen.bg_colors"));
            }
            let n = s.bg_colors.len();
            if ui
                .add_enabled(
                    n < crate::shape_gen_bg::MAX_COLORS,
                    egui::Button::new("+").small(),
                )
                .rich_tip(crate::ui::tips::keyed("shapegen.bg_add_color"))
                .clicked()
            {
                let last = s.bg_colors.last().copied().unwrap_or([128, 128, 128]);
                s.bg_colors.push(last);
            }
            if ui
                .add_enabled(n > 1, egui::Button::new("\u{2212}").small())
                .rich_tip(crate::ui::tips::keyed("shapegen.bg_remove_color"))
                .clicked()
            {
                s.bg_colors.pop();
            }
        });
    });
}

fn seed_row(ui: &mut Ui, s: &mut ShapeGenSettings) {
    row(ui, &tr("shapegen.seed"), |ui| {
        ui.add(
            crate::ui::field::NumField::new(&mut s.seed)
                .range(0..=999_999)
                .width(80.0),
        )
        .rich_tip(crate::ui::tips::keyed("shapegen.seed"));
        if ui
            .button(tr("shapegen.new_seed"))
            .rich_tip(crate::ui::tips::keyed("shapegen.new_seed"))
            .clicked()
        {
            s.seed = (s.seed.wrapping_mul(1_103_515_245).wrapping_add(12_345) >> 8) % 1_000_000;
        }
    });
}

/// The picked selection: what it is, Use Selection and Clear.
fn source_row(app: &mut App, ui: &mut Ui) {
    let s = &app.shape_gen;
    let text = if s.source_center.is_some() {
        tr("shapegen.source_picked")
    } else if matches!(s.kind, GenKind::Stipple | GenKind::Halftone)
        || (s.kind == GenKind::Hatch && s.hatch_mode == HatchMode::Tone)
        || (s.kind == GenKind::Contour && s.contour_mode == ContourMode::Tone)
    {
        tr("shapegen.source_none_tone")
    } else if s.kind != GenKind::Voronoi {
        tr("shapegen.source_none_fill")
    } else {
        tr("shapegen.source_none")
    };
    ui.label(egui::RichText::new(text).color(Tokens::text_dim()));
    let picked = s.source_center.is_some();
    let can = !app.selection.is_empty() && app.shape_gen_selected().is_none();
    ui.horizontal(|ui| {
        if ui
            .add_enabled(can, egui::Button::new(tr("shapegen.use_selection")))
            .rich_tip(crate::ui::tips::keyed("shapegen.use_selection"))
            .clicked()
        {
            app.shape_gen_use_selection();
        }
        if picked
            && ui
                .button(tr("shapegen.clear_source"))
                .rich_tip(crate::ui::tips::keyed("shapegen.clear_source"))
                .clicked()
        {
            app.shape_gen_clear_source();
        }
    });
}

pub fn shape_generator(app: &mut App, ui: &mut Ui) {
    ui.spacing_mut().slider_width = 110.0;
    let units = app.units;
    let precision = app.settings.precision.min(10) as usize;
    {
        let s = &mut app.shape_gen;
        row(ui, &tr("shapegen.kind"), |ui| {
            combo(
                ui,
                "shapegen_kind",
                &mut s.kind,
                &GenKind::ALL,
                GenKind::key,
                "shapegen.kind",
            );
        });
        let picked = s.kind.uses_source() && s.source_center.is_some();
        if s.kind.uses_box() && s.kind != GenKind::IsoBlocks && !picked {
            row(ui, &tr("shapegen.width"), |ui| {
                length(
                    ui,
                    units,
                    precision,
                    &mut s.width,
                    1.0,
                    5000.0,
                    "shapegen.width",
                );
            });
            row(ui, &tr("shapegen.height"), |ui| {
                length(
                    ui,
                    units,
                    precision,
                    &mut s.height,
                    1.0,
                    5000.0,
                    "shapegen.height",
                );
            });
        }
        section(ui, None);
        match s.kind {
            GenKind::DotGrid => {
                row(ui, &tr("shapegen.cells"), |ui| {
                    count(ui, &mut s.cells, 2..=200, "shapegen.cells")
                });
                row(ui, &tr("shapegen.cell_shape"), |ui| {
                    combo(
                        ui,
                        "shapegen_cell",
                        &mut s.cell_shape,
                        &CellShape::ALL,
                        CellShape::key,
                        "shapegen.cell_shape",
                    );
                });
                row(ui, &tr("shapegen.cell_size"), |ui| {
                    slider(
                        ui,
                        &mut s.cell_size,
                        5.0..=100.0,
                        0,
                        " %",
                        "shapegen.cell_size",
                    )
                });
                row(ui, &tr("shapegen.scale_ratio"), |ui| {
                    slider(
                        ui,
                        &mut s.scale_ratio,
                        0.0..=400.0,
                        0,
                        " %",
                        "shapegen.scale_ratio",
                    )
                });
                row(ui, &tr("shapegen.falloff"), |ui| {
                    slider(
                        ui,
                        &mut s.falloff,
                        10.0..=200.0,
                        0,
                        " %",
                        "shapegen.falloff",
                    )
                });
                row(ui, &tr("shapegen.warp"), |ui| {
                    combo(
                        ui,
                        "shapegen_warp",
                        &mut s.warp,
                        &Warp::ALL,
                        Warp::key,
                        "shapegen.warp",
                    );
                });
                if s.warp != Warp::None {
                    row(ui, &tr("shapegen.warp_strength"), |ui| {
                        slider(
                            ui,
                            &mut s.warp_strength,
                            -100.0..=100.0,
                            0,
                            "",
                            "shapegen.warp_strength",
                        )
                    });
                }
                if s.cell_shape != CellShape::Circle {
                    row(ui, &tr("shapegen.roundness"), |ui| {
                        slider(
                            ui,
                            &mut s.roundness,
                            0.0..=100.0,
                            0,
                            " %",
                            "shapegen.roundness",
                        )
                    });
                    row(ui, &tr("shapegen.cell_rotation"), |ui| {
                        slider(
                            ui,
                            &mut s.cell_rotation,
                            -180.0..=180.0,
                            1,
                            "\u{b0}",
                            "shapegen.cell_rotation",
                        )
                    });
                }
                indented(ui, |ui| {
                    ui.checkbox(&mut s.elliptical, tr("shapegen.elliptical"))
                        .rich_tip(crate::ui::tips::keyed("shapegen.elliptical"));
                });
                indented(ui, |ui| {
                    ui.checkbox(&mut s.filled, tr("shapegen.filled"))
                        .rich_tip(crate::ui::tips::keyed("shapegen.filled"));
                });
            }
            GenKind::Truchet => {
                row(ui, &tr("shapegen.tile"), |ui| {
                    length(
                        ui,
                        units,
                        precision,
                        &mut s.tile,
                        0.5,
                        1000.0,
                        "shapegen.tile",
                    )
                });
                row(ui, &tr("shapegen.truchet_style"), |ui| {
                    combo(
                        ui,
                        "shapegen_truchet",
                        &mut s.truchet_style,
                        &TruchetStyle::ALL,
                        TruchetStyle::key,
                        "shapegen.truchet_style",
                    );
                });
                if s.truchet_style == TruchetStyle::Arcs {
                    row(ui, &tr("shapegen.bands"), |ui| {
                        count(ui, &mut s.bands, 1..=12, "shapegen.bands")
                    });
                }
                seed_row(ui, s);
            }
            GenKind::Tessellation => {
                row(ui, &tr("shapegen.tess"), |ui| {
                    combo(
                        ui,
                        "shapegen_tess",
                        &mut s.tess,
                        &Tess::ALL,
                        Tess::key,
                        "shapegen.tess",
                    );
                });
                row(ui, &tr("shapegen.tile"), |ui| {
                    length(
                        ui,
                        units,
                        precision,
                        &mut s.tile,
                        0.5,
                        1000.0,
                        "shapegen.tile",
                    )
                });
                row(ui, &tr("shapegen.gap"), |ui| {
                    length(ui, units, precision, &mut s.gap, 0.0, 500.0, "shapegen.gap")
                });
                row(ui, &tr("shapegen.rotation"), |ui| {
                    slider(
                        ui,
                        &mut s.rotation,
                        -180.0..=180.0,
                        1,
                        "\u{b0}",
                        "shapegen.rotation",
                    )
                });
            }
            GenKind::LineWaves => {
                row(ui, &tr("shapegen.lines"), |ui| {
                    count(ui, &mut s.lines, 2..=1000, "shapegen.lines")
                });
                row(ui, &tr("shapegen.wave_width"), |ui| {
                    slider(
                        ui,
                        &mut s.wave_width,
                        5.0..=200.0,
                        0,
                        " %",
                        "shapegen.wave_width",
                    )
                });
                row(ui, &tr("shapegen.amplitude"), |ui| {
                    length(
                        ui,
                        units,
                        precision,
                        &mut s.amplitude,
                        0.0,
                        1000.0,
                        "shapegen.amplitude",
                    )
                });
                row(ui, &tr("shapegen.noise_x"), |ui| {
                    slider(ui, &mut s.noise_x, 0.1..=20.0, 1, "", "shapegen.noise_x")
                });
                row(ui, &tr("shapegen.noise_y"), |ui| {
                    slider(ui, &mut s.noise_y, 0.0..=1.0, 3, "", "shapegen.noise_y")
                });
                row(ui, &tr("shapegen.spread"), |ui| {
                    length(
                        ui,
                        units,
                        precision,
                        &mut s.spread,
                        -1000.0,
                        1000.0,
                        "shapegen.spread",
                    )
                });
                row(ui, &tr("shapegen.sharpness"), |ui| {
                    slider(
                        ui,
                        &mut s.sharpness,
                        0.5..=12.0,
                        1,
                        "",
                        "shapegen.sharpness",
                    )
                });
                row(ui, &tr("shapegen.fade"), |ui| {
                    slider(ui, &mut s.fade, 0.0..=1.0, 2, "", "shapegen.fade")
                });
                row(ui, &tr("shapegen.softness"), |ui| {
                    slider(ui, &mut s.softness, 0.0..=100.0, 0, "", "shapegen.softness")
                });
                seed_row(ui, s);
            }
            GenKind::Rings => {
                row(ui, &tr("shapegen.rings"), |ui| {
                    count(ui, &mut s.rings, 1..=500, "shapegen.rings")
                });
                row(ui, &tr("shapegen.min_radius"), |ui| {
                    length(
                        ui,
                        units,
                        precision,
                        &mut s.min_radius,
                        0.1,
                        2500.0,
                        "shapegen.min_radius",
                    )
                });
                row(ui, &tr("shapegen.max_radius"), |ui| {
                    length(
                        ui,
                        units,
                        precision,
                        &mut s.max_radius,
                        0.1,
                        2500.0,
                        "shapegen.max_radius",
                    )
                });
                row(ui, &tr("shapegen.ring_amplitude"), |ui| {
                    length(
                        ui,
                        units,
                        precision,
                        &mut s.ring_amplitude,
                        0.0,
                        1000.0,
                        "shapegen.ring_amplitude",
                    )
                });
                row(ui, &tr("shapegen.ring_detail"), |ui| {
                    slider(
                        ui,
                        &mut s.ring_detail,
                        0.5..=12.0,
                        1,
                        "",
                        "shapegen.ring_detail",
                    )
                });
                row(ui, &tr("shapegen.ring_drift"), |ui| {
                    slider(
                        ui,
                        &mut s.ring_drift,
                        0.0..=1.0,
                        2,
                        "",
                        "shapegen.ring_drift",
                    )
                });
                seed_row(ui, s);
            }
            GenKind::FieldLines => {
                row(ui, &tr("shapegen.poles"), |ui| {
                    count(ui, &mut s.poles, 1..=20, "shapegen.poles")
                });
                row(ui, &tr("shapegen.line_spacing"), |ui| {
                    length(
                        ui,
                        units,
                        precision,
                        &mut s.line_spacing,
                        0.2,
                        500.0,
                        "shapegen.line_spacing",
                    )
                });
                row(ui, &tr("shapegen.flow"), |ui| {
                    slider(ui, &mut s.flow, 0.0..=100.0, 0, "", "shapegen.flow")
                });
                row(ui, &tr("shapegen.swirl"), |ui| {
                    slider(
                        ui,
                        &mut s.swirl,
                        -90.0..=90.0,
                        0,
                        "\u{b0}",
                        "shapegen.swirl",
                    )
                });
                indented(ui, |ui| {
                    ui.checkbox(&mut s.show_poles, tr("shapegen.show_poles"))
                        .rich_tip(crate::ui::tips::keyed("shapegen.show_poles"));
                });
                seed_row(ui, s);
            }
            GenKind::IsoBlocks => {
                row(ui, &tr("shapegen.iso_layout"), |ui| {
                    combo(
                        ui,
                        "shapegen_iso",
                        &mut s.iso_layout,
                        &IsoLayout::ALL,
                        IsoLayout::key,
                        "shapegen.iso_layout",
                    );
                });
                row(ui, &tr("shapegen.iso_cols"), |ui| {
                    count(ui, &mut s.iso_cols, 1..=60, "shapegen.iso_cols")
                });
                row(ui, &tr("shapegen.iso_rows"), |ui| {
                    count(ui, &mut s.iso_rows, 1..=60, "shapegen.iso_rows")
                });
                row(ui, &tr("shapegen.iso_height"), |ui| {
                    count(ui, &mut s.iso_height, 1..=30, "shapegen.iso_height")
                });
                row(ui, &tr("shapegen.iso_size"), |ui| {
                    length(
                        ui,
                        units,
                        precision,
                        &mut s.iso_size,
                        0.5,
                        500.0,
                        "shapegen.iso_size",
                    )
                });
                row(ui, &tr("shapegen.iso_gap"), |ui| {
                    slider(ui, &mut s.iso_gap, 0.0..=80.0, 0, " %", "shapegen.iso_gap")
                });
                if s.iso_layout == IsoLayout::Random {
                    seed_row(ui, s);
                }
            }
            GenKind::ConeTemplate => {
                row(ui, &tr("shapegen.cone_top"), |ui| {
                    length(
                        ui,
                        units,
                        precision,
                        &mut s.cone_top,
                        0.0,
                        5000.0,
                        "shapegen.cone_top",
                    )
                });
                row(ui, &tr("shapegen.cone_bottom"), |ui| {
                    length(
                        ui,
                        units,
                        precision,
                        &mut s.cone_bottom,
                        0.0,
                        5000.0,
                        "shapegen.cone_bottom",
                    )
                });
                row(ui, &tr("shapegen.cone_height"), |ui| {
                    length(
                        ui,
                        units,
                        precision,
                        &mut s.cone_height,
                        0.5,
                        5000.0,
                        "shapegen.cone_height",
                    )
                });
                row(ui, &tr("shapegen.cone_tab"), |ui| {
                    length(
                        ui,
                        units,
                        precision,
                        &mut s.cone_tab,
                        0.0,
                        500.0,
                        "shapegen.cone_tab",
                    )
                });
                row(ui, &tr("shapegen.cone_bleed"), |ui| {
                    length(
                        ui,
                        units,
                        precision,
                        &mut s.cone_bleed,
                        0.0,
                        100.0,
                        "shapegen.cone_bleed",
                    )
                });
                row(ui, &tr("shapegen.cone_pieces"), |ui| {
                    count(ui, &mut s.cone_pieces, 1..=8, "shapegen.cone_pieces")
                });
                row(ui, &tr("shapegen.cone_repeat"), |ui| {
                    count(ui, &mut s.cone_repeat, 1..=24, "shapegen.cone_repeat")
                });
                // The measures, to check before cutting.
                let l = crate::shape_gen::cone_layout(s);
                let len = |mm: f64| format!("{:.1} {}", units.world_from_mm(mm), units.short());
                let text = match l.radii {
                    Some(r) => trf(
                        "shapegen.cone_measures",
                        &[
                            ("slant", &len(l.slant)),
                            ("top", &len(r.top)),
                            ("bottom", &len(r.bottom)),
                            ("angle", &format!("{:.1}\u{b0}", l.angle)),
                        ],
                    ),
                    None => trf(
                        "shapegen.cylinder_measures",
                        &[
                            ("width", &len(l.circumference_top)),
                            ("height", &len(l.slant)),
                        ],
                    ),
                };
                ui.label(egui::RichText::new(text).color(Tokens::text_dim()));
            }
            GenKind::Voronoi => {
                row(ui, &tr("shapegen.points"), |ui| {
                    combo(
                        ui,
                        "shapegen_points",
                        &mut s.vor_mode,
                        &PointMode::ALL,
                        PointMode::key,
                        "shapegen.points",
                    );
                });
                row(ui, &tr("shapegen.spacing"), |ui| {
                    length(
                        ui,
                        units,
                        precision,
                        &mut s.vor_spacing,
                        0.1,
                        1000.0,
                        "shapegen.spacing",
                    )
                });
                if matches!(s.vor_mode, PointMode::Hex | PointMode::Square) {
                    row(ui, &tr("shapegen.jitter"), |ui| {
                        slider(
                            ui,
                            &mut s.vor_jitter,
                            0.0..=100.0,
                            0,
                            " %",
                            "shapegen.jitter",
                        )
                    });
                }
                row(ui, &tr("shapegen.relax"), |ui| {
                    count(ui, &mut s.vor_relax, 0..=50, "shapegen.relax")
                });
                row(ui, &tr("shapegen.gradient"), |ui| {
                    combo(
                        ui,
                        "shapegen_gradient",
                        &mut s.vor_gradient,
                        &Gradient::ALL,
                        Gradient::key,
                        "shapegen.gradient",
                    );
                });
                if s.vor_gradient != Gradient::None {
                    row(ui, &tr("shapegen.grad_strength"), |ui| {
                        slider(
                            ui,
                            &mut s.vor_grad_strength,
                            0.0..=100.0,
                            0,
                            " %",
                            "shapegen.grad_strength",
                        )
                    });
                    if s.vor_gradient == Gradient::Linear {
                        row(ui, &tr("shapegen.grad_angle"), |ui| {
                            slider(
                                ui,
                                &mut s.vor_grad_angle,
                                -180.0..=180.0,
                                0,
                                "\u{b0}",
                                "shapegen.grad_angle",
                            )
                        });
                    }
                    indented(ui, |ui| {
                        ui.checkbox(&mut s.vor_grad_edge, tr("shapegen.grad_edge"))
                            .rich_tip(crate::ui::tips::keyed("shapegen.grad_edge"));
                    });
                }
                row(ui, &tr("shapegen.inset"), |ui| {
                    length(
                        ui,
                        units,
                        precision,
                        &mut s.vor_inset,
                        0.0,
                        100.0,
                        "shapegen.inset",
                    )
                });
                row(ui, &tr("shapegen.corner_radius"), |ui| {
                    length(
                        ui,
                        units,
                        precision,
                        &mut s.vor_round,
                        0.0,
                        100.0,
                        "shapegen.corner_radius",
                    )
                });
                indented(ui, |ui| {
                    ui.checkbox(&mut s.vor_cells, tr("shapegen.show_cells"))
                        .rich_tip(crate::ui::tips::keyed("shapegen.show_cells"));
                });
                indented(ui, |ui| {
                    ui.checkbox(&mut s.vor_show_sites, tr("shapegen.show_sites"))
                        .rich_tip(crate::ui::tips::keyed("shapegen.show_sites"));
                });
                indented(ui, |ui| {
                    ui.checkbox(&mut s.vor_delaunay, tr("shapegen.show_delaunay"))
                        .rich_tip(crate::ui::tips::keyed("shapegen.show_delaunay"));
                });
                if s.vor_points.is_empty() {
                    seed_row(ui, s);
                }
                ui.horizontal(|ui| {
                    if ui
                        .button(tr("shapegen.relax_once"))
                        .rich_tip(crate::ui::tips::keyed("shapegen.relax_once"))
                        .clicked()
                    {
                        s.vor_points = crate::shape_gen_cells::voronoi_relax_once(s);
                    }
                    if !s.vor_points.is_empty()
                        && ui
                            .button(tr("shapegen.reset_points"))
                            .rich_tip(crate::ui::tips::keyed("shapegen.reset_points"))
                            .clicked()
                    {
                        s.vor_points.clear();
                    }
                });
            }
            GenKind::Stipple => {
                row(ui, &tr("shapegen.dots"), |ui| {
                    count(ui, &mut s.st_dots, 10..=20_000, "shapegen.dots")
                });
                row(ui, &tr("shapegen.st_iter"), |ui| {
                    count(ui, &mut s.st_iter, 0..=100, "shapegen.st_iter")
                });
                row(ui, &tr("shapegen.st_min"), |ui| {
                    length(
                        ui,
                        units,
                        precision,
                        &mut s.st_min,
                        0.0,
                        50.0,
                        "shapegen.st_min",
                    )
                });
                row(ui, &tr("shapegen.st_max"), |ui| {
                    length(
                        ui,
                        units,
                        precision,
                        &mut s.st_max,
                        0.01,
                        50.0,
                        "shapegen.st_max",
                    )
                });
                indented(ui, |ui| {
                    ui.checkbox(&mut s.st_by_tone, tr("shapegen.st_by_tone"))
                        .rich_tip(crate::ui::tips::keyed("shapegen.st_by_tone"));
                });
                indented(ui, |ui| {
                    ui.checkbox(&mut s.invert, tr("shapegen.invert"))
                        .rich_tip(crate::ui::tips::keyed("shapegen.invert"));
                });
                seed_row(ui, s);
            }
            GenKind::Halftone => {
                row(ui, &tr("shapegen.ht_shape"), |ui| {
                    combo(
                        ui,
                        "shapegen_ht_shape",
                        &mut s.ht_shape,
                        &HtShape::ALL,
                        HtShape::key,
                        "shapegen.ht_shape",
                    );
                });
                row(ui, &tr("shapegen.ht_cell"), |ui| {
                    length(
                        ui,
                        units,
                        precision,
                        &mut s.ht_cell,
                        0.1,
                        500.0,
                        "shapegen.ht_cell",
                    )
                });
                if !s.ht_cmyk {
                    row(ui, &tr("shapegen.ht_angle"), |ui| {
                        slider(
                            ui,
                            &mut s.ht_angle,
                            -180.0..=180.0,
                            1,
                            "\u{b0}",
                            "shapegen.ht_angle",
                        )
                    });
                }
                row(ui, &tr("shapegen.ht_min"), |ui| {
                    slider(ui, &mut s.ht_min, 0.0..=200.0, 0, " %", "shapegen.ht_min")
                });
                row(ui, &tr("shapegen.ht_max"), |ui| {
                    slider(ui, &mut s.ht_max, 0.0..=200.0, 0, " %", "shapegen.ht_max")
                });
                indented(ui, |ui| {
                    ui.checkbox(&mut s.ht_cmyk, tr("shapegen.ht_cmyk"))
                        .rich_tip(crate::ui::tips::keyed("shapegen.ht_cmyk"));
                });
                indented(ui, |ui| {
                    ui.checkbox(&mut s.invert, tr("shapegen.invert"))
                        .rich_tip(crate::ui::tips::keyed("shapegen.invert"));
                });
            }
            GenKind::Spirograph => {
                row(ui, &tr("shapegen.spiro_type"), |ui| {
                    combo(
                        ui,
                        "shapegen_spiro",
                        &mut s.spiro_type,
                        &SpiroType::ALL,
                        SpiroType::key,
                        "shapegen.spiro_type",
                    );
                });
                match s.spiro_type {
                    SpiroType::Hypotrochoid | SpiroType::Epitrochoid => {
                        row(ui, &tr("shapegen.spiro_fixed"), |ui| {
                            slider(
                                ui,
                                &mut s.spiro_fixed,
                                1.0..=200.0,
                                1,
                                "",
                                "shapegen.spiro_fixed",
                            )
                        });
                        row(ui, &tr("shapegen.spiro_rolling"), |ui| {
                            slider(
                                ui,
                                &mut s.spiro_rolling,
                                0.5..=200.0,
                                1,
                                "",
                                "shapegen.spiro_rolling",
                            )
                        });
                        row(ui, &tr("shapegen.spiro_pen"), |ui| {
                            slider(
                                ui,
                                &mut s.spiro_pen,
                                0.0..=200.0,
                                1,
                                "",
                                "shapegen.spiro_pen",
                            )
                        });
                    }
                    SpiroType::Lissajous => {
                        row(ui, &tr("shapegen.liss_a"), |ui| {
                            count(ui, &mut s.liss_a, 1..=30, "shapegen.liss_a")
                        });
                        row(ui, &tr("shapegen.liss_b"), |ui| {
                            count(ui, &mut s.liss_b, 1..=30, "shapegen.liss_b")
                        });
                        row(ui, &tr("shapegen.liss_phase"), |ui| {
                            slider(
                                ui,
                                &mut s.liss_phase,
                                -180.0..=180.0,
                                0,
                                "\u{b0}",
                                "shapegen.liss_phase",
                            )
                        });
                    }
                    SpiroType::Rose => {
                        row(ui, &tr("shapegen.rose_n"), |ui| {
                            count(ui, &mut s.rose_n, 1..=30, "shapegen.rose_n")
                        });
                        row(ui, &tr("shapegen.rose_d"), |ui| {
                            count(ui, &mut s.rose_d, 1..=30, "shapegen.rose_d")
                        });
                    }
                    SpiroType::Mandala => {
                        row(ui, &tr("shapegen.mandala_fold"), |ui| {
                            count(ui, &mut s.mandala_fold, 3..=64, "shapegen.mandala_fold")
                        });
                        row(ui, &tr("shapegen.mandala_rings"), |ui| {
                            count(ui, &mut s.mandala_rings, 1..=12, "shapegen.mandala_rings")
                        });
                        seed_row(ui, s);
                    }
                }
            }
            GenKind::Guilloche => {
                row(ui, &tr("shapegen.guil_lines"), |ui| {
                    count(ui, &mut s.guil_lines, 1..=500, "shapegen.guil_lines")
                });
                row(ui, &tr("shapegen.min_radius"), |ui| {
                    length(
                        ui,
                        units,
                        precision,
                        &mut s.guil_inner,
                        0.0,
                        2500.0,
                        "shapegen.min_radius",
                    )
                });
                row(ui, &tr("shapegen.max_radius"), |ui| {
                    length(
                        ui,
                        units,
                        precision,
                        &mut s.guil_outer,
                        0.0,
                        2500.0,
                        "shapegen.max_radius",
                    )
                });
                row(ui, &tr("shapegen.guil_waves"), |ui| {
                    count(ui, &mut s.guil_waves, 0..=200, "shapegen.guil_waves")
                });
                row(ui, &tr("shapegen.amplitude"), |ui| {
                    length(
                        ui,
                        units,
                        precision,
                        &mut s.guil_amp,
                        0.0,
                        500.0,
                        "shapegen.amplitude",
                    )
                });
                row(ui, &tr("shapegen.guil_waves2"), |ui| {
                    count(ui, &mut s.guil_waves2, 0..=200, "shapegen.guil_waves2")
                });
                row(ui, &tr("shapegen.guil_amp2"), |ui| {
                    length(
                        ui,
                        units,
                        precision,
                        &mut s.guil_amp2,
                        0.0,
                        500.0,
                        "shapegen.guil_amp2",
                    )
                });
                row(ui, &tr("shapegen.guil_twist"), |ui| {
                    slider(
                        ui,
                        &mut s.guil_twist,
                        -90.0..=90.0,
                        1,
                        "\u{b0}",
                        "shapegen.guil_twist",
                    )
                });
            }
            GenKind::Fractal => {
                row(ui, &tr("shapegen.frac_type"), |ui| {
                    combo(
                        ui,
                        "shapegen_frac",
                        &mut s.frac_type,
                        &FractalType::ALL,
                        FractalType::key,
                        "shapegen.frac_type",
                    );
                });
                row(ui, &tr("shapegen.frac_depth"), |ui| {
                    count(ui, &mut s.frac_depth, 0..=14, "shapegen.frac_depth")
                });
                if matches!(s.frac_type, FractalType::Tree | FractalType::Custom) {
                    row(ui, &tr("shapegen.frac_angle"), |ui| {
                        slider(
                            ui,
                            &mut s.frac_angle,
                            -180.0..=180.0,
                            1,
                            "\u{b0}",
                            "shapegen.frac_angle",
                        )
                    });
                }
                match s.frac_type {
                    FractalType::Tree => {
                        row(ui, &tr("shapegen.frac_ratio"), |ui| {
                            slider(
                                ui,
                                &mut s.frac_ratio,
                                0.3..=0.95,
                                2,
                                "",
                                "shapegen.frac_ratio",
                            )
                        });
                        row(ui, &tr("shapegen.frac_jitter"), |ui| {
                            slider(
                                ui,
                                &mut s.frac_jitter,
                                0.0..=100.0,
                                0,
                                " %",
                                "shapegen.frac_jitter",
                            )
                        });
                        if s.frac_jitter > 0.0 {
                            seed_row(ui, s);
                        }
                    }
                    FractalType::Custom => {
                        row(ui, &tr("shapegen.frac_axiom"), |ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut s.frac_axiom).desired_width(150.0),
                            )
                            .rich_tip(crate::ui::tips::keyed("shapegen.frac_axiom"));
                        });
                        row(ui, &tr("shapegen.frac_rules"), |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut s.frac_rules)
                                    .desired_rows(2)
                                    .desired_width(150.0),
                            )
                            .rich_tip(crate::ui::tips::keyed("shapegen.frac_rules"));
                        });
                    }
                    t => {
                        if ui
                            .button(tr("shapegen.frac_edit"))
                            .rich_tip(crate::ui::tips::keyed("shapegen.frac_edit"))
                            .clicked()
                        {
                            let (a, r, ang) = crate::shape_gen_curves::preset_text(t);
                            s.frac_axiom = a;
                            s.frac_rules = r;
                            s.frac_angle = ang;
                            s.frac_type = FractalType::Custom;
                        }
                    }
                }
            }
            GenKind::Tiling => {
                row(ui, &tr("shapegen.tile_type"), |ui| {
                    combo(
                        ui,
                        "shapegen_tiling",
                        &mut s.tile_type,
                        &TilingType::ALL,
                        TilingType::key,
                        "shapegen.tile_type",
                    );
                });
                row(ui, &tr("shapegen.tile"), |ui| {
                    length(
                        ui,
                        units,
                        precision,
                        &mut s.tile_size,
                        0.2,
                        1000.0,
                        "shapegen.tile",
                    )
                });
                if s.tile_type == TilingType::Penrose {
                    indented(ui, |ui| {
                        ui.checkbox(&mut s.tile_fill, tr("shapegen.tile_fill"))
                            .rich_tip(crate::ui::tips::keyed("shapegen.tile_fill"));
                    });
                } else {
                    row(ui, &tr("shapegen.tile_angle"), |ui| {
                        slider(
                            ui,
                            &mut s.tile_angle,
                            20.0..=85.0,
                            1,
                            "\u{b0}",
                            "shapegen.tile_angle",
                        )
                    });
                    row(ui, &tr("shapegen.rotation"), |ui| {
                        slider(
                            ui,
                            &mut s.tile_rotation,
                            -180.0..=180.0,
                            1,
                            "\u{b0}",
                            "shapegen.rotation",
                        )
                    });
                }
            }
            GenKind::Hatch => {
                row(ui, &tr("shapegen.hatch_mode"), |ui| {
                    combo(
                        ui,
                        "shapegen_hatch",
                        &mut s.hatch_mode,
                        &HatchMode::ALL,
                        HatchMode::key,
                        "shapegen.hatch_mode",
                    );
                });
                row(ui, &tr("shapegen.hatch_spacing"), |ui| {
                    length(
                        ui,
                        units,
                        precision,
                        &mut s.hatch_spacing,
                        0.05,
                        1000.0,
                        "shapegen.hatch_spacing",
                    )
                });
                row(ui, &tr("shapegen.hatch_angle"), |ui| {
                    slider(
                        ui,
                        &mut s.hatch_angle,
                        -180.0..=180.0,
                        1,
                        "\u{b0}",
                        "shapegen.hatch_angle",
                    )
                });
                match s.hatch_mode {
                    HatchMode::Cross => {
                        row(ui, &tr("shapegen.hatch_cross"), |ui| {
                            slider(
                                ui,
                                &mut s.hatch_cross,
                                -180.0..=180.0,
                                1,
                                "\u{b0}",
                                "shapegen.hatch_cross",
                            )
                        });
                    }
                    HatchMode::Tone => {
                        row(ui, &tr("shapegen.hatch_layers"), |ui| {
                            count(ui, &mut s.hatch_layers, 1..=8, "shapegen.hatch_layers")
                        });
                        indented(ui, |ui| {
                            ui.checkbox(&mut s.invert, tr("shapegen.invert"))
                                .rich_tip(crate::ui::tips::keyed("shapegen.invert"));
                        });
                    }
                    HatchMode::Parallel => {}
                }
            }
            GenKind::Contour => {
                row(ui, &tr("shapegen.contour_mode"), |ui| {
                    combo(
                        ui,
                        "shapegen_contour",
                        &mut s.contour_mode,
                        &ContourMode::ALL,
                        ContourMode::key,
                        "shapegen.contour_mode",
                    );
                });
                match s.contour_mode {
                    ContourMode::Offset => {
                        row(ui, &tr("shapegen.contour_spacing"), |ui| {
                            length(
                                ui,
                                units,
                                precision,
                                &mut s.contour_spacing,
                                0.05,
                                1000.0,
                                "shapegen.contour_spacing",
                            )
                        });
                        row(ui, &tr("shapegen.contour_rings"), |ui| {
                            count(ui, &mut s.contour_rings, 1..=500, "shapegen.contour_rings")
                        });
                    }
                    ContourMode::Tone => {
                        row(ui, &tr("shapegen.contour_levels"), |ui| {
                            count(ui, &mut s.contour_levels, 1..=40, "shapegen.contour_levels")
                        });
                        indented(ui, |ui| {
                            ui.checkbox(&mut s.invert, tr("shapegen.invert"))
                                .rich_tip(crate::ui::tips::keyed("shapegen.invert"));
                        });
                    }
                }
            }
            GenKind::Maze => {
                row(ui, &tr("shapegen.maze_cell"), |ui| {
                    length(
                        ui,
                        units,
                        precision,
                        &mut s.maze_cell,
                        0.1,
                        1000.0,
                        "shapegen.maze_cell",
                    )
                });
                row(ui, &tr("shapegen.maze_style"), |ui| {
                    combo(
                        ui,
                        "shapegen_maze",
                        &mut s.maze_style,
                        &MazeStyle::ALL,
                        MazeStyle::key,
                        "shapegen.maze_style",
                    );
                });
                if s.maze_style == MazeStyle::Walls {
                    indented(ui, |ui| {
                        ui.checkbox(&mut s.maze_openings, tr("shapegen.maze_openings"))
                            .rich_tip(crate::ui::tips::keyed("shapegen.maze_openings"));
                    });
                }
                seed_row(ui, s);
            }
            GenKind::SpaceCurve => {
                row(ui, &tr("shapegen.curve_type"), |ui| {
                    combo(
                        ui,
                        "shapegen_curve",
                        &mut s.curve_type,
                        &SpaceCurve::ALL,
                        SpaceCurve::key,
                        "shapegen.curve_type",
                    );
                });
                let top = s.curve_type.max_order();
                row(ui, &tr("shapegen.curve_order"), |ui| {
                    count(ui, &mut s.curve_order, 1..=top, "shapegen.curve_order")
                });
                indented(ui, |ui| {
                    ui.checkbox(&mut s.curve_round, tr("shapegen.curve_round"))
                        .rich_tip(crate::ui::tips::keyed("shapegen.curve_round"));
                });
            }
            GenKind::SliceGrid => {
                row(ui, &tr("shapegen.slice_layout"), |ui| {
                    combo(
                        ui,
                        "shapegen_slice_layout",
                        &mut s.slice_layout,
                        &SliceLayout::ALL,
                        SliceLayout::key,
                        "shapegen.slice_layout",
                    );
                });
                if s.slice_layout != SliceLayout::Rows {
                    row(ui, &tr("shapegen.slice_cols"), |ui| {
                        count(ui, &mut s.slice_cols, 1..=200, "shapegen.slice_cols")
                    });
                }
                if s.slice_layout != SliceLayout::Columns {
                    row(ui, &tr("shapegen.slice_rows"), |ui| {
                        count(ui, &mut s.slice_rows, 1..=200, "shapegen.slice_rows")
                    });
                }
                row(ui, &tr("shapegen.slice_pattern"), |ui| {
                    combo(
                        ui,
                        "shapegen_slice_pattern",
                        &mut s.slice_pattern,
                        &SlicePattern::ALL,
                        SlicePattern::key,
                        "shapegen.slice_pattern",
                    );
                });
                if s.slice_pattern == SlicePattern::Wave {
                    row(ui, &tr("shapegen.slice_waves"), |ui| {
                        slider(
                            ui,
                            &mut s.slice_waves,
                            0.1..=20.0,
                            1,
                            "",
                            "shapegen.slice_waves",
                        )
                    });
                }
                row(ui, &tr("shapegen.slice_dx"), |ui| {
                    length(
                        ui,
                        units,
                        precision,
                        &mut s.slice_dx,
                        -1000.0,
                        1000.0,
                        "shapegen.slice_dx",
                    )
                });
                row(ui, &tr("shapegen.slice_dy"), |ui| {
                    length(
                        ui,
                        units,
                        precision,
                        &mut s.slice_dy,
                        -1000.0,
                        1000.0,
                        "shapegen.slice_dy",
                    )
                });
                row(ui, &tr("shapegen.slice_gap"), |ui| {
                    length(
                        ui,
                        units,
                        precision,
                        &mut s.slice_gap,
                        0.0,
                        1000.0,
                        "shapegen.slice_gap",
                    )
                });
                indented(ui, |ui| {
                    ui.checkbox(&mut s.slice_keep, tr("shapegen.slice_keep"))
                        .rich_tip(crate::ui::tips::keyed("shapegen.slice_keep"));
                });
                if s.slice_pattern == SlicePattern::Random {
                    seed_row(ui, s);
                }
            }
            GenKind::Growth => {
                row(ui, &tr("shapegen.growth_start"), |ui| {
                    combo(
                        ui,
                        "shapegen_growth_start",
                        &mut s.growth_start,
                        &GrowthStart::ALL,
                        GrowthStart::key,
                        "shapegen.growth_start",
                    );
                });
                if s.growth_start == GrowthStart::Seeds {
                    row(ui, &tr("shapegen.growth_seed_count"), |ui| {
                        count(
                            ui,
                            &mut s.growth_seed_count,
                            1..=50,
                            "shapegen.growth_seed_count",
                        )
                    });
                }
                row(ui, &tr("shapegen.growth_size"), |ui| {
                    slider(
                        ui,
                        &mut s.growth_size,
                        5.0..=100.0,
                        0,
                        " %",
                        "shapegen.growth_size",
                    )
                });
                row(ui, &tr("shapegen.growth_spacing"), |ui| {
                    length(
                        ui,
                        units,
                        precision,
                        &mut s.growth_spacing,
                        0.05,
                        50.0,
                        "shapegen.growth_spacing",
                    )
                });
                row(ui, &tr("shapegen.growth_repel"), |ui| {
                    slider(
                        ui,
                        &mut s.growth_repel,
                        1.0..=5.0,
                        1,
                        "",
                        "shapegen.growth_repel",
                    )
                });
                row(ui, &tr("shapegen.growth_align"), |ui| {
                    slider(
                        ui,
                        &mut s.growth_align,
                        0.0..=1.0,
                        2,
                        "",
                        "shapegen.growth_align",
                    )
                });
                row(ui, &tr("shapegen.growth_rate"), |ui| {
                    count(ui, &mut s.growth_rate, 0..=100, "shapegen.growth_rate")
                });
                row(ui, &tr("shapegen.growth_max_nodes"), |ui| {
                    count(
                        ui,
                        &mut s.growth_max_nodes,
                        100..=20_000,
                        "shapegen.growth_max_nodes",
                    )
                });
                row(ui, &tr("shapegen.growth_steps"), |ui| {
                    count(
                        ui,
                        &mut s.growth_steps,
                        0..=crate::shape_gen_growth::MAX_STEPS,
                        "shapegen.growth_steps",
                    )
                });
                seed_row(ui, s);
            }
            GenKind::LayerWaves => {
                palette_row(ui, s);
                row(ui, &tr("shapegen.bg_layers"), |ui| {
                    count(ui, &mut s.bg_layers, 1..=40, "shapegen.bg_layers")
                });
                row(ui, &tr("shapegen.bg_amplitude"), |ui| {
                    slider(
                        ui,
                        &mut s.bg_amplitude,
                        0.0..=100.0,
                        0,
                        " %",
                        "shapegen.bg_amplitude",
                    )
                });
                row(ui, &tr("shapegen.bg_frequency"), |ui| {
                    slider(
                        ui,
                        &mut s.bg_frequency,
                        0.1..=10.0,
                        1,
                        "",
                        "shapegen.bg_frequency",
                    )
                });
                row(ui, &tr("shapegen.bg_complexity"), |ui| {
                    count(ui, &mut s.bg_complexity, 1..=6, "shapegen.bg_complexity")
                });
                row(ui, &tr("shapegen.bg_side"), |ui| {
                    combo(
                        ui,
                        "shapegen_bg_side",
                        &mut s.bg_side,
                        &WaveSide::ALL,
                        WaveSide::key,
                        "shapegen.bg_side",
                    );
                });
                seed_row(ui, s);
            }
            GenKind::Blobs => {
                palette_row(ui, s);
                row(ui, &tr("shapegen.bg_blob_count"), |ui| {
                    count(ui, &mut s.bg_blob_count, 1..=200, "shapegen.bg_blob_count")
                });
                row(ui, &tr("shapegen.bg_blob_size"), |ui| {
                    slider(
                        ui,
                        &mut s.bg_blob_size,
                        1.0..=300.0,
                        0,
                        " %",
                        "shapegen.bg_blob_size",
                    )
                });
                row(ui, &tr("shapegen.bg_blob_points"), |ui| {
                    count(ui, &mut s.bg_blob_points, 3..=24, "shapegen.bg_blob_points")
                });
                row(ui, &tr("shapegen.bg_randomness"), |ui| {
                    slider(
                        ui,
                        &mut s.bg_randomness,
                        0.0..=90.0,
                        0,
                        " %",
                        "shapegen.bg_randomness",
                    )
                });
                seed_row(ui, s);
            }
            GenKind::SoftGradient => {
                palette_row(ui, s);
                row(ui, &tr("shapegen.bg_mesh_size"), |ui| {
                    count(ui, &mut s.bg_mesh_size, 1..=8, "shapegen.bg_mesh_size")
                });
                row(ui, &tr("shapegen.bg_jitter"), |ui| {
                    slider(
                        ui,
                        &mut s.bg_jitter,
                        0.0..=100.0,
                        0,
                        " %",
                        "shapegen.bg_jitter",
                    )
                });
                seed_row(ui, s);
            }
        }
        if !matches!(
            s.kind,
            GenKind::IsoBlocks
                | GenKind::Tessellation
                | GenKind::Stipple
                | GenKind::Halftone
                | GenKind::LayerWaves
                | GenKind::Blobs
                | GenKind::SoftGradient
        ) && !(s.kind == GenKind::Tiling && s.tile_fill && s.tile_type == TilingType::Penrose)
            && !(s.kind == GenKind::Truchet && s.truchet_style == TruchetStyle::Filled)
            && !(s.kind == GenKind::DotGrid && s.filled)
        {
            row(ui, &tr("shapegen.line_width"), |ui| {
                length(
                    ui,
                    units,
                    precision,
                    &mut s.line_width,
                    0.0,
                    50.0,
                    "shapegen.line_width",
                )
            });
        }
        if !matches!(
            s.kind,
            GenKind::IsoBlocks | GenKind::ConeTemplate | GenKind::SoftGradient
        ) {
            indented(ui, |ui| {
                ui.checkbox(&mut s.combine, tr("shapegen.combine"))
                    .rich_tip(crate::ui::tips::keyed("shapegen.combine"));
            });
        }
    }
    if app.shape_gen.kind.uses_source() {
        source_row(app, ui);
    }
    let warping = crate::shape_gen_warp::applies(app.shape_gen.kind);
    if warping {
        deform_split(app, ui, units, precision);
    }
    section(ui, None);
    let growing = app.shape_gen.kind == GenKind::Growth;
    let data = if growing {
        growth_controls(app, ui)
    } else {
        preview_for(ui, &app.shape_gen.sanitized())
    };
    let editing = app.shape_gen.kind == GenKind::Voronoi;
    let pinning = warping && !editing;
    let sense = if editing || pinning {
        Sense::click_and_drag()
    } else {
        Sense::hover()
    };
    let (resp, pm) = draw_preview(ui, &data, sense);
    if editing {
        let resp = resp.rich_tip(crate::ui::tips::keyed("shapegen.edit_points"));
        edit_sites(app, &data, &resp, pm);
    } else if pinning {
        let resp = resp.rich_tip(crate::ui::tips::keyed("shapegen.edit_pins"));
        edit_pins(app, &data, &resp, pm);
    }
    let n = if data.cut {
        trf(
            "shapegen.count_cut",
            &[("max", &crate::shape_gen::MAX_ITEMS.to_string())],
        )
    } else {
        trf("shapegen.count", &[("n", &data.count.to_string())])
    };
    ui.label(egui::RichText::new(n).color(Tokens::text_dim()));
    let selected = app.shape_gen_selected();
    let has_doc = app.active_layer().is_some();
    if wide_button(
        ui,
        &tr("shapegen.create"),
        has_doc && data.count > 0,
        &crate::ui::tips::keyed("shapegen.create"),
    ) {
        app.shape_gen_create();
    }
    if let Some((_, saved)) = &selected {
        ui.horizontal(|ui| {
            if ui
                .button(tr("shapegen.update"))
                .rich_tip(crate::ui::tips::keyed("shapegen.update"))
                .clicked()
            {
                app.shape_gen_update();
            }
            if ui
                .button(tr("shapegen.load"))
                .rich_tip(crate::ui::tips::keyed("shapegen.load"))
                .clicked()
            {
                app.shape_gen = saved.clone();
            }
        });
    }
    if app.shape_gen.kind == GenKind::SliceGrid {
        let can = has_doc && !app.selection.is_empty() && selected.is_none();
        if wide_button(
            ui,
            &tr("shapegen.slice_apply"),
            can,
            &crate::ui::tips::keyed("shapegen.slice_apply"),
        ) {
            app.shape_gen_slice_selection();
        }
    } else if app.shape_gen.kind == GenKind::ConeTemplate {
        let can = has_doc && !app.selection.is_empty() && selected.is_none();
        if wide_button(
            ui,
            &tr("shapegen.wrap"),
            can,
            &crate::ui::tips::keyed("shapegen.wrap"),
        ) {
            app.shape_gen_wrap_selection();
        }
    } else if wide_button(
        ui,
        &tr("shapegen.make_pattern"),
        has_doc && data.count > 0,
        &crate::ui::tips::keyed("shapegen.make_pattern"),
    ) && app.shape_gen_create().is_some()
    {
        app.dialog = crate::ui::dialogs::Dialog::CreatePattern { vector: true };
    }
}
