//! The accessibility bus, read into the `Snapshot` thodu-core replays.
//!
//! Only the frames of the window's own process are read (REQ-SRC-01), and
//! only the frame that matches the window is walked below its top. A subtree
//! that is not SHOWING and VISIBLE is not descended into, and the walk stops
//! at a node cap or a deadline, so one slow application cannot hold a
//! session up. Each level of the tree is read with all its calls in flight
//! at once, so a walk costs a round trip per level, not per call.
//!
//! Chromium and Electron keep their tree dormant until something asks for
//! it: a frame whose only child is the null object is woken with one
//! GetAttributes call, once per process (REQ-SRC-03).

use std::collections::HashSet;
use std::time::{Duration, Instant};

use futures_util::future::join_all;
use thodu_core::model::{Monitor, Window};
use thodu_core::tree::{self, App, Node, Snapshot};
use zbus::Connection;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

const ACCESSIBLE: &str = "org.a11y.atspi.Accessible";
const COMPONENT: &str = "org.a11y.atspi.Component";
const ACTION: &str = "org.a11y.atspi.Action";
const NULL_PATH: &str = "/org/a11y/atspi/null";
const COORD_WINDOW: u32 = 1;
const MAX_NODES: usize = 4000;
const MAX_DEPTH: usize = 48;
/// How long a woken tree gets to fill before it is read.
const WAKE_POLL: Duration = Duration::from_millis(10);
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

pub type Ref = (String, OwnedObjectPath);

/// What the walk needs from the bus, so a fake can stand in for it.
pub trait Tree {
    /// The applications on the bus.
    fn apps(&self) -> Vec<Ref>;
    fn pid(&self, app: &Ref) -> Option<u32>;
    fn name(&self, at: &Ref) -> String;
    /// One node without its children, and the references to them; all of
    /// `at`'s calls in flight at once.
    fn read(&self, at: &[Ref]) -> Vec<(Node, Vec<Ref>)>;
    /// The children of `at`, the null object included.
    fn children(&self, at: &Ref) -> Vec<Ref>;
    /// The call that wakes a dormant Chromium tree.
    fn attributes(&self, at: &Ref);
}

pub struct Bus {
    conn: Connection,
}

fn is_null(r: &Ref) -> bool {
    r.1.as_str() == NULL_PATH
}

/// After waking a dormant tree, an empty child list is ready to read too.
fn children_ready(children: &[Ref]) -> bool {
    children.is_empty() || !children.iter().all(is_null)
}

impl Bus {
    pub fn connect() -> Result<Self, String> {
        let session =
            zbus::blocking::Connection::session().map_err(|e| format!("no session bus: {e}"))?;
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
        let conn = futures_lite::future::block_on(async {
            zbus::connection::Builder::address(address.as_str())?
                .method_timeout(Duration::from_millis(500))
                .build()
                .await
        })
        .map_err(|e| format!("cannot join the accessibility bus: {e}"))?;
        Ok(Self { conn })
    }

    async fn call<B, T>(&self, (dest, path): &Ref, iface: &str, method: &str, body: &B) -> Option<T>
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
            .await
            .ok()?
            .body()
            .deserialize()
            .ok()
    }

    async fn property(&self, at: &Ref, name: &str) -> String {
        self.call::<_, OwnedValue>(
            at,
            "org.freedesktop.DBus.Properties",
            "Get",
            &(ACCESSIBLE, name),
        )
        .await
        .and_then(|v| String::try_from(v).ok())
        .unwrap_or_default()
    }

    async fn states(&self, at: &Ref) -> Vec<String> {
        let words: Vec<u32> = self
            .call(at, ACCESSIBLE, "GetState", &())
            .await
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

    async fn one(&self, at: &Ref) -> (Node, Vec<Ref>) {
        let (role, name, description, states, extents, action, children) = futures_util::join!(
            self.call::<_, String>(at, ACCESSIBLE, "GetRoleName", &()),
            self.property(at, "Name"),
            self.property(at, "Description"),
            self.states(at),
            self.call::<_, (i32, i32, i32, i32)>(at, COMPONENT, "GetExtents", &(COORD_WINDOW,)),
            self.call::<_, String>(at, ACTION, "GetName", &(0i32,)),
            self.call::<_, Vec<Ref>>(at, ACCESSIBLE, "GetChildren", &()),
        );
        let node = Node {
            role: role.unwrap_or_default(),
            states,
            name,
            description,
            extents: extents.map(|(x, y, w, h)| [x, y, w, h]),
            action: action.unwrap_or_default(),
            children: Vec::new(),
        };
        (node, children.unwrap_or_default())
    }
}

