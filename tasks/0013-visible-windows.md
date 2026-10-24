# 0013: Hint every visible window

Status: review
Depends on: 0011, 0012
Spec: REQ-SRC-01, REQ-SRC-06, REQ-SRC-07, REQ-SHELL-01, REQ-SESSION-04, REQ-SESSION-06
Goal: Collect AT-SPI and bounded image targets across every window visible on the focused monitor's active workspace, so a Thodu session can reach controls in each visible application and does not miss visual controls merely because they belong to a non-focused window.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-SRC-01 REQ-SRC-06 REQ-SRC-07 REQ-SHELL-01 REQ-SESSION-01 REQ-SESSION-04 REQ-SESSION-06 REQ-CLI-01`
- A Hyprland client fixture includes mapped windows on the active workspace, pinned windows, and windows on a shown special workspace, while excluding inactive workspaces, hidden or unmapped clients, and other monitors.
- Frames from each visible process are matched to their own window; non-focused windows ignore ACTIVE siblings, and an ambiguous or multiply claimed frame is omitted.
- Synthetic monitor-image detections become targets for the topmost visible window at each click point and are not duplicated by accessible targets.
- A populated AT-SPI tree still receives the image supplement; capture follows the overlay's first drawn frame, and detections inside primary targets or the reported label and status rectangles are omitted.
- A floating window owns an overlap with a tiled window, even when the tiled window has a more recent focus history; moving a window over a label before activation refreshes the session instead of clicking the new occluder.
- In focused and explicit-window scopes, image detections use the full visible window stack for ownership and are kept only when the selected window owns the click point.
- Empty and populated AT-SPI results request image supplementation; synthetic image detections remain scoped to their visible window.
- On a live desktop with two visible app windows, default hints contain targets for both window addresses; an explicit `--window` remains scoped to that window.
- When image supplementation is requested for hints, primary targets draw first; stale-generation keys are ignored, the overlay accepts Escape while holding label keys, then updates to the new generation.
- When no app window is focused, hints use Hyprland's focused monitor to label bar items; if no targets exist, the grid opens.

## Log

- 2026-10-23: created after reviewing the missing targets in a second file manager and confirming default collection was limited to the focused window. The acceptance keeps inactive workspaces out and checks per-window actions.
- 2026-10-23: claimed by codex-visible-windows; wrote the acceptance before the implementation.
- 2026-10-24: default collection now covers mapped, non-hidden windows on the focused monitor's active workspace, shown special workspace, and pinned clients. AT-SPI frames are matched by process and per-window extents; image detections are assigned to the frontmost visible window, and overlapping or newly covered targets are suppressed. `thodu targets --json` now includes window addresses and actions.
- 2026-10-24: `tests/acceptance/reqs.sh REQ-SRC-01 REQ-SRC-06 REQ-SRC-07`, `python3 -I tools/checks.py tests-removed`, `git diff --check`, and `cargo build --workspace --release --locked` passed. The release build has no warnings.
- 2026-10-24: live check on eDP-1 at 1920x1200 with visible Foot and Zenity windows: default `thodu targets` and `thodu hints --test` each returned target addresses for both windows; `--scope focused` and explicit `--window` returned only their selected window. The image fallback returned 479 visual candidates in 159 ms. `thodu doctor` reports every install item present.
- 2026-10-24: Reviewing background-window frame matching found that non-focused windows could claim an ACTIVE sibling and that explicitly selected background windows were treated as focused. Matching now uses Hyprland focus state, excludes ACTIVE siblings for background windows, and omits multiply claimed frames; REQ-SRC-01 and REQ-SRC-07 regression tests cover both cases.
- 2026-10-24: moved image-target conversion coverage onto the production mapper, restored the click-only image acceptance, and added multi-window session/JSON tests. A live default benchmark before the fix measured 276 ms to first draw and 129 ms collection when Chromium was not on the accessibility bus; hints now draw primary targets before collecting visual targets and update the session when those arrive.
- 2026-10-24: research and the file-manager screenshot showed that a populated AT-SPI tree can still omit custom-painted controls, so the proposed image behavior now supplements every visible-window collection after primary targets draw. The fallback excludes label chips and the status toast, and the heartbeat test checks that the overlay reloads the new generation without key input.
- 2026-10-24: the overlay now reports its rendered exclusion rectangles with the first-draw event, and image capture waits for that event before reading the screen. Timeout, no-overlay, closed-session and Busy-key paths have regression coverage. `tests/acceptance/reqs.sh REQ-SRC-01 REQ-SRC-06 REQ-SRC-07 REQ-SESSION-01 REQ-CLI-01` passes, including 45 daemon, 45 core and 33 QML tests.
- 2026-10-24: hints now use Hyprland's focused monitor when no app window is active; the shell source collects its cached bar items with an empty window list, and the grid opens if no source has targets. `tests/acceptance/reqs.sh REQ-SRC-01 REQ-SRC-06 REQ-SRC-07 REQ-SHELL-01 REQ-SESSION-01 REQ-SESSION-04 REQ-SESSION-06 REQ-CLI-01` passes, including 49 daemon, 43 core and 33 QML tests.
- 2026-10-24: focused and explicit-window image collection now uses the full visible window stack to assign ownership, then keeps only detections owned by the selected window. The overlap regression and updated acceptance command pass, including 50 daemon, 43 core and 33 QML tests.
