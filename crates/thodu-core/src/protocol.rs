//! The IPC of SPEC section 7: JSON lines over $XDG_RUNTIME_DIR/thodu.sock,
//! one request and one reply per line, each with `v` equal to 1.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::model::{ActionKind, Rect, Role, Source};
use crate::session::State;

pub const VERSION: u32 = 1;
pub const SOCKET_NAME: &str = "thodu.sock";

/// Hyprland's request socket for one compositor instance.
pub fn hyprland_socket_path(runtime: &Path, signature: &str) -> PathBuf {
    runtime.join("hypr").join(signature).join(".socket.sock")
}
/// The label size `thodud` places labels for, in logical units: the overlay
/// draws them in this box.
pub const LABEL_HEIGHT: i32 = 18;
pub const LABEL_CHAR_WIDTH: i32 = 9;
pub const LABEL_PADDING: i32 = 8;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
pub enum Request {
    /// Start a session on visible windows by default, or on the active window
    /// with `scope: focused`. An explicit address narrows it to one window.
    /// `test` keeps the overlay from taking the keyboard, for live checks.
    Start {
        #[serde(default = "hints")]
        mode: String,
        #[serde(default = "visible")]
        scope: String,
        #[serde(default)]
        window: Option<String>,
        #[serde(default = "yes")]
        overlay: bool,
        #[serde(default)]
        test: bool,
    },
    /// The session's monitor, targets and view; `brief` leaves out the
    /// monitor and targets, for the overlay's heartbeat.
    Session {
        session: String,
        #[serde(default)]
        brief: bool,
    },
    Key {
        session: String,
        generation: u32,
        key: String,
        #[serde(default)]
        shift: bool,
        #[serde(default)]
        ctrl: bool,
    },
    /// The overlay is gone; a pending action may run.
    Hidden {
        session: String,
    },
    /// The overlay drew the session's labels for the first time.
    Drawn {
        session: String,
        /// Exact screen-logical UI rectangles, in compositor coordinates.
        #[serde(default)]
        overlays: Option<Vec<Rect>>,
    },
    /// The overlay's standing connection: `thodud` pushes each new session
    /// down it as an [`OpenEvent`] instead of summoning the overlay.
    Listen,
    /// The targets in the requested scope; defaults to all visible windows.
    Targets {
        #[serde(default)]
        window: Option<String>,
        #[serde(default = "visible")]
        scope: String,
    },
    /// The bar's clickable items on the monitor `screen`, from the overlay
    /// plugin's bar widget, in the bar window's coordinates (REQ-SHELL-01).
    ShellItems {
        screen: String,
        items: Vec<ShellItem>,
    },
    /// The window's process's whole accessibility tree, for record-tree.
    Record {
        #[serde(default)]
        window: Option<String>,
    },
    Close {
        session: String,
    },
    /// Stop the current daemon so systemd can take over its socket.
    Shutdown,
}

/// One of the bar's clickable items, relative to the bar's window.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellItem {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    #[serde(default)]
    pub text: String,
}

fn hints() -> String {
    "hints".into()
}

fn visible() -> String {
    "visible".into()
}

const fn yes() -> bool {
    true
}

/// Wraps a request with the protocol version.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Envelope {
    pub v: u32,
    #[serde(flatten)]
    pub request: Request,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MonitorInfo {
    pub name: String,
    /// The monitor's logical area.
    pub area: Rect,
    pub scale: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LabeledTarget {
    pub id: String,
    pub bounds: Rect,
    pub role: Role,
    pub text: String,
    pub source: Source,
    #[serde(default)]
    pub actions: Vec<ActionKind>,
    #[serde(default)]
    pub window: String,
    pub label: String,
    /// Where the label is drawn (REQ-LABEL-02).
    pub placed: Rect,
}

/// How long a session took to appear (REQ-PERF-01).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Timing {
    /// Collecting the targets, in milliseconds.
    pub collect_ms: u64,
    /// When the overlay said it drew the labels, in microseconds since the
    /// Unix epoch, so a caller in another process can measure from its own
    /// start.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub drawn_at_us: Option<u64>,
}

/// What `thodud` pushes to a listening overlay when a session starts: the
/// session's full reply, so the overlay draws it with no further request.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OpenEvent {
    pub v: u32,
    pub event: String,
    pub session: String,
    pub test: bool,
    pub reply: Reply,
}

impl LabeledTarget {
    /// A target with no label, as `thodu targets` reports it.
    pub fn unlabeled(t: crate::model::Target) -> Self {
        Self {
            id: t.id,
            bounds: t.bounds,
            role: t.role,
            text: t.text,
            source: t.source,
            actions: t.actions,
            window: t.window,
            label: String::new(),
            placed: t.bounds,
        }
    }

    /// The line `thodu targets --json` prints for this target (REQ-CLI-01):
    /// the shape of SPEC section 5, without the label.
    pub fn json_line(&self) -> String {
        serde_json::json!({
            "id": self.id,
            "bounds": self.bounds,
            "role": self.role,
            "text": self.text,
            "source": self.source,
            "actions": self.actions,
            "window": self.window,
        })
        .to_string()
    }
}

/// The parts of a session the overlay draws.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct View {
    pub state: State,
    pub visible: Vec<String>,
    pub focused: Option<String>,
    /// Whether lowercase letters type labels before `/` starts a search.
    #[serde(default)]
    pub labels_only: bool,
    /// Whether the focus came from completing a label (REQ-SESSION-01).
    #[serde(default)]
    pub label_selected: bool,
    pub search: String,
    /// With labels_only, whether / started a search (REQ-SEARCH-02).
    #[serde(default)]
    pub searching: bool,
    pub label_prefix: String,
    /// A visual fallback is still adding targets to the session.
    #[serde(default)]
    pub collecting: bool,
    /// Bumped when the targets change, as the grid's do; the overlay then
    /// fetches them.
    #[serde(default)]
    pub generation: u32,
    /// Acting, with the overlay keeping the keyboard for a second Return
    /// (REQ-ACT-02).
    #[serde(default)]
    pub linger: bool,
    /// The session uses pane selection and wheel keys instead of hint actions.
    #[serde(default)]
    pub scroll_mode: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorKind {
    NoWindow,
    NoTree,
    Busy,
    Stale,
    Internal,
    BadRequest,
}

/// A reply: `ok` with the fields its request asks for, or an error.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Reply {
    pub v: u32,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub error: Option<ErrorKind>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub session: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub monitor: Option<MonitorInfo>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub targets: Option<Vec<LabeledTarget>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub view: Option<View>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub timing: Option<Timing>,
    /// For a lingering action, the first click attempt is complete.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub clicked: Option<bool>,
    /// The click changed the active workspace, so the overlay should end
    /// without reclaiming keyboard focus.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub workspace_changed: Option<bool>,
    /// A recorded snapshot, in the shape of spec/cases/atspi/*.json.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub snapshot: Option<serde_json::Value>,
}

impl Reply {
    pub fn ok() -> Self {
        Self {
            v: VERSION,
            ok: true,
            ..Self::default()
        }
    }

    pub fn error(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            v: VERSION,
            ok: false,
            error: Some(kind),
            message: Some(message.into()),
            ..Self::default()
        }
    }
}

/// The label box for a label of `len` letters.
pub const fn label_size(len: usize) -> (i32, i32) {
    (len as i32 * LABEL_CHAR_WIDTH + LABEL_PADDING, LABEL_HEIGHT)
}
