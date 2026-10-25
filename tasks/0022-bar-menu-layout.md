# 0022: Tighten the bar menu settings layout

Status: review
Depends on: none
Spec: REQ-SHELL-02
Goal: Group each scroll step value with its controls and make the settings menu more compact, then update the README screenshot to show the finished widget.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-SHELL-02`
- scenario: The scroll step and Shift step labels, values, and decrement/increment buttons read as compact groups, with keyboard focus still clear.
- live: The installed top-bar menu matches the updated README screenshot and its mode and settings actions still work.

## Log

- 2026-10-25: created after GG pointed out that the step controls look detached in the menu screenshot.
- 2026-10-25: rebuilt the menu with Omarchy's panel controls, aligned setting rows, and added layout assertions for shared edges, control alignment, and mode widths. Refreshed the README's live widget screenshot.
- 2026-10-25: `tests/acceptance/reqs.sh REQ-SHELL-02` passed, including 92 core tests and 81 QML tests. The installed bar menu showed Ready; the new screenshot matches the running menu. Component tests verify all mode dispatches, settings dispatch, and stepper bounds.
