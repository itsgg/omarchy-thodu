# 0004: Right, double and modified clicks, and the grid

Status: open
Depends on: 0001
Spec: REQ-ACT-02, REQ-GRID-01
Goal: Shift-Return, a double Return and Control-Return act as REQ-ACT-02 says, and `thodu grid` or Space opens the refining grid of REQ-GRID-01, so every point on the monitor can be clicked.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-ACT-02 REQ-GRID-01`
- On GG's session: the grid reaches an icon in Zed, which has no accessibility tree.

## Log

- 2026-10-21: created.
