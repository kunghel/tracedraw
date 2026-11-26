//! The product photos of the mockup templates (Effects > Mockup): each
//! one drawn here, from shapes, light and texture, at any size, so the
//! program ships no picture files and every photo is our own. A photo is
//! laid out on a 1600 by 1200 grid (y down); [`Canvas`] scales it to the
//! pixels asked for.

use image::RgbaImage;

/// The layout grid of every photo.
pub const REF_W: f32 = 1600.0;
pub const REF_H: f32 = 1200.0;

pub type Rgb = [f32; 3];

/// A colour from 0..255 values.
pub const fn rgb(r: u8, g: u8, b: u8) -> Rgb {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0]
}

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

fn scale(a: Rgb, k: f32) -> Rgb {
    [a[0] * k, a[1] * k, a[2] * k]
}

fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A photo being drawn: linear pixels, row by row from the top.
pub struct Canvas {
    pub w: usize,
    pub h: usize,
    /// Pixels per grid unit.
    k: f32,
    px: Vec<Rgb>,
}

impl Canvas {
    /// A blank photo `w` pixels wide (and three quarters of that high).
    pub fn new(w: u32) -> Canvas {
        let w = w.clamp(16, 4000) as usize;
        let h = ((w as f32) * REF_H / REF_W).round().max(12.0) as usize;
        Canvas {
            w,
            h,
            k: w as f32 / REF_W,
            px: vec![[1.0; 3]; w * h],
        }
    }

    /// The pixel rows and columns that cover the grid box `b`
    /// (x0, y0, x1, y1).
    fn span(&self, b: [f32; 4]) -> (usize, usize, usize, usize) {
        let c = |v: f32, n: usize| ((v * self.k).floor().max(0.0) as usize).min(n);
        let e = |v: f32, n: usize| ((v * self.k).ceil().max(0.0) as usize).min(n);
        (c(b[0], self.w), c(b[1], self.h), e(b[2], self.w), e(b[3], self.h))
    }

    /// Every pixel set from its grid position.
    pub fn fill(&mut self, f: impl Fn(f32, f32) -> Rgb) {
        self.paint([0.0, 0.0, REF_W, REF_H], |_, _| -1e9, |x, y, _| f(x, y));
    }

    /// A shape drawn over the photo: inside where `sdf` (a signed
    /// distance in grid units, negative inside) is below zero, smoothed
    /// over one pixel at the edge, coloured by `color(x, y, distance)`.
    pub fn paint(
        &mut self,
        b: [f32; 4],
        sdf: impl Fn(f32, f32) -> f32,
        color: impl Fn(f32, f32, f32) -> Rgb,
    ) {
        let (x0, y0, x1, y1) = self.span(b);
        for j in y0..y1 {
            for i in x0..x1 {
                let (x, y) = ((i as f32 + 0.5) / self.k, (j as f32 + 0.5) / self.k);
                let d = sdf(x, y);
                let cover = (0.5 - d * self.k).clamp(0.0, 1.0);
                if cover <= 0.0 {
                    continue;
                }
                let c = color(x, y, d);
                let p = &mut self.px[j * self.w + i];
                *p = mix(*p, c, cover);
            }
        }
    }

    /// Darken (or with a negative `amount`, lighten) round a shape, by
    /// `amount` at its edge and fading over `soft` grid units.
    pub fn shadow(&mut self, b: [f32; 4], sdf: impl Fn(f32, f32) -> f32, soft: f32, amount: f32) {
        let (x0, y0, x1, y1) = self.span(b);
        for j in y0..y1 {
            for i in x0..x1 {
                let (x, y) = ((i as f32 + 0.5) / self.k, (j as f32 + 0.5) / self.k);
                let t = 1.0 - smooth(-soft * 0.35, soft, sdf(x, y));
                if t <= 0.0 {
                    continue;
                }
                let p = &mut self.px[j * self.w + i];
                *p = scale(*p, 1.0 - amount * t);
            }
        }
    }

    /// The photo as 8-bit pixels.
    pub fn image(&self) -> RgbaImage {
        let mut img = RgbaImage::new(self.w as u32, self.h as u32);
        for (o, p) in img.pixels_mut().zip(&self.px) {
            let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            *o = image::Rgba([c(p[0]), c(p[1]), c(p[2]), 255]);
        }
        img
    }
}

// Noise ----------------------------------------------------------------

fn hash(x: i32, y: i32, s: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343)
        ^ (y as u32).wrapping_mul(0xd816_3841)
        ^ s.wrapping_mul(0xcb1a_b31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h & 0xffff) as f32 / 65535.0
}

