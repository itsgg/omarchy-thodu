# 0014: Document browser accessibility setup

Status: done
Depends on: none
Spec: REQ-SRC-02, REQ-SRC-03, REQ-CLI-01
Goal: Explain how to start Chromium, Electron and Chromium-based web apps with accessibility enabled, what Thodu currently supports from their trees, and how to diagnose missing browser targets. Add a privacy-safe screenshot of the real Thodu overlay on the generic Chromium reference page.

Acceptance:
- run: `tests/acceptance/docs-browser-setup.sh`
- run: `tests/acceptance/reqs.sh REQ-SRC-02 REQ-SRC-03 REQ-CLI-01`
- README gives the normal restart path, a Chromium fallback, Electron guidance, and the current limits on general web page coverage.
- README embeds a screenshot of the generic reference page with Thodu labels and an accurate caption.

## Log

- 2026-10-24: created after verifying the reference Chromium page exposes seven AT-SPI targets when launched with `ACCESSIBILITY_ENABLED=1`.
- 2026-10-24: README now documents restarting Chromium and Electron after install, the Chromium accessibility fallback, Electron environment checks, and the limits of general website coverage. Added a controlled screenshot of Thodu labeling the local Chromium reference page. `thodu doctor` reports the session environment present; the already-running Chromium process did not have it. A fresh reference page launched with `ACCESSIBILITY_ENABLED=1` exposed seven AT-SPI targets in 23 ms without `--force-renderer-accessibility`. The existing browser was left open. `tests/acceptance/reqs.sh REQ-SRC-02 REQ-SRC-03 REQ-CLI-01` passes.
- 2026-10-24: review found the docs acceptance did not check README changes, Chromium app windows could reuse an old browser process, and the screenshot crop included a partial fourth label. Added a docs acceptance check, explained full Chromium restart and per-app Flatpak overrides, and recaptured only the three reference buttons. A fresh Chromium app launched through the Hyprland session exposed seven AT-SPI targets to `thodu hints` in 20 ms; the launch environment included `ACCESSIBILITY_ENABLED=1` with no command prefix. Left GG's existing browser running.
- 2026-10-24: merged as `b017ff7` and pushed to `origin/main`.
