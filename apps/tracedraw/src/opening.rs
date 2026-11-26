//! A large `.tdraw` file read on a worker thread, with a window that
//! shows how far it got and can cancel it. A packed drawing of hundreds
//! of megabytes takes seconds to decompress and read, and the interface
//! froze meanwhile (#2849). A `.cdr` or `.cmx` goes to the worker the
//! same way.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc};

/// Files from this size on are read on a worker thread.
pub const BACKGROUND_FROM: u64 = 8 * 1024 * 1024;

/// How long the reading goes on before the window shows: a file read
/// sooner opens without it flashing up.
pub const WINDOW_AFTER: std::time::Duration = std::time::Duration::from_millis(500);

/// What a finished read gives.
pub enum OpenedFile {
    /// A `.tdraw` drawing and its format version.
    Tdraw(tracedraw_core::Document, u32),
    /// A `.cdr` or `.cmx` drawing and what the reader reported.
    Cdr(tracedraw_core::Document, Box<tracedraw_cdr::ParseReport>),
    /// A `.cdr` the reader refused as damaged: the error, and why, for
    /// the window that offers to open it anyway (#4332).
    Damaged { message: String, why: String },
}

/// What a finished read gives.
pub type OpenRead = Result<OpenedFile, String>;

/// Whether the file is read by the `.cdr` reader (a `.cdr`, `.cmx`,
/// `.cdt`, `.cdrt` or `.csl` file).
fn is_cdr(path: &Path) -> bool {
    path.extension().is_some_and(|e| {
        e.eq_ignore_ascii_case("cdr")
            || e.eq_ignore_ascii_case("cmx")
            || e.eq_ignore_ascii_case("cdt")
            || e.eq_ignore_ascii_case("cdrt")
            || e.eq_ignore_ascii_case("csl")
    })
}

/// A drawing being read on a worker thread.
pub struct Opening {
    pub path: PathBuf,
    pub password: String,
    /// The file's size and the bytes of it read so far.
    pub size: u64,
    pub done_bytes: Arc<AtomicU64>,
    /// The file is read: the drawing is being built from it.
    pub building: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
    pub started: web_time::Instant,
    rx: mpsc::Receiver<OpenRead>,
}

/// Whether `path` is a file to read on a worker thread: a large one, on
/// a desktop or phone (never in a browser, whose files are in memory,
/// nor in the tests, which open files at once).
pub fn wants_background(path: &Path) -> Option<u64> {
    wants_background_from(path, BACKGROUND_FROM)
}

