//! Object > Symbol > New Symbol from Copies: the selected object and
//! every copy of it in the drawing (the same object moved, turned,
//! scaled or mirrored, with the same fill, outline and effects) become
//! instances of one new symbol, each where its object was, in its place
//! in the stacking order. Editing the symbol then changes them all.

use crate::app::App;
use crate::i18n::trf;
use kurbo::{Affine, BezPath, PathEl, Vec2};
use tracedraw_core::{Command, Fill, LayerId, Shape, ShapeId, ShapeKind};

/// The object without what places and names it: two objects whose bare
/// forms are equal are copies under their transformations.
fn bare(s: &Shape) -> Shape {
    let mut b = s.clone();
    b.id = ShapeId(0);
    b.transform = Affine::IDENTITY;
    b.name = None;
    // Every object has an id of its own that lasts across saves.
    b.uid = None;
    b
}

/// The points of a curve, for comparing curves whose moves were made
/// part of their points.
fn points(p: &BezPath) -> Vec<(u8, kurbo::Point)> {
    let mut out = Vec::new();
    for el in p.elements() {
        match *el {
            PathEl::MoveTo(a) => out.push((0, a)),
            PathEl::LineTo(a) => out.push((1, a)),
            PathEl::QuadTo(a, b) => out.extend([(2, a), (2, b)]),
            PathEl::CurveTo(a, b, c) => out.extend([(3, a), (3, b), (3, c)]),
            PathEl::ClosePath => out.push((4, kurbo::Point::ZERO)),
        }
    }
    out
}

/// How `t` is `s` placed again: the transformation from `s` to `t` on
/// the page, if `t` is a copy of `s`. Copies are the same object under
/// another transformation, or curves the same but for a move made part
/// of their points.
pub fn copy_transform(s: &Shape, t: &Shape) -> Option<Affine> {
    let (bs, bt) = (bare(s), bare(t));
    if bs == bt {
        let det = s.transform.determinant();
        if det.abs() < 1e-12 {
            return None;
        }
        return Some(t.transform * s.transform.inverse());
    }
    // Curves moved by their points: the same but for the curve, which is
    // the other's moved on the page.
    let (
        ShapeKind::Path {
            path: ps,
            closed: cs,
        },
        ShapeKind::Path {
            path: pt,
            closed: ct,
        },
    ) = (&s.kind, &t.kind)
    else {
        return None;
    };
    if cs != ct || s.transform != t.transform {
        return None;
    }
    let mut a = bs.clone();
    a.kind = ShapeKind::Path {
        path: BezPath::new(),
        closed: *cs,
    };
    let mut b = bt.clone();
    b.kind = a.kind.clone();
    if a != b {
        return None;
    }
    let (wa, wb) = (
        points(&(s.transform * ps.clone())),
        points(&(t.transform * pt.clone())),
    );
    if wa.len() != wb.len() || wa.is_empty() {
        return None;
    }
    let d: Vec2 = wb[0].1 - wa[0].1;
    let same = wa
        .iter()
        .zip(&wb)
        .all(|(x, y)| x.0 == y.0 && (x.0 == 4 || ((y.1 - x.1) - d).hypot() < 1e-6));
    same.then(|| Affine::translate(d))
}

impl App {
    /// Whether New Symbol from Copies applies: one object selected that
    /// is not already an instance.
    pub fn can_symbol_from_copies(&self) -> bool {
        let shapes = self.selected_shapes();
        matches!(shapes.as_slice(), [s] if !matches!(s.kind, ShapeKind::SymbolInstance { .. }))
    }

