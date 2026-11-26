//! Stretching and warping pixels in a picture being edited (Bitmaps >
//! Edit Inside): Stretch Pixels repeats the selection's edge row or
//! column out to the picture's edge on a new layer, and Warp bends the
//! layer in use with a grid of points dragged on the drawing (a smooth
//! surface through them), seen live and put into the pixels on Apply.
//! See `docs/behavior/edit-inside-picture.md`.

use crate::pixel_edit::{Dirty, PixelImage};
use image::{Rgba, RgbaImage};

/// Which way Stretch Pixels pulls the selection's edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StretchDir {
    Up,
    Down,
    Left,
    Right,
}

/// The selection's edge (the row or column of its box on the side away
/// from `dir`, each pixel weighed by the selection) repeated from there
/// to the picture's edge in `dir`, inside the box's span. A new picture
/// of the same size, clear elsewhere.
pub fn stretch_pixels(pi: &PixelImage, dir: StretchDir) -> Option<RgbaImage> {
    let b = pi.sel_bounds()?;
    let (w, h) = pi.img.dimensions();
    let mut out = RgbaImage::new(w, h);
    let take = |x: u32, y: u32| {
        let q = *pi.img.get_pixel(x, y);
        let s = pi.sel_at(x, y);
        Rgba([q[0], q[1], q[2], (q[3] as f32 * s).round() as u8])
    };
    match dir {
        StretchDir::Up | StretchDir::Down => {
            // The row on the side the stretch starts from.
            let row = if dir == StretchDir::Up {
                b[3] - 1
            } else {
                b[1]
            };
            let (y0, y1) = if dir == StretchDir::Up {
                (0, row + 1)
            } else {
                (row, h)
            };
            for x in b[0]..b[2] {
                let q = take(x, row);
                for y in y0..y1 {
                    out.put_pixel(x, y, q);
                }
            }
        }
        StretchDir::Left | StretchDir::Right => {
            let col = if dir == StretchDir::Left {
                b[2] - 1
            } else {
                b[0]
            };
            let (x0, x1) = if dir == StretchDir::Left {
                (0, col + 1)
            } else {
                (col, w)
            };
            for y in b[1]..b[3] {
                let q = take(col, y);
                for x in x0..x1 {
                    out.put_pixel(x, y, q);
                }
            }
        }
    }
    Some(out)
}

/// A warp under way: the layer's pixels when it started and the grid of
/// points over them (picture pixels), row by row.
#[derive(Debug, Clone, PartialEq)]
pub struct Warp {
    pub base: RgbaImage,
    pub cols: usize,
    pub rows: usize,
    pub pts: Vec<(f32, f32)>,
    /// The point being dragged.
    pub drag: Option<usize>,
    /// How far the dragged point lies from the pointer: it keeps that
    /// distance instead of jumping under the pointer.
    pub grab: (f32, f32),
}

/// How near the pointer, in screen points, a grid point is taken: the
/// nearest one within it (#4116).
pub const GRAB_RADIUS: f32 = 20.0;

/// Points of the dense grid drawn and rasterized per control cell.
const DENSE: usize = 12;

impl Warp {
    /// A `cols` x `rows` grid of points evenly over the box `b` of the
    /// layer `base`.
    pub fn new(base: RgbaImage, b: Dirty, cols: usize, rows: usize) -> Self {
        let (cols, rows) = (cols.clamp(2, 12), rows.clamp(2, 12));
        let mut pts = Vec::with_capacity(cols * rows);
        for r in 0..rows {
            for c in 0..cols {
                let u = c as f32 / (cols - 1) as f32;
                let v = r as f32 / (rows - 1) as f32;
                pts.push((
                    b[0] as f32 + u * (b[2] - b[0]) as f32,
                    b[1] as f32 + v * (b[3] - b[1]) as f32,
                ));
            }
        }
        Warp {
            base,
            cols,
            rows,
            pts,
            drag: None,
            grab: (0.0, 0.0),
        }
    }

