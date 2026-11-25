# 0030: Match frames by size, and recheck windows and bar items more closely

Status: proposed
Why: Task 0034, review findings 7, 8, 9 and the XWayland and rotation parts of 17. The focused window took any ACTIVE frame of its process with no size check, and fell back to the process's only frame even when its size was known to differ, so a dialog the process does not expose could get its main window's tree. A background window needed an exact size match, but live on GG's machine a Chromium window on the special workspace reported a frame 26 units taller than Hyprland's window, the height of the bar. The stale check compared a window's address, position, size and workspace but not its process or class, and Hyprland reuses addresses. Bar items were scanned once a second, so a click could be checked against items up to a second old. XWayland windows report element extents in physical pixels under Omarchy's zero scaling, and Hyprland reports a rotated monitor's mode width and height unswapped.

## Modified

### REQ-SRC-01: Match by process, never by name
When collecting from the accessibility tree, the system shall inspect the window's process and consider only that process's shown frames whose width and height, in logical units, are each within 48 units of the window's. For the focused window it shall match the ACTIVE frame among them when there is exactly one. Otherwise it shall match the frame nearest the window's size, by the sum of the width and height differences, and no frame when two are equally near. For a non-focused window, including an explicitly selected window that is not focused, it shall ignore ACTIVE frames. A frame outside that size never matches, even when it is ACTIVE or the process's only frame. A collection shall assign an AT-SPI frame to at most one Hyprland window; if more than one window claims the same frame, it shall omit those windows. It shall never match by application name.
Acceptance:
- cases: `spec/cases/atspi/match.jsonl`, pairs of a Hyprland window and a registry snapshot with the expected frame, including two frames from one Chromium process and an app whose name differs from its class.
- scenario: a non-focused window cannot use an ACTIVE sibling frame, even when its extents happen to match; a frame claimed by multiple windows is omitted for all claimants.
- scenario: a frame 26 units taller than its window matches; ACTIVE chooses among several near frames for the focused window; a frame of another size never matches, even when ACTIVE or alone; of two near frames the nearer matches, and two equally near match neither.
- replay: the recorded trees in spec/cases/atspi still match their frames.

### REQ-SRC-02: The toolkits of this milestone
The system shall produce targets from GTK3, GTK4, Qt Widgets and Electron windows and from Chromium's own controls, converting window-relative extents (GTK4) and screen extents alike to logical coordinates. An XWayland window reports its extents in physical pixels under Omarchy's zero scaling; they shall be divided by its monitor's scale, both for its targets and for matching its frame by size (REQ-SRC-01).
Acceptance:
- replay: recorded trees of the reference windows in spec/reference/ (gtk3.py, gtk4.py, qt.py), an Electron app (Obsidian) and Chromium, each with the targets a person marked, at least 90 percent found (provisional) and none outside its window.
- scenario: an XWayland window's element extents divide by a scale of 1.25, and its frame of the physical size matches the window.

### REQ-ACT-03: Never act on a stale target
If the target's window has moved, resized, closed, left the active workspace, or changed its process or class at the same address since collection, then the system shall not act; it shall collect again and redraw the labels. An accessible target's element shall be read again just before the click, and if it is gone, no longer showing, or its window-relative extents moved by more than one unit, the system shall not act and shall collect again. An image target's region shall be captured again just before the click, over the same monitor area and at the same scale as the capture it was detected in, and if its pixels differ markedly from those at detection, the system shall not act and shall collect again; a capture that fails or takes longer than half a second counts as changed. Each click shall run its checks immediately before its press, with clicks and scroll actions taking turns with the one pointer, and the second click of a double click shall be checked the same way and skipped when the first click did not happen.
Acceptance:
- scenario: WHEN Hyprland reports the window 10 units to the right after collection THEN no pointer event is sent and a new collection runs.
- scenario: a window whose process or class changed at the same address, position and size is stale.
- scenario: an accessible target whose element moved, hid or no longer answers is refused while its window and click point are unchanged, and an element that cannot be read in time counts as moved.
- scenario: a click prepares, then checks and presses while holding the pointer, two clicks never hold it together, and a second click is skipped when the first never happened or its target moved.
- scenario: an image target carries a fingerprint of its region; the same pixels and a blinking cursor pass, other text in the region and a failed capture refuse the click.

