//! Second wave of tools: Contour, Crop, Knife, Spiral, Common Shapes,
//! Brush Strokes (calligraphic), Parallel Dimension, Connector, Table,
//! and Convert Outline To Object. These produce static results; see
//! docs/features.md.

use crate::app::{App, Drag};
use crate::tools::Tool;
use egui::{Modifiers, PointerButton, Response};
use tracedraw_core::{
    document::{Shape, ShapeKind},
    geometry::{BezPath, Point, Rect, Shape as _, Vec2},
    shaping, Command, Fill, ShapeId, Stroke,
};

pub use crate::common_shapes::CommonShape;

/// Archimedean or logarithmic spiral inscribed in `rect`.
pub fn spiral_path(rect: Rect, revolutions: u32, logarithmic: bool) -> BezPath {
    let c = rect.center();
    let (rx, ry) = (rect.width() / 2.0, rect.height() / 2.0);
    let turns = revolutions.max(1) as f64;
    let steps = (turns * 48.0) as usize;
    let mut pts = Vec::with_capacity(steps + 1);
    for i in 0..=steps {
        let t = i as f64 / steps as f64;
        let a = t * turns * std::f64::consts::TAU;
        let r = if logarithmic {
            (t * 4.0).exp() / 4.0f64.exp()
        } else {
            t
        };
        pts.push(Point::new(c.x + rx * r * a.cos(), c.y + ry * r * a.sin()));
    }
    tracedraw_core::geometry::smooth_path(&pts, false)
}

/// A grid of `rows` x `cols` cells covering `rect`: the outer frame and
/// the inner lines (previews of Graph Paper and Table).
pub fn grid_path(rect: Rect, rows: u32, cols: u32) -> BezPath {
    let mut p = rect.to_path(0.01);
    let (rows, cols) = (rows.max(1), cols.max(1));
    for c in 1..cols {
        let x = rect.x0 + rect.width() * c as f64 / cols as f64;
        p.move_to((x, rect.y0));
        p.line_to((x, rect.y1));
    }
    for r in 1..rows {
        let y = rect.y0 + rect.height() * r as f64 / rows as f64;
        p.move_to((rect.x0, y));
        p.line_to((rect.x1, y));
    }
    p
}

/// The Action Lines tool's speed lines across `rect`: parallel (ending at the
/// right edge, random lengths) or radial (around the centre). The lengths
/// come from a fixed pseudo-random sequence, so the live preview and the
/// object created on release are the same.
pub fn action_lines_path(rect: Rect, lines: u32, radial: bool) -> BezPath {
    let n = lines.clamp(2, 500) as usize;
    let mut path = BezPath::new();
    let mut seed: u32 = 0x9E37_79B9 ^ n as u32;
    let mut rnd = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        (seed % 1000) as f64 / 1000.0
    };
    if radial {
        let c = rect.center();
        let rmax = (rect.width().min(rect.height())) / 2.0;
        for i in 0..n {
            let a = std::f64::consts::TAU * i as f64 / n as f64;
            let dir = Vec2::new(a.cos(), a.sin());
            let r0 = rmax * (0.25 + 0.35 * rnd());
            let r1 = rmax * (0.75 + 0.25 * rnd());
            path.move_to(c + dir * r0);
            path.line_to(c + dir * r1);
        }
    } else {
        for i in 0..n {
            let y = rect.y0 + rect.height() * (i as f64 + 0.5) / n as f64;
            let len = rect.width() * (0.3 + 0.7 * rnd());
            path.move_to(Point::new(rect.x1 - len, y));
            path.line_to(Point::new(rect.x1, y));
        }
    }
    path
}

/// Calligraphic stroke: the outline swept by a flat nib along the points.
pub fn calligraphic_path(points: &[Point], width: f64, angle_deg: f64) -> BezPath {
    if points.len() < 2 {
        return BezPath::new();
    }
    let a = angle_deg.to_radians();
    let nib = Vec2::new(a.cos(), a.sin()) * (width / 2.0);
    let mut left: Vec<Point> = points.iter().map(|p| *p + nib).collect();
    let mut right: Vec<Point> = points.iter().map(|p| *p - nib).collect();
    right.reverse();
    left.append(&mut right);
    let p = tracedraw_core::geometry::smooth_path(&left, true);
    shaping::simplify(&p)
}

