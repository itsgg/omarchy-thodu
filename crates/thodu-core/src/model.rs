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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Window {
    pub address: String,
    pub class: String,
    pub title: String,
    pub pid: u32,
    pub at: (i32, i32),
    pub size: (i32, i32),
    pub workspace: i64,
    pub monitor: i64,
}

impl Window {
    pub const fn rect(&self) -> Rect {
        Rect::new(self.at.0, self.at.1, self.size.0, self.size.1)
    }
}
