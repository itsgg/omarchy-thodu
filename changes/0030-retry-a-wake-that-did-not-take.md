# 0030: Retry a Chromium wake that did not take

Status: proposed
Why: Task 0036. REQ-SRC-03 wakes a dormant tree "at most once per process". The daemon marked a process woken before the wake had any effect, so a GetAttributes call that failed, timed out or reached a process that was not ready yet left that process dormant for the daemon's whole life, and so did a tree that went dormant again later: every later session found no targets in it and never tried again. Once per process is the wrong unit; what must stay bounded is the cost inside one session.

## Modified

### REQ-SRC-03: Waking Chromium and Electron
When a Chromium or Electron window's tree is dormant, the system shall wake it with one `GetAttributes` call on the frame, at most once per process in a session, and collect after it. A process counts as woken only once its frame reports children other than the null object; a later session that finds the frame dormant again shall wake it again.
Acceptance:
- scenario: WHEN the fake bus reports a frame whose only child is the null object THEN `thodud` calls GetAttributes once and collects the children it then reports.
- scenario: WHEN a wake does not take THEN the next session calls GetAttributes again and collects the children it then reports, and a session whose wake took does not wake that process again.

## Tests

- crates/thodud/src/atspi.rs: req_src_03_a_wake_that_did_not_take_is_retried_later is added; req_src_03_a_dormant_tree_is_woken_once_and_collected is kept unchanged.
