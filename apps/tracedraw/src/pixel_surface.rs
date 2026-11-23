//! Painting that follows the surface of the photo (Bitmaps > Edit
//! Inside): the photo's own light and shade read as a relief, so paint,
//! cloned texture or a whole layer bends along its folds and takes its
//! shadows, as on cloth or skin. See `docs/behavior/edit-inside-picture.md`.

use crate::pixel_edit::{blend_through_selection, Dirty, PixelImage};
use image::{GrayImage, Rgba, RgbaImage};

/// The relief of a picture: how light each place is next to its
/// surroundings (its shading) and how near it stands (its depth).
pub struct SurfaceMap {
    w: u32,
    h: u32,
    /// The picture's lightness, a little smoothed, 0 to 1.
    light: Vec<f32>,
    /// The lightness of the surroundings, 0 to 1.
    mean: Vec<f32>,
    /// How near each place stands, -1 (far) to 1 (near).
    depth: Vec<f32>,
}

/// A box blur of radius `r` across and down, three times (close to a
/// gaussian).
fn blur(v: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    if r == 0 || w == 0 || h == 0 {
        return v.to_vec();
    }
    let mut a = v.to_vec();
    let mut b = vec![0.0; v.len()];
    let pass =
        |src: &[f32], dst: &mut [f32], n: usize, len: usize, at: &dyn Fn(usize, usize) -> usize| {
            for line in 0..n {
                let mut sum = 0.0;
                let get = |i: i64| src[at(line, i.clamp(0, len as i64 - 1) as usize)];
                for i in -(r as i64)..=(r as i64) {
                    sum += get(i);
                }
                let k = (2 * r + 1) as f32;
                for i in 0..len {
                    dst[at(line, i)] = sum / k;
                    sum += get(i as i64 + r as i64 + 1) - get(i as i64 - r as i64);
                }
            }
        };
    for _ in 0..3 {
        pass(&a, &mut b, h, w, &|y, x| y * w + x);
        pass(&b, &mut a, w, h, &|x, y| y * w + x);
    }
    a
}

impl SurfaceMap {
    /// The relief of `seen`. A `depth` map of the same size (255 nearest)
    /// stands for the depth read from the light; the shading still comes
    /// from the picture.
    pub fn new(seen: &RgbaImage, depth: Option<&GrayImage>) -> Self {
        let (w, h) = seen.dimensions();
        let (wu, hu) = (w as usize, h as usize);
        // See-through places count as a flat middle grey.
        let raw: Vec<f32> = seen
            .pixels()
            .map(|p| {
                let a = p[3] as f32 / 255.0;
                (crate::fx::util::luma(p) / 255.0) * a + 0.5 * (1.0 - a)
            })
            .collect();
        let light = blur(&raw, wu, hu, 1);
        let spread = ((w.max(h) as f32) / 40.0).clamp(4.0, 40.0) as usize;
        let mean = blur(&raw, wu, hu, spread);
        let depth = match depth.filter(|d| d.dimensions() == (w, h)) {
            Some(d) => {
                let v: Vec<f32> = d.pixels().map(|p| p[0] as f32 / 127.5 - 1.0).collect();
                blur(&v, wu, hu, 1)
            }
            None => {
                let soft = blur(&raw, wu, hu, 2);
                soft.iter()
                    .zip(&mean)
                    .map(|(s, m)| ((s - m) * 4.0).clamp(-1.0, 1.0))
                    .collect()
            }
        };
        SurfaceMap {
            w,
            h,
            light,
            mean,
            depth,
        }
    }

    fn at(&self, v: &[f32], x: f32, y: f32) -> f32 {
        if self.w == 0 || self.h == 0 {
            return 0.0;
        }
        let xi = (x.floor().max(0.0) as u32).min(self.w - 1);
        let yi = (y.floor().max(0.0) as u32).min(self.h - 1);
        v[(yi * self.w + xi) as usize]
    }

    /// How far paint at (x, y) is pushed by the relief, `depth` pixels at
    /// most.
    pub fn offset(&self, x: f32, y: f32, depth: f32) -> (f32, f32) {
        let d = self.at(&self.depth, x, y) * depth;
        // Along the slope of the relief, so lines bend over the folds.
        let gx = self.at(&self.depth, x + 1.0, y) - self.at(&self.depth, x - 1.0, y);
        let gy = self.at(&self.depth, x, y + 1.0) - self.at(&self.depth, x, y - 1.0);
        let g = (gx * gx + gy * gy).sqrt();
        if g > 1e-4 {
            (gx / g * d, gy / g * d)
        } else {
            (0.0, 0.0)
        }
    }

