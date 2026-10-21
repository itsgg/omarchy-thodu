# 0005: Recording trees, and the action that marks a clickable element

Status: merged
Why: task 0005 needed `thodu record-tree` and found two things about real trees on 2026-10-21. Chromium and Electron name an element's first action "click" when the page listens for clicks on it, whatever its role; Obsidian's own window buttons and its help icon are such elements, with the role section. And Chromium gives its web document's extents in physical pixels while the window around it is in logical units. No requirement changes.

## Modified

Section 7, Interfaces:

- `record`, `{"v":1,"op":"record","window":W}`: every frame of the window's process, walked in full and woken if dormant, returned as `snapshot` in the shape of `spec/cases/atspi/*.json`. `thodu record-tree --out FILE [--window W]` writes it.
- A recorded node carries `action`, the name of its first action when it has one; an element whose first action is "click" is a target, with the role button when its own role is not a clickable one.

## Tests

- crates/thodu-core/tests/req.rs: req_src_02_reference_windows_yield_the_marked_controls, req_cli_01_targets_json_matches_the_recorded_targets, and the reference windows in req_session_01_recorded_trees_yield_their_marked_targets.
- crates/thodud/src/atspi.rs: req_src_03_a_dormant_tree_is_woken_once_and_collected, req_src_03_a_live_tree_is_left_alone.
- crates/thodud/src/sources.rs: req_src_05_a_slow_source_is_left_out_and_named, req_src_05_sources_in_time_all_count.
