# 0024: Drop targets whose click point is covered by a popup

Status: open
Depends on: none
Spec: REQ-SRC-04
Goal: Omit an accessibility target when a later popup or dialog covers the point Thodu would click, so selecting it cannot activate the overlaying control.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-SRC-04`
- scenario: A target partly overlapped by a later popup is omitted when its center is under the popup, while an uncovered target and the popup action remain available.

## Log

- 2026-10-25: created from the confirmed popup-occlusion review finding. REQ-ACT-01 clicks the target center, so the target is unsafe when a later layer owns that point.
