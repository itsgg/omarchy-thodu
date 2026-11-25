//! The accessibility bus, read into the `Snapshot` thodu-core replays.
//!
//! Only the frames of visible windows' processes are read (REQ-SRC-01), and
//! only frames matched to those windows are walked below their tops. A
//! subtree that is not SHOWING and VISIBLE is not descended into, and the
//! walk stops at a node cap or a shared deadline. Every call to the bus is
//! bounded by the time left, and the focused window's process is read and
//! walked before any other, so one slow application cannot hold a session up
//! or cost the focused app its targets (REQ-SRC-05). Each level is read with
//! calls in flight at once.
//!
//! Chromium and Electron keep their tree dormant until something asks for
//! it: a frame whose only child is the null object is woken with one
//! GetAttributes call, at most once per process in a session, and again in a
//! later session if the wake did not take (REQ-SRC-03).

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_util::future::join_all;
use thodu_core::model::{ElementCheck, ElementRef, Monitor, Window};
use thodu_core::tree::{self, App, Node, Snapshot};
use zbus::Connection;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

const ACCESSIBLE: &str = "org.a11y.atspi.Accessible";
const COMPONENT: &str = "org.a11y.atspi.Component";
const ACTION: &str = "org.a11y.atspi.Action";
const CACHE: &str = "org.a11y.atspi.Cache";
const NULL_PATH: &str = "/org/a11y/atspi/null";
const CACHE_PATH: &str = "/org/a11y/atspi/cache";
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

/// The address a target keeps of its element, to read it again before a click.
fn element_ref(at: &Ref) -> ElementRef {
    ElementRef {
        bus: at.0.clone(),
        path: at.1.to_string(),
    }
}

/// Whether an element has moved, gone or stopped showing since it was
/// collected, read again through `tree` just before a click (REQ-ACT-03).
/// An element the bus no longer answers for reads as moved.
pub fn element_moved(tree: &dyn Tree, check: &ElementCheck) -> bool {
    let Ok(path) = OwnedObjectPath::try_from(check.at.path.as_str()) else {
        return true;
    };
    let at: Ref = (check.at.bus.clone(), path);
    tree.read(std::slice::from_ref(&at))
        .first()
        .is_none_or(|(now, _)| !tree::element_unchanged(check, now))
}
type CacheRecord = (
    Ref,
    Ref,
    Ref,
    i32,
    i32,
    Vec<String>,
    String,
    u32,
    String,
    Vec<u32>,
);

struct CachedNode {
    reference: Ref,
    parent: Ref,
    index: i32,
    interfaces: Vec<String>,
    child_count: i32,
    node: Node,
}

/// A bus link shared between sessions and the walks they leave behind.
pub type Shared = Arc<dyn Tree + Send + Sync>;

/// What the walk needs from the bus, so a fake can stand in for it.
pub trait Tree {
    /// The applications on the bus.
    fn apps(&self) -> Vec<Ref>;
    /// The applications on the bus, or why the registry did not answer.
    fn registry(&self) -> Result<Vec<Ref>, String> {
        Ok(self.apps())
    }
    fn pid(&self, app: &Ref) -> Option<u32>;
    /// The process of each of `apps`, the lookups in flight at once.
    fn pids(&self, apps: &[Ref]) -> Vec<Option<u32>> {
        apps.iter().map(|app| self.pid(app)).collect()
    }
    fn name(&self, at: &Ref) -> String;
    /// One node without its children, and the references to them; all of
    /// `at`'s calls in flight at once.
    fn read(&self, at: &[Ref]) -> Vec<(Node, Vec<Ref>)>;
    /// The children of `at`, the null object included.
    fn children(&self, at: &Ref) -> Vec<Ref>;
    /// The call that wakes a dormant Chromium tree.
    fn attributes(&self, at: &Ref);
    /// Bulk tree data from org.a11y.atspi.Cache, when the application has it.
    fn cache_frames(&self, _app: &Ref, _walk: &mut Walk) -> Option<Vec<Node>> {
        None
    }
    /// This link with every call bounded by `deadline`, when it can be.
    fn bounded(&self, _deadline: Instant) -> Option<Shared> {
        None
    }
    /// A new link to the bus, for when this one stopped answering.
    fn reopen(&self) -> Result<Shared, String> {
        Err("the accessibility bus cannot be reopened".into())
    }
    /// A process's name and parent, for matching sandboxed applications.
    fn process(&self, pid: u32) -> Option<(String, u32)> {
        process_of(pid)
    }
    /// Whether a process uses GTK4, whose Cache interface lists its tree.
    fn uses_gtk4(&self, pid: u32) -> bool {
        uses_gtk4(pid)
    }
}

/// The accessibility bus. A copy shares the connection and may carry a
/// deadline that bounds every call it makes.
#[derive(Clone)]
pub struct Bus {
    conn: Connection,
    deadline: Option<Instant>,
}

/// `request`'s answer, or None once `deadline` passes first.
pub(crate) async fn within<T>(
    deadline: Option<Instant>,
    request: impl Future<Output = Option<T>>,
) -> Option<T> {
    let Some(deadline) = deadline else {
        return request.await;
    };
    let left = deadline.saturating_duration_since(Instant::now());
    if left.is_zero() {
        return None;
    }
    futures_lite::future::race(request, async {
        async_io::Timer::after(left).await;
        None
    })
    .await
}

