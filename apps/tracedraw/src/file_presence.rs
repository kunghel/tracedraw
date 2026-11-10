//! Whether files are on the disk, asked from the interface without
//! waiting for the disk (#1388). The Welcome Screen greys out a recent
//! drawing that is gone, and it asked the disk about every one of them
//! every frame, on the window's thread: a drawing on a network share that
//! does not answer, or a drive that is asleep, held the window for as
//! long as the system took to give up (minutes for a share). Here the
//! question goes to a worker thread, at most one at a time for a file and
//! again a little later, and the window takes the last answer: a file not
//! looked at yet counts as there.
//!
//! The same question can bring back the file's size and when it was last
//! changed, for the Welcome Screen's list (#1675).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use web_time::{Duration, Instant};

/// How long an answer stands before the disk is asked again (a file
/// deleted or put back while the Welcome Screen is up).
pub const RECHECK: Duration = Duration::from_secs(2);

/// What the disk says of a file besides its being there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FileInfo {
    /// Its size in bytes.
    pub size: Option<u64>,
    /// When it was last changed, seconds since 1970.
    pub modified: Option<i64>,
}

#[derive(Debug, Clone, Copy)]
struct Entry {
    /// The last answer; `None` before the first.
    exists: Option<bool>,
    /// The size and date of a file that is there, when asked for.
    info: Option<FileInfo>,
    /// When the last question was asked.
    asked: Instant,
    /// A question is under way.
    pending: bool,
}

/// Answers about files, with the question asked by `check` on worker
/// threads.
pub struct Presence {
    entries: Arc<Mutex<HashMap<PathBuf, Entry>>>,
    check: fn(&Path) -> bool,
    /// Reads a present file's size and date along with the question.
    inspect: Option<fn(&Path) -> FileInfo>,
}

impl Presence {
    pub fn new(check: fn(&Path) -> bool) -> Self {
        Presence {
            entries: Arc::new(Mutex::new(HashMap::new())),
            check,
            inspect: None,
        }
    }

    /// The same, also reading a present file's size and date with
    /// `inspect` on the worker thread.
    pub fn with_info(mut self, inspect: fn(&Path) -> FileInfo) -> Self {
        self.inspect = Some(inspect);
        self
    }

    /// The size and date of `path` as last read; `None` before the first
    /// answer, for a file that is not there, or without `with_info`.
    /// Never asks the disk: [`exists`](Self::exists) does.
    pub fn info(&self, path: &Path) -> Option<FileInfo> {
        self.entries.lock().ok()?.get(path)?.info
    }

    /// Whether `path` is there, as last found; never waits for the disk.
    /// A question goes out when there is no answer yet or it is older than
    /// [`RECHECK`]; `wake` is asked to repaint when an answer changes.
    pub fn exists(&self, path: &Path, wake: Option<&egui::Context>) -> bool {
        let now = Instant::now();
        let Ok(mut entries) = self.entries.lock() else {
            return true;
        };
        let entry = entries.entry(path.to_path_buf()).or_insert(Entry {
            exists: None,
            info: None,
            asked: now,
            pending: false,
        });
        let known = entry.exists;
        let due = known.is_none() || now.duration_since(entry.asked) >= RECHECK;
        if due && !entry.pending {
            entry.pending = true;
            entry.asked = now;
            drop(entries);
            self.ask(path.to_path_buf(), wake.cloned());
        }
        known.unwrap_or(true)
    }

    /// Ask the disk about `path` on a worker thread (in the browser, which
    /// has no disk to wait for, at once).
    fn ask(&self, path: PathBuf, wake: Option<egui::Context>) {
        let entries = Arc::clone(&self.entries);
        let check = self.check;
        let inspect = self.inspect;
        let key = path.clone();
        let answer = move || {
            let exists = check(&path);
            let info = inspect.filter(|_| exists).map(|f| f(&path));
            let changed = entries.lock().is_ok_and(|mut e| {
                let entry = e.entry(path).or_insert(Entry {
                    exists: None,
                    info: None,
                    asked: Instant::now(),
                    pending: true,
                });
                let changed = entry.exists != Some(exists) || entry.info != info;
                entry.exists = Some(exists);
                entry.info = info;
                entry.pending = false;
                changed
            });
            if changed {
                if let Some(ctx) = wake {
                    ctx.request_repaint();
                }
            }
        };
        if cfg!(target_arch = "wasm32") {
            answer();
            return;
        }
        if let Err(e) = std::thread::Builder::new()
            .name("tracedraw-file-presence".into())
            .spawn(answer)
        {
            // The question is asked again after `RECHECK`.
            log::warn!("file presence thread: {e}");
            if let Some(entry) = self
                .entries
                .lock()
                .ok()
                .as_mut()
                .and_then(|e| e.get_mut(&key))
            {
                entry.pending = false;
            }
        }
    }
}

