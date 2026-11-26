//! TraceCut's plotter output: where a job goes (a `.plt` file, a port
//! path, or a cutter on the network on its raw port) and the sending,
//! on a thread of its own so a slow serial port never holds a frame.
//! The status line says how it went. Sending to a port or the network
//! needs a desktop: in a browser and on Android only the file is offered.
//! See docs/behavior/tracecut.md, Plotter Output.

use serde::{Deserialize, Serialize};
use std::sync::Mutex;

/// Whether this build can send to a port or over the network.
pub const CAN_SEND: bool = !(crate::files::WEB || crate::files::ANDROID);

/// The raw port network cutters and print servers listen on.
pub const RAW_PORT: u16 = 9100;

/// Where a plotter job goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Destination {
    /// A `.plt` file, saved where the user says.
    #[default]
    File,
    /// A port path: a serial or USB-serial device (`/dev/ttyUSB0`,
    /// `COM3`), already set to the cutter's speed.
    Port,
    /// A cutter on the network, `host` or `host:port` (9100 at first),
    /// sent as raw bytes over TCP.
    Network,
}

impl Destination {
    pub const ALL: [Destination; 3] = [Destination::File, Destination::Port, Destination::Network];

    /// The i18n key of its name; `<key>_desc` describes it.
    pub fn key(self) -> &'static str {
        match self {
            Destination::File => "tracecut.dest_file",
            Destination::Port => "tracecut.dest_port",
            Destination::Network => "tracecut.dest_network",
        }
    }

    /// The destination this build can use: a file where sending is not
    /// possible.
    pub fn usable(self) -> Destination {
        if CAN_SEND {
            self
        } else {
            Destination::File
        }
    }
}

/// The default port path of this system.
pub fn default_port() -> String {
    if cfg!(windows) {
        "COM1".into()
    } else if cfg!(target_os = "macos") {
        "/dev/cu.usbserial".into()
    } else {
        "/dev/ttyUSB0".into()
    }
}

/// `host` or `host:port` split into its host and port (9100 when none
/// is given). An IPv6 address goes in brackets with a port
/// (`[fe80::1]:9100`). `None` when there is no host or the port is not
/// a number.
pub fn host_port(text: &str) -> Option<(String, u16)> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    if let Some(rest) = t.strip_prefix('[') {
        let (host, after) = rest.split_once(']')?;
        let port = match after.strip_prefix(':') {
            Some(p) => p.parse().ok()?,
            None if after.is_empty() => RAW_PORT,
            None => return None,
        };
        return (!host.is_empty()).then(|| (host.to_string(), port));
    }
    match t.rsplit_once(':') {
        // More than one colon and no brackets: a bare IPv6 address.
        Some((h, _)) if h.contains(':') => Some((t.to_string(), RAW_PORT)),
        Some((h, p)) => {
            let port = p.parse().ok()?;
            (!h.is_empty()).then(|| (h.to_string(), port))
        }
        None => Some((t.to_string(), RAW_PORT)),
    }
}

/// Send `bytes` to a cutter at `host[:port]` as raw TCP.
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
pub fn send_network(target: &str, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    use std::net::ToSocketAddrs as _;
    let (host, port) = host_port(target)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "no host given"))?;
    // How long a connection may take to open.
    let timeout = std::time::Duration::from_secs(5);
    let mut last = None;
    for addr in (host.as_str(), port).to_socket_addrs()? {
        match std::net::TcpStream::connect_timeout(&addr, timeout) {
            Ok(mut s) => {
                s.set_write_timeout(Some(std::time::Duration::from_secs(60)))?;
                s.write_all(bytes)?;
                s.flush()?;
                let _ = s.shutdown(std::net::Shutdown::Write);
                return Ok(());
            }
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "the host has no address")
    }))
}

/// Messages of finished sends, for the status line.
static FINISHED: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Send `bytes` to `dest` (`target` the port path or the host) on a
/// thread of its own; the status line tells the result on a later frame
/// (see [`poll`]). `ctx` is asked to draw again when it is done.
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
pub fn start(dest: Destination, target: String, bytes: Vec<u8>, ctx: Option<egui::Context>) {
    std::thread::spawn(move || {
        let msg = send(dest, &target, &bytes);
        if let Ok(mut f) = FINISHED.lock() {
            f.push(msg);
        }
        if let Some(ctx) = ctx {
            ctx.request_repaint();
        }
    });
}

/// Send `bytes` to `dest` now and say how it went.
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
pub fn send(dest: Destination, target: &str, bytes: &[u8]) -> String {
    let result = match dest {
        Destination::Port => crate::files::write_device(target, bytes),
        Destination::Network => send_network(target, bytes),
        Destination::File => Ok(()),
    };
    match result {
        Ok(()) => crate::i18n::trf(
            "tracecut.sent",
            &[("n", &bytes.len().to_string()), ("to", target)],
        ),
        Err(e) => crate::i18n::trf(
            "tracecut.send_failed",
            &[("to", target), ("e", &e.to_string())],
        ),
    }
}

