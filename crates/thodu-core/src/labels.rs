//! Labels and where they are drawn (REQ-LABEL-01, REQ-LABEL-02).

use crate::model::{Rect, Role, Source, Target};
use crate::protocol;

/// The home row first, then the rest of the letters.
pub const ALPHABET: &str = "asdfghjklqwertyuiopzxcvbnm";

/// REQ-LABEL-01: `n` distinct labels, none a prefix of another, shortest
/// first. Labels grow a letter at a time, the way Vimium's do: the first
/// label is replaced by itself plus each letter until there are enough, so
/// every label left is a leaf and the shortest ones come first.
pub fn assign(n: usize) -> Vec<String> {
    assign_from(n, ALPHABET)
}

/// Assign primary labels without using a reserved leading key, leaving its
/// prefix tree available for targets that arrive in a later generation.
pub fn assign_reserving(n: usize, reserved: char) -> Vec<String> {
    let alphabet: String = ALPHABET.chars().filter(|&c| c != reserved).collect();
    assign_from(n, &alphabet)
}

/// REQ-LABEL-01: how many labels fit beneath one reserved letter while
/// staying within three letters: every two-letter suffix. A source that
/// adds targets in a later generation, such as the image detector, caps its
/// output at this.
pub const BENEATH_CAPACITY: usize = ALPHABET.len() * ALPHABET.len();

/// Labels beneath a reserved prefix, at most `BENEATH_CAPACITY` of them so
/// none is longer than three letters. The returned code remains prefix-free.
pub fn assign_beneath(n: usize, prefix: char) -> Vec<String> {
    assign(n.min(BENEATH_CAPACITY))
        .into_iter()
        .map(|label| format!("{prefix}{label}"))
        .collect()
}

fn assign_from(n: usize, alphabet: &str) -> Vec<String> {
    if n == 0 {
        return Vec::new();
    }
    let letters: Vec<char> = alphabet.chars().collect();
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

/// Reading order: the window's own targets first, then the bar's, each top
/// to bottom and then left to right, so the targets nearest the window's
/// top-left get the shortest labels (REQ-LABEL-01).
pub fn reading_order(targets: &mut [Target]) {
    targets.sort_by_key(|t| {
        (
            t.source == Source::Shell,
            t.bounds.y,
            t.bounds.x,
            t.source != Source::Atspi,
        )
    });
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

/// Whether a target is text the image detector found, whose label is drawn
/// small and beside the text so the text stays readable (REQ-LABEL-02).
pub fn is_image_text(target: &Target) -> bool {
    target.source == Source::Image && matches!(target.role, Role::Text | Role::Link)
}

/// The space between image text and the label beside it.
pub const TEXT_LABEL_GAP: i32 = 2;

/// REQ-LABEL-02: where the label of image text sits, centered on its line:
/// just after the text's end when that spot lies inside `room` (the
/// target's window on the monitor) and `free` says it overlaps no other
/// target or label already placed, else just before its start on the same
/// terms; otherwise over the end of the text,
/// where the overlay draws it translucent. Either way it is kept on the
/// monitor.
pub fn place_text(
    target: &Rect,
    size: (i32, i32),
    room: &Rect,
    monitor: &Rect,
    free: impl Fn(&Rect) -> bool,
) -> Rect {
    let (w, h) = size;
    let y = target.y + (target.h - h) / 2;
    let fits = |chip: &Rect| room.contains(chip) && monitor.contains(chip) && free(chip);
    let after = Rect::new(target.right() + TEXT_LABEL_GAP, y, w, h);
    if fits(&after) {
        return after;
    }
    let before = Rect::new(target.x - TEXT_LABEL_GAP - w, y, w, h);
    if fits(&before) {
        return before;
    }
    let x = (target.right() - w).max(target.x);
    let x = x.clamp(monitor.x, (monitor.right() - w).max(monitor.x));
    let y = y.clamp(monitor.y, (monitor.bottom() - h).max(monitor.y));
    Rect::new(x, y, w, h)
}

/// REQ-LABEL-02: where each target's label sits, for labels `labels` of the
/// same targets on `monitor`. Image text is labelled by place_text with the
/// smaller text label box, after every other label is placed by place;
/// `room` gives a target's window, or None for the monitor alone.
pub fn place_all(
    targets: &[Target],
    labels: &[String],
    monitor: &Rect,
    room: impl Fn(&Target) -> Option<Rect>,
) -> Vec<Rect> {
    let mut placed: Vec<Option<Rect>> = targets
        .iter()
        .zip(labels)
        .map(|(target, label)| {
            (!is_image_text(target))
                .then(|| place(&target.bounds, protocol::label_size(label.len()), monitor))
        })
        .collect();
    let mut taken: Vec<Rect> = placed.iter().flatten().copied().collect();
    for (index, (target, label)) in targets.iter().zip(labels).enumerate() {
        if placed[index].is_some() {
            continue;
        }
        let room = room(target)
            .and_then(|room| room.intersect(monitor))
            .unwrap_or(*monitor);
        let chip = place_text(
            &target.bounds,
            protocol::text_label_size(label.len()),
            &room,
            monitor,
            |chip| {
                let overlaps = |rect: &Rect| rect.intersect(chip).is_some();
                !taken.iter().any(overlaps)
                    && !targets
                        .iter()
                        .enumerate()
                        .any(|(other, t)| other != index && overlaps(&t.bounds))
            },
        );
        taken.push(chip);
        placed[index] = Some(chip);
    }
    placed.into_iter().flatten().collect()
}
