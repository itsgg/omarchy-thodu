//! thodu, the command line: Hyprland keybindings start sessions with it, and
//! agents read targets with it (SPEC section 4, item 3).
//!
//!   thodu hints [--scope visible|focused] [--window ADDRESS] [--test] [--no-overlay] start a session
//!   thodu grid [--window ADDRESS] [--test] [--no-overlay]    start on the grid
//!   thodu scroll [--scope visible|focused] [--window ADDRESS] [--test] [--no-overlay] scroll panes
//!   thodu targets [--scope visible|focused] [--json] [--window ADDRESS] print targets without labels
//!   thodu key SESSION KEY [--shift] [--ctrl]  send a key, for live checks
//!   thodu close SESSION
//!   thodu bench [--sessions N] [--window ADDRESS]   time to labels drawn
//!   thodu record-tree --out FILE [--window ADDRESS]  record the tree
//!   thodu install [--hints-key KEY --grid-key KEY] [--scroll-key KEY]
//!       defaults: SUPER + SHIFT + F12, SUPER + CTRL + F12, SUPER + ALT + F12
//!   thodu doctor                              check environment, rule and service
//!
//! ADDRESS is a Hyprland window address, or class:NAME for the first window
//! of that class.
//!
//! It starts thodud when nothing answers on the socket. Errors print an
//! ERROR line and exit 1; bad arguments print an ERROR line saying what to
//! change, then the command's usage, and exit 2.

mod lua;

use std::io::{BufRead, BufReader, Write};
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

/// Why a command failed: bad arguments exit 2 with the command's usage,
/// anything else exits 1.
#[derive(Debug, PartialEq, Eq)]
enum Failure {
    Usage { command: String, message: String },
    Run(String),
}

impl From<String> for Failure {
    fn from(message: String) -> Self {
        Failure::Run(message)
    }
}

impl From<&str> for Failure {
    fn from(message: &str) -> Self {
        Failure::Run(message.to_string())
    }
}

fn usage_error(command: &str, message: impl Into<String>) -> Failure {
    Failure::Usage {
        command: command.to_string(),
        message: message.into(),
    }
}

/// Each command's arguments: its positional names, flags and options that
/// take a value.
struct CommandSpec {
    name: &'static str,
    positional: &'static [&'static str],
    flags: &'static [&'static str],
    options: &'static [&'static str],
    usage: &'static str,
}

const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "hints",
        positional: &[],
        flags: &["--test", "--no-overlay"],
        options: &["--scope", "--window"],
        usage: "thodu hints [--scope visible|focused] [--window ADDRESS] [--test] [--no-overlay]",
    },
    CommandSpec {
        name: "grid",
        positional: &[],
        flags: &["--test", "--no-overlay"],
        options: &["--window"],
        usage: "thodu grid [--window ADDRESS] [--test] [--no-overlay]",
    },
    CommandSpec {
        name: "scroll",
        positional: &[],
        flags: &["--test", "--no-overlay"],
        options: &["--scope", "--window"],
        usage: "thodu scroll [--scope visible|focused] [--window ADDRESS] [--test] [--no-overlay]",
    },
    CommandSpec {
        name: "targets",
        positional: &[],
        flags: &["--json"],
        options: &["--scope", "--window"],
        usage: "thodu targets [--scope visible|focused] [--json] [--window ADDRESS]",
    },
    CommandSpec {
        name: "key",
        positional: &["SESSION", "KEY"],
        flags: &["--shift", "--ctrl"],
        options: &[],
        usage: "thodu key SESSION KEY [--shift] [--ctrl]",
    },
    CommandSpec {
        name: "close",
        positional: &["SESSION"],
        flags: &[],
        options: &[],
        usage: "thodu close SESSION",
    },
    CommandSpec {
        name: "bench",
        positional: &[],
        flags: &[],
        options: &["--sessions", "--window"],
        usage: "thodu bench [--sessions N] [--window ADDRESS]",
    },
    CommandSpec {
        name: "record-tree",
        positional: &[],
        flags: &[],
        options: &["--out", "--window"],
        usage: "thodu record-tree --out FILE [--window ADDRESS]",
    },
    CommandSpec {
        name: "install",
        positional: &[],
        flags: &[],
        options: &["--hints-key", "--grid-key", "--scroll-key"],
        usage: "thodu install [--hints-key KEY --grid-key KEY] [--scroll-key KEY]",
    },
    CommandSpec {
        name: "doctor",
        positional: &[],
        flags: &[],
        options: &[],
        usage: "thodu doctor",
    },
];

