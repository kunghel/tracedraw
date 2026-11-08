//! TraceDraw: a finger or a graphics tablet's pen on the window's frame
//! on Wayland (#2070).
//!
//! A compositor that leaves the title bar to the application (GNOME's)
//! gets a frame drawn by the Wayland backend, on surfaces of its own
//! beside the window's. winit 0.30 drove that frame from the mouse alone:
//! a finger's touches on it found no window and were dropped, and the pen
//! (`tablet.rs`, #1841) was told to do nothing there, so on a tablet the
//! window could neither be moved, resized, minimized, maximized nor
//! closed without a mouse. The backend now hands a finger's and a pen's
//! input over the frame to this module, which says what the frame is
//! told, the way the mouse drives it:
//!
//! - a finger pressing the frame points at that place and presses the
//!   frame's normal button there; moving the finger points again (the
//!   frame starts a move or a resize the press asked for); lifting it
//!   releases the button (a button of the title bar acts on its release)
//!   and the frame is left;
//! - a pen points where it moves; its tip presses and releases the
//!   normal button, its secondary barrel button presses the alternate
//!   one (the window menu, on a press only); leaving the frame leaves it.
//!
//! This file depends on nothing in the crate, and TraceDraw's tests
//! include and run it (`winit_frame_input::tests`).

/// The frame's buttons, as the mouse's left and right buttons press them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameButton {
    Normal,
    Alternate,
}

/// What a finger or a pen did over the frame, surface-local places in
/// logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FrameInput {
    /// A finger touched the frame here.
    FingerDown(f64, f64),
    /// A finger or a pen moved here.
    Moved(f64, f64),
    /// The finger on the frame was lifted.
    FingerUp,
    /// A pen's button pressed (`true`) or released.
    PenButton(FrameButton, bool),
    /// The pen left the frame, or the compositor took the touches back.
    Left,
}

/// What the frame is told, in this order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FrameStep {
    /// The point is at this place of the frame.
    Point(f64, f64),
    /// A button pressed (`true`) or released.
    Click(FrameButton, bool),
    /// The point left the frame: a move asked for and not started is
    /// dropped with it.
    Leave,
}

/// The steps of `input`, at most two.
pub fn frame_steps(input: FrameInput) -> impl Iterator<Item = FrameStep> {
    let (first, second) = match input {
        FrameInput::FingerDown(x, y) => (
            Some(FrameStep::Point(x, y)),
            Some(FrameStep::Click(FrameButton::Normal, true)),
        ),
        FrameInput::Moved(x, y) => (Some(FrameStep::Point(x, y)), None),
        FrameInput::FingerUp => (
            Some(FrameStep::Click(FrameButton::Normal, false)),
            Some(FrameStep::Leave),
        ),
        // The window menu opens on the press; the frame takes a release
        // of the alternate button for another menu.
        FrameInput::PenButton(FrameButton::Alternate, false) => (None, None),
        FrameInput::PenButton(button, pressed) => (Some(FrameStep::Click(button, pressed)), None),
        FrameInput::Left => (Some(FrameStep::Leave), None),
    };
    first.into_iter().chain(second)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn steps(input: FrameInput) -> Vec<FrameStep> {
        frame_steps(input).collect()
    }

    /// A finger on the title bar points there before it presses, so the
    /// frame knows what the press is on (#2070).
    #[test]
    fn a_finger_points_then_presses_and_releases_then_leaves() {
        assert_eq!(
            steps(FrameInput::FingerDown(40.0, 12.0)),
            vec![
                FrameStep::Point(40.0, 12.0),
                FrameStep::Click(FrameButton::Normal, true),
            ]
        );
        assert_eq!(
            steps(FrameInput::Moved(60.0, 14.0)),
            vec![FrameStep::Point(60.0, 14.0)]
        );
        assert_eq!(
            steps(FrameInput::FingerUp),
            vec![
                FrameStep::Click(FrameButton::Normal, false),
                FrameStep::Leave,
            ]
        );
        assert_eq!(steps(FrameInput::Left), vec![FrameStep::Leave]);
    }

    /// The pen's tip is the normal button both ways; its barrel button
    /// opens the window menu on its press only (#2070).
    #[test]
    fn a_pen_presses_with_its_tip_and_opens_the_menu_with_its_button() {
        assert_eq!(
            steps(FrameInput::PenButton(FrameButton::Normal, true)),
            vec![FrameStep::Click(FrameButton::Normal, true)]
        );
        assert_eq!(
            steps(FrameInput::PenButton(FrameButton::Normal, false)),
            vec![FrameStep::Click(FrameButton::Normal, false)]
        );
        assert_eq!(
            steps(FrameInput::PenButton(FrameButton::Alternate, true)),
            vec![FrameStep::Click(FrameButton::Alternate, true)]
        );
        assert!(steps(FrameInput::PenButton(FrameButton::Alternate, false)).is_empty());
    }
}
