//! What a start leaves behind when it goes wrong. The Windows build has
//! no console, so the log and a panic went nowhere and a start that
//! failed showed nothing at all (#570). The log goes to `tracedraw.log`
//! in the settings folder as well (written afresh each start), and a
//! panic is written to `crash.log` there and shown in a message box.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The log file of this run, in `dir`.
pub fn log_path(dir: &Path) -> PathBuf {
    dir.join("tracedraw.log")
}

/// Where a panic is written, in `dir`.
pub fn crash_path(dir: &Path) -> PathBuf {
    dir.join("crash.log")
}

/// Every log line to stderr and to the file.
struct Tee {
    file: Option<std::fs::File>,
}

impl Write for Tee {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let _ = std::io::stderr().write_all(buf);
        if let Some(f) = self.file.as_mut() {
            let _ = f.write_all(buf);
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        let _ = std::io::stderr().flush();
        if let Some(f) = self.file.as_mut() {
            let _ = f.flush();
        }
        Ok(())
    }
}

/// Start the log (stderr and the file) and the panic report.
pub fn init() {
    let dir = crate::settings::config_dir();
    // A start handed over to the next renderer adds to the log of the
    // one that failed.
    let restarted = std::env::args().any(|a| a.starts_with("--renderer="));
    let file = dir.as_ref().and_then(|d| {
        let _ = std::fs::create_dir_all(d);
        std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(restarted)
            .truncate(!restarted)
            .open(log_path(d))
            .ok()
    });
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .target(env_logger::Target::Pipe(Box::new(Tee { file })))
        .init();
    log::info!(
        "TraceDraw {} on {} {}",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    let dir = Mutex::new(dir);
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let report = report(info);
        log::error!("{report}");
        let saved = dir.lock().ok().and_then(|d| d.clone()).and_then(|d| {
            let path = crash_path(&d);
            std::fs::write(&path, &report).ok().map(|_| path)
        });
        default(info);
        show(&report, saved.as_deref());
    }));
}

/// The panic as text: where and what, and the version.
pub fn report(info: &std::panic::PanicHookInfo<'_>) -> String {
    let what = info
        .payload()
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| info.payload().downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown".into());
    let place = info
        .location()
        .map(|l| format!("{}:{}", l.file(), l.line()))
        .unwrap_or_default();
    format!(
        "TraceDraw {} ({} {}) stopped: {what} at {place}\n{}",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        std::backtrace::Backtrace::force_capture()
    )
}

/// Tell the user, who would otherwise see nothing (no console on
/// Windows). Once: a panic while showing it shows nothing more.
fn show(report: &str, saved: Option<&Path>) {
    static SHOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if SHOWN.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let first = report.lines().next().unwrap_or_default();
    let mut text = format!("{first}\n\n");
    if let Some(p) = saved {
        text.push_str(&format!(
            "The details are in {}. Please attach that file to an issue at https://github.com/igtoth/tracedraw/issues",
            p.display()
        ));
    }
    let _ = rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title("TraceDraw")
        .set_description(text)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The files sit in the settings folder; a panic's report says the
    /// version, what happened and where.
    #[test]
    fn the_log_and_the_crash_report_go_in_the_settings_folder() {
        let d = Path::new("/somewhere/TraceDraw");
        assert_eq!(log_path(d), d.join("tracedraw.log"));
        assert_eq!(crash_path(d), d.join("crash.log"));
        let seen = std::sync::Arc::new(Mutex::new(String::new()));
        let keep = seen.clone();
        let before = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            // Other tests may panic meanwhile: only this one counts.
            let r = report(info);
            if r.contains("no adapter") {
                *keep.lock().unwrap() = r;
            }
        }));
        let _ = std::panic::catch_unwind(|| panic!("no adapter"));
        std::panic::set_hook(before);
        let text = seen.lock().unwrap().clone();
        assert!(text.contains("no adapter"), "{text}");
        assert!(text.contains(env!("CARGO_PKG_VERSION")));
        assert!(text.contains("startup_log.rs"));
    }
}
