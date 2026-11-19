//! Tracing the selected bitmaps off the interface thread: Image > Trace
//! > Automatic, Repeat Last Trace and the Trace dialog's OK (when the
//! preview has not traced every picture already). A trace takes from a
//! fraction of a second to a minute (a small picture is enlarged first,
//! the best detail is searched, the curves are checked against the
//! picture, small print is matched with fonts): while it runs a window
//! shows the step and how much is done, with Cancel
//! (`ui/trace_progress.rs`), and the program keeps answering. The
//! curves are placed, as one undo step, when every picture is traced.
//! In the browser, which has no threads here, the trace runs in the
//! frame as before.

use crate::app::App;
use crate::trace::{Cancel, Settings, Traced};
use crate::trace_progress::{Progress, Step};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;
use tracedraw_core::geometry::Rect;
use tracedraw_core::ShapeId;
use web_time::Instant;

/// A bitmap traced: its shape, rectangle, pixel size, curves (pixel
/// space) and the width of an open curve's outline in pixels.
pub struct TracedBitmap {
    pub shape: ShapeId,
    pub rect: Rect,
    pub w: u32,
    pub h: u32,
    pub traced: Vec<Traced>,
    pub stroke_px: f64,
}

/// A trace running off the interface thread.
pub struct TraceJob {
    pub settings: Settings,
    pub cancel: Arc<Cancel>,
    pub progress: Arc<Progress>,
    /// The picture being traced (from 0) and how many there are.
    pub picture: Arc<AtomicUsize>,
    pub pictures: usize,
    pub started: Instant,
    rx: Receiver<Option<Vec<TracedBitmap>>>,
}

impl TraceJob {
    /// How much of the whole job is done, 0 to 1.
    pub fn done(&self) -> f32 {
        let n = self.pictures.max(1) as f32;
        let i = self.picture.load(Ordering::Relaxed).min(self.pictures) as f32;
        ((i + self.progress.done()) / n).clamp(0.0, 1.0)
    }

    /// The step running.
    pub fn step(&self) -> Step {
        self.progress.step()
    }
}

impl Drop for TraceJob {
    /// A job dropped before it ends stops its trace.
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// The step a trace of a picture starts at.
fn first_step(enlarge: bool, img: &image::RgbaImage, s: &Settings) -> Step {
    if enlarge && crate::upscale::would_enlarge(img) {
        Step::Enlarge
    } else if s.optimize && !s.centerline {
        Step::Detail
    } else {
        Step::Colors
    }
}

/// Trace one bitmap's picture as the settings say, enlarged first when
/// `enlarge`. `None` once `cancel` is set; an unreadable picture gives
/// no curves.
fn trace_one(
    png: &[u8],
    settings: &Settings,
    enlarge: bool,
    cancel: &Cancel,
    progress: &Progress,
) -> Option<(Vec<Traced>, f64)> {
    let Some(img) = crate::bitmap_fx::decode(png) else {
        return Some((Vec::new(), 1.0));
    };
    progress.restart(
        first_step(enlarge, &img, settings),
        settings.optimize && !settings.centerline,
    );
    if enlarge {
        crate::upscale::trace_unless(&img, settings, true, Some(cancel))
    } else {
        let traced = crate::trace::trace_unless(&img, settings, Some(cancel))?;
        Some((traced, crate::trace::last_stroke_width_px()))
    }
}

impl App {
    /// Trace the selected bitmaps as `settings` say, off the interface
    /// thread with the progress window (see the module's notes; the
    /// window redraws itself while the trace runs); `ready`
    /// is the trace of one of them made already (the Trace dialog's
    /// preview), not traced again. With every picture ready, or in the
    /// browser, the curves are placed at once.
    pub fn trace_bitmaps_in_background(
        &mut self,
        settings: &Settings,
        ready: Option<(ShapeId, Arc<Vec<Traced>>, f64)>,
    ) {
        let bitmaps = self.bitmap_shapes();
        let needs_work = bitmaps
            .iter()
            .any(|b| !ready.as_ref().is_some_and(|(id, _, _)| *id == b.0));
        if bitmaps.is_empty() || !needs_work || cfg!(target_arch = "wasm32") {
            self.trace_bitmaps_using(settings, ready);
            return;
        }
        // One trace at a time: a new one stops the one running.
        self.trace_job = None;
        let cancel = Arc::new(Cancel::new(false));
        let progress = Arc::new(Progress::new(Step::Colors, settings.optimize));
        let picture = Arc::new(AtomicUsize::new(0));
        let (tx, rx) = mpsc::channel();
        let pictures = bitmaps.len();
        let enlarge = self.settings.tracing.enlarge;
        let work = {
            let (settings, cancel, progress, picture) = (
                settings.clone(),
                cancel.clone(),
                progress.clone(),
                picture.clone(),
            );
            move || {
                let _watch = crate::trace_progress::watch(&cancel, &progress);
                let mut out = Vec::new();
                for (i, (shape, rect, w, h, png)) in bitmaps.into_iter().enumerate() {
                    picture.store(i, Ordering::Relaxed);
                    let done = match &ready {
                        Some((rid, t, px)) if *rid == shape => Some((t.as_ref().clone(), *px)),
                        _ => trace_one(&png, &settings, enlarge, &cancel, &progress),
                    };
                    let Some((traced, stroke_px)) = done else {
                        let _ = tx.send(None);
                        return;
                    };
                    out.push(TracedBitmap {
                        shape,
                        rect,
                        w,
                        h,
                        traced,
                        stroke_px,
                    });
                }
                picture.store(pictures, Ordering::Relaxed);
                let _ = tx.send(Some(out));
            }
        };
        std::thread::spawn(work);
        self.trace_job = Some(TraceJob {
            settings: settings.clone(),
            cancel,
            progress,
            picture,
            pictures,
            started: Instant::now(),
            rx,
        });
    }