fn usage_lines() -> String {
    COMMANDS
        .iter()
        .map(|c| format!("  {}", c.usage))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The arguments after the command's name, checked against its spec.
#[derive(Debug, Default)]
struct Parsed {
    positional: Vec<String>,
    flags: Vec<&'static str>,
    values: Vec<(&'static str, String)>,
}

impl Parsed {
    fn flag(&self, name: &str) -> bool {
        self.flags.contains(&name)
    }

    fn value(&self, name: &str) -> Option<&str> {
        self.values
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v.as_str())
    }
}

fn parse_args(spec: &CommandSpec, args: &[String]) -> Result<Parsed, Failure> {
    let command = spec.name;
    let mut parsed = Parsed::default();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        i += 1;
        if !arg.starts_with("--") || arg == "--" {
            if parsed.positional.len() >= spec.positional.len() {
                return Err(usage_error(
                    command,
                    format!("thodu {command} takes no argument {arg:?}"),
                ));
            }
            parsed.positional.push(arg.clone());
            continue;
        }
        let (name, inline) = match arg.split_once('=') {
            Some((name, value)) => (name, Some(value.to_string())),
            None => (arg.as_str(), None),
        };
        if let Some(flag) = spec.flags.iter().find(|f| **f == name) {
            if inline.is_some() {
                return Err(usage_error(command, format!("{flag} takes no value")));
            }
            if parsed.flag(flag) {
                return Err(usage_error(command, format!("{flag} is given twice")));
            }
            parsed.flags.push(flag);
        } else if let Some(option) = spec.options.iter().find(|o| **o == name) {
            if parsed.value(option).is_some() {
                return Err(usage_error(command, format!("{option} is given twice")));
            }
            let value = match inline {
                Some(value) => value,
                None => match args.get(i) {
                    Some(value) if !value.starts_with("--") => {
                        i += 1;
                        value.clone()
                    }
                    _ => return Err(usage_error(command, format!("{option} needs a value"))),
                },
            };
            if value.is_empty() {
                return Err(usage_error(command, format!("{option} needs a value")));
            }
            parsed.values.push((option, value));
        } else {
            return Err(usage_error(
                command,
                format!("thodu {command} has no option {name}"),
            ));
        }
    }
    if parsed.positional.len() < spec.positional.len() {
        let missing = spec.positional[parsed.positional.len()..].join(" and ");
        return Err(usage_error(
            command,
            format!("thodu {command} needs {missing}"),
        ));
    }
    if let Some(scope) = parsed.value("--scope")
        && !matches!(scope, "visible" | "focused")
    {
        return Err(usage_error(
            command,
            format!("--scope must be visible or focused, not {scope:?}"),
        ));
    }
    Ok(parsed)
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
        return Err(child_error(&out.stderr));
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

fn bench(args: &Parsed) -> Result<(), Failure> {
    let sessions: usize = match args.value("--sessions") {
        Some(n) => n.parse().ok().filter(|n| *n > 0).ok_or_else(|| {
            usage_error(
                "bench",
                format!("--sessions must be a whole number above 0, not {n:?}"),
            )
        })?,
        None => 50,
    };
    let window = args.value("--window");
    let mut samples = Vec::with_capacity(sessions);
    let mut collects = Vec::with_capacity(sessions);
    let mut log = String::new();
    for i in 0..sessions {
        let (ms, collect) = bench_once(window)?;
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

/// The message of a failed `thodu` run, without the ERROR its own error
/// line starts with, so bench does not print it twice.
fn child_error(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let text = text.trim();
    text.strip_prefix("ERROR ").unwrap_or(text).to_string()
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

/// The accessibility variables install sets (REQ-INSTALL-01).
const A11Y_VARIABLES: [&str; 2] = ["ACCESSIBILITY_ENABLED", "QT_LINUX_ACCESSIBILITY_ALWAYS_ON"];

const MODES: [&str; 3] = ["hints", "grid", "scroll"];

const BINDINGS_FILE: &str = ".config/hypr/bindings.lua";

/// A text file of the user's: `None` when it does not exist, an error that
/// says what to do when it cannot be read or is not UTF-8, so nothing is
/// rewritten from a guess.
fn read_user_text(path: &Path) -> Result<Option<String>, String> {
    match std::fs::read(path) {
        Ok(bytes) => String::from_utf8(bytes).map(Some).map_err(|e| {
            format!(
                "{} is not UTF-8 text (byte {} is not); fix or remove that byte and rerun thodu install",
                path.display(),
                e.utf8_error().valid_up_to() + 1
            )
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!(
            "cannot read {}: {e}; make it readable and rerun thodu install",
            path.display()
        )),
    }
}

/// The user's Hyprland bindings, read as Lua.
struct BindingsFile {
    text: String,
    tokens: Vec<lua::Token>,
    /// Each mode's `o.bind` calls, in file order.
    thodu: [Vec<lua::Call>; 3],
    env: Vec<lua::Call>,
}

impl BindingsFile {
    fn parse(path: &Path, text: String) -> Result<Self, String> {
        let unreadable = |e: String| {
            format!(
                "{} cannot be read as Lua ({e}); fix it and rerun thodu install",
                path.display()
            )
        };
        let tokens = lua::tokenize(&text).map_err(unreadable)?;
        let binds = lua::calls(&tokens, "o", &["bind", "rebind"]).map_err(unreadable)?;
        let env = lua::calls(&tokens, "hl", &["env"]).map_err(unreadable)?;
        let thodu = MODES.map(|mode| {
            binds
                .iter()
                .filter(|call| call.arg_string(1).as_deref() == Some(&format!("Thodu {mode}")))
                .cloned()
                .collect()
        });
        Ok(Self {
            text,
            tokens,
            thodu,
            env,
        })
    }

    fn has_env(&self, variable: &str) -> bool {
        self.env.iter().any(|call| {
            call.arg_string(0).as_deref() == Some(variable)
                && call.arg_string(1).as_deref() == Some("1")
        })
    }

    /// The mode is bound to a command that runs `thodu MODE`.
    fn binds_mode(&self, index: usize) -> bool {
        let mode = MODES[index];
        self.thodu[index].iter().any(|call| {
            call.arg_string(2)
                .is_some_and(|command| command.ends_with(&format!(" {mode}")))
        })
    }
}

/// The variables of the running user session, which apps started through
/// systemd (as `uwsm-app` starts them) inherit.
fn live_session_environment() -> Option<String> {
    systemctl_output(&["--user", "show-environment"])
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
}

struct DoctorReport {
    items: Vec<DoctorItem>,
    /// Why an item could not be checked.
    notes: Vec<String>,
}

fn check_doctor(home: &Path) -> DoctorReport {
    let mut items = Vec::new();
    let mut notes = Vec::new();
    let bindings_path = home.join(BINDINGS_FILE);
    let bindings = match read_user_text(&bindings_path) {
        Ok(text) => match BindingsFile::parse(&bindings_path, text.unwrap_or_default()) {
            Ok(file) => Some(file),
            Err(e) => {
                notes.push(e);
                None
            }
        },
        Err(e) => {
            notes.push(e);
            None
        }
    };
    // REQ-INSTALL-01: a variable is present only when Hyprland's config sets
    // it and the running session has it, since apps launched through systemd
    // read the session's copy, not the file.
    let live = live_session_environment().unwrap_or_default();
    for (variable, name) in A11Y_VARIABLES.iter().zip([
        "ACCESSIBILITY_ENABLED=1 in Hyprland",
        "QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1 in Hyprland",
    ]) {
        let configured = bindings.as_ref().is_some_and(|b| b.has_env(variable));
        let running = live.lines().any(|line| line == format!("{variable}=1"));
        if configured && !running {
            notes.push(format!(
                "{variable}=1 is in {} but not in the running session",
                bindings_path.display()
            ));
        }
        items.push(DoctorItem {
            name,
            present: configured && running,
        });
    }

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

    let bound = bindings
        .as_ref()
        .is_some_and(|b| (0..MODES.len()).all(|i| b.binds_mode(i)));
    items.push(DoctorItem {
        name: "hyprland keybindings",
        present: bound,
    });

    DoctorReport { items, notes }
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

const DEFAULT_KEYS: [&str; 3] = [
    "SUPER + SHIFT + F12",
    "SUPER + CTRL + F12",
    "SUPER + ALT + F12",
];

/// The keys asked for on the command line, one per mode; `None` keeps the
/// mode's existing binding, or binds its default when there is none.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct KeyRequest {
    keys: [Option<String>; 3],
}

impl KeyRequest {
    fn any(&self) -> bool {
        self.keys.iter().any(Option::is_some)
    }
}

fn validate_key(key: &str, option: &str) -> Result<String, Failure> {
    let key = key.trim();
    if key.is_empty()
        || !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || " +_-:".contains(c))
    {
        return Err(usage_error(
            "install",
            format!(
                "{option} must contain only key names, spaces, plus signs, underscores, hyphens or colons, as in \"SUPER + SHIFT + F12\""
            ),
        ));
    }
    Ok(key.to_string())
}

fn install_key_request(args: &Parsed) -> Result<KeyRequest, Failure> {
    let (hints, grid) = match (args.value("--hints-key"), args.value("--grid-key")) {
        (Some(hints), Some(grid)) => (
            Some(validate_key(hints, "--hints-key")?),
            Some(validate_key(grid, "--grid-key")?),
        ),
        (Some(_), None) | (None, Some(_)) => {
            return Err(usage_error(
                "install",
                "--hints-key and --grid-key must be supplied together",
            ));
        }
        (None, None) => (None, None),
    };
    let scroll = args
        .value("--scroll-key")
        .map(|key| validate_key(key, "--scroll-key"))
        .transpose()?;
    Ok(KeyRequest {
        keys: [hints, grid, scroll],
    })
}

/// A Lua string literal for `value`: only printable characters appear as
/// themselves, so the literal reads back as `value` in any Lua.
fn lua_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\{:03}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// The range of a whole line holding only `span` (and maybe a `;`), so
/// removing a duplicate binding leaves no blank line; otherwise `span`.
fn removal_range(text: &str, span: &std::ops::Range<usize>) -> std::ops::Range<usize> {
    let line_start = text[..span.start].rfind('\n').map_or(0, |i| i + 1);
    let line_end = text[span.end..]
        .find('\n')
        .map_or(text.len(), |i| span.end + i + 1);
    let before = &text[line_start..span.start];
    let after = text[span.end..line_end].trim();
    if before.trim().is_empty() && (after.is_empty() || after == ";") {
        line_start..line_end
    } else {
        span.clone()
    }
}

/// REQ-INSTALL-01: the bindings file with Thodu's bindings and variables
/// set, every other byte of it kept. A mode's first binding keeps its key
/// expression as written (single quotes, concatenation or a variable) unless
/// a key was asked for, and gets the current command; later duplicates go.
/// What is added goes before a top-level `return`. The result is read back
/// as Lua before it is returned, so a file Thodu cannot follow is refused.
fn plan_bindings(
    path: &Path,
    file: &BindingsFile,
    request: &KeyRequest,
    executable: &str,
) -> Result<String, Failure> {
    let text = &file.text;
    // The key each mode will have, where it can be known without running
    // the file, to refuse two modes on one key.
    let mut keys: [Option<String>; 3] = Default::default();
    let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    let mut added = String::new();
    for (index, mode) in MODES.iter().enumerate() {
        let command = lua_string(&format!("{} {mode}", shell_quote(executable)));
        let requested = request.keys[index].as_deref();
        match file.thodu[index].split_first() {
            Some((first, duplicates)) => {
                let key_source = match requested {
                    Some(key) => lua_string(key),
                    None => first
                        .arg_source(text, 0)
                        .ok_or_else(|| {
                            format!(
                                "the Thodu {mode} binding in {} has no key; fix it and rerun thodu install",
                                path.display()
                            )
                        })?
                        .to_string(),
                };
                keys[index] = requested
                    .map(str::to_string)
                    .or_else(|| first.arg_string(0));
                let description = first
                    .arg_source(text, 1)
                    .expect("matched by its description");
                let mut call = format!("{}({key_source}, {description}, {command}", first.callee);
                for extra in 3..first.args.len() {
                    let source = first.arg_source(text, extra).ok_or_else(|| {
                        format!(
                            "the Thodu {mode} binding in {} has an empty argument; fix it and rerun thodu install",
                            path.display()
                        )
                    })?;
                    call.push_str(", ");
                    call.push_str(source);
                }
                call.push(')');
                edits.push((first.span.clone(), call));
                for duplicate in duplicates {
                    edits.push((removal_range(text, &duplicate.span), String::new()));
                }
            }
            None => {
                let key = requested.unwrap_or(DEFAULT_KEYS[index]);
                keys[index] = Some(key.to_string());
                added.push_str(&format!(
                    "o.bind({}, {}, {command})\n",
                    lua_string(key),
                    lua_string(&format!("Thodu {mode}"))
                ));
            }
        }
    }
    let known: Vec<&String> = keys.iter().flatten().collect();
    if (0..known.len()).any(|i| known[i + 1..].contains(&known[i])) {
        let message = "Thodu hints, grid and scroll keybindings must be different";
        return Err(if request.any() {
            usage_error("install", message)
        } else {
            Failure::Run(format!(
                "{message}; fix {} or pass the keys",
                path.display()
            ))
        });
    }
    for variable in A11Y_VARIABLES {
        if !file.has_env(variable) {
            added.push_str(&format!("hl.env({}, \"1\")\n", lua_string(variable)));
        }
    }
    if !added.is_empty() {
        match lua::top_level_return(&file.tokens) {
            Some(at) => {
                let line_start = text[..at].rfind('\n').map_or(0, |i| i + 1);
                if text[line_start..at].trim().is_empty() {
                    edits.push((line_start..line_start, added));
                } else {
                    edits.push((at..at, format!("\n{added}")));
                }
            }
            None => {
                let separator = if text.is_empty() || text.ends_with('\n') {
                    ""
                } else {
                    "\n"
                };
                edits.push((text.len()..text.len(), format!("{separator}{added}")));
            }
        }
    }
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut updated = text.clone();
    for (range, replacement) in edits {
        updated.replace_range(range, &replacement);
    }
    let check = BindingsFile::parse(path, updated.clone()).map_err(|e| {
        format!(
            "thodu could not update the bindings safely ({e}); {} is unchanged",
            path.display()
        )
    })?;
    let complete = check.thodu.iter().all(|calls| calls.len() == 1)
        && (0..MODES.len()).all(|i| check.binds_mode(i))
        && A11Y_VARIABLES.iter().all(|v| check.has_env(v))
        && lua::top_level_return(&check.tokens)
            .is_none_or(|at| check.thodu.iter().flatten().all(|c| c.span.end <= at));
    if !complete {
        return Err(Failure::Run(format!(
            "thodu could not update the bindings safely; {} is unchanged; add the Thodu bindings by hand",
            path.display()
        )));
    }
    Ok(updated)
}

/// Replace `path` with `contents` all at once: a temporary file in the same
/// directory is written, flushed to disk and renamed over it, so a full disk
/// or a crash leaves the old file whole. A symlink is followed, so a file
/// kept elsewhere (a dotfiles checkout) is updated where it lives; the
/// file's permissions are kept.
fn write_atomic(path: &Path, contents: &[u8]) -> Result<(), String> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let target = match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => std::fs::canonicalize(path).map_err(|e| {
            format!(
                "{} is a symlink thodu cannot follow ({e}); fix the link and rerun thodu install",
                path.display()
            )
        })?,
        _ => path.to_path_buf(),
    };
    let mode = std::fs::metadata(&target)
        .map(|m| m.permissions().mode() & 0o7777)
        .unwrap_or(0o644);
    let dir = target
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = target
        .file_name()
        .ok_or_else(|| format!("{} names no file", target.display()))?
        .to_string_lossy();
    let temp = dir.join(format!(".{name}.thodu-{}.tmp", std::process::id()));
    let fail = |e: std::io::Error| {
        let _ = std::fs::remove_file(&temp);
        format!(
            "cannot write {}: {e}; it is unchanged, free space or fix permissions and rerun thodu install",
            target.display()
        )
    };
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(&temp)
        .map_err(|e| {
            format!(
                "cannot write beside {}: {e}; it is unchanged",
                target.display()
            )
        })?;
    file.write_all(contents).map_err(fail)?;
    file.set_permissions(std::fs::Permissions::from_mode(mode))
        .map_err(fail)?;
    file.sync_all().map_err(fail)?;
    drop(file);
    std::fs::rename(&temp, &target).map_err(fail)?;
    if let Ok(dir) = std::fs::File::open(dir) {
        let _ = dir.sync_all();
    }
    Ok(())
}

/// The bindings file as install will write it, or why it is refused; read
/// before install changes anything.
fn planned_bindings(home: &Path, request: &KeyRequest) -> Result<Option<String>, Failure> {
    let path = home.join(BINDINGS_FILE);
    let current = read_user_text(&path)?;
    let executable = std::env::current_exe()
        .map_err(|e| format!("cannot find thodu executable: {e}"))?
        .to_string_lossy()
        .into_owned();
    let file = BindingsFile::parse(&path, current.clone().unwrap_or_default())?;
    let updated = plan_bindings(&path, &file, request, &executable)?;
    Ok((current.as_deref() != Some(updated.as_str())).then_some(updated))
}

fn install_hyprland_config(home: &Path, updated: Option<&str>) -> Result<(), String> {
    if let Some(updated) = updated {
        let hypr_dir = home.join(".config/hypr");
        std::fs::create_dir_all(&hypr_dir).map_err(|e| format!("{}: {e}", hypr_dir.display()))?;
        write_atomic(&home.join(BINDINGS_FILE), updated.as_bytes())?;
    }
    command_succeeded("THODU_HYPRCTL", "hyprctl", &["reload"])
        .map_err(|e| format!("could not reload Hyprland config: {e}"))?;
    apply_session_environment()
}

/// REQ-INSTALL-01: Omarchy copies Hyprland's environment to systemd and
/// D-Bus only when the session starts, so install hands the variables to
/// the running session too; apps launched from now on get them without a
/// new login.
fn apply_session_environment() -> Result<(), String> {
    let assignments: Vec<String> = A11Y_VARIABLES.iter().map(|v| format!("{v}=1")).collect();
    let assignments: Vec<&str> = assignments.iter().map(String::as_str).collect();
    let mut systemctl = vec!["--user", "set-environment"];
    systemctl.extend(&assignments);
    command_succeeded("THODU_SYSTEMCTL", "systemctl", &systemctl).map_err(|e| {
        format!("could not set the accessibility variables in the user session: {e}")
    })?;
    let mut dbus = vec!["--systemd"];
    dbus.extend(&assignments);
    command_succeeded(
        "THODU_DBUS_UPDATE_ENV",
        "dbus-update-activation-environment",
        &dbus,
    )
    .map_err(|e| format!("could not set the accessibility variables for D-Bus activation: {e}"))
}

/// The thodud the service runs: the one beside this thodu, else the first
/// on PATH, as an absolute path. A unit with a bare name would fail at
/// login, so install refuses instead.
fn installed_daemon() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("cannot find thodu executable: {e}"))?;
    let beside = exe.parent().map(|dir| dir.join("thodud"));
    let on_path = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join("thodud"));
    beside
        .into_iter()
        .chain(on_path)
        .find(|candidate| {
            use std::os::unix::fs::PermissionsExt;
            std::fs::metadata(candidate)
                .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        })
        .ok_or_else(|| {
            format!(
                "cannot find thodud beside {} or on PATH; install both binaries together and rerun thodu install",
                exe.display()
            )
        })
}