### REQ-SHELL-01: The bar is a source
When a session starts, the overlay shall report the bar's clickable items on the focused window's monitor, or on Hyprland's focused monitor when no app window is focused, with their bounds in logical coordinates and a stable, opaque identity, as `shell` targets. The overlay shall scan the bar again after each Hyprland workspace, monitor focus, special workspace, window open, close, move, fullscreen and focus event, at once and again 100 ms after the last such event, as well as once a second. Each identity shall be unique among the reported items and stable for the lifetime of its QML object, independent of its text and bounds; a replacement object shall receive a different identity. The daemon shall accept earlier v1 reports without identity and invalidate a legacy target before activation if its item index or text changes. A bar target is kept only where the bar owns its click point, so a fullscreen window or an overlay layer over the bar removes it, and activation rechecks that ownership.
Acceptance:
- scenario: a QML test with a fake bar of three items reports three targets with their bounds.
- scenario: the bar rescans on workspace and window events, at once and after settling, a burst of events coalesces into those two scans, and an event that moves no bar item asks for none.
- scenario: QML tests verify an item's identity stays stable when its text changes and across repeated scans, and a replacement object receives a different identity.
- scenario: with no active app window, the source collects cached bar items by monitor even when the app window list is empty.
- scenario: an earlier v1 bar report without identity is accepted; a legacy target remains valid while its item index, text, bounds, and bar origin match, and refreshes before activation if any changes.
- scenario: bar targets are omitted under a fullscreen window, and a bar target whose point is covered when activated refreshes instead of clicking.

### REQ-GRID-01: Every point is reachable
When `thodu grid` runs, or Space is pressed in a session with no focused target, the system shall cover the monitor with a labelled grid of at most 26 by 26 cells; choosing a cell refines it the same way until a cell is at most 10 logical units. The monitor's area is as laid out: a monitor rotated a quarter or three quarters (Hyprland transforms 1, 3, 5 and 7) has Hyprland's mode width and height swapped. A label that chooses such a fine cell clicks its center at once when its final character is lowercase, as REQ-ACT-01 describes, and selects it when uppercase, for Return, Shift-Return, Control-Return or a double Return (REQ-ACT-02). Return on a focused cell that is not yet fine refines into it instead of clicking.
Acceptance:
- property: for any monitor size and any point on it, some sequence of at most four choices ends on a cell containing the point.
- scenario: a lowercase label on a coarse cell refines, one on a fine cell clicks its center with no double-Return wait, an uppercase final character selects a fine cell for Return, and Return on a coarse cell refines into it.
- scenario: a monitor with transform 1 reports its width and height swapped, and one with transform 2 does not.

## Tests

- crates/thodu-core/tests/req.rs: req_src_01_frames_match_by_size_within_slack, req_act_03_a_reused_address_with_another_process_is_stale, req_src_02_xwayland_extents_are_physical and req_src_02_an_xwayland_frame_matches_by_its_logical_size are added. req_src_07_same_process_windows_use_their_own_frame_or_skip_ambiguity changes: the focused 300 by 200 window takes its own-size frame, not an 800 by 600 ACTIVE one. req_src_01_nonfocused_named_window_does_not_use_active_sibling_frame changes: a frame one unit off the window's size is within the slack and matches; the ACTIVE sibling is still never taken.
- crates/thodud/src/hypr.rs: req_grid_01_a_rotated_monitor_swaps_width_and_height is added.
- tests/qml/tst_barscan.qml: test_req_shell_01_rescans_on_workspace_and_window_events is added.
- tests/qml/tst_barrescan.qml: test_req_shell_01_a_workspace_event_rescans_now_and_after_settling, test_req_shell_01_a_burst_of_window_events_coalesces and test_req_shell_01_an_event_that_moves_no_bar_item_asks_for_no_scan are added.
