//! Text fitted to a path that follows the path (#3571): Fit Text to Path
//! notes the path the text was fitted to in the text's object data, and
//! with Follow Path turned on in the fitted text's property bar the text
//! is fitted again whenever that path is moved, reshaped or edited,
//! keeping its distance, offset, placement and mirrors. Off, a fitted
//! text keeps its own copy of the path, as it always has. Straighten Text
//! ends it. See docs/behavior/text.md, "Following the path".

use crate::app::App;
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};
use tracedraw_core::document::{Shape, ShapeKind};
use tracedraw_core::geometry::{Affine, Point};
use tracedraw_core::{Command, ShapeId};

/// The object data key holding the link, on the text.
pub const DATA_KEY: &str = "text_path.follow";

/// The undo label of turning Follow Path on or off.
const LABEL: &str = "Follow Path";

/// The path a fitted text came from, as the text's object data keeps it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PathLink {
    /// The path's identity (`Shape::uid`, hexadecimal).
    pub path: String,
    /// Follow Path is on.
    #[serde(default)]
    pub follow: bool,
    /// The path's outline (page space) the text was last fitted to, as a
    /// hash: a change of it fits the text again.
    #[serde(default)]
    pub sig: u64,
    /// On a closed path, where the text starts as a share (0 to 1) of
    /// the outline's length from its first point: kept as the outline
    /// moves, grows or is reshaped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<f64>,
}

impl PathLink {
    pub fn of(s: &Shape) -> Option<PathLink> {
        let (_, v) = s.data.iter().find(|(k, _)| k == DATA_KEY)?;
        serde_json::from_str(v).ok()
    }

    /// `data` with this link in it (taken out with `None`).
    pub fn into_data(link: Option<&PathLink>, data: &[(String, String)]) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = data
            .iter()
            .filter(|(k, _)| k != DATA_KEY)
            .cloned()
            .collect();
        if let Some(text) = link.and_then(|l| serde_json::to_string(l).ok()) {
            out.push((DATA_KEY.to_string(), text));
        }
        out
    }
}

/// The hash of a path's outline in page space.
fn outline_sig(curve: &Shape) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    curve.page_path().to_svg().hash(&mut h);
    h.finish()
}

fn uid_text(u: tracedraw_core::document::Uid) -> String {
    format!("{:032x}", u.0)
}

/// The segments of a path's first outline when it is closed (closed
/// with a line where its ends are apart); `None` for an open one.
fn closed_segments(path: &kurbo::BezPath) -> Option<Vec<kurbo::PathSeg>> {
    use kurbo::ParamCurve;
    let mut first = kurbo::BezPath::new();
    let mut closed = false;
    for el in path.elements() {
        match el {
            kurbo::PathEl::MoveTo(_) if !first.elements().is_empty() => break,
            kurbo::PathEl::ClosePath => {
                closed = true;
                break;
            }
            e => first.push(*e),
        }
    }
    let mut segs: Vec<kurbo::PathSeg> = first.segments().collect();
    let (a, b) = (segs.first()?.start(), segs.last()?.end());
    if (a - b).hypot() < 1e-6 {
        closed |= segs.len() > 1;
    } else if closed {
        segs.push(kurbo::PathSeg::Line(kurbo::Line::new(b, a)));
    }
    closed.then_some(segs)
}

/// How far along a closed outline (a share of its length, from its
/// first point) the point of it nearest `p` lies.
fn start_share(outline: &kurbo::BezPath, p: Point) -> Option<f64> {
    use kurbo::{ParamCurve, ParamCurveArclen, ParamCurveNearest};
    let segs = closed_segments(outline)?;
    let lens: Vec<f64> = segs.iter().map(|s| s.arclen(1e-6)).collect();
    let total: f64 = lens.iter().sum();
    if total <= 1e-9 {
        return None;
    }
    let (i, t) = segs
        .iter()
        .enumerate()
        .map(|(i, s)| (i, s.nearest(p, 1e-6)))
        .min_by(|a, b| a.1.distance_sq.total_cmp(&b.1.distance_sq))
        .map(|(i, n)| (i, n.t))?;
    let before: f64 = lens[..i].iter().sum();
    let within = segs[i].subsegment(0.0..t).arclen(1e-6);
    Some(((before + within) / total).rem_euclid(1.0))
}

