//! A printer-list subprocess cannot reserve an unbounded output buffer or
//! wait forever. Read its stdout without blocking; stderr is not used by
//! the printer parser and goes to the null device.

use std::ffi::OsStr;
use std::io::{self, Read};
use std::os::fd::AsRawFd;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct Reap(Child);

impl Drop for Reap {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

/// Preserve descriptions from a working verbose query. If it fails or
/// exceeds its bounds, retry the compact query for names/state/default.
pub fn lpstat(program: impl AsRef<OsStr>, limit: usize, timeout: Duration) -> io::Result<Vec<u8>> {
    let program = program.as_ref();
    let run = |args: &[&str]| {
        output(
            Command::new(program)
                .args(args)
                .env("LC_ALL", "C")
                .env("LANG", "C"),
            limit,
            timeout,
        )
    };
    run(&["-l", "-p", "-d"]).or_else(|_| run(&["-p", "-d"]))
}

/// Return a complete successful query only. Limits apply while bytes are
/// read, before parsing, and a failed or timed-out child is killed/reaped.
fn output(command: &mut Command, limit: usize, timeout: Duration) -> io::Result<Vec<u8>> {
    let mut child = Reap(
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?,
    );
    let mut pipe = child
        .0
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("printer query has no output pipe"))?;
    let fd = pipe.as_raw_fd();
    // SAFETY: `pipe` owns this live file descriptor for both calls. The
    // F_GETFL command takes no third argument; F_SETFL takes these flags.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(io::Error::last_os_error());
    }
    let start = Instant::now();
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 8192];
    let mut eof = false;
    loop {
        if start.elapsed() >= timeout {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "printer query timed out",
            ));
        }
        match pipe.read(&mut buffer) {
            Ok(0) => eof = true,
            Ok(n) => {
                if n > limit.saturating_sub(bytes.len()) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "printer query output limit exceeded",
                    ));
                }
                if let Some(part) = buffer.get(..n) {
                    bytes.extend_from_slice(part);
                }
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
        if let Some(status) = child.0.try_wait()? {
            if !status.success() {
                return Err(io::Error::other(format!(
                    "printer query exited unsuccessfully: {status}"
                )));
            }
            if eof {
                return Ok(bytes);
            }
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct Fixture {
        dir: PathBuf,
        program: PathBuf,
    }

    impl Fixture {
        fn new(script: &str) -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let dir = std::env::temp_dir().join(format!(
                "tracedraw-printer-fixture-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&dir).expect("fixture directory");
            let program = dir.join("lpstat");
            std::fs::write(&program, format!("#!/bin/sh\n{script}\n")).expect("fixture script");
            std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700))
                .expect("fixture permissions");
            Self { dir, program }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn shell(script: &str) -> Command {
        let mut c = Command::new("sh");
        c.args(["-c", script]);
        c
    }

    #[test]
    fn complete_output_is_retained_and_stderr_is_discarded() {
        let data = output(
            &mut shell("printf 'printer sample is idle.\n'; printf ignored >&2"),
            1024,
            Duration::from_secs(2),
        )
        .expect("query");
        assert_eq!(data, b"printer sample is idle.\n");
    }

    #[test]
    fn an_output_exactly_at_the_limit_is_complete() {
        assert_eq!(
            output(&mut shell("printf abcd"), 4, Duration::from_secs(2)).expect("query"),
            b"abcd"
        );
    }

    #[test]
    fn a_repeating_list_cannot_grow_memory_without_bound() {
        let result = output(
            &mut shell("while :; do printf 'printer sample is idle.\n'; done"),
            1024,
            Duration::from_secs(2),
        );
        assert_eq!(
            result.expect_err("reject output").kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn a_stuck_query_is_timed_out_and_reaped() {
        let path =
            std::env::temp_dir().join(format!("tracedraw-printer-query-{}", std::process::id()));
        let mut command = shell("echo $$ > \"$PID_FILE\"; exec sleep 30");
        command.env("PID_FILE", &path);
        let start = Instant::now();
        let result = output(&mut command, 1024, Duration::from_millis(150));
        assert_eq!(result.expect_err("timeout").kind(), io::ErrorKind::TimedOut);
        assert!(start.elapsed() < Duration::from_secs(3));
        #[cfg(target_os = "linux")]
        {
            let pid = std::fs::read_to_string(&path).expect("child PID");
            assert!(!std::path::Path::new("/proc").join(pid.trim()).exists());
        }
        std::fs::remove_file(path).expect("remove test file");
    }

    #[test]
    fn an_inherited_pipe_does_not_extend_the_deadline() {
        let start = Instant::now();
        let result = output(
            &mut shell("sleep 2 & exit 0"),
            1024,
            Duration::from_millis(100),
        );
        assert_eq!(
            result.expect_err("inherited pipe").kind(),
            io::ErrorKind::TimedOut
        );
        assert!(start.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn unbounded_stderr_never_becomes_an_output_buffer() {
        let result = output(
            &mut shell("while :; do printf diagnostic >&2; done"),
            1024,
            Duration::from_millis(100),
        );
        assert_eq!(result.expect_err("timeout").kind(), io::ErrorKind::TimedOut);
    }

    #[test]
    fn unsuccessful_output_is_not_a_partial_printer_list() {
        let result = output(
            &mut shell("printf 'printer incomplete'; exit 2"),
            1024,
            Duration::from_secs(2),
        );
        assert_eq!(
            result.expect_err("exit failure").kind(),
            io::ErrorKind::Other
        );
    }

    #[test]
    fn a_missing_command_returns_an_error() {
        let mut command = Command::new("/tracedraw-test-no-such-printer-command");
        assert_eq!(
            output(&mut command, 1024, Duration::from_secs(1))
                .expect_err("missing command")
                .kind(),
            io::ErrorKind::NotFound
        );
    }

    #[test]
    fn a_working_verbose_query_keeps_description_and_location() {
        let fixture = Fixture::new(
            "if [ \"$1\" = -l ]; then printf 'printer sample is idle.\\n\\tDescription: Sample printer\\n\\tLocation: Sample room\\n'; else exit 7; fi",
        );
        let data = lpstat(&fixture.program, 1024, Duration::from_secs(2)).expect("verbose query");
        assert!(String::from_utf8_lossy(&data).contains("Description: Sample printer"));
        assert!(String::from_utf8_lossy(&data).contains("Location: Sample room"));
    }

    #[test]
    fn repeating_verbose_output_falls_back_to_a_complete_compact_list() {
        let fixture = Fixture::new(
            "if [ \"$1\" = -l ]; then while :; do printf 'printer repeated is idle.\\n'; done; else printf 'printer sample is idle.\\nsystem default destination: sample\\n'; fi",
        );
        let data = lpstat(&fixture.program, 128, Duration::from_secs(2)).expect("compact fallback");
        assert_eq!(
            data,
            b"printer sample is idle.\nsystem default destination: sample\n"
        );
    }

    #[test]
    fn a_stuck_verbose_query_falls_back_to_the_compact_query() {
        let fixture = Fixture::new(
            "if [ \"$1\" = -l ]; then exec sleep 30; else printf 'printer sample is idle.\\n'; fi",
        );
        let data = lpstat(&fixture.program, 1024, Duration::from_millis(150))
            .expect("compact fallback after timeout");
        assert_eq!(data, b"printer sample is idle.\n");
    }

    #[test]
    fn both_query_attempts_remain_bounded() {
        let fixture = Fixture::new("while :; do printf 'repeating output\\n'; done");
        let result = lpstat(&fixture.program, 64, Duration::from_secs(2));
        assert_eq!(
            result.expect_err("bounded attempts").kind(),
            io::ErrorKind::InvalidData
        );
    }
}
