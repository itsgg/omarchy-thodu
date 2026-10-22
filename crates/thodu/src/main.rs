//! thodu, the command line: Hyprland keybindings start sessions with it, and
//! agents read targets with it (SPEC section 4, item 3).
//!
//!   thodu hints [--window ADDRESS] [--test] [--no-overlay]   start a session
//!   thodu grid [--window ADDRESS] [--test] [--no-overlay]    start on the grid
//!   thodu targets [--json] [--window ADDRESS] the targets, without labels
//!   thodu key SESSION KEY [--shift] [--ctrl]  send a key, for live checks
//!   thodu close SESSION
//!   thodu bench [--sessions N] [--window ADDRESS]   time to labels drawn
//!   thodu record-tree --out FILE [--window ADDRESS]  record the tree
//!   thodu install                             install plugin, service and bindings
//!   thodu doctor                              check environment, rule and service
//!
//! ADDRESS is a Hyprland window address, or class:NAME for the first window
//! of that class.
//!
//! It starts thodud when nothing answers on the socket. Errors print an
//! ERROR line and exit 1; bad arguments exit 2.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::json;
use thodu_core::perf;
use thodu_core::protocol::{Reply, SOCKET_NAME, VERSION};

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
    let path = PathBuf::from(runtime)
        .join("hypr")
        .join(signature)
        .join(".socket.sock");
    UnixStream::connect(&path)
        .map_err(|e| format!("cannot connect to Hyprland on {}: {e}", path.display()))?;
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