/// The point a share `f` of a closed outline's length from its first
/// point.
fn point_at_share(outline: &kurbo::BezPath, f: f64) -> Option<Point> {
    use kurbo::{ParamCurve, ParamCurveArclen};
    let segs = closed_segments(outline)?;
    let lens: Vec<f64> = segs.iter().map(|s| s.arclen(1e-6)).collect();
    let total: f64 = lens.iter().sum();
    if total <= 1e-9 || !f.is_finite() {
        return None;
    }
    let mut left = f.rem_euclid(1.0) * total;
    for (s, len) in segs.iter().zip(&lens) {
        if left <= *len {
            return Some(s.eval(s.inv_arclen(left, 1e-6)));
        }
        left -= len;
    }
    segs.last().map(|s| s.end())
}

/// Where a fitted text's path starts, in page space.
fn text_start(text: &Shape) -> Option<Point> {
    let ShapeKind::Text {
        origin,
        on_path: Some(tp),
        ..
    } = &text.kind
    else {
        return None;
    };
    match tp.path.elements().first() {
        Some(kurbo::PathEl::MoveTo(p)) => {
            Some(text.transform * Affine::translate(origin.to_vec2()) * *p)
        }
        _ => None,
    }
}

/// `text` (a fitted text) on `curve` again: the path's outline as it is
/// now, started where the text's path starts (on a closed path, at the
/// point of the new outline nearest it), everything else kept.
pub fn refit_kind(text: &Shape, curve: &Shape, share: Option<f64>) -> Option<ShapeKind> {
    let ShapeKind::Text {
        spans,
        origin,
        frame,
        align,
        para,
        on_path: Some(tp),
    } = &text.kind
    else {
        return None;
    };
    let to_page = text.transform * Affine::translate(origin.to_vec2());
    let det = to_page.determinant();
    if !det.is_finite() || det.abs() <= 1e-12 {
        return None;
    }
    let outline = curve.page_path();
    let start = match tp.path.elements().first() {
        Some(kurbo::PathEl::MoveTo(p)) => Some(to_page * *p),
        _ => None,
    };
    // On a closed path the start keeps its share of the outline.
    let start = share.and_then(|f| point_at_share(&outline, f)).or(start);
    let (page_path, _) = tracedraw_core::text_path::path_for_text_at(&outline, start);
    if page_path.elements().is_empty() {
        return None;
    }
    let mut next = tp.clone();
    next.path = to_page.inverse() * page_path;
    Some(ShapeKind::Text {
        spans: spans.clone(),
        origin: *origin,
        frame: *frame,
        align: *align,
        para: para.clone(),
        on_path: Some(next),
    })
}