/// A process's command name and parent pid, from /proc.
fn process_of(pid: u32) -> Option<(String, u32)> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let (open, close) = (stat.find('(')?, stat.rfind(')')?);
    let name = stat.get(open + 1..close)?.to_string();
    let ppid = stat
        .get(close + 1..)?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()?;
    Some((name, ppid))
}

/// The visible window processes an application on the bus may belong to. A
/// Flatpak app reaches the accessibility bus through its instance's
/// xdg-dbus-proxy, so the bus names the proxy's pid, not the app's. Flatpak
/// starts the proxy before it becomes the instance's outer bwrap, so the
/// proxy and the app's processes share that bwrap as an ancestor, while two
/// instances share none. Sorted, so the choice among them is stable.
pub(crate) fn window_pids_of(
    bus_pid: u32,
    window_pids: &HashSet<u32>,
    process: &dyn Fn(u32) -> Option<(String, u32)>,
) -> Vec<u32> {
    if window_pids.contains(&bus_pid) {
        return vec![bus_pid];
    }
    let ancestors = |pid: u32| {
        let mut chain = Vec::new();
        let mut at = pid;
        while chain.len() < 64 {
            let Some((name, parent)) = process(at) else {
                break;
            };
            chain.push((at, name));
            if parent <= 1 {
                break;
            }
            at = parent;
        }
        chain
    };
    let proxy = ancestors(bus_pid);
    if proxy
        .first()
        .is_none_or(|(_, name)| name != "xdg-dbus-proxy")
    {
        return Vec::new();
    }
    let sandboxes: Vec<u32> = proxy
        .iter()
        .skip(1)
        .filter(|(_, name)| name == "bwrap")
        .map(|(pid, _)| *pid)
        .collect();
    let mut owners: Vec<u32> = window_pids
        .iter()
        .copied()
        .filter(|&window| {
            ancestors(window)
                .iter()
                .any(|(pid, name)| name == "bwrap" && sandboxes.contains(pid))
        })
        .collect();
    owners.sort_unstable();
    owners
}

fn is_null(r: &Ref) -> bool {
    r.1.as_str() == NULL_PATH
}

/// The numeric roles in the AT-SPI Cache interface, aligned with its public
/// AtspiRole enum. GTK4 uses TABLE_ROW for its localized "row" role name.
fn cache_role_name(role: u32) -> &'static str {
    match role {
        2 => "alert",
        7 => "check box",
        8 => "check menu item",
        11 => "combo box",
        16 => "dialog",
        23 => "frame",
        32 => "list item",
        33 => "menu",
        35 => "menu item",
        37 => "page tab",
        40 => "password text",
        41 => "popup menu",
        43 => "button",
        44 => "radio button",
        45 => "radio menu item",
        48 => "scroll bar",
        49 => "scroll pane",
        51 => "slider",
        52 => "spin button",
        56 => "table cell",
        61 => "text",
        62 => "toggle button",
        64 => "tooltip",
        68 => "viewport",
        69 => "window",
        79 => "entry",
        82 => "document frame",
        88 => "link",
        90 => "row",
        91 => "tree item",
        95 => "document web",
        98 => "list box",
        130 => "switch",
        _ => "unknown",
    }
}

fn state_names(words: &[u32]) -> Vec<String> {
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

fn is_document(role: &str) -> bool {
    matches!(role, "document web" | "document frame")
}

/// After waking a dormant tree, an empty child list is ready to read too.
fn children_ready(children: &[Ref]) -> bool {
    children.is_empty() || !children.iter().all(is_null)
}

impl Bus {
    pub fn connect() -> Result<Self, String> {
        // Reopening after at-spi-bus-launcher restarts asks for the address
        // again; a session bus that does not answer must not hold that up.
        let session = zbus::blocking::connection::Builder::session()
            .map(|builder| builder.method_timeout(Duration::from_millis(500)))
            .and_then(|builder| builder.build())
            .map_err(|e| format!("no session bus: {e}"))?;
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
        Ok(Self {
            conn,
            deadline: None,
        })
    }

    /// One call, its answer or why there is none, bounded by the deadline.
    async fn try_call<B, T>(
        &self,
        (dest, path): &Ref,
        iface: &str,
        method: &str,
        body: &B,
    ) -> Result<T, String>
    where
        B: serde::Serialize + zbus::zvariant::DynamicType,
        T: for<'d> zbus::zvariant::DynamicDeserialize<'d>,
    {
        let request = async {
            Some(
                self.conn
                    .call_method(
                        Some(dest.as_str()),
                        path.as_str(),
                        Some(iface),
                        method,
                        body,
                    )
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|m| m.body().deserialize().map_err(|e| e.to_string())),
            )
        };
        within(self.deadline, request)
            .await
            .unwrap_or_else(|| Err("no answer before the deadline".into()))
    }

    async fn call<B, T>(&self, at: &Ref, iface: &str, method: &str, body: &B) -> Option<T>
    where
        B: serde::Serialize + zbus::zvariant::DynamicType,
        T: for<'d> zbus::zvariant::DynamicDeserialize<'d>,
    {
        self.try_call(at, iface, method, body).await.ok()
    }

    async fn pid_of(&self, app: &Ref) -> Option<u32> {
        let daemon = (
            "org.freedesktop.DBus".to_string(),
            OwnedObjectPath::try_from("/org/freedesktop/DBus").ok()?,
        );
        self.call(
            &daemon,
            "org.freedesktop.DBus",
            "GetConnectionUnixProcessID",
            &(app.0.as_str(),),
        )
        .await
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
        state_names(&words)
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
            element: Some(element_ref(at)),
        };
        (node, children.unwrap_or_default())
    }
}

