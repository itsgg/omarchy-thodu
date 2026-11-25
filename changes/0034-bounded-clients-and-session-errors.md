# 0034: Clients are bounded and errors name their session

Status: proposed
Why: Task 0037. An error reply carried no session, so a late error for an old session ended the overlay's current one. A client that connected and never sent held a thread and two descriptors forever, and one line without a newline grew memory without bound. Section 7 says neither what an error names nor how long thodud waits for a client.

## Modified

Section 7's paragraph on errors now reads: "Errors are `{"v":1,"ok":false,"error":KIND,"message":TEXT}` with KIND one of no-window, no-tree, busy, stale, internal, bad-request, and `"session":ID` when the request line names a session, even one that is not a valid request. The command line prints an ERROR line with the message and exits 1 on any error, 2 on bad arguments."

Section 7 gains, after its first paragraph: "A new connection shall send its first byte within 5 seconds, and each line shall reach its newline within 5 seconds of its first byte and hold at most 1 MiB; `thodud` closes a connection that does not, replying bad-request to an overlong line first. A connection that has sent a request may then wait between requests as long as it likes, as the overlay's request and `listen` connections do. A `listen` connection that `thodud` drops, because a push failed or a newer overlay listens, is shut down, so its overlay sees the end of file and listens again."

## Tests

- crates/thodu-core/src/protocol.rs: req_session_01_errors_name_the_session_their_request_named.
- crates/thodud/src/ipc.rs: req_perf_02_a_silent_client_is_cut_off, req_perf_02_a_line_that_never_ends_is_cut_off, req_perf_02_an_overlong_line_is_refused, req_perf_02_a_client_that_spoke_may_wait.
- crates/thodud/src/listeners.rs: req_perf_01_a_dropped_listener_hangs_up_every_copy, req_perf_01_a_replaced_listener_hangs_up_every_copy.
- crates/thodud/tests/lifetime.rs: the daemon-level checks of the same bounds and of an error naming its session.
