# 0021: Stable identities for bar items

Status: merged
Why: REQ-SRC-07 requires checking a selected target before activation. Bounds and text cannot distinguish a replacement bar control at the same position, while deriving an ID from the whole bar invalidates unrelated controls when a neighbor moves. The `shell-items` v1 contract currently has no per-item identity.

## Added

None.

## Modified

### REQ-SHELL-01: The bar is a source
When a session starts, the overlay shall report the bar's clickable items on the focused window's monitor, or on Hyprland's focused monitor when no app window is focused, with their bounds in logical coordinates and a stable, opaque identity, as `shell` targets. Each identity shall be unique among the reported items and stable for the lifetime of its QML object, independent of its text and bounds; a replacement object shall receive a different identity. The daemon shall accept earlier v1 reports without identity and invalidate a legacy target before activation if its item index or text changes.
Acceptance:
- scenario: a QML test with a fake bar of three items reports three targets with their bounds.
- scenario: QML tests verify an item's identity stays stable when its text changes and across repeated scans, and a replacement object receives a different identity.
- scenario: with no active app window, the source collects cached bar items by monitor even when the app window list is empty.
- scenario: an earlier v1 bar report without identity is accepted; a legacy target remains valid while its item index, text, bounds, and bar origin match, and refreshes before activation if any changes.

## Removed

None.

## Tests

- Update the REQ-SHELL-01 behavior and `shell-items` shape in SPEC sections 4 and 7.
- Extend bar scanning and daemon stale-target tests for stable identities and old v1 payloads.