    /// The selected object and its copies lying on layers (not inside
    /// groups) become instances of one new symbol, in one undo step. The
    /// number of objects linked.
    pub fn symbol_from_copies(&mut self) -> usize {
        if !self.can_symbol_from_copies() {
            return 0;
        }
        let s = self.selected_shapes()[0].clone();
        // The copies, with their layers and their transformation from s.
        let mut found: Vec<(LayerId, ShapeId, Affine)> = Vec::new();
        for layer in self.doc().all_layers() {
            for t in &layer.shapes {
                if t.id == s.id {
                    found.push((layer.id, t.id, Affine::IDENTITY));
                } else if let Some(m) = copy_transform(&s, t) {
                    found.push((layer.id, t.id, m));
                }
            }
        }
        // The selection inside a group or clip frame is not on a layer.
        if !found.iter().any(|f| f.1 == s.id) {
            return 0;
        }
        let b = s.bounds();
        let mut def = s.clone();
        def.transform_by(Affine::translate((-b.x0, -b.y0)));
        def.id = ShapeId(0);
        let index = self.doc().symbols.len();
        let name = format!("Symbol {}", index + 1);
        let mut cmds = vec![Command::AddSymbol {
            symbol: tracedraw_core::Symbol { name, shape: def },
        }];
        let mut new_sel = Vec::new();
        for (layer, old, m) in &found {
            let id = self.engine.new_shape_id();
            let mut inst = Shape::new(id, ShapeKind::SymbolInstance { index });
            inst.transform = *m * Affine::translate((b.x0, b.y0));
            inst.fill = Fill::None;
            inst.stroke = None;
            cmds.push(Command::AddShape {
                layer: *layer,
                shape: inst,
            });
            cmds.push(Command::OrderRelative {
                shapes: vec![id],
                reference: *old,
                in_front: true,
            });
            if *old == s.id {
                new_sel.push(id);
            }
        }
        cmds.push(Command::DeleteShapes {
            shapes: found.iter().map(|f| f.1).collect(),
        });
        if let Err(e) = self.engine.run_batch("New Symbol from Copies", &cmds) {
            self.status = e.to_string();
            return 0;
        }
        self.selection = new_sel;
        let n = found.len();
        self.status = trf("symbol_copies.done", &[("count", &n.to_string())]);
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::Shape as _;
    use tracedraw_core::geometry::Rect;

    fn add(app: &mut App, kind: ShapeKind, t: Affine, color: u8) -> ShapeId {
        let layer = app.active_layer().unwrap();
        let id = app.engine.new_shape_id();
        let mut s = Shape::new(id, kind);
        s.transform = t;
        s.fill = Fill::Solid(tracedraw_core::Color::rgb8(color, 0, 0));
        app.run(Command::AddShape { layer, shape: s });
        id
    }

    #[test]
    fn copies_become_one_symbol_in_their_places() {
        let mut app = App::headless();
        let star = ShapeKind::Rect {
            rect: Rect::new(0.0, 0.0, 10.0, 5.0),
            radius: 0.0,
            corners: None,
        };
        let a = add(&mut app, star.clone(), Affine::translate((20.0, 20.0)), 200);
        let other = add(&mut app, star.clone(), Affine::translate((80.0, 20.0)), 10);
        let b = add(
            &mut app,
            star.clone(),
            Affine::translate((50.0, 60.0)) * Affine::rotate(0.5),
            200,
        );
        // A curve with its move made part of its points.
        let path = kurbo::Rect::new(0.0, 0.0, 10.0, 5.0).to_path(0.1);
        let c1 = add(
            &mut app,
            ShapeKind::Path {
                path: path.clone(),
                closed: true,
            },
            Affine::IDENTITY,
            50,
        );
        let c2 = add(
            &mut app,
            ShapeKind::Path {
                path: Affine::translate((30.0, 5.0)) * path,
                closed: true,
            },
            Affine::IDENTITY,
            50,
        );
        let before_b = app.doc().find_shape(b).unwrap().clone();
        let bounds_b = app.visual_bounds(&before_b);
        app.select(vec![a]);
        assert_eq!(app.symbol_from_copies(), 2);
        let doc = app.doc();
        assert_eq!(doc.symbols.len(), 1);
        assert!(doc.find_shape(a).is_none() && doc.find_shape(b).is_none());
        // The other colour stays an object of its own, between the two.
        let layer = doc
            .all_layers()
            .find(|l| l.shapes.iter().any(|s| s.id == other))
            .unwrap();
        let kinds: Vec<bool> = layer
            .shapes
            .iter()
            .map(|s| matches!(s.kind, ShapeKind::SymbolInstance { .. }))
            .collect();
        assert_eq!(&kinds[kinds.len() - 5..], [true, false, true, false, false]);
        // The turned copy shows where it was.
        let inst_b = &layer.shapes[layer.shapes.len() - 3];
        let after_b = app.visual_bounds(inst_b);
        assert!(
            (after_b.x0 - bounds_b.x0).abs() < 1e-6 && (after_b.y1 - bounds_b.y1).abs() < 1e-6,
            "{bounds_b:?} {after_b:?}"
        );
        // Curves moved by their points are copies too.
        app.select(vec![c1]);
        assert_eq!(app.symbol_from_copies(), 2);
        assert!(app.doc().find_shape(c2).is_none());
        // One undo brings them back.
        app.undo();
        assert!(app.doc().find_shape(c2).is_some());
    }
}
