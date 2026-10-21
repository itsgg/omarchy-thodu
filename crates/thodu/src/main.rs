//! thodu, the command line: Hyprland keybindings start sessions with it, and
//! agents read targets with it (SPEC section 4, item 3).
//!
//!   thodu hints [--window ADDRESS] [--test] [--no-overlay]   start a session
//!   thodu targets [--json] [--window ADDRESS] the targets, without labels
//!   thodu key SESSION KEY [--shift] [--ctrl]  send a key, for live checks
//!   thodu close SESSION
//!
//! It starts thodud when nothing answers on the socket. Errors print an
//! ERROR line and exit 1; bad arguments exit 2.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant};

use serde_json::json;
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
        Some("hints") => {
            let reply = ask(
                json!({"v": VERSION, "op": "start", "mode": "hints", "window": value(args, "--window"), "test": flag(args, "--test"), "overlay": !flag(args, "--no-overlay")}),
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
            eprintln!("usage: thodu hints|targets|key|close ...");
            ExitCode::from(2)
        }
        Err(e) => {
            eprintln!("ERROR {e}");
            ExitCode::from(1)
        }
    }
}
