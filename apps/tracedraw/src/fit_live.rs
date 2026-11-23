//! Live Fit Objects to Path. With Live on in the Fit Objects to Path
//! panel, the fit stays: the path keeps it in its object data
//! (`fit_path.live`): the panel's settings, the objects fitted (by their
//! identities, each with the move the fit gave it) and the copies it
//! made. After every change of the drawing each live fit is worked out
//! again from the objects as they were before the fit, in the same undo
//! step: moving or reshaping the path, or resizing, reshaping or
//! recolouring an object, fits the objects again; copies follow their
//! originals. Selecting the path or one of its objects shows the fit's
//! settings in the panel, where a change refits at once, and two handles
//! on the canvas with the Pick tool: one on the second object drags the
//! spacing along the path, one under the path's middle turns all the
//! objects. Turning Live off lets the objects go where they are. See
//! docs/behavior/fit-to-path.md.

use crate::app::{App, Drag};
use crate::fit_path::{FitAlign, FitPathSettings, FitSpacing};
use crate::tools::Tool;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracedraw_core::{
    document::{Shape, ShapeKind, Uid},
    geometry::{Affine, Point, Vec2},
    Command, ShapeId,
};

/// The object data key holding a live fit, on the path.
pub const DATA_KEY: &str = "fit_path.live";
/// The undo label of changes to a live fit.
const LABEL: &str = "Fit Objects to Path";
/// How far below the path's middle the turn handle sits, screen pixels.
pub const TURN_HANDLE_PX: f32 = 30.0;
/// The reach of a press on a handle, screen pixels.
const HANDLE_PICK_PX: f64 = 7.0;

/// An object a live fit places.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Member {
    /// Its identity ([`Shape::uid`], hexadecimal).
    pub uid: String,
    /// The move (page space) the fit gave it; `None` while it stays where
    /// it was (Keep originals).
    #[serde(default)]
    pub placed: Option<[f64; 6]>,
}

/// A live fit, as the path's object data keeps it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveFit {
    /// The path's own identity: a copy of the path carries the data with
    /// another identity and lets it go.
    pub path: String,
    pub settings: FitPathSettings,
    pub members: Vec<Member>,
    /// The copies the fit made, in the order it placed them.
    #[serde(default)]
    pub copies: Vec<String>,
}

/// The first point of `path` (any point would do to read its length).
fn page_path_start(path: &tracedraw_core::geometry::BezPath) -> Point {
    match path.elements().first() {
        Some(kurbo::PathEl::MoveTo(p)) => *p,
        _ => Point::ZERO,
    }
}

/// An identity as the data writes it.
pub fn uid_text(u: Uid) -> String {
    format!("{:032x}", u.0)
}

fn affine_of(c: &[f64; 6]) -> Option<Affine> {
    let a = Affine::new(*c);
    let det = a.determinant();
    (c.iter().all(|v| v.is_finite()) && det.is_finite() && det.abs() > 1e-12).then_some(a)
}

/// Whether `a` changes nothing that shows (a rounding error's worth).
fn near_identity(a: Affine) -> bool {
    let c = a.as_coeffs();
    let id = Affine::IDENTITY.as_coeffs();
    c.iter().zip(id.iter()).all(|(x, y)| (x - y).abs() < 1e-9)
}

fn near_same(a: Affine, b: Affine) -> bool {
    a.as_coeffs()
        .iter()
        .zip(b.as_coeffs().iter())
        .all(|(x, y)| (x - y).abs() < 1e-9)
}

impl LiveFit {
    /// The live fit a shape holds.
    pub fn of(s: &Shape) -> Option<LiveFit> {
        let (_, v) = s.data.iter().find(|(k, _)| k == DATA_KEY)?;
        let l: LiveFit = serde_json::from_str(v).ok()?;
        l.members
            .iter()
            .all(|m| m.placed.as_ref().is_none_or(|c| affine_of(c).is_some()))
            .then_some(l)
    }

    /// `data` with this fit in it (taken out with `None`).
    pub fn into_data(fit: Option<&LiveFit>, data: &[(String, String)]) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = data
            .iter()
            .filter(|(k, _)| k != DATA_KEY)
            .cloned()
            .collect();
        if let Some(text) = fit.and_then(|f| serde_json::to_string(f).ok()) {
            out.push((DATA_KEY.to_string(), text));
        }
        out
    }
}

/// How far (mm) a node of an object the fit places may lie from where the
/// file drew it for a stroke read from a `.cdr` file to be taken live.
const ADOPT_TOLERANCE: f64 = 0.15;

/// The points of `s` on the page, in order: its nodes and handles.
fn page_points(s: &Shape) -> Vec<Point> {
    let mut pts = Vec::new();
    for el in s.page_path().elements() {
        match *el {
            kurbo::PathEl::MoveTo(p) | kurbo::PathEl::LineTo(p) => pts.push(p),
            kurbo::PathEl::QuadTo(a, p) => pts.extend([a, p]),
            kurbo::PathEl::CurveTo(a, b, p) => pts.extend([a, b, p]),
            kurbo::PathEl::ClosePath => {}
        }
    }
    pts
}

