# 0048: Avoid weak collections in the live bar scan

Status: open
Depends on: 0047
Spec: REQ-SHELL-01
Goal: Replace weak-key collection lookups in the live bar scan with bounded strict-reference tracking. The later Quickshell dumps still end at `QV4::Value::sameValueZero` from a QML timer, so the scan must preserve stable target IDs and cycle handling without WeakMap or WeakSet operations on live Qt objects.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-SHELL-01`
- live: With Thodu's bar widget enabled, the shell reports its bar targets and stays running for 15 minutes without a new Quickshell dump.

## Log

- 2026-10-27: created after the iterative scan passed 68 seconds but later produced two more Quickshell dumps at 15:23.
