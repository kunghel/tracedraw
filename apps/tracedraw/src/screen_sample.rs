//! The Color Eyedropper's samples of the window's own pixels (#3040): a
//! click where no object gives a colour (the desk around the page, the
//! empty page), or any click in the window after Sample from Screen
//! (the panels, the toolbars, the palettes too), takes the colour shown
//! there. The window is read back once
//! (a screenshot of its last frame) and the pixel under the click,
//! averaged over the bar's sample size, is the colour sampled; it then
//! goes where a sampled colour goes (`App::took_sampled_color`). See
//! `docs/behavior/eyedroppers.md`.

use crate::app::App;
use tracedraw_core::Color;

/// The tag of the window read-back asked for a sample.
pub const TAG: &str = "eyedropper_screen_sample";

/// A sample of the window's pixels on its way.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenSample {
    /// The click, interface points in the window.
    pub at: egui::Pos2,
    /// Shift was held: the colour is for the outline.
    pub outline: bool,
    /// The read-back was asked for.
    pub asked: bool,
}

/// The colour of `image` (the window read back, `ppp` pixels a point)
/// under `at`, averaged over a box of `n` x `n` pixels anchored at the
/// pixel under it and clipped to the image; `None` outside it.
pub fn pixel_average(image: &egui::ColorImage, at: egui::Pos2, ppp: f32, n: u32) -> Option<Color> {
    let [w, h] = image.size;
    let x = (at.x * ppp).floor();
    let y = (at.y * ppp).floor();
    if !x.is_finite() || !y.is_finite() || x < 0.0 || y < 0.0 {
        return None;
    }
    let (x, y) = (x as usize, y as usize);
    if x >= w || y >= h {
        return None;
    }
    let n = n.max(1) as usize;
    let (mut r, mut g, mut b, mut count) = (0u32, 0u32, 0u32, 0u32);
    for yy in y..(y + n).min(h) {
        for xx in x..(x + n).min(w) {
            let c = image.pixels[yy * w + xx];
            r += c.r() as u32;
            g += c.g() as u32;
            b += c.b() as u32;
            count += 1;
        }
    }
    let avg = |v: u32| ((v + count / 2) / count) as u8;
    Some(Color::rgb8(avg(r), avg(g), avg(b)))
}

/// Whether the pointer was let go outside the window this frame (a press
/// in the window dragged out of it: the system still reports where).
pub fn let_go_outside(ctx: &egui::Context) -> bool {
    let window = ctx.viewport_rect();
    ctx.input(|i| i.pointer.primary_released() && i.pointer.latest_pos().is_some_and(|p| !window.contains(p)))
}

/// The colour the screen shows under the system pointer, averaged over a
/// box of `n` x `n` pixels: outside the window too, the desktop and other
/// programs (#4083). Only where the system lets a program read the screen
/// (Windows); `None` elsewhere.
pub fn desktop_color_at_cursor(n: u32) -> Option<Color> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::POINT;
        use windows_sys::Win32::Graphics::Gdi::{GetDC, GetPixel, ReleaseDC, CLR_INVALID};
        use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
        let mut at = POINT { x: 0, y: 0 };
        // SAFETY: plain calls with a valid out-pointer; the screen's device
        // context is released before returning.
        unsafe {
            if GetCursorPos(&mut at) == 0 {
                return None;
            }
            let dc = GetDC(0);
            if dc == 0 {
                return None;
            }
            let n = n.clamp(1, 15) as i32;
            let half = (n - 1) / 2;
            let (mut r, mut g, mut b, mut count) = (0u32, 0u32, 0u32, 0u32);
            for dy in -half..(n - half) {
                for dx in -half..(n - half) {
                    let c = GetPixel(dc, at.x + dx, at.y + dy);
                    if c == CLR_INVALID {
                        continue;
                    }
                    r += c & 0xff;
                    g += (c >> 8) & 0xff;
                    b += (c >> 16) & 0xff;
                    count += 1;
                }
            }
            ReleaseDC(0, dc);
            if count == 0 {
                return None;
            }
            let avg = |v: u32| ((v + count / 2) / count) as u8;
            Some(Color::rgb8(avg(r), avg(g), avg(b)))
        }
    }
    #[cfg(not(windows))]
    {
        let _ = n;
        None
    }
}

