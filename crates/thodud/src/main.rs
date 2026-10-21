//! thodud, Thodu's service: it collects targets, owns sessions, acts on the
//! chosen target and answers the IPC of SPEC section 7 on
//! $XDG_RUNTIME_DIR/thodu.sock.

mod atspi;
mod hypr;
mod keyboard;
mod listeners;
mod overlay;
mod pointer;
mod sources;

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use thodu_core::config::{self, Config};
use thodu_core::input;
use thodu_core::labels;
use thodu_core::model::{Monitor, Source, Target, Window};
use thodu_core::protocol::{
    self, Envelope, ErrorKind, LabeledTarget, MonitorInfo, OpenEvent, Reply, Request, Timing, View,
};
use thodu_core::session::{Act, Key, Session, State};
use thodu_core::stale::is_stale;

/// How long the accessibility source may take (REQ-SRC-05, provisional).
const ATSPI_BUDGET: Duration = Duration::from_millis(80);
/// How long the bar source may take (REQ-SRC-05, provisional).
const SHELL_BUDGET: Duration = Duration::from_millis(40);
/// How long record-tree may walk.
const RECORD_BUDGET: Duration = Duration::from_secs(5);
/// How long an action waits for the overlay to say it is gone.
const HIDE_WAIT: Duration = Duration::from_millis(400);
/// The longest a session waits after a click for a second Return before
/// it puts the pointer back without the overlay saying so.
const LINGER_MAX: Duration = Duration::from_millis(1000);
/// How long a push to a listening overlay may block.
const LISTENER_WRITE: Duration = Duration::from_millis(100);
/// Sessions nobody finished are dropped after this.
const SESSION_LIFE: Duration = Duration::from_secs(120);

struct Entry {
    session: Session,
    /// None for a grid started with no window focused.
    window: Option<Window>,
    monitor: Monitor,
    status: String,
    test: bool,
    hidden: bool,
    created: Instant,
    timing: Timing,
    /// Clicks decided and not yet done; their threads end the session.
    pending: u8,
    /// The first click is done.
    clicked: bool,
    /// The session was closed while a click was pending.
    closing: bool,
    /// Where the pointer goes back to when the session ends.
    restore: Option<(Monitor, (i32, i32))>,
}

struct Daemon {
    socket: PathBuf,
    sessions: Mutex<HashMap<String, Entry>>,
    /// Signalled whenever a session's hidden, clicked, pending or closing
    /// changes, or it ends.
    changed: Condvar,
    next: AtomicU64,
    atspi: Arc<sources::Atspi>,
    shell: Arc<sources::Shell>,
    sources: Vec<Arc<dyn sources::Source>>,
    /// The overlay listening for sessions (SPEC section 7, `listen`).
    listeners: Mutex<listeners::Listeners<UnixStream>>,
    /// The uinput keyboard for Control-clicks, made on first use.
    keyboard: Mutex<Option<keyboard::Keyboard>>,
}

