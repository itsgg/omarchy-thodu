# 0047: Bound the shell item scan

Status: review
Depends on: none
Spec: REQ-SHELL-01
Goal: Make Thodu's once-per-second bar scan traverse the Omarchy scene without consuming the QML call stack or revisiting cyclic nodes, so the widget cannot crash the shell while reporting bar targets.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-SHELL-01`
- live: With Thodu's bar widget enabled, the shell stays running for at least 60 seconds and reports its bar targets.

## Log

- 2026-10-27: created after repeated Quickshell SIGSEGVs reproduced with the Thodu bar widget enabled.
- 2026-10-27: replaced the recursive scene walk with a bounded iterative traversal and added deep-tree and cycle regressions. REQ-SHELL-01 passes, including 87 QML tests; with the widget restored, Quickshell stayed up for 68 seconds without a new dump.
