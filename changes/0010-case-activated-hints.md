# 0010: Case-signaled hints and focused feedback

Status: proposed
Why: GG asked for a clearer selected hint and a way for capitalized hint input to activate the target. Qutebrowser supports following a full hint, and Vimium gives uppercase hints a distinct action. Thodu currently focuses labels only and always draws a full target outline around a focused target.

## Added

## Modified

### REQ-SEARCH-01: Typing searches; Shift types a label
While a session is showing, lowercase letters typed without Shift and with no label prefix shall keep only the targets whose text contains the typed letters, ignoring case and spaces, and focus the first in reading order. Shifted letters and generated uppercase letters shall type a target label. Lowercase letters shall continue an active label prefix. Tab and Shift-Tab move focus through the kept targets. Completing a label follows REQ-ACT-01 when its final character is uppercase.
Acceptance:
- cases: `spec/cases/search/filter.jsonl`, typed input against target texts with the expected kept targets and focus.
- scenario: a session test covers uppercase label entry in search-first mode.

### REQ-ACT-01: Return clicks
When Return is pressed with a focused target, or the final character of a target label is uppercase, the system shall move the pointer to the target's center, click the left button there through the virtual pointer, end the session and return the pointer to where it was, unless Hyprland's focus follows the mouse and that point is not over the clicked window, where returning it would take the keyboard from that window; then the pointer stays on the target. A session started with `test` always returns it. Completing the label in lowercase focuses the target without acting.
Acceptance:
- scenario: WHEN a session on a recorded tree focuses a button at bounds (100, 200, 80, 30) on a monitor at scale 1.25 THEN the fake pointer receives a move to (140, 215) logical and one left click.
- scenario: focus follows the mouse and pointer restoration follows the existing rules.
- scenario: a session test types a two-letter label with lowercase completion and then uppercase completion, confirming focus-only then the same action as Return.

### REQ-SESSION-01: Labels on the focused window
When `thodu hints` runs and a window is focused, the system shall show a label for each target of that window and of the Omarchy bar on the same monitor. While focus comes from text search or keyboard navigation, the overlay shall outline the target bounds. After a label focuses a target, it shall hide the target outline and emphasize the label chip. The status toast shall wrap within the monitor width and a maximum width of 640 logical units, and show the lowercase-focus and uppercase-activate cue. The QML view uses an additive `label_selected` field to distinguish label focus.
Acceptance:
- replay: each recorded tree in `spec/cases/atspi/*.json` through the fake source yields the targets listed beside it in `spec/cases/atspi/*.targets.jsonl`.
- measured: REQ-PERF-01.
- scenario: a QML test checks focused label and search treatments, plus a wrapped, bounded status toast.

## Removed

## Tests

- Add requirement-named Rust session tests for REQ-SEARCH-01 and REQ-ACT-01.
- Add requirement-named QML tests for uppercase key text and REQ-SESSION-01 feedback.
- Run `tests/acceptance/reqs.sh REQ-SEARCH-01 REQ-ACT-01 REQ-SESSION-01`.
