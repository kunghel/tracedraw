//! Input for the tools added with the full toolbox, the pending "click an
//! object" operations from menus, effect-node editing with the Shape tool,
//! and the right-click context menu trigger.

use crate::app::{App, Drag};
use crate::i18n::tr;
use crate::tools::Tool;
use egui::{Modifiers, PointerButton, Response};
use tracedraw_core::{
    document::{Shape, ShapeKind},
    geometry::{Affine, BezPath, Point, Rect, Vec2},
    live::Effect,
    Command, Fill, ShapeId, Stroke,
};

impl App {
    /// Menu operations waiting for a click on an object. Returns true when
    /// the click was consumed.
    pub fn pending_click(&mut self, response: &Response, p: Point) -> bool {
        let waiting = self.pending_order.is_some()
            || self.pending_copy_properties
            || self.pending_shaping.is_some()
            || self.pending_merge_frame
            || self.pending_copy_effect.is_some()
            || self.pending_copy_envelope
            || self.pending_copy_fill
            || self.pending_clone_effect.is_some()
            || self.pending_blend_path
            || self.pending_text_path
            || self.pending_rotate_lines.is_some()
            || self.pending_palette_sample
            || self.pending_replace.is_some()
            || self.pending_copy_clip;
        if !waiting {
            return false;
        }
        if response.secondary_clicked() {
            self.pending_order = None;
            self.pending_copy_properties = false;
            self.pending_shaping = None;
            self.pending_merge_frame = false;
            self.pending_copy_effect = None;
            self.pending_copy_envelope = false;
            self.pending_copy_fill = false;
            self.pending_clone_effect = None;
            self.pending_blend_path = false;
            self.pending_text_path = false;
            self.pending_rotate_lines = None;
            self.pending_palette_sample = false;
            self.pending_replace = None;
            self.pending_copy_clip = false;
            self.status = tr("status.cancelled");
            return true;
        }
        if !response.clicked_by(PointerButton::Primary) {
            return true;
        }
        if self.pending_rotate_lines.is_some() {
            // Rotate by Two Lines: each click snaps as a drawing click.
            let constrain = response.ctx.input(|i| self.constrain_held(&i.modifiers));
            let q = self.rotate_lines_point(self.snap_point(p), constrain);
            self.rotate_lines_click(q);
            return true;
        }
        if self.pending_palette_sample {
            // The document palette's eyedropper: the colour under the
            // pointer joins the palette; Ctrl keeps sampling.
            let (more, outline) = response
                .ctx
                .input(|i| (i.modifiers.ctrl, i.modifiers.shift));
            self.pending_palette_sample = more;
            self.sample_into_document_palette(p, outline);
            return true;
        }
        if self.pending_merge_frame {
            // The shape under the pointer, also one inside a group; a clip
            // frame rather than the picture it holds (#596).
            self.pending_merge_frame = false;
            match self.merge_frame_at(p) {
                Some(id) => {
                    self.select(vec![id]);
                    self.place_merge_field_inside();
                }
                None => self.status = tr("status.cancelled"),
            }
            return true;
        }
        if self.pending_shaping.is_some() {
            // The target is found under the selected objects too.
            let target = self.hit_test_where(p, |id| self.selection.contains(&id));
            self.shaping_onto(target);
            return true;
        }
        if let Some(clones) = self.pending_replace.take() {
            // Replace with Object / Clones (#4220): Shift fits each
            // replacement to the object's size.
            let fit = response.ctx.input(|i| i.modifiers.shift);
            match self.hit_test(p) {
                Some(t) => self.replace_selection_with(t, clones, fit),
                None => self.status = tr("status.cancelled"),
            }
            return true;
        }
        if self.pending_copy_clip {
            // Copy Contents From (D#4221): the clip frame clicked.
            self.pending_copy_clip = false;
            match self.hit_test(p) {
                Some(t) => {
                    self.copy_clip_contents_from(t);
                }
                None => self.status = tr("status.cancelled"),
            }
            return true;
        }
        let target = self.hit_test(p);
        if let Some(in_front) = self.pending_order.take() {
            if let Some(t) = target {
                self.order_relative_to(t, in_front);
            }
        } else if self.pending_copy_properties {
            self.pending_copy_properties = false;
            if let Some(t) = target {
                // Read from the object clicked inside a group or clip
                // frame, which shows the properties (#1550).
                match self.leaf_of(t, p) {
                    Some(leaf) => self.copy_properties_from_shape(t, &leaf),
                    None => self.copy_properties_from(t),
                }
            }
        } else if let Some(kind) = self.pending_copy_effect.take() {
            if let Some(t) = target {
                self.copy_effect_from(t, kind);
            }
        } else if self.pending_copy_fill {
            self.pending_copy_fill = false;
            if let Some(t) = target {
                // The fill the object clicked shows, inside a group or
                // clip frame too, as Copy Properties From reads it.
                let src = match self.leaf_of(t, p) {
                    Some(leaf) => Some(leaf),
                    None => self
                        .doc()
                        .find_shape(t)
                        .map(crate::copy_props::properties_source),
                };
                if let Some(src) = src {
                    self.copy_fill_from_shape(t, &src);
                }
            }
        } else if self.pending_copy_envelope {
            self.pending_copy_envelope = false;
            if let Some(t) = target {
                self.copy_envelope_from(t);
            }
        } else if let Some(kind) = self.pending_clone_effect.take() {
            if let Some(t) = target {
                self.clone_effect_from(t, kind);
                return true;
            }
        } else if self.pending_blend_path {
            self.pending_blend_path = false;
            if let Some(t) = target {
                self.set_blend_path(t);
            }
        } else if self.pending_text_path {
            self.pending_text_path = false;
            // The path the preview showed, near the pointer or under it.
            if let Some(t) = self.text_path_target(p).or(target) {
                self.fit_text_to_picked_path(t, Some(p));
                return true;
            }
        }
        self.status.clear();
        true
    }

