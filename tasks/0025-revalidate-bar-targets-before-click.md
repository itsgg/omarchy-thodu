# 0025: Revalidate bar targets before clicking

Status: review
Depends on: none
Spec: REQ-SHELL-01, REQ-SRC-07
Goal: Check a selected shell target against the latest reported bar items and current bar origin before clicking, so a removed or moved tray item refreshes the session instead of clicking its former coordinates.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-SHELL-01 REQ-SRC-07`
- scenario: An unchanged shell item remains valid; a removed or moved item and a moved or missing bar layer are rejected before the pointer action.

## Log

- 2026-10-25: created from the confirmed bar-target activation gap. `BarProbe.qml` refreshes cached bar geometry periodically, but `thodud` currently skips the stale check when a target has no window address.
- 2026-10-25: added a REQ-SRC-07 test for unchanged, moved, removed and renamed bar items and for changed or missing bar origins. The daemon now rechecks shell targets against the latest cache and current layer origin before activation; stale targets refresh instead of clicking their previous point.
- 2026-10-25: `tests/acceptance/reqs.sh REQ-SHELL-01 REQ-SRC-07` passed with 55 requirement tests, 93 core unit tests, and 83 QML tests.
- 2026-10-25: An initial bar-wide geometry fingerprint was replaced by stable per-object identities and per-target bounds checks. This keeps unrelated targets valid when neighboring items move and rejects replacements at identical bounds. The daemon routes stale targets to refresh before the production activation callback. The task acceptance passed with 55 requirement tests, 94 core unit tests, and 84 QML tests.
- 2026-10-25: Older `shell-items` reports without identity remain readable; text or ordering changes invalidate their targets before activation.
- 2026-10-25: `tests/acceptance/reqs.sh REQ-SHELL-01 REQ-SRC-07` passed with 55 requirement tests, 95 core unit tests, and 84 QML tests.
- 2026-10-25: Merged the stable-identity contract into REQ-SHELL-01 and documented the `shell-items` field and compatibility behavior in SPEC section 7.
