# 0011: Add bounded image targets

Status: merged
Why: The Flea file manager exposes visible rows with empty accessibility names and zero bounds, so Thodu labels only one row. OCR with the installed Tesseract took about 2.7 to 3.0 seconds on the supplied 1920x1200 screenshot, beyond the existing session latency budget. Use the fast classical image detector already measured in Woints, and mark its lower-confidence targets as image-derived. GG approved this targeted fallback before the full screen-image coverage milestone.

## Added

### REQ-SRC-06: Fill accessibility gaps from the screen image
For a focused window, the system shall run the bounded image detector only when AT-SPI has no usable targets, its tree is unavailable, a visible list row lacks a name or usable bounds, or a visible list with at least one row exposes fewer than four named rows with usable bounds. It shall add image targets for detected text lines and box-like controls not substantially covered by AT-SPI targets. It shall capture the window region in compositor layout coordinates at the monitor scale, convert detections to logical coordinates, clip them to the visible window and monitor, mark their source as `image`, and report their count and elapsed time. Image targets shall offer click only, have no invented text, and use a visibly different label style. If capture or detection fails, or exceeds the image source's 180 ms budget, it shall contribute no image targets and name the failure or timeout in status.
Acceptance:
- scenario: a synthetic frame containing two text lines yields two in-window image targets with logical bounds and click actions.
- scenario: an image detection substantially covered by an AT-SPI target is discarded while an uncovered detection remains.
- scenario: a QML test checks that an image target's label chip uses the image-source style.
- measured: `thodu bench --sessions 50` remains within REQ-PERF-01 on the GTK3 reference window and Slack.

## Modified

### REQ-SRC-05: Budgets per source
While collecting, the system shall give each source a time budget (80 ms for atspi, 180 ms for image, and 40 ms for shell, provisional) and leave a source that misses it out of the session, naming it in the status line. The image source shall run only after a primary source requests fallback.
Acceptance:
- scenario: WHEN a fake source sleeps past its budget THEN the session shows the other sources' targets and the status line names the slow source, including for image.

## Removed

## Tests

- Add REQ-SRC-06 tests for synthetic image detections, coordinates, overlap filtering, and distinct QML label styling.
- Extend REQ-SRC-05 budget coverage to the image source and verify that complete primary trees skip it.
- Measure REQ-PERF-01 on the existing reference windows.
