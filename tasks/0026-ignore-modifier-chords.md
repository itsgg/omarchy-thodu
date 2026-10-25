# 0026: Ignore keys typed with Control, Alt or Super

Status: done
Depends on: none
Spec: REQ-SEARCH-01, REQ-ACT-02
Goal: A chord such as Control-Shift-V is a shortcut meant for an app, yet today the overlay sends it as a shifted letter and thodud completes and clicks the label `v`. Letters, digits, `/`, Space, Tab, Backspace and scroll keys typed with Control, Alt or Super held shall neither search, type a label, select a scroll area nor scroll. Escape still ends the session with any modifier, Control-Return still clicks with Control held, and a chord during the wait after a click still ends that wait.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-SEARCH-01 REQ-ACT-02`
- scenario: The overlay sends no key request for Control-Shift-V, Alt-Shift-V, Super-V, Control-2 in scroll mode or Alt-J in scroll mode, still sends Escape with Control held and Control-Return with `ctrl` set, and ends the post-click wait on a chord.
- scenario: thodud answers a `key` request for a letter with `ctrl` set with the unchanged view, queues no action and keeps the session showing.

## Log

- 2026-10-25: created from a confirmed defect: `parse_key` in crates/thodud/src/main.rs ignores `ctrl` for letters and SessionView.qml never reports Alt or Super, so Control-Shift-V completes the label `v` and clicks it.
- 2026-10-25: added QML tests for Control, Alt and Super chords on letters, Space, Tab, `/`, Backspace, scroll digits and scroll keys, and for a chord in the post-click wait, plus a daemon test for a Control letter; all four failed before the fix. The overlay now drops chords except Escape and Control-Return, a chord ends the post-click wait without reaching thodud, and thodud answers a Control letter with the unchanged view. Change 0022 states the rule in REQ-SEARCH-01. `tests/acceptance/reqs.sh REQ-SEARCH-01 REQ-ACT-02` passes.
- 2026-10-25: coordinator merged at `0b90fd3` after the full gate passed in 341 seconds. The same branch makes req_cli_02's stale Hyprland socket a plain file, which Linux refuses with the same ECONNREFUSED, because a dropped listener could be held open by a parallel test's child and failed the gate once.