impl Tree for Bus {
    fn apps(&self) -> Vec<Ref> {
        self.registry().unwrap_or_default()
    }

    fn registry(&self) -> Result<Vec<Ref>, String> {
        let root: Ref = (
            "org.a11y.atspi.Registry".into(),
            OwnedObjectPath::try_from("/org/a11y/atspi/accessible/root").expect("a valid path"),
        );
        futures_lite::future::block_on(self.try_call(&root, ACCESSIBLE, "GetChildren", &()))
    }

    fn pid(&self, app: &Ref) -> Option<u32> {
        futures_lite::future::block_on(self.pid_of(app))
    }

    fn pids(&self, apps: &[Ref]) -> Vec<Option<u32>> {
        futures_lite::future::block_on(join_all(apps.iter().map(|app| self.pid_of(app))))
    }

    fn bounded(&self, deadline: Instant) -> Option<Shared> {
        Some(Arc::new(Self {
            conn: self.conn.clone(),
            deadline: Some(deadline),
        }))
    }

    fn reopen(&self) -> Result<Shared, String> {
        Ok(Arc::new(Self::connect()?))
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

    fn cache_frames(&self, app: &Ref, walk: &mut Walk) -> Option<Vec<Node>> {
        // GTK4's cache returns the entire tree, including its parent links,
        // roles, names and states, in one D-Bus call. Query geometry only for
        // targets, frame roots, document scaling and covered layers.
        let remaining = walk.deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            walk.truncated = true;
            return None;
        }
        let request = async {
            let cache = (app.0.clone(), OwnedObjectPath::try_from(CACHE_PATH).ok()?);
            let records: Vec<CacheRecord> = self.call(&cache, CACHE, "GetItems", &()).await?;
            if records.is_empty() || records.len() > MAX_NODES {
                return None;
            }
            let mut nodes = Vec::with_capacity(records.len());
            for (
                reference,
                _owner,
                parent,
                index,
                child_count,
                interfaces,
                name,
                role_id,
                description,
                state_words,
            ) in records
            {
                let element = Some(element_ref(&reference));
                nodes.push(CachedNode {
                    reference,
                    parent,
                    index,
                    interfaces,
                    child_count,
                    node: Node {
                        role: cache_role_name(role_id).into(),
                        states: state_names(&state_words),
                        name,
                        description,
                        extents: None,
                        action: String::new(),
                        children: Vec::new(),
                        element,
                    },
                });
            }
            let row_paths: HashSet<&str> = nodes
                .iter()
                .filter(|entry| entry.node.role == "row")
                .map(|entry| entry.reference.1.as_str())
                .collect();
            let queries: Vec<(usize, bool, bool)> = nodes
                .iter()
                .enumerate()
                .filter_map(|(i, entry)| {
                    let role = entry.node.role.as_str();
                    let shown = entry.node.states.iter().any(|state| state == "showing")
                        && entry.node.states.iter().any(|state| state == "visible");
                    let frame_root =
                        entry.parent.0 == app.0 && entry.parent.1.as_str() == app.1.as_str();
                    let action = tree::role_of(role).is_none()
                        && shown
                        && entry.interfaces.iter().any(|name| name == ACTION)
                        && entry.child_count == 0;
                    let row_cell = matches!(role, "grid cell" | "table cell")
                        && row_paths.contains(entry.parent.1.as_str());
                    let geometry = ((tree::role_of(role).is_some()
                        || tree::is_scroll_area_role(role))
                        && shown
                        && !row_cell)
                        || frame_root
                        || (shown && is_document(role))
                        || (shown
                            && matches!(
                                role,
                                "window" | "dialog" | "alert" | "tooltip" | "popup menu"
                            ))
                        || action;
                    (geometry || action).then_some((i, action, geometry))
                })
                .collect();
            let resolved = join_all(queries.iter().map(|&(i, action_needed, geometry_needed)| {
                let entry = &nodes[i];
                async move {
                    let action = async {
                        if action_needed {
                            self.call::<_, String>(&entry.reference, ACTION, "GetName", &(0i32,))
                                .await
                        } else {
                            None
                        }
                    };
                    let extents = async {
                        if geometry_needed {
                            self.call::<_, (i32, i32, i32, i32)>(
                                &entry.reference,
                                COMPONENT,
                                "GetExtents",
                                &(COORD_WINDOW,),
                            )
                            .await
                        } else {
                            None
                        }
                    };
                    let (action, extents) = futures_util::join!(action, extents);
                    (i, action, extents)
                }
            }))
            .await;
            for (i, action, extents) in resolved {
                if let Some(action) = action {
                    nodes[i].node.action = action;
                }
                if let Some((x, y, w, h)) = extents {
                    nodes[i].node.extents = Some([x, y, w, h]);
                }
            }
            Some(nodes)
        };
        let entries = futures_lite::future::block_on(futures_lite::future::race(request, async {
            async_io::Timer::after(remaining).await;
            None
        }))?;
        build_cached_frames(app, &entries, walk)
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
        Self::until(Instant::now() + budget)
    }

