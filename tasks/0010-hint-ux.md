# 0010: Clear hint selection and activation

Status: review
Depends on: none
Spec: REQ-SEARCH-01, REQ-ACT-01, REQ-SESSION-01
Goal: Make hint selection visually clear, keep status feedback compact, and let an uppercase final label character activate a target while lowercase completion only focuses it.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-SEARCH-01 REQ-ACT-01 REQ-SESSION-01`
- A QML test checks the selected label treatment, target outline behavior and bounded wrapped status line.

## Log

- 2026-10-22: created from GG's request to improve overlay appearance and research uppercase hint activation.
- 2026-10-22: Added case-aware label activation, selected-chip feedback and a bounded wrapping status toast. Acceptance passed: 48 Rust tests and 24 QML tests.
