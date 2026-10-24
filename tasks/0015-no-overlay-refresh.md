# 0015: Preserve headless sessions on refresh

Status: open
Depends on: none
Spec: REQ-SESSION-01
Goal: Keep a session started with `overlay=false` on the IPC path if stale-target handling recollects it, so the refresh does not summon the overlay or claim keyboard focus.

Acceptance:
- run: `cargo test -p thodud req_session_01_refetch_preserves_overlay_setting -- --exact`
- scenario: When a stale target from a headless session is refreshed, the new session keeps `overlay=false` and `test` unchanged.

## Log

- 2026-10-24: created after code review found that stale-target refresh always enabled the overlay.
