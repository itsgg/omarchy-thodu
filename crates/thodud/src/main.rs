//! thodud, Thodu's service: it collects targets, owns sessions, acts on the
//! chosen target and answers the IPC of SPEC section 7 on
//! $XDG_RUNTIME_DIR/thodu.sock.

mod atspi;
mod hypr;
mod overlay;
mod pointer;

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use thodu_core::labels;
use thodu_core::model::{Monitor, Target, Window};
use thodu_core::protocol::{
    self, Envelope, ErrorKind, LabeledTarget, MonitorInfo, OpenEvent, Reply, Request, Timing, View,
};
use thodu_core::session::{Act, Key, Session, State};
use thodu_core::stale::is_stale;
use thodu_core::tree;

/// How long the accessibility walk may take (REQ-SRC-05, provisional).
const ATSPI_BUDGET: Duration = Duration::from_millis(400);
/// How long an action waits for the overlay to say it is gone.
const HIDE_WAIT: Duration = Duration::from_millis(400);
/// How long a push to a listening overlay may block.
const LISTENER_WRITE: Duration = Duration::from_millis(100);
/// Sessions nobody finished are dropped after this.
const SESSION_LIFE: Duration = Duration::from_secs(120);

struct Entry {
    session: Session,
    window: Window,
    monitor: Monitor,
    labeled: Vec<LabeledTarget>,
    status: String,
    test: bool,
    hidden: bool,
    created: Instant,
    timing: Timing,
}

struct Daemon {
    socket: PathBuf,
    sessions: Mutex<HashMap<String, Entry>>,
    hidden: Condvar,
    next: AtomicU64,
    bus: Mutex<Option<atspi::Bus>>,
    /// Overlays listening for sessions (SPEC section 7, `listen`).
    listeners: Mutex<Vec<UnixStream>>,
}

fn now_us() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or_default()
}

type Res<T> = Result<T, Box<Reply>>;

fn fail(kind: ErrorKind, message: impl Into<String>) -> Box<Reply> {
    Box::new(Reply::error(kind, message))
}

fn internal(message: impl Into<String>) -> Box<Reply> {
    fail(ErrorKind::Internal, message)
}

impl Daemon {
    fn window(&self, address: Option<&str>) -> Res<Window> {
        let found = match address {
            Some(address) => hypr::window(address),
            None => hypr::active_window(),
        }
        .map_err(internal)?;
        found.ok_or_else(|| match address {
            Some(address) => fail(ErrorKind::NoWindow, format!("no window {address}")),
            None => fail(ErrorKind::NoWindow, "no window is focused"),
        })
    }

    /// Targets of `window` from its accessibility tree, and a status line.
    fn collect(&self, window: &Window, monitor: &Monitor) -> Res<(Vec<Target>, String)> {
        let started = Instant::now();
        let mut bus = self.bus.lock().map_err(|_| internal("bus lock"))?;
        if bus.is_none() {
            *bus = Some(atspi::Bus::connect().map_err(|e| fail(ErrorKind::NoTree, e))?);
        }
        let reader = bus.as_ref().ok_or_else(|| internal("no bus"))?;
        let (snapshot, truncated) = reader.snapshot(window, monitor, ATSPI_BUDGET);
        let took = started.elapsed().as_millis();
        if snapshot.apps.is_empty() {
            let status = format!(
                "{} (pid {}) is not on the accessibility bus",
                window.class, window.pid
            );
            return Ok((Vec::new(), status));
        }
        let targets = match tree::match_frame(&snapshot) {
            Some(frame) => tree::targets(frame, window, monitor),
            None => Vec::new(),
        };
        let cut = if truncated {
            ", cut short at the budget"
        } else {
            ""
        };
        Ok((
            targets.clone(),
            format!("{} targets from atspi in {took} ms{cut}", targets.len()),
        ))
    }

