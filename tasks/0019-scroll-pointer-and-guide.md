# 0019: Keep scroll pointer steady and refresh the guide

Status: done
Depends on: none
Spec: REQ-ACT-06, REQ-SESSION-01
Goal: Keep the pointer fixed over a selected scroll area across held wheel repeats, while giving new users a short setup guide and real screenshots of hints and scroll mode.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-ACT-06 && tests/acceptance/docs-browser-setup.sh`
- scenario: A daemon test holds the selected point constant across repeated scroll actions and verifies the pointer does not receive one-pixel focus nudges.
- scenario: A QML test ends an active session when its workspace is deactivated, and the live overlay disappears after switching to another workspace.
- scenario: On the live Hyprland session, held J/K scrolls the selected app without per-tick cursor movement, the app receives no H/J/K/L keypresses, and Escape closes the mode.
- scenario: Start scroll mode on the demo in workspace 7, switch to empty workspace 8, and verify the outline and cue disappear without returning when workspace 7 is revisited.
- scenario: README has a concise install/use path and includes verified screenshots of hints and scroll mode.

## Log

- 2026-10-24: created after GG reported pointer jumps and flicker during scroll; existing documentation was long and showed only a cropped browser test page.
- 2026-10-25: live test confirmed the outline and cue remain visible after switching from workspace 7 to empty workspace 6. Adding workspace-deactivation handling and an acceptance check before proceeding.
- 2026-10-25: added same-coordinate Hyprland focus refresh for repeated scroll ticks and ended overlays when their workspace deactivates. Held J moved the demo pane from 0 to 600 px; held K returned it to 0. Ten cursor samples stayed at 768,493 and the page received no H/J/K/L keys. Switching to workspace 3 ended the session. Added a concise README, verified screenshots, and separate install, usage and browser-accessibility guides.
- 2026-10-25: bound cleanup to the monitor's active workspace ID captured before collection, including replies arriving after a workspace switch. All REQ-ACT-06 and REQ-SESSION-01 tests pass (92 Rust, 50 QML); the docs check and format check pass. Installed the release binaries and plugin, and `thodu doctor` is clean. A live scroll overlay on workspace 7 disappeared on switching to empty workspace 8 and stayed gone when returning to 7.
- 2026-10-25: cursor-focus refresh now follows the detected Lua or hyprlang dispatcher, and the workspace guard treats an absent `activeWorkspace` as inactive. REQ-ACT-06 and REQ-SESSION-01 acceptance passes with 92 Rust and 51 QML tests.
- 2026-10-25: changed the cursor refresh commands to Hyprland's request-socket syntax without a leading slash. The focused REQ-ACT-06 command test passes, and a live same-coordinate Lua dispatch returned `ok` with the pointer unchanged at 768,493.
- 2026-10-25: coordinator confirmed the merged implementation and passing acceptance, then marked the task done.