    /// `c` lit as the picture is at (x, y): darker in its shadows, lighter
    /// in its highlights, by `amount` (0 none, 1 all).
    pub fn shade(&self, c: [u8; 4], x: u32, y: u32, amount: f32) -> [u8; 4] {
        let (xf, yf) = (x as f32, y as f32);
        let l = self.at(&self.light, xf, yf);
        let m = self.at(&self.mean, xf, yf).max(0.02);
        let k = 1.0 + ((l / m).clamp(0.0, 2.0) - 1.0) * amount.clamp(0.0, 1.0);
        let ch = |v: u8| (v as f32 * k).round().clamp(0.0, 255.0) as u8;
        [ch(c[0]), ch(c[1]), ch(c[2]), c[3]]
    }
}

/// Everything seen beneath the layer in use.
pub fn seen_beneath(pi: &PixelImage) -> RgbaImage {
    let mut below = pi.clone();
    for (i, l) in below.layers.iter_mut().enumerate() {
        if i >= pi.active {
            l.visible = false;
        }
    }
    below.composite()
}

/// The layer in use laid onto the surface beneath it, inside the
/// selection: bent by `depth` pixels along the relief and lit by the
/// picture by `light` (0 to 1).
pub fn fit_to_surface(
    pi: &mut PixelImage,
    map: &SurfaceMap,
    depth: f32,
    light: f32,
) -> Option<Dirty> {
    let (w, h) = pi.img.dimensions();
    if w == 0 || h == 0 || pi.active_locked() {
        return None;
    }
    let src = pi.img.clone();
    let mut out = RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let (ox, oy) = map.offset(x as f32 + 0.5, y as f32 + 0.5, depth);
            let p = crate::fx::util::bilinear(&src, x as f32 + 0.5 - ox, y as f32 + 0.5 - oy, true);
            out.put_pixel(x, y, Rgba(map.shade(p.0, x, y, light)));
        }
    }
    blend_through_selection(pi, &out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A picture with a dark fold down its middle.
    fn folded(w: u32, h: u32) -> RgbaImage {
        RgbaImage::from_fn(w, h, |x, _| {
            let d = (x as f32 - w as f32 / 2.0).abs();
            let v = if d < 3.0 { 60 } else { 200 };
            Rgba([v, v, v, 255])
        })
    }

    #[test]
    fn the_shading_follows_the_light_of_the_picture() {
        let map = SurfaceMap::new(&folded(60, 20), None);
        let red = [220, 40, 40, 255];
        let in_fold = map.shade(red, 30, 10, 1.0);
        let flat = map.shade(red, 5, 10, 1.0);
        assert!(in_fold[0] < flat[0], "{in_fold:?} {flat:?}");
        assert_eq!(map.shade(red, 30, 10, 0.0), red);
    }

    #[test]
    fn the_relief_pushes_paint_near_the_fold_only() {
        let map = SurfaceMap::new(&folded(60, 20), None);
        let (fx, _) = map.offset(27.5, 10.0, 6.0);
        assert!(fx.abs() > 0.5, "{fx}");
        let (ex, ey) = map.offset(3.5, 10.0, 6.0);
        assert!(ex.abs() < 0.1 && ey.abs() < 0.1);
    }

    #[test]
    fn a_given_depth_map_takes_the_place_of_the_light() {
        let flat = RgbaImage::from_pixel(40, 20, Rgba([128, 128, 128, 255]));
        let ramp = GrayImage::from_fn(40, 20, |x, _| image::Luma([if x < 20 { 0 } else { 255 }]));
        let map = SurfaceMap::new(&flat, Some(&ramp));
        assert!(map.offset(20.0, 10.0, 5.0).0.abs() > 0.5);
        let none = SurfaceMap::new(&flat, None);
        assert_eq!(none.offset(20.0, 10.0, 5.0), (0.0, 0.0));
    }

    #[test]
    fn a_layer_is_fitted_to_the_surface_beneath() {
        let mut pi = PixelImage::new(folded(60, 20));
        pi.insert_layer("art", RgbaImage::from_pixel(60, 20, Rgba([0, 0, 255, 255])));
        let map = SurfaceMap::new(&seen_beneath(&pi), None);
        fit_to_surface(&mut pi, &map, 4.0, 1.0).unwrap();
        let fold = pi.img.get_pixel(30, 10)[2];
        let flat = pi.img.get_pixel(5, 10)[2];
        assert!(fold < flat, "{fold} {flat}");
    }
}
