# 0031: Recheck an accessible element before clicking it

Status: open
Depends on: 0030
Spec: REQ-ACT-03
Goal: Before a click, thodud checks that the target's window has not moved and still owns the click point, but not that the element itself is still where it was. Targets are indexes into a tree read once, so when Slack's channel list reorders, a page scrolls or a list loads inside an unchanged window, Return clicks whatever element now sits at the old center. Keep each accessible target's element address and the extents read at collection, and before the click read that element again: if it is gone, no longer showing, or its window-relative extents moved by more than a unit, refresh instead of clicking.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-ACT-03`
- scenario: an accessible target carries its element's address and collected extents; the recheck passes for the same extents, and fails for moved extents, a hidden element or one the bus no longer answers for.
- scenario: a click on a target whose element moved refreshes the session instead of pressing.

## Log

- 2026-10-25: created from a confirmed review finding: targets hold no element reference, so content that reflows inside an unchanged window takes the click.
