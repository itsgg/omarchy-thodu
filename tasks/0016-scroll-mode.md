# 0016: Scroll mode

Status: claimed
Depends on: none
Spec: REQ-SRC-08, REQ-SESSION-07, REQ-ACT-04, REQ-INSTALL-01
Goal: Add accessible scroll-area discovery, a keyboard mode to select and scroll any visible app pane, a browser-compatible wheel path, and an Omarchy shortcut with an F12 default and a per-install override.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-SRC-08 REQ-SESSION-07 REQ-ACT-04 REQ-INSTALL-01`
- run: `tools/gate`
- scenario: On the running desktop, scroll one native pane and one Chromium or Electron document, select a second pane, and exit with Escape.

## Log

- 2026-10-24: created from merged change 0017; implementation is in progress.