/// `path` as the command of a systemd `ExecStart=`: normalized, quoted so a
/// space stays in it, with `%` doubled so no specifier is expanded (systemd
/// expands no `$` in the command's path). systemd refuses a command path
/// with a quote, a backslash, a glob character or a control character, as
/// `systemd-analyze verify` shows, so install refuses one too.
fn systemd_exec_path(path: &Path) -> Result<String, String> {
    let normal: PathBuf = path.components().collect();
    let refuse = |why: &str| {
        format!(
            "{} {why}, which a systemd unit cannot run; move thodud to a plain path and rerun thodu install",
            path.display()
        )
    };
    if !normal.is_absolute()
        || normal
            .components()
            .any(|c| c == std::path::Component::ParentDir)
    {
        return Err(refuse("is not a normalized absolute path"));
    }
    let text = normal.to_str().ok_or_else(|| refuse("is not UTF-8"))?;
    if let Some(c) = text
        .chars()
        .find(|c| c.is_control() || "\"'\\*?[".contains(*c))
    {
        return Err(refuse(&format!("has the character {c:?}")));
    }
    Ok(format!("\"{}\"", text.replace('%', "%%")))
}

fn enable_user_service(home: &Path, unit: &str) -> Result<(), String> {
    let service_dir = home.join(".config/systemd/user");
    std::fs::create_dir_all(&service_dir).map_err(|e| format!("{}: {e}", service_dir.display()))?;
    write_atomic(&service_dir.join("thodud.service"), unit.as_bytes())?;

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

fn do_install(home: &Path, request: &KeyRequest) -> Result<(), Failure> {
    // Refuse before changing anything when the bindings cannot be followed
    // or the daemon cannot be found.
    planned_bindings(home, request)?;
    let daemon = systemd_exec_path(&installed_daemon()?)?;

    install_udev_rule(home)?;

    let plugin_dest = home.join(".config/omarchy/plugins/io.github.itsgg.thodu");
    std::fs::create_dir_all(&plugin_dest).map_err(|e| format!("{}: {e}", plugin_dest.display()))?;
    install_plugin_files(&plugin_dest)?;
    enable_shell_plugin(home)?;

    // Planned again from the file as it is now, in case it changed.
    let bindings = planned_bindings(home, request)?;
    install_hyprland_config(home, bindings.as_deref())?;

    let service_unit = format!(
        "[Unit]\nDescription=Thodu hints daemon\nPartOf=graphical-session.target\nAfter=graphical-session.target\n\n[Service]\nExecStart={daemon}\nEnvironment=XDG_RUNTIME_DIR=%t\nRestart=on-failure\n\n[Install]\nWantedBy=graphical-session.target\n"
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
    let report = check_doctor(&home);
    for item in &report.items {
        println!(
            "{}: {}",
            item.name,
            if item.present { "present" } else { "missing" }
        );
    }
    let missing = report.items.iter().filter(|i| !i.present).count();
    if missing == 0 {
        Ok(())
    } else {
        let mut message = format!("{missing} item(s) missing");
        for note in &report.notes {
            message.push_str("; ");
            message.push_str(note);
        }
        message.push_str("; run thodu install");
        Err(message)
    }
}

fn install(args: &Parsed) -> Result<(), Failure> {
    let home = home_dir()?;
    let request = install_key_request(args)?;
    do_install(&home, &request)?;
    println!("thodu installed successfully");
    Ok(())
}

fn run(args: &[String]) -> Result<(), Failure> {
    let Some(name) = args.first() else {
        return Err(usage_error("", "name a command"));
    };
    let Some(spec) = COMMANDS.iter().find(|c| c.name == name) else {
        return Err(usage_error("", format!("there is no command {name:?}")));
    };
    let args = parse_args(spec, &args[1..])?;
    let scope = args.value("--scope").unwrap_or("visible");
    match spec.name {
        mode @ ("hints" | "grid" | "scroll") => {
            check_hyprland()?;
            let reply = ask(
                json!({"v": VERSION, "op": "start", "mode": mode, "scope": scope, "window": args.value("--window"), "test": args.flag("--test"), "overlay": !args.flag("--no-overlay")}),
            )?;
            println!(
                "{} {}",
                reply.session.unwrap_or_default(),
                reply.status.unwrap_or_default()
            );
        }
        "targets" => {
            let reply = ask(
                json!({"v": VERSION, "op": "targets", "scope": scope, "window": args.value("--window")}),
            )?;
            let targets = reply.targets.unwrap_or_default();
            for t in &targets {
                if args.flag("--json") {
                    println!("{}", t.json_line());
                } else {
                    println!(
                        "{:>5} {:>5} {:>5}x{:<4} {:?} {}",
                        t.bounds.x, t.bounds.y, t.bounds.w, t.bounds.h, t.role, t.text
                    );
                }
            }
            if !args.flag("--json") {
                eprintln!("{}", reply.status.unwrap_or_default());
            }
        }
        "key" => {
            let (session, key) = (&args.positional[0], &args.positional[1]);
            let current =
                ask(json!({"v": VERSION, "op": "session", "session": session, "brief": true}))?;
            let generation = current
                .view
                .ok_or("thodud sent no session view")?
                .generation;
            let reply = ask(
                json!({"v": VERSION, "op": "key", "session": session, "generation": generation, "key": key, "shift": args.flag("--shift"), "ctrl": args.flag("--ctrl")}),
            )?;
            println!("{}", serde_json::to_string(&reply.view).unwrap_or_default());
        }
        "bench" => bench(&args)?,
        "record-tree" => {
            let out = args
                .value("--out")
                .ok_or_else(|| usage_error("record-tree", "--out FILE is required"))?;
            let reply =
                ask(json!({"v": VERSION, "op": "record", "window": args.value("--window")}))?;
            let snapshot = reply.snapshot.ok_or("thodud sent no snapshot")?;
            let text = serde_json::to_string_pretty(&snapshot).map_err(|e| e.to_string())?;
            std::fs::write(out, text + "\n").map_err(|e| format!("{out}: {e}"))?;
            println!("{out}: {}", reply.status.unwrap_or_default());
        }
        "close" => {
            let session = &args.positional[0];
            ask(json!({"v": VERSION, "op": "close", "session": session}))?;
        }
        "install" => install(&args)?,
        "doctor" => doctor()?,
        other => unreachable!("{other} is in COMMANDS"),
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Usage { command, message }) => {
            eprintln!("ERROR {message}; see the usage below");
            match COMMANDS.iter().find(|c| c.name == command) {
                Some(spec) => eprintln!("usage: {}", spec.usage),
                None => eprintln!("usage:\n{}", usage_lines()),
            }
            ExitCode::from(2)
        }
        Err(Failure::Run(e)) => {
            eprintln!("ERROR {e}");
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{
        BindingsFile, Failure, KeyRequest, PLUGIN_ASSETS, child_error, keep_samples, lua_string,
        plan_bindings, systemd_exec_path,
    };
    use std::path::Path;

    fn plan(text: &str, request: &KeyRequest) -> Result<String, Failure> {
        let path = Path::new("bindings.lua");
        let file = BindingsFile::parse(path, text.to_string()).map_err(Failure::Run)?;
        plan_bindings(path, &file, request, "/opt/thodu/bin/thodu")
    }

    // REQ-INSTALL-01: o.rebind, extra arguments, a return on a line with
    // other code and an empty file all come out as valid, complete bindings.
    #[test]
    fn req_install_01_bindings_plans_keep_what_they_do_not_own() {
        let none = KeyRequest::default();
        let out = plan(
            "o.rebind(\"A\", \"Thodu hints\", \"x\", { hold = true }); o.bind(\"B\", \"Thodu grid\", \"y\")\nlocal t = {} return t\n",
            &none,
        )
        .expect("planned");
        assert!(
            out.starts_with("o.rebind(\"A\", \"Thodu hints\", \"'/opt/thodu/bin/thodu' hints\", { hold = true }); o.bind(\"B\", \"Thodu grid\", "),
            "{out}"
        );
        assert!(out.ends_with("\nreturn t\n"), "{out}");
        let file = BindingsFile::parse(Path::new("b"), out.clone()).expect("reads back");
        assert!(file.has_env("ACCESSIBILITY_ENABLED"));
        assert!((0..3).all(|i| file.binds_mode(i)), "{out}");

        let fresh = plan("", &none).expect("planned");
        assert!(
            fresh.starts_with("o.bind(\"SUPER + SHIFT + F12\", \"Thodu hints\", "),
            "{fresh}"
        );
        assert_eq!(
            plan(&fresh, &none).expect("planned"),
            fresh,
            "a rerun changes nothing"
        );

        let no_newline = plan("o.bind(\"SUPER + R\", \"Run\", \"run\")", &none).expect("planned");
        assert!(
            no_newline.starts_with("o.bind(\"SUPER + R\", \"Run\", \"run\")\no.bind("),
            "{no_newline}"
        );

        for broken in [
            "o.bind(\"A\", \"Thodu hints\"",
            "x = \"open",
            "--[[ open",
            "o.bind(\"A\", \"Thodu hints\", 'x') \x01",
        ] {
            assert!(
                matches!(plan(broken, &none), Err(Failure::Run(_))),
                "{broken} is refused"
            );
        }
    }

    #[test]
    fn lua_strings_read_back_as_written() {
        assert_eq!(lua_string("a\"b\\c\nd\u{1b}"), "\"a\\\"b\\\\c\\nd\\027\"");
        let tokens = crate::lua::tokenize(&lua_string("x\u{7f}\"\\y é")).expect("tokens");
        assert_eq!(
            tokens[0].kind,
            crate::lua::Kind::Str("x\u{7f}\"\\y é".into())
        );
    }

    // REQ-INSTALL-01: the unit's command is quoted for systemd.
    #[test]
    fn req_install_01_unit_paths_are_quoted_for_systemd() {
        assert_eq!(
            systemd_exec_path(Path::new("/a b//100%/./$x/thodud")).expect("quoted"),
            "\"/a b/100%%/$x/thodud\""
        );
        for refused in [
            "thodud",
            "/a/../thodud",
            "/a\nb/thodud",
            "/q\"/thodud",
            "/q\\/thodud",
            "/q'/thodud",
            "/q*/thodud",
            "/q?/thodud",
            "/q[/thodud",
        ] {
            let err = systemd_exec_path(Path::new(refused)).expect_err(refused);
            assert!(err.contains("rerun thodu install"), "{err}");
        }
    }

    // bench's own ERROR is not repeated.
    #[test]
    fn a_child_error_is_said_once() {
        assert_eq!(child_error(b"ERROR thodud is down\n"), "thodud is down");
        assert_eq!(child_error(b"plain\n"), "plain");
    }

    // REQ-INSTALL-01: install ships exactly the plugin files git tracks, so
    // an editor's lock symlink or an untracked file beside them is neither
    // read by the build nor installed.
    #[test]
    fn req_install_01_plugin_assets_are_the_tracked_plugin_files() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(&root)
            .args([
                "ls-files",
                "--",
                "manifest.json",
                "Thodu.qml",
                "BarProbe.qml",
                "components",
                "lib",
            ])
            .output()
            .expect("run git ls-files");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let tracked: std::collections::BTreeSet<String> = String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(str::to_string)
            .collect();
        let shipped: std::collections::BTreeSet<String> = PLUGIN_ASSETS
            .iter()
            .map(|(path, _)| path.to_string())
            .collect();
        assert_eq!(shipped, tracked);
    }

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
