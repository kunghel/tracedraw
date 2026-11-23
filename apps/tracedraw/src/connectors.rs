//! Connectors that stay attached. A connector is an open path between an
//! anchor of one object and an anchor of another; what it joins is in its
//! object data (`connector`): the two objects, each anchor as fractions of
//! its object's bounds (as custom anchors are), how it runs (straight, at
//! right angles, at right angles with rounded corners and their radius)
//! and where its ends were last put.
//!
//! When the drawing changes, each connector looks at its objects: moved,
//! scaled or reshaped, the connector runs again between their anchors, in
//! the same undo step. A connector moved on its own (or pasted elsewhere)
//! no longer meets the ends it was given and is let go: it stays where it
//! is as a plain line. Moved together with its objects it stays attached.

use crate::app::App;
use serde::{Deserialize, Serialize};
use tracedraw_core::{
    document::{Shape, ShapeKind},
    geometry::{BezPath, PathEl, Point, Rect, Vec2},
    Command, Fill, ShapeId,
};

/// The object data key holding a connection.
pub const DATA_KEY: &str = "connector";

/// How a connector runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorKind {
    #[default]
    Straight,
    RightAngle,
    Rounded,
}

fn default_radius() -> f64 {
    3.0
}

/// What a connector joins.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Connection {
    pub from: ShapeId,
    /// Fractions of `from`'s bounds.
    pub from_anchor: (f64, f64),
    pub to: ShapeId,
    pub to_anchor: (f64, f64),
    #[serde(default)]
    pub kind: ConnectorKind,
    /// Corner radius of rounded connectors, mm.
    #[serde(default = "default_radius")]
    pub radius: f64,
    /// Where the ends were last put (page space).
    pub ends: (Point, Point),
    /// How far a right-angle connector runs straight out of each object
    /// before it turns, mm; 0 turns halfway as before (#1436).
    #[serde(default)]
    pub gap: f64,
}

impl Connection {
    /// The connection a shape holds.
    pub fn of(s: &Shape) -> Option<Connection> {
        if !matches!(s.kind, ShapeKind::Path { .. }) {
            return None;
        }
        let (_, v) = s.data.iter().find(|(k, _)| k == DATA_KEY)?;
        let c: Connection = serde_json::from_str(v).ok()?;
        let ok = [
            c.from_anchor.0,
            c.from_anchor.1,
            c.to_anchor.0,
            c.to_anchor.1,
            c.radius,
        ]
        .iter()
        .all(|v| v.is_finite())
            && [c.ends.0, c.ends.1]
                .iter()
                .all(|p| p.x.is_finite() && p.y.is_finite());
        ok.then_some(c)
    }

    /// The connector's path from `pa` to `pb`, leaving each object
    /// straight out of the side its anchor is on for `gap`.
    pub fn path(&self, pa: Point, pb: Point) -> BezPath {
        route_clear(
            self.kind,
            self.radius,
            self.gap,
            (pa, side_out(self.from_anchor)),
            (pb, side_out(self.to_anchor)),
        )
    }
}

/// The way out of an object from an anchor at fractions `f` of its
/// bounds: across the nearer side, nothing from the middle.
fn side_out(f: (f64, f64)) -> Vec2 {
    let (dx, dy) = (f.0 - 0.5, f.1 - 0.5);
    if dx.abs() < 1e-9 && dy.abs() < 1e-9 {
        Vec2::ZERO
    } else if dx.abs() >= dy.abs() {
        Vec2::new(dx.signum(), 0.0)
    } else {
        Vec2::new(0.0, dy.signum())
    }
}