impl App {
    pub fn tools2_input(&mut self, response: &Response, p: Point, mods: Modifiers) {
        match self.tool {
            Tool::Knife if self.knife.mode == crate::knife::KnifeMode::Freehand => {
                if response.drag_started_by(PointerButton::Primary) {
                    self.drag = Drag::Freehand { points: vec![p] };
                }
                if response.dragged_by(PointerButton::Primary) {
                    if let Drag::Freehand { points } = &mut self.drag {
                        if points
                            .last()
                            .map(|l| (*l - p).hypot() > 0.2)
                            .unwrap_or(true)
                        {
                            points.push(p);
                        }
                    }
                }
            }
            Tool::Knife => {
                let p = self.snap_point(p);
                if response.drag_started_by(PointerButton::Primary) {
                    self.drag = Drag::Box {
                        start: p,
                        current: p,
                        from_center: false,
                    };
                }
                if response.dragged_by(PointerButton::Primary) {
                    if let Drag::Box { current, .. } = &mut self.drag {
                        *current = p;
                    }
                }
            }
            // Cells, lines and edge pointers of a table; elsewhere a drag
            // draws a new table (`table_tool.rs`).
            Tool::Table => self.table_tool_input(response, p, mods),
            Tool::Spiral | Tool::CommonShapes => self.box_like_input(response, p, mods),
            Tool::BrushStrokes | Tool::PainterlyBrush => {
                if self.tool == Tool::PainterlyBrush && response.clicked_by(PointerButton::Primary)
                {
                    // A click picks the object to paint with the bar.
                    match self.hit_test(p) {
                        Some(id) => self.select(vec![id]),
                        None => self.selection.clear(),
                    }
                }
                if response.drag_started_by(PointerButton::Primary) {
                    self.drag = Drag::Freehand { points: vec![p] };
                }
                if response.dragged_by(PointerButton::Primary) {
                    if let Drag::Freehand { points } = &mut self.drag {
                        if points
                            .last()
                            .map(|l| (*l - p).hypot() > 0.3)
                            .unwrap_or(true)
                        {
                            points.push(p);
                        }
                    }
                }
            }
            Tool::ParallelDimension => self.dimension_input(response, p),
            Tool::Connector => {
                if response.drag_started_by(PointerButton::Primary) {
                    if let Some(id) = self.hit_test_inside(p) {
                        self.drag = Drag::Connector {
                            from: id,
                            start: p,
                            current: p,
                        };
                    }
                }
                if response.dragged_by(PointerButton::Primary) {
                    if let Drag::Connector { current, .. } = &mut self.drag {
                        *current = p;
                    }
                }
            }
            _ => {}
        }
    }

    pub(crate) fn box_like_input(&mut self, response: &Response, p: Point, mods: Modifiers) {
        let p = self.snap_point(p);
        if response.drag_started_by(PointerButton::Primary) {
            self.drag = Drag::Box {
                start: p,
                current: p,
                from_center: mods.shift,
            };
        }
        if response.dragged_by(PointerButton::Primary) {
            if let Drag::Box {
                start,
                current,
                from_center,
            } = &mut self.drag
            {
                let mut q = p;
                if mods.ctrl {
                    let d = q - *start;
                    let m = d.x.abs().max(d.y.abs());
                    q = *start + Vec2::new(m * d.x.signum(), m * d.y.signum());
                }
                *from_center = mods.shift;
                *current = q;
            }
        }
    }

    /// The object a box drag with the current tool would create, as page
    /// space outlines, so the shape itself follows the pointer while
    /// dragging (shape tools never show an empty box). `None` when the drag does not create an object outline
    /// (crop, zoom, text frames).
    pub fn creation_preview(
        &self,
        start: Point,
        current: Point,
        from_center: bool,
    ) -> Option<BezPath> {
        use tracedraw_core::geometry::{
            ellipse_arc_path, ellipse_path, polygon_path, polygon_rect_for_bounds,
        };
        let r = App::box_rect(start, current, from_center);
        let path = match self.tool {
            Tool::Rectangle => self.rect_corners.path(r, (1.0, 1.0)),
            Tool::Ellipse => match &self.ellipse_arc {
                None => ellipse_path(r),
                Some(a) => ellipse_arc_path(r, a.start_deg, a.end_deg, a.pie),
            },
            Tool::Polygon => polygon_path(
                polygon_rect_for_bounds(r, self.polygon_points, 0.0),
                self.polygon_points,
                0.0,
            ),
            Tool::Star if self.star_complex => tracedraw_core::geometry::complex_star_path(
                polygon_rect_for_bounds(r, self.complex_points, 0.0),
                self.complex_points,
                self.complex_sharpness,
            ),
            Tool::Star => polygon_path(
                polygon_rect_for_bounds(r, self.polygon_points, self.star_sharpness),
                self.polygon_points,
                self.star_sharpness,
            ),
            Tool::Spiral => spiral_path(r, self.spiral_revolutions, self.spiral_logarithmic),
            Tool::CommonShapes => self.common_shape.path(r),
            Tool::GraphPaper => grid_path(r, self.graph_rows, self.graph_cols),
            Tool::ActionLines => {
                action_lines_path(r, self.action_lines_count, self.action_lines_radial)
            }
            Tool::Table => grid_path(r, self.table_rows, self.table_cols),
            Tool::Knife => {
                let mut p = BezPath::new();
                p.move_to(start);
                p.line_to(current);
                p
            }
            _ => return None,
        };
        Some(path)
    }

