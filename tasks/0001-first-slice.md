# 0001: Click a button in a GTK3 window from the keyboard

Status: review
Depends on: none
Spec: REQ-SESSION-01, REQ-SESSION-02, REQ-SESSION-03, REQ-SRC-01, REQ-SRC-04, REQ-LABEL-01, REQ-LABEL-02, REQ-SEARCH-01, REQ-ACT-01, REQ-ACT-03
Goal: the first slice GG can use. `thodu hints`, bound to a key, shows labels on the focused GTK3 window's buttons; a search or Shift and a label focuses one, and Return clicks it. This sets up the Rust workspace of SPEC section 4, the overlay plugin, the IPC of section 7 (start, session, key, close), the atspi source for GTK3 with matching by process and the visibility filter, labels, search, and the left click through the virtual pointer with the stale check. Small hand-written trees in spec/cases/atspi/ stand in for recorded ones until task 0005.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-SESSION-01 REQ-SESSION-02 REQ-SESSION-03 REQ-SRC-01 REQ-SRC-04 REQ-LABEL-01 REQ-LABEL-02 REQ-SEARCH-01 REQ-ACT-01 REQ-ACT-03`
- On GG's session: over the GTK3 reference window (spec/reference/gtk3.py), labels appear on its buttons and Return clicks the chosen one, seen on his screen before the task is done.

## Log

- 2026-10-21: created.
- 2026-10-21: built the Rust workspace (thodu-core, thodud, thodu), the overlay plugin (Thodu.qml, components/SessionView.qml, components/Client.qml) and the IPC of section 7 with change 0002. The atspi source reads the GTK3 reference window recorded in spec/cases/atspi/gtk3-reference.json. Measured live on a headless output (THODUTEST): 7 targets in 27 to 85 ms; Shift+S then Return typed into the overlay clicked Save, and the cursor went back to where it was. Return to click took 132 to 252 ms with keys sent over the IPC, most of it the overlay's 200 ms heartbeat. Escape removed the overlay 37 to 67 ms after the key. A killed thodud released the keyboard in 20 ms, and a stopped one in 708 ms. Keys typed in the few milliseconds before the overlay has the keyboard go to the window beneath it, for task 0002. Waiting on GG: the same check on his own screen.