/// [`route`], except that a right-angle connector first runs `gap` (mm)
/// straight out of each end (`a` and `b`: the end and its way out)
/// before it turns.
pub fn route_clear(
    kind: ConnectorKind,
    radius: f64,
    gap: f64,
    a: (Point, Vec2),
    b: (Point, Vec2),
) -> BezPath {
    let ((pa, da), (pb, db)) = (a, b);
    if kind == ConnectorKind::Straight
        || gap.is_nan()
        || gap <= 1e-9
        || (da == Vec2::ZERO && db == Vec2::ZERO)
    {
        return route(kind, radius, pa, pb);
    }
    let g = gap.min(1000.0);
    let (qa, qb) = (pa + da * g, pb + db * g);
    let mut pts = vec![pa];
    for el in route(ConnectorKind::RightAngle, 0.0, qa, qb).elements() {
        if let PathEl::MoveTo(p) | PathEl::LineTo(p) = el {
            pts.push(*p);
        }
    }
    pts.push(pb);
    // Drop repeated points and those in the middle of a straight run.
    let mut v: Vec<Point> = Vec::new();
    for p in pts {
        if v.last().is_some_and(|q| (*q - p).hypot() < 1e-9) {
            continue;
        }
        if v.len() >= 2 {
            let (o, m) = (v[v.len() - 2], v[v.len() - 1]);
            if ((m - o).cross(p - m)).abs() < 1e-9 && (m - o).dot(p - m) >= 0.0 {
                v.pop();
            }
        }
        v.push(p);
    }
    let mut path = BezPath::new();
    path.move_to(v[0]);
    let r = radius.max(0.0);
    for i in 1..v.len().saturating_sub(1) {
        let (from, c, to) = (v[i - 1], v[i], v[i + 1]);
        let (d1, d2) = (c - from, to - c);
        let (l1, l2) = (d1.hypot(), d2.hypot());
        if kind == ConnectorKind::Rounded && r > 1e-9 && l1 > 1e-9 && l2 > 1e-9 {
            let rr = r.min(l1 / 2.0).min(l2 / 2.0);
            path.line_to(c - d1 / l1 * rr);
            path.quad_to(c, c + d2 / l2 * rr);
        } else {
            path.line_to(c);
        }
    }
    if let Some(last) = v.last().filter(|_| v.len() > 1) {
        path.line_to(*last);
    }
    path
}

/// Object data with the connection `c` (none: without one).
pub fn with_connection(
    mut data: Vec<(String, String)>,
    c: Option<&Connection>,
) -> Vec<(String, String)> {
    data.retain(|(k, _)| k != DATA_KEY);
    if let Some(v) = c.and_then(|c| serde_json::to_string(c).ok()) {
        data.push((DATA_KEY.into(), v));
    }
    data
}

/// The point at fractions `f` of `b`.
fn at(b: Rect, f: (f64, f64)) -> Point {
    Point::new(b.x0 + f.0 * b.width(), b.y0 + f.1 * b.height())
}

/// The fractions of `b` at `p` (the middle across a side of no size).
fn fractions(b: Rect, p: Point) -> (f64, f64) {
    let f = |v: f64, lo: f64, size: f64| {
        if size.abs() < 1e-9 {
            0.5
        } else {
            ((v - lo) / size).clamp(0.0, 1.0)
        }
    };
    (f(p.x, b.x0, b.width()), f(p.y, b.y0, b.height()))
}

/// The connector's path from `pa` to `pb`: straight, or at right angles
/// with an elbow halfway along the longer span (rounded: corners of up to
/// `radius`, never more than a quarter of the distance).
pub fn route(kind: ConnectorKind, radius: f64, pa: Point, pb: Point) -> BezPath {
    let mut path = BezPath::new();
    path.move_to(pa);
    if kind == ConnectorKind::Straight {
        path.line_to(pb);
        return path;
    }
    let horizontal = (pb.x - pa.x).abs() > (pb.y - pa.y).abs();
    let c1 = if horizontal {
        Point::new((pa.x + pb.x) / 2.0, pa.y)
    } else {
        Point::new(pa.x, (pa.y + pb.y) / 2.0)
    };
    let c2 = if horizontal {
        Point::new(c1.x, pb.y)
    } else {
        Point::new(pb.x, c1.y)
    };
    let r = radius.max(0.0).min((pb - pa).hypot() / 4.0);
    let corner = |path: &mut BezPath, from: Point, c: Point, to: Point| {
        let (d1, d2) = (c - from, to - c);
        let (l1, l2) = (d1.hypot(), d2.hypot());
        if kind == ConnectorKind::Rounded && r > 1e-9 && l1 > 1e-9 && l2 > 1e-9 {
            let rr = r.min(l1 / 2.0).min(l2 / 2.0);
            path.line_to(c - d1 / l1 * rr);
            path.quad_to(c, c + d2 / l2 * rr);
        } else {
            path.line_to(c);
        }
    };
    corner(&mut path, pa, c1, c2);
    corner(&mut path, c1, c2, pb);
    path.line_to(pb);
    path
}

