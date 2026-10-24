# 0013: Keep accessibility frames with their windows

Status: merged
Why: Claude's review of task 0013 found that a same-process background window could reuse the focused sibling's ACTIVE frame when their sizes matched, and an explicitly selected background window could be treated as focused because it was first in the request. Frame matching needs to use actual focus and prevent one frame from serving multiple windows.

## Added

None.

## Modified

### REQ-SRC-01: Match by process, never by name
When collecting from the accessibility tree, the system shall inspect the focused window's process and match its shown ACTIVE frame for that focused window. For a non-focused window, including an explicitly selected window that is not focused, it shall ignore ACTIVE frames and match only a unique shown frame from that process whose extents match that window's size. A collection shall assign an AT-SPI frame to at most one Hyprland window; if more than one window claims the same frame, it shall omit those windows. It shall never match by application name.
Acceptance:
- cases: `spec/cases/atspi/match.jsonl`, pairs of a Hyprland window and a registry snapshot with the expected frame, including two frames from one Chromium process and an app whose name differs from its class.
- scenario: a non-focused window cannot use an ACTIVE sibling frame, even when its extents happen to match; a frame claimed by multiple windows is omitted for all claimants.

## Removed

None.

## Tests

- Add REQ-SRC-01 coverage for explicitly selecting a non-focused window while its same-process ACTIVE sibling has matching extents.
- Add REQ-SRC-07 coverage that two windows cannot claim the same background frame.
