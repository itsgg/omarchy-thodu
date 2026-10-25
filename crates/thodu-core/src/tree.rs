//! From an accessibility tree snapshot to targets (REQ-SRC-01, REQ-SRC-04).
//!
//! `thodud` reads the live tree into the same `Snapshot` shape that the
//! recorded trees in spec/cases/atspi/ use, so the live path and the replayed
//! one go through the code here alike.

use serde::{Deserialize, Serialize};

use std::collections::HashMap;

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
    /// The name of the element's first action, when it has one. Chromium
    /// calls it "click" for an element the page listens for clicks on,
    /// whatever its role, and "doDefault" for one it does not.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub action: String,
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

/// REQ-SRC-01: the focused window's frame, found by process and ACTIVE
/// state, then by extents matching its size. An application's name is never
/// compared.
pub fn match_frame(snapshot: &Snapshot) -> Option<&Node> {
    match_frame_for_window(snapshot, true)
}

/// Match the frame belonging to one window. ACTIVE is authoritative only
/// for the focused window; a background sibling needs unique matching size.
pub fn match_frame_for_window(snapshot: &Snapshot, focused: bool) -> Option<&Node> {
    let (app, frame) = match_frame_index(snapshot, focused)?;
    Some(&snapshot.apps[app].frames[frame])
}

/// The app and frame indices matched to one window, in `snapshot.apps`.
pub fn match_frame_index(snapshot: &Snapshot, focused: bool) -> Option<(usize, usize)> {
    let frames: Vec<(usize, usize, &Node)> = snapshot
        .apps
        .iter()
        .enumerate()
        .filter(|(_, app)| app.pid == snapshot.window.pid)
        .flat_map(|(a, app)| {
            app.frames
                .iter()
                .enumerate()
                .map(move |(f, frame)| (a, f, frame))
        })
        .filter(|(_, _, frame)| frame.shown() && (focused || !frame.has("active")))
        .collect();
    if focused && let Some((app, frame, _)) = frames.iter().find(|(_, _, node)| node.has("active"))
    {
        return Some((*app, *frame));
    }
    let size = snapshot.window.size;
    let sized: Vec<_> = frames
        .iter()
        .filter(|(_, _, frame)| frame.extents.is_some_and(|e| (e[2], e[3]) == size))
        .collect();
    if sized.len() == 1 {
        Some((sized[0].0, sized[0].1))
    } else if focused && sized.is_empty() && frames.len() == 1 {
        Some((frames[0].0, frames[0].1))
    } else {
        None
    }
}

/// Match frames to several windows without assigning one AT-SPI frame to
/// multiple Hyprland windows. A candidate claimed by more than one window is
/// ambiguous and omitted for all of them.
pub fn match_frame_indices(snapshots: &[Snapshot]) -> Vec<Option<(usize, usize)>> {
    let candidates: Vec<_> = snapshots
        .iter()
        .map(|snapshot| match_frame_index(snapshot, snapshot.window.focused))
        .collect();
    let mut claims = HashMap::new();
    for candidate in candidates.iter().flatten() {
        *claims.entry(*candidate).or_insert(0usize) += 1;
    }
    candidates
        .into_iter()
        .map(|candidate| candidate.filter(|frame| claims.get(frame) == Some(&1)))
        .collect()
}

/// The role a target gets for an accessibility role name, or None when the
/// element is not something a mouse acts on.
pub fn role_of(name: &str) -> Option<Role> {
    let role = match name {
        "button" | "push button" | "push button menu" => Role::Button,
        "toggle button" => Role::Toggle,
        // GTK4 names its roles as ARIA does: checkbox, radio, text box.
        "check box" | "checkbox" | "radio button" | "radio" | "switch" => Role::Check,
        "link" => Role::Link,
        "entry" | "text" | "text box" | "password text" | "editbar" | "search box" => Role::Entry,
        "menu item" | "check menu item" | "radio menu item" | "menu" | "menu item checkbox"
        | "menu item radio" => Role::MenuItem,
        "page tab" | "tab" => Role::Tab,
        "list item" | "tree item" | "table cell" | "table row" | "row" | "option" | "grid cell" => {
            Role::ListItem
        }
        "combo box" => Role::Combo,
        "slider" | "spin button" => Role::Slider,
        _ => return None,
    };
    Some(role)
}