    /// Tools that did not exist in the first toolbox.
    pub fn tools3_input(&mut self, response: &Response, p: Point, mods: Modifiers) {
        match self.tool {
            Tool::FreeTransform => self.free_transform_input(response, p, mods),
            Tool::AttractRepel | Tool::Smudge | Tool::Roughen => {
                self.brush_input(response, p, mods)
            }
            Tool::VariableOutline => self.variable_outline_input(response, p, mods),
            Tool::Join => self.join_tool_input(response, p),
            Tool::SegmentDelete => {
                if response.clicked_by(PointerButton::Primary) {
                    self.delete_virtual_segment_at(p);
                }
                if response.drag_started_by(PointerButton::Primary) {
                    // Freehand from the bar, or the other way round with
                    // Alt held at the press (#1150).
                    self.drag = if self.segment_freehand != mods.alt {
                        Drag::Freehand { points: vec![p] }
                    } else {
                        Drag::Box {
                            start: p,
                            current: p,
                            from_center: false,
                        }
                    };
                }
                if response.dragged_by(PointerButton::Primary) {
                    let step = 1.0 / self.view.zoom.max(1e-6) as f64;
                    match &mut self.drag {
                        Drag::Box { current, .. } => *current = p,
                        Drag::Freehand { points }
                            if points.last().is_none_or(|l| (*l - p).hypot() > step) =>
                        {
                            points.push(p);
                        }
                        _ => {}
                    }
                }
            }
            Tool::ShapeRecognition | Tool::Sketch => {
                if response.drag_started_by(PointerButton::Primary) {
                    self.drag = Drag::Freehand { points: vec![p] };
                    // A pen's pressure along the stroke (#883).
                    self.pen_stroke = crate::pen::PenStroke::start(self.pen_pressure);
                }
                if response.dragged_by(PointerButton::Primary) {
                    if let Drag::Freehand { points } = &mut self.drag {
                        if points
                            .last()
                            .map(|l| (*l - p).hypot() > 0.3)
                            .unwrap_or(true)
                        {
                            points.push(p);
                            let n = points.len();
                            self.pen_stroke.record(n, self.pen_pressure);
                        }
                    }
                }
            }
            Tool::HorizontalVerticalDimension | Tool::SegmentDimension | Tool::AngularDimension => {
                self.dimension_input(response, p)
            }
            Tool::Callout => self.callout_input(response, p),
            Tool::AnchorEditing => self.anchor_input(response, p),
            Tool::RightAngleConnector | Tool::RoundedConnector => {
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
            Tool::BlockShadow | Tool::InnerShadow => {
                // A drag from an object casts the shadow from the press
                // (the Inner Shadow tool as the Block Shadow one, #2473);
                // one from the square at the selected shadow's end moves
                // that end (#1280).
                if response.drag_started_by(PointerButton::Primary) {
                    // The press point, not where the drag was noticed.
                    let press = response
                        .ctx
                        .input(|i| i.pointer.press_origin())
                        .map(|o| self.view.to_page(o))
                        .unwrap_or(p);
                    if !self.begin_offset_shadow_handle(press) {
                        if let Some(id) = self.shadow_drag_target(press) {
                            // A press on one of several selected objects
                            // keeps them all: each takes the shadow (#2218).
                            if !self.selection.contains(&id) {
                                self.select(vec![id]);
                            }
                            self.drag = Drag::ContourDrag { start: press };
                        }
                    }
                }
                if response.dragged_by(PointerButton::Primary) {
                    let ctrl = response
                        .ctx
                        .input(|i| i.modifiers.ctrl || i.modifiers.command);
                    self.offset_shadow_drag_to(p, ctrl);
                }
            }
            Tool::Envelope => self.envelope_tool_input(response, p),
            Tool::Extrude => self.extrude_input(response, p),
            Tool::Distort => self.distort_input(response, p),
            Tool::AreaFill => {
                if response.clicked_by(PointerButton::Primary)
                    && !(self.area_fill_live && self.live_paint_at(p))
                {
                    self.area_fill_at(p);
                }
            }
            Tool::MeshFill => self.mesh_tool_input(response, p),
            Tool::OutlinePen => {
                // The tool opens the Outline Pen dialog and gives the
                // canvas back to the tool before it.
                let back = match self.previous_tool {
                    Tool::OutlinePen | Tool::OutlineColor => Tool::Pick,
                    t => t,
                };
                self.set_tool(back);
                self.open_outline_pen();
            }
            Tool::OutlineColor => {
                self.set_tool(Tool::Pick);
                self.show_dockers = true;
                self.docker_tab = crate::app::DockerTab::Color;
            }
            _ => {}
        }
    }

    fn free_transform_input(&mut self, response: &Response, p: Point, mods: Modifiers) {
        use crate::app::FreeTransformMode as M;
        // A drag that ended without its release reaching here (Esc, a
        // tool change) closes its undo group.
        if !matches!(self.drag, Drag::Rotate { .. }) {
            self.end_free_transform_group();
        }
        if response.drag_started_by(PointerButton::Primary) {
            if let Some(id) = self.hit_test(p) {
                if !self.selection.contains(&id) {
                    self.select(vec![id]);
                }
            }
            if !self.selection.is_empty() {
                // The whole drag is one undo step (#1342).
                self.end_free_transform_group();
                self.free_transform_group = Some(self.engine.begin_group());
                if self.free_transform_duplicate {
                    // Transform a copy, leave the original where it is.
                    let offset = self.duplicate_offset;
                    self.duplicate_offset = Vec2::ZERO;
                    self.duplicate();
                    self.duplicate_offset = offset;
                }
                self.drag = Drag::Rotate {
                    center: p,
                    start_angle: 0.0,
                    current_angle: 0.0,
                    grab: None,
                    start_radius: 0.0,
                    scale: 1.0,
                };
                self.free_transform_last = p;
                self.free_transform_reflected = false;
            }
        }
        if response.dragged_by(PointerButton::Primary) {
            if let Drag::Rotate { center, .. } = self.drag {
                let last = self.free_transform_last;
                self.free_transform_last = p;
                let mode = if mods.alt {
                    M::Scale
                } else if mods.ctrl {
                    M::Skew
                } else if mods.shift {
                    M::Reflection
                } else {
                    self.free_transform_mode
                };
                let first = !self.free_transform_reflected;
                if mode == M::Reflection {
                    self.free_transform_reflected = true;
                }
                let t = free_transform_step(mode, center, last, p, first);
                if t != Affine::IDENTITY {
                    self.transform_selection(t);
                    // The steps so far become one at once: the drag keeps
                    // one copy of the drawing from before it, not one a
                    // pointer move.
                    if let Some(mark) = self.free_transform_group {
                        self.engine.join_group(mark, "Free Transform");
                    }
                }
            }
        }
        if response.drag_stopped_by(PointerButton::Primary)
            && matches!(self.drag, Drag::Rotate { .. })
        {
            // Every step was applied as the pointer moved; the release
            // has nothing more to do than close the undo group.
            self.drag = Drag::None;
            self.end_free_transform_group();
        }
        if response.clicked_by(PointerButton::Primary) {
            match self.hit_test(p) {
                Some(id) => self.select(vec![id]),
                None => self.select(Vec::new()),
            }
        }
    }

    /// Close the undo group of a Free Transform drag, if one is open: its
    /// steps become one "Free Transform" step.
    pub fn end_free_transform_group(&mut self) {
        if let Some(mark) = self.free_transform_group.take() {
            self.engine.end_group(mark, "Free Transform");
        }
    }

    /// Segment Delete: remove the segment of a curve under the click.
    /// Shape Recognition: recognise the sketched stroke as a basic shape.
    /// Sketch: smooth the stroke and merge it with a selected curve
    /// whose end is near the stroke's start.
    pub fn finish_sketch(&mut self, points: Vec<Point>) {
        let sketch = self.sketch;
        let pts = tracedraw_core::geometry::simplify(&points, sketch.tolerance());
        if pts.len() < 2 {
            return;
        }
        let path = tracedraw_core::geometry::smooth_path(&pts, false);
        if let Some(sel) = self
            .selected_shapes()
            .into_iter()
            .filter(|_| sketch.join)
            .find(|s| matches!(s.kind, ShapeKind::Path { closed: false, .. }))
        {
            let existing = sel.page_path();
            let end = existing.elements().iter().rev().find_map(|el| match el {
                tracedraw_core::geometry::PathEl::LineTo(q)
                | tracedraw_core::geometry::PathEl::CurveTo(_, _, q) => Some(*q),
                _ => None,
            });
            if let Some(e) = end {
                if (e - pts[0]).hypot() < sketch.join_distance {
                    let mut joined = existing.clone();
                    for el in path.elements().iter().skip(1) {
                        joined.push(*el);
                    }
                    let local = sel.transform.inverse() * joined;
                    self.run(Command::SetShapeKind {
                        shape: sel.id,
                        kind: ShapeKind::Path {
                            path: local,
                            closed: false,
                        },
                    });
                    return;
                }
            }
        }
        // Drawn with a pen: the outline follows its pressure (#883).
        let widths = self.pen_widths(&points);
        if let Some(id) = self.new_shape_styled(
            ShapeKind::Path {
                path,
                closed: false,
            },
            |shape| {
                if let Some(st) = shape.stroke.as_mut().filter(|_| !widths.is_empty()) {
                    st.widths = widths;
                }
            },
        ) {
            self.select(vec![id]);
        }
    }

    /// The width nodes a line drawn along `points` gets from the pen's
    /// pressure, for the default outline's width; empty with the mouse
    /// or with Options > Tools > Curve tools' pen pressure off (#850,
    /// #883).
    pub fn pen_widths(&self, points: &[Point]) -> Vec<tracedraw_core::style::WidthNode> {
        self.pen_pressures(points)
            .and_then(|pr| crate::pen::PressureProfile::new(points, pr))
            .zip(self.default_stroke())
            .map(|(pen, st)| pen.freehand_widths(st.width))
            .unwrap_or_default()
    }

    /// Right-angle and rounded connectors: an elbow path between the
    /// facing sides of two objects.
    /// The right-angle connector tools' drop: a connector at right angles
    /// (`rounded`: with rounded corners) to the object under the drop.
    pub fn finish_elbow_connector(
        &mut self,
        from: ShapeId,
        start: Point,
        at: Point,
        rounded: bool,
    ) {
        use crate::connectors::ConnectorKind;
        if let Some(to) = self.hit_test_inside(at).filter(|id| *id != from) {
            let kind = if rounded {
                ConnectorKind::Rounded
            } else {
                ConnectorKind::RightAngle
            };
            self.connect(from, start, to, at, kind);
        }
    }

    /// The Area Fill bar's fill and outline on a new area: the default
    /// fill (a colour when the default is none, so the area shows), its
    /// own colour, or none; the default outline, its own, or none.
    fn area_fill_style(&self, s: &mut Shape) {
        s.fill = match self.area_fill_mode {
            1 => Fill::Solid(self.area_fill_color),
            2 => Fill::None,
            _ => match self.default_fill() {
                Fill::None => Fill::Solid(self.area_fill_color),
                other => other,
            },
        };
        s.stroke = match self.area_outline_mode {
            1 => Some(Stroke::new(
                self.area_outline_color,
                self.area_outline_width.max(0.0),
            )),
            2 => None,
            _ => self.default_stroke(),
        };
    }

    /// Area Fill > Fill All Areas (#4200): every area the outlines of the
    /// selected objects (all that show on the page when nothing is
    /// selected) close off becomes a filled object, on a new layer of
    /// their own over the others, in one undo step. The number made.
    pub fn area_fill_all(&mut self) -> usize {
        let shapes: Vec<Shape> = if self.selection.is_empty() {
            self.doc()
                .layers_for_page(self.page)
                .unwrap_or_default()
                .iter()
                .filter(|l| l.visible)
                .flat_map(|l| l.shapes.clone())
                .collect()
        } else {
            self.selection
                .iter()
                .filter_map(|id| self.doc().shape_on_page(*id))
                .collect()
        };
        let edges = self.area_fill_outline_edges;
        let (mut parts, mut bands) = (Vec::new(), Vec::new());
        for s in &shapes {
            area_fill_collect(s, Affine::IDENTITY, edges, &mut parts, &mut bands);
        }
        use tracedraw_core::shaping::{self, FillRule, Op};
        let mut areas = shaping::all_enclosed_visible_areas(&parts, 5000);
        // With outline edges, the inside of an outline's band is no area.
        if !bands.is_empty() {
            areas.retain(|a| {
                let whole = shaping::filled_area(a, FillRule::NonZero);
                let covered: f64 = bands
                    .iter()
                    .map(|b| {
                        shaping::filled_area(&shaping::overlay(a, b, Op::Intersect), FillRule::NonZero)
                    })
                    .sum();
                covered < whole * 0.5
            });
        }
        if areas.is_empty() {
            self.status = tr("status.area_fill_none");
            return 0;
        }
        let page = self.page;
        let mark = self.engine.begin_group();
        let made = (|| {
            self.engine
                .run(&Command::AddLayer {
                    page,
                    name: tr("area_fill.layer_name"),
                })
                .ok()?;
            let layer = self.doc().page(page).ok()?.layers.last()?.id;
            let mut cmds = Vec::with_capacity(areas.len());
            let mut ids = Vec::with_capacity(areas.len());
            for path in &areas {
                let id = self.engine.new_shape_id();
                let mut s = Shape::new(
                    id,
                    ShapeKind::Path {
                        path: path.clone(),
                        closed: true,
                    },
                );
                self.area_fill_style(&mut s);
                s.name = Some("Area Fill".into());
                cmds.push(Command::AddShape { layer, shape: s });
                ids.push(id);
            }
            self.engine.run_batch("Fill All Areas", &cmds).ok()?;
            Some(ids)
        })();
        self.engine.end_group(mark, "Fill All Areas");
        match made {
            Some(ids) => {
                let n = ids.len();
                self.select(ids);
                n
            }
            None => 0,
        }
    }

    /// Area Fill: find the enclosed area under the click (the outlines
    /// that show count as walls: a line hidden under an object's opaque
    /// fill does not cut the area, #1281) and create a filled object from
    /// it. The area can be anywhere, on the page or off it on the desktop,
    /// and a drawing of any size (#499). With the bar's "Up to outline
    /// edges" on, each outline counts as wide as it is drawn: the area
    /// stops at its inner edge, and lines whose middles miss each other
    /// but whose widths overlap still close it (#3321).
    pub fn area_fill_at(&mut self, p: Point) {
        let shapes: Vec<Shape> = self
            .doc()
            .layers_for_page(self.page)
            .unwrap_or_default()
            .iter()
            .filter(|l| l.visible)
            .flat_map(|l| l.shapes.clone())
            .collect();
        let edges = self.area_fill_outline_edges;
        let (mut parts, mut bands) = (Vec::new(), Vec::new());
        for s in &shapes {
            area_fill_collect(s, Affine::IDENTITY, edges, &mut parts, &mut bands);
        }
        // A click on a drawn outline fills nothing, as on a thin one.
        if edges && bands.iter().any(|b| kurbo::Shape::winding(b, p) != 0) {
            return;
        }
        // The area the outlines close off around the click, its sides
        // their own curves (#734); a click in an open area or on an
        // outline makes nothing.
        let path = match tracedraw_core::shaping::enclosed_visible_area(&parts, p) {
            tracedraw_core::shaping::Enclosed::Area(path) => path,
            tracedraw_core::shaping::Enclosed::Open => {
                self.status = tr("status.area_fill_open");
                return;
            }
            tracedraw_core::shaping::Enclosed::OnOutline => return,
        };
        // With Use Guidelines on, the guidelines close the area off too:
        // only the part on the click's side of each line is kept (#3863).
        let path = if self.area_fill_guides {
            let lines = self.shown_guide_lines();
            if lines.is_empty() {
                path
            } else {
                let bounds = kurbo::Shape::bounding_box(&path).inflate(10.0, 10.0);
                let Some(cell) = crate::shape_builder::cell_at(bounds, &lines, p) else {
                    return;
                };
                match crate::shape_builder::part_in_cell(&path, &cell, p) {
                    Some(part) => part,
                    None => return,
                }
            }
        } else {
            path
        };
        if path.elements().is_empty() {
            return;
        }
        let Some(layer) = self.active_layer() else {
            return;
        };
        // Inside a symmetry the area goes on the original, mirrored back
        // from the copy clicked, and joins the symmetry: its mirrors fill
        // the same area in every copy (#2998).
        let symmetry = self.area_fill_symmetry(&shapes, p);
        let path = match symmetry {
            Some((_, back)) => back * path,
            None => path,
        };
        let id = self.engine.new_shape_id();
        let mut s = Shape::new(id, ShapeKind::Path { path, closed: true });
        self.area_fill_style(&mut s);
        s.name = Some("Area Fill".into());
        self.run(Command::AddShape { layer, shape: s });
        self.select(vec![id]);
        if let Some((sym, _)) = symmetry {
            let editing = self.symmetry_edit;
            self.symmetry_edit = Some(sym);
            self.join_symmetry(&[id]);
            if editing.is_none() {
                // Not being edited: the symmetry is selected as a whole.
                if let Some(g) = self.symmetry_edit.take() {
                    self.select(vec![g]);
                }
            }
        }
    }

    /// Smart Fill with Live on: paint the area the lines of a group close
    /// off around `p` as a live fill of the group (the selected group, or
    /// else the topmost one whose bounds hold `p`), in the bar's fill
    /// colour; with the fill set to none, take the fill there away.
    /// Whether a group took the click: when none does, the click makes an
    /// ordinary area as without Live.
    pub fn live_paint_at(&mut self, p: Point) -> bool {
        let is_group = |s: &Shape| matches!(s.kind, ShapeKind::Group { .. });
        let selected = match self.selection[..] {
            [id] => self.doc().find_shape(id).filter(|s| is_group(s)).cloned(),
            _ => None,
        };
        let group = selected.or_else(|| {
            self.doc()
                .layers_for_page(self.page)
                .unwrap_or_default()
                .iter()
                .rev()
                .filter(|l| l.visible && !l.locked)
                .flat_map(|l| l.shapes.iter().rev())
                .find(|s| is_group(s) && kurbo::Shape::bounding_box(&s.page_path()).contains(p))
                .cloned()
        });
        let (group, grouped) = match group {
            Some(g) => (g, None),
            None => match self.live_group_around(p) {
                Some((g, mark)) => (g, Some(mark)),
                None => return false,
            },
        };
        let color = match self.area_fill_mode {
            1 => Some(self.area_fill_color),
            2 => None,
            _ => match self.default_fill() {
                Fill::Solid(c) => Some(c),
                _ => Some(self.area_fill_color),
            },
        };
        let mut effects = group.effects.clone();
        let mut fills = effects
            .iter()
            .find_map(|e| match e {
                Effect::LivePaint { fills } => Some(fills.clone()),
                _ => None,
            })
            .unwrap_or_default();
        let parent = self
            .doc()
            .parent_transform(group.id)
            .unwrap_or(Affine::IDENTITY);
        let local = (parent * group.transform).inverse() * p;
        if !tracedraw_core::live_paint::paint_at(&group, &mut fills, local, color) {
            if let Some(mark) = grouped {
                // Nothing painted: the objects are not left grouped.
                self.engine.end_group(mark, "Area Fill");
                self.undo();
            }
            return tracedraw_core::live_paint::region_at(&group, local).is_some();
        }
        match effects
            .iter_mut()
            .find(|e| matches!(e, Effect::LivePaint { .. }))
        {
            Some(e) => *e = Effect::LivePaint { fills },
            None => effects.insert(0, Effect::LivePaint { fills }),
        }
        effects.retain(|e| !matches!(e, Effect::LivePaint { fills } if fills.is_empty()));
        self.run(Command::SetEffects {
            shape: group.id,
            effects,
        });
        if let Some(mark) = grouped {
            self.engine.end_group(mark, "Area Fill");
        }
        self.select(vec![group.id]);
        true
    }

    /// Live Area Fill on lines not grouped (#4199): the objects of the
    /// active layer whose lines close round `p` are grouped, so the live
    /// fill follows them as they are edited. One Undo takes back the
    /// grouping with the fill. None when no area closes round `p`.
    fn live_group_around(&mut self, p: Point) -> Option<(Shape, Option<u64>)> {
        let layer = self.active_layer()?;
        let shapes: Vec<Shape> = self
            .doc()
            .layers_for_page(self.page)
            .unwrap_or_default()
            .iter()
            .filter(|l| l.id == layer && l.visible && !l.locked)
            .flat_map(|l| l.shapes.iter())
            .filter(|s| !matches!(s.kind, ShapeKind::Group { .. }) && s.effects.is_empty())
            .cloned()
            .collect();
        if shapes.len() < 2 {
            return None;
        }
        let all = Shape::new(ShapeId(u64::MAX), ShapeKind::Group { children: shapes.clone() });
        let area = tracedraw_core::live_paint::region_at(&all, p)?;
        let around = kurbo::Shape::bounding_box(&area).inflate(0.5, 0.5);
        let members: Vec<ShapeId> = shapes
            .iter()
            .filter(|s| {
                let b = kurbo::Shape::bounding_box(&s.page_path());
                b.x0 <= around.x1 && b.x1 >= around.x0 && b.y0 <= around.y1 && b.y1 >= around.y0
            })
            .map(|s| s.id)
            .collect();
        if members.len() < 2 {
            return None;
        }
        let mark = self.engine.begin_group();
        self.select(members);
        self.group_selection();
        let group = match self.selection[..] {
            [id] => self
                .doc()
                .find_shape(id)
                .filter(|s| matches!(s.kind, ShapeKind::Group { .. }))
                .cloned(),
            _ => None,
        };
        if group.is_none() {
            self.engine.end_group(mark, "Area Fill");
        }
        // The fill joins this step (`live_paint_at` ends the group).
        group.map(|g| (g, mark))
    }

    /// The symmetry object an Area Fill click at `p` falls in, among the
    /// page's top-level `shapes` (the one being edited first), and the
    /// transformation taking the click's copy back to the original's side
    /// (#2998).
    fn area_fill_symmetry(&self, shapes: &[Shape], p: Point) -> Option<(ShapeId, Affine)> {
        let sym_of = |s: &Shape| {
            s.effects.iter().find_map(|e| match e {
                Effect::Symmetry {
                    center,
                    angle,
                    lines,
                    rotate,
                } => Some((*center, *angle, *lines, *rotate)),
                _ => None,
            })
        };
        let mut order: Vec<&Shape> = shapes.iter().filter(|s| sym_of(s).is_some()).collect();
        // The topmost first, the edited one before all.
        order.reverse();
        if let Some(edit) = self.symmetry_edit {
            order.sort_by_key(|s| s.id != edit);
        }
        for s in order {
            let (center, angle, lines, rotate) = sym_of(s)?;
            let ev = tracedraw_core::live::evaluate(s);
            let mut whole = ev.main.bounds();
            for part in ev.above.iter().chain(&ev.below) {
                whole = whole.union(part.bounds());
            }
            if self.symmetry_edit != Some(s.id) && !whole.contains(p) {
                continue;
            }
            let origin = ev.main.bounds().center();
            let back = tracedraw_core::live::symmetry_back_to_turned(
                center, angle, lines, rotate, origin, p,
            );
            return Some((s.id, back));
        }
        None
    }

    /// Effect nodes (perspective, mesh) under the pointer, for the Shape
    /// and Mesh Fill tools: (shape, node index). The nearest within the
    /// pick distance (it grows with the node size, #1935) of the nodes and
    /// of the handles of the selected mesh node (they lie along its grid
    /// lines); a node wins over a handle as near.
    pub fn effect_node_at(&self, p: Point) -> Option<(ShapeId, usize)> {
        let tol = self.settings.nodes.pick_px() / self.view.zoom.max(1e-6) as f64;
        let nearest = |points: Vec<((ShapeId, usize), Point)>| {
            points
                .into_iter()
                .map(|(key, q)| (key, (q - p).hypot()))
                .filter(|(_, d)| *d <= tol)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(key, _)| key)
        };
        let mut points: Vec<((ShapeId, usize), Point)> = self
            .selected_shapes()
            .iter()
            .flat_map(|s| {
                effect_nodes(s)
                    .into_iter()
                    .map(move |(i, q)| ((s.id, i), q))
            })
            .collect();
        // The Envelope tool takes only a mesh envelope's grid points.
        let envelope_tool = self.tool == Tool::Envelope;
        if envelope_tool {
            points.retain(|((_, i), _)| is_mesh_envelope_node(*i));
        }
        // After the nodes: a node and a handle as near win the node.
        if let Some((id, i)) = self.selected_mesh_node().filter(|_| !envelope_tool) {
            if let Some(s) = self.doc().find_shape(id) {
                points.extend(
                    mesh_handle_points(s, i)
                        .into_iter()
                        .map(|(k, q)| ((id, k), q)),
                );
            }
        }
        // The chosen colour point's spread handle, on its ring (#3947).
        if let Some((id, i)) = self.selected_color_point().filter(|_| !envelope_tool) {
            if let Some((_, _, h)) = self
                .doc()
                .find_shape(id)
                .and_then(|s| crate::color_points::spread_ring(s, i))
            {
                points.push(((id, crate::color_points::SPREAD_HANDLE_BASE + i), h));
            }
        }
        nearest(points)
    }

