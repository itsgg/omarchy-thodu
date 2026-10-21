# 0002: Measure the time to labels and settle the language

Status: review
Depends on: 0001
Spec: REQ-PERF-01
Goal: `thodu bench --sessions N` measures the time from `thodu hints` to labels drawn, at the median and the 95th percentile, and exits 1 above REQ-PERF-01's thresholds. Run on GG's session over the GTK3 reference window and Slack's app window. If Rust's path is over budget, set the task to blocked with the numbers and propose the change; otherwise record the numbers in the Log, which settles SPEC open question O1.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-PERF-01`
- On GG's session: `thodu bench --sessions 50` prints both percentiles under the thresholds, and the Log records them.

## Log

- 2026-10-21: created.
- 2026-10-21: `thodu bench` times sessions from `thodu hints` starting to the overlay's first frame with the labels, which the overlay reports with a drawn request. Summoning the overlay through omarchy-shell took 70 to 170 ms by itself, so the overlay now listens on thodud's socket and thodud pushes each session to it (change 0003); a debug build measured 110.8 ms median over 10 sessions, a release build 61.2 ms median, 72.6 ms at the 95th percentile, 52.9 to 73.9 ms over 50 sessions on the GTK3 reference window, collection 10 ms of it. Rust stays. Slack's window is not measured: it is a Chromium app window on GG's work Slack, which opening would show him online, and its tree needs task 0005's wake. Waiting on GG: `thodu bench --sessions 50 --window class:<Slack's class>` once 0005 lands.

