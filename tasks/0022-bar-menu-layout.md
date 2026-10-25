# 0022: Tighten the bar menu settings layout

Status: open
Depends on: none
Spec: REQ-SHELL-02
Goal: Group each scroll step value with its controls and make the settings menu more compact, then update the README screenshot to show the finished widget.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-SHELL-02`
- scenario: The scroll step and Shift step labels, values, and decrement/increment buttons read as compact groups, with keyboard focus still clear.
- live: The installed top-bar menu matches the updated README screenshot and its mode and settings actions still work.

## Log

- 2026-10-25: created after GG pointed out that the step controls look detached in the menu screenshot.
