# 0041: Record every session

Status: open
Depends on: none
Spec: REQ-SESSION-01
Goal: SEED section 5 says every session records which sources answered, how many targets each found, how long each took and which action ran, and section 6 measures use from those records; none was ever written, so SEED's failure test (whether GG reaches for Thodu over the touchpad) and "the order follows measured gaps" have nothing to go on. Write one JSON line per session to a file under `~/.local/share/thodu`, holding at least the time, mode and scope, the focused window's class, each source's outcome, target count and milliseconds, the time to labels drawn, the generation count, the action and its target's source, or how the session ended (Escape, refresh, timeout, no targets), with no window titles or target text. Rotate or cap the file so it stays small. Add `thodu stats` to summarise sessions per day, by action and source, and latency percentiles. Propose the requirement as a spec change in changes/, with its own ID, and name the tests for it once it is merged; until then they name REQ-SESSION-01.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-SESSION-01`
- scenario: a session that clicks, one that ends with Escape and one that refreshes each write one line with the fields above and no title or target text; the file is capped; `thodu stats` summarises a fixture of records.

## Log

- 2026-10-27: created from the 2026-10-25 review (queue item 22).
