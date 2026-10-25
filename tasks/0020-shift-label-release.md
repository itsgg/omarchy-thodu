# 0020: Release Shift before dispatching shifted keys

Status: review
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
- 2026-10-25: claimed by codex-shift-activation at 00:39 UTC.
- 2026-10-25: buffered shifted letter requests until Shift release, then dispatched them in order; ending a session clears pending requests. This keeps the physical modifier up before an uppercase label can trigger a click. `tests/acceptance/reqs.sh REQ-ACT-01` passed: 92 Rust tests and 53 QML tests.
