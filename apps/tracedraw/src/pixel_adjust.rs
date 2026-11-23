//! Adjustment layers' helpers and the histogram inside a picture being
//! edited (Bitmaps > Edit Inside). See `docs/behavior/edit-inside-picture.md`.

use image::{GrayImage, RgbaImage};

/// How many pixels have each value, 0 to 255: lightness and each colour.
#[derive(Debug, Clone, PartialEq)]
pub struct Histogram {
    pub luma: Vec<u32>,
    pub red: Vec<u32>,
    pub green: Vec<u32>,
    pub blue: Vec<u32>,
    /// Mean lightness, 0 to 255.
    pub mean: f32,
    /// Pixels counted.
    pub count: u64,
}

/// The histogram of `img` inside `sel` (soft selections count where at
/// least half selected), clear pixels left out. Large pictures are
/// sampled on a grid of at most about 250 000 pixels.
pub fn histogram(img: &RgbaImage, sel: Option<&GrayImage>) -> Histogram {
    let (w, h) = img.dimensions();
    let step = (((w as u64 * h as u64) as f64 / 250_000.0).sqrt().ceil() as u32).max(1);
    let mut out = Histogram {
        luma: vec![0; 256],
        red: vec![0; 256],
        green: vec![0; 256],
        blue: vec![0; 256],
        mean: 0.0,
        count: 0,
    };
    let mut sum = 0.0f64;
    for y in (0..h).step_by(step as usize) {
        for x in (0..w).step_by(step as usize) {
            if let Some(m) = sel {
                if m.get_pixel_checked(x, y).map_or(true, |v| v[0] < 128) {
                    continue;
                }
            }
            let q = img.get_pixel(x, y);
            if q[3] == 0 {
                continue;
            }
            let l = crate::fx::util::luma(q).round().clamp(0.0, 255.0) as usize;
            out.luma[l] += 1;
            out.red[q[0] as usize] += 1;
            out.green[q[1] as usize] += 1;
            out.blue[q[2] as usize] += 1;
            sum += l as f64;
            out.count += 1;
        }
    }
    if out.count > 0 {
        out.mean = (sum / out.count as f64) as f32;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pixel_edit::PixelImage;
    use image::Rgba;

    #[test]
    fn the_histogram_counts_the_selected_pixels() {
        let mut img = RgbaImage::from_pixel(10, 10, Rgba([0, 0, 0, 255]));
        for y in 0..10 {
            for x in 5..10 {
                img.put_pixel(x, y, Rgba([255, 255, 255, 255]));
            }
        }
        let h = histogram(&img, None);
        assert_eq!((h.count, h.luma[0], h.luma[255]), (100, 50, 50));
        assert!((h.mean - 127.5).abs() < 0.6);
        let sel = crate::pixel_edit::rect_mask(10, 10, (5.0, 0.0), (10.0, 10.0));
        let h = histogram(&img, sel.as_ref());
        assert_eq!((h.count, h.red[255]), (50, 50));
    }

    #[test]
    fn an_adjustment_layer_changes_what_is_beneath_until_merged() {
        let mut pi = PixelImage::new(RgbaImage::from_pixel(4, 4, Rgba([255, 0, 0, 255])));
        let e = crate::fx::new_effect("invert_colors");
        pi.add_adjustment_layer("Invert", e);
        assert!(pi.active_locked(), "no pixels to paint");
        assert_eq!(*pi.composite().get_pixel(1, 1), Rgba([0, 255, 255, 255]));
        // Its mask lets the picture show through.
        pi.sel = crate::pixel_edit::rect_mask(4, 4, (0.0, 0.0), (2.0, 4.0));
        assert!(pi.add_layer_mask());
        assert_eq!(*pi.composite().get_pixel(1, 1), Rgba([0, 255, 255, 255]));
        assert_eq!(*pi.composite().get_pixel(3, 1), Rgba([255, 0, 0, 255]));
        let seen = pi.composite();
        assert!(pi.merge_down());
        assert_eq!(pi.layers.len(), 1);
        assert_eq!(pi.img, seen);
    }
}
