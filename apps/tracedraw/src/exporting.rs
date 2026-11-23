//! An export made and written on a worker thread, with a window that
//! shows how far it got. A drawing of many pages, or a large picture,
//! takes seconds to minutes to export, and the interface froze meanwhile
//! with nothing saying why.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};

/// How long the export goes on before the window shows: one done sooner
/// ends without it flashing up.
pub const WINDOW_AFTER: std::time::Duration = std::time::Duration::from_millis(400);

/// An export running on a worker thread.
pub struct Exporting {
    pub path: PathBuf,
    /// The files written so far, of `steps`.
    pub done: Arc<AtomicUsize>,
    pub steps: usize,
    /// The options dialog was not shown: the status line says so at the
    /// end (`ui::dialogs::export_done`).
    pub last_options: bool,
    pub started: web_time::Instant,
    rx: mpsc::Receiver<Result<String, String>>,
}

impl Exporting {
    /// Start `work` on a thread of its own.
    pub fn start(work: crate::export::ExportWork, last_options: bool) -> Result<Exporting, String> {
        let done = Arc::new(AtomicUsize::new(0));
        let (tx, rx) = mpsc::channel();
        let (path, steps, count) = (work.path.clone(), work.steps(), done.clone());
        std::thread::Builder::new()
            .name("export".into())
            .spawn(move || {
                let _ = tx.send(work.run(&count));
            })
            .map_err(|e| e.to_string())?;
        Ok(Exporting {
            path,
            done,
            steps,
            last_options,
            started: web_time::Instant::now(),
            rx,
        })
    }

    /// The outcome once the export is over: the status line's words, or
    /// the error.
    pub fn poll(&self) -> Option<Result<String, String>> {
        match self.rx.try_recv() {
            Ok(r) => Some(r),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err("the export stopped".into())),
        }
    }

    /// Wait until the export is over (the program is closing, or another
    /// export starts): its outcome.
    pub fn wait(&self) -> Result<String, String> {
        self.rx
            .recv()
            .unwrap_or_else(|_| Err("the export stopped".into()))
    }
}

/// The window of an export on its way, once it has taken
/// [`WINDOW_AFTER`]: the file's name and a bar, which fills file by file
/// when the export writes several (a file for each page) and moves on its
/// own for one file. Nothing is typed or clicked into the drawing
/// meanwhile (the input is held back, as while a large drawing is saved).
pub fn window(app: &mut crate::app::App, ctx: &egui::Context) {
    use crate::i18n::{tr, trf};
    let Some(job) = &app.exporting else {
        return;
    };
    let waited = job.started.elapsed();
    if waited < WINDOW_AFTER {
        ctx.request_repaint_after(WINDOW_AFTER - waited);
        return;
    }
    let name = job
        .path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let done = job.done.load(Ordering::Relaxed).min(job.steps);
    egui::Window::new(trf("exporting.title", &[("name", &name)]))
        .id(egui::Id::new("exporting_window"))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_width(360.0);
            if job.steps > 1 {
                ui.label(trf(
                    "exporting.pages",
                    &[
                        ("n", &(done + 1).min(job.steps).to_string()),
                        ("total", &job.steps.to_string()),
                    ],
                ));
                ui.add(
                    egui::ProgressBar::new(done as f32 / job.steps as f32).show_percentage(),
                );
            } else {
                ui.label(tr("exporting.writing"));
                ui.add(egui::ProgressBar::new(1.0).animate(true));
            }
        });
    ctx.request_repaint_after(std::time::Duration::from_millis(50));
}

/// A large file of another format (a `.cdr`, a PDF, an SVG) about to be
/// opened: its name on screen before the reading starts, which stops the
/// window until the drawing is there. Before, the program stood still
/// from the moment the file was chosen with nothing saying why.
pub fn opening_notice(app: &mut crate::app::App, ctx: &egui::Context) {
    use crate::i18n::{tr, trf};
    let Some((path, _, _)) = &app.open_notice else {
        return;
    };
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    egui::Window::new(trf("opening.title", &[("name", &name)]))
        .id(egui::Id::new("opening_notice"))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_width(360.0);
            ui.label(tr("opening.wait"));
            ui.add(egui::ProgressBar::new(1.0).animate(true));
        });
    ctx.request_repaint();
}
