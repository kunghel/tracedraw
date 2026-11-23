//! The live Halftone effect: a finished picture of an object (premultiplied
//! RGBA) turned into a screen of dots in place. Each cell of a turned grid
//! takes the mean colour of the pixels in it, and its dot grows with the
//! tone: red, green and blue light dots (RGB), cyan, magenta, yellow and
//! black ink dots on white (CMYK), or dots of one colour as large as the
//! darkness. Dot areas follow the tone exactly until they touch.

use tiny_skia::Pixmap;

/// The halftone's settings in the picture's pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Params {
    /// 0 round, 1 square, 2 diamond, 3 line.
    pub pattern: u8,
    /// 0 RGB, 1 CMYK, 2 one colour.
    pub mode: u8,
    /// Distance between dots, pixels.
    pub cell: f64,
    /// 1: a full tone covers its cell.
    pub scale: f64,
    /// Edge softness, 0 to 1 of a cell.
    pub softness: f64,
    /// Screen angle in the picture, radians (y down).
    pub angle: f64,
    /// A dot centre, pixels.
    pub origin: (f64, f64),
    /// Cut the dots by the picture's own alpha.
    pub clip: bool,
    /// One colour mode: the dots' colour.
    pub color: [u8; 3],
}

/// A turned grid over the picture with each cell's mean premultiplied
/// colour and alpha.
struct Grid {
    cos: f64,
    sin: f64,
    cell: f64,
    origin: (f64, f64),
    i0: i64,
    j0: i64,
    ni: usize,
    nj: usize,
    /// Per cell: mean r, g, b (premultiplied) and a, 0 to 1.
    mean: Vec<[f32; 4]>,
}

impl Grid {
    fn new(data: &[u8], w: usize, h: usize, cell: f64, angle: f64, origin: (f64, f64)) -> Grid {
        let (sin, cos) = angle.sin_cos();
        let mut g = Grid {
            cos,
            sin,
            cell,
            origin,
            i0: 0,
            j0: 0,
            ni: 0,
            nj: 0,
            mean: Vec::new(),
        };
        let corners = [
            (0.0, 0.0),
            (w as f64, 0.0),
            (0.0, h as f64),
            (w as f64, h as f64),
        ];
        let (mut umin, mut umax, mut vmin, mut vmax) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
        for (x, y) in corners {
            let (u, v) = g.uv(x, y);
            umin = umin.min(u);
            umax = umax.max(u);
            vmin = vmin.min(v);
            vmax = vmax.max(v);
        }
        g.i0 = umin.floor() as i64 - 1;
        g.j0 = vmin.floor() as i64 - 1;
        g.ni = (umax.floor() as i64 - g.i0 + 2).max(1) as usize;
        g.nj = (vmax.floor() as i64 - g.j0 + 2).max(1) as usize;
        let mut sum = vec![[0f64; 5]; g.ni * g.nj];
        for y in 0..h {
            for x in 0..w {
                let k = g.cell_of(x as f64 + 0.5, y as f64 + 0.5).0;
                let p = &data[(y * w + x) * 4..(y * w + x) * 4 + 4];
                let s = &mut sum[k];
                for c in 0..4 {
                    s[c] += f64::from(p[c]);
                }
                s[4] += 1.0;
            }
        }
        g.mean = sum
            .iter()
            .map(|s| {
                if s[4] == 0.0 {
                    [0.0; 4]
                } else {
                    let n = s[4] * 255.0;
                    [
                        (s[0] / n) as f32,
                        (s[1] / n) as f32,
                        (s[2] / n) as f32,
                        (s[3] / n) as f32,
                    ]
                }
            })
            .collect();
        g
    }

    /// Grid coordinates (cells) of a picture point.
    fn uv(&self, x: f64, y: f64) -> (f64, f64) {
        let (dx, dy) = (x - self.origin.0, y - self.origin.1);
        (
            (dx * self.cos + dy * self.sin) / self.cell + 0.5,
            (-dx * self.sin + dy * self.cos) / self.cell + 0.5,
        )
    }

    /// The cell a picture point lies in and the point's place in it
    /// (-0.5 to 0.5 each way from the dot's centre).
    fn cell_of(&self, x: f64, y: f64) -> (usize, f64, f64) {
        let (u, v) = self.uv(x, y);
        let (fi, fj) = (u.floor(), v.floor());
        let i = (fi as i64 - self.i0).clamp(0, self.ni as i64 - 1) as usize;
        let j = (fj as i64 - self.j0).clamp(0, self.nj as i64 - 1) as usize;
        (j * self.ni + i, u - fi - 0.5, v - fj - 0.5)
    }
}

/// How much of a pixel at (`fu`, `fv`) in its cell a dot of `tone` (0 to
/// 1) covers; `soft` is the edge width in cells.
fn dot(pattern: u8, tone: f64, scale: f64, fu: f64, fv: f64, soft: f64) -> f64 {
    if tone <= 0.0 || scale <= 0.0 {
        return 0.0;
    }
    let t = tone.min(1.0);
    let d = match pattern {
        1 => fu.abs().max(fv.abs()) - scale * t.sqrt() / 2.0,
        2 => fu.abs() + fv.abs() - scale * (t / 2.0).sqrt(),
        3 => fv.abs() - scale * t / 2.0,
        _ => fu.hypot(fv) - scale * (t / std::f64::consts::PI).sqrt(),
    };
    (0.5 - d / soft).clamp(0.0, 1.0)
}

