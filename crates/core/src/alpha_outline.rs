//! The outline of a picture's opaque pixels (#3245): where a picture with
//! transparency is see-through, a contour or a cut line goes round what
//! shows, not round the picture's box.
//!
//! The picture is read at most [`MAX_CELLS`] cells along its longer side;
//! a cell counts as opaque when any pixel in it is at least half opaque,
//! so the outline never cuts into what shows. The edges between opaque
//! and see-through cells are joined into closed loops (read even-odd),
//! their steps straightened within [`TOLERANCE`] of a cell, and specks
//! smaller than [`MIN_AREA`] cells left out. The latest outlines are kept,
//! since a frame asks for an object's contour many times.

use crate::geometry::Rect;
use crate::SharedBytes;
use kurbo::BezPath;
use std::collections::HashMap;

/// The most cells along a picture's longer side.
pub const MAX_CELLS: u32 = 512;
/// How far (in cells) a straightened edge may stray from the steps.
pub const TOLERANCE: f64 = 0.75;
/// Loops covering fewer cells than this are specks and left out.
pub const MIN_AREA: f64 = 4.0;
/// Outlines kept.
const MEMORY: usize = 8;

type Kept = Vec<(SharedBytes, Option<Vec<Vec<(f64, f64)>>>)>;
static KEPT: std::sync::Mutex<Kept> = std::sync::Mutex::new(Vec::new());

/// The outline of the opaque part of the picture `png` placed in `rect`
/// (local space, y up), or `None` when the picture has no see-through
/// pixel (or cannot be read): its box is its outline then.
pub fn opaque_outline(png: &SharedBytes, rect: Rect) -> Option<BezPath> {
    let loops = kept_loops(png)?;
    let (gw, gh) = (loops.0 as f64, loops.1 as f64);
    let (sx, sy) = (rect.width() / gw, rect.height() / gh);
    let mut path = BezPath::new();
    for lp in &loops.2 {
        let mut pts = lp.iter();
        let Some(&(x, y)) = pts.next() else { continue };
        path.move_to((rect.x0 + x * sx, rect.y1 - y * sy));
        for &(x, y) in pts {
            path.line_to((rect.x0 + x * sx, rect.y1 - y * sy));
        }
        path.close_path();
    }
    if path.elements().is_empty() {
        None
    } else {
        Some(path)
    }
}

/// The loops of `png` in cells, with the grid's size, from the kept
/// ones when the same picture was asked for lately.
#[allow(clippy::type_complexity)]
fn kept_loops(png: &SharedBytes) -> Option<(u32, u32, Vec<Vec<(f64, f64)>>)> {
    let grid_of = |png: &SharedBytes| -> Option<(u32, u32)> {
        let (w, h) = png_size(png)?;
        let k = w.max(h).div_ceil(MAX_CELLS).max(1);
        Some((w.div_ceil(k), h.div_ceil(k)))
    };
    if let Ok(kept) = KEPT.lock() {
        if let Some((_, loops)) = kept.iter().find(|(p, _)| SharedBytes::ptr_eq(p, png)) {
            let (gw, gh) = grid_of(png)?;
            return loops.clone().map(|l| (gw, gh, l));
        }
    }
    let made = mask(png).map(|(gw, gh, cells)| loops(gw, gh, &cells));
    let made = made.filter(|l| !l.is_empty());
    if let Ok(mut kept) = KEPT.lock() {
        kept.retain(|(p, _)| !SharedBytes::ptr_eq(p, png));
        kept.push((png.clone(), made.clone()));
        if kept.len() > MEMORY {
            kept.remove(0);
        }
    }
    let (gw, gh) = grid_of(png)?;
    made.map(|l| (gw, gh, l))
}