/// How long a controller may take to answer one line before the send
/// gives up (a full planner holds the `ok` until a move finishes).
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
pub const LINE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// Send a G-code program a line at a time over `to`, waiting after each
/// for the controller's `ok` read from `answers` (GRBL and Marlin buffer
/// only a few lines). Comments and blank lines are left out. An `error`,
/// `ALARM` or `!!` answer, a closed connection or no answer (a read that
/// times out) stops the send: `M5` goes out so the beam is off, and the
/// error names the line. Returns the lines sent.
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
pub fn stream_lines(
    to: &mut dyn std::io::Write,
    answers: &mut dyn std::io::BufRead,
    gcode: &str,
) -> std::io::Result<usize> {
    let mut sent = 0;
    let mut answer = String::new();
    for (n, raw) in gcode.lines().enumerate() {
        let line = raw.split(';').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        to.write_all(line.as_bytes())?;
        to.write_all(b"\n")?;
        to.flush()?;
        let fail = |what: &str| std::io::Error::other(format!("line {}: {what}", n + 1));
        loop {
            answer.clear();
            match answers.read_line(&mut answer) {
                Ok(0) => return Err(fail("the connection closed")),
                Ok(_) => {
                    let a = answer.trim().to_ascii_lowercase();
                    if a.starts_with("ok") {
                        break;
                    }
                    if a.starts_with("error") || a.starts_with("alarm") || a.starts_with("!!") {
                        let _ = to.write_all(b"M5\n");
                        let _ = to.flush();
                        return Err(fail(&a));
                    }
                    // Status reports, echoes and messages: keep waiting.
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    let _ = to.write_all(b"M5\n");
                    let _ = to.flush();
                    return Err(fail("no answer"));
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        sent += 1;
    }
    Ok(sent)
}

/// Send a G-code program to a port or the network a line at a time
/// (see [`stream_lines`]) on a thread of its own; the status line tells
/// the result on a later frame.
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
pub fn start_gcode(dest: Destination, target: String, gcode: String, ctx: Option<egui::Context>) {
    std::thread::spawn(move || {
        let msg = send_gcode(dest, &target, &gcode);
        if let Ok(mut f) = FINISHED.lock() {
            f.push(msg);
        }
        if let Some(ctx) = ctx {
            ctx.request_repaint();
        }
    });
}

/// Send a G-code program line by line now and say how it went.
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
pub fn send_gcode(dest: Destination, target: &str, gcode: &str) -> String {
    let result = match dest {
        // A port has no read timeout: the send waits for each answer.
        Destination::Port => crate::files::open_device(target).and_then(|f| {
            let mut answers = std::io::BufReader::new(f.try_clone()?);
            let mut f = f;
            stream_lines(&mut f, &mut answers, gcode)
        }),
        Destination::Network => connect(target).and_then(|s| {
            s.set_read_timeout(Some(LINE_TIMEOUT))?;
            let mut answers = std::io::BufReader::new(s.try_clone()?);
            let mut s = s;
            let sent = stream_lines(&mut s, &mut answers, gcode);
            let _ = s.shutdown(std::net::Shutdown::Both);
            sent
        }),
        Destination::File => Ok(0),
    };
    match result {
        Ok(n) => crate::i18n::trf(
            "tracecut.laser_sent",
            &[("n", &n.to_string()), ("to", target)],
        ),
        Err(e) => crate::i18n::trf(
            "tracecut.send_failed",
            &[("to", target), ("e", &e.to_string())],
        ),
    }
}

/// Open a connection to `host[:port]`.
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
fn connect(target: &str) -> std::io::Result<std::net::TcpStream> {
    use std::net::ToSocketAddrs as _;
    let (host, port) = host_port(target)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "no host given"))?;
    let timeout = std::time::Duration::from_secs(5);
    let mut last = None;
    for addr in (host.as_str(), port).to_socket_addrs()? {
        match std::net::TcpStream::connect_timeout(&addr, timeout) {
            Ok(s) => {
                s.set_write_timeout(Some(std::time::Duration::from_secs(60)))?;
                return Ok(s);
            }
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "the host has no address")
    }))
}

/// Where sending is not possible: say so.
#[cfg(any(target_arch = "wasm32", target_os = "android"))]
pub fn start_gcode(_dest: Destination, _target: String, _gcode: String, _ctx: Option<egui::Context>) {
    if let Ok(mut f) = FINISHED.lock() {
        f.push(crate::i18n::tr("tracecut.send_unavailable"));
    }
}

/// Where sending is not possible: say so.
#[cfg(any(target_arch = "wasm32", target_os = "android"))]
pub fn start(_dest: Destination, _target: String, _bytes: Vec<u8>, _ctx: Option<egui::Context>) {
    if let Ok(mut f) = FINISHED.lock() {
        f.push(crate::i18n::tr("tracecut.send_unavailable"));
    }
}