    pub fn until(deadline: Instant) -> Self {
        Self {
            deadline,
            nodes: 0,
            truncated: false,
        }
    }
}

fn build_cached_frames(app: &Ref, entries: &[CachedNode], walk: &mut Walk) -> Option<Vec<Node>> {
    if entries.len() > MAX_NODES {
        walk.truncated = true;
        return None;
    }
    let by_path: std::collections::HashMap<&str, usize> = entries
        .iter()
        .enumerate()
        .map(|(i, entry)| (entry.reference.1.as_str(), i))
        .collect();
    let mut children: Vec<Vec<(i32, usize)>> = vec![Vec::new(); entries.len()];
    let mut roots = Vec::new();
    for (i, entry) in entries.iter().enumerate() {
        if entry.reference.0 == app.0 && entry.reference.1.as_str() == app.1.as_str() {
            continue;
        }
        if entry.parent.0 == app.0 && entry.parent.1.as_str() == app.1.as_str() {
            roots.push((entry.index, i));
        } else if let Some(parent) = by_path.get(entry.parent.1.as_str()) {
            children[*parent].push((entry.index, i));
        }
    }
    if roots.is_empty() {
        return None;
    }
    roots.sort_by_key(|(index, i)| (*index, *i));
    for siblings in &mut children {
        siblings.sort_by_key(|(index, i)| (*index, *i));
    }
    fn build(
        i: usize,
        depth: usize,
        entries: &[CachedNode],
        children: &[Vec<(i32, usize)>],
        truncated: &mut bool,
    ) -> Node {
        let mut node = entries[i].node.clone();
        if depth >= MAX_DEPTH {
            *truncated |= !children[i].is_empty();
            return node;
        }
        node.children = children[i]
            .iter()
            .map(|(_, child)| build(*child, depth + 1, entries, children, truncated))
            .collect();
        node
    }
    let frames = roots
        .iter()
        .map(|(_, i)| build(*i, 0, entries, &children, &mut walk.truncated))
        .collect();
    walk.nodes = walk.nodes.saturating_add(entries.len());
    Some(frames)
}

fn uses_gtk4(pid: u32) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/maps"))
        .is_ok_and(|maps| maps.lines().any(|line| line.contains("libgtk-4.so")))
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

/// Wake `frame` if its tree is dormant and its process was not tried in this
/// session; true when it was woken now.
fn wake(bus: &dyn Tree, frame: &Ref, pid: u32, tried: &mut HashSet<u32>) -> bool {
    let children = bus.children(frame);
    let dormant = !children.is_empty() && children.iter().all(is_null);
    if !dormant || !tried.insert(pid) {
        return false;
    }
    bus.attributes(frame);
    true
}

/// What one AT-SPI collection found, published as it goes so a caller can
/// take what has arrived by its deadline.
#[derive(Clone, Debug, Default)]
pub struct Scan {
    /// The applications read so far, the focused window's process first.
    pub apps: Vec<App>,
    /// Window processes with an application on the bus.
    pub registered: HashSet<u32>,
    /// Whether every application on the bus had its process looked up, so a
    /// window process missing from `registered` is known to be missing.
    pub looked_up: bool,
    /// Window processes read as another: the windows of one Flatpak
    /// instance whose apps the bus names by the instance's proxy all match
    /// against the apps, read under one of them.
    pub aliases: HashMap<u32, u32>,
    /// Why the registry did not answer, when it did not.
    pub registry: Option<String>,
    /// Processes woken in this collection whose tree then filled.
    pub woken: HashSet<u32>,
    /// Processes woken in this collection whose tree stayed dormant.
    pub unwoken: HashSet<u32>,
    pub truncated: bool,
    pub done: bool,
}

fn publish(out: &Mutex<Scan>, change: impl FnOnce(&mut Scan)) {
    if let Ok(mut scan) = out.lock() {
        change(&mut scan);
    }
}