    /// Drag an effect node; returns true when handled. A drag takes the
    /// node (or the selected mesh node's handle) under the press, not
    /// under the pointer where the drag is noticed a few pixels on, and
    /// the node follows the pointer, snapping where it lands (#2004,
    /// discussion #2073).
    pub fn effect_node_input(&mut self, response: &Response, p: Point) -> bool {
        // Checked before the click, which the second click of a
        // double-click also is.
        if response.double_clicked() {
            // Double-click on a mesh node: the grid lines through it go,
            // as with Delete; a row or column could
            // not be taken out with the mouse (d#2889).
            let on_node = self.effect_node_at(p).filter(|h| {
                !crate::mesh_tool::is_mesh_handle(h.1)
                    && crate::mesh_tool::mesh_node_of(h.1).is_some()
            });
            if let Some(hit) = on_node {
                self.selected_effect_node = Some(hit);
                self.mesh_more_nodes = None;
                self.delete_mesh_lines();
                return true;
            }
        }
        if response.clicked_by(PointerButton::Primary) {
            if let Some(hit) = self.effect_node_at(p) {
                // A handle keeps its node selected. Shift+click on a
                // mesh node adds it to (or takes it out of) the chosen
                // nodes (discussion #2387).
                if !crate::mesh_tool::is_mesh_handle(hit.1) {
                    let shift = response.ctx.input(|i| i.modifiers.shift);
                    match crate::mesh_tool::mesh_node_of(hit.1) {
                        Some(k) if shift => self.toggle_mesh_node(hit.0, k),
                        _ => {
                            self.selected_effect_node = Some(hit);
                            self.mesh_more_nodes = None;
                        }
                    }
                }
                return true;
            }
        }
        if response.double_clicked() {
            // Double-click inside a mesh: a node there, on new grid lines
            // through the point (#804).
            for s in self.selected_shapes() {
                if let Fill::Mesh(m) = &s.fill {
                    let inv = s.transform.inverse();
                    let lp = inv * p;
                    // Four pixels on screen, in the mesh's own space.
                    let tol = ((inv * Point::new(4.0 / self.view.zoom as f64, 0.0))
                        - (inv * Point::ZERO))
                        .hypot();
                    let mut m2 = m.clone();
                    if m2.add_lines_at(lp, tol) {
                        let node = mesh_node_near(&m2, lp, tol);
                        self.run(Command::SetFill {
                            shapes: vec![s.id],
                            fill: Fill::Mesh(m2),
                        });
                        if let Some(i) = node {
                            self.selected_effect_node =
                                Some((s.id, crate::mesh_tool::MESH_NODE_BASE + i));
                        }
                    }
                    return true;
                }
            }
            // Double-click inside an object with colour points: a point
            // there (on the Shape tool, not on its outline, where a
            // double click adds a node).
            let on_line = self.tool == Tool::Shape && self.segment_at(p).is_some();
            if !on_line && self.add_color_point(p) {
                return true;
            }
        }
        if response.drag_started_by(PointerButton::Primary) {
            let press = response
                .ctx
                .input(|i| i.pointer.press_origin())
                .map(|o| self.view.to_page(o))
                .unwrap_or(p);
            let hit = self
                .effect_node_at(press)
                .map(|h| (h, press))
                .or_else(|| self.effect_node_at(p).map(|h| (h, p)));
            if let Some(((id, index), from)) = hit {
                let at = self
                    .doc()
                    .find_shape(id)
                    .and_then(|s| effect_point(s, index))
                    .unwrap_or(from);
                self.effect_node_grab = at - from;
                self.effect_node_drag = Some((id, index));
                if !crate::mesh_tool::is_mesh_handle(index) {
                    // A drag on one of several chosen mesh nodes moves
                    // them all; on another node, that node alone.
                    let group = crate::mesh_tool::mesh_node_of(index).and_then(|k| {
                        self.selected_mesh_nodes()
                            .filter(|(sid, nodes)| *sid == id && nodes.contains(&k))
                            .map(|(_, nodes)| (k, nodes))
                    });
                    match group {
                        Some((k, mut nodes)) => {
                            nodes.retain(|&n| n != k);
                            nodes.insert(0, k);
                            self.choose_mesh_nodes(id, &nodes);
                        }
                        None => {
                            self.selected_effect_node = Some((id, index));
                            self.mesh_more_nodes = None;
                        }
                    }
                }
                self.effect_node_stepped = false;
                self.drag = Drag::Node { last: from };
                // The node catches up with the pointer now.
                self.effect_node_drag_to(p);
                return true;
            }
        }
        if self.effect_node_drag.is_none() {
            return false;
        }
        if response.dragged_by(PointerButton::Primary) {
            self.effect_node_drag_to(p);
            return true;
        }
        if response.drag_stopped() {
            self.effect_node_drag = None;
            self.drag = Drag::None;
            return true;
        }
        false
    }