/// The first and last points of a path.
fn ends_of(path: &BezPath) -> Option<(Point, Point)> {
    let first = match path.elements().first()? {
        PathEl::MoveTo(p) => *p,
        _ => return None,
    };
    let last = path.elements().iter().rev().find_map(|e| match e {
        PathEl::LineTo(p) | PathEl::QuadTo(_, p) | PathEl::CurveTo(_, _, p) => Some(*p),
        _ => None,
    })?;
    Some((first, last))
}

fn near(a: Point, b: Point) -> bool {
    (a - b).hypot() < 1e-4
}

impl App {
    /// The connectors copies bring along (#476): each connector of the
    /// page that joins a copied object (`originals[i]`, copied as
    /// `copies[i]`) to one not copied, and is not copied itself, gives a
    /// connector of the same kind, outline and anchors from that object to
    /// the copy. Their ids come from the engine; add them with the copies.
    pub fn connector_copies(&mut self, originals: &[ShapeId], copies: &[Shape]) -> Vec<Shape> {
        let copy_of = |id: ShapeId| {
            originals
                .iter()
                .position(|o| *o == id)
                .and_then(|i| copies.get(i))
        };
        let connectors: Vec<(Shape, Connection)> = self
            .page_layers()
            .iter()
            .flat_map(|l| &l.shapes)
            .filter(|s| !originals.contains(&s.id))
            .filter_map(|s| Connection::of(s).map(|c| (s.clone(), c)))
            .collect();
        let mut out = Vec::new();
        for (s, c) in connectors {
            // Exactly one end copied: the other end stays where it is.
            let (from, to, fb, tb) = match (copy_of(c.from), copy_of(c.to)) {
                (Some(f), None) => match self.anchor_box(c.to) {
                    Some(tb) => (f.id, c.to, f.bounds(), tb),
                    None => continue,
                },
                (None, Some(t)) => match self.anchor_box(c.from) {
                    Some(fb) => (c.from, t.id, fb, t.bounds()),
                    None => continue,
                },
                _ => continue,
            };
            let (pa, pb) = (at(fb, c.from_anchor), at(tb, c.to_anchor));
            let joined = Connection {
                from,
                to,
                ends: (pa, pb),
                ..c.clone()
            };
            let id = self.engine.new_shape_id();
            let mut copy = crate::app::reid_pub(s.clone(), id, &mut self.engine);
            copy.transform = tracedraw_core::geometry::Affine::IDENTITY;
            copy.kind = ShapeKind::Path {
                path: joined.path(pa, pb),
                closed: false,
            };
            copy.data = with_connection(s.data.clone(), Some(&joined));
            out.push(copy);
        }
        out
    }

    /// An object's bounds on the page, for its anchors.
    fn anchor_box(&self, id: ShapeId) -> Option<Rect> {
        self.doc().shape_on_page(id).map(|s| s.bounds())
    }

    /// A connector from `from` to `to`, from the drag that began at `start`
    /// and ended at `end` (the anchors are chosen as the Connector tools
    /// choose them); one undo step. The new connector is selected.
    pub fn connect(
        &mut self,
        from: ShapeId,
        start: Point,
        to: ShapeId,
        end: Point,
        kind: ConnectorKind,
    ) -> Option<ShapeId> {
        let (pa, pb) = self.connector_anchors(from, start, to, end)?;
        let (ba, bb) = (self.anchor_box(from)?, self.anchor_box(to)?);
        let c = Connection {
            from,
            from_anchor: fractions(ba, pa),
            to,
            to_anchor: fractions(bb, pb),
            kind,
            radius: self.connector_radius,
            ends: (pa, pb),
            gap: self.settings.connector_gap,
        };
        let layer = self.active_layer()?;
        let id = self.engine.new_shape_id();
        let mut s = Shape::new(
            id,
            ShapeKind::Path {
                path: c.path(pa, pb),
                closed: false,
            },
        );
        self.doc().defaults.apply_to(&mut s);
        s.fill = Fill::None;
        s.name = Some("Connector".into());
        s.data = with_connection(Vec::new(), Some(&c));
        if let Err(e) = self
            .engine
            .run_batch("Connector", &[Command::AddShape { layer, shape: s }])
        {
            self.status = e.to_string();
            return None;
        }
        self.select(vec![id]);
        Some(id)
    }

