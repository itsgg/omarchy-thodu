# 0021: Stable identities for bar items

Status: proposed
Why: REQ-SRC-07 requires checking a selected target before activation. Bounds and text cannot distinguish a replacement bar control at the same position, while deriving an ID from the whole bar invalidates unrelated controls when a neighbor moves. The `shell-items` v1 contract currently has no per-item identity.

## Added

### REQ-SHELL-03: Stable identities for bar items
Each bar item reported by the shell shall include an opaque identity that is unique among reported items and stable for the lifetime of its QML object, independent of its label and bounds. A replacement object shall have a different identity. The daemon shall continue to accept earlier v1 payloads without identity and treat text or ordering changes in those payloads as stale before activation.
Acceptance:
- scenario: QML tests verify that an item's identity stays stable when its text changes, differs for a replacement object, and is reused across repeated scans.
- scenario: daemon tests verify that a legacy `shell-items` payload without identity is accepted, while text or ordering changes invalidate its target.

## Modified

None.

## Removed

None.

## Tests

- Update the `shell-items` shape in SPEC section 7 to include `identity`, with compatibility behavior for prior v1 payloads.
- Add REQ-SHELL-03 acceptance to the bar scanning and daemon stale-target tests.
