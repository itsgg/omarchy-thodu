//! A session's keys and what they do (REQ-SEARCH-01, REQ-SEARCH-02,
//! REQ-ACT-01, REQ-ACT-02, REQ-GRID-01).
//!
//! With labels_only, the default, letters type labels and / starts a
//! search; otherwise lowercase letters search the targets' text and Shift
//! types a label. The final label character's case decides: lowercase
//! clicks at once, uppercase selects the target for Return to act on, so no
//! click is sent while Shift is held. Tab moves the focus; Return acts on
//! the focused target; Escape ends the session.
//! Space with nothing focused opens the grid, where letters choose cells.
//! The session decides; `thodud` carries the action out.

use serde::{Deserialize, Serialize};

use crate::config::{Config, StatusDisplay};
use crate::grid;
use crate::labels;
use crate::model::{Rect, Role, Target};

/// A second Return this soon after a click makes it a double click.
pub const DOUBLE_MS: u64 = 300;

/// A key as the overlay reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "key", rename_all = "kebab-case")]
pub enum Key {
    Char {
        c: char,
        shift: bool,
    },
    /// An uppercase character from the generated key text. Completing a
    /// label with this key activates its target (REQ-ACT-01).
    Upper {
        c: char,
    },
    Digit {
        digit: u8,
    },
    Return {
        shift: bool,
        ctrl: bool,
    },
    Tab {
        shift: bool,
    },
    Backspace,
    Escape,
    Space,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollAxis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScrollAction {
    pub target: usize,
    pub axis: ScrollAxis,
    /// Signed wheel detents. Positive vertical scrolls up; positive
    /// horizontal scrolls right, following Linux input event conventions.
    pub value: i32,
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
    /// The focus came from completing a label, so the overlay can emphasize
    /// the label chip without outlining the whole target (REQ-SESSION-01).
    pub label_selected: bool,
    /// Focus to restore when an incomplete label prefix is cancelled or
    /// does not match a visible label.
    prefix_origin: Option<(Option<usize>, bool)>,
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
    pub scroll_mode: bool,
    pub status_display: StatusDisplay,
    pub scroll_step: u16,
    pub scroll_shift_step: u16,
    pub scroll_number: String,
    scroll_number_origin: Option<Option<usize>>,
    scroll_action: Option<ScrollAction>,
    /// A disjoint label namespace for targets added after the first draw.
    reserved_label_prefix: Option<char>,
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
            label_selected: false,
            prefix_origin: None,
            state: State::Showing,
            labels_only,
            searching: false,
            act: None,
            acted_at: 0,
            area: None,
            grid: Vec::new(),
            generation: 0,
            scroll_mode: false,
            scroll_number: String::new(),
            scroll_number_origin: None,
            scroll_action: None,
            status_display: StatusDisplay::Auto,
            scroll_step: 1,
            scroll_shift_step: 5,
            reserved_label_prefix: None,
        }
    }

    /// A keyboard scroll session with decimal area labels and first-area focus.
    pub fn new_scroll(targets: Vec<Target>) -> Self {
        let mut session = Self::new(targets, false);
        session.scroll_mode = true;
        session.labels = (1..=session.targets.len())
            .map(|number| number.to_string())
            .collect();
        session.focused = (!session.targets.is_empty()).then_some(0);
        session
    }

    pub fn take_scroll_action(&mut self) -> Option<ScrollAction> {
        self.scroll_action.take()
    }

    pub fn pending_scroll_action(&self) -> Option<ScrollAction> {
        self.scroll_action
    }

    pub fn resume_after_scroll(&mut self) {
        self.state = State::Showing;
        self.act = None;
        self.scroll_action = None;
    }

    pub fn clear_scroll_targets(&mut self) {
        self.targets.clear();
        self.labels.clear();
        self.focused = None;
        self.scroll_number.clear();
        self.scroll_number_origin = None;
        self.scroll_action = None;
        self.state = State::Showing;
        self.generation = self.generation.wrapping_add(1);
    }

    pub fn with_config(mut self, config: &Config) -> Self {
        self.status_display = config.status_display;
        self.scroll_step = config.scroll_step;
        self.scroll_shift_step = config.scroll_shift_step;
        self
    }

    fn scroll_digit(&mut self, digit: u8) {
        if self.scroll_number.is_empty() {
            self.scroll_number_origin = Some(self.focused);
        }
        self.scroll_number.push(char::from(b'0' + digit));
        let selected_index = |number: &str, count: usize| {
            let number = if number == "0" {
                Some(10)
            } else {
                number.parse::<usize>().ok()
            };
            number
                .and_then(|number| number.checked_sub(1))
                .filter(|index| *index < count)
        };
        if let Some(index) = selected_index(&self.scroll_number, self.targets.len()) {
            self.focused = Some(index);
        } else {
            // Keep labels above 10 addressable, while letting a new valid
            // one-digit label recover from an out-of-range prefix such as 12
            // when the session has only two areas.
            self.scroll_number.clear();
            self.scroll_number.push(char::from(b'0' + digit));
            if let Some(index) = selected_index(&self.scroll_number, self.targets.len()) {
                self.focused = Some(index);
            }
        }
        self.label_selected = false;
    }

    fn scroll_key(&mut self, key: Key) {
        match key {
            Key::Digit { digit } if digit <= 9 => self.scroll_digit(digit),
            Key::Tab { shift } => {
                self.scroll_number.clear();
                self.scroll_number_origin = None;
                self.move_focus(shift);
            }
            Key::Backspace => {
                if self.scroll_number.pop().is_some() {
                    if self.scroll_number.is_empty() {
                        self.focused = self.scroll_number_origin.take().flatten();
                    } else if let Ok(number) = self.scroll_number.parse::<usize>()
                        && number > 0
                        && number <= self.targets.len()
                    {
                        self.focused = Some(number - 1);
                    }
                }
            }
            Key::Return { .. } => {
                self.scroll_number.clear();
                self.scroll_number_origin = None;
            }
            Key::Escape => self.state = State::Closed,
            Key::Char { c, shift } => self.queue_scroll(c, shift),
            Key::Upper { c } => self.queue_scroll(c.to_ascii_lowercase(), true),
            Key::Digit { .. } | Key::Space => {}
        }
    }

    fn queue_scroll(&mut self, key: char, shift: bool) {
        let axis_value = match key.to_ascii_lowercase() {
            'h' => Some((ScrollAxis::Horizontal, -1)),
            'j' => Some((ScrollAxis::Vertical, -1)),
            'k' => Some((ScrollAxis::Vertical, 1)),
            'l' => Some((ScrollAxis::Horizontal, 1)),
            _ => None,
        };
        let Some((axis, direction)) = axis_value else {
            return;
        };
        let Some(target) = self.focused else {
            return;
        };
        self.scroll_number.clear();
        self.scroll_number_origin = None;
        self.scroll_action = Some(ScrollAction {
            target,
            axis,
            value: direction
                * i32::from(if shift {
                    self.scroll_shift_step
                } else {
                    self.scroll_step
                }),
        });
        self.state = State::Acting;
    }

    /// Keep `prefix` free for targets that may arrive in a later generation.
    pub fn reserving_label_prefix(mut self, prefix: char) -> Self {
        self.labels = labels::assign_reserving(self.targets.len(), prefix);
        self.reserved_label_prefix = Some(prefix);
        self
    }

    /// Replace targets while retaining labels for every target already shown.
    /// New targets receive labels below the reserved prefix.
    pub fn merge_targets_preserving_labels(&mut self, mut targets: Vec<Target>) {
        let mut ids = std::collections::HashSet::new();
        targets.retain(|target| ids.insert(target.id.clone()));
        labels::reading_order(&mut targets);
        let mut existing: std::collections::HashMap<String, String> = self
            .targets
            .iter()
            .zip(&self.labels)
            .map(|(target, label)| (target.id.clone(), label.clone()))
            .collect();
        let added_count = targets
            .iter()
            .filter(|target| !existing.contains_key(&target.id))
            .count();
        let added_labels = if existing.is_empty() {
            labels::assign(added_count)
        } else {
            labels::assign_beneath(added_count, self.reserved_label_prefix.unwrap_or('z'))
        };
        let mut added_labels = added_labels.into_iter();
        self.labels = targets
            .iter()
            .map(|target| {
                existing
                    .remove(&target.id)
                    .unwrap_or_else(|| added_labels.next().expect("one label per new target"))
            })
            .collect();
        self.targets = targets;
        self.search.clear();
        self.label_prefix.clear();
        self.prefix_origin = None;
        self.focused = None;
        self.label_selected = false;
        self.searching = false;
        self.act = None;
        self.reserved_label_prefix = None;
        self.generation = self.generation.wrapping_add(1);
    }

    /// REQ-SESSION-04: a session on `targets` over the monitor's `area`,
    /// on the grid when asked for or when no source found anything.
    pub fn begin(targets: Vec<Target>, area: Rect, labels_only: bool, grid: bool) -> Self {
        let empty = targets.is_empty();
        let mut session = Self::new(targets, labels_only).with_area(area);
        if grid || empty {
            session.open_grid();
        }
        session
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
        self.reserved_label_prefix = None;
        self.search.clear();
        self.searching = false;
        self.label_prefix.clear();
        self.prefix_origin = None;
        self.focused = None;
        self.label_selected = false;
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
            // A fine grid cell still needs its bounds outlined. The selected
            // label treatment is for a labelled app or bar target.
            self.label_selected = false;
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
        self.label_prefix.clear();
        self.prefix_origin = None;
        self.label_selected = false;
        self.focused = if self.search.is_empty() {
            None
        } else {
            visible.first().copied()
        };
    }

    fn type_label(&mut self, c: char, activate: bool, at_ms: u64) -> Option<Act> {
        if self.label_prefix.is_empty() {
            self.prefix_origin = Some((self.focused, self.label_selected));
        }
        self.label_prefix.extend(c.to_lowercase());
        let prefix = self.label_prefix.clone();
        let visible = self.visible();
        if let Some(i) = self
            .labels
            .iter()
            .enumerate()
            .find_map(|(i, label)| (visible.contains(&i) && *label == prefix).then_some(i))
        {
            self.label_prefix.clear();
            self.prefix_origin = None;
            if self.grid.is_empty() {
                self.focused = Some(i);
                self.label_selected = true;
            } else {
                self.choose_cell(i);
            }
            if activate {
                return self.act_focused(false, false, false, at_ms);
            }
        } else if self
            .labels
            .iter()
            .enumerate()
            .any(|(i, label)| visible.contains(&i) && label.starts_with(&prefix))
        {
            self.focused = None;
            self.label_selected = false;
        } else {
            self.label_prefix.clear();
            if let Some((focused, selected)) = self.prefix_origin.take() {
                self.focused = focused;
                self.label_selected = selected;
            }
        }
        None
    }

    fn move_focus(&mut self, back: bool) {
        self.label_prefix.clear();
        self.prefix_origin = None;
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
        self.label_selected = false;
    }

    fn act_focused(
        &mut self,
        shift: bool,
        ctrl: bool,
        allow_linger: bool,
        at_ms: u64,
    ) -> Option<Act> {
        let target = self.focused?;
        let plain = !shift && !ctrl;
        let act = Act {
            target,
            point: self.targets[target].bounds.center(),
            button: if shift { Button::Right } else { Button::Left },
            count: 1,
            ctrl,
            again: false,
            linger: allow_linger && plain && self.targets[target].role != Role::Entry,
        };
        self.state = State::Acting;
        self.acted_at = at_ms;
        self.act = Some(act.clone());
        Some(act)
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
            State::Acting => {
                if self.scroll_mode {
                    if matches!(key, Key::Escape) {
                        self.scroll_action = None;
                        self.state = State::Closed;
                    }
                    return None;
                }
                return self.after_act(key, at_ms);
            }
            State::Showing => {}
        }
        if self.scroll_mode {
            self.scroll_key(key);
            return None;
        }
        let in_grid = !self.grid.is_empty();
        let labels_first = (self.labels_only && !self.searching) || in_grid;
        match key {
            Key::Char {
                c: '/',
                shift: false,
            } if labels_first && !in_grid => {
                self.label_prefix.clear();
                self.prefix_origin = None;
                self.searching = true;
                self.refocus();
            }
            // An uppercase final character selects; a lowercase one clicks,
            // except on grid cells, which refine until Return.
            Key::Upper { c } => return self.type_label(c, false, at_ms),
            Key::Digit { .. } => {}
            Key::Char { c, shift } if shift || labels_first || !self.label_prefix.is_empty() => {
                return self.type_label(c, !shift && !in_grid, at_ms);
            }
            Key::Char { c, .. } => {
                self.search.push(c);
                self.refocus();
            }
            Key::Backspace => {
                if self.label_prefix.pop().is_some() {
                    if self.label_prefix.is_empty() {
                        if let Some((focused, selected)) = self.prefix_origin.take() {
                            self.focused = focused;
                            self.label_selected = selected;
                        }
                    } else {
                        self.focused = None;
                        self.label_selected = false;
                    }
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
                return self.act_focused(shift, ctrl, true, at_ms);
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
