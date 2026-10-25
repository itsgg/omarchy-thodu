# 0025: Revalidate bar targets before clicking

Status: open
Depends on: none
Spec: REQ-SHELL-01, REQ-SRC-07
Goal: Check a selected shell target against the latest reported bar items and current bar origin before clicking, so a removed or moved tray item refreshes the session instead of clicking its former coordinates.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-SHELL-01 REQ-SRC-07`
- scenario: An unchanged shell item remains valid; a removed or moved item and a moved or missing bar layer are rejected before the pointer action.

## Log

- 2026-10-25: created from the confirmed bar-target activation gap. `BarProbe.qml` refreshes cached bar geometry periodically, but `thodud` currently skips the stale check when a target has no window address.