/// Roles that identify a region a pointer wheel can scroll. Scroll bars are
/// omitted because their narrow bounds are poor wheel targets.
pub fn is_scroll_area_role(name: &str) -> bool {
    matches!(
        name,
        "scroll pane" | "viewport" | "document web" | "document frame"
    )
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
        Role::ScrollArea => &[ActionKind::Scroll],
        Role::Text | Role::Unknown => &[ActionKind::Click],
        _ => &[ActionKind::Click, ActionKind::Right, ActionKind::Double],
    }
}

struct Found {
    target: Target,
    order: usize,
}

/// REQ-SRC-04: the clickable elements under `frame`, in logical coordinates.
/// Elements that are not SHOWING and VISIBLE, and everything under them, are
/// dropped, as are elements outside the window, with no area, or whose click
/// point is covered by a later layer (a menu, popup or dialog) of the same
/// window. An element with
/// exactly the bounds of an earlier one, as GTK4's combo box and the button
/// inside it have, is the same control and is dropped too.
///
/// REQ-SRC-02: extents are relative to the window. GTK3, GTK4 and Qt give
/// them in logical units; Chromium and Electron give the window's own in
/// logical units and their web document's in physical pixels, so a document
/// the monitor's scale times its parent's size is read in physical pixels.
pub fn targets(frame: &Node, window: &Window, monitor: &Monitor) -> Vec<Target> {
    let area = match window.rect().intersect(&monitor.logical()) {
        Some(area) => area,
        None => return Vec::new(),
    };
    let mut found = Vec::new();
    let mut layers = Vec::new();
    let mut order = 0;
    let mut at = Walk {
        window,
        scale: monitor.scale,
        area,
        found: &mut found,
        layers: &mut layers,
        order: &mut order,
    };
    walk(frame, &mut at, 1.0, None, false);
    let mut seen: Vec<Rect> = Vec::new();
    found
        .into_iter()
        .filter(|f| {
            let (x, y) = f.target.bounds.center();
            !layers
                .iter()
                .any(|(layer, at): &(Rect, usize)| *at > f.order && layer.contains_point(x, y))
        })
        .filter(|f| {
            let new = !seen.contains(&f.target.bounds);
            seen.push(f.target.bounds);
            new
        })
        .enumerate()
        .map(|(i, mut f)| {
            f.target.id = format!("a{i}");
            f.target
        })
        .collect()
}

/// Scrollable areas in one visible accessibility frame (REQ-SRC-08).
/// Exact duplicate bounds keep the most specific role, so a document and
/// its viewport do not produce indistinguishable labels.
pub fn scroll_targets(frame: &Node, window: &Window, monitor: &Monitor) -> Vec<Target> {
    let Some(area) = window.rect().intersect(&monitor.logical()) else {
        return Vec::new();
    };
    fn rank(role: &str) -> u8 {
        match role {
            "scroll pane" => 4,
            "document web" => 3,
            "viewport" => 2,
            "document frame" => 1,
            _ => 0,
        }
    }
    struct ScrollWalk<'a> {
        window: &'a Window,
        monitor: &'a Monitor,
        area: Rect,
        order: usize,
        found: Vec<(u8, usize, Target)>,
    }
    impl ScrollWalk<'_> {
        fn visit(&mut self, node: &Node, div: f64, parent: Option<[i32; 4]>) {
            if !node.shown() || node.has("defunct") {
                return;
            }
            self.order += 1;
            let div = match (node.extents, parent) {
                (Some(e), Some(p))
                    if div == 1.0
                        && is_document(&node.role)
                        && scaled(e, p, self.monitor.scale) =>
                {
                    self.monitor.scale
                }
                _ => div,
            };
            if is_scroll_area_role(&node.role)
                && let Some([x, y, w, h]) = node.extents
            {
                let logical = |v: i32| (f64::from(v) / div).round() as i32;
                let bounds = Rect::new(
                    self.window.at.0 + logical(x),
                    self.window.at.1 + logical(y),
                    logical(w),
                    logical(h),
                )
                .intersect(&self.area);
                if let Some(bounds) = bounds {
                    let text = [node.name.trim(), node.description.trim()]
                        .iter()
                        .filter(|value| !value.is_empty())
                        .copied()
                        .collect::<Vec<_>>()
                        .join(" ");
                    self.found.push((
                        rank(&node.role),
                        self.order,
                        Target {
                            id: String::new(),
                            bounds,
                            role: Role::ScrollArea,
                            text,
                            source: Source::Atspi,
                            actions: vec![ActionKind::Scroll],
                            window: self.window.address.clone(),
                        },
                    ));
                }
            }
            let parent = node.extents.or(parent);
            for child in &node.children {
                self.visit(child, div, parent);
            }
        }
    }

    let mut walk = ScrollWalk {
        window,
        monitor,
        area,
        order: 0,
        found: Vec::new(),
    };
    walk.visit(frame, 1.0, None);
    let mut found = walk.found;
    found.sort_by_key(|(_, order, target)| (target.bounds.y, target.bounds.x, *order));
    let mut unique: Vec<(u8, usize, Target)> = Vec::new();
    for candidate in found {
        if let Some(existing) = unique
            .iter_mut()
            .find(|(_, _, target)| target.bounds == candidate.2.bounds)
        {
            if candidate.0 > existing.0 {
                *existing = candidate;
            }
        } else {
            unique.push(candidate);
        }
    }
    unique.sort_by_key(|(_, order, target)| (target.bounds.y, target.bounds.x, *order));
    unique
        .into_iter()
        .enumerate()
        .map(|(index, (_, _, mut target))| {
            target.id = format!("s{index}");
            target
        })
        .collect()
}

