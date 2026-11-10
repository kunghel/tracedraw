//! Taps of several fingers on the drawing: two fingers tapped together
//! undo, three redo. Only a tap counts: the fingers lifted soon
//! ([`MAX_TAP_SECONDS`]) and each moved little ([`MAX_TAP_TRAVEL`]), so a
//! pinch zoom or a two-finger pan, which move, never undoes. See
//! docs/behavior/android.md.

use std::collections::HashMap;

/// The longest a tap lasts, from the first finger down to the last one
/// up, in seconds.
pub const MAX_TAP_SECONDS: f64 = 0.35;

/// How far, in interface points, a finger may move in a tap.
pub const MAX_TAP_TRAVEL: f32 = 12.0;

/// What a tap of several fingers does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TapAction {
    Undo,
    Redo,
}

impl TapAction {
    /// The action of a tap of `fingers` fingers: two undo, three redo.
    pub fn of_fingers(fingers: usize) -> Option<TapAction> {
        match fingers {
            2 => Some(TapAction::Undo),
            3 => Some(TapAction::Redo),
            _ => None,
        }
    }
}

/// The fingers of one gesture, from the first one down until the last
/// one is lifted.
#[derive(Debug, Clone, Default)]
pub struct TapGesture {
    /// Each finger down: where it went down.
    down: HashMap<u64, egui::Pos2>,
    /// When the first finger went down, seconds.
    start: f64,
    /// The most fingers down at once.
    most: usize,
    /// A finger moved too far, the gesture lasted too long, began off the
    /// drawing or was cancelled: no tap.
    spoilt: bool,
}

impl TapGesture {
    /// One touch event at `time` (seconds): a finger `id` down, moved to,
    /// or lifted at `pos`, or cancelled; `on_drawing` tells whether a
    /// finger going down is on the drawing. The tap's action when the
    /// last finger of a tap is lifted.
    pub fn touch(
        &mut self,
        id: u64,
        phase: egui::TouchPhase,
        pos: egui::Pos2,
        time: f64,
        on_drawing: bool,
    ) -> Option<TapAction> {
        use egui::TouchPhase;
        match phase {
            TouchPhase::Start => {
                if self.down.is_empty() {
                    *self = TapGesture {
                        start: time,
                        ..Default::default()
                    };
                }
                if !on_drawing {
                    self.spoilt = true;
                }
                self.down.insert(id, pos);
                self.most = self.most.max(self.down.len());
                None
            }
            TouchPhase::Move => {
                self.check(id, pos, time);
                None
            }
            TouchPhase::End => {
                self.check(id, pos, time);
                if self.down.remove(&id).is_none() || !self.down.is_empty() {
                    return None;
                }
                let action = (!self.spoilt)
                    .then(|| TapAction::of_fingers(self.most))
                    .flatten();
                *self = TapGesture::default();
                action
            }
            TouchPhase::Cancel => {
                self.down.remove(&id);
                self.spoilt = true;
                if self.down.is_empty() {
                    *self = TapGesture::default();
                }
                None
            }
        }
    }

    /// A finger at `pos` at `time`: too far from where it went down, or
    /// too late, spoils the tap.
    fn check(&mut self, id: u64, pos: egui::Pos2, time: f64) {
        if let Some(from) = self.down.get(&id) {
            if from.distance(pos) > MAX_TAP_TRAVEL {
                self.spoilt = true;
            }
        }
        if time - self.start > MAX_TAP_SECONDS {
            self.spoilt = true;
        }
    }

    /// Fingers are down.
    pub fn active(&self) -> bool {
        !self.down.is_empty()
    }
}