    /// Place the curves of a trace that has ended; true when the job is
    /// over (placed, cancelled or lost).
    pub fn poll_trace_job(&mut self) -> bool {
        let Some(job) = &self.trace_job else {
            return true;
        };
        let got = match job.rx.try_recv() {
            Ok(got) => got,
            Err(TryRecvError::Empty) => return false,
            Err(TryRecvError::Disconnected) => None,
        };
        let Some(job) = self.trace_job.take() else {
            return true;
        };
        if let Some(done) = got {
            self.place_traces(&job.settings, done);
        }
        true
    }

    /// Stop the trace running; nothing is placed.
    pub fn cancel_trace_job(&mut self) {
        self.trace_job = None;
    }

    /// Wait for the trace running and place it (the tests, which have no
    /// frames).
    #[cfg(test)]
    pub fn wait_for_trace(&mut self) {
        while !self.poll_trace_job() {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::app::App;
    use crate::ui::dialogs::Dialog;

    fn disc_app() -> App {
        let mut app = App::headless();
        let img = image::RgbaImage::from_fn(120, 120, |x, y| {
            let d = ((x as f32 - 60.0).powi(2) + (y as f32 - 60.0).powi(2)).sqrt();
            if d < 35.0 {
                image::Rgba([200, 30, 30, 255])
            } else {
                image::Rgba([255, 255, 255, 255])
            }
        });
        let bmp = app.place_image(img, Some("Disc".into())).expect("bitmap");
        app.select(vec![bmp]);
        app
    }

    fn shapes(app: &App) -> usize {
        app.doc().pages[0]
            .layers
            .iter()
            .map(|l| l.shapes.len())
            .sum()
    }

    fn frame(app: &mut App, ctx: &egui::Context) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1400.0, 900.0),
            )),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| crate::ui::dialogs::show(app, &ui.ctx().clone()));
        out.textures_delta.clear();
    }

    /// Image > Trace > Automatic traces off the interface thread: the
    /// call comes back at once with the progress window open, nothing is
    /// placed until the trace ends, then the curves are placed as one
    /// undo step, selected, and the window closes.
    #[test]
    fn automatic_traces_behind_its_progress_window() {
        let mut app = disc_app();
        let before = shapes(&app);
        let steps = app.engine.history_labels().0.len();
        app.quick_trace();
        assert!(app.trace_job.is_some(), "running off the interface thread");
        assert_eq!(shapes(&app), before, "nothing placed yet");
        let ctx = egui::Context::default();
        frame(&mut app, &ctx);
        let mut frames = 0;
        while app.trace_job.is_some() {
            assert!(matches!(app.dialog, Dialog::Tracing), "{:?}", app.dialog);
            std::thread::sleep(std::time::Duration::from_millis(10));
            frame(&mut app, &ctx);
            frames += 1;
            assert!(frames < 6000, "the trace ends");
        }
        frame(&mut app, &ctx);
        assert!(matches!(app.dialog, Dialog::None), "{:?}", app.dialog);
        assert_eq!(shapes(&app), before + 1, "the trace's group");
        let group = app.selection[0];
        assert_eq!(
            app.doc().shape(group).expect("trace").1.name.as_deref(),
            Some(crate::i18n::tr("trace.result_name").as_str())
        );
        assert_eq!(
            app.engine.history_labels().0.len(),
            steps + 1,
            "one undo step"
        );
    }

    /// Cancel stops the trace and places nothing.
    #[test]
    fn a_cancelled_trace_places_nothing() {
        let mut app = disc_app();
        let before = shapes(&app);
        app.quick_trace();
        let cancel = app.trace_job.as_ref().expect("running").cancel.clone();
        app.cancel_trace_job();
        assert!(
            cancel.load(std::sync::atomic::Ordering::Relaxed),
            "told to stop"
        );
        assert!(app.trace_job.is_none());
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(app.poll_trace_job());
        assert_eq!(shapes(&app), before);
    }
}
