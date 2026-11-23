//! The engine owns a document and its history.
//!
//! History is snapshot-based: each step keeps one copy of the drawing, the
//! state on the other side of it (on the undo stack the drawing before the
//! step, on the redo stack the drawing after it). Undo and Redo swap that
//! copy with the drawing, so they copy nothing, and an edit copies the
//! drawing once (#345: two or three copies per edit made every change of
//! a large drawing slow). A delta-based history can replace it later
//! without changing this API.
//!
//! The drawing and the copies are shared ([`Arc`]): a render on another
//! thread takes the drawing as it stands for nothing
//! ([`Engine::shared_document`]) instead of copying it on the window's
//! thread, and the edit that follows copies it once, as it did (#3440).

use crate::command::Command;
use crate::id::ShapeId;
use crate::{Document, Error, Result};
use std::sync::Arc;

#[derive(Debug, Clone)]
struct HistoryEntry {
    label: &'static str,
    /// The step's identity, unique in this engine (see
    /// [`Engine::last_step_id`]).
    id: u64,
    /// The drawing on the other side of the step: before it on the undo
    /// stack, after it on the redo stack.
    other: Arc<Document>,
    /// About how much memory `other` holds of its own (see
    /// [`snapshot_bytes`]), for the history's memory limit.
    bytes: usize,
}

#[derive(Debug)]
pub struct Engine {
    doc: Arc<Document>,
    undo: Vec<HistoryEntry>,
    redo: Vec<HistoryEntry>,
    /// Counts applied commands, so views can cache per revision.
    revision: u64,
    /// Revision at the last save, to know whether the document is dirty.
    saved_revision: u64,
    /// Revision right after the last recorded step, so a side effect can
    /// join that step only while nothing happened since.
    step_revision: u64,
    max_history: usize,
    /// Memory the undo steps may hold together before the oldest go
    /// (`usize::MAX`: no limit); the newest step always stays.
    max_history_bytes: usize,
    /// Generator of object identities ([`crate::document::Uid`]).
    uid_state: u64,
    /// The id the next recorded step gets.
    next_step: u64,
    /// This engine's identity, unlike any other engine's in this run.
    instance: u64,
    /// A preview in progress (see [`Engine::begin_preview`]).
    preview: Option<PreviewMark>,
    /// Groups begun and not yet ended (see [`Engine::begin_group`]): the
    /// history is trimmed only once they end.
    groups: usize,
    /// The size of the step the last Undo took back, until a step is
    /// recorded or redone: the next step recorded holds about the same
    /// drawing, so it takes that size instead of measuring (a live drag
    /// runs an Undo and a step per pointer move, with side effects such as
    /// a cleared link in between).
    undone_bytes: Option<usize>,
    /// Ids minted while the drawing was shared (with a render on another
    /// thread): counted here rather than copying the whole drawing for
    /// a counter, and written into it at its next change (#3440).
    ids_ahead: Option<crate::id::IdSource>,
    /// Consecutive isolated edits of a single shape `(shape, base_rev, latest_rev)`,
    /// so the canvas can keep the page texture from `base_rev` while typing (#3904).
    single_shape_run: Option<(ShapeId, u64, u64)>,
    /// Buffers for measuring a step, kept between steps.
    measure: Measure,
}

/// Picture addresses gathered while measuring a step (see
/// [`snapshot_bytes`]).
#[derive(Debug, Default)]
struct Measure {
    shared: Vec<usize>,
    own: Vec<(usize, usize)>,
}

/// How the history stood when a preview began, to put it back.
#[derive(Debug)]
struct PreviewMark {
    /// The step on top of the undo stack then.
    last_step: Option<u64>,
    /// The redo stack then, moved here (not copied: each entry holds a
    /// whole drawing) and put back when the preview ends.
    redo: Vec<HistoryEntry>,
    /// A step recorded since cleared the redo stack.
    redo_cleared: bool,
    /// Steps redone from `redo` during the preview, which withdrawing it
    /// puts back there.
    redone: Vec<u64>,
    saved: bool,
    at_step: bool,
}

/// A seed no other engine in this run has: std's per-run random hash keys
/// over a counter.
fn fresh_seed() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    h.finish()
}

/// When `cmd` only replaces the characters or span styles of an existing
/// text shape that carries no content link and does not push paragraph text
/// out of its way, keeping its origin, frame, path, baseline grid setting
/// and computed flow, return that shape's id (#3902).
fn isolated_text_shape(doc: &Document, cmd: &Command) -> Option<ShapeId> {
    let Command::SetShapeKind {
        shape,
        kind:
            crate::document::ShapeKind::Text {
                origin: o2,
                frame: f2,
                para: p2,
                on_path: op2,
                ..
            },
    } = cmd
    else {
        return None;
    };
    let s = doc.find_shape(*shape)?;
    if s.content_link.is_some() || s.wrap_text || doc.shape_or_ancestor_wraps_text(*shape) {
        return None;
    }
    let crate::document::ShapeKind::Text {
        origin: o1,
        frame: f1,
        para: p1,
        on_path: op1,
        ..
    } = &s.kind
    else {
        return None;
    };
    (o1 == o2
        && f1 == f2
        && op1 == op2
        && p1.baseline_grid_mm == p2.baseline_grid_mm
        && p1.flow == p2.flow)
        .then_some(*shape)
}

/// When `cmd` only alters the paint, opacity or hyperlink of a single
/// existing shape, return that shape's id (#3904).
fn single_shape_paint_or_link(doc: &Document, cmd: &Command) -> Option<ShapeId> {
    let shapes = match cmd {
        Command::SetFill { shapes, .. }
        | Command::SetStroke { shapes, .. }
        | Command::SetOpacity { shapes, .. }
        | Command::SetLink { shapes, .. } => shapes,
        _ => return None,
    };
    let [id] = shapes.as_slice() else {
        return None;
    };
    doc.find_shape(*id).map(|_| *id)
}

impl Default for Engine {
    fn default() -> Self {
        Engine::new(Document::default())
    }
}

impl Engine {
    pub fn new(doc: Document) -> Self {
        let mut e = Engine {
            doc: Arc::new(doc),
            undo: Vec::new(),
            redo: Vec::new(),
            revision: 0,
            saved_revision: 0,
            step_revision: u64::MAX,
            max_history: 200,
            max_history_bytes: usize::MAX,
            uid_state: fresh_seed(),
            next_step: 1,
            instance: fresh_seed(),
            preview: None,
            groups: 0,
            undone_bytes: None,
            ids_ahead: None,
            single_shape_run: None,
            measure: Measure::default(),
        };
        let uid_state = &mut e.uid_state;
        let doc = Arc::make_mut(&mut e.doc);
        doc.assign_uids(uid_state);
        doc.refresh_text_flow();
        e
    }

    pub fn document(&self) -> &Document {
        &self.doc
    }

    /// The drawing as it stands, shared: for a render on another thread,
    /// which then needs no copy of its own (#3440). The next edit copies
    /// the drawing as every edit does, so the shared one never changes.
    pub fn shared_document(&self) -> Arc<Document> {
        Arc::clone(&self.doc)
    }

    /// The drawing to change: copied first when shared (with a step of
    /// the history or a render).
    fn doc_mut(&mut self) -> &mut Document {
        let ahead = self.ids_ahead.take();
        let doc = Arc::make_mut(&mut self.doc);
        if let Some(ids) = ahead {
            doc.set_ids(ids);
        }
        doc
    }

    /// The id counters: the drawing's, or further on when ids were
    /// minted while it was shared.
    pub fn ids(&self) -> &crate::id::IdSource {
        self.ids_ahead.as_ref().unwrap_or(self.doc.ids())
    }

