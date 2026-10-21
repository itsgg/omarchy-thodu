//! thodu, the command line: Hyprland keybindings start sessions with it, and
//! agents read targets with it (SPEC section 4, item 3).
//!
//!   thodu hints [--window ADDRESS] [--test] [--no-overlay]   start a session
//!   thodu grid [--window ADDRESS] [--test] [--no-overlay]    start on the grid
//!   thodu targets [--json] [--window ADDRESS] the targets, without labels
//!   thodu key SESSION KEY [--shift] [--ctrl]  send a key, for live checks
//!   thodu close SESSION
//!   thodu bench [--sessions N] [--window ADDRESS]   time to labels drawn
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

fn run(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some(mode @ ("hints" | "grid")) => {
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
                    let line = json!({"id": t.id, "bounds": t.bounds, "role": t.role, "text": t.text, "source": t.source});
                    println!("{line}");
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
        Some("close") => {
            let session = args.get(1).ok_or("usage: thodu close SESSION")?;
            ask(json!({"v": VERSION, "op": "close", "session": session}))?;
        }
        _ => return Err(String::new()),
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) if e.is_empty() => {
            eprintln!("usage: thodu hints|grid|targets|key|close|bench ...");
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
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/test-bench");
        let file = keep_samples(&dir, "{\"ms\":1}\n").expect("written");
        assert_eq!(
            std::fs::read_to_string(&file).expect("read back"),
            "{\"ms\":1}\n"
        );
        let err = keep_samples(&PathBuf::from("/proc/thodu-bench"), "x").expect_err("not writable");
        assert!(err.starts_with("/proc/thodu-bench"), "{err}");
    }
}
