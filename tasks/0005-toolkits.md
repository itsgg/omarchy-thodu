# 0005: GTK4, Qt, Electron and Chromium, from recorded trees

Status: open
Depends on: 0001
Spec: REQ-SRC-02, REQ-SRC-03, REQ-SRC-05, REQ-CLI-01
Goal: `thodu record-tree --out FILE` records the focused window's accessibility tree; the coordinator records the reference windows in spec/reference/, Obsidian (Electron) and Chromium on GG's session into spec/cases/atspi/ and marks their targets; then the atspi source covers them all, converts GTK4's window-relative extents, wakes dormant Chromium and Electron trees, holds each source to its budget, and `thodu targets --json` prints what it found.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-SRC-02 REQ-SRC-03 REQ-SRC-05 REQ-CLI-01`
- On GG's session: labels on Slack's app window controls and on Nautilus.

## Log

- 2026-10-21: created.
