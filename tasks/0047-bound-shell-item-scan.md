# 0047: Bound the shell item scan

Status: open
Depends on: none
Spec: REQ-SHELL-01
Goal: Make Thodu's once-per-second bar scan traverse the Omarchy scene without consuming the QML call stack or revisiting cyclic nodes, so the widget cannot crash the shell while reporting bar targets.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-SHELL-01`
- live: With Thodu's bar widget enabled, the shell stays running for at least 60 seconds and reports its bar targets.

## Log

- 2026-10-27: created after repeated Quickshell SIGSEGVs reproduced with the Thodu bar widget enabled.
