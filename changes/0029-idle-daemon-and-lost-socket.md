# 0029: thodud sleeps while idle and stops once no client can reach it

Status: merged
Why: Task 0049. Section 9 said `thodud` uses no CPU while idle, with no requirement or test behind it, and the accept loop polled a nonblocking listener every 10 ms: about 100 wakeups a second, measured as 307 context switches in 3 s on the installed service. `thodu` starts a `thodud` on whatever `$XDG_RUNTIME_DIR` it is given, and a daemon started on a temporary one outlived its directory. On 2026-11-01 348 such daemons slowed GG's machine to load 178; on 2026-11-11 another 239, spawned by Arivu's verb contract test through the real `thodu`, held about three of six cores and pushed the load to 95.

## Added

### REQ-PERF-02: Sleep while idle, stop when unreachable
While idle, `thodud` shall wait in the kernel for a client and wake at most once a second. When its socket path is removed or names another file, `thodud` shall stop within 2 seconds with an ERROR line and exit status 1, leaving whatever now holds the path in place. A `shutdown` request shall release the socket at once. On shutdown `thodud` shall unlink its socket only while the path still names it, before it stops listening.
Acceptance:
- scenario: an idle `thodud` makes at most 30 context switches over 3 seconds.
- scenario: WHEN its runtime directory is removed THEN `thodud` exits 1 within 3 seconds with an ERROR line.
- scenario: WHEN its socket is replaced by another listener THEN `thodud` exits 1 and the replacement still accepts connections.
- scenario: WHEN `shutdown` is sent THEN `thodud` exits 0 within 500 ms and its socket is gone.

## Modified

Section 9's sentence "`thodud` uses no CPU while idle and under 50 MB of memory (provisional)" now reads "`thodud` uses under 50 MB of memory (provisional); idle CPU is REQ-PERF-02."

## Tests

- crates/thodud/tests/lifetime.rs: req_perf_02_an_idle_thodud_sleeps, req_perf_02_thodud_stops_once_its_runtime_directory_is_removed, req_perf_02_thodud_stops_and_keeps_a_replacement_socket and req_perf_02_shutdown_releases_the_socket_at_once are added.
