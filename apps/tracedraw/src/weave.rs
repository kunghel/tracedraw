//! Object > Order > Weave: objects that cross pass over and under one
//! another in turn, as rings of a chain or strands of a knot do. Where
//! two selected objects overlap, the overlaps take turns: in every
//! second one the lower object shows on top, through a copy of it cut
//! to that overlap and laid right above the upper object. The objects
//! and the pieces become one group, in one step to undo.

use crate::app::App;
use crate::i18n::tr;
use kurbo::{BezPath, Point, Shape as _};
use tracedraw_core::shaping::{overlay, overlay_parts, Op};
use tracedraw_core::{Command, Fill, Shape, ShapeId, ShapeKind};

/// What an object covers on the page: its fill (when it draws one) and
/// its outline.
pub fn covered(s: &Shape, fill_open: bool) -> BezPath {
    let fill = if matches!(s.fill, Fill::None)
        || (!fill_open
            && s.kind.is_open_curve()
            && tracedraw_render::closed_subpaths(&s.local_path())
                .elements()
                .is_empty())
    {
        BezPath::new()
    } else {
        s.page_path()
    };
    match crate::tools2::outline_object_path(s) {
        Some(line) if fill.elements().is_empty() => line,
        Some(line) => overlay(&fill, &line, Op::Weld),
        None => fill,
    }
}

/// The overlaps of `lo` and `hi`, in turn around `lo`'s middle, and which
/// of them `lo` passes over: every second one, from the second.
pub fn crossings(lo: &BezPath, hi: &BezPath) -> Vec<(BezPath, bool)> {
    let mut parts = overlay_parts(lo, hi, Op::Intersect);
    parts.retain(|p| {
        let b = p.bounding_box();
        b.is_finite() && b.width() > 1e-6 && b.height() > 1e-6
    });
    let c = lo.bounding_box().center();
    let angle = |p: &BezPath| {
        let m: Point = p.bounding_box().center();
        (m.y - c.y).atan2(m.x - c.x)
    };
    parts.sort_by(|a, b| angle(a).total_cmp(&angle(b)));
    parts
        .into_iter()
        .enumerate()
        .map(|(i, p)| (p, i % 2 == 1))
        .collect()
}

impl App {
    /// Two or more objects selected on one layer, none of them a bitmap.
    pub fn can_weave(&self) -> bool {
        let shapes = self.selected_shapes();
        if shapes.len() < 2
            || shapes
                .iter()
                .any(|s| matches!(s.kind, ShapeKind::Bitmap { .. }))
        {
            return false;
        }
        let layers: Vec<_> = shapes
            .iter()
            .filter_map(|s| self.doc().locate(s.id).ok().map(|(l, _)| l))
            .collect();
        layers.len() == shapes.len() && layers.windows(2).all(|w| w[0] == w[1])
    }

