# 0019: Keep scroll pointer steady and refresh the guide

Status: claimed
Depends on: none
Spec: REQ-ACT-06
Goal: Keep the pointer fixed over a selected scroll area across held wheel repeats, while giving new users a short setup guide and real screenshots of hints and scroll mode.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-ACT-06 && tests/acceptance/docs-browser-setup.sh`
- scenario: A daemon test holds the selected point constant across repeated scroll actions and verifies the pointer does not receive one-pixel focus nudges.
- scenario: On the live Hyprland session, held J/K scrolls the selected app without per-tick cursor movement, the app receives no H/J/K/L keypresses, and Escape closes the mode.
- scenario: README has a concise install/use path and includes verified screenshots of hints and scroll mode.

## Log

- 2026-10-24: created after GG reported pointer jumps and flicker during scroll; existing documentation was long and showed only a cropped browser test page.
