//! The target model of SPEC section 5, in Hyprland's logical coordinates.

use serde::{Deserialize, Serialize};

/// A rectangle in logical coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    pub const fn right(&self) -> i32 {
        self.x + self.w
    }

    pub const fn bottom(&self) -> i32 {
        self.y + self.h
    }

    pub const fn is_empty(&self) -> bool {
        self.w <= 0 || self.h <= 0
    }

    /// The center, rounded down, as the point a click lands on.
    pub const fn center(&self) -> (i32, i32) {
        (self.x + self.w / 2, self.y + self.h / 2)
    }

    pub const fn contains_point(&self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }

    pub const fn contains(&self, other: &Rect) -> bool {
        other.x >= self.x
            && other.y >= self.y
            && other.right() <= self.right()
            && other.bottom() <= self.bottom()
    }

    /// The overlap of two rectangles, if they overlap at all.
    pub fn intersect(&self, other: &Rect) -> Option<Rect> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        let rect = Rect::new(x, y, right - x, bottom - y);
        (!rect.is_empty()).then_some(rect)
    }
}

/// What a target is, as far as acting on it goes (SPEC section 5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    Button,
    Link,
    Entry,
    MenuItem,
    Tab,
    ListItem,
    Check,
    Toggle,
    Combo,
    Slider,
    ScrollArea,
    Text,
    Unknown,
}

/// Where a target came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Source {
    Atspi,
    Shell,
    Image,
    Window,
    Grid,
}

/// What can be done to a target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActionKind {
    Click,
    Right,
    Middle,
    Double,
    Focus,
    Scroll,
}

/// Something a mouse could act on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Target {
    pub id: String,
    pub bounds: Rect,
    pub role: Role,
    pub text: String,
    pub source: Source,
    pub actions: Vec<ActionKind>,
    /// The Hyprland window address, empty for shell targets.
    pub window: String,
    /// How to tell before a click that the element is still where it was;
    /// left out of the IPC and recorded cases.
    #[serde(skip)]
    pub check: Option<TargetCheck>,
}

/// How a target is checked just before a click (REQ-ACT-03): an accessible
/// element is read again, an image target's region captured again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TargetCheck {
    Element(ElementCheck),
    Pixels(PixelCheck),
}

/// An image target's region, the monitor area and scale (in thousandths) of
/// the capture it was detected in, and the fingerprint of its pixels then.
/// The recheck captures the same area at the same scale, so unchanged pixels
/// are sampled exactly as before.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PixelCheck {
    pub region: Rect,
    pub area: Rect,
    pub scale_permille: u32,
    pub fingerprint: [u8; crate::pixels::CELLS],
}

/// An accessible element's address on the accessibility bus.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ElementRef {
    pub bus: String,
    pub path: String,
}

/// An accessible element and its window-relative extents when collected,
/// to read again before a click (REQ-ACT-03).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ElementCheck {
    pub at: ElementRef,
    pub extents: [i32; 4],
}

/// A monitor as Hyprland reports it: origin in logical coordinates, size in
/// physical pixels, and the scale between them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Monitor {
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub scale: f64,
    /// The layer-shell surfaces Hyprland reports on this monitor, for deciding
    /// what takes a click; left out of recorded snapshots.
    #[serde(skip)]
    pub layers: Vec<Layer>,
}

impl Monitor {
    /// The monitor's area in logical coordinates.
    pub fn logical(&self) -> Rect {
        let w = (f64::from(self.width) / self.scale).round() as i32;
        let h = (f64::from(self.height) / self.scale).round() as i32;
        Rect::new(self.x, self.y, w, h)
    }

    /// A logical point as physical pixels from the monitor's top-left.
    pub fn to_physical(&self, x: i32, y: i32) -> (i32, i32) {
        let px = (f64::from(x - self.x) * self.scale).round() as i32;
        let py = (f64::from(y - self.y) * self.scale).round() as i32;
        (px, py)
    }

    /// Physical pixels from the monitor's top-left as a logical point.
    pub fn to_logical(&self, px: i32, py: i32) -> (i32, i32) {
        let x = (f64::from(px) / self.scale).round() as i32 + self.x;
        let y = (f64::from(py) / self.scale).round() as i32 + self.y;
        (x, y)
    }
}

/// The parts of a Hyprland window the session depends on.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Window {
    pub address: String,
    pub class: String,
    pub title: String,
    pub pid: u32,
    pub at: (i32, i32),
    pub size: (i32, i32),
    pub workspace: i64,
    pub monitor: i64,
    /// Hyprland compositor state used to resolve overlap ownership. This is
    /// deliberately omitted from recorded snapshots and the IPC contract.
    #[serde(skip)]
    pub floating: bool,
    #[serde(skip)]
    pub pinned: bool,
    #[serde(skip)]
    pub fullscreen: i64,
    #[serde(skip)]
    pub focus_history_id: i64,
    /// Hyprland draws this window above a fullscreen window on its workspace.
    #[serde(skip)]
    pub allowed_over_fullscreen: bool,
    /// The window takes no pointer input, so it owns no point.
    #[serde(skip)]
    pub ignores_input: bool,
    /// Whether Hyprland's active-window query selected this window for the
    /// current collection request.
    #[serde(skip)]
    pub focused: bool,
}

impl Window {
    pub const fn rect(&self) -> Rect {
        Rect::new(self.at.0, self.at.1, self.size.0, self.size.1)
    }

