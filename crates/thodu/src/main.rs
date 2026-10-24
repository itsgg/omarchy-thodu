//! thodu, the command line: Hyprland keybindings start sessions with it, and
//! agents read targets with it (SPEC section 4, item 3).
//!
//!   thodu hints [--scope visible|focused] [--window ADDRESS] [--test] [--no-overlay] start a session
//!   thodu grid [--window ADDRESS] [--test] [--no-overlay]    start on the grid
//!   thodu targets [--scope visible|focused] [--json] [--window ADDRESS] print targets without labels
//!   thodu key SESSION KEY [--shift] [--ctrl]  send a key, for live checks
//!   thodu close SESSION
//!   thodu bench [--sessions N] [--window ADDRESS]   time to labels drawn
//!   thodu record-tree --out FILE [--window ADDRESS]  record the tree
//!   thodu install [--hints-key KEY --grid-key KEY] default: SUPER + SHIFT + F12, SUPER + CTRL + F12
//!   thodu doctor                              check environment, rule and service
//!
//! ADDRESS is a Hyprland window address, or class:NAME for the first window
//! of that class.
//!
//! It starts thodud when nothing answers on the socket. Errors print an
//! ERROR line and exit 1; bad arguments exit 2.

use std::io::{BufRead, BufReader, IsTerminal, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::json;
use thodu_core::perf;
use thodu_core::protocol::{Reply, SOCKET_NAME, VERSION, hyprland_socket_path};

fn socket() -> Result<PathBuf, String> {
    std::env::var("XDG_RUNTIME_DIR")
        .map(|d| PathBuf::from(d).join(SOCKET_NAME))
        .map_err(|_| "XDG_RUNTIME_DIR is not set".to_string())
}

fn home_dir() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "HOME is not set".to_string())
}

/// REQ-CLI-02: verify that Hyprland's request socket can be reached.
fn check_hyprland_socket(
    runtime: Option<&str>,
    signature: Option<&str>,
) -> Result<PathBuf, String> {
    let runtime = runtime.ok_or_else(|| "XDG_RUNTIME_DIR is not set".to_string())?;
    let signature = signature.ok_or_else(|| {
        "HYPRLAND_INSTANCE_SIGNATURE is not set; is Hyprland running?".to_string()
    })?;
    let path = hyprland_socket_path(Path::new(runtime), signature);
    UnixStream::connect(&path).map_err(|e| {
        format!(
            "cannot connect to Hyprland on {}: {e}; start or return to Hyprland and retry",
            path.display()
        )
    })?;
    Ok(path)
}

fn check_hyprland() -> Result<(), String> {
    check_hyprland_socket(
        std::env::var("XDG_RUNTIME_DIR").ok().as_deref(),
        std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok().as_deref(),
    )
    .map(|_| ())
}

fn daemon_binary() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("thodud")))
        .filter(|p| p.exists())
        .unwrap_or_else(|| PathBuf::from("thodud"))
}

/// A connection to thodud, starting it if it is not running.
fn connect() -> Result<UnixStream, String> {
    let path = socket()?;
    if let Ok(stream) = UnixStream::connect(&path) {
        return Ok(stream);
    }
    Command::new(daemon_binary())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("cannot start thodud: {e}"))?;
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if let Ok(stream) = UnixStream::connect(&path) {
            return Ok(stream);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Err(format!(
        "thodud did not start listening on {}",
        path.display()
    ))
}

fn ask(request: serde_json::Value) -> Result<Reply, String> {
    let mut stream = connect()?;
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .map_err(|e| e.to_string())?;
    let mut line = request.to_string();
    line.push('\n');
    stream
        .write_all(line.as_bytes())
        .map_err(|e| e.to_string())?;
    let mut reply = String::new();
    BufReader::new(stream)
        .read_line(&mut reply)
        .map_err(|e| format!("no reply from thodud: {e}"))?;
    let reply: Reply = serde_json::from_str(&reply).map_err(|e| format!("thodud's reply: {e}"))?;
    if reply.ok {
        Ok(reply)
    } else {
        Err(reply
            .message
            .unwrap_or_else(|| "thodud refused the request".into()))
    }
}

fn now_us() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or_default()
}

/// Where bench keeps each run's samples (AGENTS.md rule 7).
fn bench_dir() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("thodu").join("bench")
}

/// Keep a run's samples under `dir`: the file, or why it was not written.
fn keep_samples(dir: &Path, log: &str) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let file = dir.join(format!("{}.jsonl", now_us() / 1_000_000));
    std::fs::write(&file, log).map_err(|e| format!("{}: {e}", file.display()))?;
    Ok(file)
}

