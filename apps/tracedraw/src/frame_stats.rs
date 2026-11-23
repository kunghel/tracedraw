//! Frame statistics for the log (#616): what the first frame found (the
//! window's size in pixels, the interface scale, the monitor, how long
//! the start took), and at debug level every 10 s while frames happen
//! how many there were, how many had input, what building them cost, the
//! gaps between them and why they were asked for. An idle window logs
//! nothing, and with the debug level off nothing is measured past the
//! first frame. `RUST_LOG=debug` (or `RUST_LOG=info,tracedraw=debug`)
//! turns it on; see docs/behavior/about-and-startup.md.
//!
//! Slow frames are always logged (d#2729, d#2670): a frame that took
//! [`SLOW`] or more to build names its slowest steps (the menus, the
//! bars, the dockers, the drawing, the dialogs, the work before and after
//! the interface), and a frame asked for at once that came [`SLOW`] or
//! more after the one before tells how long that one took to reach the
//! screen (drawing it on the graphics card and showing it). At most one
//! line every [`REPORT_EVERY`], which counts the slow frames in between.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Duration;
use web_time::Instant;

/// How often a summary is logged while frames happen.
pub const PERIOD: Duration = Duration::from_secs(10);

/// When the process started (as the log began).
static START: OnceLock<Instant> = OnceLock::new();

pub fn mark_start() {
    let _ = START.set(Instant::now());
}

/// The frames of one period.
#[derive(Default)]
pub struct Stats {
    since: Option<Instant>,
    frames: u32,
    with_input: u32,
    build: Duration,
    build_max: Duration,
    gap: Duration,
    gap_max: Duration,
    last: Option<Instant>,
    causes: HashMap<String, u32>,
}

impl Stats {
    /// A frame that began at `began` and was built by `end`, with or
    /// without input, asked for by `causes`. The summary when the period
    /// is over (and a new one begins).
    pub fn frame(
        &mut self,
        began: Instant,
        end: Instant,
        input: bool,
        causes: impl IntoIterator<Item = String>,
    ) -> Option<String> {
        let since = *self.since.get_or_insert(began);
        if let Some(last) = self.last {
            let gap = began.saturating_duration_since(last);
            self.gap += gap;
            self.gap_max = self.gap_max.max(gap);
        }
        self.last = Some(began);
        let build = end.saturating_duration_since(began);
        self.frames += 1;
        self.with_input += u32::from(input);
        self.build += build;
        self.build_max = self.build_max.max(build);
        for c in causes {
            *self.causes.entry(c).or_default() += 1;
        }
        if end.saturating_duration_since(since) < PERIOD {
            return None;
        }
        let text = self.summary(end.saturating_duration_since(since));
        *self = Stats {
            last: self.last,
            ..Default::default()
        };
        Some(text)
    }

    fn summary(&self, span: Duration) -> String {
        let n = self.frames.max(1);
        let gaps = self.frames.saturating_sub(1).max(1);
        let mut causes: Vec<(&String, &u32)> = self.causes.iter().collect();
        causes.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        let causes: Vec<String> = causes
            .iter()
            .take(5)
            .map(|(c, k)| format!("{c} x{k}"))
            .collect();
        format!(
            "frames: {} in {:.1} s ({} with input), built in {:.2} ms mean, {:.2} ms max; {:.1} ms apart mean, {:.1} ms max; asked for by: {}",
            self.frames,
            span.as_secs_f64(),
            self.with_input,
            self.build.as_secs_f64() * 1000.0 / n as f64,
            self.build_max.as_secs_f64() * 1000.0,
            self.gap.as_secs_f64() * 1000.0 / gaps as f64,
            self.gap_max.as_secs_f64() * 1000.0,
            if causes.is_empty() {
                "input".to_string()
            } else {
                causes.join(", ")
            }
        )
    }
}

/// A frame this long or longer to build, or to reach the screen, is
/// logged.
pub const SLOW: Duration = Duration::from_millis(250);

/// The slow-frame lines come at most this often.
pub const REPORT_EVERY: Duration = Duration::from_secs(5);

/// The slow frames since the last line about them.
#[derive(Default)]
pub struct Slow {
    reported: Option<Instant>,
    /// Slow frames not yet in a line.
    unreported: u32,
}