impl App {
    /// The shapes of the drawing by identity, groups and frames opened.
    fn shape_ids_by_uid(&self) -> std::collections::HashMap<String, ShapeId> {
        fn walk(s: &Shape, out: &mut std::collections::HashMap<String, ShapeId>) {
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
        let mut out = std::collections::HashMap::new();
        for l in self.doc().all_layers() {
            for s in &l.shapes {
                walk(s, &mut out);
            }
        }
        out
    }

    /// Note on the text `text`, just fitted to `curve`, where it came
    /// from, in the step that fitted it (Follow Path off).
    pub(crate) fn note_text_path_source(&mut self, text: ShapeId, curve: ShapeId) {
        let doc = self.doc();
        let (Some(t), Some(c)) = (doc.find_shape(text), doc.shape_on_page(curve)) else {
            return;
        };
        let Some(u) = c.uid else {
            return;
        };
        let link = PathLink {
            path: uid_text(u),
            follow: false,
            sig: outline_sig(&c),
            start: text_start(t).and_then(|p| start_share(&c.page_path(), p)),
        };
        let data = PathLink::into_data(Some(&link), &t.data);
        if let Err(e) = self
            .engine
            .amend(&Command::SetObjectData { shape: text, data })
        {
            self.status = e.to_string();
        }
        self.text_follow_checked = self.engine.revision();
    }

    /// The selected fitted text's link to its path, when the path is
    /// still in the drawing.
    pub fn fitted_text_link(&self) -> Option<PathLink> {
        let ft = self.fitted_text()?;
        let link = PathLink::of(self.doc().find_shape(ft.id)?)?;
        self.shape_ids_by_uid().get(&link.path)?;
        Some(link)
    }

    /// Turn Follow Path on or off for the selected fitted text: one undo
    /// step. Turned on, the text is fitted to the path as it is now.
    pub fn set_text_follows_path(&mut self, on: bool) {
        let Some(ft) = self.fitted_text() else {
            return;
        };
        let Some(link) = self.fitted_text_link() else {
            return;
        };
        if link.follow == on {
            return;
        }
        let Some(text) = self.doc().find_shape(ft.id).cloned() else {
            return;
        };
        let Some(curve) = self
            .shape_ids_by_uid()
            .get(&link.path)
            .and_then(|id| self.doc().shape_on_page(*id))
        else {
            return;
        };
        let next = PathLink {
            follow: on,
            sig: outline_sig(&curve),
            start: text_start(&text).and_then(|p| start_share(&curve.page_path(), p)),
            ..link
        };
        let mut cmds = vec![Command::SetObjectData {
            shape: ft.id,
            data: PathLink::into_data(Some(&next), &text.data),
        }];
        if on {
            if let Some(kind) = refit_kind(&text, &curve, next.start).filter(|k| *k != text.kind) {
                cmds.push(Command::SetShapeKind { shape: ft.id, kind });
            }
        }
        if let Err(e) = self.engine.run_batch(LABEL, &cmds) {
            self.status = e.to_string();
        }
        self.text_follow_checked = self.engine.revision();
    }

    /// Fit again each text following a path whose outline changed since
    /// it was last fitted, in the step that changed it. Runs each frame;
    /// does nothing while the drawing is unchanged.
    pub fn refresh_text_follow(&mut self) {
        let revision = self.engine.revision();
        if revision == self.text_follow_checked {
            return;
        }
        self.text_follow_checked = revision;
        let mut texts: Vec<(Shape, PathLink)> = Vec::new();
        fn walk(s: &Shape, out: &mut Vec<(Shape, PathLink)>) {
            if let Some(l) = PathLink::of(s).filter(|l| l.follow) {
                if matches!(
                    s.kind,
                    ShapeKind::Text {
                        on_path: Some(_),
                        ..
                    }
                ) {
                    out.push((s.clone(), l));
                }
            }
            if let ShapeKind::Group { children } = &s.kind {
                children.iter().for_each(|c| walk(c, out));
            }
        }
        for l in self.doc().all_layers() {
            for s in &l.shapes {
                walk(s, &mut texts);
            }
        }
        if texts.is_empty() {
            return;
        }
        let uids = self.shape_ids_by_uid();
        let mut cmds = Vec::new();
        for (text, link) in texts {
            // A path gone (deleted, or undone) leaves the text as it is.
            let Some(curve) = uids
                .get(&link.path)
                .and_then(|id| self.doc().shape_on_page(*id))
            else {
                continue;
            };
            let sig = outline_sig(&curve);
            if sig == link.sig {
                continue;
            }
            // The text as it stands in page space: inside a group, its
            // group's transform comes first.
            let parent = self
                .doc()
                .parent_transform(text.id)
                .unwrap_or(Affine::IDENTITY);
            let mut placed = text.clone();
            placed.transform = parent * text.transform;
            if let Some(kind) = refit_kind(&placed, &curve, link.start).filter(|k| *k != text.kind)
            {
                cmds.push(Command::SetShapeKind {
                    shape: text.id,
                    kind,
                });
            }
            let next = PathLink { sig, ..link };
            cmds.push(Command::SetObjectData {
                shape: text.id,
                data: PathLink::into_data(Some(&next), &text.data),
            });
        }
        for cmd in &cmds {
            if let Err(e) = self.engine.amend(cmd) {
                self.status = e.to_string();
                break;
            }
        }
        self.text_follow_checked = self.engine.revision();
    }
}

/// Where a point of the text's path lies in page space (for tests).
#[cfg(test)]
fn page_start(app: &App, id: ShapeId) -> Option<Point> {
    text_start(app.doc().find_shape(id)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::ParamCurve;
    use tracedraw_core::document::TextSpan;
    use tracedraw_core::geometry::{BezPath, Rect};

    fn setup() -> (App, ShapeId, ShapeId) {
        let mut app = App::headless();
        let mut line = BezPath::new();
        line.move_to((10.0, 10.0));
        line.line_to((110.0, 10.0));
        let curve = app
            .new_shape(ShapeKind::Path {
                path: line,
                closed: false,
            })
            .expect("curve");
        let text = app
            .new_shape(ShapeKind::Text {
                spans: vec![TextSpan::new("Hello", "Arial", 12.0)],
                origin: Point::new(0.0, 50.0),
                frame: None,
                align: Default::default(),
                para: Default::default(),
                on_path: None,
            })
            .expect("text");
        app.select(vec![text, curve]);
        app.fit_text_to_path();
        (app, text, curve)
    }

    fn data_link(app: &App, id: ShapeId) -> Option<PathLink> {
        PathLink::of(app.doc().find_shape(id)?)
    }

    /// Fit Text to Path notes the path, in the same step; the text does
    /// not follow it until Follow Path is turned on (#3571).
    #[test]
    fn fitting_notes_the_path_and_following_is_off() {
        let (mut app, text, curve) = setup();
        let link = data_link(&app, text).expect("the path is noted");
        assert!(!link.follow);
        let steps = app.engine.history_labels().0.len();
        app.select(vec![curve]);
        app.nudge(tracedraw_core::geometry::Vec2::new(0.0, 20.0));
        app.refresh_text_follow();
        assert_eq!(page_start(&app, text), Some(Point::new(10.0, 10.0)));
        assert_eq!(app.engine.history_labels().0.len(), steps + 1);
        // One undo takes the fit away whole, the note with it.
        app.undo();
        app.undo();
        assert!(data_link(&app, text).is_none());
    }

    /// With Follow Path on, moving or reshaping the path fits the text
    /// again in that same step, keeping its distance and offset; undo
    /// brings both back.
    #[test]
    fn a_following_text_moves_and_reshapes_with_its_path() {
        let (mut app, text, curve) = setup();
        app.select(vec![text]);
        app.edit_fitted_text(|t, _| {
            t.offset = 5.0;
            t.set_distance(2.0);
        });
        assert!(app.fitted_text_link().is_some());
        app.set_text_follows_path(true);
        assert!(data_link(&app, text).expect("link").follow);
        app.select(vec![curve]);
        let steps = app.engine.history_labels().0.len();
        app.nudge(tracedraw_core::geometry::Vec2::new(0.0, 20.0));
        app.refresh_text_follow();
        assert_eq!(app.engine.history_labels().0.len(), steps + 1, "one step");
        let s = page_start(&app, text).expect("on a path");
        assert!(
            (s.x - 10.0).abs() < 1e-6 && (s.y - 30.0).abs() < 1e-6,
            "{s:?}"
        );
        let ShapeKind::Text {
            on_path: Some(tp), ..
        } = &app.doc().find_shape(text).expect("text").kind
        else {
            panic!("fitted")
        };
        assert_eq!(tp.offset, 5.0);
        assert!((tp.distance - 2.0).abs() < 1e-9);
        // Reshaped: a longer, slanted line.
        let mut longer = BezPath::new();
        longer.move_to((10.0, 30.0));
        longer.line_to((200.0, 80.0));
        app.engine
            .run(&Command::SetShapeKind {
                shape: curve,
                kind: ShapeKind::Path {
                    path: longer.clone(),
                    closed: false,
                },
            })
            .expect("reshape");
        app.refresh_text_follow();
        let ShapeKind::Text {
            on_path: Some(tp),
            origin,
            ..
        } = &app.doc().find_shape(text).expect("text").kind
        else {
            panic!("fitted")
        };
        let t = app.doc().find_shape(text).expect("text").transform;
        let end =
            t * Affine::translate(origin.to_vec2()) * tp.path.segments().last().expect("seg").end();
        // The new outline is in the path's own space: moved down 20 by the
        // nudge before.
        assert!((end - Point::new(200.0, 100.0)).hypot() < 1e-6, "{end:?}");
        // Undo puts back the path and the text with it.
        app.undo();
        app.refresh_text_follow();
        let s = page_start(&app, text).expect("on a path");
        assert!((s.y - 30.0).abs() < 1e-6);
        assert_eq!(
            app.engine.history_labels().0.len(),
            steps + 1,
            "undo is no new step"
        );
    }

    /// A following text on a closed path stays where it starts on the
    /// outline as the outline grows; a deleted path leaves it as it is.
    #[test]
    fn a_closed_path_keeps_the_start_and_a_deleted_one_lets_go() {
        let mut app = App::headless();
        let ring = app
            .new_shape(ShapeKind::Ellipse {
                rect: Rect::new(0.0, 0.0, 100.0, 100.0),
                arc: None,
            })
            .expect("ellipse");
        let text = app
            .new_shape(ShapeKind::Text {
                spans: vec![TextSpan::new("Round", "Arial", 12.0)],
                origin: Point::new(200.0, 50.0),
                frame: None,
                align: Default::default(),
                para: Default::default(),
                on_path: None,
            })
            .expect("text");
        app.select(vec![text, ring]);
        app.fit_text_to_path();
        app.select(vec![text]);
        app.set_text_follows_path(true);
        let before = page_start(&app, text).expect("start");
        app.select(vec![ring]);
        app.nudge(tracedraw_core::geometry::Vec2::new(30.0, 0.0));
        app.refresh_text_follow();
        let after = page_start(&app, text).expect("start");
        assert!(
            (after - (before + tracedraw_core::geometry::Vec2::new(30.0, 0.0))).hypot() < 1e-3,
            "{before:?} {after:?}"
        );
        app.delete_selection();
        app.refresh_text_follow();
        assert_eq!(page_start(&app, text), Some(after));
    }
}