    fn start(&self, window: Option<String>, overlay: bool, test: bool) -> Res<Reply> {
        let window = self.window(window.as_deref())?;
        let monitor = hypr::monitor_of(&window).map_err(internal)?;
        let began = Instant::now();
        let (targets, status) = self.collect(&window, &monitor)?;
        let collect_ms = began.elapsed().as_millis() as u64;
        let session = Session::new(targets, false);
        let area = monitor.logical();
        let labeled = session
            .targets
            .iter()
            .zip(&session.labels)
            .map(|(t, label)| LabeledTarget {
                id: t.id.clone(),
                bounds: t.bounds,
                role: t.role,
                text: t.text.clone(),
                source: t.source,
                label: label.clone(),
                placed: labels::place(&t.bounds, protocol::label_size(label.len()), &area),
            })
            .collect();
        let id = format!("s{}", self.next.fetch_add(1, Ordering::Relaxed));
        {
            let mut sessions = self.sessions.lock().map_err(|_| internal("session lock"))?;
            sessions.retain(|_, e| e.created.elapsed() < SESSION_LIFE);
            sessions.insert(
                id.clone(),
                Entry {
                    session,
                    window,
                    monitor,
                    labeled,
                    status: status.clone(),
                    test,
                    hidden: false,
                    created: Instant::now(),
                    timing: Timing {
                        collect_ms,
                        drawn_at_us: None,
                    },
                },
            );
        }
        if overlay
            && !self.push(&id, test)
            && let Err(e) = overlay::summon(&id, &self.socket, test)
        {
            self.sessions
                .lock()
                .map_err(|_| internal("session lock"))?
                .remove(&id);
            return Err(internal(e));
        }
        Ok(Reply {
            session: Some(id),
            status: Some(status),
            ..Reply::ok()
        })
    }

    /// Hand the session to a listening overlay, which is much faster than
    /// summoning it (REQ-PERF-01). False when no overlay is listening.
    fn push(&self, id: &str, test: bool) -> bool {
        let Ok(reply) = self.session(id, false) else {
            return false;
        };
        let event = OpenEvent {
            v: protocol::VERSION,
            event: "open".into(),
            session: id.to_string(),
            test,
            reply,
        };
        let Ok(mut line) = serde_json::to_string(&event) else {
            return false;
        };
        line.push('\n');
        let Ok(mut listeners) = self.listeners.lock() else {
            return false;
        };
        // A listener that cannot take the line is gone; the next one may.
        while let Some(mut stream) = listeners.pop() {
            if stream.write_all(line.as_bytes()).is_ok() {
                listeners.push(stream);
                return true;
            }
        }
        false
    }

    fn drawn(&self, id: &str) -> Res<Reply> {
        let mut sessions = self.sessions.lock().map_err(|_| internal("session lock"))?;
        if let Some(entry) = sessions.get_mut(id) {
            entry.timing.drawn_at_us.get_or_insert_with(now_us);
        }
        Ok(Reply::ok())
    }

    fn view(entry: &Entry) -> View {
        let s = &entry.session;
        let id = |i: usize| s.targets[i].id.clone();
        View {
            state: s.state,
            visible: s.visible().into_iter().map(id).collect(),
            focused: s.focused.map(id),
            search: s.search.clone(),
            label_prefix: s.label_prefix.clone(),
        }
    }

    fn session(&self, id: &str, brief: bool) -> Res<Reply> {
        let sessions = self.sessions.lock().map_err(|_| internal("session lock"))?;
        let entry = sessions
            .get(id)
            .ok_or_else(|| fail(ErrorKind::BadRequest, format!("no session {id}")))?;
        if brief {
            return Ok(Reply {
                session: Some(id.to_string()),
                view: Some(Self::view(entry)),
                timing: Some(entry.timing.clone()),
                ..Reply::ok()
            });
        }
        let area = entry.monitor.logical();
        Ok(Reply {
            session: Some(id.to_string()),
            monitor: Some(MonitorInfo {
                name: entry.monitor.name.clone(),
                area,
                scale: entry.monitor.scale,
            }),
            targets: Some(entry.labeled.clone()),
            view: Some(Self::view(entry)),
            status: Some(entry.status.clone()),
            timing: Some(entry.timing.clone()),
            ..Reply::ok()
        })
    }

