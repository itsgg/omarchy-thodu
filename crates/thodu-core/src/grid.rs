//! The refining grid that reaches every point (REQ-GRID-01).

use crate::model::{ActionKind, Rect, Role, Source, Target};

/// At most this many columns and rows.
pub const MAX_CELLS: i32 = 26;
/// A cell this size or smaller in both directions is chosen, not refined.
pub const FINE: i32 = 10;

/// Whether `area` is small enough to click instead of refining.
pub const fn is_fine(area: &Rect) -> bool {
    area.w <= FINE && area.h <= FINE
}

/// Split `area` into at most 26 by 26 cells, in reading order, covering it
/// exactly: as many columns as it takes to make each at most FINE wide,
/// capped at 26, and the same for rows.
pub fn cells(area: &Rect) -> Vec<Rect> {
    let cols = ((area.w + FINE - 1) / FINE).clamp(1, MAX_CELLS);
    let rows = ((area.h + FINE - 1) / FINE).clamp(1, MAX_CELLS);
    let edge = |start: i32, len: i32, parts: i32, i: i32| start + len * i / parts;
    let mut out = Vec::with_capacity((cols * rows) as usize);
    for r in 0..rows {
        let (y0, y1) = (
            edge(area.y, area.h, rows, r),
            edge(area.y, area.h, rows, r + 1),
        );
        for c in 0..cols {
            let (x0, x1) = (
                edge(area.x, area.w, cols, c),
                edge(area.x, area.w, cols, c + 1),
            );
            out.push(Rect::new(x0, y0, x1 - x0, y1 - y0));
        }
    }
    out
}

/// The cells of `area` as targets of the grid source.
pub fn targets(area: &Rect) -> Vec<Target> {
    cells(area)
        .into_iter()
        .enumerate()
        .map(|(i, bounds)| Target {
            id: format!("g{i}"),
            bounds,
            role: Role::Unknown,
            text: String::new(),
            source: Source::Grid,
            actions: vec![ActionKind::Click, ActionKind::Right, ActionKind::Double],
            window: String::new(),
            check: None,
        })
        .collect()
}