/// The live fit that places `objects` (in their order) on `guide` as the
/// file drew them: the first object the one fitted, the others its
/// copies, spaced by distance and turned with the path. The settings are
/// measured from the objects; `None` unless the fit worked out from them
/// puts every object back where it is, within [`ADOPT_TOLERANCE`].
fn imported_fit(guide: &Shape, objects: &[Shape]) -> Option<LiveFit> {
    let path = guide.page_path();
    // From the start of the path (files list them either way).
    let mut objects: Vec<(f64, &Shape)> = objects
        .iter()
        .map(|o| {
            crate::fit_path::path_distance_of(&path, &FitPathSettings::default(), o.bounds().center())
                .map(|(d, _, _)| (d, o))
        })
        .collect::<Option<_>>()?;
    objects.sort_by(|a, b| a.0.total_cmp(&b.0));
    let objects: Vec<Shape> = objects.into_iter().map(|(_, o)| o.clone()).collect();
    let (first, rest) = objects.split_first()?;
    if rest.is_empty() {
        return None;
    }
    let mut set = FitPathSettings {
        spacing: FitSpacing::Distance,
        copies: crate::fit_path::FitCopies::Fill,
        rotation: crate::fit_path::FitRotation::Follow,
        live: true,
        ..FitPathSettings::default()
    };
    // How the path is walked (forwards, every subpath), as `set` walks it.
    let walk = FitPathSettings::default();
    let turn_of = |s: &Shape| {
        let c = s.transform.as_coeffs();
        c[1].atan2(c[0])
    };
    let direction = |d: f64| {
        crate::fit_path::path_point(&path, &walk, d).map(|(p, t)| (p, t.y.atan2(t.x), t))
    };
    // The first object turned back as it lies on the path: what the fit
    // places. Its box centre is the point the fit puts on the path.
    let (d0, _, _) = crate::fit_path::path_distance_of(&path, &walk, first.bounds().center())?;
    let (_, phi0, _) = direction(d0)?;
    let angle = (turn_of(first) - phi0).to_degrees();
    // From -180 (left out) to 180 degrees, in thousandths.
    let angle = (angle.rem_euclid(360.0) * 1000.0).round() / 1000.0;
    set.angle = if angle > 180.0 { angle - 360.0 } else { angle };
    let unturn = Affine::rotate_about(-turn_of(first), first.bounds().center());
    let mut source = first.clone();
    source.transform_by(unturn);
    let centre = source.bounds().center();
    // Where each object's centre (as the source has it) lands, and how
    // far along the path and off it that is.
    let landed = |s: &Shape| -> Option<(f64, f64)> {
        let to = s.transform * source.transform.inverse();
        let p = to * centre;
        let (d, _, _) = crate::fit_path::path_distance_of(&path, &walk, p)?;
        let (q, _, t) = direction(d)?;
        Some((d, (p - q).dot(Vec2::new(-t.y, t.x))))
    };
    let (start, shift) = landed(first)?;
    let (end, _) = landed(objects.last()?)?;
    let distance = (end - start) / rest.len() as f64;
    if !(distance.is_finite() && distance > 1e-3) {
        return None;
    }
    // Whole microns: the file's step is a round number of them.
    set.distance = (distance * 1e3).round() / 1e3;
    set.offset = (start * 1e3).round() / 1e3;
    if shift.abs() > 1e-3 {
        // The centre lies off the path: placed above it, the object's
        // half width across the path and the side make up the shift.
        let b = source.bounds();
        let a = set.angle.to_radians();
        let across = b.width() / 2.0 * a.sin().abs() + b.height() / 2.0 * a.cos().abs();
        set.place = crate::fit_path::FitPlace::Above;
        set.side = ((shift - across) * 1e4).round() / 1e4;
    }
    // As many as the file has: the room ends half a step after the last.
    let (_, total, closed) = crate::fit_path::path_distance_of(&path, &walk, centre)?;
    if closed {
        return None;
    }
    let room = total - set.offset;
    let count = (room / set.distance + 1e-9).floor() as usize + 1;
    if count != objects.len() {
        let wanted = set.distance * (objects.len() as f64 - 0.5);
        set.end_offset = ((room - wanted) * 1e4).round() / 1e4;
    }
    let shapes = [source.clone(), guide.clone()];
    let layout = crate::fit_path::fit_to_path_layout_with(&shapes, &set, &|_| None)?;
    if layout.placements.len() != objects.len() {
        return None;
    }
    for (p, o) in layout.placements.iter().zip(&objects) {
        let mut placed = source.clone();
        placed.transform_by(p.transform);
        let (a, b) = (page_points(&placed), page_points(o));
        if a.len() != b.len() || a.iter().zip(&b).any(|(x, y)| (*x - *y).hypot() > ADOPT_TOLERANCE) {
            return None;
        }
    }
    // The move the fit gave the first object (from the source to it).
    let placed = layout.placements.first()?.transform;
    let uid = |s: &Shape| s.uid.map(uid_text);
    Some(LiveFit {
        path: uid(guide)?,
        settings: set,
        members: vec![Member {
            uid: uid(first)?,
            placed: Some(placed.as_coeffs()),
        }],
        copies: rest.iter().map(uid).collect::<Option<Vec<_>>>()?,
    })
}

/// Turn the brush strokes the `.cdr` reader marked on their
/// guides (`tracedraw_cdr::artistic::DATA_KEY`) into live fits of their
/// objects to the guides, where the objects lie on the guide as a fit
/// would place them (#3680). The marks go either way; strokes that do not
/// fit stay plain objects. Returns how many were made live.
pub fn adopt_imported_strokes(doc: &mut tracedraw_core::Document) -> usize {
    #[derive(Deserialize)]
    struct Stroke {
        objects: Vec<String>,
    }
    const KEY: &str = tracedraw_cdr::artistic::DATA_KEY;
    fn walk(s: &Shape, out: &mut Vec<ShapeId>, uids: &mut HashMap<String, ShapeId>) {
        if s.data.iter().any(|(k, _)| k == KEY) {
            out.push(s.id);
        }
        if let Some(u) = s.uid {
            uids.entry(uid_text(u)).or_insert(s.id);
        }
        match &s.kind {
            ShapeKind::Group { children } => children.iter().for_each(|c| walk(c, out, uids)),
            ShapeKind::ClipFrame { frame, contents } => {
                walk(frame, out, uids);
                contents.iter().for_each(|c| walk(c, out, uids));
            }
            _ => {}
        }
    }
    let mut guides = Vec::new();
    let mut uids = HashMap::new();
    for l in doc.all_layers() {
        for s in &l.shapes {
            walk(s, &mut guides, &mut uids);
        }
    }
    let mut made = 0;
    for id in guides {
        let Some(guide) = doc.shape_on_page(id) else {
            continue;
        };
        let fit = guide
            .data
            .iter()
            .find(|(k, _)| k == KEY)
            .and_then(|(_, v)| serde_json::from_str::<Stroke>(v).ok())
            .and_then(|stroke| {
                let objects: Option<Vec<Shape>> = stroke
                    .objects
                    .iter()
                    .map(|u| uids.get(u).filter(|o| **o != id).and_then(|o| doc.shape_on_page(*o)))
                    .collect();
                imported_fit(&guide, &objects?)
            });
        let Some(shape) = doc.find_shape_mut(id) else {
            continue;
        };
        shape.data.retain(|(k, _)| k != KEY);
        if let Some(fit) = fit {
            shape.data = LiveFit::into_data(Some(&fit), &shape.data);
            made += 1;
        }
    }
    made
}