impl crate::app::App {
    /// The frame's touches on the drawing (`canvas`): a tap of two
    /// fingers undoes, one of three redoes.
    pub fn touch_taps(&mut self, ctx: &egui::Context, canvas: egui::Rect) {
        let (time, touches): (f64, Vec<(u64, egui::TouchPhase, egui::Pos2)>) = ctx.input(|i| {
            (
                i.time,
                i.events
                    .iter()
                    .filter_map(|e| match e {
                        egui::Event::Touch { id, phase, pos, .. } => Some((id.0, *phase, *pos)),
                        _ => None,
                    })
                    .collect(),
            )
        });
        for (id, phase, pos) in touches {
            // On the drawing itself, not on a window, a menu or the keys
            // drawn over it.
            let on_drawing = canvas.contains(pos)
                && ctx
                    .layer_id_at(pos)
                    .is_none_or(|l| l.order == egui::Order::Background);
            match self.touch_tap.touch(id, phase, pos, time, on_drawing) {
                Some(TapAction::Undo) => self.undo(),
                Some(TapAction::Redo) => self.redo(),
                None => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{pos2, TouchPhase};

    /// Feed `(finger, phase, x, y, time)` and collect the actions.
    fn run(steps: &[(u64, TouchPhase, f32, f32, f64)]) -> Vec<TapAction> {
        let mut g = TapGesture::default();
        steps
            .iter()
            .filter_map(|&(id, phase, x, y, t)| g.touch(id, phase, pos2(x, y), t, true))
            .collect()
    }

    use TouchPhase::{Cancel, End, Move, Start};

    #[test]
    fn two_fingers_tapped_undo_and_three_redo() {
        assert_eq!(
            run(&[
                (1, Start, 100.0, 100.0, 0.0),
                (2, Start, 160.0, 100.0, 0.03),
                (1, End, 101.0, 100.0, 0.15),
                (2, End, 160.0, 102.0, 0.17),
            ]),
            [TapAction::Undo]
        );
        assert_eq!(
            run(&[
                (1, Start, 100.0, 100.0, 0.0),
                (2, Start, 160.0, 100.0, 0.02),
                (3, Start, 220.0, 100.0, 0.04),
                (2, End, 160.0, 100.0, 0.2),
                (1, End, 100.0, 100.0, 0.21),
                (3, End, 220.0, 100.0, 0.22),
            ]),
            [TapAction::Redo]
        );
        // Two taps in a row: two undos.
        assert_eq!(
            run(&[
                (1, Start, 100.0, 100.0, 0.0),
                (2, Start, 160.0, 100.0, 0.0),
                (1, End, 100.0, 100.0, 0.1),
                (2, End, 160.0, 100.0, 0.1),
                (3, Start, 100.0, 100.0, 0.5),
                (4, Start, 160.0, 100.0, 0.5),
                (3, End, 100.0, 100.0, 0.6),
                (4, End, 160.0, 100.0, 0.6),
            ]),
            [TapAction::Undo, TapAction::Undo]
        );
    }

    /// A pinch, a two-finger pan, a long press and one finger are no taps.
    #[test]
    fn pinches_pans_holds_and_one_finger_are_no_taps() {
        // Pinch: the fingers move apart.
        assert!(run(&[
            (1, Start, 100.0, 100.0, 0.0),
            (2, Start, 160.0, 100.0, 0.0),
            (1, Move, 80.0, 100.0, 0.1),
            (2, Move, 180.0, 100.0, 0.1),
            (1, End, 80.0, 100.0, 0.2),
            (2, End, 180.0, 100.0, 0.2),
        ])
        .is_empty());
        // Pan: both move the same way, even a quick one.
        assert!(run(&[
            (1, Start, 100.0, 100.0, 0.0),
            (2, Start, 160.0, 100.0, 0.0),
            (1, Move, 100.0, 140.0, 0.1),
            (2, Move, 160.0, 140.0, 0.1),
            (1, End, 100.0, 140.0, 0.15),
            (2, End, 160.0, 140.0, 0.15),
        ])
        .is_empty());
        // Held too long, without moving.
        assert!(run(&[
            (1, Start, 100.0, 100.0, 0.0),
            (2, Start, 160.0, 100.0, 0.0),
            (1, End, 100.0, 100.0, 0.5),
            (2, End, 160.0, 100.0, 0.5),
        ])
        .is_empty());
        // One finger, and four.
        assert!(run(&[(1, Start, 100.0, 100.0, 0.0), (1, End, 100.0, 100.0, 0.1)]).is_empty());
        assert!(run(&[
            (1, Start, 0.0, 0.0, 0.0),
            (2, Start, 50.0, 0.0, 0.0),
            (3, Start, 100.0, 0.0, 0.0),
            (4, Start, 150.0, 0.0, 0.0),
            (1, End, 0.0, 0.0, 0.1),
            (2, End, 50.0, 0.0, 0.1),
            (3, End, 100.0, 0.0, 0.1),
            (4, End, 150.0, 0.0, 0.1),
        ])
        .is_empty());
        // Cancelled by the system.
        assert!(run(&[
            (1, Start, 100.0, 100.0, 0.0),
            (2, Start, 160.0, 100.0, 0.0),
            (1, Cancel, 100.0, 100.0, 0.1),
            (2, End, 160.0, 100.0, 0.1),
        ])
        .is_empty());
        // A little wobble is still a tap.
        assert_eq!(
            run(&[
                (1, Start, 100.0, 100.0, 0.0),
                (2, Start, 160.0, 100.0, 0.0),
                (1, Move, 105.0, 104.0, 0.05),
                (1, End, 106.0, 104.0, 0.1),
                (2, End, 160.0, 100.0, 0.1),
            ]),
            [TapAction::Undo]
        );
    }

    /// A finger that went down off the drawing (on a bar or a docker)
    /// makes no tap.
    #[test]
    fn a_finger_off_the_drawing_makes_no_tap() {
        let mut g = TapGesture::default();
        assert_eq!(g.touch(1, Start, pos2(0.0, 0.0), 0.0, false), None);
        assert_eq!(g.touch(2, Start, pos2(50.0, 0.0), 0.0, true), None);
        assert!(g.active());
        assert_eq!(g.touch(1, End, pos2(0.0, 0.0), 0.1, true), None);
        assert_eq!(g.touch(2, End, pos2(50.0, 0.0), 0.1, true), None);
        assert!(!g.active());
        // The next gesture starts afresh.
        g.touch(3, Start, pos2(0.0, 0.0), 1.0, true);
        g.touch(4, Start, pos2(50.0, 0.0), 1.0, true);
        g.touch(3, End, pos2(0.0, 0.0), 1.1, true);
        assert_eq!(
            g.touch(4, End, pos2(50.0, 0.0), 1.1, true),
            Some(TapAction::Undo)
        );
        assert_eq!(TapAction::of_fingers(1), None);
    }
}
