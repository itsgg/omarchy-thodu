# 0020: Release Shift before dispatching shifted keys

Status: open
Depends on: none
Spec: REQ-ACT-01
Goal: Keep physical Shift captured by the overlay until it is released before dispatching Shift-modified key requests, so activating a link with a label does not inherit browser Shift-click behavior.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-ACT-01`
- scenario: a QML test verifies Shift-modified keys are buffered in order until Shift release and discarded if the session ends first.
- scenario: REQ-ACT-01 tests verify uppercase label activation remains a single left click.
- scenario: live browser check confirms Shift+label activation opens a link in the current tab or window, with no new window.

## Log

- 2026-10-25: created after GG reported that Shift+label activation opens browser links in a new window.
