# 0046: Reach targets on every monitor

Status: open
Depends on: 0034, 0035, 0036, 0037
Spec: REQ-SESSION-01, REQ-GRID-01
Goal: SEED section 5 says the grid reaches any point on any monitor, and SEED section 9 lists more monitors last among the features. Sessions today collect and draw on the focused window's monitor only. Let a session label the visible windows and bar on every monitor, with an overlay surface per monitor, labels assigned across all of them, coordinates converted per monitor scale and offset, and the grid able to reach any monitor; the click and its checks already work in global logical coordinates, so confirm that with a monitor at a non-zero origin and a different scale. Propose the spec text as a change in changes/. GG's laptop has one monitor, so test with fake monitors, and leave a live check for the coordinator with an external display.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-SESSION-01 REQ-GRID-01`
- scenario: two fake monitors at different scales and origins each get their windows' targets with correct bounds, labels stay prefix-free across both, and the grid reaches a point on the second monitor.

## Log

- 2026-10-27: created from the 2026-10-25 review (queue item 28).
