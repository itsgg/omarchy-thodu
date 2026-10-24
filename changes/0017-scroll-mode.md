# 0017: Scroll mode

Status: merged
Why: SEED section 4 describes scroll mode as the next capability, but SPEC section 2 excludes it. Homerow documents H/J/K/L scrolling, a larger Shift step, cycling with Tab and number selection. AT-SPI exposes scroll panes and viewports; Linux provides vertical and horizontal wheel events, including high-resolution values. Thodu already installs `/dev/uinput` access for Control-clicks. Sources: [Homerow](https://github.com/nchudleigh/homerow), [AT-SPI roles](https://gnome.pages.gitlab.gnome.org/at-spi2-core/libatspi/enum.Role.html), [Linux input event codes](https://docs.kernel.org/input/event-codes.html), [virtual pointer protocol](https://github.com/swaywm/wlr-protocols/blob/master/unstable/wlr-virtual-pointer-unstable-v1.xml).

## Added

### REQ-SRC-08: Scroll areas across visible windows
When scroll mode starts, the system shall collect visible AT-SPI scroll panes and web document viewports from each window in scope. It shall use one window-sized fallback area for a window with no semantic scroll area, clip areas to the window and monitor, keep the owning window address, and omit areas whose center is owned by another visible window. If there are no app windows, it shall return an empty area list and a status message without failing.
Acceptance:
- scenario: a replayed tree with nested scroll panes produces separate visible areas in window coordinates, while hidden and off-window panes are omitted.
- scenario: a Chromium document with no scroll-pane role is exposed as a scroll area; a window with no semantic area gets one window fallback.
- scenario: areas in overlapping windows retain only the topmost window's areas at overlapped points.
- scenario: zero visible app windows returns an empty result and status instead of a no-window error.

### REQ-SESSION-07: Select scroll areas from the keyboard
When `thodu scroll` runs, the system shall start on the focused monitor and collect areas in the requested scope, defaulting to every visible window. It shall number areas in reading order, initially focus the first area, outline the focused area, and show the available keys. Tab and Shift-Tab cycle areas. Decimal digits select an area by its one-based number, with `0` selecting area 10; digits can be combined for areas 11 and above. Return clears the number prefix without scrolling. Escape ends the mode. With no areas, the overlay shall remain dismissible and report that no scroll areas were found, without opening the click grid.
Acceptance:
- scenario: a session test verifies reading-order focus, area cycling, one- and multi-digit selection, Backspace, Return and Escape.
- scenario: a QML test verifies numeric keys are sent only in scroll mode, repeat keys do not issue scrolls, and the overlay identifies the scroll bindings.
- scenario: `thodu scroll --scope focused` scopes to one app; the default mode includes all visible windows on the monitor.
- scenario: when no app window is open, `thodu scroll` opens with no areas and remains dismissible.

### REQ-ACT-04: Scroll the selected area
In scroll mode, `j` and `k` shall send one vertical wheel step down and up, and `l` and `h` shall send one horizontal wheel step right and left, at the selected area's center. Shift with those keys shall send five wheel steps. Scrolling shall not click, shall keep the selected area and mode, and shall resume the overlay after the wheel event. The input path shall support GTK, Qt, Chromium and Electron through the installed uinput device. If the owning window moved, closed or is no longer topmost before the wheel event, the system shall send no wheel event and shall report that the session must be restarted.
Acceptance:
- scenario: REQ-ACT-04 unit tests verify all four directions and the one-step and five-step amounts, with no button events.
- scenario: a daemon test waits for the overlay-hidden event, sends the wheel event to the selected point, and returns the same session to showing.
- scenario: a moved or occluded window produces no wheel event.
- live: scroll a GTK or Qt pane and a Chromium or Electron document, then confirm the overlay remains in scroll mode.

## Modified

### Section 2: Scope and non-goals
The first coherent experience includes keyboard scroll mode. Direct web page targets, full image coverage, drag and hover, the agent interface beyond `thodu targets`, and additional monitors remain later work.

### REQ-INSTALL-01: Works on Omarchy as installed
When `thodu install` runs, the system shall set `ACCESSIBILITY_ENABLED=1` and `QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1` in the Hyprland session environment, install a udev rule giving the logged-in user `/dev/uinput` through `uaccess`, install and enable the overlay plugin and the `thodud` user service, and bind keys in the user's Lua configuration; `thodu doctor` shall report each of these as present or missing. On a fresh configuration with no active Thodu bindings, an install without key options shall bind `SUPER + SHIFT + F12` to `thodu hints`, `SUPER + CTRL + F12` to `thodu grid`, and `SUPER + ALT + F12` to `thodu scroll`. Re-running without options shall preserve existing Thodu bindings and add the default scroll binding if it is missing. Supplying `--scroll-key` shall set only the scroll binding; supplying both `--hints-key` and `--grid-key` shall replace those bindings, with `--scroll-key` optional.
Acceptance:
- scenario: on a fake home directory, `thodu install` then `thodu doctor` reports every item present, and `thodu doctor` alone reports them missing.
- scenario: on a fresh fake home directory, non-interactive `thodu install` writes all three F12 defaults and `thodu doctor` reports the bindings present.
- scenario: on a fake home directory with custom hints and grid bindings, install without key options preserves them and adds the default scroll binding.
- scenario: `--scroll-key` changes only the scroll binding, and supplying all explicit options replaces all three bindings.

## Removed

None

## Tests

- Add replay and fallback tests named for REQ-SRC-08.
- Add scroll-session, wheel planning, stale-window and overlay resume tests named for REQ-SESSION-07 and REQ-ACT-04.
- Extend install tests for the third binding and local override behavior.
- Add a live check on native and Chromium or Electron scroll surfaces.
