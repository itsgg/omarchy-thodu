# 0023: Release the keyboard when the daemon disappears during scroll

Status: review
Depends on: none
Spec: REQ-SESSION-03
Goal: Ensure daemon silence or connection loss ends a scroll session immediately, even while a wheel action is passing pointer input through.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-SESSION-03`
- scenario: In a busy scroll session, daemon silence and connection loss release keyboard focus, clear the scroll pass-through state, and stop heartbeat requests.

## Log

- 2026-10-25: created from Claude review finding 1, confirmed in SessionView.qml:141-142.
- 2026-10-25: added QML regressions for daemon silence and connection loss during a busy scroll. Both failed before the fix because the end-reason guard used `thodu` while callers send `thodud`; matching the actual reasons now ends the view, releases keyboard focus, clears pass-through, and stops heartbeat requests.
- 2026-10-25: `tests/acceptance/reqs.sh REQ-SESSION-03` passed with 92 core tests and 83 QML tests.