impl App {
    /// The Color Eyedropper let go outside the window: the colour the
    /// screen shows there goes where a sampled colour goes; where the
    /// system does not let it be read, the status bar says so (#4083).
    pub fn sample_desktop(&mut self, outline: bool) {
        self.eyedropper_screen = false;
        match desktop_color_at_cursor(self.eyedropper_sample) {
            Some(c) => self.took_sampled_color(c, outline),
            None => self.status = crate::i18n::tr("status.screen_sample_failed"),
        }
    }

    /// Whether a Color Eyedropper click at `p` samples the window's pixel
    /// rather than an object: it samples (it does not apply) and no
    /// object under it gives a colour (#3040).
    pub fn eyedropper_wants_screen(&self, p: tracedraw_core::geometry::Point, shift: bool, resample: bool) -> bool {
        let apply = self.eyedropper_apply && self.eyedropper_color.is_some() && !resample;
        !apply && !self.mask.picking && self.sample_color_at(p, shift).is_none()
    }

    /// Sample the window's pixel at `at` (interface points): the read-back
    /// is asked for on the next frame.
    pub fn ask_screen_sample(&mut self, at: egui::Pos2, outline: bool) {
        self.screen_sample = Some(ScreenSample {
            at,
            outline,
            asked: false,
        });
    }

