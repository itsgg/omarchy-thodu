//! From an accessibility tree snapshot to targets (REQ-SRC-01, REQ-SRC-04).
//!
//! `thodud` reads the live tree into the same `Snapshot` shape that the
//! recorded trees in spec/cases/atspi/ use, so the live path and the replayed
//! one go through the code here alike.

use serde::{Deserialize, Serialize};

use crate::model::{ActionKind, Monitor, Rect, Role, Source, Target, Window};

/// One accessible element, with its extents relative to its window.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node {
    pub role: String,
    #[serde(default)]
    pub states: Vec<String>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// x, y, width, height relative to the window, when the element has them.
    #[serde(default)]
    pub extents: Option<[i32; 4]>,
    #[serde(default)]
    pub children: Vec<Node>,
}

impl Node {
    pub fn has(&self, state: &str) -> bool {
        self.states.iter().any(|s| s == state)
    }

    fn shown(&self) -> bool {
        self.has("showing") && self.has("visible")
    }
}

/// An application on the accessibility bus and its top-level frames.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct App {
    pub name: String,
    pub pid: u32,
    #[serde(default)]
    pub frames: Vec<Node>,
}

/// What a collection saw: the window Hyprland reported, its monitor, and the
/// applications on the bus with their frames.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub window: Window,
    pub monitor: Monitor,
    #[serde(default)]
    pub apps: Vec<App>,
}

/// REQ-SRC-01: the focused window's frame, found by the process id Hyprland
/// reports and the frame's ACTIVE state, then by extents matching the
/// window's size. An application's name is never compared.
pub fn match_frame(snapshot: &Snapshot) -> Option<&Node> {
    let frames: Vec<&Node> = snapshot
        .apps
        .iter()
        .filter(|app| app.pid == snapshot.window.pid)
        .flat_map(|app| app.frames.iter())
        .filter(|frame| frame.shown())
        .collect();
    if let Some(active) = frames.iter().find(|frame| frame.has("active")) {
        return Some(active);
    }
    let size = snapshot.window.size;
    let sized: Vec<&&Node> = frames
        .iter()
        .filter(|frame| frame.extents.is_some_and(|e| (e[2], e[3]) == size))
        .collect();
    match (sized.len(), frames.len()) {
        (1, _) => Some(sized[0]),
        (_, 1) => Some(frames[0]),
        _ => None,
    }
}

/// The role a target gets for an accessibility role name, or None when the
/// element is not something a mouse acts on.
pub fn role_of(name: &str) -> Option<Role> {
    let role = match name {
        "button" | "push button" | "push button menu" => Role::Button,
        "toggle button" => Role::Toggle,
        "check box" | "radio button" | "switch" => Role::Check,
        "link" => Role::Link,
        "entry" | "text" | "password text" | "editbar" | "search box" => Role::Entry,
        "menu item" | "check menu item" | "radio menu item" | "menu" => Role::MenuItem,
        "page tab" => Role::Tab,
        "list item" | "tree item" | "table cell" | "table row" => Role::ListItem,
        "combo box" => Role::Combo,
        "slider" | "spin button" => Role::Slider,
        _ => return None,
    };
    Some(role)
}

/// Roles drawn above what came before them in the tree; an earlier element
/// they cover is not clickable while they show.
fn is_layer(role: &str) -> bool {
    matches!(
        role,
        "popup menu" | "dialog" | "alert" | "tooltip" | "window"
    )
}

const fn actions_for(role: Role) -> &'static [ActionKind] {
    match role {
        Role::Entry => &[ActionKind::Click, ActionKind::Focus],
        Role::Text | Role::ScrollArea | Role::Unknown => &[ActionKind::Click],
        _ => &[ActionKind::Click, ActionKind::Right, ActionKind::Double],
    }
}

struct Found {
    target: Target,
    order: usize,
}

/// REQ-SRC-04: the clickable elements under `frame`, in logical coordinates.
/// Elements that are not SHOWING and VISIBLE, and everything under them, are
/// dropped, as are elements outside the window, with no area, or covered by a
/// later layer (a menu, popup or dialog) of the same window.
pub fn targets(frame: &Node, window: &Window, monitor: &Monitor) -> Vec<Target> {
    let area = match window.rect().intersect(&monitor.logical()) {
        Some(area) => area,
        None => return Vec::new(),
    };
    let mut found = Vec::new();
    let mut layers = Vec::new();
    let mut order = 0;
    walk(frame, window, area, &mut found, &mut layers, &mut order);
    found
        .into_iter()
        .filter(|f| {
            !layers.iter().any(|(layer, at): &(Rect, usize)| {
                *at > f.order && layer.contains(&f.target.bounds)
            })
        })
        .enumerate()
        .map(|(i, mut f)| {
            f.target.id = format!("a{i}");
            f.target
        })
        .collect()
}

fn walk(
    node: &Node,
    window: &Window,
    area: Rect,
    found: &mut Vec<Found>,
    layers: &mut Vec<(Rect, usize)>,
    order: &mut usize,
) {
    if !node.shown() || node.has("defunct") {
        return;
    }
    *order += 1;
    let bounds = node
        .extents
        .map(|[x, y, w, h]| Rect::new(window.at.0 + x, window.at.1 + y, w, h))
        .and_then(|r| r.intersect(&area));
    if let Some(bounds) = bounds {
        if is_layer(&node.role) && *order > 1 {
            layers.push((bounds, *order));
        }
        if let Some(role) = role_of(&node.role) {
            let text = [node.name.trim(), node.description.trim()]
                .iter()
                .filter(|s| !s.is_empty())
                .copied()
                .collect::<Vec<_>>()
                .join(" ");
            found.push(Found {
                target: Target {
                    id: String::new(),
                    bounds,
                    role,
                    text,
                    source: Source::Atspi,
                    actions: actions_for(role).to_vec(),
                    window: window.address.clone(),
                },
                order: *order,
            });
        }
    }
    for child in &node.children {
        walk(child, window, area, found, layers, order);
    }
}