/// One session timed from `thodu hints` starting to the overlay saying it
/// drew the labels (REQ-PERF-01); the overlay is summoned without the
/// keyboard. Returns the time and the collection's share, in ms.
fn bench_once(window: Option<&str>) -> Result<(f64, f64), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut cmd = Command::new(exe);
    cmd.args(["hints", "--test"]);
    if let Some(w) = window {
        cmd.args(["--window", w]);
    }
    let began = now_us();
    let out = cmd
        .output()
        .map_err(|e| format!("cannot run thodu hints: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    let said = String::from_utf8_lossy(&out.stdout);
    let session = said
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_string();
    let deadline = Instant::now() + Duration::from_secs(2);
    let result = loop {
        let reply = ask(json!({"v": VERSION, "op": "session", "session": session, "brief": true}))?;
        let timing = reply.timing.unwrap_or_default();
        if let Some(drawn) = timing.drawn_at_us {
            break Ok((
                drawn.saturating_sub(began) as f64 / 1000.0,
                timing.collect_ms as f64,
            ));
        }
        if Instant::now() > deadline {
            break Err(format!("session {session} was not drawn within 2 s"));
        }
        std::thread::sleep(Duration::from_millis(1));
    };
    let _ = ask(json!({"v": VERSION, "op": "close", "session": session}));
    result
}

fn bench(args: &[String]) -> Result<(), String> {
    let sessions: usize = match value(args, "--sessions") {
        Some(n) => n.parse().map_err(|_| String::new())?,
        None => 50,
    };
    if sessions == 0 {
        return Err(String::new());
    }
    let window = value(args, "--window");
    let mut samples = Vec::with_capacity(sessions);
    let mut collects = Vec::with_capacity(sessions);
    let mut log = String::new();
    for i in 0..sessions {
        let (ms, collect) = bench_once(window.as_deref())?;
        samples.push(ms);
        collects.push(collect);
        log.push_str(&format!(
            "{}\n",
            json!({"session": i, "ms": ms, "collect_ms": collect})
        ));
        // The overlay hides between sessions, so each one maps afresh.
        std::thread::sleep(Duration::from_millis(300));
    }
    let kept = match keep_samples(&bench_dir(), &log) {
        Ok(file) => format!("samples in {}", file.display()),
        Err(e) => format!("samples not kept: {e}"),
    };
    let summary = perf::summarize(&samples).ok_or("no samples")?;
    let collect = perf::percentile(&collects, 50.0).unwrap_or_default();
    println!(
        "{} sessions: median {:.1} ms, p95 {:.1} ms (REQ-PERF-01: {}, {}); collection median {:.0} ms; {}",
        summary.sessions,
        summary.median_ms,
        summary.p95_ms,
        perf::MEDIAN_MS,
        perf::P95_MS,
        collect,
        kept
    );
    if summary.within() {
        Ok(())
    } else {
        Err("over REQ-PERF-01's thresholds".into())
    }
}

fn flag(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

fn value(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

#[derive(Debug, PartialEq, Eq)]
struct DoctorItem {
    name: &'static str,
    present: bool,
}

const UDEV_RULE_FILE: &str = "72-thodu-uinput.rules";
const UDEV_RULE_CONTENT: &str =
    "KERNEL==\"uinput\", SUBSYSTEM==\"misc\", TAG+=\"uaccess\", OPTIONS+=\"static_node=uinput\"\n";

fn output_from(
    program_env: &str,
    program: &str,
    args: &[&str],
) -> Result<std::process::Output, String> {
    let program = std::env::var_os(program_env).unwrap_or_else(|| program.into());
    Command::new(program)
        .args(args)
        .output()
        .map_err(|e| format!("cannot run {}: {e}", args.join(" ")))
}

fn systemctl_output(args: &[&str]) -> Result<std::process::Output, String> {
    output_from("THODU_SYSTEMCTL", "systemctl", args)
}

fn shutdown_existing_daemon() -> Result<(), String> {
    let Ok(path) = socket() else {
        return Ok(());
    };
    let Ok(mut stream) = UnixStream::connect(&path) else {
        return Ok(());
    };
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| format!("cannot talk to the current thodud: {e}"))?;
    let request = json!({"v": VERSION, "op": "shutdown"});
    stream
        .write_all(format!("{request}\n").as_bytes())
        .map_err(|e| format!("cannot ask the current thodud to stop: {e}"))?;
    let mut line = String::new();
    BufReader::new(stream)
        .read_line(&mut line)
        .map_err(|e| format!("the current thodud did not answer its shutdown request: {e}"))?;
    let reply: Reply = serde_json::from_str(&line)
        .map_err(|e| format!("the current thodud does not support service handoff: {e}"))?;
    if !reply.ok {
        return Err(
            "the current thodud refused service handoff; stop it and rerun thodu install".into(),
        );
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if UnixStream::connect(&path).is_err() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Err("the current thodud did not release its socket; stop it and rerun thodu install".into())
}

fn wait_for_service_active() -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut active_since = None;
    while Instant::now() < deadline {
        let active =
            systemctl_output(&["--user", "is-active", "thodud.service"]).is_ok_and(|output| {
                output.status.success()
                    && String::from_utf8_lossy(&output.stdout).trim() == "active"
            });
        if active {
            let since = active_since.get_or_insert_with(Instant::now);
            if since.elapsed() >= Duration::from_millis(100) {
                return Ok(());
            }
        } else {
            active_since = None;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    Err(
        "thodud did not stay active after start; stop the existing daemon and rerun thodu install"
            .into(),
    )
}

fn command_succeeded(program_env: &str, program: &str, args: &[&str]) -> Result<(), String> {
    let output = output_from(program_env, program, args)?;
    if output.status.success() {
        Ok(())
    } else {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(format!(
            "{} failed with {}{}",
            args.join(" "),
            output.status,
            if detail.is_empty() {
                String::new()
            } else {
                format!(": {detail}")
            }
        ))
    }
}

fn uinput_path() -> PathBuf {
    std::env::var_os("THODU_UINPUT_DEVICE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/dev/uinput"))
}

fn uinput_accessible() -> bool {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(uinput_path())
        .is_ok()
}

fn wait_for_uinput_accessible() -> bool {
    let deadline = Instant::now() + Duration::from_millis(750);
    while Instant::now() < deadline {
        if uinput_accessible() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    uinput_accessible()
}

fn active_config_line(content: &str, needle: &str) -> bool {
    content.lines().any(|line| {
        let line = line.trim_start();
        !line.starts_with("--") && line.contains(needle)
    })
}

fn check_doctor(home: &Path) -> (Vec<DoctorItem>, bool) {
    let mut items = Vec::new();
    let bindings_file = home.join(".config/hypr/bindings.lua");
    let bindings = std::fs::read_to_string(&bindings_file).unwrap_or_default();
    let active_line = |needle: &str| active_config_line(&bindings, needle);
    let a11y = active_line("hl.env(\"ACCESSIBILITY_ENABLED\", \"1\")");
    items.push(DoctorItem {
        name: "ACCESSIBILITY_ENABLED=1 in Hyprland",
        present: a11y,
    });

    let qt_a11y = active_line("hl.env(\"QT_LINUX_ACCESSIBILITY_ALWAYS_ON\", \"1\")");
    items.push(DoctorItem {
        name: "QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1 in Hyprland",
        present: qt_a11y,
    });

    let udev_rule = std::env::var_os("THODU_UDEV_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/etc/udev/rules.d"))
        .join(UDEV_RULE_FILE);
    let udev = std::fs::read_to_string(&udev_rule)
        .is_ok_and(|content| content == UDEV_RULE_CONTENT)
        && uinput_accessible();
    items.push(DoctorItem {
        name: "uinput rule and access",
        present: udev,
    });

    let plugin_dir = home.join(".config/omarchy/plugins/io.github.itsgg.thodu");
    let plugin_files = PLUGIN_ASSETS
        .iter()
        .all(|(path, _)| plugin_dir.join(path).is_file());
    let plugin = plugin_files && shell_enables_plugin(home, PLUGIN_ID);
    items.push(DoctorItem {
        name: "overlay plugin",
        present: plugin,
    });

    let service_file = std::fs::read_to_string(home.join(".config/systemd/user/thodud.service"))
        .is_ok_and(|content| {
            content.contains("PartOf=graphical-session.target")
                && content.contains("After=graphical-session.target")
                && content.contains("WantedBy=graphical-session.target")
        });
    let service_link =
        home.join(".config/systemd/user/graphical-session.target.wants/thodud.service");
    let service_enabled = std::fs::symlink_metadata(&service_link).is_ok_and(|metadata| {
        metadata.file_type().is_symlink()
            && std::fs::read_link(&service_link).ok().as_deref()
                == Some(Path::new("../thodud.service"))
    });
    let service_running = systemctl_output(&["--user", "is-active", "thodud.service"])
        .is_ok_and(|output| output.status.success());
    items.push(DoctorItem {
        name: "thodud user service",
        present: service_file && service_enabled && service_running,
    });

    let binding_is_configured = |name: &str| {
        active_line(&format!("\"Thodu {name}\"")) && active_line(&format!(" {name}\")"))
    };
    let bindings = binding_is_configured("hints") && binding_is_configured("grid");
    items.push(DoctorItem {
        name: "hyprland keybindings",
        present: bindings,
    });

    let all_present = items.iter().all(|i| i.present);
    (items, all_present)
}

include!(concat!(env!("OUT_DIR"), "/plugin_assets.rs"));

const PLUGIN_ID: &str = "io.github.itsgg.thodu";

fn prune_plugin_dir(directory: &Path, relative: &Path, expected: &[PathBuf]) -> Result<(), String> {
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(format!("{}: {e}", directory.display())),
    };
    for entry in entries {
        let entry = entry.map_err(|e| format!("{}: {e}", directory.display()))?;
        let name = entry.file_name();
        let child_relative = relative.join(name);
        let child = entry.path();
        let metadata =
            std::fs::symlink_metadata(&child).map_err(|e| format!("{}: {e}", child.display()))?;
        if metadata.file_type().is_symlink() {
            std::fs::remove_file(&child).map_err(|e| format!("{}: {e}", child.display()))?;
            continue;
        }
        if metadata.is_dir() {
            if expected
                .iter()
                .any(|path| path.starts_with(&child_relative) && path != &child_relative)
            {
                prune_plugin_dir(&child, &child_relative, expected)?;
            } else {
                std::fs::remove_dir_all(&child).map_err(|e| format!("{}: {e}", child.display()))?;
            }
        } else if !expected.iter().any(|path| path == &child_relative) {
            std::fs::remove_file(&child).map_err(|e| format!("{}: {e}", child.display()))?;
        }
    }
    Ok(())
}

fn install_plugin_files(dest: &Path) -> Result<(), String> {
    let expected: Vec<PathBuf> = PLUGIN_ASSETS
        .iter()
        .map(|(path, _)| PathBuf::from(path))
        .collect();
    prune_plugin_dir(dest, Path::new(""), &expected)?;
    for (path, contents) in PLUGIN_ASSETS {
        let target = dest.join(path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&target, contents).map_err(|e| format!("{}: {e}", target.display()))?;
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct KeyBindings {
    hints: String,
    grid: String,
}

const DEFAULT_HINTS_KEY: &str = "SUPER + SHIFT + F12";
const DEFAULT_GRID_KEY: &str = "SUPER + CTRL + F12";

fn validate_key(key: &str, option: &str) -> Result<String, String> {
    let key = key.trim();
    if key.is_empty()
        || !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || " +_-:".contains(c))
    {
        return Err(format!(
            "{option} must contain only key names, spaces, plus signs, underscores, hyphens or colons"
        ));
    }
    Ok(key.to_string())
}

fn prompt_key(prompt: &str) -> Result<String, String> {
    print!("{prompt}");
    std::io::stdout().flush().map_err(|e| e.to_string())?;
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .map_err(|e| format!("cannot read key choice: {e}"))?;
    validate_key(&line, prompt.trim_end_matches(": "))
}

fn install_key_choices(args: &[String], home: &Path) -> Result<Option<KeyBindings>, String> {
    let hints = value(args, "--hints-key");
    let grid = value(args, "--grid-key");
    match (hints, grid) {
        (Some(hints), Some(grid)) => {
            let choices = KeyBindings {
                hints: validate_key(&hints, "--hints-key")?,
                grid: validate_key(&grid, "--grid-key")?,
            };
            if choices.hints == choices.grid {
                return Err("--hints-key and --grid-key must be different".into());
            }
            Ok(Some(choices))
        }
        (Some(_), None) | (None, Some(_)) => {
            Err("--hints-key and --grid-key must be supplied together".into())
        }
        (None, None) => {
            let path = home.join(".config/hypr/bindings.lua");
            let current = std::fs::read_to_string(path).unwrap_or_default();
            let has_hints = active_config_line(&current, "\"Thodu hints\"");
            let has_grid = active_config_line(&current, "\"Thodu grid\"");
            if has_hints && has_grid {
                return Ok(None);
            }
            if !has_hints && !has_grid {
                return Ok(Some(KeyBindings {
                    hints: DEFAULT_HINTS_KEY.to_string(),
                    grid: DEFAULT_GRID_KEY.to_string(),
                }));
            }
            if !std::io::stdin().is_terminal() {
                return Err(
                    "choose both keybindings with --hints-key and --grid-key, or run thodu install in a terminal".into(),
                );
            }
            println!("Choose the two free Omarchy keybindings for Thodu.");
            let choices = KeyBindings {
                hints: prompt_key("Key for Thodu hints: ")?,
                grid: prompt_key("Key for Thodu grid: ")?,
            };
            if choices.hints == choices.grid {
                return Err("the Thodu hints and grid keys must be different".into());
            }
            Ok(Some(choices))
        }
    }
}

fn lua_string(value: &str) -> String {
    serde_json::to_string(value).expect("strings serialize as JSON")
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn install_hyprland_config(home: &Path, choices: Option<&KeyBindings>) -> Result<(), String> {
    let hypr_dir = home.join(".config/hypr");
    std::fs::create_dir_all(&hypr_dir).map_err(|e| format!("{}: {e}", hypr_dir.display()))?;
    let bindings_file = hypr_dir.join("bindings.lua");
    let current = std::fs::read_to_string(&bindings_file).unwrap_or_default();
    let executable = std::env::current_exe()
        .map_err(|e| format!("cannot find thodu executable: {e}"))?
        .to_string_lossy()
        .into_owned();
    let binding_line = |name: &str, key: &str| {
        let command = lua_string(&format!("{} {name}", shell_quote(&executable)));
        format!(
            "o.bind({}, {}, {})\n",
            lua_string(key),
            lua_string(&format!("Thodu {name}")),
            command
        )
    };
    let mut updated = String::new();
    let (mut has_hints, mut has_grid) = (false, false);
    for line in current.lines() {
        let active_binding = !line.trim_start().starts_with("--") && line.contains("o.bind(");
        if let Some(choices) = choices.filter(|_| active_binding) {
            if line.contains("\"Thodu hints\"") {
                if !has_hints {
                    updated.push_str(&binding_line("hints", &choices.hints));
                    has_hints = true;
                }
                continue;
            }
            if line.contains("\"Thodu grid\"") {
                if !has_grid {
                    updated.push_str(&binding_line("grid", &choices.grid));
                    has_grid = true;
                }
                continue;
            }
        }
        updated.push_str(line);
        updated.push('\n');
    }
    if let Some(choices) = choices {
        if !has_hints {
            updated.push_str(&binding_line("hints", &choices.hints));
        }
        if !has_grid {
            updated.push_str(&binding_line("grid", &choices.grid));
        }
    }
    for line in [
        "hl.env(\"ACCESSIBILITY_ENABLED\", \"1\")",
        "hl.env(\"QT_LINUX_ACCESSIBILITY_ALWAYS_ON\", \"1\")",
    ] {
        if !active_config_line(&updated, line) {
            updated.push_str(line);
            updated.push('\n');
        }
    }
    if updated != current {
        std::fs::write(&bindings_file, updated)
            .map_err(|e| format!("{}: {e}", bindings_file.display()))?;
    }
    command_succeeded("THODU_HYPRCTL", "hyprctl", &["reload"])
        .map_err(|e| format!("could not reload Hyprland config: {e}"))
}

fn enable_user_service(home: &Path, unit: &str) -> Result<(), String> {
    let service_dir = home.join(".config/systemd/user");
    std::fs::create_dir_all(&service_dir).map_err(|e| format!("{}: {e}", service_dir.display()))?;
    let service_file = service_dir.join("thodud.service");
    std::fs::write(&service_file, unit).map_err(|e| format!("{}: {e}", service_file.display()))?;

    let wants_dir = service_dir.join("graphical-session.target.wants");
    std::fs::create_dir_all(&wants_dir).map_err(|e| format!("{}: {e}", wants_dir.display()))?;
    let symlink_path = wants_dir.join("thodud.service");
    let link_target = Path::new("../thodud.service");
    match std::fs::symlink_metadata(&symlink_path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            if std::fs::read_link(&symlink_path).ok().as_deref() != Some(link_target) {
                std::fs::remove_file(&symlink_path)
                    .map_err(|e| format!("{}: {e}", symlink_path.display()))?;
                std::os::unix::fs::symlink(link_target, &symlink_path)
                    .map_err(|e| format!("{}: {e}", symlink_path.display()))?;
            }
        }
        Ok(_) => {
            return Err(format!(
                "{} exists and is not a symlink",
                symlink_path.display()
            ));
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            std::os::unix::fs::symlink(link_target, &symlink_path)
                .map_err(|e| format!("{}: {e}", symlink_path.display()))?;
        }
        Err(e) => return Err(format!("{}: {e}", symlink_path.display())),
    }

    command_succeeded("THODU_SYSTEMCTL", "systemctl", &["--user", "daemon-reload"])?;
    command_succeeded(
        "THODU_SYSTEMCTL",
        "systemctl",
        &["--user", "enable", "thodud.service"],
    )?;
    let active = systemctl_output(&["--user", "is-active", "graphical-session.target"])
        .is_ok_and(|output| output.status.success());
    if active {
        command_succeeded(
            "THODU_SYSTEMCTL",
            "systemctl",
            &["--user", "stop", "thodud.service"],
        )
        .map_err(|e| format!("cannot stop the old thodud service: {e}"))?;
        shutdown_existing_daemon()?;
        command_succeeded(
            "THODU_SYSTEMCTL",
            "systemctl",
            &["--user", "start", "thodud.service"],
        )
        .map_err(|e| format!("cannot start thodud in the graphical session: {e}"))?;
        wait_for_service_active()?;
    }
    Ok(())
}

fn do_install(home: &Path, choices: Option<&KeyBindings>) -> Result<(), String> {
    install_udev_rule(home)?;

    let plugin_dest = home.join(".config/omarchy/plugins/io.github.itsgg.thodu");
    std::fs::create_dir_all(&plugin_dest).map_err(|e| format!("{}: {e}", plugin_dest.display()))?;
    install_plugin_files(&plugin_dest)?;
    enable_shell_plugin(home)?;

    install_hyprland_config(home, choices)?;

    let service_unit = format!(
        "[Unit]\nDescription=Thodu hints daemon\nPartOf=graphical-session.target\nAfter=graphical-session.target\n\n[Service]\nExecStart={}\nEnvironment=XDG_RUNTIME_DIR=%t\nRestart=on-failure\n\n[Install]\nWantedBy=graphical-session.target\n",
        daemon_binary().display()
    );
    enable_user_service(home, &service_unit)?;

    Ok(())
}

fn shell_enables_plugin(home: &Path, plugin_id: &str) -> bool {
    let path = home.join(".config/omarchy/shell.json");
    let Ok(content) = std::fs::read_to_string(path) else {
        return false;
    };
    let Ok(config) = serde_json::from_str::<serde_json::Value>(&content) else {
        return false;
    };
    let id_matches = |entry: &serde_json::Value| {
        entry.get("id").and_then(serde_json::Value::as_str) == Some(plugin_id)
    };
    let in_bar = config
        .get("bar")
        .and_then(|bar| bar.get("layout"))
        .and_then(serde_json::Value::as_object)
        .is_some_and(|layout| {
            layout.values().any(|entries| {
                entries
                    .as_array()
                    .is_some_and(|entries| entries.iter().any(id_matches))
            })
        });
    let in_plugins = config
        .get("plugins")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|entries| entries.iter().any(id_matches));
    in_bar || in_plugins
}

fn enable_shell_plugin(home: &Path) -> Result<(), String> {
    let rescan = Command::new("omarchy-shell")
        .args(["shell", "rescanPlugins"])
        .output()
        .map_err(|e| format!("cannot rescan Omarchy plugins: {e}"))?;
    if !rescan.status.success() {
        let detail = String::from_utf8_lossy(&rescan.stderr).trim().to_string();
        return Err(format!(
            "Omarchy could not rescan plugins; restart the shell and run thodu install again{}",
            if detail.is_empty() {
                String::new()
            } else {
                format!(": {detail}")
            }
        ));
    }

    if shell_enables_plugin(home, PLUGIN_ID) {
        return Ok(());
    }

    let deadline = Instant::now() + Duration::from_secs(2);
    let mut last_error: String;
    loop {
        match Command::new("omarchy")
            .args(["plugin", "list", "--json"])
            .output()
        {
            Ok(output) if output.status.success() => {
                let plugins = serde_json::from_slice::<serde_json::Value>(&output.stdout);
                if plugins.ok().and_then(|items| {
                    items.as_array().map(|items| {
                        items.iter().any(|item| {
                            item.get("id").and_then(serde_json::Value::as_str) == Some(PLUGIN_ID)
                        })
                    })
                }) == Some(true)
                {
                    break;
                }
                last_error = "Omarchy has not discovered Thodu yet".into();
            }
            Ok(output) => {
                let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
                last_error = if detail.is_empty() {
                    format!("omarchy plugin list exited with {}", output.status)
                } else {
                    detail
                };
            }
            Err(e) => last_error = format!("cannot list Omarchy plugins: {e}"),
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "Omarchy did not discover Thodu after a rescan: {last_error}; restart the shell and run thodu install again"
            ));
        }
        std::thread::sleep(Duration::from_millis(25));
    }

    let enabled = Command::new("omarchy")
        .args(["plugin", "enable", PLUGIN_ID, "--section", "right"])
        .output()
        .map_err(|e| format!("cannot enable the Thodu Omarchy plugin: {e}"))?;
    if enabled.status.success() {
        Ok(())
    } else {
        let detail = String::from_utf8_lossy(&enabled.stderr).trim().to_string();
        Err(format!(
            "cannot enable the Thodu Omarchy plugin{}; restart the shell and run thodu install again",
            if detail.is_empty() {
                String::new()
            } else {
                format!(": {detail}")
            }
        ))
    }
}

