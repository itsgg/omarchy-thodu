# 0003: Sessions pushed to a listening overlay, and the bench

Status: merged
Why: task 0002 measured `omarchy-shell shell summon` alone at 70 to 170 ms (2026-10-21), more than REQ-PERF-01's whole median budget, so a session started by summoning the overlay cannot meet it. The overlay is kept loaded (`keepLoaded`), so it can hold a connection to `thodud` and be handed each session at once. Bench also needs to know when the labels were drawn. No requirement changes.

## Modified

Section 4, item 2: the overlay is kept loaded and listens on `thodud`'s socket; `thodud` pushes each session to it, and summons it through omarchy-shell only when nothing listens.

Section 7, Interfaces:

- `listen`, `{"v":1,"op":"listen"}`: the overlay's standing connection. After its reply, `thodud` writes `{"v":1,"event":"open","session":ID,"test":B,"reply":R}` on it for each new session, R being the session's full `session` reply.
- `drawn`, `{"v":1,"op":"drawn","session":ID}`: the overlay drew the session's labels for the first time.
- `session` replies carry `timing`: `collect_ms`, and `drawn_at_us` once drawn, in microseconds since the Unix epoch.
- A `window` may be `class:NAME`, the first window of that class.
- `thodu bench [--sessions N] [--window W]` times N sessions from `thodu hints` starting to `drawn`, with the overlay summoned without the keyboard, prints the median and the 95th percentile, keeps the samples under `~/.local/share/thodu/bench/`, and exits 1 above REQ-PERF-01's thresholds.

Section 12, O1: measured on the GTK3 reference window at 61.2 ms median and 72.6 ms at the 95th percentile over 50 sessions with a release build (2026-10-21), so Rust stays; Slack's window is measured after task 0005, which wakes Chromium's tree.

## Tests

- crates/thodu-core/tests/req.rs: req_perf_01_thresholds_and_percentiles, req_perf_01_verdict, open_event_round_trips.
- tests/qml/tst_session.qml: test_a_pushed_session_draws_at_once.