    /// Each frame: ask for the window's read-back a sample waits for, and
    /// take the colour from it once it came. Sample from Screen ends with
    /// the sample; a read-back that never comes (a renderer that cannot
    /// read back) is given up after a few frames, saying so.
    pub fn screen_sample_frame(&mut self, ctx: &egui::Context) {
        let Some(mut s) = self.screen_sample else {
            return;
        };
        if !s.asked {
            s.asked = true;
            self.screen_sample = Some(s);
            self.screen_sample_frames = 0;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(TAG)));
            ctx.request_repaint();
            return;
        }
        let image = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot {
                    user_data, image, ..
                } if user_data
                    .data
                    .as_ref()
                    .and_then(|d| d.downcast_ref::<&str>())
                    .is_some_and(|tag| *tag == TAG) =>
                {
                    Some(image.clone())
                }
                _ => None,
            })
        });
        let Some(image) = image else {
            self.screen_sample_frames += 1;
            if self.screen_sample_frames > 30 {
                self.screen_sample = None;
                self.eyedropper_screen = false;
                self.status = crate::i18n::tr("status.screen_sample_failed");
            } else {
                ctx.request_repaint();
            }
            return;
        };
        self.screen_sample = None;
        self.eyedropper_screen = false;
        let ppp = ctx.pixels_per_point();
        match pixel_average(&image, s.at, ppp, self.eyedropper_sample) {
            Some(c) => self.took_sampled_color(c, s.outline),
            None => self.status = crate::i18n::tr("status.screen_sample_failed"),
        }
    }

    /// Sample from Screen: the next click anywhere in the window, on the
    /// panels and bars too, samples the colour shown there. A cover over
    /// the whole window takes that click, so the button or field under it
    /// is not pressed; Esc ends it.
    pub fn screen_pick_cover(&mut self, ctx: &egui::Context) {
        if !self.eyedropper_screen || self.screen_sample.is_some() {
            return;
        }
        if self.tool != crate::tools::Tool::ColorEyedropper
            || ctx.input(|i| i.key_pressed(egui::Key::Escape))
        {
            self.eyedropper_screen = false;
            return;
        }
        let rect = ctx.viewport_rect();
        egui::Area::new(egui::Id::new("screen_sample_cover"))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.min)
            .show(ctx, |ui| {
                let resp = ui.allocate_rect(rect, egui::Sense::click_and_drag());
                ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
                // Pressed here and let go outside the window: the desktop's
                // colour there (#4083).
                if resp.drag_stopped() && let_go_outside(ui.ctx()) {
                    let shift = ui.input(|i| i.modifiers.shift);
                    self.sample_desktop(shift);
                } else if resp.clicked() || resp.drag_stopped() {
                    if let Some(at) = resp.interact_pointer_pos() {
                        let shift = ui.input(|i| i.modifiers.shift);
                        self.ask_screen_sample(at, shift);
                    }
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(w: usize, h: usize, f: impl Fn(usize, usize) -> egui::Color32) -> egui::ColorImage {
        let mut img = egui::ColorImage::new([w, h], vec![egui::Color32::BLACK; w * h]);
        for y in 0..h {
            for x in 0..w {
                img.pixels[y * w + x] = f(x, y);
            }
        }
        img
    }

    #[test]
    fn the_pixel_under_the_point_is_read_at_the_screens_scale() {
        let img = image(20, 10, |x, _| {
            if x < 10 {
                egui::Color32::from_rgb(200, 0, 0)
            } else {
                egui::Color32::from_rgb(0, 0, 200)
            }
        });
        // Two pixels a point: point (6, 2) is pixel (12, 4), blue.
        assert_eq!(pixel_average(&img, egui::pos2(6.0, 2.0), 2.0, 1), Some(Color::rgb8(0, 0, 200)));
        assert_eq!(pixel_average(&img, egui::pos2(6.0, 2.0), 1.0, 1), Some(Color::rgb8(200, 0, 0)));
        // A 2x2 box across the edge averages both.
        assert_eq!(pixel_average(&img, egui::pos2(9.0, 0.0), 1.0, 2), Some(Color::rgb8(100, 0, 100)));
        // Outside the window: nothing.
        assert_eq!(pixel_average(&img, egui::pos2(30.0, 2.0), 1.0, 1), None);
        assert_eq!(pixel_average(&img, egui::pos2(-1.0, 2.0), 1.0, 1), None);
    }

    /// A click on the desk with the Color Eyedropper asks for the window's
    /// read-back and takes the pixel's colour from it, on to the selection
    /// as a sampled colour goes; Sample from Screen covers the window and
    /// its click samples there too (#3040).
    #[test]
    fn a_click_off_the_objects_samples_the_window() {
        crate::i18n::set_language("en");
        let mut app = App::headless();
        app.show_welcome = false;
        app.set_tool(crate::tools::Tool::ColorEyedropper);
        let page = app.page_rect();
        // Far off the page: no object there.
        let desk = tracedraw_core::geometry::Point::new(page.x1 + 500.0, page.y1 + 500.0);
        assert!(app.eyedropper_wants_screen(desk, false, false));
        app.ask_screen_sample(egui::pos2(3.0, 4.0), false);
        let ctx = crate::theme::ui_context();
        let run = |app: &mut App, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                events,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| app.screen_sample_frame(ui.ctx()));
            out.textures_delta.clear();
        };
        run(&mut app, Vec::new());
        assert!(app.screen_sample.is_some_and(|s| s.asked), "the read-back asked for");
        let img = image(10, 10, |x, y| {
            if (x, y) == (3, 4) {
                egui::Color32::from_rgb(10, 120, 230)
            } else {
                egui::Color32::GRAY
            }
        });
        let shot = egui::Event::Screenshot {
            viewport_id: egui::ViewportId::ROOT,
            user_data: egui::UserData::new(TAG),
            image: std::sync::Arc::new(img),
        };
        run(&mut app, vec![shot]);
        assert!(app.screen_sample.is_none());
        assert_eq!(app.eyedropper_color, Some(Color::rgb8(10, 120, 230)));
        assert!(app.eyedropper_apply, "on to Apply, as any sample");
        // A read-back that never comes is given up, saying so.
        app.eyedropper_apply = false;
        app.ask_screen_sample(egui::pos2(1.0, 1.0), false);
        for _ in 0..40 {
            run(&mut app, Vec::new());
        }
        assert!(app.screen_sample.is_none());
        assert!(!app.status.is_empty());
    }
}
