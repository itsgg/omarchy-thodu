//! Temporary pointer focus for wheel actions (REQ-ACT-04, REQ-ACT-06).
//!
//! Hyprland 0.56 can give pointer focus to a window with `stay_focused` even
//! while the overlay keeps exclusive keyboard focus. Thodu applies that
//! property only for one wheel action, journals the previous value per
//! window, and removes its temporary override afterwards. Per-window journal
//! files keep a failed restoration for one window from hiding another's.

use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

const RESTORE_ATTEMPTS: usize = 3;
const RESTORE_RETRY_DELAY: Duration = Duration::from_millis(10);

#[derive(Clone, Debug, PartialEq, Eq)]
struct FocusRecord {
    address: String,
    pid: Option<u32>,
    previous: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DispatchSyntax {
    Lua,
    Hyprlang,
}

impl DispatchSyntax {
    fn parse_provider(provider: &str) -> Result<Self, String> {
        match provider.trim() {
            "lua" => Ok(Self::Lua),
            "hyprlang" => Ok(Self::Hyprlang),
            other => Err(format!("unsupported Hyprland config provider: {other}")),
        }
    }
}

/// Detect the dispatch form once from Hyprland's reported config provider.
pub fn detect_dispatch_syntax<R>(mut request: R) -> Result<DispatchSyntax, String>
where
    R: FnMut(&str) -> Result<String, String>,
{
    let info = request("systeminfo")?;
    let provider = info
        .lines()
        .find_map(|line| line.trim().strip_prefix("configProvider:"))
        .ok_or_else(|| "Hyprland systeminfo did not report configProvider".to_string())?;
    DispatchSyntax::parse_provider(provider)
}

fn set_command(address: &str, value: Option<bool>, syntax: DispatchSyntax) -> String {
    let value = match value {
        Some(true) => "1",
        Some(false) => "0",
        None => "unset",
    };
    match syntax {
        DispatchSyntax::Lua => format!(
            "dispatch hl.dsp.window.set_prop({{ window = 'address:{address}', prop = 'stay_focused', value = '{value}' }})"
        ),
        DispatchSyntax::Hyprlang => {
            format!("dispatch setprop address:{address} stay_focused {value}")
        }
    }
}

fn valid_address(address: &str) -> bool {
    address
        .strip_prefix("0x")
        .is_some_and(|hex| !hex.is_empty() && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

/// Read the current effective window-rule value from Hyprland.
pub fn get_stay_focused<R>(address: &str, mut request: R) -> Result<bool, String>
where
    R: FnMut(&str) -> Result<String, String>,
{
    if !valid_address(address) {
        return Err(format!("invalid Hyprland window address: {address}"));
    }
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

/// Set or clear the temporary `stay_focused` override using the syntax
/// selected when the daemon started.
pub fn set_stay_focused<R>(
    address: &str,
    value: Option<bool>,
    syntax: DispatchSyntax,
    mut request: R,
) -> Result<(), String>
where
    R: FnMut(&str) -> Result<String, String>,
{
    if !valid_address(address) {
        return Err(format!("invalid Hyprland window address: {address}"));
    }
    let command = set_command(address, value, syntax);
    let reply = request(&command)?;
    if reply.trim() == "ok" {
        Ok(())
    } else {
        Err(format!(
            "Hyprland refused stay_focused for {address}: {}",
            reply.trim()
        ))
    }
}

/// Restore the effective value without leaving a Thodu override behind when
/// the window's underlying value was already false.
fn restore<G, S>(address: &str, previous: bool, get: &mut G, set: &mut S) -> Result<(), String>
where
    G: FnMut(&str) -> Result<bool, String>,
    S: FnMut(&str, Option<bool>) -> Result<(), String>,
{
    if previous {
        return set(address, Some(true));
    }

    set(address, None)?;
    if get(address)? {
        // Preserve the effective value if a pre-existing explicit false was
        // shadowing a lower-priority true rule.
        set(address, Some(false))?;
    }
    Ok(())
}

fn record_path(directory: &Path, address: &str, pid: u32) -> Result<PathBuf, String> {
    if !valid_address(address) {
        return Err(format!("invalid Hyprland window address: {address}"));
    }
    Ok(directory.join(format!("{address}-{pid}.state")))
}

fn legacy_record_path(directory: &Path, address: &str) -> Result<PathBuf, String> {
    if !valid_address(address) {
        return Err(format!("invalid Hyprland window address: {address}"));
    }
    Ok(directory.join(format!("{address}.state")))
}

fn write_record(path: &Path, address: &str, pid: u32, previous: bool) -> Result<(), String> {
    let pending = path.with_extension("pending");
    fs::write(&pending, format!("v2\n{address}\n{pid}\n{previous}\n"))
        .map_err(|error| format!("could not write scroll focus recovery record: {error}"))?;
    if let Err(error) = fs::rename(&pending, path) {
        let _ = fs::remove_file(&pending);
        return Err(format!(
            "could not commit scroll focus recovery record: {error}"
        ));
    }
    Ok(())
}

fn read_record(path: &Path) -> Result<FocusRecord, String> {
    let contents = fs::read_to_string(path)
        .map_err(|error| format!("could not read recovery record: {error}"))?;
    let mut lines = contents.lines();
    let version = lines.next().map(str::trim);
    let (address, pid, previous) = if version == Some("v2") {
        let address = lines
            .next()
            .map(str::trim)
            .filter(|address| !address.is_empty())
            .ok_or_else(|| "recovery record has no window address".to_string())?;
        let pid = lines
            .next()
            .map(str::trim)
            .ok_or_else(|| "recovery record has no window pid".to_string())?
            .parse::<u32>()
            .map_err(|_| "recovery record has an invalid window pid".to_string())?;
        let previous = lines
            .next()
            .map(str::trim)
            .ok_or_else(|| "recovery record has no previous value".to_string())?
            .parse::<bool>()
            .map_err(|_| "recovery record has an invalid previous value".to_string())?;
        (address, Some(pid), previous)
    } else {
        // Older formats had no process identity. The first implementation
        // stored only the address and always cleared the property to false.
        let address = version
            .filter(|address| !address.is_empty())
            .ok_or_else(|| "recovery record has no window address".to_string())?;
        let previous = match lines.next().map(str::trim) {
            Some(value) => value
                .parse::<bool>()
                .map_err(|_| "recovery record has an invalid previous value".to_string())?,
            None => false,
        };
        (address, None, previous)
    };
    if !valid_address(address) {
        return Err("recovery record has an invalid window address".to_string());
    }
    if lines.next().is_some() {
        return Err("recovery record has unexpected data".to_string());
    }
    Ok(FocusRecord {
        address: address.to_string(),
        pid,
        previous,
    })
}

fn remove_record(path: &Path) -> Result<(), String> {
    fs::remove_file(path)
        .map_err(|error| format!("could not remove recovered focus record: {error}"))
}

fn discard_unidentified_record(path: &Path, address: &str) -> Result<(), String> {
    eprintln!(
        "ERROR legacy scroll focus record for {address} has no process ID; deleting it without changing the window so scrolling can continue"
    );
    remove_record(path)
}

fn migrate_legacy_record(base: &Path, directory: &Path) -> Result<(), String> {
    let legacy = base.join("thodu/scroll-pointer-focus");
    if !legacy.exists() {
        return Ok(());
    }
    if !legacy.is_file() {
        return Err(format!(
            "legacy focus record {} is not a file",
            legacy.display()
        ));
    }
    let record = read_record(&legacy).map_err(|error| {
        format!(
            "cannot read legacy focus record {}: {error}",
            legacy.display()
        )
    })?;
    let destination = legacy_record_path(directory, &record.address)?;
    if destination.exists() {
        let existing = read_record(&destination)?;
        if existing.address != record.address || existing.previous != record.previous {
            return Err(format!(
                "legacy and per-window focus records disagree for {}",
                record.address
            ));
        }
    } else {
        fs::copy(&legacy, &destination).map_err(|error| {
            format!(
                "could not migrate legacy focus record {}: {error}",
                legacy.display()
            )
        })?;
    }
    fs::remove_file(&legacy).map_err(|error| {
        format!(
            "could not remove migrated focus record {}: {error}",
            legacy.display()
        )
    })
}

fn restore_with_retry<G, S>(
    address: &str,
    previous: bool,
    get: &mut G,
    set: &mut S,
) -> Result<(), String>
where
    G: FnMut(&str) -> Result<bool, String>,
    S: FnMut(&str, Option<bool>) -> Result<(), String>,
{
    let mut last_error = String::new();
    for attempt in 0..RESTORE_ATTEMPTS {
        match restore(address, previous, get, set) {
            Ok(()) => return Ok(()),
            Err(error) => last_error = error,
        }
        if attempt + 1 < RESTORE_ATTEMPTS {
            thread::sleep(RESTORE_RETRY_DELAY);
        }
    }
    Err(format!(
        "restore failed after {RESTORE_ATTEMPTS} attempts: {last_error}"
    ))
}

fn recover_path<G, S>(
    path: &Path,
    address: &str,
    pid: u32,
    get: &mut G,
    set: &mut S,
) -> Result<(), String>
where
    G: FnMut(&str) -> Result<bool, String>,
    S: FnMut(&str, Option<bool>) -> Result<(), String>,
{
    if !path.exists() {
        return Ok(());
    }
    let mut record = match read_record(path) {
        Ok(record) if record.address == address => record,
        Ok(_) | Err(_) => {
            // New-format journals are only written when the effective value
            // was false. The address and pid in the path remain authoritative.
            eprintln!(
                "ERROR damaged scroll focus record {}; restoring {address} to false",
                path.display()
            );
            FocusRecord {
                address: address.to_string(),
                pid: parse_record_filename(path).and_then(|(_, pid)| pid),
                previous: false,
            }
        }
    };
    if record.pid.is_none() {
        record.pid = parse_record_filename(path).and_then(|(_, pid)| pid);
    }
    if record.pid.is_none() {
        return discard_unidentified_record(path, address);
    }
    if record.pid.is_some_and(|record_pid| record_pid != pid) {
        remove_record(path)?;
        return Ok(());
    }
    restore_with_retry(address, record.previous, get, set)?;
    remove_record(path)
}

fn recover_one<G, S>(
    directory: &Path,
    address: &str,
    pid: u32,
    get: &mut G,
    set: &mut S,
) -> Result<(), String>
where
    G: FnMut(&str) -> Result<bool, String>,
    S: FnMut(&str, Option<bool>) -> Result<(), String>,
{
    recover_path(
        &record_path(directory, address, pid)?,
        address,
        pid,
        get,
        set,
    )?;
    recover_path(
        &legacy_record_path(directory, address)?,
        address,
        pid,
        get,
        set,
    )
}

/// Run `body` with the window's pointer focus forced, then restore its prior
/// value even when `body` fails. A pending record for this same window must be
/// recovered before a new wheel action can overwrite it.
pub fn with_forced_pointer_focus<G, S, B, T, E>(
    directory: &Path,
    address: &str,
    pid: u32,
    mut get: G,
    mut set: S,
    body: B,
) -> Result<T, E>
where
    G: FnMut(&str) -> Result<bool, String>,
    S: FnMut(&str, Option<bool>) -> Result<(), String>,
    B: FnOnce() -> Result<T, E>,
    E: From<String>,
{
    let record = record_path(directory, address, pid).map_err(E::from)?;
    recover_one(directory, address, pid, &mut get, &mut set).map_err(E::from)?;

    let previous = get(address).map_err(E::from)?;
    if previous {
        return body();
    }

    write_record(&record, address, pid, previous).map_err(E::from)?;
    if let Err(error) = set(address, Some(true)) {
        match restore_with_retry(address, previous, &mut get, &mut set) {
            Ok(()) => remove_record(&record).map_err(E::from)?,
            Err(rollback) => {
                return Err(E::from(format!(
                    "setting stay_focused failed: {error}; rollback also failed: {rollback}; recovery state remains at {}",
                    record.display()
                )));
            }
        }
        return Err(E::from(format!("setting stay_focused failed: {error}")));
    }

    let result = body();
    restore_with_retry(address, previous, &mut get, &mut set).map_err(|error| {
        E::from(format!(
            "could not restore stay_focused: {error}; recovery state remains at {}",
            record.display()
        ))
    })?;
    remove_record(&record).map_err(E::from)?;
    result
}

/// Restore records left by a crashed daemon. Missing windows can be discarded;
/// failures are returned for an actionable ERROR log and remain on disk.
pub fn clear_leftovers<A, G, S>(
    directory: &Path,
    mut identity: A,
    mut get: G,
    mut set: S,
) -> Vec<String>
where
    A: FnMut(&str) -> Result<Option<u32>, String>,
    G: FnMut(&str) -> Result<bool, String>,
    S: FnMut(&str, Option<bool>) -> Result<(), String>,
{
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(error) => {
            return vec![format!(
                "could not scan scroll focus recovery directory {}: {error}",
                directory.display()
            )];
        }
    };
    let mut errors = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                errors.push(format!(
                    "could not read scroll focus recovery entry: {error}"
                ));
                continue;
            }
        };
        let path = entry.path();
        match path.extension().and_then(|extension| extension.to_str()) {
            Some("pending") => {
                if let Err(error) = fs::remove_file(&path) {
                    errors.push(format!(
                        "could not remove incomplete focus record {}: {error}",
                        path.display()
                    ));
                }
                continue;
            }
            Some("state") => {}
            _ => continue,
        }
        let result = (|| {
            let (path_address, path_pid) = match parse_record_filename(&path) {
                Some(identity) => identity,
                None => {
                    let record = read_record(&path)?;
                    (record.address, record.pid)
                }
            };
            let mut record = match read_record(&path) {
                Ok(record) if record.address == path_address => record,
                Ok(_) | Err(_) => {
                    eprintln!(
                        "ERROR damaged scroll focus record {}; restoring {path_address} to false",
                        path.display()
                    );
                    FocusRecord {
                        address: path_address.clone(),
                        pid: path_pid,
                        previous: false,
                    }
                }
            };
            if record.pid.is_none() {
                record.pid = path_pid;
            }
            if record.pid.is_none() {
                return discard_unidentified_record(&path, &record.address);
            }
            let current_pid = identity(&record.address)?;
            let Some(current_pid) = current_pid else {
                remove_record(&path)?;
                return Ok(());
            };
            if record
                .pid
                .is_some_and(|record_pid| record_pid != current_pid)
            {
                // Hyprland reused an address. Do not apply an old window's
                // dynamic rule to the new process.
                remove_record(&path)?;
                return Ok(());
            }
            restore_with_retry(&record.address, record.previous, &mut get, &mut set)?;
            remove_record(&path)
        })();
        if let Err(error) = result {
            errors.push(format!("{}: {error}", path.display()));
        }
    }
    errors
}

