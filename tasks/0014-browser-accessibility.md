# 0014: Document browser accessibility setup

Status: open
Depends on: none
Spec: REQ-SRC-02, REQ-SRC-03, REQ-CLI-01
Goal: Explain how to start Chromium, Electron and Chromium-based web apps with accessibility enabled, what Thodu currently supports from their trees, and how to diagnose missing browser targets. Add a privacy-safe screenshot of the real Thodu overlay on the generic Chromium reference page.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-SRC-02 REQ-SRC-03 REQ-CLI-01`
- README gives the normal restart path, a Chromium fallback, Electron guidance, and the current limits on general web page coverage.
- README embeds a screenshot of the generic reference page with Thodu labels and an accurate caption.

## Log

- 2026-10-24: created after verifying the reference Chromium page exposes seven AT-SPI targets when launched with `ACCESSIBILITY_ENABLED=1`.
