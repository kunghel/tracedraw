//! The Shape tool: select nodes, drag nodes and handles, add and delete
//! nodes, change node and segment types. Edits go through
//! `Command::SetShapeKind`, collapsed into one undo step per drag.

use crate::app::{App, Drag};
use crate::i18n::tr;
use egui::{Modifiers, PointerButton, Response};
use tracedraw_core::{
    document::{Shape, ShapeKind},
    geometry::{BezPath, PathEl, Point, Rect, Vec2},
    nodes::{self, NodeType, Which},
    Command, ShapeId,
};

/// Keep a move horizontal (`Some(true)`) or vertical (`Some(false)`);
/// `None` leaves it free.
fn constrain_axis(d: &mut Vec2, horizontal: Option<bool>) {
    match horizontal {
        Some(true) => d.y = 0.0,
        Some(false) => d.x = 0.0,
        None => {}
    }
}

pub struct NodeHit {
    pub shape: ShapeId,
    pub index: usize,
}

/// The four corners of a picture on the page, in node order (from its
/// rectangle's first corner), or `None` for other objects.
pub(crate) fn bitmap_corners(s: &tracedraw_core::Shape) -> Option<[Point; 4]> {
    let ShapeKind::Bitmap { rect, .. } = &s.kind else {
        return None;
    };
    let t = s.transform;
    Some([
        t * Point::new(rect.x0, rect.y0),
        t * Point::new(rect.x1, rect.y0),
        t * Point::new(rect.x1, rect.y1),
        t * Point::new(rect.x0, rect.y1),
    ])
}

/// Whether the Shape tool shows only the frame of `s`: an object drawn
/// through an envelope or a perspective is not where its own nodes are,
/// so the nodes, their outline in the node colour, the corner widgets
/// and a picture's corners would stand as a second, unbent copy beside
/// what is drawn (#2213). The frame is edited instead (`envelope_edit`,
/// the perspective's corners); the nodes are back once the effect is
/// cleared, switched off or made part of the curve (Convert to Curves).
pub(crate) fn frame_only(s: &tracedraw_core::Shape) -> bool {
    s.effects.iter().any(|e| {
        matches!(
            e,
            tracedraw_core::live::Effect::Envelope { .. }
                | tracedraw_core::live::Effect::Perspective { .. }
                | tracedraw_core::live::Effect::MeshEnvelope { .. }
        )
    })
}

/// The objects a double click with the Shape tool turns into curves:
/// rectangles, ellipses and polygons. A picture, text, group or clip frame
/// would lose what it is (#60).
pub(crate) fn curves_on_double_click(kind: &ShapeKind) -> bool {
    matches!(
        kind,
        ShapeKind::Rect { .. } | ShapeKind::Ellipse { .. } | ShapeKind::Polygon { .. }
    )
}

impl App {
    /// A node drag (`Drag::NodeMove`) reaching `p`: the grabbed node snaps
    /// and the chosen nodes follow; Ctrl keeps the move horizontal or
    /// vertical. One undo step for the whole drag.
    pub(crate) fn node_move_to(&mut self, p: Point, mods: Modifiers) {
        let Drag::NodeMove {
            start,
            from,
            grab,
            begun,
            still,
        } = self.drag.clone()
        else {
            return;
        };
        let mut d = p - from;
        let horizontal = (mods.ctrl || mods.command).then_some(d.x.abs() >= d.y.abs());
        let exclude: Vec<ShapeId> = start.iter().map(|c| c.shape).collect();
        // The rest of the curve being edited stays put: the node snaps to
        // its other nodes and segments too (#673).
        let still =
            still.unwrap_or_else(|| std::rc::Rc::new(crate::snap::StillGeometry::of(&start)));
        // Where the node goes once Ctrl keeps the move on one axis.
        let mut along = d;
        constrain_axis(&mut along, horizontal);
        let own = self.still_near(&still, grab + along);
        d = self.snapped_node_move(grab, d, horizontal, &exclude, &own);
        self.note_node_drag_step(begun);
        self.move_start_nodes(&start, d, grab, begun);
        self.drag = Drag::NodeMove {
            start,
            from,
            grab,
            begun: true,
            still: Some(still),
        };
    }

    /// Node tracking (Options > Display): a press with the Pick tool on a
    /// node of a selected curve chooses that node and starts dragging it.
    pub(crate) fn start_tracked_node_drag(&mut self, p: Point) -> bool {
        if !self.settings.node_tracking {
            return false;
        }
        let Some(hit) = self.node_at(p) else {
            return false;
        };
        self.node_selection = vec![(hit.shape, hit.index)];
        let grab = self
            .selected_node_points()
            .into_iter()
            .find(|(id, i, _)| (*id, *i) == (hit.shape, hit.index))
            .map(|(_, _, q)| q)
            .unwrap_or(p);
        self.drag = Drag::NodeMove {
            start: self.start_curves(),
            from: p,
            grab,
            begun: false,
            still: None,
        };
        true
    }

    /// The move of a grabbed node by `d` (page space): the node snaps to
    /// the grid, the page, guidelines, other objects and `own` (what stays
    /// put of its own curve), and the move follows it. `horizontal` keeps
    /// the move on one axis, before and after snapping.
    pub(crate) fn snapped_node_move(
        &self,
        grab: Point,
        mut d: Vec2,
        horizontal: Option<bool>,
        exclude: &[ShapeId],
        own: &[&crate::snap_points::Geo],
    ) -> Vec2 {
        constrain_axis(&mut d, horizontal);
        let target = self.snap_point_with(grab + d, exclude, own);
        let mut snapped = target - grab;
        constrain_axis(&mut snapped, horizontal);
        snapped
    }

    /// A curve's path, whether it is closed and its transform to the page
    /// (curves inside clip frames too).
    fn path_of(&self, id: ShapeId) -> Option<(BezPath, bool, tracedraw_core::geometry::Affine)> {
        let s = self.doc().find_shape(id)?;
        let parent = self.doc().parent_transform(id)?;
        match &s.kind {
            // A B-spline shows its control points, not its nodes, and a
            // common shape its handles (#4045).
            ShapeKind::Path { .. } if s.spline.is_some() || shows_handles(s) => None,
            ShapeKind::Path { path, closed } => Some((path.clone(), *closed, parent * s.transform)),
            _ => None,
        }
    }

    /// The curve whose nodes the Shape tool shows for the selected object
    /// `id`: the object itself, or for a clip frame (a cropped picture)
    /// its frame.
    pub(crate) fn node_target(&self, id: ShapeId) -> ShapeId {
        match self.doc().find_shape(id).map(|s| &s.kind) {
            Some(ShapeKind::ClipFrame { frame, .. }) if frame.id != id => frame.id,
            _ => id,
        }
    }

    /// With more than one object selected, the curve the Shape tool marks
    /// as the one picked last: its outline shows in the node colour (blue)
    /// and the outlines of the other selected objects in the second node
    /// colour (red), so the objects can be told apart (#1007). `None`
    /// with one object or none.
    pub(crate) fn shape_tool_key_curve(&self) -> Option<ShapeId> {
        if self.selection.len() < 2 {
            return None;
        }
        self.selection.last().map(|id| self.node_target(*id))
    }

    /// The selected objects as the Shape tool sees them: a clip frame
    /// gives way to its frame, on the page (an old file whose frame shares
    /// the clip frame's id shows the clip frame itself, which has no
    /// nodes). An object drawn through an envelope or a perspective is
    /// left out: it shows its frame only (see [`frame_only`]).
    pub(crate) fn shape_tool_shapes(&self) -> Vec<tracedraw_core::Shape> {
        self.selected_shapes()
            .into_iter()
            .filter(|s| !frame_only(s))
            .map(|s| match &s.kind {
                ShapeKind::ClipFrame { frame, .. } if frame.id != s.id => {
                    self.doc().shape_on_page(frame.id).unwrap_or(s)
                }
                _ => s,
            })
            .collect()
    }

    /// A selected picture's corner under the pointer: the picture and the
    /// corner (0 to 3, in [`bitmap_corners`] order).
    fn bitmap_corner_at(&self, p: Point) -> Option<(ShapeId, usize)> {
        let tol = self.settings.nodes.pick_px() / self.view.zoom as f64;
        self.selected_shapes().iter().find_map(|s| {
            if frame_only(s) {
                return None;
            }
            bitmap_corners(s)?
                .iter()
                .position(|c| (*c - p).hypot() <= tol)
                .map(|i| (s.id, i))
        })
    }

    /// A selected picture's edge under the pointer, away from its
    /// corners: the picture and the edge (0 to 3, from corner `i` to
    /// corner `i + 1` in [`bitmap_corners`] order), #3532.
    fn bitmap_edge_at(&self, p: Point) -> Option<(ShapeId, usize)> {
        let tol = self.settings.nodes.pick_px() / self.view.zoom as f64;
        self.selected_shapes().iter().find_map(|s| {
            if frame_only(s) {
                return None;
            }
            let c = bitmap_corners(s)?;
            (0..4).find_map(|i| {
                let (a, b) = (c[i], c[(i + 1) % 4]);
                let ab = b - a;
                let len2 = ab.hypot2();
                if len2 <= 1e-12 {
                    return None;
                }
                let t = (p - a).dot(ab) / len2;
                let near = (a + ab * t.clamp(0.0, 1.0) - p).hypot() <= tol;
                let away = (p - a).hypot() > tol && (p - b).hypot() > tol;
                (near && away && (0.0..=1.0).contains(&t)).then_some((s.id, i))
            })
        })
    }

    /// A press on a selected picture's edge (#3532): the picture goes
    /// into a clip frame of its rectangle and the edge's two corners are
    /// chosen and dragged together. False when no edge is at `p`.
    pub(crate) fn begin_bitmap_edge_crop(&mut self, p: Point) -> bool {
        let Some((id, edge)) = self.bitmap_edge_at(p) else {
            return false;
        };
        let Some(frame) = self.crop_bitmap_to_frame(id) else {
            return false;
        };
        self.node_selection = vec![(frame, edge), (frame, (edge + 1) % 4)];
        self.segment_spot = None;
        self.drag = Drag::NodeMove {
            start: self.start_curves(),
            from: p,
            grab: p,
            begun: false,
            still: None,
        };
        true
    }

    /// Put the picture `id` in a clip frame whose frame is the picture's
    /// rectangle (an undo step of its own); the frame's id, whose nodes 0
    /// to 3 are the picture's corners.
    pub(crate) fn crop_bitmap_to_frame(&mut self, id: ShapeId) -> Option<ShapeId> {
        let s = self.doc().find_shape(id)?.clone();
        let ShapeKind::Bitmap { rect, .. } = &s.kind else {
            return None;
        };
        let mut path = BezPath::new();
        path.move_to((rect.x0, rect.y0));
        path.line_to((rect.x1, rect.y0));
        path.line_to((rect.x1, rect.y1));
        path.line_to((rect.x0, rect.y1));
        path.close_path();
        let mut frame = tracedraw_core::Shape::new(
            self.engine.new_shape_id(),
            ShapeKind::Path { path, closed: true },
        );
        frame.fill = tracedraw_core::Fill::None;
        frame.stroke = None;
        let inner = crate::crop_tool::framed_contents(&s, self.engine.new_shape_id());
        let frame_id = frame.id;
        // The picture inside keeps the transparency; the frame drops it
        // so it is not laid on twice (#1491).
        let mut cmds = vec![Command::SetShapeKind {
            shape: id,
            kind: ShapeKind::ClipFrame {
                frame: Box::new(frame),
                contents: vec![inner],
            },
        }];
        cmds.extend(crate::crop_tool::frame_drops_transparency(&s));
        let _ = self.engine.run_batch("Crop Bitmap", &cmds);
        Some(frame_id)
    }

    /// The node targets of the selection (see [`Self::node_target`]),
    /// without the objects that show their frame only ([`frame_only`]).
    pub(crate) fn node_targets(&self) -> Vec<ShapeId> {
        self.selection
            .iter()
            .filter(|id| !self.doc().find_shape(**id).is_some_and(frame_only))
            .map(|id| self.node_target(*id))
            .collect()
    }

    /// Nodes chosen on an object that has since taken an envelope or a
    /// perspective are let go: they no longer show, and a key (Delete,
    /// an arrow) must not change what cannot be seen.
    pub(crate) fn drop_hidden_node_choices(&mut self) {
        if self.node_selection.is_empty() && self.segment_spot.is_none() {
            return;
        }
        let hidden: Vec<ShapeId> = self
            .selection
            .iter()
            .filter(|id| self.doc().find_shape(**id).is_some_and(frame_only))
            .map(|id| self.node_target(*id))
            .collect();
        if hidden.is_empty() {
            return;
        }
        self.node_selection.retain(|(id, _)| !hidden.contains(id));
        if self
            .segment_spot
            .is_some_and(|(id, _, _)| hidden.contains(&id))
        {
            self.segment_spot = None;
        }
    }

    /// Give curve `id` the path `path`: the first step of a drag is a new
    /// undo step named `label`, later steps of the same drag join it.
    /// Only the drag's own step is joined: an earlier step of the same
    /// name, another curve's handle drag among them, stays as it was
    /// (#3689).
    fn set_path(
        &mut self,
        id: ShapeId,
        path: BezPath,
        closed: bool,
        label: &'static str,
        begun: bool,
    ) {
        let cmd = Command::SetShapeKind {
            shape: id,
            kind: ShapeKind::Path { path, closed },
        };
        let _ = if begun {
            self.engine.amend(&cmd)
        } else {
            self.engine.run_with_label(&cmd, label)
        };
    }

    fn set_path_new_step(&mut self, id: ShapeId, path: BezPath, closed: bool) {
        let _ = self.engine.run(&Command::SetShapeKind {
            shape: id,
            kind: ShapeKind::Path { path, closed },
        });
    }

    /// Node under the pointer among selected curves.
    pub fn node_at(&self, p: Point) -> Option<NodeHit> {
        // The nearest node within the pick distance, measured on the page:
        // a curve with a scale of its own (text converted to curves, for
        // one) takes the same reach as any other.
        let tol = self.settings.nodes.pick_px() / self.view.zoom as f64;
        let mut best: Option<(NodeHit, f64)> = None;
        for id in &self.node_targets() {
            let Some((path, _, t)) = self.path_of(*id) else {
                continue;
            };
            for n in nodes::nodes(&(t * path)) {
                let d = (n.pos - p).hypot();
                if d <= tol && best.as_ref().is_none_or(|b| d < b.1) {
                    let hit = NodeHit {
                        shape: *id,
                        index: n.index,
                    };
                    best = Some((hit, d));
                }
            }
        }
        best.map(|b| b.0)
    }

    /// `p` lies within three pick distances of a node, a control handle
    /// of a chosen node or a rectangle, ellipse or polygon node of the
    /// selection: a press there that took nothing is a near miss.
    pub(crate) fn near_selected_nodes(&self, p: Point) -> bool {
        let reach = 3.0 * self.settings.nodes.pick_px() / self.view.zoom as f64;
        let shown = self.handle_nodes();
        for id in &self.node_targets() {
            let Some((path, _, t)) = self.path_of(*id) else {
                continue;
            };
            for n in nodes::nodes(&(t * path)) {
                let chosen = shown.contains(&(*id, n.index));
                let mut spots = vec![n.pos];
                if chosen {
                    spots.extend(n.ctrl_in);
                    spots.extend(n.ctrl_out);
                }
                if spots.iter().any(|q| (*q - p).hypot() <= reach) {
                    return true;
                }
            }
        }
        self.shape_tool_shapes()
            .iter()
            .any(|s| crate::kind_nodes::node_at(s, p, reach).is_some())
    }