    /// The grid point nearest (x, y) within `radius` (picture pixels), and
    /// how far it lies from (x, y).
    pub fn pick(&self, x: f32, y: f32, radius: f32) -> Option<(usize, (f32, f32))> {
        let (k, d) = self
            .pts
            .iter()
            .enumerate()
            .map(|(k, p)| (k, (p.0 - x).hypot(p.1 - y)))
            .min_by(|a, b| a.1.total_cmp(&b.1))?;
        (d <= radius).then(|| (k, (self.pts[k].0 - x, self.pts[k].1 - y)))
    }

    /// Take the point nearest (x, y) within `radius` to drag it; false
    /// when none is that near.
    pub fn take(&mut self, x: f32, y: f32, radius: f32) -> bool {
        match self.pick(x, y, radius) {
            Some((k, off)) => {
                self.drag = Some(k);
                self.grab = off;
                true
            }
            None => {
                self.drag = None;
                false
            }
        }
    }

    /// The point being dragged follows the pointer at (x, y).
    pub fn drag_to(&mut self, x: f32, y: f32) {
        if let Some(p) = self.drag.and_then(|k| self.pts.get_mut(k)) {
            *p = (x + self.grab.0, y + self.grab.1);
        }
    }

    fn pt(&self, c: isize, r: isize) -> (f32, f32) {
        // Beyond the grid: carried on in a straight line.
        let cc = c.clamp(0, self.cols as isize - 1);
        let rr = r.clamp(0, self.rows as isize - 1);
        let p = self.pts[rr as usize * self.cols + cc as usize];
        if cc == c && rr == r {
            return p;
        }
        let ic = (cc - (c - cc).signum()).clamp(0, self.cols as isize - 1);
        let ir = (rr - (r - rr).signum()).clamp(0, self.rows as isize - 1);
        let q = self.pts[ir as usize * self.cols + ic as usize];
        (2.0 * p.0 - q.0, 2.0 * p.1 - q.1)
    }

    /// The smooth surface through the points (Catmull-Rom both ways) at
    /// (u, v) in grid units: 0..cols-1, 0..rows-1.
    pub fn eval(&self, u: f32, v: f32) -> (f32, f32) {
        let ci = (u.floor() as isize).clamp(0, self.cols as isize - 2);
        let ri = (v.floor() as isize).clamp(0, self.rows as isize - 2);
        let (tu, tv) = (u - ci as f32, v - ri as f32);
        let cr = |t: f32, p0: f32, p1: f32, p2: f32, p3: f32| {
            0.5 * ((2.0 * p1)
                + (-p0 + p2) * t
                + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t * t
                + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t * t * t)
        };
        let mut col = [(0.0f32, 0.0f32); 4];
        for (k, item) in col.iter_mut().enumerate() {
            let r = ri - 1 + k as isize;
            let p: [(f32, f32); 4] = [
                self.pt(ci - 1, r),
                self.pt(ci, r),
                self.pt(ci + 1, r),
                self.pt(ci + 2, r),
            ];
            *item = (
                cr(tu, p[0].0, p[1].0, p[2].0, p[3].0),
                cr(tu, p[0].1, p[1].1, p[2].1, p[3].1),
            );
        }
        (
            cr(tv, col[0].0, col[1].0, col[2].0, col[3].0),
            cr(tv, col[0].1, col[1].1, col[2].1, col[3].1),
        )
    }

    /// The dense grid: (where it lands, where it samples the layer), row
    /// by row, `n` x `m` points.
    pub fn dense(&self, b: Dirty) -> (usize, usize, Vec<((f32, f32), (f32, f32))>) {
        let n = (self.cols - 1) * DENSE + 1;
        let m = (self.rows - 1) * DENSE + 1;
        let mut out = Vec::with_capacity(n * m);
        for j in 0..m {
            for i in 0..n {
                let u = i as f32 / DENSE as f32;
                let v = j as f32 / DENSE as f32;
                let to = self.eval(u, v);
                let su = u / (self.cols - 1) as f32;
                let sv = v / (self.rows - 1) as f32;
                let from = (
                    b[0] as f32 + su * (b[2] - b[0]) as f32,
                    b[1] as f32 + sv * (b[3] - b[1]) as f32,
                );
                out.push((to, from));
            }
        }
        (n, m, out)
    }

