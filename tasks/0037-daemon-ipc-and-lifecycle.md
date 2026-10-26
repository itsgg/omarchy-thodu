# 0037: Make thodud idle, restartable and strict about its socket

Status: open
Depends on: none
Spec: REQ-SESSION-01, REQ-SESSION-02, REQ-SESSION-03, REQ-PERF-01
Goal: SPEC section 9 says thodud uses no CPU while idle, but its accept loop is non-blocking with a 10 ms sleep, so it wakes 100 times a second (504 context switches in 5 s on the live service on 2026-10-25). Any accept error other than WouldBlock, such as running out of file descriptors, ends the loop and thodud exits 0, so `Restart=on-failure` never restarts it. When a push to the overlay fails or exceeds `LISTENER_WRITE`, the listener that is dropped is a cloned copy of the socket while `serve()` keeps the others open, so the overlay never sees a disconnect and every later session goes through the slow `omarchy-shell shell summon` until the shell reloads. `Reply::error` in crates/thodu-core/src/protocol.rs carries no session id, so a late error for an old session ends the current one in SessionView.qml. A client that connects and never sends holds a thread and two descriptors forever, and one long line grows memory without bound. During the shutdown handoff the old daemon can remove the socket file after the new one has bound it. One window title that is not valid UTF-8 makes reading Hyprland's clients fail, which ends every hints session with an internal error rather than opening the grid. If releasing Control fails twice in crates/thodud/src/pointer.rs, the uinput keyboard stays open with Control down and every key GG types becomes a Ctrl chord until thodud restarts; destroying the device releases it. Two overlay paths need checking against current code: after the click, `Thodu.qml` takes the keyboard back as Exclusive while the input mask still passes the pointer through, so a real mouse click in the rest of the linger may go to the overlay; and when the `hidden` reply times out with `clicked:false`, the overlay may never take keyboard focus back. Block on accept (or poll with a wake-up for shutdown), exit non-zero on a fatal accept error, shut the socket down when a listener is dropped, put the session id on every error that has one, bound idle reads and line length, remove only a socket file the daemon still owns, read titles lossily, and force a stuck Control up.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-SESSION-01 REQ-SESSION-02 REQ-SESSION-03 REQ-PERF-01`
- scenario: an idle thodud makes no wake-ups for a second; a fatal accept error exits non-zero; a dropped listener's peer sees end of file.
- scenario: an error reply for an old session does not end the current one; a silent client and an overlong line are cut off; a title with invalid UTF-8 still lets a session open.
- scenario: a Control release that fails twice still leaves Control up, and the two overlay paths are either shown sound by a test or fixed.

## Log

- 2026-10-27: created from the 2026-10-25 review (queue items 12, 13, the error replies of 15, and the rest of 17). The idle wake-ups were measured on the live service; the listener copy was shown by a probe test; the rest were traced in the code. Confirm each with a failing test first, since line numbers have moved.