    /// A rectangle, ellipse or polygon node of the selection under the
    /// pointer, with its object.
    pub fn kind_node_at(
        &self,
        p: Point,
    ) -> Option<(tracedraw_core::Shape, crate::kind_nodes::KindNode)> {
        let tol = self.settings.nodes.pick_px() / self.view.zoom as f64;
        self.shape_tool_shapes()
            .into_iter()
            .find_map(|s| crate::kind_nodes::node_at(&s, p, tol).map(|n| (s, n)))
    }

    /// A selected B-spline whose control polygon lies under the pointer,
    /// with its control points plus one at the pointer.
    pub(crate) fn spline_polygon_at(&self, p: Point) -> Option<(ShapeId, Vec<Point>)> {
        self.spline_polygon_hit(p).map(|(s, pts)| (s.id, pts))
    }

    /// [`spline_polygon_at`](Self::spline_polygon_at) with the spline.
    fn spline_polygon_hit(&self, p: Point) -> Option<(Shape, Vec<Point>)> {
        let tol = (self.settings.nodes.pick_px() + 1.0) / self.view.zoom as f64;
        self.shape_tool_shapes()
            .into_iter()
            .find_map(|s| crate::kind_nodes::spline_with_point(&s, p, tol).map(|pts| (s, pts)))
    }

    /// The nodes whose control handles show and take a press (#4337):
    /// with one node chosen, it and the nodes joined to it by a segment
    /// (the first and last of a closed subpath joined too), so the curves
    /// on both sides of it can be shaped; with several chosen, none, and
    /// only the chosen nodes' markers show.
    pub(crate) fn handle_nodes(&self) -> Vec<(ShapeId, usize)> {
        let [(id, index)] = self.node_selection[..] else {
            return Vec::new();
        };
        let Some((path, _, _)) = self.path_of(id) else {
            return Vec::new();
        };
        nodes::handle_neighbours(&path, index)
            .into_iter()
            .map(|n| (id, n))
            .collect()
    }

    /// Control handle under the pointer, for selected nodes only, or one
    /// of the two handles a chosen segment shows (its start node's
    /// outgoing handle and its end node's incoming one).
    pub(crate) fn ctrl_handle_at(&self, p: Point) -> Option<(ShapeId, usize, Which)> {
        // The nearest handle within the pick distance, on the page; a
        // press anywhere on its arrow head takes it (#1643).
        let tol = self.settings.nodes.handle_pick_px() / self.view.zoom as f64;
        let mut best: Option<((ShapeId, usize, Which), f64)> = None;
        let mut take = |key: (ShapeId, usize, Which), c: Option<Point>| {
            let Some(c) = c else { return };
            let d = (c - p).hypot();
            if d <= tol && best.as_ref().is_none_or(|b| d < b.1) {
                best = Some((key, d));
            }
        };
        if let Some((id, index)) = self.chosen_segment().filter(|s| s.1 > 0) {
            if let Some((path, _, t)) = self.path_of(id) {
                for n in nodes::nodes(&(t * path)) {
                    if n.index + 1 == index {
                        take((id, n.index, Which::Out), n.ctrl_out);
                    } else if n.index == index {
                        take((id, n.index, Which::In), n.ctrl_in);
                    }
                }
            }
            return best.map(|b| b.0);
        }
        // The nodes whose handles show (#4337), each curve worked out once.
        let shown = self.handle_nodes();
        for (k, &(id, _)) in shown.iter().enumerate() {
            if shown[..k].iter().any(|s| s.0 == id) {
                continue;
            }
            let Some((path, _, t)) = self.path_of(id) else {
                continue;
            };
            for n in nodes::nodes(&(t * path)) {
                if shown.contains(&(id, n.index)) {
                    take((id, n.index, Which::In), n.ctrl_in);
                    take((id, n.index, Which::Out), n.ctrl_out);
                }
            }
        }
        best.map(|b| b.0)
    }

    /// A step of dragging a rectangle, ellipse, polygon or B-spline node,
    /// or a common shape's handle (#4045): the object drawn again from
    /// where the drag began, one undo step for the whole drag. The Shape
    /// tool and the Common Shapes tool drag them alike.
    pub(crate) fn kind_node_drag_step(&mut self, p: Point, mods: Modifiers) {
        let Drag::KindNode {
            shape,
            node,
            start,
            single,
            chosen,
            begun,
            mut turn,
        } = self.drag.clone()
        else {
            return;
        };
        // Ctrl as it is held now, not only at the press: one
        // corner of a rectangle, an ellipse's angle in steps of
        // the constrain angle, a star's points keeping their
        // turn, from the first drag on (#1325, #1331, #1340).
        let ctrl = mods.ctrl || mods.command;
        let q = self.snap_point_excluding(p, &[shape]);
        let step = self.settings.constrain_angle;
        let cmd = match node {
            crate::kind_nodes::KindNode::Spline { index } => {
                let chosen: Vec<usize> = self
                    .node_selection
                    .iter()
                    .filter(|(id, _)| *id == shape)
                    .map(|(_, i)| *i)
                    .collect();
                if chosen.contains(&index) {
                    crate::kind_nodes::spline_moved_many(&start, &chosen, index, q)
                } else {
                    crate::kind_nodes::spline_moved(&start, index, q)
                }
                .map(|points| Command::SetSpline {
                    shape,
                    points,
                    fixed: None,
                })
            }
            crate::kind_nodes::KindNode::Adjust { index } => {
                crate::common_shapes::adjust_command(&start, index, q)
            }
            _ => crate::kind_nodes::drag_constrained(
                &start, node, q, ctrl, chosen, &mut turn, step,
            )
            .map(|kind| Command::SetShapeKind { shape, kind }),
        };
        // One undo step for the whole drag, begun by the first step that
        // changes something (a step that changed nothing must not fold
        // the drag into the step before it).
        let ran = match cmd {
            Some(cmd) if begun => self.engine.amend(&cmd).is_ok(),
            Some(cmd) => self
                .engine
                .run_with_label(&cmd, crate::kind_nodes::label_for(&start, Some(node)))
                .is_ok(),
            None => false,
        };
        self.drag = Drag::KindNode {
            shape,
            node,
            start,
            single,
            chosen,
            begun: begun || ran,
            turn,
        };
    }

