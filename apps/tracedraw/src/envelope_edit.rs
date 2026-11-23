//! Editing envelopes. The Envelope tool shows a frame on the object it
//! clicks, which becomes its envelope when it is edited (#2611), and
//! drags its nodes; the Shape tool drags them too. A node
//! moves the way the chosen mode says (straight line, single arc, double
//! arc or unconstrained); in the Unconstrained mode the selected node's
//! handles show and move on their own. Dragging a segment of the frame
//! bends it, as the Shape tool bends a curve's segment (#1339).
//! Double-clicking the frame adds a node, double-clicking a node that is
//! not a corner removes it. Presets,
//! the mapping, Keep Lines, Add New, Clear and Copy act on the selection.

use crate::app::App;
use crate::i18n::tr;
use egui::{PointerButton, Response};
use tracedraw_core::{
    document::Shape,
    envelope::{Frame, Mapping, Mode},
    geometry::{Affine, Point, Rect, Vec2},
    live::Effect,
    Command, ShapeId,
};

/// A part of an envelope's frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Part {
    Node(usize),
    /// Handle `end` (0 at the segment's start, 1 at its end) of segment
    /// `seg`.
    Handle {
        seg: usize,
        end: usize,
    },
    /// Segment `seg`, grabbed at its point `t` (0 to 1): a drag bends it.
    Segment {
        seg: usize,
        t: f64,
    },
}

/// A part being dragged: the effects as they were when the drag began,
/// so every step starts again from them.
#[derive(Debug, Clone, PartialEq)]
pub struct EnvelopeDrag {
    pub shape: ShapeId,
    pub part: Part,
    pub start: Point,
    pub original: Vec<Effect>,
    /// A step of this drag is on the undo list.
    pub stepped: bool,
}

/// The envelope presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Arch,
    Bulge,
    Squeeze,
    Flag,
    Perspective,
    Circle,
}

impl Preset {
    pub const ALL: [Preset; 6] = [
        Preset::Arch,
        Preset::Bulge,
        Preset::Squeeze,
        Preset::Flag,
        Preset::Perspective,
        Preset::Circle,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Preset::Arch => "docker.preset_arch",
            Preset::Bulge => "docker.preset_bulge",
            Preset::Squeeze => "docker.preset_squeeze",
            Preset::Flag => "docker.preset_flag",
            Preset::Perspective => "docker.preset_perspective",
            Preset::Circle => "docker.preset_circle",
        }
    }

    /// The preset's frame around `b`.
    pub fn frame(self, b: Rect) -> Frame {
        let mut f = Frame::around(b);
        let (w, h) = (b.width(), b.height());
        // Nodes: 0 BL, 1 BM, 2 BR, 3 RM, 4 TR, 5 TM, 6 TL, 7 LM.
        let moves: Vec<(usize, Vec2)> = match self {
            Preset::Arch => vec![(5, Vec2::new(0.0, h * 0.3))],
            Preset::Bulge => vec![
                (1, Vec2::new(0.0, -h * 0.2)),
                (5, Vec2::new(0.0, h * 0.2)),
                (3, Vec2::new(w * 0.2, 0.0)),
                (7, Vec2::new(-w * 0.2, 0.0)),
            ],
            Preset::Squeeze => vec![(1, Vec2::new(0.0, h * 0.2)), (5, Vec2::new(0.0, -h * 0.2))],
            Preset::Flag => vec![
                (1, Vec2::new(0.0, -h * 0.2)),
                (5, Vec2::new(0.0, -h * 0.2)),
                (0, Vec2::new(0.0, h * 0.1)),
                (6, Vec2::new(0.0, h * 0.1)),
            ],
            Preset::Perspective => {
                vec![(4, Vec2::new(-w * 0.2, 0.0)), (6, Vec2::new(w * 0.2, 0.0))]
            }
            Preset::Circle => {
                // (sqrt 2 - 1) / 2 out: the middles sit on the circle.
                let k = 0.4142 * 0.5;
                vec![
                    (1, Vec2::new(0.0, -h * k)),
                    (5, Vec2::new(0.0, h * k)),
                    (3, Vec2::new(w * k, 0.0)),
                    (7, Vec2::new(-w * k, 0.0)),
                ]
            }
        };
        let mode = if self == Preset::Perspective {
            Mode::StraightLine
        } else {
            Mode::SingleArc
        };
        for (i, d) in moves {
            f.move_node(i, d, mode);
        }
        for s in 0..4 {
            f.reshape_side(s, mode);
        }
        f
    }
}

/// The last envelope in a list of effects: its place and its frame.
pub fn last_envelope(effects: &[Effect]) -> Option<(usize, Frame)> {
    effects
        .iter()
        .enumerate()
        .rev()
        .find_map(|(i, e)| e.envelope_frame().map(|f| (i, f)))
}

/// The object's own bounding box where the effect at `at` takes it: after
/// the warps before it (an extrusion before it is built afterwards, from
/// the outline the envelope leaves, #2214).
pub fn box_at(shape: &Shape, at: usize) -> Rect {
    tracedraw_core::live::warp_box(shape, at)
}

/// The effect at `at` with its frame replaced (an envelope keeps its Keep
/// Lines and mapping).
fn with_frame(effects: &[Effect], at: usize, frame: &Frame) -> Vec<Effect> {
    let mut out = effects.to_vec();
    if let Some(e) = out.get_mut(at) {
        if let Effect::Envelope {
            keep_lines,
            mapping,
            reference,
            ..
        } = e
        {
            // An envelope kept from a group still maps the group's box.
            let reference = *reference;
            *e = Effect::from_envelope(frame, *keep_lines, *mapping);
            if let Effect::Envelope { reference: r, .. } = e {
                *r = reference;
            }
        }
    }
    out
}

/// The linear part of `m` applied to `d`.
fn local_delta(m: Affine, d: Vec2) -> Vec2 {
    let inv = m.inverse();
    (inv * (Point::ZERO + d)) - (inv * Point::ZERO)
}

impl App {
    /// The last envelope of object `id`.
    pub fn envelope_of(&self, id: ShapeId) -> Option<(usize, Frame)> {
        let s = self.doc().find_shape(id)?;
        last_envelope(&s.effects)
    }

    /// Object `id`'s effects with the envelope that is edited, where it
    /// sits and its frame: its last envelope or, with the Envelope tool
    /// on a selected object that has none, a frame around the object as
    /// it now looks. That frame only shows: it becomes the object's
    /// envelope when a node, handle or side of it is moved, as in
    /// CorelDRAW. A click put an envelope on the object at once (#2611).
    pub fn editable_envelope(&self, id: ShapeId) -> Option<(Vec<Effect>, usize, Frame)> {
        let s = self.doc().find_shape(id)?;
        if let Some((at, f)) = last_envelope(&s.effects) {
            return Some((s.effects.clone(), at, f));
        }
        if self.tool != crate::tools::Tool::Envelope || !self.selection.contains(&id) {
            return None;
        }
        let b = self.unenveloped_box(s);
        if b.width() < 1e-9 || b.height() < 1e-9 {
            return None;
        }
        let mut effects = s.effects.clone();
        let mut e = Effect::envelope_around(b, self.envelope_keep_lines);
        if let Effect::Envelope { mapping, .. } = &mut e {
            *mapping = self.envelope_mapping;
        }
        effects.push(e);
        let at = effects.len() - 1;
        let f = effects[at].envelope_frame()?;
        Some((effects, at, f))
    }

