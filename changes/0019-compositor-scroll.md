# 0019: Deliver scroll through the compositor pointer

Status: proposed
Why: GG reported that H/J/K/L left the selected application unchanged even though the daemon resumed the scroll session without an input error. That test did not identify whether the uinput wheel event reached the compositor or the application. The current implementation sends pointer motion through Hyprland's virtual-pointer protocol and wheel events through a separate uinput device, leaving two event paths to diagnose. Hyprland implements `zwlr_virtual_pointer_v1.axis_discrete` and forwards it through the pointer seat; Thodu already depends on that protocol for pointer motion and clicks. Use the existing protocol for motion and wheel axes in order, retaining the post-motion stale-target check.

## Added

None.

## Modified

### REQ-ACT-04: Scroll the selected area
In scroll mode, `j` and `k` shall send configured vertical wheel detents down and up, and `l` and `h` shall send configured horizontal wheel detents right and left, at the selected area's center. Shift with those keys shall send the configured Shift step. The step defaults and configuration bounds are specified by REQ-ACT-05. Scrolling shall not click, shall keep the selected area and mode, and shall return the overlay to its normal input region after the wheel event. On Hyprland, the system shall send the pointer motion and discrete wheel axis through `zwlr_virtual_pointer_v1`, in order, so the compositor delivers the wheel event to the surface under the selected point. This shall support GTK, Qt, Chromium and Electron. If the owning window moved, closed or is no longer topmost before the wheel event, the system shall send no wheel event and shall report that the session must be restarted.
Acceptance:
- scenario: REQ-ACT-04 unit tests verify all four directions and configured detent amounts, with no button events.
- scenario: a daemon test waits for the overlay's pointer-pass-through-ready event, sends the wheel event to the selected point, and returns the same session to showing.
- scenario: a moved or occluded window produces no wheel event.
- live: scroll a GTK or Qt pane and a Chromium or Electron document, then confirm the overlay remains in scroll mode.

## Removed

None.

## Tests

- Update the REQ-ACT-04 pointer-axis test to verify Wayland axis and detent values in each direction.
- Keep the daemon handshake and stale-target tests, and verify the live scenario against an app window.