    pub fn shape_tool_input(&mut self, response: &Response, p: Point, mods: Modifiers) {
        self.drop_hidden_node_choices();
        if response.double_clicked_by(PointerButton::Primary) {
            // A character's node: a second click there within the double
            // click time and distance chooses it as a click does.
            // Neighbouring characters' nodes lie a few pixels apart, so a
            // quick Shift+click on the next one is a double click; it went
            // on to select the text again below, which let go of the
            // chosen characters (#1933).
            if let Some(key) = self.text_node_at(p) {
                self.click_node(key, mods);
                return;
            }
            // A B-spline: a double-click on a control point removes it,
            // one on the control polygon adds one there.
            if let Some((s, crate::kind_nodes::KindNode::Spline { index })) = self.kind_node_at(p) {
                if let Some(points) = crate::kind_nodes::spline_without(&s, index) {
                    self.run(Command::SetSpline {
                        shape: s.id,
                        fixed: crate::kind_nodes::spline_fixed_for(&s, &points),
                        points,
                    });
                }
                return;
            }
            if let Some((s, points)) = self.spline_polygon_hit(p) {
                self.run(Command::SetSpline {
                    shape: s.id,
                    fixed: crate::kind_nodes::spline_fixed_for(&s, &points),
                    points,
                });
                return;
            }
            if let Some(hit) = self.node_at(p) {
                // Double-click a node deletes it.
                if let Some((path, closed, _)) = self.path_of(hit.shape) {
                    let np = nodes::delete_node(&path, hit.index);
                    self.set_path_new_step(hit.shape, np, closed);
                    self.node_selection.clear();
                }
                return;
            }
            // A polygon or star turns into a curve at the edge clicked, so
            // the node goes in there (#2451).
            self.polygon_edge_to_curve(p);
            // Double-click a segment of a selected curve adds a node; on a
            // non-curve object, convert it to curves.
            if let Some((id, index, t)) = self.segment_at(p) {
                if let Some((path, closed, _)) = self.path_of(id) {
                    let np = nodes::add_node(&path, index, t);
                    self.set_path_new_step(id, np, closed);
                    self.node_selection = vec![(id, index)];
                }
                return;
            }
            // Only shapes become curves: a picture, text, group or clip
            // frame would lose what it is (#60).
            if let Some(id) = self.hit_test(p) {
                let shape = self
                    .doc()
                    .find_shape(id)
                    .map(|s| curves_on_double_click(&s.kind));
                self.select(vec![id]);
                if shape == Some(true) {
                    // The nodes to edit; an envelope being edited stays.
                    self.convert_selection_to_curves(false);
                }
            }
            return;
        }

        if response.drag_started_by(PointerButton::Primary) {
            // Text: the spacing arrows, then the character nodes.
            if let Some((id, horizontal)) = self.text_arrow_at(p) {
                if let Some(s) = self.doc().find_shape(id).cloned() {
                    self.drag = Drag::TextSpacing {
                        start: Box::new(s),
                        horizontal,
                        word: mods.shift,
                        from: p,
                        begun: false,
                    };
                    return;
                }
            }
            if let Some(key) = self.text_node_at(p) {
                if !self.node_selection.contains(&key) {
                    // As for a curve's node: Shift adds, Ctrl alone only
                    // constrains the drag (#1325).
                    if mods.shift {
                        self.click_node(key, mods);
                    } else {
                        self.node_selection = vec![key];
                    }
                }
                if let Some(s) = self.doc().find_shape(key.0).cloned() {
                    self.drag = Drag::TextChars {
                        chars: self.chosen_chars(key.0),
                        start: Box::new(s),
                        from: p,
                        begun: false,
                    };
                    return;
                }
            }
            // A picture's corner: the picture becomes a clip frame whose
            // frame is its rectangle, and the corner moves as a node of it.
            if let Some((id, corner)) = self.bitmap_corner_at(p) {
                if let Some(frame) = self.crop_bitmap_to_frame(id) {
                    self.node_selection = vec![(frame, corner)];
                    let grab = self
                        .selected_node_points()
                        .first()
                        .map(|(_, _, q)| *q)
                        .unwrap_or(p);
                    self.drag = Drag::NodeMove {
                        start: self.start_curves(),
                        from: p,
                        grab,
                        begun: false,
                        still: None,
                    };
                    return;
                }
            }
            // A picture's edge: cropped the same way, the edge's two
            // corners moving together (Ctrl keeps the move to one axis),
            // #3532.
            if self.begin_bitmap_edge_crop(p) {
                return;
            }
            if let Some((s, node)) = self.kind_node_at(p) {
                // A B-spline's control point is chosen as a node is: a
                // click chooses it alone (unless it is chosen already, so
                // the chosen ones drag together), Shift adds it.
                if let crate::kind_nodes::KindNode::Spline { index } = node {
                    let key = (s.id, index);
                    if mods.shift {
                        if !self.node_selection.contains(&key) {
                            self.node_selection.push(key);
                        }
                    } else if !self.node_selection.contains(&key) {
                        self.node_selection = vec![key];
                    }
                    self.segment_spot = None;
                }
                let single = crate::kind_nodes::single_corner(node, mods.ctrl);
                let chosen =
                    crate::kind_nodes::chosen_corners(&self.rect_corners_selected, &s, node);
                self.drag = Drag::KindNode {
                    shape: s.id,
                    node,
                    start: Box::new(s),
                    single,
                    chosen,
                    begun: false,
                    turn: 0.0,
                };
                return;
            }
            // The node transform handles, while a transform mode is on.
            if let Some(handle) = self.node_transform_handle_at(p) {
                if let Some(bounds) = self.selected_nodes_bounds() {
                    self.drag = Drag::NodeTransform {
                        handle,
                        start: self.start_curves(),
                        bounds,
                        from: p,
                        begun: false,
                    };
                    return;
                }
            }
            // A corner widget rounds its own corner, the chosen nodes'
            // corners when it is one of them, or with Shift every corner
            // (#635); a Ctrl+click on it chooses it (see the click below).
            if let Some(w) = self.corner_widget_at(p) {
                // Ctrl+click chooses the corner, or takes it out.
                let one_at_a_time = self.settings.nodes.corner_one_at_a_time;
                // With every corner rounding together, Ctrl+drag rounds
                // this corner alone (#4216).
                let own_only = !one_at_a_time && mods.command && !mods.shift;
                if self.start_corner_drag_with(w, p, one_at_a_time && mods.shift, own_only) {
                    return;
                }
            }
            if let Some((shape, index, which)) = self.ctrl_handle_at(p) {
                // The page's picture is kept from the drag's first step
                // (#1259); a handle drag has no `begun` of its own.
                self.node_drag_shown = None;
                self.choose_handle((shape, index, which));
                self.drag = Drag::Handle {
                    shape,
                    index,
                    which,
                };
                return;
            }
            if let Some(hit) = self.node_at(p) {
                let key = (hit.shape, hit.index);
                if !self.node_selection.contains(&key) {
                    // Shift adds the node (or a run up to it) to the
                    // chosen ones, which then drag together. Ctrl alone
                    // only constrains the drag: the node pressed is the
                    // one dragged, as with no key held; it used to be
                    // added to the chosen ones, so Ctrl+drag on a node
                    // moved the nodes chosen before as well (#1325).
                    // Ctrl+click still adds or takes out a node.
                    if mods.shift {
                        self.click_node(key, mods);
                    } else {
                        self.node_run = None;
                        self.node_selection = vec![key];
                    }
                }
                let grab = self
                    .selected_node_points()
                    .into_iter()
                    .find(|(id, i, _)| (*id, *i) == key)
                    .map(|(_, _, q)| q)
                    .unwrap_or(p);
                self.drag = Drag::NodeMove {
                    start: self.start_curves(),
                    from: p,
                    grab,
                    begun: false,
                    still: None,
                };
                return;
            }
            // Alt: a freehand marquee whatever the bar's selection mode,
            // also over a curve; Shift keeps the nodes chosen already
            // (#1341).
            if mods.alt {
                self.node_lasso_keeps = mods.shift;
                self.drag = Drag::NodeLasso { points: vec![p] };
                return;
            }
            // A segment of a selected curve bends.
            if let Some((shape, index, t)) = self.segment_at(p) {
                if let Some((path, closed, _)) = self.path_of(shape) {
                    self.segment_spot = None;
                    self.drag = Drag::SegmentBend {
                        shape,
                        path,
                        closed,
                        index,
                        t,
                        from: p,
                        begun: false,
                    };
                    return;
                }
            }
            // An edge of a selected polygon or star: a drag bends all its
            // sides alike (#2451).
            if let Some(shape) = self.polygon_edge_at(p) {
                let start = self
                    .doc()
                    .find_shape(shape)
                    .map(|s| (s.kind.clone(), s.transform));
                let parent = self.doc().parent_transform(shape);
                if let (Some((start, t)), Some(parent)) = (start, parent) {
                    self.drag = Drag::PolygonEdge {
                        shape,
                        from: p,
                        start,
                        to_local: (parent * t).inverse(),
                        begun: false,
                    };
                    return;
                }
            }
            // A marquee over the selected objects' nodes. It keeps the
            // selection, also when it starts over another object (the one
            // behind the curve, or the group holding it), so the curve
            // being edited stays the one edited (#1594); only with
            // nothing selected does the object under the pointer get
            // selected, so a marquee can start on it right away.
            if self.selection.is_empty() {
                if let Some(id) = self.hit_test(p) {
                    self.select(vec![id]);
                }
            }
            // Shift keeps the nodes and characters chosen already and adds
            // the ones inside, as Shift+click does; the marquee used to
            // replace them, so the first line's characters let go when a
            // Shift+marquee took the second line's (#2331).
            self.node_lasso_keeps = mods.shift;
            self.drag = if self.node_lasso {
                Drag::NodeLasso { points: vec![p] }
            } else {
                Drag::NodeMarquee {
                    start: p,
                    current: p,
                }
            };
        }

        if response.dragged_by(PointerButton::Primary) {
            match self.drag.clone() {
                Drag::TextChars {
                    start,
                    chars,
                    from,
                    begun,
                } => {
                    let mut d = p - from;
                    if mods.ctrl || mods.command {
                        if d.x.abs() >= d.y.abs() {
                            d.y = 0.0;
                        } else {
                            d.x = 0.0;
                        }
                    }
                    self.move_text_chars(&start, &chars, d, begun);
                    self.drag = Drag::TextChars {
                        start,
                        chars,
                        from,
                        begun: true,
                    };
                }
                Drag::TextSpacing {
                    start,
                    horizontal,
                    word,
                    from,
                    begun,
                } => {
                    self.drag_text_spacing(&start, horizontal, word, p - from, begun);
                    self.drag = Drag::TextSpacing {
                        start,
                        horizontal,
                        word,
                        from,
                        begun: true,
                    };
                }
                Drag::NodeMove { .. } => self.node_move_to(p, mods),
                Drag::PolygonEdge {
                    shape,
                    from,
                    start,
                    to_local,
                    begun,
                } if !matches!(
                    start,
                    ShapeKind::Polygon {
                        complex: Some(_),
                        ..
                    }
                ) =>
                {
                    // Every side bends alike as the pointer goes across
                    // the edge grabbed; one undo step for the drag.
                    if begun || (p - from).hypot() * self.view.zoom as f64 >= 2.0 {
                        if let Some(kind) =
                            crate::kind_nodes::polygon_bent(&start, to_local * from, to_local * p)
                        {
                            let cmd = Command::SetShapeKind { shape, kind };
                            let _ = if begun {
                                self.engine.amend(&cmd)
                            } else {
                                self.engine.run_with_label(&cmd, "Bend Sides")
                            };
                        }
                        self.drag = Drag::PolygonEdge {
                            shape,
                            from,
                            start,
                            to_local,
                            begun: true,
                        };
                    }
                }
                Drag::PolygonEdge { from, .. } => {
                    // A complex star: curve first, then the edge bends.
                    if (p - from).hypot() * self.view.zoom as f64 >= 2.0 {
                        self.drag = Drag::None;
                        if self.polygon_edge_to_curve(from) {
                            if let Some((shape, index, t)) = self.segment_at(from) {
                                if let Some((path, closed, _)) = self.path_of(shape) {
                                    self.drag = Drag::SegmentBend {
                                        shape,
                                        path,
                                        closed,
                                        index,
                                        t,
                                        from,
                                        begun: false,
                                    };
                                }
                            }
                        }
                    }
                }
                Drag::CornerWidget(d) => self.corner_drag_to(*d, p),
                Drag::NodeTransform {
                    handle,
                    start,
                    bounds,
                    from,
                    begun,
                } => {
                    let a = crate::node_edit::handle_transform(
                        self.node_transform,
                        handle,
                        bounds,
                        from,
                        p,
                        mods.shift,
                    );
                    self.note_node_drag_step(begun);
                    self.transform_start_nodes(&start, a, begun);
                    self.drag = Drag::NodeTransform {
                        handle,
                        start,
                        bounds,
                        from,
                        begun: true,
                    };
                }
                Drag::NodeLasso { mut points } => {
                    if points
                        .last()
                        .is_none_or(|q| (*q - p).hypot() > 0.2 / self.view.zoom as f64)
                    {
                        points.push(p);
                    }
                    self.drag = Drag::NodeLasso { points };
                }
                Drag::Handle {
                    shape,
                    index,
                    which,
                } => {
                    // A handle dragged is not chosen: no ring follows it,
                    // and none stays once the button is let go. A click
                    // without a drag chooses it for the arrow keys
                    // (#2819, #3041, #3042).
                    self.chosen_handle = None;
                    // Alt held: the node is dragged by its handle, the
                    // handle keeping its place about it (#3719).
                    let by_handle = mods.alt
                        && self.path_of(shape).is_some_and(|(path, closed, t)| {
                            let Some(n) = nodes::nodes(&path).into_iter().find(|n| n.index == index)
                            else {
                                return false;
                            };
                            let tip = match which {
                                nodes::Which::In => n.ctrl_in,
                                nodes::Which::Out => n.ctrl_out,
                            };
                            let Some(tip) = tip else { return false };
                            let q = t.inverse() * self.snap_point_excluding(p, &[shape]);
                            let np = nodes::move_node(&path, index, q - tip);
                            let begun = self.node_drag_shown.is_some();
                            self.note_node_drag_step(begun);
                            self.set_path(shape, np, closed, "Edit Handle", begun);
                            true
                        });
                    let plain = if by_handle { None } else { self.path_of(shape) };
                    if let Some((path, closed, t)) = plain {
                        // The handle snaps like a node does; Ctrl or Shift
                        // keeps it to the constrain angle about its node,
                        // on the page, and a smooth node's other handle
                        // still follows (#510).
                        let mut q = self.snap_point_excluding(p, &[shape]);
                        if mods.ctrl || mods.command || mods.shift {
                            if let Some(n) = nodes::nodes(&path).iter().find(|n| n.index == index) {
                                q = crate::interaction::constrain_from(
                                    t * n.pos,
                                    q,
                                    self.settings.constrain_angle,
                                );
                            }
                        }
                        let lp = t.inverse() * q;
                        let ty = self.node_type_of(shape, index);
                        let np = nodes::move_handle(&path, index, which, lp, ty);
                        // The press cleared the page's kept picture: it is
                        // there from the drag's first step on.
                        let begun = self.node_drag_shown.is_some();
                        self.note_node_drag_step(begun);
                        self.set_path(shape, np, closed, "Edit Handle", begun);
                    }
                }
                Drag::NodeMarquee { start, .. } => {
                    self.drag = Drag::NodeMarquee { start, current: p }
                }
                Drag::SegmentBend {
                    shape,
                    path,
                    closed,
                    index,
                    t,
                    from,
                    begun,
                } => {
                    // The grabbed point follows the pointer (in the curve's
                    // own space); one undo step for the whole drag.
                    if let Some((_, _, tf)) = self.path_of(shape) {
                        let det = tf.determinant();
                        if det.is_finite() && det.abs() > 1e-12 {
                            let inv = tf.inverse();
                            let d = (inv * p) - (inv * from);
                            let chosen = match self.doc().shape(shape) {
                                Ok((_, s)) => s.node_types.clone(),
                                Err(_) => Default::default(),
                            };
                            // Rounded corners beside the segment stay
                            // round (#3265).
                            let bent = if self.keeps_rounded_corners()(&path) {
                                tracedraw_core::corner_cut::bend_keeping_fillets(
                                    &path,
                                    index,
                                    t,
                                    |p, i, t, map| {
                                        let chosen = chosen
                                            .iter()
                                            .filter_map(|(k, v)| {
                                                Some((map.get(*k).copied().flatten()?, *v))
                                            })
                                            .collect();
                                        nodes::bend_segment_with(p, i, t, d, &chosen)
                                    },
                                )
                            } else {
                                nodes::bend_segment_with(&path, index, t, d, &chosen)
                            };
                            let cmd = Command::SetShapeKind {
                                shape,
                                kind: ShapeKind::Path { path: bent, closed },
                            };
                            self.note_node_drag_step(begun);
                            let _ = if begun {
                                self.engine.amend(&cmd)
                            } else {
                                self.engine.run_with_label(&cmd, "Bend Segment")
                            };
                        }
                    }
                    self.drag = Drag::SegmentBend {
                        shape,
                        path,
                        closed,
                        index,
                        t,
                        from,
                        begun: true,
                    };
                }
                Drag::KindNode { .. } => self.kind_node_drag_step(p, mods),
                _ => {}
            }
        }

        if response.clicked_by(PointerButton::Primary) {
            // A click anywhere but on a handle lets go of the chosen
            // handle (below, a click on one chooses it again).
            if self.ctrl_handle_at(p).is_none() {
                self.chosen_handle = None;
            }
            // Ctrl+click on a corner widget chooses its corner, or takes
            // it out, so one or several corners round together.
            if self.settings.nodes.corner_one_at_a_time && mods.command && !mods.shift {
                if let Some(w) = self.corner_widget_at(p) {
                    self.toggle_corner_widget(w);
                    return;
                }
            }
            if let Some(key) = self.text_node_at(p) {
                self.click_node(key, mods);
                return;
            }
            if let Some((s, node)) = self.kind_node_at(p) {
                // A click on a rectangle corner chooses it alone, Shift or
                // Ctrl adds or removes it: dragging a chosen corner then
                // changes the chosen corners only (#671).
                match node {
                    crate::kind_nodes::KindNode::Corner { corner, .. } => {
                        let key = (s.id, corner);
                        if mods.shift || mods.ctrl || mods.command {
                            if let Some(i) =
                                self.rect_corners_selected.iter().position(|k| *k == key)
                            {
                                self.rect_corners_selected.remove(i);
                            } else {
                                self.rect_corners_selected.push(key);
                            }
                        } else {
                            self.rect_corners_selected = vec![key];
                        }
                    }
                    _ => self.rect_corners_selected.clear(),
                }
                // A B-spline's control point: chosen alone, Shift adds or
                // removes it.
                if let crate::kind_nodes::KindNode::Spline { index } = node {
                    self.click_node((s.id, index), mods);
                }
                return;
            }
            if let Some(hit) = self.node_at(p) {
                self.segment_spot = None;
                self.click_node((hit.shape, hit.index), mods);
                return;
            }
            // A click on a control handle keeps the nodes chosen, so the
            // handle can be dragged next; the segment under it is not
            // taken instead (#639). A node comes first, so a handle lying
            // on its node never swallows a click on the node.
            // The handle is chosen: the arrow keys then move it, not its
            // node (#2819).
            if let Some(key) = self.ctrl_handle_at(p) {
                self.choose_handle(key);
                return;
            }
            // A plain click on a corner widget chooses its corner alone:
            // the ring is drawn chosen and the property bar's corner
            // radius sets it (#2057). With a key held it keeps the
            // selection.
            if let Some(w) = self.corner_widget_at(p) {
                if !(mods.shift || mods.ctrl || mods.command || mods.alt) {
                    self.choose_corner_widget(w);
                }
                return;
            }
            // A click on a segment chooses the segment: it is drawn
            // highlighted, Delete removes it, and as its end node is the
            // one chosen To Line and To Curve act on it. The spot clicked
            // is marked; Add Node puts the new node there.
            if let Some((shape, index, t)) = self.segment_at(p) {
                self.node_selection = vec![(shape, index)];
                self.segment_spot = Some((shape, index, t));
                return;
            }
            self.segment_spot = None;
            // A near miss of a node or a handle of the selected curves
            // keeps the selection: only a click clearly on empty canvas
            // deselects.
            if self.near_selected_nodes(p) || self.spline_polygon_at(p).is_some() {
                return;
            }
            match self.hit_test(p) {
                Some(id) => {
                    if mods.shift {
                        if !self.selection.contains(&id) {
                            self.selection.push(id);
                        }
                    } else if !self.selection.contains(&id) {
                        self.select(vec![id]);
                    } else {
                        self.node_selection.clear();
                    }
                }
                None => self.select(Vec::new()),
            }
        }
    }

    /// Finish a node marquee: select the nodes inside the rectangle.
    pub fn finish_node_marquee(&mut self, start: Point, current: Point) {
        let keeps = std::mem::take(&mut self.node_lasso_keeps);
        let r = Rect::from_points(start, current);
        if r.width() < 0.05 && r.height() < 0.05 {
            return;
        }
        // The Mesh Fill tool's box chooses mesh nodes (discussion #2387);
        // the Shape tool's chooses those of a selected mesh too (#2506).
        if self.tool == crate::tools::Tool::MeshFill {
            self.choose_mesh_nodes_in(r, keeps);
            return;
        }
        // The Envelope tool's box chooses envelope nodes (#3001).
        if self.tool == crate::tools::Tool::Envelope {
            self.choose_envelope_nodes_in(r, keeps);
            return;
        }
        if self.selected_mesh().is_some() {
            self.choose_mesh_nodes_in(r, keeps);
        }
        let before = (
            self.node_selection.clone(),
            self.rect_corners_selected.clone(),
        );
        let mut sel = Vec::new();
        for id in &self.node_targets() {
            let Some((path, _, t)) = self.path_of(*id) else {
                continue;
            };
            for n in nodes::nodes(&path) {
                if r.contains(t * n.pos) {
                    sel.push((*id, n.index));
                }
            }
        }
        // B-splines: their control points inside.
        sel.extend(self.spline_points_where(|q| r.contains(q)));
        self.node_selection = sel;
        // Rectangles: the corners whose nodes are inside (#671).
        self.rect_corners_selected =
            crate::kind_nodes::corners_where(&self.shape_tool_shapes(), |q| r.contains(q));
        self.segment_spot = None;
        // Character nodes of selected texts too.
        self.choose_chars_in(|q| r.contains(q));
        if keeps {
            self.keep_earlier_choice(before);
        }
    }

    /// Shift+marquee (rectangular or freehand): the nodes, corners and
    /// characters chosen before the marquee (`before`) stay chosen, the
    /// ones the marquee chose join them (#1341, #2331).
    pub(crate) fn keep_earlier_choice(
        &mut self,
        before: (Vec<(ShapeId, usize)>, Vec<(ShapeId, usize)>),
    ) {
        let (mut nodes, mut corners) = before;
        for k in std::mem::take(&mut self.node_selection) {
            if !nodes.contains(&k) {
                nodes.push(k);
            }
        }
        for k in std::mem::take(&mut self.rect_corners_selected) {
            if !corners.contains(&k) {
                corners.push(k);
            }
        }
        self.node_selection = nodes;
        self.rect_corners_selected = corners;
    }

    /// The control points of the selected B-splines (page space) for which
    /// `inside` holds, as node selection keys.
    pub(crate) fn spline_points_where(
        &self,
        inside: impl Fn(Point) -> bool,
    ) -> Vec<(ShapeId, usize)> {
        let mut out = Vec::new();
        for s in self.shape_tool_shapes() {
            if s.spline.is_none() {
                continue;
            }
            for (node, q) in crate::kind_nodes::nodes(&s) {
                if let crate::kind_nodes::KindNode::Spline { index } = node {
                    if inside(q) {
                        out.push((s.id, index));
                    }
                }
            }
        }
        out
    }

    /// Nearest segment of a selected curve within a few pixels, or
    /// anywhere on its outline as drawn: a thick outline takes a press
    /// out to its edge, not only near the path down its middle (#1935).
    pub(crate) fn segment_at(&self, p: Point) -> Option<(ShapeId, usize, f64)> {
        let tol = (self.settings.nodes.pick_px() + 1.0) / self.view.zoom as f64;
        let mut best: Option<(ShapeId, usize, f64, f64)> = None;
        for id in &self.node_targets() {
            let Some((path, _, t)) = self.path_of(*id) else {
                continue;
            };
            let reach = self
                .doc()
                .find_shape(*id)
                .map(|s| crate::interaction::outline_reach(s, t))
                .unwrap_or(0.0);
            // Measured on the page, as the nodes are.
            if let Some((i, tt, d)) = nodes::nearest_segment(&(t * path), p) {
                if d <= tol + reach && best.map(|b| d < b.3).unwrap_or(true) {
                    best = Some((*id, i, tt, d));
                }
            }
        }
        best.map(|(id, i, t, _)| (id, i, t))
    }

