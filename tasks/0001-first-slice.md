# 0001: Click a button in a GTK3 window from the keyboard

Status: open
Depends on: none
Spec: REQ-SESSION-01, REQ-SESSION-02, REQ-SESSION-03, REQ-SRC-01, REQ-SRC-04, REQ-LABEL-01, REQ-LABEL-02, REQ-SEARCH-01, REQ-ACT-01, REQ-ACT-03
Goal: the first slice GG can use. `thodu hints`, bound to a key, shows labels on the focused GTK3 window's buttons; a search or Shift and a label focuses one, and Return clicks it. This sets up the Rust workspace of SPEC section 4, the overlay plugin, the IPC of section 7 (start, session, key, close), the atspi source for GTK3 with matching by process and the visibility filter, labels, search, and the left click through the virtual pointer with the stale check. Small hand-written trees in spec/cases/atspi/ stand in for recorded ones until task 0005.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-SESSION-01 REQ-SESSION-02 REQ-SESSION-03 REQ-SRC-01 REQ-SRC-04 REQ-LABEL-01 REQ-LABEL-02 REQ-SEARCH-01 REQ-ACT-01 REQ-ACT-03`
- On GG's session: over the GTK3 reference window (spec/reference/gtk3.py), labels appear on its buttons and Return clicks the chosen one, seen on his screen before the task is done.

## Log

- 2026-10-21: created.
