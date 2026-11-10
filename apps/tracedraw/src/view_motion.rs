//! The view's own motion between input events: the wheel zoom's glide,
//! whose length follows how fast the wheel turns (#1648), and the pan's
//! slide after a flick (#831). A few float operations a frame, and a
//! repaint asked only while the view moves.

use crate::app::App;
use egui::{Pos2, Vec2};

/// Time a glide takes to reach 90 % of its zoom when the wheel turns
/// slowly (a notch now and then), seconds.
pub const GLIDE_SLOW: f32 = 0.10;
/// The shortest glide, for the fastest wheel turns, seconds.
pub const GLIDE_FAST: f32 = 0.03;
/// Wheel speed, notches a second, up to which the glide is the slow one;
/// faster turns shorten it in proportion, down to [`GLIDE_FAST`].
pub const SLOW_NOTCHES_PER_SECOND: f32 = 8.0;
/// The longest gap between notches still read as one turn of the wheel,
/// seconds: a notch after a longer pause counts as slow.
const TURN_GAP: f32 = 0.25;

/// How long a glide takes to reach 90 % of its zoom for a wheel turning
/// `notches_per_second`.
pub fn glide_time(notches_per_second: f32) -> f32 {
    let s = notches_per_second.abs();
    if !s.is_finite() || s <= SLOW_NOTCHES_PER_SECOND {
        GLIDE_SLOW
    } else {
        (GLIDE_SLOW * SLOW_NOTCHES_PER_SECOND / s).max(GLIDE_FAST)
    }
}

/// The wheel zoom still to come and how fast it goes.
#[derive(Debug, Clone, Default)]
pub struct ZoomGlide {
    /// Natural log of the zoom factor still to apply.
    left: f32,
    /// The screen point the zoom goes about.
    anchor: Pos2,
    /// Time to reach 90 % of what is left, seconds.
    glide: f32,
    /// Input time of the last notch, seconds.
    last_notch: Option<f64>,
}

impl ZoomGlide {
    /// Wheel `notches` at input time `now`, each zooming `step` times
    /// (above 1), about `anchor`. The glide gets as long as the wheel's
    /// speed asks: the notches of this event over the time since the
    /// last one.
    pub fn add(&mut self, notches: f32, step: f32, anchor: Pos2, now: f64) {
        if notches == 0.0 || !notches.is_finite() || !(step > 0.0) || !step.is_finite() {
            return;
        }
        let gap = self
            .last_notch
            .map(|t| (now - t) as f32)
            .filter(|g| g.is_finite() && *g >= 0.0)
            .unwrap_or(TURN_GAP)
            .clamp(1.0 / 240.0, TURN_GAP);
        self.glide = glide_time(notches.abs() / gap);
        self.last_notch = Some(now);
        self.left += notches * step.ln();
        self.anchor = anchor;
    }

    /// The zoom factor for a frame `dt` seconds long (1 when nothing is
    /// left); the last bit goes at once, so a glide always ends.
    pub fn advance(&mut self, dt: f32) -> f32 {
        if self.left == 0.0 {
            return 1.0;
        }
        let glide = if self.glide > 0.0 {
            self.glide
        } else {
            GLIDE_SLOW
        };
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.1)
        } else {
            0.0
        };
        let share = 1.0 - 0.1f32.powf(dt / glide);
        let mut d = self.left * share;
        if (self.left - d).abs() < 1e-4 {
            d = self.left;
        }
        self.left -= d;
        d.exp()
    }

    /// The point the zoom goes about.
    pub fn anchor(&self) -> Pos2 {
        self.anchor
    }

    /// Some zoom is still to come.
    pub fn active(&self) -> bool {
        self.left != 0.0
    }

    /// The time the glide takes to reach 90 %, seconds.
    pub fn glide(&self) -> f32 {
        self.glide
    }

    /// Drop the zoom still to come.
    pub fn stop(&mut self) {
        self.left = 0.0;
    }
}

/// Time constant of a flick's slide, seconds: the speed falls to 37 %
/// in this time (to 5 % in three times it).
pub const SLIDE_TIME: f32 = 0.30;
/// The slowest release that slides on, points a second.
pub const FLICK_SPEED: f32 = 150.0;
/// A slide stops under this speed, points a second.
const STOP_SPEED: f32 = 15.0;
/// A pointer held still this long before release slides nothing,
/// seconds.
const HELD_STILL: f64 = 0.06;
/// The fastest slide, points a second (a jittery last frame does not
/// throw the view away).
const MAX_SPEED: f32 = 8000.0;

