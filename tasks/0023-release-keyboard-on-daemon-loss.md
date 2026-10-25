# 0023: Release the keyboard when the daemon disappears during scroll

Status: open
Depends on: none
Spec: REQ-SESSION-03
Goal: Ensure daemon silence or connection loss ends a scroll session immediately, even while a wheel action is passing pointer input through.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-SESSION-03`
- scenario: In a busy scroll session, daemon silence and connection loss release keyboard focus, clear the scroll pass-through state, and stop heartbeat requests.

## Log

- 2026-10-25: created from Claude review finding 1, confirmed in SessionView.qml:141-142.