    /// The selected polygon or star whose outline lies within the pick
    /// distance of `p` (an edge, as a curve's segment is found). Nodes
    /// are looked for first by the callers.
    fn polygon_edge_at(&self, p: Point) -> Option<ShapeId> {
        let tol = (self.settings.nodes.pick_px() + 1.0) / self.view.zoom as f64;
        let mut ids = self.selection.clone();
        ids.dedup();
        let mut symbols_free_id = || ShapeId(0);
        for id in ids {
            let Some(s) = self.doc().find_shape(id) else {
                continue;
            };
            if frame_only(s) || !matches!(s.kind, ShapeKind::Polygon { .. }) {
                continue;
            }
            let Some(parent) = self.doc().parent_transform(id) else {
                continue;
            };
            let Some(ShapeKind::Path { path, .. }) = s.curves_kind(&[], &mut symbols_free_id)
            else {
                continue;
            };
            let reach = crate::interaction::outline_reach(s, parent * s.transform);
            if let Some((_, _, d)) = nodes::nearest_segment(&((parent * s.transform) * path), p) {
                if d <= tol + reach {
                    return Some(id);
                }
            }
        }
        None
    }

    /// Turns the polygon or star whose edge lies at `p` into a curve (one
    /// undo step, outline warps staying live), so its edge can take a
    /// node or bend like a curve's segment (#2451). `true` when it did.
    fn polygon_edge_to_curve(&mut self, p: Point) -> bool {
        let Some(id) = self.polygon_edge_at(p) else {
            return false;
        };
        let Some(s) = self.doc().find_shape(id).cloned() else {
            return false;
        };
        self.convert_shapes_to_curves(vec![s], false);
        self.doc()
            .find_shape(id)
            .is_some_and(|s| matches!(s.kind, ShapeKind::Path { .. }))
    }

    // ----- property bar actions ---------------------------------------------

    fn for_each_selected_node(
        &mut self,
        label: &'static str,
        f: impl Fn(&BezPath, usize) -> BezPath,
    ) {
        let sel = self.node_selection.clone();
        let mut by_shape: std::collections::BTreeMap<ShapeId, Vec<usize>> = Default::default();
        for (id, i) in sel {
            by_shape.entry(id).or_default().push(i);
        }
        let mut cmds = Vec::new();
        for (id, mut idxs) in by_shape {
            let Some((mut path, closed, _)) = self.path_of(id) else {
                continue;
            };
            // Highest index first so earlier indices stay valid when elements are removed.
            idxs.sort_unstable_by(|a, b| b.cmp(a));
            for i in idxs {
                path = f(&path, i);
            }
            cmds.push(Command::SetShapeKind {
                shape: id,
                kind: ShapeKind::Path { path, closed },
            });
        }
        if !cmds.is_empty() {
            let _ = self.engine.run_batch(label, &cmds);
        }
    }

    /// Reduce Nodes: simplify the selected curves (all nodes when none are selected).
    pub fn reduce_selected_nodes(&mut self) {
        let tol = 0.3 / self.view.zoom.max(0.05) as f64;
        let ids: Vec<ShapeId> = if self.node_selection.is_empty() {
            self.selection.clone()
        } else {
            let mut v: Vec<ShapeId> = self.node_selection.iter().map(|(id, _)| *id).collect();
            v.dedup();
            v
        };
        let mut cmds = Vec::new();
        for id in ids {
            let Some((path, closed, _)) = self.path_of(id) else {
                continue;
            };
            let reduced = nodes::reduce_nodes(&path, tol);
            if reduced.elements().len() < path.elements().len() {
                cmds.push(Command::SetShapeKind {
                    shape: id,
                    kind: ShapeKind::Path {
                        path: reduced,
                        closed,
                    },
                });
            }
        }
        let spots = self.chosen_node_spots();
        if !cmds.is_empty() {
            let _ = self.engine.run_batch("Reduce Nodes", &cmds);
        }
        self.choose_nodes_at(&spots);
    }

    /// Object > Nodes > Add Node to Every Segment: a node in the middle of
    /// every segment of the selected curves (their shape unchanged), with
    /// any tool; one undo step. Run again, it halves the segments again.
    pub fn add_node_every_segment(&mut self) {
        let mut cmds = Vec::new();
        for id in self.selection.clone() {
            let Some((path, closed, _)) = self.path_of(id) else {
                continue;
            };
            let more = nodes::add_node_every_segment(&path);
            if more.elements().len() > path.elements().len() {
                cmds.push(Command::SetShapeKind {
                    shape: id,
                    kind: ShapeKind::Path { path: more, closed },
                });
            }
        }
        if !cmds.is_empty() {
            let _ = self.engine.run_batch("Add Node to Every Segment", &cmds);
            self.node_selection.clear();
        }
    }

    /// Whether Add Node to Every Segment has a curve to work on.
    pub fn can_add_node_every_segment(&self) -> bool {
        self.selection.iter().any(|id| self.path_of(*id).is_some())
    }

    /// Align Nodes: move the selected nodes onto a common horizontal and/or vertical.
    pub fn align_selected_nodes(&mut self, horizontal: bool, vertical: bool) {
        let sel = self.node_selection.clone();
        let mut by_shape: std::collections::BTreeMap<ShapeId, Vec<usize>> = Default::default();
        for (id, i) in sel {
            by_shape.entry(id).or_default().push(i);
        }
        let mut cmds = Vec::new();
        for (id, idxs) in by_shape {
            let Some((path, closed, _)) = self.path_of(id) else {
                continue;
            };
            cmds.push(Command::SetShapeKind {
                shape: id,
                kind: ShapeKind::Path {
                    path: nodes::align_nodes(&path, &idxs, horizontal, vertical),
                    closed,
                },
            });
        }
        if !cmds.is_empty() {
            let _ = self.engine.run_batch("Align Nodes", &cmds);
        }
    }

    /// The align keys and commands (L, R, T, B, C, E) on the Shape tool's
    /// selected nodes, two or more, as on objects (#364): to the leftmost,
    /// rightmost, top or bottom node, or to the centre of their box.
    /// Handles move with their node. One undo step; false when fewer than
    /// two nodes are selected or the alignment has no node version.
    pub fn align_nodes_to(&mut self, how: crate::ops::Align) -> bool {
        use crate::ops::Align;
        use tracedraw_core::geometry::{Point, Vec2};
        if self.tool != crate::tools::Tool::Shape
            || self.node_selection.len() < 2
            || how == Align::CenterPage
        {
            return false;
        }
        // Each selected node on the page, with its curve's transform.
        let mut picked: Vec<(ShapeId, usize, Point, tracedraw_core::geometry::Affine)> = Vec::new();
        for (id, i) in self.node_selection.clone() {
            let Some((path, _, t)) = self.path_of(id) else {
                continue;
            };
            if let Some(n) = nodes::nodes(&path).into_iter().find(|n| n.index == i) {
                picked.push((id, i, t * n.pos, t));
            }
        }
        if picked.len() < 2 {
            return false;
        }
        let xs = picked.iter().map(|p| p.2.x);
        let ys = picked.iter().map(|p| p.2.y);
        let (x0, x1) = (
            xs.clone().fold(f64::MAX, f64::min),
            xs.fold(f64::MIN, f64::max),
        );
        let (y0, y1) = (
            ys.clone().fold(f64::MAX, f64::min),
            ys.fold(f64::MIN, f64::max),
        );
        let target = |p: Point| match how {
            Align::Left => Point::new(x0, p.y),
            Align::Right => Point::new(x1, p.y),
            Align::Top => Point::new(p.x, y1),
            Align::Bottom => Point::new(p.x, y0),
            Align::CenterV => Point::new((x0 + x1) / 2.0, p.y),
            Align::CenterH => Point::new(p.x, (y0 + y1) / 2.0),
            Align::CenterPage => p,
        };
        let mut paths: std::collections::BTreeMap<ShapeId, (BezPath, bool)> = Default::default();
        for (id, i, at, t) in picked {
            let to = target(at);
            let inv = t.inverse();
            let d: Vec2 = inv * to - inv * at;
            if d.hypot() < 1e-12 {
                continue;
            }
            let entry = match paths.entry(id) {
                std::collections::btree_map::Entry::Occupied(e) => e.into_mut(),
                std::collections::btree_map::Entry::Vacant(e) => {
                    let Some((path, closed, _)) = self.path_of(id) else {
                        continue;
                    };
                    e.insert((path, closed))
                }
            };
            entry.0 = nodes::move_node(&entry.0, i, d);
        }
        let cmds: Vec<Command> = paths
            .into_iter()
            .map(|(id, (path, closed))| Command::SetShapeKind {
                shape: id,
                kind: ShapeKind::Path { path, closed },
            })
            .collect();
        if !cmds.is_empty() {
            let _ = self.engine.run_batch("Align Nodes", &cmds);
        }
        true
    }

    /// Choose control handle `key`, with the nodes chosen now (#2819).
    fn choose_handle(&mut self, key: (ShapeId, usize, Which)) {
        self.chosen_handle = Some((key, self.node_selection.clone()));
    }

    /// Where control handle `key` is, on the page.
    fn handle_point(&self, (id, index, which): (ShapeId, usize, Which)) -> Option<Point> {
        let (path, _, t) = self.path_of(id)?;
        let n = nodes::nodes(&path).into_iter().find(|n| n.index == index)?;
        let c = match which {
            Which::In => n.ctrl_in,
            Which::Out => n.ctrl_out,
        }?;
        Some(t * c)
    }

    /// The control handle chosen by a click or a drag on it, while the
    /// Shape tool is on and the nodes chosen then still are (#2819):
    /// the arrow keys move it.
    pub(crate) fn chosen_handle(&self) -> Option<(ShapeId, usize, Which)> {
        let (key, chosen_nodes) = self.chosen_handle.as_ref()?;
        (self.tool == crate::tools::Tool::Shape && *chosen_nodes == self.node_selection)
            .then_some(*key)
            .filter(|k| self.handle_point(*k).is_some())
    }

    /// The chosen control handle's node and tip, on the page: the ring
    /// goes round the handle's head, which points from the node to the
    /// tip (#3041).
    /// While a handle is dragged the ring stays on it, as long as the
    /// button is held, and goes when it is let go (#3141).
    pub(crate) fn chosen_handle_ends(&self) -> Option<(Point, Point)> {
        let dragged = match self.drag {
            Drag::Handle {
                shape,
                index,
                which,
            } if self.tool == crate::tools::Tool::Shape => Some((shape, index, which)),
            _ => None,
        };
        let key = dragged.or_else(|| self.chosen_handle())?;
        let (path, _, t) = self.path_of(key.0)?;
        let n = nodes::nodes(&path).into_iter().find(|n| n.index == key.1)?;
        Some((t * n.pos, self.handle_point(key)?))
    }

    /// Move the chosen control handle by `d` (page millimetres), one undo
    /// step: the arrow keys with the Shape tool. A smooth or symmetrical
    /// node's other handle follows, as in a drag. False when no handle
    /// is chosen (the chosen nodes move instead).
    pub fn nudge_chosen_handle(&mut self, d: Vec2) -> bool {
        let Some(key) = self.chosen_handle() else {
            return false;
        };
        let (id, index, which) = key;
        let (Some((path, closed, t)), Some(at)) = (self.path_of(id), self.handle_point(key)) else {
            return false;
        };
        let lp = t.inverse() * (at + d);
        let ty = self.node_type_of(id, index);
        let np = nodes::move_handle(&path, index, which, lp, ty);
        let _ = self.engine.run_with_label(
            &Command::SetShapeKind {
                shape: id,
                kind: ShapeKind::Path { path: np, closed },
            },
            "Nudge Handle",
        );
        true
    }

    /// The segment chosen by a click on it, as (curve, element drawing
    /// it): the spot clicked is kept and its end node is the only node
    /// chosen.
    pub(crate) fn chosen_segment(&self) -> Option<(ShapeId, usize)> {
        let (shape, index, _) = self.segment_spot?;
        (self.node_selection == [(shape, index)]).then_some((shape, index))
    }

    /// Delete the chosen segment: an open curve splits there (or loses an
    /// end), a closed one opens there; a curve left with no segment goes.
    /// One undo step. False when no segment is chosen.
    pub fn delete_chosen_segment(&mut self) -> bool {
        let Some((id, index)) = self.chosen_segment() else {
            return false;
        };
        let Some((path, closed, _)) = self.path_of(id) else {
            return false;
        };
        let left = nodes::delete_segment(&path, index);
        if left == path {
            return false;
        }
        let drawn = left
            .elements()
            .iter()
            .any(|e| !matches!(e, PathEl::MoveTo(_) | PathEl::ClosePath));
        let cmd = if drawn {
            let closed = closed
                && left
                    .elements()
                    .iter()
                    .any(|e| matches!(e, PathEl::ClosePath));
            Command::SetShapeKind {
                shape: id,
                kind: ShapeKind::Path { path: left, closed },
            }
        } else {
            Command::DeleteShapes { shapes: vec![id] }
        };
        if let Err(e) = self.engine.run_with_label(&cmd, "Delete Segment") {
            self.status = e.to_string();
        }
        self.node_selection.clear();
        self.segment_spot = None;
        true
    }

    pub fn delete_selected_nodes(&mut self) {
        // B-splines lose their chosen control points (a spline left with
        // fewer than two goes); curves their chosen nodes.
        let mut cmds = Vec::new();
        for s in self.shape_tool_shapes() {
            if s.spline.is_none() {
                continue;
            }
            let chosen: Vec<usize> = self
                .node_selection
                .iter()
                .filter(|(id, _)| *id == s.id)
                .map(|(_, i)| *i)
                .collect();
            if chosen.is_empty() {
                continue;
            }
            cmds.push(match crate::kind_nodes::spline_without_many(&s, &chosen) {
                Some(points) => Command::SetSpline {
                    shape: s.id,
                    fixed: crate::kind_nodes::spline_fixed_for(&s, &points),
                    points,
                },
                None => Command::DeleteShapes { shapes: vec![s.id] },
            });
        }
        if !cmds.is_empty() {
            if let Err(e) = self.engine.run_batch("Delete Nodes", &cmds) {
                self.status = e.to_string();
            }
            self.engine.document().retain_existing(&mut self.selection);
        }
        // The chosen nodes of a curve go together: neighbouring ones
        // become one curve keeping the outline's shape (#675).
        let mut by_shape: std::collections::BTreeMap<ShapeId, Vec<usize>> = Default::default();
        for &(id, i) in &self.node_selection {
            by_shape.entry(id).or_default().push(i);
        }
        let mut cmds = Vec::new();
        for (id, idxs) in by_shape {
            let Some((path, closed, _)) = self.path_of(id) else {
                continue;
            };
            cmds.push(Command::SetShapeKind {
                shape: id,
                kind: ShapeKind::Path {
                    path: nodes::delete_nodes(&path, &idxs),
                    closed,
                },
            });
        }
        if !cmds.is_empty() {
            let _ = self.engine.run_batch("Delete Nodes", &cmds);
        }
        self.node_selection.clear();
    }