    fn key(self: &Arc<Self>, id: &str, name: &str, shift: bool, ctrl: bool) -> Res<Reply> {
        let key = parse_key(name, shift, ctrl)
            .ok_or_else(|| fail(ErrorKind::BadRequest, format!("unknown key {name}")))?;
        let mut sessions = self.sessions.lock().map_err(|_| internal("session lock"))?;
        let entry = sessions
            .get_mut(id)
            .ok_or_else(|| fail(ErrorKind::BadRequest, format!("no session {id}")))?;
        let act = entry.session.key(key);
        let view = Self::view(entry);
        if entry.session.state == State::Closed {
            sessions.remove(id);
        }
        drop(sessions);
        if let Some(act) = act {
            let daemon = Arc::clone(self);
            let id = id.to_string();
            thread::spawn(move || daemon.act(&id, &act));
        }
        Ok(Reply {
            session: Some(id.to_string()),
            view: Some(view),
            ..Reply::ok()
        })
    }

    /// Wait for the overlay to go, check the window has not moved, and click
    /// (REQ-ACT-01, REQ-ACT-03). A moved window gets a fresh session instead.
    fn act(&self, id: &str, act: &Act) {
        let Ok(sessions) = self.sessions.lock() else {
            return;
        };
        let Ok((mut sessions, _)) = self.hidden.wait_timeout_while(sessions, HIDE_WAIT, |s| {
            s.get(id).is_some_and(|e| !e.hidden)
        }) else {
            return;
        };
        let Some(entry) = sessions.remove(id) else {
            return;
        };
        drop(sessions);
        let now = hypr::window(&entry.window.address).ok().flatten();
        if is_stale(&entry.window, now.as_ref()) {
            eprintln!("thodud: {id}: the window moved since collection; collecting again");
            if let Err(e) = self.start(Some(entry.window.address.clone()), true, entry.test) {
                eprintln!("thodud: {id}: {}", e.message.unwrap_or_default());
            }
            return;
        }
        let restore = hypr::cursor().ok().and_then(|p| {
            hypr::monitors()
                .ok()?
                .into_iter()
                .map(|m| m.monitor)
                .find(|m| m.logical().contains_point(p.0, p.1))
                .map(|m| (m, p))
        });
        let restore = restore.as_ref().map(|(m, p)| (m, *p));
        if let Err(e) = pointer::click(&entry.monitor, act.point, act.button, act.count, restore) {
            eprintln!("thodud: {id}: the click failed: {e}");
        }
    }

    fn hidden(&self, id: &str) -> Res<Reply> {
        let mut sessions = self.sessions.lock().map_err(|_| internal("session lock"))?;
        if let Some(entry) = sessions.get_mut(id) {
            entry.hidden = true;
            if entry.session.state != State::Acting {
                sessions.remove(id);
            }
        }
        self.hidden.notify_all();
        Ok(Reply::ok())
    }

    fn targets(&self, window: Option<String>) -> Res<Reply> {
        let window = self.window(window.as_deref())?;
        let monitor = hypr::monitor_of(&window).map_err(internal)?;
        let (targets, status) = self.collect(&window, &monitor)?;
        let mut session = Session::new(targets, false);
        let labeled = session
            .targets
            .drain(..)
            .map(|t| LabeledTarget {
                id: t.id,
                bounds: t.bounds,
                role: t.role,
                text: t.text,
                source: t.source,
                label: String::new(),
                placed: t.bounds,
            })
            .collect();
        Ok(Reply {
            targets: Some(labeled),
            status: Some(status),
            ..Reply::ok()
        })
    }

