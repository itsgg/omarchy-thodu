# 0020: Release Shift before dispatching shifted keys

Status: review
Depends on: none
Spec: REQ-ACT-01
Goal: Hold a shifted key only when it completes a visible label until physical Shift is released, so the resulting click cannot inherit the browser's Shift-click behavior while shifted prefixes still respond immediately.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-ACT-01`
- scenario: QML tests verify shifted prefixes dispatch immediately, completed visible labels wait for Shift release, later keys cannot overtake or burst after an activation, and Escape or session end clears the pending activation.
- scenario: REQ-ACT-01 tests verify uppercase label activation remains a single left click.
- scenario: live browser check confirms Shift+label activation opens a link in the current tab or window, with no new window.

## Log

- 2026-10-25: created after GG reported that Shift+label activation opens browser links in a new window.
- 2026-10-25: claimed by codex-shift-activation at 00:39 UTC.
- 2026-10-25: buffered shifted letter requests until Shift release, then dispatched them in order; ending a session clears pending requests. This keeps the physical modifier up before an uppercase label can trigger a click. `tests/acceptance/reqs.sh REQ-ACT-01` passed: 92 Rust tests and 53 QML tests.
- 2026-10-25: narrowed the buffer to a shifted key that completes a visible non-grid label. Prefixes now dispatch immediately, a pending activation shows a release cue, and Escape cancels it. Added coverage for ordering and release behavior after review.
- 2026-10-25: `tests/acceptance/reqs.sh REQ-ACT-01` passed after the refinement: 92 Rust tests and 55 QML tests.
