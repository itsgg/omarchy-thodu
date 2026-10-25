//! thodud, Thodu's service: it collects targets, owns sessions, acts on the
//! chosen target and answers the IPC of SPEC section 7 on
//! $XDG_RUNTIME_DIR/thodu.sock.

mod atspi;
mod focus;
mod hypr;
mod image;
mod keyboard;
mod listeners;
mod overlay;
mod pointer;
mod sources;
mod wheel;

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use thodu_core::config::{self, Config};
use thodu_core::input;
use thodu_core::labels;
use thodu_core::model::{ElementCheck, Monitor, Owner, Rect, Source, Target, Window};
use thodu_core::protocol::{
    self, Envelope, ErrorKind, LabeledTarget, MonitorInfo, OpenEvent, Reply, Request, Timing, View,
};
use thodu_core::session::{Act, Key, ScrollAction, Session, State};
use thodu_core::stale::target_is_stale;

/// How long the accessibility source may take (REQ-SRC-05, provisional).
const ATSPI_BUDGET: Duration = Duration::from_millis(80);
/// How long the bar source may take (REQ-SRC-05, provisional).
const SHELL_BUDGET: Duration = Duration::from_millis(40);
/// How long screen capture and visual detection may take (REQ-SRC-05).
const IMAGE_BUDGET: Duration = Duration::from_millis(180);
/// How long the visual source waits for the overlay's first drawn frame.
const FALLBACK_DRAW_WAIT: Duration = Duration::from_millis(500);
/// How long record-tree may walk.
const RECORD_BUDGET: Duration = Duration::from_secs(5);
/// How long an action waits for the overlay to say it is gone.
const HIDE_WAIT: Duration = Duration::from_millis(400);
/// Wait briefly for an Omarchy bar command to report a workspace switch.
const WORKSPACE_CHANGE_WAIT: Duration = Duration::from_millis(150);
/// Avoid reconnecting to Hyprland's JSON socket in a busy loop.
const WORKSPACE_CHANGE_POLL: Duration = Duration::from_millis(25);
/// The longest a session waits after a click for a second Return before
/// it puts the pointer back without the overlay saying so.
const LINGER_MAX: Duration = Duration::from_millis(1000);
/// How long a push to a listening overlay may block.
const LISTENER_WRITE: Duration = Duration::from_millis(100);
/// Sessions nobody finished are dropped after this.
const SESSION_LIFE: Duration = Duration::from_secs(120);

struct WindowScope {
    windows: HashMap<String, Window>,
    refetch_address: Option<String>,
}

type CollectionScope = (Option<Window>, Monitor, Vec<Window>, Vec<Window>);

#[derive(Debug, PartialEq, Eq)]
enum ScrollFailure {
    Stale,
    Input(String),
}

impl From<String> for ScrollFailure {
    fn from(message: String) -> Self {
        ScrollFailure::Input(message)
    }
}

struct Entry {
    session: Session,
    /// The geometry snapshot for each app window that contributed targets.
    window: Option<WindowScope>,
    monitor: Monitor,
    status: String,
    collecting: bool,
    overlay_bounds: Option<Vec<Rect>>,
    overlay: bool,
    test: bool,
    hidden: bool,
    created: Instant,
    timing: Timing,
    /// Clicks decided and not yet done; their threads end the session.
    pending: u8,
    /// The first click is done.
    clicked: bool,
    /// The click changed this monitor's active workspace.
    workspace_changed: bool,
    /// The session was closed while a click was pending.
    closing: bool,
    /// Where the pointer goes back to when the session ends.
    restore: Option<(Monitor, (i32, i32))>,
    /// The pointer position to restore when the active scroll hold ends.
    scroll_restore: Option<(Monitor, (i32, i32))>,
    /// A release can arrive before the pass-through-ready wheel event.
    scroll_released: bool,
}

#[derive(Debug, PartialEq, Eq)]
struct SessionOptions {
    overlay: bool,
    test: bool,
}

fn refresh_options(entry: &Entry) -> SessionOptions {
    SessionOptions {
        overlay: entry.overlay,
        test: entry.test,
    }
}

fn start_refreshed_session<R>(
    options: SessionOptions,
    refetch: Option<String>,
    start: impl FnOnce(Option<String>, &str, &str, bool, bool) -> R,
) -> R {
    start(refetch, "visible", "hints", options.overlay, options.test)
}

