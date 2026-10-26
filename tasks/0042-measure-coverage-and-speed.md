# 0042: Measure coverage and speed on GG's daily apps

Status: open
Depends on: 0041
Spec: REQ-PERF-01
Goal: SEED section 6 measures coverage on a reference set that includes saved captures of GG's daily apps, and speed per source and app, but the set holds only demo apps and a local Chromium page with seven buttons whose marks sit at the found bounds' centres, so coverage is partly self-graded. Slack's latency (REQ-PERF-01 names Slack's window) was never measured, and bench files record no window, commit or build, so the 291 ms median of 2026-10-24 cannot be traced to anything. Make `thodu bench` record the window class, the commit and whether the build is release in each result; add reference cases for Slack, Zed, Foot and Evince with their targets marked by hand rather than from Thodu's own output; and write a comparison with neru on the same windows (SEED risk table: neru overtaking Thodu). The recordings and runs need GG's session, so build the tooling and its tests here, and leave the runs to the coordinator with exact commands in the Log.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-PERF-01`
- scenario: a bench result names its window class, commit and build; the coverage report for a reference case counts hand-marked targets that got a label.

## Log

- 2026-10-27: created from the 2026-10-25 review (queue item 29). neru issue 1807 was still open and neru had no release after 1.59.0 on 2026-10-25.
