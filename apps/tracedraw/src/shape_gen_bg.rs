//! Shape Generator backgrounds: layered waves, organic blobs and a soft
//! gradient, coloured from the panel's palette.
//!
//! - Layered waves: filled bands stacked from the back to the front, each
//!   top edge a sum of a few seeded sine waves, smooth curves down to the
//!   bottom (or up to the top) of the box.
//! - Blobs: smooth closed shapes through points round a circle whose
//!   distances vary at random; one in the middle or several scattered.
//! - Soft gradient: the box as one rectangle with a smooth mesh fill whose
//!   nodes take palette colours at random and move off the grid a little,
//!   so the colours melt into each other.
//!
//! Colours: [`Paint::Tone`] carries a share (0 to 255) along the palette,
//! mixed between its neighbouring colours; [`Generated::palette`] holds
//! the palette.
//!
//! Model space: mm, y up.

use crate::shape_gen::{smooth_through, Generated, Paint, Rng, ShapeGenSettings};
use tracedraw_core::geometry::{BezPath, Point, Rect};
use tracedraw_core::{Color, Mesh};

/// The most colours in the palette.
pub const MAX_COLORS: usize = 6;

/// The default palette: deep blue to warm peach.
pub fn default_palette() -> Vec<[u8; 3]> {
    vec![[24, 32, 84], [92, 64, 196], [236, 96, 160], [255, 192, 128]]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaveSide {
    /// The bands rise from the bottom of the box.
    #[default]
    Bottom,
    /// The bands hang from the top.
    Top,
}

impl WaveSide {
    pub const ALL: [WaveSide; 2] = [WaveSide::Bottom, WaveSide::Top];
    pub fn key(self) -> &'static str {
        match self {
            WaveSide::Bottom => "shapegen.bg_side_bottom",
            WaveSide::Top => "shapegen.bg_side_top",
        }
    }
}

/// The colour a share `t` (0 to 1) along the palette, mixed between its
/// neighbours.
pub fn palette_color(palette: &[[u8; 3]], t: f64) -> [u8; 3] {
    match palette.len() {
        0 => [0, 0, 0],
        1 => palette[0],
        n => {
            let x = t.clamp(0.0, 1.0) * (n - 1) as f64;
            let i = (x.floor() as usize).min(n - 2);
            let f = x - i as f64;
            let (a, b) = (palette[i], palette[i + 1]);
            let mix = |a: u8, b: u8| (a as f64 + (b as f64 - a as f64) * f).round() as u8;
            [mix(a[0], b[0]), mix(a[1], b[1]), mix(a[2], b[2])]
        }
    }
}

/// The paint for the `i`th of `n` shapes, from the first colour to the
/// last.
fn tone(i: usize, n: usize) -> Paint {
    if n <= 1 {
        return Paint::Tone(0);
    }
    Paint::Tone(((i as f64 / (n - 1) as f64) * 255.0).round() as u8)
}

/// Layered waves.
pub(crate) fn waves(s: &ShapeGenSettings, out: &mut Generated) {
    out.palette = s.bg_colors.clone();
    let (w, h) = (s.width, s.height);
    out.frame = Some(Rect::new(0.0, 0.0, w, h));
    let n = s.bg_layers.max(1) as usize;
    let amp = s.bg_amplitude / 100.0 * h;
    let terms = s.bg_complexity.clamp(1, 6) as usize;
    let samples = 96usize;
    let mut rng = Rng::new(s.seed, 0xba5e);
    for i in 0..n {
        // The back band fills the box, so no paper shows behind; each one
        // in front reaches a step less.
        if i == 0 && n > 1 {
            let rect = crate::shape_gen::polygon(&[
                Point::new(0.0, 0.0),
                Point::new(w, 0.0),
                Point::new(w, h),
                Point::new(0.0, h),
            ]);
            out.push(rect, true, tone(0, n));
            continue;
        }
        let level = h * (n - i) as f64 / n as f64;
        // Each band its own waves: frequencies near the setting, phases
        // and weights at random, the weights adding up to one.
        let mut parts = Vec::with_capacity(terms);
        let mut total = 0.0;
        for k in 0..terms {
            let f = s.bg_frequency * (k as f64 + 1.0) * (0.75 + 0.5 * rng.unit());
            let ph = rng.unit() * std::f64::consts::TAU;
            let wgt = 1.0 / (k as f64 + 1.0) * (0.5 + rng.unit());
            total += wgt;
            parts.push((f, ph, wgt));
        }
        let edge: Vec<Point> = (0..=samples)
            .map(|j| {
                let x = w * j as f64 / samples as f64;
                let u = x / w.max(1e-9) * std::f64::consts::TAU;
                let y: f64 = parts
                    .iter()
                    .map(|(f, ph, wgt)| wgt / total * (u * f + ph).sin())
                    .sum();
                let y = (level + amp * y).clamp(0.0, h);
                let y = match s.bg_side {
                    WaveSide::Bottom => y,
                    WaveSide::Top => h - y,
                };
                Point::new(x, y)
            })
            .collect();
        let mut path = smooth_through(&edge, false);
        let base = match s.bg_side {
            WaveSide::Bottom => 0.0,
            WaveSide::Top => h,
        };
        path.line_to(Point::new(w, base));
        path.line_to(Point::new(0.0, base));
        path.close_path();
        out.push(path, true, tone(i, n));
    }
}

