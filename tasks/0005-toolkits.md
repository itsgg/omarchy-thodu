# 0005: GTK4, Qt, Electron and Chromium, from recorded trees

Status: review
Depends on: 0001
Spec: REQ-SRC-02, REQ-SRC-03, REQ-SRC-05, REQ-CLI-01
Goal: `thodu record-tree --out FILE` records the focused window's accessibility tree; the coordinator records the reference windows in spec/reference/, Obsidian (Electron) and Chromium on GG's session into spec/cases/atspi/ and marks their targets; then the atspi source covers them all, converts GTK4's window-relative extents, wakes dormant Chromium and Electron trees, holds each source to its budget, and `thodu targets --json` prints what it found.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-SRC-02 REQ-SRC-03 REQ-SRC-05 REQ-CLI-01`
- On GG's session: labels on Slack's app window controls and on Nautilus.

## Log

- 2026-10-21: created.
- 2026-10-21: `thodu record-tree` records a window's process's tree through thodud. The reference windows gtk4.py, qt.py and page.html (in Chromium as an app window) and Obsidian's vault chooser, each on a fresh profile under ~/.local/share/thodu/reference, were recorded on a headless output into spec/cases/atspi/. Each recording has its exact targets in .targets.jsonl and the controls a person marked in .marked.jsonl: each reference window's widgets, and Obsidian's nine visible controls from a screenshot, at the found bounds' centers, except Qt's Docs link, placed by hand. Found: GTK3 7/7, GTK4 7/7, Qt 6/7, Chromium 7/7, Obsidian 9/9; Qt's miss is a QLabel link that Qt reports as a plain label. The threshold is read over all the windows; change 0006 asks GG to confirm that reading. The atspi source now reads each tree level with its calls in flight at once and walks only the matched frame: 4 to 19 ms per window, inside the 80 ms budget it is now held to, and a source past its budget is left out and named. A dormant Chromium or Electron tree is woken with one GetAttributes call per process and filled within 50 ms. GTK4's role names (text box, checkbox) are mapped, Chromium's document is read in physical pixels, an element whose first action is "click" is a target (Obsidian's window buttons), and duplicates with equal bounds are dropped (GTK4's combo box). Live clicks landed in the GTK4, Qt and Chromium windows. Waiting on GG: labels on Slack's app window and on Nautilus on his session, and change 0006.

