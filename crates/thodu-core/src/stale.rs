//! REQ-ACT-03: a target is acted on only while its window is where it was.

use crate::model::Window;

/// Whether the window a session collected from has moved, resized, closed
/// or left the workspace it was on, given what Hyprland reports now.
pub fn is_stale(collected: &Window, now: Option<&Window>) -> bool {
    match now {
        None => true,
        Some(now) => {
            now.address != collected.address
                || now.at != collected.at
                || now.size != collected.size
                || now.workspace != collected.workspace
        }
    }
}