    /// Box drags for the second-wave tools, called from `end_drag`.
    pub fn finish_tools2_box(&mut self, start: Point, current: Point) -> bool {
        let rect = Rect::from_points(start, current);
        match self.tool {
            Tool::Spiral => {
                if rect.width() > 0.1 && rect.height() > 0.1 {
                    let path = spiral_path(rect, self.spiral_revolutions, self.spiral_logarithmic);
                    if let Some(id) = self.new_shape(ShapeKind::Path {
                        path,
                        closed: false,
                    }) {
                        self.select(vec![id]);
                    }
                }
                true
            }
            Tool::CommonShapes => {
                if rect.width() > 0.1 && rect.height() > 0.1 {
                    let path = self.common_shape.path(rect);
                    if let Some(id) = self.new_shape(ShapeKind::Path { path, closed: true }) {
                        self.select(vec![id]);
                    }
                }
                true
            }
            Tool::Table => {
                if rect.width() > 1.0 && rect.height() > 1.0 {
                    self.create_table(rect);
                }
                true
            }
            Tool::Knife => {
                if (current - start).hypot() > 0.5 {
                    self.knife(start, current);
                }
                true
            }
            Tool::SegmentDelete => {
                // A click whose pointer slipped a few pixels is still a
                // click: when its little box reaches nothing, the outline
                // near the press goes as a click would take it.
                let zoom = self.view.zoom.max(1e-6) as f64;
                let small = rect.width() * zoom < 8.0 && rect.height() * zoom < 8.0;
                if !self.delete_virtual_segments_in(rect) && small {
                    self.delete_virtual_segment_at(start);
                }
                true
            }
            _ => false,
        }
    }

    /// The Connector tool's drop: a straight connector to the object under
    /// the drop.
    pub fn finish_connector(&mut self, from: ShapeId, start: Point, at: Point) {
        if let Some(to) = self.hit_test_inside(at).filter(|id| *id != from) {
            self.connect(
                from,
                start,
                to,
                at,
                crate::connectors::ConnectorKind::Straight,
            );
        }
    }

    pub fn create_table(&mut self, rect: Rect) {
        let (rows, cols) = (self.table_rows.max(1), self.table_cols.max(1));
        let mut table = tracedraw_core::Table::new(rect, rows, cols, self.table_border.clone());
        table.cell_fill = self.table_fill.clone();
        if let Some(id) = self.new_shape(ShapeKind::Table(table)) {
            self.select(vec![id]);
        }
    }

