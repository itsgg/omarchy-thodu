# 0020: Make the Thodu bar widget visible and useful

Status: merged
Why: GG asked for a visible top-bar control that shows whether Thodu is running and provides a way to start modes and adjust settings.

## Added

### REQ-SHELL-02: The bar widget shows Thodu status and controls
When the Thodu bar widget is configured, it shall show a visible, themed control with a tooltip. The control shall indicate whether the `thodud` user service is ready. Activating it shall open a themed, keyboard-navigable menu with actions to start hints, grid, and scroll sessions; controls to change `labels_only`, `status_display`, `scroll_step`, and `scroll_shift_step`; and an action to open the Thodu configuration file in an Omarchy terminal editor. Configuration changes shall persist to `~/.config/thodu/config.toml`, preserve comments and all supported settings, use the existing value constraints, and apply to newly started sessions. If the file contains invalid or unsupported settings, the menu shall explain that it needs editing and disable direct setting changes while keeping the editor action available. Escape or an outside click shall close the menu. The bar widget shall continue to report other clickable bar items for hint sessions.
Acceptance:
- scenario: QML tests verify settings persist without dropping comments or supported settings, reject invalid TOML values, and honor the existing value constraints.
- scenario: a QML test instantiates the menu, verifies keyboard navigation, and verifies all three session mode actions close it.
- live: the visible control appears in the bar, indicates service readiness, opens and closes its menu, launches each mode, and persists setting changes.

## Modified

None.

## Removed

None.

## Tests

- Add REQ-SHELL-02 QML tests for settings persistence and menu actions.
- Add `tests/acceptance/reqs.sh REQ-SHELL-02` to the task acceptance.