    pub fn add_node_midpoints(&mut self) {
        // A spot clicked on the chosen segment: the node goes there.
        if let Some((shape, index, t)) = self.segment_spot.take() {
            if self.node_selection == [(shape, index)] {
                if let Some((path, closed, _)) = self.path_of(shape) {
                    let np = nodes::add_node(&path, index, t);
                    self.set_path_new_step(shape, np, closed);
                    self.node_selection = vec![(shape, index)];
                }
                return;
            }
        }
        // Add a node in the middle of the segment ending at each selected
        // node. The nodes stay chosen with the new ones, so the next press
        // halves every one of those segments again (#1326).
        let mut by_shape: std::collections::BTreeMap<ShapeId, Vec<usize>> = Default::default();
        for (id, i) in self.node_selection.clone() {
            by_shape.entry(id).or_default().push(i);
        }
        let mut cmds = Vec::new();
        let mut chosen = Vec::new();
        for (id, mut idxs) in by_shape {
            let Some((mut path, closed, _)) = self.path_of(id) else {
                chosen.extend(idxs.into_iter().map(|i| (id, i)));
                continue;
            };
            path = nodes::normalize(&path);
            idxs.sort_unstable();
            idxs.dedup();
            // The segments ending at the nodes (the closing one for the
            // start of a closed curve), the last first: a node added moves
            // those after it on.
            let mut segments: Vec<usize> = idxs
                .iter()
                .filter_map(|i| nodes::segment_into(&path, *i))
                .collect();
            segments.sort_unstable_by(|a, b| b.cmp(a));
            segments.dedup();
            let mut picked = idxs;
            for k in segments {
                let before = path.elements().len();
                path = nodes::add_node(&path, k, 0.5);
                if path.elements().len() == before + 1 {
                    for p in picked.iter_mut().filter(|p| **p >= k) {
                        *p += 1;
                    }
                    // The new node takes the segment's place.
                    picked.push(k);
                }
            }
            picked.sort_unstable();
            chosen.extend(picked.into_iter().map(|i| (id, i)));
            cmds.push(Command::SetShapeKind {
                shape: id,
                kind: ShapeKind::Path { path, closed },
            });
        }
        if !cmds.is_empty() {
            let _ = self.engine.run_batch("Add Nodes", &cmds);
        }
        self.node_selection = chosen;
    }

    /// Make the chosen nodes `ty`: their handles are aligned (and of equal
    /// length) as the type needs, and the type is kept with the curve, so
    /// a cusp node with aligned handles stays cusp. One undo step.
    pub fn set_selected_node_type(&mut self, ty: NodeType) {
        // A B-spline's chosen control points: Cusp makes them fixed (the
        // curve passes through them), Smooth and Symmetrical floating
        // (#1758); its outline stays a B-spline.
        if self.set_selected_spline_fixed(ty == NodeType::Cusp) {
            return;
        }
        let mut by_shape: std::collections::BTreeMap<ShapeId, Vec<usize>> = Default::default();
        for (id, i) in self.node_selection.clone() {
            by_shape.entry(id).or_default().push(i);
        }
        let mut cmds = Vec::new();
        for (id, idxs) in by_shape {
            let Some((mut path, closed, _)) = self.path_of(id) else {
                continue;
            };
            let mut types = self
                .doc()
                .shape(id)
                .map(|(_, s)| s.node_types.clone())
                .unwrap_or_default();
            // Highest index first: a closing line made a curve puts an
            // element in before its close (#1618), which moves only the
            // elements after it, so the nodes still to do keep their
            // indices; the types and chosen nodes past it move up by one.
            let mut idxs = idxs;
            idxs.sort_unstable_by(|a, b| b.cmp(a));
            idxs.dedup();
            for i in idxs {
                let before = path.clone();
                path = nodes::set_node_type(&path, i, ty);
                if let Some(at) = nodes::curve_put_in_before_close(&before, &path) {
                    let up = |k: usize| if k >= at { k + 1 } else { k };
                    types = types.into_iter().map(|(k, t)| (up(k), t)).collect();
                    for (sid, k) in self.node_selection.iter_mut() {
                        if *sid == id {
                            *k = up(*k);
                        }
                    }
                }
                types.insert(i, ty);
            }
            cmds.push(Command::SetShapeKind {
                shape: id,
                kind: ShapeKind::Path { path, closed },
            });
            cmds.push(Command::SetNodeTypes { shape: id, types });
        }
        if !cmds.is_empty() {
            let _ = self.engine.run_batch("Node Type", &cmds);
        }
    }

    /// The chosen control points of the selected B-splines become fixed
    /// (the curve passes through them) or floating, as one undo step;
    /// false when no control point of a B-spline is chosen (#1758).
    pub fn set_selected_spline_fixed(&mut self, fixed: bool) -> bool {
        let mut cmds = Vec::new();
        for s in self.shape_tool_shapes() {
            let Some(points) = s.spline.clone() else {
                continue;
            };
            let chosen: Vec<usize> = self
                .node_selection
                .iter()
                .filter(|(id, i)| *id == s.id && *i < points.len())
                .map(|(_, i)| *i)
                .collect();
            if chosen.is_empty() {
                continue;
            }
            let mut flags = s.spline_fixed.clone();
            flags.resize(points.len(), false);
            for i in chosen {
                flags[i] = fixed;
            }
            cmds.push(Command::SetSpline {
                shape: s.id,
                points,
                fixed: Some(flags),
            });
        }
        if cmds.is_empty() {
            return false;
        }
        let label = if fixed {
            "Fixed Control Point"
        } else {
            "Floating Control Point"
        };
        if let Err(e) = self.engine.run_batch(label, &cmds) {
            self.status = e.to_string();
        }
        true
    }

    /// Whether a control point of a selected B-spline is chosen, and
    /// whether all chosen ones are fixed.
    pub fn chosen_spline_points(&self) -> Option<bool> {
        let mut any = false;
        let mut all_fixed = true;
        for s in self.shape_tool_shapes() {
            if s.spline.is_none() {
                continue;
            }
            for (id, i) in &self.node_selection {
                if *id == s.id {
                    any = true;
                    all_fixed &= s.spline_fixed.get(*i).copied().unwrap_or(false);
                }
            }
        }
        any.then_some(all_fixed)
    }

    /// The type of node `index` of curve `id`: as chosen for it, else as
    /// its handles read.
    pub fn node_type_of(&self, id: ShapeId, index: usize) -> NodeType {
        match self.doc().shape(id) {
            Ok((_, s)) => match &s.kind {
                ShapeKind::Path { path, .. } => nodes::node_type_with(path, index, &s.node_types),
                _ => NodeType::Cusp,
            },
            Err(_) => NodeType::Cusp,
        }
    }

    pub fn selected_segments_to_line(&mut self) {
        self.for_each_selected_node("To Line", nodes::to_line);
    }

    pub fn selected_segments_to_curve(&mut self) {
        self.for_each_selected_node("To Curve", nodes::to_curve);
    }

    /// Bulge: the segments ending at the chosen nodes bow into arcs by
    /// the bar's percentage, outward on a closed curve, as a rectangle's
    /// Edge Bulge bows its sides; one undo step (#3335).
    pub fn bulge_selected_segments(&mut self) {
        let amount = self.segment_bulge / 100.0;
        self.for_each_selected_node("Bulge", |p, i| nodes::bulge(p, i, amount));
    }

    pub fn break_selected_nodes(&mut self) {
        // All of a curve's chosen nodes at once: opening a closed subpath
        // turns it to start at the break, which moves the other nodes.
        let mut by_shape: std::collections::BTreeMap<ShapeId, Vec<usize>> = Default::default();
        for (id, i) in self.node_selection.clone() {
            by_shape.entry(id).or_default().push(i);
        }
        let cmds: Vec<Command> = by_shape
            .into_iter()
            .filter_map(|(id, idxs)| {
                let (path, closed, _) = self.path_of(id)?;
                Some(Command::SetShapeKind {
                    shape: id,
                    kind: ShapeKind::Path {
                        path: nodes::break_nodes(&path, &idxs),
                        closed,
                    },
                })
            })
            .collect();
        let spots = self.chosen_node_spots();
        if !cmds.is_empty() {
            let _ = self.engine.run_batch("Break Curve", &cmds);
        }
        self.choose_nodes_at(&spots);
    }

    pub fn close_selected_curves(&mut self) {
        let ids: Vec<ShapeId> = if self.node_selection.is_empty() {
            self.selection.clone()
        } else {
            self.node_selection.iter().map(|(id, _)| *id).collect()
        };
        let mut cmds = Vec::new();
        for id in ids {
            if let Some((path, _, _)) = self.path_of(id) {
                let np = nodes::close_subpath(&path, 0);
                cmds.push(Command::SetShapeKind {
                    shape: id,
                    kind: ShapeKind::Path {
                        path: np,
                        closed: true,
                    },
                });
            }
        }
        if !cmds.is_empty() {
            let _ = self.engine.run_batch("Close Curve", &cmds);
        }
    }

    pub fn reverse_selected_curves(&mut self) {
        let mut cmds = Vec::new();
        for id in self.selection.clone() {
            if let Some((path, closed, _)) = self.path_of(id) {
                cmds.push(Command::SetShapeKind {
                    shape: id,
                    kind: ShapeKind::Path {
                        path: nodes::reverse(&path),
                        closed,
                    },
                });
            }
        }
        let spots = self.chosen_node_spots();
        if !cmds.is_empty() {
            let _ = self.engine.run_batch("Reverse Direction", &cmds);
        }
        self.choose_nodes_at(&spots);
    }

    pub fn select_all_nodes(&mut self) {
        self.segment_spot = None;
        let mut sel = Vec::new();
        for id in &self.node_targets() {
            if let Some((path, _, _)) = self.path_of(*id) {
                for n in nodes::nodes(&path) {
                    sel.push((*id, n.index));
                }
            }
        }
        self.node_selection = sel;
    }

    /// Node type of the first selected node, for the property bar.
    pub fn current_node_type(&self) -> Option<NodeType> {
        // B-spline control points: fixed ones read as cusps (#1758).
        if let Some(all_fixed) = self.chosen_spline_points() {
            return Some(if all_fixed {
                NodeType::Cusp
            } else {
                NodeType::Smooth
            });
        }
        let (id, i) = *self.node_selection.first()?;
        self.path_of(id)?;
        Some(self.node_type_of(id, i))
    }
}

impl App {
    /// Where the chosen nodes sit, in their curves' own coordinates, to
    /// choose them again after an edit with [`App::choose_nodes_at`].
    pub(crate) fn chosen_node_spots(&self) -> Vec<(ShapeId, Point)> {
        let mut out = Vec::new();
        for &(id, i) in &self.node_selection {
            let Some((path, _, _)) = self.path_of(id) else {
                continue;
            };
            if let Some(n) = nodes::nodes(&path).into_iter().find(|n| n.index == i) {
                out.push((id, n.pos));
            }
        }
        out
    }

    /// After a node edit, choose the nodes now sitting at `spots`, so the
    /// result stays chosen (#2358): the joined node after
    /// Join, both ends after Break, the nodes left after Reduce Nodes.
    pub(crate) fn choose_nodes_at(&mut self, spots: &[(ShapeId, Point)]) {
        let mut sel = Vec::new();
        for &(id, at) in spots {
            let Some((path, _, _)) = self.path_of(id) else {
                continue;
            };
            for n in nodes::nodes(&path) {
                if (n.pos - at).hypot() <= 1e-6 && !sel.contains(&(id, n.index)) {
                    sel.push((id, n.index));
                }
            }
        }
        self.node_selection = sel;
    }

    /// Join two selected end nodes of one curve. Nodes of two objects
    /// are not joined: the objects must be combined first (#1007).
    pub fn join_selected_nodes(&mut self) {
        if self.node_selection.len() != 2 {
            return;
        }
        let (a, b) = (self.node_selection[0], self.node_selection[1]);
        if a.0 != b.0 {
            self.status = tr("status.join_other_object");
            return;
        }
        let spots = self.chosen_node_spots();
        if self.join_nodes(a, b) {
            self.choose_joined_node(&spots);
        }
    }

    /// Choose the node two end nodes at `spots` were joined into: it sits
    /// halfway between them (#2358).
    fn choose_joined_node(&mut self, spots: &[(ShapeId, Point)]) {
        match spots {
            [(id, a), (_, b)] => self.choose_nodes_at(&[(*id, a.lerp(*b, 0.5))]),
            _ => self.node_selection.clear(),
        }
    }

    /// Join end node `a` with end node `b` of the same curve into one node
    /// halfway between them, with no segment added: the ends of one open
    /// subpath close it, ends of two subpaths make one subpath. One undo
    /// step (Join Nodes). False, and nothing changes, when either is not an
    /// end of an open subpath or the two are on different objects (those
    /// are combined first, #1007).
    pub(crate) fn join_nodes(&mut self, a: (ShapeId, usize), b: (ShapeId, usize)) -> bool {
        let (sa, ia) = a;
        let (sb, ib) = b;
        if sa != sb {
            return false;
        }
        let Some((path, _, _)) = self.path_of(sa) else {
            return false;
        };
        let Some(joined) = nodes::join_ends(&path, ia, ib) else {
            return false;
        };
        let closed = every_subpath_closed(&joined);
        let cmd = Command::SetShapeKind {
            shape: sa,
            kind: ShapeKind::Path {
                path: joined,
                closed,
            },
        };
        if let Err(e) = self.engine.run_batch("Join Nodes", &[cmd]) {
            self.status = e.to_string();
            return false;
        }
        true
    }

    /// After dragging one end node: dropped on another end node of the
    /// same curve (within the pick distance), the two are joined; an end
    /// node of another object only snaps there (#1007).
    pub(crate) fn join_dropped_end_node(&mut self) {
        let [(id, index)] = self.node_selection[..] else {
            return;
        };
        let Some((path, _, t)) = self.path_of(id) else {
            return;
        };
        let Some(at) = nodes::nodes(&path)
            .into_iter()
            .find(|n| n.index == index)
            .map(|n| t * n.pos)
        else {
            return;
        };
        let tol = (self.settings.nodes.pick_px() + 1.0) / self.view.zoom as f64;
        let mut near: Vec<(usize, f64)> = nodes::nodes(&path)
            .into_iter()
            .filter(|n| n.index != index)
            .map(|n| (n.index, (t * n.pos - at).hypot()))
            .filter(|(_, d)| *d <= tol)
            .collect();
        near.sort_by(|a, b| a.1.total_cmp(&b.1));
        // The nearest end node of an open subpath.
        for (target, _) in near {
            let Some(other) = nodes::nodes(&path).into_iter().find(|n| n.index == target) else {
                continue;
            };
            let spots = [(id, other.pos), (id, t.inverse() * at)];
            if self.join_nodes((id, target), (id, index)) {
                self.choose_joined_node(&spots);
                return;
            }
        }
    }