    /// The pointer of an effect node drag is at `p`: the node (or handle)
    /// goes to it, less the offset it was grabbed at, snapped; one undo
    /// step for the whole drag.
    pub fn effect_node_drag_to(&mut self, p: Point) {
        let Some((id, index)) = self.effect_node_drag else {
            return;
        };
        let Some(s) = self.doc().find_shape(id).cloned() else {
            return;
        };
        // A colour point's spread handle: the spread follows the
        // pointer's distance from the point, one undo step (#3947).
        if let Some(i) = crate::color_points::spread_handle_of(index) {
            if let Some((at, _, _)) = crate::color_points::spread_ring(&s, i) {
                use tracedraw_core::geometry::Shape as _;
                let b = s.page_path().bounding_box();
                let unit = crate::color_points::spread_radius(b, 1.0).max(1e-9);
                let spread = ((p + self.effect_node_grab) - at).hypot() / unit;
                if let Some(cmd) = self.spread_color_point(id, i, spread) {
                    if self.effect_node_stepped && self.engine.undo_label() == Some("Edit Effect")
                    {
                        let _ = self.engine.undo();
                    }
                    self.effect_node_stepped = true;
                    if let Err(e) = self.engine.run_batch("Edit Effect", &[cmd]) {
                        self.status = e.to_string();
                    }
                }
            }
            self.drag = Drag::Node { last: p };
            return;
        }
        let Some(at) = effect_point(&s, index) else {
            return;
        };
        let to = self.snap_point(p + self.effect_node_grab);
        let delta = to - at;
        // A colour point goes where it is dragged, one undo step too.
        let point = crate::mesh_tool::mesh_node_of(index)
            .filter(|_| !crate::color_points::point_nodes(&s).is_empty());
        if let Some(i) = point {
            if delta.hypot() > 0.0 {
                if let Some(cmd) = self.move_color_point(id, i, to) {
                    if self.effect_node_stepped && self.engine.undo_label() == Some("Edit Effect")
                    {
                        let _ = self.engine.undo();
                    }
                    self.effect_node_stepped = true;
                    if let Err(e) = self.engine.run_batch("Edit Effect", &[cmd]) {
                        self.status = e.to_string();
                    }
                }
            }
            self.drag = Drag::Node { last: p };
            return;
        }
        if delta.hypot() > 0.0 {
            let inv = s.transform.inverse();
            let ld = (inv * (Point::ZERO + delta)) - (inv * Point::ZERO);
            let mut effects = s.effects.clone();
            let mut fill = s.fill.clone();
            move_effect_node(&mut effects, &mut fill, index, ld);
            // The other chosen mesh nodes go along (discussion #2387).
            if let Some(k) = crate::mesh_tool::mesh_node_of(index) {
                if let Some((sid, nodes)) = self.selected_mesh_nodes() {
                    if sid == id && nodes.first() == Some(&k) {
                        for &n in &nodes[1..] {
                            move_effect_node(
                                &mut effects,
                                &mut fill,
                                crate::mesh_tool::MESH_NODE_BASE + n,
                                ld,
                            );
                        }
                    }
                }
            }
            let mut cmds = Vec::new();
            if effects != s.effects {
                cmds.push(Command::SetEffects { shape: id, effects });
            }
            if fill != s.fill {
                cmds.push(Command::SetFill {
                    shapes: vec![id],
                    fill,
                });
            }
            if !cmds.is_empty() {
                // One undo step for the whole drag.
                if self.effect_node_stepped && self.engine.undo_label() == Some("Edit Effect") {
                    let _ = self.engine.undo();
                }
                self.effect_node_stepped = true;
                if let Err(e) = self.engine.run_batch("Edit Effect", &cmds) {
                    self.status = e.to_string();
                }
            }
        }
        self.drag = Drag::Node { last: p };
    }

