# 0049: thodud sleeps while idle and stops once no client can reach it

Status: done
Depends on: none
Spec: REQ-PERF-02
Goal: Stop idle `thodud` from polling its listener every 10 ms, and make a `thodud` whose socket was removed or replaced stop instead of running forever, so daemons that `thodu` starts on a temporary runtime directory (tests, sandboxes) go away with that directory. 239 of them held about three of six cores on 2026-11-11.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-PERF-02`
- After installing, the service's `thodud` makes at most 30 context switches over 3 idle seconds.

## Log

- 2026-11-11: created after 239 stray daemons, spawned by Arivu's verb contract test through the real `thodu`, pushed the load to 95; the TODO of 2026-11-01 records 348 more.
- 2026-11-11: the accept loop waits on the listener in async-io's reactor with a one-second timer, checks on each wake that the socket path still names the file it bound (device and inode), and stops with exit 1 when it does not; shutdown wakes the loop with a connection to its own socket and unlinks the socket only while it is still its own, before closing the listener. On main's code the new tests fail three of four (336 wakeups in 3 s, and the daemon outlived a removed directory and a replaced socket); the shutdown test guards the new wake.
- 2026-11-11: merged to main at 9cec983 and installed; the service's thodud made 9 context switches over 3 idle seconds, against 307 before.
- 2026-11-12: the gate failed once in about 37 runs on req_perf_02_thodud_stops_once_its_runtime_directory_is_removed. thodud bound the public path and restricted it and noted its identity afterwards, so a client could reach the socket first; a watcher saw it at mode 755 in 5 of 5 runs on 9cec983. thodud now binds a staging name, restricts it, notes its identity and renames it over the socket path; the watcher sees mode 600 and the kept file in 10 of 10 rounds.