    /// This engine's identity: no other engine of this run has it, so a
    /// value cached for (instance, revision) belongs to one drawing state.
    pub fn instance(&self) -> u64 {
        self.instance
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// True when every revision recorded since `since_revision` only changed
    /// `shape` (or non-drawn document state such as the document palette),
    /// leaving every other object on the page as it stood at `since_revision`
    /// (#3904).
    pub fn unchanged_except(&self, shape: ShapeId, since_revision: u64) -> bool {
        self.revision == since_revision
            || self.single_shape_run.is_some_and(|(id, base, now)| {
                id == shape && base <= since_revision && now == self.revision
            })
    }

    fn note_single_shape_edit(&mut self, shape: ShapeId, before_rev: u64) {
        let base = match self.single_shape_run {
            Some((id, base, prev)) if id == shape && prev == before_rev => base,
            _ => before_rev,
        };
        self.single_shape_run = Some((shape, base, self.revision));
    }

    fn record_run_for(&mut self, cmd_shape: Option<ShapeId>, palette_only: bool, before_rev: u64) {
        if let Some(id) = cmd_shape {
            self.note_single_shape_edit(id, before_rev);
        } else if palette_only {
            if let Some((id, base, prev)) = self.single_shape_run {
                if prev == before_rev {
                    self.single_shape_run = Some((id, base, self.revision));
                    return;
                }
            }
            self.single_shape_run = None;
        } else {
            self.single_shape_run = None;
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.revision != self.saved_revision
    }

    pub fn mark_saved(&mut self) {
        self.saved_revision = self.revision;
    }

    /// Unsaved changes whatever the history holds (a drawing recovered
    /// from a backup), until the next save.
    pub fn mark_unsaved(&mut self) {
        self.saved_revision = u64::MAX;
    }

    /// Replace the document (open file, new document). Clears history.
    pub fn replace(&mut self, doc: Document) {
        self.doc = Arc::new(doc);
        self.ids_ahead = None;
        let uid_state = &mut self.uid_state;
        let doc = Arc::make_mut(&mut self.doc);
        doc.assign_uids(uid_state);
        doc.refresh_text_flow();
        self.undo.clear();
        self.redo.clear();
        // A preview of the drawing that went has nothing to go back to.
        self.preview = None;
        self.undone_bytes = None;
        self.single_shape_run = None;
        self.revision += 1;
        self.saved_revision = self.revision;
        self.step_revision = u64::MAX;
    }

    /// Apply a command and record it. A failing command leaves the document
    /// and history untouched.
    pub fn run(&mut self, cmd: &Command) -> Result<()> {
        self.run_with_label(cmd, cmd.label())
    }

    /// Like [`run`](Self::run) with a custom history label.
    pub fn run_with_label(&mut self, cmd: &Command, label: &'static str) -> Result<()> {
        if cmd.outside_history() {
            // No step, no redo loss, no revision: views and the unsaved
            // marker do not change for it.
            return cmd.apply(self.doc_mut());
        }
        let before_rev = self.revision;
        let isolated = isolated_text_shape(&self.doc, cmd);
        let single = isolated.or_else(|| single_shape_paint_or_link(&self.doc, cmd));
        let palette_only = matches!(cmd, Command::SetDocumentPalette { .. });
        let before = Arc::clone(&self.doc);
        if let Err(e) = cmd.apply(self.doc_mut()) {
            // A command that fails part-way leaves nothing behind (but
            // ids never rewind).
            self.ids_ahead = Some(self.doc.ids().clone());
            self.doc = before;
            return Err(e);
        }
        if isolated.is_none() {
            self.settle_after(&before, cmd.leaves_text_alone());
        }
        let id = self.next_step;
        self.next_step += 1;
        let bytes = self.bytes_of(&before);
        self.undo.push(HistoryEntry {
            label,
            id,
            other: before,
            bytes,
        });
        self.finish_step();
        self.record_run_for(single, palette_only, before_rev);
        Ok(())
    }

    /// After a change from `before`: linked texts, new objects' ids and
    /// text flow brought up to date. `text_untouched` (see
    /// [`Command::leaves_text_alone`]) skips the two text passes: they
    /// walk every shape, which a paint command on a heavy drawing does
    /// not deserve.
    fn settle_after(&mut self, before: &Document, text_untouched: bool) {
        self.doc_mut();
        let uid_state = &mut self.uid_state;
        let doc = Arc::make_mut(&mut self.doc);
        if !text_untouched {
            doc.sync_linked_text(before);
        }
        doc.assign_uids(uid_state);
        if !text_untouched {
            doc.refresh_text_flow();
        }
    }

    fn finish_step(&mut self) {
        self.trim_history();
        self.clear_redo();
        self.revision += 1;
        self.step_revision = self.revision;
    }

    /// Apply several commands as one undo step.
    pub fn run_batch(&mut self, label: &'static str, cmds: &[Command]) -> Result<()> {
        let before = Arc::clone(&self.doc);
        for c in cmds {
            if let Err(e) = c.apply(self.doc_mut()) {
                self.ids_ahead = Some(self.doc.ids().clone());
                self.doc = before;
                return Err(e);
            }
        }
        self.settle_after(&before, cmds.iter().all(Command::leaves_text_alone));
        let id = self.next_step;
        self.next_step += 1;
        let bytes = self.bytes_of(&before);
        self.undo.push(HistoryEntry {
            label,
            id,
            other: before,
            bytes,
        });
        self.finish_step();
        self.single_shape_run = None;
        Ok(())
    }

    /// Apply a command as part of the step just recorded (a side effect of
    /// it, such as the document palette remembering a colour that was just
    /// applied), so one Undo takes both back. Recorded as a step of its own
    /// when anything else happened since.
    pub fn amend(&mut self, cmd: &Command) -> Result<()> {
        if cmd.outside_history() {
            return cmd.apply(self.doc_mut());
        }
        if self.revision != self.step_revision {
            return self.run(cmd);
        }
        if self.undo.is_empty() {
            return self.run(cmd);
        }
        let before_rev = self.revision;
        // Isolated text edits (typing characters or changing span styles on an
        // unlinked, non-wrapping text object) cannot fail once the shape is
        // found and need none of the full-document passes: mutate in place
        // through Arc::make_mut so unshared drawings never clone (#3902).
        if let Some(id) = isolated_text_shape(&self.doc, cmd) {
            cmd.apply(self.doc_mut())?;
            self.revision += 1;
            self.step_revision = self.revision;
            self.note_single_shape_edit(id, before_rev);
            return Ok(());
        }
        let single = single_shape_paint_or_link(&self.doc, cmd);
        let palette_only = matches!(cmd, Command::SetDocumentPalette { .. });
        // The step keeps the drawing from before it; the change joins it.
        let mut doc = Document::clone(&self.doc);
        if let Some(ids) = self.ids_ahead.clone() {
            doc.set_ids(ids);
        }
        cmd.apply(&mut doc)?;
        self.ids_ahead = None;
        if !cmd.leaves_text_alone() {
            doc.sync_linked_text(&self.doc);
        }
        doc.assign_uids(&mut self.uid_state);
        if !cmd.leaves_text_alone() {
            doc.refresh_text_flow();
        }
        self.doc = Arc::new(doc);
        self.revision += 1;
        self.step_revision = self.revision;
        self.record_run_for(single, palette_only, before_rev);
        Ok(())
    }

    /// The drawing is as the step just recorded left it: no undo, redo or
    /// change outside the history since. A side effect added now with
    /// [`amend`](Self::amend) joins that step.
    pub fn at_recorded_step(&self) -> bool {
        self.revision == self.step_revision && !self.undo.is_empty()
    }

    /// Take back the step `id` as if it had never been recorded, with no
    /// redo of it (a text started and left with no characters). Only the
    /// step just recorded, with nothing done since, can go: otherwise
    /// nothing changes and `false` is returned.
    pub fn withdraw_step(&mut self, id: u64) -> bool {
        if !self.at_recorded_step() || self.last_step_id() != Some(id) {
            return false;
        }
        let Some(e) = self.undo.pop() else {
            return false;
        };
        let _ = self.swap_in(e.other);
        self.undone_bytes = None;
        self.step_revision = u64::MAX;
        true
    }

    /// [`Engine::amend`] for several commands: they join the step just
    /// recorded, or make a step of their own named `label` when anything
    /// else happened since. A failing command leaves the drawing as it was.
    pub fn amend_batch(&mut self, label: &'static str, cmds: &[Command]) -> Result<()> {
        if cmds.is_empty() {
            return Ok(());
        }
        if self.revision != self.step_revision || self.undo.is_empty() {
            return self.run_batch(label, cmds);
        }
        let mut doc = Document::clone(&self.doc);
        if let Some(ids) = self.ids_ahead.clone() {
            doc.set_ids(ids);
        }
        for c in cmds {
            c.apply(&mut doc)?;
        }
        self.ids_ahead = None;
        if !cmds.iter().all(Command::leaves_text_alone) {
            doc.sync_linked_text(&self.doc);
        }
        doc.assign_uids(&mut self.uid_state);
        self.doc = Arc::new(doc);
        self.clear_redo();
        self.revision += 1;
        self.step_revision = self.revision;
        self.single_shape_run = None;
        Ok(())
    }

    /// Begin a group of steps that [`end_group`](Self::end_group) joins
    /// into one, so one Undo takes them all back (an action made of
    /// several commands run one by one). Until it ends, no step is dropped
    /// as too old, so the group is never cut short. Returns the mark to
    /// end it with.
    pub fn begin_group(&mut self) -> Option<u64> {
        self.groups += 1;
        self.last_step_id()
    }

    /// Join the steps recorded since `mark` (from
    /// [`begin_group`](Self::begin_group)) into the oldest of them, named
    /// `label`, with the newest one's id; then drop steps past the limits.
    pub fn end_group(&mut self, mark: Option<u64>, label: &'static str) {
        self.groups = self.groups.saturating_sub(1);
        self.join_group(mark, label);
        self.trim_history();
    }

    /// Join the steps recorded since `mark` into one, named `label`, and
    /// leave the group open: a long action (a drag of many steps) keeps
    /// one snapshot from before it instead of one a step. Ended as usual
    /// with [`end_group`](Self::end_group).
    pub fn join_group(&mut self, mark: Option<u64>, label: &'static str) {
        let first = match mark {
            None => Some(0),
            Some(id) => self.undo.iter().position(|e| e.id == id).map(|i| i + 1),
        };
        if let Some(first) = first.filter(|&i| i < self.undo.len()) {
            let last = self.undo.last().map(|e| e.id).unwrap_or(0);
            self.undo.truncate(first + 1);
            if let Some(e) = self.undo.last_mut() {
                e.label = label;
                e.id = last;
            }
        }
    }

    /// Mint a fresh shape id. Ids never rewind on undo.
    pub fn new_shape_id(&mut self) -> ShapeId {
        // Shared with a render: counted aside, not by copying the drawing.
        if Arc::get_mut(&mut self.doc).is_none() {
            let ids = self
                .ids_ahead
                .get_or_insert_with(|| self.doc.ids().clone());
            return ids.shape();
        }
        self.doc_mut().ids_mut().shape()
    }

    /// Put `snapshot` in place of the drawing, keeping the id counters, the
    /// time tracker and the saved view (they never rewind); the drawing it replaced is
    /// returned, for the other stack.
    fn swap_in(&mut self, snapshot: Arc<Document>) -> Arc<Document> {
        let mut slot = snapshot;
        self.swap_slot(&mut slot);
        slot
    }

    /// [`Self::swap_in`] with the copy in `slot`, which then holds the
    /// drawing it replaced.
    fn swap_slot(&mut self, slot: &mut Arc<Document>) {
        let ids = self.ids_ahead.take().unwrap_or_else(|| self.doc.ids().clone());
        let tracker = self.doc.time_tracker.clone();
        let saved_view = self.doc.saved_view;
        std::mem::swap(&mut self.doc, slot);
        let doc = self.doc_mut();
        doc.set_ids(ids);
        doc.time_tracker = tracker;
        doc.saved_view = saved_view;
        // The copy kept for the other stack needs neither (they are put
        // back from the drawing when it returns); one a render still
        // shares is left as it is rather than copied for that.
        if let Some(was) = Arc::get_mut(slot) {
            was.time_tracker = Default::default();
            was.saved_view = None;
        }
        self.single_shape_run = None;
        self.revision += 1;
    }

    /// Mark the drawing changed without an undo step (an edit of state kept
    /// outside the history, which should still be saved).
    pub fn mark_changed(&mut self) {
        self.single_shape_run = None;
        self.revision += 1;
    }

    pub fn undo(&mut self) -> Result<&'static str> {
        let mut e = self.undo.pop().ok_or(Error::NothingToUndo)?;
        self.swap_slot(&mut e.other);
        self.undone_bytes = Some(e.bytes);
        let label = e.label;
        self.redo.push(e);
        Ok(label)
    }

    pub fn redo(&mut self) -> Result<&'static str> {
        self.undone_bytes = None;
        let held = self.preview.as_mut().filter(|m| !m.redo_cleared);
        let mut e = match self.redo.pop() {
            Some(e) => e,
            None => {
                let m = held.ok_or(Error::NothingToRedo)?;
                let e = m.redo.pop().ok_or(Error::NothingToRedo)?;
                m.redone.push(e.id);
                e
            }
        };
        self.swap_slot(&mut e.other);
        let label = e.label;
        self.undo.push(e);
        Ok(label)
    }