impl Tree for Bus {
    fn apps(&self) -> Vec<Ref> {
        let root: Ref = (
            "org.a11y.atspi.Registry".into(),
            OwnedObjectPath::try_from("/org/a11y/atspi/accessible/root").expect("a valid path"),
        );
        self.children(&root)
    }

    fn pid(&self, app: &Ref) -> Option<u32> {
        futures_lite::future::block_on(async {
            self.conn
                .call_method(
                    Some("org.freedesktop.DBus"),
                    "/org/freedesktop/DBus",
                    Some("org.freedesktop.DBus"),
                    "GetConnectionUnixProcessID",
                    &(app.0.as_str(),),
                )
                .await
                .ok()?
                .body()
                .deserialize()
                .ok()
        })
    }

    fn name(&self, at: &Ref) -> String {
        futures_lite::future::block_on(self.property(at, "Name"))
    }

    fn read(&self, at: &[Ref]) -> Vec<(Node, Vec<Ref>)> {
        futures_lite::future::block_on(join_all(at.iter().map(|r| self.one(r))))
    }

    fn children(&self, at: &Ref) -> Vec<Ref> {
        futures_lite::future::block_on(self.call::<_, Vec<Ref>>(at, ACCESSIBLE, "GetChildren", &()))
            .unwrap_or_default()
    }

    fn attributes(&self, at: &Ref) {
        let _: Option<std::collections::HashMap<String, String>> =
            futures_lite::future::block_on(self.call(at, ACCESSIBLE, "GetAttributes", &()));
    }
}

/// A walk's limits and what it cost.
pub struct Walk {
    deadline: Instant,
    nodes: usize,
    pub truncated: bool,
}

impl Walk {
    pub fn new(budget: Duration) -> Self {
        Self {
            deadline: Instant::now() + budget,
            nodes: 0,
            truncated: false,
        }
    }
}

/// Read `roots` and every shown subtree below them, a level at a time.
fn walk_from(bus: &dyn Tree, roots: Vec<Ref>, walk: &mut Walk) -> Vec<Node> {
    // Arena of read nodes: the node, its parent's index, its depth.
    let mut arena: Vec<(Node, Option<usize>, usize)> = Vec::new();
    let mut level: Vec<(Ref, Option<usize>, usize)> =
        roots.into_iter().map(|r| (r, None, 0)).collect();
    while !level.is_empty() {
        let room = MAX_NODES.saturating_sub(walk.nodes);
        if room == 0 || Instant::now() >= walk.deadline {
            walk.truncated = true;
            break;
        }
        if level.len() > room {
            level.truncate(room);
            walk.truncated = true;
        }
        let refs: Vec<Ref> = level.iter().map(|(r, _, _)| r.clone()).collect();
        let read = bus.read(&refs);
        walk.nodes += read.len();
        let mut next = Vec::new();
        for ((_, parent, depth), (node, children)) in level.into_iter().zip(read) {
            let index = arena.len();
            let descend = node.has("showing") && node.has("visible") && depth < MAX_DEPTH;
            arena.push((node, parent, depth));
            if descend {
                next.extend(
                    children
                        .into_iter()
                        .filter(|c| !is_null(c))
                        .map(|c| (c, Some(index), depth + 1)),
                );
            }
        }
        level = next;
    }
    // Children come after their parents in the arena; fold back to front.
    let mut built: Vec<Option<Node>> = arena.iter().map(|_| None).collect();
    let mut kids: Vec<Vec<Node>> = arena.iter().map(|_| Vec::new()).collect();
    for i in (0..arena.len()).rev() {
        let mut node = arena[i].0.clone();
        let mut children = std::mem::take(&mut kids[i]);
        children.reverse();
        node.children = children;
        match arena[i].1 {
            Some(p) => kids[p].push(node),
            None => built[i] = Some(node),
        }
    }
    built.into_iter().flatten().collect()
}

