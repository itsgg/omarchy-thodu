//! Hyprland's request socket for queries and the cursor-focus refresh used
//! by repeated scrolling.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;
use thodu_core::model::{Monitor, Window};
use thodu_core::protocol::hyprland_socket_path as hypr_socket_path;

use crate::focus::DispatchSyntax;

pub fn socket_path() -> Result<PathBuf, String> {
    let runtime =
        std::env::var("XDG_RUNTIME_DIR").map_err(|_| "XDG_RUNTIME_DIR is not set".to_string())?;
    let signature = std::env::var("HYPRLAND_INSTANCE_SIGNATURE")
        .map_err(|_| "HYPRLAND_INSTANCE_SIGNATURE is not set; is Hyprland running?".to_string())?;
    Ok(hypr_socket_path(Path::new(&runtime), &signature))
}

/// One request; Hyprland answers and closes the connection.
pub fn request(command: &str) -> Result<String, String> {
    let path = socket_path()?;
    let mut stream = UnixStream::connect(&path)
        .map_err(|e| format!("cannot reach Hyprland at {}: {e}", path.display()))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    stream
        .write_all(command.as_bytes())
        .map_err(|e| e.to_string())?;
    let mut out = String::new();
    stream.read_to_string(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

fn json(command: &str) -> Result<Value, String> {
    serde_json::from_str(&request(&format!("j/{command}"))?)
        .map_err(|e| format!("Hyprland's {command}: {e}"))
}

fn pair(v: &Value) -> (i32, i32) {
    let get = |i: usize| v.get(i).and_then(Value::as_i64).unwrap_or_default() as i32;
    (get(0), get(1))
}

pub fn window_from(v: &Value) -> Option<Window> {
    let address = v.get("address")?.as_str()?.to_string();
    if address.is_empty() {
        return None;
    }
    Some(Window {
        address,
        class: v
            .get("class")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        title: v
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        pid: v.get("pid").and_then(Value::as_u64).unwrap_or_default() as u32,
        at: pair(v.get("at")?),
        size: pair(v.get("size")?),
        workspace: v
            .get("workspace")
            .and_then(|w| w.get("id"))
            .and_then(Value::as_i64)
            .unwrap_or_default(),
        monitor: v.get("monitor").and_then(Value::as_i64).unwrap_or_default(),
        floating: v.get("floating").and_then(Value::as_bool) == Some(true),
        pinned: v.get("pinned").and_then(Value::as_bool) == Some(true),
        fullscreen: v
            .get("fullscreen")
            .and_then(Value::as_i64)
            .unwrap_or_default(),
        focus_history_id: v
            .get("focusHistoryID")
            .and_then(Value::as_i64)
            .unwrap_or(i64::MAX),
        focused: false,
    })
}

pub fn active_window() -> Result<Option<Window>, String> {
    Ok(window_from(&json("activewindow")?))
}

/// Windows shown on one monitor: its active workspace, any shown special
/// workspace, and pinned clients. The returned order is front to back.
pub fn visible_windows(monitor_id: i64) -> Result<Vec<Window>, String> {
    let monitors = json("monitors")?;
    let clients = json("clients")?;
    Ok(visible_windows_from(&monitors, &clients, monitor_id))
}

fn visible_windows_from(monitors: &Value, clients: &Value, monitor_id: i64) -> Vec<Window> {
    let Some(monitor) = monitors
        .as_array()
        .into_iter()
        .flatten()
        .find(|m| m.get("id").and_then(Value::as_i64) == Some(monitor_id))
    else {
        return Vec::new();
    };
    let active = monitor
        .get("activeWorkspace")
        .and_then(|w| w.get("id"))
        .and_then(Value::as_i64);
    let special = monitor
        .get("specialWorkspace")
        .and_then(|w| w.get("id"))
        .and_then(Value::as_i64)
        .filter(|id| *id != 0);
    let mut windows: Vec<_> = clients
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
        .filter_map(|(order, client)| {
            if client.get("mapped").and_then(Value::as_bool) == Some(false)
                || client.get("hidden").and_then(Value::as_bool) == Some(true)
            {
                return None;
            }
            let window = window_from(client)?;
            let pinned = client.get("pinned").and_then(Value::as_bool) == Some(true);
            let shown = active == Some(window.workspace) || special == Some(window.workspace);
            if window.monitor != monitor_id || (!pinned && !shown) {
                return None;
            }
            Some(((window.stacking_key(), order), window))
        })
        .collect();
    windows.sort_by_key(|(key, _)| *key);
    windows.into_iter().map(|(_, window)| window).collect()
}

/// The window at `address`, or with `class:NAME` the first window of that
/// class, for live checks and bench on a window that is not focused.
pub fn window(address: &str) -> Result<Option<Window>, String> {
    let clients = json("clients")?;
    let class = address.strip_prefix("class:");
    Ok(clients
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(window_from)
        .find(|w| match class {
            Some(c) => w.class.eq_ignore_ascii_case(c),
            None => w.address == address,
        }))
}

pub struct MonitorEntry {
    pub id: i64,
    pub monitor: Monitor,
}

pub fn monitors() -> Result<Vec<MonitorEntry>, String> {
    let list = json("monitors")?;
    Ok(list
        .as_array()
        .into_iter()
        .flatten()
        .map(|m| MonitorEntry {
            id: m.get("id").and_then(Value::as_i64).unwrap_or_default(),
            monitor: Monitor {
                name: m
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                x: m.get("x").and_then(Value::as_i64).unwrap_or_default() as i32,
                y: m.get("y").and_then(Value::as_i64).unwrap_or_default() as i32,
                width: m.get("width").and_then(Value::as_i64).unwrap_or_default() as i32,
                height: m.get("height").and_then(Value::as_i64).unwrap_or_default() as i32,
                scale: m.get("scale").and_then(Value::as_f64).unwrap_or(1.0),
            },
        })
        .collect())
}

/// The monitor Hyprland has focused, with its id.
pub fn focused_monitor() -> Result<MonitorEntry, String> {
    let list = json("monitors")?;
    let focused = focused_monitor_id(&list).ok_or("no monitor is focused")?;
    monitors()?
        .into_iter()
        .find(|m| m.id == focused)
        .ok_or_else(|| format!("no monitor {focused}"))
}

fn focused_monitor_id(list: &Value) -> Option<i64> {
    list.as_array()?
        .iter()
        .find(|m| m.get("focused").and_then(Value::as_bool) == Some(true))?
        .get("id")?
        .as_i64()
}

/// The active workspace on a monitor, or None when Hyprland has no report.
pub fn active_workspace_id(name: &str) -> Option<i64> {
    let list = json("monitors").ok()?;
    active_workspace_id_from(&list, name)
}

fn active_workspace_id_from(list: &Value, name: &str) -> Option<i64> {
    list.as_array()?
        .iter()
        .find(|m| m.get("name").and_then(Value::as_str) == Some(name))?
        .get("activeWorkspace")?
        .get("id")?
        .as_i64()
}

pub fn monitor_of(window: &Window) -> Result<Monitor, String> {
    monitors()?
        .into_iter()
        .find(|m| m.id == window.monitor)
        .map(|m| m.monitor)
        .ok_or_else(|| {
            format!(
                "no monitor {} for window {}",
                window.monitor, window.address
            )
        })
}

/// The cursor in logical coordinates.
pub fn cursor() -> Result<(i32, i32), String> {
    let v = json("cursorpos")?;
    let get = |k: &str| v.get(k).and_then(Value::as_f64).unwrap_or_default().round() as i32;
    Ok((get("x"), get("y")))
}

fn cursor_focus_refresh_command((x, y): (i32, i32), syntax: DispatchSyntax) -> String {
    match syntax {
        DispatchSyntax::Lua => format!("/dispatch hl.dsp.cursor.move({{ x = {x}, y = {y} }})"),
        DispatchSyntax::Hyprlang => format!("/dispatch movecursor {x} {y}"),
    }
}

/// Ask Hyprland to recompute pointer focus without changing the coordinates.
/// Its cursor dispatcher refreshes pointer focus even when the cursor is
/// already at this position, which lets held wheel events reach the app
/// without a visible cursor nudge.
pub fn refresh_cursor_focus(point: (i32, i32), syntax: DispatchSyntax) -> Result<(), String> {
    let response = request(&cursor_focus_refresh_command(point, syntax))?;
    if response.trim() == "ok" {
        Ok(())
    } else {
        Err(format!(
            "Hyprland did not refresh pointer focus: {}",
            response.trim()
        ))
    }
}

/// Where the layer-shell surface `namespace` sits on the monitor `name`,
/// in logical coordinates.
pub fn layer_origin(name: &str, namespace: &str) -> Option<(i32, i32)> {
    let layers = json("layers").ok()?;
    let levels = layers.get(name)?.get("levels")?.as_object()?;
    levels
        .values()
        .flat_map(|l| l.as_array().into_iter().flatten())
        .find_map(|l| {
            (l.get("namespace")?.as_str()? == namespace).then(|| {
                let get = |k: &str| l.get(k).and_then(Value::as_i64).unwrap_or_default() as i32;
                (get("x"), get("y"))
            })
        })
}

/// Whether moving the pointer moves the keyboard focus
/// (`input:follow_mouse` above 0).
pub fn follows_mouse() -> bool {
    json("getoption input:follow_mouse")
        .ok()
        .and_then(|v| v.get("int").and_then(Value::as_i64))
        .is_none_or(|n| n != 0)
}

/// The address of the frontmost window under a logical point on a shown
/// workspace.
pub fn window_at(x: i32, y: i32) -> Option<String> {
    let monitors = json("monitors").ok()?;
    let clients = json("clients").ok()?;
    window_at_from(&monitors, &clients, x, y)
}

fn window_at_from(monitors: &Value, clients: &Value, x: i32, y: i32) -> Option<String> {
    let shown: Vec<i64> = monitors
        .as_array()?
        .iter()
        .flat_map(|m| {
            ["activeWorkspace", "specialWorkspace"]
                .into_iter()
                .filter_map(|k| m.get(k)?.get("id")?.as_i64())
        })
        .filter(|id| *id != 0)
        .collect();
    clients
        .as_array()?
        .iter()
        .enumerate()
        .filter(|(_, c)| c.get("mapped").and_then(Value::as_bool) != Some(false))
        .filter(|(_, c)| c.get("hidden").and_then(Value::as_bool) != Some(true))
        .filter_map(|(order, c)| {
            let w = window_from(c)?;
            ((w.pinned || shown.contains(&w.workspace)) && w.rect().contains_point(x, y))
                .then_some((w.stacking_key(), order, w.address))
        })
        .min_by_key(|(key, order, _)| (*key, *order))
        .map(|(_, _, address)| address)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::cursor_focus_refresh_command;
    use crate::focus::DispatchSyntax;

    // REQ-ACT-06: a stationary scroll pointer refreshes Hyprland focus at
    // the same coordinate instead of using a visible movement as a nudge.
    #[test]
    fn req_act_06_focus_refresh_dispatch_keeps_the_pointer_stationary() {
        assert_eq!(
            cursor_focus_refresh_command((768, 493), DispatchSyntax::Lua),
            "/dispatch hl.dsp.cursor.move({ x = 768, y = 493 })"
        );
        assert_eq!(
            cursor_focus_refresh_command((768, 493), DispatchSyntax::Hyprlang),
            "/dispatch movecursor 768 493"
        );
    }

    #[test]
    fn req_act_02_active_workspace_is_scoped_to_monitor() {
        let monitors = json!([
            { "name": "DP-1", "activeWorkspace": { "id": 3 } },
            { "name": "eDP-1", "activeWorkspace": { "id": 1 } }
        ]);
        assert_eq!(super::active_workspace_id_from(&monitors, "eDP-1"), Some(1));
        assert_eq!(super::active_workspace_id_from(&monitors, "DP-1"), Some(3));
        assert_eq!(super::active_workspace_id_from(&monitors, "missing"), None);
    }

    // REQ-SRC-07: mapped clients on inactive workspaces are not visible
    // targets; pinned and shown special-workspace clients are.
    #[test]
    fn req_src_07_visible_windows_are_scoped_to_the_monitor_and_shown_workspaces() {
        let monitors = json!([
            {
                "id": 0,
                "activeWorkspace": { "id": 5 },
                "specialWorkspace": { "id": -99 }
            },
            {
                "id": 1,
                "activeWorkspace": { "id": 8 },
                "specialWorkspace": { "id": 0 }
            }
        ]);
        let client = |address: &str, workspace: i64, monitor: i64, history: i64| {
            json!({
                "address": address,
                "class": "app",
                "title": "window",
                "pid": 10,
                "at": [0, 0],
                "size": [100, 100],
                "workspace": { "id": workspace },
                "monitor": monitor,
                "mapped": true,
                "hidden": false,
                "visible": true,
                "pinned": false,
                "floating": false,
                "fullscreen": 0,
                "focusHistoryID": history
            })
        };
        let mut pinned = client("0xp", 3, 0, 2);
        pinned["pinned"] = json!(true);
        let mut hidden = client("0xh", 5, 0, 1);
        hidden["hidden"] = json!(true);
        let mut unmapped = client("0xu", 5, 0, 1);
        unmapped["mapped"] = json!(false);
        let clients = json!([
            client("0xi", 4, 0, 0),
            client("0xs", -99, 0, 3),
            pinned,
            client("0xo", 5, 1, 1),
            hidden,
            unmapped,
            client("0xa", 5, 0, 1)
        ]);

        let windows = super::visible_windows_from(&monitors, &clients, 0);
        assert_eq!(
            windows
                .iter()
                .map(|window| window.address.as_str())
                .collect::<Vec<_>>(),
            ["0xp", "0xs", "0xa"]
        );
    }

    // REQ-SRC-07: Hyprland draws floating windows above tiled windows even
    // when the tiled window has the newer focus history.
    #[test]
    fn req_src_07_floating_window_owns_an_overlap_above_the_active_tiled_window() {
        let monitors = json!([{
            "activeWorkspace": { "id": 5 },
            "specialWorkspace": { "id": 0 }
        }]);
        let client = |address: &str, floating: bool, history: i64| {
            json!({
                "address": address,
                "class": "app",
                "title": "window",
                "pid": 10,
                "at": [0, 0],
                "size": [100, 100],
                "workspace": { "id": 5 },
                "monitor": 0,
                "mapped": true,
                "hidden": false,
                "pinned": false,
                "floating": floating,
                "fullscreen": 0,
                "focusHistoryID": history
            })
        };
        let clients = json!([client("0xtiled", false, 0), client("0xfloating", true, 3)]);

        assert_eq!(
            super::window_at_from(&monitors, &clients, 50, 50).as_deref(),
            Some("0xfloating")
        );
    }

    // REQ-SESSION-06: Hyprland exposes the focused monitor independently of
    // whether any app window is active.
    #[test]
    fn req_session_06_focused_monitor_exists_without_active_window() {
        let monitors = json!([
            {
                "id": 4,
                "name": "eDP-1",
                "focused": true,
                "activeWorkspace": { "id": 2 }
            }
        ]);
        assert_eq!(super::focused_monitor_id(&monitors), Some(4));
    }
}
