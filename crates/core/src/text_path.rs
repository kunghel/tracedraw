//! Geometry and document updates for text attached to an outline.

use crate::document::Uid;
use crate::geometry::{Affine, BezPath, ParamCurve, ParamCurveNearest, PathEl, PathSeg, Point};
use crate::{Document, Shape, ShapeId, ShapeKind, TextOnPath};
use kurbo::ParamCurveExtrema;
use serde::{Deserialize, Serialize};

/// The identity of the outline, with a stable UID to reject a different
/// object using the same local ID in another drawing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathSource {
    pub shape: ShapeId,
    pub uid: Uid,
}

/// The path text is fitted to, in page space: its first subpath, and for a
/// closed one turned to run clockwise (so the letters stand on its
/// outside) and started at the point nearest to `at` (where the path was
/// clicked), or at its highest point without one. Returns the path and
/// whether it is closed.
pub fn path_for_text_at(path: &BezPath, at: Option<Point>) -> (BezPath, bool) {
    let (first, mut closed) = first_subpath(path);
    let mut segs: Vec<PathSeg> = first.segments().collect();
    if let (Some(a), Some(b)) = (segs.first(), segs.last()) {
        if (a.start() - b.end()).hypot() < 1e-6 && segs.len() > 1 {
            closed = true;
        }
    }
    if !closed || segs.is_empty() {
        return (first, false);
    }
    if let (Some(a), Some(b)) = (segs.first(), segs.last()) {
        if (a.start() - b.end()).hypot() >= 1e-6 {
            segs.push(PathSeg::Line(crate::geometry::Line::new(
                b.end(),
                a.start(),
            )));
        }
    }
    // Shoelace over the flattened outline: positive runs anticlockwise
    // (y up), whose left side, where letters stand, is the inside.
    let mut area = 0.0;
    let mut prev: Option<Point> = None;
    let mut start: Option<Point> = None;
    crate::geometry::flatten(&first, 0.1, &mut |el| match el {
        crate::geometry::PathEl::MoveTo(p) => {
            start = Some(p);
            prev = Some(p);
        }
        crate::geometry::PathEl::LineTo(p) => {
            if let Some(q) = prev {
                area += q.x * p.y - p.x * q.y;
            }
            prev = Some(p);
        }
        _ => {}
    });
    if let (Some(q), Some(p)) = (prev, start) {
        area += q.x * p.y - p.x * q.y;
    }
    if area > 0.0 {
        segs = segs.iter().rev().map(|s| s.reverse()).collect();
    }
    // The start: the point nearest to `at`, or the highest point (the
    // segment and parameter where y is largest).
    let mut best = (0usize, 0.0f64, f64::NEG_INFINITY);
    match at.filter(|p| p.x.is_finite() && p.y.is_finite()) {
        Some(p) => {
            for (i, seg) in segs.iter().enumerate() {
                let n = seg.nearest(p, 1e-6);
                if -n.distance_sq > best.2 {
                    best = (i, n.t, -n.distance_sq);
                }
            }
        }
        None => {
            for (i, seg) in segs.iter().enumerate() {
                let mut ts = vec![0.0, 1.0];
                ts.extend(seg.extrema());
                for t in ts {
                    let y = seg.eval(t).y;
                    if y > best.2 + 1e-9 {
                        best = (i, t, y);
                    }
                }
            }
        }
    }
    let (k, t, _) = best;
    let mut order: Vec<PathSeg> = Vec::new();
    if t < 1.0 - 1e-9 {
        order.push(segs[k].subsegment(t..1.0));
    }
    order.extend(segs[k + 1..].iter().copied());
    order.extend(segs[..k].iter().copied());
    if t > 1e-9 {
        order.push(segs[k].subsegment(0.0..t));
    }
    let mut out = BezPath::new();
    if let Some(s0) = order.first() {
        out.move_to(s0.start());
    }
    for seg in order {
        match seg {
            PathSeg::Line(l) => out.line_to(l.p1),
            PathSeg::Quad(q) => out.quad_to(q.p1, q.p2),
            PathSeg::Cubic(c) => out.curve_to(c.p1, c.p2, c.p3),
        }
    }
    out.close_path();
    (out, true)
}

fn first_subpath(path: &BezPath) -> (BezPath, bool) {
    let mut first = BezPath::new();
    let mut closed = false;
    for el in path.elements() {
        match el {
            PathEl::MoveTo(_) if !first.elements().is_empty() => break,
            PathEl::ClosePath => {
                closed = true;
                break;
            }
            e => first.push(*e),
        }
    }
    (first, closed)
}