/// The settings, read at each session so an edit takes effect at once; a
/// file that does not parse gives the defaults and says why.
fn settings() -> (Config, Option<String>) {
    let dir = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")));
    let Some(path) = dir.map(|d| d.join("thodu").join("config.toml")) else {
        return (Config::default(), None);
    };
    match std::fs::read_to_string(&path) {
        Ok(text) => match config::parse(&text) {
            Ok(c) => (c, None),
            Err(e) => (
                Config::default(),
                Some(format!("{}: {e}; using the defaults", path.display())),
            ),
        },
        Err(_) => (Config::default(), None),
    }
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

    /// Targets of `window` from every source within its budget
    /// (REQ-SRC-05), and a status line.
    fn collect(&self, window: &Window, monitor: &Monitor) -> Res<(Vec<Target>, String)> {
        let got = sources::collect(&self.sources, window, monitor);
        Ok((got.targets, got.status.join("; ")))
    }

    /// Every frame of a window's process, for record-tree.
    fn record(&self, window: Option<String>) -> Res<Reply> {
        let window = self.window(window.as_deref())?;
        let monitor = hypr::monitor_of(&window).map_err(internal)?;
        let mut bus = self.atspi.bus.lock().map_err(|_| internal("bus lock"))?;
        if bus.is_none() {
            *bus = Some(atspi::Bus::connect().map_err(|e| fail(ErrorKind::NoTree, e))?);
        }
        let reader = bus.as_ref().ok_or_else(|| internal("no bus"))?;
        let mut walk = atspi::Walk::new(RECORD_BUDGET);
        let snapshot = atspi::record(reader, &window, &monitor, &mut walk);
        let cut = if walk.truncated { ", cut short" } else { "" };
        Ok(Reply {
            status: Some(format!("{} application(s){cut}", snapshot.apps.len())),
            snapshot: Some(serde_json::to_value(&snapshot).map_err(|e| internal(e.to_string()))?),
            ..Reply::ok()
        })
    }

    fn start(&self, window: Option<String>, mode: &str, overlay: bool, test: bool) -> Res<Reply> {
        let grid = mode == "grid";
        let named = window.is_some();
        let window = match self.window(window.as_deref()) {
            Ok(w) => Some(w),
            Err(_) if grid => None,
            Err(e) => return Err(e),
        };
        // The grid covers the focused monitor unless a window is named; it
        // needs no window, as on an empty workspace.
        let (window, monitor) = if grid && !named {
            let focused = hypr::focused_monitor().map_err(internal)?;
            (grid_window(window, focused.id), focused.monitor)
        } else {
            let monitor = match &window {
                Some(w) => hypr::monitor_of(w),
                None => hypr::focused_monitor().map(|m| m.monitor),
            }
            .map_err(internal)?;
            (window, monitor)
        };
        let began = Instant::now();
        let (targets, status) = match &window {
            Some(w) if !grid => self.collect(w, &monitor)?,
            _ => (Vec::new(), "the grid".to_string()),
        };
        let collect_ms = began.elapsed().as_millis() as u64;
        let (settings, problem) = settings();
        let status = match problem {
            Some(p) => format!("{status}; {p}"),
            None => status,
        };
        let mut session = Session::new(targets, settings.labels_only).with_area(monitor.logical());
        if grid {
            session.open_grid();
        }
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
                    status: status.clone(),
                    test,
                    hidden: false,
                    created: Instant::now(),
                    timing: Timing {
                        collect_ms,
                        drawn_at_us: None,
                    },
                    pending: 0,
                    clicked: false,
                    closing: false,
                    restore: None,
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
        self.listeners
            .lock()
            .is_ok_and(|mut l| l.push(line.as_bytes()))
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
            searching: s.searching,
            label_prefix: s.label_prefix.clone(),
            generation: s.generation,
            linger: s.state == State::Acting && s.act.as_ref().is_some_and(|a| a.linger),
        }
    }

    /// The session's targets with their labels and where the labels sit.
    fn labeled(entry: &Entry) -> Vec<LabeledTarget> {
        let s = &entry.session;
        let area = entry.monitor.logical();
        s.targets
            .iter()
            .zip(&s.labels)
            .map(|(t, label)| LabeledTarget {
                id: t.id.clone(),
                bounds: t.bounds,
                role: t.role,
                text: t.text.clone(),
                source: t.source,
                label: label.clone(),
                placed: labels::place(&t.bounds, protocol::label_size(label.len()), &area),
            })
            .collect()
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
        Ok(Reply {
            session: Some(id.to_string()),
            monitor: Some(MonitorInfo {
                name: entry.monitor.name.clone(),
                area: entry.monitor.logical(),
                scale: entry.monitor.scale,
            }),
            targets: Some(Self::labeled(entry)),
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
        let generation = entry.session.generation;
        let at = entry.created.elapsed().as_millis() as u64;
        let act = entry.session.key_at(key, at);
        let view = Self::view(entry);
        // New targets, from the grid, go with the reply.
        let targets = (entry.session.generation != generation).then(|| Self::labeled(entry));
        let closed = entry.session.state == State::Closed;
        if act.is_some() {
            entry.pending += 1;
        }
        drop(sessions);
        match act {
            Some(act) => {
                let daemon = Arc::clone(self);
                let id = id.to_string();
                thread::spawn(move || {
                    if act.again {
                        daemon.act_again(&id, &act);
                    } else {
                        daemon.act(&id, &act);
                    }
                });
            }
            None if closed => self.finish(id),
            None => {}
        }
        Ok(Reply {
            session: Some(id.to_string()),
            view: Some(view),
            targets,
            ..Reply::ok()
        })
    }

    /// Carry out `act`'s inputs on `monitor`, Control through the uinput
    /// keyboard, which is made the first time a Control-click needs it.
    fn perform(
        &self,
        monitor: &Monitor,
        act: &Act,
        restore: Option<(&Monitor, (i32, i32))>,
    ) -> Result<(), String> {
        let mut ctrl = |down: bool| -> Result<(), String> {
            let mut keyboard = self
                .keyboard
                .lock()
                .map_err(|_| "keyboard lock".to_string())?;
            if keyboard.is_none() {
                *keyboard = Some(keyboard::Keyboard::open()?);
            }
            keyboard.as_mut().ok_or("no keyboard")?.ctrl(down)
        };
        pointer::perform(monitor, &input::plan(act), restore, &mut ctrl)
    }

    /// The first click of a session (REQ-ACT-01, REQ-ACT-02, REQ-ACT-03):
    /// wait for the overlay to be out of the way, check the window has not
    /// moved, and click. A moved window gets a fresh session instead. A
    /// lingering click leaves the pointer where it is until a second Return
    /// or the end of the wait; any other goes back at once.
    fn act(&self, id: &str, act: &Act) {
        let Ok(sessions) = self.sessions.lock() else {
            return;
        };
        let Ok((sessions, _)) = self.changed.wait_timeout_while(sessions, HIDE_WAIT, |s| {
            s.get(id).is_some_and(|e| !e.hidden)
        }) else {
            return;
        };
        let Some((window, monitor, test, grid)) = sessions.get(id).map(|e| {
            // Grid and bar targets are points on the monitor, not in the
            // window, so the window moving does not make them stale.
            let grid = e.session.targets[act.target].source != Source::Atspi;
            (e.window.clone(), e.monitor.clone(), e.test, grid)
        }) else {
            return;
        };
        drop(sessions);
        if let Some(window) = window.filter(|_| !grid) {
            let now = hypr::window(&window.address).ok().flatten();
            if is_stale(&window, now.as_ref()) {
                eprintln!("thodud: {id}: the window moved since collection; collecting again");
                self.remove(id);
                if let Err(e) = self.start(Some(window.address.clone()), "hints", true, test) {
                    eprintln!("thodud: {id}: {}", e.message.unwrap_or_default());
                }
                return;
            }
        }
        let restore = hypr::cursor().ok().and_then(|p| {
            hypr::monitors()
                .ok()?
                .into_iter()
                .map(|m| m.monitor)
                .find(|m| m.logical().contains_point(p.0, p.1))
                .map(|m| (m, p))
        });
        // A test session never takes GG's focus, so it always puts the
        // pointer back.
        let restore = restore.filter(|(_, p)| {
            test || input::should_restore(
                hypr::follows_mouse(),
                hypr::window_at(p.0, p.1).as_deref(),
                hypr::window_at(act.point.0, act.point.1).as_deref(),
            )
        });
        let now = if act.linger {
            None
        } else {
            restore.as_ref().map(|(m, p)| (m, *p))
        };
        if let Err(e) = self.perform(&monitor, act, now) {
            eprintln!("thodud: {id}: the click failed: {e}");
        }
        let Ok(mut sessions) = self.sessions.lock() else {
            return;
        };
        let Some(entry) = sessions.get_mut(id) else {
            return;
        };
        entry.clicked = true;
        entry.pending = entry.pending.saturating_sub(1);
        if act.linger {
            entry.restore = restore;
        }
        let wait = act.linger && entry.pending == 0 && !entry.closing;
        self.changed.notify_all();
        if wait {
            // The overlay closes after its wait and says so; this is the
            // backstop if it never does.
            let Ok((sessions, _)) = self.changed.wait_timeout_while(sessions, LINGER_MAX, |s| {
                s.get(id).is_some_and(|e| e.pending == 0 && !e.closing)
            }) else {
                return;
            };
            if sessions.get(id).is_some_and(|e| e.pending > 0) {
                return;
            }
            drop(sessions);
        } else if entry.pending > 0 {
            // A second click is waiting for this one; it finishes.
            return;
        } else {
            drop(sessions);
        }
        self.finish(id);
    }

    /// The second click of a double click: where the first one was, once it
    /// has happened; then the pointer goes back.
    fn act_again(&self, id: &str, act: &Act) {
        let Ok(sessions) = self.sessions.lock() else {
            return;
        };
        let Ok((sessions, _)) = self.changed.wait_timeout_while(sessions, LINGER_MAX, |s| {
            s.get(id).is_some_and(|e| !e.clicked)
        }) else {
            return;
        };
        let Some(monitor) = sessions.get(id).map(|e| e.monitor.clone()) else {
            return;
        };
        drop(sessions);
        if let Err(e) = self.perform(&monitor, act, None) {
            eprintln!("thodud: {id}: the second click failed: {e}");
        }
        if let Ok(mut sessions) = self.sessions.lock()
            && let Some(entry) = sessions.get_mut(id)
        {
            entry.pending = entry.pending.saturating_sub(1);
        }
        self.finish(id);
    }

    fn remove(&self, id: &str) {
        if let Ok(mut sessions) = self.sessions.lock() {
            sessions.remove(id);
        }
        self.changed.notify_all();
    }

    /// End a session: at once, putting the pointer back if a click moved
    /// it, or, while a click is still to come, once that click is done.
    fn finish(&self, id: &str) {
        let restore = {
            let Ok(mut sessions) = self.sessions.lock() else {
                return;
            };
            let Some(entry) = sessions.get_mut(id) else {
                return;
            };
            if entry.pending > 0 {
                entry.closing = true;
                self.changed.notify_all();
                return;
            }
            sessions.remove(id).and_then(|e| e.restore)
        };
        self.changed.notify_all();
        if let Some((monitor, point)) = restore
            && let Err(e) = pointer::move_to(&monitor, point)
        {
            eprintln!("thodud: {id}: the pointer did not go back: {e}");
        }
    }

    /// The overlay is out of the way: a pending click may run. A session
    /// that is not acting has nothing to wait for and ends.
    fn hidden(&self, id: &str) -> Res<Reply> {
        let acting = {
            let mut sessions = self.sessions.lock().map_err(|_| internal("session lock"))?;
            match sessions.get_mut(id) {
                Some(entry) => {
                    entry.hidden = true;
                    entry.session.state == State::Acting || entry.pending > 0
                }
                None => true,
            }
        };
        self.changed.notify_all();
        if !acting {
            self.finish(id);
        }
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
            .map(LabeledTarget::unlabeled)
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
                mode,
                window,
                overlay,
                test,
            } => self.start(window, &mode, overlay, test),
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
            Request::Record { window } => self.record(window),
            Request::ShellItems { screen, items } => {
                if let Ok(mut known) = self.shell.items.lock() {
                    known.insert(screen, items);
                }
                Ok(Reply::ok())
            }
            Request::Close { session } => {
                self.finish(&session);
                Ok(Reply::ok())
            }
        };
        result.unwrap_or_else(|e| *e)
    }
}

