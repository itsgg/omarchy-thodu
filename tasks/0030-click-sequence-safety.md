# 0030: Check each click immediately before it, one click at a time

Status: done
Depends on: 0029
Spec: REQ-ACT-01, REQ-ACT-02, REQ-ACT-03
Goal: The click pipeline leaves room for a wrong click. Between the stale and ownership check and the press, thodud queries Hyprland several times for the pointer restore and, on the first Control-click, opens a uinput keyboard that sleeps 300 ms after the pointer has moved. Clicks from different sessions run on their own threads with no lock, though scrolls take one because the compositor has one pointer. The second click of a double Return clicks again after the 1 s wait even if the first click never happened, and never rechecks its target, so a first click that closes a dialog sends the second into whatever lies beneath. Take one pointer lock for clicks and scrolls alike, open the Control keyboard and compute the restore before the check, run the check right before the press, and give the second click the same check, skipping it when the first did not happen or the target moved or was covered.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-ACT-01 REQ-ACT-02 REQ-ACT-03`
- scenario: a click opens the Control keyboard when needed and computes the restore before its check, checks under the pointer lock, and presses right after the check; a failed check presses nothing.
- scenario: a second click is not sent when the first never happened within the wait, or when its target moved or became covered, and is sent once otherwise.
- scenario: two clicks never hold the pointer at once.

## Log

- 2026-10-25: created from confirmed review findings on the click pipeline (second click without a check, check-to-click gap, clicks not serialised).
- 2026-10-25: clicks and scroll actions share one pointer lock (`pointer_action`), held from the check until the last input. A click opens the Control keyboard and computes the pointer restore before its check, and `checked_click` runs the check and the press back to back. The second click of a double click is skipped when the first never happened within the wait or when its target moved or became covered. Four tests cover these; planted faults (second click without the first, without its recheck, clicks without the lock) were each caught. The order inside `act` is held by `checked_click`'s types, not by a test. `tests/acceptance/reqs.sh REQ-ACT-01 REQ-ACT-02 REQ-ACT-03` passes.
- 2026-10-25: the click's slow preparation now runs before the pointer is taken and the refresh after it is let go, so only the check and the press hold the pointer; restores in `finish` and after scrolling take the pointer too. A first click that failed is not recorded as clicked, the second click wakes when the first attempt ends or the session closes, and it checks bar items as the first does. Each fix has a test, and six planted faults were each caught. The lock is released.
- 2026-10-25: coordinator merged at `92254e1` after the full gate passed in 350 seconds; the review's six findings were fixed with tests first.