fn install_udev_rule(home: &Path) -> Result<(), String> {
    let custom_directory = std::env::var_os("THODU_UDEV_DIR").map(PathBuf::from);
    let directory = custom_directory
        .as_deref()
        .unwrap_or_else(|| Path::new("/etc/udev/rules.d"));
    let target = directory.join(UDEV_RULE_FILE);
    let unchanged =
        std::fs::read_to_string(&target).is_ok_and(|content| content == UDEV_RULE_CONTENT);
    let mut changed = false;

    if !unchanged {
        if let Some(directory) = custom_directory.as_deref() {
            std::fs::create_dir_all(directory)
                .map_err(|e| format!("{}: {e}", directory.display()))?;
            std::fs::write(&target, UDEV_RULE_CONTENT)
                .map_err(|e| format!("{}: {e}", target.display()))?;
        } else {
            let staged_dir = home.join(".local/share/thodu/install");
            std::fs::create_dir_all(&staged_dir)
                .map_err(|e| format!("{}: {e}", staged_dir.display()))?;
            let staged = staged_dir.join(UDEV_RULE_FILE);
            std::fs::write(&staged, UDEV_RULE_CONTENT)
                .map_err(|e| format!("{}: {e}", staged.display()))?;
            let result = Command::new("/usr/bin/pkexec")
                .args(["/usr/bin/install", "-D", "-m", "644"])
                .arg(&staged)
                .arg(&target)
                .status();
            let _ = std::fs::remove_file(&staged);
            match result {
                Ok(status) if status.success() => {}
                Ok(status) => {
                    return Err(format!(
                        "pkexec could not install the udev rule at {} (exit status {status}); rerun thodu install and approve the system prompt",
                        target.display()
                    ));
                }
                Err(e) => {
                    return Err(format!(
                        "cannot start pkexec to install the udev rule at {}: {e}",
                        target.display()
                    ));
                }
            }
        }
        changed = true;
    }

    let marker = home.join(".local/share/thodu/install/udev-applied");
    if changed || !marker.is_file() || !uinput_accessible() {
        if custom_directory.is_some()
            || std::env::var_os("THODU_UDEVADM").is_some()
            || std::env::var_os("THODU_MODPROBE").is_some()
        {
            command_succeeded("THODU_UDEVADM", "udevadm", &["control", "--reload-rules"])
                .map_err(|e| format!("could not reload the udev rules: {e}"))?;
            command_succeeded("THODU_MODPROBE", "modprobe", &["uinput"])
                .map_err(|e| format!("could not load the uinput module: {e}"))?;
            for args in [
                &[
                    "trigger",
                    "--action=change",
                    "--subsystem-match=misc",
                    "--sysname-match=uinput",
                ][..],
                &["settle"][..],
            ] {
                command_succeeded("THODU_UDEVADM", "udevadm", args)
                    .map_err(|e| format!("could not apply the udev rule: {e}"))?;
            }
        } else {
            let script = "/usr/bin/udevadm control --reload-rules && /usr/bin/modprobe uinput && /usr/bin/udevadm trigger --action=change --subsystem-match=misc --sysname-match=uinput && /usr/bin/udevadm settle";
            let output = Command::new("/usr/bin/pkexec")
                .args(["/usr/bin/sh", "-c", script])
                .output()
                .map_err(|e| format!("cannot reload udev rules through pkexec: {e}"))?;
            if !output.status.success() {
                let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
                return Err(format!(
                    "pkexec could not reload and apply the udev rule{}; rerun thodu install and approve the system prompt",
                    if detail.is_empty() {
                        String::new()
                    } else {
                        format!(": {detail}")
                    }
                ));
            }
        }
        if !wait_for_uinput_accessible() {
            return Err(
                "the udev rule was applied, but /dev/uinput is still unavailable; check the uinput module and rerun thodu install".into(),
            );
        }
        if let Some(parent) = marker.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&marker, UDEV_RULE_CONTENT)
            .map_err(|e| format!("{}: {e}", marker.display()))?;
    }
    Ok(())
}