/// The handles of the selected live fit, page millimetres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LiveHandles {
    pub path: ShapeId,
    /// On the second object's point on the path: drags the spacing.
    pub spacing: Option<Point>,
    /// The path's middle, which the turn handle turns about.
    pub centre: Point,
}

/// What a handle drag changes.
#[derive(Debug, Clone, PartialEq)]
pub enum HandleKind {
    /// The spacing: `first` is how far along the path the first object
    /// is, `gap` what the spacing leaves between the first two points
    /// (half of each object with edges measured), `length` and `closed`
    /// the path's.
    Spacing {
        first: f64,
        gap: f64,
        length: f64,
        closed: bool,
    },
    /// The angle: the pointer's direction from `centre` at the press.
    Turn { centre: Point, from: f64 },
}

/// A handle being dragged.
#[derive(Debug, Clone, PartialEq)]
pub struct HandleDrag {
    pub path: ShapeId,
    pub kind: HandleKind,
    /// The settings at the press (made to space by distance from the
    /// first object for a spacing drag).
    pub base: FitPathSettings,
    /// What the tag beside the pointer says.
    pub shown: String,
}

impl App {
    /// Every object (inside groups and clip frames too) by its identity.
    fn shapes_by_uid(&self) -> HashMap<String, ShapeId> {
        fn walk(s: &Shape, out: &mut HashMap<String, ShapeId>) {
            if let Some(u) = s.uid {
                out.entry(uid_text(u)).or_insert(s.id);
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
        let mut out = HashMap::new();
        for l in self.doc().all_layers() {
            for s in &l.shapes {
                walk(s, &mut out);
            }
        }
        out
    }

    /// Every path holding a live fit, and its fit.
    pub fn live_fits(&self) -> Vec<(ShapeId, LiveFit)> {
        fn walk(s: &Shape, out: &mut Vec<(ShapeId, LiveFit)>) {
            if !s.data.is_empty() {
                if let Some(l) = LiveFit::of(s) {
                    out.push((s.id, l));
                }
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

    /// The live fit of the selection: the one whose path, object or copy
    /// is selected.
    pub fn live_fit_for_selection(&self) -> Option<(ShapeId, LiveFit)> {
        if self.selection.is_empty() {
            return None;
        }
        let chosen: Vec<String> = self
            .selection
            .iter()
            .filter_map(|id| self.doc().find_shape(*id)?.uid.map(uid_text))
            .collect();
        self.live_fits().into_iter().find(|(path, l)| {
            self.selection.contains(path)
                || l.members.iter().any(|m| chosen.contains(&m.uid))
                || l.copies.iter().any(|c| chosen.contains(c))
        })
    }

    /// The objects of a live fit as they were before it (in page space,
    /// each with its id) and, last, the path: what the fit is worked out
    /// from. `None` when the path or every object is gone.
    fn live_fit_sources(
        &self,
        path: ShapeId,
        fit: &LiveFit,
        index: &HashMap<String, ShapeId>,
    ) -> Option<Vec<Shape>> {
        let path_shape = self.doc().shape_on_page(path)?;
        let mut shapes = Vec::with_capacity(fit.members.len() + 1);
        for m in &fit.members {
            let Some(id) = index.get(&m.uid).copied().filter(|id| *id != path) else {
                continue;
            };
            let Some(mut s) = self.doc().shape_on_page(id) else {
                continue;
            };
            if let Some(p) = m.placed.as_ref().and_then(affine_of) {
                s.transform_by(p.inverse());
            }
            shapes.push(s);
        }
        if shapes.is_empty() {
            return None;
        }
        shapes.push(path_shape);
        Some(shapes)
    }

    fn live_fit_layout(
        &self,
        shapes: &[Shape],
        set: &FitPathSettings,
    ) -> Option<crate::fit_path::FitLayout> {
        let pivot = |s: &Shape| {
            self.pivots
                .single
                .get(&s.id)
                .map(|local| s.transform * *local)
        };
        crate::fit_path::fit_to_path_layout_with(shapes, set, &pivot)
    }

    /// Make the fit just applied live: `shapes` is what was fitted (the
    /// objects, then the path) and `plan` how; `placed` the ids of the
    /// placements, in the plan's order (copies with their new ids).
    pub(crate) fn link_live_fit(
        &mut self,
        shapes: &[Shape],
        plan: &[crate::fit_path::Placement],
        placed: &[ShapeId],
    ) {
        let Some((path, objects)) = shapes.split_last() else {
            return;
        };
        let uid = |app: &App, id: ShapeId| app.doc().find_shape(id).and_then(|s| s.uid);
        let Some(path_uid) = uid(self, path.id) else {
            return;
        };
        let mut members = Vec::with_capacity(objects.len());
        for o in objects {
            let Some(u) = uid(self, o.id) else {
                continue;
            };
            let placed = plan
                .iter()
                .find(|p| p.source == o.id && !p.copy)
                .map(|p| p.transform.as_coeffs());
            members.push(Member {
                uid: uid_text(u),
                placed,
            });
        }
        let copies = plan
            .iter()
            .zip(placed)
            .filter(|(p, _)| p.copy)
            .filter_map(|(_, id)| uid(self, *id).map(uid_text))
            .collect();
        let mut settings = self.fit_path;
        settings.live = true;
        let fit = LiveFit {
            path: uid_text(path_uid),
            settings,
            members,
            copies,
        };
        let data = self
            .doc()
            .find_shape(path.id)
            .map(|s| s.data.clone())
            .unwrap_or_default();
        let cmd = Command::SetObjectData {
            shape: path.id,
            data: LiveFit::into_data(Some(&fit), &data),
        };
        if let Err(e) = self.engine.amend(&cmd) {
            self.status = e.to_string();
        }
        self.live_fits_checked = self.engine.revision();
    }

    /// Change the settings of the live fit on `path`: the objects are
    /// fitted again at once. A run of changes with nothing in between
    /// (a handle dragged, a field's arrows held) is one undo step.
    pub fn set_live_fit_settings(&mut self, path: ShapeId, settings: FitPathSettings) {
        let key = (self.engine.revision(), vec![path], LABEL.to_string());
        if self.engine.undo_label() == Some(LABEL) && self.effect_edit_step.as_ref() == Some(&key) {
            let _ = self.engine.undo();
        }
        let Some(s) = self.doc().find_shape(path) else {
            return;
        };
        let Some(mut fit) = LiveFit::of(s) else {
            return;
        };
        fit.settings = settings;
        fit.settings.live = true;
        let data = LiveFit::into_data(Some(&fit), &s.data);
        if let Err(e) = self
            .engine
            .run_batch(LABEL, &[Command::SetObjectData { shape: path, data }])
        {
            self.status = e.to_string();
            return;
        }
        self.refresh_live_fits_now();
        self.effect_edit_step = Some((self.engine.revision(), vec![path], LABEL.to_string()));
    }

    /// Let the objects of the live fit on `path` go: they stay where they
    /// are, and no longer follow.
    pub fn release_live_fit(&mut self, path: ShapeId) {
        let Some(s) = self.doc().find_shape(path) else {
            return;
        };
        let data = LiveFit::into_data(None, &s.data);
        if let Err(e) = self.engine.run_batch(
            "Release From Path",
            &[Command::SetObjectData { shape: path, data }],
        ) {
            self.status = e.to_string();
        }
        self.effect_edit_step = None;
    }

    /// Live fits follow their paths and objects (see the module notes).
    /// Runs once per change of the drawing; the changes join the step
    /// that made it.
    pub fn refresh_live_fits(&mut self) {
        if self.engine.revision() == self.live_fits_checked {
            return;
        }
        self.refresh_live_fits_now();
    }

    fn refresh_live_fits_now(&mut self) {
        let fits = self.live_fits();
        if !fits.is_empty() {
            let index = self.shapes_by_uid();
            // An object is placed by one live fit only: the first path
            // that names it keeps it.
            let mut claimed: Vec<String> = Vec::new();
            for (path, mut fit) in fits {
                fit.members.retain(|m| {
                    let free = !claimed.contains(&m.uid);
                    if free {
                        claimed.push(m.uid.clone());
                    }
                    free
                });
                self.refit(path, fit, &index);
            }
        }
        self.live_fits_checked = self.engine.revision();
    }

    /// Fit the objects of the live fit on `path` again, when anything
    /// changed, amending the step that changed it.
    fn refit(&mut self, path: ShapeId, mut fit: LiveFit, index: &HashMap<String, ShapeId>) {
        let Some(path_shape) = self.doc().find_shape(path).cloned() else {
            return;
        };
        let set_data = |app: &mut App, fit: Option<&LiveFit>| {
            let data = LiveFit::into_data(fit, &path_shape.data);
            if data != path_shape.data {
                if let Err(e) = app
                    .engine
                    .amend(&Command::SetObjectData { shape: path, data })
                {
                    app.status = e.to_string();
                }
            }
        };
        // A copy of the path (Duplicate, Paste) does not take the
        // objects over: it lets the fit go.
        if path_shape.uid.map(uid_text).as_deref() != Some(fit.path.as_str()) {
            set_data(self, None);
            return;
        }
        let before = fit.clone();
        fit.members
            .retain(|m| index.get(&m.uid).is_some_and(|id| *id != path));
        let Some(shapes) = self.live_fit_sources(path, &fit, index) else {
            set_data(self, None);
            return;
        };
        let Some(layout) = self.live_fit_layout(&shapes, &fit.settings) else {
            if fit != before {
                set_data(self, Some(&fit));
            }
            return;
        };
        let plan = layout.placements;
        let mut cmds = Vec::new();
        // The objects onto their new places, or back where they were.
        for m in &mut fit.members {
            let Some(id) = index.get(&m.uid).copied() else {
                continue;
            };
            let now = m
                .placed
                .as_ref()
                .and_then(affine_of)
                .unwrap_or(Affine::IDENTITY);
            let next = plan
                .iter()
                .find(|p| p.source == id && !p.copy)
                .map(|p| p.transform);
            let to = next.unwrap_or(Affine::IDENTITY);
            let delta = to * now.inverse();
            if !near_identity(delta) {
                // In the space of whatever holds it.
                let parent = self.doc().parent_transform(id).unwrap_or(Affine::IDENTITY);
                let det = parent.determinant();
                let local = if det.is_finite() && det.abs() > 1e-12 {
                    parent.inverse() * delta * parent
                } else {
                    delta
                };
                cmds.push(Command::TransformShapes {
                    shapes: vec![id],
                    transform: local,
                });
                m.placed = next.map(|t| t.as_coeffs());
            } else if next.is_none() {
                m.placed = None;
            }
        }
        // The copies: made again when they are not what the fit gives.
        let source_of = |id: ShapeId| shapes.iter().find(|s| s.id == id);
        let wanted: Vec<(ShapeId, Shape)> = plan
            .iter()
            .filter(|p| p.copy)
            .filter_map(|p| {
                let mut s = source_of(p.source)?.clone();
                s.transform_by(p.transform);
                Some((p.source, s))
            })
            .collect();
        let old: Vec<ShapeId> = fit
            .copies
            .iter()
            .filter_map(|u| index.get(u).copied())
            .collect();
        let same = old.len() == wanted.len()
            && old.iter().zip(&wanted).all(|(id, (_, want))| {
                let Some(have) = self.doc().shape_on_page(*id) else {
                    return false;
                };
                let mut want = want.clone();
                want.id = have.id;
                want.uid = have.uid;
                if near_same(want.transform, have.transform) {
                    want.transform = have.transform;
                }
                want == have
            });
        let mut made: Vec<ShapeId> = Vec::new();
        if !same {
            for (k, (source, want)) in wanted.iter().enumerate() {
                let top = self
                    .doc()
                    .ancestors(*source)
                    .last()
                    .copied()
                    .unwrap_or(*source);
                let Some(layer) = self
                    .doc()
                    .locate(top)
                    .ok()
                    .map(|(l, _)| l)
                    .or_else(|| self.active_layer())
                else {
                    continue;
                };
                let id = self.engine.new_shape_id();
                let shape = crate::app::reid_pub(want.clone(), id, &mut self.engine);
                cmds.push(Command::AddShape { layer, shape });
                cmds.push(Command::PlaceBeside {
                    shapes: vec![id],
                    reference: old.get(k).copied().unwrap_or(*source),
                    in_front: true,
                });
                made.push(id);
            }
            if !old.is_empty() {
                cmds.push(Command::DeleteShapes {
                    shapes: old.clone(),
                });
            }
        }
        if !cmds.is_empty() {
            if let Err(e) = self.engine.amend_batch(LABEL, &cmds) {
                self.status = e.to_string();
                return;
            }
        }
        if !same {
            fit.copies = made
                .iter()
                .filter_map(|id| self.doc().find_shape(*id)?.uid.map(uid_text))
                .collect();
            // Copies that were selected stay selected as their new selves.
            let mut changed = false;
            let mut sel = self.selection.clone();
            for (k, o) in old.iter().enumerate() {
                if let Some(pos) = sel.iter().position(|s| s == o) {
                    match made.get(k) {
                        Some(n) => sel[pos] = *n,
                        None => {
                            sel.remove(pos);
                        }
                    }
                    changed = true;
                }
            }
            if changed {
                self.selection = sel;
            }
        }
        if fit != before {
            set_data(self, Some(&fit));
        }
    }

    /// The handles of the selected live fit, with the Pick tool.
    pub fn live_fit_handles(&self) -> Option<LiveHandles> {
        if self.tool != Tool::Pick || !matches!(self.drag, Drag::None | Drag::FitPathHandle(_)) {
            return None;
        }
        let key = (self.engine.revision(), self.selection.clone());
        if let Some((k, h)) = self.live_handle_cache.borrow().as_ref() {
            if *k == key {
                return *h;
            }
        }
        let h = self.live_fit_for_selection().and_then(|(path, fit)| {
            let index = self.shapes_by_uid();
            let shapes = self.live_fit_sources(path, &fit, &index)?;
            let page_path = shapes.last()?.page_path();
            let centre = kurbo::Shape::bounding_box(&page_path).center();
            let spacing = self
                .live_fit_layout(&shapes, &fit.settings)
                .and_then(|l| l.dist.get(1).copied())
                .and_then(|d| crate::fit_path::path_point(&page_path, &fit.settings, d))
                .map(|(p, _)| p);
            Some(LiveHandles {
                path,
                spacing,
                centre,
            })
        });
        *self.live_handle_cache.borrow_mut() = Some((key, h));
        h
    }

    /// Where the turn handle of `h` is on the page.
    pub fn live_turn_handle(&self, h: &LiveHandles) -> Point {
        let c = self.view.to_screen(h.centre);
        self.view.to_page(c + egui::vec2(0.0, TURN_HANDLE_PX))
    }

    /// A press at `at` on a handle of the selected live fit takes it.
    pub fn begin_live_fit_handle(&mut self, at: Point) -> bool {
        let Some(h) = self.live_fit_handles() else {
            return false;
        };
        let tol = HANDLE_PICK_PX / f64::from(self.view.zoom.max(1e-6));
        let Some(s) = self.doc().find_shape(h.path) else {
            return false;
        };
        let Some(fit) = LiveFit::of(s) else {
            return false;
        };
        let turn = self.live_turn_handle(&h);
        let (kind, base) = if h.spacing.is_some_and(|p| (p - at).hypot() <= tol) {
            let Some((base, first, gap, length, closed)) = self.spacing_base(h.path, &fit) else {
                return false;
            };
            (
                HandleKind::Spacing {
                    first,
                    gap,
                    length,
                    closed,
                },
                base,
            )
        } else if (turn - at).hypot() <= tol {
            let d = at - h.centre;
            (
                HandleKind::Turn {
                    centre: h.centre,
                    from: d.y.atan2(d.x),
                },
                fit.settings,
            )
        } else {
            return false;
        };
        self.effect_edit_step = None;
        self.drag = Drag::FitPathHandle(Box::new(HandleDrag {
            path: h.path,
            kind,
            base,
            shown: String::new(),
        }));
        true
    }

    /// The fit's settings spaced by a distance from the first object, at
    /// the offset and distance that leave every object where it is, with
    /// how far along the path the first object is, what the spacing
    /// leaves between the first two points, and the path's length and
    /// whether it is closed. `None` with fewer than two objects placed.
    fn spacing_base(
        &self,
        path: ShapeId,
        fit: &LiveFit,
    ) -> Option<(FitPathSettings, f64, f64, f64, bool)> {
        let index = self.shapes_by_uid();
        let shapes = self.live_fit_sources(path, fit, &index)?;
        let page_path = shapes.last()?.page_path();
        let cur = fit.settings;
        let now = self.live_fit_layout(&shapes, &cur)?;
        let (&d0, &d1) = (now.dist.first()?, now.dist.get(1)?);
        let mut base = cur;
        base.spacing = FitSpacing::Distance;
        base.align = FitAlign::Start;
        let probe_at = if cur.spacing == FitSpacing::Distance {
            cur.distance
        } else {
            (d1 - d0).abs().max(1.0)
        };
        base.distance = probe_at;
        let probe = self.live_fit_layout(&shapes, &base)?;
        let (&p0, &p1) = (probe.dist.first()?, probe.dist.get(1)?);
        base.offset += d0 - p0;
        let gap = (p1 - p0) - probe_at;
        base.distance = (d1 - d0 - gap).max(0.0);
        let (_, length, closed) =
            crate::fit_path::path_distance_of(&page_path, &base, page_path_start(&page_path))?;
        Some((base, d0, gap, length, closed))
    }

    /// The pointer moved to `p` while a live fit's handle is dragged;
    /// `ctrl` turns in steps of 15 degrees.
    pub fn live_fit_handle_drag_to(&mut self, p: Point, ctrl: bool) {
        let Drag::FitPathHandle(d) = &self.drag else {
            return;
        };
        let mut d = (**d).clone();
        let mut next = d.base;
        match &d.kind {
            HandleKind::Spacing {
                first,
                gap,
                length,
                closed,
            } => {
                let Some(path) = self.doc().shape_on_page(d.path) else {
                    return;
                };
                let Some((at, _, _)) =
                    crate::fit_path::path_distance_of(&path.page_path(), &d.base, p)
                else {
                    return;
                };
                let mut run = at - first;
                if *closed && *length > 0.0 {
                    run = run.rem_euclid(*length);
                }
                next.distance = (run - gap).max(0.0);
                let u = self.units;
                d.shown = crate::i18n::trf(
                    "status.fit_live_spacing",
                    &[(
                        "value",
                        &format!("{:.2} {}", u.world_from_mm(next.distance), u.label()),
                    )],
                );
            }
            HandleKind::Turn { centre, from } => {
                let v = p - *centre;
                if v.hypot() < 1e-9 {
                    return;
                }
                let mut a = d.base.angle + (v.y.atan2(v.x) - from).to_degrees();
                if ctrl {
                    a = (a / 15.0).round() * 15.0;
                }
                a = a.rem_euclid(360.0);
                if a > 180.0 {
                    a -= 360.0;
                }
                next.angle = a;
                d.shown =
                    crate::i18n::trf("status.fit_live_angle", &[("value", &format!("{a:.2}°"))]);
            }
        }
        let path = d.path;
        self.drag = Drag::FitPathHandle(Box::new(d));
        self.set_live_fit_settings(path, next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fit_path::{FitCopies, FitRotation};
    use tracedraw_core::geometry::{BezPath, Rect, Vec2};

    fn square(app: &mut App, x: f64, y: f64, side: f64) -> ShapeId {
        app.new_shape(ShapeKind::Rect {
            rect: Rect::new(x, y, x + side, y + side),
            radius: 0.0,
            corners: None,
        })
        .expect("square")
    }

    fn line(app: &mut App, a: (f64, f64), b: (f64, f64)) -> ShapeId {
        let mut p = BezPath::new();
        p.move_to(a);
        p.line_to(b);
        app.new_shape(ShapeKind::Path {
            path: p,
            closed: false,
        })
        .expect("line")
    }

    fn centre(app: &App, id: ShapeId) -> Point {
        app.doc()
            .shape_on_page(id)
            .expect("shape")
            .bounds()
            .center()
    }

    /// Three squares fitted live on a line from (0,0) to (100,0).
    fn fitted(app: &mut App) -> (Vec<ShapeId>, ShapeId) {
        let a = square(app, 10.0, 50.0, 10.0);
        let b = square(app, 30.0, 50.0, 6.0);
        let c = square(app, 50.0, 50.0, 8.0);
        let path = line(app, (0.0, 0.0), (100.0, 0.0));
        app.select(vec![a, b, c, path]);
        app.fit_path_follow_selection();
        app.fit_path.live = true;
        app.fit_path_edited = true;
        app.fit_objects_to_path();
        app.refresh_live_fits();
        (vec![a, b, c], path)
    }

    fn close(p: Point, x: f64, y: f64) -> bool {
        (p.x - x).abs() < 1e-6 && (p.y - y).abs() < 1e-6
    }

    #[test]
    fn a_live_fit_follows_its_path_when_it_moves() {
        let mut app = App::headless();
        let (objs, path) = fitted(&mut app);
        assert!(close(centre(&app, objs[0]), 0.0, 0.0));
        assert!(close(centre(&app, objs[1]), 50.0, 0.0));
        assert!(close(centre(&app, objs[2]), 100.0, 0.0));
        let fit = app.live_fits();
        assert_eq!(fit.len(), 1);
        assert_eq!(fit[0].0, path);
        assert!(fit[0].1.settings.live);
        // The path moves: the objects go with it, in the same step.
        app.run(Command::TransformShapes {
            shapes: vec![path],
            transform: Affine::translate(Vec2::new(5.0, 20.0)),
        });
        app.refresh_live_fits();
        assert!(close(centre(&app, objs[0]), 5.0, 20.0));
        assert!(close(centre(&app, objs[1]), 55.0, 20.0));
        assert!(close(centre(&app, objs[2]), 105.0, 20.0));
        // One undo takes the move and the refit back together.
        let _ = app.engine.undo();
        app.refresh_live_fits();
        assert!(close(centre(&app, objs[1]), 50.0, 0.0));
        // Nothing changed: refreshing again changes nothing.
        let rev = app.engine.revision();
        app.live_fits_checked = 0;
        app.refresh_live_fits();
        assert_eq!(app.engine.revision(), rev);
    }

    #[test]
    fn a_reshaped_path_and_a_resized_object_refit() {
        let mut app = App::headless();
        let (objs, path) = fitted(&mut app);
        // The path made twice as long: the objects spread over it.
        app.run(Command::TransformShapes {
            shapes: vec![path],
            transform: Affine::scale_non_uniform(2.0, 1.0),
        });
        app.refresh_live_fits();
        assert!(close(centre(&app, objs[1]), 100.0, 0.0));
        assert!(close(centre(&app, objs[2]), 200.0, 0.0));
        // An object moved off the path comes back onto it; one resized
        // keeps its new size.
        app.run(Command::TransformShapes {
            shapes: vec![objs[1]],
            transform: Affine::translate(Vec2::new(0.0, 40.0)),
        });
        app.refresh_live_fits();
        assert!(close(centre(&app, objs[1]), 100.0, 0.0));
        let c = centre(&app, objs[0]);
        app.run(Command::TransformShapes {
            shapes: vec![objs[0]],
            transform: Affine::translate(c.to_vec2())
                * Affine::scale(2.0)
                * Affine::translate(-c.to_vec2()),
        });
        app.refresh_live_fits();
        let s = app.doc().shape_on_page(objs[0]).expect("a");
        assert!((s.bounds().width() - 20.0).abs() < 1e-6);
        assert!(close(s.bounds().center(), 0.0, 0.0));
    }

    #[test]
    fn settings_changed_on_a_live_fit_refit_and_release_lets_go() {
        let mut app = App::headless();
        let (objs, path) = fitted(&mut app);
        app.select(vec![objs[1]]);
        let (p, fit) = app.live_fit_for_selection().expect("live fit");
        assert_eq!(p, path);
        let mut set = fit.settings;
        set.rotation = FitRotation::Fixed;
        set.angle = 45.0;
        app.set_live_fit_settings(path, set);
        app.set_live_fit_settings(path, {
            set.angle = 90.0;
            set
        });
        let s = app.doc().shape_on_page(objs[0]).expect("a");
        let c = s.transform.as_coeffs();
        assert!((c[1].atan2(c[0]).to_degrees() - 90.0).abs() < 1e-6);
        // The two changes in a row are one step.
        let _ = app.engine.undo();
        let s = app.doc().shape_on_page(objs[0]).expect("a");
        let c = s.transform.as_coeffs();
        assert!(c[1].abs() < 1e-9);
        // Released, the objects stay where they are when the path moves.
        app.release_live_fit(path);
        assert!(app.live_fits().is_empty());
        app.run(Command::TransformShapes {
            shapes: vec![path],
            transform: Affine::translate(Vec2::new(0.0, 30.0)),
        });
        app.refresh_live_fits();
        assert!(close(centre(&app, objs[1]), 50.0, 0.0));
    }

    #[test]
    fn copies_follow_the_path_and_their_original() {
        let mut app = App::headless();
        let a = square(&mut app, 10.0, 50.0, 10.0);
        let path = line(&mut app, (0.0, 0.0), (90.0, 0.0));
        app.select(vec![a, path]);
        app.fit_path_follow_selection();
        app.fit_path.live = true;
        app.fit_path.copies = FitCopies::Count;
        app.fit_path.count = 4;
        app.fit_path_edited = true;
        app.fit_objects_to_path();
        app.refresh_live_fits();
        let fit = app.live_fits()[0].1.clone();
        assert_eq!(fit.copies.len(), 3);
        let count = |app: &App| {
            app.doc()
                .all_layers()
                .map(|l| l.shapes.len())
                .sum::<usize>()
        };
        assert_eq!(count(&app), 5);
        // The path made shorter: the copies close up.
        app.run(Command::TransformShapes {
            shapes: vec![path],
            transform: Affine::scale_non_uniform(0.5, 1.0),
        });
        app.refresh_live_fits();
        assert_eq!(count(&app), 5);
        let xs: Vec<f64> = app
            .doc()
            .all_layers()
            .flat_map(|l| l.shapes.iter())
            .filter(|s| s.id != path)
            .map(|s| s.bounds().center().x)
            .collect();
        for want in [0.0, 15.0, 30.0, 45.0] {
            assert!(xs.iter().any(|x| (x - want).abs() < 1e-6), "{xs:?}");
        }
        // The original resized: its copies are resized too.
        let c = centre(&app, a);
        app.run(Command::TransformShapes {
            shapes: vec![a],
            transform: Affine::translate(c.to_vec2())
                * Affine::scale(0.5)
                * Affine::translate(-c.to_vec2()),
        });
        app.refresh_live_fits();
        assert_eq!(count(&app), 5);
        for s in app.doc().all_layers().flat_map(|l| l.shapes.iter()) {
            if s.id != path {
                assert!((s.bounds().width() - 5.0).abs() < 1e-6);
            }
        }
    }

    /// A drawing whose guide carries the `.cdr` reader's mark of an
    /// A brush stroke: triangles every 5 mm along a bent path,
    /// turned half round from it and lying a little off it, listed from
    /// the far end (as the file of #3680 lists them).
    fn stroke_drawing(nudge: f64) -> (tracedraw_core::Document, Uid) {
        use crate::fit_path::{FitCopies, FitPlace, FitRotation};
        let mut doc = tracedraw_core::Document::new("Stroke", tracedraw_core::Size::new(200.0, 200.0));
        let mut path = BezPath::new();
        path.move_to((10.0, 10.0));
        path.line_to((70.0, 10.0));
        path.curve_to((100.0, 10.0), (110.0, 40.0), (110.0, 70.0));
        let mut guide = Shape::new(doc.ids_mut().shape(), ShapeKind::Path { path, closed: false });
        guide.uid = Some(Uid(0x100));
        let mut tri = BezPath::new();
        tri.move_to((0.0, -1.0));
        tri.line_to((1.25, 1.2));
        tri.line_to((-1.25, 1.2));
        tri.close_path();
        let mut source = Shape::new(doc.ids_mut().shape(), ShapeKind::Path { path: tri, closed: true });
        source.transform = Affine::translate((40.0, 100.0)) * Affine::scale(0.6);
        let set = FitPathSettings {
            spacing: FitSpacing::Distance,
            distance: 5.0,
            copies: FitCopies::Fill,
            rotation: FitRotation::Follow,
            angle: 180.0,
            place: FitPlace::Above,
            side: -0.5,
            ..FitPathSettings::default()
        };
        let layout = crate::fit_path::fit_to_path_layout_with(&[source.clone(), guide.clone()], &set, &|_| None)
            .expect("layout");
        let mut marks = Vec::new();
        for (k, p) in layout.placements.iter().enumerate() {
            let mut m = source.clone();
            m.id = doc.ids_mut().shape();
            m.uid = Some(Uid(0x200 + k as u128));
            m.transform_by(p.transform);
            if k == 3 {
                m.transform_by(Affine::translate((nudge, 0.0)));
            }
            marks.push(uid_text(m.uid.expect("uid")));
            doc.pages[0].layers[0].shapes.push(m);
        }
        marks.reverse();
        let text = serde_json::json!({ "objects": marks }).to_string();
        guide.data.push((tracedraw_cdr::artistic::DATA_KEY.to_string(), text));
        doc.pages[0].layers[0].shapes.push(guide);
        (doc, Uid(0x100))
    }

    #[test]
    fn a_brush_stroke_read_from_a_cdr_file_opens_live() {
        let (mut doc, guide_uid) = stroke_drawing(0.0);
        assert_eq!(adopt_imported_strokes(&mut doc), 1);
        let guide = doc
            .all_layers()
            .flat_map(|l| l.shapes.iter())
            .find(|s| s.uid == Some(guide_uid))
            .expect("guide");
        assert!(guide.data.iter().all(|(k, _)| k != tracedraw_cdr::artistic::DATA_KEY));
        let fit = LiveFit::of(guide).expect("live fit");
        assert_eq!(fit.settings.distance, 5.0);
        assert_eq!(fit.settings.angle, 180.0);
        assert_eq!(fit.members.len(), 1);
        // From the path's start: the first triangle is fitted, the others
        // are its copies.
        assert_eq!(fit.members[0].uid, uid_text(Uid(0x200)));
        assert_eq!(fit.copies.first(), Some(&uid_text(Uid(0x201))));
        let n = 1 + fit.copies.len();
        // Opened: moving the guide takes the triangles along.
        let mut app = App::headless();
        app.open_document(doc, None);
        let path = app.live_fits().first().map(|(p, _)| *p).expect("fit");
        let first = |app: &App| {
            let index = app.shapes_by_uid();
            centre(app, index[&uid_text(Uid(0x200))])
        };
        let before = first(&app);
        app.engine
            .run(&Command::TransformShapes {
                shapes: vec![path],
                transform: Affine::translate((5.0, 7.0)),
            })
            .expect("move");
        app.refresh_live_fits();
        let after = first(&app);
        assert!(close(after, before.x + 5.0, before.y + 7.0), "{before:?} -> {after:?}");
        let fit = LiveFit::of(app.doc().find_shape(path).expect("guide")).expect("fit");
        assert_eq!(1 + fit.copies.len(), n);
    }

    #[test]
    fn a_stroke_whose_objects_do_not_follow_a_fit_stays_as_drawn() {
        let (mut doc, _) = stroke_drawing(1.0);
        assert_eq!(adopt_imported_strokes(&mut doc), 0);
        // The mark goes all the same.
        assert!(doc
            .all_layers()
            .flat_map(|l| l.shapes.iter())
            .all(|s| s.data.is_empty()));
    }

    #[test]
    fn a_copied_path_lets_the_fit_go() {
        let mut app = App::headless();
        let (_, path) = fitted(&mut app);
        let s = app.doc().find_shape(path).cloned().expect("path");
        let layer = app.active_layer().expect("layer");
        let id = app.engine.new_shape_id();
        let copy = crate::app::reid_pub(s, id, &mut app.engine);
        app.run(Command::AddShape { layer, shape: copy });
        app.refresh_live_fits();
        let fits = app.live_fits();
        assert_eq!(fits.len(), 1);
        assert_eq!(fits[0].0, path);
    }

    #[test]
    fn the_handles_drag_the_spacing_and_turn_all_objects() {
        let mut app = App::headless();
        app.tool = Tool::Pick;
        let (objs, path) = fitted(&mut app);
        app.select(vec![path]);
        let h = app.live_fit_handles().expect("handles");
        let at = h.spacing.expect("spacing handle");
        assert!(close(at, 50.0, 0.0));
        assert!(app.begin_live_fit_handle(at));
        app.live_fit_handle_drag_to(Point::new(30.0, 3.0), false);
        app.live_fit_handle_drag_to(Point::new(20.0, 3.0), false);
        app.end_drag();
        assert!(close(centre(&app, objs[0]), 0.0, 0.0));
        assert!(close(centre(&app, objs[1]), 20.0, 0.0));
        assert!(close(centre(&app, objs[2]), 40.0, 0.0));
        let fit = app.live_fits()[0].1.clone();
        assert_eq!(fit.settings.spacing, FitSpacing::Distance);
        assert!((fit.settings.distance - 20.0).abs() < 1e-6);
        // One step for the whole drag.
        let _ = app.engine.undo();
        app.refresh_live_fits();
        assert!(close(centre(&app, objs[1]), 50.0, 0.0));
        // The turn handle under the middle turns every object.
        let h = app.live_fit_handles().expect("handles");
        let turn = app.live_turn_handle(&h);
        assert!(app.begin_live_fit_handle(turn));
        let r = (turn - h.centre).hypot();
        app.live_fit_handle_drag_to(h.centre + Vec2::new(0.0, r), false);
        app.end_drag();
        let fit = app.live_fits()[0].1.clone();
        assert!(
            (fit.settings.angle.abs() - 180.0).abs() < 1e-6,
            "{}",
            fit.settings.angle
        );
        for id in objs {
            let c = app
                .doc()
                .shape_on_page(id)
                .expect("o")
                .transform
                .as_coeffs();
            assert!((c[0] + 1.0).abs() < 1e-6);
        }
    }
}
