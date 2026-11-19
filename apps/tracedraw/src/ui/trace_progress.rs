//! The window shown while a trace runs off the interface thread
//! (`trace_job.rs`): the turning busy mark, the step the trace is at
//! ("Enlarging the picture", "Choosing the detail", "Finding the colors",
//! "Drawing the curves", "Checking against the picture", "Matching the
//! letters"), the picture of how many when several are traced, a bar
//! with the percentage, the time taken, and Cancel (Esc and the close
//! button too), which stops the trace and places nothing. When the trace
//! ends the window closes and the curves are placed.

use crate::app::App;
use crate::i18n::{tr, trf};
use crate::theme::Tokens;
use egui::{Context, RichText, Sense, Vec2};

/// How often the window is redrawn while the trace runs.
const FRAME: std::time::Duration = std::time::Duration::from_millis(100);

/// The bar's width.
const BAR_W: f32 = 340.0;

/// Draw the window of the trace running; `close` is set once it ended
/// or was cancelled.
pub fn dialog(app: &mut App, ctx: &Context, close: &mut bool) {
    if app.poll_trace_job() {
        *close = true;
        return;
    }
    let Some(job) = &app.trace_job else {
        *close = true;
        return;
    };
    ctx.request_repaint_after(FRAME);
    let step = tr(job.step().label_key());
    let pct = (job.done() * 100.0).floor().clamp(0.0, 99.0) as u32;
    let done = job.done();
    let picture = (job.picture.load(std::sync::atomic::Ordering::Relaxed) + 1).min(job.pictures);
    let pictures = job.pictures;
    let secs = job.started.elapsed().as_secs();
    let mut cancel = false;
    let before = crate::ui::chrome::take_close_request(ctx);
    crate::ui::chrome::window(tr("dialog.trace"))
        .id_salt("trace_progress")
        .show(ctx, |ui| {
            ui.set_min_width(BAR_W + 60.0);
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(32.0), Sense::hover());
                crate::ui::chrome::paint_message_icon(
                    ui,
                    rect,
                    crate::ui::chrome::MessageIcon::Busy,
                );
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.add_space(2.0);
                    ui.label(RichText::new(format!("{step}...")).size(13.5).strong());
                    if pictures > 1 {
                        let n = trf(
                            "trace.progress_picture",
                            &[("n", &picture.to_string()), ("of", &pictures.to_string())],
                        );
                        ui.label(RichText::new(n).size(12.5).color(Tokens::text_dim()));
                    }
                    ui.add_space(4.0);
                    ui.add(
                        egui::ProgressBar::new(done)
                            .desired_width(BAR_W)
                            .desired_height(10.0)
                            .corner_radius(5.0)
                            .fill(Tokens::ACCENT),
                    );
                    let time = format!("{}:{:02}", secs / 60, secs % 60);
                    ui.label(
                        RichText::new(format!("{pct} %   {time}"))
                            .size(12.5)
                            .color(Tokens::text_dim()),
                    );
                });
            });
            ui.add_space(10.0);
            if crate::ui::chrome::button_row(ui, &[&tr("dialog.cancel")], true).is_some() {
                cancel = true;
            }
        });
    if cancel || before || crate::ui::chrome::take_close_request(ctx) {
        app.cancel_trace_job();
        *close = true;
    }
}