    /// Extract the subpath holding the first selected node into its own object.
    pub fn extract_subpath(&mut self) {
        let Some(&(id, index)) = self.node_selection.first() else {
            return;
        };
        let Some((path, closed, _)) = self.path_of(id) else {
            return;
        };
        // Find the subpath boundaries around `index`.
        let els = path.elements();
        let mut start = 0;
        for (i, el) in els.iter().enumerate() {
            if i > index {
                break;
            }
            if matches!(el, tracedraw_core::geometry::PathEl::MoveTo(_)) {
                start = i;
            }
        }
        let mut end = els.len();
        for (i, el) in els.iter().enumerate().skip(start + 1) {
            if matches!(el, tracedraw_core::geometry::PathEl::MoveTo(_)) {
                end = i;
                break;
            }
        }
        if start == 0 && end == els.len() {
            return; // single subpath, nothing to extract
        }
        let mut extracted = tracedraw_core::BezPath::new();
        for el in &els[start..end] {
            extracted.push(*el);
        }
        let mut rest = tracedraw_core::BezPath::new();
        for el in els[..start].iter().chain(els[end..].iter()) {
            rest.push(*el);
        }
        let Ok((layer, src)) = self.doc().shape(id) else {
            return;
        };
        let layer = layer.id;
        let mut new_shape = src.clone();
        new_shape.id = self.engine.new_shape_id();
        new_shape.kind = ShapeKind::Path {
            path: extracted,
            closed,
        };
        let new_id = new_shape.id;
        let cmds = vec![
            Command::SetShapeKind {
                shape: id,
                kind: ShapeKind::Path { path: rest, closed },
            },
            Command::AddShape {
                layer,
                shape: new_shape,
            },
        ];
        let _ = self.engine.run_batch("Extract Subpath", &cmds);
        self.node_selection.clear();
        self.selection = vec![new_id];
    }

    /// Object > Align with Pixel Grid: move the selection so its bounds
    /// sit on whole 96 dpi pixels.
    pub fn align_to_pixel_grid(&mut self) {
        let ids: Vec<ShapeId> = self.selection.clone();
        self.align_shapes_to_pixel_grid(&ids);
    }

    /// Snap the bottom-left corner of each object's bounds to the 96 dpi grid.
    pub fn align_shapes_to_pixel_grid(&mut self, ids: &[ShapeId]) {
        let px = 25.4 / 96.0;
        for id in ids {
            let Some(s) = self.doc().find_shape(*id).cloned() else {
                continue;
            };
            let b = s.bounds();
            let dx = (b.x0 / px).round() * px - b.x0;
            let dy = (b.y0 / px).round() * px - b.y0;
            if dx.abs() > 1e-9 || dy.abs() > 1e-9 {
                self.run(Command::TransformShapes {
                    shapes: vec![s.id],
                    transform: tracedraw_core::Affine::translate((dx, dy)),
                });
            }
        }
    }

    /// Object > Object Hinting: objects flagged for hinting stay on the pixel
    /// grid (they are re-aligned after every move).
    pub fn object_hinted(&self, id: ShapeId) -> bool {
        self.doc()
            .find_shape(id)
            .map(|s| s.data.iter().any(|(k, v)| k == "hinting" && v == "1"))
            .unwrap_or(false)
    }

    pub fn toggle_object_hinting(&mut self) {
        let ids = self.selection.clone();
        let all_on = ids.iter().all(|id| self.object_hinted(*id));
        for id in &ids {
            let Some(s) = self.doc().find_shape(*id).cloned() else {
                continue;
            };
            let mut data = s.data;
            data.retain(|(k, _)| k != "hinting");
            if !all_on {
                data.push(("hinting".into(), "1".into()));
            }
            self.run(Command::SetObjectData { shape: *id, data });
        }
        if !all_on {
            self.align_shapes_to_pixel_grid(&ids);
        }
    }

    /// Re-align hinted objects among `ids` (after a transform).
    pub fn apply_hinting(&mut self, ids: &[ShapeId]) {
        let hinted: Vec<ShapeId> = ids
            .iter()
            .copied()
            .filter(|id| self.object_hinted(*id))
            .collect();
        if !hinted.is_empty() {
            self.align_shapes_to_pixel_grid(&hinted);
        }
    }
}

// ----- Eraser ----------------------------------------------------------------------

impl App {
    /// The band the Eraser takes out along `points`: its width all along,
    /// or, dragged with a pen and the round nib, as wide as the pen's
    /// pressure gives at each place (#883), from a fifth of the width at
    /// the lightest touch.
    pub fn eraser_band(&self, points: &[Point]) -> tracedraw_core::geometry::BezPath {
        // The hand's shake, about two screen pixels, is smoothed away
        // so the edge left is smooth and has few nodes (#2831).
        let steady = 2.0 / f64::from(self.view.zoom.max(1e-3));
        let even = || {
            tracedraw_core::shaping::eraser_band(
                points,
                self.eraser_width,
                self.eraser_square,
                steady,
            )
        };
        if self.eraser_square || points.len() < 2 {
            return even();
        }
        let Some(pen) = self
            .pen_pressures(points)
            .and_then(|pr| crate::pen::PressureProfile::new(points, pr))
        else {
            return even();
        };
        let mut line = tracedraw_core::geometry::BezPath::new();
        line.move_to(points[0]);
        for p in &points[1..] {
            line.line_to(*p);
        }
        let mut stroke =
            tracedraw_core::Stroke::new(tracedraw_core::Color::BLACK, self.eraser_width);
        stroke.cap = tracedraw_core::style::LineCap::Round;
        stroke.widths = pen.width_nodes(
            crate::pen::FREEHAND_NODES,
            self.eraser_width,
            crate::brushes::PEN_NIB_FLOOR,
            &|_| 1.0,
        );
        tracedraw_core::shaping::outline_band(&line, &stroke, 1.0)
            .filter(|b| !b.elements().is_empty())
            .unwrap_or_else(even)
    }

    /// Eraser: subtract the nib's band along `points` (page space) from
    /// the selected objects, or from the object under the first point.
    /// Objects become curves; what vanishes entirely is deleted.
    pub fn erase_along(&mut self, points: &[Point]) {
        use tracedraw_core::document::Shape;
        use tracedraw_core::geometry::Shape as _;
        let Some(first) = points.first().copied() else {
            return;
        };
        let band = self.eraser_band(points);
        if band.elements().is_empty() {
            return;
        }
        let targets: Vec<Shape> = if self.selection.is_empty() {
            self.hit_test(first)
                .and_then(|id| self.doc().find_shape(id).cloned())
                .into_iter()
                .collect()
        } else {
            self.selected_shapes()
        };
        let mut cmds = Vec::new();
        let mut deleted = Vec::new();
        for s in targets {
            if let ShapeKind::ClipFrame { frame, contents } = &s.kind {
                // A clip frame is erased through its frame (#1672), as Crop
                // and the Knife take it: the frame loses the band and the
                // contents stay inside, clipped by what is left of it.
                match erase_frame(&s, frame, &band) {
                    Erased::Keep => {}
                    Erased::Gone => deleted.push(s.id),
                    Erased::Into(f) => cmds.push(Command::SetShapeKind {
                        shape: s.id,
                        kind: ShapeKind::ClipFrame {
                            frame: f,
                            contents: contents.clone(),
                        },
                    }),
                }
                continue;
            }
            if let ShapeKind::Bitmap {
                rect,
                width_px,
                height_px,
                fx,
                png,
            } = &s.kind
            {
                // A picture loses its pixels along the nib, the edge
                // fading out by the bar's hardness and strength (#3043).
                let nib = SoftNib {
                    radius: self.eraser_width / 2.0,
                    hardness: self.eraser_hardness,
                    strength: self.eraser_strength,
                };
                if let Some(png) = erase_picture(&s, png.as_slice(), points, nib) {
                    cmds.push(Command::SetShapeKind {
                        shape: s.id,
                        kind: ShapeKind::Bitmap {
                            rect: *rect,
                            width_px: *width_px,
                            height_px: *height_px,
                            png,
                            fx: fx.clone(),
                        },
                    });
                }
                continue;
            }
            if matches!(s.kind, ShapeKind::Group { .. }) {
                continue;
            }
            let page_path = s.page_path();
            // Quick reject: the band must touch the object's bounds.
            if page_path
                .bounding_box()
                .intersect(band.bounding_box())
                .area()
                <= 0.0
            {
                continue;
            }
            let remaining = tracedraw_core::shaping::overlay(
                &page_path,
                &band,
                tracedraw_core::shaping::Op::Trim,
            );
            if remaining.elements().is_empty() {
                deleted.push(s.id);
                continue;
            }
            // Auto-reduce nodes (Options > Tools > Eraser, #1436): the
            // nodes the nib's edge left that the shape does not need go,
            // the curve staying within 2% of the nib's width.
            let remaining = if self.settings.eraser_reduce_nodes {
                tracedraw_core::nodes::reduce_nodes(
                    &remaining,
                    (self.eraser_width * 0.02).max(0.005),
                )
            } else {
                remaining
            };
            let local = s.transform.inverse() * remaining;
            cmds.push(Command::SetShapeKind {
                shape: s.id,
                kind: ShapeKind::Path {
                    path: local,
                    closed: true,
                },
            });
        }
        if !deleted.is_empty() {
            cmds.push(Command::DeleteShapes {
                shapes: deleted.clone(),
            });
            self.selection.retain(|id| !deleted.contains(id));
        }
        if !cmds.is_empty() {
            let _ = self.engine.run_batch("Eraser", &cmds);
        }
    }
}

/// The Eraser's nib on a picture (#3043): `radius` mm, erased fully
/// within `hardness` of it and fading out to its edge, `strength` the
/// most it takes of a pixel.
#[derive(Debug, Clone, Copy)]
pub struct SoftNib {
    pub radius: f64,
    pub hardness: f64,
    pub strength: f64,
}

impl SoftNib {
    /// How much is erased at `d` mm from the nib's middle line (0 to 1).
    pub fn amount(&self, d: f64) -> f64 {
        let r = self.radius.max(1e-6);
        if d >= r {
            return 0.0;
        }
        let core = r * self.hardness.clamp(0.0, 1.0);
        let k = if d <= core {
            1.0
        } else {
            let t = ((r - d) / (r - core).max(1e-9)).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        };
        k * self.strength.clamp(0.0, 1.0)
    }
}

/// The picture `s` (its pixels `png`, PNG or JPEG) with the pixels along
/// `points` (page space) erased by `nib`: their alpha goes down by the
/// nib's amount at their middles' distance from the line. `None` when
/// the line misses the picture or it cannot be read.
pub fn erase_picture(
    s: &tracedraw_core::document::Shape,
    png: &[u8],
    points: &[Point],
    nib: SoftNib,
) -> Option<tracedraw_core::SharedBytes> {
    let ShapeKind::Bitmap {
        rect,
        width_px,
        height_px,
        ..
    } = &s.kind
    else {
        return None;
    };
    let (w, h) = (*width_px as usize, *height_px as usize);
    if w == 0 || h == 0 || rect.width() <= 0.0 || rect.height() <= 0.0 || points.is_empty() {
        return None;
    }
    // Pixel (x, y) (its middle at x + 0.5, y + 0.5; row 0 at the top) to
    // the page, and back.
    let to_page = s.transform
        * tracedraw_core::geometry::Affine::new([
            rect.width() / w as f64,
            0.0,
            0.0,
            -rect.height() / h as f64,
            rect.x0,
            rect.y1,
        ]);
    if to_page.determinant().abs() < 1e-18 {
        return None;
    }
    let to_px = to_page.inverse();
    let r = nib.radius.max(1e-6);
    let mut segs: Vec<(Point, Point)> = points.windows(2).map(|w| (w[0], w[1])).collect();
    if segs.is_empty() {
        segs.push((points[0], points[0]));
    }
    let mut amount: Option<Vec<f32>> = None;
    for (a, b) in segs {
        let lo = Point::new(a.x.min(b.x) - r, a.y.min(b.y) - r);
        let hi = Point::new(a.x.max(b.x) + r, a.y.max(b.y) + r);
        let corners = [
            to_px * lo,
            to_px * hi,
            to_px * Point::new(lo.x, hi.y),
            to_px * Point::new(hi.x, lo.y),
        ];
        let x0 = corners
            .iter()
            .map(|c| c.x)
            .fold(f64::INFINITY, f64::min)
            .floor()
            .max(0.0);
        let x1 = corners
            .iter()
            .map(|c| c.x)
            .fold(f64::NEG_INFINITY, f64::max)
            .ceil()
            .min(w as f64);
        let y0 = corners
            .iter()
            .map(|c| c.y)
            .fold(f64::INFINITY, f64::min)
            .floor()
            .max(0.0);
        let y1 = corners
            .iter()
            .map(|c| c.y)
            .fold(f64::NEG_INFINITY, f64::max)
            .ceil()
            .min(h as f64);
        if x0 >= x1 || y0 >= y1 {
            continue;
        }
        let buf = amount.get_or_insert_with(|| vec![0.0; w * h]);
        let ab = b - a;
        let len2 = ab.hypot2();
        for y in y0 as usize..y1 as usize {
            for x in x0 as usize..x1 as usize {
                let p = to_page * Point::new(x as f64 + 0.5, y as f64 + 0.5);
                let t = if len2 > 1e-18 {
                    ((p - a).dot(ab) / len2).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let d = (p - (a + ab * t)).hypot();
                let k = nib.amount(d) as f32;
                let cell = &mut buf[y * w + x];
                if k > *cell {
                    *cell = k;
                }
            }
        }
    }
    let amount = amount?;
    if amount.iter().all(|k| *k <= 0.0) {
        return None;
    }
    let mut img = image::load_from_memory(png).ok()?.to_rgba8();
    if img.width() as usize != w || img.height() as usize != h {
        img = image::imageops::resize(
            &img,
            w as u32,
            h as u32,
            image::imageops::FilterType::Triangle,
        );
    }
    for (i, px) in img.pixels_mut().enumerate() {
        let k = amount[i];
        if k > 0.0 {
            px[3] = (px[3] as f32 * (1.0 - k)).round().clamp(0.0, 255.0) as u8;
        }
    }
    crate::bitmap_fx::encode(&img).map(tracedraw_core::SharedBytes::from)
}

/// What the Eraser leaves of a clip frame's frame.
enum Erased {
    /// The band misses the frame (or the frame cannot be erased).
    Keep,
    /// Nothing of the frame is left.
    Gone,
    /// The frame that is left, in the clip frame's space.
    Into(Box<tracedraw_core::document::Shape>),
}

/// Subtract `band` (page space) from the frame of the clip frame `clip`:
/// the frame becomes a closed curve with its own fill, outline and id.
/// A frame that is a group, a bitmap or a clip frame is left alone.
fn erase_frame(
    clip: &tracedraw_core::document::Shape,
    frame: &tracedraw_core::document::Shape,
    band: &tracedraw_core::geometry::BezPath,
) -> Erased {
    use tracedraw_core::geometry::Shape as _;
    if matches!(
        frame.kind,
        ShapeKind::Bitmap { .. } | ShapeKind::Group { .. } | ShapeKind::ClipFrame { .. }
    ) {
        return Erased::Keep;
    }
    let to_page = clip.transform * frame.transform;
    if to_page.determinant().abs() < 1e-12 {
        return Erased::Keep;
    }
    let page_path = to_page * frame.local_path();
    if page_path
        .bounding_box()
        .intersect(band.bounding_box())
        .area()
        <= 0.0
    {
        return Erased::Keep;
    }
    let remaining =
        tracedraw_core::shaping::overlay(&page_path, band, tracedraw_core::shaping::Op::Trim);
    if remaining.elements().is_empty() {
        return Erased::Gone;
    }
    let mut f = frame.clone();
    f.transform = tracedraw_core::geometry::Affine::IDENTITY;
    f.kind = ShapeKind::Path {
        path: clip.transform.inverse() * remaining,
        closed: true,
    };
    f.node_types.clear();
    f.spline = None;
    f.spline_fixed.clear();
    f.common_shape = None;
    Erased::Into(Box::new(f))
}

/// Whether the curve `s` shows a common shape's handles instead of its
/// nodes (#4045).
pub(crate) fn shows_handles(s: &Shape) -> bool {
    s.common_shape.is_some() && crate::common_shapes::live(s).is_some()
}

#[cfg(test)]
mod constrain_tests {
    use super::constrain_axis;
    use crate::app::App;
    use tracedraw_core::{document::Guide, geometry::Point, geometry::Vec2, Command};

    /// A headless app at 1 px per mm with a guideline at x = 50 and the
    /// snapping radius at 10 px (10 mm).
    fn app_with_guide() -> App {
        let mut app = App::headless();
        app.view.zoom = 1.0;
        let page = app.page;
        app.run(Command::AddGuide {
            page,
            guide: Guide::vertical(50.0),
        });
        app
    }

    #[test]
    fn a_grabbed_node_snaps_to_a_guideline_and_the_move_follows_it() {
        let app = app_with_guide();
        // The node at x = 45 moved by 4.5 mm lands at 49.5, which snaps to 50.
        let d = app.snapped_node_move(Point::new(45.0, 20.0), Vec2::new(4.5, 0.0), None, &[], &[]);
        assert!((d.x - 5.0).abs() < 1e-9, "{d:?}");
        assert!(d.y.abs() < 1e-9, "{d:?}");
    }

    #[test]
    fn a_constrained_node_move_stays_on_its_axis_after_snapping() {
        let app = app_with_guide();
        // Horizontal: the guideline pulls x, y stays put.
        let d = app.snapped_node_move(
            Point::new(45.0, 20.0),
            Vec2::new(4.5, 3.0),
            Some(true),
            &[],
            &[],
        );
        assert!((d.x - 5.0).abs() < 1e-9 && d.y == 0.0, "{d:?}");
    }

    #[test]
    fn a_grabbed_node_snaps_to_the_document_grid() {
        let mut app = App::headless();
        app.view.zoom = 1.0;
        app.snap.grid = true;
        app.snap.guides = false;
        app.snap.objects = false;
        app.snap.page = false;
        assert_eq!(app.doc().metadata.grid.spacing_x, 10.0);
        let origin = app.doc().metadata.rulers.origin_x;
        let grab = Point::new(origin + 31.0, 20.0);
        let d = app.snapped_node_move(grab, Vec2::new(7.0, 0.0), None, &[], &[]);
        assert!((grab.x + d.x - (origin + 40.0)).abs() < 1e-9, "{d:?}");
    }

    #[test]
    fn a_grabbed_node_snaps_to_the_baseline_grid_alone() {
        // Only the baseline grid is on: snapping still works (#3).
        let mut app = App::headless();
        app.view.zoom = 1.0;
        app.snap.guides = false;
        app.snap.objects = false;
        app.snap.page = false;
        app.snap.baseline_grid = true;
        let y = app.baseline_ys()[2];
        let grab = Point::new(30.0, y + 6.0);
        let d = app.snapped_node_move(grab, Vec2::new(0.0, -4.0), None, &[], &[]);
        assert!((grab.y + d.y - y).abs() < 1e-9, "{d:?}");
    }

    #[test]
    fn a_constrained_move_keeps_one_axis() {
        let mut d = Vec2::new(3.0, 4.0);
        constrain_axis(&mut d, Some(true));
        assert_eq!((d.x, d.y), (3.0, 0.0));

        let mut d = Vec2::new(3.0, 4.0);
        constrain_axis(&mut d, Some(false));
        assert_eq!((d.x, d.y), (0.0, 4.0));

        let mut d = Vec2::new(3.0, 4.0);
        constrain_axis(&mut d, None);
        assert_eq!((d.x, d.y), (3.0, 4.0));
    }
}

#[cfg(test)]
mod bulge_tests {
    use super::*;
    use tracedraw_core::geometry::Rect;

    /// A rectangle converted to curves still bows (#3335): its chosen
    /// segments bulge by the bar's percentage in one undo step, its
    /// nodes staying, and 100 % all round makes the circle through them.
    #[test]
    fn a_converted_rectangle_bulges_like_a_live_one() {
        let mut app = App::headless();
        app.show_welcome = false;
        let id = app
            .new_shape(ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 20.0, 20.0),
                radius: 0.0,
                corners: None,
            })
            .unwrap();
        app.select(vec![id]);
        app.convert_to_curves();
        let before = app.doc().find_shape(id).unwrap().bounds();
        let Some((path, _, _)) = app.path_of(id) else {
            panic!("a curve");
        };
        let n = path.elements().len();
        app.node_selection = (0..n).map(|i| (id, i)).collect();
        app.segment_bulge = 100.0;
        app.bulge_selected_segments();
        assert_eq!(app.engine.undo_label(), Some("Bulge"), "one step");
        let s = app.doc().find_shape(id).unwrap();
        let b = s.bounds();
        // The circle through the corners: 20 * sqrt(2) across.
        let across = 20.0 * 2f64.sqrt();
        assert!((b.width() - across).abs() < 0.2, "{b:?} from {before:?}");
        assert!((b.height() - across).abs() < 0.2, "{b:?}");
        app.engine.undo().unwrap();
        let b = app.doc().find_shape(id).unwrap().bounds();
        assert!((b.width() - 20.0).abs() < 1e-6, "{b:?}");
    }
}

