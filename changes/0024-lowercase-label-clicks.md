# 0024: A lowercase label clicks; Shift selects

Status: merged
Why: Task 0028. Clicking a label needed Shift: an uppercase final character activated and a lowercase one only focused. Thodu clicks through the real pointer, so a Shift still held made the click a Shift-click (a new browser window), which required a wait for Shift release (change 0023) and recognising that release across layouts such as `shift:both_capslock_cancel`. GG's use found the flow confusing next to tools that click as soon as the label is typed. With the rule inverted, the activating keystroke never holds a modifier, and the wait goes. GG's use also answers open question O3: labels-only becomes the default.

## Modified

### Section 3, Label
The letters drawn over a target; typing it clicks the target, and typing its final letter with Shift selects it.

### REQ-SEARCH-01: Typing searches; Shift types a label
"Completing a label with a lowercase final character focuses its target; completing it with an uppercase final character activates it according to REQ-ACT-01" becomes: Completing a label with a lowercase final character clicks its target according to REQ-ACT-01; completing it with an uppercase final character selects the target without clicking, for Return, Shift-Return, Control-Return or a double Return to act on. Its acceptance scenario on search-first prefixes now expects a lowercase completion to click and a label typed wholly with Shift to select, and Caps Lock text is no selection signal.

### REQ-SEARCH-02: Labels-only mode
The setting `labels_only` defaults to true. Where it is true, letters without Shift type labels and `/` starts a search; where it is false, a session is search-first as REQ-SEARCH-01 describes. Its cases in `spec/cases/search/labels-only.jsonl` expect a lowercase label to click, an uppercase final character to select, and name search-first as a setting.

### REQ-ACT-01: Return clicks
Return with a focused target, or a label completed with a lowercase final character, clicks. Completing a label with an uppercase final character selects the target without acting. A label click performs one click and does not begin the double-Return wait. Since an uppercase final character never clicks, no click is sent while Shift is held and none waits for its release. Change 0023's sentence and scenario on the release wait are removed; the scenarios now cover a lowercase completion that clicks once, an uppercase completion that selects for Return and Shift-Return, and a Shift-completed label sent at once with no release cue.

### Section 7, keys inside a hints session; section 12, O3
The keys paragraph describes labels-only as the default and the case rule. O3 is resolved: labels-only is the default and search-first a setting.

## Tests

- crates/thodu-core/tests/req.rs: req_act_01_uppercase_completion_acts_but_lowercase_only_focuses and req_act_01_single_letter_d_selects_and_uppercase_d_acts are replaced by req_act_01_lowercase_completion_clicks_but_uppercase_only_selects and req_act_01_single_letter_d_clicks_and_uppercase_d_selects; req_search_01_shift_types_labels_and_tab_moves, req_search_01_lowercase_continues_a_shifted_label_prefix, req_session_01_new_label_prefix_clears_previous_focus, req_session_06_fallback_keeps_displayed_labels and req_search_02_setting follow the inverted rule and the new default.
- crates/thodud/src/main.rs: req_act_01_shifted_lowercase_key_text_activates_label is renamed req_act_01_shifted_lowercase_key_text_is_uppercase, with the same assertion.
- tests/qml/tst_session.qml: the Shift-release tests of changes 0023 and task 0020 are removed with the wait; test_req_act_01_shifted_label_is_sent_at_once_to_select is added; test_req_session_01_overlapping_focused_hint_is_raised_and_status_wraps checks the new cue text.
