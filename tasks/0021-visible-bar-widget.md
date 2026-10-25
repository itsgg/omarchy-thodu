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
