//! Live paint (the Smart Fill tool with Live ticked): the areas the lines
//! of a group close off, filled where the user clicked and kept live.
//! Each fill is a point in the group's own space with its colour; the
//! group draws, under its objects, the area its outlines close around each
//! point as they are now, so moving a line moves the edge of every fill
//! it bounds. A point that ends up outside every closed area (the lines
//! opened up) draws nothing until the lines close round it again. Drawn
//! by the live effect [`crate::live::Effect::LivePaint`].

use crate::document::{Shape, ShapeKind};
use crate::geometry::{Affine, BezPath, Point};
use crate::shaping::{enclosed_visible_area, Enclosed};
use crate::style::Fill;
use crate::Color;

/// The most fills one group keeps.
pub const MAX_FILLS: usize = 500;

/// The outlines of the objects in `children` (in the group's space),
/// inside groups too, each as a wall only (fills hide nothing).
fn walls(children: &[Shape], parent: Affine, out: &mut Vec<(BezPath, bool)>) {
    for c in children {
        let t = parent * c.transform;
        match &c.kind {
            ShapeKind::Group { children } => walls(children, t, out),
            ShapeKind::Bitmap { .. } => {}
            _ => out.push((t * c.local_path(), false)),
        }
    }
}

/// The area the lines of `group` close around `p` (both in the group's
/// own space), if they close one.
pub fn region_at(group: &Shape, p: Point) -> Option<BezPath> {
    let ShapeKind::Group { children } = &group.kind else {
        return None;
    };
    let mut parts = Vec::new();
    walls(children, Affine::IDENTITY, &mut parts);
    match enclosed_visible_area(&parts, p) {
        Enclosed::Area(a) if !a.elements().is_empty() => Some(a),
        _ => None,
    }
}

/// The filled areas the effect draws under `group`'s objects: one for
/// each fill whose point the lines close round, in the fills' order.
pub fn paint_shapes(group: &Shape, fills: &[(f64, f64, Color)]) -> Vec<Shape> {
    let ShapeKind::Group { children } = &group.kind else {
        return Vec::new();
    };
    let mut parts = Vec::new();
    walls(children, Affine::IDENTITY, &mut parts);
    fills
        .iter()
        .take(MAX_FILLS)
        .filter_map(
            |&(x, y, color)| match enclosed_visible_area(&parts, Point::new(x, y)) {
                Enclosed::Area(path) if !path.elements().is_empty() => {
                    let mut s = Shape::new(group.id, ShapeKind::Path { path, closed: true });
                    s.transform = group.transform;
                    s.fill = Fill::Solid(color);
                    s.stroke = None;
                    s.name = Some("Live Paint".into());
                    Some(s)
                }
                _ => None,
            },
        )
        .collect()
}

/// `fills` with a click at `p` (group space) in `color`: the fill whose
/// area holds `p` takes the colour (or goes, with `None`), otherwise a
/// new fill is added there. Whether anything changed: a click outside
/// every closed area changes nothing.
pub fn paint_at(
    group: &Shape,
    fills: &mut Vec<(f64, f64, Color)>,
    p: Point,
    color: Option<Color>,
) -> bool {
    let Some(area) = region_at(group, p) else {
        return false;
    };
    let same = |&(x, y, _): &(f64, f64, Color)| {
        region_at(group, Point::new(x, y)).is_some_and(|a| a == area)
    };
    let had = fills.len();
    fills.retain(|f| !same(f));
    match color {
        Some(c) if fills.len() < MAX_FILLS => {
            fills.push((p.x, p.y, c));
            true
        }
        _ => fills.len() != had,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(a: (f64, f64), b: (f64, f64)) -> Shape {
        let mut p = BezPath::new();
        p.move_to(a);
        p.line_to(b);
        Shape::new(
            crate::ShapeId(1),
            ShapeKind::Path {
                path: p,
                closed: false,
            },
        )
    }

    /// A noughts-and-crosses grid of four lines in a square: a click in
    /// a cell fills that cell; the fill follows a line that moves; a
    /// second click in the same cell recolours it, None takes it away,
    /// and a click outside the square changes nothing.
    #[test]
    fn live_paint_fills_follow_the_lines() {
        let grid = |x1: f64| {
            let kids = vec![
                line((0.0, 0.0), (30.0, 0.0)),
                line((30.0, 0.0), (30.0, 30.0)),
                line((30.0, 30.0), (0.0, 30.0)),
                line((0.0, 30.0), (0.0, 0.0)),
                line((x1, -5.0), (x1, 35.0)),
                line((-5.0, 10.0), (35.0, 10.0)),
            ];
            Shape::new(crate::ShapeId(9), ShapeKind::Group { children: kids })
        };
        let g = grid(10.0);
        let red = Color::rgb8(255, 0, 0);
        let mut fills = Vec::new();
        assert!(paint_at(&g, &mut fills, Point::new(5.0, 5.0), Some(red)));
        let shapes = paint_shapes(&g, &fills);
        assert_eq!(shapes.len(), 1);
        let b = kurbo::Shape::bounding_box(&shapes[0].local_path());
        assert!(
            (b.x1 - 10.0).abs() < 1e-6 && (b.y1 - 10.0).abs() < 1e-6,
            "{b:?}"
        );
        // The line moves: the fill's edge goes with it.
        let moved = grid(15.0);
        let b = kurbo::Shape::bounding_box(&paint_shapes(&moved, &fills)[0].local_path());
        assert!((b.x1 - 15.0).abs() < 1e-6, "{b:?}");
        // Recolour, then remove.
        let blue = Color::rgb8(0, 0, 255);
        assert!(paint_at(&g, &mut fills, Point::new(2.0, 8.0), Some(blue)));
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].2, blue);
        assert!(paint_at(&g, &mut fills, Point::new(3.0, 3.0), None));
        assert!(fills.is_empty());
        assert!(!paint_at(&g, &mut fills, Point::new(50.0, 50.0), Some(red)));
        assert!(paint_shapes(&line((0.0, 0.0), (1.0, 1.0)), &[(0.0, 0.0, red)]).is_empty());
    }
}
