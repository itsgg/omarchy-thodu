# 0002: Measure the time to labels and settle the language

Status: open
Depends on: 0001
Spec: REQ-PERF-01
Goal: `thodu bench --sessions N` measures the time from `thodu hints` to labels drawn, at the median and the 95th percentile, and exits 1 above REQ-PERF-01's thresholds. Run on GG's session over gtk3-widget-factory and Slack's app window. If Rust's path is over budget, set the task to blocked with the numbers and propose the change; otherwise record the numbers in the Log, which settles SPEC open question O1.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-PERF-01`
- On GG's session: `thodu bench --sessions 50` prints both percentiles under the thresholds, and the Log records them.

## Log

- 2026-10-21: created.
