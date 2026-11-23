//! A large drawing written to its `.tdraw` file on a worker thread, with
//! a window that shows it is being saved. Packing and writing a drawing
//! of hundreds of megabytes takes seconds, and the interface froze
//! meanwhile with nothing saying why (#4011).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc};

/// A file from this size on is written on a worker thread the next time
/// it is saved.
pub const BACKGROUND_FROM: u64 = 2 * 1024 * 1024;

/// A drawing of this many objects on its layers or more is written on a
/// worker thread, whatever its file (a new drawing has none yet).
pub const BACKGROUND_FROM_OBJECTS: usize = 10_000;

/// How long the writing goes on before the window shows: a file written
/// sooner is saved without it flashing up.
pub const WINDOW_AFTER: std::time::Duration = std::time::Duration::from_millis(400);

/// A `.tdraw` file being written on a worker thread.
pub struct Saving {
    pub path: PathBuf,
    /// The bytes written into the file so far.
    pub written: Arc<AtomicU64>,
    /// The drawing saved and its state when its copy was taken (the
    /// engine's instance and revision): what is marked as saved once the
    /// file is written.
    pub state: (u64, u64),
    pub started: web_time::Instant,
    rx: mpsc::Receiver<Result<(), String>>,
}

/// Whether the drawing `doc` saved to `path` is one to write on a worker
/// thread: a large file, or many objects.
pub fn is_large(path: &Path, doc: &tracedraw_core::Document) -> bool {
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    size >= BACKGROUND_FROM
        || doc.all_layers().map(|l| l.shapes.len()).sum::<usize>() >= BACKGROUND_FROM_OBJECTS
}

impl Saving {
    /// Start writing `doc` (a copy of the drawing, taken at `state`) to
    /// `path` in format `version`.
    pub fn start(
        path: PathBuf,
        doc: tracedraw_core::Document,
        version: u32,
        state: (u64, u64),
    ) -> Result<Saving, String> {
        let written = Arc::new(AtomicU64::new(0));
        let (tx, rx) = mpsc::channel();
        let (p, count) = (path.clone(), written.clone());
        std::thread::Builder::new()
            .name("save".into())
            .spawn(move || {
                let _ = tx.send(write_tdraw(&p, &doc, version, &count));
            })
            .map_err(|e| e.to_string())?;
        Ok(Saving {
            path,
            written,
            state,
            started: web_time::Instant::now(),
            rx,
        })
    }

    /// The outcome once the file is written.
    pub fn poll(&self) -> Option<Result<(), String>> {
        match self.rx.try_recv() {
            Ok(r) => Some(r),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err("the writing stopped".into())),
        }
    }

    /// Wait until the file is written (the program is closing, or the
    /// drawing is saved again): its outcome.
    pub fn wait(&self) -> Result<(), String> {
        self.rx
            .recv()
            .unwrap_or_else(|_| Err("the writing stopped".into()))
    }
}

/// Write `doc` to the file `path`, never losing the file that was there
/// (`files::write_with`), counting the bytes in `written`.
fn write_tdraw(
    path: &Path,
    doc: &tracedraw_core::Document,
    version: u32,
    written: &Arc<AtomicU64>,
) -> Result<(), String> {
    crate::files::write_with(path, |w| {
        doc.write_tdraw_version(
            Counting {
                inner: w,
                written: written.clone(),
            },
            version,
        )
        .map_err(std::io::Error::other)
    })
    .map_err(|e| e.to_string())
}

/// Counts the bytes written through it.
struct Counting<'a> {
    inner: &'a mut dyn Write,
    written: Arc<AtomicU64>,
}

impl Write for Counting<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.written.fetch_add(n as u64, Ordering::Relaxed);
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// What the user does is held back while a large drawing is written:
/// the keys, clicks, touches and dropped files of `raw` are left out
/// (the window can still be moved, resized and closed; the pointer's
/// place is kept, so nothing jumps when the input comes back).
pub fn hold_input(raw: &mut egui::RawInput) {
    raw.events
        .retain(|e| matches!(e, egui::Event::PointerMoved(_) | egui::Event::WindowFocused(_)));
    raw.dropped_files.clear();
    raw.hovered_files.clear();
}

/// The window of a large drawing being saved, once it has taken
/// [`WINDOW_AFTER`]: its name, how much is written and a bar that moves
/// while it is. Nothing is typed or clicked into the drawing meanwhile
/// (`App::saving` holds the input back), so there is nothing to cancel.
pub fn window(app: &mut crate::app::App, ctx: &egui::Context) {
    use crate::i18n::trf;
    let Some(job) = &app.saving else {
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
    let mb = format!(
        "{:.1}",
        job.written.load(Ordering::Relaxed) as f64 / (1024.0 * 1024.0)
    );
    egui::Window::new(trf("saving.title", &[("name", &name)]))
        .id(egui::Id::new("saving_large_drawing"))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_width(360.0);
            ui.label(trf("saving.written", &[("done", &mb)]));
            ui.add(egui::ProgressBar::new(1.0).animate(true));
        });
    ctx.request_repaint_after(std::time::Duration::from_millis(50));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A drawing written on the worker thread is the same file as one
    /// written at once, every byte counted; a folder that is not there
    /// gives the error and no file.
    #[test]
    fn a_drawing_is_written_on_a_worker_with_its_bytes_counted() {
        let dir = std::env::temp_dir().join(format!("td-saving-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let doc =
            tracedraw_core::Document::new("big", tracedraw_core::geometry::Size::new(100.0, 100.0));
        for (name, version) in [("packed.tdraw", 2), ("plain.tdraw", 0)] {
            let mut want = Vec::new();
            doc.write_tdraw_version(&mut want, version).expect("write");
            let path = dir.join(name);
            // Written over a file already there, too.
            for _ in 0..2 {
                let job = Saving::start(path.clone(), doc.clone(), version, (1, 2)).expect("starts");
                job.wait().expect("written");
                assert_eq!(job.written.load(Ordering::Relaxed), want.len() as u64);
                assert_eq!(std::fs::read(&path).expect("file"), want, "{name}");
                assert_eq!(job.state, (1, 2));
            }
            assert!(!is_large(&path, &doc));
        }
        let gone = dir.join("no").join("folder").join("x.tdraw");
        let job = Saving::start(gone.clone(), doc.clone(), 2, (0, 0)).expect("starts");
        let done = loop {
            if let Some(r) = job.poll() {
                break r;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        assert!(done.is_err());
        assert!(!gone.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
