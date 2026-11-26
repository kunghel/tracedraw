//! Blends keep their end object as an object of the drawing. The start
//! object holds the Blend effect, which names the end object (`end_id`)
//! and keeps a copy of it in page space for drawing the steps. After
//! every change of the drawing the copy is brought up to date, in the
//! same undo step, so moving, resizing or recolouring either object
//! redraws the steps; deleting the end object takes the blend away.
//!
//! The Blend bar and panel edit the blends of the selection: a selected
//! start object's, or the blend whose end object is selected.

use crate::app::App;
use tracedraw_core::{document::Shape, live::Effect, Command, ShapeId, ShapeKind};

/// The end object as the blend keeps it: in page space, its own effects
/// left out (a blend of a blend does not draw twice), but for envelopes
/// and perspectives, which bend it where it is drawn: the steps run to
/// the end as it looks, as they do in a group the envelope bends (#3969).
pub fn end_copy(s: &Shape) -> Shape {
    let mut e = s.clone();
    e.effects.retain(|f| {
        matches!(
            f,
            Effect::Envelope { .. } | Effect::Perspective { .. } | Effect::MeshEnvelope { .. }
        )
    });
    e.parked_effects.clear();
    e
}

fn is_blend(e: &Effect) -> bool {
    matches!(e, Effect::Blend { .. })
}

/// The first object holding a blend inside the group `s`, at any depth.
fn blend_start_in(s: &Shape) -> Option<ShapeId> {
    let ShapeKind::Group { children } = &s.kind else {
        return None;
    };
    children.iter().find_map(|c| {
        if c.effects.iter().any(is_blend) {
            Some(c.id)
        } else {
            blend_start_in(c)
        }
    })
}

/// `effects` with the blend replaced in place by `b` (removed with
/// `None`).
fn with_blend(effects: &[Effect], b: Option<Effect>) -> Vec<Effect> {
    let mut out = Vec::with_capacity(effects.len());
    let mut placed = false;
    for e in effects {
        if is_blend(e) {
            if !placed {
                out.extend(b.clone());
                placed = true;
            }
        } else {
            out.push(e.clone());
        }
    }
    if !placed {
        out.extend(b);
    }
    out
}

impl App {
    /// Blends among `pasted` (inside groups and clip frames too) whose end
    /// object did not come along and is not in this drawing, the same
    /// object, keep their steps as they are, linked to no end object.
    /// Pasted in another drawing they named whatever object had the end's
    /// number there and blended into it at the next change.
    pub(crate) fn unlink_foreign_blend_ends(&self, pasted: &mut [Shape]) {
        fn ids(s: &Shape, out: &mut Vec<ShapeId>) {
            out.push(s.id);
            match &s.kind {
                ShapeKind::Group { children } => children.iter().for_each(|c| ids(c, out)),
                ShapeKind::ClipFrame { frame, contents } => {
                    ids(frame, out);
                    contents.iter().for_each(|c| ids(c, out));
                }
                _ => {}
            }
        }
        fn walk(s: &mut Shape, keep: &dyn Fn(ShapeId, &Shape) -> bool) {
            for e in &mut s.effects {
                if let Effect::Blend {
                    end,
                    end_id: Some(id),
                    ..
                } = e
                {
                    if !keep(*id, end) {
                        if let Effect::Blend { end_id, .. } = e {
                            *end_id = None;
                        }
                    }
                }
            }
            match &mut s.kind {
                ShapeKind::Group { children } => children.iter_mut().for_each(|c| walk(c, keep)),
                ShapeKind::ClipFrame { frame, contents } => {
                    walk(frame, keep);
                    contents.iter_mut().for_each(|c| walk(c, keep));
                }
                _ => {}
            }
        }
        let mut along = Vec::new();
        for s in pasted.iter() {
            ids(s, &mut along);
        }
        let keep = |id: ShapeId, end: &Shape| {
            along.contains(&id)
                || self
                    .doc()
                    .find_shape(id)
                    .is_some_and(|here| here.uid.is_some() && here.uid == end.uid)
        };
        for s in pasted.iter_mut() {
            walk(s, &keep);
        }
    }

    /// Whether `a` is drawn below `b` on the page: an earlier layer, or
    /// earlier in the order the page is painted, at any depth (inside
    /// groups and clip frames too, the frame before its contents). False
    /// when either is not on the page. Only top-level objects counted
    /// before, so a blend that was grouped or placed in a frame had its
    /// steps moved in front of its start object at the next change of
    /// the drawing (#2023, #2120).
    pub(crate) fn stacked_below(&self, a: ShapeId, b: ShapeId) -> bool {
        fn walk(s: &Shape, a: ShapeId, b: ShapeId, n: &mut usize, at: &mut [Option<usize>; 2]) {
            if s.id == a {
                at[0] = Some(*n);
            }
            if s.id == b {
                at[1] = Some(*n);
            }
            *n += 1;
            match &s.kind {
                ShapeKind::Group { children } => children.iter().for_each(|c| walk(c, a, b, n, at)),
                ShapeKind::ClipFrame { frame, contents } => {
                    walk(frame, a, b, n, at);
                    contents.iter().for_each(|c| walk(c, a, b, n, at));
                }
                _ => {}
            }
        }
        let mut n = 0;
        let mut at = [None, None];
        for l in self.page_layers() {
            for s in &l.shapes {
                walk(s, a, b, &mut n, &mut at);
            }
        }
        match at {
            [Some(pa), Some(pb)] => pa < pb,
            _ => false,
        }
    }

