//! How far a trace running off the interface thread has got, for the
//! window Image > Trace > Automatic shows while it works (see
//! `ui/trace_progress.rs`).
//!
//! The tracer is handed only its cancel flag ([`Cancel`]), so the
//! progress is found by that flag: [`watch`] ties a [`Progress`] to the
//! flag of a trace, and the steps of the trace [`report`] how far they
//! are with the flag they hold. A trace nobody watches (the Trace
//! dialog's preview, the tests) reports to nothing, at the cost of one
//! look at an empty list. The samples the best detail is looked for on
//! are traced with the same flag: while that search runs ([`hush`]) only
//! its own step is taken, so the samples' steps do not move the bar.

use crate::trace::Cancel;
use std::sync::atomic::{AtomicU16, AtomicU32, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, Weak};

/// The steps of a trace, in the order they run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Step {
    /// A small picture enlarged first (`upscale.rs`).
    Enlarge = 0,
    /// Best optimization: the detail that draws the picture best.
    Detail = 1,
    /// The picture's colours and their edges.
    Colors = 2,
    /// The curves around each colour.
    Curves = 3,
    /// The curves drawn and compared with the picture.
    Check = 4,
    /// Small print redrawn with the letters of its font.
    Letters = 5,
}

impl Step {
    pub const ALL: [Step; 6] = [
        Step::Enlarge,
        Step::Detail,
        Step::Colors,
        Step::Curves,
        Step::Check,
        Step::Letters,
    ];

    fn from_u8(v: u8) -> Step {
        Step::ALL
            .get(usize::from(v))
            .copied()
            .unwrap_or(Step::Letters)
    }

    /// The share of a trace's time the step takes, about: what the bar
    /// moves by while it runs (measured on logos of 300 to 1000 pixels).
    fn weight(self) -> f32 {
        match self {
            Step::Enlarge => 30.0,
            Step::Detail => 20.0,
            Step::Colors => 10.0,
            Step::Curves => 20.0,
            Step::Check => 10.0,
            Step::Letters => 25.0,
        }
    }

    /// The key of the step's line in the window.
    pub fn label_key(self) -> &'static str {
        match self {
            Step::Enlarge => "trace.progress_enlarge",
            Step::Detail => "trace.progress_detail",
            Step::Colors => "trace.progress_colors",
            Step::Curves => "trace.progress_curves",
            Step::Check => "trace.progress_check",
            Step::Letters => "trace.progress_letters",
        }
    }
}

/// Where a watched trace is: its step and how much of it is done.
#[derive(Debug, Default)]
pub struct Progress {
    step: AtomicU8,
    /// Thousandths of the step.
    part: AtomicU16,
    /// Searches for the best detail running ([`hush`]).
    hushed: AtomicU32,
    /// The first step this trace takes (a trace not enlarged starts at
    /// [`Step::Detail`] or later): the bar runs over its own steps only.
    first: AtomicU8,
    /// Whether the trace looks for the best detail.
    searches: std::sync::atomic::AtomicBool,
}

impl Progress {
    /// A trace about to start at `first`, looking for the best detail or
    /// not.
    pub fn new(first: Step, searches: bool) -> Self {
        let p = Progress::default();
        p.restart(first, searches);
        p
    }

    /// Back to the start, for the next picture.
    pub fn restart(&self, first: Step, searches: bool) {
        self.first.store(first as u8, Ordering::Relaxed);
        self.searches.store(searches, Ordering::Relaxed);
        self.step.store(first as u8, Ordering::Relaxed);
        self.part.store(0, Ordering::Relaxed);
        self.hushed.store(0, Ordering::Relaxed);
    }

    /// The step running.
    pub fn step(&self) -> Step {
        Step::from_u8(self.step.load(Ordering::Relaxed))
    }

    /// How much of the whole trace is done, 0 to 1: the steps before the
    /// one running and its own part, by their weights; steps the trace
    /// skips count for nothing.
    pub fn done(&self) -> f32 {
        let first = Step::from_u8(self.first.load(Ordering::Relaxed));
        let searches = self.searches.load(Ordering::Relaxed);
        let step = self.step();
        let part = f32::from(self.part.load(Ordering::Relaxed)) / 1000.0;
        let taken = |s: &Step| *s >= first && (*s != Step::Detail || searches);
        let total: f32 = Step::ALL
            .iter()
            .filter(|s| taken(s))
            .map(|s| s.weight())
            .sum();
        let before: f32 = Step::ALL
            .iter()
            .filter(|s| taken(s) && **s < step)
            .map(|s| s.weight())
            .sum();
        let now = if taken(&step) {
            step.weight() * part
        } else {
            0.0
        };
        ((before + now) / total.max(1.0)).clamp(0.0, 1.0)
    }