fn check_doctor(home: &Path) -> (Vec<DoctorItem>, bool) {
    let mut items = Vec::new();
    let env_file = home.join(".config/environment.d/90-thodu.conf");
    let env_content = std::fs::read_to_string(&env_file).unwrap_or_default();

    let a11y = env_content
        .lines()
        .any(|l| l.trim() == "ACCESSIBILITY_ENABLED=1");
    items.push(DoctorItem {
        name: "ACCESSIBILITY_ENABLED=1",
        present: a11y,
    });

    let qt_a11y = env_content
        .lines()
        .any(|l| l.trim() == "QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1");
    items.push(DoctorItem {
        name: "QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1",
        present: qt_a11y,
    });

    let udev_rule_content = |path: &Path| -> bool {
        std::fs::read_to_string(path)
            .map(|content| {
                content.contains("KERNEL==\"uinput\"") && content.contains("TAG+=\"uaccess\"")
            })
            .unwrap_or(false)
    };
    let udev_rule = std::env::var_os("THODU_UDEV_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/etc/udev/rules.d"))
        .join("99-thodu-uinput.rules");
    let udev = udev_rule_content(&udev_rule);
    items.push(DoctorItem {
        name: "uinput udev rule",
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

    let service_file = home.join(".config/systemd/user/thodud.service").exists();
    let service_enabled = home
        .join(".config/systemd/user/default.target.wants/thodud.service")
        .exists();
    items.push(DoctorItem {
        name: "thodud user service",
        present: service_file && service_enabled,
    });

    let bindings_file = home.join(".config/hypr/bindings.lua");
    let bindings = std::fs::read_to_string(&bindings_file).unwrap_or_default();
    let binding_is_configured = |name: &str| {
        bindings.lines().any(|line| {
            let line = line.trim_start();
            !line.starts_with("--") && line.contains(name)
        })
    };
    let bindings = binding_is_configured("thodu hints") && binding_is_configured("thodu grid");
    items.push(DoctorItem {
        name: "hyprland keybindings",
        present: bindings,
    });

    let all_present = items.iter().all(|i| i.present);
    (items, all_present)
}

const PLUGIN_ASSETS: &[(&str, &[u8])] = &[
    ("manifest.json", include_bytes!("../../../manifest.json")),
    ("Thodu.qml", include_bytes!("../../../Thodu.qml")),
    ("BarProbe.qml", include_bytes!("../../../BarProbe.qml")),
    (
        "components/Client.qml",
        include_bytes!("../../../components/Client.qml"),
    ),
    (
        "components/Listener.qml",
        include_bytes!("../../../components/Listener.qml"),
    ),
    (
        "components/SessionView.qml",
        include_bytes!("../../../components/SessionView.qml"),
    ),
    ("lib/BarScan.js", include_bytes!("../../../lib/BarScan.js")),
];

const PLUGIN_ID: &str = "io.github.itsgg.thodu";

fn install_plugin_files(dest: &Path) -> Result<(), String> {
    for (path, contents) in PLUGIN_ASSETS {
        let target = dest.join(path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&target, contents).map_err(|e| format!("{}: {e}", target.display()))?;
    }
    Ok(())
}

fn do_install(home: &Path) -> Result<(), String> {
    let env_dir = home.join(".config/environment.d");
    std::fs::create_dir_all(&env_dir).map_err(|e| format!("{}: {e}", env_dir.display()))?;
    let env_file = env_dir.join("90-thodu.conf");
    let env_content = "ACCESSIBILITY_ENABLED=1\nQT_LINUX_ACCESSIBILITY_ALWAYS_ON=1\n";
    std::fs::write(&env_file, env_content).map_err(|e| format!("{}: {e}", env_file.display()))?;

    install_udev_rule(home)?;

    let plugin_dest = home.join(".config/omarchy/plugins/io.github.itsgg.thodu");
    std::fs::create_dir_all(&plugin_dest).map_err(|e| format!("{}: {e}", plugin_dest.display()))?;
    install_plugin_files(&plugin_dest)?;
    enable_shell_plugin()?;

    let service_dir = home.join(".config/systemd/user");
    std::fs::create_dir_all(&service_dir).map_err(|e| format!("{}: {e}", service_dir.display()))?;
    let service_unit = format!(
        "[Unit]\nDescription=Thodu hints daemon\nAfter=graphical-session.target\n\n[Service]\nExecStart={}\nRestart=on-failure\n\n[Install]\nWantedBy=default.target\n",
        daemon_binary().display()
    );
    let service_file = service_dir.join("thodud.service");
    std::fs::write(&service_file, service_unit)
        .map_err(|e| format!("{}: {e}", service_file.display()))?;
    let wants_dir = service_dir.join("default.target.wants");
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

    let hypr_dir = home.join(".config/hypr");
    std::fs::create_dir_all(&hypr_dir).map_err(|e| format!("{}: {e}", hypr_dir.display()))?;
    let bindings_file = hypr_dir.join("bindings.lua");
    let current_bindings = std::fs::read_to_string(&bindings_file).unwrap_or_default();
    let binding_is_configured = |name: &str| {
        current_bindings.lines().any(|line| {
            let line = line.trim_start();
            !line.starts_with("--") && line.contains(name)
        })
    };
    let mut binding_lines = String::new();
    if !binding_is_configured("thodu hints") || !binding_is_configured("thodu grid") {
        binding_lines.push_str("\n-- Thodu: keyboard hints for everything on an Omarchy screen\n");
    }
    if !binding_is_configured("thodu hints") {
        binding_lines.push_str("o.bind(\"SUPER + SHIFT + J\", \"Thodu hints\", \"thodu hints\")\n");
    }
    if !binding_is_configured("thodu grid") {
        binding_lines.push_str("o.bind(\"SUPER + CTRL + J\", \"Thodu grid\", \"thodu grid\")\n");
    }
    if !binding_lines.is_empty() {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&bindings_file)
            .map_err(|e| format!("{}: {e}", bindings_file.display()))?;
        std::io::Write::write_all(&mut f, binding_lines.as_bytes())
            .map_err(|e| format!("{}: {e}", bindings_file.display()))?;
    }

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

fn enable_shell_plugin() -> Result<(), String> {
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
    const CONTENT: &str = "KERNEL==\"uinput\", SUBSYSTEM==\"misc\", TAG+=\"uaccess\", OPTIONS+=\"static_node=uinput\"\n";
    const FILE: &str = "99-thodu-uinput.rules";

    if let Some(custom) = std::env::var_os("THODU_UDEV_DIR") {
        let directory = PathBuf::from(custom);
        std::fs::create_dir_all(&directory).map_err(|e| format!("{}: {e}", directory.display()))?;
        let target = directory.join(FILE);
        return std::fs::write(&target, CONTENT).map_err(|e| format!("{}: {e}", target.display()));
    }

    let target = Path::new("/etc/udev/rules.d").join(FILE);
    if std::fs::write(&target, CONTENT).is_ok() {
        return Ok(());
    }

    let staged_dir = home.join(".local/share/thodu/install");
    std::fs::create_dir_all(&staged_dir).map_err(|e| format!("{}: {e}", staged_dir.display()))?;
    let staged = staged_dir.join(FILE);
    std::fs::write(&staged, CONTENT).map_err(|e| format!("{}: {e}", staged.display()))?;
    let result = Command::new("/usr/bin/pkexec")
        .args(["/usr/bin/install", "-D", "-m", "644"])
        .arg(&staged)
        .arg(&target)
        .status();
    let _ = std::fs::remove_file(&staged);
    match result {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(format!(
            "pkexec could not install the udev rule at {} (exit status {status}); rerun thodu install and approve the system prompt",
            target.display()
        )),
        Err(e) => Err(format!(
            "cannot start pkexec to install the udev rule at {}: {e}",
            target.display()
        )),
    }
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

fn install() -> Result<(), String> {
    let home = home_dir()?;
    do_install(&home)?;
    println!("thodu installed successfully");
    Ok(())
}

fn run(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some(mode @ ("hints" | "grid")) => {
            check_hyprland()?;
            let reply = ask(
                json!({"v": VERSION, "op": "start", "mode": mode, "window": value(args, "--window"), "test": flag(args, "--test"), "overlay": !flag(args, "--no-overlay")}),
            )?;
            println!(
                "{} {}",
                reply.session.unwrap_or_default(),
                reply.status.unwrap_or_default()
            );
        }
        Some("targets") => {
            let reply =
                ask(json!({"v": VERSION, "op": "targets", "window": value(args, "--window")}))?;
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
        Some("install") => install()?,
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
                "usage: thodu hints|grid|targets|key|close|bench|record-tree|install|doctor ..."
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
