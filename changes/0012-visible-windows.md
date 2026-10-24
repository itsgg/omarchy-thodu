# 0012: Collect targets from visible windows

Status: merged
Why: Task 0013 follows the user report that controls in other visible applications receive no labels. The current default collects only the focused window, and its image fallback explicitly skips every non-focused window. Hyprland reports mapped windows from inactive workspaces too, so the default scope must be derived from each monitor's active and shown special workspaces rather than the mapped flag alone.

## Added

### REQ-SRC-07: Targets from every visible window
In default hints mode, the system shall collect targets from each mapped, non-hidden window on the focused monitor's active workspace, plus pinned windows and windows on a shown special workspace on that monitor. It shall exclude windows on inactive workspaces and other monitors. One session shall assign labels across those windows and the bar. Every target shall retain its own Hyprland window address for stale checks and activation. Where windows overlap, it shall omit a target whose click point belongs to a different topmost visible window. Before activation in the default visible scope, it shall verify that the target's window is still topmost at that point and refresh if ownership changed. If multiple same-process frames cannot be matched to a non-focused window unambiguously, the system shall omit that window's AT-SPI targets rather than use another window's frame.
Acceptance:
- scenario: Hyprland client fixtures include active-workspace, pinned, shown-special, inactive-workspace, hidden, unmapped, and other-monitor windows; only the first three eligible groups enter the session.
- scenario: two visible windows from different processes both produce address-scoped targets in one session; acting on either target checks and uses its own window address.
- scenario: two same-process frames with distinct sizes match their corresponding windows; equal-size ambiguous background frames are omitted, while the focused window may use its ACTIVE frame.
- scenario: a monitor screenshot produces an image target only for the topmost visible window at its click point; accessible coverage suppresses duplicate image targets.
- scenario: a floating window owns overlap with a tiled window even when the tiled window has a more recent focus history; if another window moves over a target before activation, the session refreshes instead of clicking the occluder.
- live: default hints on a monitor with two visible apps reports targets from both window addresses; `--window` remains a single-window collection.

## Modified

### REQ-SESSION-01: Labels on visible windows
When `thodu hints` runs and a window is focused, the system shall show a label for each target of every window in REQ-SRC-07's visible scope and of the Omarchy bar on the same monitor. A `scope` of `focused` limits a request to the active window, and an explicit `window` selects one window. While focus comes from text search or keyboard navigation, the overlay shall outline the target bounds. After a label focuses a target, it shall hide the target outline and emphasize the label chip; while a label prefix is incomplete, it shall not leave the previous target outlined as if it were still focused. The status toast shall wrap within the monitor width and a maximum width of 640 logical units. Its cue shall describe the current search-first or labels-only behavior accurately. The session view carries additive `label_selected` and `labels_only` fields for this presentation.
Acceptance:
- replay: each recorded tree in `spec/cases/atspi/*.json` through the fake source yields the targets listed beside it in `spec/cases/atspi/*.targets.jsonl`.
- measured: REQ-PERF-01.
- scenario: a QML test checks label focus, search focus, a mode-accurate cue and a wrapped, bounded status toast.
- scenario: a request without `scope` uses all windows in REQ-SRC-07's visible scope; `scope: focused` and an explicit `window` each restrict the collection to one window.

### REQ-SRC-01: Match by process, never by name
When collecting from the accessibility tree, the system shall inspect the focused window's process and match its ACTIVE frame for that focused window. For other windows, it shall match a unique shown frame whose extents match that window's size; it shall never match by application name and shall omit a same-process window when its frame is ambiguous.
Acceptance:
- cases: `spec/cases/atspi/match.jsonl`, pairs of Hyprland windows and registry snapshots with the expected frame, including two frames from one Chromium process and an app whose name differs from its class.
- scenario: a non-focused window never receives the focused sibling's frame when extents do not identify a unique match.

### REQ-CLI-01: Targets for agents
When `thodu targets --json` runs, the system shall print the targets of every window in REQ-SRC-07's visible scope as one JSON object per line, in the shape of section 5, without showing labels. A `scope` of `focused` or an explicit `window` limits the output to one window.
Acceptance:
- cases: the replayed trees of REQ-SRC-02 printed through the command match their `.targets.jsonl` files.
- scenario: two visible app windows produce targets with distinct `window` addresses; focused scope and an explicit window each return only their selected window's targets.

### REQ-SRC-06: Fill accessibility gaps from the screen image
For visible windows, the system shall run the bounded image detector only when AT-SPI has no usable targets, its tree is unavailable, a visible list row lacks a name or usable bounds, a visible list with at least one row exposes fewer than four named rows with usable bounds, or a large visible scrollable or panel-like content region has fewer than four targetable descendants. It shall capture the focused monitor once, map a detection only when its click point belongs to a visible window, assign it to the topmost such window, and discard candidates substantially covered by AT-SPI targets for that window or shell targets. It shall convert detections to logical coordinates and clip them to that window and monitor. Image targets shall offer click only, have no invented text, and use a visibly different label style. If capture or detection fails, or exceeds the image source's 180 ms budget, it shall contribute no image targets and name the failure or timeout in status.
Acceptance:
- scenario: a synthetic monitor frame yields correctly addressed image targets with logical bounds; detections substantially covered by AT-SPI targets are discarded.
- scenario: complete AT-SPI trees skip image capture; an incomplete visible row, unavailable tree, or AT-SPI error requests the fallback across the visible-window session.
- scenario: a large scrollable or panel-like content region with fewer than four targetable descendants requests the image fallback.
- scenario: a detection whose click point is outside all visible windows is discarded; a detection in an overlap is assigned to the topmost visible window.
- scenario: a QML test checks that an image target's label chip uses the image-source style.
- measured: the supplied Flea screenshot's visible file rows receive image labels within the 180 ms image-source budget, and `thodu bench --sessions 50` remains within REQ-PERF-01 on the GTK3 reference window and Slack.

## Removed

None.

## Tests

- Add REQ-SRC-07 tests for Hyprland visibility filtering, per-window frame matching, multi-window target addressing, and topmost image-target ownership.
- Extend REQ-SRC-01 matching cases and REQ-SRC-06 image-fallback coverage.
- The coordinator updated REQ-SESSION-01 and REQ-CLI-01 scope acceptance and the section 7 start and targets defaults when merging the proposal.
