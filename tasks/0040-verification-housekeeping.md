# 0040: Deliver the verification SPEC section 10 promises, and fix two repository gaps

Status: open
Depends on: none
Spec: REQ-SESSION-01, REQ-PERF-01
Goal: SPEC section 10 says accessibility trees are recorded from real apps by `tools/record-tree` and that `tools/gate --nightly` adds live checks against real apps and `thodu bench`, but no task delivered either: the nightly tier runs only cargo-mutants, and open question O2 promised a task that was never written. Write `tools/record-tree`, which records the focused window's tree and its expected targets into spec/cases/atspi in the existing case format, testing it against a fake bus; and add the nightly live checks and bench to the nightly tier, skipping with a clear message when GG's session is not reachable. Both are protected paths, so declare each on a `Protected:` line; they add checks rather than loosen any. The first real recording runs on GG's session, by the coordinator. Separately, cargo-deny never runs because the repository has no `deny.toml`; add one that passes on the current dependencies, including the woints git source. And docs/install.md starts its command block with `set -eu`, so pasting it into an interactive shell closes the terminal at the first failing command; make the block safe to paste.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-SESSION-01 REQ-PERF-01`
- run: `test -f deny.toml && cargo deny check bans licenses sources`
- run: `! grep -qx 'set -eu' docs/install.md`
- scenario: `tools/record-tree` writes a case from a fake bus that the replay accepts; `tools/gate --nightly` runs the live checks and bench on GG's session and says why it skips them elsewhere.

## Log

- 2026-10-27: created from the 2026-10-25 review (queue items 23 and 24, the parts an agent can do). The coordinator's parts stay with the coordinator: marking landed tasks still in review as done, task 0002's cleared block, the unmerged `template-update` branch, change 0006, and deleting stale branches and build output, which waits for GG.
