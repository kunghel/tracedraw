//! The scatter brush inside a picture being edited (Bitmaps > Edit
//! Inside): along a drag it scatters little pictures (dots, stars,
//! bubbles, leaves, confetti, sparkles) round the pointer, each of its
//! own size, turn and shade of the brush colour. See
//! `docs/behavior/edit-inside-picture.md`.

use crate::pixel_edit::{over, union, BrushOpts, Dirty, PixelImage};


/// What the sprayer scatters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SprayShape {
    Dots,
    Stars,
    Bubbles,
    Leaves,
    Confetti,
    Sparkles,
}

impl SprayShape {
    pub const ALL: [SprayShape; 6] = [
        SprayShape::Dots,
        SprayShape::Stars,
        SprayShape::Bubbles,
        SprayShape::Leaves,
        SprayShape::Confetti,
        SprayShape::Sparkles,
    ];

    pub fn key(self) -> &'static str {
        match self {
            SprayShape::Dots => "pixel.spray.dots",
            SprayShape::Stars => "pixel.spray.stars",
            SprayShape::Bubbles => "pixel.spray.bubbles",
            SprayShape::Leaves => "pixel.spray.leaves",
            SprayShape::Confetti => "pixel.spray.confetti",
            SprayShape::Sparkles => "pixel.spray.sparkles",
        }
    }
}

/// The sprayer's settings (the spray's width is the brush size).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SprayOpts {
    pub shape: SprayShape,
    /// One little picture's size, pixels.
    pub item: f32,
    /// How many each step of the drag.
    pub count: u32,
    /// How much the sizes differ, 0 to 1.
    pub size_jitter: f32,
    /// How far the colours stray from the brush colour, 0 to 1.
    pub hue_jitter: f32,
    /// Each little picture turned at random.
    pub rotate: bool,
}

impl Default for SprayOpts {
    fn default() -> Self {
        SprayOpts {
            shape: SprayShape::Dots,
            item: 12.0,
            count: 4,
            size_jitter: 0.5,
            hue_jitter: 0.15,
            rotate: true,
        }
    }
}

/// One drag of the sprayer.
pub struct Spray {
    opts: BrushOpts,
    last: Option<(f32, f32)>,
    seed: u32,
}

impl Spray {
    pub fn new(opts: BrushOpts) -> Self {
        Spray {
            opts,
            last: None,
            seed: 0x9E37_79B9,
        }
    }

    /// A random number from 0 to 1 (the same each drag).
    fn rand(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        self.seed as f32 / u32::MAX as f32
    }

    /// The sprayer taken to (x, y): a burst every step of half the
    /// spray's width.
    pub fn to(&mut self, pi: &mut PixelImage, x: f32, y: f32) -> Option<Dirty> {
        let step = (self.opts.size * 0.5).max(2.0);
        match self.last {
            None => {
                self.last = Some((x, y));
                self.burst(pi, x, y)
            }
            Some((lx, ly)) => {
                let d = (x - lx).hypot(y - ly);
                if d < step {
                    return None;
                }
                let n = (d / step).floor() as usize;
                let mut b = None;
                for i in 1..=n {
                    let t = i as f32 * step / d;
                    if let Some(r) = self.burst(pi, lx + (x - lx) * t, ly + (y - ly) * t) {
                        b = Some(union(b, r));
                    }
                }
                let t = n as f32 * step / d;
                self.last = Some((lx + (x - lx) * t, ly + (y - ly) * t));
                b
            }
        }
    }

    fn burst(&mut self, pi: &mut PixelImage, cx: f32, cy: f32) -> Option<Dirty> {
        let s = self.opts.spray;
        let mut b = None;
        for _ in 0..s.count.max(1) {
            // Spread evenly over the round spray.
            let r = self.opts.size * 0.5 * self.rand().sqrt();
            let a = self.rand() * std::f32::consts::TAU;
            let size = (s.item * (1.0 - s.size_jitter.clamp(0.0, 1.0) * self.rand())).max(1.0);
            let turn = if s.rotate {
                self.rand() * std::f32::consts::TAU
            } else {
                0.0
            };
            let shade = (self.rand() * 2.0 - 1.0) * s.hue_jitter.clamp(0.0, 1.0);
            let c = shifted(self.opts.color, shade, s.shape);
            if let Some(d) = stamp(pi, cx + r * a.cos(), cy + r * a.sin(), size, turn, c, s.shape, self.opts.opacity) {
                b = Some(union(b, d));
            }
        }
        b
    }
}

/// The colour with its hue turned by `shade` of a full turn (confetti
/// takes any hue).
fn shifted(c: [u8; 4], shade: f32, shape: SprayShape) -> [u8; 4] {
    if shade == 0.0 {
        return c;
    }
    let f = |v: u8| v as f32 / 255.0;
    let (h, s, l) = crate::fx::util::rgb_to_hsl(f(c[0]), f(c[1]), f(c[2]));
    let turn = if shape == SprayShape::Confetti {
        shade * 3.0
    } else {
        shade
    };
    let h = (h + turn * 360.0).rem_euclid(360.0);
    let (s, l) = if shape == SprayShape::Confetti {
        (s.max(0.7), l.clamp(0.4, 0.6))
    } else {
        (s, l)
    };
    let (r, g, b) = crate::fx::util::hsl_to_rgb(h, s, l);
    let b8 = |v: f32| (v * 255.0).round().clamp(0.0, 255.0) as u8;
    let (r, g, b) = (b8(r), b8(g), b8(b));
    [r, g, b, c[3]]
}

