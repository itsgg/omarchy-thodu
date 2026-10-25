# 0022: Keys typed with Control, Alt or Super

Status: merged
Why: Task 0026. Control-Shift-V reached thodud as a shifted letter and completed the label `v`, clicking a target nobody chose (SEED principle 1). REQ-SEARCH-01 says what Shift does and is silent on the other modifiers, so a chord meant for an app could search, type a label or scroll.

## Modified

### REQ-SEARCH-01: Typing searches; Shift types a label
While a session is showing in search-first mode, lowercase letters typed without Shift and with no active label prefix shall keep only the targets whose text contains the typed letters, ignoring case and spaces, and focus the first in reading order. In labels-only mode, lowercase letters type labels until `/` starts a search. Shifted letters type a target label in either mode. Lowercase letters continue an active label prefix. Completing a label with a lowercase final character focuses its target; completing it with an uppercase final character activates it according to REQ-ACT-01. A label that is hidden by the current search shall not activate or take focus. An invalid label prefix shall leave the prior focus intact. Operating-system key repeats shall not complete or activate a label. A key typed with Control, Alt or Super held shall neither search, type a label, select a scroll area nor scroll; Escape keeps its meaning with any modifier, and Return with Control is the Control click of REQ-ACT-02. Tab and Shift-Tab move the focus through the kept targets. This follows Homerow's defaults (SEED R1).
Acceptance:
- cases: `spec/cases/search/filter.jsonl`, typed input against target texts with the expected kept targets and focus.
- scenario: a session test covers an uppercase prefix and lowercase completion in search-first mode.
- scenario: session tests cover hidden labels, invalid prefixes, prefix cancellation with `/`, and focus restoration on Backspace.
- scenario: a QML test covers a shifted key whose event text is lowercase, ignores Caps Lock text as an activation signal, and filters repeats while label input is active but preserves search repeats.
- scenario: QML and daemon tests send Control, Alt and Super chords on letters, digits, Space, Tab, `/`, Backspace and scroll keys; none searches, types a label, selects an area or scrolls, Escape with a modifier still ends the session, and Control-Return still clicks with Control held.

## Tests

- tests/qml/tst_session.qml: test_req_search_01_modifier_chords_neither_search_nor_type_labels, test_req_search_01_scroll_mode_chords_neither_select_nor_scroll and test_req_act_02_a_chord_ends_the_wait_without_reaching_thodud are added.
- crates/thodud/src/main.rs: req_search_01_control_chord_neither_types_a_label_nor_acts is added.