    fn handle(self: &Arc<Self>, line: &str) -> Reply {
        let envelope: Envelope = match serde_json::from_str(line) {
            Ok(e) => e,
            Err(e) => return Reply::error(ErrorKind::BadRequest, format!("not a request: {e}")),
        };
        if envelope.v != protocol::VERSION {
            return Reply::error(
                ErrorKind::BadRequest,
                format!(
                    "protocol version {} is not {}",
                    envelope.v,
                    protocol::VERSION
                ),
            );
        }
        let result = match envelope.request {
            Request::Start {
                window,
                overlay,
                test,
                ..
            } => self.start(window, overlay, test),
            Request::Session { session, brief } => self.session(&session, brief),
            Request::Key {
                session,
                key,
                shift,
                ctrl,
            } => self.key(&session, &key, shift, ctrl),
            Request::Hidden { session } => self.hidden(&session),
            Request::Drawn { session } => self.drawn(&session),
            // serve() registers the connection once the reply is written.
            Request::Listen => Ok(Reply::ok()),
            Request::Targets { window } => self.targets(window),
            Request::Close { session } => {
                if let Ok(mut s) = self.sessions.lock() {
                    s.remove(&session);
                }
                self.hidden.notify_all();
                Ok(Reply::ok())
            }
        };
        result.unwrap_or_else(|e| *e)
    }
}

fn parse_key(name: &str, shift: bool, ctrl: bool) -> Option<Key> {
    let key = match name {
        "Return" | "Enter" => Key::Return { shift, ctrl },
        "Tab" => Key::Tab { shift },
        "Backtab" => Key::Tab { shift: true },
        "BackSpace" | "Backspace" => Key::Backspace,
        "Escape" => Key::Escape,
        "space" | "Space" => Key::Space,
        other => {
            let mut chars = other.chars();
            let c = chars.next()?;
            if chars.next().is_some() {
                return None;
            }
            Key::Char {
                c,
                shift: shift || c.is_uppercase(),
            }
        }
    };
    Some(key)
}

fn serve(daemon: &Arc<Daemon>, stream: UnixStream) {
    let Ok(writer) = stream.try_clone() else {
        return;
    };
    let mut writer = writer;
    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else { return };
        if line.trim().is_empty() {
            continue;
        }
        let reply = daemon.handle(&line);
        let mut text =
            serde_json::to_string(&reply).unwrap_or_else(|_| "{\"v\":1,\"ok\":false}".into());
        text.push('\n');
        if writer.write_all(text.as_bytes()).is_err() {
            return;
        }
        // A listening overlay's connection carries pushed sessions from
        // here on; nothing else writes to it.
        if reply.ok
            && matches!(
                serde_json::from_str::<Envelope>(&line).map(|e| e.request),
                Ok(Request::Listen)
            )
        {
            // A hung overlay must not hold up every later session.
            if let Ok(stream) = writer.try_clone()
                && stream.set_write_timeout(Some(LISTENER_WRITE)).is_ok()
                && let Ok(mut listeners) = daemon.listeners.lock()
            {
                listeners.push(stream);
            }
        }
    }
}

fn main() {
    let Ok(runtime) = std::env::var("XDG_RUNTIME_DIR") else {
        eprintln!("ERROR XDG_RUNTIME_DIR is not set");
        std::process::exit(1);
    };
    let socket = PathBuf::from(runtime).join(protocol::SOCKET_NAME);
    if UnixStream::connect(&socket).is_ok() {
        eprintln!("thodud is already running on {}", socket.display());
        return;
    }
    let _ = std::fs::remove_file(&socket);
    let listener = match UnixListener::bind(&socket) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("ERROR cannot listen on {}: {e}", socket.display());
            std::process::exit(1);
        }
    };
    if let Err(e) = std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600)) {
        eprintln!("ERROR cannot restrict {}: {e}", socket.display());
        std::process::exit(1);
    }
    let daemon = Arc::new(Daemon {
        socket,
        sessions: Mutex::new(HashMap::new()),
        hidden: Condvar::new(),
        next: AtomicU64::new(1),
        bus: Mutex::new(None),
        listeners: Mutex::new(Vec::new()),
    });
    for stream in listener.incoming().flatten() {
        let daemon = Arc::clone(&daemon);
        thread::spawn(move || serve(&daemon, stream));
    }
}