    /// The box a new envelope takes around `s`, kept for the document's
    /// revision: the frame shows every frame and measuring a long text
    /// each time would be slow.
    fn unenveloped_box(&self, s: &Shape) -> Rect {
        let key = (self.engine.instance(), self.engine.revision());
        if let Ok(mut cache) = self.envelope_box_cache.try_borrow_mut() {
            if (cache.0, cache.1) != key {
                *cache = (key.0, key.1, Default::default());
            }
            return *cache
                .2
                .entry(s.id)
                .or_insert_with(|| box_at(s, s.effects.len()));
        }
        box_at(s, s.effects.len())
    }

    /// The selected objects with an envelope to edit, as on the page.
    fn enveloped_selection(&self) -> Vec<(Shape, Frame)> {
        self.selected_shapes()
            .into_iter()
            .filter_map(|s| self.editable_envelope(s.id).map(|(_, _, f)| (s, f)))
            .collect()
    }

    /// The handles shown for object `id`: those of its selected node, in
    /// the Unconstrained mode. (segment, end, handle, its node), in local
    /// space.
    pub fn envelope_handles(
        &self,
        id: ShapeId,
        frame: &Frame,
    ) -> Vec<(usize, usize, Point, Point)> {
        if self.envelope_mode != Mode::Unconstrained {
            return Vec::new();
        }
        let Some((sid, i)) = self.selected_effect_node else {
            return Vec::new();
        };
        let n = frame.len();
        if sid != id || i >= n {
            return Vec::new();
        }
        let prev = (i + n - 1) % n;
        let node = frame.nodes[i];
        let mut out = Vec::new();
        if let Some(h) = frame.handles.get(i) {
            out.push((i, 0, h[0], node));
        }
        if let Some(h) = frame.handles.get(prev) {
            out.push((prev, 1, h[1], node));
        }
        out
    }

    /// The envelope part under `p` (page space): handles first, then nodes.
    pub fn envelope_part_at(&self, p: Point) -> Option<(ShapeId, Part)> {
        let tol = 5.0 / self.view.zoom as f64;
        let shapes = self.enveloped_selection();
        for (s, f) in &shapes {
            for (seg, end, h, _) in self.envelope_handles(s.id, f) {
                if (s.transform * h - p).hypot() <= tol {
                    return Some((s.id, Part::Handle { seg, end }));
                }
            }
        }
        for (s, f) in &shapes {
            for (i, n) in f.nodes.iter().enumerate() {
                if (s.transform * *n - p).hypot() <= tol {
                    return Some((s.id, Part::Node(i)));
                }
            }
        }
        None
    }

    /// The frame segment under `p`: (object, segment, parameter).
    pub fn envelope_segment_at(&self, p: Point) -> Option<(ShapeId, usize, f64)> {
        let tol = 4.0 / self.view.zoom as f64;
        for (s, f) in self.enveloped_selection() {
            // Measure on the page: the frame moved there.
            let page = f.transformed(s.transform);
            if let Some((seg, t, d)) = page.nearest_segment(p) {
                if d <= tol {
                    return Some((s.id, seg, t));
                }
            }
        }
        None
    }

    /// Put `frame` in place of the envelope at `at` of object `id`, one
    /// undo step.
    fn set_envelope_frame(
        &mut self,
        id: ShapeId,
        base: &[Effect],
        at: usize,
        frame: &Frame,
        label: &'static str,
    ) {
        let effects = with_frame(base, at, frame);
        if let Err(e) = self
            .engine
            .run_with_label(&Command::SetEffects { shape: id, effects }, label)
        {
            self.status = e.to_string();
        }
    }

    /// Double-click on the frame: a node there; on a node that is not a
    /// corner: the node goes. True when it did either.
    pub fn envelope_double_click(&mut self, p: Point) -> bool {
        if let Some((id, Part::Node(i))) = self.envelope_part_at(p) {
            return self.remove_envelope_node(id, i);
        }
        if let Some((id, seg, t)) = self.envelope_segment_at(p) {
            return self.add_envelope_node(id, seg, t);
        }
        false
    }

    /// Add a node to segment `seg` of object `id`'s envelope at `t`; it
    /// becomes the selected node.
    pub fn add_envelope_node(&mut self, id: ShapeId, seg: usize, t: f64) -> bool {
        let Some((base, at, mut f)) = self.editable_envelope(id) else {
            return false;
        };
        let Some(i) = f.add_node(seg, t) else {
            return false;
        };
        self.set_envelope_frame(id, &base, at, &f, "Add Envelope Node");
        self.selected_effect_node = Some((id, i));
        true
    }

    /// Remove node `i` (not a corner) of object `id`'s envelope.
    pub fn remove_envelope_node(&mut self, id: ShapeId, i: usize) -> bool {
        let Some((base, at, mut f)) = self.editable_envelope(id) else {
            return false;
        };
        if !f.remove_node(i) {
            self.status = tr("status.envelope_corner_stays");
            return false;
        }
        self.set_envelope_frame(id, &base, at, &f, "Delete Envelope Node");
        self.selected_effect_node = None;
        true
    }

    /// Every chosen envelope node of the object of the chosen one: that
    /// node first, then those chosen with it by Shift+click or a marquee
    /// (#3001).
    pub fn selected_envelope_nodes(&self) -> Option<(ShapeId, Vec<usize>)> {
        let (id, i) = self.selected_envelope_node()?;
        let count = self.editable_envelope(id).map_or(0, |(_, _, f)| f.len());
        let mut out = vec![i];
        if let Some((key, more)) = &self.envelope_more_nodes {
            if *key == (id, i) {
                for &k in more {
                    if k < count && !out.contains(&k) {
                        out.push(k);
                    }
                }
            }
        }
        Some((id, out))
    }

    /// Choose the envelope nodes `nodes` of object `id`, the first the one
    /// whose handles show; none chooses no node.
    pub fn choose_envelope_nodes(&mut self, id: ShapeId, nodes: &[usize]) {
        let Some((&first, more)) = nodes.split_first() else {
            self.selected_effect_node = None;
            self.envelope_more_nodes = None;
            return;
        };
        self.selected_effect_node = Some((id, first));
        self.envelope_more_nodes = Some(((id, first), more.to_vec()));
    }

    /// Shift+click (or Ctrl+Shift+click) on envelope node `i` of object
    /// `id`: it joins the chosen nodes, or leaves them when it was
    /// chosen. A node of another object starts a new choice (#3001).
    pub fn toggle_envelope_node(&mut self, id: ShapeId, i: usize) {
        let mut nodes = match self.selected_envelope_nodes() {
            Some((sid, nodes)) if sid == id => nodes,
            _ => Vec::new(),
        };
        match nodes.iter().position(|&k| k == i) {
            Some(at) => {
                nodes.remove(at);
            }
            None => nodes.push(i),
        }
        self.choose_envelope_nodes(id, &nodes);
    }

