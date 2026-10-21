# 0007: Install, doctor and the unwanted paths

Status: open
Depends on: 0001, 0005
Spec: REQ-INSTALL-01, REQ-CLI-02, REQ-SESSION-04, REQ-SESSION-05
Goal: `thodu install` sets what Omarchy leaves off (the accessibility variables, the uinput rule, the plugin, the service, the keybindings GG chose) and `thodu doctor` reports each; a missing Hyprland, an empty window and an unregistered app each get the behavior section 8 gives them.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-INSTALL-01 REQ-CLI-02 REQ-SESSION-04 REQ-SESSION-05`
- GG runs `thodu install` once and `thodu doctor` reports everything present.

## Log

- 2026-10-21: created.