/// Read each visible process, the focused window's first, and walk each
/// frame matched to one of its windows, publishing into `out` after each
/// process so the focused app's targets are there even if another process
/// never answers.
pub fn scan(
    bus: &dyn Tree,
    windows: &[Window],
    monitor: &Monitor,
    walk: &mut Walk,
    out: &Mutex<Scan>,
) {
    let finish = |out: &Mutex<Scan>, walk: &Walk| {
        publish(out, |scan| {
            scan.truncated |= walk.truncated;
            scan.done = true;
        });
    };
    if windows.is_empty() {
        return finish(out, walk);
    }
    let on_bus = match bus.registry() {
        Ok(apps) => apps,
        Err(error) => {
            publish(out, |scan| scan.registry = Some(error));
            return finish(out, walk);
        }
    };
    let pids = bus.pids(&on_bus);
    // A lookup that did not answer may have been any window's process.
    let looked_up = pids.len() == on_bus.len()
        && pids.iter().all(Option::is_some)
        && Instant::now() < walk.deadline;
    let window_pids: HashSet<u32> = windows.iter().map(|window| window.pid).collect();
    let focused_pid = windows
        .iter()
        .find(|window| window.focused)
        .or_else(|| windows.first())
        .map(|window| window.pid);
    let mut registered = HashSet::new();
    let mut aliases = HashMap::new();
    let mut app_refs: Vec<(Ref, u32)> = Vec::new();
    for (app, pid) in on_bus.into_iter().zip(pids) {
        let Some(pid) = pid else { continue };
        let owners = window_pids_of(pid, &window_pids, &|p| bus.process(p));
        registered.extend(owners.iter().copied());
        // The app is read under one of its windows' processes, the focused
        // one when it is among them, and its other windows are read as it.
        let owner = owners
            .iter()
            .copied()
            .find(|owner| Some(*owner) == focused_pid)
            .or_else(|| owners.first().copied());
        if let Some(owner) = owner {
            for other in owners.iter().copied().filter(|other| *other != owner) {
                aliases.insert(other, owner);
            }
            app_refs.push((app, owner));
        }
    }
    let aliases_found = aliases.clone();
    publish(out, |scan| {
        scan.registered = registered;
        scan.looked_up = looked_up;
        scan.aliases = aliases_found;
    });
    prioritize_applications(&mut app_refs, focused_pid);
    let mut order: Vec<u32> = Vec::new();
    for (_, pid) in &app_refs {
        if !order.contains(pid) {
            order.push(*pid);
        }
    }
    let mut tried = HashSet::new();
    for pid in order {
        if Instant::now() >= walk.deadline {
            walk.truncated = true;
            break;
        }
        let group: Vec<&Ref> = app_refs
            .iter()
            .filter(|(_, p)| *p == pid)
            .map(|(app, _)| app)
            .collect();
        let apps = read_process(
            bus, &group, pid, windows, &aliases, monitor, walk, &mut tried, out,
        );
        publish(out, |scan| scan.apps.extend(apps));
    }
    finish(out, walk);
}

/// One process's applications, each frame read at its top and the frames
/// matched to its windows walked.
#[allow(clippy::too_many_arguments)]
fn read_process(
    bus: &dyn Tree,
    group: &[&Ref],
    pid: u32,
    windows: &[Window],
    aliases: &HashMap<u32, u32>,
    monitor: &Monitor,
    walk: &mut Walk,
    tried: &mut HashSet<u32>,
    out: &Mutex<Scan>,
) -> Vec<App> {
    let mut apps = Vec::new();
    let mut frame_refs = Vec::new();
    let mut cached_apps = Vec::new();
    let gtk4 = bus.uses_gtk4(pid);
    for app in group {
        if Instant::now() >= walk.deadline {
            walk.truncated = true;
            break;
        }
        if gtk4 {
            if let Some(frames) = bus.cache_frames(app, walk) {
                apps.push(App {
                    name: String::new(),
                    pid,
                    frames,
                });
                frame_refs.push(Vec::new());
                cached_apps.push(true);
                continue;
            }
            if Instant::now() >= walk.deadline {
                walk.truncated = true;
                break;
            }
        }
        let refs: Vec<Ref> = bus
            .children(app)
            .into_iter()
            .filter(|r| !is_null(r))
            .collect();
        let frames = bus.read(&refs).into_iter().map(|(n, _)| n).collect();
        apps.push(App {
            name: bus.name(app),
            pid,
            frames,
        });
        frame_refs.push(refs);
        cached_apps.push(false);
    }
    let shallow: Vec<_> = windows
        .iter()
        .map(|window| read_as(window, aliases))
        .filter(|window| window.pid == pid)
        .map(|window| Snapshot {
            window,
            monitor: monitor.clone(),
            apps: apps.clone(),
        })
        .collect();
    let selected: Vec<_> = tree::match_frame_indices(&shallow)
        .into_iter()
        .flatten()
        .collect();
    for (a, f) in selected {
        if cached_apps[a] {
            continue;
        }
        let at = frame_refs[a][f].clone();
        if wake(bus, &at, pid, tried) {
            // The tree fills in tens of milliseconds; wait for its first
            // children, within the shared session budget. A wake that has
            // not taken by then is tried again in a later session.
            let took = loop {
                if children_ready(&bus.children(&at)) {
                    break true;
                }
                if Instant::now() + WAKE_POLL >= walk.deadline {
                    break false;
                }
                std::thread::sleep(WAKE_POLL);
            };
            publish(out, |scan| {
                if took {
                    scan.woken.insert(pid);
                } else {
                    scan.unwoken.insert(pid);
                }
            });
        }
        if let Some(full) = walk_from(bus, vec![at], walk).pop() {
            apps[a].frames[f] = full;
        }
    }
    apps
}

/// The applications of `window`'s process, each frame read at its top,
/// and the frame that matches the window walked within `walk`'s budget.
/// `woken` keeps the processes whose tree was woken.
#[cfg(test)]
pub fn snapshot(
    bus: &dyn Tree,
    window: &Window,
    monitor: &Monitor,
    walk: &mut Walk,
    woken: &mut HashSet<u32>,
) -> Snapshot {
    snapshot_many(bus, std::slice::from_ref(window), monitor, walk, woken)
        .into_iter()
        .next()
        .unwrap_or_else(|| Snapshot {
            window: window.clone(),
            monitor: monitor.clone(),
            apps: Vec::new(),
        })
}