/// One blob: a smooth closed curve through `points` points round
/// `centre`, each at a radius between (1 - randomness) and 1 of `r`.
fn blob(rng: &mut Rng, centre: Point, r: f64, points: usize, randomness: f64) -> BezPath {
    let n = points.max(3);
    let turn = rng.unit() * std::f64::consts::TAU;
    let pts: Vec<Point> = (0..n)
        .map(|k| {
            let a = turn
                + std::f64::consts::TAU * (k as f64 + (rng.unit() - 0.5) * 0.4 * randomness)
                    / n as f64;
            let d = r * (1.0 - randomness * rng.unit());
            Point::new(centre.x + d * a.cos(), centre.y + d * a.sin())
        })
        .collect();
    smooth_through(&pts, true)
}

/// Organic blobs.
pub(crate) fn blobs(s: &ShapeGenSettings, out: &mut Generated) {
    out.palette = s.bg_colors.clone();
    let (w, h) = (s.width, s.height);
    out.frame = Some(Rect::new(0.0, 0.0, w, h));
    let n = s.bg_blob_count.max(1) as usize;
    let randomness = s.bg_randomness / 100.0;
    let points = s.bg_blob_points as usize;
    let base = s.bg_blob_size / 100.0 * w.min(h) / 2.0;
    let mut rng = Rng::new(s.seed, 0xb10b);
    for i in 0..n {
        if out.full() {
            out.cut = true;
            return;
        }
        let (centre, r) = if n == 1 {
            (Point::new(w / 2.0, h / 2.0), base)
        } else {
            let r = base * (0.45 + 0.55 * rng.unit());
            (Point::new(rng.unit() * w, rng.unit() * h), r)
        };
        out.push(
            blob(&mut rng, centre, r, points, randomness),
            true,
            tone(i, n),
        );
    }
}

