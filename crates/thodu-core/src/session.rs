//! A session's keys and what they do (REQ-SEARCH-01, REQ-SEARCH-02,
//! REQ-ACT-01, REQ-ACT-02, REQ-GRID-01).
//!
//! Letters search the targets' text; Shift and letters type a label; Tab
//! moves the focus; Return acts on the focused target; Escape ends the
//! session. With labels_only, letters type labels and / starts a search.
//! Space with nothing focused opens the grid, where letters choose cells.
//! The session decides; `thodud` carries the action out.

use serde::{Deserialize, Serialize};

use crate::grid;
use crate::labels;
use crate::model::{Rect, Role, Target};

/// A second Return this soon after a click makes it a double click.
pub const DOUBLE_MS: u64 = 300;

/// A key as the overlay reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "key", rename_all = "kebab-case")]
pub enum Key {
    Char { c: char, shift: bool },
    Return { shift: bool, ctrl: bool },
    Tab { shift: bool },
    Backspace,
    Escape,
    Space,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum State {
    Showing,
    Acting,
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Button {
    Left,
    Right,
    Middle,
}

/// What `thodud` does once the overlay is out of the way.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Act {
    pub target: usize,
    pub point: (i32, i32),
    pub button: Button,
    pub count: u8,
    pub ctrl: bool,
    /// A second click where the first one was, making a double click.
    pub again: bool,
    /// Whether the overlay keeps the keyboard for DOUBLE_MS after this
    /// click, for a second Return. Never for an entry, where typing comes
    /// next, nor for a right or Control click, which need none.
    pub linger: bool,
}

/// The targets of one session, in reading order, with their labels.
#[derive(Clone, Debug)]
pub struct Session {
    pub targets: Vec<Target>,
    pub labels: Vec<String>,
    pub search: String,
    pub label_prefix: String,
    pub focused: Option<usize>,
    pub state: State,
    pub labels_only: bool,
    /// With labels_only, whether / started a search.
    pub searching: bool,
    pub act: Option<Act>,
    /// When the first action came, in the caller's milliseconds.
    pub acted_at: u64,
    /// The monitor's logical area, for the grid.
    pub area: Option<Rect>,
    /// The grid's areas from the monitor down to the one shown; empty when
    /// the session shows its sources' targets.
    pub grid: Vec<Rect>,
    /// Bumped whenever the targets change, so the overlay fetches them.
    pub generation: u32,
}

/// Lower case with every space removed, for matching typed text.
pub fn normalize(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

impl Session {
    pub fn new(mut targets: Vec<Target>, labels_only: bool) -> Self {
        labels::reading_order(&mut targets);
        let labels = labels::assign(targets.len());
        Self {
            targets,
            labels,
            search: String::new(),
            label_prefix: String::new(),
            focused: None,
            state: State::Showing,
            labels_only,
            searching: false,
            act: None,
            acted_at: 0,
            area: None,
            grid: Vec::new(),
            generation: 0,
        }
    }

    /// A session over the monitor's `area`, for the grid.
    pub fn with_area(mut self, area: Rect) -> Self {
        self.area = Some(area);
        self
    }

    /// Show the grid over `area`, one level below the areas before it.
    fn show_grid(&mut self, area: Rect) {
        self.grid.push(area);
        self.targets = grid::targets(&area);
        self.labels = labels::assign(self.targets.len());
        self.search.clear();
        self.label_prefix.clear();
        self.focused = None;
        self.generation += 1;
    }

    /// REQ-GRID-01: open the grid over the whole monitor.
    pub fn open_grid(&mut self) {
        if let Some(area) = self.area {
            self.grid.clear();
            self.show_grid(area);
        }
    }

    /// A cell chosen by its label: refined, or focused once it is fine.
    fn choose_cell(&mut self, i: usize) {
        let cell = self.targets[i].bounds;
        if grid::is_fine(&cell) {
            self.focused = Some(i);
        } else {
            self.show_grid(cell);
        }
    }

    /// Back to the grid's previous level.
    fn grid_back(&mut self) {
        if self.grid.len() > 1 {
            self.grid.pop();
            let area = self.grid.pop().expect("a level");
            self.show_grid(area);
        }
    }

    /// The targets still shown: every one, or those whose text holds the
    /// search, in reading order.
    pub fn visible(&self) -> Vec<usize> {
        let wanted = normalize(&self.search);
        (0..self.targets.len())
            .filter(|&i| wanted.is_empty() || normalize(&self.targets[i].text).contains(&wanted))
            .collect()
    }

    fn refocus(&mut self) {
        let visible = self.visible();
        self.focused = if self.search.is_empty() {
            None
        } else {
            visible.first().copied()
        };
    }

    fn type_label(&mut self, c: char) {
        self.label_prefix.extend(c.to_lowercase());
        let prefix = self.label_prefix.clone();
        if let Some(i) = self.labels.iter().position(|l| *l == prefix) {
            self.label_prefix.clear();
            if self.grid.is_empty() {
                self.focused = Some(i);
            } else {
                self.choose_cell(i);
            }
        } else if !self.labels.iter().any(|l| l.starts_with(&prefix)) {
            self.label_prefix.clear();
        }
    }

    fn move_focus(&mut self, back: bool) {
        let visible = self.visible();
        if visible.is_empty() {
            return;
        }
        let at = self
            .focused
            .and_then(|f| visible.iter().position(|&v| v == f));
        let next = match (at, back) {
            (None, false) => 0,
            (None, true) => visible.len() - 1,
            (Some(i), false) => (i + 1) % visible.len(),
            (Some(i), true) => (i + visible.len() - 1) % visible.len(),
        };
        self.focused = Some(visible[next]);
    }

    /// Handle one key with no clock: every key counts as at the same time.
    pub fn key(&mut self, key: Key) -> Option<Act> {
        self.key_at(key, 0)
    }

    /// Handle one key pressed at `at_ms`; returns the action when the key
    /// acts on a target.
    pub fn key_at(&mut self, key: Key, at_ms: u64) -> Option<Act> {
        match self.state {
            State::Closed => return None,
            State::Acting => return self.after_act(key, at_ms),
            State::Showing => {}
        }
        let in_grid = !self.grid.is_empty();
        let labels_first = (self.labels_only && !self.searching) || in_grid;
        match key {
            Key::Char {
                c: '/',
                shift: false,
            } if labels_first && !in_grid => self.searching = true,
            Key::Char { c, shift } if shift || labels_first => self.type_label(c),
            Key::Char { c, .. } => {
                self.search.push(c);
                self.refocus();
            }
            Key::Backspace => {
                if self.label_prefix.pop().is_some() {
                    // The typed label lost a letter.
                } else if in_grid {
                    self.grid_back();
                } else {
                    if self.search.pop().is_none() {
                        self.searching = false;
                    }
                    self.refocus();
                }
            }
            Key::Tab { shift } => self.move_focus(shift),
            Key::Escape => self.state = State::Closed,
            Key::Space => {
                if self.focused.is_none() && !in_grid {
                    self.open_grid();
                }
            }
            Key::Return { shift, ctrl } => {
                let target = self.focused?;
                let plain = !shift && !ctrl;
                let act = Act {
                    target,
                    point: self.targets[target].bounds.center(),
                    button: if shift { Button::Right } else { Button::Left },
                    count: 1,
                    ctrl,
                    again: false,
                    linger: plain && self.targets[target].role != Role::Entry,
                };
                self.state = State::Acting;
                self.acted_at = at_ms;
                self.act = Some(act.clone());
                return Some(act);
            }
        }
        None
    }

    /// REQ-ACT-02: after a click the overlay may still hold the keyboard; a
    /// plain Return within DOUBLE_MS clicks again, and any key ends the
    /// session.
    fn after_act(&mut self, key: Key, at_ms: u64) -> Option<Act> {
        self.state = State::Closed;
        let first = self.act.clone()?;
        let plain_return = key
            == Key::Return {
                shift: false,
                ctrl: false,
            };
        if !first.linger || first.again || !plain_return {
            return None;
        }
        if at_ms.saturating_sub(self.acted_at) > DOUBLE_MS {
            return None;
        }
        let again = Act {
            again: true,
            linger: false,
            ..first
        };
        self.act = Some(again.clone());
        Some(again)
    }
}
