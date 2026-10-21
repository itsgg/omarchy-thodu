# 0002: The requests the overlay and live checks need

Status: merged
Why: building task 0001 showed section 7 lacked what the overlay needs to hand a click over to `thodud` and to notice a hung `thodud`, and what a live check needs to run a session on a window it names without taking GG's keyboard. The key reply also needs the session's view, since the overlay draws the search and the focus from it. No requirement changes.

## Modified

Section 7, Interfaces:

- `start` takes an optional `window` (a Hyprland address) instead of the focused window, `overlay` (default true; false runs the session over the IPC alone) and `test` (default false; true summons the overlay without the keyboard). Its reply carries the status line.
- `session` takes an optional `brief`, which leaves out the monitor and targets: the overlay's heartbeat (REQ-SESSION-03).
- `key` returns the session's view: state, the targets still kept, the focused target, the search and the typed label.
- `hidden`, `{"v":1,"op":"hidden","session":ID}`: the overlay's surface is gone, so a pending action may run (REQ-ACT-01). `thodud` acts anyway 400 ms after the action if no hidden comes.
- `targets` takes an optional `window`.
- KIND adds bad-request, for a request that does not parse, names an unknown key or an unknown session.

## Tests

- crates/thodu-core/tests/req.rs: overlay_requests_parse.
- tests/qml/tst_session.qml: the heartbeat, the key names and the hand-over.
