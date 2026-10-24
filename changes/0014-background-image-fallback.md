# 0014: Draw hints before image fallback completes

Status: merged
Why: With a visible app that is not registered on the accessibility bus, image detection added 127 ms to collection and a live hint session took 276 ms to draw. The overlay can show AT-SPI and shell targets first while visual targets are collected, then refresh its target generation after detection.

## Added

None.

## Modified

### REQ-SRC-06: Fill accessibility gaps from the screen image
For visible windows, the system shall run the bounded image detector only when AT-SPI has no usable targets, its tree is unavailable, a visible list row lacks a name or usable bounds, a visible list with at least one row exposes fewer than four named rows with usable bounds, or a large visible scrollable or panel-like content region has fewer than four targetable descendants. It shall capture the focused monitor once, map a detection only when its click point belongs to a visible window, assign it to the topmost such window, and discard candidates substantially covered by AT-SPI targets for that window or shell targets. It shall convert detections to logical coordinates and clip them to that window and monitor. Image targets shall offer click only, have no invented text, and use a visibly different label style. If capture or detection fails, or exceeds the image source's 180 ms budget, it shall contribute no image targets and name the failure or timeout in status. In an interactive hints session, primary targets shall draw before a requested image fallback completes; the session shall then update to the image targets in a new generation. While the fallback is pending, hint keys shall not select labels and Escape shall remain available.
Acceptance:
- scenario: a synthetic monitor frame yields correctly addressed image targets with logical bounds and click-only actions; detections substantially covered by AT-SPI targets are discarded.
- scenario: an interactive hints session draws primary targets before a requested image fallback completes, then updates to the image targets in a new generation; keys cannot select labels until that generation is ready, and Escape remains available.
- scenario: complete AT-SPI trees skip image capture; an incomplete visible row, unavailable tree, or AT-SPI error requests the fallback across the visible-window session.
- scenario: a large visible scrollable or panel-like content region with fewer than four targetable descendants requests the image fallback.
- scenario: a detection outside every visible window is discarded; a detection in an overlap is assigned to the topmost window.
- scenario: a QML test checks that an image target's label chip uses the image-source style.
- measured: the supplied Flea screenshot's visible file rows receive image labels within the 180 ms image-source budget, and `thodu bench --sessions 50` remains within REQ-PERF-01 on the GTK3 reference window and Slack.

## Removed

None.

## Tests

- Add REQ-SRC-06 coverage that primary collection requests but does not wait for the image fallback.
- Add REQ-SESSION-01 coverage that the overlay holds selection keys while collecting, accepts Escape and resumes after the target generation changes.