/// The width and height a PNG header gives.
fn png_size(png: &[u8]) -> Option<(u32, u32)> {
    if png.len() < 24 || &png[..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let w = u32::from_be_bytes(png.get(16..20)?.try_into().ok()?);
    let h = u32::from_be_bytes(png.get(20..24)?.try_into().ok()?);
    (w > 0 && h > 0).then_some((w, h))
}

/// The opaque cells of the picture, rows from the top, or `None` when
/// every pixel is opaque (or none is, or it cannot be read).
fn mask(png: &[u8]) -> Option<(u32, u32, Vec<bool>)> {
    png_size(png)?;
    let (w, h, rgba) = crate::bitmap_warp::decode_rgba(png)?;
    let (wu, hu) = (w as usize, h as usize);
    if rgba.len() < wu * hu * 4 {
        return None;
    }
    let alpha = |x: usize, y: usize| rgba[(y * wu + x) * 4 + 3];
    let k = w.max(h).div_ceil(MAX_CELLS).max(1) as usize;
    let (gw, gh) = (wu.div_ceil(k), hu.div_ceil(k));
    let mut cells = vec![false; gw * gh];
    let (mut any_clear, mut any_solid) = (false, false);
    for y in 0..hu {
        for x in 0..wu {
            if alpha(x, y) >= 128 {
                any_solid = true;
                cells[(y / k) * gw + x / k] = true;
            } else {
                any_clear = true;
            }
        }
    }
    (any_clear && any_solid).then_some((gw as u32, gh as u32, cells))
}

/// The closed loops round the opaque cells of a `gw` by `gh` grid, in
/// cell units (y down), straightened and without specks.
fn loops(gw: u32, gh: u32, cells: &[bool]) -> Vec<Vec<(f64, f64)>> {
    let (gw, gh) = (gw as i64, gh as i64);
    let on = |x: i64, y: i64| {
        x >= 0 && y >= 0 && x < gw && y < gh && cells.get((y * gw + x) as usize) == Some(&true)
    };
    // Each boundary edge, from its start corner to its end corner, the
    // opaque cell always on the same side.
    let mut from: HashMap<(i64, i64), Vec<(i64, i64)>> = HashMap::new();
    let mut count = 0usize;
    for y in 0..gh {
        for x in 0..gw {
            if !on(x, y) {
                continue;
            }
            let mut edge = |a: (i64, i64), b: (i64, i64)| {
                from.entry(a).or_default().push(b);
                count += 1;
            };
            if !on(x, y - 1) {
                edge((x, y), (x + 1, y));
            }
            if !on(x + 1, y) {
                edge((x + 1, y), (x + 1, y + 1));
            }
            if !on(x, y + 1) {
                edge((x + 1, y + 1), (x, y + 1));
            }
            if !on(x - 1, y) {
                edge((x, y + 1), (x, y));
            }
        }
    }
    let mut starts: Vec<(i64, i64)> = from.keys().copied().collect();
    starts.sort_unstable();
    let mut out = Vec::new();
    for s in starts {
        while let Some(next) = from.get_mut(&s).and_then(|v| v.pop()) {
            let mut lp = vec![s];
            let mut at = next;
            let mut guard = count + 1;
            while at != s && guard > 0 {
                guard -= 1;
                lp.push(at);
                match from.get_mut(&at).and_then(|v| v.pop()) {
                    Some(n) => at = n,
                    None => break,
                }
            }
            if at != s {
                continue;
            }
            let pts: Vec<(f64, f64)> = lp.iter().map(|&(x, y)| (x as f64, y as f64)).collect();
            if area(&pts).abs() < MIN_AREA {
                continue;
            }
            let straight = straighten(&pts, TOLERANCE);
            if straight.len() >= 3 {
                out.push(straight);
            }
        }
    }
    out
}

/// The signed area of a closed polygon.
fn area(p: &[(f64, f64)]) -> f64 {
    let n = p.len();
    (0..n)
        .map(|i| {
            let (a, b) = (p[i], p[(i + 1) % n]);
            a.0 * b.1 - b.0 * a.1
        })
        .sum::<f64>()
        / 2.0
}

/// A closed polygon with its points dropped where the line between the
/// kept ones strays no more than `tol` from them (Douglas-Peucker, from
/// the point farthest from the first).
fn straighten(p: &[(f64, f64)], tol: f64) -> Vec<(f64, f64)> {
    let n = p.len();
    if n < 4 {
        return p.to_vec();
    }
    let d2 = |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).powi(2) + (a.1 - b.1).powi(2);
    let far = (1..n)
        .max_by(|&i, &j| d2(p[0], p[i]).total_cmp(&d2(p[0], p[j])))
        .unwrap_or(n / 2);
    let mut keep = vec![false; n];
    keep[0] = true;
    keep[far] = true;
    let ring: Vec<(f64, f64)> = p.iter().copied().chain(std::iter::once(p[0])).collect();
    let mut stack = vec![(0usize, far), (far, n)];
    while let Some((a, b)) = stack.pop() {
        if b <= a + 1 {
            continue;
        }
        let (pa, pb) = (ring[a], ring[b]);
        let len = d2(pa, pb).sqrt();
        let mut best = (0.0, a);
        for (i, q) in ring.iter().enumerate().take(b).skip(a + 1) {
            let d = if len < 1e-12 {
                d2(*q, pa).sqrt()
            } else {
                ((pb.0 - pa.0) * (pa.1 - q.1) - (pa.0 - q.0) * (pb.1 - pa.1)).abs() / len
            };
            if d > best.0 {
                best = (d, i);
            }
        }
        if best.0 > tol {
            keep[best.1 % n] = true;
            stack.push((a, best.1));
            stack.push((best.1, b));
        }
    }
    p.iter()
        .zip(keep)
        .filter_map(|(q, k)| k.then_some(*q))
        .collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A `w` by `h` RGBA PNG whose pixel alpha `alpha(x, y)` gives.
    pub fn png_with(w: u32, h: u32, alpha: impl Fn(u32, u32) -> u8) -> SharedBytes {
        let mut data = Vec::new();
        for y in 0..h {
            for x in 0..w {
                data.extend_from_slice(&[200, 30, 30, alpha(x, y)]);
            }
        }
        let mut out = Vec::new();
        {
            let mut e = png::Encoder::new(&mut out, w, h);
            e.set_color(png::ColorType::Rgba);
            e.set_depth(png::BitDepth::Eight);
            let mut wr = e.write_header().expect("header");
            wr.write_image_data(&data).expect("data");
        }
        SharedBytes::from(out)
    }

    #[test]
    fn a_disc_on_clear_pixels_is_outlined_round_the_disc() {
        let png = png_with(100, 100, |x, y| {
            let (dx, dy) = (x as f64 - 49.5, y as f64 - 49.5);
            if dx * dx + dy * dy <= 30.0 * 30.0 {
                255
            } else {
                0
            }
        });
        let rect = Rect::new(0.0, 0.0, 100.0, 100.0);
        let path = opaque_outline(&png, rect).expect("outlined");
        let b = kurbo::Shape::bounding_box(&path);
        assert!((b.width() - 60.0).abs() < 2.5, "{b:?}");
        assert!((b.center().x - 50.0).abs() < 1.0 && (b.center().y - 50.0).abs() < 1.0);
        // The steps are straightened: far fewer points than edges.
        let points = path.elements().len();
        assert!(points < 120, "{points}");
        let a = kurbo::Shape::area(&path).abs();
        let disc = std::f64::consts::PI * 30.0 * 30.0;
        assert!((a - disc).abs() / disc < 0.06, "{a} vs {disc}");
    }

    #[test]
    fn opaque_pictures_and_specks_keep_their_box() {
        let solid = png_with(20, 10, |_, _| 255);
        assert!(opaque_outline(&solid, Rect::new(0.0, 0.0, 20.0, 10.0)).is_none());
        let clear = png_with(20, 10, |_, _| 0);
        assert!(opaque_outline(&clear, Rect::new(0.0, 0.0, 20.0, 10.0)).is_none());
        // One opaque pixel is a speck.
        let speck = png_with(20, 10, |x, y| if x == 3 && y == 3 { 255 } else { 0 });
        assert!(opaque_outline(&speck, Rect::new(0.0, 0.0, 20.0, 10.0)).is_none());
        // Not a picture at all.
        let junk = SharedBytes::from(vec![1u8, 2, 3]);
        assert!(opaque_outline(&junk, Rect::new(0.0, 0.0, 2.0, 2.0)).is_none());
    }

    #[test]
    fn the_outline_lands_where_the_pixels_are_drawn() {
        // The opaque block in the top left quarter of the picture: the
        // top left of the rect (y up), as the picture is drawn.
        let png = png_with(40, 40, |x, y| if x < 20 && y < 20 { 255 } else { 0 });
        let path = opaque_outline(&png, Rect::new(10.0, 10.0, 50.0, 50.0)).expect("outlined");
        let b = kurbo::Shape::bounding_box(&path);
        assert!((b.x0 - 10.0).abs() < 1e-9 && (b.x1 - 30.0).abs() < 1e-9, "{b:?}");
        assert!((b.y0 - 30.0).abs() < 1e-9 && (b.y1 - 50.0).abs() < 1e-9, "{b:?}");
    }

    #[test]
    fn a_large_picture_is_read_in_cells() {
        let png = png_with(2000, 100, |x, _| if x < 1000 { 255 } else { 0 });
        let path = opaque_outline(&png, Rect::new(0.0, 0.0, 200.0, 10.0)).expect("outlined");
        let b = kurbo::Shape::bounding_box(&path);
        assert!((b.x1 - 100.0).abs() < 0.5, "{b:?}");
    }
}