/// [`wants_background`] with an explicit byte threshold `from`.
pub fn wants_background_from(path: &Path, from: u64) -> Option<u64> {
    if crate::files::WEB || cfg!(test) {
        return None;
    }
    let size = std::fs::metadata(path).ok()?.len();
    (size >= from).then_some(size)
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
        let cdr = is_cdr(&path);
        std::thread::spawn(move || {
            let read = if cdr {
                read_cdr(&p, &d, &b, &c)
            } else {
                read_tdraw(&p, &d, &b, &c)
            };
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
    Ok(OpenedFile::Tdraw(doc, version))
}

/// Read a `.cdr` or `.cmx` file, counting its bytes in `done`, then
/// parsing it (the bar then says the drawing is being built). The free
/// RAM is told to the reader first, so a large picture is sampled.
fn read_cdr(
    path: &Path,
    done: &Arc<AtomicU64>,
    building: &AtomicBool,
    cancel: &Arc<AtomicBool>,
) -> OpenRead {
    let bytes = read_bytes(path, done, cancel).map_err(|e| e.to_string())?;
    if cancel.load(Ordering::Relaxed) {
        return Err("cancelled".into());
    }
    building.store(true, Ordering::Relaxed);
    let free = free_ram();
    tracedraw_cdr::set_open_ram(free);
    let title = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled")
        .to_string();
    tracedraw_cdr::open_bytes(&bytes, &title)
        .map(|(mut doc, report)| {
            // Clones saved by TraceDraw follow their masters again (#2879).
            crate::object_clone::rebase_clone_links(&mut doc);
            OpenedFile::Cdr(doc, Box::new(report))
        })
        .or_else(|e| match e {
            tracedraw_cdr::Error::Damaged(ref why) => Ok(OpenedFile::Damaged {
                message: e.to_string(),
                why: why.clone(),
            }),
            e => Err(e.to_string()),
        })
}

/// Free physical RAM in bytes; 0 when the OS will not say.
pub(crate) fn free_ram() -> u64 {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn GlobalMemoryStatusEx(lpBuffer: *mut MEMORYSTATUSEX) -> i32;
        }
        #[repr(C)]
        struct MEMORYSTATUSEX {
            dw_length: u32,
            dw_memory_load: u32,
            ull_total_phys: u64,
            ull_avail_phys: u64,
            ull_total_page_file: u64,
            ull_avail_page_file: u64,
            ull_total_virtual: u64,
            ull_avail_virtual: u64,
            ull_avail_extended_virtual: u64,
        }
        let mut s: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
        s.dw_length = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        if unsafe { GlobalMemoryStatusEx(&mut s) } != 0 {
            s.ull_avail_phys
        } else {
            0
        }
    }
    #[cfg(not(windows))]
    {
        // Linux: MemAvailable of /proc/meminfo, in KiB.
        #[cfg(target_os = "linux")]
        {
            let text = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
            for line in text.lines() {
                if let Some(rest) = line.strip_prefix("MemAvailable:") {
                    let kib: u64 = rest
                        .trim()
                        .trim_end_matches("kB")
                        .trim()
                        .parse()
                        .unwrap_or(0);
                    return kib.saturating_mul(1024);
                }
            }
            0
        }
        #[cfg(not(target_os = "linux"))]
        {
            0
        }
    }
}

/// The bytes of `path`, counted in `done` and stopped when cancelled.
fn read_bytes(
    path: &Path,
    done: &Arc<AtomicU64>,
    cancel: &Arc<AtomicBool>,
) -> std::io::Result<Vec<u8>> {
    let file = std::fs::File::open(path)?;
    let cap = file.metadata().map(|m| m.len() as usize).unwrap_or(0);
    let mut r = std::io::BufReader::with_capacity(
        1 << 20,
        Counting {
            inner: file,
            done: done.clone(),
            cancel: cancel.clone(),
        },
    );
    let mut out = Vec::with_capacity(cap.min(512 << 20));
    r.read_to_end(&mut out)?;
    Ok(out)
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
            let OpenedFile::Tdraw(back, v) = read.expect("reads") else {
                panic!("a .tdraw read gives a .tdraw drawing");
            };
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

    /// A `.cdr` file is read on the worker as well: the drawing comes
    /// back with the reader's report, every byte counted.
    #[test]
    fn a_cdr_file_is_read_on_a_worker() {
        let dir = std::env::temp_dir().join(format!("td-opening-cdr-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let mut doc = tracedraw_core::Document::new(
            "from cdr",
            tracedraw_core::geometry::Size::new(80.0, 60.0),
        );
        let id = doc.ids_mut().shape();
        let mut r = tracedraw_core::Shape::new(
            id,
            tracedraw_core::ShapeKind::Rect {
                rect: tracedraw_core::Rect::new(10.0, 10.0, 40.0, 30.0),
                radius: 0.0,
                corners: None,
            },
        );
        r.fill = tracedraw_core::Fill::Solid(tracedraw_core::Color::rgb8(200, 40, 40));
        doc.pages[0].layers[0].shapes.push(r);
        let path = dir.join("drawing.cdr");
        let bytes = tracedraw_cdr::write::document_to_cdr(&doc);
        std::fs::write(&path, &bytes).expect("write");
        let job = Opening::start(path, String::new(), bytes.len() as u64);
        let read = loop {
            if let Some(r) = job.poll() {
                break r;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        let OpenedFile::Cdr(back, report) = read.expect("reads") else {
            panic!("a .cdr read gives a .cdr drawing");
        };
        assert_eq!(back.pages.len(), 1);
        assert_eq!(report.shapes, 1);
        assert_eq!(report.pages, 1);
        assert_eq!(job.fraction(), 1.0);
        assert!(job.building.load(Ordering::Relaxed));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