    /// Right-click: remember where, the UI draws the context menu.
    pub fn open_context_menu(&mut self, screen: egui::Pos2, p: Point) {
        self.context_locked = None;
        if self.tool == Tool::Pick || self.tool == Tool::FreeformPick {
            if let Some(id) = self.locked_at(p) {
                // A locked object on top: its own menu, to unlock it.
                self.context_locked = Some(id);
            } else if self.selection_under(p) {
                // A right click on a selected object keeps the selection,
                // also when another object covers it there (one picked
                // with Alt+click or a marquee): the menu is for what is
                // selected, not for the object in front (#1684).
            } else if let Some(id) = self.hit_test(p) {
                if !self.selection.contains(&id) {
                    self.select(vec![id]);
                }
            } else if let Some(i) = self.pick_guide_at(p) {
                // A guideline under the pointer gets its own menu.
                if !self.selected_guides.contains(&i) {
                    self.select(Vec::new());
                    self.selected_guides = vec![i];
                }
            }
        }
        if self.tool == Tool::Shape {
            // A right click on a node takes it (unless it is chosen
            // already); on a segment, the node it ends at, so that the
            // menu's To Line and To Curve act on that segment; on another
            // object, that object.
            if let Some(hit) = self.node_at(p) {
                let key = (hit.shape, hit.index);
                if !self.node_selection.contains(&key) {
                    self.node_selection = vec![key];
                }
            } else if let Some((id, i, _)) = self.segment_at(p) {
                self.node_selection = vec![(id, i)];
            } else if let Some(id) = self.hit_test(p) {
                if !self.selection.contains(&id) {
                    self.select(vec![id]);
                }
            }
        }
        self.context_menu = Some((screen, p));
    }
}