/// Smooth value noise 0..1.
pub fn noise(x: f32, y: f32, s: u32) -> f32 {
    let (xi, yi) = (x.floor(), y.floor());
    let (fx, fy) = (x - xi, y - yi);
    let (ux, uy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let (i, j) = (xi as i32, yi as i32);
    let a = hash(i, j, s);
    let b = hash(i + 1, j, s);
    let c = hash(i, j + 1, s);
    let d = hash(i + 1, j + 1, s);
    a + (b - a) * ux + (c - a) * uy + (a - b - c + d) * ux * uy
}

/// Layered noise 0..1 (roughly), `oct` octaves.
pub fn fbm(x: f32, y: f32, s: u32, oct: u32) -> f32 {
    let (mut sum, mut amp, mut f, mut norm) = (0.0, 0.5, 1.0, 0.0);
    for o in 0..oct {
        sum += amp * noise(x * f, y * f, s + o * 17);
        norm += amp;
        amp *= 0.5;
        f *= 2.03;
    }
    sum / norm
}

// Distances ----------------------------------------------------------

/// A rounded rectangle centred on (cx, cy), half sizes hw and hh.
pub fn rrect(x: f32, y: f32, cx: f32, cy: f32, hw: f32, hh: f32, r: f32) -> f32 {
    let r = r.min(hw).min(hh);
    let qx = (x - cx).abs() - hw + r;
    let qy = (y - cy).abs() - hh + r;
    qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - r
}

/// An ellipse (close to the true distance near its edge).
pub fn ellipse(x: f32, y: f32, cx: f32, cy: f32, rx: f32, ry: f32) -> f32 {
    let (px, py) = ((x - cx) / rx, (y - cy) / ry);
    let k0 = px.hypot(py);
    let k1 = (px / rx).hypot(py / ry);
    if k1 < 1e-6 {
        return -rx.min(ry);
    }
    k0 * (k0 - 1.0) / k1
}

/// A polygon (exact).
pub fn poly(x: f32, y: f32, pts: &[(f32, f32)]) -> f32 {
    if pts.len() < 3 {
        return 1e9;
    }
    let mut d = f32::MAX;
    let mut inside = false;
    let mut j = pts.len() - 1;
    for i in 0..pts.len() {
        let (ax, ay) = pts[j];
        let (bx, by) = pts[i];
        d = d.min(segment(x, y, (ax, ay), (bx, by)));
        if (by > y) != (ay > y) && x < (ax - bx) * (y - by) / (ay - by) + bx {
            inside = !inside;
        }
        j = i;
    }
    if inside {
        -d
    } else {
        d
    }
}

/// The distance to a line segment.
pub fn segment(x: f32, y: f32, a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = dx * dx + dy * dy;
    let t = if len > 0.0 {
        (((x - a.0) * dx + (y - a.1) * dy) / len).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (x - a.0 - dx * t).hypot(y - a.1 - dy * t)
}

/// The box round some points, grown by `m`.
pub fn bbox(pts: &[(f32, f32)], m: f32) -> [f32; 4] {
    let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for &(x, y) in pts {
        b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
    }
    [b[0] - m, b[1] - m, b[2] + m, b[3] + m]
}

const ALL: [f32; 4] = [0.0, 0.0, REF_W, REF_H];

// Backdrops ------------------------------------------------------------

/// A seamless studio paper: lighter at the top, a soft vignette.
fn studio(c: &mut Canvas, top: Rgb, bottom: Rgb) {
    c.fill(|x, y| {
        let t = smooth(0.0, REF_H, y);
        let v = 1.0 - 0.18 * ((x - 800.0) / 900.0).powi(2) - 0.08 * ((y - 500.0) / 700.0).powi(2);
        let g = 1.0 + (fbm(x * 0.02, y * 0.02, 3, 2) - 0.5) * 0.015;
        scale(mix(top, bottom, t), v * g)
    });
}

/// A wooden table top seen at an angle from `horizon` down.
fn wood(c: &mut Canvas, horizon: f32, base: Rgb) {
    c.paint(
        [0.0, horizon, REF_W, REF_H],
        |_, y| horizon - y,
        |x, y, _| {
            // Boards run away from the eye: narrower towards the horizon.
            // A negative horizon: the table seen from straight above.
            let depth = if horizon < 0.0 {
                1.0
            } else {
                (y - horizon + 60.0) / (REF_H - horizon + 60.0)
            };
            let u = (x - 800.0) / depth;
            let bw = 150.0;
            let board = (u / bw).floor();
            let along = y / depth.max(0.05);
            let n = fbm(u * 0.03, along * 0.004 + board * 7.0, 11, 4);
            let grain = ((u * 0.09 + n * 7.0).sin() * 0.5 + 0.5).powi(4);
            let tone = 0.86 + 0.08 * hash(board as i32, 0, 5) + 0.08 * grain;
            let f = u.rem_euclid(bw);
            let seam = smooth(1.5, 0.0, f.min(bw - f) * depth);
            let light = 0.78 + 0.32 * depth;
            scale(base, tone * light * (1.0 - 0.35 * seam))
        },
    );
}

/// A painted plaster wall, with light falling from the top left.
fn plaster(c: &mut Canvas, b: [f32; 4], base: Rgb) {
    c.paint(b, |_, _| -1e9, |x, y, _| {
        let n = fbm(x * 0.05, y * 0.05, 21, 4) - 0.5;
        let light = 1.04 - 0.16 * ((x - 300.0).hypot(y - 100.0) / 1700.0);
        scale(base, light + n * 0.05)
    });
}

/// Brick courses.
fn bricks(c: &mut Canvas, b: [f32; 4], base: Rgb, course: f32) {
    c.paint(b, |_, _| -1e9, |x, y, _| {
        let row = (y / course).floor();
        let shift = if row as i32 % 2 == 0 { 0.0 } else { course * 1.5 };
        let len = course * 3.0;
        let col = ((x + shift) / len).floor();
        let (fx, fy) = (((x + shift) / len).fract() * len, (y / course).fract() * course);
        let mortar = smooth(4.0, 2.0, fx.min(len - fx)).max(smooth(4.0, 2.0, fy.min(course - fy)));
        let tone = 0.8 + 0.25 * hash(col as i32, row as i32, 9);
        let n = fbm(x * 0.08, y * 0.08, 4, 3) - 0.5;
        let brick = scale(base, tone + n * 0.18);
        let light = 1.02 - 0.2 * ((x - 200.0).hypot(y) / 1800.0);
        scale(mix(brick, rgb(196, 190, 178), mortar), light)
    });
}

/// Cloth: a fine weave and soft grain, 1 at rest.
fn weave(x: f32, y: f32) -> f32 {
    1.0 + 0.02 * ((x * 1.7).sin() * (y * 1.7).sin()) + (fbm(x * 0.25, y * 0.25, 2, 2) - 0.5) * 0.05
}

// The photos ------------------------------------------------------------

/// The T-shirt laid flat on the studio paper, front up.
pub fn shirt_outline() -> Vec<(f32, f32)> {
    vec![
        (680.0, 150.0),
        (735.0, 190.0),
        (800.0, 205.0),
        (865.0, 190.0),
        (920.0, 150.0),
        (1080.0, 205.0),
        (1250.0, 430.0),
        (1125.0, 505.0),
        (1060.0, 410.0),
        (1070.0, 1080.0),
        (530.0, 1080.0),
        (540.0, 410.0),
        (475.0, 505.0),
        (350.0, 430.0),
        (520.0, 205.0),
    ]
}

pub fn shirt(c: &mut Canvas, cloth: Rgb) {
    studio(c, rgb(236, 236, 234), rgb(212, 212, 210));
    let o = shirt_outline();
    let b = bbox(&o, 70.0);
    let dark = cloth[0] + cloth[1] + cloth[2] < 0.9;
    let sd = |x: f32, y: f32| poly(x, y, &o) - 10.0;
    c.shadow(b, |x, y| sd(x - 8.0, y - 14.0), 40.0, 0.28);
    c.paint(b, sd, |x, y, d| {
        let light = 1.0 - (x - 600.0).hypot(y - 200.0) / 2900.0;
        let folds = 0.05 * ((x * 0.03 + y * 0.009).sin() * (y * 0.008).cos())
            + 0.06 * ((x - 800.0) / 70.0).sin().powi(8) * smooth(500.0, 900.0, y)
            + 0.05 * ((y - 700.0) / 95.0 + x / 230.0).sin().powi(6);
        let arm = (-((x - 560.0).powi(2) + (y - 440.0).powi(2)) / 7000.0).exp()
            + (-((x - 1040.0).powi(2) + (y - 440.0).powi(2)) / 7000.0).exp();
        let rim = smooth(-26.0, 0.0, d) * 0.18;
        let mut s = (light + 0.06 - folds - 0.12 * arm - rim) * weave(x, y);
        if dark {
            // Black cloth shows its folds as soft highlights.
            s = 0.85 + (s - 0.85) * 1.6;
        }
        scale(cloth, s.clamp(0.35, 1.15))
    });
    // The collar band and the stitch lines.
    let collar = |x: f32, y: f32| (ellipse(x, y, 800.0, 150.0, 128.0, 66.0)).abs() - 9.0;
    c.paint([650.0, 70.0, 950.0, 240.0], |x, y| collar(x, y).max(150.0 - y), |x, y, _| {
        scale(cloth, 0.86 * weave(x, y))
    });
    c.paint([650.0, 70.0, 950.0, 240.0], |x, y| ellipse(x, y, 800.0, 140.0, 112.0, 52.0).max(150.0 - y), |_, _, _| {
        scale(cloth, if dark { 0.55 } else { 0.72 })
    });
    for (a, bb) in [((540.0, 410.0), (520.0, 205.0)), ((1060.0, 410.0), (1080.0, 205.0))] {
        c.shadow(bbox(&[a, bb], 20.0), |x, y| segment(x, y, a, bb) - 1.0, 5.0, 0.12);
    }
    c.shadow([520.0, 1040.0, 1080.0, 1080.0], |x, y| (y - 1052.0).abs().max(poly(x, y, &o) + 6.0) - 0.8, 4.0, 0.12);
}

/// The print area of the shirt: the chest, a little rounded.
pub const SHIRT_CHEST: [(f32, f32); 4] = [(620.0, 290.0), (980.0, 290.0), (980.0, 760.0), (620.0, 760.0)];

/// A mug on a wooden table, handle to the right.
pub fn mug(c: &mut Canvas, glaze: Rgb) {
    plaster(c, [0.0, 0.0, REF_W, 760.0], rgb(226, 220, 210));
    wood(c, 740.0, rgb(150, 104, 66));
    let dark = glaze[0] + glaze[1] + glaze[2] < 0.9;
    let (cx, top, bottom, r) = (760.0, 300.0, 930.0, 270.0);
    // Shadow on the table, to the right and behind.
    c.shadow(ALL, |x, y| ellipse(x, y, cx + 70.0, bottom + 6.0, r + 120.0, 52.0), 90.0, 0.5);
    // The handle: a ring, on the right.
    let ring = |x: f32, y: f32| ellipse(x, y, cx + r + 20.0, 600.0, 120.0, 160.0).abs() - 30.0;
    c.paint([cx + r - 80.0, 400.0, cx + r + 190.0, 800.0], |x, y| ring(x, y).max(cx + r - 40.0 - x), |x, y, d| {
        let s = 0.75 + 0.35 * smooth(20.0, -30.0, d) * ((y - 470.0) / 260.0).clamp(0.0, 1.0).sin();
        let s = s - 0.25 * smooth(cx + r + 40.0, cx + r + 140.0, x);
        scale(glaze, s.max(0.3))
    });
    // The body: a cylinder, a rounded foot.
    let body = |x: f32, y: f32| {
        let side = (x - cx).abs() - r;
        let rim = ellipse(x, y, cx, bottom, r, 42.0).max(y - bottom - 42.0);
        side.max(top - y).max(if y > bottom { rim } else { -1.0 })
    };
    c.paint([cx - r - 4.0, top - 50.0, cx + r + 4.0, bottom + 50.0], body, |x, y, _| {
        let a = ((x - cx) / r).clamp(-1.0, 1.0);
        let lambert = (0.65 + 0.35 * (a.asin() + 0.5).cos()).max(0.25);
        let spec = (-((a + 0.42) / 0.07).powi(2)).exp() * 0.5 + (-((a - 0.72) / 0.05).powi(2)).exp() * 0.12;
        let foot = smooth(bottom - 30.0, bottom + 30.0, y) * 0.18;
        let base = if dark {
            scale(glaze, lambert * 1.1 - foot)
        } else {
            scale(glaze, lambert - foot)
        };
        mix(base, [1.0; 3], spec)
    });
    // The rim and the inside.
    c.paint([cx - r - 4.0, top - 50.0, cx + r + 4.0, top + 50.0], |x, y| ellipse(x, y, cx, top, r, 42.0), |x, _, _| {
        let a = ((x - cx) / r).clamp(-1.0, 1.0);
        scale(glaze, 0.92 + 0.08 * a)
    });
    c.paint([cx - r, top - 46.0, cx + r, top + 46.0], |x, y| ellipse(x, y, cx, top + 3.0, r - 14.0, 34.0), |_, y, _| {
        let s = 0.42 + 0.4 * smooth(top - 34.0, top + 30.0, y);
        scale(glaze, if dark { s * 0.7 } else { s })
    });
}

/// The mug's print area: the body between the rim and the foot, the
/// side facing the eye.
pub const MUG: (f32, f32, f32, f32, f32) = (760.0, 370.0, 860.0, 270.0, 42.0);

/// A steel bottle standing on a table.
pub fn bottle(c: &mut Canvas, paint: Rgb) {
    studio(c, rgb(214, 222, 226), rgb(184, 192, 196));
    let (cx, top, bottom, r) = (800.0, 260.0, 1080.0, 165.0);
    c.shadow(ALL, |x, y| ellipse(x, y, cx + 40.0, bottom, r + 90.0, 34.0), 70.0, 0.45);
    // The cap, a narrower cylinder.
    c.paint([cx - 120.0, 60.0, cx + 120.0, top + 30.0], |x, y| rrect(x, y, cx, 170.0, 110.0, 100.0, 22.0), |x, y, _| {
        let a = ((x - cx) / 110.0).clamp(-1.0, 1.0);
        let s = 0.25 + 0.35 * (a.asin() + 0.5).cos().max(0.0) + 0.25 * (-((a + 0.45) / 0.08).powi(2)).exp();
        let ridge = if ((y - 80.0) / 14.0).fract() < 0.18 && y < 230.0 { 0.85 } else { 1.0 };
        scale(rgb(60, 62, 66), s * ridge * 2.0)
    });
    // The shoulder and the body.
    let body = |x: f32, y: f32| {
        let w = r - 60.0 * smooth(top + 70.0, top, y);
        ((x - cx).abs() - w).max(top - y).max(rrect(x, y, cx, (top + bottom) / 2.0, r, (bottom - top) / 2.0, 40.0))
    };
    c.paint([cx - r - 4.0, top - 10.0, cx + r + 4.0, bottom + 4.0], body, |x, y, _| {
        let a = ((x - cx) / r).clamp(-1.0, 1.0);
        let lambert = 0.62 + 0.38 * (a.asin() + 0.45).cos().max(0.0);
        let spec = (-((a + 0.5) / 0.06).powi(2)).exp() * 0.55 + (-((a - 0.78) / 0.04).powi(2)).exp() * 0.2;
        let band = smooth(bottom - 60.0, bottom - 20.0, y) * 0.25 + smooth(top + 90.0, top + 40.0, y) * 0.15;
        mix(scale(paint, lambert - band), [1.0; 3], spec)
    });
}

pub const BOTTLE: (f32, f32, f32, f32, f32) = (800.0, 420.0, 960.0, 165.0, 20.0);

/// A canvas tote bag hanging on a wall hook.
pub fn tote(c: &mut Canvas) {
    plaster(c, ALL, rgb(204, 212, 206));
    let cloth = rgb(232, 222, 198);
    let bag = [(500.0, 420.0), (1100.0, 420.0), (1130.0, 1120.0), (470.0, 1120.0)];
    // The handles: two strips over the hook.
    for (a, b) in [((580.0, 430.0), (790.0, 140.0)), ((1020.0, 430.0), (810.0, 140.0))] {
        c.shadow(bbox(&[a, b], 60.0), |x, y| segment(x, y, (a.0 + 10.0, a.1 + 12.0), (b.0 + 10.0, b.1 + 12.0)) - 22.0, 22.0, 0.25);
        c.paint(bbox(&[a, b], 30.0), |x, y| segment(x, y, a, b) - 22.0, |x, y, d| {
            scale(cloth, (0.9 - smooth(-22.0, 0.0, d) * 0.15) * weave(x, y))
        });
    }
    c.paint([770.0, 110.0, 830.0, 170.0], |x, y| ellipse(x, y, 800.0, 138.0, 22.0, 22.0), |_, y, _| scale(rgb(80, 80, 84), 1.4 - (y - 116.0) / 50.0));
    c.shadow(bbox(&bag, 80.0), |x, y| poly(x - 14.0, y - 18.0, &bag) - 8.0, 40.0, 0.3);
    c.paint(bbox(&bag, 10.0), |x, y| poly(x, y, &bag) - 6.0, |x, y, d| {
        let sag = 0.06 * ((x - 800.0) / 300.0).powi(2) * smooth(420.0, 520.0, y);
        let creases = 0.035 * ((x * 0.018 + y * 0.004).sin()).powi(2) + 0.03 * ((y - 600.0) / 120.0).sin().powi(8);
        let s = 1.0 - 0.1 * ((x - 650.0).hypot(y - 500.0) / 900.0) - sag - creases - smooth(-24.0, 0.0, d) * 0.15;
        scale(cloth, s * weave(x, y))
    });
    c.shadow([500.0, 430.0, 1100.0, 470.0], |x, y| (y - 448.0).abs().max((x - 800.0).abs() - 298.0) - 0.8, 4.0, 0.15);
}

pub const TOTE: [(f32, f32); 4] = [(560.0, 540.0), (1040.0, 540.0), (1056.0, 1040.0), (544.0, 1040.0)];

/// A square cushion on a sofa.
pub fn cushion(c: &mut Canvas) {
    c.fill(|x, y| {
        let n = fbm(x * 0.06, y * 0.06, 8, 3) - 0.5;
        let seat = smooth(840.0, 860.0, y);
        let base = mix(rgb(96, 108, 120), rgb(80, 92, 104), seat);
        scale(base, 1.0 - 0.18 * (x - 800.0).abs() / 900.0 + n * 0.08)
    });
    let pillow = |x: f32, y: f32| {
        // Puffed sides that pinch in at the corners.
        let (u, v) = ((x - 800.0) / 400.0, (y - 560.0) / 400.0);
        let bulge = 1.0 - 0.07 * (1.0 - v * v) - 0.07 * (1.0 - u * u);
        let (hw, hh) = (400.0 * (1.0 - 0.07 * (1.0 - v * v)), 400.0 * (1.0 - 0.07 * (1.0 - u * u)));
        let _ = bulge;
        rrect(x, y, 800.0, 560.0, hw, hh, 70.0)
    };
    c.shadow([300.0, 100.0, 1300.0, 1050.0], |x, y| pillow(x - 20.0, y - 30.0), 70.0, 0.45);
    let cloth = rgb(240, 238, 232);
    c.paint([380.0, 140.0, 1220.0, 980.0], pillow, |x, y, d| {
        let (u, v) = ((x - 800.0) / 400.0, (y - 560.0) / 400.0);
        let dome = 1.0 - 0.14 * (u * u + v * v) - 0.08 * (u + v);
        let wrinkle = 0.04 * ((u * 9.0 + v * 2.0).sin() * (1.0 - v.abs())).powi(2) * smooth(0.6, 1.0, u.abs().max(v.abs()));
        let s = dome - wrinkle - smooth(-30.0, 0.0, d) * 0.22;
        scale(cloth, s * weave(x, y))
    });
}

pub const CUSHION: [(f32, f32); 4] = [(520.0, 290.0), (1080.0, 290.0), (1080.0, 830.0), (520.0, 830.0)];

/// A framed poster hanging on a wall, seen from the left.
pub fn poster(c: &mut Canvas) {
    plaster(c, [0.0, 0.0, REF_W, 1020.0], rgb(232, 228, 220));
    c.paint([0.0, 1000.0, REF_W, REF_H], |_, y| 1010.0 - y, |x, y, _| {
        let n = fbm(x * 0.01, y * 0.1, 31, 3);
        scale(rgb(176, 136, 98), 0.8 + 0.25 * n)
    });
    c.paint([0.0, 990.0, REF_W, 1012.0], |_, y| (y - 1000.0).abs() - 10.0, |_, _, _| rgb(244, 242, 236));
    let frame = [(520.0, 150.0), (1130.0, 205.0), (1130.0, 935.0), (520.0, 990.0)];
    c.shadow(bbox(&frame, 80.0), |x, y| poly(x - 20.0, y - 16.0, &frame), 45.0, 0.35);
    c.paint(bbox(&frame, 4.0), |x, y| poly(x, y, &frame), |x, y, _| {
        let n = fbm(x * 0.4, y * 0.02, 5, 2);
        scale(rgb(36, 32, 30), 0.9 + 0.3 * n)
    });
    let mat = [(552.0, 185.0), (1098.0, 233.0), (1098.0, 907.0), (552.0, 955.0)];
    c.paint(bbox(&mat, 4.0), |x, y| poly(x, y, &mat), |x, y, _| {
        let light = 1.0 - 0.08 * ((x - 520.0) / 600.0) - 0.04 * ((y - 560.0) / 400.0).abs();
        scale(rgb(250, 249, 245), light)
    });
    // The bevel of the mat round the picture.
    c.shadow(bbox(&POSTER, 12.0), |x, y| {
        let d = poly(x, y, &POSTER);
        if d < 0.0 { -d } else { 1e9 }
    }, 6.0, 0.18);
}

pub const POSTER: [(f32, f32); 4] = [(600.0, 236.0), (1050.0, 274.0), (1050.0, 866.0), (600.0, 904.0)];

/// A shop front: a sign board over a glass door in a brick wall.
pub fn storefront(c: &mut Canvas) {
    bricks(c, ALL, rgb(150, 72, 52), 34.0);
    // The pavement.
    c.paint([0.0, 1050.0, REF_W, REF_H], |_, y| 1060.0 - y, |x, y, _| {
        let n = fbm(x * 0.05, y * 0.05, 41, 3);
        let slab = smooth(2.0, 0.0, ((x / 180.0).fract() * 180.0).min(180.0 - (x / 180.0).fract() * 180.0));
        scale(rgb(160, 158, 152), 0.8 + 0.2 * n - 0.15 * slab)
    });
    // The window and the door, dark glass with reflections.
    for (cx, hw) in [(560.0, 300.0), (1150.0, 150.0)] {
        c.paint([cx - hw - 30.0, 520.0, cx + hw + 30.0, 1060.0], |x, y| rrect(x, y, cx, 790.0, hw + 26.0, 270.0, 4.0), |_, _, _| rgb(46, 52, 50));
        c.paint([cx - hw, 540.0, cx + hw, 1050.0], |x, y| rrect(x, y, cx, 795.0, hw, 245.0, 2.0), |x, y, _| {
            let streak = smooth(40.0, 0.0, ((x + y * 0.6) / 260.0).fract() * 260.0 - 120.0).max(0.0);
            mix(rgb(30, 40, 46), rgb(120, 140, 150), 0.12 + 0.25 * streak + 0.2 * (1.0 - (y - 540.0) / 510.0))
        });
    }
    // The sign board and its lamps.
    let board = [(250.0, 220.0), (1360.0, 240.0), (1360.0, 470.0), (250.0, 490.0)];
    c.shadow(bbox(&board, 60.0), |x, y| poly(x - 6.0, y - 24.0, &board), 30.0, 0.45);
    c.paint(bbox(&board, 4.0), |x, y| poly(x, y, &board), |x, y, d| {
        let light = 0.95 + 0.06 * smooth(200.0, 300.0, y) - smooth(-18.0, 0.0, d) * 0.5;
        scale(rgb(246, 244, 238), light - 0.05 * (x - 800.0).abs() / 600.0)
    });
    for lx in [420.0, 800.0, 1180.0] {
        c.paint([lx - 40.0, 150.0, lx + 40.0, 230.0], |x, y| segment(x, y, (lx, 160.0), (lx, 200.0)) - 6.0, |_, _, _| rgb(40, 40, 40));
        c.paint([lx - 40.0, 180.0, lx + 40.0, 230.0], |x, y| rrect(x, y, lx, 205.0, 34.0, 12.0, 8.0), |_, y, _| scale(rgb(56, 56, 58), 1.6 - (y - 193.0) / 24.0));
    }
}

pub const SIGN: [(f32, f32); 4] = [(280.0, 244.0), (1330.0, 263.0), (1330.0, 447.0), (280.0, 466.0)];

/// A white van, side on, parked on a street.
pub fn van(c: &mut Canvas) {
    c.fill(|x, y| {
        if y < 820.0 {
            let t = y / 820.0;
            let n = fbm(x * 0.003, y * 0.006, 51, 4);
            mix(rgb(150, 190, 228), rgb(226, 236, 242), t * 0.8 + n * 0.2)
        } else {
            let n = fbm(x * 0.08, y * 0.08, 52, 3);
            scale(rgb(84, 86, 90), 0.8 + 0.3 * n)
        }
    });
    c.paint([0.0, 800.0, REF_W, 840.0], |_, y| (y - 820.0).abs() - 14.0, |_, _, _| rgb(190, 188, 182));
    let body = [
        (130.0, 950.0),
        (130.0, 430.0),
        (180.0, 330.0),
        (1100.0, 330.0),
        (1180.0, 360.0),
        (1360.0, 560.0),
        (1470.0, 620.0),
        (1490.0, 900.0),
        (1470.0, 950.0),
    ];
    c.shadow([60.0, 880.0, 1560.0, 1080.0], |x, y| ellipse(x, y, 810.0, 985.0, 720.0, 40.0), 50.0, 0.6);
    c.paint(bbox(&body, 30.0), |x, y| poly(x, y, &body) - 24.0, |x, y, d| {
        let curve = 0.92 + 0.1 * (1.0 - ((y - 640.0) / 330.0).powi(2)).max(0.0) - smooth(-30.0, 0.0, d) * 0.25;
        let sky = 0.05 * smooth(700.0, 400.0, y);
        let _ = x;
        mix(scale(rgb(244, 245, 246), curve), rgb(170, 200, 230), sky)
    });
    // The cab's windows, the side door's seam, the bumper and the lamps.
    let glass = [(1120.0, 380.0), (1170.0, 380.0), (1330.0, 560.0), (1120.0, 560.0)];
    c.paint(bbox(&glass, 20.0), |x, y| poly(x, y, &glass) - 10.0, |x, y, _| {
        mix(rgb(40, 50, 60), rgb(150, 175, 196), 0.3 * smooth(560.0, 380.0, y) + 0.1 * ((x + y) * 0.02).sin())
    });
    c.shadow([1080.0, 360.0, 1110.0, 940.0], |x, y| (x - 1095.0).abs().max((y - 650.0).abs() - 280.0) - 1.0, 5.0, 0.4);
    c.paint([1360.0, 640.0, 1500.0, 700.0], |x, y| rrect(x, y, 1460.0, 668.0, 26.0, 18.0, 8.0), |_, _, _| rgb(250, 240, 200));
    c.paint([110.0, 870.0, 1510.0, 960.0], |x, y| rrect(x, y, 810.0, 925.0, 690.0, 24.0, 12.0), |_, y, _| scale(rgb(60, 62, 66), 1.2 - (y - 900.0) / 80.0));
    c.paint([1180.0, 760.0, 1220.0, 790.0], |x, y| rrect(x, y, 1200.0, 774.0, 18.0, 6.0, 4.0), |_, _, _| rgb(70, 70, 72));
    for wx in [360.0, 1240.0] {
        c.paint([wx - 140.0, 830.0, wx + 140.0, 1050.0], |x, y| ellipse(x, y, wx, 930.0, 130.0, 130.0).max(y - 1060.0).max(-(ellipse(x, y, wx, 930.0, 150.0, 150.0).min(0.0)) - 1e9), |_, _, _| rgb(30, 30, 32));
        c.paint([wx - 140.0, 790.0, wx + 140.0, 1070.0], |x, y| ellipse(x, y, wx, 930.0, 124.0, 124.0), |x, y, _| {
            let r = (x - wx).hypot(y - 930.0);
            let tread = 0.85 + 0.15 * (((y - 930.0).atan2(x - wx) * 24.0).sin()).abs();
            if r < 70.0 {
                scale(rgb(190, 192, 196), 0.75 + 0.35 * (1.0 - r / 70.0) - 0.2 * smooth(20.0, 24.0, r) * smooth(30.0, 26.0, r))
            } else {
                scale(rgb(36, 36, 38), tread)
            }
        });
    }
}

pub const VAN: [(f32, f32); 4] = [(200.0, 400.0), (1060.0, 400.0), (1060.0, 840.0), (200.0, 840.0)];

/// Two business cards on a desk, one over the other.
pub fn cards(c: &mut Canvas) {
    c.fill(|x, y| {
        let n = fbm(x * 0.012, y * 0.012, 61, 4);
        let fine = fbm(x * 0.3, y * 0.3, 62, 2);
        scale(rgb(70, 74, 80), 0.85 + 0.15 * n + 0.06 * fine - 0.12 * ((x - 800.0).hypot(y - 600.0) / 1000.0))
    });
    let under = [(330.0, 560.0), (930.0, 420.0), (1050.0, 760.0), (440.0, 920.0)];
    c.shadow(bbox(&under, 60.0), |x, y| poly(x - 8.0, y - 10.0, &under) - 3.0, 22.0, 0.45);
    c.paint(bbox(&under, 6.0), |x, y| poly(x, y, &under) - 3.0, |x, y, _| scale(rgb(246, 246, 243), 0.86 + 0.06 * fbm(x * 0.2, y * 0.2, 63, 2)));
    c.shadow(bbox(&CARD, 60.0), |x, y| poly(x - 10.0, y - 14.0, &CARD) - 3.0, 26.0, 0.5);
    c.paint(bbox(&CARD, 6.0), |x, y| poly(x, y, &CARD) - 3.0, |x, y, _| {
        let light = 1.0 - 0.07 * ((x - 600.0).hypot(y - 300.0) / 800.0);
        scale(rgb(250, 250, 248), light * (0.98 + 0.03 * fbm(x * 0.25, y * 0.25, 64, 2)))
    });
}

pub const CARD: [(f32, f32); 4] = [(600.0, 300.0), (1240.0, 360.0), (1190.0, 720.0), (540.0, 655.0)];

/// A sheet of paper on a desk, with a pen beside it.
pub fn flyer(c: &mut Canvas) {
    wood(c, -200.0, rgb(196, 160, 118));
    c.shadow(bbox(&FLYER, 60.0), |x, y| poly(x - 12.0, y - 16.0, &FLYER), 30.0, 0.4);
    c.paint(bbox(&FLYER, 4.0), |x, y| poly(x, y, &FLYER), |x, y, _| {
        let light = 1.0 - 0.08 * ((x - 500.0).hypot(y - 100.0) / 1200.0);
        let curl = 0.05 * smooth(980.0, 1060.0, y + (x - 800.0) * 0.1);
        scale(rgb(252, 251, 248), light - curl)
    });
    let (a, b) = ((1180.0, 980.0), (1440.0, 330.0));
    c.shadow(bbox(&[a, b], 40.0), |x, y| segment(x, y, (a.0 + 12.0, a.1 + 14.0), (b.0 + 12.0, b.1 + 14.0)) - 12.0, 16.0, 0.35);
    c.paint(bbox(&[a, b], 20.0), |x, y| segment(x, y, a, b) - 12.0, |_, _, d| scale(rgb(24, 40, 90), 0.8 + 0.6 * smooth(-12.0, -2.0, -d) * 0.0 + 0.5 * smooth(-4.0, -12.0, d)));
}

pub const FLYER: [(f32, f32); 4] = [(520.0, 90.0), (1110.0, 130.0), (1080.0, 1060.0), (430.0, 1020.0)];

/// A flag waving on a pole against the sky.
pub fn flag(c: &mut Canvas) {
    c.fill(|x, y| {
        let n = fbm(x * 0.002 + 3.0, y * 0.004, 71, 5);
        let cloud = smooth(0.5, 0.8, n);
        mix(mix(rgb(70, 130, 200), rgb(170, 200, 230), y / REF_H), rgb(250, 250, 252), cloud * 0.85)
    });
    c.paint([280.0, 60.0, 340.0, REF_H], |x, _| (x - 310.0).abs() - 12.0, |x, _, _| scale(rgb(200, 202, 206), 0.6 + 0.6 * smooth(322.0, 300.0, x)));
    c.paint([280.0, 40.0, 340.0, 100.0], |x, y| ellipse(x, y, 310.0, 70.0, 22.0, 22.0), |_, y, _| scale(rgb(220, 190, 90), 1.3 - (y - 48.0) / 44.0));
    let edge = |u: f32| flag_wave(u);
    let sd = |x: f32, y: f32| {
        let u = ((x - 322.0) / 1000.0).clamp(0.0, 1.0);
        let w = edge(u);
        let (t, b) = (130.0 + w, 760.0 + w);
        (y - b).max(t - y).max(322.0 - x).max(x - 1322.0 + 30.0 * u * 0.0)
    };
    c.shadow([300.0, 100.0, 1400.0, 820.0], |x, y| sd(x, y), 2.0, 0.0);
    c.paint([310.0, 90.0, 1340.0, 840.0], sd, |x, y, _| {
        let u = ((x - 322.0) / 1000.0).clamp(0.0, 1.0);
        let slope = flag_slope(u);
        let s = 0.92 - slope * 0.55 + 0.04 * ((y - 130.0) / 630.0);
        scale(rgb(250, 250, 250), s * weave(x, y))
    });
}

/// How far down the flag's cloth is at `u` along it (0 at the pole).
pub fn flag_wave(u: f32) -> f32 {
    38.0 * u * (u * 1.3 * std::f32::consts::TAU).sin()
}

fn flag_slope(u: f32) -> f32 {
    let e = 0.002;
    ((flag_wave(u + e) - flag_wave(u - e)) / (2.0 * e * 1000.0)).clamp(-1.0, 1.0)
}

pub const FLAG: [(f32, f32); 4] = [(322.0, 130.0), (1322.0, 130.0), (1322.0, 760.0), (322.0, 760.0)];

/// A phone in a white case, back up, on a table.
pub fn phone(c: &mut Canvas) {
    studio(c, rgb(246, 222, 206), rgb(226, 196, 180));
    let sd = |x: f32, y: f32| rrect(x, y, 800.0, 600.0, 230.0, 470.0, 70.0);
    c.shadow([500.0, 80.0, 1100.0, 1150.0], |x, y| sd(x - 18.0, y - 24.0), 50.0, 0.4);
    c.paint([560.0, 120.0, 1040.0, 1080.0], sd, |x, y, d| {
        let edge = smooth(-22.0, 0.0, d);
        let s = 0.97 - 0.06 * ((x - 650.0).hypot(y - 250.0) / 900.0) - edge * 0.2 + edge * 0.12 * smooth(560.0, 600.0, x);
        scale(rgb(250, 250, 250), s)
    });
    c.paint([600.0, 170.0, 800.0, 400.0], |x, y| rrect(x, y, 690.0, 270.0, 80.0, 90.0, 34.0), |_, _, _| rgb(36, 36, 40));
    for (lx, ly) in [(660.0, 230.0), (660.0, 310.0), (722.0, 270.0)] {
        c.paint([lx - 30.0, ly - 30.0, lx + 30.0, ly + 30.0], |x, y| ellipse(x, y, lx, ly, 24.0, 24.0), |x, y, _| {
            mix(rgb(16, 18, 24), rgb(80, 90, 120), smooth(18.0, 0.0, (x - lx + 8.0).hypot(y - ly + 8.0)))
        });
    }
}

pub const PHONE: [(f32, f32); 4] = [(615.0, 420.0), (985.0, 420.0), (985.0, 1000.0), (615.0, 1000.0)];

/// A cardboard box seen from above and the left: its front, a side and
/// the top.
pub fn product_box(c: &mut Canvas) {
    studio(c, rgb(232, 234, 238), rgb(204, 206, 212));
    let front = BOX_FRONT;
    let side = [front[1], (1260.0, 270.0), (1260.0, 880.0), front[2]];
    let top = [front[0], (560.0, 200.0), (1260.0, 270.0), front[1]];
    let all = [front[0], top[1], side[1], side[2], front[2], front[3]];
    c.shadow(bbox(&all, 120.0), |x, y| poly(x - 40.0, y - 10.0, &all), 80.0, 0.4);
    let board = rgb(236, 232, 224);
    c.paint(bbox(&side, 4.0), |x, y| poly(x, y, &side), |x, y, _| scale(board, 0.7 + 0.03 * fbm(x * 0.2, y * 0.2, 81, 2)));
    c.paint(bbox(&top, 4.0), |x, y| poly(x, y, &top), |x, y, _| scale(board, 1.02 + 0.02 * fbm(x * 0.2, y * 0.2, 82, 2)));
    c.paint(bbox(&front, 4.0), |x, y| poly(x, y, &front), |x, y, _| {
        let light = 0.93 - 0.05 * ((x - 340.0) / 760.0);
        scale(board, light + 0.03 * fbm(x * 0.2, y * 0.2, 83, 2))
    });
    for (a, b) in [(front[0], front[1]), (front[1], front[2])] {
        c.paint(bbox(&[a, b], 6.0), |x, y| segment(x, y, a, b) - 1.2, |_, _, _| scale(board, 1.05));
    }
}

pub const BOX_FRONT: [(f32, f32); 4] = [(340.0, 330.0), (1100.0, 380.0), (1100.0, 1010.0), (340.0, 1000.0)];
