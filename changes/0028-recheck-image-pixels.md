# 0028: Recheck an image target's pixels before clicking it

Status: merged
Why: Task 0033. Change 0026 rechecks accessible elements before a click, but image targets come from one screen capture with nothing to recheck, and in Foot new output scrolls a different word under a label. A recapture of the target's region alone sampled pixels differently from the full-monitor detection capture at scale 1.25: unchanged regions measured up to 7.2 of the 10-level threshold. Recapturing the same monitor area at the same scale and cropping it the same way makes unchanged regions compare equal.

## Modified

### REQ-ACT-03: Never act on a stale target
Adds, before the sentence on the click sequence: An image target's region shall be captured again just before the click, over the same monitor area and at the same scale as the capture it was detected in, and if its pixels differ markedly from those at detection, the system shall not act and shall collect again; a capture that fails or takes longer than half a second counts as changed.
Acceptance, added:
- scenario: an image target carries a fingerprint of its region; the same pixels and a blinking cursor pass, other text in the region and a failed capture refuse the click.

## Tests

- crates/thodu-core/tests/req.rs: req_act_03_pixel_fingerprints_tell_new_text_from_a_blinking_cursor is added; req_act_03_live_targets_carry_element_checks compares with the element kind of the new target check.
- crates/thodud/src/image.rs: req_src_06_image_targets_carry_pixel_fingerprints and req_act_03_image_pixels_are_captured_again_before_a_click are added.
- crates/thodud/src/main.rs: req_act_03_an_image_target_with_changed_pixels_refuses_the_click and req_act_03_each_target_check_goes_to_its_own_recheck are added; req_act_03_a_moved_element_refuses_the_click builds the element kind of the new target check.
