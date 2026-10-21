//! Clicks through the Wayland virtual-pointer protocol, which Hyprland
//! offers to clients and which needs no device permission (REQ-ACT-01).

use std::time::{SystemTime, UNIX_EPOCH};

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

/// Move to the logical point on `monitor`, press and release `button`
/// `count` times, then move back to `restore` on `restore_monitor`.
pub fn click(
    monitor: &Monitor,
    point: (i32, i32),
    button: Button,
    count: u8,
    restore: Option<(&Monitor, (i32, i32))>,
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
    move_to(&pointer, monitor, point);
    let code = button_code(button);
    for _ in 0..count.max(1) {
        pointer.button(now_ms(), code, wl_pointer::ButtonState::Pressed);
        pointer.frame();
        pointer.button(now_ms(), code, wl_pointer::ButtonState::Released);
        pointer.frame();
    }
    queue.roundtrip(&mut globals).map_err(|e| e.to_string())?;
    if let Some((m, p)) = restore {
        let back = if m.name == monitor.name {
            pointer.clone()
        } else {
            pointer_on(m)?
        };
        move_to(&back, m, p);
        if back != pointer {
            back.destroy();
        }
    }
    pointer.destroy();
    queue.roundtrip(&mut globals).map_err(|e| e.to_string())?;
    Ok(())
}