struct Walk<'a> {
    window: &'a Window,
    scale: f64,
    area: Rect,
    found: &'a mut Vec<Found>,
    layers: &'a mut Vec<(Rect, usize)>,
    order: &'a mut usize,
}

/// Whether `extents` are `scale` times `parent`'s size, within two pixels.
fn scaled(extents: [i32; 4], parent: [i32; 4], scale: f64) -> bool {
    let near = |v: i32, p: i32| (f64::from(v) - f64::from(p) * scale).abs() <= 2.0;
    scale > 1.01 && near(extents[2], parent[2]) && near(extents[3], parent[3])
}

fn walk(node: &Node, at: &mut Walk<'_>, div: f64, parent: Option<[i32; 4]>, inside_row: bool) {
    if !node.shown() || node.has("defunct") {
        return;
    }
    *at.order += 1;
    let div = match (node.extents, parent) {
        (Some(e), Some(p)) if div == 1.0 && is_document(&node.role) && scaled(e, p, at.scale) => {
            at.scale
        }
        _ => div,
    };
    let logical = |v: i32| (f64::from(v) / div).round() as i32;
    let bounds = node
        .extents
        .map(|[x, y, w, h]| {
            Rect::new(
                at.window.at.0 + logical(x),
                at.window.at.1 + logical(y),
                logical(w),
                logical(h),
            )
        })
        .and_then(|r| r.intersect(&at.area));
    if let Some(bounds) = bounds {
        if is_layer(&node.role) && *at.order > 1 {
            at.layers.push((bounds, *at.order));
        }
        // GTK4 list views expose one actionable row and a grid cell for each
        // column. The cells share the row action, so label the row once while
        // still descending to retain any independent controls inside a cell.
        let role = if inside_row && matches!(node.role.as_str(), "grid cell" | "table cell") {
            None
        } else {
            role_of(&node.role).or_else(|| (node.action == "click").then_some(Role::Button))
        };
        if let Some(role) = role {
            let text = [node.name.trim(), node.description.trim()]
                .iter()
                .filter(|s| !s.is_empty())
                .copied()
                .collect::<Vec<_>>()
                .join(" ");
            at.found.push(Found {
                target: Target {
                    id: String::new(),
                    bounds,
                    role,
                    text,
                    source: Source::Atspi,
                    actions: actions_for(role).to_vec(),
                    window: at.window.address.clone(),
                },
                order: *at.order,
            });
        }
    }
    let parent = node.extents.or(parent);
    for child in &node.children {
        walk(child, at, div, parent, inside_row || node.role == "row");
    }
}

/// A web document, whose extents Chromium gives in physical pixels.
fn is_document(role: &str) -> bool {
    matches!(role, "document web" | "document frame")
}