    /// Begin a preview: steps recorded from now on can be withdrawn with
    /// [`withdraw_preview`](Self::withdraw_preview), which leaves the
    /// drawing, the history (redo included) and the saved state as they
    /// are now, or kept with [`keep_preview`](Self::keep_preview). A
    /// preview already begun goes on.
    pub fn begin_preview(&mut self) {
        if self.preview.is_some() {
            return;
        }
        self.preview = Some(PreviewMark {
            last_step: self.last_step_id(),
            redo: std::mem::take(&mut self.redo),
            redo_cleared: false,
            redone: Vec::new(),
            saved: !self.is_dirty(),
            at_step: self.at_recorded_step(),
        });
    }

    /// A preview is in progress.
    pub fn in_preview(&self) -> bool {
        self.preview.is_some()
    }

    /// Take back what was recorded since [`begin_preview`](Self::begin_preview)
    /// and put the history back as it was then.
    pub fn withdraw_preview(&mut self) {
        let Some(mut m) = self.preview.take() else {
            return;
        };
        // Each step goes back as Undo takes it (swaps, no copies); one
        // redone from the held stack returns there, newest first, so the
        // stack comes out as it was.
        while !self.undo.is_empty() && self.last_step_id() != m.last_step {
            let Some(mut e) = self.undo.pop() else {
                break;
            };
            if m.redone.contains(&e.id) {
                self.swap_slot(&mut e.other);
                m.redo.push(e);
            } else {
                self.swap_in(e.other);
            }
        }
        self.redo = m.redo;
        if m.saved {
            self.saved_revision = self.revision;
        }
        self.step_revision = if m.at_step { self.revision } else { u64::MAX };
    }

    /// Keep what was recorded since [`begin_preview`](Self::begin_preview)
    /// as ordinary steps.
    pub fn keep_preview(&mut self) {
        let Some(m) = self.preview.take() else {
            return;
        };
        if !m.redo_cleared {
            let mut redo = m.redo;
            redo.append(&mut self.redo);
            self.redo = redo;
        }
        // Steps recorded during the preview were not measured (a live
        // preview records one per change); the kept ones are, once.
        let first = match m.last_step {
            None => 0,
            Some(id) => self
                .undo
                .iter()
                .position(|e| e.id == id)
                .map_or(self.undo.len(), |i| i + 1),
        };
        for i in first..self.undo.len() {
            let snapshot = Arc::clone(&self.undo[i].other);
            let bytes = self.bytes_of(&snapshot);
            self.undo[i].bytes = bytes;
        }
        self.trim_history();
    }

    /// Drop the steps that could be redone, those a preview holds included.
    fn clear_redo(&mut self) {
        self.redo.clear();
        if let Some(m) = self.preview.as_mut() {
            m.redo_cleared = true;
        }
    }

    /// The id of the step on top of the undo stack (the last one still in
    /// effect); a step keeps its id through [`amend`](Self::amend), undo
    /// and redo, and no other step ever gets it.
    pub fn last_step_id(&self) -> Option<u64> {
        self.undo.last().map(|e| e.id)
    }

    /// The step `id` is in effect: it is on the undo stack (not undone,
    /// not dropped as too old).
    pub fn step_in_effect(&self, id: u64) -> bool {
        self.undo.iter().any(|e| e.id == id)
    }

    /// The id of the step just under step `id` on the undo stack (the
    /// one made right before it and still in effect); `None` when `id`
    /// is the oldest or not in effect.
    pub fn step_below(&self, id: u64) -> Option<u64> {
        let i = self.undo.iter().position(|e| e.id == id)?;
        i.checked_sub(1)
            .and_then(|j| self.undo.get(j))
            .map(|e| e.id)
    }

    /// Labels of the undo stack (oldest first) and the redo stack.
    pub fn history_labels(&self) -> (Vec<&'static str>, Vec<&'static str>) {
        (
            self.undo.iter().map(|e| e.label).collect(),
            self.held_redo()
                .iter()
                .chain(&self.redo)
                .map(|e| e.label)
                .collect(),
        )
    }

    /// How many steps Undo keeps (Options > General > Undo levels); older
    /// steps are dropped at once when the limit shrinks.
    pub fn set_max_history(&mut self, n: usize) {
        self.max_history = n.max(1);
        self.trim_history();
    }

    /// How much memory the undo steps may hold together (`usize::MAX`
    /// for no limit, the default). Past it the oldest steps go, at once
    /// when the limit shrinks; the newest step always stays. Steps are
    /// measured only while there is a limit.
    pub fn set_max_history_bytes(&mut self, bytes: usize) {
        self.max_history_bytes = bytes;
        self.trim_history();
    }

