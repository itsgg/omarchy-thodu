//! A session's keys and what they do (REQ-SEARCH-01, REQ-ACT-01).
//!
//! Letters search the targets' text; Shift and letters type a label; Tab
//! moves the focus; Return acts on the focused target; Escape ends the
//! session. The session decides; `thodud` carries the action out.

use serde::{Deserialize, Serialize};

use crate::labels;
use crate::model::Target;

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

/// What `thodud` does once the overlay is gone.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Act {
    pub target: usize,
    pub point: (i32, i32),
    pub button: Button,
    pub count: u8,
    pub ctrl: bool,
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
    pub act: Option<Act>,
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
            act: None,
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
            self.focused = Some(i);
            self.label_prefix.clear();
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

    /// Handle one key; returns the action when the key acts on a target.
    pub fn key(&mut self, key: Key) -> Option<Act> {
        if self.state != State::Showing {
            return None;
        }
        match key {
            Key::Char { c, shift } if shift || self.labels_only => self.type_label(c),
            Key::Char { c, .. } => {
                self.search.push(c);
                self.refocus();
            }
            Key::Backspace => {
                if self.label_prefix.pop().is_none() {
                    self.search.pop();
                    self.refocus();
                }
            }
            Key::Tab { shift } => self.move_focus(shift),
            Key::Escape => self.state = State::Closed,
            Key::Space => {}
            Key::Return { shift, ctrl } => {
                let target = self.focused?;
                let act = Act {
                    target,
                    point: self.targets[target].bounds.center(),
                    button: if shift { Button::Right } else { Button::Left },
                    count: 1,
                    ctrl,
                };
                self.state = State::Acting;
                self.act = Some(act.clone());
                return Some(act);
            }
        }
        None
    }
}