    /// The selected connectors.
    pub fn selected_connectors(&self) -> Vec<(ShapeId, Connection)> {
        self.selection
            .iter()
            .filter_map(|id| {
                let s = self.doc().find_shape(*id)?;
                Connection::of(s).map(|c| (*id, c))
            })
            .collect()
    }

    /// Change the selected connectors (how they run, their corner radius),
    /// one undo step; the bar's values for the next ones change too.
    pub fn set_connectors(&mut self, kind: Option<ConnectorKind>, radius: Option<f64>) {
        if let Some(r) = radius.filter(|r| r.is_finite()) {
            self.connector_radius = r.clamp(0.0, 1000.0);
        }
        let mut cmds = Vec::new();
        for (id, mut c) in self.selected_connectors() {
            if let Some(k) = kind {
                c.kind = k;
            }
            if radius.is_some() {
                c.radius = self.connector_radius;
            }
            let Some(s) = self.doc().shape_on_page(id) else {
                continue;
            };
            cmds.push(Command::SetShapeKind {
                shape: id,
                kind: ShapeKind::Path {
                    path: s.transform.inverse() * c.path(c.ends.0, c.ends.1),
                    closed: false,
                },
            });
            cmds.push(Command::SetObjectData {
                shape: id,
                data: with_connection(s.data.clone(), Some(&c)),
            });
        }
        if cmds.is_empty() {
            return;
        }
        // A radius dragged or typed on: one step while nothing else
        // changes in between.
        let label = if kind.is_none() {
            "Connector Radius"
        } else {
            "Connector"
        };
        let ids: Vec<ShapeId> = self.selection.clone();
        if label == "Connector Radius"
            && self.engine.undo_label() == Some(label)
            && self.connector_radius_step.as_ref() == Some(&(self.engine.revision(), ids.clone()))
        {
            let _ = self.engine.undo();
        }
        if let Err(e) = self.engine.run_batch(label, &cmds) {
            self.status = e.to_string();
            return;
        }
        self.connector_radius_step =
            (label == "Connector Radius").then(|| (self.engine.revision(), ids));
    }

    /// The connection of the connector `id` once its moved end (the `to`
    /// end when `keep_from`, else the `from` end), now at `ends`, is
    /// joined to the anchor of an object it was dropped on, and its new
    /// path; None when that end is on no anchor (or on the object the
    /// other end holds).
    fn reanchor(
        &self,
        id: ShapeId,
        c: &Connection,
        ends: (Point, Point),
        keep_from: bool,
    ) -> Option<(Connection, BezPath)> {
        let moved = if keep_from { ends.1 } else { ends.0 };
        let other = if keep_from { c.from } else { c.to };
        let tol = self.anchor_tolerance().max(0.5);
        let target = self.doc().all_layers().flat_map(|l| l.shapes.iter()).find_map(|s| {
            if s.id == id || s.id == other || Connection::of(s).is_some() {
                return None;
            }
            let p = self
                .anchors_of(s.id)
                .into_iter()
                .find(|p| (*p - moved).hypot() <= tol)?;
            Some((s.id, p))
        })?;
        let (obj, p) = target;
        let b = self.anchor_box(obj)?;
        let mut c2 = c.clone();
        if keep_from {
            c2.to = obj;
            c2.to_anchor = fractions(b, p);
            c2.ends = (ends.0, p);
        } else {
            c2.from = obj;
            c2.from_anchor = fractions(b, p);
            c2.ends = (p, ends.1);
        }
        let path = c2.path(c2.ends.0, c2.ends.1);
        Some((c2, path))
    }