/// The window a grid with no window named belongs to: the active window
/// when it is on the focused monitor the grid covers, else none.
fn grid_window(active: Option<Window>, focused_monitor: i64) -> Option<Window> {
    active.filter(|w| w.monitor == focused_monitor)
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
                listeners.add(stream);
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
    let atspi = Arc::new(sources::Atspi {
        bus: Mutex::new(None),
        woken: Mutex::new(HashSet::new()),
        budget: ATSPI_BUDGET,
    });
    let shell = Arc::new(sources::Shell {
        items: Mutex::new(HashMap::new()),
        budget: SHELL_BUDGET,
    });
    let daemon = Arc::new(Daemon {
        socket,
        sessions: Mutex::new(HashMap::new()),
        changed: Condvar::new(),
        next: AtomicU64::new(1),
        atspi: Arc::clone(&atspi),
        shell: Arc::clone(&shell),
        sources: vec![atspi, shell],
        listeners: Mutex::new(listeners::Listeners::new()),
        keyboard: Mutex::new(None),
    });
    for stream in listener.incoming().flatten() {
        let daemon = Arc::clone(&daemon);
        thread::spawn(move || serve(&daemon, stream));
    }
}

#[cfg(test)]
mod tests {
    use thodu_core::model::Window;

    use super::grid_window;

    fn on(monitor: i64) -> Window {
        Window {
            address: "0x1".into(),
            class: "app".into(),
            title: String::new(),
            pid: 1,
            at: (0, 0),
            size: (10, 10),
            workspace: 1,
            monitor,
        }
    }

    // REQ-GRID-01: thodu grid covers the focused monitor; an active window
    // on another one does not move it there.
    #[test]
    fn req_grid_01_the_grid_covers_the_focused_monitor() {
        assert_eq!(grid_window(Some(on(0)), 0), Some(on(0)));
        assert_eq!(grid_window(Some(on(1)), 0), None);
        assert_eq!(grid_window(None, 0), None);
    }
}
