# 0018: Keep scroll focus and distinguish the active area

Status: review
Depends on: none
Spec: REQ-SESSION-07, REQ-ACT-04, REQ-ACT-06
Goal: Keep scroll keys inside Thodu, deliver wheel input to the selected application, and make the selected area easy to distinguish from a click target.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-SESSION-07 REQ-ACT-04 REQ-ACT-06`
- scenario: In a live Hyprland session with focus-following enabled, tap and hold H/J/K/L across multiple wheel actions; none of those keys appear in the target application's input, the mode remains open, and Escape closes it.
- scenario: With a GTK or Qt scroll pane selected, H/J/K/L moves its content in the expected direction and keeps scroll mode open; Escape closes it.
- scenario: The selected scroll area has a distinct, high-contrast outline while normal hint selection keeps its existing style; the visible key cue still names Escape as the exit.

## Log

- 2026-10-24: created after live feedback reported scroll keys reaching the underlying terminal; acceptance covers retained keyboard capture and scroll-area feedback.
- 2026-10-24: requested a render after the scroll input mask changes, so the frame-swapped handshake reaches thodud before wheel input. Kept the scroll focus outline on the theme accent. Rebuilt and installed both executables and the plugin. The acceptance suite passed; live Hyprland held H/J/K/L with an empty target input recorder, kept the session open, and Escape closed it.
- 2026-10-24: GG reported that J/K did not move the selected app. Research found Hyprland handles discrete scroll on the existing virtual-pointer protocol, so scroll now sends motion and a discrete axis through that ordered path. Added an offline direction mapping test. The prior live test did not verify application scrolling; no new live input test was run while GG was working.
- 2026-10-24: restored the spec-required uinput wheel path and made the focus grab inactive while pointer input passes through to a scroll area; layer-shell retains exclusive keyboard focus. Added a QML policy regression. The live app-scroll scenario remains unverified while GG is using the desktop, so this task stays open.
- 2026-10-24: Added a scoped Hyprland pointer-focus override around wheel input. It records and restores the owning window's prior `stay_focused` value, including after a daemon restart, and preserves a value already set by a window rule. The full gate passed earlier; fresh acceptance passed. Installed this branch and restored the Ctrl hints, grid, and scroll bindings. Live Chromium test: labels 1 and 2 selected different visible windows; held J scrolled only the selected window and neither page received keypresses. Live Qt test: held J moved a QScrollArea from 0 to 240, held K returned it to 0, the app received no keys, and Escape removed the overlay.
- 2026-10-24: Enabled Thodu in Omarchy's right bar by removing the stale overlay-only entry and enabling it with the `right` placement; both the Omarchy plugin list and shell IPC now report `enabled: true`. `thodu doctor` passed. In a live Qt scroll pane with no AT-SPI pane, Thodu used the window fallback; held J moved the scrollbar from 0 to 120, held K returned it to 0, the app recorded zero keypresses, and Escape removed the overlay. Closed the disposable test app and restored the original terminal focus.
- 2026-10-24: Agy review found that a migrated address-only legacy record had no PID and remained on disk, blocking later scroll actions. Startup cleanup and per-window recovery now discard that record without mutating a window, so scrolling can continue safely. Added regression coverage for cleanup and the next scroll action; both focused Rust tests pass.
