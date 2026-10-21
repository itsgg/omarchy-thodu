//! Labels and where they are drawn (REQ-LABEL-01, REQ-LABEL-02).

use crate::model::{Rect, Target};

/// The home row first, then the rest of the letters.
pub const ALPHABET: &str = "asdfghjklqwertyuiopzxcvbnm";

/// REQ-LABEL-01: `n` distinct labels, none a prefix of another, shortest
/// first. Labels grow a letter at a time, the way Vimium's do: the first
/// label is replaced by itself plus each letter until there are enough, so
/// every label left is a leaf and the shortest ones come first.
pub fn assign(n: usize) -> Vec<String> {
    if n == 0 {
        return Vec::new();
    }
    let letters: Vec<char> = ALPHABET.chars().collect();
    let mut labels = vec![String::new()];
    let mut offset = 0;
    while labels.len() - offset < n || labels.len() == 1 {
        let prefix = labels[offset].clone();
        offset += 1;
        for letter in &letters {
            labels.push(format!("{prefix}{letter}"));
        }
    }
    labels[offset..offset + n].to_vec()
}

/// Reading order: top to bottom, then left to right, so the targets nearest
/// the window's top-left get the shortest labels.
pub fn reading_order(targets: &mut [Target]) {
    targets.sort_by_key(|t| (t.bounds.y, t.bounds.x));
}

/// REQ-LABEL-02: where a label of `size` sits for a target: inside the
/// target, at its top-left, when the target is at least as large as the
/// label; otherwise centered on the target's top-left corner. Either way it
/// is kept on the monitor.
pub fn place(target: &Rect, size: (i32, i32), monitor: &Rect) -> Rect {
    let (w, h) = size;
    let (x, y) = if target.w >= w && target.h >= h {
        (target.x, target.y)
    } else {
        (target.x - w / 2, target.y - h / 2)
    };
    let x = x.clamp(monitor.x, (monitor.right() - w).max(monitor.x));
    let y = y.clamp(monitor.y, (monitor.bottom() - h).max(monitor.y));
    Rect::new(x, y, w, h)
}
