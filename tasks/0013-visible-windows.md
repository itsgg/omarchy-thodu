# 0013: Hint every visible window

Status: review
Depends on: 0011, 0012
Spec: REQ-SRC-01, REQ-SRC-06, REQ-SRC-07
Goal: Collect AT-SPI and bounded image targets across every window visible on the focused monitor's active workspace, so a Thodu session can reach controls in each visible application and does not miss visual controls merely because they belong to a non-focused window.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-SRC-01 REQ-SRC-06 REQ-SRC-07`
- A Hyprland client fixture includes mapped windows on the active workspace, pinned windows, and windows on a shown special workspace, while excluding inactive workspaces, hidden or unmapped clients, and other monitors.
- Frames from each visible process are matched to their own window; non-focused windows ignore ACTIVE siblings, and an ambiguous or multiply claimed frame is omitted.
- Synthetic monitor-image detections become targets for the topmost visible window at each click point and are not duplicated by accessible targets.
- A floating window owns an overlap with a tiled window, even when the tiled window has a more recent focus history; moving a window over a label before activation refreshes the session instead of clicking the new occluder.
- A large scrollable or panel-like content region with no accessible rows requests the image fallback.
- On a live desktop with two visible app windows, default hints contain targets for both window addresses; an explicit `--window` remains scoped to that window.

## Log

- 2026-10-23: created after reviewing the missing targets in a second file manager and confirming default collection was limited to the focused window. The acceptance keeps inactive workspaces out and checks per-window actions.
- 2026-10-23: claimed by codex-visible-windows; wrote the acceptance before the implementation.
- 2026-10-24: default collection now covers mapped, non-hidden windows on the focused monitor's active workspace, shown special workspace, and pinned clients. AT-SPI frames are matched by process and per-window extents; image detections are assigned to the frontmost visible window, and overlapping or newly covered targets are suppressed. `thodu targets --json` now includes window addresses and actions.
- 2026-10-24: `tests/acceptance/reqs.sh REQ-SRC-01 REQ-SRC-06 REQ-SRC-07`, `python3 -I tools/checks.py tests-removed`, `git diff --check`, and `cargo build --workspace --release --locked` passed. The release build has no warnings.
- 2026-10-24: live check on eDP-1 at 1920x1200 with visible Foot and Zenity windows: default `thodu targets` and `thodu hints --test` each returned target addresses for both windows; `--scope focused` and explicit `--window` returned only their selected window. The image fallback returned 479 visual candidates in 159 ms. `thodu doctor` reports every install item present.