/// Pan inertia (#831): a pan dragged and let go while moving keeps the
/// view sliding and easing out.
#[derive(Debug, Clone, Default)]
pub struct PanInertia {
    /// The drag's speed, points a second, smoothed over its last frames;
    /// while sliding, the slide's.
    velocity: Vec2,
    /// Input time of the last pan of the drag.
    last_move: f64,
    /// A pan drag is going on.
    dragging: bool,
    /// The view slides on after a flick.
    sliding: bool,
}

impl PanInertia {
    /// A pan drag moved the view by `delta` in a frame `dt` seconds long
    /// at input time `now`.
    pub fn track(&mut self, delta: Vec2, dt: f32, now: f64) {
        self.sliding = false;
        if !(dt > 0.0) || !dt.is_finite() || !delta.x.is_finite() || !delta.y.is_finite() {
            return;
        }
        let v = delta / dt.max(1.0 / 240.0);
        self.velocity = if self.dragging {
            self.velocity * 0.4 + v * 0.6
        } else {
            v
        };
        if delta != Vec2::ZERO {
            self.last_move = now;
        }
        self.dragging = true;
    }

    /// One frame `dt` seconds long at input time `now`: the pan the view
    /// slides by. `down` tells whether a pointer button is held (the drag
    /// ends when none is), `on` whether pan inertia is on.
    pub fn tick(&mut self, dt: f32, now: f64, down: bool, on: bool) -> Vec2 {
        if self.dragging && !down {
            self.dragging = false;
            let speed = self.velocity.length();
            self.sliding = on
                && now - self.last_move <= HELD_STILL
                && speed.is_finite()
                && speed >= FLICK_SPEED;
            if self.sliding && speed > MAX_SPEED {
                self.velocity *= MAX_SPEED / speed;
            }
        }
        if !self.sliding {
            return Vec2::ZERO;
        }
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.1)
        } else {
            0.0
        };
        let step = self.velocity * dt;
        self.velocity *= (-dt / SLIDE_TIME).exp();
        if self.velocity.length() < STOP_SPEED {
            self.stop();
        }
        step
    }

    /// The view slides on.
    pub fn sliding(&self) -> bool {
        self.sliding
    }

    /// End a slide (a press, a wheel turn).
    pub fn stop(&mut self) {
        self.sliding = false;
        self.velocity = Vec2::ZERO;
    }
}

impl App {
    /// A pan drag (the Pan tool, the middle button, two fingers) moves
    /// the view by `delta`; pan inertia follows its speed.
    pub fn pan_drag(&mut self, delta: Vec2, ctx: &egui::Context) {
        self.view.pan(delta);
        let (dt, now) = ctx.input(|i| (i.stable_dt, i.time));
        self.pan_inertia.track(delta, dt, now);
    }