/// Wake `frame` if its tree is dormant and its process was not woken
/// before; true when it was woken now.
fn wake(bus: &dyn Tree, frame: &Ref, pid: u32, woken: &mut HashSet<u32>) -> bool {
    let children = bus.children(frame);
    let dormant = !children.is_empty() && children.iter().all(is_null);
    if !dormant || !woken.insert(pid) {
        return false;
    }
    bus.attributes(frame);
    true
}

/// The applications of `window`'s process, each frame read at its top,
/// and the frame that matches the window walked within `walk`'s budget.
/// `woken` keeps the processes whose tree was woken.
pub fn snapshot(
    bus: &dyn Tree,
    window: &Window,
    monitor: &Monitor,
    walk: &mut Walk,
    woken: &mut HashSet<u32>,
) -> Snapshot {
    let mut apps = Vec::new();
    let mut frame_refs = Vec::new();
    for app in bus.apps() {
        if bus.pid(&app) != Some(window.pid) {
            continue;
        }
        let refs: Vec<Ref> = bus
            .children(&app)
            .into_iter()
            .filter(|r| !is_null(r))
            .collect();
        let frames = bus.read(&refs).into_iter().map(|(n, _)| n).collect();
        apps.push(App {
            name: bus.name(&app),
            pid: window.pid,
            frames,
        });
        frame_refs.push(refs);
    }
    let shallow = Snapshot {
        window: window.clone(),
        monitor: monitor.clone(),
        apps,
    };
    let Some(matched) = tree::match_frame(&shallow).cloned() else {
        return shallow;
    };
    let mut snap = shallow.clone();
    for (a, app) in shallow.apps.iter().enumerate() {
        for (f, frame) in app.frames.iter().enumerate() {
            if *frame != matched {
                continue;
            }
            let at = frame_refs[a][f].clone();
            if wake(bus, &at, window.pid, woken) {
                // The tree fills in tens of milliseconds; wait for its first
                // children, within the budget.
                while Instant::now() + WAKE_POLL < walk.deadline
                    && !children_ready(&bus.children(&at))
                {
                    std::thread::sleep(WAKE_POLL);
                }
            }
            if let Some(full) = walk_from(bus, vec![at], walk).pop() {
                snap.apps[a].frames[f] = full;
            }
            return snap;
        }
    }
    snap
}

