//! Tiles along a path (the Brush Strokes tool's Tiles mode): one small
//! picture repeated end to end along each part of the path and bent to
//! follow it, `width` mm across. The number of tiles is the one that
//! fits the part best, each stretched a little so they meet the ends
//! exactly. Drawn by the live effect [`crate::live::Effect::Tiles`].

use crate::document::{Shape, ShapeKind};
use crate::geometry::{BezPath, PathEl, Point, Vec2};
use crate::style::Fill;
use crate::Color;

/// The tile pictures, in the order of the Brush Strokes bar.
pub const PRESETS: u8 = 6;

/// The most tiles one effect draws.
const MAX_TILES: usize = 5000;

/// Tile `preset`'s picture in width units: `x` from 0 to the tile's
/// length (returned first), `y` across from -0.5 to 0.5. Closed
/// subpaths, filled even-odd.
pub fn tile(preset: u8) -> (f64, BezPath) {
    let circle = |p: &mut BezPath, cx: f64, cy: f64, rx: f64, ry: f64| {
        let k = 0.552_284_75;
        p.move_to((cx + rx, cy));
        p.curve_to(
            (cx + rx, cy + ry * k),
            (cx + rx * k, cy + ry),
            (cx, cy + ry),
        );
        p.curve_to(
            (cx - rx * k, cy + ry),
            (cx - rx, cy + ry * k),
            (cx - rx, cy),
        );
        p.curve_to(
            (cx - rx, cy - ry * k),
            (cx - rx * k, cy - ry),
            (cx, cy - ry),
        );
        p.curve_to(
            (cx + rx * k, cy - ry),
            (cx + rx, cy - ry * k),
            (cx + rx, cy),
        );
        p.close_path();
    };
    let poly = |p: &mut BezPath, pts: &[(f64, f64)]| {
        for (i, q) in pts.iter().enumerate() {
            if i == 0 {
                p.move_to(*q);
            } else {
                p.line_to(*q);
            }
        }
        p.close_path();
    };
    let mut p = BezPath::new();
    match preset {
        // Chain: oval links, each reaching a little into the next.
        0 => {
            circle(&mut p, 0.8, 0.0, 0.9, 0.36);
            circle(&mut p, 0.8, 0.0, 0.62, 0.17);
            (1.6, p)
        }
        // Rope: slanted strands.
        1 => {
            p.move_to((0.0, -0.5));
            p.curve_to((0.25, -0.5), (0.45, 0.5), (0.7, 0.5));
            p.line_to((1.05, 0.5));
            p.curve_to((0.8, 0.5), (0.6, -0.5), (0.35, -0.5));
            p.close_path();
            (0.7, p)
        }
        // Zigzag: a band going up and down.
        2 => {
            poly(
                &mut p,
                &[
                    (0.0, -0.5),
                    (0.5, 0.25),
                    (1.0, -0.5),
                    (1.0, -0.25),
                    (0.5, 0.5),
                    (0.0, -0.25),
                ],
            );
            (1.0, p)
        }
        // Dash and dot.
        3 => {
            poly(
                &mut p,
                &[(0.1, -0.12), (1.1, -0.12), (1.1, 0.12), (0.1, 0.12)],
            );
            circle(&mut p, 1.5, 0.0, 0.17, 0.17);
            (1.8, p)
        }
        // Waves: a band like a sine.
        4 => {
            let n = 32;
            let y = |x: f64| 0.3 * (x * std::f64::consts::PI).sin();
            let mut pts: Vec<(f64, f64)> = (0..=n)
                .map(|i| {
                    let x = 2.0 * i as f64 / n as f64;
                    (x, y(x) - 0.12)
                })
                .collect();
            pts.extend((0..=n).rev().map(|i| {
                let x = 2.0 * i as f64 / n as f64;
                (x, y(x) + 0.12)
            }));
            poly(&mut p, &pts);
            (2.0, p)
        }
        // Beads: a round bead each.
        _ => {
            circle(&mut p, 0.5, 0.0, 0.42, 0.42);
            (1.0, p)
        }
    }
}

