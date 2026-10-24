//! Pointer focus for a wheel action (REQ-ACT-04, REQ-ACT-06).
//!
//! Hyprland 0.56 gives pointer focus to any layer surface with exclusive
//! keyboard focus, ignoring its input region (InputManager.cpp,
//! mouseMoveUnified, "forced above all"), so a wheel event under the
//! overlay's pass-through hole still goes to the overlay. The one focus
//! Hyprland ranks above that layer is a window with `stay_focused`, and
//! while the exclusive layer holds the keyboard that window gets only the
//! pointer. So each wheel action sets `stay_focused` on the owning window,
//! moves the pointer so Hyprland recomputes focus, sends the wheel and
//! clears the property again. The address is recorded until it is cleared,
//! so a restarted thodud can clear a property a crash left behind.

use std::fs;
use std::path::{Path, PathBuf};

/// The dispatch for `stay_focused` on `address`, in the Lua syntax first
/// and the older dispatcher syntax second; the first one Hyprland accepts
/// is used.
pub fn stay_focused_commands(address: &str, on: bool) -> [String; 2] {
    let value = if on { "1" } else { "0" };
    [
        format!(
            "dispatch hl.dsp.window.set_prop({{ window = 'address:{address}', prop = 'stay_focused', value = '{value}' }})"
        ),
        format!("dispatch setprop address:{address} stay_focused {value}"),
    ]
}

/// Read the current dynamic window-rule value from Hyprland.
pub fn get_stay_focused<R>(address: &str, mut request: R) -> Result<bool, String>
where
    R: FnMut(&str) -> Result<String, String>,
{
    let command = format!("getprop address:{address} stay_focused");
    let reply = request(&command)?;
    match reply.trim() {
        "true" => Ok(true),
        "false" => Ok(false),
        value => Err(format!(
            "Hyprland returned an invalid stay_focused value for {address}: {value}"
        )),
    }
}

/// Send the first accepted form of a `stay_focused` change.
pub fn set_stay_focused<R>(address: &str, on: bool, mut request: R) -> Result<(), String>
where
    R: FnMut(&str) -> Result<String, String>,
{
    let mut last = String::new();
    for command in stay_focused_commands(address, on) {
        let reply = request(&command)?;
        if reply.trim() == "ok" {
            return Ok(());
        }
        last = reply.trim().to_string();
    }
    Err(format!(
        "Hyprland refused stay_focused for {address}: {last}"
    ))
}

/// Run `body` with the window's pointer focus forced, then clear it
/// whatever `body` returned. A failure to clear wins over `body`'s result.
pub fn with_forced_pointer_focus<G, S, B, T, E>(
    record: &Path,
    address: &str,
    mut get: G,
    mut set: S,
    body: B,
) -> Result<T, E>
where
    G: FnMut(&str) -> Result<bool, String>,
    S: FnMut(&str, bool) -> Result<(), String>,
    B: FnOnce() -> Result<T, E>,
    E: From<String>,
{
    let was_stay_focused = get(address).map_err(E::from)?;
    if was_stay_focused {
        return body();
    }

    fs::write(record, format!("{address}\n{was_stay_focused}\n"))
        .map_err(|error| E::from(format!("could not record pointer focus state: {error}")))?;
    if let Err(error) = set(address, true) {
        if set(address, was_stay_focused).is_ok() {
            let _ = fs::remove_file(record);
        }
        return Err(E::from(error));
    }
    let result = body();
    let restored = set(address, was_stay_focused);
    if restored.is_ok() {
        let _ = fs::remove_file(record);
    }
    match restored {
        Ok(()) => result,
        Err(error) => Err(E::from(error)),
    }
}

fn read_recovery(record: &Path) -> Option<(String, bool)> {
    let contents = fs::read_to_string(record).ok()?;
    let mut lines = contents.lines();
    let address = lines.next()?.trim();
    let value = lines.next()?.trim().parse().ok()?;
    if address.is_empty() || lines.next().is_some() {
        return None;
    }
    Some((address.to_string(), value))
}

/// Restore a property left changed by a crashed thodud.
pub fn clear_leftover<S>(record: &Path, mut set: S)
where
    S: FnMut(&str, bool) -> Result<(), String>,
{
    let Some((address, was_stay_focused)) = read_recovery(record) else {
        return;
    };
    if set(&address, was_stay_focused).is_ok() {
        let _ = fs::remove_file(record);
    }
}

