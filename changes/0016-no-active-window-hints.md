# 0016: Start hints without an active app window

Status: merged
Why: `thodu hints` failed before source collection when Hyprland had no focused app, even though Hyprland still reports a focused monitor and the Omarchy bar remains available there.

## Added

### REQ-SESSION-06: Hints work without an active app window
When `thodu hints` runs and Hyprland reports no active app window, the system shall use Hyprland's focused monitor and continue collecting targets for the requested scope. If no source finds a target, it shall open the grid as REQ-SESSION-04 specifies.
Acceptance:
- scenario: with no active app window and a cached bar item, hints labels the bar item on the focused monitor.
- scenario: with no active app window and no source targets, hints opens the grid and reports that no targets were found.

## Modified

### REQ-SHELL-01: The bar is a source
When a session starts, the overlay shall report the bar's clickable items on the focused window's monitor, or on Hyprland's focused monitor when no app window is focused, with their bounds in logical coordinates, as `shell` targets.
Acceptance:
- scenario: a QML test with a fake bar of three items reports three targets with their bounds.
- scenario: with no active app window, the source collects cached bar items by monitor even when the app window list is empty.

## Removed

None

## Tests

- Test focused-monitor selection when the monitor has no active app window.
- Test the no-window session scope with a bar item and with no targets.
- Collect shell targets by monitor with an empty app window list.
