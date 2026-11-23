//! The view of a picture being edited in the image editing mode (Bitmaps >
//! Edit Inside, a double click on a picture): the picture alone, upright,
//! at a zoom of its own, panned and zoomed about the pointer, apart from
//! the drawing's view. See `docs/behavior/edit-inside-picture.md`.

use egui::{Pos2, Rect, Vec2};

/// The zoom steps the zoom buttons and keys go through (screen points
/// per picture pixel).
pub const STEPS: [f32; 21] = [
    0.02, 0.03, 0.05, 0.0625, 0.083, 0.125, 0.167, 0.25, 0.333, 0.5, 0.667, 1.0, 1.5, 2.0, 3.0,
    4.0, 6.0, 8.0, 12.0, 16.0, 32.0,
];

/// The least and most zoom.
pub const MIN_ZOOM: f32 = 0.01;
pub const MAX_ZOOM: f32 = 64.0;

/// Where the picture lies on the screen: screen points per picture
/// pixel, the picture pixel at the middle of the view, and whether it
/// still follows the window (fitted until the user zooms or pans).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PixelView {
    pub zoom: f32,
    pub center: (f32, f32),
    pub fitted: bool,
}

impl Default for PixelView {
    fn default() -> Self {
        PixelView {
            zoom: 1.0,
            center: (0.0, 0.0),
            fitted: true,
        }
    }
}

impl PixelView {
    /// The picture pixel (x, y) on the screen, in the view `area`.
    pub fn to_screen(&self, area: Rect, x: f32, y: f32) -> Pos2 {
        area.center() + Vec2::new((x - self.center.0) * self.zoom, (y - self.center.1) * self.zoom)
    }

    /// The screen point `p` in picture pixels.
    pub fn to_px(&self, area: Rect, p: Pos2) -> (f32, f32) {
        let d = (p - area.center()) / self.zoom.max(MIN_ZOOM);
        (self.center.0 + d.x, self.center.1 + d.y)
    }

    /// The whole picture of `w` x `h` pixels in the view, a margin round
    /// it, never past 100 % for a small picture unless `enlarge`.
    pub fn fit(&mut self, area: Rect, w: u32, h: u32, enlarge: bool) {
        let (w, h) = (w.max(1) as f32, h.max(1) as f32);
        let room = (area.size() - Vec2::splat(48.0)).max(Vec2::splat(16.0));
        let mut z = (room.x / w).min(room.y / h);
        if !enlarge {
            z = z.min(1.0);
        }
        self.zoom = z.clamp(MIN_ZOOM, MAX_ZOOM);
        self.center = (w / 2.0, h / 2.0);
        self.fitted = true;
    }

    /// Zoom by `factor` keeping the picture pixel under `p` where it is.
    pub fn zoom_about(&mut self, area: Rect, p: Pos2, factor: f32) {
        self.zoom_to(area, p, self.zoom * factor);
    }

    /// Zoom to `zoom` keeping the picture pixel under `p` where it is.
    pub fn zoom_to(&mut self, area: Rect, p: Pos2, zoom: f32) {
        let at = self.to_px(area, p);
        self.zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        let d = (p - area.center()) / self.zoom;
        self.center = (at.0 - d.x, at.1 - d.y);
        self.fitted = false;
    }

    /// The next zoom step in (`dir` > 0) or out from the zoom now.
    pub fn step(&self, dir: i32) -> f32 {
        if dir > 0 {
            STEPS
                .iter()
                .copied()
                .find(|s| *s > self.zoom * 1.001)
                .unwrap_or(MAX_ZOOM)
        } else {
            STEPS
                .iter()
                .rev()
                .copied()
                .find(|s| *s < self.zoom / 1.001)
                .unwrap_or(MIN_ZOOM)
        }
    }

    /// Move the picture by `d` screen points.
    pub fn pan(&mut self, d: Vec2) {
        let z = self.zoom.max(MIN_ZOOM);
        self.center.0 -= d.x / z;
        self.center.1 -= d.y / z;
        self.fitted = false;
    }

    /// The zoom as a whole percent.
    pub fn percent(&self) -> u32 {
        (self.zoom * 100.0).round() as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area() -> Rect {
        Rect::from_min_size(Pos2::new(100.0, 50.0), Vec2::new(848.0, 648.0))
    }

    #[test]
    fn a_fitted_picture_fills_the_view_and_maps_both_ways() {
        let mut v = PixelView::default();
        v.fit(area(), 1600, 1200, false);
        assert!((v.zoom - 0.5).abs() < 1e-4, "{}", v.zoom);
        let corner = v.to_screen(area(), 0.0, 0.0);
        assert!((corner.x - 124.0).abs() < 1e-3 && (corner.y - 74.0).abs() < 1e-3);
        let back = v.to_px(area(), corner);
        assert!(back.0.abs() < 1e-3 && back.1.abs() < 1e-3);
        // A small picture stays at 100 % unless asked to fill.
        v.fit(area(), 100, 50, false);
        assert_eq!(v.zoom, 1.0);
        v.fit(area(), 100, 50, true);
        assert!(v.zoom > 7.0);
    }

    #[test]
    fn zooming_keeps_the_pixel_under_the_pointer_and_panning_moves_it() {
        let mut v = PixelView::default();
        v.fit(area(), 400, 300, false);
        let p = Pos2::new(300.0, 200.0);
        let before = v.to_px(area(), p);
        v.zoom_about(area(), p, 4.0);
        let after = v.to_px(area(), p);
        assert!((before.0 - after.0).abs() < 1e-3 && (before.1 - after.1).abs() < 1e-3);
        assert!(!v.fitted);
        let s = v.to_screen(area(), 10.0, 10.0);
        v.pan(Vec2::new(30.0, -20.0));
        let s2 = v.to_screen(area(), 10.0, 10.0);
        assert!((s2 - s - Vec2::new(30.0, -20.0)).length() < 1e-3);
        assert_eq!(PixelView { zoom: 1.0, ..v }.step(1), 1.5);
        assert_eq!(PixelView { zoom: 1.0, ..v }.step(-1), 0.667);
    }
}
