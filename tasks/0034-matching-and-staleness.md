# 0034: Match frames by size, and recheck windows and bar items more closely

Status: open
Depends on: none
Spec: REQ-SRC-01, REQ-SRC-02, REQ-ACT-03, REQ-SHELL-01, REQ-GRID-01
Goal: A batch of review findings about whose element or window a target belongs to. The focused window takes any ACTIVE frame of its process with no size check, and falls back to the process's only frame even when its size is known to differ, so a dialog the process does not expose can get its main window's tree. Background windows need an exact size match, but live on GG's machine a Chromium window on the special workspace reports a frame 26 units taller than Hyprland's window, the height of the bar. The stale check compares a window's address, position, size and workspace but not its process or class, so a reused address passes. Bar items are scanned once a second, so a click can be checked against items up to a second old. XWayland windows report element extents in physical pixels under Omarchy's zero scaling, and rotated monitors swap Hyprland's width and height. Match a frame when its size is within 48 units of the window's in each dimension, with ACTIVE choosing among matches for the focused window and the sole-frame fallback held to the same check; add the process id and class to the stale check; rescan the bar on Hyprland's workspace and window events as well as every second; convert XWayland extents by the monitor's scale; and swap width and height for rotated monitors.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-SRC-01 REQ-SRC-02 REQ-ACT-03 REQ-SHELL-01 REQ-GRID-01`
- scenario: a frame within 48 units of the window's size matches, ACTIVE chooses among several, a frame of a different size never matches even when ACTIVE or alone, and the recorded trees still match.
- scenario: a window whose process or class changed at the same address is stale.
- scenario: the bar rescans on workspace and window events; an XWayland window's extents are divided by the scale; a rotated monitor reports swapped width and height.

## Log

- 2026-10-25: created to batch review findings 7, 8, 9 and the XWayland and rotation parts of 17, at GG's request to batch the remaining fixes.
- 2026-10-27: partial work is commit `10897bd` on the branch `coord/0034-matching-and-staleness`; cherry-pick it onto your branch. It makes all five changes and adds their tests, and `cargo test`, clippy and the QML suite pass with it. It changes two existing tests to the size rule, declared in its commit body. Left to do: confirm `tools/gate` replays the recorded trees in spec/cases/atspi with the new rule; plant a fault in each change and see a test catch it; propose the spec text as change 0029 (REQ-SRC-01 size slack, REQ-ACT-03 process and class, REQ-SHELL-01 rescans on Hyprland events, REQ-SRC-02 XWayland scale, REQ-GRID-01 rotated monitors); then review.