    /// Weave the selected objects: the number of overlaps where the lower
    /// object now passes over (0 when nothing changed).
    pub fn weave(&mut self) -> usize {
        if !self.can_weave() {
            return 0;
        }
        let fill_open = self.doc().metadata.fill_open_curves;
        // Bottom to top, as the layer stacks them.
        let mut shapes: Vec<(usize, Shape)> = self
            .selected_shapes()
            .into_iter()
            .filter_map(|s| self.doc().locate(s.id).ok().map(|(_, i)| (i, s)))
            .collect();
        shapes.sort_by_key(|(i, _)| *i);
        let Some(layer) = shapes
            .first()
            .and_then(|(_, s)| self.doc().locate(s.id).ok())
            .map(|(l, _)| l)
        else {
            return 0;
        };
        let areas: Vec<BezPath> = shapes.iter().map(|(_, s)| covered(s, fill_open)).collect();
        let mut cmds = Vec::new();
        let mut members: Vec<ShapeId> = shapes.iter().map(|(_, s)| s.id).collect();
        let mut over = 0;
        for i in 0..shapes.len() {
            for j in i + 1..shapes.len() {
                let (lo, hi) = (&shapes[i].1, &shapes[j].1);
                for (part, lo_on_top) in crossings(&areas[i], &areas[j]) {
                    if !lo_on_top {
                        continue;
                    }
                    let engine = &mut self.engine;
                    let mut next = || engine.new_shape_id();
                    let copy = lo.with_fresh_ids(&mut next);
                    let mut frame = Shape::new(
                        next(),
                        ShapeKind::Path {
                            path: part,
                            closed: true,
                        },
                    );
                    frame.fill = Fill::None;
                    frame.stroke = None;
                    let id = next();
                    let mut piece = Shape::new(
                        id,
                        ShapeKind::ClipFrame {
                            frame: Box::new(frame),
                            contents: vec![copy],
                        },
                    );
                    piece.fill = Fill::None;
                    piece.stroke = None;
                    piece.name = Some(tr("weave.piece_name"));
                    cmds.push(Command::AddShape {
                        layer,
                        shape: piece,
                    });
                    cmds.push(Command::OrderRelative {
                        shapes: vec![id],
                        reference: hi.id,
                        in_front: true,
                    });
                    members.push(id);
                    over += 1;
                }
            }
        }
        if over == 0 {
            self.status = tr("weave.none");
            return 0;
        }
        cmds.push(Command::Group { shapes: members });
        if let Err(e) = self.engine.run_batch("Weave", &cmds) {
            self.status = e.to_string();
            return 0;
        }
        let group = self
            .doc()
            .layer(layer)
            .ok()
            .and_then(|l| {
                l.shapes
                    .iter()
                    .filter(|s| matches!(s.kind, ShapeKind::Group { .. }))
                    .max_by_key(|s| s.id)
            })
            .map(|g| g.id);
        self.selection = group.into_iter().collect();
        self.raster.borrow_mut().invalidate();
        over
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::Rect;

    fn ring(app: &mut App, x: f64) -> ShapeId {
        let layer = app.active_layer().unwrap();
        let id = app.engine.new_shape_id();
        let mut s = Shape::new(
            id,
            ShapeKind::Ellipse {
                rect: Rect::new(x, 0.0, x + 40.0, 40.0),
                arc: None,
            },
        );
        s.fill = Fill::None;
        s.stroke = Some(tracedraw_core::Stroke::new(
            tracedraw_core::Color::BLACK,
            4.0,
        ));
        app.run(Command::AddShape { layer, shape: s });
        id
    }

    #[test]
    fn two_rings_link_like_a_chain() {
        let mut app = App::headless();
        let a = ring(&mut app, 0.0);
        let b = ring(&mut app, 25.0);
        app.select(vec![a]);
        assert!(!app.can_weave());
        app.select(vec![a, b]);
        assert!(app.can_weave());
        assert_eq!(app.weave(), 1);
        let &[g] = app.selection.as_slice() else {
            panic!("{:?}", app.selection);
        };
        let group = app.doc().find_shape(g).unwrap();
        let ShapeKind::Group { children } = &group.kind else {
            panic!();
        };
        // The lower ring, the upper ring, then the lower ring's piece on
        // top of it, cut to one of the two overlaps.
        assert_eq!(children.len(), 3);
        assert_eq!(children[0].id, a);
        assert_eq!(children[1].id, b);
        let ShapeKind::ClipFrame { frame, contents } = &children[2].kind else {
            panic!("{:?}", children[2].kind);
        };
        assert!(matches!(contents[0].kind, ShapeKind::Ellipse { .. }));
        let fb = frame.bounds();
        assert!(fb.width() < 10.0 && fb.x0 > 20.0, "{fb:?}");
        app.undo();
        assert!(app.doc().find_shape(g).is_none());
        assert!(app.doc().find_shape(a).is_some());
    }

    #[test]
    fn objects_apart_are_not_woven() {
        let mut app = App::headless();
        let a = ring(&mut app, 0.0);
        let b = ring(&mut app, 100.0);
        app.select(vec![a, b]);
        assert_eq!(app.weave(), 0);
        assert_eq!(app.status, tr("weave.none"));
    }
}