/// The tiles of `preset`, `width` mm across, along each part of `path`
/// (its own space): one path per tile.
pub fn tiles_along(path: &BezPath, preset: u8, width: f64) -> Vec<BezPath> {
    if !width.is_finite() || width <= 0.0 {
        return Vec::new();
    }
    let (aspect, pic) = tile(preset);
    // The picture flat, as lines in width units.
    let mut flat: Vec<PathEl> = Vec::new();
    crate::geometry::flatten(&pic, 0.01, &mut |el| flat.push(el));
    let mut parts: Vec<Vec<Point>> = Vec::new();
    crate::geometry::flatten(path, 0.02, &mut |el| match el {
        PathEl::MoveTo(p) => parts.push(vec![p]),
        PathEl::LineTo(p) => {
            if let Some(part) = parts.last_mut() {
                part.push(p);
            }
        }
        PathEl::ClosePath => {
            if let Some(part) = parts.last_mut() {
                if let Some(&first) = part.first() {
                    part.push(first);
                }
            }
        }
        _ => {}
    });
    let mut out = Vec::new();
    for pts in parts.iter().filter(|p| p.len() >= 2) {
        let mut cum = vec![0.0];
        for w in pts.windows(2) {
            cum.push(cum.last().copied().unwrap_or(0.0) + (w[1] - w[0]).hypot());
        }
        let total = cum.last().copied().unwrap_or(0.0);
        if total < 1e-6 {
            continue;
        }
        // Where the part is `s` along it, and its way there; past an end
        // it goes on straight.
        let at = |s: f64| -> (Point, Vec2) {
            let i = cum
                .partition_point(|c| *c <= s)
                .saturating_sub(1)
                .min(pts.len() - 2);
            let seg = pts[i + 1] - pts[i];
            let len = seg.hypot().max(1e-12);
            let dir = seg / len;
            (pts[i] + dir * (s - cum[i]), dir)
        };
        let n = ((total / (aspect * width)).round() as usize).clamp(1, MAX_TILES);
        let step = total / n as f64;
        for k in 0..n {
            if out.len() >= MAX_TILES {
                return out;
            }
            let place = |q: Point| -> Point {
                let s = k as f64 * step + q.x / aspect * step;
                let (c, dir) = at(s);
                c + Vec2::new(-dir.y, dir.x) * (q.y * width)
            };
            let mut t = BezPath::new();
            for el in &flat {
                match el {
                    PathEl::MoveTo(q) => t.move_to(place(*q)),
                    PathEl::LineTo(q) => t.line_to(place(*q)),
                    PathEl::ClosePath => t.close_path(),
                    _ => {}
                }
            }
            out.push(t);
        }
    }
    out
}

/// The group of tiles the effect draws along `main`, in `color`.
pub fn tiles_group(main: &Shape, preset: u8, width: f64, color: Color) -> Option<Shape> {
    let path = match &main.kind {
        ShapeKind::Group { .. }
        | ShapeKind::ClipFrame { .. }
        | ShapeKind::Table(_)
        | ShapeKind::SymbolInstance { .. }
        | ShapeKind::Bitmap { .. } => return None,
        _ => main.local_path(),
    };
    let children: Vec<Shape> = tiles_along(&path, preset, width)
        .into_iter()
        .map(|p| {
            let mut c = Shape::new(
                main.id,
                ShapeKind::Path {
                    path: p,
                    closed: true,
                },
            );
            c.fill = Fill::Solid(color);
            c.stroke = None;
            c.name = Some("Tile".into());
            c
        })
        .collect();
    if children.is_empty() {
        return None;
    }
    let mut g = Shape::new(main.id, ShapeKind::Group { children });
    g.transform = main.transform;
    g.name = Some("Tiles".into());
    Some(g)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Shape as _;

    /// Tiles fill a straight line end to end, as many as fit best, each
    /// as wide across as asked; on an arc they bend along it; bad input
    /// draws nothing.
    #[test]
    fn tiles_follow_the_path_end_to_end() {
        let mut line = BezPath::new();
        line.move_to((0.0, 0.0));
        line.line_to((100.0, 0.0));
        for preset in 0..PRESETS {
            let (aspect, _) = tile(preset);
            let tiles = tiles_along(&line, preset, 5.0);
            let want = (100.0 / (aspect * 5.0)).round() as usize;
            assert_eq!(tiles.len(), want, "preset {preset}");
            let b = tiles
                .iter()
                .map(|t| t.bounding_box())
                .reduce(|a, b| a.union(b))
                .expect("tiles");
            assert!(b.y0 >= -2.5 - 1e-6 && b.y1 <= 2.5 + 1e-6, "{preset}: {b:?}");
            assert!(b.height() > 1.0, "{preset}: {b:?}");
            assert!(
                b.x0 > -1.0 && b.x1 < 100.5 + 0.6 * 5.0 * aspect,
                "{preset}: {b:?}"
            );
        }
        // Beads on a half circle stay on it.
        let mut arc = BezPath::new();
        arc.move_to((50.0, 0.0));
        let k = 0.552_284_75 * 50.0;
        arc.curve_to((50.0, k), (k, 50.0), (0.0, 50.0));
        arc.curve_to((-k, 50.0), (-50.0, k), (-50.0, 0.0));
        for t in tiles_along(&arc, 5, 4.0) {
            let c = t.bounding_box().center();
            let r = c.to_vec2().hypot();
            assert!((r - 50.0).abs() < 0.5, "{c:?}");
        }
        assert!(tiles_along(&line, 0, 0.0).is_empty());
        assert!(tiles_along(&line, 0, f64::NAN).is_empty());
        assert!(tiles_along(&BezPath::new(), 0, 5.0).is_empty());
    }
}
