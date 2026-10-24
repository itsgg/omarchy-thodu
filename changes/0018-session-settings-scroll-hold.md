# 0018: Session status and scroll controls

Status: merged
Why: The status toast keeps routine source counts and timings visible for the full session, and scroll distance is fixed at one or five detents. Homerow documents HJKL scrolling and Shift-HJKL Dash; its changelog allows Dash to be slower than ordinary scrolling, supporting independent step sizes. Thodu should retain its defaults while allowing users to tune them. GG also requested hold-to-scroll. The current action path unmaps the overlay for each wheel event, so native key repeat could leak letters to the focused application. The overlay needs a pointer pass-through state that retains keyboard capture. Sources: [Homerow README](https://github.com/nchudleigh/homerow), [Homerow changelog](https://www.homerow.app/changelog).

## Added

### REQ-SESSION-08: Keep guidance while controlling status detail
The overlay shall keep its key cue visible throughout an active session. The `status_display` setting shall accept `auto`, `always` or `off`, defaulting to `auto`. In `auto`, routine collection details shall be visible for 2000 ms after the session first draws, then hidden. In `always`, routine details shall remain visible. In `off`, routine details shall be hidden. Warnings, missing-source or budget failures, no-target states, and active collection shall remain visible in every mode. Live search text and an incomplete label prefix shall remain visible in every mode. These rules shall not apply during the click-through linger state.
Acceptance:
- scenario: a QML test verifies the cue remains visible after routine details expire in auto mode.
- scenario: QML tests verify routine details persist in always mode and are hidden in off mode.
- scenario: QML tests verify warnings, empty-target status, active collection, live search and label prefixes remain visible in off mode.
- scenario: a daemon test verifies which source and no-target statuses are marked persistent.

### REQ-ACT-05: Configure scroll distance
`~/.config/thodu/config.toml` shall accept `scroll_step` and `scroll_shift_step`, each an integer wheel detent count from 1 through 100. Their defaults shall be 1 and 5 respectively. A normal H/J/K/L press shall send `scroll_step` signed detents; a Shift-H/J/K/L press shall send `scroll_shift_step` signed detents. The Shift step may be smaller than the normal step. Invalid values shall make the configuration report an error and use defaults for the session.
Acceptance:
- scenario: config tests verify both defaults, custom values, invalid bounds and the ability for the Shift step to be smaller than the normal step.
- scenario: session tests verify each direction uses its configured signed detent count.

### REQ-ACT-06: Repeat scroll while a key is held
While scroll mode is active, pressing H/J/K/L shall send one wheel action immediately and repeat it at 80 ms intervals until the same key is released. The repeat shall use the step size selected at key-down. Operating-system key-repeat events shall not add actions. During each wheel action, the overlay shall remain mapped and retain exclusive keyboard focus while a small pointer input region over the selected area's center passes through, so wheel events reach the selected area and physical key releases do not reach the application. The pointer shall return to its prior safe position when the hold ends, not after each wheel action. Releasing the key shall stop further actions. Pressing a different non-modifier key shall stop the current repeat before processing that key. Escape shall end scroll mode and stop repetition.
Acceptance:
- scenario: QML tests verify held H/J/K/L repeat at the 80 ms interval and release stops further requests.
- scenario: a QML test verifies OS repeat events do not create extra actions and Escape stops the repeat.
- scenario: a QML test verifies the overlay retains keyboard focus while a small pointer region passes through for wheel events.
- scenario: daemon tests verify each pass-through-ready action sends one wheel event and resumes the same scroll session.
- scenario: the pointer remains over the selected area across repeat ticks and returns to its prior safe position on release.
- scenario: a stale or failed scroll action stops repetition and leaves the session dismissible.

## Modified

### REQ-SESSION-07: Select scroll areas from the keyboard
When `thodu scroll` runs, the system shall start on the focused monitor and collect areas in the requested scope, defaulting to every visible window. It shall number areas in reading order, initially focus the first area, outline the focused area, and show the available keys, including that holding H/J/K/L repeats scrolling. Tab and Shift-Tab cycle areas. Decimal digits select an area by its one-based number, with `0` selecting area 10; digits can be combined for areas 11 and above. Return clears the number prefix without scrolling. Escape ends the mode. With no areas, the overlay shall remain dismissible and report that no scroll areas were found, without opening the click grid.
Acceptance:
- scenario: a session test verifies reading-order focus, area cycling, one- and multi-digit selection, Backspace, Return and Escape.
- scenario: a QML test verifies numeric keys are sent only in scroll mode and the overlay identifies the scroll bindings and hold behavior.
- scenario: `thodu scroll --scope focused` scopes to one app; the default mode includes all visible windows on the monitor.
- scenario: when no app window is open, `thodu scroll` opens with no areas and remains dismissible.

### REQ-ACT-04: Scroll the selected area
In scroll mode, `j` and `k` shall send configured vertical wheel detents down and up, and `l` and `h` shall send configured horizontal wheel detents right and left, at the selected area's center. Shift with those keys shall send the configured Shift step. The step defaults and configuration bounds are specified by REQ-ACT-05. Scrolling shall not click, shall keep the selected area and mode, and shall resume the overlay after the wheel event. The input path shall support GTK, Qt, Chromium and Electron through the installed uinput device. If the owning window moved, closed or is no longer topmost before the wheel event, the system shall send no wheel event and shall report that the session must be restarted.
Acceptance:
- scenario: REQ-ACT-04 unit tests verify all four directions and configured detent amounts, with no button events.
- scenario: a daemon test waits for the overlay's pointer-pass-through-ready event, sends the wheel event to the selected point, and returns the same session to showing.
- scenario: a moved or occluded window produces no wheel event.
- live: scroll a GTK or Qt pane and a Chromium or Electron document, then confirm the overlay remains in scroll mode.

## Removed

None

## Tests

- Add config and session tests named for REQ-ACT-05.
- Add QML and daemon tests named for REQ-ACT-06.
- Add QML and daemon status visibility tests named for REQ-SESSION-08.
- Replace the scroll action's hide-and-resummon path with a pointer pass-through handshake that keeps exclusive keyboard capture; test the handshake for REQ-ACT-04 and REQ-ACT-06.
- Update the scroll cue and README configuration guidance.
