# 0050: Make the timing tests pass on a loaded machine

Status: done
Depends on: none
Spec: REQ-SRC-05, REQ-SRC-06
Goal: The full gate's acceptance step runs the whole Rust suite once per task in review or done, about 40 times, so a test that fails one run in twenty fails the gate most times, and no branch can land. Three tests fail that way on main, from load alone; this task makes each one hold on a loaded machine without checking less.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-SRC-05 REQ-SRC-06`
- `tools/gate` passes three times in a row while ten busy loops run (`timeout 900 sh -c 'while :; do :; done' &`, ten times).
- The three tests below check the same behavior as before; a test that only got looser is not done.

## Log

- 2026-11-27: created. Measured on main and on coord/target-per-worktree under the same load, interleaved: main failed 2 of 20 full suites, the branch 1 of 20; under ten busy loops the full suite failed 7 of 15 runs. Two `tools/land check` runs of coord/target-per-worktree failed in acceptance on these tests.
  - `image::tests::req_src_06_a_capture_complete_at_its_limit_is_kept` (4 of 15 loaded runs): it spawns `true` and calls `exit_status` with a deadline of now, so only `EXIT_GRACE` covers the child's exit; under load `true` has not exited yet. A race in the test: wait until the child has exited (its /proc state is Z) before the call.
  - `image::tests::req_src_06_a_detection_that_reaches_its_deadline_is_used_in_part` (3 of 15): the image result reached `collect_phase` 2.7 to 16 ms past the 180 ms budget, past the 30 ms the detector's early deadline reserves. Run alone it passed 25 of 25 under the same busy loops, so the cause is the other tests the binary runs in parallel.
  - `sources::tests::req_src_05_a_hung_background_app_leaves_the_focused_targets_in_time` ("atspi missed its 300 ms budget"; failed in a land check, not in the loaded runs): the same kind of wall-clock budget as the one above.
  - Decided: the tests that hold a real budget run in a pass of their own, one thread, after the parallel pass: marked `#[ignore = "timing: run alone"]`, and `tests/acceptance/reqs.sh` and the gate's cargo-test step add `cargo test --workspace --locked -- --ignored --test-threads=1`. That adds a pass and drops none (a protected change that adds a check). Find every test with a wall-clock budget, not only these three. Widening a reserve or a budget is not the fix, since REQ-SRC-05 and REQ-SRC-06 fix the budgets.
- 2026-11-27: review. The cause under the image test was the debug build: capture and PNG decode took 24 to 180 ms of the 180 ms budget under load, because the png and inflate crates were unoptimized, and the fake capture encoded a fresh PNG inside the budget. Dependencies now build at opt-level 2 in dev and test builds (the workspace's crates stay debug), and the fake capture encodes once; the same step then took 3 to 9 ms. The twelve tests that hold a wall-clock budget are marked `#[ignore = "timing: run alone"]` and run in a one-thread pass after the parallel run, in reqs.sh and in the gate's new cargo-test-timing step. Measured in a scope of 4 CPUs with 3 busy loops: before, the one-thread pass alone failed 3 of 15 rounds; after, the parallel run and the one-thread pass passed 15 of 15. A quota-capped scope was not used for the measurement, since its throttling stalls the whole group for tens of milliseconds by itself.
- 2026-11-27: review found that waiting for `true` to exit made the capture test pass with `EXIT_GRACE` set to zero, so it no longer checked the grace; its original body is back, and it runs in the one-thread pass, where it caught that fault 5 of 5 times and passed 5 of 5 without it. Not run as written: ten busy loops beside the full gate on all ten CPUs, which GG stopped as too heavy for his machine. Run instead at the same ratio of loops to CPUs, in a scope of 4 CPUs with 4 busy loops: the parallel run and the one-thread pass passed 30 of 30 rounds, against the about 40 rounds one acceptance step runs.
- 2026-11-27: done; landed on main at 4108cfd.
