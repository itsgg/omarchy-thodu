# 0015: Preserve headless sessions on refresh

Status: done
Depends on: none
Spec: REQ-SESSION-01
Goal: Keep a session started with `overlay=false` on the IPC path if stale-target handling recollects it, so the refresh does not summon the overlay or claim keyboard focus.

Acceptance:
- run: `cargo test -p thodud tests::req_session_01_refetch_preserves_overlay_setting -- --exact`
- scenario: When a stale target from a headless session is refreshed, the new session keeps `overlay=false` and `test` unchanged.

## Log

- 2026-10-24: created after code review found that stale-target refresh always enabled the overlay.
- 2026-10-24: stored the overlay mode in each session and reused it with the test flag when stale-target refresh recollects. Corrected the exact acceptance selector after confirming the old command ran zero tests. The requirement test passed (1 test); `tools/gate` passed in 148.8 seconds. `cargo-deny` was unavailable and skipped.
- 2026-10-24: Found that the test did not cover the settings passed to the refreshed start. Routed the refresh through a testable start dispatcher and asserted the request, mode, overlay and both test values. A deliberate `overlay=true` mutation failed the acceptance test; restoring the implementation made it pass.
- 2026-10-24: The updated refresh-start test passed the full gate (163.6 seconds); merged as `f8f40e7`.