    /// Once a frame: the slide after a flick goes on (Options > Tools >
    /// Zoom/Pan > Pan inertia); a press or a wheel turn ends it.
    pub fn pan_inertia_tick(&mut self, ctx: &egui::Context) {
        let (dt, now, down, pressed, wheel) = ctx.input(|i| {
            (
                i.stable_dt,
                i.time,
                i.pointer.any_down(),
                i.pointer.any_pressed(),
                i.events
                    .iter()
                    .any(|e| matches!(e, egui::Event::MouseWheel { .. })),
            )
        });
        if (pressed || wheel) && self.pan_inertia.sliding() {
            self.pan_inertia.stop();
        }
        let d = self
            .pan_inertia
            .tick(dt, now, down, self.settings.pan_inertia);
        if d != Vec2::ZERO {
            self.view.pan(d);
        }
        if self.pan_inertia.sliding() {
            ctx.request_repaint();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A drag at `speed` points a second for a few frames, let go at once.
    fn flick(p: &mut PanInertia, speed: f32, t: &mut f64) {
        let dt = 1.0 / 60.0;
        for _ in 0..5 {
            *t += dt as f64;
            p.track(Vec2::new(speed * dt, 0.0), dt, *t);
            assert_eq!(
                p.tick(dt, *t, true, true),
                Vec2::ZERO,
                "no slide while held"
            );
        }
    }

    #[test]
    fn a_flick_slides_on_and_eases_out() {
        let mut p = PanInertia::default();
        let mut t = 1.0;
        flick(&mut p, 1200.0, &mut t);
        let dt = 1.0 / 60.0;
        t += dt as f64;
        let first = p.tick(dt, t, false, true);
        assert!(p.sliding());
        assert!((first.x - 1200.0 * dt).abs() < 1.0, "{first:?}");
        let mut total = first.x;
        let mut last = first.x;
        let mut frames = 0;
        while p.sliding() {
            t += dt as f64;
            let d = p.tick(dt, t, false, true);
            assert!(d.x <= last && d.x >= 0.0, "easing out: {d:?} after {last}");
            last = d.x;
            total += d.x;
            frames += 1;
            assert!(frames < 300, "the slide ends");
        }
        // About the speed times the slide's time constant.
        assert!((total - 1200.0 * SLIDE_TIME).abs() < 40.0, "{total}");
        assert_eq!(p.tick(dt, t, false, true), Vec2::ZERO);
    }

    #[test]
    fn no_slide_when_off_held_still_slow_or_stopped() {
        let dt = 1.0 / 60.0;
        // Off: nothing.
        let mut p = PanInertia::default();
        let mut t = 1.0;
        flick(&mut p, 1200.0, &mut t);
        assert_eq!(p.tick(dt, t + dt as f64, false, false), Vec2::ZERO);
        assert!(!p.sliding());
        // Held still before letting go: nothing.
        let mut p = PanInertia::default();
        flick(&mut p, 1200.0, &mut t);
        t += 0.2;
        p.track(Vec2::ZERO, dt, t);
        assert_eq!(p.tick(dt, t + dt as f64, false, true), Vec2::ZERO);
        // A slow drag: nothing.
        let mut p = PanInertia::default();
        flick(&mut p, FLICK_SPEED / 2.0, &mut t);
        assert_eq!(p.tick(dt, t + dt as f64, false, true), Vec2::ZERO);
        // A slide stopped (a press): nothing more; a new drag ends it too.
        let mut p = PanInertia::default();
        flick(&mut p, 1200.0, &mut t);
        assert_ne!(p.tick(dt, t + dt as f64, false, true), Vec2::ZERO);
        p.stop();
        assert_eq!(p.tick(dt, t + 2.0 * dt as f64, false, true), Vec2::ZERO);
        flick(&mut p, 1200.0, &mut t);
        p.tick(dt, t + dt as f64, false, true);
        assert!(p.sliding());
        p.track(Vec2::new(1.0, 0.0), dt, t + 2.0 * dt as f64);
        assert!(!p.sliding());
        // A wild last frame is capped.
        let mut p = PanInertia::default();
        flick(&mut p, 1.0e7, &mut t);
        let d = p.tick(dt, t + dt as f64, false, true);
        assert!(d.length() <= 8000.0 * dt + 1e-3, "{d:?}");
    }

    #[test]
    fn faster_wheel_turns_glide_shorter() {
        assert_eq!(glide_time(0.0), GLIDE_SLOW);
        assert_eq!(glide_time(SLOW_NOTCHES_PER_SECOND), GLIDE_SLOW);
        assert!(glide_time(16.0) < GLIDE_SLOW);
        assert!(glide_time(32.0) < glide_time(16.0));
        assert_eq!(glide_time(1000.0), GLIDE_FAST);
        assert_eq!(glide_time(f32::NAN), GLIDE_SLOW);
        // A notch after a pause glides slowly; one 30 ms after the last
        // glides shorter and applies more of itself in the first frame.
        let mut g = ZoomGlide::default();
        g.add(1.0, 1.15, Pos2::ZERO, 10.0);
        assert_eq!(g.glide(), GLIDE_SLOW);
        let slow_first = g.advance(1.0 / 60.0);
        g.stop();
        g.add(1.0, 1.15, Pos2::ZERO, 10.03);
        assert!(g.glide() < GLIDE_SLOW / 2.0, "{}", g.glide());
        let fast_first = g.advance(1.0 / 60.0);
        assert!(fast_first > slow_first, "{fast_first} {slow_first}");
        // Several notches in one event after a pause: a fast turn too.
        let mut g = ZoomGlide::default();
        g.add(3.0, 1.15, Pos2::ZERO, 50.0);
        assert!(g.glide() < GLIDE_SLOW);
    }

    #[test]
    fn a_glide_adds_up_to_the_steps_and_ends() {
        let mut g = ZoomGlide::default();
        assert!(!g.active());
        assert_eq!(g.advance(0.016), 1.0);
        g.add(2.0, 1.25, Pos2::new(3.0, 4.0), 1.0);
        g.add(-1.0, 1.25, Pos2::new(5.0, 6.0), 1.2);
        assert_eq!(g.anchor(), Pos2::new(5.0, 6.0));
        let mut total = 1.0f32;
        let mut frames = 0;
        while g.active() {
            total *= g.advance(1.0 / 60.0);
            frames += 1;
            assert!(frames < 200, "the glide ends");
        }
        assert!((total - 1.25).abs() < 1e-3, "{total}");
        // Nothing for a frame of no time; nonsense is ignored.
        g.add(1.0, 1.15, Pos2::ZERO, 2.0);
        assert_eq!(g.advance(0.0), 1.0);
        g.stop();
        assert!(!g.active());
        g.add(f32::NAN, 1.15, Pos2::ZERO, 3.0);
        g.add(1.0, -2.0, Pos2::ZERO, 3.0);
        assert!(!g.active());
    }
}
