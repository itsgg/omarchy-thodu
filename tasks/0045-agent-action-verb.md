# 0045: Let agents act on a target

Status: open
Depends on: 0034, 0035, 0036, 0037
Spec: REQ-CLI-01, REQ-ACT-03
Goal: SEED principle 6 says GG and agents get the same targets and the same actions, and SEED section 1 offers Thodu's targets and actions to Arivu. Agents can read targets (`thodu targets --json`) but cannot act on one. Add `thodu act ID ACTION` and the matching IPC op, taking a target id from a `targets` reply and an action thodud already has (click, right click, middle click, double click, scroll), and running it with the same stale, element and pixel checks and the same pointer lock as GG's own sessions; a target that is stale or unknown fails with an ERROR line and a non-zero exit rather than acting. Target ids must be tied to the collection that produced them, so an id from an old collection never acts on a new target. Propose the spec text as a change in changes/.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-CLI-01 REQ-ACT-03`
- scenario: `thodu act` clicks a target from the latest `targets` reply; an id from an older collection, a stale window and an unknown action each exit non-zero without a pointer event.

## Log

- 2026-10-27: created from the 2026-10-25 review (queue item 25).