/// Read each visible process once, then walk each matching frame once for a
/// session spanning multiple windows, waiting for the whole scan. `woken`
/// keeps the processes whose tree was woken and filled.
#[cfg(test)]
pub fn snapshot_many(
    bus: &dyn Tree,
    windows: &[Window],
    monitor: &Monitor,
    walk: &mut Walk,
    woken: &mut HashSet<u32>,
) -> Vec<Snapshot> {
    let out = Mutex::new(Scan::default());
    scan(bus, windows, monitor, walk, &out);
    let found = out.into_inner().unwrap_or_default();
    woken.retain(|pid| !found.unwoken.contains(pid));
    woken.extend(found.woken.iter().copied());
    snapshots(&found, windows, monitor)
}

/// `window` with the process its apps are read under.
fn read_as(window: &Window, aliases: &HashMap<u32, u32>) -> Window {
    let mut window = window.clone();
    if let Some(pid) = aliases.get(&window.pid) {
        window.pid = *pid;
    }
    window
}

/// One snapshot per window over every application read, each window under
/// the process its apps were read as.
pub fn snapshots(found: &Scan, windows: &[Window], monitor: &Monitor) -> Vec<Snapshot> {
    windows
        .iter()
        .map(|window| Snapshot {
            window: read_as(window, &found.aliases),
            monitor: monitor.clone(),
            apps: found.apps.clone(),
        })
        .collect()
}