/// How much of the point (u, v) (in the little picture's own space, -1
/// to 1) the shape covers, with a soft edge `aa` wide.
fn cover(shape: SprayShape, u: f32, v: f32, aa: f32) -> f32 {
    let edge = |d: f32| (0.5 - d / aa).clamp(0.0, 1.0);
    match shape {
        SprayShape::Dots => edge(u.hypot(v) - 1.0),
        SprayShape::Bubbles => {
            let d = u.hypot(v);
            let ring = edge((d - 0.85).abs() - 0.15);
            // A shine at the top left.
            let shine = edge((u + 0.35).hypot(v + 0.35) - 0.2);
            (ring + shine * 0.8).min(1.0)
        }
        SprayShape::Stars => {
            // Five points: the radius swings between 0.45 and 1.
            let a = v.atan2(u);
            let k = (a * 5.0 / 2.0).cos().abs();
            let r = 0.45 + 0.55 * k.powf(3.0);
            edge(u.hypot(v) - r)
        }
        SprayShape::Leaves => {
            // Two arcs meeting at the tips, along u.
            let w = 0.5 * (1.0 - u * u).max(0.0).sqrt();
            edge(v.abs() - w).min(edge(u.abs() - 1.0))
        }
        SprayShape::Confetti => edge(u.abs().max(v.abs() * 2.0) - 0.9),
        SprayShape::Sparkles => {
            let arm = |a: f32, b: f32| edge(b.abs() - 0.12 * (1.0 - a.abs()).max(0.0) * 2.0);
            (arm(u, v).max(arm(v, u))).min(edge(u.hypot(v) - 1.0))
        }
    }
}

/// One little picture at (cx, cy), `size` pixels across, turned by
/// `turn` radians, painted over the layer inside the selection.
#[allow(clippy::too_many_arguments)]
fn stamp(
    pi: &mut PixelImage,
    cx: f32,
    cy: f32,
    size: f32,
    turn: f32,
    color: [u8; 4],
    shape: SprayShape,
    opacity: f32,
) -> Option<Dirty> {
    let (w, h) = pi.img.dimensions();
    let r = size * 0.5;
    let x0 = (cx - r - 1.0).floor().max(0.0) as u32;
    let y0 = (cy - r - 1.0).floor().max(0.0) as u32;
    let x1 = ((cx + r + 1.0).ceil().max(0.0) as u32).min(w);
    let y1 = ((cy + r + 1.0).ceil().max(0.0) as u32).min(h);
    if x0 >= x1 || y0 >= y1 {
        return None;
    }
    let (sin, cos) = turn.sin_cos();
    let aa = 2.0 / size.max(1.0);
    for y in y0..y1 {
        for x in x0..x1 {
            let (dx, dy) = ((x as f32 + 0.5 - cx) / r, (y as f32 + 0.5 - cy) / r);
            let (u, v) = (dx * cos + dy * sin, -dx * sin + dy * cos);
            let k = cover(shape, u, v, aa) * pi.sel_at(x, y) * opacity.clamp(0.0, 1.0);
            if k <= 0.0 {
                continue;
            }
            let q = *pi.img.get_pixel(x, y);
            pi.img.put_pixel(x, y, over(&q, color, k));
        }
    }
    Some([x0, y0, x1, y1])
}

/// How many pixels the sprayer painted (for tests).
#[cfg(test)]
fn painted(pi: &PixelImage) -> usize {
    pi.img.pixels().filter(|q| q[3] > 0).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shape_sprays_something_inside_the_spray() {
        for shape in SprayShape::ALL {
            let mut pi = PixelImage::new(image::RgbaImage::new(100, 100));
            let mut o = BrushOpts {
                size: 40.0,
                color: [200, 30, 30, 255],
                ..Default::default()
            };
            o.spray.shape = shape;
            o.spray.item = 10.0;
            let mut s = Spray::new(o);
            assert!(s.to(&mut pi, 50.0, 50.0).is_some(), "{shape:?}");
            s.to(&mut pi, 80.0, 50.0);
            assert!(painted(&pi) > 10, "{shape:?}");
            // Nothing far from the drag.
            assert_eq!(pi.img.get_pixel(5, 5)[3], 0, "{shape:?}");
        }
    }

    #[test]
    fn the_spray_stays_inside_the_selection() {
        let mut pi = PixelImage::new(image::RgbaImage::new(60, 60));
        pi.sel = Some(image::GrayImage::new(60, 60));
        let mut s = Spray::new(BrushOpts::default());
        s.to(&mut pi, 30.0, 30.0);
        assert_eq!(painted(&pi), 0);
    }
}
