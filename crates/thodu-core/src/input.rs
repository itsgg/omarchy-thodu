//! What an action does to the pointer and the keyboard, in order
//! (REQ-ACT-01, REQ-ACT-02). `thodud` carries the plan out; the
//! conformance cases check it.

use std::fmt;

use crate::session::{Act, Button};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    /// Move the pointer to a logical point.
    Move(i32, i32),
    Press(Button),
    Release(Button),
    /// Hold or let go of Control.
    Ctrl(bool),
}

impl fmt::Display for Input {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = |b: &Button| match b {
            Button::Left => "left",
            Button::Right => "right",
            Button::Middle => "middle",
        };
        match self {
            Input::Move(x, y) => write!(f, "move {x} {y}"),
            Input::Press(b) => write!(f, "press {}", name(b)),
            Input::Release(b) => write!(f, "release {}", name(b)),
            Input::Ctrl(true) => write!(f, "ctrl down"),
            Input::Ctrl(false) => write!(f, "ctrl up"),
        }
    }
}

/// The inputs for `act`: a move to its point unless it repeats the click
/// before it, Control around the clicks when asked, and `count` clicks.
pub fn plan(act: &Act) -> Vec<Input> {
    let mut inputs = Vec::new();
    if !act.again {
        inputs.push(Input::Move(act.point.0, act.point.1));
    }
    if act.ctrl {
        inputs.push(Input::Ctrl(true));
    }
    for _ in 0..act.count.max(1) {
        inputs.push(Input::Press(act.button));
        inputs.push(Input::Release(act.button));
    }
    if act.ctrl {
        inputs.push(Input::Ctrl(false));
    }
    inputs
}

/// REQ-ACT-01: whether the pointer goes back where it was after a click.
/// When Hyprland's focus follows the mouse, moving it back over another
/// window, or off every window, would take the keyboard from the window
/// just clicked, so it stays on the target instead.
pub fn should_restore(
    follows_mouse: bool,
    under_restore: Option<&str>,
    under_click: Option<&str>,
) -> bool {
    !follows_mouse || under_restore == under_click
}