fn prioritize_applications(apps: &mut [(Ref, u32)], focused_pid: Option<u32>) {
    if let Some(focused_pid) = focused_pid {
        apps.sort_by_key(|(_, pid)| *pid != focused_pid);
    }
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

    /// A bus that answers one element read with `node`, or with nothing.
    struct Rechecked(Option<Node>);

    impl Tree for Rechecked {
        fn apps(&self) -> Vec<Ref> {
            Vec::new()
        }

        fn pid(&self, _: &Ref) -> Option<u32> {
            None
        }

        fn name(&self, _: &Ref) -> String {
            String::new()
        }

        fn read(&self, at: &[Ref]) -> Vec<(Node, Vec<Ref>)> {
            at.iter()
                .map(|_| (self.0.clone().unwrap_or_default(), Vec::new()))
                .collect()
        }

        fn children(&self, _: &Ref) -> Vec<Ref> {
            Vec::new()
        }

        fn attributes(&self, _: &Ref) {}
    }

    // REQ-ACT-03: before a click the element is read again; the same extents
    // pass, while moved extents, an element the bus no longer answers for and
    // an address that is not a path count as moved.
    #[test]
    fn req_act_03_element_is_read_again_before_a_click() {
        let check = ElementCheck {
            at: ElementRef {
                bus: ":1.5".into(),
                path: "/button".into(),
            },
            extents: [10, 10, 80, 30],
        };
        let same = Node {
            states: vec!["showing".into(), "visible".into()],
            extents: Some([10, 10, 80, 30]),
            ..Node::default()
        };
        assert!(!element_moved(&Rechecked(Some(same.clone())), &check));
        let moved = Node {
            extents: Some([10, 50, 80, 30]),
            ..same.clone()
        };
        assert!(element_moved(&Rechecked(Some(moved)), &check));
        assert!(element_moved(&Rechecked(None), &check));
        let not_a_path = ElementCheck {
            at: ElementRef {
                bus: ":1.5".into(),
                path: "not a path".into(),
            },
            ..check
        };
        assert!(element_moved(&Rechecked(Some(same)), &not_a_path));
    }

    struct MultiProcess;

    impl MultiProcess {
        fn node(path: &str) -> Node {
            let shown = vec!["showing".to_string(), "visible".to_string()];
            match path {
                "/frame1" => Node {
                    role: "frame".into(),
                    states: shown,
                    name: "First frame".into(),
                    extents: Some([0, 0, 100, 100]),
                    ..Node::default()
                },
                "/frame2" => Node {
                    role: "frame".into(),
                    states: shown,
                    name: "Second frame".into(),
                    extents: Some([0, 0, 100, 100]),
                    ..Node::default()
                },
                "/button1" => Node {
                    role: "button".into(),
                    states: shown,
                    name: "First app".into(),
                    extents: Some([10, 10, 20, 20]),
                    ..Node::default()
                },
                "/button2" => Node {
                    role: "button".into(),
                    states: shown,
                    name: "Second app".into(),
                    extents: Some([10, 10, 20, 20]),
                    ..Node::default()
                },
                _ => Node::default(),
            }
        }
    }

    impl Tree for MultiProcess {
        fn apps(&self) -> Vec<Ref> {
            vec![r("/app1"), r("/app2")]
        }

        fn pid(&self, app: &Ref) -> Option<u32> {
            match app.1.as_str() {
                "/app1" => Some(11),
                "/app2" => Some(12),
                _ => None,
            }
        }

        fn name(&self, app: &Ref) -> String {
            app.1.as_str().to_string()
        }

        fn read(&self, at: &[Ref]) -> Vec<(Node, Vec<Ref>)> {
            at.iter()
                .map(|reference| (Self::node(reference.1.as_str()), self.children(reference)))
                .collect()
        }

        fn children(&self, at: &Ref) -> Vec<Ref> {
            match at.1.as_str() {
                "/app1" => vec![r("/frame1")],
                "/app2" => vec![r("/frame2")],
                "/frame1" => vec![r("/button1")],
                "/frame2" => vec![r("/button2")],
                _ => Vec::new(),
            }
        }

        fn attributes(&self, _: &Ref) {}
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
            ..Window::default()
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
            layers: Vec::new(),
        }
    }

    // REQ-SRC-02: cached GTK4 parent references reconstruct the frame's rows
    // and preserve their nested controls in the same order as the tree.
    #[test]
    fn req_src_02_cache_entries_reconstruct_gtk4_rows() {
        let app = r("/app");
        let entry = |path: &str, parent: &str, index, role: &str| CachedNode {
            reference: r(path),
            parent: r(parent),
            index,
            interfaces: Vec::new(),
            child_count: 0,
            node: Node {
                role: role.into(),
                ..Node::default()
            },
        };
        let entries = vec![
            entry("/button", "/cell", 0, "button"),
            entry("/row", "/frame", 1, "row"),
            entry("/app", "/app", -1, "application"),
            entry("/cell", "/row", 0, "table cell"),
            entry("/frame", "/app", 0, "frame"),
        ];
        let mut walk = Walk::new(Duration::from_millis(100));
        let frames = build_cached_frames(&app, &entries, &mut walk).expect("the cached frame");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].role, "frame");
        assert_eq!(frames[0].children[0].role, "row");
        assert_eq!(frames[0].children[0].children[0].role, "table cell");
        assert_eq!(frames[0].children[0].children[0].children[0].role, "button");
        assert_eq!(cache_role_name(90), "row");
        assert_eq!(cache_role_name(56), "table cell");
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

    // REQ-SRC-07: one AT-SPI scan walks frames from all visible processes.
    #[test]
    fn req_src_07_snapshot_collects_each_visible_process() {
        let bus = MultiProcess;
        let mut first = window();
        first.pid = 11;
        first.size = (100, 100);
        first.focused = true;
        let second = Window {
            address: "0x2".into(),
            class: "other-app".into(),
            title: String::new(),
            pid: 12,
            at: (100, 0),
            size: (100, 100),
            workspace: 1,
            monitor: 0,
            ..Window::default()
        };
        let monitor = Monitor {
            width: 200,
            ..monitor()
        };
        let mut woken = HashSet::new();
        let mut walk = Walk::new(Duration::from_secs(1));

        let snapshots = snapshot_many(
            &bus,
            &[first.clone(), second.clone()],
            &monitor,
            &mut walk,
            &mut woken,
        );

        assert_eq!(snapshots.len(), 2);
        for (i, (window, expected)) in [(&first, "First app"), (&second, "Second app")]
            .into_iter()
            .enumerate()
        {
            let frame =
                tree::match_frame_for_window(&snapshots[i], window.focused).expect("own frame");
            let targets = tree::targets(frame, window, &monitor);
            assert_eq!(targets.len(), 1);
            assert_eq!(targets[0].text, expected);
            assert_eq!(targets[0].window, window.address);
        }
    }

    // REQ-SRC-02: process cache work starts with the focused window so other
    // GTK4 applications cannot consume the walk budget first.
    #[test]
    fn req_src_02_focused_application_is_prioritized() {
        let mut apps = vec![(r("/background"), 11), (r("/focused"), 12)];

        prioritize_applications(&mut apps, Some(12));

        assert_eq!(apps[0].1, 12);
        assert_eq!(apps[1].1, 11);
    }

    // REQ-SRC-02: an expired shared walk budget does not read app frames.
    #[test]
    fn req_src_02_snapshot_respects_an_expired_walk_deadline() {
        let bus = Fake::new();
        let mut walk = Walk::new(Duration::ZERO);
        let mut woken = HashSet::new();

        let snapshots = snapshot_many(&bus, &[window()], &monitor(), &mut walk, &mut woken);

        assert!(walk.truncated);
        assert!(snapshots.iter().all(|snapshot| snapshot.apps.is_empty()));
        assert!(bus.reads.borrow().is_empty());
    }

    // REQ-SRC-05: every bus call races the time left, so one application
    // that never answers costs a collection no more than its deadline.
    #[test]
    fn req_src_05_every_bus_call_is_bounded_by_the_time_left() {
        use futures_lite::future::block_on as block;
        let started = Instant::now();
        let hung = block(within(
            Some(Instant::now() + Duration::from_millis(30)),
            std::future::pending::<Option<u32>>(),
        ));
        assert_eq!(hung, None);
        let took = started.elapsed();
        assert!(
            took >= Duration::from_millis(30) && took < Duration::from_millis(300),
            "a hung call ends at its deadline, took {took:?}"
        );
        let started = Instant::now();
        assert_eq!(
            block(within(
                Some(Instant::now()),
                std::future::pending::<Option<u32>>()
            )),
            None
        );
        assert!(
            started.elapsed() < Duration::from_millis(20),
            "no time left"
        );
        assert_eq!(
            block(within(
                Some(Instant::now() + Duration::from_secs(5)),
                async { Some(3) }
            )),
            Some(3)
        );
        assert_eq!(block(within(None, async { Some(4) })), Some(4));
    }

    /// A Chromium-like app whose first wake does not take.
    struct Stubborn {
        attribute_calls: Cell<u32>,
    }

    impl Tree for Stubborn {
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
                .map(|a| (Fake::node(a.1.as_str()), self.children(a)))
                .collect()
        }

        fn children(&self, at: &Ref) -> Vec<Ref> {
            match at.1.as_str() {
                "/app" => vec![r("/frame")],
                "/frame" if self.attribute_calls.get() >= 2 => vec![r("/button")],
                "/frame" => vec![r(NULL_PATH)],
                _ => Vec::new(),
            }
        }

        fn attributes(&self, _: &Ref) {
            self.attribute_calls.set(self.attribute_calls.get() + 1);
        }

        fn uses_gtk4(&self, _: u32) -> bool {
            false
        }
    }

    // REQ-SRC-03 (change 0030): a wake that did not take is tried again in
    // a later session, and once it takes the process is left alone.
    #[test]
    fn req_src_03_a_wake_that_did_not_take_is_retried_later() {
        let bus = Stubborn {
            attribute_calls: Cell::new(0),
        };
        let mut woken = HashSet::new();
        let mut walk = Walk::new(Duration::from_millis(40));
        let snap = snapshot(&bus, &window(), &monitor(), &mut walk, &mut woken);
        assert_eq!(bus.attribute_calls.get(), 1, "one wake in a session");
        assert!(tree::match_frame(&snap).expect("frame").children.is_empty());
        assert!(woken.is_empty(), "a wake that did not take is not kept");

        let mut walk = Walk::new(Duration::from_millis(200));
        let snap = snapshot(&bus, &window(), &monitor(), &mut walk, &mut woken);
        assert_eq!(bus.attribute_calls.get(), 2, "the next session wakes again");
        let frame = tree::match_frame(&snap).expect("frame");
        assert_eq!(frame.children.len(), 1, "the woken tree's button");
        assert!(woken.contains(&7));

        let mut walk = Walk::new(Duration::from_millis(200));
        snapshot(&bus, &window(), &monitor(), &mut walk, &mut woken);
        assert_eq!(bus.attribute_calls.get(), 2, "a wake that took holds");
    }

    /// Two Flatpak instances and a host process, as /proc shows them: each
    /// instance's xdg-dbus-proxy runs in a bwrap of its own under the
    /// instance's outer bwrap, which also holds the app.
    fn flatpak_process(pid: u32) -> Option<(String, u32)> {
        let (name, parent) = match pid {
            500 => ("xdg-dbus-proxy", 499),
            499 => ("bwrap", 400),
            400 => ("bwrap", 300),
            402 => ("electron", 401),
            401 => ("bwrap", 400),
            600 => ("xdg-dbus-proxy", 599),
            599 => ("bwrap", 598),
            598 => ("bwrap", 300),
            602 => ("other", 601),
            601 => ("bwrap", 598),
            300 => ("bash", 1),
            700 => ("foot", 300),
            _ => return None,
        };
        Some((name.to_string(), parent))
    }

    // REQ-SESSION-05: a Flatpak app reaches the bus through its instance's
    // xdg-dbus-proxy; it is matched to the window process in the proxy's
    // sandbox, never to another instance or a host process.
    #[test]
    fn req_session_05_a_flatpak_app_behind_its_proxy_is_matched_to_its_window() {
        let windows: HashSet<u32> = [402, 602, 700].into_iter().collect();
        assert_eq!(window_pids_of(500, &windows, &flatpak_process), [402]);
        assert_eq!(window_pids_of(600, &windows, &flatpak_process), [602]);
        assert_eq!(window_pids_of(700, &windows, &flatpak_process), [700]);
        let only_host: HashSet<u32> = [700].into_iter().collect();
        assert!(window_pids_of(500, &only_host, &flatpak_process).is_empty());
        assert!(window_pids_of(300, &windows, &flatpak_process).is_empty());
        assert!(window_pids_of(999, &windows, &flatpak_process).is_empty());
    }

    /// The Chromium-like fake, seen through a Flatpak proxy.
    struct Proxied(Fake);

    impl Tree for Proxied {
        fn apps(&self) -> Vec<Ref> {
            self.0.apps()
        }

        fn pid(&self, _: &Ref) -> Option<u32> {
            Some(500)
        }

        fn name(&self, at: &Ref) -> String {
            self.0.name(at)
        }

        fn read(&self, at: &[Ref]) -> Vec<(Node, Vec<Ref>)> {
            self.0.read(at)
        }

        fn children(&self, at: &Ref) -> Vec<Ref> {
            self.0.children(at)
        }

        fn attributes(&self, at: &Ref) {
            self.0.attributes(at);
        }

        fn process(&self, pid: u32) -> Option<(String, u32)> {
            flatpak_process(pid)
        }

        fn uses_gtk4(&self, _: u32) -> bool {
            false
        }
    }

    // REQ-SESSION-05: the proxied app's frame is walked for its window.
    #[test]
    fn req_session_05_a_proxied_app_gets_its_targets() {
        let bus = Proxied(Fake::new());
        bus.0.woken.set(true);
        let mut window = window();
        window.pid = 402;
        let mut woken = HashSet::new();
        let mut walk = Walk::new(Duration::from_millis(200));
        let snap = snapshot(&bus, &window, &monitor(), &mut walk, &mut woken);
        let frame = tree::match_frame(&snap).expect("the proxied frame");
        assert_eq!(frame.children[0].name, "Save");
    }
}
