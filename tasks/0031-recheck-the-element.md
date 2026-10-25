# 0031: Recheck an accessible element before clicking it

Status: done
Depends on: 0030
Spec: REQ-ACT-03
Goal: Before a click, thodud checks that the target's window has not moved and still owns the click point, but not that the element itself is still where it was. Targets are indexes into a tree read once, so when Slack's channel list reorders, a page scrolls or a list loads inside an unchanged window, Return clicks whatever element now sits at the old center. Keep each accessible target's element address and the extents read at collection, and before the click read that element again: if it is gone, no longer showing, or its window-relative extents moved by more than a unit, refresh instead of clicking.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-ACT-03`
- scenario: an accessible target carries its element's address and collected extents; the recheck passes for the same extents, and fails for moved extents, a hidden element or one the bus no longer answers for.
- scenario: a click on a target whose element moved refreshes the session instead of pressing.

## Log

- 2026-10-25: created from a confirmed review finding: targets hold no element reference, so content that reflows inside an unchanged window takes the click.
- 2026-10-25: accessible nodes read live keep their bus address, and their targets carry an element check with the window-relative extents read at collection. Before a click, thodud reads the element again and refreshes when it is gone, no longer showing, or moved by more than a unit; a bus busy for more than 150 ms counts as moved. The recheck's decision is split from its Hyprland reads so its wiring is tested. Five tests cover it, and five planted faults were each caught. Change 0026 states it in REQ-ACT-03, with task 0030's click sequence. Installed live with 0030: 36 accessible and 21 bar targets on GG's screen. `tests/acceptance/reqs.sh REQ-ACT-03` passes.
- 2026-10-25: coordinator merged at `09fa642` after the full gate passed in 380 seconds; the review's one finding, the lock kept at review, is closed here.