    /// Approximate Hyprland's top-to-bottom scene layers. Hyprland draws
    /// pinned windows over everything, a special workspace over the regular
    /// one, floating windows allowed over fullscreen above a fullscreen
    /// window and the rest below it, floating windows over tiled windows,
    /// and its focused window last within a layer. Entering fullscreen clears
    /// `allowed_over_fullscreen` on every other window of the workspace, so
    /// without a fullscreen window every floating window carries it.
    pub fn stacking_key(&self) -> (std::cmp::Reverse<u8>, i64) {
        let layer = if self.pinned {
            5
        } else if self.workspace < 0 {
            4
        } else if self.floating && self.allowed_over_fullscreen {
            3
        } else if self.fullscreen == 2 {
            2
        } else if self.floating {
            1
        } else {
            0
        };
        (std::cmp::Reverse(layer), self.focus_history_id)
    }
}

/// The width Omarchy's notification toasts take at a monitor's right edge:
/// a card of 380 units and its gap, with room for a larger spacing scale.
const NOTIFICATION_BAND: i32 = 440;

/// A layer-shell surface on a monitor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layer {
    pub namespace: String,
    /// Hyprland's level: 0 background, 1 bottom, 2 top, 3 overlay.
    pub level: u8,
    pub rect: Rect,
}

impl Layer {
    /// Whether the surface takes pointer input at a point. Hyprland reports a
    /// layer's whole surface but not its input region, so Omarchy's
    /// full-screen surfaces with input masks are known by namespace: Thodu's
    /// own overlay and the OSD take none, and notifications take only their
    /// toast column. Any other layer above the windows takes its whole surface.
    pub fn takes_input_at(&self, x: i32, y: i32) -> bool {
        match self.namespace.as_str() {
            "thodu" | "omarchy-osd" => false,
            "omarchy-notifications" => Rect::new(
                self.rect.right() - NOTIFICATION_BAND,
                self.rect.y,
                NOTIFICATION_BAND,
                self.rect.h,
            )
            .contains_point(x, y),
            _ => self.level >= 2 && self.rect.contains_point(x, y),
        }
    }
}

/// What receives a click at a point.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Owner {
    Window(String),
    Layer(String),
    Nothing,
}

impl Owner {
    /// The owning window's address, when a window owns the point.
    pub fn window(&self) -> Option<&str> {
        match self {
            Owner::Window(address) => Some(address),
            _ => None,
        }
    }
}

/// What receives a click at a point among a monitor's visible windows and
/// layers, in Hyprland's drawing order: overlay layers first; then, when a
/// fullscreen window is shown, the pinned, special-workspace and
/// allowed-over-fullscreen windows above it and the fullscreen window
/// itself, which covers top layers and the windows behind it ("if in
/// fullscreen, only overlay can be above", Hyprland 0.56's LayerSurface.cpp);
/// otherwise top layers such as the bar, then windows front to back. A top
/// layer mapped after the window went fullscreen is drawn above it, but
/// Hyprland's layer report does not say so; Omarchy's menus and panels use
/// the overlay level.
pub fn owner_at(windows: &[Window], monitor: &Monitor, x: i32, y: i32) -> Owner {
    owner_among(windows, &monitor.layers, x, y)
}

/// `owner_at` over a monitor's windows and layers given apart.
pub fn owner_among(windows: &[Window], layers: &[Layer], x: i32, y: i32) -> Owner {
    let layer_at = |level: u8| {
        layers
            .iter()
            .find(|layer| layer.level == level && layer.takes_input_at(x, y))
            .map(|layer| Owner::Layer(layer.namespace.clone()))
    };
    if let Some(owner) = layer_at(3) {
        return owner;
    }
    let under = |window: &&Window| !window.ignores_input && window.rect().contains_point(x, y);
    let front = |candidates: Vec<&Window>| {
        candidates
            .into_iter()
            .min_by_key(|window| window.stacking_key())
            .map(|window| Owner::Window(window.address.clone()))
    };
    if let Some(fullscreen) = windows
        .iter()
        .find(|window| window.fullscreen == 2 && window.workspace >= 0)
    {
        let above = windows
            .iter()
            .filter(under)
            .filter(|window| {
                window.address != fullscreen.address
                    && (window.pinned || window.workspace < 0 || window.allowed_over_fullscreen)
            })
            .collect();
        if let Some(owner) = front(above) {
            return owner;
        }
        if fullscreen.rect().contains_point(x, y) {
            return if fullscreen.ignores_input {
                Owner::Nothing
            } else {
                Owner::Window(fullscreen.address.clone())
            };
        }
    }
    if let Some(owner) = layer_at(2) {
        return owner;
    }
    front(windows.iter().filter(under).collect()).unwrap_or(Owner::Nothing)
}

/// Whether a window is drawn at all beside a fullscreen window among the
/// visible ones: the fullscreen window and those Hyprland draws above it.
pub fn shown_beside_fullscreen(window: &Window, windows: &[Window]) -> bool {
    match windows
        .iter()
        .find(|other| other.fullscreen == 2 && other.workspace >= 0)
    {
        None => true,
        Some(fullscreen) => {
            window.address == fullscreen.address
                || window.pinned
                || window.workspace < 0
                || window.allowed_over_fullscreen
        }
    }
}