/// The node of `m` within `tol` of `p` (both in the mesh's space).
fn mesh_node_near(m: &tracedraw_core::Mesh, p: Point, tol: f64) -> Option<usize> {
    m.nearest(p).filter(|i| {
        m.nodes
            .get(*i)
            .is_some_and(|n| (n.pos - p).hypot() <= tol.max(1e-6))
    })
}

/// The grid points of a mesh envelope count from here in the effect node
/// numbers (at most 81 of them, below the mesh fill's 200).
pub const MESH_ENVELOPE_BASE: usize = 110;

/// The effect node number is a mesh envelope's grid point.
pub fn is_mesh_envelope_node(index: usize) -> bool {
    (MESH_ENVELOPE_BASE..crate::mesh_tool::MESH_NODE_BASE).contains(&index)
}

/// Page-space positions of the editable nodes of a shape's perspective,
/// mesh envelope, blur pins and mesh fill, with a global index: perspective
/// 100..104, mesh envelope 110 + k (its last one), blur pins
/// [`BLUR_PIN_BASE`] + k, mesh 200 + k (frame envelopes have their own
/// editing, see `envelope_edit`).
pub fn effect_nodes(s: &Shape) -> Vec<(usize, Point)> {
    let mut out = Vec::new();
    for e in &s.effects {
        if let Effect::Perspective { corners } = e {
            for (i, c) in corners.iter().enumerate() {
                out.push((100 + i, s.transform * *c));
            }
        }
    }
    if let Some(Effect::MeshEnvelope { points, .. }) = s
        .effects
        .iter()
        .rev()
        .find(|e| matches!(e, Effect::MeshEnvelope { .. }))
    {
        for (k, q) in points.iter().enumerate() {
            out.push((MESH_ENVELOPE_BASE + k, s.transform * *q));
        }
    }
    // The pins of a variable Gaussian blur, BLUR_PIN_BASE + k.
    for e in &s.effects {
        if let Effect::GaussianBlur { pins, .. } = e {
            for (k, pin) in pins.iter().take(BLUR_PIN_COUNT).enumerate() {
                out.push((BLUR_PIN_BASE + k, s.transform * pin.at));
            }
        }
    }
    if let Fill::Mesh(m) = &s.fill {
        for (k, n) in m.nodes.iter().enumerate() {
            out.push((200 + k, s.transform * n.pos));
        }
    }
    // A colour points gradient's points, numbered as mesh nodes are.
    out.extend(crate::color_points::point_nodes(s));
    out
}