    /// Connectors follow their objects (see the module notes). Runs once
    /// per change of the drawing; the changes join the step that made it.
    pub fn refresh_connectors(&mut self) {
        let revision = self.engine.revision();
        if revision == self.connectors_checked {
            return;
        }
        fn walk(s: &Shape, out: &mut Vec<ShapeId>) {
            if Connection::of(s).is_some() {
                out.push(s.id);
            }
            if let ShapeKind::Group { children } = &s.kind {
                children.iter().for_each(|c| walk(c, out));
            }
        }
        let mut ids = Vec::new();
        for layer in self.doc().all_layers() {
            for s in &layer.shapes {
                walk(s, &mut ids);
            }
        }
        for id in ids {
            let Some(s) = self.doc().shape_on_page(id) else {
                continue;
            };
            let (Some(c), ShapeKind::Path { path, .. }) = (Connection::of(&s), &s.kind) else {
                continue;
            };
            let Some((qa, qb)) = ends_of(&(s.transform * path.clone())) else {
                continue;
            };
            let anchors = self
                .anchor_box(c.from)
                .zip(self.anchor_box(c.to))
                .map(|(ba, bb)| (at(ba, c.from_anchor), at(bb, c.to_anchor)));
            let mut cmds = Vec::new();
            match anchors {
                // In place: nothing to do but remember where.
                Some((pa, pb)) if near(qa, pa) && near(qb, pb) => {
                    if !(near(c.ends.0, pa) && near(c.ends.1, pb)) {
                        let c2 = Connection {
                            ends: (pa, pb),
                            ..c.clone()
                        };
                        cmds.push(Command::SetObjectData {
                            shape: id,
                            data: with_connection(s.data.clone(), Some(&c2)),
                        });
                    }
                }
                // Its objects moved, the connector did not: it follows.
                Some((pa, pb)) if near(qa, c.ends.0) && near(qb, c.ends.1) => {
                    // The path in the connector's own space.
                    let local = s.transform.inverse();
                    cmds.push(Command::SetShapeKind {
                        shape: id,
                        kind: ShapeKind::Path {
                            path: local * c.path(pa, pb),
                            closed: false,
                        },
                    });
                    let c2 = Connection {
                        ends: (pa, pb),
                        ..c.clone()
                    };
                    cmds.push(Command::SetObjectData {
                        shape: id,
                        data: with_connection(s.data.clone(), Some(&c2)),
                    });
                }
                // One end dragged (with the Shape tool) onto an anchor of
                // an object: joined there instead, the other end kept
                // (#3860: changing an anchor let the connector go).
                Some((pa, pb))
                    if (near(qa, pa) || near(qb, pb))
                        && self.reanchor(id, &c, (qa, qb), near(qa, pa)).is_some() =>
                {
                    let (c2, path) = self.reanchor(id, &c, (qa, qb), near(qa, pa)).expect("checked");
                    cmds.push(Command::SetShapeKind {
                        shape: id,
                        kind: ShapeKind::Path {
                            path: s.transform.inverse() * path,
                            closed: false,
                        },
                    });
                    cmds.push(Command::SetObjectData {
                        shape: id,
                        data: with_connection(s.data.clone(), Some(&c2)),
                    });
                }
                // Moved on its own, or an object is gone: let go.
                Some(_) => cmds.push(Command::SetObjectData {
                    shape: id,
                    data: with_connection(s.data.clone(), None),
                }),
                None => {}
            }
            for cmd in cmds {
                if let Err(e) = self.engine.amend(&cmd) {
                    self.status = e.to_string();
                    break;
                }
            }
        }
        self.connectors_checked = self.engine.revision();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::{Affine, Vec2};

    fn rect(app: &mut App, x0: f64, y0: f64) -> ShapeId {
        app.new_shape(ShapeKind::Rect {
            rect: Rect::new(x0, y0, x0 + 20.0, y0 + 20.0),
            radius: 0.0,
            corners: None,
        })
        .expect("shape")
    }

    fn ends(app: &App, id: ShapeId) -> (Point, Point) {
        let s = app.doc().shape_on_page(id).expect("connector");
        match &s.kind {
            ShapeKind::Path { path, .. } => ends_of(&(s.transform * path.clone())).expect("ends"),
            other => panic!("{other:?}"),
        }
    }

    /// A copy of a connected object comes with a copy of its connector,
    /// joining the object at the other end to the copy, of the same kind
    /// and in the same undo step; copied with its connector, or pasted
    /// from elsewhere, nothing more comes (#476).
    #[test]
    fn copies_of_connected_objects_bring_their_connector() {
        let mut app = App::headless();
        app.view.zoom = 1.0;
        let a = rect(&mut app, 0.0, 0.0);
        let b = rect(&mut app, 60.0, 0.0);
        let c = app
            .connect(
                a,
                Point::new(20.0, 10.0),
                b,
                Point::new(60.0, 10.0),
                ConnectorKind::RightAngle,
            )
            .expect("connector");
        app.refresh_connectors();
        let connectors = |app: &App| -> Vec<(ShapeId, Connection)> {
            app.page_layers()
                .iter()
                .flat_map(|l| &l.shapes)
                .filter_map(|s| Connection::of(s).map(|k| (s.id, k)))
                .collect()
        };
        let steps = app.engine.history_labels().0.len();
        app.duplicate_offset = tracedraw_core::geometry::Vec2::new(0.0, 40.0);
        app.select(vec![b]);
        app.duplicate();
        let copy = app.selection[0];
        assert_eq!(app.engine.history_labels().0.len(), steps + 1, "one step");
        let all = connectors(&app);
        assert_eq!(all.len(), 2);
        let (nid, n) = all
            .iter()
            .find(|(id, _)| *id != c)
            .cloned()
            .expect("a new connector");
        assert_eq!((n.from, n.to, n.kind), (a, copy, ConnectorKind::RightAngle));
        app.refresh_connectors();
        assert_eq!(
            ends(&app, nid),
            (Point::new(20.0, 10.0), Point::new(60.0, 50.0)),
            "from a to the copy's left side"
        );
        // The object and its connector copied together: no third one.
        app.select(vec![b, c]);
        app.duplicate();
        assert_eq!(connectors(&app).len(), 3);
    }

    /// #3860: an end of a connector dragged onto another anchor (of the
    /// same object or another one) joins it there and stays attached;
    /// dropped away from any anchor it is let go, as before.
    #[test]
    fn a_connector_end_dragged_onto_an_anchor_joins_it_there() {
        let mut app = App::headless();
        app.view.zoom = 1.0;
        let a = rect(&mut app, 0.0, 0.0);
        let b = rect(&mut app, 60.0, 0.0);
        let d = rect(&mut app, 60.0, 60.0);
        let c = app
            .connect(a, Point::new(20.0, 10.0), b, Point::new(60.0, 10.0), ConnectorKind::Straight)
            .expect("connector");
        app.refresh_connectors();
        let drag_end = |app: &mut App, to: Point| {
            let (qa, _) = ends(app, c);
            let mut path = BezPath::new();
            path.move_to(qa);
            path.line_to(to);
            app.run(Command::SetShapeKind {
                shape: c,
                kind: ShapeKind::Path { path, closed: false },
            });
            app.refresh_connectors();
        };
        // b's top anchor.
        let top = app.anchors_of(b).into_iter().find(|p| (p.y - 0.0).abs() < 1e-6).expect("top");
        drag_end(&mut app, top);
        let k = Connection::of(&app.doc().shape_on_page(c).expect("c")).expect("still joined");
        assert_eq!((k.from, k.to), (a, b));
        assert!((k.to_anchor.1 - 0.0).abs() < 1e-6, "{:?}", k.to_anchor);
        // It follows b from that anchor.
        app.run(Command::TransformShapes {
            shapes: vec![b],
            transform: tracedraw_core::geometry::Affine::translate((0.0, 5.0)),
        });
        app.refresh_connectors();
        assert!((ends(&app, c).1 - Point::new(top.x, top.y + 5.0)).hypot() < 1e-6);
        // Onto an anchor of another object: joined to it.
        let left = app
            .anchors_of(d)
            .into_iter()
            .min_by(|p, q| p.x.total_cmp(&q.x))
            .expect("left");
        drag_end(&mut app, left);
        let k = Connection::of(&app.doc().shape_on_page(c).expect("c")).expect("joined to d");
        assert_eq!((k.from, k.to), (a, d));
        // Away from every anchor: let go.
        drag_end(&mut app, Point::new(200.0, 200.0));
        assert!(Connection::of(&app.doc().shape_on_page(c).expect("c")).is_none());
    }

    /// A connector follows the objects it joins; moved with them it stays
    /// attached; moved on its own it is let go.
    #[test]
    fn connectors_follow_their_objects() {
        let mut app = App::headless();
        app.view.zoom = 1.0;
        let a = rect(&mut app, 0.0, 0.0);
        let b = rect(&mut app, 60.0, 0.0);
        // From a's right side to b's left side.
        let c = app
            .connect(
                a,
                Point::new(20.0, 10.0),
                b,
                Point::new(60.0, 10.0),
                ConnectorKind::Straight,
            )
            .expect("connector");
        app.refresh_connectors();
        assert_eq!(
            ends(&app, c),
            (Point::new(20.0, 10.0), Point::new(60.0, 10.0))
        );
        // b moves up: the connector follows in the same undo step.
        app.run(Command::TransformShapes {
            shapes: vec![b],
            transform: Affine::translate(Vec2::new(0.0, 30.0)),
        });
        let depth = app.engine.history_labels().0.len();
        app.refresh_connectors();
        assert_eq!(app.engine.history_labels().0.len(), depth);
        assert_eq!(
            ends(&app, c),
            (Point::new(20.0, 10.0), Point::new(60.0, 40.0))
        );
        // All three moved together: still attached.
        app.run(Command::TransformShapes {
            shapes: vec![a, b, c],
            transform: Affine::translate(Vec2::new(5.0, 0.0)),
        });
        app.refresh_connectors();
        let s = app.doc().find_shape(c).cloned().expect("connector");
        assert!(Connection::of(&s).is_some());
        app.run(Command::TransformShapes {
            shapes: vec![b],
            transform: Affine::translate(Vec2::new(10.0, 0.0)),
        });
        app.refresh_connectors();
        assert_eq!(ends(&app, c).1, Point::new(75.0, 40.0));
        // The connector alone moved: let go, it stays where it was put.
        app.run(Command::TransformShapes {
            shapes: vec![c],
            transform: Affine::translate(Vec2::new(0.0, -50.0)),
        });
        app.refresh_connectors();
        let s = app.doc().find_shape(c).cloned().expect("connector");
        assert!(Connection::of(&s).is_none());
        assert_eq!(ends(&app, c).0, Point::new(25.0, -40.0));
        // Undo brings the attachment back.
        app.undo();
        app.refresh_connectors();
        let s = app.doc().find_shape(c).cloned().expect("connector");
        assert!(Connection::of(&s).is_some());
    }

    #[test]
    fn right_angle_and_rounded_routes() {
        let (pa, pb) = (Point::new(0.0, 0.0), Point::new(40.0, 20.0));
        let straight = route(ConnectorKind::Straight, 3.0, pa, pb);
        assert_eq!(straight.elements().len(), 2);
        let elbow = route(ConnectorKind::RightAngle, 3.0, pa, pb);
        let pts: Vec<Point> = elbow
            .elements()
            .iter()
            .filter_map(|e| match e {
                PathEl::MoveTo(p) | PathEl::LineTo(p) => Some(*p),
                _ => None,
            })
            .collect();
        assert_eq!(
            pts,
            vec![pa, Point::new(20.0, 0.0), Point::new(20.0, 20.0), pb]
        );
        let rounded = route(ConnectorKind::Rounded, 3.0, pa, pb);
        assert_eq!(
            rounded
                .elements()
                .iter()
                .filter(|e| matches!(e, PathEl::QuadTo(..)))
                .count(),
            2
        );
        assert_eq!(ends_of(&rounded), Some((pa, pb)));
    }

    /// The bar changes how a selected connector runs and its radius.
    #[test]
    fn selected_connectors_change_kind() {
        let mut app = App::headless();
        let a = rect(&mut app, 0.0, 0.0);
        let b = rect(&mut app, 60.0, 40.0);
        let c = app
            .connect(
                a,
                Point::new(20.0, 10.0),
                b,
                Point::new(60.0, 50.0),
                ConnectorKind::Straight,
            )
            .expect("connector");
        app.select(vec![c]);
        app.set_connectors(Some(ConnectorKind::Rounded), Some(5.0));
        let s = app.doc().find_shape(c).cloned().expect("connector");
        let conn = Connection::of(&s).expect("connection");
        assert_eq!((conn.kind, conn.radius), (ConnectorKind::Rounded, 5.0));
        match &s.kind {
            ShapeKind::Path { path, .. } => {
                assert!(path
                    .elements()
                    .iter()
                    .any(|e| matches!(e, PathEl::QuadTo(..))))
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(app.connector_radius, 5.0);
    }

    /// The points a connector's path runs through.
    fn points(app: &App, id: ShapeId) -> Vec<Point> {
        let s = app.doc().shape_on_page(id).expect("connector");
        let ShapeKind::Path { path, .. } = &s.kind else {
            panic!("{:?}", s.kind);
        };
        (s.transform * path.clone())
            .elements()
            .iter()
            .filter_map(|e| match e {
                PathEl::MoveTo(p) | PathEl::LineTo(p) => Some(*p),
                _ => None,
            })
            .collect()
    }

    /// Options > Tools > Connector, route distance: a right-angle
    /// connector runs that far straight out of each object before it
    /// turns, and keeps doing so when they move (#1436).
    #[test]
    fn right_angle_connectors_keep_their_distance_from_objects() {
        let mut app = App::headless();
        app.view.zoom = 1.0;
        app.settings.connector_gap = 5.0;
        let a = rect(&mut app, 0.0, 0.0);
        let b = rect(&mut app, 60.0, 30.0);
        let c = app
            .connect(
                a,
                Point::new(20.0, 10.0),
                b,
                Point::new(60.0, 40.0),
                ConnectorKind::RightAngle,
            )
            .expect("connector");
        let pts = points(&app, c);
        assert_eq!(pts.first(), Some(&Point::new(20.0, 10.0)));
        assert_eq!(pts.get(1), Some(&Point::new(25.0, 10.0)), "{pts:?}");
        assert_eq!(pts[pts.len() - 2], Point::new(55.0, 40.0), "{pts:?}");
        assert_eq!(pts.last(), Some(&Point::new(60.0, 40.0)));
        // Moved, it still leaves b straight for 5 mm.
        app.run(Command::TransformShapes {
            shapes: vec![b],
            transform: Affine::translate((10.0, 0.0)),
        });
        app.refresh_connectors();
        let pts = points(&app, c);
        let (q, end) = (pts[pts.len() - 2], pts[pts.len() - 1]);
        assert_eq!(end, Point::new(70.0, 40.0));
        assert!(q.y == end.y && end.x - q.x >= 5.0, "{pts:?}");
        // Without a distance it turns halfway, as before.
        app.settings.connector_gap = 0.0;
        let d = app
            .connect(
                a,
                Point::new(20.0, 10.0),
                b,
                Point::new(70.0, 40.0),
                ConnectorKind::RightAngle,
            )
            .expect("connector");
        assert_eq!(points(&app, d)[1], Point::new(45.0, 10.0));
    }

    /// The way out of an object from an anchor is across its side.
    #[test]
    fn anchors_lead_out_across_their_side() {
        assert_eq!(side_out((1.0, 0.5)), Vec2::new(1.0, 0.0));
        assert_eq!(side_out((0.5, 0.0)), Vec2::new(0.0, -1.0));
        assert_eq!(side_out((0.5, 0.5)), Vec2::ZERO);
        // A straight connector ignores the distance.
        let p = route_clear(
            ConnectorKind::Straight,
            0.0,
            5.0,
            (Point::ZERO, Vec2::new(1.0, 0.0)),
            (Point::new(10.0, 10.0), Vec2::new(-1.0, 0.0)),
        );
        assert_eq!(p.elements().len(), 2);
    }
}
