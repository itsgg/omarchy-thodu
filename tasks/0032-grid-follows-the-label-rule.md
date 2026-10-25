# 0032: The grid follows the label rule

Status: open
Depends on: none
Spec: REQ-GRID-01, REQ-ACT-01
Goal: Hints click when a label is completed in lowercase and select with Shift, but the grid still needs Return after its last label, and Return on a cell that Tab focused before it is fine clicks the center of a coarse cell, such as 59 by 37 units, which is imprecise enough to hit the wrong thing. Make the grid follow the label rule: labels on cells larger than 10 units refine as now, a label that chooses a fine cell clicks its center at once when its final character is lowercase and selects it when uppercase, and Return on a focused cell that is not yet fine refines into it instead of clicking.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-GRID-01 REQ-ACT-01`
- scenario: in the grid, a lowercase label on a coarse cell refines, a lowercase label on a fine cell clicks its center with no double-Return wait, and an uppercase final character on a fine cell selects it for Return and Shift-Return.
- scenario: Return on a focused coarse cell refines into it and sends no click.

## Log

- 2026-10-25: created at GG's request after change 0024, and from a confirmed review finding that Tab and Return could click an unrefined grid cell.