fn doctor() -> Result<(), String> {
    let home = home_dir()?;
    let (items, ok) = check_doctor(&home);
    for item in &items {
        println!(
            "{}: {}",
            item.name,
            if item.present { "present" } else { "missing" }
        );
    }
    if ok {
        Ok(())
    } else {
        let missing = items.iter().filter(|i| !i.present).count();
        Err(format!("{missing} item(s) missing; run thodu install"))
    }
}

fn install(args: &[String]) -> Result<(), String> {
    let home = home_dir()?;
    let choices = install_key_choices(args, &home)?;
    do_install(&home, choices.as_ref())?;
    println!("thodu installed successfully");
    Ok(())
}

fn run(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some(mode @ ("hints" | "grid")) => {
            check_hyprland()?;
            let reply = ask(
                json!({"v": VERSION, "op": "start", "mode": mode, "scope": value(args, "--scope").unwrap_or_else(|| "visible".into()), "window": value(args, "--window"), "test": flag(args, "--test"), "overlay": !flag(args, "--no-overlay")}),
            )?;
            println!(
                "{} {}",
                reply.session.unwrap_or_default(),
                reply.status.unwrap_or_default()
            );
        }
        Some("targets") => {
            let reply = ask(
                json!({"v": VERSION, "op": "targets", "scope": value(args, "--scope").unwrap_or_else(|| "visible".into()), "window": value(args, "--window")}),
            )?;
            let targets = reply.targets.unwrap_or_default();
            for t in &targets {
                if flag(args, "--json") {
                    println!("{}", t.json_line());
                } else {
                    println!(
                        "{:>5} {:>5} {:>5}x{:<4} {:?} {}",
                        t.bounds.x, t.bounds.y, t.bounds.w, t.bounds.h, t.role, t.text
                    );
                }
            }
            if !flag(args, "--json") {
                eprintln!("{}", reply.status.unwrap_or_default());
            }
        }
        Some("key") => {
            let (Some(session), Some(key)) = (args.get(1), args.get(2)) else {
                return Err("usage: thodu key SESSION KEY [--shift] [--ctrl]".into());
            };
            let reply = ask(
                json!({"v": VERSION, "op": "key", "session": session, "key": key, "shift": flag(args, "--shift"), "ctrl": flag(args, "--ctrl")}),
            )?;
            println!("{}", serde_json::to_string(&reply.view).unwrap_or_default());
        }
        Some("bench") => bench(args)?,
        Some("record-tree") => {
            let out = value(args, "--out").ok_or_else(String::new)?;
            let reply =
                ask(json!({"v": VERSION, "op": "record", "window": value(args, "--window")}))?;
            let snapshot = reply.snapshot.ok_or("thodud sent no snapshot")?;
            let text = serde_json::to_string_pretty(&snapshot).map_err(|e| e.to_string())?;
            std::fs::write(&out, text + "\n").map_err(|e| format!("{out}: {e}"))?;
            println!("{out}: {}", reply.status.unwrap_or_default());
        }
        Some("close") => {
            let session = args.get(1).ok_or("usage: thodu close SESSION")?;
            ask(json!({"v": VERSION, "op": "close", "session": session}))?;
        }
        Some("install") => install(args)?,
        Some("doctor") => doctor()?,
        _ => return Err(String::new()),
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) if e.is_empty() => {
            eprintln!(
                "usage: thodu hints|grid|targets|key|close|bench|record-tree|install [--hints-key KEY --grid-key KEY --scope visible|focused --window ADDRESS]|doctor ..."
            );
            ExitCode::from(2)
        }
        Err(e) => {
            eprintln!("ERROR {e}");
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::keep_samples;

    // bench says where its samples are only when it wrote them.
    #[test]
    fn samples_are_kept_or_the_failure_is_said() {
        let dir = PathBuf::from(std::env::var_os("HOME").expect("HOME is set"))
            .join(".local/share/thodu/tests")
            .join(format!("bench-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = keep_samples(&dir, "{\"ms\":1}\n").expect("written");
        assert_eq!(
            std::fs::read_to_string(&file).expect("read back"),
            "{\"ms\":1}\n"
        );
        let err = keep_samples(&PathBuf::from("/proc/thodu-bench"), "x").expect_err("not writable");
        assert!(err.starts_with("/proc/thodu-bench"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
