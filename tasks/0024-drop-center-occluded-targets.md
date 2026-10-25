# 0024: Drop targets whose click point is covered by a popup

Status: review
Depends on: none
Spec: REQ-SRC-04
Goal: Omit an accessibility target when a later popup or dialog covers the point Thodu would click, so selecting it cannot activate the overlaying control.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-SRC-04`
- scenario: A target partly overlapped by a later popup is omitted when its center is under the popup, while an uncovered target and the popup action remain available.

## Log

- 2026-10-25: created from the confirmed popup-occlusion review finding. REQ-ACT-01 clicks the target center, so the target is unsafe when a later layer owns that point.
- 2026-10-25: added a REQ-SRC-04 regression for a partly overlapped target whose center is under a popup; it failed before the fix. Filtering now drops targets only when a later layer contains the click center, and retains targets whose unused edge alone is overlapped.
- 2026-10-25: `tests/acceptance/reqs.sh REQ-SRC-04` passed with 55 core requirement tests, 92 core unit tests, and 83 QML tests.