struct Daemon {
    socket: PathBuf,
    shutdown: AtomicBool,
    sessions: Mutex<HashMap<String, Entry>>,
    /// Signalled when a session's first draw or hidden, clicked, pending or
    /// closing state changes, or it ends.
    changed: Condvar,
    next: AtomicU64,
    atspi: Arc<sources::Atspi>,
    shell: Arc<sources::Shell>,
    sources: Vec<Arc<dyn sources::Source>>,
    /// The overlay listening for sessions (SPEC section 7, `listen`).
    listeners: Mutex<listeners::Listeners<UnixStream>>,
    /// The uinput keyboard for Control-clicks, made on first use.
    keyboard: Mutex<Option<keyboard::Keyboard>>,
    /// The uinput wheel for scroll mode, made on first use.
    wheel: Mutex<Option<wheel::Wheel>>,
    /// The compositor has one pointer: clicks and scroll actions take turns
    /// with it, each from its last check until its last input.
    pointer_action: Mutex<()>,
    focus_records: Option<PathBuf>,
    dispatch_syntax: Option<focus::DispatchSyntax>,
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

fn wait_for_workspace_change<F>(before: i64, wait: Duration, mut current: F) -> bool
where
    F: FnMut() -> Option<i64>,
{
    let deadline = Instant::now() + wait;
    loop {
        if current().is_some_and(|workspace| workspace != before) {
            return true;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return false;
        }
        thread::sleep(WORKSPACE_CHANGE_POLL.min(remaining));
    }
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

    /// The default scope is every mapped client shown on the focused
    /// window's monitor; an explicit address keeps the single-window scope.
    fn collection_scope(&self, address: Option<&str>, scope: &str) -> Res<CollectionScope> {
        if !matches!(scope, "visible" | "focused") {
            return Err(fail(
                ErrorKind::BadRequest,
                format!("unknown window scope {scope}; use visible or focused"),
            ));
        }
        let active = match address {
            Some(address) => Some(self.window(Some(address))?),
            None => hypr::active_window().map_err(internal)?,
        };
        let Some(active) = active else {
            let focused = hypr::focused_monitor().map_err(internal)?;
            let visible_windows = hypr::visible_windows(focused.id).map_err(internal)?;
            return Ok(no_active_window_scope(
                scope,
                focused.monitor,
                visible_windows,
            ));
        };
        let focused_address = if address.is_some() {
            hypr::active_window()
                .map_err(internal)?
                .map(|window| window.address)
        } else {
            Some(active.address.clone())
        };
        let monitor = hypr::monitor_of(&active).map_err(internal)?;
        let visible_windows = hypr::visible_windows(active.monitor).map_err(internal)?;
        let visible = if address.is_some() || scope == "focused" {
            Vec::new()
        } else {
            visible_windows.clone()
        };
        let windows = scoped_windows(
            &active,
            visible,
            address.is_some(),
            scope,
            focused_address.as_deref(),
        );
        Ok((Some(active), monitor, windows, visible_windows))
    }

    /// Targets of `window` from every source within its budget
    /// (REQ-SRC-05), and a status line.
    fn collect(
        &self,
        windows: &[Window],
        visible_windows: &[Window],
        monitor: &Monitor,
    ) -> Res<(Vec<Target>, String)> {
        let got = sources::collect_visible_scoped(&self.sources, windows, visible_windows, monitor);
        Ok((got.targets, got.status.join("; ")))
    }

    fn collect_primary(
        &self,
        windows: &[Window],
        visible_windows: &[Window],
        monitor: &Monitor,
    ) -> (Vec<Target>, String, bool) {
        let got = sources::collect_primary_visible_scoped(
            &self.sources,
            windows,
            visible_windows,
            monitor,
        );
        (got.targets, got.status.join("; "), got.fallback_requested)
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

    fn start(
        self: &Arc<Self>,
        window: Option<String>,
        scope: &str,
        mode: &str,
        overlay: bool,
        test: bool,
    ) -> Res<Reply> {
        let grid = mode == "grid";
        let scroll = mode == "scroll";
        if !grid && !scroll && mode != "hints" {
            return Err(fail(ErrorKind::BadRequest, format!("unknown mode {mode}")));
        }
        let named = window.is_some();
        let (_window, monitor, windows, visible_windows) = if grid && !named {
            let focused = hypr::focused_monitor().map_err(internal)?;
            let window = self.window(None).ok();
            (
                grid_window(window, focused.id),
                focused.monitor,
                Vec::new(),
                Vec::new(),
            )
        } else if grid {
            let window = self.window(window.as_deref())?;
            let monitor = hypr::monitor_of(&window).map_err(internal)?;
            (Some(window), monitor, Vec::new(), Vec::new())
        } else {
            self.collection_scope(window.as_deref(), scope)?
        };
        let began = Instant::now();
        let (targets, status, collecting) = if grid {
            (Vec::new(), "the grid".to_string(), false)
        } else if scroll {
            let (targets, status) =
                self.atspi
                    .collect_scroll_areas(&windows, &visible_windows, &monitor);
            (targets, status, false)
        } else {
            self.collect_primary(&windows, &visible_windows, &monitor)
        };
        let collect_ms = began.elapsed().as_millis() as u64;
        let (settings, problem) = settings();
        let mut status = match problem {
            Some(p) => format!("{status}; {p}"),
            None => status,
        };
        if collecting {
            status.push_str("; collecting visual targets");
        }
        let session = if scroll {
            Session::new_scroll(targets).with_config(&settings)
        } else if collecting {
            Session::new(targets, settings.labels_only)
                .with_config(&settings)
                .reserving_label_prefix('z')
                .with_area(monitor.logical())
        } else {
            Session::begin(targets, monitor.logical(), settings.labels_only, grid)
                .with_config(&settings)
        };
        let status = with_grid_note(status, &session, grid);
        let id = format!("s{}", self.next.fetch_add(1, Ordering::Relaxed));
        let fallback_monitor = monitor.clone();
        {
            let mut sessions = self.sessions.lock().map_err(|_| internal("session lock"))?;
            sessions.retain(|_, e| e.created.elapsed() < SESSION_LIFE);
            sessions.insert(
                id.clone(),
                Entry {
                    session,
                    window: (!windows.is_empty()).then(|| WindowScope {
                        windows: windows
                            .iter()
                            .cloned()
                            .map(|window| (window.address.clone(), window))
                            .collect(),
                        refetch_address: restart_address(named, scope, &windows),
                    }),
                    monitor,
                    status: status.clone(),
                    collecting,
                    overlay_bounds: None,
                    overlay,
                    test,
                    hidden: false,
                    created: Instant::now(),
                    timing: Timing {
                        collect_ms,
                        drawn_at_us: None,
                    },
                    pending: 0,
                    clicked: false,
                    workspace_changed: false,
                    closing: false,
                    restore: None,
                    scroll_restore: None,
                    scroll_released: true,
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
        if collecting {
            let daemon = Arc::clone(self);
            let sources = self.sources.clone();
            let windows = windows.clone();
            let visible_windows = visible_windows.clone();
            let monitor = fallback_monitor;
            let fallback_id = id.clone();
            thread::spawn(move || {
                if overlay {
                    daemon.wait_for_first_draw(&fallback_id, FALLBACK_DRAW_WAIT);
                }
                let Some(visual_exclusions) = daemon.fallback_exclusions(&fallback_id, overlay)
                else {
                    return;
                };
                let fallback = sources::collect_fallback_visible_scoped(
                    &sources,
                    &windows,
                    &visible_windows,
                    &monitor,
                    &visual_exclusions,
                );
                daemon.finish_fallback(&fallback_id, fallback, &visible_windows);
            });
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

    fn drawn(&self, id: &str, overlays: Option<Vec<Rect>>) -> Res<Reply> {
        let mut sessions = self.sessions.lock().map_err(|_| internal("session lock"))?;
        if let Some(entry) = sessions.get_mut(id) {
            entry.timing.drawn_at_us.get_or_insert_with(now_us);
            entry.overlay_bounds = overlays;
        }
        self.changed.notify_all();
        Ok(Reply::ok())
    }

    fn wait_for_first_draw(&self, id: &str, wait: Duration) -> bool {
        let deadline = Instant::now() + wait;
        let Ok(mut sessions) = self.sessions.lock() else {
            return false;
        };
        loop {
            let Some(entry) = sessions.get(id) else {
                return false;
            };
            if !entry.collecting || entry.closing || entry.session.state != State::Showing {
                return false;
            }
            if entry.timing.drawn_at_us.is_some() {
                return true;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return false;
            }
            let Ok((next, result)) = self.changed.wait_timeout(sessions, remaining) else {
                return false;
            };
            sessions = next;
            if result.timed_out() {
                return false;
            }
        }
    }

    fn fallback_exclusions(&self, id: &str, overlay_expected: bool) -> Option<Vec<Target>> {
        let mut sessions = self.sessions.lock().ok()?;
        let entry = sessions.get_mut(id)?;
        if !entry.collecting {
            return None;
        }
        if entry.closing || entry.session.state != State::Showing {
            stop_collecting(entry);
            return None;
        }
        let overlay_bounds = if !overlay_expected {
            Some(&[][..])
        } else {
            entry.overlay_bounds.as_deref()
        };
        Some(image_fallback_exclusions(
            &entry.session,
            &entry.monitor,
            &entry.status,
            overlay_bounds,
        ))
    }

    fn finish_fallback(&self, id: &str, fallback: sources::Collected, visible_windows: &[Window]) {
        let Ok(mut sessions) = self.sessions.lock() else {
            return;
        };
        let Some(entry) = sessions.get_mut(id) else {
            return;
        };
        merge_fallback(entry, fallback, visible_windows);
    }

    fn view(entry: &Entry) -> View {
        let s = &entry.session;
        let id = |i: usize| s.targets[i].id.clone();
        View {
            state: s.state,
            visible: s.visible().into_iter().map(id).collect(),
            focused: s.focused.map(id),
            labels_only: s.labels_only,
            label_selected: s.label_selected,
            search: s.search.clone(),
            searching: s.searching,
            label_prefix: s.label_prefix.clone(),
            generation: s.generation,
            collecting: entry.collecting,
            linger: s.state == State::Acting && s.act.as_ref().is_some_and(|a| a.linger),
            scroll_mode: s.scroll_mode,
            status_display: s.status_display,
            status_persistent: status_persistent(
                &entry.status,
                entry.collecting,
                s.targets.is_empty(),
            ),
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
                actions: t.actions.clone(),
                window: t.window.clone(),
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

    fn key(
        self: &Arc<Self>,
        id: &str,
        name: &str,
        shift: bool,
        ctrl: bool,
        expected_generation: u32,
    ) -> Res<Reply> {
        let key = parse_key(name, shift, ctrl)
            .ok_or_else(|| fail(ErrorKind::BadRequest, format!("unknown key {name}")))?;
        let mut sessions = self.sessions.lock().map_err(|_| internal("session lock"))?;
        let entry = sessions
            .get_mut(id)
            .ok_or_else(|| fail(ErrorKind::BadRequest, format!("no session {id}")))?;
        // A chord with Control is a shortcut meant for an app; only Return,
        // for a Control click, and Escape take Control.
        let chord = ctrl && !matches!(key, Key::Return { .. } | Key::Escape);
        if (expected_generation != entry.session.generation && key != Key::Escape) || chord {
            return Ok(Reply {
                session: Some(id.to_string()),
                targets: Some(Self::labeled(entry)),
                view: Some(Self::view(entry)),
                status: Some(entry.status.clone()),
                timing: Some(entry.timing.clone()),
                ..Reply::ok()
            });
        }
        if entry.collecting && key != Key::Escape {
            return Err(fail(ErrorKind::Busy, "visual targets are still collecting"));
        }
        let generation = entry.session.generation;
        let at = entry.created.elapsed().as_millis() as u64;
        let cancel_unstarted_scroll = key == Key::Escape
            && entry.session.scroll_mode
            && entry.session.state == State::Acting
            && entry.session.pending_scroll_action().is_some();
        let act = entry.session.key_at(key, at);
        if cancel_unstarted_scroll {
            entry.pending = entry.pending.saturating_sub(1);
            entry.scroll_released = true;
        }
        let scroll = entry.session.pending_scroll_action();
        if scroll.is_some() {
            entry.scroll_released = false;
        }
        let view = Self::view(entry);
        // New targets, from the grid, go with the reply.
        let targets = (entry.session.generation != generation).then(|| Self::labeled(entry));
        let closed = entry.session.state == State::Closed;
        if act.is_some() || scroll.is_some() {
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
            None if scroll.is_some() => {}
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

    /// Make the Control keyboard before a Control-click's check: the
    /// compositor needs a moment to find a new device, which must not fall
    /// between the check and the press. A failure here is left for the click
    /// to report.
    fn prepare_keyboard(&self) {
        if let Ok(mut keyboard) = self.keyboard.lock()
            && keyboard.is_none()
        {
            *keyboard = keyboard::Keyboard::open().ok();
        }
    }

    /// The first click of a session (REQ-ACT-01, REQ-ACT-02, REQ-ACT-03):
    /// wait for the overlay to be out of the way, recheck window targets and
    /// the latest shell item geometry, and click. A changed target gets a
    /// fresh session instead. A lingering click leaves the pointer where it
    /// is until a second Return or the end of the wait; any other goes back.
    fn act(self: &Arc<Self>, id: &str, act: &Act) {
        let Ok(sessions) = self.sessions.lock() else {
            return;
        };
        let Ok((sessions, _)) = self.changed.wait_timeout_while(sessions, HIDE_WAIT, |s| {
            s.get(id).is_some_and(|e| !e.hidden)
        }) else {
            return;
        };
        let Some((window, refetch, monitor, options, target, source)) = sessions.get(id).map(|e| {
            let target = e.session.targets[act.target].clone();
            let source = target.source;
            (
                window_for_target(e.window.as_ref(), &target).cloned(),
                e.window
                    .as_ref()
                    .and_then(|scope| scope.refetch_address.clone()),
                e.monitor.clone(),
                refresh_options(e),
                target,
                source,
            )
        }) else {
            return;
        };
        drop(sessions);
        if !target.window.is_empty() && window.is_none() {
            self.remove(id);
            return;
        }
        let test = options.test;
        // The slow preparation comes before the pointer is taken: a new
        // Control keyboard needs a moment, and the restore asks Hyprland.
        if act.ctrl {
            self.prepare_keyboard();
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
        let on_bar = source == Source::Shell
            || hypr::owner_at(act.point.0, act.point.1)
                == Owner::Layer(sources::BAR_NAMESPACE.to_string());
        let workspace_before = watches_workspace(act.linger, on_bar)
            .then(|| hypr::active_workspace_id(&monitor.name))
            .flatten();
        let pressed = checked_click(
            &self.pointer_action,
            || {
                target_is_current(&target, &self.shell, &monitor, || {
                    target_moved_or_covered(&target, window.as_ref(), |check| {
                        self.atspi.element_moved(check)
                    })
                })
            },
            || {
                let now = if act.linger {
                    None
                } else {
                    restore.as_ref().map(|(m, p)| (m, *p))
                };
                self.perform(&monitor, act, now)
            },
        );
        let Some(click_result) = pressed else {
            eprintln!("thodud: {id}: the target changed or became covered; collecting again");
            self.remove(id);
            let result =
                start_refreshed_session(options, refetch, |window, scope, mode, overlay, test| {
                    self.start(window, scope, mode, overlay, test)
                });
            if let Err(e) = result {
                eprintln!("thodud: {id}: {}", e.message.unwrap_or_default());
            }
            return;
        };
        if let Err(e) = &click_result {
            eprintln!("thodud: {id}: the click failed: {e}");
        }
        let workspace_changed = click_result.is_ok()
            && workspace_before.is_some_and(|before| {
                wait_for_workspace_change(before, WORKSPACE_CHANGE_WAIT, || {
                    hypr::active_workspace_id(&monitor.name)
                })
            });
        let Ok(mut sessions) = self.sessions.lock() else {
            return;
        };
        let Some(entry) = sessions.get_mut(id) else {
            return;
        };
        record_first_click(entry, click_result.is_ok(), workspace_changed);
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
    /// has happened and only while its target still owns the point; then the
    /// pointer goes back.
    fn act_again(&self, id: &str, act: &Act) {
        self.act_again_with(
            id,
            act,
            |target, window| {
                target_moved_or_covered(target, window, |check| self.atspi.element_moved(check))
            },
            |monitor| self.perform(monitor, act, None),
        );
    }

    fn act_again_with(
        &self,
        id: &str,
        act: &Act,
        moved_or_covered: impl FnOnce(&Target, Option<&Window>) -> bool,
        press: impl FnOnce(&Monitor) -> Result<(), String>,
    ) {
        let Ok(sessions) = self.sessions.lock() else {
            return;
        };
        // The first click's attempt ends when its pending count is gone.
        let Ok((sessions, _)) = self.changed.wait_timeout_while(sessions, LINGER_MAX, |s| {
            s.get(id)
                .is_some_and(|e| !e.clicked && !e.closing && e.pending > 1)
        }) else {
            return;
        };
        let Some((clicked, closing, target, window, monitor)) = sessions.get(id).map(|e| {
            let target = e.session.targets[act.target].clone();
            let window = window_for_target(e.window.as_ref(), &target).cloned();
            (e.clicked, e.closing, target, window, e.monitor.clone())
        }) else {
            return;
        };
        drop(sessions);
        if !clicked || closing {
            eprintln!(
                "thodud: {id}: the first click never happened or the session closed; no second click"
            );
        } else {
            let pressed = checked_click(
                &self.pointer_action,
                || {
                    (target.window.is_empty() || window.is_some())
                        && target_is_current(&target, &self.shell, &monitor, || {
                            moved_or_covered(&target, window.as_ref())
                        })
                },
                || press(&monitor),
            );
            match pressed {
                None => eprintln!(
                    "thodud: {id}: the target changed or became covered after the first click; no second click"
                ),
                Some(Err(e)) => eprintln!("thodud: {id}: the second click failed: {e}"),
                Some(Ok(())) => {}
            }
        }
        if let Ok(mut sessions) = self.sessions.lock()
            && let Some(entry) = sessions.get_mut(id)
        {
            entry.pending = entry.pending.saturating_sub(1);
        }
        self.finish(id);
    }

    fn scroll_ready(&self, id: &str) -> Res<Reply> {
        self.scroll_ready_with(id, |target, window, monitor, action, _test| {
            self.perform_scroll_event(target, window, monitor, action)
        })
    }

    fn scroll_ready_with<F>(&self, id: &str, perform: F) -> Res<Reply>
    where
        F: FnOnce(
            &Target,
            Option<&Window>,
            &Monitor,
            ScrollAction,
            bool,
        ) -> Result<(), ScrollFailure>,
    {
        let action = {
            let mut sessions = self.sessions.lock().map_err(|_| internal("session lock"))?;
            let entry = sessions
                .get_mut(id)
                .ok_or_else(|| fail(ErrorKind::BadRequest, format!("no session {id}")))?;
            entry.session.take_scroll_action().ok_or_else(|| {
                fail(
                    ErrorKind::BadRequest,
                    "no scroll action is waiting for pass-through",
                )
            })?
        };
        self.scroll_with(id, action, perform)
    }

    fn scroll_stop(&self, id: &str) -> Res<Reply> {
        self.scroll_stop_with(id, |monitor, point| {
            under_pointer(&self.pointer_action, || pointer::move_to(monitor, point))
        })
    }

    fn scroll_stop_with<F>(&self, id: &str, restore_pointer: F) -> Res<Reply>
    where
        F: FnOnce(&Monitor, (i32, i32)) -> Result<(), String>,
    {
        let restore = {
            let mut sessions = self.sessions.lock().map_err(|_| internal("session lock"))?;
            let entry = sessions
                .get_mut(id)
                .ok_or_else(|| fail(ErrorKind::BadRequest, format!("no session {id}")))?;
            entry.scroll_released = true;
            if entry.pending == 0 {
                entry.scroll_restore.take()
            } else {
                None
            }
        };
        let restore_error = restore
            .as_ref()
            .and_then(|(monitor, point)| restore_pointer(monitor, *point).err());
        if let Some(error) = restore_error {
            let mut sessions = self.sessions.lock().map_err(|_| internal("session lock"))?;
            if let Some(entry) = sessions.get_mut(id) {
                entry.status = format!("scroll pointer restore failed: {error}");
                return Ok(Reply {
                    session: Some(id.to_string()),
                    view: Some(Self::view(entry)),
                    status: Some(entry.status.clone()),
                    ..Reply::ok()
                });
            }
        }
        Ok(Reply::ok())
    }

    /// Send one wheel action after the overlay has made its pointer region
    /// transparent. Tests inject the wheel input through `perform`.
    fn scroll_with<F>(&self, id: &str, action: ScrollAction, perform: F) -> Res<Reply>
    where
        F: FnOnce(
            &Target,
            Option<&Window>,
            &Monitor,
            ScrollAction,
            bool,
        ) -> Result<(), ScrollFailure>,
    {
        let (target, window, monitor, test, capture_pointer) = {
            let sessions = self.sessions.lock().map_err(|_| internal("session lock"))?;
            let entry = sessions
                .get(id)
                .ok_or_else(|| fail(ErrorKind::BadRequest, format!("no session {id}")))?;
            let target = entry.session.targets.get(action.target).cloned();
            let window = target
                .as_ref()
                .and_then(|target| window_for_target(entry.window.as_ref(), target))
                .cloned();
            (
                target,
                window,
                entry.monitor.clone(),
                entry.test,
                entry.scroll_restore.is_none(),
            )
        };

        if capture_pointer
            && let Some(restore) = target
                .as_ref()
                .and_then(|target| scroll_restore_candidate(target, test))
        {
            let mut sessions = self.sessions.lock().map_err(|_| internal("session lock"))?;
            if let Some(entry) = sessions.get_mut(id)
                && entry.scroll_restore.is_none()
            {
                entry.scroll_restore = Some(restore);
            }
        }

        let result = if let Some(target) = target.as_ref() {
            perform(target, window.as_ref(), &monitor, action, test)
        } else {
            Err(ScrollFailure::Stale)
        };

        let mut sessions = self.sessions.lock().map_err(|_| internal("session lock"))?;
        let entry = sessions
            .get_mut(id)
            .ok_or_else(|| fail(ErrorKind::BadRequest, format!("no session {id}")))?;
        entry.pending = entry.pending.saturating_sub(1);
        let closing = entry.closing;
        let failed = result.is_err();
        if failed {
            entry.scroll_released = true;
        }
        let restore = if failed || entry.scroll_released || closing {
            entry.scroll_restore.take()
        } else {
            None
        };
        if !closing {
            entry.session.resume_after_scroll();
            match result {
                Ok(()) => {}
                Err(ScrollFailure::Stale) => {
                    entry.session.clear_scroll_targets();
                    entry.status =
                        "scroll target moved, closed or became covered; restart scroll mode".into();
                }
                Err(ScrollFailure::Input(message)) => {
                    entry.status = format!("scroll failed: {message}")
                }
            }
        }
        let mut reply = if closing {
            None
        } else {
            Some(Reply {
                session: Some(id.to_string()),
                targets: Some(Self::labeled(entry)),
                view: Some(Self::view(entry)),
                status: Some(entry.status.clone()),
                timing: Some(entry.timing.clone()),
                ..Reply::ok()
            })
        };
        drop(sessions);
        self.changed.notify_all();
        if let Some((monitor, point)) = restore
            && let Err(error) =
                under_pointer(&self.pointer_action, || pointer::move_to(&monitor, point))
        {
            if !closing {
                let mut sessions = self.sessions.lock().map_err(|_| internal("session lock"))?;
                if let Some(entry) = sessions.get_mut(id) {
                    entry.status =
                        format!("{}; scroll pointer restore failed: {error}", entry.status);
                    reply = Some(Reply {
                        session: Some(id.to_string()),
                        targets: Some(Self::labeled(entry)),
                        view: Some(Self::view(entry)),
                        status: Some(entry.status.clone()),
                        timing: Some(entry.timing.clone()),
                        ..Reply::ok()
                    });
                }
            } else {
                eprintln!("thodud: {id}: the pointer did not go back after scrolling: {error}");
            }
        }
        if closing {
            self.finish(id);
            return Err(fail(ErrorKind::BadRequest, "scroll session was closed"));
        }
        reply.ok_or_else(|| internal("scroll action reply was not created"))
    }

    fn perform_scroll_event(
        &self,
        target: &Target,
        window: Option<&Window>,
        monitor: &Monitor,
        action: ScrollAction,
    ) -> Result<(), ScrollFailure> {
        if !target.window.is_empty() && self.dispatch_syntax.is_none() {
            return Err(ScrollFailure::Input(
                "Hyprland dispatch syntax is unavailable; check `hyprctl systeminfo` and restart thodud"
                    .into(),
            ));
        }
        if !target.window.is_empty() && self.focus_records.is_none() {
            return Err(ScrollFailure::Input(
                "scroll recovery storage is unavailable; check Thodu's data directory and restart thodud"
                    .into(),
            ));
        }
        let _serial = self
            .pointer_action
            .lock()
            .map_err(|_| ScrollFailure::Input("scroll action lock failed".into()))?;
        let point = target.bounds.center();
        let stale = window.is_some_and(|window| scroll_target_is_stale(target, window));
        let mut get = |address: &str| focus::get_stay_focused(address, hypr::request);
        let dispatch_syntax = self.dispatch_syntax;
        let mut set = |address: &str, value: Option<bool>| {
            match dispatch_syntax {
            Some(syntax) => focus::set_stay_focused(address, value, syntax, hypr::request),
            None => Err(
                "Hyprland dispatch syntax is unavailable; check `hyprctl systeminfo` and restart thodud"
                    .to_string(),
            ),
        }
        };
        let mut window_exists =
            |address: &str| hypr::window(address).map(|window| window.map(|window| window.pid));
        // The exclusive overlay keeps the keyboard; Hyprland gives the
        // pointer to the owning window only while it is forced (focus.rs).
        perform_scroll_event_with(
            target,
            window,
            ScrollFocus {
                records: self.focus_records.as_deref(),
                stale,
                get: &mut get,
                set: &mut set,
                window_exists: &mut window_exists,
            },
            || self.send_wheel(target, window, monitor, point, action),
        )
    }

    /// Move the pointer to `point` only when it is not already there, then
    /// send the wheel to the selected area.
    fn send_wheel(
        &self,
        target: &Target,
        window: Option<&Window>,
        monitor: &Monitor,
        point: (i32, i32),
        action: ScrollAction,
    ) -> Result<(), ScrollFailure> {
        let result = send_wheel_with(
            point,
            hypr::cursor().ok(),
            |point| pointer::move_to(monitor, point),
            |point| {
                match self.dispatch_syntax {
                Some(syntax) => hypr::refresh_cursor_focus(point, syntax),
                None => Err(
                    "Hyprland dispatch syntax is unavailable; check `hyprctl systeminfo` and restart thodud"
                        .into(),
                ),
            }
            },
            || {
                let mut wheel = self
                    .wheel
                    .lock()
                    .map_err(|_| ScrollFailure::Input("wheel lock failed".into()))?;
                if wheel.is_none() {
                    *wheel = Some(wheel::Wheel::open().map_err(ScrollFailure::Input)?);
                }
                if !target.window.is_empty()
                    && window.is_some_and(|window| scroll_target_is_stale(target, window))
                {
                    return Err(ScrollFailure::Stale);
                }
                wheel
                    .as_mut()
                    .ok_or_else(|| ScrollFailure::Input("no wheel device".into()))?
                    .emit(action)
                    .map_err(ScrollFailure::Input)
            },
        );
        if result.is_ok() {
            thread::sleep(Duration::from_millis(30));
        }
        result
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
        self.finish_with(id, pointer::move_to);
    }

    /// End a session once nothing is pending, putting the pointer back with
    /// `move_to` while holding the pointer, so no other action is mid-way.
    fn finish_with(&self, id: &str, move_to: impl Fn(&Monitor, (i32, i32)) -> Result<(), String>) {
        let restores = {
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
            sessions
                .remove(id)
                .map(|entry| [entry.restore, entry.scroll_restore])
        };
        self.changed.notify_all();
        if let Some(restores) = restores {
            for (monitor, point) in restores.into_iter().flatten() {
                if let Err(e) = under_pointer(&self.pointer_action, || move_to(&monitor, point)) {
                    eprintln!("thodud: {id}: the pointer did not go back: {e}");
                }
            }
        }
    }

    /// The overlay is out of the way: a pending click may run. A session
    /// that is not acting has nothing to wait for and ends.
    fn hidden(&self, id: &str) -> Res<Reply> {
        let mut sessions = self.sessions.lock().map_err(|_| internal("session lock"))?;
        let wait_for_click = match sessions.get_mut(id) {
            Some(entry) => {
                entry.hidden = true;
                entry.session.state == State::Acting
                    && entry.pending > 0
                    && entry.session.act.as_ref().is_some_and(|act| act.linger)
            }
            None => false,
        };
        self.changed.notify_all();

        if wait_for_click {
            let (sessions, _) = self
                .changed
                .wait_timeout_while(sessions, HIDE_WAIT, |sessions| {
                    sessions
                        .get(id)
                        .is_some_and(|entry| !entry.clicked && !entry.closing && entry.pending > 0)
                })
                .map_err(|_| internal("session lock"))?;
            let clicked = sessions.get(id).is_some_and(|entry| entry.clicked);
            let workspace_changed = sessions
                .get(id)
                .is_some_and(|entry| entry.workspace_changed);
            return Ok(Reply {
                session: Some(id.to_string()),
                clicked: Some(clicked),
                workspace_changed: Some(workspace_changed),
                ..Reply::ok()
            });
        }

        let acting = sessions
            .get(id)
            .is_some_and(|entry| entry.session.state == State::Acting || entry.pending > 0);
        drop(sessions);
        if !acting {
            self.finish(id);
        }
        Ok(Reply::ok())
    }

    fn targets(&self, window: Option<String>, scope: &str) -> Res<Reply> {
        let (_, monitor, windows, visible_windows) =
            self.collection_scope(window.as_deref(), scope)?;
        let (targets, status) = self.collect(&windows, &visible_windows, &monitor)?;
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
                scope,
                window,
                overlay,
                test,
            } => self.start(window, &scope, &mode, overlay, test),
            Request::Session { session, brief } => self.session(&session, brief),
            Request::Key {
                session,
                generation,
                key,
                shift,
                ctrl,
            } => self.key(&session, &key, shift, ctrl, generation),
            Request::ScrollReady { session } => self.scroll_ready(&session),
            Request::ScrollStop { session } => self.scroll_stop(&session),
            Request::Hidden { session } => self.hidden(&session),
            Request::Drawn { session, overlays } => self.drawn(&session, overlays),
            // serve() registers the connection once the reply is written.
            Request::Listen => Ok(Reply::ok()),
            Request::Targets { window, scope } => self.targets(window, &scope),
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
            Request::Shutdown => Ok(Reply::ok()),
        };
        result.unwrap_or_else(|e| *e)
    }
}

fn send_wheel_with<M, R, E>(
    point: (i32, i32),
    pointer_position: Option<(i32, i32)>,
    mut move_pointer: M,
    mut refresh_focus: R,
    emit: E,
) -> Result<(), ScrollFailure>
where
    M: FnMut((i32, i32)) -> Result<(), String>,
    R: FnMut((i32, i32)) -> Result<(), String>,
    E: FnOnce() -> Result<(), ScrollFailure>,
{
    if pointer_position == Some(point) {
        refresh_focus(point).map_err(ScrollFailure::Input)?;
    } else {
        move_pointer(point).map_err(ScrollFailure::Input)?;
    }
    emit()
}

type GetFocusState<'a> = dyn FnMut(&str) -> Result<bool, String> + 'a;
type SetFocusState<'a> = dyn FnMut(&str, Option<bool>) -> Result<(), String> + 'a;
type WindowIdentity<'a> = dyn FnMut(&str) -> Result<Option<u32>, String> + 'a;

struct ScrollFocus<'a> {
    records: Option<&'a std::path::Path>,
    stale: bool,
    get: &'a mut GetFocusState<'a>,
    set: &'a mut SetFocusState<'a>,
    window_exists: &'a mut WindowIdentity<'a>,
}

fn perform_scroll_event_with<B>(
    target: &Target,
    window: Option<&Window>,
    focus: ScrollFocus<'_>,
    perform: B,
) -> Result<(), ScrollFailure>
where
    B: FnOnce() -> Result<(), ScrollFailure>,
{
    let ScrollFocus {
        records,
        stale,
        get,
        set,
        window_exists,
    } = focus;
    if target.window.is_empty() {
        return perform();
    }
    if window.is_none() {
        return Err(ScrollFailure::Stale);
    }
    if stale {
        return Err(ScrollFailure::Stale);
    }
    let Some(records) = records else {
        return Err(ScrollFailure::Input(
            "scroll recovery storage is unavailable; check Thodu's data directory and restart thodud"
                .into(),
        ));
    };
    let Some(window) = window else {
        return Err(ScrollFailure::Stale);
    };

    focus::with_forced_pointer_focus(records, &target.window, window.pid, get, set, perform)
        .map_err(|failure| match failure {
            ScrollFailure::Input(_) if matches!(window_exists(&target.window), Ok(None)) => {
                ScrollFailure::Stale
            }
            other => other,
        })
}

/// REQ-SESSION-04: the status line says when the grid opened because no
/// source found anything.
fn with_grid_note(status: String, session: &Session, asked: bool) -> String {
    if asked || session.grid.is_empty() {
        status
    } else {
        format!("{status}; no targets found, so the grid is open")
    }
}

/// REQ-SESSION-08: routine collection counts expire, while incomplete or
/// failed work and empty sessions keep their explanation visible.
fn status_persistent(status: &str, collecting: bool, no_targets: bool) -> bool {
    if collecting || no_targets {
        return true;
    }
    let status = status.to_ascii_lowercase();
    [
        "missed its",
        "unavailable",
        "unregistered",
        "failed",
        "error",
        "could not",
        "budget",
        "cut short",
        "no targets found",
        "scroll target moved",
        "scroll cancelled",
        "needs /dev/uinput",
        "wheel lock failed",
        "using the defaults",
    ]
    .iter()
    .any(|marker| status.contains(marker))
}

fn no_active_window_scope(
    scope: &str,
    monitor: Monitor,
    visible_windows: Vec<Window>,
) -> CollectionScope {
    let windows = if scope == "visible" {
        visible_windows.clone()
    } else {
        Vec::new()
    };
    (None, monitor, windows, visible_windows)
}

/// Rectangles rendered by the overlay are not app controls. The image pass
/// runs after the first draw, so it removes every label chip and the status
/// toast before treating screen detections as app targets.
fn image_fallback_exclusions(
    session: &Session,
    monitor: &Monitor,
    status: &str,
    overlay_bounds: Option<&[Rect]>,
) -> Vec<Target> {
    let area = monitor.logical();
    let mut excluded = session.targets.clone();
    if let Some(overlays) = overlay_bounds {
        for (index, rect) in overlays.iter().enumerate() {
            if let Some(bounds) = rect.intersect(&area) {
                excluded.push(overlay_exclusion(format!("overlay:{index}"), bounds));
            }
        }
        return excluded;
    }

    // When the overlay does not report its first frame, conservatively
    // estimate its geometry so a slow or absent overlay does not block this session.
    for (index, (target, label)) in session.targets.iter().zip(&session.labels).enumerate() {
        excluded.push(overlay_exclusion(
            format!("overlay:label:{index}"),
            labels::place(&target.bounds, protocol::label_size(label.len()), &area),
        ));
    }
    if !session.label_selected
        && let Some(target) = session.focused.and_then(|index| session.targets.get(index))
    {
        let ring = Rect::new(
            target.bounds.x - 2,
            target.bounds.y - 2,
            target.bounds.w + 4,
            target.bounds.h + 4,
        );
        if let Some(ring) = ring.intersect(&area) {
            excluded.push(overlay_exclusion("overlay:focus-ring".into(), ring));
        }
    }
    if !status.is_empty() && !area.is_empty() {
        let max_width = (area.w - 32).clamp(1, 640);
        let cue = "visual targets loading | Esc cancels";
        let width = (status.chars().count() as i32 * 8)
            .max(cue.chars().count() as i32 * 7)
            .saturating_add(24)
            .min(max_width);
        let text_width = (width - 24).max(1);
        let status_lines = wrapped_line_count(status, text_width, 8);
        let cue_lines = wrapped_line_count(cue, text_width, 7);
        // Include padding, line spacing and a little layout slack so antialiased
        // text and borders in the toast never become image-derived targets.
        let height = (16 + status_lines * 16 + 3 + cue_lines * 14 + 16).min(area.h);
        let toast = Rect::new(
            area.x + (area.w - width) / 2,
            area.bottom() - 24 - height - 3,
            width,
            height + 6,
        );
        if let Some(toast) = toast.intersect(&area) {
            excluded.push(overlay_exclusion("overlay:status-line".into(), toast));
        }
    }
    excluded
}

fn overlay_exclusion(id: String, bounds: Rect) -> Target {
    Target {
        id,
        bounds,
        role: thodu_core::model::Role::Unknown,
        text: String::new(),
        source: Source::Shell,
        actions: Vec::new(),
        window: String::new(),
        check: None,
    }
}

fn wrapped_line_count(text: &str, width: i32, char_width: i32) -> i32 {
    let per_line = (width / char_width.max(1)).max(1) as usize;
    let mut lines = 1_usize;
    let mut current = 0;
    for word in text.split_whitespace() {
        let chars = word.chars().count();
        let chunks = chars.div_ceil(per_line);
        if current > 0 && current + 1 + chars > per_line {
            lines += chunks;
            current = chars % per_line;
        } else if chunks > 1 {
            lines += chunks - 1;
            current = chars % per_line;
        } else {
            current += usize::from(current > 0) + chars;
        }
    }
    lines as i32
}

fn merge_fallback(
    entry: &mut Entry,
    fallback: sources::Collected,
    visible_windows: &[Window],
) -> bool {
    if !entry.collecting {
        return false;
    }
    if entry.closing || entry.session.state != State::Showing {
        stop_collecting(entry);
        return false;
    }
    let mut targets = entry.session.targets.clone();
    targets.extend(fallback.targets);
    let monitor = entry.monitor.clone();
    sources::finish_visible_targets(&mut targets, visible_windows, &monitor);
    entry.session.merge_targets_preserving_labels(targets);
    if entry.session.targets.is_empty() {
        entry.session.open_grid();
    }
    let primary_status = entry
        .status
        .strip_suffix("; collecting visual targets")
        .unwrap_or(&entry.status);
    entry.status = format!("{primary_status}; {}", fallback.status.join("; "));
    entry.status = with_grid_note(entry.status.clone(), &entry.session, false);
    entry.collecting = false;
    true
}

fn stop_collecting(entry: &mut Entry) {
    entry.collecting = false;
    entry.status = entry
        .status
        .strip_suffix("; collecting visual targets")
        .unwrap_or(&entry.status)
        .to_string();
}

fn window_for_target<'a>(scope: Option<&'a WindowScope>, target: &Target) -> Option<&'a Window> {
    scope?.windows.get(&target.window)
}

fn scroll_target_is_stale(target: &Target, window: &Window) -> bool {
    let now = hypr::window(&window.address).ok().flatten();
    let owner = hypr::window_at(target.bounds.center().0, target.bounds.center().1);
    target_is_stale(target, window, now.as_ref())
        || visible_target_owner_changed(target, owner.as_deref())
}

fn scroll_restore_candidate(target: &Target, test: bool) -> Option<(Monitor, (i32, i32))> {
    let cursor = hypr::cursor().ok()?;
    let monitor = hypr::monitors()
        .ok()?
        .into_iter()
        .map(|monitor| monitor.monitor)
        .find(|monitor| monitor.logical().contains_point(cursor.0, cursor.1))?;
    let point = target.bounds.center();
    (test
        || thodu_core::input::should_restore(
            hypr::follows_mouse(),
            hypr::window_at(cursor.0, cursor.1).as_deref(),
            hypr::window_at(point.0, point.1).as_deref(),
        ))
    .then_some((monitor, cursor))
}

fn restart_address(named: bool, scope: &str, windows: &[Window]) -> Option<String> {
    if named || scope == "focused" {
        windows.first().map(|window| window.address.clone())
    } else {
        None
    }
}

fn visible_target_owner_changed(target: &Target, current_owner: Option<&str>) -> bool {
    !target.window.is_empty() && current_owner != Some(target.window.as_str())
}

/// Press only while the target is current, checking it right before the
/// press with the pointer held, so no other action moves the pointer in
/// between (REQ-ACT-03). Slow preparation belongs before this, and a refresh
/// after it, so neither holds the pointer. None means the check failed and
/// nothing was pressed.
fn checked_click<R>(
    pointer: &Mutex<()>,
    current: impl FnOnce() -> bool,
    press: impl FnOnce() -> R,
) -> Option<R> {
    let _turn = pointer.lock().ok()?;
    current().then(press)
}

/// Run `f` holding the pointer, for moves outside a click such as putting
/// the pointer back.
fn under_pointer<R>(
    pointer: &Mutex<()>,
    f: impl FnOnce() -> Result<R, String>,
) -> Result<R, String> {
    let _turn = pointer.lock().map_err(|_| "pointer lock".to_string())?;
    f()
}

/// Whether a click is watched for a workspace switch: a click on the bar,
/// whether through a bar target or a grid cell over it, can switch
/// workspaces, and the overlay must then end instead of taking the keyboard
/// back for a second Return, which would switch back (REQ-ACT-02).
fn watches_workspace(linger: bool, on_bar: bool) -> bool {
    linger && on_bar
}

/// Note the first click's outcome; a second click goes only after one that
/// happened.
fn record_first_click(entry: &mut Entry, happened: bool, workspace_changed: bool) {
    entry.clicked = happened;
    entry.workspace_changed = workspace_changed;
    entry.pending = entry.pending.saturating_sub(1);
}

/// Whether a target's click point may no longer reach it: a bar target's
/// point must still belong to the bar, and a window target's window must
/// not have moved, resized, closed or left its workspace and must still
/// own the point, and an accessible element must still be where it was.
fn target_moved_or_covered(
    target: &Target,
    window: Option<&Window>,
    element_moved: impl FnOnce(&ElementCheck) -> bool,
) -> bool {
    let (x, y) = target.bounds.center();
    let owner = hypr::owner_at(x, y);
    let now = window.and_then(|window| hypr::window(&window.address).ok().flatten());
    moved_or_covered_given(target, window, now.as_ref(), &owner, element_moved)
}

/// `target_moved_or_covered` once Hyprland has answered: `now` is the
/// target's window as Hyprland reports it now and `owner` what owns the
/// click point now. The element is read last, only when the rest holds.
fn moved_or_covered_given(
    target: &Target,
    window: Option<&Window>,
    now: Option<&Window>,
    owner: &Owner,
    element_moved: impl FnOnce(&ElementCheck) -> bool,
) -> bool {
    if target.window.is_empty() {
        return target.source == Source::Shell
            && *owner != Owner::Layer(sources::BAR_NAMESPACE.to_string());
    }
    let Some(window) = window else {
        return false;
    };
    target_is_stale(target, window, now)
        || visible_target_owner_changed(target, owner.window())
        || target.check.as_ref().is_some_and(element_moved)
}

/// Whether a target may be clicked now: a shell target must match the
/// latest bar report, and any target must still own its click point, which
/// for a window target includes its window's geometry.
fn target_is_current(
    target: &Target,
    shell: &sources::Shell,
    monitor: &Monitor,
    moved_or_covered: impl FnOnce() -> bool,
) -> bool {
    !((target.source == Source::Shell && shell.target_is_stale(target, monitor))
        || moved_or_covered())
}

fn scoped_windows(
    active: &Window,
    visible: Vec<Window>,
    named: bool,
    scope: &str,
    focused_address: Option<&str>,
) -> Vec<Window> {
    let mut windows = if named || scope == "focused" {
        vec![active.clone()]
    } else {
        visible
    };
    if let Some(index) = windows
        .iter()
        .position(|window| window.address == active.address)
    {
        let focused = windows.remove(index);
        windows.insert(0, focused);
    } else {
        windows.insert(0, active.clone());
    }
    for window in &mut windows {
        window.focused = focused_address == Some(window.address.as_str());
    }
    windows
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
        digit if digit.len() == 1 && digit.as_bytes()[0].is_ascii_digit() => Key::Digit {
            digit: digit.as_bytes()[0] - b'0',
        },
        other => {
            let mut chars = other.chars();
            let c = chars.next()?;
            if chars.next().is_some() {
                return None;
            }
            if c.is_uppercase() {
                Key::Upper { c }
            } else if shift && c.is_ascii_alphabetic() {
                Key::Upper {
                    c: c.to_ascii_uppercase(),
                }
            } else {
                Key::Char { c, shift }
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
        if reply.ok
            && matches!(
                serde_json::from_str::<Envelope>(&line).map(|e| e.request),
                Ok(Request::Shutdown)
            )
        {
            daemon.shutdown.store(true, Ordering::Release);
            return;
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
        if std::env::var_os("INVOCATION_ID").is_none() {
            eprintln!("thodud is already running on {}", socket.display());
            return;
        }
        let request = format!("{{\"v\":{},\"op\":\"shutdown\"}}\n", protocol::VERSION);
        let handed_off = UnixStream::connect(&socket).and_then(|mut stream| {
            stream.set_read_timeout(Some(Duration::from_secs(2)))?;
            stream.write_all(request.as_bytes())?;
            let mut reply = String::new();
            BufReader::new(stream).read_line(&mut reply)?;
            let reply: Reply = serde_json::from_str(&reply).map_err(std::io::Error::other)?;
            if reply.ok {
                Ok(())
            } else {
                Err(std::io::Error::other("existing daemon refused shutdown"))
            }
        });
        if handed_off.is_err() {
            eprintln!(
                "ERROR an existing thodud owns {}; stop it and start the Thodu service again",
                socket.display()
            );
            return;
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline && UnixStream::connect(&socket).is_ok() {
            thread::sleep(Duration::from_millis(10));
        }
        if UnixStream::connect(&socket).is_ok() {
            eprintln!(
                "ERROR the existing thodud did not release {}; stop it and start the Thodu service again",
                socket.display()
            );
            return;
        }
    }
    let dispatch_syntax = match focus::detect_dispatch_syntax(hypr::request) {
        Ok(syntax) => Some(syntax),
        Err(error) => {
            eprintln!(
                "ERROR cannot detect Hyprland dispatch syntax: {error}; app-window scroll is unavailable until `hyprctl systeminfo` works and thodud is restarted"
            );
            None
        }
    };
    let focus_records = match focus::record_directory() {
        Ok(directory) => Some(directory),
        Err(error) => {
            eprintln!(
                "ERROR cannot prepare Thodu scroll recovery state: {error}; hints and clicks remain available, set XDG_DATA_HOME or HOME and restart thodud to enable app-window scroll"
            );
            None
        }
    };
    if let (Some(directory), Some(syntax)) = (&focus_records, dispatch_syntax) {
        for error in focus::clear_leftovers(
            directory,
            |address| hypr::window(address).map(|window| window.map(|window| window.pid)),
            |address| focus::get_stay_focused(address, hypr::request),
            |address, value| focus::set_stay_focused(address, value, syntax, hypr::request),
        ) {
            eprintln!(
                "ERROR cannot recover scroll focus: {error}; check `hyprctl systeminfo`, the named window, and Thodu's state files, then restart thodud"
            );
        }
    } else if focus_records.is_some() {
        eprintln!(
            "ERROR skipped scroll focus recovery because Hyprland dispatch syntax is unavailable; restore it with `hyprctl systeminfo` and restart thodud"
        );
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
        layer_origin: hypr::layer_origin,
    });
    let image = Arc::new(image::ImageSource::new(IMAGE_BUDGET));
    let daemon = Arc::new(Daemon {
        socket,
        shutdown: AtomicBool::new(false),
        sessions: Mutex::new(HashMap::new()),
        changed: Condvar::new(),
        next: AtomicU64::new(1),
        atspi: Arc::clone(&atspi),
        shell: Arc::clone(&shell),
        sources: vec![atspi, image, shell],
        listeners: Mutex::new(listeners::Listeners::new()),
        keyboard: Mutex::new(None),
        wheel: Mutex::new(None),
        pointer_action: Mutex::new(()),
        focus_records,
        dispatch_syntax,
    });
    if let Err(e) = listener.set_nonblocking(true) {
        eprintln!("ERROR cannot make the service listener nonblocking: {e}");
        std::process::exit(1);
    }
    while !daemon.shutdown.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((stream, _)) => {
                let daemon = Arc::clone(&daemon);
                thread::spawn(move || serve(&daemon, stream));
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(e) => {
                eprintln!("ERROR cannot accept a Thodu client: {e}");
                break;
            }
        }
    }
    drop(listener);
    let _ = std::fs::remove_file(&daemon.socket);
}

#[cfg(test)]
mod tests {
    use thodu_core::model::Window;

    use std::cell::{Cell, RefCell};
    use std::collections::{HashMap, HashSet};
    use std::ops::Deref;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::{Condvar, Mutex, mpsc};
    use std::time::{Duration, Instant};

    use thodu_core::model::{ActionKind, Monitor, Rect, Role, Source as TargetSource, Target};
    use thodu_core::protocol::{ErrorKind, Timing};
    use thodu_core::session::{Key, ScrollAxis, Session, State};
    use thodu_core::{labels, protocol};

    use super::{
        Daemon, Entry, ScrollFailure, ScrollFocus, WORKSPACE_CHANGE_WAIT, WindowScope,
        checked_click, grid_window, image_fallback_exclusions, merge_fallback,
        moved_or_covered_given, no_active_window_scope, parse_key, perform_scroll_event_with,
        record_first_click, refresh_options, restart_address, scoped_windows, send_wheel_with,
        start_refreshed_session, status_persistent, target_is_current,
        visible_target_owner_changed, wait_for_workspace_change, watches_workspace,
        window_for_target, with_grid_note,
    };
    use crate::sources::{self, Found, Source};

    fn daemon_with_entry(entry: Entry) -> Arc<Daemon> {
        Arc::new(Daemon {
            socket: PathBuf::new(),
            shutdown: AtomicBool::new(false),
            sessions: Mutex::new(HashMap::from([("s1".into(), entry)])),
            changed: Condvar::new(),
            next: AtomicU64::new(0),
            atspi: Arc::new(sources::Atspi {
                bus: Mutex::new(None),
                woken: Mutex::new(HashSet::new()),
                budget: Duration::ZERO,
            }),
            shell: Arc::new(sources::Shell {
                items: Mutex::new(HashMap::new()),
                budget: Duration::ZERO,
                layer_origin: crate::hypr::layer_origin,
            }),
            sources: Vec::new(),
            listeners: Mutex::new(crate::listeners::Listeners::new()),
            keyboard: Mutex::new(None),
            wheel: Mutex::new(None),
            pointer_action: Mutex::new(()),
            focus_records: None,
            dispatch_syntax: None,
        })
    }

    struct FocusTestDir(PathBuf);

    impl Deref for FocusTestDir {
        type Target = Path;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl Drop for FocusTestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn test_focus_dir() -> FocusTestDir {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = PathBuf::from(std::env::var_os("HOME").unwrap())
            .join(".local/share/thodu/tests/scroll-events")
            .join(format!(
                "{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&directory).unwrap();
        FocusTestDir(directory)
    }

    // REQ-ACT-04, REQ-ACT-06: the production scroll wrapper forces the
    // owning window while the action executes, then clears its override.
    #[test]
    fn req_act_04_req_act_06_scroll_event_routes_through_owner_focus() {
        let directory = test_focus_dir();
        let window = on(0);
        let target = Target {
            id: "pane".into(),
            bounds: Rect::new(2, 2, 4, 4),
            role: Role::ScrollArea,
            text: "Document".into(),
            source: TargetSource::Atspi,
            actions: vec![ActionKind::Scroll],
            window: window.address.clone(),
            check: None,
        };
        let focused = RefCell::new(false);
        let points = RefCell::new(Vec::new());
        let wheels = RefCell::new(0);
        let mut get = |_: &str| Ok(*focused.borrow());
        let mut set = |_: &str, value: Option<bool>| {
            *focused.borrow_mut() = value.unwrap_or(false);
            Ok(())
        };
        let mut exists = |_: &str| Ok(Some(window.pid));
        let result = perform_scroll_event_with(
            &target,
            Some(&window),
            ScrollFocus {
                records: Some(&directory),
                stale: false,
                get: &mut get,
                set: &mut set,
                window_exists: &mut exists,
            },
            || {
                send_wheel_with(
                    target.bounds.center(),
                    None,
                    |point| {
                        assert!(*focused.borrow(), "pointer must move under owner focus");
                        points.borrow_mut().push(point);
                        Ok(())
                    },
                    |_| Err("unexpected stationary focus refresh".into()),
                    || {
                        assert!(*focused.borrow(), "wheel must run under owner focus");
                        *wheels.borrow_mut() += 1;
                        Ok(())
                    },
                )
            },
        );
        assert_eq!(result, Ok(()));
        assert!(!*focused.borrow());
        assert_eq!(*points.borrow(), vec![(4, 4)]);
        assert_eq!(*wheels.borrow(), 1);
    }

    // REQ-ACT-06: a scroll action positions the pointer at the selected area
    // without a one-pixel focus nudge that makes held scrolling flicker.
    #[test]
    fn req_act_06_scroll_event_does_not_nudge_the_pointer() {
        let positions = RefCell::new(Vec::new());
        let result = send_wheel_with(
            (4, 4),
            None,
            |point| {
                positions.borrow_mut().push(point);
                Ok(())
            },
            |_| Err("unexpected stationary focus refresh".into()),
            || Ok(()),
        );
        assert_eq!(result, Ok(()));
        assert_eq!(*positions.borrow(), [(4, 4)]);
    }

    // REQ-ACT-06: when the cursor is already over the selected area, a
    // compositor focus refresh does not move it before the wheel event.
    #[test]
    fn req_act_06_scroll_refreshes_stationary_pointer_focus_without_moving_it() {
        let point = (4, 4);
        let positions = RefCell::new(Vec::new());
        let refreshes = RefCell::new(Vec::new());
        let result = send_wheel_with(
            point,
            Some(point),
            |destination| {
                positions.borrow_mut().push(destination);
                Ok(())
            },
            |destination| {
                refreshes.borrow_mut().push(destination);
                Ok(())
            },
            || Ok(()),
        );
        assert_eq!(result, Ok(()));
        assert!(positions.borrow().is_empty());
        assert_eq!(*refreshes.borrow(), [point]);
    }

    // REQ-ACT-06: if Hyprland cannot confirm pointer focus, do not send a
    // wheel event that could land on a different surface.
    #[test]
    fn req_act_06_failed_focus_refresh_does_not_emit_wheel_input() {
        let point = (4, 4);
        let moved = RefCell::new(Vec::new());
        let emitted = Cell::new(false);
        let result = send_wheel_with(
            point,
            Some(point),
            |destination| {
                moved.borrow_mut().push(destination);
                Ok(())
            },
            |_| Err("focus refresh unavailable".into()),
            || {
                emitted.set(true);
                Ok(())
            },
        );
        assert_eq!(
            result,
            Err(ScrollFailure::Input("focus refresh unavailable".into()))
        );
        assert!(moved.borrow().is_empty());
        assert!(!emitted.get());
    }

    // REQ-ACT-06: held repeats keep the pointer at the selected point and
    // refresh compositor focus without movement on subsequent ticks.
    #[test]
    fn req_act_06_scroll_repeats_do_not_reposition_the_pointer() {
        let point = (4, 4);
        let next_point = (8, 8);
        let pointer = Cell::new(Some((40, 40)));
        let positions = RefCell::new(Vec::new());
        let refreshes = RefCell::new(Vec::new());
        let wheels = Cell::new(0);
        for _ in 0..4 {
            let result = send_wheel_with(
                point,
                pointer.get(),
                |destination| {
                    positions.borrow_mut().push(destination);
                    pointer.set(Some(destination));
                    Ok(())
                },
                |destination| {
                    refreshes.borrow_mut().push(destination);
                    Ok(())
                },
                || {
                    wheels.set(wheels.get() + 1);
                    Ok(())
                },
            );
            assert_eq!(result, Ok(()));
        }
        assert_eq!(*positions.borrow(), [point]);
        assert_eq!(*refreshes.borrow(), [point, point, point]);
        assert_eq!(wheels.get(), 4);

        let result = send_wheel_with(
            next_point,
            pointer.get(),
            |destination| {
                positions.borrow_mut().push(destination);
                pointer.set(Some(destination));
                Ok(())
            },
            |destination| {
                refreshes.borrow_mut().push(destination);
                Ok(())
            },
            || Ok(()),
        );
        assert_eq!(result, Ok(()));
        assert_eq!(*positions.borrow(), [point, next_point]);
        assert_eq!(*refreshes.borrow(), [point, point, point]);
    }

    // REQ-ACT-04: Omarchy shell scroll targets have no app window and should
    // send their wheel action without querying or changing a window property.
    #[test]
    fn req_act_04_shell_scroll_event_skips_window_focus_override() {
        let directory = test_focus_dir();
        let target = Target {
            id: "bar".into(),
            bounds: Rect::new(2, 2, 4, 4),
            role: Role::ScrollArea,
            text: "Bar".into(),
            source: TargetSource::Shell,
            actions: vec![ActionKind::Scroll],
            window: String::new(),
            check: None,
        };
        let mut get =
            |_: &str| -> Result<bool, String> { panic!("shell target has no window property") };
        let mut set = |_: &str, _: Option<bool>| -> Result<(), String> {
            panic!("shell target has no window property")
        };
        let mut exists = |_: &str| -> Result<Option<u32>, String> {
            panic!("shell target has no owner to check")
        };
        let result = perform_scroll_event_with(
            &target,
            None,
            ScrollFocus {
                records: Some(&directory),
                stale: false,
                get: &mut get,
                set: &mut set,
                window_exists: &mut exists,
            },
            || Ok(()),
        );
        assert_eq!(result, Ok(()));
    }

    // REQ-ACT-06: if an owner disappears between the geometry check and the
    // property query, report a stale target rather than a generic input error.
    #[test]
    fn req_act_06_closed_owner_during_focus_setup_is_stale() {
        let directory = test_focus_dir();
        let window = on(0);
        let target = Target {
            id: "pane".into(),
            bounds: Rect::new(2, 2, 4, 4),
            role: Role::ScrollArea,
            text: "Document".into(),
            source: TargetSource::Atspi,
            actions: vec![ActionKind::Scroll],
            window: window.address.clone(),
            check: None,
        };
        let mut get = |_: &str| Err("window not found".into());
        let mut set = |_: &str, _: Option<bool>| Ok(());
        let mut exists = |_: &str| Ok(None);
        let result = perform_scroll_event_with(
            &target,
            Some(&window),
            ScrollFocus {
                records: Some(&directory),
                stale: false,
                get: &mut get,
                set: &mut set,
                window_exists: &mut exists,
            },
            || panic!("closed owner must not receive a wheel action"),
        );
        assert_eq!(result, Err(super::ScrollFailure::Stale));
    }

    #[test]
    fn req_act_06_missing_window_for_app_scroll_is_stale() {
        let target = Target {
            id: "pane".into(),
            bounds: Rect::new(2, 2, 4, 4),
            role: Role::ScrollArea,
            text: "Document".into(),
            source: TargetSource::Atspi,
            actions: vec![ActionKind::Scroll],
            window: "0x1".into(),
            check: None,
        };
        let mut get = |_: &str| -> Result<bool, String> { panic!("no window to query") };
        let mut set =
            |_: &str, _: Option<bool>| -> Result<(), String> { panic!("no window to update") };
        let mut identity =
            |_: &str| -> Result<Option<u32>, String> { panic!("no window identity to query") };
        let result = perform_scroll_event_with(
            &target,
            None,
            ScrollFocus {
                records: None,
                stale: false,
                get: &mut get,
                set: &mut set,
                window_exists: &mut identity,
            },
            || panic!("a missing window must not receive wheel input"),
        );
        assert_eq!(result, Err(super::ScrollFailure::Stale));
    }

    #[test]
    fn req_act_06_missing_scroll_storage_keeps_app_action_disabled() {
        let window = on(0);
        let target = Target {
            id: "pane".into(),
            bounds: Rect::new(2, 2, 4, 4),
            role: Role::ScrollArea,
            text: "Document".into(),
            source: TargetSource::Atspi,
            actions: vec![ActionKind::Scroll],
            window: window.address.clone(),
            check: None,
        };
        let mut get = |_: &str| -> Result<bool, String> { panic!("storage check comes first") };
        let mut set = |_: &str, _: Option<bool>| -> Result<(), String> {
            panic!("storage check comes first")
        };
        let mut identity =
            |_: &str| -> Result<Option<u32>, String> { panic!("storage check comes first") };
        let result = perform_scroll_event_with(
            &target,
            Some(&window),
            ScrollFocus {
                records: None,
                stale: false,
                get: &mut get,
                set: &mut set,
                window_exists: &mut identity,
            },
            || panic!("scrolling must fail visibly without recovery storage"),
        );
        assert!(
            matches!(result, Err(super::ScrollFailure::Input(message)) if message.contains("recovery storage is unavailable"))
        );
    }

    // REQ-ACT-02: the shell workspace guard checks at a bounded rate and
    // still notices a workspace update before the wait expires.
    #[test]
    fn req_act_02_workspace_change_wait_is_rate_limited() {
        let mut probes = 0;
        assert!(!wait_for_workspace_change(
            1,
            Duration::from_millis(80),
            || {
                probes += 1;
                Some(1)
            }
        ));
        assert!(probes <= 5, "the guard queried Hyprland {probes} times");

        let mut probes = 0;
        assert!(wait_for_workspace_change(1, WORKSPACE_CHANGE_WAIT, || {
            probes += 1;
            (probes == 3).then_some(2)
        }));
        assert_eq!(probes, 3);
    }

    // REQ-ACT-04: wheel input waits for pointer pass-through, then resumes
    // the same scroll session without unmapping its keyboard surface.
    #[test]
    fn req_act_04_scroll_ready_sends_one_wheel_action_and_resumes_session() {
        let monitor = Monitor {
            name: "M".into(),
            x: 0,
            y: 0,
            width: 100,
            height: 100,
            scale: 1.0,
            layers: Vec::new(),
        };
        let window = on(0);
        let target = Target {
            id: "pane".into(),
            bounds: Rect::new(20, 30, 40, 20),
            role: Role::ScrollArea,
            text: "Document".into(),
            source: TargetSource::Atspi,
            actions: vec![ActionKind::Scroll],
            window: window.address.clone(),
            check: None,
        };
        let mut session = Session::new_scroll(vec![target.clone()]);
        session.key(Key::Char {
            c: 'j',
            shift: false,
        });
        let entry = Entry {
            session,
            window: Some(WindowScope {
                windows: HashMap::from([(window.address.clone(), window.clone())]),
                refetch_address: None,
            }),
            monitor,
            status: "1 scroll area".into(),
            collecting: false,
            overlay_bounds: None,
            overlay: true,
            test: true,
            hidden: false,
            created: Instant::now(),
            timing: Timing::default(),
            pending: 1,
            clicked: false,
            workspace_changed: false,
            closing: false,
            restore: None,
            scroll_restore: None,
            scroll_released: false,
        };
        let daemon = daemon_with_entry(entry);
        let (sent, received) = mpsc::channel();
        let reply = daemon
            .scroll_ready_with("s1", move |pane, owner, _, action, _| {
                sent.send((
                    pane.bounds.center(),
                    owner.map(|w| w.address.clone()),
                    action,
                ))
                .expect("report injected wheel event");
                Ok(())
            })
            .expect("pointer pass-through ready");
        let (point, owner, action) = received.recv().expect("one wheel event");
        assert_eq!(point, (40, 40));
        assert_eq!(owner.as_deref(), Some("0x1"));
        assert_eq!(action.axis, ScrollAxis::Vertical);
        assert_eq!(action.value, -1);
        let view = reply.view.expect("resumed view");
        assert_eq!(view.state, State::Showing);
        assert_eq!(view.visible, ["pane"]);

        let sessions = daemon.sessions.lock().unwrap();
        let entry = sessions.get("s1").expect("session remains open");
        assert_eq!(entry.session.state, State::Showing);
        assert_eq!(entry.session.focused, Some(0));
        assert_eq!(entry.pending, 0);
        assert!(!entry.hidden);
    }

    // REQ-SESSION-02 and REQ-ACT-06: Escape closes scroll mode during both
    // the pass-through wait and an already-started wheel event.
    #[test]
    fn req_session_02_escape_closes_scroll_mode_during_a_pending_action() {
        let monitor = Monitor {
            name: "M".into(),
            x: 0,
            y: 0,
            width: 100,
            height: 100,
            scale: 1.0,
            layers: Vec::new(),
        };
        let target = Target {
            id: "pane".into(),
            bounds: Rect::new(20, 30, 40, 20),
            role: Role::ScrollArea,
            text: "Document".into(),
            source: TargetSource::Atspi,
            actions: vec![ActionKind::Scroll],
            window: String::new(),
            check: None,
        };
        let pending_entry = |action_started: bool| {
            let mut session = Session::new_scroll(vec![target.clone()]);
            session.key(Key::Char {
                c: 'j',
                shift: false,
            });
            if action_started {
                assert!(session.take_scroll_action().is_some());
            }
            Entry {
                session,
                window: None,
                monitor: monitor.clone(),
                status: "1 scroll area".into(),
                collecting: false,
                overlay_bounds: None,
                overlay: true,
                test: true,
                hidden: false,
                created: Instant::now(),
                timing: Timing::default(),
                pending: 1,
                clicked: false,
                workspace_changed: false,
                closing: false,
                restore: None,
                scroll_restore: None,
                scroll_released: false,
            }
        };

        let waiting = daemon_with_entry(pending_entry(false));
        let waiting_reply = waiting.key("s1", "Escape", false, false, 0).unwrap();
        assert_eq!(
            waiting_reply
                .view
                .expect("Escape returns a closed view")
                .state,
            State::Closed
        );
        assert!(
            waiting.sessions.lock().unwrap().get("s1").is_none(),
            "an action not yet sent is cancelled and the session is removed"
        );

        let running = daemon_with_entry(pending_entry(true));
        let running_reply = running.key("s1", "Escape", false, false, 0).unwrap();
        assert_eq!(
            running_reply
                .view
                .expect("Escape returns a closed view")
                .state,
            State::Closed
        );
        let sessions = running.sessions.lock().unwrap();
        let entry = sessions
            .get("s1")
            .expect("the wheel event still owns its cleanup");
        assert!(entry.closing);
        assert_eq!(entry.pending, 1);
    }

    // REQ-SESSION-08: a pointer restoration error stays visible without
    // turning a normal scroll-key release into a closed session.
    #[test]
    fn req_session_08_scroll_restore_error_is_visible_and_keeps_session_open() {
        let monitor = Monitor {
            name: "M".into(),
            x: 0,
            y: 0,
            width: 100,
            height: 100,
            scale: 1.0,
            layers: Vec::new(),
        };
        let daemon = daemon_with_entry(Entry {
            session: Session::new_scroll(vec![Target {
                id: "pane".into(),
                bounds: Rect::new(20, 30, 40, 20),
                role: Role::ScrollArea,
                text: "Document".into(),
                source: TargetSource::Atspi,
                actions: vec![ActionKind::Scroll],
                window: String::new(),
                check: None,
            }]),
            window: None,
            monitor: monitor.clone(),
            status: "1 scroll area".into(),
            collecting: false,
            overlay_bounds: None,
            overlay: true,
            test: true,
            hidden: false,
            created: Instant::now(),
            timing: Timing::default(),
            pending: 0,
            clicked: false,
            workspace_changed: false,
            closing: false,
            restore: None,
            scroll_restore: Some((monitor, (5, 7))),
            scroll_released: false,
        });

        let reply = daemon
            .scroll_stop_with("s1", |_, _| {
                Err("the compositor offers no virtual pointer".into())
            })
            .expect("a restore error is reported as session status");
        assert!(reply.ok);
        assert!(reply.view.as_ref().unwrap().status_persistent);
        assert_eq!(
            reply.status.as_deref(),
            Some("scroll pointer restore failed: the compositor offers no virtual pointer")
        );
        assert_eq!(
            daemon
                .sessions
                .lock()
                .unwrap()
                .get("s1")
                .unwrap()
                .session
                .state,
            State::Showing
        );
    }

    struct Empty(&'static str);

    impl Source for Empty {
        fn name(&self) -> &'static str {
            self.0
        }

        fn budget(&self) -> Duration {
            Duration::from_millis(100)
        }

        fn collect(&self, _: &Window, _: &Monitor) -> Found {
            Ok(sources::SourceTargets::new(
                Vec::<Target>::new(),
                format!("0 targets from {}", self.0),
            ))
        }
    }

    // REQ-SESSION-08: important diagnostics persist while routine counts can expire.
    #[test]
    fn req_session_08_status_persistence_marks_attention_states() {
        assert!(!status_persistent(
            "20 targets from atspi in 14 ms",
            false,
            false
        ));
        assert!(status_persistent(
            "atspi missed its 80 ms budget",
            false,
            false
        ));
        assert!(status_persistent("atspi unavailable", false, false));
        assert!(status_persistent("0 scroll areas from atspi", false, true));
        assert!(status_persistent("20 targets from atspi", true, false));
        assert!(status_persistent(
            "config parse failed; using the defaults",
            false,
            false
        ));
        assert!(status_persistent(
            "scroll failed: the compositor offers no virtual pointer",
            false,
            false
        ));
        assert!(status_persistent(
            "scroll pointer restore failed: the compositor offers no virtual pointer",
            false,
            false
        ));
    }

    // REQ-SESSION-04: when every source returns nothing, the grid opens
    // and the status line says no targets were found.
    #[test]
    fn req_session_04_every_source_empty_opens_the_grid() {
        let sources: Vec<Arc<dyn Source>> =
            vec![Arc::new(Empty("atspi")), Arc::new(Empty("shell"))];
        let monitor = Monitor {
            name: "M".into(),
            x: 0,
            y: 0,
            width: 1536,
            height: 864,
            scale: 1.0,
            layers: Vec::new(),
        };
        let got = sources::collect(&sources, &on(0), &monitor);
        let session = Session::begin(got.targets, monitor.logical(), false, false);
        assert!(!session.grid.is_empty(), "the grid is open");
        let status = with_grid_note(got.status.join("; "), &session, false);
        assert_eq!(
            status,
            "0 targets from atspi; 0 targets from shell; no targets found, so the grid is open"
        );
        let asked = Session::begin(Vec::new(), monitor.logical(), false, true);
        assert_eq!(with_grid_note("the grid".into(), &asked, true), "the grid");
    }

    // REQ-SESSION-06 and REQ-SHELL-01: with no active app, hints use the
    // focused monitor's bar; if the bar has no targets, the grid opens.
    #[test]
    fn req_session_06_req_shell_01_hints_work_without_open_apps() {
        let chosen_monitor = Monitor {
            name: "M".into(),
            x: 120,
            y: 0,
            width: 100,
            height: 50,
            scale: 1.0,
            layers: vec![thodu_core::model::Layer {
                namespace: "omarchy-bar".into(),
                level: 2,
                rect: Rect::new(120, 0, 100, 26),
            }],
        };
        let (active, monitor, windows, visible_windows) =
            no_active_window_scope("visible", chosen_monitor.clone(), Vec::new());
        assert!(active.is_none());
        assert_eq!(monitor, chosen_monitor);
        assert!(windows.is_empty());

        let shell = Arc::new(sources::Shell {
            items: Mutex::new(HashMap::from([(
                monitor.name.clone(),
                vec![thodu_core::protocol::ShellItem {
                    identity: "workspace".into(),
                    x: 5,
                    y: 2,
                    w: 20,
                    h: 20,
                    text: "workspace".into(),
                }],
            )])),
            budget: Duration::from_millis(40),
            layer_origin: |_, _| Some((120, 0)),
        });
        let got = sources::collect_visible_scoped(&[shell], &windows, &visible_windows, &monitor);
        let session = Session::begin(got.targets, monitor.logical(), false, false);
        assert_eq!(session.targets.len(), 1, "the bar target is labeled");
        assert!(session.grid.is_empty(), "a bar target prevents the grid");

        let no_bar = Arc::new(sources::Shell {
            items: Mutex::new(HashMap::new()),
            budget: Duration::from_millis(40),
            layer_origin: |_, _| None,
        });
        let got = sources::collect_visible_scoped(&[no_bar], &windows, &visible_windows, &monitor);
        let session = Session::begin(got.targets, monitor.logical(), false, false);
        let status = with_grid_note(got.status.join("; "), &session, false);
        assert!(
            !session.grid.is_empty(),
            "the grid opens when the bar is empty"
        );
        assert!(status.contains("no targets found"));
    }

    // REQ-SRC-06 and REQ-SESSION-01: image targets join the drawn primary
    // session as one generation, then input can safely resume.
    #[test]
    fn req_src_06_req_session_01_fallback_updates_visible_targets_after_primary_draw() {
        let mut first = on(0);
        first.size = (100, 100);
        let mut second = first.clone();
        second.address = "0x2".into();
        second.at = (100, 0);
        let monitor = Monitor {
            name: "M".into(),
            x: 0,
            y: 0,
            width: 200,
            height: 100,
            scale: 1.0,
            layers: Vec::new(),
        };
        let primary = Target {
            id: "a0".into(),
            bounds: Rect::new(10, 10, 20, 20),
            role: Role::Button,
            text: "Open".into(),
            source: TargetSource::Atspi,
            actions: vec![ActionKind::Click],
            window: first.address.clone(),
            check: None,
        };
        let image = Target {
            id: "0x2:i0".into(),
            bounds: Rect::new(110, 10, 20, 20),
            role: Role::Text,
            text: String::new(),
            source: TargetSource::Image,
            actions: vec![ActionKind::Click],
            window: second.address.clone(),
            check: None,
        };
        let mut entry = Entry {
            session: Session::new(vec![primary], false)
                .reserving_label_prefix('z')
                .with_area(monitor.logical()),
            window: None,
            monitor,
            status: "primary targets; collecting visual targets".into(),
            collecting: true,
            overlay_bounds: None,
            overlay: true,
            test: true,
            hidden: false,
            created: Instant::now(),
            timing: Timing::default(),
            pending: 0,
            clicked: false,
            workspace_changed: false,
            closing: false,
            restore: None,
            scroll_restore: None,
            scroll_released: true,
        };

        assert!(merge_fallback(
            &mut entry,
            sources::Collected {
                targets: vec![image],
                status: vec!["1 visual candidate from image".into()],
                fallback_requested: false,
            },
            &[first, second],
        ));

        assert!(!entry.collecting);
        assert_eq!(entry.session.generation, 1);
        assert_eq!(entry.session.targets.len(), 2);
        assert_eq!(entry.session.targets[1].source, TargetSource::Image);
        assert!(entry.status.contains("1 visual candidate from image"));
        assert!(!entry.status.contains("collecting visual targets"));

        entry.collecting = true;
        entry.session.state = State::Acting;
        assert!(!merge_fallback(
            &mut entry,
            sources::Collected {
                targets: Vec::new(),
                status: vec!["late fallback".into()],
                fallback_requested: false,
            },
            &[],
        ));
        assert!(
            !entry.collecting,
            "a non-showing session is no longer locked"
        );
        assert!(!entry.status.contains("collecting visual targets"));
        assert_eq!(entry.session.state, State::Acting);
    }

    // REQ-SRC-06: the image source omits visible hint chips and the status
    // toast that are drawn over the app after primary targets arrive.
    #[test]
    fn req_src_06_fallback_excludes_overlay_labels_focus_ring_and_status() {
        let monitor = Monitor {
            name: "M".into(),
            x: 100,
            y: 50,
            width: 1200,
            height: 900,
            scale: 1.0,
            layers: Vec::new(),
        };
        let target = Target {
            id: "a0".into(),
            bounds: Rect::new(120, 80, 30, 24),
            role: Role::Button,
            text: "Open".into(),
            source: TargetSource::Atspi,
            actions: vec![ActionKind::Click],
            window: "0x1".into(),
            check: None,
        };
        let mut session = Session::new(vec![target.clone()], false).with_area(monitor.logical());
        session.focused = Some(0);
        let exclusions = image_fallback_exclusions(
            &session,
            &monitor,
            "20 targets from atspi across 2 windows in 14 ms; collecting visual targets",
            None,
        );

        assert!(exclusions.iter().any(|item| item.id == target.id));
        let chip = labels::place(&target.bounds, protocol::label_size(1), &monitor.logical());
        assert!(
            exclusions
                .iter()
                .any(|item| item.id == "overlay:label:0" && item.bounds == chip)
        );
        assert!(
            exclusions
                .iter()
                .any(|item| item.id == "overlay:focus-ring")
        );
        let toast = exclusions
            .iter()
            .find(|item| item.id == "overlay:status-line")
            .expect("status toast is excluded");
        assert!(toast.bounds.w <= 640);
        assert!(toast.bounds.bottom() <= monitor.logical().bottom());
        assert_eq!(toast.source, TargetSource::Shell);
        assert!(toast.window.is_empty());

        let short = image_fallback_exclusions(&session, &monitor, "3 targets", None);
        let short_toast = short
            .iter()
            .find(|item| item.id == "overlay:status-line")
            .expect("short status toast is excluded");
        assert!(short_toast.bounds.w < toast.bounds.w);

        let rendered = [chip, Rect::new(700, 820, 320, 45)];
        let exact = image_fallback_exclusions(&session, &monitor, "3 targets", Some(&rendered));
        assert!(exact.iter().any(|item| {
            item.id == "overlay:0" && item.bounds == rendered[0] && item.window.is_empty()
        }));
        assert!(exact.iter().any(|item| {
            item.id == "overlay:1" && item.bounds == rendered[1] && item.window.is_empty()
        }));
        assert!(!exact.iter().any(|item| item.id == "overlay:status-line"));
    }

    // REQ-SESSION-01: image capture starts after the overlay reports its
    // first frame, and the source receives those rendered rectangles.
    #[test]
    fn req_session_01_drawn_overlay_geometry_releases_visual_fallback() {
        let monitor = Monitor {
            name: "M".into(),
            x: 0,
            y: 0,
            width: 100,
            height: 100,
            scale: 1.0,
            layers: Vec::new(),
        };
        let target = Target {
            id: "a0".into(),
            bounds: Rect::new(10, 10, 20, 20),
            role: Role::Button,
            text: "Open".into(),
            source: TargetSource::Atspi,
            actions: vec![ActionKind::Click],
            window: "0x1".into(),
            check: None,
        };
        let daemon = daemon_with_entry(Entry {
            session: Session::new(vec![target], false).with_area(monitor.logical()),
            window: None,
            monitor,
            status: "1 target; collecting visual targets".into(),
            collecting: true,
            overlay_bounds: None,
            overlay: true,
            test: false,
            hidden: false,
            created: Instant::now(),
            timing: Timing::default(),
            pending: 0,
            clicked: false,
            workspace_changed: false,
            closing: false,
            restore: None,
            scroll_restore: None,
            scroll_released: true,
        });
        let worker = Arc::clone(&daemon);
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            tx.send(worker.wait_for_first_draw("s1", Duration::from_secs(1)))
                .unwrap();
        });

        let rendered = [Rect::new(7, 16, 26, 18), Rect::new(20, 70, 60, 26)];
        daemon.drawn("s1", Some(rendered.to_vec())).unwrap();
        assert!(rx.recv_timeout(Duration::from_secs(1)).unwrap());
        let exclusions = daemon.fallback_exclusions("s1", true).unwrap();
        assert!(
            exclusions
                .iter()
                .any(|item| { item.id == "overlay:0" && item.bounds == rendered[0] })
        );
        assert!(
            exclusions
                .iter()
                .any(|item| { item.id == "overlay:1" && item.bounds == rendered[1] })
        );
    }

    // REQ-SESSION-01: timeout keeps the fallback bounded, legacy overlay
    // geometry gets a conservative estimate, and ended sessions do no work.
    #[test]
    fn req_session_01_draw_timeout_and_no_overlay_use_safe_exclusions() {
        let monitor = Monitor {
            name: "M".into(),
            x: 0,
            y: 0,
            width: 100,
            height: 100,
            scale: 1.0,
            layers: Vec::new(),
        };
        let target = Target {
            id: "a0".into(),
            bounds: Rect::new(10, 10, 20, 20),
            role: Role::Button,
            text: "Open".into(),
            source: TargetSource::Atspi,
            actions: vec![ActionKind::Click],
            window: "0x1".into(),
            check: None,
        };
        let entry = || Entry {
            session: Session::new(vec![target.clone()], false).with_area(monitor.logical()),
            window: None,
            monitor: monitor.clone(),
            status: "1 target; collecting visual targets".into(),
            collecting: true,
            overlay_bounds: None,
            overlay: true,
            test: true,
            hidden: false,
            created: Instant::now(),
            timing: Timing::default(),
            pending: 0,
            clicked: false,
            workspace_changed: false,
            closing: false,
            restore: None,
            scroll_restore: None,
            scroll_released: true,
        };

        let active = daemon_with_entry(entry());
        assert!(!active.wait_for_first_draw("s1", Duration::ZERO));
        let estimated = active.fallback_exclusions("s1", true).unwrap();
        assert!(
            estimated
                .iter()
                .any(|item| item.id == "overlay:status-line")
        );
        let no_overlay = active.fallback_exclusions("s1", false).unwrap();
        assert_eq!(no_overlay.len(), 1, "no overlay geometry is excluded");

        let mut ended = entry();
        ended.session.state = State::Closed;
        let ended = daemon_with_entry(ended);
        assert!(!ended.wait_for_first_draw("s1", Duration::from_millis(10)));
        assert!(ended.fallback_exclusions("s1", true).is_none());
        let sessions = ended.sessions.lock().unwrap();
        let ended_entry = sessions.get("s1").unwrap();
        assert!(!ended_entry.collecting);
        assert!(!ended_entry.status.contains("collecting visual targets"));
    }

    // REQ-SESSION-01: labels cannot act on a generation while image targets
    // are pending, but Escape still closes the session immediately.
    #[test]
    fn req_session_01_collecting_session_rejects_labels_and_allows_escape() {
        let monitor = Monitor {
            name: "M".into(),
            x: 0,
            y: 0,
            width: 100,
            height: 100,
            scale: 1.0,
            layers: Vec::new(),
        };
        let target = Target {
            id: "a0".into(),
            bounds: Rect::new(10, 10, 20, 20),
            role: Role::Button,
            text: "Open".into(),
            source: TargetSource::Atspi,
            actions: vec![ActionKind::Click],
            window: "0x1".into(),
            check: None,
        };
        let daemon = daemon_with_entry(Entry {
            session: Session::new(vec![target], false).with_area(monitor.logical()),
            window: None,
            monitor,
            status: "1 target; collecting visual targets".into(),
            collecting: true,
            overlay_bounds: None,
            overlay: true,
            test: true,
            hidden: false,
            created: Instant::now(),
            timing: Timing::default(),
            pending: 0,
            clicked: false,
            workspace_changed: false,
            closing: false,
            restore: None,
            scroll_restore: None,
            scroll_released: true,
        });

        let busy = daemon.key("s1", "A", false, false, 0).unwrap_err();
        assert_eq!(busy.error, Some(ErrorKind::Busy));
        let escape = daemon.key("s1", "Escape", false, false, 0).unwrap();
        assert_eq!(
            escape.view.expect("escape returns a view").state,
            State::Closed
        );
    }

    // REQ-SRC-06 and REQ-SESSION-01: input queued against old labels cannot
    // act after an asynchronous source has changed the session generation.
    #[test]
    fn req_src_06_req_session_01_stale_generation_key_returns_current_targets() {
        let monitor = Monitor {
            name: "M".into(),
            x: 0,
            y: 0,
            width: 100,
            height: 100,
            scale: 1.0,
            layers: Vec::new(),
        };
        let target = Target {
            id: "a0".into(),
            bounds: Rect::new(10, 10, 20, 20),
            role: Role::Button,
            text: "Open".into(),
            source: TargetSource::Atspi,
            actions: vec![ActionKind::Click],
            window: "0x1".into(),
            check: None,
        };
        let mut session = Session::new(vec![target], false).with_area(monitor.logical());
        session.generation = 1;
        let daemon = daemon_with_entry(Entry {
            session,
            window: None,
            monitor,
            status: "2 targets from image".into(),
            collecting: false,
            overlay_bounds: None,
            overlay: true,
            test: true,
            hidden: false,
            created: Instant::now(),
            timing: Timing::default(),
            pending: 0,
            clicked: false,
            workspace_changed: false,
            closing: false,
            restore: None,
            scroll_restore: None,
            scroll_released: true,
        });

        let reply = daemon.key("s1", "A", true, false, 0).unwrap();
        let view = reply.view.expect("stale key returns the current view");
        assert_eq!(view.generation, 1);
        assert_eq!(view.state, State::Showing);
        assert_eq!(
            reply
                .targets
                .expect("stale key returns current labels")
                .len(),
            1
        );
        let sessions = daemon.sessions.lock().unwrap();
        let entry = sessions.get("s1").unwrap();
        assert_eq!(entry.session.state, State::Showing, "the key did not act");
        assert_eq!(entry.pending, 0, "no action was queued");
        drop(sessions);
        let escape = daemon.key("s1", "Escape", false, false, 0).unwrap();
        assert_eq!(
            escape.view.expect("Escape remains available").state,
            State::Closed
        );
    }

    fn click_monitor() -> Monitor {
        Monitor {
            name: "M".into(),
            x: 0,
            y: 0,
            width: 100,
            height: 100,
            scale: 1.0,
            layers: Vec::new(),
        }
    }

    fn bar_click_target() -> Target {
        Target {
            id: "b:item".into(),
            bounds: Rect::new(10, 2, 20, 20),
            role: Role::Button,
            text: "item".into(),
            source: TargetSource::Shell,
            actions: vec![ActionKind::Click],
            window: String::new(),
            check: None,
        }
    }

    fn bar_item(identity: &str) -> thodu_core::protocol::ShellItem {
        thodu_core::protocol::ShellItem {
            identity: identity.into(),
            x: 0,
            y: 2,
            w: 20,
            h: 20,
            text: "item".into(),
        }
    }

    fn second_click_entry(clicked: bool, pending: u8) -> Entry {
        let monitor = click_monitor();
        let session = Session::new(vec![bar_click_target()], false).with_area(monitor.logical());
        Entry {
            session,
            window: None,
            monitor,
            status: String::new(),
            collecting: false,
            overlay_bounds: None,
            overlay: true,
            test: true,
            hidden: true,
            created: Instant::now(),
            timing: Timing::default(),
            pending,
            clicked,
            workspace_changed: false,
            closing: false,
            restore: None,
            scroll_restore: None,
            scroll_released: true,
        }
    }

    /// A daemon whose bar reports `item` at a fixed origin, so bar targets
    /// are checked without asking Hyprland.
    fn second_click_daemon(entry: Entry, item: &str) -> Arc<Daemon> {
        let daemon = daemon_with_entry(entry);
        let shell = sources::Shell {
            items: Mutex::new(HashMap::from([("M".to_string(), vec![bar_item(item)])])),
            budget: Duration::ZERO,
            layer_origin: |_, _| Some((10, 0)),
        };
        let mut daemon = Arc::try_unwrap(daemon).unwrap_or_else(|_| panic!("one owner"));
        daemon.shell = Arc::new(shell);
        Arc::new(daemon)
    }

    fn second_click() -> thodu_core::session::Act {
        thodu_core::session::Act {
            target: 0,
            point: (20, 12),
            button: thodu_core::session::Button::Left,
            count: 1,
            ctrl: false,
            again: true,
            linger: false,
        }
    }

    fn run_second_click(daemon: &Daemon, moved: bool) -> usize {
        let pressed = Cell::new(0);
        daemon.act_again_with(
            "s1",
            &second_click(),
            |_, _| moved,
            |_| {
                pressed.set(pressed.get() + 1);
                Ok(())
            },
        );
        pressed.get()
    }

    // REQ-ACT-03: the check and the press run back to back with the pointer
    // held; a failed check presses nothing, and the pointer is free again
    // for the refresh that follows.
    #[test]
    fn req_act_03_click_checks_right_before_pressing_with_the_pointer_held() {
        let pointer = Mutex::new(());
        let log = RefCell::new(Vec::new());
        let run = |current: bool| {
            checked_click(
                &pointer,
                || {
                    assert!(pointer.try_lock().is_err(), "the check holds the pointer");
                    log.borrow_mut().push("check");
                    current
                },
                || {
                    assert!(pointer.try_lock().is_err(), "the press holds the pointer");
                    log.borrow_mut().push("press");
                },
            )
        };
        assert_eq!(run(true), Some(()));
        assert_eq!(*log.borrow(), ["check", "press"]);
        log.borrow_mut().clear();
        assert_eq!(run(false), None);
        assert_eq!(*log.borrow(), ["check"]);
        assert!(
            pointer.try_lock().is_ok(),
            "the pointer is free for a refresh"
        );
    }

    // REQ-ACT-03: clicks from different sessions take turns with the one
    // pointer instead of interleaving their moves and presses.
    #[test]
    fn req_act_03_two_clicks_never_hold_the_pointer_at_once() {
        use std::sync::atomic::AtomicUsize;
        let pointer = Arc::new(Mutex::new(()));
        let active = Arc::new(AtomicUsize::new(0));
        let most = Arc::new(AtomicUsize::new(0));
        let clicks: Vec<_> = (0..4)
            .map(|_| {
                let (pointer, active, most) =
                    (Arc::clone(&pointer), Arc::clone(&active), Arc::clone(&most));
                std::thread::spawn(move || {
                    checked_click(
                        &pointer,
                        || true,
                        || {
                            let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                            most.fetch_max(now, Ordering::SeqCst);
                            std::thread::sleep(Duration::from_millis(5));
                            active.fetch_sub(1, Ordering::SeqCst);
                        },
                    )
                })
            })
            .collect();
        for click in clicks {
            click.join().expect("a click thread");
        }
        assert_eq!(most.load(Ordering::SeqCst), 1);
    }

    // REQ-ACT-02: a first click that failed is no click, so a double Return
    // sends no second one.
    #[test]
    fn req_act_02_a_failed_first_click_is_not_recorded_as_clicked() {
        let mut entry = second_click_entry(false, 2);
        record_first_click(&mut entry, false, false);
        assert!(!entry.clicked);
        assert_eq!(entry.pending, 1, "the second click is still pending");
        record_first_click(&mut entry, true, true);
        assert!(entry.clicked && entry.workspace_changed);
    }

    // REQ-ACT-02 and REQ-ACT-03: the second click of a double click goes
    // only after a first click that happened; it is skipped at once when the
    // first attempt ended without a click.
    #[test]
    fn req_act_02_second_click_is_not_sent_without_the_first() {
        let daemon = second_click_daemon(second_click_entry(false, 1), "item");
        let started = Instant::now();
        assert_eq!(
            run_second_click(&daemon, false),
            0,
            "no second click without the first"
        );
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "no wait for a click that ended"
        );
        assert!(
            daemon.sessions.lock().unwrap().get("s1").is_none(),
            "the session ends"
        );
    }

    // REQ-SESSION-02: a session that closes while the second click waits for
    // the first wakes it at once, and nothing is clicked.
    #[test]
    fn req_session_02_closing_wakes_a_waiting_second_click() {
        let daemon = second_click_daemon(second_click_entry(false, 2), "item");
        let closer = {
            let daemon = Arc::clone(&daemon);
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(50));
                daemon
                    .sessions
                    .lock()
                    .unwrap()
                    .get_mut("s1")
                    .unwrap()
                    .closing = true;
                daemon.changed.notify_all();
            })
        };
        let started = Instant::now();
        assert_eq!(run_second_click(&daemon, false), 0);
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "closing ends the wait"
        );
        closer.join().unwrap();
    }

    // REQ-ACT-03: the second click rechecks its target as the first did: a
    // target that moved or became covered, or a bar item that was replaced,
    // gets no second click, and a current one gets exactly one.
    #[test]
    fn req_act_03_second_click_rechecks_its_target() {
        for (item, moved, expected) in [
            ("item", true, 0),
            ("replacement", false, 0),
            ("item", false, 1),
        ] {
            let daemon = second_click_daemon(second_click_entry(true, 1), item);
            assert_eq!(
                run_second_click(&daemon, moved),
                expected,
                "item {item}, moved {moved}"
            );
            assert!(daemon.sessions.lock().unwrap().get("s1").is_none());
        }
    }

    // REQ-ACT-03: putting the pointer back when a session ends holds the
    // pointer, so it cannot move another session's action mid-way.
    #[test]
    fn req_act_03_putting_the_pointer_back_holds_the_pointer() {
        let mut entry = second_click_entry(true, 0);
        entry.restore = Some((click_monitor(), (5, 5)));
        let daemon = daemon_with_entry(entry);
        let moved = Cell::new(false);
        daemon.finish_with("s1", |_, point| {
            assert!(
                daemon.pointer_action.try_lock().is_err(),
                "the restore holds the pointer"
            );
            assert_eq!(point, (5, 5));
            moved.set(true);
            Ok(())
        });
        assert!(moved.get(), "the pointer went back");
        assert!(daemon.sessions.lock().unwrap().get("s1").is_none());
    }

    // REQ-ACT-03: a window target whose window and click point are unchanged
    // is still refused when its element moved, and the element is read only
    // once the window checks pass.
    #[test]
    fn req_act_03_a_moved_element_refuses_the_click() {
        let window = Window {
            address: "0x1".into(),
            at: (0, 0),
            size: (200, 200),
            workspace: 1,
            ..Window::default()
        };
        let target = Target {
            id: "0x1:a0".into(),
            bounds: Rect::new(10, 10, 20, 20),
            role: Role::Button,
            text: "Send".into(),
            source: TargetSource::Atspi,
            actions: vec![ActionKind::Click],
            window: "0x1".into(),
            check: Some(thodu_core::model::ElementCheck {
                at: thodu_core::model::ElementRef {
                    bus: ":1.5".into(),
                    path: "/send".into(),
                },
                extents: [10, 10, 20, 20],
            }),
        };
        let owner = thodu_core::model::Owner::Window("0x1".into());
        let read = Cell::new(0);
        let element = |moved: bool| {
            let read = &read;
            move |_: &thodu_core::model::ElementCheck| {
                read.set(read.get() + 1);
                moved
            }
        };
        assert!(!moved_or_covered_given(
            &target,
            Some(&window),
            Some(&window),
            &owner,
            element(false)
        ));
        assert!(moved_or_covered_given(
            &target,
            Some(&window),
            Some(&window),
            &owner,
            element(true)
        ));
        assert_eq!(read.get(), 2);
        let covered = thodu_core::model::Owner::Layer("omarchy-notifications".into());
        assert!(moved_or_covered_given(
            &target,
            Some(&window),
            Some(&window),
            &covered,
            element(false)
        ));
        assert_eq!(
            read.get(),
            2,
            "a covered point refuses before reading the element"
        );
    }

    // REQ-ACT-02: a lingering click on the bar, through a bar target or a
    // grid cell over it, is watched for a workspace switch; other clicks are
    // not delayed by the watch.
    #[test]
    fn req_act_02_clicks_on_the_bar_watch_for_a_workspace_switch() {
        assert!(watches_workspace(true, true));
        assert!(!watches_workspace(true, false), "a click off the bar");
        assert!(
            !watches_workspace(false, true),
            "a click that does not linger"
        );
    }

    // REQ-SEARCH-01: a letter typed with Control is a shortcut meant for an
    // app, so it neither types the label nor acts, and the session stays.
    #[test]
    fn req_search_01_control_chord_neither_types_a_label_nor_acts() {
        let monitor = Monitor {
            name: "M".into(),
            x: 0,
            y: 0,
            width: 100,
            height: 100,
            scale: 1.0,
            layers: Vec::new(),
        };
        let target = Target {
            id: "a0".into(),
            bounds: Rect::new(10, 10, 20, 20),
            role: Role::Button,
            text: "Open".into(),
            source: TargetSource::Atspi,
            actions: vec![ActionKind::Click],
            window: "0x1".into(),
            check: None,
        };
        let session = Session::new(vec![target], false).with_area(monitor.logical());
        let daemon = daemon_with_entry(Entry {
            session,
            window: None,
            monitor,
            status: "1 target from atspi".into(),
            collecting: false,
            overlay_bounds: None,
            overlay: true,
            test: true,
            hidden: false,
            created: Instant::now(),
            timing: Timing::default(),
            pending: 0,
            clicked: false,
            workspace_changed: false,
            closing: false,
            restore: None,
            scroll_restore: None,
            scroll_released: true,
        });

        for (name, shift) in [("A", true), ("a", true), ("a", false), ("space", false)] {
            let reply = daemon.key("s1", name, shift, true, 0).unwrap();
            let view = reply.view.expect("a chord returns the current view");
            assert_eq!(
                view.state,
                State::Showing,
                "{name} with Control did not act"
            );
        }
        let sessions = daemon.sessions.lock().unwrap();
        let entry = sessions.get("s1").unwrap();
        assert_eq!(entry.session.state, State::Showing);
        assert_eq!(entry.pending, 0, "no action was queued");
        drop(sessions);
        assert_eq!(
            parse_key("Return", false, true),
            Some(Key::Return {
                shift: false,
                ctrl: true
            }),
            "REQ-ACT-02: Control-Return still parses as a Control click"
        );
    }

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
            ..Window::default()
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

    // REQ-SRC-07: an action uses the window that produced the target,
    // instead of the window that happened to start the session.
    #[test]
    fn req_src_07_action_uses_the_target_window_address() {
        let mut second = on(0);
        second.address = "0x2".into();
        let scope = WindowScope {
            windows: HashMap::from([
                (on(0).address.clone(), on(0)),
                (second.address.clone(), second),
            ]),
            refetch_address: None,
        };
        let target = Target {
            id: "target".into(),
            bounds: Rect::new(10, 10, 20, 20),
            role: Role::Button,
            text: "target".into(),
            source: TargetSource::Atspi,
            actions: vec![ActionKind::Click],
            window: "0x2".into(),
            check: None,
        };

        assert_eq!(
            window_for_target(Some(&scope), &target).map(|window| window.address.as_str()),
            Some("0x2")
        );
    }

    // REQ-SRC-07: a stale target refresh retains a request's single-window scope.
    #[test]
    fn req_src_07_stale_refresh_preserves_single_window_scope() {
        let window = on(0);
        assert_eq!(
            restart_address(false, "visible", std::slice::from_ref(&window)),
            None
        );
        assert_eq!(
            restart_address(false, "focused", std::slice::from_ref(&window)),
            Some(window.address.clone())
        );
        assert_eq!(
            restart_address(true, "visible", std::slice::from_ref(&window)),
            Some(window.address)
        );
    }

    // REQ-SESSION-01: refresh keeps the original overlay and test settings.
    #[test]
    fn req_session_01_refetch_preserves_overlay_setting() {
        for test in [false, true] {
            let monitor = Monitor {
                name: "M".into(),
                x: 0,
                y: 0,
                width: 100,
                height: 100,
                scale: 1.0,
                layers: Vec::new(),
            };
            let entry = Entry {
                session: Session::begin(Vec::new(), monitor.logical(), false, false),
                window: None,
                monitor,
                status: String::new(),
                collecting: false,
                overlay_bounds: None,
                overlay: false,
                test,
                hidden: false,
                created: Instant::now(),
                timing: Timing::default(),
                pending: 0,
                clicked: false,
                workspace_changed: false,
                closing: false,
                restore: None,
                scroll_restore: None,
                scroll_released: true,
            };

            let actual = start_refreshed_session(
                refresh_options(&entry),
                Some("0x1".into()),
                |window, scope, mode, overlay, test| {
                    (window, scope.to_string(), mode.to_string(), overlay, test)
                },
            );
            assert_eq!(
                actual,
                (
                    Some("0x1".into()),
                    "visible".into(),
                    "hints".into(),
                    false,
                    test
                )
            );
        }
    }

    // REQ-SRC-07: if another visible window moves over a target after
    // collection, activation refreshes instead of clicking the occluder.
    #[test]
    fn req_src_07_visible_target_refreshes_when_another_window_owns_its_click_point() {
        let target = Target {
            id: "a0".into(),
            bounds: Rect::new(20, 20, 20, 20),
            role: Role::Button,
            text: "Open".into(),
            source: TargetSource::Atspi,
            actions: vec![ActionKind::Click],
            window: "0x1".into(),
            check: None,
        };

        assert!(!visible_target_owner_changed(&target, Some("0x1")));
        assert!(visible_target_owner_changed(&target, Some("0x2")));
        assert!(visible_target_owner_changed(&target, None));
        assert!(visible_target_owner_changed(&target, Some("0x2")));
    }

    // REQ-SRC-07: a stale shell target refreshes before the activation callback,
    // using the same dispatch function called by Daemon::act.
    #[test]
    fn req_src_07_shell_staleness_triggers_activation_refresh() {
        let monitor = Monitor {
            name: "M".into(),
            x: 0,
            y: 0,
            width: 100,
            height: 50,
            scale: 1.0,
            layers: Vec::new(),
        };
        let item = thodu_core::protocol::ShellItem {
            identity: "tray-item".into(),
            x: 10,
            y: 2,
            w: 20,
            h: 20,
            text: "workspace".into(),
        };
        let shell_target = Target {
            id: "b:tray-item".into(),
            bounds: Rect::new(20, 2, 20, 20),
            role: Role::Button,
            text: "workspace".into(),
            source: TargetSource::Shell,
            actions: vec![ActionKind::Click],
            window: String::new(),
            check: None,
        };
        let shell = sources::Shell {
            items: Mutex::new(HashMap::from([(
                monitor.name.clone(),
                vec![thodu_core::protocol::ShellItem {
                    identity: "replacement".into(),
                    ..item.clone()
                }],
            )])),
            budget: Duration::ZERO,
            layer_origin: |_, _| Some((10, 0)),
        };
        assert!(
            !target_is_current(&shell_target, &shell, &monitor, || false),
            "a stale target must not reach pointer activation"
        );

        shell
            .items
            .lock()
            .unwrap()
            .insert(monitor.name.clone(), vec![item]);
        assert!(
            target_is_current(&shell_target, &shell, &monitor, || false),
            "a current target may reach pointer activation"
        );
        assert!(
            !target_is_current(&shell_target, &shell, &monitor, || true),
            "REQ-SRC-07: a covered bar target is not clicked"
        );
    }

    // REQ-SRC-07: defaults include every visible window; focused and named
    // requests stay limited to the active window.
    #[test]
    fn req_src_07_request_scope_selects_visible_or_single_window() {
        let active = on(0);
        let mut other = on(0);
        other.address = "0x2".into();
        let visible = vec![other.clone(), active.clone()];

        let all = scoped_windows(
            &active,
            visible.clone(),
            false,
            "visible",
            Some(&active.address),
        );
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].address, active.address);
        assert!(all[0].focused);
        assert_eq!(all[1].address, other.address);
        assert!(!all[1].focused);
        assert_eq!(
            scoped_windows(
                &active,
                visible.clone(),
                false,
                "focused",
                Some(&active.address)
            )
            .len(),
            1
        );
        let named = scoped_windows(&other, visible, true, "visible", Some(&active.address));
        assert_eq!(named.len(), 1);
        assert_eq!(named[0].address, other.address);
        assert!(!named[0].focused);
    }

    // REQ-ACT-02: the focus handoff reply waits until the first click has
    // completed and reports whether exclusive keyboard focus can be reclaimed.
    #[test]
    fn req_act_02_hidden_reply_waits_for_lingering_click() {
        let target = Target {
            id: "target".into(),
            bounds: Rect::new(10, 10, 20, 20),
            role: Role::Button,
            text: "target".into(),
            source: TargetSource::Shell,
            actions: vec![ActionKind::Click],
            window: String::new(),
            check: None,
        };
        let mut session = Session::new(vec![target], true);
        session.focused = Some(0);
        assert!(
            session
                .key_at(
                    Key::Return {
                        shift: false,
                        ctrl: false,
                    },
                    0,
                )
                .is_some()
        );
        let daemon = daemon_with_entry(Entry {
            session,
            window: None,
            monitor: Monitor {
                name: "M".into(),
                x: 0,
                y: 0,
                width: 100,
                height: 100,
                scale: 1.0,
                layers: Vec::new(),
            },
            status: String::new(),
            collecting: false,
            overlay_bounds: None,
            overlay: true,
            test: false,
            hidden: false,
            created: Instant::now(),
            timing: Timing::default(),
            pending: 1,
            clicked: false,
            workspace_changed: true,
            closing: false,
            restore: None,
            scroll_restore: None,
            scroll_released: true,
        });
        let worker = Arc::clone(&daemon);
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || tx.send(worker.hidden("s1")).unwrap());

        assert!(matches!(
            rx.recv_timeout(Duration::from_millis(20)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        {
            let mut sessions = daemon.sessions.lock().unwrap();
            let entry = sessions.get_mut("s1").unwrap();
            assert!(entry.hidden, "the action is released while the reply waits");
            entry.clicked = true;
        }
        daemon.changed.notify_all();

        let reply = rx.recv_timeout(Duration::from_secs(1)).unwrap().unwrap();
        assert_eq!(reply.session.as_deref(), Some("s1"));
        assert_eq!(reply.clicked, Some(true));
        assert_eq!(reply.workspace_changed, Some(true));
    }

    // REQ-SEARCH-01: generated uppercase key text survives IPC parsing.
    #[test]
    fn req_search_01_uppercase_key_text_marks_label_input() {
        assert_eq!(parse_key("A", false, false), Some(Key::Upper { c: 'A' }));
        assert_eq!(
            parse_key("a", false, false),
            Some(Key::Char {
                c: 'a',
                shift: false
            })
        );
    }

    // REQ-ACT-01: shifted lowercase key text is the same uppercase key as the
    // text the overlay sends, which selects a label without clicking.
    #[test]
    fn req_act_01_shifted_lowercase_key_text_is_uppercase() {
        assert_eq!(parse_key("d", true, false), Some(Key::Upper { c: 'D' }));
    }
}