/// Where the address waits while its property is set.
pub fn record_path() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."));
    let dir = base.join("thodu");
    let _ = fs::create_dir_all(&dir);
    dir.join("scroll-pointer-focus")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            PathBuf::from(std::env::var_os("HOME").unwrap()).join(".local/share/thodu/tests/focus");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        let _ = fs::remove_file(&path);
        path
    }

    // REQ-ACT-04, REQ-ACT-06: the wheel runs while the owning window holds
    // forced pointer focus, and the force is cleared afterwards.
    #[test]
    fn req_act_04_req_act_06_wheel_runs_inside_forced_pointer_focus() {
        let record = scratch("order");
        let log = RefCell::new(Vec::new());
        let result: Result<(), String> = with_forced_pointer_focus(
            &record,
            "0xabc",
            |_| Ok(false),
            |address, on| {
                log.borrow_mut().push(format!("set {address} {on}"));
                Ok(())
            },
            || {
                assert_eq!(fs::read_to_string(&record).unwrap(), "0xabc\nfalse\n");
                log.borrow_mut().push("wheel".into());
                Ok(())
            },
        );
        assert!(result.is_ok());
        assert_eq!(
            *log.borrow(),
            vec!["set 0xabc true", "wheel", "set 0xabc false"]
        );
        assert!(!record.exists());
    }

    // REQ-ACT-04: a window's existing stay_focused rule is left intact.
    #[test]
    fn req_act_04_existing_stay_focused_rule_is_preserved() {
        let record = scratch("preexisting");
        let changed = RefCell::new(false);
        let mut wheel = false;
        let result: Result<(), String> = with_forced_pointer_focus(
            &record,
            "0xabc",
            |_| Ok(true),
            |_, _| {
                *changed.borrow_mut() = true;
                Ok(())
            },
            || {
                wheel = true;
                Ok(())
            },
        );
        assert!(result.is_ok());
        assert!(wheel);
        assert!(!*changed.borrow());
        assert!(!record.exists());
    }

    // REQ-ACT-06: a failed wheel action still clears the forced focus.
    #[test]
    fn req_act_06_failed_wheel_still_clears_forced_focus() {
        let record = scratch("failed");
        let cleared = RefCell::new(false);
        let result: Result<(), String> = with_forced_pointer_focus(
            &record,
            "0xabc",
            |_| Ok(false),
            |_, on| {
                if !on {
                    *cleared.borrow_mut() = true;
                }
                Ok(())
            },
            || Err("wheel failed".to_string()),
        );
        assert_eq!(result, Err("wheel failed".to_string()));
        assert!(*cleared.borrow());
        assert!(!record.exists());
    }

    // REQ-ACT-06: when the force cannot be set, no wheel is sent.
    #[test]
    fn req_act_06_refused_force_sends_no_wheel() {
        let record = scratch("refused");
        let mut wheel = false;
        let result: Result<(), String> = with_forced_pointer_focus(
            &record,
            "0xabc",
            |_| Ok(false),
            |_, on| if on { Err("refused".into()) } else { Ok(()) },
            || {
                wheel = true;
                Ok(())
            },
        );
        assert!(result.is_err());
        assert!(!wheel);
        assert!(!record.exists());
    }

    // REQ-ACT-06: a crash record restores the value captured before focus.
    #[test]
    fn req_act_06_leftover_focus_restores_recorded_value() {
        let record = scratch("leftover");
        fs::write(&record, "0xdef\ntrue\n").unwrap();
        let mut calls = Vec::new();
        clear_leftover(&record, |address, on| {
            calls.push((address.to_string(), on));
            Ok(())
        });
        assert_eq!(calls, vec![("0xdef".to_string(), true)]);
        assert!(!record.exists());
    }

    // REQ-ACT-04: parse the runtime query's boolean response exactly.
    #[test]
    fn req_act_04_get_stay_focused_queries_and_parses_hyprland() {
        let mut commands = Vec::new();
        assert!(
            get_stay_focused("0x5", |command| {
                commands.push(command.to_string());
                Ok("true\n".into())
            })
            .unwrap()
        );
        assert_eq!(commands, vec!["getprop address:0x5 stay_focused"]);
        assert!(!get_stay_focused("0x5", |_| Ok("false\n".into())).unwrap());
        assert!(get_stay_focused("0x5", |_| Ok("unknown".into())).is_err());
    }

    // REQ-ACT-04: Lua dispatch first, the older dispatcher when Lua is refused.
    #[test]
    fn req_act_04_stay_focused_dispatch_detects_syntax() {
        let [lua, legacy] = stay_focused_commands("0x5", true);
        assert_eq!(
            lua,
            "dispatch hl.dsp.window.set_prop({ window = 'address:0x5', prop = 'stay_focused', value = '1' })"
        );
        assert_eq!(legacy, "dispatch setprop address:0x5 stay_focused 1");
        let mut sent = Vec::new();
        let result = set_stay_focused("0x5", false, |command| {
            sent.push(command.to_string());
            Ok(if sent.len() == 1 {
                "Invalid dispatcher".into()
            } else {
                "ok".into()
            })
        });
        assert!(result.is_ok());
        assert_eq!(sent.len(), 2);
        assert!(sent[1].ends_with("stay_focused 0"));
        assert!(set_stay_focused("0x5", true, |_| Ok("error".into())).is_err());
    }
}
