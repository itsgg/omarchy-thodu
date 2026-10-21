# 0004: Right, double and modified clicks, and the grid

Status: review
Depends on: 0001
Spec: REQ-ACT-02, REQ-GRID-01
Goal: Shift-Return, a double Return and Control-Return act as REQ-ACT-02 says, and `thodu grid` or Space opens the refining grid of REQ-GRID-01, so every point on the monitor can be clicked.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-ACT-02 REQ-GRID-01`
- On GG's session: the grid reaches an icon in Zed, which has no accessibility tree.

## Log

- 2026-10-21: created.
- 2026-10-21: Shift-Return right-clicks, a second plain Return within 300 ms double-clicks, Control-Return holds Control through a uinput keyboard around the click, and the core plans each action as inputs the conformance cases check. After a plain Return on anything but an entry the overlay keeps the keyboard for 300 ms with nothing drawn and the pointer passing through; thodud clicks at once and puts the pointer back when the wait ends. `thodu grid` and Space with nothing focused open the 26 by 26 grid; below the monitor's level the cells are drawn again, magnified, beside the outlined area. With focus following the mouse on GG's Hyprland, the pointer goes back only where it keeps the clicked window's focus (change 0004). Live on the GTK3 reference window: one press for Return, GTK's double-press for Return Return, one press for a second Return after 450 ms or a letter in the wait, button 3 for Shift-Return, and two grid choices reached Open's center and clicked it. Not checked live: Control-Return, since /dev/uinput is root's until task 0007's uaccess rule; and the grid over Zed, which needs GG's Zed window on his screen.

