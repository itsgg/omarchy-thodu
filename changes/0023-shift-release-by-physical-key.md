# 0023: Activate a Shift-completed label once Shift is up

Status: merged
Why: Task 0027. Task 0020 made a Shift-completed label wait for Shift to come up, so the click does not carry Shift into the app, but the spec never said so, and the overlay recognised the release only as a Shift key release. GG's layout option `shift:both_capslock_cancel` gives the Shift key Caps_Lock at its shifted level, so its release arrived as Caps Lock and the label never activated.

## Modified

### REQ-ACT-01: Return clicks
The existing text stands, with this sentence added after "An uppercase label activation performs one click and does not begin the double-Return wait.": When the uppercase final character is typed with Shift, the activation waits until Shift is physically released, so the click does not carry Shift; the release is recognised by the key that was pressed as Shift whatever symbol the keyboard layout gives its release, by a Shift or Caps Lock release when that press was not seen, or by any later key event without Shift held, and Escape cancels the waiting activation.
Acceptance, added:
- scenario: QML tests hold a Shift-completed label until Shift is up: a Shift released as Caps Lock (`shift:both_capslock_cancel`) with the scan code it was pressed with, any later key event without Shift, a Shift held since the session's hotkey and released as Caps Lock, and two Shift keys held, which wait for the last; Escape cancels the wait.

## Tests

- tests/qml/tst_session.qml: test_req_act_01_shift_released_as_caps_lock_activates_by_scan_code, test_req_act_01_a_key_event_without_shift_ends_the_wait, test_req_act_01_unseen_shift_released_as_caps_lock_activates, test_req_act_01_both_shift_keys_wait_for_the_last and test_req_act_01_caps_lock_symbol_release_through_the_key_handlers are added; the existing Escape and session-end tests cover cancellation.