fn parse_record_filename(path: &Path) -> Option<(String, Option<u32>)> {
    let stem = path.file_stem()?.to_str()?;
    if let Some((address, pid)) = stem.rsplit_once('-')
        && valid_address(address)
    {
        return Some((address.to_string(), Some(pid.parse().ok()?)));
    }
    valid_address(stem).then(|| (stem.to_string(), None))
}

fn data_base(
    xdg_data_home: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
) -> Result<PathBuf, String> {
    xdg_data_home
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            home.filter(|path| !path.is_empty())
                .map(|home| PathBuf::from(home).join(".local/share"))
        })
        .ok_or_else(|| "XDG_DATA_HOME and HOME are not set".to_string())
}

/// Where the window-addressed recovery records live. Never fall back to the
/// working directory: the state must survive daemon restarts predictably.
pub fn record_directory() -> Result<PathBuf, String> {
    let base = data_base(std::env::var_os("XDG_DATA_HOME"), std::env::var_os("HOME"))?;
    let directory = base.join("thodu/scroll-pointer-focus-records");
    fs::create_dir_all(&directory)
        .map_err(|error| format!("could not create {}: {error}", directory.display()))?;
    if let Err(error) = migrate_legacy_record(&base, &directory) {
        eprintln!(
            "ERROR could not migrate old scroll focus state: {error}; Thodu remains available, but inspect the state before using scroll mode"
        );
    }
    Ok(directory)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::ops::Deref;
    use std::sync::atomic::{AtomicU64, Ordering};

    const TEST_PID: u32 = 4242;

    struct Scratch(PathBuf);

    impl Deref for Scratch {
        type Target = Path;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn scratch(name: &str) -> Scratch {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = PathBuf::from(std::env::var_os("HOME").unwrap())
            .join(".local/share/thodu/tests/focus")
            .join(format!(
                "{name}-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(&directory).unwrap();
        Scratch(directory)
    }

    fn state(directory: &Path, address: &str) -> PathBuf {
        record_path(directory, address, TEST_PID).unwrap()
    }

    // REQ-ACT-04, REQ-ACT-06: the wheel runs under forced focus and clearing
    // the temporary override reveals the original false window-rule value.
    #[test]
    fn req_act_04_req_act_06_wheel_runs_inside_forced_pointer_focus() {
        let directory = scratch("order");
        let focused = RefCell::new(false);
        let log = RefCell::new(Vec::new());
        let result: Result<(), String> = with_forced_pointer_focus(
            &directory,
            "0xabc",
            TEST_PID,
            |_| Ok(*focused.borrow()),
            |address, value| {
                log.borrow_mut().push(format!("set {address} {value:?}"));
                *focused.borrow_mut() = value.unwrap_or(false);
                Ok(())
            },
            || {
                assert_eq!(
                    fs::read_to_string(state(&directory, "0xabc")).unwrap(),
                    "v2\n0xabc\n4242\nfalse\n"
                );
                assert!(*focused.borrow());
                log.borrow_mut().push("wheel".into());
                Ok(())
            },
        );
        assert!(result.is_ok());
        assert_eq!(
            *log.borrow(),
            vec!["set 0xabc Some(true)", "wheel", "set 0xabc None"]
        );
        assert!(!*focused.borrow());
        assert!(!state(&directory, "0xabc").exists());
    }

    // REQ-ACT-04: an already true window rule is left unchanged.
    #[test]
    fn req_act_04_existing_stay_focused_rule_is_preserved() {
        let directory = scratch("preexisting");
        let mut changed = false;
        let mut wheel = false;
        let result: Result<(), String> = with_forced_pointer_focus(
            &directory,
            "0xabc",
            TEST_PID,
            |_| Ok(true),
            |_, _| {
                changed = true;
                Ok(())
            },
            || {
                wheel = true;
                Ok(())
            },
        );
        assert!(result.is_ok());
        assert!(wheel);
        assert!(!changed);
        assert!(!state(&directory, "0xabc").exists());
    }

    // REQ-ACT-06: even a failed wheel restores the temporary override.
    #[test]
    fn req_act_06_failed_wheel_still_clears_forced_focus() {
        let directory = scratch("failed");
        let focused = RefCell::new(false);
        let result: Result<(), String> = with_forced_pointer_focus(
            &directory,
            "0xabc",
            TEST_PID,
            |_| Ok(*focused.borrow()),
            |_, value| {
                *focused.borrow_mut() = value.unwrap_or(false);
                Ok(())
            },
            || {
                assert!(*focused.borrow(), "the wheel runs under forced focus");
                Err("wheel failed".to_string())
            },
        );
        assert_eq!(result, Err("wheel failed".to_string()));
        assert!(!*focused.borrow());
        assert!(!state(&directory, "0xabc").exists());
    }

    // REQ-ACT-06: a rejected set sends no wheel and removes its record after
    // the failed attempt has been rolled back.
    #[test]
    fn req_act_06_refused_force_sends_no_wheel() {
        let directory = scratch("refused");
        let mut wheel = false;
        let result: Result<(), String> = with_forced_pointer_focus(
            &directory,
            "0xabc",
            TEST_PID,
            |_| Ok(false),
            |_, value| match value {
                Some(true) => Err("refused".into()),
                None => Ok(()),
                Some(false) => unreachable!(),
            },
            || {
                wheel = true;
                Ok(())
            },
        );
        assert!(result.is_err());
        assert!(!wheel);
        assert!(!state(&directory, "0xabc").exists());
    }

    #[test]
    fn req_act_06_force_and_rollback_errors_are_both_reported() {
        let directory = scratch("rollback-error");
        let focused = RefCell::new(false);
        let result: Result<(), String> = with_forced_pointer_focus(
            &directory,
            "0xabc",
            TEST_PID,
            |_| Ok(*focused.borrow()),
            |_, value| match value {
                Some(true) => {
                    *focused.borrow_mut() = true;
                    Err("force response lost".into())
                }
                None => Err("rollback timeout".into()),
                Some(false) => unreachable!(),
            },
            || panic!("wheel must not run after the force request fails"),
        );
        let error = result.unwrap_err();
        assert!(error.contains("force response lost"));
        assert!(error.contains("rollback timeout"));
        assert!(error.contains("recovery state remains at"));
        assert!(*focused.borrow());
        assert!(state(&directory, "0xabc").exists());
    }

    // REQ-ACT-06: a daemon restart clears a crashed temporary property.
    #[test]
    fn req_act_06_leftover_focus_restores_recorded_value() {
        let directory = scratch("leftover");
        let record = state(&directory, "0xdef");
        fs::write(&record, "v2\n0xdef\n4242\nfalse\n").unwrap();
        let calls = RefCell::new(Vec::new());
        let errors = clear_leftovers(
            &directory,
            |_| Ok(Some(TEST_PID)),
            |_| Ok(false),
            |address, value| {
                calls.borrow_mut().push(format!("{address} {value:?}"));
                Ok(())
            },
        );
        assert!(errors.is_empty());
        assert_eq!(*calls.borrow(), vec!["0xdef None"]);
        assert!(!record.exists());

        let record_true = state(&directory, "0x123");
        fs::write(&record_true, "v2\n0x123\n4242\ntrue\n").unwrap();
        let errors = clear_leftovers(
            &directory,
            |_| Ok(Some(TEST_PID)),
            |_| Ok(false),
            |address, value| {
                calls.borrow_mut().push(format!("{address} {value:?}"));
                Ok(())
            },
        );
        assert!(errors.is_empty());
        assert_eq!(*calls.borrow(), vec!["0xdef None", "0x123 Some(true)"]);
        assert!(!record_true.exists());
    }

    // REQ-ACT-06: a failed restore for one window cannot overwrite another
    // window's recovery record; the same address retries before another wheel.
    #[test]
    fn req_act_06_focus_records_are_per_window_and_retried() {
        let directory = scratch("per-window");
        let a = RefCell::new(false);
        let result: Result<(), String> = with_forced_pointer_focus(
            &directory,
            "0xaaa",
            TEST_PID,
            |_| Ok(*a.borrow()),
            |_, value| {
                if value.is_none() {
                    return Err("restore failed".into());
                }
                *a.borrow_mut() = value.unwrap_or(false);
                Ok(())
            },
            || Ok(()),
        );
        assert!(result.is_err());
        assert!(state(&directory, "0xaaa").exists());

        let b = RefCell::new(false);
        let result: Result<(), String> = with_forced_pointer_focus(
            &directory,
            "0xbbb",
            TEST_PID,
            |_| Ok(*b.borrow()),
            |_, value| {
                *b.borrow_mut() = value.unwrap_or(false);
                Ok(())
            },
            || Ok(()),
        );
        assert!(result.is_ok());
        assert!(!state(&directory, "0xbbb").exists());
        assert!(state(&directory, "0xaaa").exists());

        let retried = RefCell::new(true);
        let mut calls = 0;
        let result: Result<(), String> = with_forced_pointer_focus(
            &directory,
            "0xaaa",
            TEST_PID,
            |_| Ok(*retried.borrow()),
            |_, value| {
                calls += 1;
                *retried.borrow_mut() = value.unwrap_or(false);
                Ok(())
            },
            || Ok(()),
        );
        assert!(result.is_ok());
        assert_eq!(calls, 3, "recover, force, restore");
        assert!(!state(&directory, "0xaaa").exists());
    }

    // REQ-ACT-04: detect syntax from the active Hyprland config provider once.
    #[test]
    fn req_act_04_stay_focused_dispatch_detects_syntax() {
        assert_eq!(
            detect_dispatch_syntax(|command| {
                assert_eq!(command, "systeminfo");
                Ok("configProvider: lua\nbackend: wayland\n".into())
            })
            .unwrap(),
            DispatchSyntax::Lua
        );
        assert_eq!(
            detect_dispatch_syntax(|_| Ok("configProvider: hyprlang\n".into())).unwrap(),
            DispatchSyntax::Hyprlang
        );
        assert!(detect_dispatch_syntax(|_| Ok("backend: wayland".into())).is_err());
    }

    // REQ-ACT-04: use only the detected dispatcher, including `unset`.
    #[test]
    fn req_act_04_setprop_uses_detected_syntax_and_unset() {
        let mut commands = Vec::new();
        assert!(
            set_stay_focused("0x5", Some(true), DispatchSyntax::Lua, |command| {
                commands.push(command.to_string());
                Ok("ok".into())
            })
            .is_ok()
        );
        assert!(
            set_stay_focused("0x5", None, DispatchSyntax::Hyprlang, |command| {
                commands.push(command.to_string());
                Ok("ok".into())
            })
            .is_ok()
        );
        assert_eq!(
            commands,
            vec![
                "dispatch hl.dsp.window.set_prop({ window = 'address:0x5', prop = 'stay_focused', value = '1' })",
                "dispatch setprop address:0x5 stay_focused unset"
            ]
        );
        assert!(
            set_stay_focused("0x5", Some(false), DispatchSyntax::Lua, |_| {
                Ok("error".into())
            })
            .is_err()
        );
    }

    // REQ-ACT-04: parse Hyprland's effective property response exactly.
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

    #[test]
    fn recovery_data_path_has_no_working_directory_fallback() {
        assert!(data_base(None, None).is_err());
        assert_eq!(
            data_base(None, Some("/home/gg".into())).unwrap(),
            PathBuf::from("/home/gg/.local/share")
        );
    }

    #[test]
    fn previous_single_record_is_migrated_to_its_window_file() {
        let base = scratch("migration");
        let thodu = base.join("thodu");
        fs::create_dir_all(&thodu).unwrap();
        let legacy = thodu.join("scroll-pointer-focus");
        fs::write(&legacy, "0xabc\n").unwrap();
        let records = thodu.join("scroll-pointer-focus-records");
        fs::create_dir_all(&records).unwrap();

        migrate_legacy_record(&base, &records).unwrap();

        assert!(!legacy.exists());
        assert_eq!(
            fs::read_to_string(legacy_record_path(&records, "0xabc").unwrap()).unwrap(),
            "0xabc\n"
        );
    }

    #[test]
    fn req_act_06_legacy_record_without_pid_is_never_applied_to_current_window() {
        let directory = scratch("legacy-no-pid");
        let record = legacy_record_path(&directory, "0xabc").unwrap();
        fs::write(&record, "0xabc\n").unwrap();
        let errors = clear_leftovers(
            &directory,
            |_| Ok(Some(TEST_PID)),
            |_| panic!("legacy record has no safe process identity"),
            |_, _| panic!("legacy record must not change the current window"),
        );
        assert!(errors.is_empty());
        assert!(!record.exists());
    }

    #[test]
    fn req_act_06_legacy_record_without_pid_does_not_block_scroll() {
        let directory = scratch("legacy-does-not-block");
        let record = legacy_record_path(&directory, "0xabc").unwrap();
        fs::write(&record, "0xabc\n").unwrap();
        let focused = RefCell::new(false);
        let mut changes = Vec::new();

        let result = with_forced_pointer_focus(
            &directory,
            "0xabc",
            TEST_PID,
            |_| Ok(*focused.borrow()),
            |address, value| {
                assert_eq!(address, "0xabc");
                changes.push(value);
                *focused.borrow_mut() = value.unwrap_or(false);
                Ok(())
            },
            || Ok::<_, String>("scrolled"),
        );

        assert_eq!(result.unwrap(), "scrolled");
        assert_eq!(changes, vec![Some(true), None]);
        assert!(!record.exists());
    }

    #[test]
    fn req_act_06_corrupt_record_is_recovered_from_its_filename() {
        let directory = scratch("corrupt");
        let record = state(&directory, "0xabc");
        fs::write(&record, "").unwrap();
        let focused = RefCell::new(true);
        let mut calls = Vec::new();
        let errors = clear_leftovers(
            &directory,
            |_| Ok(Some(TEST_PID)),
            |_| Ok(*focused.borrow()),
            |address, value| {
                calls.push(format!("{address} {value:?}"));
                *focused.borrow_mut() = value.unwrap_or(false);
                Ok(())
            },
        );
        assert!(errors.is_empty());
        assert_eq!(calls, vec!["0xabc None"]);
        assert!(!*focused.borrow());
        assert!(!record.exists());
    }

    #[test]
    fn req_act_06_stale_pid_record_is_discarded_without_changing_window() {
        let directory = scratch("reused-address");
        let old_record = record_path(&directory, "0xabc", 111).unwrap();
        fs::write(&old_record, "v2\n0xabc\n111\nfalse\n").unwrap();
        let mut changed = false;
        let errors = clear_leftovers(
            &directory,
            |_| Ok(Some(TEST_PID)),
            |_| Ok(true),
            |_, _| {
                changed = true;
                Ok(())
            },
        );
        assert!(errors.is_empty());
        assert!(!changed);
        assert!(!old_record.exists());
    }

    #[test]
    fn req_act_06_pending_record_is_removed_at_startup() {
        let directory = scratch("pending");
        let pending = directory.join("0xabc-4242.pending");
        fs::write(&pending, "partial").unwrap();
        let errors = clear_leftovers(
            &directory,
            |_| Ok(Some(TEST_PID)),
            |_| Ok(false),
            |_, _| Ok(()),
        );
        assert!(errors.is_empty());
        assert!(!pending.exists());
    }

    #[test]
    fn req_act_06_missing_window_record_is_removed_without_dispatch() {
        let directory = scratch("closed-window");
        let record = state(&directory, "0xabc");
        fs::write(&record, "v2\n0xabc\n4242\nfalse\n").unwrap();
        let errors = clear_leftovers(
            &directory,
            |_| Ok(None),
            |_| panic!("closed window has no property"),
            |_, _| panic!("closed window must not be changed"),
        );
        assert!(errors.is_empty());
        assert!(!record.exists());
    }

    #[test]
    fn req_act_06_failed_startup_restore_keeps_record() {
        let directory = scratch("failed-startup-restore");
        let record = state(&directory, "0xabc");
        fs::write(&record, "v2\n0xabc\n4242\nfalse\n").unwrap();
        let errors = clear_leftovers(
            &directory,
            |_| Ok(Some(TEST_PID)),
            |_| Ok(true),
            |_, _| Err("Hyprland is not responding".into()),
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("restore failed after 3 attempts"));
        assert!(record.exists());
    }

    #[test]
    fn req_act_06_restore_unset_preserves_false_over_a_true_rule() {
        let focused = RefCell::new(false);
        let calls = RefCell::new(Vec::new());
        let result = restore(
            "0xabc",
            false,
            &mut |_| Ok(*focused.borrow()),
            &mut |_, value| {
                calls.borrow_mut().push(value);
                *focused.borrow_mut() = value.unwrap_or(true);
                Ok(())
            },
        );
        assert!(result.is_ok());
        assert_eq!(*calls.borrow(), vec![None, Some(false)]);
        assert!(!*focused.borrow());
    }
}
