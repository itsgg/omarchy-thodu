//! Hyprland's request socket: the focused window, a window by address,
//! monitors and the cursor. Only queries; nothing here dispatches.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;
use thodu_core::model::{Monitor, Window};

pub fn socket_path() -> Result<PathBuf, String> {
    let runtime =
        std::env::var("XDG_RUNTIME_DIR").map_err(|_| "XDG_RUNTIME_DIR is not set".to_string())?;
    let signature = std::env::var("HYPRLAND_INSTANCE_SIGNATURE")
        .map_err(|_| "HYPRLAND_INSTANCE_SIGNATURE is not set; is Hyprland running?".to_string())?;
    Ok(PathBuf::from(runtime)
        .join("hypr")
        .join(signature)
        .join(".socket.sock"))
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
    })
}

pub fn active_window() -> Result<Option<Window>, String> {
    Ok(window_from(&json("activewindow")?))
}

pub fn window(address: &str) -> Result<Option<Window>, String> {
    let clients = json("clients")?;
    Ok(clients
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(window_from)
        .find(|w| w.address == address))
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