    /// Ctrl+Shift+Q: replace an outline by a filled object of its shape.
    pub fn convert_outline_to_object(&mut self) {
        use i_overlay::mesh::float::stroke::offset::StrokeOffset;
        use i_overlay::mesh::float::style::{LineCap as ICap, LineJoin as IJoin, StrokeStyle};
        let shapes = self.selected_shapes();
        let Some(layer) = self.active_layer() else {
            return;
        };
        let mut cmds = Vec::new();
        let mut ids = Vec::new();
        for s in shapes {
            let Some(st) = &s.stroke else { continue };
            let width = if st.width <= Stroke::HAIRLINE + 1e-9 {
                0.25
            } else {
                st.width
            };
            let pp = s.page_path();
            let mut out = BezPath::new();
            // A variable-width or calligraphic outline becomes its band.
            let band = tracedraw_core::shaping::outline_band(&pp, st, 1.0);
            if let Some(b) = &band {
                out = b.clone();
            }
            // A dashed outline: each dash becomes a closed shape of its
            // own, the gaps kept (#490). The pattern is in widths, as the
            // renderer draws it.
            let pp = if st.dash.is_empty() || band.is_some() {
                pp
            } else {
                let mut pattern: Vec<f64> = st.dash.iter().map(|d| (d * width).max(0.01)).collect();
                if pattern.len() % 2 == 1 {
                    pattern = [pattern.clone(), pattern].concat();
                }
                kurbo::dash(pp.elements().iter().copied(), 0.0, &pattern).collect()
            };
            for contour in polylines(&pp).into_iter().filter(|_| band.is_none()) {
                let closed = contour.1;
                let pts = contour.0;
                let style = StrokeStyle::new(width)
                    .line_join(match st.join {
                        tracedraw_core::LineJoin::Round => IJoin::Round(0.05),
                        tracedraw_core::LineJoin::Bevel => IJoin::Bevel,
                        tracedraw_core::LineJoin::Miter => IJoin::Miter(4.0),
                    })
                    .start_cap(match st.cap {
                        tracedraw_core::LineCap::Round => ICap::Round(0.05),
                        tracedraw_core::LineCap::Square => ICap::Square,
                        tracedraw_core::LineCap::Butt => ICap::Butt,
                    })
                    .end_cap(match st.cap {
                        tracedraw_core::LineCap::Round => ICap::Round(0.05),
                        tracedraw_core::LineCap::Square => ICap::Square,
                        tracedraw_core::LineCap::Butt => ICap::Butt,
                    });
                let shapes_out = pts.stroke(style, closed);
                for shape in shapes_out {
                    for c in shape {
                        for (i, p) in c.iter().enumerate() {
                            let pt = Point::new(p[0], p[1]);
                            if i == 0 {
                                out.move_to(pt);
                            } else {
                                out.line_to(pt);
                            }
                        }
                        out.close_path();
                    }
                }
            }
            if out.elements().is_empty() {
                continue;
            }
            // The stroke comes back as many short lines: fit it to lines
            // and curves with as few nodes as keep its shape (#622).
            if band.is_none() {
                out = tracedraw_core::fit::fit_path(&out, OUTLINE_FIT_TOLERANCE);
            }
            let id = self.engine.new_shape_id();
            let mut o = Shape::new(
                id,
                ShapeKind::Path {
                    path: out,
                    closed: true,
                },
            );
            o.fill = Fill::Solid(st.color);
            o.stroke = None;
            o.name = Some("Outline".into());
            cmds.push(Command::AddShape { layer, shape: o });
            cmds.push(Command::SetStroke {
                shapes: vec![s.id],
                stroke: None,
            });
            ids.push(id);
        }
        if !cmds.is_empty() {
            let _ = self.engine.run_batch("Convert Outline To Object", &cmds);
            self.select(ids);
        }
    }
}

/// How far (mm) the curves of a converted outline may stray from the
/// flattened stroke they are fitted to.
const OUTLINE_FIT_TOLERANCE: f64 = 0.01;

