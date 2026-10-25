# 0029: Decide what owns a point from Hyprland's whole stack

Status: open
Depends on: none
Spec: REQ-SRC-07, REQ-SRC-04, REQ-SHELL-01, REQ-SRC-06, REQ-SRC-08
Goal: Ownership of a click point looks only at client windows, so a notification or other layer surface over a target does not stop Thodu from labelling and clicking it, a window target under the bar strip is kept, bar targets stay labelled under a fullscreen window that covers the bar, and a dialog Hyprland draws over a fullscreen window loses to the fullscreen window. Decide ownership from Hyprland's whole stack on the monitor: overlay layers first; with a fullscreen window, pinned, special-workspace and allowed-over-fullscreen windows, then the fullscreen window, which covers top layers and the windows behind it; otherwise top layers such as the bar, then windows front to back. Windows that accept no input own nothing. A layer takes input over its whole surface except where Omarchy documents otherwise: Thodu's own overlay and the OSD take none, and notifications take only the toast column at the right edge. Collection, image mapping, scroll areas and the recheck before a click all use this owner, and windows hidden behind a fullscreen window are not collected.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-SRC-07 REQ-SRC-04 REQ-SHELL-01 REQ-SRC-06 REQ-SRC-08`
- scenario: a window target under a notification's toast column or another top or overlay layer is omitted, while one under the OSD or outside the toast column is kept; a window target under the bar strip is omitted and the bar's own target kept.
- scenario: with a fullscreen window, bar targets and the targets of windows behind it are omitted, a window allowed over fullscreen keeps its targets, and windows behind the fullscreen window are not collected; a window that accepts no input owns no point.
- scenario: image detections and scroll areas follow the same owner, and the recheck before a click refreshes when a layer now covers the point.

## Log

- 2026-10-25: created from confirmed review findings on layer surfaces and fullscreen stacking. Hyprland's clients report `allowedOverFullscreen` and `acceptsInput`; its layers report only their surface, so Omarchy's masked full-screen layers are known by namespace from their QML.