impl Slow {
    /// A frame built in `built` through `steps` (each step's name and
    /// time, in order), which came `waited` after the frame before when
    /// that one asked for it at once (the time the frame before took to
    /// be drawn and shown), seen at `now`. The line for the log when it
    /// was slow and none was written in the last [`REPORT_EVERY`].
    pub fn frame(
        &mut self,
        now: Instant,
        built: Duration,
        waited: Option<Duration>,
        steps: &[(&'static str, Duration)],
        renderer: &str,
    ) -> Option<String> {
        let screen = waited.filter(|w| *w >= SLOW);
        if built < SLOW && screen.is_none() {
            return None;
        }
        self.unreported += 1;
        if self
            .reported
            .is_some_and(|r| now.saturating_duration_since(r) < REPORT_EVERY)
        {
            return None;
        }
        let mut parts = Vec::new();
        if built >= SLOW {
            let mut slowest: Vec<&(&str, Duration)> = steps
                .iter()
                .filter(|(_, d)| *d >= Duration::from_millis(10))
                .collect();
            slowest.sort_by(|a, b| b.1.cmp(&a.1));
            let named: Vec<String> = slowest
                .iter()
                .take(4)
                .map(|(n, d)| format!("{n} {:.0} ms", ms(*d)))
                .collect();
            parts.push(if named.is_empty() {
                format!("built in {:.0} ms", ms(built))
            } else {
                format!("built in {:.0} ms ({})", ms(built), named.join(", "))
            });
        }
        if let Some(w) = screen {
            parts.push(format!(
                "the frame before took {:.0} ms to reach the screen ({renderer})",
                ms(w)
            ));
        }
        let more = self.unreported - 1;
        let tail = if more > 0 {
            format!("; {more} more slow frame(s) since the last report")
        } else {
            String::new()
        };
        self.reported = Some(now);
        self.unreported = 0;
        Some(format!("slow frame: {}{tail}", parts.join("; ")))
    }
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

thread_local! {
    static STATS: RefCell<Stats> = RefCell::new(Stats::default());
    static SLOW_FRAMES: RefCell<Slow> = RefCell::new(Slow::default());
    /// The steps of the frame being built: where each one ended.
    static MARKS: RefCell<Vec<(&'static str, Instant)>> = const { RefCell::new(Vec::new()) };
    /// When the last frame was built, and whether it asked for the next
    /// at once.
    static LAST_END: std::cell::Cell<Option<Instant>> = const { std::cell::Cell::new(None) };
    /// How long after the frame before this frame came, when that one
    /// asked for it at once.
    static WAITED: std::cell::Cell<Option<Duration>> = const { std::cell::Cell::new(None) };
    /// Whether frames are measured (the debug level is on for this
    /// module), found at the first frame; none yet before it.
    static ON: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

/// At the start of building a frame: when it began.
pub fn begin(ctx: &egui::Context) -> Instant {
    ON.with(|on| {
        if on.get().is_none() {
            first_frame(ctx);
            on.set(Some(log::log_enabled!(log::Level::Debug)));
        }
    });
    let now = Instant::now();
    let asked = ctx.requested_repaint_last_pass_for(&ctx.viewport_id());
    let waited = LAST_END
        .with(|l| l.get())
        .filter(|_| asked)
        .map(|end| now.saturating_duration_since(end));
    WAITED.with(|w| w.set(waited));
    MARKS.with(|m| m.borrow_mut().clear());
    now
}

/// The end of a step of the frame being built, named for the slow-frame
/// log: the time since the step before (or the frame's start) is its.
pub fn mark(name: &'static str) {
    crate::hang_watch::mark(name);
    MARKS.with(|m| m.borrow_mut().push((name, Instant::now())));
}

/// The steps of a frame that began at `began` and ended at `end`: each
/// mark's time since the one before, and what came after the last one.
fn steps(
    began: Instant,
    end: Instant,
    marks: &[(&'static str, Instant)],
) -> Vec<(&'static str, Duration)> {
    let mut out = Vec::with_capacity(marks.len() + 1);
    let mut from = began;
    for (name, at) in marks {
        out.push((*name, at.saturating_duration_since(from)));
        from = (*at).max(from);
    }
    out.push(("after the interface", end.saturating_duration_since(from)));
    out
}

/// Once the frame is built.
pub fn end(ctx: &egui::Context, began: Instant) {
    let now = Instant::now();
    LAST_END.with(|l| l.set(Some(now)));
    let waited = WAITED.with(|w| w.get());
    let built = now.saturating_duration_since(began);
    if built >= SLOW || waited.is_some_and(|w| w >= SLOW) {
        let steps = MARKS.with(|m| steps(began, now, &m.borrow()));
        let renderer = crate::gpu::mode_in_use()
            .map(|m| m.name())
            .unwrap_or("renderer unknown");
        let line = SLOW_FRAMES.with(|s| s.borrow_mut().frame(now, built, waited, &steps, renderer));
        if let Some(line) = line {
            log::info!("{line}");
        }
    }
    if ON.with(|on| on.get()) != Some(true) {
        return;
    }
    let input = ctx.input(|i| !i.events.is_empty());
    let causes = ctx
        .repaint_causes()
        .into_iter()
        .map(|c| short_cause(&c.to_string()));
    let summary = STATS.with(|s| s.borrow_mut().frame(began, Instant::now(), input, causes));
    if let Some(text) = summary {
        log::debug!("{text}");
    }
}

/// A repaint cause as the log shows it: the crate and file of a
/// dependency without the registry's folders, no trailing blanks.
fn short_cause(cause: &str) -> String {
    let cause = cause.trim_end();
    match cause.find("index.crates.io-") {
        Some(i) => match cause[i..].find('/') {
            Some(j) => cause[i + j + 1..].to_string(),
            None => cause.to_string(),
        },
        None => cause.to_string(),
    }
}

fn first_frame(ctx: &egui::Context) {
    let ppp = ctx.pixels_per_point();
    let size = ctx.content_rect().size() * ppp;
    let monitor = ctx.input(|i| i.viewport().monitor_size);
    let after = START.get().map(|t| t.elapsed()).unwrap_or_default();
    log::info!(
        "first frame {:.2} s after the start: window {:.0} x {:.0} px, {ppp} px per point, monitor {}",
        after.as_secs_f64(),
        size.x,
        size.y,
        monitor
            .map(|m| format!("{:.0} x {:.0} points", m.x, m.y))
            .unwrap_or_else(|| "unknown".into())
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A summary every 10 s of frames: their count, how many had input,
    /// the build times and gaps, the causes most frequent first; then the
    /// next period starts afresh (#616).
    #[test]
    fn frames_are_summed_up_every_ten_seconds() {
        let t0 = Instant::now();
        let ms = Duration::from_millis;
        let mut s = Stats::default();
        let mut out = None;
        // A frame every 100 ms, built in 4 ms (8 ms every tenth), every
        // other one with input; the others asked for by an animation.
        for i in 0..=100u32 {
            let began = t0 + ms(100) * i;
            let build = if i % 10 == 0 { ms(8) } else { ms(4) };
            let causes = (i % 2 == 1).then(|| "app.rs:1 animation".to_string());
            if let Some(t) = s.frame(began, began + build, i % 2 == 0, causes) {
                assert!(out.is_none(), "one summary in 10 s");
                out = Some((i, t));
            }
        }
        let (at, text) = out.expect("a summary after 10 s");
        assert_eq!(at, 100);
        assert!(
            text.starts_with("frames: 101 in 10.0 s (51 with input)"),
            "{text}"
        );
        assert!(text.contains("8.00 ms max"), "{text}");
        assert!(text.contains("100.0 ms apart mean, 100.0 ms max"), "{text}");
        assert!(text.contains("app.rs:1 animation x50"), "{text}");
        // The next period counts from nothing, its first gap included.
        let next = t0 + ms(10_200);
        assert_eq!(s.frame(next, next + ms(1), false, None::<String>), None);
        assert_eq!(s.frames, 1);
        assert_eq!(s.gap_max, ms(200));
        assert_eq!(
            short_cause(
                "/home/u/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/egui-0.36.2/src/context.rs:538  "
            ),
            "egui-0.36.2/src/context.rs:538"
        );
        assert_eq!(
            short_cause("apps/tracedraw/src/app.rs:3061 "),
            "apps/tracedraw/src/app.rs:3061"
        );
    }

    /// A slow frame is logged with its slowest steps, or with how long the
    /// frame before took to reach the screen; quick frames are not, and
    /// the slow ones within 5 s of a line are counted in the next one
    /// (d#2729, d#2670).
    #[test]
    fn slow_frames_name_their_slowest_steps() {
        let t0 = Instant::now();
        let ms = Duration::from_millis;
        let mut s = Slow::default();
        let quick = [("the drawing", ms(8))];
        assert_eq!(s.frame(t0, ms(16), Some(ms(16)), &quick, "hardware"), None);
        let parts = [
            ("before the interface", ms(2)),
            ("menus", ms(1)),
            ("the drawing", ms(1900)),
            ("dialogs", ms(40)),
            ("after the interface", ms(3)),
        ];
        let line = s
            .frame(t0, ms(1946), None, &parts, "hardware")
            .expect("logged");
        assert_eq!(
            line,
            "slow frame: built in 1946 ms (the drawing 1900 ms, dialogs 40 ms)"
        );
        // Within 5 s: counted, not logged.
        assert_eq!(
            s.frame(t0 + ms(1000), ms(300), None, &parts, "hardware"),
            None
        );
        assert_eq!(
            s.frame(t0 + ms(2000), ms(5), Some(ms(2000)), &quick, "hardware"),
            None
        );
        let line = s
            .frame(t0 + ms(6000), ms(5), Some(ms(1800)), &quick, "gl")
            .expect("logged");
        assert_eq!(
            line,
            "slow frame: the frame before took 1800 ms to reach the screen (gl); 2 more slow frame(s) since the last report"
        );
        // The steps between the marks, the rest after the last one.
        let marks = [("menus", t0 + ms(5)), ("the drawing", t0 + ms(25))];
        assert_eq!(
            steps(t0, t0 + ms(30), &marks),
            vec![
                ("menus", ms(5)),
                ("the drawing", ms(20)),
                ("after the interface", ms(5))
            ]
        );
    }
}