#[cfg(test)]
mod eraser_tests {
    use super::*;
    use tracedraw_core::geometry::Rect;

    #[test]
    fn a_band_through_a_rectangle_splits_it_and_a_dot_outside_does_nothing() {
        let mut app = App::headless();
        let id = app
            .new_shape(ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 40.0, 20.0),
                radius: 0.0,
                corners: None,
            })
            .unwrap();
        app.select(vec![id]);
        app.eraser_width = 4.0;
        // Vertical band down the middle: two pieces remain.
        app.erase_along(&[Point::new(20.0, -5.0), Point::new(20.0, 25.0)]);
        let s = app.doc().find_shape(id).unwrap();
        let ShapeKind::Path { path, .. } = &s.kind else {
            panic!("{:?}", s.kind);
        };
        let subpaths = path
            .elements()
            .iter()
            .filter(|e| matches!(e, tracedraw_core::geometry::PathEl::MoveTo(_)))
            .count();
        assert_eq!(subpaths, 2);
        let b = s.bounds();
        assert!(
            (b.width() - 40.0).abs() < 0.1 && (b.height() - 20.0).abs() < 0.1,
            "{b:?}"
        );
        // A dot far away leaves it alone.
        app.erase_along(&[Point::new(100.0, 100.0)]);
        assert!(app.doc().find_shape(id).is_some());
        // Erasing everything deletes the object.
        app.eraser_width = 100.0;
        app.erase_along(&[Point::new(0.0, 10.0), Point::new(40.0, 10.0)]);
        assert!(app.doc().find_shape(id).is_none());
    }

    /// A shaky drag of the Eraser across an ellipse (#2831): the cut edges
    /// are smooth and the ellipse keeps few nodes, its own curves outside
    /// the band untouched.
    #[test]
    fn a_shaky_eraser_drag_leaves_a_smooth_edge_with_few_nodes() {
        let mut app = App::headless();
        let id = app
            .new_shape(ShapeKind::Ellipse {
                rect: Rect::new(0.0, 0.0, 60.0, 40.0),
                arc: None,
            })
            .unwrap();
        app.select(vec![id]);
        app.eraser_width = 6.0;
        app.view.zoom = 4.0;
        let points: Vec<Point> = (0..=400)
            .map(|i| {
                let x = -5.0 + i as f64 * 0.175;
                Point::new(x, 20.0 + 0.2 * ((i as f64) * 1.9).sin())
            })
            .collect();
        app.erase_along(&points);
        let s = app.doc().find_shape(id).unwrap();
        let ShapeKind::Path { path, .. } = &s.kind else {
            panic!("{:?}", s.kind);
        };
        let n = path.elements().len();
        assert!(n <= 20, "{n} nodes");
        assert!(path
            .elements()
            .iter()
            .any(|e| matches!(e, tracedraw_core::geometry::PathEl::CurveTo(..))));
    }

    /// A 40 mm square clip frame holding a 30 mm square, moved 10 mm right.
    fn clip_frame(app: &mut App) -> ShapeId {
        let frame = app
            .new_shape(ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 40.0, 40.0),
                radius: 0.0,
                corners: None,
            })
            .unwrap();
        let inner = app
            .new_shape(ShapeKind::Rect {
                rect: Rect::new(5.0, 5.0, 35.0, 35.0),
                radius: 0.0,
                corners: None,
            })
            .unwrap();
        app.engine
            .run(&Command::PlaceInside {
                contents: vec![inner],
                frame,
            })
            .unwrap();
        app.engine
            .run(&Command::TransformShapes {
                shapes: vec![frame],
                transform: tracedraw_core::geometry::Affine::translate((10.0, 0.0)),
            })
            .unwrap();
        frame
    }

    #[test]
    fn a_clip_frame_is_erased_through_its_frame() {
        use tracedraw_core::geometry::Shape as _;
        let mut app = App::headless();
        let id = clip_frame(&mut app);
        let before = app.doc().find_shape(id).unwrap().clone();
        let ShapeKind::ClipFrame {
            frame: old_frame,
            contents: old_contents,
        } = &before.kind
        else {
            panic!("{:?}", before.kind);
        };
        app.select(vec![id]);
        app.eraser_width = 4.0;
        // A band down the middle of the frame (page x = 30).
        app.erase_along(&[Point::new(30.0, -5.0), Point::new(30.0, 45.0)]);
        let s = app.doc().find_shape(id).unwrap().clone();
        let ShapeKind::ClipFrame { frame, contents } = &s.kind else {
            panic!("no longer a clip frame: {:?}", s.kind);
        };
        let ShapeKind::Path { path, closed } = &frame.kind else {
            panic!("the frame was not erased: {:?}", frame.kind);
        };
        assert!(closed);
        assert_eq!(frame.id, old_frame.id, "the frame keeps its id");
        assert_eq!(frame.fill, old_frame.fill, "the frame keeps its look");
        assert_eq!(frame.stroke, old_frame.stroke);
        let subpaths = path
            .elements()
            .iter()
            .filter(|e| matches!(e, tracedraw_core::geometry::PathEl::MoveTo(_)))
            .count();
        assert_eq!(subpaths, 2);
        // On the page the frame keeps its place, with a hole where the
        // band ran: the contents, clipped by it, are gone there only.
        let page = s.transform * frame.page_path();
        let b = page.bounding_box();
        assert!(
            (b.x0 - 10.0).abs() < 0.1 && (b.x1 - 50.0).abs() < 0.1,
            "{b:?}"
        );
        assert!(!page.contains(Point::new(30.0, 20.0)));
        assert!(page.contains(Point::new(20.0, 20.0)));
        assert!(page.contains(Point::new(40.0, 20.0)));
        assert_eq!(contents, old_contents, "the contents are untouched");
        // One undo step brings the frame back whole.
        app.undo();
        assert_eq!(app.doc().find_shape(id), Some(&before));
    }

    #[test]
    fn a_clip_frame_under_the_pointer_is_erased_and_one_erased_away_goes() {
        let mut app = App::headless();
        let id = clip_frame(&mut app);
        app.select(Vec::new());
        app.eraser_width = 4.0;
        app.erase_along(&[Point::new(30.0, 20.0)]);
        let s = app.doc().find_shape(id).unwrap();
        let ShapeKind::ClipFrame { frame, .. } = &s.kind else {
            panic!("{:?}", s.kind);
        };
        assert!(matches!(frame.kind, ShapeKind::Path { .. }), "a dot erased");
        // A dot beside it leaves it alone.
        let kept = s.clone();
        app.select(vec![id]);
        app.erase_along(&[Point::new(200.0, 200.0)]);
        assert_eq!(app.doc().find_shape(id), Some(&kept));
        // Erasing the whole frame deletes the clip frame with its contents.
        app.eraser_width = 100.0;
        app.erase_along(&[Point::new(10.0, 20.0), Point::new(50.0, 20.0)]);
        assert!(app.doc().find_shape(id).is_none());
        assert!(app.selection.is_empty());
    }

    /// Options > Tools > Eraser, "Auto-reduce nodes": what a round nib
    /// leaves keeps fewer nodes (#1436).
    #[test]
    fn auto_reduce_leaves_fewer_nodes() {
        let nodes = |reduce: bool| {
            let mut app = App::headless();
            app.settings.eraser_reduce_nodes = reduce;
            let id = app
                .new_shape(ShapeKind::Rect {
                    rect: Rect::new(0.0, 0.0, 40.0, 20.0),
                    radius: 0.0,
                    corners: None,
                })
                .unwrap();
            app.select(vec![id]);
            app.eraser_width = 10.0;
            app.eraser_square = false;
            let stroke: Vec<Point> = (0..=20)
                .map(|i| Point::new(f64::from(i) * 2.0, 20.0 + (f64::from(i) * 0.7).sin() * 2.0))
                .collect();
            app.erase_along(&stroke);
            let s = app.doc().find_shape(id).unwrap();
            s.page_path().elements().len()
        };
        let (plain, reduced) = (nodes(false), nodes(true));
        assert!(reduced < plain, "{reduced} < {plain}");
    }
}

#[cfg(test)]
mod bitmap_node_tests {
    use super::*;
    use tracedraw_core::geometry::{Rect, Vec2};

    fn picture(app: &mut App) -> ShapeId {
        let png = tiny_skia::Pixmap::new(2, 2)
            .and_then(|p| p.encode_png().ok())
            .unwrap_or_default();
        app.new_shape(ShapeKind::Bitmap {
            rect: Rect::new(10.0, 10.0, 50.0, 40.0),
            width_px: 2,
            height_px: 2,
            png: png.into(),
            fx: None,
        })
        .expect("picture")
    }

    /// Trim, Intersect and Weld with a picture as the target keep the
    /// picture showing inside the result: its frame takes the result's
    /// shape (#827). A cropped picture works the same way.
    #[test]
    fn shaping_a_picture_crops_it_to_the_result() {
        use crate::ops::Shaping;
        for op in [Shaping::Trim, Shaping::Intersect, Shaping::Weld] {
            let mut app = App::headless();
            let pic = picture(&mut app);
            let ellipse = app
                .new_shape(ShapeKind::Ellipse {
                    rect: Rect::new(40.0, 30.0, 60.0, 50.0),
                    arc: None,
                })
                .expect("ellipse");
            // The picture last: the target, keeping its looks.
            app.select(vec![ellipse, pic]);
            app.shaping_with(op, false, false);
            let id = app.selection[0];
            let s = app.doc().find_shape(id).expect("result").clone();
            let ShapeKind::ClipFrame { frame, contents } = &s.kind else {
                panic!("{op:?}: {:?}", s.kind);
            };
            assert!(
                matches!(contents[0].kind, ShapeKind::Bitmap { .. }),
                "{op:?}"
            );
            let b = tracedraw_core::geometry::Shape::bounding_box(&frame.page_path());
            match op {
                Shaping::Intersect => {
                    assert!(
                        (b.x0 - 40.0).abs() < 0.1 && (b.y1 - 40.0).abs() < 0.1,
                        "{b:?}"
                    )
                }
                Shaping::Weld => {
                    assert!(
                        (b.x1 - 60.0).abs() < 0.1 && (b.y1 - 50.0).abs() < 0.1,
                        "{b:?}"
                    )
                }
                _ => assert!(
                    (b.x1 - 50.0).abs() < 0.1 && (b.y1 - 40.0).abs() < 0.1,
                    "{b:?}"
                ),
            }
            assert!(app.doc().find_shape(pic).is_none(), "{op:?}: replaced");
            let left = b.x0.max(30.0);
            // A second cut on the cropped picture keeps it a picture.
            let cutter = app
                .new_shape(ShapeKind::Rect {
                    rect: Rect::new(0.0, 0.0, 30.0, 100.0),
                    radius: 0.0,
                    corners: None,
                })
                .expect("rect");
            app.select(vec![cutter, id]);
            app.shaping_with(Shaping::Trim, false, false);
            let s = app
                .doc()
                .find_shape(app.selection[0])
                .expect("result")
                .clone();
            let ShapeKind::ClipFrame { frame, contents } = &s.kind else {
                panic!("{op:?} then Trim: {:?}", s.kind);
            };
            assert!(matches!(contents[0].kind, ShapeKind::Bitmap { .. }));
            let b = tracedraw_core::geometry::Shape::bounding_box(&frame.page_path());
            assert!((b.x0 - left).abs() < 0.1, "{op:?}: {b:?}");
        }
    }