    /// The warped layer: every dense cell drawn as two triangles, each
    /// pixel sampling the start layer (bilinear). The picture's size.
    pub fn render(&self, b: Dirty) -> RgbaImage {
        let (w, h) = self.base.dimensions();
        let mut out = RgbaImage::new(w, h);
        // Outside the grid's box the layer stays as it was.
        for (x, y, q) in self.base.enumerate_pixels() {
            if x < b[0] || x >= b[2] || y < b[1] || y >= b[3] {
                out.put_pixel(x, y, *q);
            }
        }
        let (n, m, g) = self.dense(b);
        for j in 0..m - 1 {
            for i in 0..n - 1 {
                let a = g[j * n + i];
                let bb = g[j * n + i + 1];
                let c = g[(j + 1) * n + i + 1];
                let d = g[(j + 1) * n + i];
                tri(&mut out, &self.base, a, bb, c);
                tri(&mut out, &self.base, a, c, d);
            }
        }
        out
    }
}

/// The source picture at (x, y), bilinear, clear outside.
fn sample(img: &RgbaImage, x: f32, y: f32) -> [f32; 4] {
    let (w, h) = img.dimensions();
    let (x, y) = (x - 0.5, y - 0.5);
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let at = |xi: f32, yi: f32| -> [f32; 4] {
        if xi < 0.0 || yi < 0.0 || xi >= w as f32 || yi >= h as f32 {
            return [0.0; 4];
        }
        let q = img.get_pixel(xi as u32, yi as u32);
        // Premultiplied, so clear pixels do not darken the edge.
        let a = q[3] as f32 / 255.0;
        [
            q[0] as f32 * a,
            q[1] as f32 * a,
            q[2] as f32 * a,
            q[3] as f32,
        ]
    };
    let p = [
        at(x0, y0),
        at(x0 + 1.0, y0),
        at(x0, y0 + 1.0),
        at(x0 + 1.0, y0 + 1.0),
    ];
    let mut o = [0.0; 4];
    for (k, v) in o.iter_mut().enumerate() {
        let top = p[0][k] + (p[1][k] - p[0][k]) * fx;
        let bot = p[2][k] + (p[3][k] - p[2][k]) * fx;
        *v = top + (bot - top) * fy;
    }
    o
}