/// Every frame of `window`'s process walked in full, for record-tree.
pub fn record(bus: &dyn Tree, window: &Window, monitor: &Monitor, walk: &mut Walk) -> Snapshot {
    let mut woken = HashSet::new();
    let mut apps = Vec::new();
    for app in bus.apps() {
        if bus.pid(&app) != Some(window.pid) {
            continue;
        }
        let refs: Vec<Ref> = bus
            .children(&app)
            .into_iter()
            .filter(|r| !is_null(r))
            .collect();
        for r in &refs {
            if wake(bus, r, window.pid, &mut woken) {
                std::thread::sleep(Duration::from_millis(200));
            }
        }
        apps.push(App {
            name: bus.name(&app),
            pid: window.pid,
            frames: walk_from(bus, refs, walk),
        });
    }
    Snapshot {
        window: window.clone(),
        monitor: monitor.clone(),
        apps,
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use super::*;

    fn r(path: &str) -> Ref {
        (
            ":1.5".into(),
            OwnedObjectPath::try_from(path).expect("a path"),
        )
    }

    /// A Chromium-like app: one frame whose only child is the null object
    /// until GetAttributes is called on it.
    struct Fake {
        woken: Cell<bool>,
        attribute_calls: Cell<u32>,
        reads: RefCell<Vec<String>>,
    }

    impl Fake {
        fn new() -> Self {
            Self {
                woken: Cell::new(false),
                attribute_calls: Cell::new(0),
                reads: RefCell::new(Vec::new()),
            }
        }

        fn node(path: &str) -> Node {
            let shown = vec!["showing".to_string(), "visible".to_string()];
            match path {
                "/frame" => Node {
                    role: "frame".into(),
                    states: shown,
                    name: "Page".into(),
                    extents: Some([0, 0, 400, 300]),
                    ..Node::default()
                },
                _ => Node {
                    role: "push button".into(),
                    states: shown,
                    name: "Save".into(),
                    extents: Some([10, 10, 80, 30]),
                    ..Node::default()
                },
            }
        }
    }

    impl Tree for Fake {
        fn apps(&self) -> Vec<Ref> {
            vec![r("/app")]
        }

        fn pid(&self, _: &Ref) -> Option<u32> {
            Some(7)
        }

        fn name(&self, _: &Ref) -> String {
            "Chromium".into()
        }

        fn read(&self, at: &[Ref]) -> Vec<(Node, Vec<Ref>)> {
            at.iter()
                .map(|a| {
                    self.reads.borrow_mut().push(a.1.to_string());
                    (Self::node(a.1.as_str()), self.children(a))
                })
                .collect()
        }

        fn children(&self, at: &Ref) -> Vec<Ref> {
            match at.1.as_str() {
                "/app" => vec![r("/frame")],
                "/frame" if self.woken.get() => vec![r("/button")],
                "/frame" => vec![r(NULL_PATH)],
                _ => Vec::new(),
            }
        }

        fn attributes(&self, at: &Ref) {
            assert_eq!(at.1.as_str(), "/frame", "the frame is what wakes");
            self.attribute_calls.set(self.attribute_calls.get() + 1);
            self.woken.set(true);
        }
    }

    fn window() -> Window {
        Window {
            address: "0x1".into(),
            class: "chromium".into(),
            title: String::new(),
            pid: 7,
            at: (0, 0),
            size: (400, 300),
            workspace: 1,
            monitor: 0,
        }
    }

    fn monitor() -> Monitor {
        Monitor {
            name: "M".into(),
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
            scale: 1.0,
        }
    }

    // REQ-SRC-03: a frame whose only child is the null object is woken
    // with one GetAttributes call, and the children it then reports are
    // collected.
    #[test]
    fn req_src_03_a_dormant_tree_is_woken_once_and_collected() {
        let bus = Fake::new();
        let mut woken = HashSet::new();
        let mut walk = Walk::new(Duration::from_millis(200));
        let snap = snapshot(&bus, &window(), &monitor(), &mut walk, &mut woken);
        assert_eq!(bus.attribute_calls.get(), 1);
        let frame = tree::match_frame(&snap).expect("the frame");
        assert_eq!(frame.children.len(), 1, "the woken tree's button");
        assert_eq!(frame.children[0].name, "Save");
        assert!(
            !bus.reads.borrow().iter().any(|p| p == NULL_PATH),
            "the null object is never read"
        );
        // A second session on the same process does not wake it again.
        let mut walk = Walk::new(Duration::from_millis(200));
        snapshot(&bus, &window(), &monitor(), &mut walk, &mut woken);
        assert_eq!(bus.attribute_calls.get(), 1, "at most once per process");
    }

    // REQ-SRC-03: an empty child list after a wake is not mistaken for a
    // null-only dormant tree, so polling does not consume the whole budget.
    #[test]
    fn req_src_03_empty_children_are_ready_after_wake() {
        assert!(children_ready(&[]));
        assert!(!children_ready(&[r(NULL_PATH)]));
        assert!(children_ready(&[r("/button")]));
    }

    // REQ-SRC-03: a tree that is not dormant is never woken.
    #[test]
    fn req_src_03_a_live_tree_is_left_alone() {
        let bus = Fake::new();
        bus.woken.set(true);
        let mut woken = HashSet::new();
        let mut walk = Walk::new(Duration::from_millis(200));
        let snap = snapshot(&bus, &window(), &monitor(), &mut walk, &mut woken);
        assert_eq!(bus.attribute_calls.get(), 0);
        assert!(woken.is_empty());
        assert_eq!(tree::match_frame(&snap).expect("frame").children.len(), 1);
    }
}