    /// The envelope nodes of the selected objects inside `area` (page
    /// space): a marquee with the Envelope tool chooses them, with `add`
    /// (Shift) they join those chosen. The first object with nodes inside
    /// wins (#3001).
    pub fn choose_envelope_nodes_in(&mut self, area: Rect, add: bool) {
        let found = self.enveloped_selection().into_iter().find_map(|(s, f)| {
            let inside: Vec<usize> = f
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| area.contains(s.transform * **n))
                .map(|(i, _)| i)
                .collect();
            (!inside.is_empty()).then_some((s.id, inside))
        });
        let Some((id, inside)) = found else {
            if !add {
                self.choose_envelope_nodes(ShapeId(0), &[]);
            }
            return;
        };
        let mut nodes = match self.selected_envelope_nodes() {
            Some((sid, nodes)) if add && sid == id => nodes,
            _ => Vec::new(),
        };
        for i in inside {
            if !nodes.contains(&i) {
                nodes.push(i);
            }
        }
        self.choose_envelope_nodes(id, &nodes);
    }

    /// Whether object `id`'s envelope node `i` is among the chosen ones.
    pub fn envelope_node_chosen(&self, id: ShapeId, i: usize) -> bool {
        self.selected_envelope_nodes()
            .is_some_and(|(sid, nodes)| sid == id && nodes.contains(&i))
    }

    /// The selected envelope node, if it is one of a selected object.
    pub fn selected_envelope_node(&self) -> Option<(ShapeId, usize)> {
        let (id, i) = self.selected_effect_node?;
        let (_, _, f) = self.editable_envelope(id)?;
        (self.selection.contains(&id) && i < f.len()).then_some((id, i))
    }

    /// The property bar's Add Node: a node halfway along the segment after
    /// the selected node.
    pub fn add_node_after_selected(&mut self) {
        if let Some((id, i)) = self.selected_envelope_node() {
            self.add_envelope_node(id, i, 0.5);
        }
    }

    /// The property bar's Delete Node: the selected node goes.
    pub fn delete_selected_envelope_node(&mut self) {
        if let Some((id, i)) = self.selected_envelope_node() {
            self.remove_envelope_node(id, i);
        }
    }

    pub(crate) fn begin_envelope_drag(&mut self, id: ShapeId, part: Part, p: Point) {
        // An object the Envelope tool shows a frame on takes it as its
        // envelope with the first move of the drag (#2611).
        let Some((original, _, _)) = self.editable_envelope(id) else {
            return;
        };
        self.envelope_drag = Some(EnvelopeDrag {
            shape: id,
            part,
            start: p,
            original,
            stepped: false,
        });
        if let Part::Node(i) = part {
            // A node of the chosen ones moves them all (#3001); another
            // node is chosen alone.
            if !self.envelope_node_chosen(id, i) {
                self.choose_envelope_nodes(id, &[i]);
            }
        }
    }

    /// A drag step: the part moved from where the drag began to `p`, the
    /// whole drag one undo step.
    pub(crate) fn envelope_drag_to(&mut self, p: Point) {
        let Some(drag) = self.envelope_drag.clone() else {
            return;
        };
        let Some(page) = self.doc().shape_on_page(drag.shape) else {
            return;
        };
        let Some((at, mut f)) = last_envelope(&drag.original) else {
            return;
        };
        let d = local_delta(page.transform, p - drag.start);
        match drag.part {
            // The chosen nodes move together, the one dragged first
            // (#3001).
            Part::Node(i) => {
                let mut all = vec![i];
                if let Some((sid, nodes)) = self.selected_envelope_nodes() {
                    if sid == drag.shape && nodes.contains(&i) {
                        all.extend(nodes.into_iter().filter(|&k| k != i));
                    }
                }
                for k in all {
                    if k < f.len() {
                        f.move_node(k, d, self.envelope_mode);
                    }
                }
            }
            Part::Handle { seg, end } => f.move_handle(seg, end, d),
            Part::Segment { seg, t } => f.bend_segment(seg, t, d),
        }
        let effects = with_frame(&drag.original, at, &f);
        if drag.stepped && self.engine.undo_label() == Some("Edit Envelope") {
            let _ = self.engine.undo();
        }
        if self
            .engine
            .run_with_label(
                &Command::SetEffects {
                    shape: drag.shape,
                    effects,
                },
                "Edit Envelope",
            )
            .is_ok()
        {
            if let Some(d) = self.envelope_drag.as_mut() {
                d.stepped = true;
            }
        }
    }

    /// Clicks, double-clicks and drags on the selected objects' envelopes,
    /// for the Envelope and the Shape tools. True when the input was
    /// theirs.
    pub fn envelope_input(&mut self, response: &Response, p: Point) -> bool {
        if response.double_clicked() && self.envelope_double_click(p) {
            return true;
        }
        if response.drag_started_by(PointerButton::Primary) {
            // Handles and nodes first; then a segment of the frame, which
            // bends (#1339).
            let part = self.envelope_part_at(p).or_else(|| {
                self.envelope_segment_at(p)
                    .map(|(id, seg, t)| (id, Part::Segment { seg, t }))
            });
            if let Some((id, part)) = part {
                self.begin_envelope_drag(id, part, p);
                return true;
            }
        }
        if self.envelope_drag.is_some() {
            if response.dragged_by(PointerButton::Primary) {
                self.envelope_drag_to(p);
            }
            if response.drag_stopped() {
                self.envelope_drag = None;
            }
            return true;
        }
        if response.clicked_by(PointerButton::Primary) {
            match self.envelope_part_at(p) {
                Some((id, Part::Node(i))) => {
                    // Shift (or Ctrl+Shift) adds the node to the chosen
                    // ones or takes it out (#3001).
                    if response.ctx.input(|i| i.modifiers.shift) {
                        self.toggle_envelope_node(id, i);
                    } else {
                        self.choose_envelope_nodes(id, &[i]);
                    }
                    return true;
                }
                Some((_, Part::Handle { .. } | Part::Segment { .. })) => return true,
                None => {}
            }
        }
        false
    }

    /// The Envelope tool: the frame's parts as with the Shape tool; a click
    /// on an object selects it and shows a frame on it, which becomes an
    /// envelope only when it is edited (#2611); a click on nothing clears
    /// the selection.
    pub fn envelope_tool_input(&mut self, response: &Response, p: Point) {
        // A frame's nodes, then a mesh envelope's grid points.
        if self.envelope_input(response, p) || self.effect_node_input(response, p) {
            return;
        }
        // A drag from outside the frame of a selected object: a marquee
        // that chooses the envelope nodes inside it, Shift adding them
        // (#3001; it ends in `finish_node_marquee`).
        if response.drag_started_by(PointerButton::Primary) && !self.enveloped_selection().is_empty()
        {
            let start = response
                .ctx
                .input(|i| i.pointer.press_origin())
                .map(|o| self.view.to_page(o))
                .unwrap_or(p);
            self.node_lasso_keeps = response.ctx.input(|i| i.modifiers.shift);
            self.drag = crate::app::Drag::NodeMarquee { start, current: p };
            return;
        }
        if response.clicked_by(PointerButton::Primary) {
            match self.hit_test(p) {
                Some(id) => {
                    self.select(vec![id]);
                    self.selected_effect_node = None;
                }
                None => {
                    self.select(Vec::new());
                    self.selected_effect_node = None;
                }
            }
        }
    }

    /// Add New Envelope: a fresh frame around each selected object as it
    /// now looks (over an envelope it already has), one undo step.
    pub fn add_new_envelope(&mut self) {
        let mut cmds = Vec::new();
        for id in self.selection.clone() {
            let Some(s) = self.doc().find_shape(id) else {
                continue;
            };
            let b = box_at(s, s.effects.len());
            if b.width() < 1e-9 && b.height() < 1e-9 {
                continue;
            }
            let mut effects = s.effects.clone();
            let mut e = Effect::envelope_around(b, self.envelope_keep_lines);
            if let Effect::Envelope { mapping, .. } = &mut e {
                *mapping = self.envelope_mapping;
            }
            effects.push(e);
            cmds.push(Command::SetEffects { shape: id, effects });
        }
        if cmds.is_empty() {
            return;
        }
        if let Err(e) = self.engine.run_batch("Add Envelope", &cmds) {
            self.status = e.to_string();
        }
        self.selected_effect_node = None;
    }

    /// Add Mesh Envelope: a grid of `envelope_mesh` columns and rows laid
    /// over each selected object as it now looks, its points dragged with
    /// the Envelope (or Shape) tool to bend the object; it takes the
    /// place of a mesh envelope the object has last, one undo step.
    pub fn add_mesh_envelope(&mut self) {
        let (cols, rows) = self.envelope_mesh;
        let mut cmds = Vec::new();
        for id in self.selection.clone() {
            let Some(s) = self.doc().find_shape(id) else {
                continue;
            };
            let mut effects = s.effects.clone();
            let last_mesh = matches!(effects.last(), Some(Effect::MeshEnvelope { .. }));
            if last_mesh {
                effects.pop();
            }
            let b = box_at(s, effects.len());
            if b.width() < 1e-9 || b.height() < 1e-9 {
                continue;
            }
            effects.push(Effect::mesh_envelope_around(b, cols, rows));
            cmds.push(Command::SetEffects { shape: id, effects });
        }
        if cmds.is_empty() {
            return;
        }
        if let Err(e) = self.engine.run_batch("Add Mesh Envelope", &cmds) {
            self.status = e.to_string();
        }
        self.selected_effect_node = None;
    }

    /// The object has an envelope: a frame or a mesh.
    pub fn has_any_envelope(&self, id: ShapeId) -> bool {
        self.doc().find_shape(id).is_some_and(|s| {
            s.effects
                .iter()
                .any(|e| matches!(e, Effect::Envelope { .. } | Effect::MeshEnvelope { .. }))
        })
    }

    /// Clear Envelope: each selected object loses its last envelope (a
    /// frame or a mesh), one undo step.
    pub fn clear_envelope(&mut self) {
        let mut cmds = Vec::new();
        for id in self.selection.clone() {
            let Some(s) = self.doc().find_shape(id) else {
                continue;
            };
            let Some(at) = s
                .effects
                .iter()
                .rposition(|e| matches!(e, Effect::Envelope { .. } | Effect::MeshEnvelope { .. }))
            else {
                continue;
            };
            let mut effects = s.effects.clone();
            effects.remove(at);
            cmds.push(Command::SetEffects { shape: id, effects });
        }
        if !cmds.is_empty() {
            if let Err(e) = self.engine.run_batch("Clear Envelope", &cmds) {
                self.status = e.to_string();
            }
        }
        self.selected_effect_node = None;
    }

    /// Change the last envelope of each selected object with `f`, one undo
    /// step named `label`.
    fn edit_envelopes(&mut self, label: &'static str, f: impl Fn(&mut Effect)) {
        let mut cmds = Vec::new();
        for id in self.selection.clone() {
            let Some(s) = self.doc().find_shape(id) else {
                continue;
            };
            let Some((at, _)) = last_envelope(&s.effects) else {
                continue;
            };
            let mut effects = s.effects.clone();
            if let Some(e) = effects.get_mut(at) {
                f(e);
            }
            if effects != s.effects {
                cmds.push(Command::SetEffects { shape: id, effects });
            }
        }
        if !cmds.is_empty() {
            if let Err(e) = self.engine.run_batch(label, &cmds) {
                self.status = e.to_string();
            }
        }
    }

    /// The mapping new envelopes take and the selected ones change to.
    pub fn set_envelope_mapping(&mut self, m: Mapping) {
        self.envelope_mapping = m;
        self.edit_envelopes("Envelope Mapping", |e| {
            if let Effect::Envelope { mapping, .. } = e {
                *mapping = m;
            }
        });
    }

    /// Keep Lines for new envelopes and the selected ones.
    pub fn set_envelope_keep_lines(&mut self, on: bool) {
        self.envelope_keep_lines = on;
        self.edit_envelopes("Keep Lines", |e| {
            if let Effect::Envelope { keep_lines, .. } = e {
                *keep_lines = on;
            }
        });
    }

    /// The mapping and Keep Lines the bar shows: the first selected
    /// envelope's, or what new envelopes take.
    pub fn envelope_settings_shown(&self) -> (Mapping, bool) {
        for id in &self.selection {
            if let Some(s) = self.doc().find_shape(*id) {
                if let Some((at, _)) = last_envelope(&s.effects) {
                    if let Some(Effect::Envelope {
                        mapping,
                        keep_lines,
                        ..
                    }) = s.effects.get(at)
                    {
                        return (*mapping, *keep_lines);
                    }
                }
            }
        }
        (self.envelope_mapping, self.envelope_keep_lines)
    }

    /// A preset in place of each selected object's last envelope (or as a
    /// new one), around the object where the envelope takes it.
    pub fn apply_envelope_preset(&mut self, preset: Preset) {
        self.apply_envelope_frame(|b| preset.frame(b));
    }

    /// The frame `make` gives each selected object's box in place of its
    /// last envelope (or as a new one), one undo step.
    pub fn apply_envelope_frame(&mut self, make: impl Fn(Rect) -> Frame) {
        let mut cmds = Vec::new();
        for id in self.selection.clone() {
            let Some(s) = self.doc().find_shape(id) else {
                continue;
            };
            let mut effects = s.effects.clone();
            let at = match last_envelope(&effects) {
                Some((at, _)) => at,
                None => {
                    effects.push(Effect::envelope_around(
                        Rect::ZERO,
                        self.envelope_keep_lines,
                    ));
                    effects.len() - 1
                }
            };
            let b = box_at(s, at);
            if b.width() < 1e-9 || b.height() < 1e-9 {
                continue;
            }
            let (keep, mapping) = match effects.get(at) {
                Some(Effect::Envelope {
                    keep_lines,
                    mapping,
                    ..
                }) if last_envelope(&s.effects).is_some() => (*keep_lines, *mapping),
                _ => (self.envelope_keep_lines, self.envelope_mapping),
            };
            if let Some(e) = effects.get_mut(at) {
                let reference = match e {
                    Effect::Envelope { reference, .. } => *reference,
                    _ => None,
                };
                *e = Effect::from_envelope(&make(b), keep, mapping);
                if let Effect::Envelope { reference: r, .. } = e {
                    *r = reference;
                }
            }
            cmds.push(Command::SetEffects { shape: id, effects });
        }
        if cmds.is_empty() {
            return;
        }
        if let Err(e) = self.engine.run_batch("Envelope Preset", &cmds) {
            self.status = e.to_string();
        }
        self.selected_effect_node = None;
    }

    /// Copy Envelope Properties: the next click picks the object whose
    /// envelope the selection takes.
    pub fn start_copy_envelope(&mut self) {
        if self.selection.is_empty() {
            return;
        }
        self.pending_copy_envelope = true;
        self.status = tr("status.click_source_object");
    }

    /// The selected objects take `source`'s last envelope, its frame
    /// stretched from `source`'s box to each object's, one undo step. A
    /// source with no envelope gives the shape of its outline, so text
    /// can be fitted into a drawn shape (#993): each
    /// object takes that shape at its size, centred where the object is
    /// (#2178).
    pub fn copy_envelope_from(&mut self, source: ShapeId) {
        self.envelope_from(source, false);
    }

    /// Envelope from Top Object: of two or more selected objects, the top
    /// one's outline becomes the envelope of the others, which are fitted
    /// into it where it is, and the top object goes; one undo step.
    pub fn envelope_from_top_object(&mut self) {
        if self.selection.len() < 2 {
            return;
        }
        let doc = self.doc();
        let tops: Vec<ShapeId> = self
            .selection
            .iter()
            .map(|id| doc.ancestors(*id).first().copied().unwrap_or(*id))
            .collect();
        let mut top = None;
        for l in self.page_layers().iter() {
            for s in &l.shapes {
                if let Some(k) = tops.iter().position(|t| *t == s.id) {
                    top = Some(self.selection[k]);
                }
            }
        }
        if let Some(top) = top {
            self.envelope_from(top, true);
        }
    }

    /// [`Self::copy_envelope_from`]; with `fit`, an outline (a source
    /// with no envelope) is the envelope where it lies on the page, the
    /// objects fitted into it, and the source is deleted.
    fn envelope_from(&mut self, source: ShapeId, fit: bool) {
        let Some(src) = self.doc().find_shape(source).cloned() else {
            return;
        };
        // The outline's frame on the page, when the source has no envelope.
        let mut on_page = false;
        let (frame, from, keep, mapping) = match last_envelope(&src.effects) {
            Some((src_at, frame)) => {
                let (keep, mapping) = match src.effects.get(src_at) {
                    Some(Effect::Envelope {
                        keep_lines,
                        mapping,
                        ..
                    }) => (*keep_lines, *mapping),
                    _ => (false, Mapping::Putty),
                };
                (frame, box_at(&src, src_at), keep, mapping)
            }
            None => {
                let outline = self
                    .doc()
                    .parent_transform(source)
                    .unwrap_or(Affine::IDENTITY)
                    * src.page_path();
                let Some(frame) = Frame::from_outline(&outline) else {
                    self.status = tr("status.no_envelope_to_copy");
                    return;
                };
                let from = frame.bounds();
                on_page = true;
                (frame, from, self.envelope_keep_lines, self.envelope_mapping)
            }
        };
        if from.width() < 1e-9 || from.height() < 1e-9 {
            return;
        }
        let mut cmds = Vec::new();
        for id in self.selection.clone() {
            if id == source {
                continue;
            }
            let Some(s) = self.doc().find_shape(id) else {
                continue;
            };
            let mut effects = s.effects.clone();
            let at = match last_envelope(&effects) {
                Some((at, _)) => at,
                None => {
                    effects.push(Effect::envelope_around(Rect::ZERO, keep));
                    effects.len() - 1
                }
            };
            let to = box_at(s, at);
            // The frame of an outline keeps the outline's size and shape,
            // centred where the object is: from the page into the
            // object's own space. As in #2178 (and the video
            // of #2344, where the object was moved into the shape by
            // hand afterwards), the object stays where it is; it was
            // moved into the outline.
            let to_page = self.doc().parent_transform(id).unwrap_or(Affine::IDENTITY) * s.transform;
            let into_object = to_page.inverse();
            let finite = into_object.as_coeffs().iter().all(|c| c.is_finite());
            let m = if on_page && finite && fit {
                into_object
            } else if on_page && finite {
                let shift = to_page * to.center() - from.center();
                into_object * Affine::translate(shift)
            } else {
                Affine::translate(to.origin().to_vec2())
                    * Affine::scale_non_uniform(
                        to.width() / from.width(),
                        to.height() / from.height(),
                    )
                    * Affine::translate(-from.origin().to_vec2())
            };
            if let Some(e) = effects.get_mut(at) {
                *e = Effect::from_envelope(&frame.transformed(m), keep, mapping);
            }
            cmds.push(Command::SetEffects { shape: id, effects });
        }
        if cmds.is_empty() {
            return;
        }
        let label = if fit {
            cmds.push(Command::DeleteShapes {
                shapes: vec![source],
            });
            "Envelope from Top Object"
        } else {
            "Copy Envelope"
        };
        if let Err(e) = self.engine.run_batch(label, &cmds) {
            self.status = e.to_string();
        }
        if fit {
            self.selection.retain(|id| *id != source);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::ParamCurve;
    use tracedraw_core::ShapeKind;

    fn rect(app: &mut App, x0: f64, y0: f64, x1: f64, y1: f64) -> ShapeId {
        app.new_shape(ShapeKind::Rect {
            rect: Rect::new(x0, y0, x1, y1),
            radius: 0.0,
            corners: None,
        })
        .expect("shape")
    }

    fn bounds(app: &App, id: ShapeId) -> Rect {
        let s = app.doc().shape_on_page(id).expect("shape");
        tracedraw_core::live::evaluate(&s).main.bounds()
    }

    /// Add Mesh lays a grid of the chosen columns and rows over the
    /// object, leaving it as it was; a moved grid point bends it; Clear
    /// Envelope takes the mesh off.
    #[test]
    fn a_mesh_envelope_bends_the_object_and_clears() {
        let mut app = App::headless();
        let id = rect(&mut app, 0.0, 0.0, 40.0, 20.0);
        app.select(vec![id]);
        app.envelope_mesh = (4, 2);
        app.add_mesh_envelope();
        let effects = |app: &App| app.doc().find_shape(id).expect("shape").effects.clone();
        let Some(Effect::MeshEnvelope { cols, rows, points }) = effects(&app).last().cloned() else {
            panic!("no mesh: {:?}", effects(&app));
        };
        assert_eq!((cols, rows, points.len()), (4, 2, 15));
        let b = bounds(&app, id);
        assert!((b.width() - 40.0).abs() < 1e-6 && (b.height() - 20.0).abs() < 1e-6, "{b:?}");
        // The top middle point pulled up 10 mm.
        let mut moved = points.clone();
        moved[2 * 5 + 2].y += 10.0;
        let mut e = effects(&app);
        e.pop();
        e.push(Effect::MeshEnvelope { cols, rows, points: moved });
        app.engine
            .run(&Command::SetEffects { shape: id, effects: e })
            .expect("set");
        let b = bounds(&app, id);
        assert!(b.height() > 29.0, "{b:?}");
        // The Envelope tool finds the grid point.
        app.set_tool(crate::tools::Tool::Envelope);
        let s = app.doc().shape_on_page(id).expect("shape");
        let at = crate::interaction2::effect_point(&s, crate::interaction2::MESH_ENVELOPE_BASE + 12)
            .expect("point");
        assert_eq!(
            app.effect_node_at(at),
            Some((id, crate::interaction2::MESH_ENVELOPE_BASE + 12))
        );
        // Add Mesh again replaces it; Clear takes it off.
        app.add_mesh_envelope();
        assert_eq!(
            effects(&app).iter().filter(|e| matches!(e, Effect::MeshEnvelope { .. })).count(),
            1
        );
        app.clear_envelope();
        assert!(effects(&app).is_empty());
    }

    /// Envelope from Top Object: the top object's outline is the others'
    /// envelope where it lies, and it goes, in one undo step.
    #[test]
    fn the_top_object_becomes_the_envelope_of_the_others() {
        let mut app = App::headless();
        let under = rect(&mut app, 0.0, 0.0, 40.0, 20.0);
        let top = app
            .new_shape(ShapeKind::Ellipse {
                rect: Rect::new(100.0, 100.0, 160.0, 130.0),
                arc: None,
            })
            .expect("top");
        app.select(vec![top, under]);
        let depth = app.engine.history_labels().0.len();
        app.envelope_from_top_object();
        assert!(app.doc().find_shape(top).is_none());
        assert!(app.selection == vec![under]);
        let b = bounds(&app, under);
        assert!((b.x0 - 100.0).abs() < 0.5 && (b.x1 - 160.0).abs() < 0.5, "{b:?}");
        assert!((b.y0 - 100.0).abs() < 0.5 && (b.y1 - 130.0).abs() < 0.5, "{b:?}");
        assert_eq!(app.engine.history_labels().0.len(), depth + 1);
    }

    /// Several envelope nodes can be chosen, by Shift+click or a marquee
    /// of the Envelope tool, and a drag on one of them moves them all,
    /// in one undo step (#3001).
    #[test]
    fn several_envelope_nodes_move_together() {
        let mut app = App::headless();
        let id = rect(&mut app, 0.0, 0.0, 40.0, 20.0);
        app.select(vec![id]);
        app.add_new_envelope();
        app.envelope_mode = Mode::Unconstrained;
        let nodes = |app: &App| app.envelope_of(id).expect("envelope").1.nodes.clone();
        let page = |app: &App, i: usize| {
            let s = app.doc().shape_on_page(id).expect("shape");
            s.transform * nodes(app)[i]
        };
        let before = nodes(&app);
        // Nodes 0 and 4 (two corners) chosen, then a third added and
        // taken out again.
        app.choose_envelope_nodes(id, &[0]);
        app.toggle_envelope_node(id, 4);
        app.toggle_envelope_node(id, 2);
        app.toggle_envelope_node(id, 2);
        assert_eq!(app.selected_envelope_nodes(), Some((id, vec![0, 4])));
        let depth = app.engine.history_labels().0.len();
        let from = page(&app, 0);
        app.begin_envelope_drag(id, Part::Node(0), from);
        app.envelope_drag_to(from + Vec2::new(3.0, 0.0));
        app.envelope_drag_to(from + Vec2::new(5.0, 2.0));
        app.envelope_drag = None;
        let after = nodes(&app);
        for i in [0, 4] {
            let d = after[i] - before[i];
            assert!((d.hypot() - Vec2::new(5.0, 2.0).hypot()).abs() < 1e-6, "node {i}: {d:?}");
        }
        assert_eq!(after[2], before[2], "a node not chosen stays");
        assert_eq!(app.engine.history_labels().0.len(), depth + 1, "one undo step");
        // A marquee round the bottom side chooses its three nodes; with
        // Shift it adds to them.
        let s = app.doc().shape_on_page(id).expect("shape");
        let low: Vec<Point> = [1, 2].iter().map(|&i| s.transform * after[i]).collect();
        let r = Rect::from_points(low[0], low[1]).inflate(1.0, 1.0);
        app.choose_envelope_nodes_in(r, false);
        assert_eq!(app.selected_envelope_nodes(), Some((id, vec![1, 2])));
        let r0 = Rect::from_points(s.transform * after[0], s.transform * after[0]).inflate(1.0, 1.0);
        app.choose_envelope_nodes_in(r0, true);
        assert_eq!(app.selected_envelope_nodes(), Some((id, vec![1, 2, 0])));
        // A drag on a node not chosen chooses it alone.
        let p6 = page(&app, 6);
        app.begin_envelope_drag(id, Part::Node(6), p6);
        app.envelope_drag = None;
        assert_eq!(app.selected_envelope_nodes(), Some((id, vec![6])));
        // An empty marquee chooses nothing.
        app.choose_envelope_nodes_in(Rect::new(500.0, 500.0, 501.0, 501.0), false);
        assert_eq!(app.selected_envelope_nodes(), None);
    }

    /// A drag moves a node the chosen way from where the drag began, the
    /// whole drag one undo step; a second drag is a second step.
    #[test]
    fn dragging_a_node_is_one_step_in_the_chosen_mode() {
        let mut app = App::headless();
        let id = rect(&mut app, 0.0, 0.0, 40.0, 20.0);
        app.select(vec![id]);
        app.add_new_envelope();
        assert!(app.envelope_of(id).is_some());
        let depth = app.engine.history_labels().0.len();
        app.envelope_mode = Mode::StraightLine;
        // The top's middle node, (20, 20), up by 10 in three steps.
        app.begin_envelope_drag(id, Part::Node(5), Point::new(20.0, 20.0));
        for y in [22.0, 26.0, 30.0] {
            app.envelope_drag_to(Point::new(20.0, y));
        }
        app.envelope_drag = None;
        assert_eq!(app.engine.history_labels().0.len(), depth + 1);
        let (_, f) = app.envelope_of(id).expect("envelope");
        assert!((f.nodes[5].y - 30.0).abs() < 1e-9);
        // Straight from the corner to the raised middle.
        let mid = f.segment(4).eval(0.5);
        assert!((mid.y - 25.0).abs() < 1e-9, "{mid:?}");
        assert!((bounds(&app, id).y1 - 30.0).abs() < 1e-6);
        // Another drag is another step.
        app.begin_envelope_drag(id, Part::Node(1), Point::new(20.0, 0.0));
        app.envelope_drag_to(Point::new(20.0, -5.0));
        app.envelope_drag = None;
        assert_eq!(app.engine.history_labels().0.len(), depth + 2);
        app.undo();
        assert!((app.envelope_of(id).expect("env").1.nodes[1].y).abs() < 1e-9);
    }

    /// The Envelope tool on an extruded object (#2214): the frame goes
    /// around the object, and dragging a node bends the body with it. The
    /// far face stayed a rectangle behind the bent object.
    #[test]
    fn an_envelope_bends_the_body_of_an_extruded_object() {
        let mut app = App::headless();
        let id = rect(&mut app, 0.0, 0.0, 40.0, 20.0);
        let extrude = Effect::Extrude {
            depth: tracedraw_core::geometry::Vec2::new(10.0, -10.0),
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
        };
        app.engine
            .run(&Command::SetEffects {
                shape: id,
                effects: vec![extrude],
            })
            .expect("extrude");
        app.select(vec![id]);
        app.add_new_envelope();
        let (at, f) = app.envelope_of(id).expect("envelope");
        assert_eq!(at, 1);
        assert_eq!(f.bounds(), Rect::new(0.0, 0.0, 40.0, 20.0));
        // The top's middle node up by 10: an arch.
        app.envelope_mode = Mode::SingleArc;
        app.begin_envelope_drag(id, Part::Node(5), Point::new(20.0, 20.0));
        app.envelope_drag_to(Point::new(20.0, 30.0));
        app.envelope_drag = None;
        let s = app.doc().shape_on_page(id).expect("shape");
        let ev = tracedraw_core::live::evaluate(&s);
        assert!((ev.main.bounds().y1 - 30.0).abs() < 1e-6);
        // The far face is the arch moved by the offset: its top at 30 - 10.
        let far = ev.below.first().expect("the far face").bounds();
        assert!((far.y1 - 20.0).abs() < 0.05, "{far:?}");
        assert!((far.x0 - 10.0).abs() < 0.05 && (far.y0 + 10.0).abs() < 0.05);
    }

    /// #2213: with the Shape tool an enveloped curve shows its frame
    /// only. Its own nodes lie where the curve was before the envelope,
    /// so they, the curve's outline in the node colour and the corner
    /// widgets stood as a second, unbent curve beside the bent one.
    /// Nothing of the unbent curve is shown or taken while the envelope
    /// is on; the frame is edited, and the nodes are back once the
    /// envelope is cleared.
    #[test]
    fn the_shape_tool_shows_only_the_frame_of_an_enveloped_curve() {
        let mut app = App::headless();
        let id = rect(&mut app, 0.0, 0.0, 40.0, 20.0);
        app.select(vec![id]);
        app.convert_selection_to_curves(false);
        app.tool = crate::tools::Tool::Shape;
        app.settings.nodes.corner_widgets = true;
        // A plain curve: its nodes, segments and corner widgets.
        let corner = Point::new(40.0, 20.0);
        assert_eq!(app.shape_tool_shapes().len(), 1);
        assert!(app.node_at(corner).is_some());
        assert!(app.segment_at(Point::new(20.0, 20.0)).is_some());
        assert!(!app.corner_widgets().is_empty());
        // A node chosen before the envelope is let go with it.
        app.node_selection = vec![(id, 2)];
        app.add_new_envelope();
        // The top right corner of the frame pulled far out: the curve is
        // drawn out to there, its own node stays at (40, 20).
        app.envelope_mode = Mode::Unconstrained;
        app.begin_envelope_drag(id, Part::Node(4), corner);
        app.envelope_drag_to(Point::new(90.0, 70.0));
        app.envelope_drag = None;
        assert!(bounds(&app, id).x1 > 80.0);
        assert!(app.shape_tool_shapes().is_empty());
        assert!(app.node_at(corner).is_none());
        assert!(app.segment_at(Point::new(20.0, 20.0)).is_none());
        assert!(!app.near_selected_nodes(corner));
        assert!(app.corner_widgets().is_empty());
        app.drop_hidden_node_choices();
        assert!(app.node_selection.is_empty());
        app.select_all_nodes();
        assert!(app.node_selection.is_empty());
        // The frame is what the tool takes.
        assert_eq!(
            app.envelope_part_at(Point::new(90.0, 70.0)),
            Some((id, Part::Node(4)))
        );
        // A perspective bends the outline as well.
        let effects = app.doc().find_shape(id).expect("shape").effects.clone();
        app.run(Command::SetEffects {
            shape: id,
            effects: vec![Effect::Perspective {
                corners: [
                    Point::new(0.0, 0.0),
                    Point::new(40.0, 5.0),
                    Point::new(40.0, 15.0),
                    Point::new(0.0, 20.0),
                ],
            }],
        });
        assert!(app.shape_tool_shapes().is_empty());
        assert!(app.node_at(corner).is_none());
        app.run(Command::SetEffects { shape: id, effects });
        // Envelope cleared: the nodes are back.
        app.clear_envelope();
        assert!(app.envelope_of(id).is_none());
        assert_eq!(app.shape_tool_shapes().len(), 1);
        assert!(app.node_at(corner).is_some());
        assert!(!app.corner_widgets().is_empty());
    }

    /// In the Unconstrained mode the selected node's handles show and move
    /// on their own; nodes come and go with double-clicks.
    #[test]
    fn handles_and_nodes_in_the_unconstrained_mode() {
        let mut app = App::headless();
        let id = rect(&mut app, 0.0, 0.0, 40.0, 20.0);
        app.select(vec![id]);
        app.add_new_envelope();
        app.envelope_mode = Mode::Unconstrained;
        assert!(app
            .envelope_handles(id, &app.envelope_of(id).expect("e").1)
            .is_empty());
        app.selected_effect_node = Some((id, 5));
        let (_, f) = app.envelope_of(id).expect("e");
        let handles = app.envelope_handles(id, &f);
        assert_eq!(handles.len(), 2);
        let (seg, end, h, _) = handles[0];
        assert_eq!((seg, end), (5, 0));
        app.begin_envelope_drag(id, Part::Handle { seg, end }, h);
        app.envelope_drag_to(h + Vec2::new(0.0, 8.0));
        app.envelope_drag = None;
        let (_, f2) = app.envelope_of(id).expect("e");
        assert!((f2.handles[5][0].y - (h.y + 8.0)).abs() < 1e-9);
        assert_eq!(f2.nodes, f.nodes);
        // No handles in the other modes.
        app.envelope_mode = Mode::SingleArc;
        assert!(app.envelope_handles(id, &f2).is_empty());
        // A double-click on the bottom adds a node; on it, removes it; a
        // corner stays.
        app.view.zoom = 1.0;
        assert!(app.envelope_double_click(Point::new(10.0, 0.0)));
        assert_eq!(app.envelope_of(id).expect("e").1.len(), 9);
        assert!(app.envelope_double_click(Point::new(10.0, 0.0)));
        assert_eq!(app.envelope_of(id).expect("e").1.len(), 8);
        assert!(!app.envelope_double_click(Point::new(0.0, 0.0)));
        assert_eq!(app.envelope_of(id).expect("e").1.len(), 8);
    }

    /// An enveloped object is picked and boxed where it is drawn: a click
    /// on the arch above the rectangle selects it, and the selection's
    /// bounds reach the top of the arch.
    #[test]
    fn enveloped_objects_are_hit_and_boxed_as_drawn() {
        let mut app = App::headless();
        let id = rect(&mut app, 0.0, 0.0, 40.0, 20.0);
        app.select(vec![id]);
        app.apply_envelope_preset(Preset::Arch);
        app.select(Vec::new());
        app.view.zoom = 1.0;
        // Above the rectangle, inside the arch (rectangles are filled).
        let fill = tracedraw_core::Fill::Solid(tracedraw_core::Color::BLACK);
        app.run(Command::SetFill {
            shapes: vec![id],
            fill,
        });
        assert_eq!(app.hit_test(Point::new(20.0, 24.0)), Some(id));
        assert_eq!(app.hit_test(Point::new(20.0, 40.0)), None);
        app.select(vec![id]);
        let b = app.selection_bounds().expect("bounds");
        assert!((b.y1 - 26.0).abs() < 0.1, "{b:?}");
        // A marquee around the drawn object takes it; one around the
        // rectangle alone does not.
        use crate::select_region::{takes, Region};
        let s = app.doc().shape_on_page(id).expect("shape");
        let all = |_: &tracedraw_core::Shape| true;
        assert!(takes(
            Region::Rect(Rect::new(-1.0, -1.0, 41.0, 27.0)),
            &s,
            false,
            &all
        ));
        assert!(!takes(
            Region::Rect(Rect::new(-1.0, -1.0, 41.0, 21.0)),
            &s,
            false,
            &all
        ));
    }

    /// Presets, mapping, Keep Lines, Clear and Copy work on the selection.
    #[test]
    fn presets_settings_clear_and_copy() {
        let mut app = App::headless();
        let a = rect(&mut app, 0.0, 0.0, 40.0, 20.0);
        let b = rect(&mut app, 100.0, 0.0, 120.0, 40.0);
        app.select(vec![a]);
        app.apply_envelope_preset(Preset::Arch);
        assert!(
            (bounds(&app, a).y1 - 26.0).abs() < 0.1,
            "{:?}",
            bounds(&app, a)
        );
        app.set_envelope_mapping(Mapping::Vertical);
        app.set_envelope_keep_lines(true);
        assert_eq!(app.envelope_settings_shown(), (Mapping::Vertical, true));
        // Copy onto b: the arch stretched to b's box.
        app.select(vec![b]);
        app.copy_envelope_from(a);
        let (_, f) = app.envelope_of(b).expect("copied");
        assert!(
            (f.nodes[5] - Point::new(110.0, 52.0)).hypot() < 1e-6,
            "{:?}",
            f.nodes[5]
        );
        assert_eq!(app.envelope_settings_shown(), (Mapping::Vertical, true));
        // Add New goes over the first; Clear takes the last away.
        app.add_new_envelope();
        let n = |app: &App| {
            app.doc()
                .find_shape(b)
                .map(|s| {
                    s.effects
                        .iter()
                        .filter(|e| e.envelope_frame().is_some())
                        .count()
                })
                .unwrap_or(0)
        };
        assert_eq!(n(&app), 2);
        app.clear_envelope();
        assert_eq!(n(&app), 1);
        app.clear_envelope();
        assert_eq!(n(&app), 0);
        // Every preset makes a frame around the box, corners included.
        for p in Preset::ALL {
            let f = p.frame(Rect::new(0.0, 0.0, 10.0, 10.0));
            assert_eq!(f.len(), 8, "{p:?}");
        }
    }

    /// Copy Envelope Properties from a band with no envelope: as in
    /// CorelDRAW each object takes the band's shape at the band's size,
    /// its corners at the band's four corners (#2344), centred where the
    /// object was, a moved and turned one too. It was moved into the band
    /// (#2178: text above an arched band went down onto it, where in
    /// CorelDRAW it stays above it, arched like it).
    #[test]
    fn an_object_taking_a_shapes_outline_keeps_its_place() {
        let mut app = App::headless();
        let mut band = tracedraw_core::BezPath::new();
        band.move_to((110.0, 100.0));
        band.curve_to((116.0, 104.0), (124.0, 104.0), (130.0, 100.0));
        band.line_to((140.0, 130.0));
        band.curve_to((128.0, 136.0), (112.0, 136.0), (100.0, 130.0));
        band.close_path();
        let src = app
            .new_shape(ShapeKind::Path {
                path: band.clone(),
                closed: true,
            })
            .expect("band");
        let a = rect(&mut app, 0.0, 0.0, 40.0, 20.0);
        let b = rect(&mut app, 0.0, 0.0, 30.0, 30.0);
        app.engine
            .run(&Command::TransformShapes {
                shapes: vec![b],
                transform: Affine::translate((50.0, 200.0)) * Affine::rotate(0.3),
            })
            .expect("moved");
        let centres = [bounds(&app, a).center(), bounds(&app, b).center()];
        app.select(vec![a, b]);
        app.copy_envelope_from(src);
        use tracedraw_core::geometry::Shape as _;
        let bb = band.bounding_box();
        for (id, c) in [a, b].into_iter().zip(centres) {
            let want = bb + (c - bb.center());
            let got = bounds(&app, id);
            assert!(
                (got.x0 - want.x0).abs() < 0.05
                    && (got.x1 - want.x1).abs() < 0.05
                    && (got.y0 - want.y0).abs() < 0.05
                    && (got.y1 - want.y1).abs() < 0.05,
                "{got:?} {want:?}"
            );
            let s = page_shape(&app, id);
            let drawn = tracedraw_core::live::evaluate(&s).main.page_path();
            let area = drawn.area().abs();
            assert!(
                (area - band.area().abs()).abs() < band.area().abs() * 0.02,
                "{area}"
            );
        }
    }

    fn page_shape(app: &App, id: ShapeId) -> tracedraw_core::Shape {
        app.doc().shape_on_page(id).expect("shape")
    }

    /// Convert to Curves makes the envelope part of the curve (#647): it
    /// used to do nothing on an enveloped curve, and a rectangle lost its
    /// envelope's shape. One undo step brings the envelope back.
    #[test]
    fn convert_to_curves_bakes_the_envelope() {
        let mut app = App::headless();
        let id = rect(&mut app, 0.0, 0.0, 40.0, 20.0);
        app.select(vec![id]);
        app.convert_to_curves();
        assert!(!app.can_convert_to_curves());
        app.apply_envelope_preset(Preset::Arch);
        assert!(app.can_convert_to_curves());
        app.convert_to_curves();
        let s = page_shape(&app, id);
        assert!(matches!(s.kind, ShapeKind::Path { .. }), "{:?}", s.kind);
        assert!(s.effects.is_empty() && app.envelope_of(id).is_none());
        assert!((s.bounds().y1 - 26.0).abs() < 0.1, "{:?}", s.bounds());
        assert_eq!(app.engine.undo_label(), Some("Convert To Curves"));
        assert!(!app.can_convert_to_curves());
        app.undo();
        assert!(app.envelope_of(id).is_some());
        // A rectangle with an envelope becomes the warped curve at once.
        let r = rect(&mut app, 100.0, 0.0, 140.0, 20.0);
        app.select(vec![r]);
        app.apply_envelope_preset(Preset::Arch);
        app.convert_to_curves();
        let s = page_shape(&app, r);
        assert!(matches!(s.kind, ShapeKind::Path { .. }), "{:?}", s.kind);
        assert!(s.effects.is_empty());
        assert!((s.bounds().y1 - 26.0).abs() < 0.1, "{:?}", s.bounds());
        assert!((s.bounds().x0 - 100.0).abs() < 0.1, "{:?}", s.bounds());
    }

    /// Break Apart of an enveloped curve breaks the warped curve: the
    /// pieces are drawn where the whole was, with no envelope left (they
    /// used to lose the warp).
    #[test]
    fn break_apart_bakes_an_enveloped_curve() {
        let mut app = App::headless();
        let a = rect(&mut app, 0.0, 0.0, 15.0, 20.0);
        let b = rect(&mut app, 25.0, 0.0, 40.0, 20.0);
        app.select(vec![a, b]);
        app.combine();
        assert_eq!(app.selection.len(), 1);
        app.apply_envelope_preset(Preset::Arch);
        let drawn = app.selection_bounds().expect("bounds");
        assert!(drawn.y1 > 25.0, "{drawn:?}");
        app.break_apart();
        assert_eq!(app.selection.len(), 2, "{:?}", app.selection);
        let mut all: Option<Rect> = None;
        for id in app.selection.clone() {
            let s = page_shape(&app, id);
            assert!(s.effects.is_empty(), "{:?}", s.effects);
            assert!(matches!(s.kind, ShapeKind::Path { .. }));
            all = Some(all.map_or(s.bounds(), |u| u.union(s.bounds())));
        }
        let all = all.expect("pieces");
        for (got, want) in [
            (all.x0, drawn.x0),
            (all.y0, drawn.y0),
            (all.x1, drawn.x1),
            (all.y1, drawn.y1),
        ] {
            assert!((got - want).abs() < 0.1, "{all:?} {drawn:?}");
        }
        assert_eq!(app.engine.undo_label(), Some("Break Apart"));
    }

    /// An envelope on a group bakes into each of its objects: the group
    /// stays, with the same objects, now curves.
    #[test]
    fn an_enveloped_group_converts_object_by_object() {
        let mut app = App::headless();
        let a = rect(&mut app, 0.0, 0.0, 15.0, 20.0);
        let b = rect(&mut app, 25.0, 0.0, 40.0, 20.0);
        app.select(vec![a, b]);
        app.group_selection();
        let g = app.selection[0];
        app.apply_envelope_preset(Preset::Arch);
        assert!(app.envelope_of(g).is_some());
        assert!(app.can_convert_to_curves());
        app.convert_to_curves();
        let s = page_shape(&app, g);
        assert!(s.effects.is_empty() && app.envelope_of(g).is_none());
        let ShapeKind::Group { children } = &s.kind else {
            panic!("still a group: {:?}", s.kind);
        };
        let mut ids: Vec<ShapeId> = children.iter().map(|c| c.id).collect();
        ids.sort_by_key(|id| id.0);
        let mut want = vec![a, b];
        want.sort_by_key(|id| id.0);
        assert_eq!(ids, want);
        assert!(children
            .iter()
            .all(|c| matches!(c.kind, ShapeKind::Path { .. })));
        assert!(s.bounds().y1 > 25.0, "{:?}", s.bounds());
    }

    /// Break Apart has nothing to break in a group: it keeps its envelope
    /// and its objects (Convert to Curves bakes it).
    #[test]
    fn break_apart_leaves_an_enveloped_group() {
        let mut app = App::headless();
        let a = rect(&mut app, 0.0, 0.0, 15.0, 20.0);
        let b = rect(&mut app, 25.0, 0.0, 40.0, 20.0);
        app.select(vec![a, b]);
        app.group_selection();
        let g = app.selection[0];
        app.apply_envelope_preset(Preset::Arch);
        let before = page_shape(&app, g);
        app.break_apart();
        assert_eq!(page_shape(&app, g).kind, before.kind);
        assert!(app.envelope_of(g).is_some());
    }

    /// A brush press on an enveloped group leaves it as it is: brushes
    /// never touch groups, so nothing is converted or baked.
    #[test]
    fn a_brush_press_leaves_an_enveloped_group() {
        let mut app = App::headless();
        let a = rect(&mut app, 0.0, 0.0, 15.0, 20.0);
        let b = rect(&mut app, 25.0, 0.0, 40.0, 20.0);
        app.select(vec![a, b]);
        app.group_selection();
        let g = app.selection[0];
        app.apply_envelope_preset(Preset::Arch);
        let before = page_shape(&app, g);
        let depth = app.engine.history_labels().0.len();
        app.brush_begin(Point::new(10.0, 10.0));
        let after = page_shape(&app, g);
        assert_eq!(after.kind, before.kind);
        assert_eq!(after.effects, before.effects);
        assert_eq!(app.engine.history_labels().0.len(), depth);
    }
}