/// One textured triangle: corners (where it lands, where it samples).
fn tri(
    out: &mut RgbaImage,
    src: &RgbaImage,
    a: ((f32, f32), (f32, f32)),
    b: ((f32, f32), (f32, f32)),
    c: ((f32, f32), (f32, f32)),
) {
    let (w, h) = out.dimensions();
    let (p0, p1, p2) = (a.0, b.0, c.0);
    let area = (p1.0 - p0.0) * (p2.1 - p0.1) - (p2.0 - p0.0) * (p1.1 - p0.1);
    if area.abs() < 1e-6 {
        return;
    }
    let minx = p0.0.min(p1.0).min(p2.0).floor().max(0.0) as u32;
    let maxx = (p0.0.max(p1.0).max(p2.0).ceil().max(0.0) as u32).min(w);
    let miny = p0.1.min(p1.1).min(p2.1).floor().max(0.0) as u32;
    let maxy = (p0.1.max(p1.1).max(p2.1).ceil().max(0.0) as u32).min(h);
    for y in miny..maxy {
        for x in minx..maxx {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let w0 = ((p1.0 - px) * (p2.1 - py) - (p2.0 - px) * (p1.1 - py)) / area;
            let w1 = ((p2.0 - px) * (p0.1 - py) - (p0.0 - px) * (p2.1 - py)) / area;
            let w2 = 1.0 - w0 - w1;
            if w0 < -1e-4 || w1 < -1e-4 || w2 < -1e-4 {
                continue;
            }
            let sx = a.1 .0 * w0 + b.1 .0 * w1 + c.1 .0 * w2;
            let sy = a.1 .1 * w0 + b.1 .1 * w1 + c.1 .1 * w2;
            let s = sample(src, sx, sy);
            if s[3] <= 0.0 {
                continue;
            }
            let k = 255.0 / s[3];
            let px = Rgba([
                (s[0] * k).round().clamp(0.0, 255.0) as u8,
                (s[1] * k).round().clamp(0.0, 255.0) as u8,
                (s[2] * k).round().clamp(0.0, 255.0) as u8,
                s[3].round().clamp(0.0, 255.0) as u8,
            ]);
            out.put_pixel(x, y, px);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pixel_edit::rect_mask;

    #[test]
    fn a_grid_point_is_taken_near_it_and_keeps_its_distance_from_the_pointer() {
        let mut w = Warp::new(RgbaImage::new(40, 40), [0, 0, 40, 40], 3, 3);
        // The middle point is at (20, 20): a press 5 px off takes it.
        assert!(w.take(24.0, 23.0, 8.0));
        assert_eq!(w.drag, Some(4));
        w.drag_to(30.0, 23.0);
        assert_eq!(w.pts[4], (26.0, 20.0));
        // Too far from every point: nothing is taken.
        assert!(!w.take(10.0, 10.0, 8.0));
        assert_eq!(w.drag, None);
        w.drag_to(0.0, 0.0);
        assert_eq!(w.pts[0], (0.0, 0.0));
    }

    #[test]
    fn stretch_repeats_the_selections_edge_to_the_picture_edge() {
        let mut pi = PixelImage::new(RgbaImage::from_fn(10, 10, |x, _| {
            Rgba([x as u8 * 20, 0, 0, 255])
        }));
        pi.sel = rect_mask(10, 10, (2.0, 4.0), (6.0, 6.0));
        let up = stretch_pixels(&pi, StretchDir::Up).unwrap();
        assert_eq!(up.get_pixel(3, 0)[0], 60, "the column's colour carried up");
        assert_eq!(up.get_pixel(3, 9)[3], 0, "nothing below the selection");
        assert_eq!(up.get_pixel(8, 0)[3], 0, "nothing beside it");
        let right = stretch_pixels(&pi, StretchDir::Right).unwrap();
        assert_eq!(
            right.get_pixel(9, 5)[0],
            40,
            "the left column carried right"
        );
        assert_eq!(right.get_pixel(0, 5)[3], 0);
    }

    #[test]
    fn an_untouched_warp_leaves_the_layer_as_it_was() {
        let base = RgbaImage::from_fn(20, 20, |x, y| {
            Rgba([(x * 12) as u8, (y * 12) as u8, 0, 255])
        });
        let w = Warp::new(base.clone(), [0, 0, 20, 20], 4, 4);
        let out = w.render([0, 0, 20, 20]);
        for (x, y) in [(1, 1), (10, 10), (18, 3)] {
            let (a, b) = (out.get_pixel(x, y), base.get_pixel(x, y));
            for k in 0..4 {
                assert!(
                    (a[k] as i16 - b[k] as i16).abs() <= 2,
                    "{x},{y}: {a:?} {b:?}"
                );
            }
        }
    }

    #[test]
    fn dragging_a_point_moves_the_pixels_with_it() {
        let mut base = RgbaImage::new(30, 30);
        for y in 13..17 {
            for x in 13..17 {
                base.put_pixel(x, y, Rgba([255, 0, 0, 255]));
            }
        }
        let mut w = Warp::new(base, [0, 0, 30, 30], 3, 3);
        // The middle point moved 6 pixels right.
        w.pts[4].0 += 6.0;
        let out = w.render([0, 0, 30, 30]);
        assert_eq!(out.get_pixel(21, 15)[0], 255, "moved right");
        assert_eq!(out.get_pixel(13, 15)[3], 0, "left behind clear");
        // The surface passes through the points.
        let p = w.eval(1.0, 1.0);
        assert!((p.0 - 21.0).abs() < 1e-3 && (p.1 - 15.0).abs() < 1e-3);
    }
}