    /// About how much memory the undo and redo steps hold. Only the undo
    /// steps count against the limit: the redo ones are steps taken back
    /// from them.
    pub fn history_bytes(&self) -> usize {
        self.undo.iter().chain(&self.redo).map(|e| e.bytes).sum()
    }

    /// Drop the oldest undo steps past the step count or the memory limit
    /// (not while a group or a preview is open; they trim when they end).
    fn trim_history(&mut self) {
        if self.groups > 0 || self.preview.is_some() {
            return;
        }
        let over = self.undo.len().saturating_sub(self.max_history);
        let mut drop = over;
        if self.max_history_bytes != usize::MAX {
            let mut total: usize = self.undo[over..].iter().map(|e| e.bytes).sum();
            while total > self.max_history_bytes && drop + 1 < self.undo.len() {
                total -= self.undo[drop].bytes;
                drop += 1;
            }
        }
        if drop > 0 {
            self.undo.drain(..drop);
        }
    }

    /// The memory `snapshot` holds that the drawing does not share with
    /// it: 0 without a limit, and for a step recorded during a preview
    /// (measured if the preview is kept); taken from the step just undone
    /// when nothing happened since.
    fn bytes_of(&mut self, snapshot: &Document) -> usize {
        let undone = self.undone_bytes.take();
        if self.max_history_bytes == usize::MAX || self.preview.is_some() {
            return 0;
        }
        if let Some(bytes) = undone {
            return bytes;
        }
        snapshot_bytes(snapshot, &self.doc, &mut self.measure)
    }

    pub fn max_history(&self) -> usize {
        self.max_history
    }

    pub fn undo_label(&self) -> Option<&'static str> {
        self.undo.last().map(|e| e.label)
    }

    pub fn redo_label(&self) -> Option<&'static str> {
        self.redo
            .last()
            .or_else(|| self.held_redo().last())
            .map(|e| e.label)
    }

    /// The redo steps a preview holds while it goes on (see
    /// [`begin_preview`](Self::begin_preview)).
    fn held_redo(&self) -> &[HistoryEntry] {
        match &self.preview {
            Some(m) if !m.redo_cleared => &m.redo,
            _ => &[],
        }
    }
}

/// About how much memory `snapshot` holds of its own, next to `current`:
/// each object and its path points, and the pictures that `current` does
/// not share with it (pictures are shared between copies of a drawing
/// until one is changed). Text, tables, node types and effect settings
/// are left out; they are small next to paths and pictures. Two walks
/// over the objects, into buffers kept between calls.
fn snapshot_bytes(snapshot: &Document, current: &Document, m: &mut Measure) -> usize {
    use crate::document::{Shape, ShapeKind};
    use crate::style::{Fill, Pattern};
    #[cfg(test)]
    MEASURED.with(|c| c.set(c.get() + 1));
    fn fill_pictures<'a>(fill: &'a Fill, f: &mut dyn FnMut(&'a crate::SharedBytes)) {
        match fill {
            Fill::Pattern(Pattern::Bitmap { png, .. }) => f(png),
            Fill::Pattern(Pattern::Vector { shapes, .. }) => {
                shapes.iter().for_each(|c| pictures(c, f))
            }
            _ => {}
        }
    }
    fn pictures<'a>(s: &'a Shape, f: &mut dyn FnMut(&'a crate::SharedBytes)) {
        match &s.kind {
            ShapeKind::Bitmap { png, fx, .. } => {
                f(png);
                if let Some(fx) = fx {
                    f(&fx.png);
                }
            }
            ShapeKind::Group { children } => children.iter().for_each(|c| pictures(c, f)),
            ShapeKind::ClipFrame { frame, contents } => {
                pictures(frame, f);
                contents.iter().for_each(|c| pictures(c, f));
            }
            _ => {}
        }
        if let Some(r) = &s.raster_fx {
            f(&r.png);
        }
        fill_pictures(&s.fill, f);
        s.extra_fills.iter().for_each(|x| fill_pictures(x, f));
        if let Some(fill) = s.stroke.as_ref().and_then(|st| st.fill.as_deref()) {
            fill_pictures(fill, f);
        }
    }
    fn objects(s: &Shape) -> usize {
        let own = std::mem::size_of::<Shape>();
        let kids = match &s.kind {
            ShapeKind::Path { path, .. } => std::mem::size_of_val(path.elements()),
            ShapeKind::Group { children } => children.iter().map(objects).sum(),
            ShapeKind::ClipFrame { frame, contents } => {
                objects(frame) + contents.iter().map(objects).sum::<usize>()
            }
            _ => 0,
        };
        let fill = match &s.fill {
            Fill::Pattern(Pattern::Vector { shapes, .. }) => shapes.iter().map(objects).sum(),
            _ => 0,
        };
        own + kids + fill
    }
    fn every(d: &Document) -> impl Iterator<Item = &Shape> {
        d.pages
            .iter()
            .flat_map(|p| &p.layers)
            .chain(&d.master)
            .flat_map(|l| &l.shapes)
            .chain(d.symbols.iter().map(|s| &s.shape))
    }
    m.shared.clear();
    m.own.clear();
    for s in every(current) {
        pictures(s, &mut |b| m.shared.push(b.addr()));
    }
    let mut total = 0usize;
    for s in every(snapshot) {
        total += objects(s);
        pictures(s, &mut |b| m.own.push((b.addr(), b.len())));
    }
    m.shared.sort_unstable();
    m.own.sort_unstable();
    m.own.dedup_by_key(|p| p.0);
    for &(addr, len) in &m.own {
        if m.shared.binary_search(&addr).is_err() {
            total += len;
        }
    }
    total
}