/// Flattened subpaths of a page path as point lists with a closed flag.
/// A subpath that ends where it starts (an ellipse) is closed, its
/// closing point not repeated: stroked open, its ends made a notch there
/// (#622).
fn polylines(path: &BezPath) -> Vec<(Vec<[f64; 2]>, bool)> {
    use tracedraw_core::geometry::PathEl;
    fn finish(cur: &mut Vec<[f64; 2]>, closed: bool, out: &mut Vec<(Vec<[f64; 2]>, bool)>) {
        let meets = cur.len() > 2
            && cur
                .first()
                .zip(cur.last())
                .is_some_and(|(a, b)| (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-6);
        if meets {
            cur.pop();
        }
        if cur.len() >= 2 {
            out.push((std::mem::take(cur), closed || meets));
        }
        cur.clear();
    }
    let mut out: Vec<(Vec<[f64; 2]>, bool)> = Vec::new();
    let mut cur: Vec<[f64; 2]> = Vec::new();
    let mut closed = false;
    // Fine enough that the stroke's corners at the joins of the short
    // lines stay well inside the tolerance the outline is fitted to.
    kurbo::flatten(path.elements().iter().copied(), 0.005, &mut |el| match el {
        PathEl::MoveTo(p) => {
            finish(&mut cur, closed, &mut out);
            closed = false;
            cur.push([p.x, p.y]);
        }
        PathEl::LineTo(p) => cur.push([p.x, p.y]),
        PathEl::ClosePath => closed = true,
        _ => {}
    });
    finish(&mut cur, closed, &mut out);
    out
}

// ---------------------------------------------------------------------------
// Effects tools: Blend, Extrude, Distort; brush tools: Smooth, Smear, Twirl;
// Freehand Pick lasso.
// ---------------------------------------------------------------------------

impl App {
    pub fn effects_input(&mut self, response: &Response, p: Point, mods: Modifiers) {
        match self.tool {
            Tool::Blend => {
                if response.drag_started_by(PointerButton::Primary) {
                    if let Some(id) = self.hit_test_inside(p) {
                        self.drag = Drag::Connector {
                            from: id,
                            start: p,
                            current: p,
                        };
                    }
                }
                if response.dragged_by(PointerButton::Primary) {
                    if let Drag::Connector { current, .. } = &mut self.drag {
                        *current = p;
                    }
                }
            }
            Tool::Smooth | Tool::Smear | Tool::Twirl => self.brush_input(response, p, mods),
            _ => {}
        }
    }
}

/// Even-odd point-in-polygon on a flattened path.
pub fn point_in_path(path: &BezPath, p: Point) -> bool {
    let pts = tracedraw_core::effects::resample(path, 256);
    let mut inside = false;
    let n = pts.len();
    if n < 3 {
        return false;
    }
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (pts[i], pts[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y + 1e-12) + a.x
        {
            inside = !inside;
        }
        j = i;
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(p: &BezPath) -> Rect {
        p.bounding_box()
    }

    /// Convert Outline to Object on a dashed line keeps the dashes: each
    /// becomes a closed shape and the gaps stay empty (#490).
    #[test]
    fn a_dashed_outline_becomes_one_shape_per_dash() {
        let mut app = App::headless();
        let mut line = BezPath::new();
        line.move_to((10.0, 10.0));
        line.line_to((70.0, 10.0));
        let id = app
            .new_shape(ShapeKind::Path {
                path: line,
                closed: false,
            })
            .expect("line");
        let mut st = Stroke::new(tracedraw_core::Color::BLACK, 1.0);
        // 4 on, 2 off, in widths: 10 dashes over 60 mm.
        st.dash = vec![4.0, 2.0];
        st.cap = tracedraw_core::LineCap::Butt;
        app.select(vec![id]);
        app.run(Command::SetStroke {
            shapes: vec![id],
            stroke: Some(st),
        });
        app.convert_outline_to_object();
        let out = app
            .doc()
            .find_shape(app.selection[0])
            .cloned()
            .expect("object");
        let ShapeKind::Path { path, .. } = &out.kind else {
            panic!("a curve");
        };
        let moves = path
            .elements()
            .iter()
            .filter(|e| matches!(e, kurbo::PathEl::MoveTo(_)))
            .count();
        assert_eq!(moves, 10, "one closed shape per dash");
        // The gap between the first two dashes (4 to 6 mm in) is empty.
        let pp = out.page_path();
        assert!(!kurbo::Shape::contains(&pp, Point::new(15.0, 10.0)));
        assert!(kurbo::Shape::contains(&pp, Point::new(12.0, 10.0)));
    }

    /// Ctrl+Shift+Q on an object of `kind` with a `width` mm black
    /// outline set up by `style`: the new object's path.
    fn outline_object(kind: ShapeKind, width: f64, style: impl Fn(&mut Stroke)) -> (App, BezPath) {
        let mut app = App::headless();
        let id = app.new_shape(kind).expect("shape");
        let mut st = Stroke::new(tracedraw_core::Color::BLACK, width);
        style(&mut st);
        app.select(vec![id]);
        app.run(Command::SetStroke {
            shapes: vec![id],
            stroke: Some(st),
        });
        app.convert_outline_to_object();
        let out = app
            .doc()
            .find_shape(app.selection[0])
            .map(|s| s.page_path())
            .expect("object");
        (app, out)
    }

    /// Lines and curves of a path.
    fn segments(path: &BezPath) -> (usize, usize) {
        let els = path.elements();
        let lines = els
            .iter()
            .filter(|e| matches!(e, kurbo::PathEl::LineTo(_)))
            .count();
        let curves = els
            .iter()
            .filter(|e| matches!(e, kurbo::PathEl::CurveTo(..)))
            .count();
        (lines, curves)
    }

    /// Convert Outline to Object on an ellipse keeps it round: two rings
    /// of a few curves each, not hundreds of short lines (#622).
    #[test]
    fn an_ellipse_outline_becomes_a_few_curves() {
        let ring = ShapeKind::Ellipse {
            rect: Rect::new(10.0, 10.0, 70.0, 50.0),
            arc: None,
        };
        let (_, out) = outline_object(ring, 4.0, |_| {});
        let (lines, curves) = segments(&out);
        assert_eq!(lines, 0, "{:?}", out.elements());
        assert!((8..=16).contains(&curves), "{curves} curves");
        // The ring is 4 mm wide around the ellipse.
        assert!(kurbo::Shape::contains(&out, Point::new(11.0, 30.0)));
        assert!(!kurbo::Shape::contains(&out, Point::new(13.5, 30.0)));
        assert!(!kurbo::Shape::contains(&out, Point::new(7.5, 30.0)));
        let b = out.bounding_box();
        assert!(
            (b.width() - 64.0).abs() < 0.05 && (b.height() - 44.0).abs() < 0.05,
            "{b:?}"
        );
    }

    /// A straight line with butt ends becomes the rectangle of its
    /// outline: four straight sides, four nodes (#622).
    #[test]
    fn a_straight_outline_becomes_a_rectangle() {
        let mut line = BezPath::new();
        line.move_to((10.0, 10.0));
        line.line_to((70.0, 10.0));
        let line = ShapeKind::Path {
            path: line,
            closed: false,
        };
        let (_, out) = outline_object(line, 2.0, |st| {
            st.cap = tracedraw_core::LineCap::Butt;
        });
        assert_eq!(segments(&out), (4, 0), "{:?}", out.elements());
    }

    /// Every shape tool previews the object it is about to create, sized
    /// to the dragged box, while the drag is still in progress.
    #[test]
    fn shape_tools_preview_the_shape_being_drawn() {
        let mut app = App::headless();
        let (a, b) = (Point::new(10.0, 20.0), Point::new(60.0, 50.0));
        let full = Rect::from_points(a, b);
        for tool in [
            Tool::Rectangle,
            Tool::Ellipse,
            Tool::Polygon,
            Tool::Star,
            Tool::CommonShapes,
            Tool::GraphPaper,
            Tool::Table,
        ] {
            app.tool = tool;
            let p = app
                .creation_preview(a, b, false)
                .unwrap_or_else(|| panic!("{tool:?} has a preview"));
            let r = bounds(&p);
            assert!(
                (r.width() - full.width()).abs() < 0.5 && (r.height() - full.height()).abs() < 0.5,
                "{tool:?}: {r:?}"
            );
        }
        // The spiral fills the box but ends inside its corners.
        app.tool = Tool::Spiral;
        let r = bounds(&app.creation_preview(a, b, false).expect("spiral"));
        assert!(full.inflate(0.01, 0.01).contains_rect(r) && r.width() > full.width() * 0.8);
        // Shift: the box grows from the start point as its centre.
        app.tool = Tool::Rectangle;
        let r = bounds(&app.creation_preview(a, b, true).expect("rect"));
        assert!((r.center() - a).hypot() < 1e-6);
        // Crop and zoom boxes are not objects.
        app.tool = Tool::Crop;
        assert!(app.creation_preview(a, b, false).is_none());
    }

    #[test]
    fn knife_preview_follows_the_drag_direction() {
        let mut app = App::headless();
        app.tool = Tool::Knife;
        let (a, b) = (Point::new(10.0, 50.0), Point::new(60.0, 20.0));
        let p = app.creation_preview(a, b, false).expect("knife line");
        let els = p.elements();
        assert!(matches!(els[0], kurbo::PathEl::MoveTo(q) if q == a));
        assert!(matches!(els[1], kurbo::PathEl::LineTo(q) if q == b));
    }

    #[test]
    fn action_lines_preview_matches_the_created_object() {
        let r = Rect::new(0.0, 0.0, 80.0, 40.0);
        assert_eq!(
            action_lines_path(r, 12, false),
            action_lines_path(r, 12, false)
        );
        assert_eq!(action_lines_path(r, 12, true).elements().len(), 24);
        let g = grid_path(r, 4, 3);
        // Frame (5 elements) plus 2 vertical and 3 horizontal lines.
        assert_eq!(g.elements().len(), 5 + 2 * 2 + 3 * 2);
    }
}