/// Put the message of a finished send on the status line.
pub fn poll(app: &mut crate::app::App) {
    let Ok(mut f) = FINISHED.try_lock() else {
        return;
    };
    let last = f.pop();
    f.clear();
    if let Some(last) = last {
        app.status = last;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A controller on the network that answers each line with `ok`, or
    /// with `error:2` at the line `fail_at` (counting from 1).
    fn controller(fail_at: Option<usize>) -> (String, std::thread::JoinHandle<Vec<String>>) {
        use std::io::{BufRead as _, Write as _};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listen");
        let to = format!("127.0.0.1:{}", listener.local_addr().expect("addr").port());
        let got = std::thread::spawn(move || {
            let (s, _) = listener.accept().expect("accept");
            let mut w = s.try_clone().expect("clone");
            let mut lines = Vec::new();
            for l in std::io::BufReader::new(s).lines() {
                let Ok(l) = l else { break };
                lines.push(l);
                let answer = if Some(lines.len()) == fail_at {
                    "error:2\n"
                } else {
                    "<Idle|MPos:0,0,0>\nok\n"
                };
                if w.write_all(answer.as_bytes()).is_err() {
                    break;
                }
            }
            lines
        });
        (to, got)
    }

    #[test]
    fn gcode_goes_a_line_at_a_time_without_comments() {
        let (to, got) = controller(None);
        let msg = send_gcode(
            Destination::Network,
            &to,
            "; job\nG21\n\nG1 X1 Y2 S10 F600 ; burn\nM5\n",
        );
        assert!(msg.contains('3'), "{msg}");
        assert_eq!(got.join().expect("join"), vec!["G21", "G1 X1 Y2 S10 F600", "M5"]);
    }

    #[test]
    fn an_error_answer_stops_the_send_and_turns_the_beam_off() {
        let (to, got) = controller(Some(2));
        let msg = send_gcode(Destination::Network, &to, "G21\nG1 X1\nG1 X2\n");
        assert!(msg.contains("line 2"), "{msg}");
        let lines = got.join().expect("join");
        assert_eq!(lines, vec!["G21", "G1 X1", "M5"]);
    }

    #[test]
    fn hosts_and_ports() {
        assert_eq!(host_port("cutter"), Some(("cutter".into(), 9100)));
        assert_eq!(
            host_port(" 192.168.1.20:9101 "),
            Some(("192.168.1.20".into(), 9101))
        );
        assert_eq!(host_port("[fe80::1]:9100"), Some(("fe80::1".into(), 9100)));
        assert_eq!(host_port("[fe80::1]"), Some(("fe80::1".into(), 9100)));
        assert_eq!(host_port("fe80::1"), Some(("fe80::1".into(), 9100)));
        assert_eq!(host_port(""), None);
        assert_eq!(host_port("cutter:port"), None);
        assert_eq!(host_port(":9100"), None);
        assert_eq!(host_port("[]:9100"), None);
    }

    #[test]
    fn a_job_reaches_a_listener_on_the_network() {
        use std::io::Read as _;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listen");
        let port = listener.local_addr().expect("addr").port();
        let got = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().expect("accept");
            let mut buf = Vec::new();
            s.read_to_end(&mut buf).expect("read");
            buf
        });
        send_network(&format!("127.0.0.1:{port}"), b"IN;PU0,0;").expect("send");
        assert_eq!(got.join().expect("join"), b"IN;PU0,0;");
    }

    #[test]
    fn a_job_reaches_a_port_path_and_a_missing_one_fails() {
        let dir = std::env::temp_dir().join(format!(
            "tracedraw-tracecut-port-{}",
            crate::recovery::new_id()
        ));
        std::fs::create_dir_all(&dir).expect("dir");
        let dev = dir.join("tty");
        // The device must exist: nothing is created.
        let missing = dev.to_string_lossy().to_string();
        assert!(crate::files::write_device(&missing, b"x").is_err());
        assert!(!dev.exists());
        std::fs::write(&dev, b"").expect("device");
        crate::files::write_device(&missing, b"IN;").expect("write");
        assert_eq!(std::fs::read(&dev).expect("read"), b"IN;");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_send_says_how_it_went() {
        // Nothing listens on port 1.
        let failed = send(Destination::Network, "127.0.0.1:1", b"IN;");
        assert!(failed.contains("127.0.0.1:1"), "{failed}");
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listen");
        let to = format!("127.0.0.1:{}", listener.local_addr().expect("addr").port());
        let got = std::thread::spawn(move || {
            use std::io::Read as _;
            let (mut s, _) = listener.accept().expect("accept");
            let mut buf = Vec::new();
            s.read_to_end(&mut buf).expect("read");
            buf
        });
        let ok = send(Destination::Network, &to, b"IN;");
        assert_eq!(got.join().expect("join"), b"IN;");
        assert_ne!(ok, failed);
        assert!(ok.contains('3'), "{ok}");
    }
}