fn luma(r: f64, g: f64, b: f64) -> f64 {
    0.299 * r + 0.587 * g + 0.114 * b
}

/// The cell's colour without its alpha, and that alpha.
fn straight(m: [f32; 4]) -> ([f64; 3], f64) {
    let a = f64::from(m[3]);
    if a <= 1e-6 {
        return ([0.0; 3], 0.0);
    }
    (
        [
            (f64::from(m[0]) / a).min(1.0),
            (f64::from(m[1]) / a).min(1.0),
            (f64::from(m[2]) / a).min(1.0),
        ],
        a,
    )
}

/// Cyan, magenta, yellow and black of a colour.
fn cmyk(c: [f64; 3]) -> [f64; 4] {
    let k = 1.0 - c[0].max(c[1]).max(c[2]);
    if k >= 1.0 - 1e-9 {
        return [0.0, 0.0, 0.0, 1.0];
    }
    let f = |v: f64| (1.0 - v - k) / (1.0 - k);
    [f(c[0]), f(c[1]), f(c[2]), k]
}

/// Turn `pic` into its halftone in place.
pub fn halftone(pic: &mut Pixmap, p: &Params) {
    let (w, h) = (pic.width() as usize, pic.height() as usize);
    if w == 0 || h == 0 || !p.cell.is_finite() {
        return;
    }
    let cell = p.cell.max(1.5);
    let soft = (1.0 / cell).max(p.softness.clamp(0.0, 1.0));
    let scale = p.scale.clamp(0.0, 2.0);
    let data = pic.data().to_vec();
    let angles: Vec<f64> = if p.mode == 1 {
        [15f64, 75.0, 0.0, 45.0]
            .iter()
            .map(|d| p.angle - d.to_radians())
            .collect()
    } else {
        vec![p.angle]
    };
    let grids: Vec<Grid> = angles
        .iter()
        .map(|a| Grid::new(&data, w, h, cell, *a, p.origin))
        .collect();
    let out = pic.data_mut();
    let ink = [
        f64::from(p.color[0]) / 255.0,
        f64::from(p.color[1]) / 255.0,
        f64::from(p.color[2]) / 255.0,
    ];
    for y in 0..h {
        for x in 0..w {
            let at = (y * w + x) * 4;
            let own = f64::from(data[at + 3]) / 255.0;
            let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
            // Cut by the object's edge, or whole dots weighed by how much
            // of their cell the object covers.
            let px_rgba: [f64; 4] = match p.mode {
                1 => {
                    let mut cov = [0.0; 4];
                    for (n, g) in grids.iter().enumerate() {
                        let (k, fu, fv) = g.cell_of(px, py);
                        let (c, a) = straight(g.mean[k]);
                        let mut t = cmyk(c)[n];
                        if !p.clip {
                            t *= a;
                        }
                        cov[n] = dot(p.pattern, t, scale, fu, fv, soft);
                    }
                    let paper = 1.0 - cov[3];
                    [
                        (1.0 - cov[0]) * paper,
                        (1.0 - cov[1]) * paper,
                        (1.0 - cov[2]) * paper,
                        1.0,
                    ]
                }
                2 => {
                    let g = &grids[0];
                    let (k, fu, fv) = g.cell_of(px, py);
                    let (c, a) = straight(g.mean[k]);
                    let mut t = 1.0 - luma(c[0], c[1], c[2]);
                    if !p.clip {
                        t *= a;
                    }
                    let cov = dot(p.pattern, t, scale, fu, fv, soft);
                    [ink[0], ink[1], ink[2], cov]
                }
                _ => {
                    let g = &grids[0];
                    let (k, fu, fv) = g.cell_of(px, py);
                    let (c, a) = straight(g.mean[k]);
                    let weight = if p.clip { 1.0 } else { a };
                    let cov: Vec<f64> = c
                        .iter()
                        .map(|v| dot(p.pattern, v * weight, scale, fu, fv, soft))
                        .collect();
                    let alpha = cov[0].max(cov[1]).max(cov[2]);
                    if alpha <= 0.0 {
                        [0.0; 4]
                    } else {
                        [cov[0] / alpha, cov[1] / alpha, cov[2] / alpha, alpha]
                    }
                }
            };
            // Paper (CMYK) and dots end at the object's edge when cut;
            // CMYK paper always follows it.
            let a = px_rgba[3] * if p.clip || p.mode == 1 { own } else { 1.0 };
            let a = a.clamp(0.0, 1.0);
            for c in 0..3 {
                out[at + c] = (px_rgba[c].clamp(0.0, 1.0) * a * 255.0).round() as u8;
            }
            out[at + 3] = (a * 255.0).round() as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(w: u32, h: u32, rgba: [u8; 4]) -> Pixmap {
        let mut p = Pixmap::new(w, h).expect("pixmap");
        for px in p.data_mut().chunks_mut(4) {
            px.copy_from_slice(&rgba);
        }
        p
    }

    fn params(mode: u8) -> Params {
        Params {
            pattern: 0,
            mode,
            cell: 10.0,
            scale: 1.0,
            softness: 0.0,
            angle: 0.0,
            origin: (5.0, 5.0),
            clip: true,
            color: [0, 0, 0],
        }
    }

    /// Mean of each channel, 0 to 1.
    fn means(p: &Pixmap) -> [f64; 4] {
        let mut s = [0.0; 4];
        for px in p.data().chunks(4) {
            for c in 0..4 {
                s[c] += f64::from(px[c]) / 255.0;
            }
        }
        let n = (p.width() * p.height()) as f64;
        s.map(|v| v / n)
    }

    #[test]
    fn dot_areas_follow_the_tone() {
        // A grey of 25 % ink in one colour mode covers about a quarter.
        for pattern in 0..3 {
            let mut p = flat(100, 100, [191, 191, 191, 255]);
            halftone(
                &mut p,
                &Params {
                    pattern,
                    mode: 2,
                    ..params(2)
                },
            );
            let a = means(&p)[3];
            assert!((a - 0.25).abs() < 0.04, "{pattern} {a}");
        }
        // Lines: a band as wide as the tone.
        let mut p = flat(100, 100, [128, 128, 128, 255]);
        halftone(
            &mut p,
            &Params {
                pattern: 3,
                ..params(2)
            },
        );
        assert!((means(&p)[3] - 0.5).abs() < 0.05);
    }

    #[test]
    fn rgb_dots_are_light_and_keep_the_hue() {
        // Pure red: red dots only, as many as the red.
        let mut p = flat(100, 100, [255, 0, 0, 255]);
        halftone(&mut p, &params(0));
        let m = means(&p);
        assert!(m[1] < 1e-6 && m[2] < 1e-6, "{m:?}");
        assert!((m[0] - m[3]).abs() < 1e-6 && m[3] > 0.7, "{m:?}");
        // A dark red makes smaller dots.
        let mut d = flat(100, 100, [80, 0, 0, 255]);
        halftone(&mut d, &params(0));
        assert!(means(&d)[3] < m[3] * 0.5);
        // Transparent stays transparent.
        let mut t = flat(40, 40, [0, 0, 0, 0]);
        halftone(&mut t, &params(0));
        assert_eq!(means(&t), [0.0; 4]);
    }

    #[test]
    fn cmyk_prints_inks_on_white_inside_the_object() {
        // Cyan: no red light where the cyan dots are, white paper between.
        let mut p = flat(100, 100, [0, 255, 255, 255]);
        halftone(&mut p, &params(1));
        let m = means(&p);
        assert!(
            m[0] < 0.5 && m[1] > 0.99 && m[2] > 0.99 && m[3] > 0.99,
            "{m:?}"
        );
        // Half the object transparent: the paper ends at its edge.
        let mut half = Pixmap::new(100, 100).expect("pixmap");
        for (i, px) in half.data_mut().chunks_mut(4).enumerate() {
            if i % 100 < 50 {
                px.copy_from_slice(&[255, 255, 255, 255]);
            }
        }
        halftone(&mut half, &params(1));
        assert!((means(&half)[3] - 0.5).abs() < 0.01);
    }

    #[test]
    fn unclipped_dots_are_whole_and_weighed_by_cover() {
        // An object covering the left half of each cell: cut dots end at
        // its edge, whole dots reach past it.
        let mut cut = Pixmap::new(100, 100).expect("pixmap");
        for (i, px) in cut.data_mut().chunks_mut(4).enumerate() {
            if i % 10 < 5 {
                px.copy_from_slice(&[0, 0, 0, 255]);
            }
        }
        let mut whole = cut.clone();
        halftone(&mut cut, &params(2));
        halftone(
            &mut whole,
            &Params {
                clip: false,
                ..params(2)
            },
        );
        let right = |p: &Pixmap| {
            p.data()
                .chunks(4)
                .enumerate()
                .filter(|(i, _)| i % 10 >= 5)
                .map(|(_, px)| u32::from(px[3]))
                .sum::<u32>()
        };
        assert_eq!(right(&cut), 0);
        assert!(right(&whole) > 0);
    }

    #[test]
    fn the_screen_turns_and_moves_with_its_centre() {
        let mut a = flat(60, 60, [128, 128, 128, 255]);
        let mut b = a.clone();
        let mut c = a.clone();
        halftone(&mut a, &params(2));
        halftone(
            &mut b,
            &Params {
                angle: 0.5,
                ..params(2)
            },
        );
        halftone(
            &mut c,
            &Params {
                origin: (0.0, 0.0),
                ..params(2)
            },
        );
        assert_ne!(a.data(), b.data());
        assert_ne!(a.data(), c.data());
        // A dot sits at the centre.
        let at = (5 * 60 + 5) * 4 + 3;
        assert_eq!(a.data()[at], 255);
    }
}