    /// Every object with a blend, and its blend.
    fn all_blends(&self) -> Vec<(ShapeId, Effect)> {
        fn walk(s: &Shape, out: &mut Vec<(ShapeId, Effect)>) {
            if let Some(e) = s.effects.iter().find(|e| is_blend(e)) {
                out.push((s.id, e.clone()));
            }
            match &s.kind {
                ShapeKind::Group { children } => children.iter().for_each(|c| walk(c, out)),
                ShapeKind::ClipFrame { frame, contents } => {
                    walk(frame, out);
                    contents.iter().for_each(|c| walk(c, out));
                }
                _ => {}
            }
        }
        let mut out = Vec::new();
        for l in self.doc().all_layers() {
            for s in &l.shapes {
                walk(s, &mut out);
            }
        }
        out
    }

    /// A double click on a group bent by an envelope or a perspective
    /// that holds a blend (#3968): the group comes into focus with the
    /// blend's start object selected and the Blend panel open, so the
    /// blend is changed where it is, envelope and group kept. False when
    /// `id` is not such a group.
    pub fn edit_bent_blend(&mut self, id: ShapeId) -> bool {
        let Some(group) = self.doc().find_shape(id) else {
            return false;
        };
        let bent = group.effects.iter().any(|e| {
            matches!(
                e,
                Effect::Envelope { .. } | Effect::Perspective { .. } | Effect::MeshEnvelope { .. }
            )
        });
        if !bent || !matches!(group.kind, ShapeKind::Group { .. }) {
            return false;
        }
        let Some(start) = blend_start_in(group) else {
            return false;
        };
        self.focus_on(id);
        self.select(vec![start]);
        self.open_docker(crate::app::DockerTab::Blend);
        true
    }

    /// The blends of the selection: those of selected start objects and
    /// those whose end object is selected (start object, blend), each
    /// once.
    pub fn selected_blends(&self) -> Vec<(ShapeId, Effect)> {
        let mut out: Vec<(ShapeId, Effect)> = Vec::new();
        for (id, e) in self.all_blends() {
            let end = match &e {
                Effect::Blend { end_id, .. } => *end_id,
                _ => None,
            };
            let chosen = self.selection.contains(&id)
                || end.is_some_and(|end| self.selection.contains(&end));
            if chosen && !out.iter().any(|(o, _)| *o == id) {
                out.push((id, e));
            }
        }
        out
    }

    /// Change the selection's blends with `f` (`None` clears one), one
    /// undo step; a run of the same change on the same blends with
    /// nothing in between is one step.
    pub fn change_blends(&mut self, label: &'static str, f: impl Fn(&Effect) -> Option<Effect>) {
        let mut cmds = Vec::new();
        let mut ids = Vec::new();
        for (id, e) in self.selected_blends() {
            let next = f(&e);
            if next.as_ref() == Some(&e) {
                continue;
            }
            let Some(s) = self.doc().find_shape(id) else {
                continue;
            };
            ids.push(id);
            cmds.push(Command::SetEffects {
                shape: id,
                effects: with_blend(&s.effects, next),
            });
        }
        if cmds.is_empty() {
            return;
        }
        let key = (self.engine.revision(), ids.clone(), label.to_string());
        if self.engine.undo_label() == Some(label) && self.effect_edit_step.as_ref() == Some(&key) {
            let _ = self.engine.undo();
        }
        if let Err(e) = self.engine.run_batch(label, &cmds) {
            self.status = e.to_string();
            return;
        }
        self.effect_edit_step = Some((self.engine.revision(), ids, label.to_string()));
    }

    /// Place the selected blends' start and end objects `from` and `to`
    /// of the way along their paths (shares of its length, 0 to 1): each
    /// object's centre goes onto the path there and the steps run between
    /// them along it, one undo step for a run of changes (#2946).
    pub fn set_blend_path_ends(&mut self, from: f64, to: f64) {
        let label = "Blend Path Ends";
        let range = [from.clamp(0.0, 1.0), to.clamp(0.0, 1.0)];
        // A run of changes is one step: the last one is taken back first,
        // and the objects move from where they stood before the run.
        let ids_now: Vec<ShapeId> = self.selected_blends().iter().map(|(id, _)| *id).collect();
        let key = (self.engine.revision(), ids_now, label.to_string());
        if self.engine.undo_label() == Some(label) && self.effect_edit_step.as_ref() == Some(&key) {
            let _ = self.engine.undo();
            self.blends_checked = self.engine.revision();
        }
        let mut cmds = Vec::new();
        for (id, e) in self.selected_blends() {
            let Effect::Blend {
                path: Some(path),
                end_id,
                options,
                ..
            } = &e
            else {
                continue;
            };
            let Some(start) = self.doc().find_shape(id).cloned() else {
                continue;
            };
            let mut b = e.clone();
            if let Effect::Blend { options: o, .. } = &mut b {
                o.path_range = Some(range);
            }
            if options.path_range != Some(range) {
                cmds.push(Command::SetEffects {
                    shape: id,
                    effects: with_blend(&start.effects, Some(b)),
                });
            }
            // Each object's centre onto the path (in the space the path
            // and the objects share: the page for objects on a layer).
            let mut place = |shape: Option<&Shape>, share: f64| {
                let Some(shape) = shape else {
                    return;
                };
                let Some((at, _)) = tracedraw_core::live::point_along(path, share) else {
                    return;
                };
                let c = shape.bounds().center();
                let d = at - c;
                if d.hypot() > 1e-6 {
                    cmds.push(Command::TransformShapes {
                        shapes: vec![shape.id],
                        transform: tracedraw_core::Affine::translate(d),
                    });
                }
            };
            place(Some(&start), range[0]);
            let end = end_id.and_then(|e| self.doc().find_shape(e).cloned());
            place(end.as_ref(), range[1]);
        }
        if cmds.is_empty() {
            return;
        }
        if let Err(e) = self.engine.run_batch(label, &cmds) {
            self.status = e.to_string();
            return;
        }
        // The blends' copies of their end objects follow now, in the same
        // step, so the next change of the run finds the drawing as it was.
        self.refresh_blends();
        let ids_now: Vec<ShapeId> = self.selected_blends().iter().map(|(id, _)| *id).collect();
        self.effect_edit_step = Some((self.engine.revision(), ids_now, label.to_string()));
    }

