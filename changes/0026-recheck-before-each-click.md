# 0026: Recheck the element, and each click right before it

Status: merged
Why: Tasks 0030 and 0031. REQ-ACT-03 covered a window that moved, but not content that reflows inside an unchanged window, such as Slack's channel list reordering or a page scrolling: targets are indexes into a tree read once. The click pipeline also left time between the check and the press, shared the pointer between sessions without a lock, and sent the second click of a double click unchecked, even when the first never happened.

## Modified

### REQ-ACT-03: Never act on a stale target
The existing sentence stands, followed by: An accessible target's element shall be read again just before the click, and if it is gone, no longer showing, or its window-relative extents moved by more than one unit, the system shall not act and shall collect again. Each click shall run its checks immediately before its press, with clicks and scroll actions taking turns with the one pointer, and the second click of a double click shall be checked the same way and skipped when the first click did not happen.
Acceptance, added:
- scenario: an accessible target whose element moved, hid or no longer answers is refused while its window and click point are unchanged, and an element that cannot be read in time counts as moved.
- scenario: a click prepares, then checks and presses while holding the pointer, two clicks never hold it together, and a second click is skipped when the first never happened or its target moved.

## Tests

- crates/thodu-core/tests/req.rs: req_act_03_element_check_compares_extents_and_showing and req_act_03_live_targets_carry_element_checks are added.
- crates/thodud/src/atspi.rs: req_act_03_element_is_read_again_before_a_click is added.
- crates/thodud/src/sources.rs: req_act_03_an_unreadable_element_counts_as_moved is added.
- crates/thodud/src/main.rs: req_act_03_a_moved_element_refuses_the_click is added; task 0030 added req_act_03_click_prepares_then_checks_right_before_pressing, req_act_03_two_clicks_never_hold_the_pointer_at_once, req_act_02_second_click_is_not_sent_without_the_first and req_act_03_second_click_rechecks_its_target.
- Target literals across the tests gain `check: None`.
