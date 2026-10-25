# 0027: Recognise the Shift release by its physical key

Status: open
Depends on: none
Spec: REQ-ACT-01, REQ-SEARCH-01
Goal: A shifted key that completes a visible label waits for Shift to come up before it activates, so the click does not carry Shift. The overlay recognises that release only as a `Key_Shift` release, but GG's layout option `shift:both_capslock_cancel` gives the Shift key Caps_Lock at its shifted level, so its release arrives as `Key_CapsLock` and the label never activates: the cue "release Shift to activate" stays up. The overlay shall recognise the release by the physical key that was pressed as Shift, whatever symbol the layout gives the release, and shall treat any later key event without Shift held as Shift being up.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-ACT-01 REQ-SEARCH-01`
- scenario: A pending shifted activation is sent when the key pressed as Shift is released as Caps Lock with the same scan code, when any later key event arrives without Shift held, and still when a plain `Key_Shift` release arrives; it is not sent while another key is released with Shift still held, and with both Shift keys down it waits for the last.
- live: on GG's session with `shift:both_capslock_cancel`, Shift plus a visible label then releasing everything clicks the target.

## Log

- 2026-10-25: created from GG's live report and a key log of his attempt: Shift press as key 16777248 (`Key_Shift`), its release as 16777252 (`Key_CapsLock`) with Shift still in the modifiers, then the letter's release with no modifiers; the pending activation was never sent.