    /// Take the selected blends off their paths: straight blends again
    /// (#2946).
    pub fn detach_blend_path(&mut self) {
        self.change_blends("Detach From Path", |e| {
            let mut e = e.clone();
            if let Effect::Blend { path, options, .. } = &mut e {
                *path = None;
                options.path_range = None;
                options.full_path = false;
            }
            Some(e)
        });
    }

    /// Blends follow their end objects (see the module notes). Runs once
    /// per change of the drawing; the changes join the step that made it.
    pub fn refresh_blends(&mut self) {
        let revision = self.engine.revision();
        if revision == self.blends_checked {
            return;
        }
        let mut cmds = Vec::new();
        for (id, e) in self.all_blends() {
            let Effect::Blend {
                end,
                end_id: Some(end_id),
                ..
            } = &e
            else {
                continue;
            };
            let Some(start) = self.doc().find_shape(id) else {
                continue;
            };
            let next = match self.doc().shape_on_page(*end_id) {
                // The end object is gone: so is the blend.
                None => None,
                Some(now) => {
                    // The copy lives where the start object's effects do:
                    // in the space of the group or frame holding it, which
                    // is the page for an object on a layer. Kept in page
                    // space, the steps of a grouped blend were drawn
                    // through the group's transform twice and stayed
                    // behind when the group moved (#2023).
                    let mut copy = end_copy(&now);
                    let parent = self
                        .doc()
                        .parent_transform(id)
                        .unwrap_or(tracedraw_core::Affine::IDENTITY);
                    let det = parent.determinant();
                    if det.is_finite()
                        && det.abs() > 1e-12
                        && parent != tracedraw_core::Affine::IDENTITY
                    {
                        copy.transform = parent.inverse() * copy.transform;
                    }
                    // The steps go between the two objects: below the
                    // start when the end is below it.
                    let below = self.stacked_below(*end_id, id);
                    let was_below =
                        matches!(&e, Effect::Blend { options, .. } if options.steps_below);
                    if copy == **end && below == was_below {
                        continue;
                    }
                    let mut b = e.clone();
                    if let Effect::Blend { end, options, .. } = &mut b {
                        **end = copy;
                        options.steps_below = below;
                    }
                    Some(b)
                }
            };
            cmds.push(Command::SetEffects {
                shape: id,
                effects: with_blend(&start.effects, next),
            });
        }
        for cmd in &cmds {
            if let Err(e) = self.engine.amend(cmd) {
                self.status = e.to_string();
                break;
            }
        }
        self.blends_checked = self.engine.revision();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::{
        geometry::{Affine, Rect},
        Color, Fill,
    };

    fn square(app: &mut App, x: f64, c: Color) -> ShapeId {
        let id = app
            .new_shape(ShapeKind::Rect {
                rect: Rect::new(x, 0.0, x + 10.0, 10.0),
                radius: 0.0,
                corners: None,
            })
            .expect("square");
        app.run(Command::SetFill {
            shapes: vec![id],
            fill: Fill::Solid(c),
        });
        id
    }

    fn steps_of(app: &App, id: ShapeId) -> Vec<Shape> {
        let s = app.doc().find_shape(id).cloned().expect("start");
        tracedraw_core::live::evaluate(&s).above
    }

    /// #4389: three objects picked blend one into the next, in the order
    /// picked, in one undo step.
    #[test]
    fn three_objects_blend_as_a_chain() {
        let mut app = App::headless();
        let a = square(&mut app, 0.0, Color::BLACK);
        let b = square(&mut app, 100.0, Color::WHITE);
        let c = square(&mut app, 200.0, Color::BLACK);
        app.selection = vec![a, b, c];
        let depth = app.engine.history_labels().0.len();
        app.blend_selection(4, 0.0, 0.0, 0.0);
        assert_eq!(app.engine.history_labels().0.len(), depth + 1);
        let end_of = |app: &App, id| {
            app.doc().find_shape(id).and_then(|s| {
                s.effects.iter().find_map(|e| match e {
                    Effect::Blend { end_id, .. } => *end_id,
                    _ => None,
                })
            })
        };
        assert_eq!(end_of(&app, a), Some(b));
        assert_eq!(end_of(&app, b), Some(c));
        assert_eq!(end_of(&app, c), None);
        assert_eq!(app.selection, vec![a]);
    }

    /// A double click on a blend grouped and bent by the group's
    /// envelope opens the blend's options: the group in focus, the start
    /// object selected, the Blend panel shown; a change of the blend
    /// keeps the envelope and the group (#3968). A group with no envelope
    /// is left to the double click it had.
    #[test]
    fn a_double_click_on_a_bent_blend_group_edits_the_blend() {
        use tracedraw_core::geometry::Shape as _;
        let mut app = App::headless();
        let a = square(&mut app, 0.0, Color::BLACK);
        let b = square(&mut app, 100.0, Color::WHITE);
        app.selection = vec![a, b];
        app.blend_selection(4, 0.0, 0.0, 0.0);
        app.refresh_blends();
        app.select(vec![a, b]);
        app.group_selection();
        let g = app.selection[0];
        assert!(!app.edit_bent_blend(g), "no envelope yet");
        let bb = app.doc().find_shape(g).expect("group").local_path().bounding_box();
        let mut nodes = tracedraw_core::envelope::default_nodes(bb);
        nodes[1].y -= 30.0;
        let envelope = Effect::Envelope {
            nodes,
            keep_lines: false,
            handles: Vec::new(),
            corners: Vec::new(),
            mapping: tracedraw_core::envelope::Mapping::Putty,
            reference: None,
        };
        app.run(Command::SetEffects {
            shape: g,
            effects: vec![envelope.clone()],
        });
        app.refresh_blends();
        app.select(vec![g]);
        assert!(app.edit_bent_blend(g));
        assert_eq!(app.focus_target(), Some(g));
        assert_eq!(app.selection, vec![a]);
        assert!(app.docker_shown(crate::app::DockerTab::Blend));
        assert_eq!(app.selected_blends().len(), 1);
        app.change_blends("Blend", |e| match e {
            Effect::Blend { .. } => {
                let mut e = e.clone();
                if let Effect::Blend { steps, .. } = &mut e {
                    *steps = 9;
                }
                Some(e)
            }
            _ => None,
        });
        let group = app.doc().find_shape(g).cloned().expect("the group");
        assert_eq!(group.effects, vec![envelope]);
        let start = app.doc().find_shape(a).cloned().expect("start");
        assert!(start
            .effects
            .iter()
            .any(|e| matches!(e, Effect::Blend { steps: 9, .. })));
        // Nothing to edit in a bent group without a blend.
        let c = square(&mut app, 200.0, Color::BLACK);
        let d = square(&mut app, 220.0, Color::BLACK);
        app.exit_focus();
        app.select(vec![c, d]);
        app.group_selection();
        let plain = app.selection[0];
        app.run(Command::SetEffects {
            shape: plain,
            effects: vec![Effect::envelope_around(
                Rect::new(200.0, 0.0, 230.0, 10.0),
                false,
            )],
        });
        assert!(!app.edit_bent_blend(plain));
    }

    /// A blend grouped and bent by the group's envelope, then ungrouped,
    /// stays as it was drawn in the group: the start, the end and every
    /// step, each bent the same way. The envelope each object takes from
    /// the group (#3714) stays in the blend's copy of its end, so the
    /// steps run from the bent start to the bent end; they ran to the
    /// unbent end and came apart from both objects (#3969).
    #[test]
    fn an_ungrouped_bent_blend_keeps_its_steps() {
        for texts in [false, true] {
            bent_blend_ungrouped(texts);
        }
    }

    fn bent_blend_ungrouped(texts: bool) {
        use tracedraw_core::geometry::Shape as _;
        let mut app = App::headless();
        let text = |app: &mut App, x: f64| {
            app.new_shape(ShapeKind::Text {
                spans: vec![tracedraw_core::TextSpan::new("Bend", "Arial", 24.0)],
                origin: tracedraw_core::geometry::Point::new(x, 100.0),
                frame: None,
                align: tracedraw_core::TextAlign::Left,
                para: Default::default(),
                on_path: None,
            })
            .expect("a text")
        };
        let (a, b) = if texts {
            (text(&mut app, 0.0), text(&mut app, 150.0))
        } else {
            (
                square(&mut app, 0.0, Color::BLACK),
                square(&mut app, 100.0, Color::WHITE),
            )
        };
        app.selection = vec![a, b];
        app.blend_selection(4, 0.0, 0.0, 0.0);
        app.refresh_blends();
        app.select(vec![a, b]);
        app.group_selection();
        let g = app.selection[0];
        let bb = app.doc().find_shape(g).expect("group").local_path().bounding_box();
        let mut nodes = tracedraw_core::envelope::default_nodes(bb);
        nodes[1].y -= 30.0;
        nodes[5].y += 20.0;
        app.run(Command::SetEffects {
            shape: g,
            effects: vec![Effect::Envelope {
                nodes,
                keep_lines: false,
                handles: Vec::new(),
                corners: Vec::new(),
                mapping: tracedraw_core::envelope::Mapping::Putty,
                reference: None,
            }],
        });
        app.refresh_blends();
        // Everything the group draws, each object's box.
        let drawn = |s: &Shape| -> Vec<Rect> {
            let ev = tracedraw_core::live::evaluate(s);
            let mut out = vec![ev.main.visual_bounds()];
            out.extend(ev.above.iter().map(|x| x.visual_bounds()));
            out.extend(ev.below.iter().map(|x| x.visual_bounds()));
            out
        };
        let group = app.doc().find_shape(g).cloned().expect("group");
        let ShapeKind::Group { children } = &tracedraw_core::live::evaluate(&group).main.kind
        else {
            panic!("a group");
        };
        let mut before: Vec<Rect> = children
            .iter()
            .flat_map(|c| {
                let mut c = c.clone();
                c.transform_by(group.transform);
                drawn(&c)
            })
            .collect();
        app.select(vec![g]);
        app.ungroup_selection();
        app.refresh_blends();
        let mut after: Vec<Rect> = [a, b]
            .iter()
            .flat_map(|id| drawn(app.doc().find_shape(*id).expect("object")))
            .collect();
        let key = |r: &Rect| (r.center().x * 1000.0) as i64;
        before.sort_by_key(key);
        after.sort_by_key(key);
        assert_eq!(before.len(), after.len(), "{texts} {before:?}\n{after:?}");
        for (x, y) in before.iter().zip(&after) {
            for (p, q) in [(x.x0, y.x0), (x.y0, y.y0), (x.x1, y.x1), (x.y1, y.y1)] {
                assert!((p - q).abs() < 0.5, "{texts} {x:?} != {y:?}\n{before:?}\n{after:?}");
            }
        }
    }

    /// A blend copied with its end object (Paste, Duplicate) ends at the
    /// copy of the end: it named the original's end, so the copy moved
    /// away drew its steps back to the original. Pasted in another
    /// drawing, a blend whose end did not come along keeps its steps and
    /// no end object: it named whatever object had that number there and
    /// blended into it.
    #[test]
    fn copied_blends_end_at_their_own_copies() {
        let end_of = |app: &App, id: ShapeId| {
            app.doc().find_shape(id).and_then(|s| {
                s.effects.iter().find_map(|e| match e {
                    Effect::Blend { end_id, end, .. } => Some((*end_id, end.bounds())),
                    _ => None,
                })
            })
        };
        let blend = |app: &mut App| {
            let start = square(app, 0.0, Color::rgb8(250, 220, 0));
            let end = square(app, 100.0, Color::rgb8(0, 160, 80));
            app.select(vec![start, end]);
            app.blend_selection(3, 0.0, 0.0, 0.0);
            app.refresh_blends();
            (start, end)
        };
        for duplicate in [false, true] {
            let mut app = App::headless();
            let (start, end) = blend(&mut app);
            app.select(vec![start, end]);
            if duplicate {
                app.duplicate();
            } else {
                app.copy();
                app.paste();
            }
            let copies = app.selection.clone();
            assert_eq!(copies.len(), 2);
            app.run(Command::TransformShapes {
                shapes: copies.clone(),
                transform: Affine::translate((0.0, 100.0)),
            });
            app.refresh_blends();
            let (copy_start, copy_end) = (copies[0], copies[1]);
            let (to, end_box) = end_of(&app, copy_start).expect("a blend");
            assert_eq!(to, Some(copy_end), "duplicate {duplicate}");
            let moved_end = app.doc().find_shape(copy_end).expect("end").bounds();
            assert!((end_box.y0 - moved_end.y0).abs() < 1e-9, "{end_box:?}");
            // The original is as it was.
            assert_eq!(end_of(&app, start).expect("blend").0, Some(end));
        }
        // The start alone, pasted in another drawing whose objects have
        // the same numbers.
        let mut app = App::headless();
        let (start, _) = blend(&mut app);
        app.select(vec![start]);
        app.copy();
        app.new_document();
        for x in [200.0, 220.0, 240.0, 260.0, 280.0] {
            square(&mut app, x, Color::rgb8(0, 0, 0));
        }
        app.paste();
        let pasted = app.selection[0];
        let (to, end_box) = end_of(&app, pasted).expect("the steps stay");
        assert_eq!(to, None);
        app.refresh_blends();
        let (to, after) = end_of(&app, pasted).expect("still there");
        assert_eq!((to, after), (None, end_box));
        assert!((after.x0 - 100.0).abs() < 1e-9, "{after:?}");
    }

    /// A page duplicated with its contents: the copied blend ends at the
    /// copy of its end object, on the new page. It named the original
    /// page's end object, so moving the copy of the end did nothing and
    /// the copied steps followed the original instead.
    #[test]
    fn a_duplicated_page_blends_to_its_own_end_object() {
        let mut app = App::headless();
        let start = square(&mut app, 0.0, Color::rgb8(250, 220, 0));
        let end = square(&mut app, 100.0, Color::rgb8(0, 160, 80));
        app.select(vec![start, end]);
        app.blend_selection(3, 0.0, 0.0, 0.0);
        app.refresh_blends();
        app.duplicate_page(false, true);
        let copies: Vec<ShapeId> = app.doc().pages[1].layers[0]
            .shapes
            .iter()
            .map(|s| s.id)
            .collect();
        let blend_of = |app: &App, id: ShapeId| {
            app.doc().find_shape(id).and_then(|s| {
                s.effects.iter().find_map(|e| match e {
                    Effect::Blend { end_id, end, .. } => Some((*end_id, end.bounds())),
                    _ => None,
                })
            })
        };
        let copy_start = copies
            .iter()
            .copied()
            .find(|id| blend_of(&app, *id).is_some())
            .expect("the copied blend");
        let copy_end = copies
            .iter()
            .copied()
            .find(|id| *id != copy_start)
            .expect("the copied end");
        assert_eq!(
            blend_of(&app, copy_start).map(|b| b.0),
            Some(Some(copy_end))
        );
        app.run(Command::TransformShapes {
            shapes: vec![copy_end],
            transform: Affine::translate((0.0, 100.0)),
        });
        app.refresh_blends();
        let (_, end_box) = blend_of(&app, copy_start).expect("blend");
        assert!((end_box.y0 - 100.0).abs() < 1e-9, "{end_box:?}");
        assert_eq!(blend_of(&app, start).map(|b| b.0), Some(Some(end)));
        let (_, original) = blend_of(&app, start).expect("blend");
        assert!(original.y0.abs() < 1e-9, "the original is as it was");
    }

    /// The steps sit between the two objects: with the end object below
    /// the start one (a front object blended to one behind it, as in
    /// #232) they are drawn below the start, from the end's side up, and
    /// they move above it when the end is brought to the front.
    #[test]
    fn the_steps_lie_between_the_two_objects() {
        let mut app = App::headless();
        // The end first (below), the start on top of it.
        let end = square(&mut app, 100.0, Color::rgb8(0, 160, 80));
        let start = square(&mut app, 0.0, Color::rgb8(250, 220, 0));
        app.select(vec![start, end]);
        app.blend_selection(3, 0.0, 0.0, 0.0);
        app.refresh_blends();
        let ev = |app: &App| {
            let s = app.doc().find_shape(start).cloned().expect("start");
            tracedraw_core::live::evaluate(&s)
        };
        let e = ev(&app);
        assert!(e.above.is_empty());
        assert_eq!(e.below.len(), 3);
        // Bottom to top: the step nearest the end first.
        let xs: Vec<f64> = e.below.iter().map(|s| s.bounds().center().x).collect();
        assert!(xs[0] > xs[1] && xs[1] > xs[2], "{xs:?}");
        // The end brought to the front: the steps go above the start, in
        // the same step as the reordering.
        let before = app.engine.history_labels().0.len();
        app.select(vec![end]);
        app.order(0);
        app.refresh_blends();
        assert_eq!(app.engine.history_labels().0.len(), before + 1);
        let e = ev(&app);
        assert!(e.below.is_empty());
        assert_eq!(e.above.len(), 3);
    }

    /// A missing range-page endpoint is kept as a hidden local reference.
    /// Refreshing a duplicated page must not move its steps across the start.
    #[test]
    fn range_output_missing_blends_keep_their_order_after_refresh() {
        use tracedraw_core::{Document, Layer, MasterScope, Page, PageRanges, Size};

        for below in [false, true] {
            let mut doc = Document::new("range", Size::new(100.0, 100.0));
            let page = Page {
                id: doc.ids_mut().page(),
                name: String::new(),
                size: Size::new(100.0, 100.0),
                layers: vec![Layer::new(doc.ids_mut().layer(), "Page")],
                guides: Vec::new(),
                background: None,
            };
            doc.pages.push(page);
            let mut start = Shape::new(
                doc.ids_mut().shape(),
                ShapeKind::Rect {
                    rect: Rect::new(10.0, 10.0, 20.0, 20.0),
                    radius: 0.0,
                    corners: None,
                },
            );
            let mut end = start.with_fresh_ids(&mut || doc.ids_mut().shape());
            end.transform_by(Affine::translate((30.0, 0.0)));
            start.effects.push(Effect::Blend {
                end: Box::new(end.clone()),
                steps: 3,
                accel_objects: 0.0,
                accel_colors: 0.0,
                rotation: 0.0,
                path: None,
                rotate_on_path: false,
                end_id: Some(end.id),
                options: tracedraw_core::live::BlendOptions {
                    steps_below: below,
                    ..Default::default()
                },
            });
            let mut range = Layer::master(doc.ids_mut().layer(), "Range", MasterScope::All);
            range.page_range = Some(PageRanges::parse("2").expect("start range"));
            range.shapes.push(start);
            let mut endpoint = Layer::master(doc.ids_mut().layer(), "End", MasterScope::All);
            endpoint.page_range = Some(PageRanges::parse("1").expect("endpoint range"));
            endpoint.shapes.push(end);
            doc.master.extend([range, endpoint]);

            let output = doc.with_master_layers_on_pages(&[1]);
            let loaded =
                Document::from_json(&output.to_json().expect("save output")).expect("read output");
            let wrapper = loaded.pages[0]
                .layers
                .iter()
                .find(|layer| layer.name == "Range")
                .expect("materialized range")
                .shapes[0]
                .id;
            let mut app = App::headless();
            app.open_document(loaded, None);
            app.select_all();
            assert_eq!(app.selection, vec![wrapper], "selectable output blend");
            let blends = app.all_blends();
            assert_eq!(blends.len(), 1);
            let start = blends[0].0;
            app.run(Command::SetFill {
                shapes: vec![start],
                fill: Fill::Solid(Color::BLACK),
            });
            app.refresh_blends();

            let start = app.doc().find_shape(start).expect("output start");
            let [Effect::Blend { options, .. }] = start.effects.as_slice() else {
                panic!("output blend")
            };
            assert_eq!(options.steps_below, below);
            let evaluated = tracedraw_core::live::evaluate(start);
            assert_eq!(evaluated.below.len(), if below { 3 } else { 0 });
            assert_eq!(evaluated.above.len(), if below { 0 } else { 3 });
        }
    }

    /// A blend made with the tool's defaults has 20 steps, enough for the
    /// depth of a front object blended to a copy just behind it to read
    /// as a smooth side rather than bands (#232).
    #[test]
    fn a_new_blend_takes_twenty_steps() {
        assert_eq!(crate::settings::ToolPrefs::default().blend_steps, 20);
        let mut app = App::headless();
        assert_eq!(app.blend_steps, 20);
        let end = square(&mut app, 30.0, Color::rgb8(0, 160, 80));
        let start = square(&mut app, 0.0, Color::rgb8(250, 220, 0));
        app.select(vec![start, end]);
        app.tool = crate::tools::Tool::Blend;
        let (a, b) = (
            app.doc()
                .find_shape(start)
                .expect("start")
                .bounds()
                .center(),
            app.doc().find_shape(end).expect("end").bounds().center(),
        );
        app.drag = crate::app::Drag::Connector {
            from: app.hit_test_inside(a).expect("start is hit"),
            start: a,
            current: b,
        };
        app.end_drag();
        app.refresh_blends();
        let s = app.doc().find_shape(start).cloned().expect("start");
        let e = tracedraw_core::live::evaluate(&s);
        assert_eq!(e.below.len() + e.above.len(), 20);
    }

    /// A blend grouped and the group moved: its steps go with the two
    /// objects and stay between them (#2023).
    #[test]
    fn a_grouped_blend_moves_with_its_group() {
        let mut app = App::headless();
        let a = square(&mut app, 0.0, Color::rgb8(255, 0, 0));
        let b = square(&mut app, 100.0, Color::rgb8(0, 0, 255));
        app.select(vec![a, b]);
        app.blend_selection(3, 0.0, 0.0, 0.0);
        app.refresh_blends();
        app.select(vec![a, b]);
        app.group_selection();
        app.refresh_blends();
        let group = app.selection[0];
        app.run(Command::TransformShapes {
            shapes: vec![group],
            transform: Affine::translate((0.0, 40.0)),
        });
        app.refresh_blends();
        // The middle step, through the group's transform, lies halfway
        // between the two moved squares.
        let parent = app.doc().parent_transform(a).expect("in the group");
        let mut mid = steps_of(&app, a)[1].clone();
        mid.transform = parent * mid.transform;
        let c = mid.bounds().center();
        assert!(
            (c.x - 55.0).abs() < 0.5 && (c.y - 45.0).abs() < 0.5,
            "{c:?}"
        );
    }

    /// A front object blended to one behind it, then grouped, recoloured
    /// as a group and placed inside a frame: the steps stay below the
    /// start object every time instead of jumping in front of it, where
    /// the grouped blend looked all in the end's colour (#2023, #2120).
    #[test]
    fn a_grouped_blend_keeps_its_steps_behind_the_front_object() {
        let mut app = App::headless();
        let end = square(&mut app, 30.0, Color::rgb8(0, 160, 80));
        let start = square(&mut app, 0.0, Color::rgb8(250, 220, 0));
        app.select(vec![start, end]);
        app.blend_selection(5, 0.0, 0.0, 0.0);
        app.refresh_blends();
        let ev = |app: &App| {
            let s = app.doc().find_shape(start).cloned().expect("start");
            tracedraw_core::live::evaluate(&s)
        };
        let below = |app: &App, what: &str| {
            let e = ev(app);
            assert!(e.above.is_empty(), "{what}: steps above the start");
            assert_eq!(e.below.len(), 5, "{what}");
        };
        below(&app, "made");
        app.select(vec![start, end]);
        app.group_selection();
        app.refresh_blends();
        below(&app, "grouped");
        let group = app.selection[0];
        app.run(Command::SetFill {
            shapes: vec![group],
            fill: Fill::Solid(Color::rgb8(200, 0, 0)),
        });
        app.refresh_blends();
        app.run(Command::SetFill {
            shapes: vec![start],
            fill: Fill::Solid(Color::rgb8(250, 220, 0)),
        });
        app.refresh_blends();
        below(&app, "group recoloured");
        app.run(Command::TransformShapes {
            shapes: vec![group],
            transform: Affine::translate((5.0, 5.0)),
        });
        app.refresh_blends();
        below(&app, "group moved");
        // The group placed inside a frame.
        let frame = app
            .new_shape(ShapeKind::Rect {
                rect: Rect::new(-50.0, -50.0, 100.0, 100.0),
                radius: 0.0,
                corners: None,
            })
            .expect("frame");
        app.place_inside(vec![group], frame);
        app.refresh_blends();
        below(&app, "inside a frame");
    }

    /// A blend fitted to a new path, its two objects placed anywhere
    /// along it: they move there, the steps run between them along the
    /// path, a run of changes is one undo step, the blend's settings still
    /// edit it, and Detach from path makes it straight again (#2946).
    #[test]
    fn a_blend_fitted_to_a_path_places_its_ends_anywhere_along_it() {
        let mut app = App::headless();
        let a = square(&mut app, 0.0, Color::rgb8(255, 0, 0));
        let b = square(&mut app, 50.0, Color::rgb8(0, 0, 255));
        app.select(vec![a, b]);
        app.blend_selection(3, 0.0, 0.0, 0.0);
        let mut line = tracedraw_core::geometry::BezPath::new();
        line.move_to((0.0, 100.0));
        line.line_to((200.0, 100.0));
        let curve = app
            .new_shape(ShapeKind::Path {
                path: line,
                closed: false,
            })
            .expect("curve");
        // The blend picked by its end object takes the path too.
        app.select(vec![b]);
        app.set_blend_path(curve);
        assert!(matches!(
            app.selected_blends().first(),
            Some((_, Effect::Blend { path: Some(_), .. }))
        ));
        let depth = app.engine.history_labels().0.len();
        for to in [0.6, 0.7, 0.8] {
            app.set_blend_path_ends(0.2, to);
        }
        assert_eq!(app.engine.history_labels().0.len(), depth + 1, "one step");
        let centre = |app: &App, id: ShapeId| app.doc().find_shape(id).expect("object").bounds().center();
        let (ca, cb) = (centre(&app, a), centre(&app, b));
        assert!((ca.x - 40.0).abs() < 0.01 && (ca.y - 100.0).abs() < 0.01, "{ca:?}");
        assert!((cb.x - 160.0).abs() < 0.01 && (cb.y - 100.0).abs() < 0.01, "{cb:?}");
        let xs: Vec<f64> = steps_of(&app, a).iter().map(|s| s.bounds().center().x).collect();
        for (x, want) in xs.iter().zip([70.0, 100.0, 130.0]) {
            assert!((x - want).abs() < 0.5, "{xs:?}");
        }
        // The steps still change with the blend's settings.
        app.change_blends("Blend", |e| {
            let mut e = e.clone();
            if let Effect::Blend { steps, .. } = &mut e {
                *steps = 5;
            }
            Some(e)
        });
        assert_eq!(steps_of(&app, a).len(), 5);
        app.engine.undo().expect("undo");
        app.engine.undo().expect("undo");
        let back = centre(&app, a);
        assert!((back.x - 5.0).abs() < 0.01, "{back:?}");
        app.engine.redo().expect("redo");
        app.engine.redo().expect("redo");
        app.detach_blend_path();
        assert!(matches!(
            app.selected_blends().first(),
            Some((_, Effect::Blend { path: None, options, .. })) if options.path_range.is_none()
        ));
        let ys: Vec<f64> = steps_of(&app, a).iter().map(|s| s.bounds().center().y).collect();
        assert!(ys.iter().all(|y| (y - 100.0).abs() < 0.5), "{ys:?}");
    }

    #[test]
    fn the_end_object_stays_and_the_steps_follow_it() {
        let mut app = App::headless();
        let a = square(&mut app, 0.0, Color::rgb8(255, 0, 0));
        let b = square(&mut app, 100.0, Color::rgb8(0, 0, 255));
        app.select(vec![a, b]);
        app.blend_selection(3, 0.0, 0.0, 0.0);
        app.refresh_blends();
        // Both objects are there; the effect draws the three steps only.
        assert!(app.doc().find_shape(b).is_some());
        assert_eq!(steps_of(&app, a).len(), 3);
        let mid = steps_of(&app, a)[1].bounds().center();
        assert!((mid.x - 55.0).abs() < 0.5, "{mid:?}");
        // The end object moved: the steps follow, in the same step.
        let before = app.engine.history_labels().0.len();
        app.run(Command::TransformShapes {
            shapes: vec![b],
            transform: Affine::translate((0.0, 40.0)),
        });
        app.refresh_blends();
        assert_eq!(app.engine.history_labels().0.len(), before + 1);
        let mid = steps_of(&app, a)[1].bounds().center();
        assert!((mid.y - 25.0).abs() < 0.5, "{mid:?}");
        // Undo puts both back and needs no refresh (Redo stays).
        app.undo();
        app.refresh_blends();
        assert!(app.engine.redo_label().is_some());
        // Selecting the end object picks the blend for the bar.
        app.select(vec![b]);
        assert_eq!(app.selected_blends().len(), 1);
        app.change_blends("Blend", |e| {
            let mut e = e.clone();
            if let Effect::Blend { steps, .. } = &mut e {
                *steps = 7;
            }
            Some(e)
        });
        assert_eq!(steps_of(&app, a).len(), 7);
        // Deleting the end object takes the blend away.
        app.run(Command::DeleteShapes { shapes: vec![b] });
        app.refresh_blends();
        assert!(steps_of(&app, a).is_empty());
        let s = app.doc().find_shape(a).expect("start");
        assert!(!s.effects.iter().any(is_blend));
        // Clear Blend on a fresh pair leaves both objects.
        let c = square(&mut app, 200.0, Color::BLACK);
        app.select(vec![a, c]);
        app.blend_selection(2, 0.0, 0.0, 0.0);
        app.select(vec![c]);
        app.clear_blend();
        assert!(app.doc().find_shape(c).is_some());
        assert!(steps_of(&app, a).is_empty());
    }

    /// A blend of two texts saved as `.cdr` and opened again is still a
    /// blend: the end text is its end object, moving it redraws the
    /// steps, and the Blend panel finds it (#986).
    #[test]
    fn a_text_blend_survives_saving_and_opening_as_cdr() {
        let mut app = App::headless();
        let text = |app: &mut App, x: f64, c: Color| {
            let id = app
                .new_shape(ShapeKind::Text {
                    spans: vec![tracedraw_core::document::TextSpan::new("A", "Sans", 200.0)],
                    origin: tracedraw_core::geometry::Point::new(x, 100.0),
                    frame: None,
                    align: tracedraw_core::document::TextAlign::Left,
                    para: Default::default(),
                    on_path: None,
                })
                .expect("text");
            app.run(Command::SetFill {
                shapes: vec![id],
                fill: Fill::Solid(c),
            });
            id
        };
        let start = text(&mut app, 20.0, Color::rgb8(255, 240, 0));
        let end = text(&mut app, 90.0, Color::rgb8(0, 180, 80));
        app.select(vec![start, end]);
        app.blend_selection(20, 0.0, 0.0, 0.0);
        app.refresh_blends();
        assert_eq!(steps_of(&app, start).len(), 20);
        let bytes = tracedraw_cdr::write::document_to_cdr(app.doc());
        let (doc, _) = tracedraw_cdr::open_bytes(&bytes, "t").expect("read back");
        let mut back = App::headless();
        back.open_document(doc, None);
        let shapes = back.doc().pages[0].layers[0].shapes.clone();
        assert_eq!(shapes.len(), 2, "{shapes:?}");
        let (a, b) = (shapes[0].id, shapes[1].id);
        // The steps run from the start text to the end text, each a bit
        // further right than the one before. (Their exact places are
        // checked in the cdr crate: here other tests of the run carry
        // fonts in, which changes what "Sans" draws with meanwhile.)
        let again: Vec<Rect> = steps_of(&back, a).iter().map(|s| s.bounds()).collect();
        assert_eq!(again.len(), 20);
        let first = back.doc().find_shape(a).expect("start").bounds();
        let last = back.doc().find_shape(b).expect("end").bounds();
        assert!(
            first.x0 < again[0].x0 && again[19].x1 < last.x1,
            "{first:?} {again:?} {last:?}"
        );
        for w in again.windows(2) {
            assert!(w[0].x0 < w[1].x0, "{again:?}");
        }
        back.select(vec![b]);
        assert_eq!(back.selected_blends().len(), 1);
        back.run(Command::TransformShapes {
            shapes: vec![b],
            transform: Affine::translate((0.0, 40.0)),
        });
        back.refresh_blends();
        let moved: Vec<Rect> = steps_of(&back, a).iter().map(|s| s.bounds()).collect();
        assert!(moved[10].center().y > again[10].center().y + 15.0);
    }
}
