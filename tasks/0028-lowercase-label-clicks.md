# 0028: A lowercase label clicks; Shift selects

Status: review
Depends on: 0027
Spec: REQ-SEARCH-01, REQ-SEARCH-02, REQ-ACT-01
Goal: Clicking needs Shift today: an uppercase final label character activates and a lowercase one only focuses. Thodu clicks through the real pointer, so a Shift still held becomes a Shift-click, which forced the wait for Shift release of tasks 0020 and 0027, its cue, and scan-code tracking for layouts that report the release as Caps Lock. GG found the flow confusing next to tools that click when the label is typed. Invert the rule so the final character's case decides: completing a label with a lowercase character clicks it at once, completing it with an uppercase character selects it without clicking, after which Return, Shift-Return, Control-Return or Return twice act. Make labels-only the default, so plain letters type labels and `/` starts a text search; search-first stays a setting. Remove the Shift-release wait, which no click needs any more. Grid cells keep refining on a label and clicking on Return.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-SEARCH-01 REQ-SEARCH-02 REQ-ACT-01`
- scenario: in labels-only mode a lowercase label clicks at once with no wait for a second Return; an uppercase final character selects without clicking and Return then clicks; a lowercase final character after a shifted prefix clicks; `/` searches and Return clicks the focused result.
- scenario: in search-first mode lowercase letters search, a shifted label selects, and a lowercase completion of a shifted prefix clicks.
- scenario: `labels_only` defaults to true; grid cells still refine on a label and click on Return; the overlay sends a Shift-completed label at once and shows no release cue.

## Log

- 2026-10-25: created at GG's request after the Shift-release fault of task 0027: from first principles the activating keystroke must not hold a modifier, since the click carries whatever is held.
- 2026-10-25: the session's case rule is inverted (lowercase final clicks, uppercase final selects), `labels_only` defaults to true in thodu-core and the bar menu, and the overlay sends every key at once, with the Shift-release wait, its cue and scan-code tracking removed. Core, daemon and QML tests follow the rule; the labels-only cases are updated. Change 0024 records it and resolves O3. README and usage describe the keys. `tests/acceptance/reqs.sh REQ-SEARCH-01 REQ-SEARCH-02 REQ-ACT-01` passes.
