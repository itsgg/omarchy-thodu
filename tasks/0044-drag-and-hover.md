# 0044: Drag from one target to another, and hover

Status: open
Depends on: 0034, 0035, 0036, 0037
Spec: REQ-ACT-02, REQ-ACT-03
Goal: SEED section 5 lists hover and drag from one target to another among Thodu's actions, and SEED section 9 orders drag and hover after web page targets and the screen image. Neither exists. Add hover (move the pointer to a target and leave it there) and drag (press on one target, move to a second chosen in the same session, release), each through the one pointer lock and with the checks REQ-ACT-03 gives a click: both targets rechecked just before the press, and the drag abandoned with the button released if the second check fails. Propose the key bindings and the spec text as a change in changes/.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-ACT-02 REQ-ACT-03`
- scenario: hover moves the pointer to the target and sends no button; a drag presses on the first target, moves to the second and releases; a stale second target releases the button without completing the drag.

## Log

- 2026-10-27: created from the 2026-10-25 review (queue item 27).