#[cfg(test)]
thread_local! {
    /// How often a step was measured on this thread, for tests.
    static MEASURED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Shape, ShapeKind};
    use crate::geometry::Rect;

    /// A paint command (and a batch of paint commands) skips the text
    /// passes: they walk every shape, and a fill cannot change their
    /// result. A geometry command still pays for them.
    #[test]
    fn a_paint_command_skips_the_text_flow_walk() {
        use crate::geometry::Affine;
        use crate::style::Fill;
        use crate::text_flow::refreshes_for_test;
        let mut e = Engine::default();
        let layer = e.document().pages[0].layers[0].id;
        let id = e.new_shape_id();
        e.run(&Command::AddShape {
            layer,
            shape: Shape::new(
                id,
                ShapeKind::Rect {
                    rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                    radius: 0.0,
                    corners: None,
                },
            ),
        })
        .unwrap();
        refreshes_for_test(true);
        e.run(&Command::SetFill {
            shapes: vec![id],
            fill: Fill::Solid(crate::Color::BLACK),
        })
        .unwrap();
        assert_eq!(refreshes_for_test(true), 0, "a fill is no text change");
        e.run_batch(
            "Fill",
            &[
                Command::SetFill {
                    shapes: vec![id],
                    fill: Fill::None,
                },
                Command::SetStroke {
                    shapes: vec![id],
                    stroke: None,
                },
            ],
        )
        .unwrap();
        assert_eq!(refreshes_for_test(true), 0, "a paint batch neither");
        e.run(&Command::TransformShapes {
            shapes: vec![id],
            transform: Affine::translate((5.0, 0.0)),
        })
        .unwrap();
        assert_eq!(refreshes_for_test(true), 1, "a move still refreshes");
        e.run_batch(
            "Fill and move",
            &[
                Command::SetFill {
                    shapes: vec![id],
                    fill: Fill::Solid(crate::Color::BLACK),
                },
                Command::TransformShapes {
                    shapes: vec![id],
                    transform: Affine::translate((5.0, 0.0)),
                },
            ],
        )
        .unwrap();
        assert_eq!(refreshes_for_test(true), 1, "one mover refreshes a batch");
    }

    /// #3440: the drawing a render takes is shared, not copied, and an
    /// edit, an undo and a redo leave it as it was taken.
    #[test]
    fn shared_drawing_is_not_copied_and_never_changes() {
        let mut e = Engine::new(Document::default());
        let a = e.shared_document();
        assert!(Arc::ptr_eq(&a, &e.shared_document()), "nothing copied");
        e.run(&Command::SetTitle { title: "one".into() }).unwrap();
        let b = e.shared_document();
        assert_eq!(e.document().title, "one");
        assert_ne!(a.title, "one", "the shared drawing stays as taken");
        e.undo().unwrap();
        assert_eq!(e.document().title, a.title);
        assert_eq!(b.title, "one");
        e.redo().unwrap();
        assert_eq!(e.document().title, "one");
        assert_ne!(a.title, "one");
        // Ids minted while shared: no copy for them, and never minted
        // twice, before or after the next change.
        let shared = e.shared_document();
        let x = e.new_shape_id();
        let y = e.new_shape_id();
        assert!(Arc::ptr_eq(&shared, &e.shared_document()), "no copy for an id");
        assert_ne!(x, y);
        e.run(&Command::SetTitle { title: "two".into() }).unwrap();
        drop(shared);
        let z = e.new_shape_id();
        assert!(z != x && z != y && z.0 > y.0, "{x:?} {y:?} {z:?}");
    }

    /// #3474 follow-up: an engine whose drawing renders keep taking (and
    /// letting go) goes through edits, ids, undo, redo, amends, groups
    /// and previews exactly as one nobody shares it with; every drawing
    /// taken stays as it was taken, and no id is minted twice.
    #[test]
    fn sharing_the_drawing_changes_nothing_of_what_the_engine_does() {
        use crate::geometry::Affine;
        let mut shared = Engine::default();
        let mut alone = Engine {
            uid_state: shared.uid_state,
            doc: Arc::new(Document::clone(&shared.doc)),
            ..Engine::default()
        };
        let layer = shared.document().pages[0].layers[0].id;
        let mut seed = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = move |n: u64| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed % n
        };
        let mut held: Vec<(Arc<Document>, Document)> = Vec::new();
        let mut minted = std::collections::HashSet::new();
        let mut shapes: Vec<ShapeId> = Vec::new();
        let mut group = None;
        for step in 0..3000 {
            // Renders take the drawing and let it go at random.
            match next(4) {
                0 => {
                    let d = shared.shared_document();
                    let copy = Document::clone(&d);
                    held.push((d, copy));
                }
                1 if !held.is_empty() => {
                    let i = next(held.len() as u64) as usize;
                    let (d, copy) = held.swap_remove(i);
                    assert!(*d == copy, "step {step}: a drawing taken changed");
                }
                _ => {}
            }
            let op = next(12);
            match op {
                0..=2 => {
                    let a = shared.new_shape_id();
                    let b = alone.new_shape_id();
                    assert_eq!(a, b, "step {step}");
                    assert!(minted.insert(a), "step {step}: {a:?} minted twice");
                    let shape = Shape::new(
                        a,
                        ShapeKind::Rect {
                            rect: Rect::new(0.0, 0.0, 5.0 + step as f64, 5.0),
                            radius: 0.0,
                            corners: None,
                        },
                    );
                    let cmd = Command::AddShape { layer, shape };
                    assert_eq!(shared.run(&cmd).is_ok(), alone.run(&cmd).is_ok());
                    shapes.push(a);
                }
                3 => {
                    // An id minted and never used.
                    let a = shared.new_shape_id();
                    assert_eq!(a, alone.new_shape_id());
                    assert!(minted.insert(a), "step {step}: {a:?} minted twice");
                }
                4 if !shapes.is_empty() => {
                    let id = shapes[next(shapes.len() as u64) as usize];
                    let cmd = Command::TransformShapes {
                        shapes: vec![id],
                        transform: Affine::translate((1.0, 2.0)),
                    };
                    assert_eq!(shared.run(&cmd).is_ok(), alone.run(&cmd).is_ok());
                }
                5 if !shapes.is_empty() => {
                    let id = shapes[next(shapes.len() as u64) as usize];
                    let cmd = Command::DeleteShapes { shapes: vec![id] };
                    assert_eq!(shared.run(&cmd).is_ok(), alone.run(&cmd).is_ok());
                }
                6 => {
                    let _ = (shared.undo().is_ok(), alone.undo().is_ok());
                }
                7 => {
                    let _ = (shared.redo().is_ok(), alone.redo().is_ok());
                }
                8 if !shapes.is_empty() => {
                    let id = shapes[next(shapes.len() as u64) as usize];
                    let cmd = Command::SetOpacity {
                        shapes: vec![id],
                        opacity: 0.5,
                    };
                    assert_eq!(shared.amend(&cmd).is_ok(), alone.amend(&cmd).is_ok());
                }
                9 => match group.take() {
                    None => group = Some((shared.begin_group(), alone.begin_group())),
                    Some((a, b)) => {
                        shared.end_group(a, "Group");
                        alone.end_group(b, "Group");
                    }
                },
                10 => {
                    if shared.in_preview() {
                        if next(2) == 0 {
                            shared.withdraw_preview();
                            alone.withdraw_preview();
                        } else {
                            shared.keep_preview();
                            alone.keep_preview();
                        }
                    } else {
                        shared.begin_preview();
                        alone.begin_preview();
                    }
                }
                _ => {
                    // A command that fails leaves both as they were.
                    let cmd = Command::TransformShapes {
                        shapes: vec![ShapeId(u64::MAX)],
                        transform: Affine::IDENTITY,
                    };
                    assert_eq!(shared.run(&cmd).is_ok(), alone.run(&cmd).is_ok());
                }
            }
            // The counters may sit aside (ids minted while shared) until
            // the next change: compared through `ids()` below.
            let mut x = Document::clone(shared.document());
            let mut y = Document::clone(alone.document());
            x.set_ids(shared.ids().clone());
            y.set_ids(alone.ids().clone());
            assert!(x == y, "step {step} (op {op})");
            assert_eq!(shared.revision(), alone.revision(), "step {step}");
            assert_eq!(shared.ids().peek_next(), alone.ids().peek_next(), "step {step}");
        }
        for (d, copy) in held {
            assert!(*d == copy, "a drawing taken changed");
        }
    }

    /// Each step has an id of its own, kept through undo, redo and
    /// amend; a step undone and replaced by another is gone for good.
    #[test]
    fn steps_have_ids_that_follow_undo_and_redo() {
        let mut eng = Engine::default();
        let layer = eng.document().pages[0].layers[0].id;
        let add = |eng: &mut Engine| {
            let id = eng.new_shape_id();
            let shape = Shape::new(
                id,
                ShapeKind::Rect {
                    rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                    radius: 0.0,
                    corners: None,
                },
            );
            eng.run(&Command::AddShape { layer, shape }).unwrap();
            (id, eng.last_step_id().expect("a step"))
        };
        assert_eq!(eng.last_step_id(), None);
        let (shape, a) = add(&mut eng);
        let (_, b) = add(&mut eng);
        assert_ne!(a, b);
        // The steps in the order they were made.
        assert_eq!(eng.step_below(b), Some(a));
        assert_eq!(eng.step_below(a), None);
        assert!(eng.step_in_effect(a) && eng.step_in_effect(b));
        eng.amend(&Command::SetOpacity {
            shapes: vec![shape],
            opacity: 0.5,
        })
        .unwrap();
        assert_eq!(eng.last_step_id(), Some(b));
        eng.undo().unwrap();
        assert!(eng.step_in_effect(a) && !eng.step_in_effect(b));
        assert_eq!(eng.last_step_id(), Some(a));
        eng.redo().unwrap();
        assert!(eng.step_in_effect(b));
        eng.undo().unwrap();
        let (_, c) = add(&mut eng);
        assert!(c != a && c != b);
        assert!(!eng.step_in_effect(b));
    }

    /// A step withdrawn leaves no trace: the drawing is as before it and
    /// neither Undo nor Redo knows it. Only the last step, with nothing
    /// done since, can be withdrawn (#1481).
    #[test]
    fn a_withdrawn_step_leaves_no_undo_or_redo() {
        let mut eng = Engine::default();
        let layer = eng.document().pages[0].layers[0].id;
        let add = |eng: &mut Engine| {
            let id = eng.new_shape_id();
            let shape = Shape::new(
                id,
                ShapeKind::Rect {
                    rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                    radius: 0.0,
                    corners: None,
                },
            );
            eng.run(&Command::AddShape { layer, shape }).unwrap();
            (id, eng.last_step_id().expect("a step"))
        };
        let (first, a) = add(&mut eng);
        let (second, b) = add(&mut eng);
        assert!(!eng.withdraw_step(a), "not the last step");
        assert!(eng.withdraw_step(b));
        assert!(eng.document().find_shape(second).is_none());
        assert!(eng.document().find_shape(first).is_some());
        assert_eq!(eng.history_labels().0.len(), 1);
        assert!(eng.redo().is_err(), "nothing to redo");
        // Something done since the step: it stays.
        let (_, c) = add(&mut eng);
        eng.undo().unwrap();
        eng.redo().unwrap();
        assert!(!eng.withdraw_step(c));
        assert!(eng.step_in_effect(c));
    }

    #[test]
    fn objects_keep_their_identity_and_copies_get_their_own() {
        use crate::document::{Shape, ShapeKind};
        use crate::geometry::Rect;
        let mut eng = Engine::default();
        let layer = eng.document().pages[0].layers[0].id;
        let id = eng.new_shape_id();
        let shape = Shape::new(
            id,
            ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                radius: 0.0,
                corners: None,
            },
        );
        eng.run(&Command::AddShape { layer, shape }).unwrap();
        let uid = eng.document().layer(layer).unwrap().shapes[0]
            .uid
            .expect("given one");
        // Saved and opened again: the same.
        let back = Document::from_json(&eng.document().to_json().unwrap()).unwrap();
        let again = Engine::new(back);
        assert_eq!(
            again.document().layer(layer).unwrap().shapes[0].uid,
            Some(uid)
        );
        // A copy carrying it gets its own; the original keeps it.
        let mut copy = eng.document().layer(layer).unwrap().shapes[0].clone();
        copy.id = eng.new_shape_id();
        eng.run(&Command::AddShape { layer, shape: copy }).unwrap();
        let shapes = &eng.document().layer(layer).unwrap().shapes;
        assert_eq!(shapes[0].uid, Some(uid));
        assert!(shapes[1].uid.is_some() && shapes[1].uid != Some(uid));
        // Undo and redo bring back the same identities.
        let second = shapes[1].uid;
        eng.undo().unwrap();
        eng.redo().unwrap();
        assert_eq!(eng.document().layer(layer).unwrap().shapes[1].uid, second);
        // Written as 32 hex digits.
        let json = serde_json::to_string(&uid).unwrap();
        assert_eq!(json.len(), 34);
        assert_eq!(
            serde_json::from_str::<crate::document::Uid>(&json).unwrap(),
            uid
        );
        assert!(serde_json::from_str::<crate::document::Uid>("\"zz\"").is_err());
    }

    #[test]
    fn a_drawing_marked_unsaved_stays_so_until_saved() {
        let mut eng = Engine::default();
        assert!(!eng.is_dirty());
        eng.mark_unsaved();
        assert!(eng.is_dirty());
        eng.mark_saved();
        assert!(!eng.is_dirty());
    }

    #[test]
    fn undo_redo_round_trip() {
        let mut eng = Engine::default();
        let layer = eng.document().pages[0].layers[0].id;
        let id = eng.new_shape_id();
        let shape = Shape::new(
            id,
            ShapeKind::Ellipse {
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                arc: None,
            },
        );
        eng.run(&Command::AddShape { layer, shape }).unwrap();
        assert_eq!(eng.document().layer(layer).unwrap().shapes.len(), 1);
        assert!(eng.is_dirty());
        assert_eq!(eng.undo().unwrap(), "Create Object");
        assert_eq!(eng.document().layer(layer).unwrap().shapes.len(), 0);
        eng.redo().unwrap();
        assert_eq!(eng.document().layer(layer).unwrap().shapes.len(), 1);
        assert!(matches!(eng.redo(), Err(Error::NothingToRedo)));
    }

    #[test]
    fn amending_joins_the_last_step_only_right_after_it() {
        let mut eng = Engine::default();
        let layer = eng.document().pages[0].layers[0].id;
        let id = eng.new_shape_id();
        let shape = Shape::new(
            id,
            ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                radius: 0.0,
                corners: None,
            },
        );
        eng.run(&Command::AddShape { layer, shape }).unwrap();
        let red = crate::Color::rgb8(255, 0, 0);
        eng.amend(&Command::SetDocumentPalette { colors: vec![red] })
            .unwrap();
        assert_eq!(eng.history_labels().0.len(), 1);
        eng.undo().unwrap();
        assert!(eng.document().palette.is_empty());
        assert_eq!(eng.document().layer(layer).unwrap().shapes.len(), 0);
        // After an undo there is no step to join: a step of its own.
        eng.amend(&Command::SetDocumentPalette { colors: vec![red] })
            .unwrap();
        assert_eq!(eng.history_labels().0, vec!["Document Palette"]);
    }

    #[test]
    fn batches_respect_the_undo_levels() {
        let mut eng = Engine::default();
        eng.set_max_history(2);
        for i in 0..4 {
            let colors = vec![crate::Color::rgb8(i, 0, 0)];
            eng.run_batch("Palette", &[Command::SetDocumentPalette { colors }])
                .unwrap();
        }
        assert_eq!(eng.history_labels().0.len(), 2);
    }

    /// The view a drawing is saved with is kept without an undo step,
    /// without marking the drawing changed, through undo and redo, and
    /// goes into the file and back (#1134).
    #[test]
    fn the_saved_view_stays_out_of_the_history() {
        let mut eng = Engine::default();
        let layer = eng.document().pages[0].layers[0].id;
        let id = eng.new_shape_id();
        let shape = Shape::new(
            id,
            ShapeKind::Ellipse {
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                arc: None,
            },
        );
        eng.run(&Command::AddShape { layer, shape }).unwrap();
        eng.mark_saved();
        let rev = eng.revision();
        let view = crate::document::SavedView {
            page: 0,
            x: 12.5,
            y: -3.0,
            zoom_percent: 250.0,
        };
        eng.run(&Command::SetSavedView { view: Some(view) })
            .unwrap();
        assert_eq!(eng.history_labels().0, vec!["Create Object"]);
        assert_eq!(eng.revision(), rev);
        assert!(!eng.is_dirty());
        eng.undo().unwrap();
        assert_eq!(eng.document().saved_view, Some(view));
        eng.redo().unwrap();
        assert_eq!(eng.document().saved_view, Some(view));
        let mut json = Vec::new();
        eng.document()
            .write_json_version(&mut json, crate::document::TDRAW_FORMAT_VERSION)
            .unwrap();
        let back = crate::Document::from_json(std::str::from_utf8(&json).unwrap()).unwrap();
        assert_eq!(back.saved_view, Some(view));
        assert!(view.is_valid(1) && !view.is_valid(0));
        assert!(!crate::document::SavedView {
            zoom_percent: f64::NAN,
            ..view
        }
        .is_valid(1));
        // A file without one opens without one.
        let plain = crate::Document::from_json(
            &String::from_utf8(json.clone())
                .unwrap()
                .replace("\"saved_view\"", "\"unknown_saved_view\""),
        )
        .unwrap();
        assert_eq!(plain.saved_view, None);
    }

    #[test]
    fn time_tracker_stays_out_of_the_history() {
        let mut eng = Engine::default();
        let layer = eng.document().pages[0].layers[0].id;
        let id = eng.new_shape_id();
        let shape = Shape::new(
            id,
            ShapeKind::Ellipse {
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                arc: None,
            },
        );
        eng.run(&Command::AddShape { layer, shape }).unwrap();
        let rev = eng.revision();
        let mut tracker = crate::TimeTracker::default();
        tracker.tasks.push(crate::TrackedTask {
            name: "Layout".into(),
            seconds: 90.0,
            ..Default::default()
        });
        tracker.active = Some(0);
        eng.run(&Command::SetTimeTracker {
            tracker: tracker.clone(),
        })
        .unwrap();
        eng.amend(&Command::SetTimeTracker {
            tracker: tracker.clone(),
        })
        .unwrap();
        // No step and no new revision for it.
        assert_eq!(eng.history_labels().0, vec!["Create Object"]);
        assert_eq!(eng.revision(), rev);
        // Undo and redo of the drawing leave the records alone.
        eng.undo().unwrap();
        assert_eq!(eng.document().time_tracker, tracker);
        eng.redo().unwrap();
        assert_eq!(eng.document().time_tracker, tracker);
        // An explicit edit still marks the drawing changed, without a step.
        eng.mark_saved();
        assert!(!eng.is_dirty());
        eng.mark_changed();
        assert!(eng.is_dirty());
        assert_eq!(eng.history_labels().0.len(), 1);
    }

    #[test]
    fn recording_moves_the_start_and_finish() {
        let mut t = crate::TimeTracker::default();
        // Without an active task nothing is recorded.
        t.record(10.0, 1000);
        assert!(t.tasks.is_empty() && t.active_task().is_none());
        t.tasks.push(crate::TrackedTask {
            name: "A".into(),
            ..Default::default()
        });
        t.active = Some(0);
        t.record(30.0, 1000);
        t.record(f64::NAN, 2000);
        t.record(-5.0, 2000);
        t.record(15.0, 1100);
        let a = &t.tasks[0];
        assert_eq!(a.seconds, 45.0);
        assert_eq!(a.start, Some(970));
        assert_eq!(a.finish, Some(1100));
        assert_eq!(t.total_seconds(), 45.0);
        // A stale index is no active task.
        t.active = Some(3);
        assert!(t.active_task().is_none());
        t.record(5.0, 1200);
        assert_eq!(t.total_seconds(), 45.0);
    }

    #[test]
    fn failed_command_leaves_history_alone() {
        let mut eng = Engine::default();
        let rev = eng.revision();
        assert!(eng
            .run(&Command::DeleteShapes {
                shapes: vec![crate::ShapeId(42)]
            })
            .is_err());
        assert_eq!(eng.revision(), rev);
        assert!(eng.undo_label().is_none());
    }

    #[test]
    fn a_withdrawn_preview_leaves_the_history_as_it_was() {
        use crate::document::{Shape, ShapeKind};
        use crate::geometry::Rect;
        let mut e = Engine::default();
        let layer = e.document().pages[0].layers[0].id;
        let add = |e: &mut Engine| {
            let id = e.new_shape_id();
            let shape = Shape::new(
                id,
                ShapeKind::Rect {
                    rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                    radius: 0.0,
                    corners: None,
                },
            );
            e.run(&Command::AddShape { layer, shape }).unwrap();
        };
        add(&mut e);
        add(&mut e);
        e.undo().unwrap();
        e.mark_saved();
        let doc = e.document().clone();
        let (undo, redo) = e.history_labels();
        e.begin_preview();
        add(&mut e);
        add(&mut e);
        assert!(e.in_preview());
        e.withdraw_preview();
        assert_eq!(e.document().pages, doc.pages);
        assert_eq!(e.history_labels(), (undo.clone(), redo.clone()));
        assert!(!e.is_dirty());
        // The step that could be redone still can.
        e.redo().unwrap();
        // Kept: the preview's steps are ordinary steps.
        e.begin_preview();
        add(&mut e);
        e.keep_preview();
        assert!(!e.in_preview());
        assert_eq!(e.history_labels().0.len(), undo.len() + 2);
    }

    /// An undo step shares the pictures with the drawing instead of
    /// copying them.
    #[test]
    fn an_undo_step_shares_picture_bytes_with_the_drawing() {
        use crate::document::{Shape, ShapeKind};
        use crate::geometry::Rect;
        use crate::style::{Fill, Pattern};
        use crate::SharedBytes;
        let mut e = Engine::default();
        let layer = e.document().pages[0].layers[0].id;
        let png = SharedBytes::from(vec![7u8; 4096]);
        let id = e.new_shape_id();
        let mut shape = Shape::new(
            id,
            ShapeKind::Bitmap {
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                width_px: 8,
                height_px: 8,
                png: png.clone(),
                fx: None,
            },
        );
        shape.fill = Fill::Pattern(Pattern::Bitmap {
            png: png.clone(),
            width_px: 8,
            height_px: 8,
            size_mm: 5.0,
            placement: Default::default(),
        });
        e.run(&Command::AddShape { layer, shape }).unwrap();
        e.run(&Command::TransformShapes {
            shapes: vec![id],
            transform: crate::Affine::translate((1.0, 0.0)),
        })
        .unwrap();
        let pictures = |d: &Document| {
            let s = &d.pages[0].layers[0].shapes[0];
            let ShapeKind::Bitmap { png: a, .. } = &s.kind else {
                panic!("not a bitmap")
            };
            let Fill::Pattern(Pattern::Bitmap { png: b, .. }) = &s.fill else {
                panic!("not a bitmap fill")
            };
            (a.clone(), b.clone())
        };
        let (now_a, now_b) = pictures(e.document());
        let (was_a, was_b) = pictures(&e.undo[1].other);
        assert!(SharedBytes::ptr_eq(&now_a, &png));
        assert!(SharedBytes::ptr_eq(&now_a, &was_a));
        assert!(SharedBytes::ptr_eq(&now_b, &was_b));
    }

    /// Past the memory limit the oldest steps go; the newest one stays
    /// however large it is, and pictures shared with the drawing do not
    /// count.
    #[test]
    fn the_history_keeps_to_its_memory_limit() {
        use crate::document::{Shape, ShapeKind};
        use crate::geometry::Rect;
        let mut e = Engine::default();
        // Measured only with a limit.
        e.set_max_history_bytes(usize::MAX - 1);
        let layer = e.document().pages[0].layers[0].id;
        let picture = crate::SharedBytes::from(vec![1u8; 1 << 20]);
        let id = e.new_shape_id();
        let shape = Shape::new(
            id,
            ShapeKind::Bitmap {
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                width_px: 8,
                height_px: 8,
                png: picture.clone(),
                fx: None,
            },
        );
        e.run(&Command::AddShape { layer, shape }).unwrap();
        let mv = |e: &mut Engine| {
            e.run(&Command::TransformShapes {
                shapes: vec![id],
                transform: crate::Affine::translate((1.0, 0.0)),
            })
            .unwrap()
        };
        for _ in 0..10 {
            mv(&mut e);
        }
        // The picture is shared by every step: none counts it.
        assert!(e.history_bytes() < 64 << 10, "{}", e.history_bytes());
        assert_eq!(e.history_labels().0.len(), 11);
        // A limit of two steps' worth keeps two.
        let step = e.undo[5].bytes;
        e.set_max_history_bytes(step * 2);
        assert_eq!(e.history_labels().0.len(), 2);
        mv(&mut e);
        assert_eq!(e.history_labels().0.len(), 2);
        // Smaller than one step: the newest still stays, so Undo works.
        e.set_max_history_bytes(1);
        assert_eq!(e.history_labels().0.len(), 1);
        e.undo().unwrap();
        // A step that replaced the picture holds the old one of its own.
        let mut e = Engine::default();
        e.set_max_history_bytes(usize::MAX - 1);
        let shape = Shape::new(
            id,
            ShapeKind::Bitmap {
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                width_px: 8,
                height_px: 8,
                png: picture.clone(),
                fx: None,
            },
        );
        e.run(&Command::AddShape { layer, shape }).unwrap();
        e.run(&Command::DeleteShapes { shapes: vec![id] }).unwrap();
        assert!(e.history_bytes() >= 1 << 20);
    }

    /// Steps grouped into one stay one at the memory limit: the limit
    /// waits for the group to end, so the copy and its move are one Undo.
    #[test]
    fn a_group_stays_one_step_at_the_memory_limit() {
        use crate::document::{Shape, ShapeKind};
        use crate::geometry::Rect;
        let mut e = Engine::default();
        let layer = e.document().pages[0].layers[0].id;
        let add = |e: &mut Engine| {
            let id = e.new_shape_id();
            let shape = Shape::new(
                id,
                ShapeKind::Rect {
                    rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                    radius: 0.0,
                    corners: None,
                },
            );
            e.run(&Command::AddShape { layer, shape }).unwrap();
            id
        };
        e.set_max_history_bytes(usize::MAX - 1);
        let first = add(&mut e);
        add(&mut e);
        let step = e.undo.iter().map(|x| x.bytes).max().unwrap_or(1);
        // Two steps' worth.
        e.set_max_history_bytes(step * 2 + step / 2);
        let before = e.document().pages[0].layers[0].shapes.len();
        let mark = e.begin_group();
        let copy = add(&mut e);
        e.run(&Command::TransformShapes {
            shapes: vec![copy, first],
            transform: crate::Affine::translate((5.0, 0.0)),
        })
        .unwrap();
        e.end_group(mark, "Duplicate");
        assert_eq!(e.undo_label(), Some("Duplicate"));
        e.undo().unwrap();
        assert_eq!(e.document().pages[0].layers[0].shapes.len(), before);
        let moved = e.document().find_shape(first).map(|s| s.transform);
        assert_eq!(moved, Some(crate::Affine::IDENTITY), "the move went too");
    }

    /// Joining a group's steps as they come keeps one step (one snapshot)
    /// however many steps the action runs, and Undo still goes back to
    /// before the group (#1342).
    #[test]
    fn joining_a_group_keeps_one_step_while_it_runs() {
        use crate::document::{Shape, ShapeKind};
        use crate::geometry::Rect;
        let mut e = Engine::default();
        let layer = e.document().pages[0].layers[0].id;
        let id = e.new_shape_id();
        let shape = Shape::new(
            id,
            ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                radius: 0.0,
                corners: None,
            },
        );
        e.run(&Command::AddShape { layer, shape }).unwrap();
        let steps = e.undo.len();
        let mark = e.begin_group();
        for _ in 0..50 {
            e.run(&Command::TransformShapes {
                shapes: vec![id],
                transform: crate::Affine::rotate(0.1),
            })
            .unwrap();
            e.join_group(mark, "Free Transform");
            assert_eq!(e.undo.len(), steps + 1);
        }
        e.end_group(mark, "Free Transform");
        assert_eq!(e.undo.len(), steps + 1);
        assert_eq!(e.undo_label(), Some("Free Transform"));
        e.undo().unwrap();
        let back = e.document().find_shape(id).map(|s| s.transform);
        assert_eq!(back, Some(crate::Affine::IDENTITY));
    }

    /// A drag that undoes and records a step per pointer move, and steps
    /// recorded during a preview, are not measured each time.
    #[test]
    fn live_steps_are_not_measured_again() {
        use crate::document::{Shape, ShapeKind};
        use crate::geometry::Rect;
        let mut e = Engine::default();
        e.set_max_history_bytes(1 << 30);
        let layer = e.document().pages[0].layers[0].id;
        let id = e.new_shape_id();
        let shape = Shape::new(
            id,
            ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                radius: 0.0,
                corners: None,
            },
        );
        e.run(&Command::AddShape { layer, shape }).unwrap();
        let fill = |e: &mut Engine, k: u8| {
            e.run(&Command::SetFill {
                shapes: vec![id],
                fill: crate::Fill::Solid(crate::Color::rgb8(k, 0, 0)),
            })
            .unwrap()
        };
        fill(&mut e, 0);
        let count = || MEASURED.with(|c| c.get());
        let start = count();
        for k in 1..20 {
            e.undo().unwrap();
            fill(&mut e, k);
        }
        assert_eq!(count(), start, "undo + run per move");
        e.begin_preview();
        for k in 0..20 {
            fill(&mut e, k);
        }
        assert_eq!(count(), start, "inside a preview");
        e.withdraw_preview();
        assert_eq!(count(), start);
        e.begin_preview();
        fill(&mut e, 3);
        fill(&mut e, 4);
        e.keep_preview();
        assert_eq!(count(), start + 2, "kept steps, once each");
        // Without a limit nothing is measured.
        e.set_max_history_bytes(usize::MAX);
        fill(&mut e, 5);
        assert_eq!(count(), start + 2);
    }

    /// A step redone during a preview that recorded nothing comes back
    /// to the redo stack when the preview is withdrawn.
    #[test]
    fn a_step_redone_during_a_preview_can_be_redone_after_it() {
        use crate::document::{Shape, ShapeKind};
        use crate::geometry::Rect;
        let mut e = Engine::default();
        let layer = e.document().pages[0].layers[0].id;
        let add = |e: &mut Engine| {
            let id = e.new_shape_id();
            let shape = Shape::new(
                id,
                ShapeKind::Rect {
                    rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                    radius: 0.0,
                    corners: None,
                },
            );
            e.run(&Command::AddShape { layer, shape }).unwrap();
        };
        add(&mut e);
        add(&mut e);
        e.undo().unwrap();
        e.undo().unwrap();
        let labels = e.history_labels();
        let pages = e.document().pages.clone();
        e.begin_preview();
        e.redo().unwrap();
        add(&mut e);
        e.withdraw_preview();
        assert_eq!(e.history_labels(), labels);
        assert_eq!(e.document().pages, pages);
        e.redo().unwrap();
        e.redo().unwrap();
        assert_eq!(e.document().pages[0].layers[0].shapes.len(), 2);
        // Redone and nothing recorded: the same.
        e.undo().unwrap();
        e.undo().unwrap();
        e.begin_preview();
        e.redo().unwrap();
        e.withdraw_preview();
        assert_eq!(e.history_labels(), labels);
        e.redo().unwrap();
        e.redo().unwrap();
        assert!(e.redo().is_err());
    }

    /// A preview moves the redo stack aside instead of copying it, and
    /// puts it back when nothing recorded since cleared it.
    #[test]
    fn a_preview_keeps_the_redo_stack_without_copying_it() {
        use crate::document::{Shape, ShapeKind};
        use crate::geometry::Rect;
        let mut e = Engine::default();
        let layer = e.document().pages[0].layers[0].id;
        let add = |e: &mut Engine| {
            let id = e.new_shape_id();
            let shape = Shape::new(
                id,
                ShapeKind::Rect {
                    rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                    radius: 0.0,
                    corners: None,
                },
            );
            e.run(&Command::AddShape { layer, shape }).unwrap();
        };
        add(&mut e);
        add(&mut e);
        e.undo().unwrap();
        e.undo().unwrap();
        let labels = e.history_labels();
        e.begin_preview();
        // Moved, not copied; still listed and still redoable.
        assert!(e.redo.is_empty());
        assert_eq!(e.history_labels(), labels);
        assert!(e.redo_label().is_some());
        e.keep_preview();
        assert_eq!(e.history_labels(), labels);
        e.redo().unwrap();
        e.redo().unwrap();
        // A step recorded during a kept preview drops the redo stack.
        e.undo().unwrap();
        e.begin_preview();
        add(&mut e);
        e.keep_preview();
        assert!(e.redo_label().is_none());
    }

    /// #3902, #3904: amending an isolated text object mutates in place
    /// without cloning unrelated curves on the page when the document is
    /// unshared, tracks single-shape runs across characters, and still
    /// propagates content links when present.
    #[test]
    fn isolated_text_amends_mutate_in_place_without_cloning_other_shapes() {
        use crate::document::{spans_text, ParagraphStyle, TextAlign, TextSpan};
        use crate::geometry::{rect_path, Point};
        let mut e = Engine::default();
        let layer = e.document().pages[0].layers[0].id;
        let curve_id = e.new_shape_id();
        let curve = Shape::new(
            curve_id,
            ShapeKind::Path {
                path: rect_path(Rect::new(0.0, 0.0, 50.0, 50.0), 0.0),
                closed: true,
            },
        );
        e.run(&Command::AddShape {
            layer,
            shape: curve,
        })
        .unwrap();
        let text_id = e.new_shape_id();
        let make_kind = |s: &str| ShapeKind::Text {
            spans: vec![TextSpan::new(s, "Sans", 12.0)],
            origin: Point::new(10.0, 20.0),
            frame: None,
            align: TextAlign::Left,
            para: ParagraphStyle::default(),
            on_path: None,
        };
        e.run(&Command::AddShape {
            layer,
            shape: Shape::new(text_id, make_kind("")),
        })
        .unwrap();
        let start_rev = e.revision();
        assert!(e.unchanged_except(text_id, start_rev));

        // First character creates the Edit Text step; subsequent characters
        // amend it in place without cloning the bystander path's elements.
        e.run_with_label(
            &Command::SetShapeKind {
                shape: text_id,
                kind: make_kind("a"),
            },
            "Edit Text",
        )
        .unwrap();
        let curve_ptr = |eng: &Engine| match &eng.document().find_shape(curve_id).unwrap().kind {
            ShapeKind::Path { path, .. } => path.elements().as_ptr() as usize,
            _ => panic!("expected path"),
        };
        let ptr_before = curve_ptr(&e);
        for s in ["ab", "abc", "abcd"] {
            e.amend(&Command::SetShapeKind {
                shape: text_id,
                kind: make_kind(s),
            })
            .unwrap();
            assert_eq!(
                curve_ptr(&e),
                ptr_before,
                "unshared amend should not clone bystander paths"
            );
            assert!(e.unchanged_except(text_id, start_rev));
        }
        // Editing another shape breaks the single-shape run.
        assert!(!e.unchanged_except(curve_id, start_rev));
        e.run(&Command::SetFill {
            shapes: vec![curve_id],
            fill: crate::Fill::Solid(crate::Color::rgb8(10, 20, 30)),
        })
        .unwrap();
        assert!(!e.unchanged_except(text_id, start_rev));

        // Undo the fill, then one Undo takes all four characters back.
        e.undo().unwrap();
        e.undo().unwrap();
        let ShapeKind::Text { spans, .. } = &e.document().find_shape(text_id).unwrap().kind else {
            panic!("expected text");
        };
        assert_eq!(spans_text(spans), "");

        // Linked texts do not skip sync_linked_text.
        let linked_id = e.new_shape_id();
        let mut s2 = Shape::new(linked_id, make_kind(""));
        s2.content_link = Some(7);
        e.run(&Command::AddShape { layer, shape: s2 }).unwrap();
        e.run(&Command::LinkTextContent {
            shapes: vec![text_id, linked_id],
        })
        .unwrap();
        e.run_with_label(
            &Command::SetShapeKind {
                shape: text_id,
                kind: make_kind("x"),
            },
            "Edit Text",
        )
        .unwrap();
        e.amend(&Command::SetShapeKind {
            shape: text_id,
            kind: make_kind("xy"),
        })
        .unwrap();
        let ShapeKind::Text { spans: other, .. } =
            &e.document().find_shape(linked_id).unwrap().kind
        else {
            panic!("expected linked text");
        };
        assert_eq!(spans_text(other), "xy");
    }
}