    /// To Curve and To Line on the start node of a closed curve act on its
    /// closing segment, the segment ending at that node, as the property
    /// bar's buttons do on any other node (#1618).
    #[test]
    fn to_curve_takes_the_closing_segment_through_the_start_node() {
        let mut app = App::headless();
        let id = app
            .new_shape(ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 40.0, 40.0),
                radius: 0.0,
                corners: None,
            })
            .expect("rect");
        app.select(vec![id]);
        app.convert_to_curves();
        let path_of = |app: &App| match &app.doc().find_shape(id).expect("curve").kind {
            ShapeKind::Path { path, .. } => path.clone(),
            other => panic!("not a curve: {other:?}"),
        };
        let curves = |p: &BezPath| {
            p.elements()
                .iter()
                .filter(|e| matches!(e, PathEl::CurveTo(..)))
                .count()
        };
        let before = path_of(&app);
        assert_eq!(curves(&before), 0);
        let start = match before.elements()[0] {
            PathEl::MoveTo(p) => p,
            _ => Point::ZERO,
        };
        app.node_selection = vec![(id, 0)];
        app.selected_segments_to_curve();
        let after = path_of(&app);
        assert_eq!(curves(&after), 1, "{after:?}");
        // The closing segment is the curve: it ends on the start node, and
        // the curve keeps its four nodes.
        assert!(after
            .elements()
            .iter()
            .any(|e| matches!(e, PathEl::CurveTo(_, _, p) if (*p - start).hypot() < 1e-9)));
        assert_eq!(nodes::nodes(&after).len(), 4);
        app.selected_segments_to_line();
        assert_eq!(curves(&path_of(&app)), 0);
        // The closing segment clicked (the Shape tool chooses the element
        // drawing it) converts as well.
        let z = path_of(&app)
            .elements()
            .iter()
            .position(|e| matches!(e, PathEl::ClosePath))
            .expect("closed");
        app.node_selection = vec![(id, z)];
        app.selected_segments_to_curve();
        assert_eq!(curves(&path_of(&app)), 1);
    }

    /// Smooth on every node of two rectangles converted to curves and
    /// combined rounds every corner on both sides, the start nodes and the
    /// nodes before them too, where the closing lines meet them; the types
    /// kept and the chosen nodes stay on the nodes although the closing
    /// lines made curves move the second rectangle's elements (#1618).
    #[test]
    fn smooth_rounds_both_sides_where_the_closing_lines_meet() {
        let mut app = App::headless();
        let mut ids = Vec::new();
        for x in [0.0, 60.0] {
            ids.push(
                app.new_shape(ShapeKind::Rect {
                    rect: Rect::new(x, 0.0, x + 40.0, 40.0),
                    radius: 0.0,
                    corners: None,
                })
                .expect("rect"),
            );
        }
        app.select(ids);
        app.convert_to_curves();
        app.combine();
        let id = app.selection[0];
        let path_of = |app: &App| match &app.doc().find_shape(id).expect("curve").kind {
            ShapeKind::Path { path, .. } => path.clone(),
            other => panic!("not a curve: {other:?}"),
        };
        let before = nodes::nodes(&path_of(&app));
        assert_eq!(before.len(), 8);
        app.node_selection = before.iter().map(|n| (id, n.index)).collect();
        app.set_selected_node_type(NodeType::Smooth);
        let path = path_of(&app);
        let after = nodes::nodes(&path);
        assert_eq!(after.len(), 8, "{path:?}");
        for (a, b) in before.iter().zip(&after) {
            assert!((a.pos - b.pos).hypot() < 1e-9, "{path:?}");
            assert!(
                b.ctrl_in.is_some() && b.ctrl_out.is_some(),
                "node {} kept a straight side: {path:?}",
                b.index
            );
            assert_ne!(nodes::node_type(&path, b.index), NodeType::Cusp);
        }
        let indices: Vec<usize> = after.iter().map(|n| n.index).collect();
        let s = app.doc().find_shape(id).expect("curve");
        assert_eq!(s.node_types.keys().copied().collect::<Vec<_>>(), indices);
        assert!(s.node_types.values().all(|t| *t == NodeType::Smooth));
        let mut chosen: Vec<usize> = app.node_selection.iter().map(|(_, i)| *i).collect();
        chosen.sort_unstable();
        assert_eq!(chosen, indices);
    }

    /// A press on a picture's edge (away from its corners) crops it from
    /// that side: the top edge dragged down leaves the picture below it,
    /// unstretched (#3532).
    #[test]
    fn an_edge_of_a_picture_drags_to_crop_it() {
        let mut app = App::headless();
        let id = picture(&mut app);
        app.select(vec![id]);
        app.view.zoom = 1.0;
        // Near a corner it is the corner, in the middle of a side an edge.
        assert_eq!(app.bitmap_edge_at(Point::new(10.5, 10.5)), None);
        assert_eq!(app.bitmap_edge_at(Point::new(30.0, 40.2)), Some((id, 2)));
        assert_eq!(app.bitmap_edge_at(Point::new(30.0, 25.0)), None);
        assert!(app.begin_bitmap_edge_crop(Point::new(30.0, 40.0)));
        let start = app.start_curves();
        app.move_start_nodes(&start, Vec2::new(0.0, -15.0), Point::new(30.0, 25.0), false);
        let s = app.doc().find_shape(id).expect("clip frame");
        let ShapeKind::ClipFrame { frame, contents } = &s.kind else {
            panic!("not a clip frame: {:?}", s.kind);
        };
        let b = tracedraw_core::geometry::Shape::bounding_box(&frame.page_path());
        assert!(
            (b.y1 - 25.0).abs() < 1e-9 && (b.y0 - 10.0).abs() < 1e-9,
            "{b:?}"
        );
        assert!((b.x0 - 10.0).abs() < 1e-9 && (b.x1 - 50.0).abs() < 1e-9);
        // The picture itself keeps its size.
        let ShapeKind::Bitmap { rect, .. } = &contents[0].kind else {
            panic!("picture");
        };
        assert_eq!(*rect, Rect::new(10.0, 10.0, 50.0, 40.0));
        assert!(!app.begin_bitmap_edge_crop(Point::new(500.0, 500.0)));
    }

    /// #1491: a picture with a gradient transparency, cut down by moving
    /// two of its corner nodes with the Shape tool, keeps its
    /// transparency where it lay on the page: the ramp is cut, not
    /// started again across what is left, and not laid on twice.
    #[test]
    fn moving_a_see_through_pictures_nodes_keeps_its_look() {
        use crate::crop_tool::tests::{green_at, see_through_picture};
        let mut app = App::headless();
        let id = see_through_picture(&mut app);
        let before = [green_at(&app, 35.0), green_at(&app, 45.0)];
        app.select(vec![id]);
        let frame = app.crop_bitmap_to_frame(id).expect("frame");
        // The left corners, both selected, moved 20 mm to the right.
        app.node_selection = vec![(frame, 0), (frame, 3)];
        let start = app.start_curves();
        app.move_start_nodes(&start, Vec2::new(20.0, 0.0), Point::new(30.0, 10.0), false);
        let s = app.doc().find_shape(id).expect("clip frame");
        let ShapeKind::ClipFrame { frame: f, .. } = &s.kind else {
            panic!("not a clip frame: {:?}", s.kind);
        };
        let b = tracedraw_core::geometry::Shape::bounding_box(&f.page_path());
        assert!((b.x0 - 30.0).abs() < 1e-9, "{b:?}");
        let after = [green_at(&app, 35.0), green_at(&app, 45.0)];
        for (b, a) in before.iter().zip(&after) {
            assert!(b.abs_diff(*a) <= 2, "before {before:?}, after {after:?}");
        }
    }

    #[test]
    fn a_pictures_corner_crops_it_and_double_click_keeps_it_a_picture() {
        let mut app = App::headless();
        let id = picture(&mut app);
        app.select(vec![id]);
        let corners = bitmap_corners(app.doc().find_shape(id).expect("shape")).expect("corners");
        assert_eq!(corners[0], Point::new(10.0, 10.0));
        // A corner of the selected picture is a node for the Shape tool.
        app.view.zoom = 1.0;
        assert_eq!(app.bitmap_corner_at(Point::new(10.5, 10.5)), Some((id, 0)));
        // Cropping: the picture goes into a clip frame of its rectangle,
        // whose nodes the Shape tool now edits.
        let frame = app.crop_bitmap_to_frame(id).expect("frame");
        assert_eq!(app.node_target(id), frame);
        app.node_selection = vec![(frame, 2)];
        let start = app.start_curves();
        assert_eq!(start.len(), 1);
        app.move_start_nodes(
            &start,
            Vec2::new(-20.0, -10.0),
            Point::new(50.0, 40.0),
            false,
        );
        let s = app.doc().find_shape(id).expect("clip frame");
        let ShapeKind::ClipFrame { frame: f, contents } = &s.kind else {
            panic!("not a clip frame: {:?}", s.kind);
        };
        assert!(matches!(contents[0].kind, ShapeKind::Bitmap { .. }));
        let b = tracedraw_core::geometry::Shape::bounding_box(&f.page_path());
        assert!(
            (b.x1 - 50.0).abs() < 1e-9 && (b.y1 - 40.0).abs() < 1e-9,
            "{b:?}"
        );
        let moved = nodes::nodes(match &f.kind {
            ShapeKind::Path { path, .. } => path,
            _ => panic!("frame"),
        })
        .into_iter()
        .any(|n| (n.pos - Point::new(30.0, 30.0)).hypot() < 1e-9);
        assert!(moved);
        // A double click with the Shape tool turns shapes into curves, not
        // pictures (#60).
        assert!(!curves_on_double_click(&contents[0].kind));
        assert!(curves_on_double_click(&ShapeKind::Rect {
            rect: Rect::new(0.0, 0.0, 1.0, 1.0),
            radius: 0.0,
            corners: None,
        }));
    }
}

/// Whether every subpath of `p` ends with a close.
fn every_subpath_closed(p: &BezPath) -> bool {
    let mut open = false;
    let mut any = false;
    for el in p.elements() {
        match el {
            tracedraw_core::geometry::PathEl::MoveTo(_) => {
                if open {
                    return false;
                }
                open = true;
                any = true;
            }
            tracedraw_core::geometry::PathEl::ClosePath => open = false,
            _ => {}
        }
    }
    any && !open
}

#[cfg(test)]
mod soft_eraser_tests {
    use super::*;
    use tracedraw_core::geometry::Rect;

    /// The Eraser over a picture takes its pixels away along the nib,
    /// fully in the middle and fading out to the edge, in one undo step;
    /// the picture stays a picture of the same size (#3043: pictures could
    /// not be erased at all).
    #[test]
    fn the_eraser_fades_a_picture_out_along_the_nib() {
        let mut app = App::headless();
        // 100 x 100 mm at 1 pixel a millimetre, opaque red.
        let img = image::RgbaImage::from_pixel(100, 100, image::Rgba([255, 0, 0, 255]));
        let png = crate::bitmap_fx::encode(&img).expect("png");
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
        app.eraser_width = 20.0;
        app.eraser_hardness = 0.5;
        app.eraser_strength = 1.0;
        let depth = app.engine.history_labels().0.len();
        // Across the picture at y = 50.
        app.erase_along(&[Point::new(-10.0, 50.0), Point::new(110.0, 50.0)]);
        assert_eq!(app.engine.history_labels().0.len(), depth + 1, "one step");
        let s = app.doc().find_shape(id).expect("still there");
        let ShapeKind::Bitmap {
            width_px,
            height_px,
            png,
            ..
        } = &s.kind
        else {
            panic!("still a picture: {:?}", s.kind);
        };
        assert_eq!((*width_px, *height_px), (100, 100));
        let out = image::load_from_memory(png.as_slice())
            .expect("decode")
            .to_rgba8();
        // Row y holds page y = 99.5 - y: the line at page y 50 is between
        // rows 49 and 50.
        let alpha = |page_y: f64| out.get_pixel(50, (99.5 - page_y) as u32)[3];
        assert_eq!(alpha(50.5), 0, "the middle goes");
        assert_eq!(alpha(54.5), 0, "within the hard core (5 mm)");
        let soft = alpha(57.5);
        assert!(soft > 0 && soft < 255, "the edge fades: {soft}");
        assert!(alpha(58.5) > soft, "fading out");
        assert_eq!(alpha(70.5), 255, "beyond the nib");
        assert_eq!(out.get_pixel(50, 10)[0], 255, "the colour stays");
        // Half strength: never below half.
        app.eraser_strength = 0.5;
        app.erase_along(&[Point::new(50.0, -10.0), Point::new(50.0, 110.0)]);
        let s = app.doc().find_shape(id).expect("picture");
        let ShapeKind::Bitmap { png, .. } = &s.kind else {
            panic!();
        };
        let out = image::load_from_memory(png.as_slice())
            .expect("decode")
            .to_rgba8();
        let a = out.get_pixel(50, 10)[3];
        assert!((126..=129).contains(&a), "half taken: {a}");
        // The nib's amount: full inside, smooth to the edge.
        let nib = SoftNib {
            radius: 10.0,
            hardness: 0.0,
            strength: 1.0,
        };
        assert!((nib.amount(0.0) - 1.0).abs() < 1e-9);
        assert!((nib.amount(5.0) - 0.5).abs() < 1e-9);
        assert_eq!(nib.amount(10.0), 0.0);
    }
}

#[cfg(test)]
mod every_segment_tests {
    use crate::app::App;
    use tracedraw_core::geometry::{BezPath, Rect};
    use tracedraw_core::ShapeKind;

    /// Object > Nodes > Add Node to Every Segment, with the Pick tool: a
    /// node in the middle of each segment of the selected curve, one undo
    /// step; a rectangle (not a curve) leaves the command off.
    #[test]
    fn every_segment_of_the_selected_curves_gets_a_node() {
        let mut app = App::headless();
        let r = app
            .new_shape(ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                radius: 0.0,
                corners: None,
            })
            .expect("rect");
        app.select(vec![r]);
        assert!(!app.can_add_node_every_segment());
        let mut p = BezPath::new();
        p.move_to((0.0, 0.0));
        p.line_to((10.0, 0.0));
        p.line_to((10.0, 10.0));
        let id = app
            .new_shape(ShapeKind::Path {
                path: p,
                closed: false,
            })
            .expect("curve");
        app.select(vec![id]);
        assert!(app.can_add_node_every_segment());
        let len = |app: &App| match &app.doc().find_shape(id).expect("curve").kind {
            ShapeKind::Path { path, .. } => path.elements().len(),
            _ => 0,
        };
        app.add_node_every_segment();
        assert_eq!(len(&app), 5);
        app.add_node_every_segment();
        assert_eq!(len(&app), 9);
        app.undo();
        assert_eq!(len(&app), 5);
    }
}
