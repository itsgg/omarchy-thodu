# 0021: Show an interactive Thodu control in the bar

Status: open
Depends on: none
Spec: REQ-SHELL-02
Goal: Replace the invisible scan-only bar probe with a visible status control and settings menu, while preserving bar target collection.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-SHELL-02`
- scenario: The menu launches hints, grid, and scroll; changes existing Thodu settings persist without overwriting unrelated configuration.
- live: The bar control is visible, reports service readiness, opens a themed menu, and its actions work in GG's session.

## Log

- 2026-10-25: created after GG asked for a visible, interactive top-bar widget and a matching spec change.
- 2026-10-25: implemented the visible, themed status control, keyboard-navigable mode and settings menu, persistent settings edits, and popup exclusion from bar scanning. Acceptance and live checks remain before review.
- 2026-10-25: `tests/acceptance/reqs.sh REQ-SHELL-02` passed: 92 Rust tests and 73 QML tests, including component-level menu navigation and mode dispatch. Fixture lookup now finds the active worktree when Cargo reuses compiled tests. Live checks remain.