/// Follow the same segment and parameter when nodes move. If segments
/// were added or removed, keep the nearest point to the old start.
fn moved_anchor(before: &BezPath, after: &BezPath, start: Point, fallback: Point) -> Point {
    let (mut before, old_closed) = first_subpath(before);
    let (mut after, new_closed) = first_subpath(after);
    if old_closed {
        before.close_path();
    }
    if new_closed {
        after.close_path();
    }
    if before.segments().count() != after.segments().count() {
        return fallback;
    }
    before
        .segments()
        .zip(after.segments())
        .map(|(old, new)| {
            let nearest = old.nearest(start, 1e-6);
            (nearest.distance_sq, new.eval(nearest.t))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map_or(fallback, |(_, p)| p)
}

fn fitted(shape: &Shape) -> Option<(&TextOnPath, Affine)> {
    let ShapeKind::Text {
        origin,
        on_path: Some(text),
        ..
    } = &shape.kind
    else {
        return None;
    };
    Some((text, shape.transform * Affine::translate(origin.to_vec2())))
}

fn outline(doc: &Document, source: PathSource) -> Option<Shape> {
    let shape = doc.shape_on_page(source.shape)?;
    (shape.uid == Some(source.uid)
        && matches!(
            shape.kind,
            ShapeKind::Path { .. }
                | ShapeKind::Ellipse { .. }
                | ShapeKind::Rect { .. }
                | ShapeKind::Polygon { .. }
                | ShapeKind::ClipFrame { .. }
        ))
    .then_some(shape)
}

impl Document {
    /// Update the cached guide after an edit, in the same history step
    /// as its outline. The cached geometry also carries the text's look
    /// when the source is removed or unavailable in another file.
    pub(crate) fn sync_text_path(&mut self, before: &Self, id: ShapeId, source: PathSource) {
        let Some(curve) = outline(self, source) else {
            if let Some(Shape {
                kind: ShapeKind::Text {
                    on_path: Some(tp), ..
                },
                ..
            }) = self.find_shape_mut(id)
            {
                tp.source = None;
            }
            return;
        };
        let (Some(text), Some(old_text), Some(old_curve)) = (
            self.shape_on_page(id),
            before.shape_on_page(id),
            outline(before, source),
        ) else {
            return;
        };
        let (Some((current, to_page)), Some((previous, old_to_page))) =
            (fitted(&text), fitted(&old_text))
        else {
            return;
        };
        if previous.source != Some(source)
            || (curve.transform == old_curve.transform
                && curve.kind == old_curve.kind
                && to_page == old_to_page)
            || !to_page.determinant().is_finite()
            || to_page.determinant().abs() < 1e-12
        {
            return;
        }
        let old_outline = old_curve.page_path();
        let new_outline = curve.page_path();
        let at = previous.path.segments().next().map(|s| {
            let start = old_to_page * s.start();
            let fallback = if old_curve.transform.determinant().abs() >= 1e-12 {
                curve.transform * old_curve.transform.inverse() * start
            } else {
                start
            };
            moved_anchor(&old_outline, &new_outline, start, fallback)
        });
        let (new_path, _) = path_for_text_at(&new_outline, at);
        if new_path.segments().next().is_none() {
            return;
        }
        let mut updated = current.clone();
        updated.path = to_page.inverse() * new_path;
        let old_base = current.baseline();
        let new_base = updated.baseline();
        if old_base.is_closed() || new_base.is_closed() {
            // Closed centred text is based half a circuit from the guide
            // start. Keep the user's offset from that position.
            let half = |base: &crate::Baseline| {
                if base.is_closed() {
                    base.length() / 2.0
                } else {
                    0.0
                }
            };
            let sign = if current.offset < 0.0 && current.mirrors().0 {
                -1.0
            } else {
                1.0
            };
            updated.offset += sign * (half(&new_base) - half(&old_base));
        }
        if let Some(Shape {
            kind: ShapeKind::Text { on_path, .. },
            ..
        }) = self.find_shape_mut(id)
        {
            *on_path = Some(updated);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Engine, TextAlign, TextSpan};

    fn line_to(end: Point) -> BezPath {
        let mut path = BezPath::new();
        path.move_to(Point::ZERO);
        path.line_to(end);
        path
    }

    fn guide(engine: &Engine, id: ShapeId) -> &TextOnPath {
        fitted(engine.document().find_shape(id).expect("text"))
            .expect("fitted text")
            .0
    }

    #[test]
    fn an_attached_guide_survives_native_round_trip_and_follows_node_edits() {
        let mut doc = Document::default();
        let curve = doc.ids_mut().shape();
        let text = doc.ids_mut().shape();
        let source = PathSource {
            shape: curve,
            uid: Uid(123),
        };
        let initial = line_to(Point::new(100.0, 0.0));
        let curve_kind = |path| ShapeKind::Path {
            path,
            closed: false,
        };
        let mut shape = Shape::new(curve, curve_kind(initial.clone()));
        shape.uid = Some(source.uid);
        let kind = ShapeKind::Text {
            spans: vec![TextSpan::new("Keep this text", "sans-serif", 12.0)],
            origin: Point::ZERO,
            frame: None,
            align: TextAlign::Left,
            para: Default::default(),
            on_path: Some(TextOnPath {
                path: initial.clone(),
                offset: 8.0,
                source: Some(source),
                ..Default::default()
            }),
        };
        doc.pages[0].layers[0]
            .shapes
            .extend([shape, Shape::new(text, kind)]);
        let mut bytes = Vec::new();
        doc.write_tdraw_version(&mut bytes, crate::document::TDRAW_FORMAT_VERSION)
            .expect("save");
        let mut engine = Engine::new(Document::from_tdraw_bytes(&bytes).expect("reopen"));
        assert_eq!(guide(&engine, text).source, Some(source));
        assert_eq!(guide(&engine, text).path, initial);

        let mut bent = BezPath::new();
        bent.move_to(Point::ZERO);
        bent.quad_to(Point::new(50.0, 40.0), Point::new(100.0, 0.0));
        engine
            .run(&Command::SetShapeKind {
                shape: curve,
                kind: curve_kind(bent.clone()),
            })
            .expect("bend the guide");
        assert_eq!(guide(&engine, text).path, bent);
        assert_eq!(guide(&engine, text).offset, 8.0);
        engine.undo().expect("undo bending");
        assert_eq!(guide(&engine, text).path, initial);
        engine.redo().expect("redo bending");
        assert_eq!(guide(&engine, text).path, bent);

        engine
            .run(&Command::DeleteShapes {
                shapes: vec![curve],
            })
            .expect("delete the guide");
        assert_eq!(guide(&engine, text).path, bent);
        assert_eq!(guide(&engine, text).source, None);
        engine.undo().expect("undo deletion");
        assert_eq!(guide(&engine, text).source, Some(source));
    }
}