/// The soft gradient: the box with a smooth mesh of palette colours.
pub(crate) fn soft_gradient(s: &ShapeGenSettings, out: &mut Generated) {
    out.palette = s.bg_colors.clone();
    let (w, h) = (s.width, s.height);
    let rect = Rect::new(0.0, 0.0, w, h);
    out.frame = Some(rect);
    let size = s.bg_mesh_size.clamp(1, 8);
    let mut mesh = Mesh::new(rect, size, size, Color::BLACK);
    let mut rng = Rng::new(s.seed, 0x50f7);
    let (cw, ch) = (w / size as f64, h / size as f64);
    let jitter = s.bg_jitter / 100.0 * 0.45;
    let colors = if s.bg_colors.is_empty() {
        default_palette()
    } else {
        s.bg_colors.clone()
    };
    for r in 0..=size {
        for c in 0..=size {
            let node = &mut mesh.nodes[(r * (size + 1) + c) as usize];
            let [cr, cg, cb] = colors[rng.below(colors.len() as u32) as usize];
            node.color = Color::rgb8(cr, cg, cb);
            // Inner nodes wander; the edge nodes stay on the box.
            let dx = (rng.unit() * 2.0 - 1.0) * jitter * cw;
            let dy = (rng.unit() * 2.0 - 1.0) * jitter * ch;
            if c > 0 && c < size {
                node.pos.x += dx;
            }
            if r > 0 && r < size {
                node.pos.y += dy;
            }
        }
    }
    mesh.smooth = true;
    let path = crate::shape_gen::polygon(&[
        Point::new(0.0, 0.0),
        Point::new(w, 0.0),
        Point::new(w, h),
        Point::new(0.0, h),
    ]);
    out.mesh = Some(mesh);
    out.push(path, true, Paint::Mesh);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape_gen::{generate, GenKind};
    use tracedraw_core::geometry::Shape as _;

    fn bg(kind: GenKind) -> ShapeGenSettings {
        ShapeGenSettings {
            kind,
            width: 200.0,
            height: 100.0,
            ..ShapeGenSettings::default()
        }
    }

    #[test]
    fn palette_colours_mix_between_neighbours() {
        let p = [[0, 0, 0], [200, 100, 0], [200, 100, 200]];
        assert_eq!(palette_color(&p, 0.0), [0, 0, 0]);
        assert_eq!(palette_color(&p, 0.25), [100, 50, 0]);
        assert_eq!(palette_color(&p, 0.5), [200, 100, 0]);
        assert_eq!(palette_color(&p, 1.0), [200, 100, 200]);
        assert_eq!(palette_color(&p[..1], 0.7), [0, 0, 0]);
    }

    #[test]
    fn waves_stack_bands_from_the_back_coloured_along_the_palette() {
        let s = ShapeGenSettings {
            bg_layers: 4,
            ..bg(GenKind::LayerWaves)
        };
        let g = generate(&s);
        assert_eq!(g.items.len(), 4);
        assert!(g.items.iter().all(|i| i.closed));
        assert_eq!(g.items[0].paint, Paint::Tone(0));
        assert_eq!(g.items[3].paint, Paint::Tone(255));
        // Each band inside the box, the back one the tallest.
        let tops: Vec<f64> = g.items.iter().map(|i| i.path.bounding_box().y1).collect();
        for i in &g.items {
            let b = i.path.bounding_box();
            assert!(
                b.x0 >= -1e-6 && b.x1 <= 200.0 + 1e-6 && b.y0 >= -1e-6,
                "{b:?}"
            );
        }
        assert!(tops[0] > tops[3], "{tops:?}");
        // The same seed draws the same waves, another seed others.
        assert_eq!(generate(&s).items[1].path, g.items[1].path);
        let other = generate(&ShapeGenSettings {
            seed: 9,
            ..s.clone()
        });
        assert_ne!(other.items[1].path, g.items[1].path);
        // Hanging from the top: the bands reach the top edge.
        let top = generate(&ShapeGenSettings {
            bg_side: WaveSide::Top,
            ..s
        });
        assert!(top
            .items
            .iter()
            .all(|i| (i.path.bounding_box().y1 - 100.0).abs() < 1e-6));
    }

    #[test]
    fn blobs_are_smooth_closed_shapes_in_the_box() {
        let one = generate(&ShapeGenSettings {
            bg_blob_count: 1,
            bg_blob_size: 80.0,
            ..bg(GenKind::Blobs)
        });
        assert_eq!(one.items.len(), 1);
        let b = one.items[0].path.bounding_box();
        // Round the middle, at most the size across.
        assert!((b.center().x - 100.0).abs() < 25.0 && (b.center().y - 50.0).abs() < 25.0);
        assert!(
            b.width() <= 80.0 + 1e-6 && b.height() <= 80.0 + 1e-6,
            "{b:?}"
        );
        assert!(one.items[0]
            .path
            .elements()
            .iter()
            .any(|e| matches!(e, tracedraw_core::geometry::PathEl::CurveTo(..))));
        let many = generate(&ShapeGenSettings {
            bg_blob_count: 6,
            ..bg(GenKind::Blobs)
        });
        assert_eq!(many.items.len(), 6);
        assert_eq!(many.items[5].paint, Paint::Tone(255));
    }

    #[test]
    fn the_soft_gradient_is_a_box_with_a_smooth_mesh_of_palette_colours() {
        let s = ShapeGenSettings {
            bg_mesh_size: 3,
            bg_colors: vec![[255, 0, 0], [0, 0, 255]],
            ..bg(GenKind::SoftGradient)
        };
        let g = generate(&s);
        assert_eq!(g.items.len(), 1);
        assert_eq!(g.items[0].paint, Paint::Mesh);
        let m = g.mesh.as_ref().expect("mesh");
        assert_eq!((m.rows, m.cols, m.nodes.len()), (3, 3, 16));
        assert!(m.smooth);
        let red = Color::rgb8(255, 0, 0);
        let blue = Color::rgb8(0, 0, 255);
        assert!(m.nodes.iter().all(|n| n.color == red || n.color == blue));
        // Corners stay on the box.
        assert_eq!(m.nodes[0].pos, Point::new(0.0, 0.0));
        assert_eq!(m.nodes[15].pos, Point::new(200.0, 100.0));
    }

    #[test]
    fn create_makes_tone_fills_and_a_mesh_filled_box() {
        use tracedraw_core::{Fill, ShapeKind};
        let mut app = crate::app::App::headless();
        app.shape_gen = ShapeGenSettings {
            bg_layers: 3,
            bg_colors: vec![[10, 20, 30], [250, 240, 230]],
            ..bg(GenKind::LayerWaves)
        };
        let gid = app.shape_gen_create().expect("made");
        let g = app.doc().find_shape(gid).expect("group").clone();
        let ShapeKind::Group { children } = &g.kind else {
            panic!("not a group");
        };
        assert_eq!(children.len(), 3);
        assert_eq!(children[0].fill, Fill::Solid(Color::rgb8(10, 20, 30)));
        assert_eq!(children[2].fill, Fill::Solid(Color::rgb8(250, 240, 230)));
        assert!(children.iter().all(|c| c.stroke.is_none()));
        app.shape_gen.kind = GenKind::SoftGradient;
        let gid = app.shape_gen_create().expect("made");
        let g = app.doc().find_shape(gid).expect("group").clone();
        let ShapeKind::Group { children } = &g.kind else {
            panic!("not a group");
        };
        let Fill::Mesh(m) = &children[0].fill else {
            panic!("no mesh: {:?}", children[0].fill);
        };
        // The mesh moved with the box onto the page.
        let b = children[0].bounds();
        let p = children[0].transform * m.nodes[0].pos;
        assert!(
            (p.x - b.x0).abs() < 1e-6 && (p.y - b.y0).abs() < 1e-6,
            "{p:?} {b:?}"
        );
    }
}
