# 0035: End scroll sessions cleanly and answer heartbeats on time

Status: open
Depends on: none
Spec: REQ-ACT-04, REQ-ACT-06, REQ-SESSION-03
Goal: A scroll session can outlive its use and leave the pointer where it scrolled. In scroll mode `key` in crates/thodud/src/main.rs adds one to `pending` for any key that arrives while a scroll waits for `scroll-ready`, though that key is ignored, and `scroll_with` takes only one back off; `scroll-stop` then never restores the pointer, and after Escape `finish` only sets `closing`, so the session stays until the 120 s `SESSION_LIFE` prune. The `close` op does not cancel a queued scroll as Escape does. When the overlay shows a new session while a scroll step is in flight, `Thodu.qml` ends the old view with "replaced", components/SessionView.qml defers that end while `scrollBusy`, and `begin()` clears `deferredEnd`, so no close reaches thodud, its pointer restore never runs, and the next session restores the pointer to the old scroll area. The overlay's requests share one connection that `serve()` answers in order, so a `hidden` that blocks up to 400 ms, or a first `scroll-ready` that includes the 300 ms `Wheel::open` settle and the focus override's Hyprland calls, can hold a heartbeat past the overlay's 800 ms silence limit; the overlay then reports that thodud stopped answering, sends no close, and the pointer stays moved. Count a pending scroll only for a key that queues one, let `close` cancel a queued scroll, close a replaced session on both sides with its pointer restored, and answer heartbeats without waiting behind slow requests (a thread per request, or the wheel device opened before the first scroll, are two ways).

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-ACT-04 REQ-ACT-06 REQ-SESSION-03`
- scenario: keys that arrive during a queued scroll leave one scroll pending, `scroll-stop` restores the pointer, and `close` cancels a queued scroll and removes the session; the tests drive `key()` rather than setting `pending` by hand.
- scenario: a session replaced mid-scroll is closed in thodud, and the pointer returns to where it was before that session.
- scenario: a heartbeat sent while a 400 ms `hidden` or the first `scroll-ready` runs is answered within the overlay's limit.

## Log

- 2026-10-27: created from the 2026-10-25 review (queue items 14 and the heartbeat part of 15). Each defect was traced in the code then; the pending drift and the replaced session were also shown by probe tests. Confirm each with a failing test first, since line numbers have moved.
