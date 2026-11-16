//! A large `.tdraw` file read on a worker thread, with a window that
//! shows how far it got and can cancel it. A packed drawing of hundreds
//! of megabytes takes seconds to decompress and read, and the interface
//! froze meanwhile (#2849).

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc};

/// Files from this size on are read on a worker thread.
pub const BACKGROUND_FROM: u64 = 8 * 1024 * 1024;

/// How long the reading goes on before the window shows: a file read
/// sooner opens without it flashing up.
pub const WINDOW_AFTER: std::time::Duration = std::time::Duration::from_millis(500);

/// What a finished read gives: the drawing and its format version.
pub type OpenRead = Result<(tracedraw_core::Document, u32), String>;

/// A `.tdraw` file being read on a worker thread.
pub struct Opening {
    pub path: PathBuf,
    pub password: String,
    /// The file's size and the bytes of it read so far.
    pub size: u64,
    pub done_bytes: Arc<AtomicU64>,
    /// The text is read: the drawing is being built from it.
    pub building: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
    pub started: web_time::Instant,
    rx: mpsc::Receiver<OpenRead>,
}

/// Whether `path` is a file to read on a worker thread: a large one, on
/// a desktop or phone (never in a browser, whose files are in memory,
/// nor in the tests, which open files at once).
pub fn wants_background(path: &Path) -> Option<u64> {
    if crate::files::WEB || cfg!(test) {
        return None;
    }
    let size = std::fs::metadata(path).ok()?.len();
    (size >= BACKGROUND_FROM).then_some(size)
}

impl Opening {
    pub fn start(path: PathBuf, password: String, size: u64) -> Opening {
        let done_bytes = Arc::new(AtomicU64::new(0));
        let building = Arc::new(AtomicBool::new(false));
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let (p, d, b, c) = (
            path.clone(),
            done_bytes.clone(),
            building.clone(),
            cancel.clone(),
        );
        std::thread::spawn(move || {
            let read = read_tdraw(&p, &d, &b, &c);
            let _ = tx.send(read);
        });
        Opening {
            path,
            password,
            size,
            done_bytes,
            building,
            cancel,
            started: web_time::Instant::now(),
            rx,
        }
    }

    /// The read's result once it is done.
    pub fn poll(&self) -> Option<OpenRead> {
        match self.rx.try_recv() {
            Ok(r) => Some(r),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err("the reading stopped".into())),
        }
    }

    /// Stop reading: the worker gives up at its next block (a drawing
    /// being built finishes and is dropped).
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// How much of the file is read, 0 to 1.
    pub fn fraction(&self) -> f32 {
        let done = self.done_bytes.load(Ordering::Relaxed) as f64;
        (done / self.size.max(1) as f64).clamp(0.0, 1.0) as f32
    }
}

/// The window of a large drawing being opened, once it has taken
/// [`WINDOW_AFTER`]: its name, a bar that fills as the file is read (and
/// moves on its own while the drawing is built from it), and Cancel.
pub fn window(app: &mut crate::app::App, ctx: &egui::Context) {
    use crate::i18n::{tr, trf};
    let Some(job) = &app.opening else {
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
    let building = job.building.load(Ordering::Relaxed);
    let fraction = job.fraction();
    let mut cancel = false;
    egui::Window::new(trf("opening.title", &[("name", &name)]))
        .id(egui::Id::new("opening_large_drawing"))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_width(360.0);
            if building {
                ui.label(tr("opening.building"));
                ui.add(egui::ProgressBar::new(1.0).animate(true));
            } else {
                let mb = |b: u64| format!("{:.0}", b as f64 / (1024.0 * 1024.0));
                ui.label(trf(
                    "opening.reading",
                    &[
                        ("done", &mb(job.done_bytes.load(Ordering::Relaxed))),
                        ("size", &mb(job.size)),
                    ],
                ));
                ui.add(egui::ProgressBar::new(fraction).show_percentage());
            }
            ui.add_space(4.0);
            ui.vertical_centered(|ui| {
                if ui.button(tr("dialog.cancel")).clicked() {
                    cancel = true;
                }
            });
        });
    if cancel {
        app.cancel_opening();
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(50));
}

/// Counts the bytes read through it, and stops when cancelled.
struct Counting<R> {
    inner: R,
    done: Arc<AtomicU64>,
    cancel: Arc<AtomicBool>,
}

impl<R: Read> Read for Counting<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "cancelled",
            ));
        }
        let n = self.inner.read(buf)?;
        self.done.fetch_add(n as u64, Ordering::Relaxed);
        Ok(n)
    }
}

/// Read a `.tdraw` file, packed or plain, counting its bytes in `done`.
fn read_tdraw(
    path: &Path,
    done: &Arc<AtomicU64>,
    building: &AtomicBool,
    cancel: &Arc<AtomicBool>,
) -> OpenRead {
    let text = read_text(path, done, cancel).map_err(|e| e.to_string())?;
    if cancel.load(Ordering::Relaxed) {
        return Err("cancelled".into());
    }
    building.store(true, Ordering::Relaxed);
    let version = tracedraw_core::document::tdraw_text_version(&text)
        .min(tracedraw_core::document::TDRAW_FORMAT_VERSION);
    let doc = tracedraw_core::Document::from_json(&text).map_err(|e| e.to_string())?;
    Ok((doc, version))
}

/// The JSON text of a `.tdraw` file (decompressed when packed).
fn read_text(
    path: &Path,
    done: &Arc<AtomicU64>,
    cancel: &Arc<AtomicBool>,
) -> std::io::Result<String> {
    let file = std::fs::File::open(path)?;
    let mut r = std::io::BufReader::with_capacity(
        1 << 20,
        Counting {
            inner: file,
            done: done.clone(),
            cancel: cancel.clone(),
        },
    );
    // Packed or plain, decoded as it is read.
    tracedraw_core::document::read_tdraw_text(&mut r)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file read on the worker thread gives the drawing and its version,
    /// packed or plain, and counts every byte; a cancelled read gives up.
    #[test]
    fn a_large_file_is_read_on_a_worker_and_can_be_cancelled() {
        let dir = std::env::temp_dir().join(format!("td-opening-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let doc =
            tracedraw_core::Document::new("big", tracedraw_core::geometry::Size::new(100.0, 100.0));
        for (name, version) in [("packed.tdraw", 2), ("plain.tdraw", 0)] {
            let path = dir.join(name);
            let mut out = Vec::new();
            doc.write_tdraw_version(&mut out, version).expect("write");
            std::fs::write(&path, &out).expect("write");
            let job = Opening::start(path, String::new(), out.len() as u64);
            let read = loop {
                if let Some(r) = job.poll() {
                    break r;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            };
            let (back, v) = read.expect("reads");
            assert_eq!(v, version);
            assert_eq!(back.pages.len(), doc.pages.len());
            assert_eq!(job.fraction(), 1.0, "{name}");
            assert!(job.building.load(Ordering::Relaxed));
        }
        // Cancelled before it starts reading: it gives up.
        let done = Arc::new(AtomicU64::new(0));
        let cancel = Arc::new(AtomicBool::new(true));
        let building = AtomicBool::new(false);
        assert!(read_tdraw(&dir.join("packed.tdraw"), &done, &building, &cancel).is_err());
        assert!(!building.load(Ordering::Relaxed));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
