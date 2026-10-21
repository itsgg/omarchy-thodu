//! The IPC of SPEC section 7: JSON lines over $XDG_RUNTIME_DIR/thodu.sock,
//! one request and one reply per line, each with `v` equal to 1.

use serde::{Deserialize, Serialize};

use crate::model::{Rect, Role, Source};
use crate::session::State;

pub const VERSION: u32 = 1;
pub const SOCKET_NAME: &str = "thodu.sock";
/// The label size `thodud` places labels for, in logical units: the overlay
/// draws them in this box.
pub const LABEL_HEIGHT: i32 = 18;
pub const LABEL_CHAR_WIDTH: i32 = 9;
pub const LABEL_PADDING: i32 = 8;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
pub enum Request {
    /// Start a session on the focused window, or on `window` (an address).
    /// `test` keeps the overlay from taking the keyboard, for live checks.
    Start {
        #[serde(default = "hints")]
        mode: String,
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
    Targets {
        #[serde(default)]
        window: Option<String>,
    },
    Close {
        session: String,
    },
}

fn hints() -> String {
    "hints".into()
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
    pub label: String,
    /// Where the label is drawn (REQ-LABEL-02).
    pub placed: Rect,
}

/// The parts of a session the overlay draws.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct View {
    pub state: State,
    pub visible: Vec<String>,
    pub focused: Option<String>,
    pub search: String,
    pub label_prefix: String,
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