/// The effect node number of a variable blur's first pin; its pins
/// count on from it, up to [`BLUR_PIN_COUNT`] of them (past the mesh
/// envelope's and the mesh nodes' numbers, below the mesh handles').
pub const BLUR_PIN_BASE: usize = 900_000;
pub const BLUR_PIN_COUNT: usize = tracedraw_core::live::MAX_BLUR_PINS;

/// The blur pin an effect node number stands for.
pub fn blur_pin_of(index: usize) -> Option<usize> {
    index
        .checked_sub(BLUR_PIN_BASE)
        .filter(|k| *k < BLUR_PIN_COUNT)
}

/// The handles of mesh node `i` of `s` on the page, with their effect
/// node numbers (see [`crate::mesh_tool::mesh_handle_index`]): one on
/// each grid line that leaves the node, the mesh's edge lines included.
pub fn mesh_handle_points(s: &Shape, i: usize) -> Vec<(usize, Point)> {
    use tracedraw_core::MeshHandle;
    let Fill::Mesh(m) = &s.fill else {
        return Vec::new();
    };
    [
        MeshHandle::Right,
        MeshHandle::Left,
        MeshHandle::Down,
        MeshHandle::Up,
    ]
    .into_iter()
    .filter_map(|h| {
        m.handle_point(i, h)
            .map(|q| (crate::mesh_tool::mesh_handle_index(i, h), s.transform * q))
    })
    .collect()
}

/// Where effect node `index` of `s` is on the page: a perspective
/// corner, a mesh node or a mesh node's handle.
pub fn effect_point(s: &Shape, index: usize) -> Option<Point> {
    if let Some(i) = crate::color_points::spread_handle_of(index) {
        return crate::color_points::spread_ring(s, i).map(|(_, _, h)| h);
    }
    if let Some((i, h)) = crate::mesh_tool::mesh_handle_of(index) {
        let Fill::Mesh(m) = &s.fill else {
            return None;
        };
        return m.handle_point(i, h).map(|q| s.transform * q);
    }
    effect_nodes(s)
        .into_iter()
        .find(|(k, _)| *k == index)
        .map(|(_, q)| q)
}

fn move_effect_node(effects: &mut [Effect], fill: &mut Fill, index: usize, d: Vec2) {
    if let Some((i, h)) = crate::mesh_tool::mesh_handle_of(index) {
        if let Fill::Mesh(m) = fill {
            m.move_handle(i, h, d);
        }
        return;
    }
    if is_mesh_envelope_node(index) {
        if let Some(Effect::MeshEnvelope { points, .. }) = effects
            .iter_mut()
            .rev()
            .find(|e| matches!(e, Effect::MeshEnvelope { .. }))
        {
            if let Some(q) = points.get_mut(index - MESH_ENVELOPE_BASE) {
                *q += d;
            }
        }
        return;
    }
    for e in effects.iter_mut() {
        match e {
            Effect::Perspective { corners } => {
                if let Some(c) = index
                    .checked_sub(100)
                    .filter(|i| *i < 4)
                    .and_then(|i| corners.get_mut(i))
                {
                    *c += d;
                }
            }
            Effect::GaussianBlur { pins, .. } => {
                if let Some(pin) = blur_pin_of(index).and_then(|k| pins.get_mut(k)) {
                    pin.at += d;
                }
            }
            _ => {}
        }
    }
    if let Fill::Mesh(m) = fill {
        if index >= 200 {
            if let Some(n) = m.nodes.get_mut(index - 200) {
                n.translate(d);
            }
        }
    }
}

pub fn _unused(_: Rect) {}