/// The answers the window shares.
fn shared() -> &'static Presence {
    static SHARED: OnceLock<Presence> = OnceLock::new();
    SHARED.get_or_init(|| Presence::new(|p| crate::files::exists(p)).with_info(disk_info))
}

/// Whether `path` is there (see [`Presence::exists`]), for the window.
pub fn exists(path: &Path, wake: Option<&egui::Context>) -> bool {
    shared().exists(path, wake)
}

/// The size and date of `path` as last read (see [`Presence::info`]).
pub fn info(path: &Path) -> Option<FileInfo> {
    shared().info(path)
}

/// A file's size and date from the disk (nothing for a document the
/// system's picker handed over, which has no path to read).
pub fn disk_info(path: &Path) -> FileInfo {
    let Ok(meta) = std::fs::metadata(path) else {
        return FileInfo::default();
    };
    FileInfo {
        size: Some(meta.len()),
        modified: meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .and_then(|d| i64::try_from(d.as_secs()).ok()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    fn wait_for(mut done: impl FnMut() -> bool) -> bool {
        let start = std::time::Instant::now();
        while start.elapsed() < std::time::Duration::from_secs(10) {
            if done() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        false
    }

    /// A file not looked at yet counts as there, and the answer of the
    /// disk follows: a missing file turns false, a present one stays true.
    #[test]
    fn the_answer_comes_from_the_disk() {
        let dir = std::env::temp_dir().join(format!("td-presence-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let here = dir.join("here.tdraw");
        std::fs::write(&here, b"x").expect("write");
        let gone = dir.join("gone.tdraw");
        let p = Presence::new(|p| p.is_file());
        assert!(p.exists(&gone, None), "not looked at yet");
        assert!(wait_for(|| !p.exists(&gone, None)));
        assert!(p.exists(&here, None));
        assert!(wait_for(|| p
            .entries
            .lock()
            .unwrap()
            .get(&here)
            .is_some_and(|e| e.exists == Some(true))));
        assert!(p.exists(&here, None));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A disk that does not answer (a network share that hangs) never
    /// holds the window: every call returns at once, and only one
    /// question is out for the file however often the window asks
    /// (#1388).
    #[test]
    fn a_disk_that_hangs_never_holds_the_window() {
        static RELEASE: AtomicBool = AtomicBool::new(false);
        static ASKED: AtomicUsize = AtomicUsize::new(0);
        let p = Presence::new(|_| {
            ASKED.fetch_add(1, Ordering::SeqCst);
            while !RELEASE.load(Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            false
        });
        let path = Path::new("/net/share/away.tdraw");
        let start = std::time::Instant::now();
        for _ in 0..1000 {
            assert!(p.exists(path, None));
        }
        assert!(
            start.elapsed() < std::time::Duration::from_secs(1),
            "{:?}",
            start.elapsed()
        );
        assert!(wait_for(|| ASKED.load(Ordering::SeqCst) == 1));
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert_eq!(ASKED.load(Ordering::SeqCst), 1, "one question at a time");
        RELEASE.store(true, Ordering::SeqCst);
        assert!(wait_for(|| !p.exists(path, None)));
    }

    /// An answer is asked again once it is older than `RECHECK`, so a
    /// file put back shows as there again.
    #[test]
    fn an_old_answer_is_asked_again() {
        static ASKED: AtomicUsize = AtomicUsize::new(0);
        let p = Presence::new(|_| {
            ASKED.fetch_add(1, Ordering::SeqCst);
            true
        });
        let path = Path::new("/a/b.tdraw");
        assert!(p.exists(path, None));
        assert!(wait_for(|| p
            .entries
            .lock()
            .unwrap()
            .get(path)
            .is_some_and(|e| !e.pending)));
        assert!(p.exists(path, None));
        assert_eq!(ASKED.load(Ordering::SeqCst), 1, "the answer stands");
        p.entries.lock().unwrap().get_mut(path).unwrap().asked -= RECHECK;
        assert!(p.exists(path, None));
        assert!(wait_for(|| ASKED.load(Ordering::SeqCst) == 2));
    }
}
