# 0016: Scroll mode

Status: review
Depends on: none
Spec: REQ-SRC-08, REQ-SESSION-07, REQ-ACT-04, REQ-INSTALL-01
Goal: Add accessible scroll-area discovery, a keyboard mode to select and scroll any visible app pane, a browser-compatible wheel path, and an Omarchy shortcut with an F12 default and a per-install override.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-SRC-08 REQ-SESSION-07 REQ-ACT-04 REQ-INSTALL-01`
- scenario: On the running desktop, scroll one native pane and one Chromium or Electron document, select a second pane, and exit with Escape.

## Log

- 2026-10-24: added AT-SPI scroll areas with window fallbacks, numeric pane selection, HJKL uinput wheel actions, same-session overlay resume, the F12-family binding and Chromium/Electron setup guidance. REQ-SRC-08, REQ-SESSION-07, REQ-ACT-04 and REQ-INSTALL-01 acceptance passed; full gate passed in 144.8 seconds.
- 2026-10-24: removed the redundant `tools/gate` run command from Acceptance because the full gate replays task run commands; this avoids recursive gate execution.
- 2026-10-24: corrected pending-key handling in scroll mode and added coverage for web documents without scroll-pane roles; focused scroll tests pass.