    fn set(&self, step: Step, part: f64) {
        let hushed = self.hushed.load(Ordering::Relaxed) > 0;
        if hushed && step != Step::Detail {
            return;
        }
        let was = self.step();
        // Never back to an earlier step (the check makes curves again).
        if step < was {
            return;
        }
        let part = (part.clamp(0.0, 1.0) * 1000.0).round() as u16;
        if step == was && part < self.part.load(Ordering::Relaxed) {
            return;
        }
        self.step.store(step as u8, Ordering::Relaxed);
        self.part.store(part, Ordering::Relaxed);
    }
}

/// The watched traces, by the address of their cancel flag.
static WATCHED: Mutex<Vec<(usize, Weak<Progress>)>> = Mutex::new(Vec::new());

fn key(cancel: &Cancel) -> usize {
    cancel as *const Cancel as usize
}

fn find(cancel: Option<&Cancel>) -> Option<Arc<Progress>> {
    let cancel = cancel?;
    let k = key(cancel);
    let list = WATCHED.lock().ok()?;
    list.iter()
        .find(|(at, _)| *at == k)
        .and_then(|(_, p)| p.upgrade())
}

/// Tie `progress` to the trace that runs with `cancel`, until the
/// returned guard is dropped.
pub fn watch(cancel: &Arc<Cancel>, progress: &Arc<Progress>) -> Watch {
    let k = key(cancel);
    if let Ok(mut list) = WATCHED.lock() {
        list.retain(|(at, p)| *at != k && p.strong_count() > 0);
        list.push((k, Arc::downgrade(progress)));
    }
    Watch {
        key: k,
        _cancel: cancel.clone(),
    }
}

/// While alive, the trace's progress is watched (see [`watch`]). It
/// holds the flag, so its address is not taken by another before the
/// watch ends.
pub struct Watch {
    key: usize,
    _cancel: Arc<Cancel>,
}

impl Drop for Watch {
    fn drop(&mut self) {
        if let Ok(mut list) = WATCHED.lock() {
            list.retain(|(at, _)| *at != self.key);
        }
    }
}

/// The trace running with `cancel` is at `step`, `part` (0 to 1) of it
/// done. Nothing when nobody watches it.
pub fn report(cancel: Option<&Cancel>, step: Step, part: f64) {
    if let Some(p) = find(cancel) {
        p.set(step, part);
    }
}

/// The search for the best detail runs until the returned guard is
/// dropped: the steps of the samples it traces are not reported.
pub fn hush(cancel: Option<&Cancel>) -> Hush {
    let p = find(cancel);
    if let Some(p) = &p {
        p.set(Step::Detail, 0.0);
        p.hushed.fetch_add(1, Ordering::Relaxed);
    }
    Hush(p)
}

pub struct Hush(Option<Arc<Progress>>);

impl Drop for Hush {
    fn drop(&mut self) {
        if let Some(p) = &self.0 {
            p.hushed.fetch_sub(1, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_watched_trace_moves_forward_through_its_steps() {
        let cancel = Arc::new(Cancel::new(false));
        let p = Arc::new(Progress::new(Step::Detail, true));
        let w = watch(&cancel, &p);
        assert_eq!(p.done(), 0.0);
        {
            let _h = hush(Some(&cancel));
            report(Some(&cancel), Step::Detail, 0.5);
            // The samples' own steps are not the trace's.
            report(Some(&cancel), Step::Curves, 0.9);
            assert_eq!(p.step(), Step::Detail);
        }
        let half_detail = p.done();
        assert!(half_detail > 0.0);
        report(Some(&cancel), Step::Curves, 0.5);
        assert_eq!(p.step(), Step::Curves);
        assert!(p.done() > half_detail);
        // Never back.
        report(Some(&cancel), Step::Colors, 1.0);
        assert_eq!(p.step(), Step::Curves);
        report(Some(&cancel), Step::Letters, 1.0);
        assert!((p.done() - 1.0).abs() < 1e-6);
        drop(w);
        // Unwatched: nothing moves.
        p.restart(Step::Detail, true);
        report(Some(&cancel), Step::Curves, 0.5);
        assert_eq!(p.step(), Step::Detail);
    }

    #[test]
    fn a_trace_not_enlarged_counts_its_own_steps_only() {
        let p = Progress::new(Step::Colors, false);
        p.set(Step::Letters, 1.0);
        assert!((p.done() - 1.0).abs() < 1e-6);
        let p = Progress::new(Step::Colors, false);
        p.set(Step::Check, 0.0);
        let shown = p.done();
        // Colours and curves done of colours, curves, check and letters.
        assert!((shown - 30.0 / 65.0).abs() < 1e-3, "{shown}");
    }
}
