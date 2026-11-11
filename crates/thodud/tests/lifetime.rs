//! REQ-PERF-02: thodud sleeps while idle and stops once no client can reach
//! it. Each test runs the real binary on its own runtime directory and kills
//! it on the way out, so a failing test leaves no daemon behind.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// A daemon on a private runtime directory, killed and cleaned up on drop.
struct Daemon {
    root: PathBuf,
    child: Child,
}

impl Daemon {
    fn start(name: &str) -> Daemon {
        let home = std::env::var_os("HOME").map(PathBuf::from).expect("HOME");
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        // Under ~/.local/share/thodu (AGENTS.md rule 7), short enough for a
        // socket path.
        let root = home
            .join(".local/share/thodu/tests")
            .join(format!("{name}-{}-{stamp}", std::process::id()));
        let runtime = root.join("rt");
        std::fs::create_dir_all(&runtime).expect("create runtime directory");
        let child = Command::new(env!("CARGO_BIN_EXE_thodud"))
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("HOME", root.join("home"))
            .env("XDG_DATA_HOME", root.join("data"))
            .env("XDG_RUNTIME_DIR", &runtime)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start thodud");
        let daemon = Daemon { root, child };
        let deadline = Instant::now() + Duration::from_secs(5);
        while UnixStream::connect(daemon.socket()).is_err() {
            assert!(Instant::now() < deadline, "thodud never listened");
            std::thread::sleep(Duration::from_millis(10));
        }
        daemon
    }

    fn runtime(&self) -> PathBuf {
        self.root.join("rt")
    }

    fn socket(&self) -> PathBuf {
        self.runtime().join("thodu.sock")
    }

    /// The exit status once the daemon ends, or None if it is still running
    /// after `wait`.
    fn exits_within(&mut self, wait: Duration) -> Option<ExitStatus> {
        let deadline = Instant::now() + wait;
        loop {
            if let Some(status) = self.child.try_wait().expect("poll thodud") {
                return Some(status);
            }
            if Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn stderr(&mut self) -> String {
        let mut text = String::new();
        if let Some(mut err) = self.child.stderr.take() {
            let _ = err.read_to_string(&mut text);
        }
        text
    }

    /// Context switches of every thread: each wakeup is at least one.
    fn context_switches(&self) -> u64 {
        let tasks = PathBuf::from(format!("/proc/{}/task", self.child.id()));
        let mut total = 0;
        for task in std::fs::read_dir(tasks).expect("read threads").flatten() {
            let Ok(status) = std::fs::read_to_string(task.path().join("status")) else {
                continue;
            };
            for line in status.lines() {
                if line.contains("ctxt_switches:") {
                    total += line
                        .split_whitespace()
                        .last()
                        .and_then(|n| n.parse::<u64>().ok())
                        .unwrap_or(0);
                }
            }
        }
        total
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn shutdown(socket: &Path) -> String {
    let mut stream = UnixStream::connect(socket).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("timeout");
    stream
        .write_all(b"{\"v\":1,\"op\":\"shutdown\"}\n")
        .expect("send shutdown");
    let mut reply = String::new();
    BufReader::new(stream)
        .read_line(&mut reply)
        .expect("shutdown reply");
    reply
}

#[test]
fn req_perf_02_an_idle_thodud_sleeps() {
    let daemon = Daemon::start("idle");
    // Let start-up settle, then count wakeups over three idle seconds. The
    // socket check wakes it once a second; a 10 ms accept poll was about 300.
    std::thread::sleep(Duration::from_secs(1));
    let before = daemon.context_switches();
    std::thread::sleep(Duration::from_secs(3));
    let woke = daemon.context_switches() - before;
    assert!(woke <= 30, "an idle thodud woke {woke} times in 3 s");
}

#[test]
fn req_perf_02_thodud_stops_once_its_runtime_directory_is_removed() {
    let mut daemon = Daemon::start("rmdir");
    std::fs::remove_dir_all(daemon.runtime()).expect("remove runtime directory");
    let status = daemon
        .exits_within(Duration::from_secs(3))
        .expect("thodud kept running after its runtime directory was removed");
    assert_eq!(status.code(), Some(1), "a lost socket is a failure");
    let stderr = daemon.stderr();
    assert!(
        stderr
            .lines()
            .any(|l| l.starts_with("ERROR ") && l.contains("removed or replaced")),
        "{stderr}"
    );
}

#[test]
fn req_perf_02_thodud_stops_and_keeps_a_replacement_socket() {
    let mut daemon = Daemon::start("swap");
    std::fs::remove_file(daemon.socket()).expect("remove socket");
    let successor = UnixListener::bind(daemon.socket()).expect("bind replacement");
    let status = daemon
        .exits_within(Duration::from_secs(3))
        .expect("thodud kept running after its socket was replaced");
    assert_eq!(status.code(), Some(1));
    assert!(
        UnixStream::connect(daemon.socket()).is_ok(),
        "the replacement socket was unlinked"
    );
    drop(successor);
}

#[test]
fn req_perf_02_shutdown_releases_the_socket_at_once() {
    let mut daemon = Daemon::start("stop");
    let reply = shutdown(&daemon.socket());
    assert!(reply.contains("\"ok\":true"), "{reply}");
    // Well inside the one-second socket check: shutdown wakes the loop.
    let status = daemon
        .exits_within(Duration::from_millis(500))
        .expect("thodud did not stop promptly after shutdown");
    assert!(status.success(), "shutdown exits 0: {status}");
    assert!(
        !daemon.socket().exists(),
        "shutdown leaves no socket behind"
    );
}
