//! Clicks through the Wayland virtual-pointer protocol, which Hyprland
//! offers to clients and which needs no device permission (REQ-ACT-01,
//! REQ-ACT-02).

use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use thodu_core::input::Input;
use thodu_core::model::Monitor;
use thodu_core::session::Button;
use wayland_client::protocol::{wl_output, wl_pointer, wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, QueueHandle, delegate_noop};
use wayland_protocols_wlr::virtual_pointer::v1::client::{
    zwlr_virtual_pointer_manager_v1, zwlr_virtual_pointer_v1,
};

const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const BTN_MIDDLE: u32 = 0x112;

#[derive(Default)]
struct Globals {
    manager: Option<zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1>,
    seat: Option<wl_seat::WlSeat>,
    outputs: Vec<(wl_output::WlOutput, String)>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for Globals {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match interface.as_str() {
                "zwlr_virtual_pointer_manager_v1" => {
                    state.manager = Some(registry.bind(name, version.min(2), qh, ()))
                }
                "wl_seat" if state.seat.is_none() => {
                    state.seat = Some(registry.bind(name, version.min(7), qh, ()))
                }
                "wl_output" if version >= 4 => {
                    let output: wl_output::WlOutput = registry.bind(name, 4, qh, ());
                    state.outputs.push((output, String::new()));
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<wl_output::WlOutput, ()> for Globals {
    fn event(
        state: &mut Self,
        output: &wl_output::WlOutput,
        event: wl_output::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_output::Event::Name { name } = event
            && let Some(entry) = state.outputs.iter_mut().find(|(o, _)| o == output)
        {
            entry.1 = name;
        }
    }
}

delegate_noop!(Globals: ignore wl_seat::WlSeat);
delegate_noop!(Globals: zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1);
delegate_noop!(Globals: zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1);

fn now_ms() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u32)
        .unwrap_or_default()
}

pub const fn button_code(button: Button) -> u32 {
    match button {
        Button::Left => BTN_LEFT,
        Button::Right => BTN_RIGHT,
        Button::Middle => BTN_MIDDLE,
    }
}

/// How long a Control press through uinput takes to reach the compositor,
/// which reads it on another path than the virtual pointer.
const CTRL_SETTLE: Duration = Duration::from_millis(30);

/// Carry out `inputs` on `monitor` in order (REQ-ACT-01, REQ-ACT-02), with
/// `ctrl` holding or releasing Control, then release Control and move the
/// pointer back to `restore`, even if an input or roundtrip fails.
pub fn perform(
    monitor: &Monitor,
    inputs: &[Input],
    restore: Option<(&Monitor, (i32, i32))>,
    ctrl: &mut dyn FnMut(bool) -> Result<(), String>,
) -> Result<(), String> {
    let conn = Connection::connect_to_env().map_err(|e| format!("no Wayland display: {e}"))?;
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    conn.display().get_registry(&qh, ());
    let mut globals = Globals::default();
    queue.roundtrip(&mut globals).map_err(|e| e.to_string())?;
    queue.roundtrip(&mut globals).map_err(|e| e.to_string())?;
    // Proxies are cheap handles; copies leave `globals` free for the queue.
    let manager = globals
        .manager
        .clone()
        .ok_or("the compositor offers no virtual pointer")?;
    let seat = globals.seat.clone();
    let outputs = globals.outputs.clone();
    let pointer_on =
        |m: &Monitor| -> Result<zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1, String> {
            let output = outputs
                .iter()
                .find(|(_, n)| *n == m.name)
                .map(|(o, _)| o)
                .ok_or_else(|| format!("no output named {}", m.name))?;
            Ok(manager.create_virtual_pointer_with_output(seat.as_ref(), Some(output), &qh, ()))
        };
    let move_to =
        |p: &zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1, m: &Monitor, (x, y): (i32, i32)| {
            let area = m.logical();
            let local_x = (x - area.x).clamp(0, area.w - 1) as u32;
            let local_y = (y - area.y).clamp(0, area.h - 1) as u32;
            p.motion_absolute(now_ms(), local_x, local_y, area.w as u32, area.h as u32);
            p.frame();
        };
    let pointer = pointer_on(monitor)?;
    let mut result = Ok(());
    let mut ctrl_down = false;
    for input in inputs {
        match *input {
            Input::Move(x, y) => move_to(&pointer, monitor, (x, y)),
            Input::Press(b) | Input::Release(b) => {
                let state = if matches!(input, Input::Press(_)) {
                    wl_pointer::ButtonState::Pressed
                } else {
                    wl_pointer::ButtonState::Released
                };
                pointer.button(now_ms(), button_code(b), state);
                pointer.frame();
            }
            Input::Ctrl(down) => {
                // What the pointer did so far reaches the compositor first.
                if let Err(e) = queue.roundtrip(&mut globals) {
                    result = Err(e.to_string());
                    break;
                }
                // A failed emit leaves the modifier state uncertain, so
                // cleanup tries to release it after a failed press too.
                if down {
                    ctrl_down = true;
                }
                if let Err(e) = ctrl(down) {
                    result = Err(e);
                    break;
                }
                ctrl_down = down;
                thread::sleep(CTRL_SETTLE);
            }
        }
    }
    if let Err(e) = release_ctrl_if_held(ctrl, &mut ctrl_down) {
        keep_error(&mut result, e);
    }
    if let Err(e) = queue.roundtrip(&mut globals) {
        keep_error(&mut result, e.to_string());
    }
    if let Some((m, p)) = restore {
        match if m.name == monitor.name {
            Ok(pointer.clone())
        } else {
            pointer_on(m)
        } {
            Ok(back) => {
                move_to(&back, m, p);
                if back != pointer {
                    back.destroy();
                }
            }
            Err(e) => keep_error(&mut result, e),
        }
    }
    pointer.destroy();
    if let Err(e) = queue.roundtrip(&mut globals) {
        keep_error(&mut result, e.to_string());
    }
    result
}

fn release_ctrl_if_held(
    ctrl: &mut dyn FnMut(bool) -> Result<(), String>,
    ctrl_down: &mut bool,
) -> Result<(), String> {
    if !*ctrl_down {
        return Ok(());
    }
    ctrl(false)?;
    *ctrl_down = false;
    Ok(())
}

fn keep_error(result: &mut Result<(), String>, error: String) {
    match result {
        Ok(()) => *result = Err(error),
        Err(previous) => {
            previous.push_str("; cleanup: ");
            previous.push_str(&error);
        }
    }
}

/// Move the pointer to a logical point on `monitor`.
pub fn move_to(monitor: &Monitor, point: (i32, i32)) -> Result<(), String> {
    perform(monitor, &[Input::Move(point.0, point.1)], None, &mut |_| {
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::release_ctrl_if_held;

    // REQ-ACT-02: when an action exits while Control is held, cleanup sends
    // a release before returning.
    #[test]
    fn req_act_02_control_is_released_after_action_error() {
        let mut ctrl_down = true;
        let mut sent = Vec::new();
        release_ctrl_if_held(
            &mut |down| {
                sent.push(down);
                Ok(())
            },
            &mut ctrl_down,
        )
        .expect("release succeeds");
        assert_eq!(sent, [false]);
        assert!(!ctrl_down);
    }
}