/// One drag step of the Free Transform tool: the affine that takes the
/// selection from the pointer at `last` to the pointer at `p`, about the
/// press point `center`. `first` is true for the first step of a drag.
pub fn free_transform_step(
    mode: crate::app::FreeTransformMode,
    center: Point,
    last: Point,
    p: Point,
    first: bool,
) -> Affine {
    use crate::app::FreeTransformMode as M;
    let about =
        |t: Affine| Affine::translate(center.to_vec2()) * t * Affine::translate(-center.to_vec2());
    match mode {
        M::Scale => {
            // Scale about the press point by the ratio of distances. The
            // first move away from the press point has no distance to
            // scale from: it sets it.
            let d0 = (last - center).hypot();
            let d1 = (p - center).hypot();
            if d0 < 1e-3 || d1 < 1e-3 {
                return Affine::IDENTITY;
            }
            about(Affine::scale(d1 / d0))
        }
        M::Skew => {
            // Horizontal motion skews along x, vertical along y. One shear
            // after the other, so the step keeps the area and never turns
            // the object over: a single skew by both amounts mirrors it
            // once their product passes 1, as a quick diagonal stroke did
            // to text (#1342).
            let d = p - last;
            about(Affine::skew(d.x / 50.0, 0.0) * Affine::skew(0.0, d.y / 50.0))
        }
        M::Reflection => {
            // Mirror across the line from the press point through the
            // pointer. The first step reflects; later steps turn the mirror
            // line, which is a rotation by twice the angle change.
            let a1 = (p - center).atan2();
            if first {
                let (sn, cs) = (2.0 * a1).sin_cos();
                about(Affine::new([cs, sn, sn, -cs, 0.0, 0.0]))
            } else {
                let a0 = (last - center).atan2();
                Affine::rotate_about(2.0 * (a1 - a0), center)
            }
        }
        M::Rotation => {
            let a0 = (last - center).atan2();
            let a1 = (p - center).atan2();
            Affine::rotate_about(a1 - a0, center)
        }
    }
}

/// Area Fill's walls, bottom to top: each outline of `s` (under
/// `parent`) and whether its fill hides what lies beneath it; with
/// `edges`, each outline's band too, kept in `bands`.
pub(crate) fn area_fill_collect(
    s: &Shape,
    parent: Affine,
    edges: bool,
    out: &mut Vec<(BezPath, bool)>,
    bands: &mut Vec<BezPath>,
) {
    let t = parent * s.transform;
    if !s.effects.is_empty() {
        // What the object draws, in drawing order: the objects its
        // effects add below it, its warped outline, those above.
        let mut on_page = s.clone();
        on_page.transform_by(parent);
        let ev = tracedraw_core::live::evaluate(&on_page);
        let see_through = s
            .effects
            .iter()
            .any(|e| matches!(e, Effect::Transparency { .. } | Effect::Lens(_)));
        for part in ev.below {
            let covers = hides_below(&part);
            out.push((part.page_path(), covers));
        }
        let covers = !see_through && hides_below(&ev.main);
        out.push((ev.main.page_path(), covers));
        for part in ev.above {
            let covers = hides_below(&part);
            out.push((part.page_path(), covers));
        }
        return;
    }
    match &s.kind {
        ShapeKind::Group { children } => children
            .iter()
            .for_each(|c| area_fill_collect(c, t, edges, out, bands)),
        _ => {
            out.push((t * s.local_path(), hides_below(s)));
            // The band the outline draws, as an opaque object: its
            // edges are the walls and the middle line under it is
            // cut away.
            if edges && s.stroke.as_ref().is_some_and(|st| st.width > 0.0) {
                let mut on_page = s.clone();
                on_page.transform_by(parent);
                if let Some(band) = crate::tools2::outline_object_path(&on_page) {
                    bands.push(band.clone());
                    out.push((band, true));
                }
            }
        }
    }
}


/// The object's fill hides what lies beneath it: an opaque fill at full
/// opacity, with no transparency mask (Area Fill, #1281).
fn hides_below(s: &Shape) -> bool {
    if s.opacity < 1.0 || s.opacity_mask.is_some() {
        return false;
    }
    match &s.fill {
        Fill::None => false,
        Fill::Solid(_) | Fill::Pattern(_) | Fill::Texture(_) => true,
        Fill::Fountain(f) => f.stops.iter().all(|st| st.opacity >= 1.0),
        Fill::Mesh(m) => m.nodes.iter().all(|n| n.alpha >= 1.0),
        // Without a background what is beneath shows between the paths.
        Fill::Procedural(p) => p.background.is_some(),
    }
}

#[cfg(test)]
mod free_transform_tests {
    use super::free_transform_step;
    use crate::app::FreeTransformMode as M;
    use tracedraw_core::geometry::Point;

    fn close(a: Point, b: Point) -> bool {
        (a - b).hypot() < 1e-6
    }

    #[test]
    fn reflection_mirrors_across_the_pointer_line_and_follows_it() {
        let c = Point::new(10.0, 10.0);
        // Mirror line along x: (20, 15) lands on (20, 5).
        let t = free_transform_step(M::Reflection, c, c, Point::new(30.0, 10.0), true);
        assert!(close(t * Point::new(20.0, 15.0), Point::new(20.0, 5.0)));
        // Turning the line by 45 degrees rotates the already mirrored
        // object by 90 degrees: (20, 5) goes to (15, 20).
        let t2 = free_transform_step(
            M::Reflection,
            c,
            Point::new(30.0, 10.0),
            Point::new(30.0, 30.0),
            false,
        );
        assert!(close(t2 * Point::new(20.0, 5.0), Point::new(15.0, 20.0)));
        // The composite equals a direct reflection across the 45 degree line.
        let direct = free_transform_step(M::Reflection, c, c, Point::new(30.0, 30.0), true);
        assert!(close(
            (t2 * t) * Point::new(20.0, 15.0),
            direct * Point::new(20.0, 15.0)
        ));
    }

    #[test]
    fn scale_rotation_and_skew_keep_the_press_point_fixed() {
        let c = Point::new(5.0, 5.0);
        for m in [M::Scale, M::Rotation, M::Skew] {
            let t = free_transform_step(m, c, Point::new(15.0, 5.0), Point::new(15.0, 15.0), true);
            assert!(close(t * c, c), "{m:?}");
        }
        let t = free_transform_step(
            M::Scale,
            c,
            Point::new(15.0, 5.0),
            Point::new(25.0, 5.0),
            true,
        );
        assert!(close(t * Point::new(15.0, 5.0), Point::new(25.0, 5.0)));
        let t = free_transform_step(
            M::Rotation,
            c,
            Point::new(15.0, 5.0),
            Point::new(5.0, 15.0),
            true,
        );
        assert!(close(t * Point::new(15.0, 5.0), Point::new(5.0, 15.0)));
        // The first move of a Free Scale starts at the press point: no
        // distance to scale from, so nothing happens (#1342).
        let t = free_transform_step(M::Scale, c, c, Point::new(6.0, 5.0), true);
        assert_eq!(t, tracedraw_core::Affine::IDENTITY);
    }
}
