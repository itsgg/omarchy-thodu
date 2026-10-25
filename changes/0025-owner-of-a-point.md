# 0025: The owner of a point follows Hyprland's whole stack

Status: merged
Why: Task 0029. Ownership of a click point considered only client windows, so a notification or other layer over a target did not stop Thodu labelling and clicking it, a window target under the bar strip was kept, bar targets stayed labelled under a fullscreen window that hides the bar, and a dialog drawn over a fullscreen window lost to it. Hyprland 0.56.2's source settles the order: floating windows allowed over fullscreen are drawn after the fullscreen window, entering fullscreen clears that permission and blocks input on every other window of the workspace (`setNoMembersAboveFullscreen`), and "if in fullscreen, only overlay can be above" for layers (LayerSurface.cpp). Hyprland reports a layer's surface but not its input region, so Omarchy's masked full-screen surfaces are described from their QML: the OSD's mask is empty and notifications mask to the toast column.

## Added

### Section 3, Owner of a point
What receives a click there, in Hyprland's order: overlay layers; with a fullscreen window, the pinned, special-workspace and allowed-over-fullscreen windows and then the fullscreen window, which covers top layers and the windows behind it; otherwise top layers such as the bar, then visible windows front to back. A window that accepts no input owns nothing. A layer takes input over its whole surface except where Omarchy's own surfaces say otherwise: Thodu's overlay and the OSD take none, and notifications take only their toast column at the monitor's right edge.

## Modified

### Section 3, Visible window
Adds: and not hidden behind a fullscreen window.

### REQ-SRC-07: Targets from every visible window
A target is omitted when its click point is owned by anything other than its window, such as another window, the bar, a notification or a fullscreen window, and activation verifies that its window still owns the point. New scenarios cover layers, the OSD, windows without input, fullscreen windows and windows allowed over them.

### REQ-SRC-06 and REQ-SRC-08
An image detection maps only when a visible window owns its click point, to that window; a scroll area is omitted when its center is owned by anything other than its window.

### REQ-SHELL-01: The bar is a source
A bar target is kept only where the bar owns its click point, so a fullscreen window or an overlay layer over the bar removes it, and activation rechecks that ownership.

## Tests

- crates/thodu-core/tests/req.rs: req_src_07_layers_own_the_points_where_they_take_input, req_src_07_fullscreen_window_covers_bar_and_windows_behind_it and req_src_07_window_without_input_owns_nothing are added.
- crates/thodud/src/sources.rs: req_src_07_targets_under_layers_and_fullscreen_are_omitted is added; req_src_07_targets_follow_floating_over_tiled_stacking calls the new filter.
- crates/thodud/src/hypr.rs: req_src_07_layers_are_read_per_monitor_and_own_their_points and req_src_07_windows_behind_a_fullscreen_window_are_not_visible are added; req_src_07_floating_window_owns_an_overlap_above_the_active_tiled_window asks the new owner with a monitor that has geometry.
- crates/thodud/src/image.rs: req_src_07_monitor_detections_under_layers_are_dropped is added.
- crates/thodud/src/main.rs: req_src_07_shell_staleness_triggers_activation_refresh checks that a covered bar target refreshes; req_session_06_req_shell_01_hints_work_without_open_apps gives its monitor the bar's layer.
