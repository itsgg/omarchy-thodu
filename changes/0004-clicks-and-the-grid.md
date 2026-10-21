# 0004: Where the pointer goes back to, the wait for a second Return, and the grid's interface

Status: merged
Why: task 0004 found two things the spec left out. GG's Hyprland has `input:follow_mouse = 1` (read 2026-10-21), so returning the pointer over another window after a click moves the keyboard focus there, and typing after clicking an entry lands in the wrong window; refocusing the clicked window instead warps the pointer to it, undoing the return. And a double Return needs the overlay to keep the keyboard after the first click, which REQ-ACT-02 did not say. The grid also needed its targets to change within a session.

## Modified

### REQ-ACT-01: Return clicks
When Return is pressed with a focused target, the system shall move the pointer to the target's center, click the left button there through the virtual pointer, end the session and return the pointer to where it was, unless Hyprland's focus follows the mouse and that point is not over the clicked window, where returning it would take the keyboard from that window; then the pointer stays on the target. A session started with `test` always returns it.
Acceptance:
- scenario: WHEN a session on a recorded tree focuses a button at bounds (100, 200, 80, 30) on a monitor at scale 1.25 THEN the fake pointer receives a move to (140, 215) logical and one left click.
- scenario: WHEN focus follows the mouse and the pointer was over another window, or over none while the click was on a window, THEN it is not returned; over the same window, over none after a click on none, or with focus on click, it is.

### REQ-ACT-02: The other clicks
When Shift-Return is pressed the system shall right-click, when Return is pressed twice within 300 ms it shall double-click, and when Control-Return is pressed it shall click with Control held. After a plain Return on a target that is not an entry, the overlay keeps the keyboard for 300 ms, drawing nothing and letting the pointer through, so a second Return can come; any other key ends the session. After an entry, a right click or a Control click it gives the keyboard back at once.
Acceptance:
- cases: `spec/cases/act/keys.jsonl`, key sequences with the expected pointer and keyboard events and whether the overlay keeps the keyboard.

Section 7, Interfaces: `start` with mode grid needs no focused window and covers the focused monitor; `thodu grid [--window W] [--test] [--no-overlay]` starts one. The view carries `generation`, bumped when the targets change, and `linger`, true while the overlay keeps the keyboard after a click; a `key` reply carries the new targets when the key changed them. Below the monitor's level the overlay draws the grid's cells again, magnified, beside the outlined area.

## Tests

- crates/thodu-core/tests/req.rs: req_act_01_pointer_goes_back_unless_it_would_take_the_focus, req_act_02_key_cases, req_grid_01_every_point_is_reachable, req_grid_01_cells_tile_and_backspace_goes_back; req_act_01_return_clicks_the_center now expects a second Return after 300 ms to do nothing, instead of any second Return.
- tests/qml/tst_session.qml: the wait after a click, a key ending it, a click without it, the grid's cells replacing the targets, and small cells magnified.
