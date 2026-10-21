//! The accessibility bus, read into the `Snapshot` thodu-core replays.
//!
//! Only the frames of the window's own process are walked (REQ-SRC-01);
//! a subtree that is not SHOWING and VISIBLE is not descended into, and the
//! walk stops at a node cap or a deadline, so one slow application cannot
//! hold a session up.

use std::time::{Duration, Instant};

use thodu_core::model::{Monitor, Window};
use thodu_core::tree::{App, Node, Snapshot};
use zbus::blocking::Connection;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

const ACCESSIBLE: &str = "org.a11y.atspi.Accessible";
const COMPONENT: &str = "org.a11y.atspi.Component";
const COORD_WINDOW: u32 = 1;
const MAX_NODES: usize = 4000;
const MAX_DEPTH: usize = 48;
/// AT-SPI state numbers (atspi-constants.h) that targets depend on.
const STATES: [(u32, &str); 11] = [
    (1, "active"),
    (4, "checked"),
    (6, "defunct"),
    (7, "editable"),
    (8, "enabled"),
    (11, "focusable"),
    (12, "focused"),
    (16, "modal"),
    (24, "sensitive"),
    (25, "showing"),
    (30, "visible"),
];

pub struct Bus {
    conn: Connection,
}

type Ref = (String, OwnedObjectPath);

struct Walk {
    deadline: Instant,
    nodes: usize,
    truncated: bool,
}

impl Bus {
    pub fn connect() -> Result<Self, String> {
        let session = Connection::session().map_err(|e| format!("no session bus: {e}"))?;
        let address: String = session
            .call_method(
                Some("org.a11y.Bus"),
                "/org/a11y/bus",
                Some("org.a11y.Bus"),
                "GetAddress",
                &(),
            )
            .and_then(|m| m.body().deserialize())
            .map_err(|e| format!("no accessibility bus: {e}"))?;
        let conn = zbus::blocking::connection::Builder::address(address.as_str())
            .map_err(|e| e.to_string())?
            .method_timeout(Duration::from_millis(500))
            .build()
            .map_err(|e| format!("cannot join the accessibility bus: {e}"))?;
        Ok(Self { conn })
    }

    fn call<B, T>(&self, (dest, path): &Ref, iface: &str, method: &str, body: &B) -> Option<T>
    where
        B: serde::Serialize + zbus::zvariant::DynamicType,
        T: for<'d> zbus::zvariant::DynamicDeserialize<'d>,
    {
        self.conn
            .call_method(
                Some(dest.as_str()),
                path.as_str(),
                Some(iface),
                method,
                body,
            )
            .ok()?
            .body()
            .deserialize()
            .ok()
    }

    fn property(&self, at: &Ref, name: &str) -> String {
        self.call::<_, OwnedValue>(
            at,
            "org.freedesktop.DBus.Properties",
            "Get",
            &(ACCESSIBLE, name),
        )
        .and_then(|v| String::try_from(v).ok())
        .unwrap_or_default()
    }

    fn children(&self, at: &Ref) -> Vec<Ref> {
        self.call::<_, Vec<Ref>>(at, ACCESSIBLE, "GetChildren", &())
            .unwrap_or_default()
    }

    fn pid(&self, bus: &str) -> Option<u32> {
        self.conn
            .call_method(
                Some("org.freedesktop.DBus"),
                "/org/freedesktop/DBus",
                Some("org.freedesktop.DBus"),
                "GetConnectionUnixProcessID",
                &(bus,),
            )
            .ok()?
            .body()
            .deserialize()
            .ok()
    }

    fn states(&self, at: &Ref) -> Vec<String> {
        let words: Vec<u32> = self
            .call(at, ACCESSIBLE, "GetState", &())
            .unwrap_or_default();
        STATES
            .iter()
            .filter(|(bit, _)| {
                words
                    .get(*bit as usize / 32)
                    .is_some_and(|w| w & (1 << (bit % 32)) != 0)
            })
            .map(|(_, name)| (*name).to_string())
            .collect()
    }

    fn node(&self, at: &Ref, depth: usize, walk: &mut Walk) -> Node {
        walk.nodes += 1;
        let states = self.states(at);
        let mut node = Node {
            role: self
                .call(at, ACCESSIBLE, "GetRoleName", &())
                .unwrap_or_default(),
            name: self.property(at, "Name"),
            description: self.property(at, "Description"),
            extents: self
                .call::<_, (i32, i32, i32, i32)>(at, COMPONENT, "GetExtents", &(COORD_WINDOW,))
                .map(|(x, y, w, h)| [x, y, w, h]),
            states,
            children: Vec::new(),
        };
        let shown = node.has("showing") && node.has("visible");
        if !shown || depth >= MAX_DEPTH {
            return node;
        }
        for child in self.children(at) {
            if walk.nodes >= MAX_NODES || Instant::now() >= walk.deadline {
                walk.truncated = true;
                break;
            }
            node.children.push(self.node(&child, depth + 1, walk));
        }
        node
    }

    /// The applications of `window`'s process with their frames, read within
    /// `budget`. Returns the snapshot and whether the walk was cut short.
    pub fn snapshot(
        &self,
        window: &Window,
        monitor: &Monitor,
        budget: Duration,
    ) -> (Snapshot, bool) {
        let root: Ref = (
            "org.a11y.atspi.Registry".into(),
            OwnedObjectPath::try_from("/org/a11y/atspi/accessible/root").expect("a valid path"),
        );
        let mut walk = Walk {
            deadline: Instant::now() + budget,
            nodes: 0,
            truncated: false,
        };
        let mut apps = Vec::new();
        for app in self.children(&root) {
            if self.pid(&app.0) != Some(window.pid) {
                continue;
            }
            let frames = self
                .children(&app)
                .iter()
                .map(|frame| self.node(frame, 0, &mut walk))
                .collect();
            apps.push(App {
                name: self.property(&app, "Name"),
                pid: window.pid,
                frames,
            });
        }
        (
            Snapshot {
                window: window.clone(),
                monitor: monitor.clone(),
                apps,
            },
            walk.truncated,
        )
    }
}
